//! A chat's permission prompt in the window's needs-you list, answered there (HP-6).
//!
//! A Claude Code chat's `PermissionRequest` hook hands its ask to this project's hook channel
//! and waits (`purlis_core::hookwire::permission`). The ask is held in the project's
//! [`HookAsks`], the window is told the asks it now has ([`EVENT`]), and the operator's choice
//! goes back on that hook ([`answer_ask`]), so the harness carries it out without its pane
//! having focus. Unanswered, the hook decides nothing and the pane asks as it always did.
//!
//! **The window answers, and nothing else does.** [`answer_ask`] is a Tauri command the
//! window invokes, admitted as `local-ui`; it is never served on the link to `charterd`
//! (`purlis_session_protocol::ui::WINDOW_ONLY`), and the session protocol refuses its own
//! `answer`. On the hook channel the host reads asks and writes back only the window's choice,
//! and the hook believes a reply only from a listener that is its own ancestor
//! (`purlis_same_user::admit_host`). An answer names its project, chat and ask, and lands only
//! on that chat's ask, once.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use purlis_core::harness::asks::{Admitted, Answerer, Raised};
use purlis_core::harness::hooked::HookAsks;
use purlis_core::harness::model::{Action, ChoiceKind};
use purlis_core::hookwire::Permitting;

use crate::planes::{PlaneId, Planes};

/// The event the window is told a project's asks on: [`Asking`].
pub const EVENT: &str = "asks-changed";

/// Every ask a project holds open, as the window lists them: the whole list each time, so the
/// window never assembles it from events it might have missed one of.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Asking {
    pub plane: PlaneId,
    pub asks: Vec<Shown>,
}

/// One ask, as its row in the needs-you list draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Shown {
    /// The chat that asked.
    pub session: u32,
    /// The ask's id, which its answer names.
    pub ask: String,
    /// What it asks, in one line, every credential shape masked.
    pub says: String,
    /// The answers it offers, in the harness's order and words.
    pub options: Vec<Offered>,
}

/// One answer an ask offers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Offered {
    pub id: String,
    pub label: String,
    /// Whether choosing it lets the call run.
    pub allows: bool,
}

/// Told a project's asks each time they change.
pub type Teller = Arc<dyn Fn(Asking) + Send + Sync + 'static>;

/// Where a project's [`Teller`] goes once there is one: a slot, filled after the hook channel
/// is listening, as the project's other listeners are (`hooks::Hooks`).
pub type Telling = Arc<Mutex<Option<Teller>>>;

/// The asks `hooks` holds now, as the window lists them for `plane`.
pub fn asking(plane: &PlaneId, hooks: &HookAsks) -> Asking {
    Asking {
        plane: plane.clone(),
        asks: hooks
            .pending(Instant::now())
            .into_iter()
            .filter_map(shown)
            .collect(),
    }
}

/// An ask as its row draws it, or none for one whose chat is no chat of this project's.
fn shown(raised: Raised) -> Option<Shown> {
    let session = raised.chat.parse().ok()?;
    let ask = raised.ask;
    let said = ask.summary.as_str();
    let says = if said.is_empty() {
        match &ask.action {
            Action::Command { line } => format!("Run {line}"),
            Action::Edit { path } => format!("Change {path}"),
            Action::Tool { name, .. } => format!("Use {name}"),
            Action::Unsaid => "Asks for your permission".to_owned(),
        }
    } else {
        said.to_owned()
    };
    Some(Shown {
        session,
        ask: raised.id.to_string(),
        says: purlis_core::harness::model::Summary::of(&says)
            .as_str()
            .to_owned(),
        options: ask
            .options
            .into_iter()
            .map(|option| Offered {
                allows: option.kind == ChoiceKind::Allow,
                id: option.id,
                label: option.label,
            })
            .collect(),
    })
}

/// Tells the window `plane`'s asks, where something listens.
pub fn tell(plane: &PlaneId, hooks: &HookAsks, telling: &Telling) {
    let teller = telling
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(teller) = teller {
        teller(asking(plane, hooks));
    }
}

/// What hears a permission hook's ask on `plane`'s channel: held in `hooks` until it is
/// answered, its hook goes away, or its deadline passes, and the window told each change.
pub fn permitting(plane: PlaneId, hooks: Arc<HookAsks>, telling: Telling) -> Permitting {
    let told = Arc::clone(&hooks);
    purlis_core::hookwire::permission::held_in(
        hooks,
        Arc::new(move || tell(&plane, &told, &telling)),
    )
}

/// The asks a project holds open now: what the window lists before any [`EVENT`] arrives.
#[tauri::command]
#[specta::specta]
pub fn pending_asks(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<Asking, String> {
    Ok(planes.held(&plane)?.asks())
}

/// Answers chat `session`'s ask `ask` with its option `option`, as the operator in this window.
///
/// The answer goes back on the channel the ask came on, the chat's own permission hook, and
/// the harness carries it out; nothing is typed into the chat. The first answer wins: one that
/// comes too late, twice, or for another chat's ask is refused with a sentence saying why.
#[tauri::command]
#[specta::specta]
pub fn answer_ask(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    session: u32,
    ask: String,
    option: String,
) -> Result<(), String> {
    planes.held(&plane)?.answer_ask(session, &ask, &option)
}

/// The window, as an answerer: a Tauri command is invoked only by the app's own window, which
/// is the `local-ui` scope (ADR 0068 §5).
pub fn the_window() -> Answerer {
    Answerer::admitted(Admitted::LocalUi).expect("the window is a human scope")
}
