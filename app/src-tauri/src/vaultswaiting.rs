//! The vaults the launch's keychain copy left waiting, and the window's way to finish them
//! (#1306).
//!
//! At launch the app copies each project's keychain items to the purlis names with the
//! Keychain's dialogs off (`renamelocal::keychain`), so an item macOS would ask about (one an
//! earlier build made) is not read, and its vault stays where it was, whole and working. The
//! launch's answer names those vaults ([`renamelocal::Waiting`]); this holds them for the life of
//! the app, and the window draws a Notice that offers to finish moving them. Only on that press
//! does the copy run with the dialogs on, so the system asks once for each item while the person
//! is expecting it.
//!
//! Held in memory, never written down: every launch copies quietly again and finds what still
//! waits, so the list is never older than this launch.

use std::sync::Mutex;

use purlis_core::renamelocal::{self, Local, Seams, Waiting};
use tauri::Manager;

/// What the launch left waiting, and since the window finished some, what still does.
///
/// **The list's lock is only ever held for a moment**: `vaults_to_move` runs on the main thread,
/// and a finish waits on the person for every Keychain dialog. One press at a time is a lock of
/// its own ([`Self::pressing`]), held for the whole finish; the list is read out before it and
/// written back after.
#[derive(Debug, Default)]
pub struct VaultsWaiting {
    list: Mutex<Vec<Waiting>>,
    pressing: Mutex<()>,
    /// Whether this app holds the config home's shared lock (`renamelocal::busy::LOCK`), which
    /// is what keeps a terminal's run or undo from starting during a finish.
    holds_the_config_home: bool,
}

impl VaultsWaiting {
    pub fn of(waiting: Vec<Waiting>, holds_the_config_home: bool) -> Self {
        Self {
            list: Mutex::new(waiting),
            pressing: Mutex::new(()),
            holds_the_config_home,
        }
    }

    fn list(&self) -> std::sync::MutexGuard<'_, Vec<Waiting>> {
        self.list.lock().unwrap_or_else(|held| held.into_inner())
    }

    fn now(&self) -> Option<VaultsToMove> {
        summary(&self.list())
    }

    /// `finish` run over what waits, one press at a time, and what still waits after it kept.
    /// The list is free to be read meanwhile.
    fn finishing(
        &self,
        finish: impl FnOnce(&[Waiting]) -> renamelocal::Moved,
    ) -> Result<FinishedMoving, String> {
        if !self.holds_the_config_home {
            tracing::warn!(
                "purlis: finishing the vaults' move was refused: this app does not hold the \
                 config home's lock, so a terminal's migrate could run at the same time"
            );
            return Err(REFUSED.to_owned());
        }
        let _one_at_a_time = self
            .pressing
            .lock()
            .unwrap_or_else(|held| held.into_inner());
        let waiting = self.list().clone();
        let moved = finish(&waiting);
        for line in &moved.said {
            tracing::info!("purlis: rename-local: {line}");
        }
        if let Some(why) = &moved.refused {
            tracing::warn!("purlis: finishing the vaults' move was refused: {why}");
            return Err(REFUSED.to_owned());
        }
        let left = summary(&moved.waiting);
        *self.list() = moved.waiting;
        Ok(FinishedMoving {
            left,
            failed: u32::try_from(moved.said.iter().filter(|l| l.starts_with('✗')).count())
                .unwrap_or(u32::MAX),
        })
    }
}

/// What the window says when a finish moved nothing; the log says why.
const REFUSED: &str = "Nothing was moved; the app's log says why.";

/// What the window says about the vaults that wait.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct VaultsToMove {
    /// How many vaults wait: each counted once, whether its keyring items, its identity record
    /// or both wait.
    pub vaults: u32,
    /// How many items they hold: the most times the system asks when they are finished.
    pub items: u32,
}

/// What finishing them came to. What each step said is in the app's log.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct FinishedMoving {
    /// What still waits, if anything: an item the person did not allow keeps its vault here.
    pub left: Option<VaultsToMove>,
    /// How many vaults or records could not be moved this time.
    pub failed: u32,
}

fn summary(waiting: &[Waiting]) -> Option<VaultsToMove> {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let mut vaults: Vec<(&std::path::Path, &str)> = waiting
        .iter()
        .map(|waits| (waits.plane.as_path(), waits.vault.as_str()))
        .collect();
    vaults.sort_unstable();
    vaults.dedup();
    (!waiting.is_empty()).then(|| VaultsToMove {
        vaults: count(vaults.len()),
        items: count(waiting.iter().map(|waits| waits.items).sum()),
    })
}

/// The vaults this launch's keychain copy left waiting, or nothing.
#[tauri::command]
#[specta::specta]
pub fn vaults_to_move(waiting: tauri::State<'_, VaultsWaiting>) -> Option<VaultsToMove> {
    waiting.now()
}

/// Finish moving the vaults that wait, on the person's press: the copy with the Keychain's
/// dialogs on, so the system asks once for each item. On a blocking thread, because each ask
/// waits for the person; one press at a time. Refused when this app does not hold the config
/// home's lock.
#[tauri::command]
#[specta::specta]
pub async fn finish_moving_vaults(app: tauri::AppHandle) -> Result<FinishedMoving, String> {
    let identifier = app.config().identifier.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<VaultsWaiting>().finishing(|waiting| {
            let Some(mut local) = Local::of_this_machine(&[]) else {
                return renamelocal::Moved {
                    refused: Some(
                        "purlis cannot tell where this machine's config home is, so there is \
                         nowhere to journal a move"
                            .to_owned(),
                    ),
                    ..renamelocal::Moved::default()
                };
            };
            local.logs = None;
            local.own_app = Some(identifier);
            renamelocal::finish(&local, &Seams::real(), waiting)
        })
    })
    .await
    .map_err(|err| format!("moving the vaults did not finish: {err}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::time::Duration;

    fn waits(vault: &str, identity: bool, items: usize) -> Waiting {
        Waiting {
            plane: "/p".into(),
            vault: vault.to_owned(),
            identity,
            items,
            hold: false,
        }
    }

    #[test]
    fn nothing_waiting_says_nothing() {
        assert_eq!(VaultsWaiting::default().now(), None);
    }

    #[test]
    fn the_window_is_told_how_many_vaults_wait_and_how_many_items_they_hold() {
        let waiting = VaultsWaiting::of(vec![waits("ops", false, 3), waits("team", true, 1)], true);
        assert_eq!(
            waiting.now(),
            Some(VaultsToMove {
                vaults: 2,
                items: 4
            })
        );
    }

    #[test]
    fn a_vault_whose_items_and_identity_both_wait_is_one_vault() {
        let waiting = VaultsWaiting::of(vec![waits("ops", false, 2), waits("ops", true, 1)], true);
        assert_eq!(
            waiting.now(),
            Some(VaultsToMove {
                vaults: 1,
                items: 3
            })
        );
    }

    #[test]
    fn what_waits_is_read_while_a_finish_waits_on_the_person() {
        let waiting = Arc::new(VaultsWaiting::of(vec![waits("ops", false, 2)], true));
        let (started, has_started) = mpsc::channel();
        let (answer, answered) = mpsc::channel::<()>();
        let finishing = {
            let waiting = waiting.clone();
            std::thread::spawn(move || {
                waiting.finishing(|_| {
                    started.send(()).unwrap();
                    // The Keychain's dialog, waiting on the person.
                    answered.recv().unwrap();
                    renamelocal::Moved::default()
                })
            })
        };
        has_started.recv_timeout(Duration::from_secs(10)).unwrap();

        // The main thread's read, during the finish: answered at once.
        let (read, was_read) = mpsc::channel();
        {
            let waiting = waiting.clone();
            std::thread::spawn(move || read.send(waiting.now()).unwrap());
        }
        let now = was_read.recv_timeout(Duration::from_secs(5));

        answer.send(()).unwrap();
        let finished = finishing.join().unwrap().unwrap();
        assert_eq!(
            now,
            Ok(Some(VaultsToMove {
                vaults: 1,
                items: 2
            }))
        );
        assert_eq!(finished.left, None);
        assert_eq!(waiting.now(), None);
    }

    #[test]
    fn a_finish_is_refused_when_the_app_does_not_hold_the_config_home() {
        let waiting = VaultsWaiting::of(vec![waits("ops", false, 2)], false);
        let mut ran = false;

        let refused = waiting.finishing(|_| {
            ran = true;
            renamelocal::Moved::default()
        });

        assert_eq!(refused, Err(REFUSED.to_owned()));
        assert!(!ran);
        assert!(waiting.now().is_some());
    }
}
