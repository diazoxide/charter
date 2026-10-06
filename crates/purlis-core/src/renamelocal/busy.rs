//! Whether anything that may hold charter's folders is running (RN-5, D-RN5-11).
//!
//! A move made under a running charter splits its state between two names for good, since
//! rename-local never merges. So the check is conservative: a run refuses, moving nothing,
//! when ANY of these says something is running:
//!
//! * the app's single-instance endpoint answers, under the old and the purlis identifier;
//! * a hook socket answers, beside a project or in the fallback folders an app uses when it has
//!   no project open or the path is too long;
//! * another process of this user runs a program named like charter's own;
//! * a process of this build holds [`LOCK`] shared (the app for its life, `mcp` for a chat's).
//!
//! The first three see older builds too; the lock is for builds from this one on.

use std::fs::File;
use std::path::{Path, PathBuf};

use crate::names::{BUNDLE_ID, STATE_DIR};

/// The advisory lock in the config root (beside the config home's folder, so it is the same
/// file before and after that folder moves). Held shared by every long-running process of this
/// build; rename-local takes it exclusively.
pub const LOCK: &str = ".purlis.lock";

/// The program names a running charter goes by: both CLI names and the app's.
pub const PROGRAMS: &[&str] = &["purlis", "charter", "purlis-app", "charter-app"];

/// What a refusal says to do.
pub const QUIT_FIRST: &str = "quit every charter/purlis window, chat and terminal first";

fn open_lock(config_root: &Path) -> std::io::Result<File> {
    std::fs::create_dir_all(config_root)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    // Never through a link: the lock is a file of its own in the config root.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NOFOLLOW);
    options.open(config_root.join(LOCK))
}

/// Hold [`LOCK`] shared for as long as the returned file lives: what the app and `mcp` do, so
/// that no rename-local moves their folders under them. `None` when it could not be taken,
/// which only means a migration could start; nothing else depends on it.
pub fn hold_shared(config_root: &Path) -> Option<File> {
    let file = open_lock(config_root).ok()?;
    file.try_lock_shared().ok()?;
    Some(file)
}

/// [`LOCK`] taken exclusively for a migration, or why not.
pub(super) fn exclusive(config_root: &Path) -> Result<File, String> {
    let file = open_lock(config_root).map_err(|e| {
        format!(
            "{} could not be opened ({e}), so nothing could be moved safely",
            config_root.join(LOCK).display()
        )
    })?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => {
            Err("a running purlis holds this machine's config home".to_owned())
        }
        Err(std::fs::TryLockError::Error(e)) => Err(format!(
            "{} could not be locked ({e}), so nothing could be moved safely",
            config_root.join(LOCK).display()
        )),
    }
}

/// Whether something answers on the unix socket at `socket`. A socket nothing answers on is a
/// crashed process's leftover.
pub fn answers(socket: &Path) -> bool {
    #[cfg(unix)]
    {
        std::fs::symlink_metadata(socket).is_ok()
            && std::os::unix::net::UnixStream::connect(socket).is_ok()
    }
    #[cfg(not(unix))]
    {
        let _ = socket;
        false
    }
}

/// The hook sockets beside the project at `plane`, under each state folder spelling.
pub fn beside(plane: &Path) -> Vec<PathBuf> {
    STATE_DIR
        .spellings()
        .map(|state| plane.join(state).join("app").join("hooks.sock"))
        .collect()
}

/// The hook sockets in the fallback folders under `base` (`$XDG_RUNTIME_DIR` or the temporary
/// directory): `charter-<user>-<key>/hooks.sock` and its purlis spelling, for every user name,
/// since the name the app picked is not this process's to guess.
pub fn in_fallback(base: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(base) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            crate::names::BINARY
                .spellings()
                .any(|program| name.starts_with(&format!("{program}-")))
        })
        .map(|entry| entry.path().join("hooks.sock"))
        .collect()
}

/// Where the app's single-instance endpoints are on a machine: what [`Instances::of`] reads off
/// this one, and what a test hands in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Places {
    /// Whether this is macOS, where the endpoint is tauri-plugin-single-instance's socket.
    pub macos: bool,
    /// Whether this is Linux, where it is `instance.rs`'s lock file.
    pub linux: bool,
    /// The folder that socket is in (`/tmp`).
    pub socket_dir: PathBuf,
    /// `$XDG_RUNTIME_DIR`.
    pub runtime: Option<PathBuf>,
    /// The OS's local data folder, the lock's fallback.
    pub data_local: Option<PathBuf>,
    pub uid: u32,
}

impl Places {
    /// This machine's.
    pub fn here() -> Self {
        #[cfg(unix)]
        let uid = rustix::process::getuid().as_raw();
        #[cfg(not(unix))]
        let uid = 0;
        Self {
            macos: cfg!(target_os = "macos"),
            linux: cfg!(target_os = "linux"),
            socket_dir: PathBuf::from("/tmp"),
            runtime: std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
            data_local: dirs::data_local_dir(),
            uid,
        }
    }
}

/// The app's single-instance endpoints under every identifier it has had, but `own`'s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Instances {
    pub sockets: Vec<PathBuf>,
    pub locks: Vec<PathBuf>,
}

impl Instances {
    /// At `places`, leaving out the identifier `own`: the app that asks at its launch holds its
    /// own lock and socket by then, and its single-instance handoff already showed no second
    /// launch of it runs. Every other identifier's are still asked.
    pub fn of(places: &Places, own: Option<&str>) -> Self {
        let mut found = Self::default();
        for id in BUNDLE_ID.spellings().filter(|id| Some(*id) != own) {
            if places.macos {
                // tauri-plugin-single-instance names it for the identifier with `.` and `-`
                // made `_`: `/tmp/dev_charter_app_si.sock`.
                let name = id.replace(['.', '-'], "_");
                found
                    .sockets
                    .push(places.socket_dir.join(format!("{name}_si.sock")));
            }
            if places.linux {
                if let Some(dir) = &places.runtime {
                    found.locks.push(dir.join(format!("{id}.lock")));
                }
                if let Some(data) = &places.data_local {
                    found
                        .locks
                        .push(data.join(id).join(format!("{id}-{}.lock", places.uid)));
                }
            }
        }
        found
    }

    /// Why one of them says the app is running, or `None`.
    pub fn running(&self) -> Option<String> {
        if let Some(socket) = self.sockets.iter().find(|socket| answers(socket)) {
            return Some(format!("the app is running ({} answers)", socket.display()));
        }
        self.locks
            .iter()
            .find(|lock| lock_held(lock))
            .map(|lock| format!("the app is running ({} is held)", lock.display()))
    }
}

/// The app under a name it had before, running beside this one (RN-9), asked by the app whose
/// identifier is `own` at its launch: its old identifier's single-instance endpoint answering
/// (the socket on macOS, the lock on Linux). The two identifiers are two single-instance names,
/// so neither app hands a launch to the other, and two apps would run chats on one project.
/// Only the purlis app asks: a scenario build's own identifier (`dev.purlis.app.e2e`) asks
/// nothing, so the operator's app never stops a test run.
pub fn older_app(places: &Places, own: &str) -> Option<String> {
    if own != BUNDLE_ID.write {
        return None;
    }
    Instances::of(places, Some(own)).running()
}

/// [`older_app`] at the app's launch, unless the process that started this one is the app's own
/// program (`parent`, its program as `ps` names it): that is the update's restart, where the old
/// app is this one's parent, still letting go as it exits, and never a second app.
pub fn older_app_at_launch(places: &Places, own: &str, parent: Option<&str>) -> Option<String> {
    if started_by_the_app(parent) {
        return None;
    }
    older_app(places, own)
}

/// Whether `parent`, a program as `ps` names it (a path or a bare name), is the app's own
/// binary ([`crate::secrets::keyhold::APP_BINARY`]).
pub fn started_by_the_app(parent: Option<&str>) -> bool {
    parent
        .and_then(|program| Path::new(program.trim()).file_name())
        .is_some_and(|name| name == crate::secrets::keyhold::APP_BINARY)
}

/// The program of the process that started this one, as `ps` names it, or `None` when it
/// cannot be told.
pub fn parent_program() -> Option<String> {
    #[cfg(unix)]
    {
        let parent = rustix::process::getppid()?.as_raw_nonzero().get();
        let mut ps = std::process::Command::new("ps");
        ps.args(["-o", "comm=", "-p", &parent.to_string()]);
        let out = crate::forklock::output(&mut ps).ok()?;
        let program = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        (out.status.success() && !program.is_empty()).then_some(program)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// What a launch that found the old app running says before it ends.
pub const OLDER_APP_RUNNING: &str = "purlis: the app is already running under its old name \
     (charter.app). Quit it, then open purlis.app again. If you installed purlis.app beside it, \
     delete charter.app: purlis.app takes its place.";

/// Whether the lock file at `path` is held by a process: it exists and cannot be taken.
pub fn lock_held(path: &Path) -> bool {
    let Ok(file) = std::fs::OpenOptions::new().read(true).open(path) else {
        return false;
    };
    matches!(file.try_lock(), Err(std::fs::TryLockError::WouldBlock))
}

/// The processes in `listing` — `ps -A -o pid=,ppid=,uid=,comm=` — of user `uid` whose program
/// is named like charter's own, other than `me` and `parent`: each as `<name> (pid <pid>)`.
pub fn others_named(listing: &str, uid: u32, me: u32, parent: u32) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid: u32 = fields.next()?.parse().ok()?;
            let _ppid: u32 = fields.next()?.parse().ok()?;
            let owner: u32 = fields.next()?.parse().ok()?;
            let command: Vec<&str> = fields.collect();
            let command = command.join(" ");
            let name = Path::new(&command)
                .file_name()?
                .to_string_lossy()
                .into_owned();
            (owner == uid && pid != me && pid != parent && PROGRAMS.contains(&name.as_str()))
                .then(|| format!("{name} (pid {pid})"))
        })
        .collect()
}

/// What `ps` answered: whether it ran and exited 0, and what it printed.
pub type Listing = std::io::Result<(bool, String)>;

/// The processes named like charter's own in `listing` ([`others_named`]), or why the listing
/// cannot be trusted: `ps` did not start, did not exit 0, or left out `me` — a listing that
/// does not show this very process shows nothing reliably, and a check that cannot look must
/// refuse rather than pass.
pub fn others_in(listing: Listing, uid: u32, me: u32, parent: u32) -> Result<Vec<String>, String> {
    let (ok, text) = listing.map_err(|e| format!("the process list could not be read ({e})"))?;
    if !ok {
        return Err("the process list could not be read (ps failed)".to_owned());
    }
    let shows_me = text
        .lines()
        .any(|line| line.split_whitespace().next() == Some(me.to_string().as_str()));
    if !shows_me {
        return Err(
            "the process list could not be read (it does not show this process)".to_owned(),
        );
    }
    Ok(others_named(&text, uid, me, parent))
}

#[cfg(unix)]
fn other_processes() -> Result<Vec<String>, String> {
    let mut ps = std::process::Command::new("ps");
    ps.args(["-A", "-o", "pid=,ppid=,uid=,comm="]);
    let listing = crate::forklock::output(&mut ps).map(|out| {
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    });
    let parent = rustix::process::getppid().map_or(0, |pid| pid.as_raw_nonzero().get() as u32);
    others_in(
        listing,
        rustix::process::getuid().as_raw(),
        std::process::id(),
        parent,
    )
}

#[cfg(not(unix))]
fn other_processes() -> Result<Vec<String>, String> {
    Ok(Vec::new())
}

/// Why a migration may not run now, or `None`: the module's checks, on this machine, for the
/// projects `planes`, asked by the app whose identifier is `own` (or by a terminal: `None`).
///
/// **A test build (`fenced`) asks only its own projects' sockets.** Its suite runs many charter
/// processes at once, and the machine's own app may be running beside it; neither is the
/// fixture's to wait for. The machine-wide checks are tested through [`Instances::of`] on
/// handed-in [`Places`], [`others_in`], [`in_fallback`], [`answers`] and [`lock_held`].
pub fn why(own: Option<&str>, planes: &[PathBuf]) -> Option<String> {
    if let Some(socket) = planes
        .iter()
        .flat_map(|plane| beside(plane))
        .find(|socket| answers(socket))
    {
        return Some(format!("a running purlis answers on {}", socket.display()));
    }
    if cfg!(feature = "fenced") {
        return None;
    }
    if let Some(why) = Instances::of(&Places::here(), own).running() {
        return Some(why);
    }
    let bases = [
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        Some(std::env::temp_dir()),
    ];
    if let Some(socket) = bases
        .iter()
        .flatten()
        .flat_map(|base| in_fallback(base))
        .find(|socket| answers(socket))
    {
        return Some(format!("a running purlis answers on {}", socket.display()));
    }
    match other_processes() {
        Err(why) => Some(why),
        Ok(others) if !others.is_empty() => {
            Some(format!("purlis is running: {}", others.join(", ")))
        }
        Ok(_) => None,
    }
}
