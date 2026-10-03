//! One way to write a program a test is about to run.
//!
//! Charter's tests are full of stand-ins: a `git` that refuses, a `claude` that answers the
//! wiring probe, a `gh` that replays recorded JSON, an fsmonitor that touches a marker. Every
//! one of them is a shell script the test writes and then something — charter, or git, or the
//! test itself — runs. Written the obvious way, that loses to `ETXTBSY`, *Text file busy*,
//! and it did: charter-app#81 and charter-app#39.
//!
//! **`ETXTBSY` is a property of the inode, not of the name.** The kernel refuses to `execve`
//! a file that any process holds open for writing, and it refuses to open for writing a file
//! any process is executing. Both directions bite here, and they are not the same bug.
//!
//! **Running a program this process wrote (charter-app#81).** `fs::write` closes its own
//! descriptor before it returns, so the test is not the one still holding it — but `cargo
//! test` runs the tests in one binary on many threads, and *every* `Command::spawn` on any of
//! those threads forks. A fork copies the descriptor table, so a child forked while this
//! thread had the script open for writing holds a copy of that descriptor until it reaches
//! its own `execve`, microseconds later. Exec the script inside that window and the kernel
//! answers `ETXTBSY`. A rename does **not** close that window: the descriptor the child holds
//! is on the inode, and renaming hands exec the same inode under another name. What closes it
//! is never opening the program for writing in this process at all — a fork cannot copy a
//! descriptor the forking process does not have. So the bytes are written by a child
//! (`/bin/sh`), which this function waits out, and afterwards no descriptor for that inode
//! exists anywhere.
//!
//! **Writing over a program that is running (charter-app#39).** The app's stand-in `claude`
//! ends in `sleep 600`, so an earlier chat is still executing it when the next test rewrites
//! it, and writing a running program is `ETXTBSY` too. That one a rename does fix: it
//! replaces the directory entry and leaves the running inode alone. Measured there at two
//! failures in five runs.
//!
//! Hence both halves below, and both are load-bearing: the child writes it (so exec never
//! races a descriptor), and a rename puts it in place (so the write never races an exec).
//!
//! # What Darwin answers now (charter-app#184, measured on macOS 26.2 / Darwin 25.2)
//!
//! **Darwin no longer answers `ETXTBSY` in either direction.** A `#!` script whose inode
//! another process holds open for writing execs and runs; a running program can be opened for
//! writing and truncated. Both were checked with the setup verified first rather than the
//! kernel blamed first — `lsof` on the holding child shows it open `1w` on the same inode the
//! exec resolves to — because this repo has been wrong about exactly this before, and a
//! rename-based fix for charter-app#81 was disproven on CI at round 20 of 250.
//!
//! **It has not become safe; the refusal has changed shape and lost its errno.** A Mach-O that
//! the kernel has already validated once is `SIGKILL`ed when it is exec'd again while somebody
//! holds it open for writing: the writable open invalidates the cached code signature for that
//! vnode. One that has never been exec'd runs. So on Darwin the hazard has moved off
//! [`program`], whose stand-ins are scripts, and onto [`copy_of`], whose stand-ins are
//! binaries that charter runs, rewrites and runs again — and where the failure is now a kill
//! a caller cannot read an error out of.
//!
//! Neither half below is therefore relaxed. Linux still answers `ETXTBSY` and is what CI gates
//! on, and on Darwin the discipline is what keeps [`copy_of`] out of the kill.
//!
//! # Ending what a test started (#923)
//!
//! The crate's other job: making sure a fixture does not outlive its test. [`Ends`] kills a
//! fixture's process group when the test ends or unwinds, and [`stubborn`] writes a fixture
//! that ignores signals but still ends on its own after [`FIXTURE_LIFETIME_SECS`].

use std::path::{Path, PathBuf};

/// Writes `contents` as `dir/name`, makes it runnable, and returns its path.
///
/// `contents` is the whole file, shebang included — this is not a script template. It travels
/// to `/bin/sh` as an argument, so it has to fit in `ARG_MAX` (a megabyte and more on every
/// platform charter builds for) and hold no NUL byte. Both hold for a stand-in.
///
/// Panics rather than returning an error: every caller is a test, and a stand-in that could
/// not be written has nothing to say about the subject.
#[cfg(unix)]
pub fn program(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let (path, beside) = places(dir, name);

    // The write happens in the CHILD, which is the whole point: this process opens no
    // descriptor on the inode it is about to hand to `execve`, so there is none for a fork on
    // another thread to copy. `printf %s` is a builtin in every /bin/sh charter runs on
    // (dash on the Linux runners, bash in sh mode on macOS), so nothing is looked up on PATH
    // and `env_clear` cannot starve it. The redirection, not the argument, is what writes:
    // `%s` is the format and the script is data, so a script full of `%` or `\` arrives
    // unchanged.
    let wrote = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"printf %s "$1" > "$2""#)
        .arg("sh")
        .arg(contents)
        .arg(&beside)
        .env_clear()
        .status()
        .expect("/bin/sh runs");
    assert!(
        wrote.success(),
        "the stand-in {} was not written: {wrote}",
        path.display()
    );

    put_in_place(&beside, path)
}

/// A copy of an existing program — a real binary, where a shell script would not do — under
/// `dir/name`, runnable, at its own path.
///
/// Same reasoning as [`program`]: `/bin/cp` does the writing, so this process never opens the
/// copy for writing and no fork of it can be holding the copy open when it is run.
#[cfg(unix)]
pub fn copy_of(source: &Path, dir: &Path, name: &str) -> PathBuf {
    let (path, beside) = places(dir, name);

    let copied = std::process::Command::new("/bin/cp")
        .arg(source)
        .arg(&beside)
        .env_clear()
        .status()
        .expect("/bin/cp runs");
    assert!(
        copied.success(),
        "{} was not copied to {}: {copied}",
        source.display(),
        path.display()
    );

    put_in_place(&beside, path)
}

/// Where the program goes, and where it is written first.
///
/// Beside the program, never in a temp directory of its own: `rename` is only atomic — only a
/// rename at all — within one filesystem, and `/tmp` need not be the one the caller's
/// directory is on.
#[cfg(unix)]
fn places(dir: &Path, name: &str) -> (PathBuf, PathBuf) {
    let path = dir.join(name);
    let parent = path
        .parent()
        .unwrap_or_else(|| panic!("{} has a parent directory", path.display()))
        .to_path_buf();
    let stem = path
        .file_name()
        .unwrap_or_else(|| panic!("{} names a file", path.display()))
        .to_string_lossy()
        .into_owned();
    let beside = parent.join(format!(
        ".{stem}.{}.{}.writing",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_nanos()
    ));
    (path, beside)
}

#[cfg(unix)]
fn put_in_place(beside: &Path, path: PathBuf) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;

    // `chmod` takes a path and opens nothing, so it is safe to do from here.
    std::fs::set_permissions(beside, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|e| panic!("{} is made runnable: {e}", beside.display()));
    // And the rename is the OTHER half: it never truncates whatever is at `path` now, which
    // may be a program another chat is still running (charter-app#39).
    std::fs::rename(beside, &path)
        .unwrap_or_else(|e| panic!("{} is put in place: {e}", path.display()));
    path
}

/// Writes `bytes` to `child`'s standard input and closes it, and lets the child have
/// answered without reading them.
///
/// A command under test may refuse, or answer, before it reads its stdin — a hook word that
/// has nothing to do exits at once — and then this write meets a closed pipe. Whether it
/// does is a race between this write and the child's exit, so a plain `write_all().expect()`
/// fails a test only sometimes, and before its real assertions: main went red that way in
/// `hook.rs` on b93e58e and a68cf8e, after the same race was fixed one file at a time
/// (#228, #323). A closed pipe here is an answer, and the caller asserts on it; any other
/// write error still fails.
///
/// A test whose meaning is that the child *consumed* the payload — a value set from
/// `--stdin` — should not use this: a strict write is part of what it checks.
///
/// Relies on `SIGPIPE` being ignored, which the Rust runtime does before `main`, so the
/// write returns `EPIPE` instead of killing the test.
pub fn feed(child: &mut std::process::Child, bytes: &[u8]) {
    use std::io::Write as _;
    let mut stdin = child.stdin.take().expect("the child's stdin is piped");
    match stdin.write_all(bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(error) => panic!("the payload is written: {error}"),
    }
}

/// How long a fixture that ignores signals keeps going before it ends on its own, in seconds.
///
/// Five minutes: past any one test's own deadline here (the slowest wait is thirty seconds
/// for a first-run assessment on a loaded Mac), and short enough that one which slips past
/// every guard is gone well before the next run starts (#923).
pub const FIXTURE_LIFETIME_SECS: u32 = 300;

/// A shell script that ignores `signals`, runs `first`, and then waits — for
/// [`FIXTURE_LIFETIME_SECS`] at most, and never for ever.
///
/// What a test means by `trap '' INT HUP; while :; do sleep 600; done`: a program only a kill
/// can end. Written that way it outlived its test by days whenever the kill never came — a
/// failed assertion, or a test binary that exited before the reaper thread got to it (#923).
/// The bound is counted in one-second sleeps, so it needs nothing but `sleep` and the shell's
/// own arithmetic, and a group kill takes at most one `sleep 1` with it.
pub fn stubborn(signals: &str, first: &str) -> String {
    stubborn_for(signals, first, FIXTURE_LIFETIME_SECS)
}

/// [`stubborn`], with a bound of `secs` seconds instead.
pub fn stubborn_for(signals: &str, first: &str, secs: u32) -> String {
    format!(
        "trap '' {signals}; {first}{sep}i=0; while [ \"$i\" -lt {secs} ]; do sleep 1; \
         i=$((i+1)); done",
        sep = if first.is_empty() { "" } else { "; " },
    )
}

/// Kills a fixture's whole process group when it drops — when the test ends, and on the
/// unwind out of a test that failed (#923).
///
/// **Synchronous, on purpose.** What leaked was a fixture whose ending was left to someone
/// else: a session's reaper thread, which hangs up, waits its grace and only then kills, and
/// which a test binary that exits first never lets finish; or a program a test meant to stop
/// after its last assertion, when an earlier one failed. This kills before `drop` returns.
///
/// **By the pid this test caused to exist, never by name**: either one the test was handed
/// ([`Ends::group`]), or one the fixture wrote down itself ([`Ends::named_in`]), read when the
/// guard drops so a test that failed before reading it is still covered. The group is killed
/// and then the pid itself, since a fixture that is not its group's leader is still named.
///
/// **And only while that pid still names the same process.** A guard usually drops after its
/// fixture already ended — the passing case — and by then the kernel may have handed the pid
/// to something else; on a machine running other sessions' work beside the tests, that is
/// someone else's process. So the guard knows when its process started (asked of `ps` when it
/// is made, or, for a marker, no earlier than the marker was written) and kills nothing if
/// the process holding the pid now started later.
///
/// **It fails toward a leak, never toward a wrong kill.** The kernel is asked first whether any
/// process holds the pid (the null signal). `ESRCH` — none does — kills only the group: what is
/// left in it can only be the fixture's own children, since a pid is not handed out again while
/// a group still bears it as its id. A process that is there but whose start cannot be read —
/// `ps` would not start, out of processes or descriptors, or answered nothing — is left alone,
/// and so is one the kernel says is somebody else's (`EPERM`). A fixture that leaks that way is
/// caught by CI's orphan check (`tools/no-orphans.sh`); a wrong kill would be caught by nobody.
///
/// A pid of 0 or 1 is never signalled: those are this process's own group and init.
#[derive(Debug)]
#[must_use = "a guard that is dropped at once kills its fixture at once"]
pub struct Ends {
    target: Target,
    ps: PathBuf,
}

#[derive(Debug)]
enum Target {
    /// A pid, and the second its process started, when that could be asked.
    Pid(u32, Option<u64>),
    NamedIn(PathBuf),
}

/// How far apart two readings of one process's start may be: `ps` answers its elapsed time to
/// the second, and the reading and the clock it is subtracted from are not one instant.
const SAME_START_SECS: u64 = 2;

/// The `ps` asked, the same path on Linux and macOS.
const PS: &str = "/bin/ps";

impl Ends {
    /// Kill the group `pid` leads, and `pid` itself, when this drops — if `pid` still names
    /// the process it names now.
    pub fn group(pid: u32) -> Self {
        Self::of(Target::Pid(pid, started(Path::new(PS), pid)))
    }

    /// [`Ends::group`], for a process known to have started at `started` (seconds since the
    /// epoch) rather than whatever holds `pid` now.
    pub fn started_at(pid: u32, started: u64) -> Self {
        Self::of(Target::Pid(pid, Some(started)))
    }

    /// Kill the group led by the pid written in `marker`, and that pid, when this drops —
    /// nothing, if the marker was never written or does not hold a pid.
    pub fn named_in(marker: impl Into<PathBuf>) -> Self {
        Self::of(Target::NamedIn(marker.into()))
    }

    /// The pid this guard was handed, or `None` for one that reads it from a marker.
    pub fn pid(&self) -> Option<u32> {
        match self.target {
            Target::Pid(pid, _) => Some(pid),
            Target::NamedIn(_) => None,
        }
    }

    /// The same guard, asking `ps` at `ps` when it drops: how a test makes `ps` unavailable.
    pub fn asking_ps(mut self, ps: impl Into<PathBuf>) -> Self {
        self.ps = ps.into();
        self
    }

    fn of(target: Target) -> Self {
        Ends {
            target,
            ps: PathBuf::from(PS),
        }
    }
}

impl Drop for Ends {
    fn drop(&mut self) {
        let ps = self.ps.as_path();
        match &self.target {
            Target::Pid(pid, Some(at)) => {
                kill_if(ps, *pid, |now| now.abs_diff(*at) <= SAME_START_SECS);
            }
            // `ps` could not be asked when the guard was made: the process had ended already,
            // or `ps` would not start. Nothing to tell a reused pid by, so only a pid no
            // process holds is acted on — its group.
            Target::Pid(pid, None) => kill_if(ps, *pid, |_| false),
            Target::NamedIn(marker) => {
                let Ok(text) = std::fs::read_to_string(marker) else {
                    return;
                };
                let Ok(pid) = text.trim().parse() else {
                    return;
                };
                let written = std::fs::metadata(marker)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());
                // The fixture is started, then writes its pid: a process that started after
                // the marker was written is not the one that wrote it.
                kill_if(ps, pid, |now| {
                    written.is_some_and(|written| now <= written + SAME_START_SECS)
                });
            }
        }
    }
}

/// Kill `pid`'s group, and `pid`, if the process holding `pid` now passes `same` on its start
/// second; kill only the group if no process holds `pid`; and nothing otherwise — including
/// when the process is there and its start cannot be read.
#[cfg(unix)]
fn kill_if(ps: &Path, pid: u32, same: impl Fn(u64) -> bool) {
    use rustix::io::Errno;
    use rustix::process::{Pid, Signal, kill_process, kill_process_group, test_kill_process};
    let Some(raw) = i32::try_from(pid)
        .ok()
        .filter(|&raw| raw > 1)
        .and_then(Pid::from_raw)
    else {
        return;
    };
    match test_kill_process(raw) {
        // No process holds the pid: the passing case, or a leader that died before its group.
        Err(Errno::SRCH) => {
            let _ = kill_process_group(raw, Signal::KILL);
        }
        // There, and ours to signal: only if it is still the process this guard is for.
        Ok(()) => {
            if started(ps, pid).is_some_and(same) {
                let _ = kill_process_group(raw, Signal::KILL);
                let _ = kill_process(raw, Signal::KILL);
            }
        }
        // `EPERM` is another user's process; anything else is an answer this cannot place.
        Err(_) => {}
    }
}

#[cfg(not(unix))]
fn kill_if(_ps: &Path, _pid: u32, _same: impl Fn(u64) -> bool) {}

/// The second the process holding `pid` started, from its elapsed time as `ps` reads it
/// (`[[dd-]hh:]mm:ss`, the same on Linux and macOS); `None` if no process holds it, or `ps`
/// would not start or answered nothing.
#[cfg(unix)]
fn started(ps: &Path, pid: u32) -> Option<u64> {
    let out = std::process::Command::new(ps)
        .args(["-o", "etime=", "-p", &pid.to_string()])
        .env("LC_ALL", "C")
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let elapsed = elapsed_secs(String::from_utf8_lossy(&out.stdout).trim())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(now.saturating_sub(elapsed))
}

#[cfg(not(unix))]
fn started(_ps: &Path, _pid: u32) -> Option<u64> {
    None
}

/// `[[dd-]hh:]mm:ss` in seconds.
fn elapsed_secs(etime: &str) -> Option<u64> {
    if etime.is_empty() {
        return None;
    }
    let (days, clock) = match etime.split_once('-') {
        Some((d, rest)) => (d.parse::<u64>().ok()?, rest),
        None => (0, etime),
    };
    let mut secs = 0u64;
    for part in clock.split(':') {
        secs = secs * 60 + part.parse::<u64>().ok()?;
    }
    Some(days * 86_400 + secs)
}

/// A folder of a test's own that no sandboxed chat may write, removed when this is dropped.
///
/// A sandboxed start refuses a program, or a file its command names, anywhere a chat can write
/// (charter's ruling V87g): the project, the chat's folders, and the system temp folders. So a
/// stand-in a sandboxed start is to run cannot live in a `tempfile` folder. This one is made
/// under `$HOME/.cache`, which no charter sandbox grants.
pub struct NoChatWrites {
    path: PathBuf,
}

impl NoChatWrites {
    /// Makes a fresh, empty folder.
    ///
    /// Panics if it cannot, or if `HOME` is unset: every caller is a test.
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static MADE: AtomicU32 = AtomicU32::new(0);
        let base = PathBuf::from(std::env::var_os("HOME").expect("HOME is set"))
            .join(".cache/charter-stand-ins");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.subsec_nanos());
        let path = base.join(format!(
            "{}-{}-{nanos}",
            std::process::id(),
            MADE.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).expect("a folder no chat writes");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Default for NoChatWrites {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for NoChatWrites {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
