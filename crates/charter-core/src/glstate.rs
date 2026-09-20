//! **When** the forge cache is refreshed — `charter/glstate.py:maybe_spawn`, which is the
//! half of that module M2.7 deliberately did not port (charter-app#69).
//!
//! [`crate::cistate`] reads the cache, [`crate::glrefresh`] writes it, and until this module
//! **nothing decided to run one**: the app's CI column showed whatever the last
//! `charter gl-refresh` typed by hand had left, which on a plane where nobody typed it is
//! "nothing has fetched this checkout" for ever.
//!
//! # No daemon, ever
//!
//! Python's decision lives on the status line's own render path, so every turn that draws the
//! footer also kicks a refresh. charter-app draws no footer, and the answer is **not** a timer
//! in the app: a refresh is spawned from something the operator did, and from nothing else.
//! The trigger here is focusing a workspace (`app/src-tauri/src/panels.rs:repo_states`, the
//! panel that already reads the cache) — one user action, one policy question, at most one
//! process. Nothing wakes up on its own, and an app sitting untouched makes no forge call.
//!
//! # Three brakes and a lock, and every one of them is a defect that happened
//!
//! - [`REFRESH_TTL`] — nothing is spawned unless some tree's entry is older than this.
//! - [`SPAWN_COOLDOWN`] — at most one refresh per this many seconds, measured from the last
//!   spawn **or the last completion**, whichever the lock's mtime records.
//! - [`STUCK_AFTER`] — a refresh still claiming to be in flight after this is presumed wedged
//!   and replaced, which is also what stops a recycled pid suppressing refreshes for ever.
//! - **the lock's content is the pid of an in-flight refresh** (charter#324). The cooldown
//!   alone said "a refresh was STARTED 120 s ago", which is not a reason to skip another; what
//!   suppresses one is that the first is still running, and what starts the cooldown is that
//!   it stopped. A wedged refresh otherwise invited a replacement every 120 s for as long as
//!   the session stayed open, **each one holding the forge credential**.
//!
//! The credential is why the brakes are the feature rather than an optimisation. A refresh
//! runs `gh` or `glab`; a trigger with no cooldown is a forge call per render from a process
//! holding a token, which is the shape of charter#324 and one layer under charter#326.
//!
//! # What this module does NOT do
//!
//! It never fetches and never draws. [`decide`] is a decision over two files, and
//! [`maybe_spawn`] starts a process that does the fetching somewhere else — so the process
//! that holds the forge token is still not the process that draws the window, which is the
//! whole of [`crate::glrefresh`]'s own argument.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

use crate::contain;
use crate::glrefresh;

/// Entries older than this are due a refresh: `charter/glstate.py:REFRESH_TTL`.
///
/// The same constant [`crate::cistate`] already names, because a panel that says "this is old
/// enough that a refresh is due" and a policy that refuses to spawn one would be two numbers
/// telling the operator two different stories about one file.
pub const REFRESH_TTL: Duration = crate::cistate::REFRESH;

/// At most one refresh per this: `charter/glstate.py:SPAWN_COOLDOWN`.
pub const SPAWN_COOLDOWN: Duration = Duration::from_secs(120);

/// How long a refresh may claim to be in flight before it is presumed wedged and replaced:
/// `charter/glstate.py:STUCK_AFTER`.
///
/// Three times [`REFRESH_TTL`]: a refresh still running after this would land data that was
/// already stale several times over, so waiting on it is worse than starting a fresh one even
/// if it IS alive. It is also what stops a RECYCLED pid suppressing refreshes indefinitely —
/// the failure it bounds is a slightly stale CI column, never a stuck panel, since
/// `cistate::read` keeps serving up to `cistate::DISPLAY`.
pub const STUCK_AFTER: Duration = Duration::from_secs(900);

/// The operator's own brake, honoured here as `update.maybe_spawn` honours it (charter#945).
///
/// **On whenever it holds more than whitespace** — `0`, `false` and `no` included. This is a
/// request not to phone home, and the two ways of misreading it are not the same size:
/// reading an unintended value as "on" costs a CI column that does not fill, and reading one
/// as "off" runs the forge client for somebody who asked charter not to. Blank is unset, as a
/// blank `$CHARTER_WORKSPACE` is (charter#1055): `export CHARTER_NO_BACKGROUND_CHECKS=` is how
/// a shell spells taking a value away.
pub const NO_BACKGROUND_CHECKS: &str = "CHARTER_NO_BACKGROUND_CHECKS";

/// What the policy decided, and **why** — never a bare boolean.
///
/// The reason is carried because the caller is a panel: a column that stays empty has to be
/// able to say whether nothing was fetched because a refresh is already running, because the
/// operator turned background checks off, or because charter would not read its own lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decided {
    /// Every brake is off and some tree's entry is stale or absent: a refresh is due.
    Due,
    /// [`NO_BACKGROUND_CHECKS`] is set, and it is read **before** the lock or the cache is
    /// touched — the operator asked charter not to look, so charter does not look.
    TurnedOff,
    /// A refresh was spawned, or finished, less than [`SPAWN_COOLDOWN`] ago. This is the arm
    /// that makes two triggers in quick succession one refresh.
    CoolingDown { seconds: u64 },
    /// Past the cooldown, but the refresh the lock NAMES is still running and has not been
    /// going long enough to be presumed wedged. charter#324.
    AlreadyRefreshing { pid: u32, seconds: u64 },
    /// charter would not read its own lock. **Suppress rather than spawn**: the cost of
    /// skipping a refresh is a stale column, and the cost of getting this wrong is the
    /// pile-up the cooldown exists to prevent.
    LockUnreadable { why: String },
    /// Every tree has an entry inside [`REFRESH_TTL`] — including the workspace with no trees
    /// at all, where there is nothing to be stale.
    FreshEnough,
}

/// Everything [`decide`] reads that is not one of the two files, passed rather than read so a
/// test drives every brake and so nothing here consults the real environment by accident.
pub struct When<'a> {
    /// The plane whose `.charter/cache/` holds the cache and the lock.
    pub plane: &'a Path,
    /// The trees a refresh would fetch for — [`crate::glrefresh::trees`], which is the same
    /// list the panel draws.
    pub trees: &'a [PathBuf],
    /// Seconds since the epoch, as Python's `time.time()` answers, taken ONCE for the whole
    /// decision so the lock's age and the cache's age are measured against one instant.
    pub now: f64,
    /// Where [`NO_BACKGROUND_CHECKS`] is read from.
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// Whether a pid is a live process — [`alive`] in anything that is not a test.
    pub alive: &'a dyn Fn(u32) -> bool,
}

/// Whether a refresh should be spawned, and why not where it should not.
///
/// The order is `charter/glstate.py:maybe_spawn`'s, and the order is load-bearing: the
/// operator's brake comes before anything is read, and the lock comes before the cache,
/// because a refresh already in flight makes the cache's age nobody's business.
///
/// **Python's second brake has no arm here.** `if not config.HAS_CONTROL_PLANE: return` stops
/// a render outside a plane from scattering `.charter/` into whatever directory it ran in
/// (charter#527). Every caller on this side is handed a plane that
/// [`crate::plane::resolve`] already found, so there is no state in which this is asked
/// without one — the brake is satisfied by the signature rather than by a check.
pub fn decide(when: &When) -> Decided {
    if background_checks_off(when.env) {
        return Decided::TurnedOff;
    }
    match in_flight(when.plane, when.now) {
        Err(why) => return Decided::LockUnreadable { why },
        // No lock at all: nothing has ever refreshed here, so only staleness decides.
        Ok(None) => {}
        Ok(Some((pid, age))) => {
            if age < SPAWN_COOLDOWN.as_secs_f64() {
                return Decided::CoolingDown {
                    seconds: age as u64,
                };
            }
            // Past the cooldown, and the previous refresh is STILL RUNNING. Without this the
            // old code had already forgotten the child existed, so a wedged refresh invited a
            // replacement every 120 s for as long as the session stayed open.
            if let Some(pid) = pid.filter(|pid| (when.alive)(*pid))
                && age < STUCK_AFTER.as_secs_f64()
            {
                return Decided::AlreadyRefreshing {
                    pid,
                    seconds: age as u64,
                };
            }
        }
    }
    if !stale(&glrefresh::load(when.plane), when.trees, when.now) {
        return Decided::FreshEnough;
    }
    Decided::Due
}

/// Whether [`NO_BACKGROUND_CHECKS`] is on in `env` — `charter/util.py:background_checks_off`.
pub fn background_checks_off(env: &dyn Fn(&str) -> Option<String>) -> bool {
    env(NO_BACKGROUND_CHECKS).is_some_and(|value| !crate::memstore::py_strip(&value).is_empty())
}

/// `(the pid of an in-flight refresh, how many seconds ago the lock last changed)`, or `None`
/// when there is no lock at all — `charter/glstate.py:_read_lock`.
///
/// Two facts rather than one, because charter#324 needed both and the file carried neither.
///
/// **Deliberately tolerant of the CONTENT**: a truncated write, a hand edit, or a pid from
/// another machine all read as "no pid", which degrades to the plain cooldown rather than to
/// a refusal. What is *not* tolerated is the PATH — a link at `.charter/cache/glstate.refreshing`
/// redirects the read exactly as one at the directory does, and a check on the parent cannot
/// see it. That is [`crate::cistate::read`]'s own rule about the file beside this one, and the
/// answer to a link here is [`Decided::LockUnreadable`], which suppresses.
pub fn in_flight(plane: &Path, now: f64) -> Result<Option<(Option<u32>, f64)>, String> {
    let path = plane.join(glrefresh::LOCK);
    contain::no_link_on_the_way(plane, &path).map_err(|why| {
        format!(
            "{} is reached through a symlink, and charter's own path may not be ({why})",
            path.display()
        )
    })?;
    // No lock, or one whose timestamp the filesystem will not give up: Python's `stat` in a
    // `try` that answers `None`, which is "nothing has refreshed here" and not a refusal.
    let Ok(when) = std::fs::symlink_metadata(&path).and_then(|found| found.modified()) else {
        return Ok(None);
    };
    // Never negative, as Python's `max(0.0, …)`: a clock that moved backwards must not read
    // as a refresh spawned in the future and suppress every one after it.
    let age = (now - epoch_seconds(when)).max(0.0);
    // A read that fails is an empty lock — Python's `except OSError: txt = ""` — so a
    // directory left at this path reads as "a lock with no pid" rather than raising.
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let digits = text.trim();
    // `str.isdigit()` and then `int()`, not Rust's parse: `"+5".parse::<u32>()` is `Ok(5)` and
    // Python refuses it, and a lock the two charters read differently is a lock that stops
    // meaning anything on a plane both of them touch.
    let pid = digits
        .chars()
        .all(|c| c.is_ascii_digit())
        .then(|| digits.parse::<u32>().ok())
        .flatten()
        .filter(|pid| *pid > 0);
    Ok(Some((pid, age)))
}

/// Whether any tree is missing from the cache or older than [`REFRESH_TTL`] —
/// `charter/glstate.py:maybe_spawn`'s `stale`.
///
/// A workspace with **no trees** is not stale: `any([])` is false there too, and a plane whose
/// workspace holds no clone has nothing a forge could be asked about.
///
/// An entry that is not an object at all reads as stale here and RAISES in Python, whose
/// `cache[str(d)].get("ts", 0)` sits outside `maybe_spawn`'s only `try`. Refreshing over a
/// cache charter cannot make sense of is the better of the two answers, and it is said out
/// loud rather than inherited.
pub fn stale(cache: &Map<String, Value>, trees: &[PathBuf], now: f64) -> bool {
    trees.iter().any(|tree| {
        match cache.get(&glrefresh::key_for(tree)) {
            // Never fetched: the case a plane starts in, and the one the issue is about.
            None => true,
            Some(entry) => {
                let stamped = entry.get("ts").and_then(Value::as_f64).unwrap_or(0.0);
                now - stamped > REFRESH_TTL.as_secs_f64()
            }
        }
    })
}

/// Whether `pid` is a live process — `charter/glstate.py:_alive`.
///
/// The null signal checks for existence without delivering anything. `EPERM` means it exists
/// and belongs to somebody else, which is still "in flight" for this purpose; `ESRCH` means it
/// is gone. Anything else is read as gone, so an answer charter cannot make sense of unblocks
/// the refresh rather than wedging it.
#[cfg(unix)]
pub fn alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    let Some(pid) = rustix::process::Pid::from_raw(pid) else {
        return false;
    };
    match rustix::process::test_kill_process(pid) {
        Ok(()) => true,
        Err(rustix::io::Errno::PERM) => true,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
pub fn alive(_pid: u32) -> bool {
    false
}

/// What [`maybe_spawn`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refreshing {
    /// A refresh is running under this pid, and the lock now names it.
    Started { pid: u32 },
    /// The policy said not to, and why.
    Declined(Decided),
    /// The fork itself failed. **The cooldown is NOT armed**: a spawn that never happened
    /// must not suppress the next trigger's retry, which is Python's own reason for returning
    /// before `_write_lock` here.
    NotStarted { why: String },
}

/// Ask the policy, and start a detached `charter gl-refresh` where it says to.
///
/// Non-blocking by contract: the caller is on a path with a 100 ms budget for a workspace
/// switch, so this is a `spawn` and never a wait. The answer is what happened, so the caller
/// can say it rather than guess.
///
/// `binary` is the `charter` the app ships beside itself — never one found on `PATH`, which
/// may be an older install or the Python charter.
pub fn maybe_spawn(plane: &Path, workspace: &str, trees: &[PathBuf], binary: &Path) -> Refreshing {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs_f64())
        .unwrap_or(0.0);
    let decided = decide(&When {
        plane,
        trees,
        now,
        env: &|name| std::env::var(name).ok(),
        alive: &alive,
    });
    if decided != Decided::Due {
        return Refreshing::Declined(decided);
    }
    match spawn(plane, workspace, binary) {
        Err(why) => Refreshing::NotStarted { why },
        Ok(pid) => {
            // **Which** process is refreshing, not merely that one was started: that is what
            // makes `_alive` mean something and what moves the cooldown to the completion,
            // since the child rewrites this to empty when it finishes
            // (`glrefresh::refresh` → `write_lock(plane, None)`).
            glrefresh::write_lock(plane, Some(pid));
            Refreshing::Started { pid }
        }
    }
}

/// Start `charter gl-refresh -w <workspace>` in a process of its own, and answer with its pid.
///
/// **Its own process GROUP**, the same choice `charter gl-refresh --detach` makes and for the
/// same reason: a harness tears down a hook's process group when the turn ends, and the app
/// ends a chat's program group when the tab closes, so the group is what has to be left.
/// `Command::process_group` is safe where `setsid` would need a `pre_exec` closure, and this
/// workspace forbids `unsafe`.
///
/// **`--detach` is deliberately not used.** It would fork once more, and the pid this gets
/// back would be the middleman's — already gone by the time the lock is read, so `alive`
/// would answer "no refresh in flight" about one that is running. The pid written to the
/// lock has to be the pid of the process doing the work.
///
/// The plane is handed over explicitly, as `charter/util.py:child_env` hands it over, and for
/// the bigger half of `maybe_spawn`'s own argument about the workspace: a child left to walk
/// up from its own directory refreshed a DIFFERENT plane's cache (charter#527).
fn spawn(plane: &Path, workspace: &str, binary: &Path) -> Result<u32, String> {
    use std::process::{Command, Stdio};

    let mut command = Command::new(binary);
    command
        .arg("gl-refresh")
        .arg("-w")
        .arg(workspace)
        .current_dir(plane)
        .env("CHARTER_ROOT", plane)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = crate::forklock::spawn(&mut command).map_err(|why| why.to_string())?;
    let pid = child.id();
    reap(child);
    Ok(pid)
}

/// Wait for the refresh on a thread of its own, so it is reaped when it ends.
///
/// The app runs for hours and a refresh every two minutes is thirty zombies an hour otherwise
/// — and a zombie holds its pid, which is exactly the pid the lock names, so `alive` would
/// answer "still in flight" about a refresh that finished. The thread costs nothing until the
/// child goes, at most one exists at a time (the cooldown), and it is not a daemon: it ends
/// when the process it is waiting on does.
fn reap(mut child: std::process::Child) {
    // No thread to spare is not a failure worth reporting: the refresh still runs and still
    // writes the cache, and it is left unreaped rather than waited for here, because waiting
    // here would block the panel this was called from.
    let _ = std::thread::Builder::new()
        .name("charter-gl-refresh".into())
        .spawn(move || {
            let _ = child.wait();
        });
}

/// A file time as seconds since the epoch, the way Python's `st_mtime` reads.
fn epoch_seconds(when: SystemTime) -> f64 {
    match when.duration_since(UNIX_EPOCH) {
        Ok(since) => since.as_secs_f64(),
        Err(before) => -before.duration().as_secs_f64(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_alive_and_one_that_has_been_reaped_is_not() {
        assert!(alive(std::process::id()));

        let mut gone =
            crate::forklock::spawn(std::process::Command::new("/bin/sh").args(["-c", "exit 0"]))
                .expect("a program starts");
        let pid = gone.id();
        gone.wait().expect("it ends");

        assert!(!alive(pid), "a reaped pid is not a refresh in flight");
        assert!(!alive(0), "pid 0 is not a process charter may ask about");
    }

    #[test]
    fn the_brake_is_on_for_anything_that_is_not_blank() {
        let off = |value: Option<&str>| {
            let held = value.map(str::to_owned);
            background_checks_off(&|name| {
                if name == NO_BACKGROUND_CHECKS {
                    held.clone()
                } else {
                    None
                }
            })
        };
        // A request not to phone home, so every spelling of a value means the request.
        assert!(off(Some("1")));
        assert!(off(Some("0")));
        assert!(off(Some("false")));
        assert!(off(Some("no")));
        // Blank is how a shell spells taking a value away.
        assert!(!off(Some("")));
        assert!(!off(Some("  \t ")));
        assert!(!off(None));
    }
}
