//! A level-2 ask: a harness's permission hook, held open on the hook channel until the operator
//! answers it from the window, and answered back on that same hook (HP-6, research track 04
//! §5.2).
//!
//! Claude Code runs `charter hook permissionrequest` when it would ask for a permission. The
//! hook hands its payload to the host on the hook channel and waits; the host raises it in the
//! shared [`Asks`] and holds the connection. The operator's answer goes back on that connection
//! as the option they chose, and the hook prints the harness's own decision for it
//! ([`decision`]). Nothing is typed into the chat's terminal, so the pane need not have focus.
//!
//! **What decides, when nobody answers.** The ask carries a deadline below the hook's timeout
//! ([`HOOK_TIMEOUT`], [`super::model::Deadline::below_hook_timeout`]). Past it, and whenever the
//! hook goes away first, the hook prints nothing and the harness's own prompt decides in the
//! pane. charter never allows or denies on its own.
//!
//! **Only a human answers** ([`Answerer`], V16, V75). On the hook channel a chat can raise an
//! ask and never answer one: the host reads no answer there, and the hook believes a reply
//! only from a listener that is its own ancestor ([`crate::hookwire::permission`]).
//!
//! **An answer is bound to its chat.** The registry is keyed by ask and chat together
//! ([`Asks::answer`]), so an answer naming another chat, or one replayed after the first, is
//! refused and never reaches a hook.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::asks::{Answerer, Applied, AskId, Asks, Raised, Refused};
use super::model::{Action, Ask, Choice, ChoiceKind, ChoiceScope};

/// The word after `charter hook` that a permission hook runs.
pub const WORD: &str = "permissionrequest";

/// How long the harness waits on charter's permission hook before it gives up on it: Claude
/// Code's own default for a hook. The ask's deadline sits [`super::model::HOOK_MARGIN`] below it.
pub const HOOK_TIMEOUT: Duration = Duration::from_secs(60);

/// The most open hook asks one chat may hold at once, as at level 3 ([`crate::acp`]): one past
/// it is not raised, and the harness's own prompt decides it.
pub const MOST_OPEN_ASKS: usize = crate::acp::MOST_OPEN_ASKS;

/// The most a permission hook's payload may be. A larger one is not raised and not cut short:
/// a command shown for approval without its tail is not the command that runs.
pub const MOST_ASK_BYTES: usize = crate::acp::MOST_ASK_BYTES;

/// The harness whose permission hook an ask came on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Claude Code's `PermissionRequest`.
    ClaudeCode,
}

/// The ask a permission hook's `payload` is, as the window shows it and the host holds it.
///
/// **"From now on" is never offered** (V28c): a Claude Code suggestion saved to a settings file
/// would write into the worktree's harness settings, which a chat can then read as standing
/// permission. Allow and deny for this call, and what lasts only for the session, are kept.
///
/// **Nothing that lets the call run is offered on a display that hides part of it** (HP-6
/// review). The window shows an ask's one-line summary, which collapses newlines, cuts at
/// [`super::model::SUMMARY_WIDTH`] and masks credential shapes; a command whose harmless head
/// is all that shows is not the command that runs. Unless the summary IS the action, word for
/// word ([`shown_in_full`]), the allowing options are dropped and the window offers deny, or
/// the chat's own pane, which shows it whole. An edit shows only its path, never what it
/// writes, so it is never allowed from here. A shell call that carries anything beyond its
/// command, description and timeout (`dangerouslyDisableSandbox`, `run_in_background`, a field
/// a later harness adds) runs differently from what the line says, so it is not allowed from
/// here either.
///
/// **No option that switches the session's permission mode is ever offered** (D-88o): only
/// allow and deny for this call, and rules for this session, come from the window. A mode
/// switch (`bypassPermissions`, `acceptEdits`, any `setMode`) stays in the chat's own pane.
pub fn ask(source: Source, payload: &Value) -> Ask {
    let mut ask = match source {
        Source::ClaudeCode => super::asked::claude_permission_request(payload, HOOK_TIMEOUT),
    };
    // An option whose own words would draw otherwise (a rule's text, a directory's path) says
    // something other than what it grants, so it is not offered either.
    ask.options.retain(|option| {
        option.scope != ChoiceScope::Always
            && !switches_the_mode(source, payload, option)
            && !option.label.chars().any(drawn_otherwise)
    });
    if !shown_in_full(&ask) || !plain_shell_input(&ask, payload) {
        ask.options
            .retain(|option| option.kind != ChoiceKind::Allow);
    }
    ask
}

/// Whether choosing `option` would switch the session's permission mode: a Claude Code
/// suggestion of type `setMode`.
fn switches_the_mode(source: Source, payload: &Value, option: &Choice) -> bool {
    match source {
        Source::ClaudeCode => option
            .id
            .strip_prefix("suggestion:")
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| payload["permission_suggestions"].get(n))
            .is_some_and(|entry| entry["type"].as_str() == Some("setMode")),
    }
}

/// The keys a shell call's input may carry and still run as its line says.
const PLAIN_SHELL_KEYS: [&str; 3] = ["command", "description", "timeout"];

/// Whether a command ask's tool input holds nothing beyond [`PLAIN_SHELL_KEYS`]. Any other
/// action has its whole input on the line already, so this is about commands alone.
fn plain_shell_input(ask: &Ask, payload: &Value) -> bool {
    if !matches!(ask.action, Action::Command { .. }) {
        return true;
    }
    payload["tool_input"].as_object().is_some_and(|input| {
        input
            .keys()
            .all(|key| PLAIN_SHELL_KEYS.contains(&key.as_str()))
    })
}

/// Whether `ask`'s summary says everything its action would run, exactly, and draws it as it
/// is: the command line or the tool's whole input, with nothing collapsed, cut or masked, and
/// no character a window draws as nothing or draws elsewhere (a control, a zero-width or
/// format character, a bidirectional override). Never for an edit, whose summary names only
/// the file, nor for an ask that says nothing.
pub fn shown_in_full(ask: &Ask) -> bool {
    let whole = match &ask.action {
        Action::Command { line } => format!("Run a command: {line}"),
        Action::Tool { name, input } => format!("Use {name}: {input}"),
        Action::Edit { .. } | Action::Unsaid => return false,
    };
    ask.summary.as_str() == whole && !whole.chars().any(drawn_otherwise)
}

/// Whether a window draws `c` as nothing, or moves what is around it: a control character, a
/// format character (general category Cf, which holds the zero-width and bidirectional
/// controls), or any other Default_Ignorable_Code_Point (variation selectors, fillers, tags).
pub fn drawn_otherwise(c: char) -> bool {
    use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};

    c.is_control() || c.general_category() == GeneralCategory::Format || ignorable_beyond_cf(c)
}

/// Default_Ignorable_Code_Point beyond the Cf characters: Other_Default_Ignorable_Code_Point and
/// the variation selectors, as Unicode's DerivedCoreProperties.txt derives the property.
fn ignorable_beyond_cf(c: char) -> bool {
    matches!(
        c,
        '\u{034F}'
            | '\u{115F}'..='\u{1160}'
            | '\u{17B4}'..='\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{2065}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FFA0}'
            | '\u{FFF0}'..='\u{FFF8}'
            | '\u{E0000}'..='\u{E0FFF}'
    )
}

/// What the hook prints for the option `chosen` of `payload`'s ask: the harness's own
/// decision, as one line of JSON. `None` for an option the ask does not offer, and the hook then
/// prints nothing, so the harness's prompt decides.
pub fn decision(source: Source, payload: &Value, chosen: &str) -> Option<String> {
    let choice = ask(source, payload)
        .options
        .into_iter()
        .find(|option| option.id == chosen)?;
    let decision = match source {
        Source::ClaudeCode => claude_decision(payload, &choice)?,
    };
    Some(
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": decision,
            }
        })
        .to_string(),
    )
}

/// Claude Code's `decision` for `choice`: `allow` or `deny`, and for a suggestion, the
/// suggestion itself as the permission update it asks for.
fn claude_decision(payload: &Value, choice: &Choice) -> Option<Value> {
    let behavior = match choice.kind {
        ChoiceKind::Allow => "allow",
        ChoiceKind::Reject | ChoiceKind::Cancel => "deny",
    };
    let mut decision = serde_json::json!({ "behavior": behavior });
    if let Some(n) = choice.id.strip_prefix("suggestion:") {
        let n: usize = n.parse().ok()?;
        let entry = payload["permission_suggestions"].get(n)?.clone();
        decision["updatedPermissions"] = Value::Array(vec![entry]);
    }
    if behavior == "deny" {
        decision["message"] = Value::from("The operator denied this in purlis's window.");
    }
    Some(decision)
}

/// Why a permission hook's ask was not raised. The hook then decides nothing, and the harness's
/// own prompt asks in the pane.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotRaised {
    #[error("this chat already has {MOST_OPEN_ASKS} asks waiting")]
    TooMany,
    #[error("this ask is larger than {MOST_ASK_BYTES} bytes")]
    TooBig,
}

/// A raised hook ask, and where its answer arrives: the option the operator chose.
#[derive(Debug)]
pub struct Held {
    pub raised: Raised,
    pub answered: mpsc::Receiver<Choice>,
}

/// The hook asks a host holds open, over the [`Asks`] it shares with every other source.
#[derive(Debug)]
pub struct HookAsks {
    asks: Arc<Asks>,
    /// Each open hook ask, the chat that raised it, and where its answer goes.
    waiting: Mutex<HashMap<AskId, Waiter>>,
}

#[derive(Debug)]
struct Waiter {
    chat: String,
    tell: mpsc::Sender<Choice>,
}

impl HookAsks {
    pub fn new(asks: Arc<Asks>) -> Self {
        Self {
            asks,
            waiting: Mutex::new(HashMap::new()),
        }
    }

    fn waiting(&self) -> std::sync::MutexGuard<'_, HashMap<AskId, Waiter>> {
        self.waiting
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Raises the ask `payload` is, from chat `chat`'s permission hook, at `now`.
    pub fn raise(
        &self,
        chat: &str,
        source: Source,
        payload: &Value,
        now: Instant,
    ) -> Result<Held, NotRaised> {
        if payload.to_string().len() > MOST_ASK_BYTES {
            return Err(NotRaised::TooBig);
        }
        let mut waiting = self.waiting();
        let open = waiting.values().filter(|w| w.chat == chat).count();
        if open >= MOST_OPEN_ASKS {
            return Err(NotRaised::TooMany);
        }
        let raised = self.asks.raise(chat, ask(source, payload), now).raised;
        let (tell, answered) = mpsc::channel();
        waiting.insert(
            raised.id.clone(),
            Waiter {
                chat: chat.to_owned(),
                tell,
            },
        );
        Ok(Held { raised, answered })
    }

    /// Answers hook ask `id` of chat `chat` with `choice`, from `by`, at `now`, and sends the
    /// option to the hook that waits on it. The answer and the send happen under one hold, so
    /// a hook that goes away is withdrawn either before the answer (which is then refused) or
    /// after it (and was told).
    ///
    /// An open ask from another source is [`Refused::Unknown`] here: its answer goes back on its
    /// own channel, never on a hook.
    pub fn answer(
        &self,
        chat: &str,
        id: &AskId,
        choice: &str,
        by: Answerer,
        now: Instant,
    ) -> Result<Applied, Refused> {
        let mut waiting = self.waiting();
        if !waiting.contains_key(id) && self.asks.chat_of(id).is_some() {
            return Err(Refused::Unknown);
        }
        let applied = self.asks.answer(chat, id, choice, by, now)?;
        if let Some(waiter) = waiting.remove(id) {
            let _ = waiter.tell.send(applied.choice.clone());
        }
        Ok(applied)
    }

    /// Withdraws hook ask `id`: its hook went away, or the harness answered it in the pane.
    pub fn withdraw(&self, id: &AskId) -> bool {
        let mut waiting = self.waiting();
        waiting.remove(id);
        self.asks.withdraw(id)
    }

    /// Stops holding every ask past its deadline at `now`, and says which, so each hook can
    /// decide nothing and let the harness's own prompt ask.
    pub fn time_out(&self, now: Instant) -> Vec<AskId> {
        let mut waiting = self.waiting();
        let late = self.asks.expire(now);
        for id in &late {
            waiting.remove(id);
        }
        late
    }

    /// Every ask still waiting at `now`, oldest first.
    pub fn pending(&self, now: Instant) -> Vec<Raised> {
        self.asks.pending(now)
    }
}

#[cfg(test)]
mod tests;
