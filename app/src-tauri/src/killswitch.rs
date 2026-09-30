//! The kill switch as the app holds it (OV-1, ADR 0069): one switch for every plane and every
//! window this process has, and the watch that hears `charter stop --all` throw it.
//!
//! **While the app runs, the switch in its memory is the authority.** The files on disk
//! (`charter_core::halt`) are how another process throws it and how the next launch learns it
//! was thrown. They can stop this process but never re-arm it: a stop on disk is in force the
//! moment it is written, and a marker that goes away while agents are stopped is put back and
//! journaled as a tamper. Only [`KillSwitch::rearm`], which the window's control alone calls,
//! lets chats start again.
//!
//! **The residual.** A process running as the operator's user can rewrite both files, and
//! one that forges a re-arm in the journal and removes the marker while the app is not running
//! is believed at the next launch. ADR 0069 says what closes that and where.
//!
//! **What it stops is every session charter started**, shell tabs included: a harness can be
//! started by hand in a shell (ADR 0062), and leaving those running would stop only the agents
//! charter happened to know about. **What it refuses until re-armed is every chat and harness
//! start**, from whatever asks — the operator's new chat, a relaunch putting a record back, a
//! handoff, a curation action — at [`crate::sessions::Sessions::open`]. **A shell the operator
//! opens from the window is let through**: looking at what the agents did is a human act, and
//! the switch is there to stop agents, not to lock the operator out of their own machine.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use charter_core::halt::{self, Actor};
use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};

/// The event every window is sent when the switch moves, carrying whether agents are stopped.
pub(crate) const CHANGED: &str = "kill-switch";

/// How long a burst of writes to charter's directory is folded for. A stop is a marker and a
/// journal line, two renames; this makes them one look at the switch.
const FOLDED_FOR: Duration = Duration::from_millis(100);

/// What a look at the files did to the switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moved {
    /// Nothing: the files agree with the switch.
    Nothing,
    /// Another process stopped every agent; this one now has too.
    Stopped,
    /// The marker went away while agents were stopped. It was put back and journaled.
    Tampered,
}

/// Whether this machine's agents are stopped, as this process holds it.
pub struct KillSwitch {
    /// The machine's config home, where the files are. None on a machine with none: the switch
    /// still works, for as long as this process runs.
    config: Option<PathBuf>,
    /// The authority while this process runs. Held across every change so a re-arm and a look
    /// at the files can never interleave — the re-arm's own removal of the marker would
    /// otherwise read as a tamper.
    stopped: Mutex<bool>,
}

impl KillSwitch {
    /// The switch kept under `config`: stopped already if the files say so.
    pub fn kept_in(config: Option<PathBuf>) -> Arc<Self> {
        let stopped = config.as_deref().is_some_and(halt::stopped_on_disk);
        Arc::new(Self {
            config,
            stopped: Mutex::new(stopped),
        })
    }

    /// A switch nobody has thrown, kept nowhere: what a `Sessions` a test builds is given.
    pub fn unthrown() -> Arc<Self> {
        Self::kept_in(None)
    }

    /// Whether agents are stopped: by this process, or by the files. A stop on disk counts the
    /// moment it is written, before the watch has seen it; files saying otherwise never count.
    pub fn is_stopped(&self) -> bool {
        *self.held() || self.config.as_deref().is_some_and(halt::stopped_on_disk)
    }

    /// Stops every agent. Stopped in this process whatever happens on disk; the error is the
    /// sentence saying the stop was not kept, for the operator to see.
    pub fn stop(&self, by: Actor) -> Result<(), String> {
        let mut stopped = self.held();
        *stopped = true;
        let Some(config) = &self.config else {
            return Err(
                "every agent is stopped in this app, but there is no config home to keep the \
                 stop in: a relaunch will start chats again"
                    .to_owned(),
            );
        };
        halt::stop(config, by, halt::now())
            .map_err(|not_kept| format!("every agent is stopped in this app, but {not_kept}"))
    }

    /// Lets chats start again: the window's alone. Answers whether agents had been stopped. A
    /// re-arm the files could not take is refused, and agents stay stopped.
    pub fn rearm(&self) -> Result<bool, String> {
        let mut stopped = self.held();
        let was = *stopped || self.config.as_deref().is_some_and(halt::stopped_on_disk);
        if let Some(config) = &self.config {
            halt::rearm(config, halt::now()).map_err(|why| {
                format!("agents are still stopped: the re-arm was not kept ({why})")
            })?;
        }
        *stopped = false;
        Ok(was)
    }

    /// Looks at the files and brings the switch in line with them, in the one direction it may
    /// move: towards stopped.
    pub fn look(&self) -> Moved {
        let Some(config) = &self.config else {
            return Moved::Nothing;
        };
        let mut stopped = self.held();
        if *stopped {
            if halt::marker_present(config) {
                return Moved::Nothing;
            }
            if let Err(why) = halt::restore(config, halt::now()) {
                eprintln!(
                    "charter: the kill switch's marker was removed and could not be put back ({why})"
                );
            }
            return Moved::Tampered;
        }
        if halt::stopped_on_disk(config) {
            *stopped = true;
            return Moved::Stopped;
        }
        Moved::Nothing
    }

    /// The directory the files are in, for the watch.
    fn directory(&self) -> Option<PathBuf> {
        self.config
            .as_deref()
            .and_then(|config| halt::directory(config).ok())
    }

    fn held(&self) -> MutexGuard<'_, bool> {
        self.stopped.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A watch on the switch's files. Dropping it stops it.
pub struct Watch {
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
}

/// Watches charter's directory, non-recursively, and calls `moved` after every batch of writes
/// to the switch's files with what [`KillSwitch::look`] made of them — and once at the start,
/// for a stop written while the app was coming up. None where there is no directory to watch.
pub fn watch(
    switch: Arc<KillSwitch>,
    moved: impl Fn(Moved) + Send + 'static,
) -> Option<notify::Result<Watch>> {
    let dir = switch.directory()?;
    Some(watch_in(&dir, switch, moved))
}

fn watch_in(
    dir: &Path,
    switch: Arc<KillSwitch>,
    moved: impl Fn(Moved) + Send + 'static,
) -> notify::Result<Watch> {
    let looking = Arc::clone(&switch);
    let mut debouncer = new_debouncer(FOLDED_FOR, None, move |batch: DebounceEventResult| {
        let Ok(events) = batch else { return };
        let ours = events.iter().any(|event| {
            !matches!(event.kind, notify::EventKind::Access(_))
                && event
                    .paths
                    .iter()
                    .filter_map(|path| path.file_name())
                    .any(halt::concerns)
        });
        if ours {
            moved(looking.look());
        }
    })?;
    debouncer.watch(dir, RecursiveMode::NonRecursive)?;
    // Anything written before the watch began is looked at once, here.
    let _ = switch.look();
    Ok(Watch {
        _debouncer: debouncer,
    })
}

/// Whether every agent is stopped, for a window drawing its title bar.
#[tauri::command]
#[specta::specta]
pub fn agents_stopped(planes: tauri::State<'_, crate::planes::Planes>) -> bool {
    planes.is_stopped()
}

/// The title bar's stop: every chat and shell charter started, in every project and window,
/// ended, and no chat started until re-armed. Answers how many it stopped, or the sentence
/// saying the stop holds here but was not kept on disk.
///
/// On a blocking thread, because ending takes about a second and the window must go on drawing
/// while it does. Every window is told the switch's state once it is done, whichever way.
#[tauri::command]
#[specta::specta]
pub async fn stop_every_agent(app: tauri::AppHandle) -> Result<u32, String> {
    use tauri::{Emitter, Manager};
    tauri::async_runtime::spawn_blocking(move || {
        let planes = app.state::<crate::planes::Planes>();
        let stopped = planes.stop_every_agent(Actor::Window);
        let _ = app.emit(CHANGED, planes.is_stopped());
        stopped.map(|count| u32::try_from(count).unwrap_or(u32::MAX))
    })
    .await
    .map_err(|err| format!("the stop did not finish: {err}"))?
}

/// The title bar's re-arm: chats may start again. Nothing that was stopped is restarted.
#[tauri::command]
#[specta::specta]
pub fn rearm_agents(
    app: tauri::AppHandle,
    planes: tauri::State<'_, crate::planes::Planes>,
) -> Result<(), String> {
    use tauri::Emitter;
    let rearmed = planes.rearm();
    let _ = app.emit(CHANGED, planes.is_stopped());
    rearmed.map(|_| ())
}

/// Starts the watch for this app and keeps it for the app's life: a stop another process wrote
/// ends every chat here, and every window is told how the switch stands.
pub fn hear(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    let planes = app.state::<crate::planes::Planes>();
    let told = app.clone();
    match watch(planes.kill_switch(), move |moved| {
        let planes = told.state::<crate::planes::Planes>();
        if moved == Moved::Stopped {
            planes.end_every_chat();
        }
        let _ = told.emit(CHANGED, planes.is_stopped());
    }) {
        Some(Ok(watch)) => {
            app.manage(watch);
        }
        Some(Err(why)) => eprintln!(
            "charter: `charter stop --all` will not be heard by this app until it restarts ({why})"
        ),
        None => {}
    }
    // A stop written while the app was coming up, which the watch's first look caught.
    if planes.is_stopped() {
        planes.end_every_chat();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use charter_core::halt::{Entry, Event};

    fn a_config_home() -> tempfile::TempDir {
        tempfile::tempdir().expect("a config home")
    }

    fn switch_in(config: &Path) -> Arc<KillSwitch> {
        KillSwitch::kept_in(Some(config.to_path_buf()))
    }

    fn events(config: &Path) -> Vec<(Event, Actor)> {
        halt::journal(config)
            .into_iter()
            .map(|Entry { event, by, .. }| (event, by))
            .collect()
    }

    #[test]
    fn a_stop_another_process_wrote_is_in_force_before_anything_looked_at_it() {
        let config = a_config_home();
        let switch = switch_in(config.path());

        halt::stop(config.path(), Actor::Cli, 1).expect("stopped from a terminal");

        assert!(switch.is_stopped());
    }

    #[test]
    fn an_app_started_after_a_stop_starts_stopped() {
        let config = a_config_home();
        halt::stop(config.path(), Actor::Cli, 1).expect("stopped from a terminal");

        assert!(switch_in(config.path()).is_stopped());
    }

    #[test]
    fn an_app_started_after_a_stop_whose_marker_was_removed_starts_stopped() {
        let config = a_config_home();
        halt::stop(config.path(), Actor::Cli, 1).expect("stopped from a terminal");
        std::fs::remove_file(halt::marker(config.path())).expect("removed by an agent");

        assert!(switch_in(config.path()).is_stopped());
    }

    #[test]
    fn a_marker_removed_while_stopped_does_not_re_arm_and_is_put_back_as_a_tamper() {
        let config = a_config_home();
        let switch = switch_in(config.path());
        switch.stop(Actor::Window).expect("stopped");

        std::fs::remove_file(halt::marker(config.path())).expect("removed by an agent");
        // Even with a forged re-arm in the journal: the switch in memory is the authority.
        let forged = serde_json::to_string(&Entry {
            at: 2,
            event: Event::Rearm,
            by: Actor::Window,
        })
        .unwrap();
        let journal = halt::journal_path(config.path());
        let mut text = std::fs::read_to_string(&journal).unwrap();
        text.push_str(&forged);
        text.push('\n');
        std::fs::write(&journal, text).unwrap();

        assert_eq!(switch.look(), Moved::Tampered);
        assert!(switch.is_stopped());
        assert!(
            halt::marker_present(config.path()),
            "the marker was not put back"
        );
        assert_eq!(
            events(config.path()).last(),
            Some(&(Event::Tamper, Actor::App))
        );
    }

    #[test]
    fn a_look_after_a_stop_on_disk_stops_this_process_and_the_marker_then_cannot_undo_it() {
        let config = a_config_home();
        let switch = switch_in(config.path());
        halt::stop(config.path(), Actor::Cli, 1).expect("stopped from a terminal");

        assert_eq!(switch.look(), Moved::Stopped);
        assert_eq!(switch.look(), Moved::Nothing, "a second look moves nothing");
    }

    #[test]
    fn only_a_re_arm_lets_chats_start_again_and_its_own_removal_is_no_tamper() {
        let config = a_config_home();
        let switch = switch_in(config.path());
        switch.stop(Actor::Window).expect("stopped");

        assert!(switch.rearm().expect("re-armed"));

        assert!(!switch.is_stopped());
        assert_eq!(switch.look(), Moved::Nothing);
        assert_eq!(
            events(config.path()),
            [(Event::Stop, Actor::Window), (Event::Rearm, Actor::Window)]
        );
    }

    #[test]
    fn the_window_s_stop_holds_with_no_config_home_and_says_it_will_not_outlive_the_app() {
        let switch = KillSwitch::unthrown();
        let said = switch
            .stop(Actor::Window)
            .expect_err("nothing to keep it in");
        assert!(said.contains("a relaunch will start chats again"), "{said}");
        assert!(switch.is_stopped());
        assert!(switch.rearm().expect("nothing to remove"));
        assert!(!switch.is_stopped());
    }

    #[cfg(unix)]
    #[test]
    fn a_stop_defeated_in_advance_by_a_chmod_still_stops_this_app_and_says_so() {
        use std::os::unix::fs::PermissionsExt;
        let config = a_config_home();
        let dir = charter_core::machine::dir(config.path());
        std::fs::create_dir_all(&dir).expect("charter's directory");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).expect("chmod");
        let switch = switch_in(config.path());

        let said = switch.stop(Actor::Window);

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("chmod");
        let said = said.expect_err("nothing could be written");
        assert!(
            said.contains("every agent is stopped in this app"),
            "{said}"
        );
        assert!(switch.is_stopped());
    }

    #[test]
    fn the_watch_hears_charter_stop_all_and_puts_back_a_marker_taken_away() {
        let config = a_config_home();
        let switch = switch_in(config.path());
        let (tell, heard) = mpsc::channel();
        let _watch = watch(Arc::clone(&switch), move |moved| {
            let _ = tell.send(moved);
        })
        .expect("a directory to watch")
        .expect("watching");
        let patience = Duration::from_secs(10);
        let next = |wanted: Moved| loop {
            let moved = heard
                .recv_timeout(patience)
                .expect("the watch said nothing");
            if moved == wanted {
                break;
            }
        };

        halt::stop(config.path(), Actor::Cli, 1).expect("stopped from a terminal");
        next(Moved::Stopped);
        assert!(switch.is_stopped());

        std::fs::remove_file(halt::marker(config.path())).expect("removed by an agent");
        next(Moved::Tampered);
        assert!(halt::marker_present(config.path()));
        assert!(switch.is_stopped());
    }
}
