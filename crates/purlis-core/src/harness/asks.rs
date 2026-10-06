//! The asks that wait on a human, and the one rule for answering them (HP-5): **one answer per
//! ask, and the first wins.**
//!
//! Several clients can show the same ask at once: the window, `charter inbox`, later a paired
//! device. Whichever answers first applies; every other answer, from another client or the same
//! one twice, is refused with "answered elsewhere". An answer that comes too late, to an ask the
//! source has sent again, or to one the chat withdrew is refused too, each with a sentence
//! saying why, so a client can tell its operator instead of failing quietly.
//!
//! **A timeout never answers.** An ask on a hook carries a deadline below the hook's timeout
//! ([`super::model::Deadline::below_hook_timeout`]). Once it passes, charter stops holding the ask and the
//! source's own prompt decides, in the pane; charter neither allows nor denies on the
//! operator's behalf. An ACP ask has no deadline and waits (ADR 0080 §5, V28d).
//!
//! **Only a human answers** (V16, V75, ADR 0080 §5.3). An [`Answerer`] is made only from a
//! connection the host admitted as a human scope, the window or `charter inbox`; a chat never
//! is one. An ask that elicits a secret is answered from the window alone.
//!
//! The registry is in memory and holds no store: it lives in the host that holds the chats'
//! hooks and protocols, and an ask dies with its run (ADR 0080 §6). Time is passed in, so a
//! deadline is decided by the caller's clock and tested without sleeping.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::model::{Ask, Channel, Choice};

/// An ask's id: charter's, given when it is raised, a ULID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AskId(ulid::Ulid);

impl std::fmt::Display for AskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for AskId {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        ulid::Ulid::from_string(text)
            .map(Self)
            .map_err(|_| format!("{text:?} is not an ask's id"))
    }
}

impl TryFrom<String> for AskId {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<AskId> for String {
    fn from(id: AskId) -> Self {
        id.to_string()
    }
}

/// An ask as charter holds it: its id, the chat that asked, and what it asks.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Raised {
    pub id: AskId,
    pub chat: String,
    pub ask: Ask,
}

/// The scope a connection to the host was admitted as: the client scopes of `charterd.sock`
/// (ADR 0068 §5, the session protocol's `Scope`), and a chat on its hook socket. Only the host
/// names one, from the connection it authenticated (FD-6's `Link::scope()`), never from what a
/// client says it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admitted {
    /// The app's window.
    LocalUi,
    /// `charter attach` and `charter ls`.
    Terminal,
    /// The fleet MCP server.
    FleetMcp,
    /// `charter inbox` and approvals from a shell.
    Approval,
    /// The editor's charter extension.
    Editor,
    /// A chat: its hook socket, or a level-3 agent's stdio.
    Chat,
}

/// A human scope that answers asks (ADR 0080 §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HumanScope {
    LocalUi,
    Approval,
}

/// Who sends an answer: the operator, through a connection the host admitted as a human
/// scope. It is made only by [`Answerer::admitted`] and is never read off the wire, so no
/// client can say it is one; an agent never is (V16, V75).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Answerer(HumanScope);

impl Answerer {
    /// The answerer a connection admitted as `scope` is, or `None` for a scope that never
    /// answers: a chat, and every client scope but the window and `charter inbox`.
    pub fn admitted(scope: Admitted) -> Option<Self> {
        match scope {
            Admitted::LocalUi => Some(Self(HumanScope::LocalUi)),
            Admitted::Approval => Some(Self(HumanScope::Approval)),
            Admitted::Terminal | Admitted::FleetMcp | Admitted::Editor | Admitted::Chat => None,
        }
    }

    /// The human scope it answers through.
    pub fn scope(self) -> HumanScope {
        self.0
    }
}

/// What raising an ask did: the ask as held, and the asks of the same request it superseded,
/// which the caller answers or withdraws at their source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raising {
    pub raised: Raised,
    pub superseded: Vec<AskId>,
}

/// The answer that applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub id: AskId,
    pub chat: String,
    /// The option chosen, as the source offered it, to send back on [`Ask::channel`].
    pub choice: Choice,
    pub channel: Channel,
    pub by: Answerer,
    /// From the ask being raised to this answer.
    pub waited: Duration,
}

/// Why an answer was not applied. Each says so in a sentence a client can show.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("answered elsewhere: another client answered this ask first")]
    AnsweredElsewhere,
    #[error("this ask timed out, so the chat's own prompt decides it in its pane")]
    TimedOut,
    #[error("the chat asked again, so this ask was replaced by a newer ask")]
    Superseded,
    #[error("the chat withdrew this ask: its turn moved on or was stopped")]
    Withdrawn,
    #[error("no such ask is waiting")]
    Unknown,
    #[error("{0:?} is not one of the answers this ask offers")]
    NotAnOption(String),
    #[error("this ask elicits a secret, so it is answered only in purlis's window")]
    NotYours,
    #[error("this ask offers no answers here; answer it in the chat's pane")]
    InThePane,
}

/// How many closed asks are remembered, so a late answer to one hears why. Older ones read as
/// [`Refused::Unknown`].
const REMEMBERED: usize = 4096;

/// The asks that wait on a human, shared by every client that answers them.
#[derive(Debug, Default)]
pub struct Asks {
    held: Mutex<Held>,
}

#[derive(Debug, Default)]
struct Held {
    /// Waiting, in the order they were raised.
    open: Vec<Open>,
    /// Closed, the chat that raised each, and why: what a late answer is refused with.
    closed: HashMap<AskId, (String, Refused)>,
    closed_order: VecDeque<AskId>,
    /// How long each applied answer took, for the median (QA-16's bar for HP-5).
    waits: VecDeque<Duration>,
}

#[derive(Debug)]
struct Open {
    raised: Raised,
    at: Instant,
}

impl Open {
    fn past_deadline(&self, now: Instant) -> bool {
        self.raised
            .ask
            .deadline
            .duration()
            .is_some_and(|deadline| now.saturating_duration_since(self.at) >= deadline)
    }

    /// Whether `ask`, raised by `chat`, is this one sent again: the same request id from the
    /// same source, or a new pane nudge from the same chat. Hook asks are never the same: each
    /// hook process asks once, and its ask is withdrawn when it goes.
    fn sent_again_as(&self, chat: &str, ask: &Ask) -> bool {
        let mine = &self.raised;
        if mine.chat != chat {
            return false;
        }
        match (&mine.ask.channel, &ask.channel) {
            (Channel::Pane, Channel::Pane) => true,
            (old, new) => old.request().is_some() && old == new,
        }
    }
}

impl Held {
    fn close(&mut self, id: AskId, chat: String, why: Refused) {
        self.closed.insert(id.clone(), (chat, why));
        self.closed_order.push_back(id);
        while self.closed_order.len() > REMEMBERED {
            if let Some(old) = self.closed_order.pop_front() {
                self.closed.remove(&old);
            }
        }
    }

    fn take(&mut self, id: &AskId) -> Option<Open> {
        let at = self.open.iter().position(|open| open.raised.id == *id)?;
        Some(self.open.remove(at))
    }
}

impl Asks {
    pub fn new() -> Self {
        Self::default()
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Held> {
        // A panic while held leaves nothing half-written: every change is one push or remove.
        self.held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Holds `ask`, raised by `chat` at `now`, under a new id. An ask the source sent again
    /// supersedes the one before it, whose late answers then hear so.
    pub fn raise(&self, chat: &str, ask: Ask, now: Instant) -> Raising {
        let raised = Raised {
            id: AskId(ulid::Ulid::generate()),
            chat: chat.to_owned(),
            ask,
        };
        let mut held = self.held();
        let (again, kept): (Vec<Open>, Vec<Open>) = std::mem::take(&mut held.open)
            .into_iter()
            .partition(|open| open.sent_again_as(chat, &raised.ask));
        held.open = kept;
        let superseded: Vec<AskId> = again.into_iter().map(|old| old.raised.id).collect();
        for old in &superseded {
            held.close(old.clone(), chat.to_owned(), Refused::Superseded);
        }
        held.open.push(Open {
            raised: raised.clone(),
            at: now,
        });
        Raising { raised, superseded }
    }

    /// Answers ask `id` of chat `chat` with the option `choice`, from `by`, at `now`. The first
    /// answer that passes every check applies and closes the ask; the check and the close are
    /// one step, so two clients racing can never both apply.
    ///
    /// **An ask is answered only together with the chat that raised it** (HP-6). An answer
    /// naming another chat is [`Refused::Unknown`], whether the id is open, closed or never
    /// was, so it can neither land on another chat's ask nor learn that one exists.
    pub fn answer(
        &self,
        chat: &str,
        id: &AskId,
        choice: &str,
        by: Answerer,
        now: Instant,
    ) -> Result<Applied, Refused> {
        let mut held = self.held();
        let Some(open) = held.open.iter().find(|open| open.raised.id == *id) else {
            let closed = held.closed.get(id).filter(|(of, _)| of == chat);
            return Err(closed.map_or(Refused::Unknown, |(_, why)| why.clone()));
        };
        if open.raised.chat != chat {
            return Err(Refused::Unknown);
        }
        if open.past_deadline(now) {
            held.take(id);
            held.close(id.clone(), chat.to_owned(), Refused::TimedOut);
            return Err(Refused::TimedOut);
        }
        let ask = &open.raised.ask;
        if !may_answer(ask, by) {
            return Err(Refused::NotYours);
        }
        if ask.options.is_empty() {
            return Err(Refused::InThePane);
        }
        let Some(chosen) = ask
            .options
            .iter()
            .find(|option| option.id == choice)
            .cloned()
        else {
            return Err(Refused::NotAnOption(choice.to_owned()));
        };
        let open = held.take(id).expect("found above, under the same lock");
        held.close(id.clone(), chat.to_owned(), Refused::AnsweredElsewhere);
        let waited = now.saturating_duration_since(open.at);
        held.waits.push_back(waited);
        if held.waits.len() > REMEMBERED {
            held.waits.pop_front();
        }
        Ok(Applied {
            id: id.clone(),
            chat: open.raised.chat,
            choice: chosen,
            channel: open.raised.ask.channel,
            by,
            waited,
        })
    }

    /// Withdraws ask `id`: its turn moved on, its hook went away, or its chat was stopped.
    /// Whether it was still waiting.
    pub fn withdraw(&self, id: &AskId) -> bool {
        let mut held = self.held();
        match held.take(id) {
            Some(open) => {
                held.close(open.raised.id, open.raised.chat, Refused::Withdrawn);
                true
            }
            None => false,
        }
    }

    /// Stops holding every ask whose deadline has passed at `now`, and says which, so the
    /// caller can let each source's own prompt decide.
    pub fn expire(&self, now: Instant) -> Vec<AskId> {
        let mut held = self.held();
        let (late, kept): (Vec<Open>, Vec<Open>) = std::mem::take(&mut held.open)
            .into_iter()
            .partition(|open| open.past_deadline(now));
        held.open = kept;
        late.into_iter()
            .map(|open| {
                let id = open.raised.id;
                held.close(id.clone(), open.raised.chat, Refused::TimedOut);
                id
            })
            .collect()
    }

    /// The chat that raised ask `id`, while it is open, its deadline passed or not.
    pub fn chat_of(&self, id: &AskId) -> Option<String> {
        self.held()
            .open
            .iter()
            .find(|open| open.raised.id == *id)
            .map(|open| open.raised.chat.clone())
    }

    /// Every ask still waiting at `now`, oldest first.
    pub fn pending(&self, now: Instant) -> Vec<Raised> {
        self.held()
            .open
            .iter()
            .filter(|open| !open.past_deadline(now))
            .map(|open| open.raised.clone())
            .collect()
    }

    /// The median time from an ask being raised to its answer applying, over the answers held
    /// here: the outcome bar QA-16 gives HP-5. `None` before any answer.
    pub fn median_time_to_answer(&self) -> Option<Duration> {
        let mut waits: Vec<Duration> = self.held().waits.iter().copied().collect();
        if waits.is_empty() {
            return None;
        }
        waits.sort();
        let middle = waits.len() / 2;
        Some(match waits.len() % 2 {
            1 => waits[middle],
            _ => (waits[middle - 1] + waits[middle]) / 2,
        })
    }
}

/// Whether `by` may answer `ask`: any human scope, but only the window for an ask that elicits
/// a secret, whose value goes through a dialog the app owns (ADR 0080 §2).
fn may_answer(ask: &Ask, by: Answerer) -> bool {
    !ask.elicits_secret || by.scope() == HumanScope::LocalUi
}

#[cfg(test)]
mod tests;
