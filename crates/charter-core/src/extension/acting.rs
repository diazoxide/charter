//! What an extension may be asked to DO: the actions on its rows, and the commands it adds to the
//! palette (charter-app#341, ADR 0053).
//!
//! **An action is the extension's own verb, never charter's.** `panel::NO_VERB` still refuses a
//! row that names one of charter's catalogue rows; what a row may carry now is the id of an
//! action this extension's manifest declares, and pressing it asks the extension's own program
//! — one request, *run action `<id>` on `<subject>`*, through the same gate, deadline and kill
//! as a view's question ([`crate::executor::Executor::act`]). ADR 0043's amendment of
//! 2026-09-25 records the difference.
//!
//! **Asking first is charter's, and a delete is always asked about.** An action says whether
//! charter asks the operator first (`confirm`), and whether it deletes (`deletes`). An action
//! that deletes is asked about whatever `confirm` says ([`Action::asks_first`]), and the
//! executor refuses to run an action that asks first without the operator's yes, so no window
//! can forget to ask. An extension that deletes without declaring it is not stopped — it runs as
//! the operator does (`super::RUNS_AS_YOU`) — and the executor reports what it saw deleted
//! (`crate::executor`'s watch on the plane).

use super::{ok_in_a_part_id, title_of};

/// The most actions one extension may declare. Each one is a button charter may draw on a row;
/// more than a row has room for is more than an operator reads before pressing one.
const MOST_ACTIONS: usize = 16;

/// The most palette commands one extension may add. The palette is charter's, and one
/// extension's commands are a handful among a few hundred rows.
const MOST_COMMANDS: usize = 16;

/// The keys a declared action may carry.
const ACTION_KEYS: [&str; 4] = ["id", "title", "confirm", "deletes"];

/// The keys a declared palette command may carry: an id, a title, and exactly one of what it
/// opens or runs.
const COMMAND_KEYS: [&str; 4] = ["id", "title", "view", "action"];

/// One action an extension declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// One segment, unique within the extension. It is what a row names and what the program is
    /// asked to run.
    pub id: String,
    /// What its button says.
    pub title: String,
    /// Whether the manifest asks charter to ask the operator first.
    pub confirm: bool,
    /// Whether the manifest says it deletes something.
    pub deletes: bool,
}

impl Action {
    /// Whether charter asks the operator before running it: when the manifest says so, and
    /// **always for an action that deletes, whatever `confirm` says** (charter-app#336, story 22).
    pub fn asks_first(&self) -> bool {
        self.confirm || self.deletes
    }
}

/// What a palette command does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Does {
    /// Opens the view with this id, as the view's own button does.
    Open(String),
    /// Runs the action with this id on nothing in particular — no view and no row.
    Run(String),
}

/// One command an extension adds to the palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// One segment, unique within the extension.
    pub id: String,
    /// What the row says, after the extension's name — the palette names it
    /// `<extension's name>: <title>`, so where it came from is on the row.
    pub title: String,
    pub does: Does,
}

/// The actions a manifest's `contributes.actions` declares, or why charter will not read them.
pub(super) fn actions_of(
    value: &serde_json::Value,
    program: Option<&str>,
) -> Result<Vec<Action>, String> {
    let list = value
        .as_array()
        .ok_or("has a 'contributes.actions' that is not an array")?;
    if list.is_empty() {
        return Err(
            "declares no actions under 'contributes.actions', so there is nothing it \
                    would do"
                .into(),
        );
    }
    if program.is_none() {
        return Err(
            "declares actions and no program ('runs') to run them, so charter would draw \
             buttons that can never do anything"
                .into(),
        );
    }
    if list.len() > MOST_ACTIONS {
        return Err(format!(
            "declares {} actions, and charter offers at most {MOST_ACTIONS} from one extension",
            list.len()
        ));
    }
    let mut actions: Vec<Action> = Vec::with_capacity(list.len());
    for (at, raw) in list.iter().enumerate() {
        let object = raw
            .as_object()
            .ok_or_else(|| format!("declares an action at {at} that is not an object"))?;
        only(object, &ACTION_KEYS, "an action", at)?;
        let id = id_of(object, "an action", at)?;
        if actions.iter().any(|seen| seen.id == id) {
            return Err(format!(
                "declares two actions called {id:?}, and an action's id is how a row names it"
            ));
        }
        let title = title_of(object.get("title"), "the action", id)?;
        // **Said, never assumed.** An action that asks nothing on a click is a choice the
        // operator reads in the prompt; a manifest that left it out has not made it.
        let confirm = object
            .get("confirm")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| {
                format!(
                    "declares the action {id:?} without 'confirm' — an action says whether \
                     charter asks you first, true or false"
                )
            })?;
        let deletes = match object.get("deletes") {
            None => false,
            Some(value) => value.as_bool().ok_or_else(|| {
                format!("declares the action {id:?} with a 'deletes' that is not true or false")
            })?,
        };
        actions.push(Action {
            id: id.to_owned(),
            title,
            confirm,
            deletes,
        });
    }
    Ok(actions)
}

/// The palette commands a manifest's `contributes.palette` declares, held to the views and
/// actions the same manifest declares.
pub(super) fn palette_of(
    value: &serde_json::Value,
    views: &[super::View],
    actions: &[Action],
) -> Result<Vec<Command>, String> {
    let list = value
        .as_array()
        .ok_or("has a 'contributes.palette' that is not an array")?;
    if list.is_empty() {
        return Err(
            "declares no commands under 'contributes.palette', so there is nothing it \
                    would add"
                .into(),
        );
    }
    if list.len() > MOST_COMMANDS {
        return Err(format!(
            "declares {} palette commands, and charter adds at most {MOST_COMMANDS} from one \
             extension",
            list.len()
        ));
    }
    let mut commands: Vec<Command> = Vec::with_capacity(list.len());
    for (at, raw) in list.iter().enumerate() {
        let object = raw
            .as_object()
            .ok_or_else(|| format!("declares a palette command at {at} that is not an object"))?;
        only(object, &COMMAND_KEYS, "a palette command", at)?;
        let id = id_of(object, "a palette command", at)?;
        if commands.iter().any(|seen| seen.id == id) {
            return Err(format!("declares two palette commands called {id:?}"));
        }
        let title = title_of(object.get("title"), "the palette command", id)?;
        let word = |key: &str| object.get(key).and_then(serde_json::Value::as_str);
        let does = match (word("view"), word("action")) {
            (Some(view), None) => {
                if !views.iter().any(|it| it.id == view) {
                    return Err(format!(
                        "declares the palette command {id:?} opening the view {view:?}, which it \
                         does not declare"
                    ));
                }
                Does::Open(view.to_owned())
            }
            (None, Some(action)) => {
                if !actions.iter().any(|it| it.id == action) {
                    return Err(format!(
                        "declares the palette command {id:?} running the action {action:?}, \
                         which it does not declare"
                    ));
                }
                Does::Run(action.to_owned())
            }
            _ => {
                return Err(format!(
                    "declares the palette command {id:?} without exactly one of 'view' or \
                     'action' — a command opens one of its views or runs one of its actions"
                ));
            }
        };
        commands.push(Command {
            id: id.to_owned(),
            title,
            does,
        });
    }
    Ok(commands)
}

/// One line for the approval prompt, for each action.
pub(super) fn declares_action(action: &Action) -> String {
    let asks = match (action.deletes, action.confirm) {
        (true, _) => "it deletes, so charter always asks you first",
        (false, true) => "charter asks you first",
        (false, false) => "it runs when you press it, without asking",
    };
    format!("an action on its rows, “{}” — {asks}", action.title)
}

/// One line for the approval prompt, for each palette command.
pub(super) fn declares_command(command: &Command, name: &str, actions: &[Action]) -> String {
    let does = match &command.does {
        Does::Open(view) => format!("opens its view '{view}'"),
        Does::Run(id) => {
            let title = actions
                .iter()
                .find(|it| &it.id == id)
                .map_or(id.as_str(), |it| it.title.as_str());
            format!("runs its action “{title}”")
        }
    };
    format!("a palette command, “{name}: {}” — {does}", command.title)
}

fn only(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
    what: &str,
    at: usize,
) -> Result<(), String> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!(
                "declares {what} at {at} carrying {key:?}, which is not part of what it may say \
                 — {what} is {}",
                allowed.join(", ")
            ));
        }
    }
    Ok(())
}

fn id_of<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    what: &str,
    at: usize,
) -> Result<&'a str, String> {
    let id = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("declares {what} at {at} with no id"))?;
    if !ok_in_a_part_id(id) {
        return Err(format!(
            "declares {what} with the id {id:?}, and its id is letters, digits, '-' and '_', \
             starting with a letter or a digit"
        ));
    }
    Ok(id)
}
