//! The kill switch as the app holds it (OV-1): one switch for every plane and every window
//! this process has, and the watch that hears `charter stop --all` throw it from a terminal.
//!
//! The switch itself is the machine's marker (`charter_core::halt`). The app keeps a flag of
//! its own beside it so the title bar's stop holds on a machine with no config home to write a
//! marker into, and asks the marker as well on every start so a stop another process wrote is
//! in force from the moment it is written, before the watch has noticed it.
//!
//! **What it stops is every session**, shell tabs included: a harness can be started by hand
//! in a shell (ADR 0062), and a switch that left those running would stop only the agents
//! charter happened to know about. **What it refuses until re-armed is every start**, from
//! whatever asks for one — the operator's new chat, a relaunch putting a record back, a
//! handoff, a curation action — because each of them reaches [`crate::sessions::Sessions::open`],
//! and the refusal is there rather than in each caller.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use charter_core::halt::{self, By};

/// The event every window is sent when the switch is thrown or re-armed, carrying whether
/// agents are now stopped.
pub(crate) const CHANGED: &str = "kill-switch";

/// How often the watch asks whether another process threw the switch. The acceptance is five
/// seconds from the stop to every chat ended, and ending takes most of one; a stat twice a
/// second is nothing next to that.
pub const EVERY: Duration = Duration::from_millis(500);

/// Whether this machine's agents are stopped, as this process holds it.
pub struct Halt {
    /// The machine's config home, where the marker is. None on a machine with none: the switch
    /// still works, for as long as this process runs.
    config: Option<PathBuf>,
    /// Thrown in this process without a marker behind it: there is no config home, or the
    /// marker could not be written. Everything else the marker answers, so the watch, the
    /// window and a start can never disagree about a marker somebody took away by hand.
    thrown_here: AtomicBool,
}

impl Halt {
    /// The switch kept under `config`: thrown already if the marker is there.
    pub fn kept_in(config: Option<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            config,
            thrown_here: AtomicBool::new(false),
        })
    }

    /// A switch nobody has thrown, kept nowhere: what a `Sessions` a test builds is given.
    pub fn never() -> Arc<Self> {
        Self::kept_in(None)
    }

    /// Whether agents are stopped: this process threw the switch, or another one did.
    pub fn is_thrown(&self) -> bool {
        self.thrown_here.load(Ordering::SeqCst) || self.marker()
    }

    /// Throws the switch. A marker that cannot be written still stops this process, and the
    /// error says the stop will not outlive it.
    pub fn throw(&self, by: By) -> io::Result<()> {
        let written = match &self.config {
            Some(config) => halt::stop(config, by, now()),
            None => Ok(()),
        };
        if written.is_err() || self.config.is_none() {
            self.thrown_here.store(true, Ordering::SeqCst);
        }
        written
    }

    /// Re-arms it: chats may start again. Answers whether it had been thrown.
    pub fn rearm(&self) -> io::Result<bool> {
        let was = self.is_thrown();
        if let Some(config) = &self.config {
            halt::rearm(config, By::Window, now())?;
        }
        self.thrown_here.store(false, Ordering::SeqCst);
        Ok(was)
    }

    /// What the marker alone says, for the watch: the flag is this process's own and needs no
    /// watching.
    fn marker(&self) -> bool {
        self.config.as_deref().is_some_and(halt::halted)
    }
}

/// Watches the marker for another process throwing or re-arming the switch, and calls `changed`
/// with the new answer each time it moves, from a thread of its own, every `every`.
///
/// A thread that could not be started is said and is not fatal: the window's own control still
/// works, and a start is refused as soon as the marker is there whether or not anyone watches.
pub fn watch(halt: Arc<Halt>, every: Duration, changed: impl Fn(bool) + Send + 'static) {
    let mut was = halt.marker();
    let started = std::thread::Builder::new()
        .name("charter-kill-switch".into())
        .spawn(move || {
            loop {
                std::thread::sleep(every);
                let now = halt.marker();
                if now != was {
                    was = now;
                    changed(now);
                }
            }
        });
    if let Err(why) = started {
        eprintln!("charter: `charter stop --all` will not be heard by this app ({why})");
    }
}

/// Whether every agent is stopped, for a window drawing its title bar.
#[tauri::command]
#[specta::specta]
pub fn agents_stopped(planes: tauri::State<'_, crate::planes::Planes>) -> bool {
    planes.is_stopped()
}

/// The title bar's stop: every chat in every project, in every window, ended, and none started
/// until re-armed. Answers how many chats it stopped.
///
/// On a blocking thread, because ending takes up to half a second and the window must go on
/// drawing while it does. Every window is told once it is done; the window that pressed says
/// so at once without waiting.
#[tauri::command]
#[specta::specta]
pub async fn stop_every_agent(app: tauri::AppHandle) -> Result<u32, String> {
    use tauri::{Emitter, Manager};
    tauri::async_runtime::spawn_blocking(move || {
        let planes = app.state::<crate::planes::Planes>();
        let stopped = planes.stop_every_agent(By::Window);
        let _ = app.emit(CHANGED, true);
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
    planes.rearm()?;
    let _ = app.emit(CHANGED, false);
    Ok(())
}

/// Starts the watch for this app: a stop another process wrote ends every chat here and tells
/// every window; a re-arm tells them too.
pub fn hear(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    let halt = app.state::<crate::planes::Planes>().halt();
    let app = app.clone();
    watch(halt, EVERY, move |thrown| {
        let _ = app.emit(CHANGED, thrown);
        if thrown {
            app.state::<crate::planes::Planes>().end_every_chat();
        }
    });
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn a_stop_another_process_wrote_is_in_force_before_anything_watched_it() {
        let config = tempfile::tempdir().expect("a config home");
        let held = Halt::kept_in(Some(config.path().to_path_buf()));
        assert!(!held.is_thrown());

        halt::stop(config.path(), By::Cli, 1).expect("stopped from a terminal");

        assert!(held.is_thrown());
    }

    #[test]
    fn an_app_started_after_a_stop_starts_stopped() {
        let config = tempfile::tempdir().expect("a config home");
        halt::stop(config.path(), By::Cli, 1).expect("stopped from a terminal");

        assert!(Halt::kept_in(Some(config.path().to_path_buf())).is_thrown());
    }

    #[test]
    fn the_window_s_stop_holds_with_no_config_home_and_its_rearm_lets_go() {
        let held = Halt::never();
        held.throw(By::Window).expect("nothing to write");
        assert!(held.is_thrown());
        assert!(held.rearm().expect("nothing to remove"));
        assert!(!held.is_thrown());
    }

    #[test]
    fn a_rearm_in_the_window_takes_away_a_stop_the_command_line_wrote() {
        let config = tempfile::tempdir().expect("a config home");
        halt::stop(config.path(), By::Cli, 1).expect("stopped from a terminal");
        let held = Halt::kept_in(Some(config.path().to_path_buf()));

        assert!(held.rearm().expect("re-armed"));

        assert!(!held.is_thrown());
        assert!(!halt::halted(config.path()));
        let journal = halt::journal(config.path());
        assert_eq!(journal.last().expect("a line")["event"], "rearm");
        assert_eq!(journal.last().expect("a line")["by"], "window");
    }

    #[test]
    fn the_watch_hears_charter_stop_all_and_the_rearm_after_it() {
        let config = tempfile::tempdir().expect("a config home");
        let held = Halt::kept_in(Some(config.path().to_path_buf()));
        let (tell, heard) = mpsc::channel();
        watch(Arc::clone(&held), Duration::from_millis(20), move |now| {
            let _ = tell.send(now);
        });

        halt::stop(config.path(), By::Cli, 1).expect("stopped from a terminal");
        assert_eq!(heard.recv_timeout(Duration::from_secs(2)), Ok(true));

        held.rearm().expect("re-armed");
        assert_eq!(heard.recv_timeout(Duration::from_secs(2)), Ok(false));
    }
}
