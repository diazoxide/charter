//! Whether a dispatch may start: the one decision every dispatch goes through (#1434, #1436).
//!
//! A dispatch is one chat starting another, running as a persona, with a brief (ADR 0090). An
//! agent's `purlis dispatch`, the matching chat tool, the person's own dispatch from a tab and
//! the vault-refused Notice's button all ask [`decide`], and nothing starts a persona chat
//! without it answering [`Decision::Start`].
//!
//! **Pure, and fed only what the app holds.** Every field of a [`Request`] is the app's own
//! record of the asking chat, the project's definition of the persona, and the limits and
//! grants in force. Nothing in it is a word the asking chat sent, except the persona it names.
//! So what a chat can say changes which persona is asked for, and nothing else about the answer.
//!
//! # The order of the checks
//!
//! 1. the asker is a chat, not a harness's helper sub-agent ([`Refused::Helper`]);
//! 2. the asking chat is on a harness profile ([`Refused::NoProfile`]);
//! 3. the persona exists and is not a draft ([`Refused::NoPersona`], [`Refused::Draft`]);
//! 4. the loop rule ([`Refused::Loop`]) and the depth ([`Refused::TooDeep`]);
//! 5. how many the asking chat has running, and how many its lineage holds
//!    ([`Refused::TooManyRunning`], [`Refused::LineageFull`]);
//! 6. the grant: none is needed for the asking chat's own persona, or when the person
//!    dispatches; any other pair answers [`Decision::NeedsGrant`] until one is in force.
//!
//! Policy locks stand between 3 and 4, the per-persona counts beside 5 and the machine's memory
//! after 6 (#1434's order). Each arrives with its ticket (#1437, #1439, #1440) as a field of
//! [`Request`], which is why the request is a struct and not a list of arguments.

/// Why a dispatch is made: what the persona chat is for. One type with the record's
/// ([`crate::reopen::Mode`]), which is where a chat keeps it.
pub use crate::reopen::Mode;

/// The asking chat, as the app's record has it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AskingChat<'a> {
    /// The persona it runs as: its own, else the one a new chat adopts by default. `None` is a
    /// chat on no persona.
    pub persona: Option<&'a str>,
    /// Whether it holds another persona's grants instead of its own (#1362), which the person
    /// has not yet allowed away. Such a chat's own persona is not yet its own to pass on.
    pub held: bool,
    /// The harness profile it started on.
    pub profile: Option<&'a str>,
    /// How many dispatches stand between it and the chat the person started: 0 for that chat.
    pub depth: u32,
    /// The personas of the chats above it in its lineage, nearest first.
    pub chain: &'a [Option<&'a str>],
    /// How many persona chats it dispatched are still running.
    pub running: u32,
    /// How many chats its lineage holds that are still running, itself included.
    pub lineage: u32,
}

/// Who asks for a dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asker<'a> {
    /// A chat the app has open.
    Chat(AskingChat<'a>),
    /// A harness's helper sub-agent, inside a chat. It is not a chat: nobody can see it, open
    /// it or stop it apart from its chat.
    Helper,
    /// The person, from this chat's tab (#1438). No grant is needed: the person is the one a
    /// grant is asked of.
    Person(AskingChat<'a>),
}

/// The persona a dispatch names, as the project defines it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Persona<'a> {
    /// Defined, and finished.
    Defined(&'a str),
    /// Defined, and still a draft (`draft: true`).
    Draft(&'a str),
    /// Not one this project defines, or one that does not load.
    Unknown(&'a str),
    /// No persona: a chat on none dispatching to "its own".
    None,
}

impl<'a> Persona<'a> {
    fn name(self) -> Option<&'a str> {
        match self {
            Self::Defined(name) | Self::Draft(name) | Self::Unknown(name) => Some(name),
            Self::None => None,
        }
    }
}

/// The limits in force for one dispatch, already resolved to one number each: the most specific
/// of the project's, the workspace's and the persona's, under policy's ceiling (#1439, #1440).
/// 0 switches dispatch off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// How many running persona chats one asking chat may have.
    pub running: u32,
    /// How many running chats one lineage may hold.
    pub lineage: u32,
    /// How deep a chain of dispatches may go. Never read above [`DEEPEST`].
    pub depth: u32,
}

/// The depth no setting raises (#1434).
pub const DEEPEST: u32 = 8;

impl Default for Limits {
    /// The defaults a project that set none runs with (#1434).
    fn default() -> Self {
        Self {
            running: 6,
            lineage: 16,
            depth: 3,
        }
    }
}

/// Whether a dispatch grant is in force for a pair of personas. How grants are stored and
/// given is #1437's; the decision only reads the answer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Grant {
    /// No grant covers this pair.
    #[default]
    Missing,
    /// The person granted it: for this chat, for themselves on this machine, or for the project.
    InForce,
}

/// One dispatch, as the app asks about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request<'a> {
    pub asker: Asker<'a>,
    /// The persona the new chat would run as.
    pub to: Persona<'a>,
    pub mode: Mode,
    /// Whether a grant covers the asking chat's persona dispatching to [`Self::to`].
    pub grant: Grant,
    pub limits: Limits,
}

/// What the app does with a dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Start the persona chat.
    Start,
    /// Start nothing, and ask the person for a dispatch grant for this pair.
    NeedsGrant {
        /// The asking chat's persona, or `None` for a chat on none.
        from: Option<String>,
        /// The persona it asked for.
        to: String,
    },
    /// Start nothing: this dispatch is not one a grant could allow.
    Refused(Refused),
}

/// Why a dispatch is refused. Each says what to do in [`Refused::say`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// A helper sub-agent asked.
    Helper,
    /// The asking chat is on no harness profile.
    NoProfile,
    /// The asking chat holds another persona's grants instead of its own (#1362), which the
    /// person has not yet allowed away: it has no persona of its own to dispatch as or from.
    Held,
    /// No persona was named, and the asking chat's own is one.
    NoPersonaNamed,
    /// The project defines no such persona, or it does not load.
    NoPersona(String),
    /// The persona is a draft.
    Draft(String),
    /// The persona is already in the asking chat's own chain.
    Loop(String),
    /// The chain is as deep as it may go.
    TooDeep { limit: u32 },
    /// The asking chat has as many persona chats running as it may.
    TooManyRunning { limit: u32 },
    /// The lineage holds as many running chats as it may.
    LineageFull { limit: u32 },
}

impl Refused {
    /// The sentence the asking chat reads: what was refused, and what to do instead.
    pub fn say(&self) -> String {
        match self {
            Self::Helper => HELPER.to_owned(),
            Self::NoProfile => "this chat is not on a harness profile, so there is no harness \
                                to start a persona chat on. Dispatch from a chat that was \
                                started on a profile."
                .to_owned(),
            Self::Held => HELD.to_owned(),
            Self::NoPersonaNamed => "a dispatch names the persona its chat runs as. Name one \
                                     with --to, or leave it out for this chat's own."
                .to_owned(),
            Self::NoPersona(name) => format!(
                "this project has no persona '{}' that loads. List the personas with `purlis \
                 persona list`, then dispatch to one of them.",
                crate::shown::short(name)
            ),
            Self::Draft(name) => {
                let name = crate::shown::short(name);
                format!(
                    "persona '{name}' is still a draft, and a draft persona runs no chat. \
                     Finish its definition and drop its `draft: true` line, or dispatch to \
                     another persona."
                )
            }
            Self::Loop(name) => format!(
                "persona '{}' is already in this chat's own chain of dispatches, and a persona \
                 is never dispatched to from below itself. Send it what you found in your \
                 report instead.",
                crate::shown::short(name)
            ),
            // A limit of 0 is the off switch, and says so: nothing is "already" at it.
            Self::TooDeep { limit: 0 }
            | Self::TooManyRunning { limit: 0 }
            | Self::LineageFull { limit: 0 } => "dispatch is switched off here: a limit it \
                                                 runs under is set to 0. Do the work in this \
                                                 chat, or ask the person to raise the limit."
                .to_owned(),
            Self::TooDeep { limit } => format!(
                "this chat is {limit} dispatches below the chat the person started, which is \
                 as deep as a chain may go here. Do the work in this chat, or say in your \
                 report what is left."
            ),
            Self::TooManyRunning { limit } => format!(
                "this chat already has {limit} persona chats that have not reported, which is \
                 as many as it may have at once. Wait for one to report or stop one, then \
                 dispatch again."
            ),
            Self::LineageFull { limit } => format!(
                "this chat's lineage already holds {limit} chats that have not reported, \
                 which is as many as it may hold. Wait for one to report or stop one, then \
                 dispatch again."
            ),
        }
    }
}

/// What a helper sub-agent that tries to dispatch is told ([`Refused::Helper`]). Spelled once:
/// the tool guard says it before the command runs, where a sub-agent is known for one.
pub const HELPER: &str = "a dispatch is refused from inside a sub-agent. A persona chat belongs \
     to a chat the person can see, open and stop, and a sub-agent is not one. Return what you \
     found to your chat, and let that chat dispatch.";

/// What a chat that holds another persona's grants is told ([`Refused::Held`]).
pub const HELD: &str = "this chat runs with another persona's grants until the person allows \
     its own, so it cannot dispatch yet. Ask the person to allow this chat its own grants on \
     its tab, then dispatch again.";

/// A task's name as purlis will draw it, or why not.
///
/// A chat's name by the rule every chat's name is held to ([`crate::reopen::label`]), and not
/// empty. And none of the marks purlis's own lines are made of: a task's name is written into
/// the stamp of whatever its chat dispatches and into the heading of its report, between `⟨`
/// and `⟩`, beside `·` and inside a code span. A name holding one could close purlis's line
/// early and write the rest of it. **Refused, never stripped**, for the label's own reason.
pub fn task_name(raw: &str) -> Result<String, String> {
    let Some(name) = crate::reopen::label(raw)? else {
        return Err("a task needs a name, which its chat is called and listed under.".to_owned());
    };
    if name.contains(['⟨', '⟩', '·', '`']) {
        return Err(
            "That name holds one of ⟨ ⟩ · or a backtick, which purlis writes its own lines \
             with, so it will not draw them in a task's name."
                .to_owned(),
        );
    }
    Ok(name)
}

/// What a chat's dispatch comes to: the decision, and the facts the app starts the persona
/// chat with when the decision is to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub decision: Decision,
    /// The persona the new chat runs as: the one named, else the asking chat's own.
    pub to: Option<String>,
    /// How deep the new chat is in its chain: one below the chat that asks.
    pub depth: u32,
}

/// The decision for a task the chat recorded as `asking` asks for, naming `named` (or no
/// persona, for its own), in the project at `root`.
///
/// **Every fact about the asker is read here from the app's record of it**, in one place, so
/// no caller builds a [`Request`] its own way: the persona it runs as (its own, else
/// `default`, the one a new chat adopts), whether it holds another's grants, its profile, and
/// where `lineage` says it stands.
pub fn asked_by_a_chat(
    root: &std::path::Path,
    asking: &crate::reopen::Chat,
    named: Option<&str>,
    default: Option<&str>,
    lineage: &Lineage,
    grant: Grant,
    limits: Limits,
) -> Asked {
    // The persona it runs as: its own, else the one a new chat adopts by default.
    let runs_as = asking.persona.as_deref().or(default);
    // The persona asked for: the one named, else its own.
    let to_name = named
        .map(str::trim)
        .filter(|named| !named.is_empty())
        .or(runs_as);
    let chain: Vec<Option<&str>> = lineage.chain.iter().map(Option::as_deref).collect();
    let request = Request {
        asker: Asker::Chat(AskingChat {
            persona: runs_as,
            held: asking.held.is_some(),
            profile: asking.profile.as_deref(),
            depth: lineage.depth,
            chain: &chain,
            running: lineage.running,
            lineage: lineage.lineage,
        }),
        to: match to_name {
            Some(name) => persona_in(root, name),
            None => Persona::None,
        },
        mode: Mode::Task,
        grant,
        limits,
    };
    Asked {
        decision: decide(&request),
        to: to_name.map(str::to_owned),
        depth: lineage.depth.saturating_add(1).min(DEEPEST),
    }
}

/// Whether the dispatch `request` describes may start.
pub fn decide(request: &Request<'_>) -> Decision {
    let refused = |why| Decision::Refused(why);
    // 1. Who asks.
    let (asking, by_the_person) = match request.asker {
        Asker::Helper => return refused(Refused::Helper),
        Asker::Chat(asking) => (asking, false),
        Asker::Person(asking) => (asking, true),
    };
    // 2. A harness to start the persona chat on, and grants of its own to dispatch with: a
    // chat still holding another persona's has none (#1362), whatever it asks for. The person
    // dispatching from its tab is not held to that: the hold is theirs to lift.
    if asking.profile.is_none() {
        return refused(Refused::NoProfile);
    }
    if asking.held && !by_the_person {
        return refused(Refused::Held);
    }
    // 3. The persona.
    match request.to {
        Persona::Unknown(name) => return refused(Refused::NoPersona(name.to_owned())),
        Persona::Draft(name) => return refused(Refused::Draft(name.to_owned())),
        Persona::Defined(_) | Persona::None => {}
    }
    let to = request.to.name();
    // 4. The loop rule: never to a persona above the asking chat. Its own persona is not above
    // it, so a chat may split its own work, as deep as the depth allows.
    if let Some(name) = to
        && to != asking.persona
        && asking.chain.contains(&to)
    {
        return refused(Refused::Loop(name.to_owned()));
    }
    let deepest = request.limits.depth.min(DEEPEST);
    if asking.depth >= deepest {
        return refused(Refused::TooDeep { limit: deepest });
    }
    // 5. The counts.
    if asking.running >= request.limits.running {
        return refused(Refused::TooManyRunning {
            limit: request.limits.running,
        });
    }
    if asking.lineage >= request.limits.lineage {
        return refused(Refused::LineageFull {
            limit: request.limits.lineage,
        });
    }
    // 6. The grant. A chat's own persona needs none; the person needs none; a pair the person
    // granted has one.
    let its_own = to == asking.persona;
    if by_the_person || its_own {
        return Decision::Start;
    }
    match to {
        Some(_) if request.grant == Grant::InForce => Decision::Start,
        Some(to) => Decision::NeedsGrant {
            from: asking.persona.map(str::to_owned),
            to: to.to_owned(),
        },
        // No persona named, for a chat that runs as one: not a pair a grant could name.
        None => refused(Refused::NoPersonaNamed),
    }
}

// ----------------------------------------------------------------------------------------
// the facts a request is made of, read from what the app holds
// ----------------------------------------------------------------------------------------

/// `name` as the project at `root` defines it: the persona a dispatch names, asked of the
/// definition and never of the request ([`crate::personas::name_refusal`], the answer every
/// command that takes a persona's name gives).
pub fn persona_in<'a>(root: &std::path::Path, name: &'a str) -> Persona<'a> {
    if crate::personas::name_refusal(root, name).is_some() {
        Persona::Unknown(name)
    } else if crate::personaverbs::is_draft(root, name) {
        Persona::Draft(name)
    } else {
        Persona::Defined(name)
    }
}

/// Where an asking chat stands among the chats the app has open: what [`AskingChat`] says of
/// its lineage, owned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lineage {
    /// [`AskingChat::depth`].
    pub depth: u32,
    /// [`AskingChat::chain`].
    pub chain: Vec<Option<String>>,
    /// [`AskingChat::running`].
    pub running: u32,
    /// [`AskingChat::lineage`].
    pub lineage: u32,
}

/// The lineage of chat `asking` among `open`, the chats the app has open by its number for
/// each. `default` is the persona a chat that names none runs as, and `working` whether a
/// chat's program is still running.
///
/// **Read from the app's records and nothing else.** A chat's depth is the one its own record
/// holds, written when it was dispatched, so closing the chat above it never makes a chain
/// look shallower. The chain is of the chats still open.
///
/// **The counts are of chats that still owe work** (D-1436-18): one that has not reported and
/// whose program has not ended. A chat that reported stays open for the person to read and
/// costs nothing; one whose program ended without a report has failed, and is not running.
pub fn lineage_of(
    asking: u32,
    open: &[(u32, &crate::reopen::Chat)],
    default: Option<&str>,
    working: &dyn Fn(u32) -> bool,
) -> Lineage {
    let record = |number: u32| {
        open.iter()
            .find(|(n, _)| *n == number)
            .map(|(_, chat)| *chat)
    };
    let asker_of = |number: u32| record(number)?.from.as_ref().map(|from| from.chat);
    let persona =
        |chat: &crate::reopen::Chat| chat.persona.clone().or_else(|| default.map(str::to_owned));
    // Up: the chats above it that are still open, nearest first. `seen` ends a loop that only
    // a record somebody else wrote could hold.
    let mut seen = vec![asking];
    let mut chain = Vec::new();
    let mut top = asking;
    while let Some(above) = asker_of(top).filter(|above| !seen.contains(above)) {
        let Some(chat) = record(above) else { break };
        chain.push(persona(chat));
        seen.push(above);
        top = above;
    }
    // Down from the top: every open chat that descends from it, itself included.
    let mut lineage = vec![top];
    let mut at = 0;
    while at < lineage.len() {
        let above = lineage[at];
        for (number, _) in open {
            if asker_of(*number) == Some(above) && !lineage.contains(number) {
                lineage.push(*number);
            }
        }
        at += 1;
    }
    // Still owing work: its program runs, and it has not sent the report it owes.
    let owes_work = |number: u32| {
        working(number)
            && record(number).is_some_and(|chat| {
                chat.from
                    .as_ref()
                    .is_none_or(|from| from.report != crate::reopen::Owed::Sent)
            })
    };
    // The tasks it dispatched that still owe it work. A handoff's work is not this chat's to
    // wait on.
    let running = open
        .iter()
        .filter(|(number, chat)| {
            *number != asking
                && owes_work(*number)
                && chat
                    .from
                    .as_ref()
                    .is_some_and(|from| from.chat == asking && from.mode == Mode::Task)
        })
        .count();
    let lineage: Vec<u32> = lineage
        .into_iter()
        .filter(|number| owes_work(*number))
        .collect();
    Lineage {
        depth: record(asking)
            .and_then(|chat| chat.from.as_ref())
            .map_or(0, |from| from.depth),
        chain,
        running: u32::try_from(running).unwrap_or(u32::MAX),
        lineage: u32::try_from(lineage.len()).unwrap_or(u32::MAX),
    }
}

/// The start of the persona chat a dispatch from `asking` opens, running as `persona` under
/// the name `name`.
///
/// **Everything it takes from the asking chat is here, and it is two things**: the profile and
/// the folder. Its sandbox is what the project compiles for `persona` in that folder
/// ([`crate::start::ready`]): the asking chat's per-chat grants, the grants it holds, its
/// opt-out and whatever its harness was switched to while it ran are not the new chat's, and
/// this never copies them (#1434).
pub fn start_for(
    asking: &crate::reopen::Chat,
    persona: Option<String>,
    name: String,
) -> crate::start::Start {
    crate::start::Start {
        profile: asking.profile.clone(),
        persona,
        name,
        cwd: asking.cwd.clone(),
        resume: None,
        show_footer: false,
        resuming: None,
        without_sandbox: None,
        held: None,
        grants: crate::sandbox::grant::Grants::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- the facts, from the app's records ------------------------------------------------

    use crate::reopen::{Chat, HandedFrom, HeldGrants, Owed};

    fn chat(persona: Option<&str>) -> Chat {
        Chat {
            program: "claude".to_owned(),
            name: "1".to_owned(),
            profile: Some("work".to_owned()),
            persona: persona.map(str::to_owned),
            ..Default::default()
        }
    }

    fn dispatched(by: u32, depth: u32, mode: Mode, report: Owed, persona: Option<&str>) -> Chat {
        Chat {
            from: Some(HandedFrom {
                chat: by,
                name: "steward 1".to_owned(),
                workspace: crate::active::Place::Workspace("alpha".to_owned()),
                report,
                mode,
                depth,
            }),
            ..chat(persona)
        }
    }

    #[test]
    fn a_chat_the_person_started_stands_alone_at_depth_zero() {
        let one = chat(Some("steward"));
        assert_eq!(
            lineage_of(1, &[(1, &one)], None, &|_| true),
            Lineage {
                depth: 0,
                chain: Vec::new(),
                running: 0,
                lineage: 1,
            }
        );
    }

    #[test]
    fn a_lineage_is_read_from_the_records_of_the_chats_that_are_open() {
        // 1 (steward) ── 2 (devops, task) ── 4 (devops, task)
        //             ├─ 3 (steward, task, reported)
        //             └─ 5 (qa, handoff)
        // 6 is another lineage.
        let one = chat(Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        let three = dispatched(1, 1, Mode::Task, Owed::Sent, Some("steward"));
        let four = dispatched(2, 2, Mode::Task, Owed::Due, Some("devops"));
        let five = dispatched(1, 1, Mode::Handoff, Owed::Nothing, Some("qa"));
        let six = chat(Some("steward"));
        let open = [
            (1, &one),
            (2, &two),
            (3, &three),
            (4, &four),
            (5, &five),
            (6, &six),
        ];

        assert_eq!(
            lineage_of(1, &open, None, &|_| true),
            Lineage {
                depth: 0,
                chain: Vec::new(),
                // 2 is still working. 3 has reported, and 5 was handed off: neither is a task
                // this chat is waiting on.
                running: 1,
                // And 3, having reported, is no longer one the lineage holds.
                lineage: 4,
            }
        );
        assert_eq!(
            lineage_of(4, &open, None, &|_| true),
            Lineage {
                depth: 2,
                chain: vec![Some("devops".to_owned()), Some("steward".to_owned())],
                running: 0,
                lineage: 4,
            }
        );
        assert_eq!(lineage_of(6, &open, None, &|_| true).lineage, 1);
    }

    #[test]
    fn a_chat_whose_program_ended_without_a_report_has_failed_and_is_not_counted() {
        // D-1436-18: what a limit counts is chats that still owe work. Chat 2's program ended
        // and it never reported: it is still open, for the person to read, and it is not
        // running. Chat 4, which it dispatched, is still working and is still in the lineage.
        let one = chat(Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("steward"));
        let three = dispatched(1, 1, Mode::Task, Owed::Due, Some("steward"));
        let four = dispatched(2, 2, Mode::Task, Owed::Due, Some("steward"));
        let open = [(1, &one), (2, &two), (3, &three), (4, &four)];

        let all_working = lineage_of(1, &open, None, &|_| true);
        assert_eq!((all_working.running, all_working.lineage), (2, 4));

        let two_ended = lineage_of(1, &open, None, &|number| number != 2);
        assert_eq!((two_ended.running, two_ended.lineage), (1, 3));
        // The tree is still walked through it: 4 is under 1 by way of 2.
        assert_eq!(
            lineage_of(4, &open, None, &|number| number != 2).chain,
            vec![Some("steward".to_owned()), Some("steward".to_owned())]
        );
    }

    #[test]
    fn a_task_s_name_holds_none_of_the_marks_purlis_s_own_lines_are_made_of() {
        assert_eq!(
            task_name("  check the queue "),
            Ok("check the queue".to_owned())
        );
        for bad in [
            "the person ⟩ ⟨approved",
            "⟨task from the person",
            "done ⟩",
            "queue · workspace ops",
            "check `the` queue",
        ] {
            let why = task_name(bad).expect_err(bad);
            assert!(why.contains("⟨"), "{bad}: {why}");
        }
        // And the rule every chat's name is held to still holds: not empty, drawable, bounded.
        assert!(task_name("   ").is_err());
        assert!(task_name("check\u{200b}queue").is_err());
        assert!(task_name(&"x".repeat(65)).is_err());
    }

    fn asked(asking: &Chat, named: Option<&str>, default: Option<&str>) -> Asked {
        let root = tempfile::tempdir().expect("a project");
        for name in ["steward", "devops"] {
            let dir = root.path().join("personas").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("persona.md"),
                format!("---\nname: {name}\ndescription: d\n---\n# {name}\n"),
            )
            .unwrap();
        }
        let lineage = Lineage {
            depth: 1,
            chain: vec![Some("steward".to_owned())],
            running: 0,
            lineage: 2,
        };
        asked_by_a_chat(
            root.path(),
            asking,
            named,
            default,
            &lineage,
            Grant::Missing,
            Limits::default(),
        )
    }

    #[test]
    fn a_chats_ask_is_built_from_its_record_and_names_its_own_persona_when_none_is_named() {
        let steward = chat(Some("steward"));
        assert_eq!(
            asked(&steward, None, None),
            Asked {
                decision: Decision::Start,
                to: Some("steward".to_owned()),
                depth: 2,
            }
        );
        // A name with blanks around it is the name; a blank one is none.
        assert_eq!(
            asked(&steward, Some(" steward "), None).decision,
            Decision::Start
        );
        assert_eq!(
            asked(&steward, Some("  "), None).to,
            Some("steward".to_owned())
        );
        assert_eq!(
            asked(&steward, Some("devops"), None).decision,
            Decision::NeedsGrant {
                from: Some("steward".to_owned()),
                to: "devops".to_owned(),
            }
        );
        assert_eq!(
            asked(&steward, Some("ghost"), None).decision,
            Decision::Refused(Refused::NoPersona("ghost".to_owned()))
        );
    }

    #[test]
    fn a_chat_on_no_persona_asks_as_the_default_and_with_no_default_as_none() {
        let plain = chat(None);
        assert_eq!(
            asked(&plain, None, Some("steward")),
            Asked {
                decision: Decision::Start,
                to: Some("steward".to_owned()),
                depth: 2,
            }
        );
        assert_eq!(
            asked(&plain, None, None),
            Asked {
                decision: Decision::Start,
                to: None,
                depth: 2,
            }
        );
    }

    #[test]
    fn a_chats_ask_reads_its_profile_its_hold_and_its_lineage_from_the_record() {
        let shell = Chat {
            profile: None,
            ..chat(Some("steward"))
        };
        assert_eq!(
            asked(&shell, None, None).decision,
            Decision::Refused(Refused::NoProfile)
        );
        let holding = Chat {
            held: Some(HeldGrants {
                persona: Some("steward".to_owned()),
            }),
            ..chat(Some("devops"))
        };
        assert_eq!(
            asked(&holding, None, None).decision,
            Decision::Refused(Refused::Held)
        );
        // The lineage it was handed: devops is asked for from under steward, by a chat that is
        // not steward, which the loop rule refuses whatever the grant.
        let devops = chat(Some("devops"));
        assert_eq!(
            asked(&devops, Some("steward"), None).decision,
            Decision::Refused(Refused::Loop("steward".to_owned()))
        );
    }

    #[test]
    fn a_chat_on_no_persona_is_in_a_chain_as_the_default_persona() {
        let one = chat(None);
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        assert_eq!(
            lineage_of(2, &[(1, &one), (2, &two)], Some("steward"), &|_| true).chain,
            vec![Some("steward".to_owned())]
        );
        assert_eq!(
            lineage_of(2, &[(1, &one), (2, &two)], None, &|_| true).chain,
            vec![None]
        );
    }

    #[test]
    fn a_chat_whose_asker_has_closed_keeps_the_depth_its_record_holds() {
        // Chat 2 closed. Chat 4's own record says it is two dispatches down, and that is
        // what counts: a chain never looks shallower because a chat above it went away.
        let four = dispatched(2, 2, Mode::Task, Owed::Due, Some("devops"));
        let seen = lineage_of(4, &[(4, &four)], None, &|_| true);
        assert_eq!(seen.depth, 2);
        assert_eq!(seen.chain, Vec::<Option<String>>::new());
        assert_eq!(seen.lineage, 1);
    }

    #[test]
    fn records_that_name_each_other_are_walked_once() {
        // The record is a file anything running as the person can write. Two chats that each
        // say the other dispatched it are a loop no dispatch made, and reading it ends.
        let one = dispatched(2, 1, Mode::Task, Owed::Due, Some("steward"));
        let two = dispatched(1, 1, Mode::Task, Owed::Due, Some("devops"));
        let seen = lineage_of(1, &[(1, &one), (2, &two)], None, &|_| true);
        assert_eq!(seen.chain, vec![Some("devops".to_owned())]);
        assert_eq!(seen.lineage, 2);
    }

    #[test]
    fn a_persona_chat_takes_the_asking_chats_profile_and_folder_and_nothing_else_of_it() {
        // The asking chat holds another persona's grants, ran its last run without the
        // sandbox, draws its footer and is pinned. None of it is the new chat's.
        let asking = Chat {
            cwd: Some("/plane/workspaces/alpha/svc".into()),
            held: Some(HeldGrants {
                persona: Some("qa".to_owned()),
            }),
            unsandboxed: true,
            show_footer: true,
            pinned: true,
            args: vec!["--dangerously-skip-permissions".to_owned()],
            resume: crate::harness::SessionId::new("0b0f2b7e-6f0f-4b6e-9d57-0d0c5a3c2b1a").ok(),
            ..chat(Some("steward"))
        };

        let start = start_for(&asking, Some("devops".to_owned()), "7".to_owned());

        assert_eq!(
            start,
            crate::start::Start {
                profile: Some("work".to_owned()),
                persona: Some("devops".to_owned()),
                name: "7".to_owned(),
                cwd: Some("/plane/workspaces/alpha/svc".into()),
                resume: None,
                show_footer: false,
                resuming: None,
                without_sandbox: None,
                held: None,
                grants: crate::sandbox::grant::Grants::default(),
            }
        );
    }

    const DEVOPS_AND_QA: &str = "[sandbox]\nmode = \"on\"\negress = []\n\
        [sandbox.personas.devops]\nhosts = [\"10.100.39.145:6443\"]\n\
        [sandbox.personas.qa]\nhosts = [\"qa.example\"]\n";

    /// The hosts the sandbox compiled for `start` reaches, in a project whose file is `text`.
    fn reached(text: &str, start: &crate::start::Start) -> Vec<String> {
        use crate::sandbox::{Compiled, Machine, Os, Plane};
        let plane = Plane::of(Some(text));
        let policy = plane.said().policy.expect("the sandbox is on");
        let root = tempfile::tempdir().expect("a project");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(std::path::PathBuf::from("/home/op")),
            os: Os::MacOs,
        };
        // The persona a start compiles its grants for, exactly as `start::ready` picks it.
        let persona =
            crate::start::grants_persona(start.held.as_ref(), start.persona.as_deref(), || None);
        Compiled::granted(
            &policy,
            &plane,
            root.path(),
            &machine,
            persona.as_deref(),
            &start.grants,
        )
        .hosts
    }

    #[test]
    fn the_persona_chats_compiled_sandbox_is_the_projects_for_its_persona_and_not_the_askers() {
        // The asking chat runs as qa, and the person let it reach one more host from a block's
        // Notice. What a chat it dispatches reaches is the project's answer for that chat's
        // own persona: not qa's host, and not the one the person allowed the asking chat.
        let asking = chat(Some("qa"));
        let mut askers_own = crate::start::Start {
            persona: Some("qa".to_owned()),
            ..Default::default()
        };
        askers_own
            .grants
            .hosts
            .push(crate::sandbox::hosts::Host::parse("allowed-once.example").expect("a host"));
        assert_eq!(
            reached(DEVOPS_AND_QA, &askers_own),
            ["allowed-once.example", "qa.example"],
            "what the asking chat itself reaches"
        );

        let same = start_for(&asking, Some("qa".to_owned()), "7".to_owned());
        assert_eq!(reached(DEVOPS_AND_QA, &same), ["qa.example"]);

        let other = start_for(&asking, Some("devops".to_owned()), "8".to_owned());
        assert_eq!(reached(DEVOPS_AND_QA, &other), ["10.100.39.145:6443"]);
    }

    #[test]
    fn a_chat_that_opted_out_of_the_sandbox_dispatches_a_chat_that_did_not() {
        // The opt-out is the picker's, for one chat. The start a dispatch makes carries none,
        // so the sandbox decision is the project's: sandboxed, or not started.
        let asking = Chat {
            unsandboxed: true,
            ..chat(Some("qa"))
        };
        let start = start_for(&asking, Some("qa".to_owned()), "7".to_owned());
        assert_eq!(start.without_sandbox, None);
        // And a chat holding another persona's grants passes no hold on either.
        let held = Chat {
            held: Some(HeldGrants { persona: None }),
            ..chat(Some("devops"))
        };
        assert_eq!(
            start_for(&held, Some("devops".to_owned()), "7".to_owned()).held,
            None
        );
    }

    #[test]
    fn a_persona_is_read_from_the_projects_definition() {
        let root = tempfile::tempdir().expect("a project");
        let personas = root.path().join("personas");
        std::fs::create_dir_all(personas.join("devops")).unwrap();
        std::fs::write(
            personas.join("devops/persona.md"),
            "---\nname: devops\ndescription: runs the cluster\n---\n# devops\n",
        )
        .unwrap();
        std::fs::create_dir_all(personas.join("intern")).unwrap();
        std::fs::write(
            personas.join("intern/persona.md"),
            "---\nname: intern\ndescription: learning\ndraft: true\n---\n# intern\n",
        )
        .unwrap();

        assert_eq!(
            persona_in(root.path(), "devops"),
            Persona::Defined("devops")
        );
        assert_eq!(persona_in(root.path(), "intern"), Persona::Draft("intern"));
        assert_eq!(
            persona_in(root.path(), "nobody"),
            Persona::Unknown("nobody")
        );
        // A name that cannot be a persona's is no persona, wherever it would point.
        assert_eq!(
            persona_in(root.path(), "../devops"),
            Persona::Unknown("../devops")
        );
    }

    // ----- the decision ---------------------------------------------------------------------

    fn steward() -> AskingChat<'static> {
        AskingChat {
            persona: Some("steward"),
            held: false,
            profile: Some("work"),
            depth: 0,
            chain: &[],
            running: 0,
            lineage: 1,
        }
    }

    fn task(asker: Asker<'static>, to: Persona<'static>) -> Request<'static> {
        Request {
            asker,
            to,
            mode: Mode::Task,
            grant: Grant::Missing,
            limits: Limits::default(),
        }
    }

    fn refused(request: &Request<'_>) -> Refused {
        match decide(request) {
            Decision::Refused(why) => why,
            other => panic!("refused, not {other:?}"),
        }
    }

    #[test]
    fn a_chat_dispatches_to_its_own_persona_with_no_grant() {
        let request = task(Asker::Chat(steward()), Persona::Defined("steward"));
        assert_eq!(decide(&request), Decision::Start);
    }

    #[test]
    fn a_chat_on_no_persona_dispatches_to_no_persona_with_no_grant() {
        let plain = AskingChat {
            persona: None,
            ..steward()
        };
        assert_eq!(
            decide(&task(Asker::Chat(plain), Persona::None)),
            Decision::Start
        );
    }

    #[test]
    fn another_persona_needs_a_grant_and_starts_with_one() {
        let request = task(Asker::Chat(steward()), Persona::Defined("devops"));
        assert_eq!(
            decide(&request),
            Decision::NeedsGrant {
                from: Some("steward".to_owned()),
                to: "devops".to_owned(),
            }
        );
        let granted = Request {
            grant: Grant::InForce,
            ..request
        };
        assert_eq!(decide(&granted), Decision::Start);
    }

    #[test]
    fn a_chat_on_no_persona_needs_a_grant_for_any_persona() {
        let plain = AskingChat {
            persona: None,
            ..steward()
        };
        assert_eq!(
            decide(&task(Asker::Chat(plain), Persona::Defined("devops"))),
            Decision::NeedsGrant {
                from: None,
                to: "devops".to_owned(),
            }
        );
    }

    #[test]
    fn a_chat_holding_another_personas_grants_is_refused_and_told_to_have_its_own_allowed() {
        // #1362, D-1436-19: a chat handed off to `devops` from `qa` runs with qa's grants until
        // the person allows its own. A dispatch to "its own" persona would start a chat with
        // devops's hosts and vaults, which is the climb the hold exists to stop. It is refused
        // outright, never answered "needs a grant": a grant named devops to devops would read
        // as harmless and lift the hold for every chat like it.
        let held = AskingChat {
            persona: Some("devops"),
            held: true,
            ..steward()
        };
        for to in [
            Persona::Defined("devops"),
            Persona::Defined("qa"),
            Persona::None,
        ] {
            let said = refused(&task(Asker::Chat(held), to));
            assert_eq!(said, Refused::Held, "{to:?}");
            assert!(
                said.say().contains("allow this chat its own grants"),
                "{}",
                said.say()
            );
        }
        // Whatever grant is in force for the pair its name suggests.
        let granted = Request {
            grant: Grant::InForce,
            ..task(Asker::Chat(held), Persona::Defined("devops"))
        };
        assert_eq!(refused(&granted), Refused::Held);
        // A held chat on no persona is refused the same: nothing it holds is its own.
        let plain = AskingChat {
            persona: None,
            held: true,
            ..steward()
        };
        assert_eq!(
            refused(&task(Asker::Chat(plain), Persona::None)),
            Refused::Held
        );
    }

    #[test]
    fn no_persona_named_is_only_ever_a_chats_own() {
        // The app names the asking chat's own persona when none is named. A request that
        // names none for a chat that runs as one is not one a grant could name.
        assert_eq!(
            refused(&task(Asker::Chat(steward()), Persona::None)),
            Refused::NoPersonaNamed
        );
    }

    #[test]
    fn the_person_dispatches_to_any_persona_with_no_grant() {
        let request = task(Asker::Person(steward()), Persona::Defined("devops"));
        assert_eq!(decide(&request), Decision::Start);
    }

    #[test]
    fn a_helper_sub_agent_is_refused_before_anything_else_is_asked() {
        // Even for a persona that is not there: the first question is who asks.
        let said = refused(&task(Asker::Helper, Persona::Unknown("nobody")));
        assert_eq!(said, Refused::Helper);
        assert!(said.say().contains("Return what you found to your chat"));
    }

    #[test]
    fn a_chat_on_no_profile_is_refused() {
        let shell = AskingChat {
            profile: None,
            ..steward()
        };
        let said = refused(&task(Asker::Chat(shell), Persona::Defined("steward")));
        assert_eq!(said, Refused::NoProfile);
        assert!(
            said.say().contains("started on a profile"),
            "{}",
            said.say()
        );
    }

    #[test]
    fn an_unknown_persona_and_a_draft_are_refused_and_never_ask_for_a_grant() {
        let unknown = refused(&task(Asker::Chat(steward()), Persona::Unknown("nobody")));
        assert_eq!(unknown, Refused::NoPersona("nobody".to_owned()));
        assert!(unknown.say().contains("purlis persona list"));
        let draft = refused(&task(Asker::Chat(steward()), Persona::Draft("intern")));
        assert_eq!(draft, Refused::Draft("intern".to_owned()));
        assert!(draft.say().contains("draft: true"));
        // The person is refused the same: a grant is not what is missing.
        assert_eq!(
            refused(&task(Asker::Person(steward()), Persona::Draft("intern"))),
            Refused::Draft("intern".to_owned())
        );
    }

    #[test]
    fn a_persona_above_the_asking_chat_is_never_dispatched_to_from_below() {
        // steward → devops → steward: the grant is in force and the chain still refuses it.
        let devops = AskingChat {
            persona: Some("devops"),
            depth: 1,
            chain: &[Some("steward")],
            ..steward()
        };
        let request = Request {
            grant: Grant::InForce,
            ..task(Asker::Chat(devops), Persona::Defined("steward"))
        };
        assert_eq!(refused(&request), Refused::Loop("steward".to_owned()));
        // Its own persona is not above it: a chat splits its own work as far as depth allows.
        assert_eq!(
            decide(&task(Asker::Chat(devops), Persona::Defined("devops"))),
            Decision::Start
        );
    }

    #[test]
    fn a_chain_goes_as_deep_as_the_limit_and_no_deeper() {
        let at = |depth| AskingChat { depth, ..steward() };
        let own = Persona::Defined("steward");
        assert_eq!(decide(&task(Asker::Chat(at(2)), own)), Decision::Start);
        assert_eq!(
            refused(&task(Asker::Chat(at(3)), own)),
            Refused::TooDeep { limit: 3 }
        );
    }

    #[test]
    fn no_setting_takes_a_chain_past_the_fixed_ceiling() {
        let generous = Limits {
            depth: 100,
            ..Limits::default()
        };
        let request = Request {
            limits: generous,
            ..task(
                Asker::Chat(AskingChat {
                    depth: DEEPEST,
                    ..steward()
                }),
                Persona::Defined("steward"),
            )
        };
        assert_eq!(refused(&request), Refused::TooDeep { limit: DEEPEST });
    }

    #[test]
    fn an_asking_chat_has_at_most_its_limit_running() {
        let with = |running| AskingChat {
            running,
            ..steward()
        };
        let own = Persona::Defined("steward");
        assert_eq!(decide(&task(Asker::Chat(with(5)), own)), Decision::Start);
        assert_eq!(
            refused(&task(Asker::Chat(with(6)), own)),
            Refused::TooManyRunning { limit: 6 }
        );
        let said = Refused::TooManyRunning { limit: 6 }.say();
        assert!(
            said.contains("6 persona chats that have not reported"),
            "{said}"
        );
        assert!(
            said.contains("Wait for one to report or stop one"),
            "{said}"
        );
    }

    #[test]
    fn a_lineage_holds_at_most_its_limit() {
        let with = |lineage| AskingChat {
            lineage,
            ..steward()
        };
        let own = Persona::Defined("steward");
        assert_eq!(decide(&task(Asker::Chat(with(15)), own)), Decision::Start);
        assert_eq!(
            refused(&task(Asker::Chat(with(16)), own)),
            Refused::LineageFull { limit: 16 }
        );
        let said = Refused::LineageFull { limit: 16 }.say();
        assert!(said.contains("16 chats that have not reported"), "{said}");
        assert!(
            said.contains("Wait for one to report or stop one"),
            "{said}"
        );
    }

    #[test]
    fn a_limit_of_zero_switches_dispatch_off() {
        let off = Limits {
            running: 0,
            ..Limits::default()
        };
        let request = Request {
            limits: off,
            ..task(Asker::Chat(steward()), Persona::Defined("steward"))
        };
        assert_eq!(refused(&request), Refused::TooManyRunning { limit: 0 });
        // And the sentence says so, for each limit: never "already has 0".
        for off in [
            Refused::TooManyRunning { limit: 0 },
            Refused::LineageFull { limit: 0 },
            Refused::TooDeep { limit: 0 },
        ] {
            let said = off.say();
            assert!(said.starts_with("dispatch is switched off here"), "{said}");
            assert!(!said.contains("already"), "{said}");
        }
    }

    #[test]
    fn the_limits_hold_for_the_person_too() {
        let full = AskingChat {
            running: 6,
            ..steward()
        };
        assert_eq!(
            refused(&task(Asker::Person(full), Persona::Defined("devops"))),
            Refused::TooManyRunning { limit: 6 }
        );
    }

    #[test]
    fn a_limit_is_said_before_a_grant_is_asked_for() {
        // Asking the person for a grant that would start nothing wastes their yes.
        let full = AskingChat {
            running: 6,
            ..steward()
        };
        assert_eq!(
            refused(&task(Asker::Chat(full), Persona::Defined("devops"))),
            Refused::TooManyRunning { limit: 6 }
        );
    }

    #[test]
    fn every_refusal_is_one_line_that_says_what_to_do() {
        for refusal in [
            Refused::Helper,
            Refused::NoProfile,
            Refused::Held,
            Refused::NoPersonaNamed,
            Refused::TooDeep { limit: 0 },
            Refused::NoPersona("x".to_owned()),
            Refused::Draft("x".to_owned()),
            Refused::Loop("x".to_owned()),
            Refused::TooDeep { limit: 3 },
            Refused::TooManyRunning { limit: 6 },
            Refused::LineageFull { limit: 16 },
        ] {
            let said = refusal.say();
            assert!(!said.contains('\n'), "{said}");
            assert!(said.matches(". ").count() >= 1, "two sentences: {said}");
        }
    }

    #[test]
    fn a_mode_is_read_back_from_its_word_and_anything_else_is_a_handoff() {
        assert_eq!(Mode::of(Mode::Task.word()), Mode::Task);
        assert_eq!(Mode::of(Mode::Handoff.word()), Mode::Handoff);
        assert_eq!(Mode::of(""), Mode::Handoff);
    }
}
