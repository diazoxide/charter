//! One charter per user, whether or not there is a session bus.
//!
//! On Linux, `tauri-plugin-single-instance` is a name on the D-Bus session bus, and it is what
//! `hookwire` relies on to remove a stale hook socket safely: "there is no second live app
//! whose socket this could be". A launch that starts without the bus — because the desktop
//! portal is silent (`portal.rs`, charter-app#24), or because there is no bus at all — has no
//! name to hold, and before this nothing stopped a second app on the same plane.
//!
//! So the app also holds an advisory lock on a file of its own, with `flock` through
//! [`std::fs::File::try_lock`] — the standard way a Unix program says "one of me". The kernel
//! lets go of it when the process ends however it ends, so a killed app leaves nothing stale
//! behind. It is taken in `setup`, after the plugin has already handed an ordinary second
//! launch to the running app, so what reaches it is only a launch the bus could not hand over.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

/// Why this launch does not hold the lock.
#[derive(Debug)]
pub enum NotHeld {
    /// Another charter holds it.
    Taken,
    /// The file could not be opened or locked; the launch goes on without the guarantee.
    Failed(std::io::Error),
}

/// Where the lock lives: `$XDG_RUNTIME_DIR`, which is per user, per machine and cleared at
/// logout, and otherwise `fallback` — the app's own per-user data directory, or the temporary
/// directory when even that cannot be named.
///
/// Keyed by the app's identifier, as the single-instance name is, so a scenario build
/// (`dev.purlis.app.e2e`) and the operator's charter do not refuse each other; and in the
/// fallback by `user` too, because the temporary directory is every user's.
pub fn path_for(
    identifier: &str,
    user: u32,
    runtime_dir: Option<&Path>,
    fallback: &Path,
) -> PathBuf {
    match runtime_dir.filter(|dir| dir.is_absolute() && dir.is_dir()) {
        Some(dir) => dir.join(format!("{identifier}.lock")),
        None => fallback.join(format!("{identifier}-{user}.lock")),
    }
}

/// One charter per user: takes the lock for `app`, holds it for the app's life, and ends a
/// launch that finds it taken. Called at the top of `setup`, after the single-instance plugin
/// has already handed an ordinary second launch over; a lock that cannot be taken at all is
/// said, and the launch goes on without the guarantee.
#[cfg(target_os = "linux")]
pub fn one_per_user(app: &tauri::App) {
    use tauri::Manager;

    let fallback = app
        .path()
        .app_local_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir());
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let lock = path_for(
        &app.config().identifier,
        rustix::process::getuid().as_raw(),
        runtime.as_deref(),
        &fallback,
    );
    match hold(&lock) {
        Ok(held) => {
            app.manage(Instance::holding(held));
        }
        Err(NotHeld::Taken) => {
            tracing::info!("{ALREADY_RUNNING}");
            app.handle().cleanup_before_exit();
            std::process::exit(0);
        }
        Err(NotHeld::Failed(why)) => tracing::warn!(
            "charter: could not take {} ({why}); going on without the guard against a second \
             charter",
            lock.display()
        ),
    }
}

/// Lets go of the lock at `Exit`, first, so a restart finds it free.
pub fn let_go_at_exit(app: &tauri::AppHandle) {
    use tauri::Manager;

    if let Some(instance) = app.try_state::<Instance>() {
        instance.let_go();
    }
}

/// Takes the lock at `path`, for as long as the returned file is open.
pub fn hold(path: &Path) -> Result<File, NotHeld> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(NotHeld::Failed)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(NotHeld::Failed)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(NotHeld::Taken),
        Err(TryLockError::Error(err)) => Err(NotHeld::Failed(err)),
    }
}

/// The lock, held for the app's life and let go of at `Exit` — before Tauri's restart to
/// update starts the new process, which would otherwise find it taken.
pub struct Instance(std::sync::Mutex<Option<File>>);

impl Instance {
    pub fn holding(file: File) -> Self {
        Self(std::sync::Mutex::new(Some(file)))
    }

    /// Unlocked before it is closed. A program forked on another thread holds a copy of the
    /// descriptor until it execs, and a copy shares the lock: closing this one alone would
    /// leave the lock held until then, and a restart in that moment would find it taken.
    /// Unlocking lets go of it for every copy at once.
    pub fn let_go(&self) {
        if let Ok(mut held) = self.0.lock()
            && let Some(file) = held.take()
        {
            let _ = file.unlock();
        }
    }
}

/// What a launch that found another charter running says before it ends.
pub const ALREADY_RUNNING: &str = "charter: charter is already running for this user, and \
     without a session bus this launch cannot be handed to it — switch to its window. \
     charter-app#24.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_launch_holds_the_lock_and_a_second_is_refused() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("charter.lock");

        let _first = hold(&path).expect("the first launch holds it");

        assert!(
            matches!(hold(&path), Err(NotHeld::Taken)),
            "a second launch was let in beside the first"
        );
    }

    #[test]
    fn the_lock_is_free_again_once_the_app_that_held_it_lets_go() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("charter.lock");
        let instance = Instance::holding(hold(&path).expect("held"));

        instance.let_go();

        hold(&path).expect("the next launch holds it");
    }

    #[test]
    fn the_lock_is_free_again_even_while_a_program_forked_before_the_let_go_has_not_started() {
        // A program started in a terminal is forked, and until its exec the child holds a copy
        // of every descriptor this process has, the lock's included, close-on-exec or not. A
        // copy shares the lock, so closing this process's descriptor alone would leave it held
        // for as long as that child takes to exec — under load, long enough for a restart to
        // find it taken. A duplicate is that child's copy, deterministically.
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("charter.lock");
        let held = hold(&path).expect("held");
        let forked_child_s_copy = held.try_clone().expect("a copy of the descriptor");
        let instance = Instance::holding(held);

        instance.let_go();

        hold(&path).expect("the next launch holds it");
        drop(forked_child_s_copy);
    }

    #[test]
    fn a_lock_file_left_behind_by_an_app_that_is_gone_is_no_obstacle() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("charter.lock");
        std::fs::write(&path, "").expect("a stale file");

        hold(&path).expect("a file with no lock on it is free");
    }

    #[test]
    fn the_lock_lives_in_the_runtime_directory_and_is_keyed_by_the_identifier() {
        let runtime = tempfile::tempdir().expect("a directory");
        let fallback = Path::new("/tmp");

        assert_eq!(
            path_for("dev.purlis.app", 1000, Some(runtime.path()), fallback),
            runtime.path().join("dev.purlis.app.lock")
        );
        assert_ne!(
            path_for("dev.purlis.app", 1000, Some(runtime.path()), fallback),
            path_for("dev.purlis.app.e2e", 1000, Some(runtime.path()), fallback),
            "a scenario build and the operator's charter would refuse each other"
        );
    }

    /// The app's identity is the purlis one (RN-9): the identifier its lock, its single-instance
    /// name (a D-Bus name on Linux, `/tmp/dev_purlis_app_si.sock` on macOS) and its log folder
    /// are keyed by, the product name its bundle and `.deb` are called by, and the old
    /// identifier the rename-local busy check still asks about.
    #[test]
    fn the_app_is_dev_purlis_app_and_its_scenario_build_beside_it() {
        let conf = |name: &str| -> serde_json::Value {
            let path = concat!(env!("CARGO_MANIFEST_DIR"), "/").to_owned() + name;
            serde_json::from_str(&std::fs::read_to_string(path).expect("beside the crate"))
                .expect("json")
        };
        let release = conf("tauri.conf.json");
        assert_eq!(release["identifier"], charter_core::names::BUNDLE_ID.write);
        assert_eq!(release["productName"], charter_core::names::BINARY.write);
        assert_eq!(
            conf("tauri.e2e.conf.json")["identifier"],
            format!("{}.e2e", charter_core::names::BUNDLE_ID.write)
        );
        let runtime = tempfile::tempdir().expect("a directory");
        assert_eq!(
            path_for(
                release["identifier"].as_str().unwrap(),
                1000,
                Some(runtime.path()),
                Path::new("/tmp")
            ),
            runtime.path().join("dev.purlis.app.lock")
        );
        let old = charter_core::renamelocal::busy::Instances::of(
            &charter_core::renamelocal::busy::Places {
                linux: true,
                runtime: Some(runtime.path().to_owned()),
                ..Default::default()
            },
            Some(charter_core::names::BUNDLE_ID.write),
        );
        assert_eq!(old.locks, [runtime.path().join("dev.charter.app.lock")]);
    }

    /// The `.deb` takes the old package's place: `apt` removes `charter` rather than
    /// refusing two packages that both ship the app's files.
    #[test]
    fn the_deb_replaces_the_package_it_was_called_before() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("json");
        let deb = &conf["bundle"]["linux"]["deb"];
        let old = charter_core::names::BINARY.reads[0];
        assert_eq!(deb["replaces"], serde_json::json!([old]));
        assert_eq!(deb["conflicts"], serde_json::json!([old]));
    }

    /// The fallback can be a directory every user shares — the temporary directory, when the
    /// app's own cannot be named — so the name says whose charter and which one it is.
    #[test]
    fn with_no_usable_runtime_directory_the_lock_is_keyed_by_the_user_and_the_identifier() {
        let fallback = Path::new("/tmp");

        assert_eq!(
            path_for("dev.purlis.app", 1000, None, fallback),
            fallback.join("dev.purlis.app-1000.lock")
        );
        assert_eq!(
            path_for(
                "dev.purlis.app",
                1000,
                Some(Path::new("relative")),
                fallback
            ),
            fallback.join("dev.purlis.app-1000.lock")
        );
        assert_ne!(
            path_for("dev.purlis.app", 1000, None, fallback),
            path_for("dev.purlis.app", 1001, None, fallback),
            "two users on one machine would refuse each other"
        );
        assert_ne!(
            path_for("dev.purlis.app", 1000, None, fallback),
            path_for("dev.purlis.app.e2e", 1000, None, fallback),
        );
    }
}
