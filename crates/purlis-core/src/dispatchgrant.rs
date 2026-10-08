//! **A dispatch grant** (#1437, spec #1434; ADR 0090 as amended): the rule that a chat running
//! as one persona may dispatch to another. A chat allowed to dispatch to a persona can ask for
//! anything that persona can do, so the pair is the person's to allow, once, from a Notice that
//! shows the first brief — and never a chat's.
//!
//! # The one question
//!
//! [`covers`] answers it, purely: is a dispatch from a chat running as `asking` to `target`
//! [`Covers::Covered`], does it [`Covers::NeedsGrant`], or is it [`Covers::Locked`] by an
//! administrator's policy, with who locked it. A chat dispatching to its own persona needs no
//! grant. The asking persona is the app's record of the asking chat, never the request's.
//!
//! # Levels, and where each is kept
//!
//! The levels of a sandbox grant ([`Level`]), kept where a host of that level is:
//!
//! - **This chat**: the app's record of the chat, in memory, gone when it closes
//!   ([`ChatPair`], handed to [`InForce::read`]).
//! - **Me on this machine**: `app/sandbox.json` ([`crate::sandbox::local::grant_dispatch`]),
//!   the record a host of yours is confirmed in, which no sandboxed chat writes. Never
//!   committed.
//! - **Everyone in this project**: the committed project file, `[dispatch.grants]`
//!   ([`committed`], written by [`crate::settings::dispatch`]), which every teammate follows
//!   and is told of once when it changes ([`changed`]):
//!
//!   ```toml
//!   [dispatch.grants]
//!   steward = ["devops"]
//!   ```
//!
//! **Nothing a chat sends creates, widens or revokes one.** No line of the hook socket names a
//! grant; the project file is a later-code name a sandboxed chat is denied writing, and a
//! brokered write refuses any change under `[dispatch]` ([`crate::brokered::guard`]);
//! `app/sandbox.json` is the integrity class's.
//!
//! **The two file-backed levels are held by the sandbox and by nothing else** (D-1437-R2). A
//! chat in a project with the sandbox off, or one a person started without it, runs as the
//! person and can write either file: there a grant is not protected from a chat, and nothing
//! here claims it is.
//!
//! **A pulled grant waits for this machine's yes** (D-1437-R1): a committed pair is in force
//! here only once someone here allowed it ([`InForce::read`], [`acknowledge_pair`]), on the
//! project's one-time Notice or on a chat's tab. One made in this window is acknowledged as it
//! is written. So a chat nobody is at never dispatches under a pair nobody here has seen.
//!
//! # Policy
//!
//! An administrator's policy can lock all dispatch, or a pair
//! ([`Locks::dispatch_refused`]): a locked dispatch is refused with the policy's sentence, and
//! no grant covers it, one already made included.
//!
//! # What a covered dispatch starts
//!
//! [`grants_for_a_dispatched_chat`]: the persona chat holds its **own** persona's hosts and
//! vaults from its first command. The "held on the asking chat's grants until Allow" rule
//! (D-1362-5, [`crate::sandbox::persona::held_unless_within`]) does not apply to a dispatch:
//! the grant is the person's consent to the pair, given before anything started.

use std::fmt;
use std::path::Path;

use crate::sandbox::grant::Level;
use crate::sandbox::policy::Locks;

/// The table of the committed project file that holds dispatch settings.
pub const TABLE: &str = crate::dispatchlimits::TABLE;

/// The key in [`TABLE`] that holds the project's grants: asking persona to target personas.
pub const KEY: &str = "grants";

/// **Who may dispatch to whom**: chats running as `asking`, to `target`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pair {
    pub asking: String,
    pub target: String,
}

impl Pair {
    /// The pair `asking` to `target`, or why it is not one: each is a persona's name, and they
    /// differ (a chat dispatching to its own persona needs no grant, so none is ever kept).
    pub fn new(asking: &str, target: &str) -> Result<Self, String> {
        for name in [asking, target] {
            if !crate::personas::valid_name(name) {
                return Err(format!(
                    "{} is not a persona's name, so purlis keeps no dispatch grant for it.",
                    crate::shown::short(name)
                ));
            }
        }
        if asking == target {
            return Err(format!(
                "A {asking} chat dispatches to {asking} with no grant, so there is none to keep."
            ));
        }
        Ok(Self {
            asking: asking.to_owned(),
            target: target.to_owned(),
        })
    }

    /// The pair [`fmt::Display`] wrote, or none.
    pub fn parse(said: &str) -> Option<Self> {
        let (asking, target) = said.split_once(" -> ")?;
        Self::new(asking, target).ok()
    }
}

/// As the audit, the Granted list and the teammate Notice name it: `steward -> devops`. A
/// persona's name holds no space, so the two never run together.
impl fmt::Display for Pair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} -> {}", self.asking, self.target)
    }
}

/// **A grant for one chat**: the persona it ran as when the person allowed it (none for a chat
/// on no persona), and the target. Kept by the app for that chat alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPair {
    pub asking: Option<String>,
    pub target: String,
}

/// **The grants in force for one asking chat**: its own, yours on this machine, and the
/// project's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InForce {
    pub chat: Vec<ChatPair>,
    pub you: Vec<Pair>,
    pub project: Vec<Pair>,
}

impl InForce {
    /// The grants in force in the project at `root` for a chat the app holds `chat` for.
    ///
    /// **A committed pair is in force only once this machine acknowledged it** (D-1437-R1): one
    /// a pull brought in covers nothing here until a person allows it on the one-time Notice,
    /// or on a chat's tab. A grant made in this window is acknowledged as it is written.
    pub fn read(root: &Path, chat: Vec<ChatPair>) -> Self {
        let seen = crate::sandbox::local::dispatch_seen(root).unwrap_or_default();
        Self {
            chat,
            you: yours(root),
            project: committed_at(root)
                .into_iter()
                .filter(|pair| seen.contains(&pair.to_string()))
                .collect(),
        }
    }

    /// The widest level a grant of `asking` to `target` is held at, if any is.
    pub fn level_of(&self, asking: Option<&str>, target: &str) -> Option<Level> {
        let named = |pairs: &[Pair]| {
            asking.is_some_and(|asking| {
                pairs
                    .iter()
                    .any(|pair| pair.asking == asking && pair.target == target)
            })
        };
        if named(&self.project) {
            Some(Level::Project)
        } else if named(&self.you) {
            Some(Level::You)
        } else if self
            .chat
            .iter()
            .any(|pair| pair.asking.as_deref() == asking && pair.target == target)
        {
            Some(Level::Chat)
        } else {
            None
        }
    }
}

/// What [`covers`] answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Covers {
    /// The dispatch may start with no prompt: the same persona, or a grant in force.
    Covered,
    /// Nothing covers the pair: nothing starts, and the person is asked once.
    NeedsGrant,
    /// An administrator's policy locks it: the policy's sentence, naming who set it. No Allow
    /// is offered, and no grant covers it.
    Locked(String),
}

/// **Whether a chat running as `asking` may dispatch to `target`** under `grants` and `policy`.
///
/// In order: a policy that locks all dispatch, which holds a chat's own persona too; a policy
/// lock on the pair, a persona's own included where the policy names it twice; the same
/// persona, which needs no grant; then a grant at any level.
/// `asking` is `None` for a chat on no persona, which only a grant for that chat covers.
pub fn covers(asking: Option<&str>, target: &str, grants: &InForce, policy: &Locks) -> Covers {
    if policy.forbids_dispatch() {
        return Covers::Locked(
            policy
                .dispatch_refused(asking, target)
                .unwrap_or_else(|| policy.locked_by()),
        );
    }
    // Before the same-persona answer: a pair that names one persona twice locks that
    // persona's dispatch to itself.
    if let Some(why) = policy.dispatch_refused(asking, target) {
        return Covers::Locked(why);
    }
    if asking == Some(target) {
        return Covers::Covered;
    }
    match grants.level_of(asking, target) {
        Some(_) => Covers::Covered,
        None => Covers::NeedsGrant,
    }
}

/// What `[dispatch.grants]` of a project file holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Committed {
    /// Each pair it grants, in file order, once.
    pub pairs: Vec<Pair>,
    /// Each thing in it that grants nothing, as one sentence.
    pub refused: Vec<String>,
}

/// **The project's dispatch grants as `text`, the whole project file, writes them** (`None`: no
/// file). A file that is not TOML grants nothing; so does anything in the table that is not an
/// asking persona's name with a list of target personas' names, each said in a sentence.
pub fn committed(text: Option<&str>) -> Committed {
    let mut out = Committed::default();
    let Some(top) = text.and_then(|text| text.parse::<toml::Table>().ok()) else {
        return out;
    };
    let Some(value) = top.get(TABLE).and_then(|table| table.get(KEY)) else {
        return out;
    };
    let at = format!("{TABLE}.{KEY}");
    let Some(table) = value.as_table() else {
        out.refused.push(format!(
            "{at} is not a table, so it grants nothing: write the asking persona, then the \
             personas it may dispatch to, as steward = [\"devops\"]"
        ));
        return out;
    };
    for (asking, targets) in table {
        let Some(targets) = targets.as_array() else {
            out.refused.push(format!(
                "{at}.{} is not a list of personas, so it grants nothing",
                crate::shown::short(asking)
            ));
            continue;
        };
        for target in targets {
            match target.as_str().map(|target| Pair::new(asking, target)) {
                Some(Ok(pair)) => {
                    if !out.pairs.contains(&pair) {
                        out.pairs.push(pair);
                    }
                }
                Some(Err(why)) => out.refused.push(why),
                None => out.refused.push(format!(
                    "{at}.{} holds something that is not a persona's name, which grants nothing",
                    crate::shown::short(asking)
                )),
            }
        }
    }
    out
}

/// The project's dispatch grants in the project at `root`, read as the sandbox reads the
/// project file: never through a link, and none from a file that cannot be read.
pub fn committed_at(root: &Path) -> Vec<Pair> {
    let text = crate::sandbox::read_plane_file(&crate::names::manifest(root))
        .ok()
        .flatten();
    committed(text.as_deref()).pairs
}

/// Your own dispatch grants for the project at `root`, on this machine.
pub fn yours(root: &Path) -> Vec<Pair> {
    crate::sandbox::local::granted_dispatch(root)
        .iter()
        .filter_map(|(asking, target)| Pair::new(asking, target).ok())
        .collect()
}

/// The project's dispatch grants as they changed since this machine last told the person: what
/// was added, what was taken away, and the whole list now, each as [`Pair`] is displayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub now: Vec<String>,
}

/// How `now`, the project's grants, differs from `seen`, what the person was last told of.
/// `None` when nothing differs; order is no change.
pub fn change_between(seen: &[String], now: &[Pair]) -> Option<Change> {
    let now: Vec<String> = now.iter().map(ToString::to_string).collect();
    let added: Vec<String> = now
        .iter()
        .filter(|one| !seen.contains(one))
        .cloned()
        .collect();
    let removed: Vec<String> = seen
        .iter()
        .filter(|one| !now.contains(one))
        .cloned()
        .collect();
    (!added.is_empty() || !removed.is_empty()).then_some(Change {
        added,
        removed,
        now,
    })
}

/// **How the project's dispatch grants changed since this machine last told the person**: the
/// one-time Notice each teammate sees, so a pulled change never silently widens what chats do.
/// `None` when nothing did. A project first seen here with grants is a change. Whether the
/// project's chats are sandboxed makes no difference: a grant is about whose vaults and hosts a
/// chat can ask for, sandbox or none.
pub fn changed(root: &Path) -> Option<Change> {
    let seen = crate::sandbox::local::dispatch_seen(root).unwrap_or_default();
    change_between(&seen, &committed_at(root))
}

/// Records that the person allowed `shown`, the project's grants as the Notice showed them:
/// each of them the file holds is in force here from now on ([`InForce::read`]), and a change
/// after it was shown is told again ([`changed`]).
pub fn acknowledge(root: &Path, shown: &[String]) -> std::io::Result<()> {
    crate::sandbox::local::acknowledge_dispatch(root, shown)
}

/// The project's pairs this machine has not acknowledged: in the file, in force for no chat
/// here yet.
pub fn unacknowledged(root: &Path) -> Vec<Pair> {
    let seen = crate::sandbox::local::dispatch_seen(root).unwrap_or_default();
    committed_at(root)
        .into_iter()
        .filter(|pair| !seen.contains(&pair.to_string()))
        .collect()
}

/// Records that the person allowed the project's `pair` on this machine, beside what was
/// acknowledged before.
pub fn acknowledge_pair(root: &Path, pair: &Pair) -> std::io::Result<()> {
    let mut seen = crate::sandbox::local::dispatch_seen(root).unwrap_or_default();
    let said = pair.to_string();
    if seen.contains(&said) {
        return Ok(());
    }
    seen.push(said);
    crate::sandbox::local::acknowledge_dispatch(root, &seen)
}

/// Takes `pair` off what this machine acknowledged: a revoke made here is no news here.
pub fn forget_pair(root: &Path, pair: &Pair) -> std::io::Result<()> {
    let Some(mut seen) = crate::sandbox::local::dispatch_seen(root) else {
        return Ok(());
    };
    let said = pair.to_string();
    if !seen.contains(&said) {
        return Ok(());
    }
    seen.retain(|one| *one != said);
    crate::sandbox::local::acknowledge_dispatch(root, &seen)
}

/// **A dispatch grant or revoke, as the audit records it** (`trust.dispatch.grant`,
/// `trust.dispatch.revoke`; ADR 0075 §4): who (the person, by scope, never a login), the level
/// and the pair. Which chat it came from, when and on which machine are the envelope's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Audited<'a> {
    /// A grant made, or one taken back.
    pub granted: bool,
    /// The asking persona; none for a chat on no persona.
    pub asking: Option<&'a str>,
    pub target: &'a str,
    pub level: Level,
}

impl Audited<'_> {
    /// The event's kind.
    pub fn kind(&self) -> &'static str {
        if self.granted {
            "trust.dispatch.grant"
        } else {
            "trust.dispatch.revoke"
        }
    }

    /// The event's body.
    pub fn body(&self) -> serde_json::Value {
        serde_json::json!({
            "actor_kind": "human",
            "actor": "operator",
            "scope": "local-ui",
            "level": self.level.word(),
            "asking": self.asking,
            "target": self.target,
        })
    }
}

/// **What a persona chat started by a covered dispatch holds** ([`grants_for_a_dispatched_chat`]):
/// the pieces of its [`crate::start::Start`] that decide whose hosts and vaults it reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispatched {
    /// The persona it runs as: its vault is the one brokered `secret exec` hands it, and its
    /// hosts the ones its sandbox is compiled with.
    pub persona: String,
    /// Never held on another chat's grants: always `None`.
    pub held: Option<crate::reopen::HeldGrants>,
    /// Never the asking chat's own grants: always empty.
    pub grants: crate::sandbox::grant::Grants,
    /// Never the asking chat's opt-out: always `None`.
    pub without_sandbox: Option<crate::sandbox::OptOut>,
}

/// **The grants a persona chat dispatched to `target` starts with: the target's own, never
/// held** (#1437; retires D-1362-5 for a dispatch a grant covers). Called once [`covers`]
/// answered [`Covers::Covered`]: the person's consent to the pair is the grant, so nothing is
/// left to allow on the new chat's tab, and nothing of the asking chat's (its per-chat grants,
/// its opt-out) comes with it.
pub fn grants_for_a_dispatched_chat(target: &str) -> Dispatched {
    Dispatched {
        persona: target.to_owned(),
        held: None,
        grants: crate::sandbox::grant::Grants::default(),
        without_sandbox: None,
    }
}

/// The most of a brief the grant Notice shows, in bytes: the most a chat's first message may
/// be, so a brief that could start a chat is always shown whole.
pub const MOST_BRIEF_BYTES: usize = crate::handoff::FIRST_MESSAGE_MAX_BYTES;

/// A brief as the grant Notice shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownBrief {
    /// The text, inert: lines and tabs kept, every other character with no glyph written out
    /// as its escape, and a backslash doubled so the text cannot spell an escape itself.
    pub text: String,
    /// Whether the brief was longer than [`MOST_BRIEF_BYTES`] and is cut there.
    pub cut: bool,
    /// How many lines `text` is, blank ones counted: the Notice says it, so a brief whose ask
    /// sits below what its box shows is not read as its first lines alone.
    pub lines: u32,
}

/// **`brief` as the Notice shows it**: untrusted text from a chat, so nothing in it can move
/// the cursor, change direction or hide what follows. Shown whole up to [`MOST_BRIEF_BYTES`].
pub fn shown_brief(brief: &str) -> ShownBrief {
    let mut end = brief.len().min(MOST_BRIEF_BYTES);
    while !brief.is_char_boundary(end) {
        end -= 1;
    }
    let text = inert(&brief[..end]);
    let lines = u32::try_from(text.lines().count()).unwrap_or(u32::MAX);
    ShownBrief {
        text,
        cut: end < brief.len(),
        lines,
    }
}

/// **`text` written out inertly**: a chat's words, so nothing in them can move the cursor,
/// change direction or hide what follows. Lines and tabs are kept (a `\r\n` is a line break),
/// every other character with no glyph is written out as its escape, and a backslash is
/// doubled so the text cannot spell an escape itself. Nothing is cut: what [`shown_brief`]
/// shows of a brief before it is sent, and what the window shows of one after
/// ([`crate::dispatchrecord::brief_sent`]).
pub fn inert(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.replace("\r\n", "\n").chars() {
        match ch {
            '\n' | '\t' => out.push(ch),
            '\\' => out.push_str("\\\\"),
            // The house's one table of what draws as nothing ([`crate::shown`]), never a list
            // of ranges kept here.
            _ if crate::shown::invisible(ch) => out.push_str(&crate::shown::escape_char(ch)),
            _ => out.push(ch),
        }
    }
    out
}

/// Whether `text` holds a character [`inert`] writes out as an escape: one that draws as
/// nothing, moves the cursor or turns the words around it. A line break (`\n`, `\r\n`) and a
/// tab are plain text, and are not one.
pub fn holds_what_draws_as_nothing(text: &str) -> bool {
    text.replace("\r\n", "\n")
        .chars()
        .any(|ch| ch != '\n' && ch != '\t' && crate::shown::invisible(ch))
}

#[cfg(test)]
#[path = "dispatchgrant_tests.rs"]
mod tests;
