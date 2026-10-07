//! **Dispatch limits** (#1439, #1440; spec #1434, "Limits"): how many persona chats a chat may
//! have running, how many a lineage may hold, how deep a chain may go and how many messages a
//! minute may pass, and, per persona, how many chats it may dispatch and how many may run as
//! it at once.
//!
//! Two pure functions, and the dispatch decision calls both:
//!
//! - [`in_force`] answers the limits in force for one dispatch, from the project's committed
//!   table, the workspace, the asking persona, the target persona, this machine's own table and
//!   an administrator's ceiling;
//! - [`decide`] takes those limits and a snapshot of the asking chat's lineage ([`Lineage`]),
//!   and answers [`Decision::Allowed`] or [`Decision::Refused`] with which limit
//!   ([`Refused`]) and its sentence ([`Refused::say`]).
//!
//! The names are the dispatch decision's (`dispatchdecision`, #1436) wherever it has one:
//! `running`, `lineage`, `depth`, `chain`, `Loop`, `TooDeep`, `TooManyRunning`, `LineageFull`,
//! [`DEEPEST`].
//!
//! # Where the limits are written
//!
//! In the project's committed `charter.toml`, at three levels:
//!
//! ```toml
//! [dispatch]
//! running-per-chat = 6
//! live-per-lineage = 16
//! depth = 3
//! messages-per-minute = 10
//!
//! [dispatch.workspaces.alpha]
//! running-per-chat = 12
//!
//! [dispatch.personas.devops]
//! depth = 1
//! may-dispatch = 2
//! may-run-at-once = 1
//! ```
//!
//! **The most specific level wins**: the persona's over the workspace's over the project's, and
//! a level that does not set a limit inherits it. The four general limits follow the *asking*
//! chat: its workspace, then its persona. `may-dispatch` is the asking persona's, counted
//! across every chat running as it in the project, and `may-run-at-once` the target persona's;
//! only a persona's table holds them, and neither has a cap until one is written.
//!
//! **A persona's limits are here, never in its own `persona.md`** (as its sandbox hosts are,
//! D-1362-1): a chat may edit its persona's charter, so a limit written there would be one a
//! chat could raise. Nothing in this module reads a persona's file. The manifest is a
//! later-code name no sandboxed chat writes, and a brokered write refuses any change under
//! `[dispatch]` ([`crate::brokered::guard`]).
//!
//! **This machine's own table** is `[dispatch]` in `charter.local.toml`, in the same shape,
//! and **it only ever lowers** a limit: a value above what the project's files give is
//! ignored, and [`Limits::ignored`] says so. A chat that can write that file can therefore
//! make dispatch stricter and never looser. For the same reason it is read even where git would
//! carry the file, which leaves every other table of it out ([`Files::read`]).
//!
//! **An administrator's policy is a ceiling** on every limit (`dispatch` in
//! [`crate::sandbox::policy::MACHINE_FILE`], [`ceiling`]).
//!
//! # Fixed rules
//!
//! - A persona is never dispatched to from below itself: one already above the asking chat in
//!   its chain is refused, whatever the limits say. A chat's own persona is not above it, so a
//!   chat may split its own work.
//! - `depth` is never above [`DEEPEST`]: a higher one is refused as a setting.
//! - A limit of 0 switches dispatch off at the level that sets it, and a more specific level
//!   may set it back above 0: a project's 0 with a workspace's 2 is 2 in that workspace. A
//!   policy's 0 is a ceiling, and nothing lifts it. `messages-per-minute = 0` stops messages
//!   only, never a dispatch.
//! - A policy file that is refused switches dispatch off, and says the file is refused.
//! - Every count is of chats that still owe work: not yet reported, and their program not
//!   ended.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

/// The table of a settings file that holds the limits.
pub const TABLE: &str = "dispatch";

/// The key of [`TABLE`] that holds each workspace's own limits.
pub const WORKSPACES: &str = "workspaces";

/// The key of [`TABLE`] that holds each persona's own limits.
pub const PERSONAS: &str = "personas";

/// The most `depth` may ever be set to, at any level and by policy.
pub const DEEPEST: u32 = 8;

/// The most any limit is read as: far past any machine's, and a bound on what a file can ask.
pub const MOST: u32 = 10_000;

/// One limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Limit {
    /// The persona chats one asking chat may have running.
    RunningPerChat,
    /// The live chats one lineage may hold.
    LivePerLineage,
    /// How deep a chain of dispatches may go.
    Depth,
    /// The messages a minute one chat may send another.
    MessagesPerMinute,
    /// The persona chats a chat running as this persona may have running.
    MayDispatch,
    /// The chats that may run as this persona at once, in the project.
    MayRunAtOnce,
}

impl Limit {
    /// Every limit, in the order a table draws them.
    pub const ALL: [Limit; 6] = [
        Limit::RunningPerChat,
        Limit::LivePerLineage,
        Limit::Depth,
        Limit::MessagesPerMinute,
        Limit::MayDispatch,
        Limit::MayRunAtOnce,
    ];

    /// The key a settings file and the policy file write it as.
    pub fn word(self) -> &'static str {
        match self {
            Self::RunningPerChat => "running-per-chat",
            Self::LivePerLineage => "live-per-lineage",
            Self::Depth => "depth",
            Self::MessagesPerMinute => "messages-per-minute",
            Self::MayDispatch => "may-dispatch",
            Self::MayRunAtOnce => "may-run-at-once",
        }
    }

    /// The limit `word` names.
    pub fn of_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|limit| limit.word() == word)
    }

    /// What Settings calls it.
    pub fn label(self) -> &'static str {
        match self {
            Self::RunningPerChat => "Running per chat",
            Self::LivePerLineage => "Live per lineage",
            Self::Depth => "Depth",
            Self::MessagesPerMinute => "Messages per minute",
            Self::MayDispatch => "May dispatch",
            Self::MayRunAtOnce => "May run at once",
        }
    }

    /// One line on what it limits.
    pub fn help(self) -> &'static str {
        match self {
            Self::RunningPerChat => "The persona chats one chat may have running at once.",
            Self::LivePerLineage => {
                "The chats that may be live at once below one chat you started, itself included."
            }
            Self::Depth => "How many dispatches deep a chain may go. Never above 8.",
            Self::MessagesPerMinute => "The messages one chat may send another in a minute.",
            Self::MayDispatch => {
                "The persona chats the chats running as this persona may have running at once, \
                 in the project."
            }
            Self::MayRunAtOnce => "The chats that may run as this persona at once, in the project.",
        }
    }

    /// What is in force where no file sets it: `None` is no cap.
    pub fn when_unset(self) -> Option<u32> {
        match self {
            Self::RunningPerChat => Some(6),
            Self::LivePerLineage => Some(16),
            Self::Depth => Some(3),
            Self::MessagesPerMinute => Some(10),
            Self::MayDispatch | Self::MayRunAtOnce => None,
        }
    }

    /// Whether only a persona's table holds it.
    pub fn persona_only(self) -> bool {
        matches!(self, Self::MayDispatch | Self::MayRunAtOnce)
    }

    fn at(self) -> usize {
        match self {
            Self::RunningPerChat => 0,
            Self::LivePerLineage => 1,
            Self::Depth => 2,
            Self::MessagesPerMinute => 3,
            Self::MayDispatch => 4,
            Self::MayRunAtOnce => 5,
        }
    }
}

/// What one level sets: the project, one workspace, one persona, or a policy's ceiling. A
/// limit it does not set is inherited.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Level {
    values: [Option<u32>; 6],
    /// A policy file that was refused: no limit of it was read, and dispatch is off.
    refused: bool,
}

impl Level {
    /// A level that sets nothing.
    pub fn unset() -> Self {
        Self::default()
    }

    /// What this level sets `limit` to, where it does.
    pub fn get(&self, limit: Limit) -> Option<u32> {
        self.values[limit.at()]
    }

    /// This level, with `limit` set to `value`.
    #[must_use]
    pub fn with(mut self, limit: Limit, value: u32) -> Self {
        self.values[limit.at()] = Some(value);
        self
    }

    /// Whether it sets nothing.
    pub fn is_unset(&self) -> bool {
        !self.refused && self.values.iter().all(Option::is_none)
    }

    /// Whether this is the ceiling of a policy file that was refused
    /// ([`ceiling_when_refused`]): nothing was read from it, and dispatch is off.
    pub fn is_refused(&self) -> bool {
        self.refused
    }
}

/// The limits one settings file holds: the project's own, each workspace's and each persona's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    pub project: Level,
    pub workspaces: BTreeMap<String, Level>,
    pub personas: BTreeMap<String, Level>,
}

/// The level that decided a limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// No file sets it.
    Default,
    /// The project's own row.
    Project,
    /// A workspace's row.
    Workspace(String),
    /// A persona's row.
    Persona(String),
    /// This machine's own table, which lowered it.
    You,
    /// An administrator's ceiling, which lowered it.
    Policy,
    /// This machine's policy file was refused, so nothing is allowed until it is fixed.
    PolicyRefused,
}

impl Source {
    /// Where dispatch is off, when this level set a limit to 0: the end of "Dispatch is off …".
    fn off_here(&self) -> String {
        match self {
            Self::Default | Self::Project => "in this project".to_owned(),
            Self::Workspace(name) => format!("in the workspace {name}"),
            Self::Persona(name) => format!("for the persona {name}"),
            Self::You => "on this machine, by your own limit".to_owned(),
            Self::Policy | Self::PolicyRefused => "on this machine, by policy".to_owned(),
        }
    }
}

/// A limit of this machine's own that is above the project's, and so decides nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ignored {
    pub limit: Limit,
    /// What this machine's table asks for.
    pub yours: u32,
    /// What the project's files give, which stays in force.
    pub committed: u32,
}

impl fmt::Display for Ignored {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Your own limit of {} for {} is above the project's {}, so it is ignored: a limit \
             on this machine can only lower one.",
            self.yours,
            self.limit.label().to_lowercase(),
            self.committed
        )
    }
}

/// **The limits in force for one dispatch**: what [`in_force`] answers and [`decide`] reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// The persona chats the asking chat may have running (`running-per-chat`).
    pub running: u32,
    /// The live chats the lineage may hold (`live-per-lineage`).
    pub lineage: u32,
    /// How deep the chain may go; never above [`DEEPEST`].
    pub depth: u32,
    /// The messages a minute the asking chat may send another.
    pub messages_per_minute: u32,
    /// The persona chats a chat as the asking persona may have running; `None` is no cap.
    pub may_dispatch: Option<u32>,
    /// The chats that may run as the target persona at once; `None` is no cap.
    pub may_run_at_once: Option<u32>,
    /// The persona the asking chat runs as, where it runs as one.
    pub asking: Option<String>,
    /// The persona dispatched to, where it is to one.
    pub target: Option<String>,
    /// Which level decided each limit.
    set_by: [Source; 6],
    /// Each limit of this machine's own that is above the project's, and was not applied.
    pub ignored: Vec<Ignored>,
}

impl Limits {
    /// What `limit` is; `None` is no cap.
    pub fn value(&self, limit: Limit) -> Option<u32> {
        match limit {
            Limit::RunningPerChat => Some(self.running),
            Limit::LivePerLineage => Some(self.lineage),
            Limit::Depth => Some(self.depth),
            Limit::MessagesPerMinute => Some(self.messages_per_minute),
            Limit::MayDispatch => self.may_dispatch,
            Limit::MayRunAtOnce => self.may_run_at_once,
        }
    }

    /// The level that decided `limit`.
    pub fn set_by(&self, limit: Limit) -> &Source {
        &self.set_by[limit.at()]
    }

    /// The first limit that is 0 and so switches dispatch off, and the level that set it.
    /// Messages a minute is not one: its 0 stops messages only ([`may_send`]).
    fn off(&self) -> Option<(Limit, &Source)> {
        Limit::ALL
            .into_iter()
            .filter(|limit| *limit != Limit::MessagesPerMinute)
            .find(|limit| self.value(*limit) == Some(0))
            .map(|limit| (limit, self.set_by(limit)))
    }
}

/// What one file's levels give `limit` for this dispatch, most specific first: the persona's,
/// the workspace's, the project's.
fn most_specific(
    table: &Table,
    limit: Limit,
    workspace: Option<&str>,
    asking: Option<&str>,
    target: Option<&str>,
) -> Option<(u32, Source)> {
    let persona = match limit {
        Limit::MayRunAtOnce => target,
        _ => asking,
    };
    let of_persona = persona.and_then(|name| {
        let value = table.personas.get(name)?.get(limit)?;
        Some((value, Source::Persona(name.to_owned())))
    });
    if limit.persona_only() {
        return of_persona;
    }
    of_persona
        .or_else(|| {
            let name = workspace?;
            let value = table.workspaces.get(name)?.get(limit)?;
            Some((value, Source::Workspace(name.to_owned())))
        })
        .or_else(|| Some((table.project.get(limit)?, Source::Project)))
}

/// **The limits in force for one dispatch.**
///
/// - `project`: the committed file's table ([`read`]).
/// - `workspace`: the workspace the asking chat works in, where it works in one.
/// - `asking`: the persona the asking chat runs as, from the app's own record of it.
/// - `target`: the persona dispatched to, where the dispatch is to one.
/// - `mine`: this machine's own table, which only ever lowers.
/// - `policy`: an administrator's ceiling ([`crate::sandbox::policy::Locks::dispatch_ceiling`]).
///
/// Each limit is the most specific level's of `project` (persona over workspace over project,
/// then the default), lowered by `mine` where that is lower, and then by `policy` where that is
/// lower still. `depth` is never above [`DEEPEST`].
pub fn in_force(
    project: &Table,
    workspace: Option<&str>,
    asking: Option<&str>,
    target: Option<&str>,
    mine: &Table,
    policy: &Level,
) -> Limits {
    let mut ignored = Vec::new();
    let mut resolved = Limit::ALL.map(|limit| {
        let (mut value, mut from) = match most_specific(project, limit, workspace, asking, target) {
            Some((value, from)) => (Some(value), from),
            None => (limit.when_unset(), Source::Default),
        };
        if let Some((yours, _)) = most_specific(mine, limit, workspace, asking, target) {
            match value {
                Some(committed) if yours > committed => ignored.push(Ignored {
                    limit,
                    yours,
                    committed,
                }),
                Some(committed) if yours == committed => {}
                _ => (value, from) = (Some(yours), Source::You),
            }
        }
        if policy.is_refused() {
            // Nothing was read from the file, so nothing is allowed: no level lifts this.
            (value, from) = (Some(0), Source::PolicyRefused);
        } else if let Some(ceiling) = policy.get(limit)
            && value.is_none_or(|value| value > ceiling)
        {
            (value, from) = (Some(ceiling), Source::Policy);
        }
        (value, from)
    });
    let depth = &mut resolved[Limit::Depth.at()].0;
    *depth = depth.map(|depth| depth.min(DEEPEST));
    let value = |limit: Limit| resolved[limit.at()].0;
    let capped = |limit: Limit| value(limit).or(limit.when_unset()).unwrap_or(MOST);
    Limits {
        running: capped(Limit::RunningPerChat),
        lineage: capped(Limit::LivePerLineage),
        depth: capped(Limit::Depth),
        messages_per_minute: capped(Limit::MessagesPerMinute),
        may_dispatch: value(Limit::MayDispatch),
        may_run_at_once: value(Limit::MayRunAtOnce),
        asking: asking.map(str::to_owned),
        target: target.map(str::to_owned),
        set_by: resolved.map(|(_, from)| from),
        ignored,
    }
}

/// **Where the asking chat stands among the chats the app has open**, from the app's own
/// record and never from the request: what [`decide`] counts against the limits. The first
/// four fields are the dispatch decision's own lineage, by the same names.
///
/// **A chat counts while it still owes work**: it has not reported and its program has not
/// ended (the dispatch decision's D-1436-18). One that reported and stays open for the person
/// to read counts nowhere here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lineage {
    /// How many dispatches stand between the asking chat and the chat the person started: 0
    /// for that chat.
    pub depth: u32,
    /// The personas of the chats above the asking chat in its lineage, nearest first; `None`
    /// for a chat on no persona. The asking chat itself is not in it.
    pub chain: Vec<Option<String>>,
    /// How many persona chats the asking chat dispatched are still running.
    pub running: u32,
    /// How many chats its lineage holds that are still running, itself included.
    pub lineage: u32,
    /// How many chats are running as the target persona, in every workspace of the project.
    pub as_target: u32,
    /// How many persona chats the chats running as the asking persona have dispatched that are
    /// still running, in every workspace of the project, the asking chat's own among them:
    /// what `may-dispatch` counts.
    pub by_asking: u32,
}

/// **Why a dispatch is refused**: which limit or rule, with its number. [`Refused::say`] is the
/// sentence the asking chat reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// A limit of 0: dispatch is off at the level that set it.
    Off {
        limit: Limit,
        /// The level that set it to 0.
        by: Source,
        /// The persona dispatched to, which `may-run-at-once` names.
        target: Option<String>,
    },
    /// The persona is already above the asking chat in its chain.
    Loop(String),
    /// The chain is as deep as it may go: `depth` dispatches below the chat the person started.
    TooDeep { limit: u32, depth: u32 },
    /// The asking chat has as many persona chats running as it may: `running` of them.
    TooManyRunning { limit: u32, running: u32 },
    /// The lineage holds as many running chats as it may: `lineage` of them.
    LineageFull { limit: u32, lineage: u32 },
    /// The chats running as this persona have dispatched as many persona chats as the persona
    /// may have running in the project: `running` of them.
    PersonaDispatches {
        persona: String,
        limit: u32,
        running: u32,
    },
    /// As many chats run as this persona as may at once: `running` of them.
    PersonaFull {
        persona: String,
        limit: u32,
        running: u32,
    },
    /// The asking chat has sent as many messages this minute as it may: `sent` of them.
    TooManyMessages { limit: u32, sent: u32 },
}

/// What a chat is told to do about a limit of 0: only the person changes a limit.
pub const ASK_THE_PERSON: &str = "Only the person can change it, in Settings › Project › Dispatch.";

/// What a chat is told to do about a 0 an administrator's policy set.
pub const ASK_AN_ADMINISTRATOR: &str =
    "Only an administrator can change it, in this machine's policy file.";

/// What a chat is told where this machine's policy file was refused: nothing in it was read,
/// so nothing is said to be "set".
pub const POLICY_REFUSED: &str = "its policy file is refused, so purlis cannot tell what an \
     administrator allows. Only an administrator can fix it.";

/// What a chat is told to do about the loop rule.
pub const REPORT_INSTEAD: &str = "Send it what you found in your report instead.";

/// What a chat is told to do at the depth limit.
pub const DO_IT_HERE: &str = "Do the work in this chat, or say in your report what is left.";

/// What a chat is told to do when it has as many persona chats running as it may.
pub const WAIT_TO_DISPATCH: &str = "Wait for one to report, then dispatch again.";

/// What a chat is told to do when a count will fall as other chats finish.
pub const WAIT_FOR_ONE: &str = "Wait for one to finish, then dispatch again.";

/// What a chat is told to do at the message limit.
pub const WAIT_TO_SEND: &str = "Wait a minute, then send it.";

/// `1 chat`, `2 chats`.
fn counted(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl Refused {
    /// The limit that refused, where one did: the loop rule is no limit.
    pub fn limit(&self) -> Option<Limit> {
        match self {
            Self::Off { limit, .. } => Some(*limit),
            Self::Loop(_) => None,
            Self::TooDeep { .. } => Some(Limit::Depth),
            Self::TooManyRunning { .. } => Some(Limit::RunningPerChat),
            Self::LineageFull { .. } => Some(Limit::LivePerLineage),
            Self::PersonaDispatches { .. } => Some(Limit::MayDispatch),
            Self::PersonaFull { .. } => Some(Limit::MayRunAtOnce),
            Self::TooManyMessages { .. } => Some(Limit::MessagesPerMinute),
        }
    }

    /// **The sentence the asking chat reads**: what was refused, and what to do instead. It
    /// starts lowercase, as every refusal of a dispatch does: the caller says what was refused
    /// before it. Written once, here, and pinned by this module's tests.
    pub fn say(&self) -> String {
        match self {
            Self::Off { limit, by, target } => {
                let what = match (limit, target) {
                    (Limit::MessagesPerMinute, _) => "messages between chats are off".to_owned(),
                    (Limit::MayRunAtOnce, Some(target)) if *by != Source::PolicyRefused => {
                        format!("dispatch to {} is off", crate::shown::short(target))
                    }
                    _ => "dispatch is off".to_owned(),
                };
                match by {
                    // Nobody set a limit to 0: the file was not read at all.
                    Source::PolicyRefused => format!("{what} on this machine: {POLICY_REFUSED}"),
                    // A policy's 0 is not the person's to change.
                    Source::Policy => format!(
                        "{what} {}: {} is set to 0. {ASK_AN_ADMINISTRATOR}",
                        by.off_here(),
                        limit.label().to_lowercase()
                    ),
                    _ => format!(
                        "{what} {}: {} is set to 0. {ASK_THE_PERSON}",
                        by.off_here(),
                        limit.label().to_lowercase()
                    ),
                }
            }
            Self::Loop(name) => format!(
                "persona '{}' is already in this chat's own chain of dispatches, and a persona \
                 is never dispatched to from below itself. {REPORT_INSTEAD}",
                crate::shown::short(name)
            ),
            Self::TooDeep { limit, depth } => format!(
                "this chat is {} below the chat the person started, and a chain may go {limit} \
                 deep here. {DO_IT_HERE}",
                counted(*depth, "dispatch", "dispatches")
            ),
            Self::TooManyRunning { limit, running } => format!(
                "this chat already has {} running, and it may have {limit} at once. \
                 {WAIT_TO_DISPATCH}",
                counted(*running, "persona chat", "persona chats")
            ),
            Self::LineageFull { limit, lineage } => format!(
                "this chat's lineage already holds {}, and it may hold {limit}. {WAIT_FOR_ONE}",
                counted(*lineage, "running chat", "running chats")
            ),
            Self::PersonaDispatches {
                persona,
                limit,
                running,
            } => format!(
                "chats as {persona} already have {} running between them, and may have {limit} \
                 at once in this project. {WAIT_FOR_ONE}",
                counted(*running, "persona chat", "persona chats"),
                persona = crate::shown::short(persona)
            ),
            Self::PersonaFull {
                persona,
                limit,
                running,
            } => format!(
                "{} already running as {persona}, and {limit} may run as it at once in this \
                 project. {WAIT_FOR_ONE}",
                if *running == 1 {
                    "1 chat is".to_owned()
                } else {
                    format!("{running} chats are")
                },
                persona = crate::shown::short(persona)
            ),
            Self::TooManyMessages { limit, sent } => format!(
                "this chat has sent that chat {} in the last minute, and it may send {limit}. \
                 {WAIT_TO_SEND}",
                counted(*sent, "message", "messages")
            ),
        }
    }
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.say())
    }
}

/// What [`decide`] answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// No limit stands in the way.
    Allowed,
    /// A limit or the loop rule does: which, and [`Refused::say`] is its sentence.
    Refused(Refused),
}

impl Decision {
    /// The refusal, where it is one.
    pub fn refused(&self) -> Option<&Refused> {
        match self {
            Self::Allowed => None,
            Self::Refused(refused) => Some(refused),
        }
    }
}

impl Limits {
    /// The refusal a limit of 0 gives, where one is 0.
    fn switched_off(&self) -> Option<Refused> {
        self.off().map(|(limit, by)| Refused::Off {
            limit,
            by: by.clone(),
            target: self.target.clone(),
        })
    }
}

/// **Whether a dispatch may start**, by `limits` and the asking chat's `lineage` as it stands.
///
/// In the order the dispatch decision asks (spec #1434): a limit of 0, which switches dispatch
/// off; the loop rule; depth; then the counts, per chat, per lineage and per persona. Each
/// limit refuses at its number and allows one below it. Messages a minute are [`may_send`]'s.
///
/// **The loop rule**: never to a persona above the asking chat. Its own persona is not above
/// it, so a chat may split its own work, as deep as the depth allows.
pub fn decide(limits: &Limits, lineage: &Lineage) -> Decision {
    if let Some(off) = limits.switched_off() {
        return Decision::Refused(off);
    }
    if let Some(target) = &limits.target
        && limits.target != limits.asking
        && lineage.chain.iter().any(|one| one.as_ref() == Some(target))
    {
        return Decision::Refused(Refused::Loop(target.clone()));
    }
    if lineage.depth >= limits.depth {
        return Decision::Refused(Refused::TooDeep {
            limit: limits.depth,
            depth: lineage.depth,
        });
    }
    if lineage.running >= limits.running {
        return Decision::Refused(Refused::TooManyRunning {
            limit: limits.running,
            running: lineage.running,
        });
    }
    if lineage.lineage >= limits.lineage {
        return Decision::Refused(Refused::LineageFull {
            limit: limits.lineage,
            lineage: lineage.lineage,
        });
    }
    if let (Some(limit), Some(persona)) = (limits.may_dispatch, &limits.asking)
        && lineage.by_asking >= limit
    {
        return Decision::Refused(Refused::PersonaDispatches {
            persona: persona.clone(),
            limit,
            running: lineage.by_asking,
        });
    }
    if let (Some(limit), Some(persona)) = (limits.may_run_at_once, &limits.target)
        && lineage.as_target >= limit
    {
        return Decision::Refused(Refused::PersonaFull {
            persona: persona.clone(),
            limit,
            running: lineage.as_target,
        });
    }
    Decision::Allowed
}

/// **Whether the asking chat may send another message** to a chat of its lineage, having sent
/// `in_the_last_minute` already. Only messages a minute decides it: a 0 there stops messages
/// and nothing else, and a 0 elsewhere stops new dispatches and no message of a chat already
/// running.
pub fn may_send(limits: &Limits, in_the_last_minute: u32) -> Decision {
    if limits.messages_per_minute == 0 {
        return Decision::Refused(Refused::Off {
            limit: Limit::MessagesPerMinute,
            by: limits.set_by(Limit::MessagesPerMinute).clone(),
            target: limits.target.clone(),
        });
    }
    if in_the_last_minute >= limits.messages_per_minute {
        return Decision::Refused(Refused::TooManyMessages {
            limit: limits.messages_per_minute,
            sent: in_the_last_minute,
        });
    }
    Decision::Allowed
}

// ---- reading a settings file -------------------------------------------------------------------

/// What a file's `[dispatch]` holds, and each thing in it that is not taken, as one sentence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Read {
    pub table: Table,
    pub refused: Vec<String>,
}

/// One value as a limit, or why it is not one. `here` is its key, dotted.
fn value_of(limit: Limit, value: &toml::Value, here: &str, file: &str) -> Result<u32, String> {
    let most = if limit == Limit::Depth { DEEPEST } else { MOST };
    let whole = value.as_integer().filter(|n| *n >= 0).ok_or_else(|| {
        format!(
            "{here} in {file} is not a whole number of 0 or more, so it is not read and \
                 the level beneath it is in force — write {} = {}",
            limit.word(),
            limit.when_unset().unwrap_or(1)
        )
    })?;
    u32::try_from(whole)
        .ok()
        .filter(|n| *n <= most)
        .ok_or_else(|| {
            if limit == Limit::Depth {
                format!(
                    "{here} in {file} is {whole}, and depth is never above {DEEPEST}, so \
                     it is not read and the level beneath it is in force"
                )
            } else {
                format!(
                    "{here} in {file} is {whole}, and no limit is above {MOST}, so it is not \
                     read and the level beneath it is in force"
                )
            }
        })
}

/// One level's table: its limits, with each key that is not one refused. `nested` are the
/// keys that hold further tables, which the project's own level has.
fn level_of(
    table: &toml::Table,
    here: &str,
    file: &str,
    persona: bool,
    nested: &[&str],
    refused: &mut Vec<String>,
) -> Level {
    let mut level = Level::unset();
    for (key, value) in table {
        if nested.contains(&key.as_str()) {
            continue;
        }
        let at = format!("{here}.{key}");
        let Some(limit) = Limit::of_word(key) else {
            refused.push(format!(
                "{at} in {file} is not a key purlis reads — a dispatch limit is one of {}",
                listed(persona)
            ));
            continue;
        };
        if limit.persona_only() && !persona {
            refused.push(format!(
                "{at} in {file} is a persona's limit, so it is not read here — write it under \
                 [{TABLE}.{PERSONAS}.<persona>]"
            ));
            continue;
        }
        match value_of(limit, value, &at, file) {
            Ok(value) => level = level.with(limit, value),
            Err(why) => refused.push(why),
        }
    }
    level
}

/// The limits' keys, as a refusal lists them.
fn listed(persona: bool) -> String {
    Limit::ALL
        .into_iter()
        .filter(|limit| persona || !limit.persona_only())
        .map(Limit::word)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The tables under `[dispatch] <key>`, one per name `ok` takes.
fn named_levels(
    value: Option<&toml::Value>,
    key: &str,
    what: &str,
    ok: fn(&str) -> bool,
    file: &str,
    refused: &mut Vec<String>,
) -> BTreeMap<String, Level> {
    let mut out = BTreeMap::new();
    let Some(value) = value else {
        return out;
    };
    let at = format!("{TABLE}.{key}");
    let Some(table) = value.as_table() else {
        refused.push(format!(
            "{at} in {file} is not a table, so no {what} has limits of its own — write \
             [{at}.<{what}>]"
        ));
        return out;
    };
    for (name, level) in table {
        let here = format!("{at}.{name}");
        if !ok(name) {
            refused.push(format!(
                "{here} in {file} is not a {what}'s name, so it sets nothing"
            ));
            continue;
        }
        let Some(level) = level.as_table() else {
            refused.push(format!(
                "{here} in {file} is not a table, so it sets nothing — it holds limits"
            ));
            continue;
        };
        out.insert(
            name.clone(),
            level_of(level, &here, file, key == PERSONAS, &[], refused),
        );
    }
    out
}

/// **What `text`, a whole settings file, says in `[dispatch]`** (`None`: no file). A file that
/// is not TOML says nothing here: every other reader of it refuses it already.
pub fn read(text: Option<&str>, file: &str) -> Read {
    let mut out = Read::default();
    let Some(top) = text.and_then(|text| text.parse::<toml::Table>().ok()) else {
        return out;
    };
    let Some(value) = top.get(TABLE) else {
        return out;
    };
    let Some(table) = value.as_table() else {
        out.refused.push(format!(
            "{TABLE} in {file} is not a table, so no dispatch limit is read from it — write \
             [{TABLE}] with limits"
        ));
        return out;
    };
    let refused = &mut out.refused;
    // `grants` is the project's dispatch grants (#1437), read by `dispatchgrant`: not a limit
    // and not a level, and what it holds that grants nothing is said with the rest.
    let grants = crate::dispatchgrant::KEY;
    out.table.project = level_of(
        table,
        TABLE,
        file,
        false,
        &[WORKSPACES, PERSONAS, grants],
        refused,
    );
    if table.contains_key(grants) {
        if file == crate::profiles::LOCAL_FILE {
            refused.push(format!(
                "{TABLE}.{grants} in {file} is not read: a dispatch grant of your own is kept \
                 by purlis on this machine, and the project's are in its committed file"
            ));
        } else {
            refused.extend(crate::dispatchgrant::committed(text).refused);
        }
    }
    out.table.workspaces = named_levels(
        table.get(WORKSPACES),
        WORKSPACES,
        "workspace",
        crate::contain::workspace_name_ok,
        file,
        refused,
    );
    out.table.personas = named_levels(
        table.get(PERSONAS),
        PERSONAS,
        "persona",
        crate::personas::valid_name,
        file,
        refused,
    );
    out
}

/// Everything in `text`'s `[dispatch]` that purlis would not read as written, as `file` holds
/// it: what the Settings tab's save refuses to write. A depth above [`DEEPEST`] is one.
pub fn refusals(text: &str, file: &str) -> Vec<String> {
    read(Some(text), file).refused
}

/// **A policy's ceiling**, from the `dispatch` object of an administrator's policy file: each
/// key is a limit's word and each value a whole number, or why the file is refused.
pub fn ceiling(dispatch: &serde_json::Map<String, serde_json::Value>) -> Result<Level, String> {
    let mut level = Level::unset();
    for (key, value) in dispatch {
        let Some(limit) = Limit::of_word(key) else {
            return Err(format!(
                "its dispatch says \"{}\", which purlis does not know",
                crate::shown::one_line(key, 40)
            ));
        };
        let most = if limit == Limit::Depth { DEEPEST } else { MOST };
        let whole = value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n <= most)
            .ok_or_else(|| format!("its \"{key}\" is not a whole number from 0 to {most}"))?;
        level = level.with(limit, whole);
    }
    Ok(level)
}

/// **The ceiling of a policy file that was refused**: dispatch is off, and no message passes,
/// until an administrator fixes the file. purlis cannot tell what the file meant to allow, so
/// it says the file is refused ([`Source::PolicyRefused`]) and never that a limit "is set to 0",
/// which no one set.
pub fn ceiling_when_refused() -> Level {
    Level {
        refused: true,
        ..Limit::ALL
            .into_iter()
            .fold(Level::unset(), |level, limit| level.with(limit, 0))
    }
}

// ---- the project's files -----------------------------------------------------------------------

/// The project's two tables as its files stand: the committed one, and this machine's own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Files {
    /// The committed file's.
    pub project: Table,
    /// This machine's own, which only ever lowers: read even where git would carry its file.
    pub mine: Table,
    /// What either file holds in `[dispatch]` that is not read, each as one sentence.
    pub refused: Vec<String>,
}

impl Files {
    /// The tables of the project at `root`. The committed file is read as the sandbox reads it:
    /// never through a link, and never one too large. **Nothing else is read**: not a
    /// workspace's manifest, and not a persona's own file, which a chat may write.
    pub fn read(root: &Path) -> Self {
        let committed = crate::sandbox::read_plane_file(&crate::names::manifest(root))
            .ok()
            .flatten();
        let local = crate::settings::layer_text(root, crate::settings::Which::Local);
        let mut files = Self::of(committed.as_deref(), &local);
        for said in &mut files.refused {
            *said = crate::settings::named_at(root, said);
        }
        files
    }

    /// The tables of `committed`, the committed file's text (`None`: no file), and of `local`,
    /// this machine's file as its layer was read.
    ///
    /// **This machine's table is read whether or not git would carry its file.** Every other
    /// table of that file is left out when it would ([`crate::settings::layer_text`]), because
    /// it could loosen what the project says with no trace in git. This one only ever lowers a
    /// limit, so honouring it is the strict reading: dropping it would lift a limit the person
    /// set for themselves the moment the file's ignore rule went missing.
    pub fn of(committed: Option<&str>, local: &crate::settings::LayerText) -> Self {
        use crate::settings::LayerText;
        let shared = read(committed, crate::profiles::COMMITTED_FILE);
        let local = match local {
            LayerText::Text(text) | LayerText::LeftOut { text, .. } => Some(text.as_str()),
            LayerText::Nothing => None,
        };
        let mine = read(local, crate::profiles::LOCAL_FILE);
        Self {
            project: shared.table,
            mine: mine.table,
            refused: shared.refused.into_iter().chain(mine.refused).collect(),
        }
    }

    /// The limits in force for a dispatch in the project these were read from, under `policy`.
    pub fn in_force(
        &self,
        workspace: Option<&str>,
        asking: Option<&str>,
        target: Option<&str>,
        policy: &Level,
    ) -> Limits {
        in_force(&self.project, workspace, asking, target, &self.mine, policy)
    }
}

/// **Off at the destination is off** (#1453, D-1453-17): the refusal where workspace
/// `workspace`, the one a dispatch's chat is to work **in**, sets a limit that switches
/// dispatch off to 0, in the project's file or on this machine. `target` is the persona
/// dispatched to, for the sentence.
///
/// **Whatever a persona's own row says.** For the asking chat's own workspace the most
/// specific level wins, so a persona's value outranks its workspace's 0 (ruling 9). A
/// workspace a chat is sent *into* from elsewhere is different: its 0 is that workspace
/// saying no chat is started in it by another, and a persona carrying a value of its own into
/// every workspace would make that 0 mean nothing. An administrator's policy is not read
/// here: it is a ceiling, and [`in_force`] has applied it.
pub fn off_in(root: &Path, workspace: &str, target: Option<&str>) -> Option<Refused> {
    let files = Files::read(root);
    let off = |table: &Table| {
        let level = table.workspaces.get(workspace)?;
        Limit::ALL
            .into_iter()
            .filter(|limit| *limit != Limit::MessagesPerMinute && !limit.persona_only())
            .find(|limit| level.get(*limit) == Some(0))
    };
    off(&files.project)
        .or_else(|| off(&files.mine))
        .map(|limit| Refused::Off {
            limit,
            by: Source::Workspace(workspace.to_owned()),
            target: target.map(str::to_owned),
        })
}

/// **The limits in force for a dispatch in the project at `root`**, on this machine: its files
/// as they stand now, under this machine's policy. Read afresh each time, so a limit changed in
/// Settings applies to the next dispatch.
pub fn of(
    root: &Path,
    workspace: Option<&str>,
    asking: Option<&str>,
    target: Option<&str>,
) -> Limits {
    let policy = crate::sandbox::policy::Locks::of(root);
    Files::read(root).in_force(workspace, asking, target, policy.dispatch_ceiling())
}

#[cfg(test)]
#[path = "dispatchlimits_tests.rs"]
mod tests;
