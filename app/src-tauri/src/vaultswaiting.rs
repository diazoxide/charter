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

use charter_core::renamelocal::{self, Local, Seams, Waiting};
use tauri::Manager;

/// What the launch left waiting, and since the window finished some, what still does.
#[derive(Debug, Default)]
pub struct VaultsWaiting(Mutex<Vec<Waiting>>);

impl VaultsWaiting {
    pub fn of(waiting: Vec<Waiting>) -> Self {
        Self(Mutex::new(waiting))
    }

    fn now(&self) -> Option<VaultsToMove> {
        summary(&self.0.lock().unwrap_or_else(|held| held.into_inner()))
    }
}

/// What the window says about the vaults that wait.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct VaultsToMove {
    /// How many wait: keyring vaults and vaults' identity records.
    pub vaults: u32,
    /// How many items they hold: the most times the system asks when they are finished.
    pub items: u32,
}

/// What finishing them came to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct FinishedMoving {
    /// What still waits, if anything: an item the person did not allow keeps its vault here.
    pub left: Option<VaultsToMove>,
    /// One line a step, as `purlis migrate` says them.
    pub said: Vec<String>,
}

fn summary(waiting: &[Waiting]) -> Option<VaultsToMove> {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    (!waiting.is_empty()).then(|| VaultsToMove {
        vaults: count(waiting.len()),
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
/// waits for the person; one press at a time.
#[tauri::command]
#[specta::specta]
pub async fn finish_moving_vaults(app: tauri::AppHandle) -> Result<FinishedMoving, String> {
    let identifier = app.config().identifier.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<VaultsWaiting>();
        let mut waiting = state.0.lock().unwrap_or_else(|held| held.into_inner());
        let Some(mut local) = Local::of_this_machine(&[]) else {
            return Err(
                "charter cannot tell where this machine's config home is, so there is nowhere \
                 to journal a move; nothing was moved"
                    .to_owned(),
            );
        };
        local.logs = None;
        local.own_app = Some(identifier);
        let moved = renamelocal::finish(&local, &Seams::real(), &waiting);
        if let Some(why) = moved.refused {
            return Err(why);
        }
        *waiting = moved.waiting;
        Ok(FinishedMoving {
            left: summary(&waiting),
            said: moved.said,
        })
    })
    .await
    .map_err(|err| format!("moving the vaults did not finish: {err}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waits(vault: &str, identity: bool, items: usize) -> Waiting {
        Waiting {
            plane: "/p".into(),
            vault: vault.to_owned(),
            identity,
            items,
        }
    }

    #[test]
    fn nothing_waiting_says_nothing() {
        assert_eq!(VaultsWaiting::default().now(), None);
    }

    #[test]
    fn the_window_is_told_how_many_vaults_wait_and_how_many_items_they_hold() {
        let waiting = VaultsWaiting::of(vec![waits("ops", false, 3), waits("team", true, 1)]);
        assert_eq!(
            waiting.now(),
            Some(VaultsToMove {
                vaults: 2,
                items: 4
            })
        );
    }
}
