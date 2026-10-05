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
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(config_root.join(LOCK))
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
            Err("a running charter holds this machine's config home".to_owned())
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

/// The single-instance endpoints of the app under each identifier it has had.
fn instances() -> Vec<PathBuf> {
    let ids = BUNDLE_ID.spellings();
    if cfg!(target_os = "macos") {
        // tauri-plugin-single-instance's socket on macOS.
        ids.map(|id| PathBuf::from(format!("/tmp/{id}_si.sock")))
            .collect()
    } else {
        Vec::new()
    }
}

/// The app's one-per-user lock files on Linux (`app/src-tauri/src/instance.rs`), under each
/// identifier: the runtime directory's, and the fallback in the app's data folder.
fn instance_locks() -> Vec<PathBuf> {
    if !cfg!(target_os = "linux") {
        return Vec::new();
    }
    #[cfg(unix)]
    let uid = rustix::process::getuid().as_raw();
    #[cfg(not(unix))]
    let uid = 0;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let mut locks = Vec::new();
    for id in BUNDLE_ID.spellings() {
        if let Some(dir) = &runtime {
            locks.push(dir.join(format!("{id}.lock")));
        }
        if let Some(data) = dirs::data_local_dir() {
            locks.push(data.join(id).join(format!("{id}-{uid}.lock")));
        }
    }
    locks
}

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

#[cfg(unix)]
fn other_processes() -> Vec<String> {
    let mut ps = std::process::Command::new("ps");
    ps.args(["-A", "-o", "pid=,ppid=,uid=,comm="]);
    let Ok(out) = crate::forklock::output(&mut ps) else {
        return Vec::new();
    };
    let me = std::process::id();
    let parent = rustix::process::getppid().map_or(0, |pid| pid.as_raw_nonzero().get() as u32);
    others_named(
        &String::from_utf8_lossy(&out.stdout),
        rustix::process::getuid().as_raw(),
        me,
        parent,
    )
}

#[cfg(not(unix))]
fn other_processes() -> Vec<String> {
    Vec::new()
}

/// Why a migration may not run now, or `None`: the module's checks, on this machine, for the
/// projects `planes`.
///
/// **A test build (`fenced`) asks only its own projects' sockets.** Its suite runs many charter
/// processes at once, and the machine's own app may be running beside it; neither is the
/// fixture's to wait for. The machine-wide checks are tested through [`others_named`],
/// [`in_fallback`], [`answers`] and [`lock_held`].
pub fn why(planes: &[PathBuf]) -> Option<String> {
    if let Some(socket) = planes
        .iter()
        .flat_map(|plane| beside(plane))
        .find(|socket| answers(socket))
    {
        return Some(format!("a running charter answers on {}", socket.display()));
    }
    if cfg!(feature = "fenced") {
        return None;
    }
    if let Some(socket) = instances().into_iter().find(|socket| answers(socket)) {
        return Some(format!("the app is running ({} answers)", socket.display()));
    }
    if let Some(lock) = instance_locks().into_iter().find(|lock| lock_held(lock)) {
        return Some(format!("the app is running ({} is held)", lock.display()));
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
        return Some(format!("a running charter answers on {}", socket.display()));
    }
    let others = other_processes();
    if !others.is_empty() {
        return Some(format!("charter is running: {}", others.join(", ")));
    }
    None
}
