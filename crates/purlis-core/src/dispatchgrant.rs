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
//!   and is told of when it changes ([`crate::dispatcharrival::arrival`]):
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
//! Notice that says it arrived or on a chat's tab. One made in this window is acknowledged as
//! it is written. So a chat nobody is at never dispatches under a pair nobody here has seen.
//! **And the yes is bound to what it accepted** (#1506, [`crate::dispatcharrival`]): a grant
//! a commit takes out of the project's file is no longer accepted here, so one that is taken
//! out and put back waits for a new yes, whether or not anything was read in between. A grant
//! the file on disk does not hold is not in force, and nothing is dropped for that alone.
//! Where the project's history cannot be asked, no acceptance of its grants counts.
//!
//! # Any persona, and never (#1503)
//!
//! **A grant is one-way**: `steward -> devops` allows nothing from devops to steward. A report
//! back needs none; a new task the other way is its own pair.
//!
//! **Any persona** is a grant with no named target ([`ANY`]): for you on this machine
//! (`app/sandbox.json` `dispatch_any`) or for everyone in the project (`steward = ["*"]`,
//! accepted per machine in `dispatch_any_seen`). [`allow_any`] is the one way it is made, and
//! Settings is the one caller: a Notice's answer keeps a [`Pair`], and a pair's target is a
//! persona's name. It covers a persona added later, since nothing reads the list of personas.
//!
//! **Never for a pair** ([`never`], [`Covers::Never`]) is the person's refusal on this machine,
//! in a file of its own (`app/dispatch-never.json`, [`crate::dispatchnever`]). It is read
//! before any grant, so nothing granted at any level covers the pair, "any persona" and the
//! project's file included, and nothing in the project's file lifts it. [`lift_never`] does,
//! from Settings. **It holds down the chain** ([`covers_in_chain`], [`Covers::NeverAbove`]): a
//! chat with a chat of the refused persona above it does not dispatch to that target either.
//! **The person's own dispatch from a tab is not held to it**: it is their rule for chats.
//!
//! **A record edited by hand fails closed.** A star among the named grants, or as an asking
//! persona, grants nothing; a never is matched as it is spelled and never dropped; a record of
//! grants that does not read grants nothing; and **a record of nevers that does not read is
//! not "no nevers"**: no grant covers any pair of two personas until it reads
//! ([`Covers::Unread`]).
//!
//! # Not on my machine, and a persona that is gone (#1504)
//!
//! **Not on my machine** ([`decline`]) stops this machine following one grant of the
//! project's, a pair or "any persona", without touching the committed file. It takes away
//! this machine's acceptance, which is what puts a project grant in force here, and remembers
//! the grant as declined so it is not told as new again.
//!
//! **A grant is in force only while both personas exist**, and a name that was seen gone and
//! is another persona's now has its grants set aside until the person gives them back
//! ([`crate::dispatchdormant`], which also says what that does not catch). The existence
//! check is the app's, where a dispatch is judged, before [`covers`] is asked. purlis has no
//! persona rename.
//!
//! # In one workspace, or in any (#1505)
//!
//! A grant carries one condition ([`crate::dispatchwithin`], which says how each level
//! spells it): it holds in any workspace, or only for a task that **works in** one named
//! workspace. [`InForce::for_task_in`] says where the task works, and a limited grant covers
//! nothing until it is said: read without it, [`covers`] counts only the grants that hold
//! everywhere. "Any persona" may be limited the same way. A never is not limited.
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

/// **Any persona**, as a target is spelled where a grant covers every persona (#1503): in
/// `[dispatch.grants]` (`steward = ["*"]`), and in the audit. Not a persona's name, so no pair
/// is ever made of it.
pub const ANY: &str = "*";

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
    /// The personas whose chats you let dispatch to any persona, on this machine (#1503).
    pub you_any: Vec<String>,
    /// The personas the project's file lets dispatch to any persona, each accepted on this
    /// machine (#1503).
    pub project_any: Vec<String>,
    /// The pairs you said never to on this machine (#1503), as the record spells them.
    pub never: Vec<(String, String)>,
    /// Whether that record is there and does not read ([`crate::dispatchnever::Nevers::Unread`]):
    /// what you refused is unknown, so no grant here covers any pair ([`Covers::Unread`]).
    pub never_unread: bool,
    /// The grants that hold in one workspace only (#1505), yours and the project's accepted
    /// here, each only while its workspace stands ([`crate::dispatchwithin::in_force`]).
    pub limited: Vec<(Level, crate::dispatchwithin::Limited)>,
    /// The workspace the task being judged works in ([`InForce::for_task_in`]); none for the
    /// project's root, **and none until it is said**, so a limited grant covers nothing for a
    /// reader that never says where the task works.
    pub works_in: Option<String>,
}

impl InForce {
    /// The grants in force in the project at `root` for a chat the app holds `chat` for.
    ///
    /// **A committed pair is in force only once this machine acknowledged it** (D-1437-R1): one
    /// a pull brought in covers nothing here until a person allows it on the one-time Notice,
    /// or on a chat's tab. A grant made in this window is acknowledged as it is written.
    ///
    /// **And only while the last settling of this machine's acceptances answered**
    /// ([`crate::dispatcharrival::for_read`]): where the project's history could not be
    /// asked, or the record could not be written, none of the project's grants is in force.
    /// **This runs no git and writes nothing**: it goes by the verdict of the last settling,
    /// which the app runs before a dispatch is decided, and it is safe under the lock a
    /// decision is made under. Only the first read of a project in a process settles.
    pub fn read(root: &Path, chat: Vec<ChatPair>) -> Self {
        let settled = crate::dispatcharrival::for_read(root).read;
        let seen = crate::sandbox::local::dispatch_seen(root)
            .filter(|_| settled)
            .unwrap_or_default();
        let (never, never_unread) = match crate::dispatchnever::read(root) {
            crate::dispatchnever::Nevers::Read(pairs) => (pairs, false),
            crate::dispatchnever::Nevers::Unread => (Vec::new(), true),
        };
        Self {
            chat,
            you: yours(root),
            project: committed_at(root)
                .into_iter()
                .filter(|pair| seen.contains(&pair.to_string()))
                .collect(),
            you_any: any_yours(root),
            project_any: if settled {
                any_of_the_project(root)
            } else {
                Vec::new()
            },
            never,
            never_unread,
            limited: crate::dispatchwithin::in_force(root),
            works_in: None,
        }
    }

    /// These grants, for a task that works in `workspace` (`None`: at the project's root):
    /// the workspace the task works in, never the one the asking chat is in
    /// ([`crate::dispatchwithin::works_in`]). A grant limited to one workspace counts from
    /// here on for a task in that workspace, and for no other.
    #[must_use]
    pub fn for_task_in(mut self, workspace: Option<&str>) -> Self {
        self.works_in = workspace.map(str::to_owned);
        self
    }

    /// The widest standing level at which a grant limited to `workspace` covers `asking` to
    /// `target`: one that names the pair, or with `any` one for any persona.
    fn limited_level(
        &self,
        asking: Option<&str>,
        target: &str,
        workspace: Option<&str>,
        any: bool,
    ) -> Option<Level> {
        let (asking, workspace) = (asking?, workspace?);
        let wanted = if any { ANY } else { target };
        let at = |level: Level| {
            self.limited.iter().any(|(held, one)| {
                *held == level
                    && one.asking == asking
                    && one.target == wanted
                    && one.workspace == workspace
            })
        };
        if at(Level::Project) {
            Some(Level::Project)
        } else if at(Level::You) {
            Some(Level::You)
        } else {
            None
        }
    }

    /// Whether you said never to `asking` dispatching to `target`.
    pub fn refuses(&self, asking: Option<&str>, target: &str) -> bool {
        asking.is_some_and(|asking| {
            self.never
                .iter()
                .any(|(from, to)| from == asking && to == target)
        })
    }

    /// The persona above the asking chat that you said never to dispatching to `target`, where
    /// there is one: the nearest in `chain`, the personas of the chats above it in its lineage
    /// ([`crate::dispatchlimits::Lineage::chain`]).
    pub fn refuses_above<'a>(&self, chain: &'a [Option<String>], target: &str) -> Option<&'a str> {
        chain
            .iter()
            .flatten()
            .map(String::as_str)
            .find(|above| self.refuses(Some(above), target))
    }

    /// The widest **standing** level at which a grant **names** the pair `asking` to
    /// `target`: the project's or yours. Never "any persona", and never one chat's.
    pub fn named_level_of(&self, asking: Option<&str>, target: &str) -> Option<Level> {
        self.named_level_in(asking, target, self.works_in.as_deref())
    }

    /// [`InForce::named_level_of`], for a task that works in `workspace` whatever
    /// [`InForce::works_in`] says: a grant that names the pair and holds in any workspace, or
    /// one that names the pair and is limited to that workspace.
    pub fn named_level_in(
        &self,
        asking: Option<&str>,
        target: &str,
        workspace: Option<&str>,
    ) -> Option<Level> {
        let limited = self.limited_level(asking, target, workspace, false);
        let named = |pairs: &[Pair]| {
            asking.is_some_and(|asking| {
                pairs
                    .iter()
                    .any(|pair| pair.asking == asking && pair.target == target)
            })
        };
        if named(&self.project) || limited == Some(Level::Project) {
            Some(Level::Project)
        } else if named(&self.you) || limited == Some(Level::You) {
            Some(Level::You)
        } else {
            None
        }
    }

    /// The widest level a grant of `asking` to `target` is held at, if any is: a pair named,
    /// or "any persona".
    pub fn level_of(&self, asking: Option<&str>, target: &str) -> Option<Level> {
        // "Any persona" covers whichever persona the target is, one added after it was granted
        // included: nothing here reads the list of personas. It covers nothing that is not a
        // persona's name, as no named pair could.
        let any = |personas: &[String]| {
            crate::personas::valid_name(target)
                && asking.is_some_and(|asking| personas.iter().any(|one| one == asking))
        };
        let named = self.named_level_of(asking, target);
        // "Any persona", limited to the workspace the task works in.
        let any_here = if crate::personas::valid_name(target) {
            self.limited_level(asking, target, self.works_in.as_deref(), true)
        } else {
            None
        };
        if named == Some(Level::Project)
            || any(&self.project_any)
            || any_here == Some(Level::Project)
        {
            Some(Level::Project)
        } else if named == Some(Level::You) || any(&self.you_any) || any_here == Some(Level::You) {
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
    /// The person said never to the pair on this machine (#1503): nothing starts, nobody is
    /// asked, and no grant covers it, an "any persona" one and the project's included.
    Never,
    /// The person said never to the persona named here, a chat above the asking one in its
    /// chain, dispatching to this target ([`covers_in_chain`]): work that persona asked for
    /// does not reach the target through another. Nothing starts and nobody is asked.
    NeverAbove(String),
    /// This machine's record of nevers is there and does not read
    /// ([`InForce::never_unread`]): no grant covers the pair, since it may be one the person
    /// refused. The person is asked, and told why; a chat nobody is at is refused.
    Unread,
}

/// **[`covers`], for a chat with `chain` above it**: the personas of the chats above the
/// asking chat in its lineage, nearest first, which is what the loop rule reads.
///
/// A never for `above` to `target` also refuses a dispatch to `target` from any chat with a
/// chat running as `above` over it ([`Covers::NeverAbove`]): the person who said that persona
/// never dispatches there is not shown its work arriving through a third. A policy lock and
/// the asking chat's own never are said first.
pub fn covers_in_chain(
    asking: Option<&str>,
    target: &str,
    grants: &InForce,
    policy: &Locks,
    chain: &[Option<String>],
) -> Covers {
    match covers(asking, target, grants, policy) {
        said @ (Covers::Locked(_) | Covers::Never) => said,
        other => match grants.refuses_above(chain, target) {
            Some(above) => Covers::NeverAbove(above.to_owned()),
            None => other,
        },
    }
}

/// **Whether a chat running as `asking` may dispatch to `target`** under `grants` and `policy`.
///
/// In order: a policy that locks all dispatch, which holds a chat's own persona too; a policy
/// lock on the pair, a persona's own included where the policy names it twice; the person's
/// never for the pair, which no grant lifts; the same persona, which needs no grant; a record
/// of nevers that does not read, under which no grant counts; then a grant at any level, a
/// named pair or "any persona".
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
    // Before any grant is read, so nothing granted anywhere answers for a pair the person
    // refused; and before the same-persona answer, so a never written by hand is held as
    // written.
    if grants.refuses(asking, target) {
        return Covers::Never;
    }
    if asking == Some(target) {
        return Covers::Covered;
    }
    // What the person refused is unknown, so no grant answers for a pair of two personas: not
    // a standing one, and not one made for this chat.
    if grants.never_unread {
        return Covers::Unread;
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
    /// Each asking persona it lets dispatch to any persona (`"*"` in its list), once (#1503).
    pub any: Vec<String>,
    /// Each grant it limits to one workspace (#1505), a pair or any persona, once: written
    /// `{ to = "devops", in = "runners" }` in the asking persona's list
    /// ([`crate::dispatchwithin::Limited::from_entry`]). Never in `pairs` or `any`.
    pub limited: Vec<crate::dispatchwithin::Limited>,
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
            // A grant for one workspace is a table, never a string: a build that knows only
            // names reads it as nothing.
            if let Some(entry) = target.as_table() {
                match crate::dispatchwithin::Limited::from_entry(asking, entry) {
                    Ok(one) => {
                        if !out.limited.contains(&one) {
                            out.limited.push(one);
                        }
                    }
                    Err(why) => out.refused.push(why),
                }
                continue;
            }
            if target.as_str() == Some(ANY) && crate::personas::valid_name(asking) {
                if !out.any.contains(asking) {
                    out.any.push(asking.clone());
                }
                continue;
            }
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

// ---- never for a pair, and any persona (#1503) ------------------------------------------------

/// The pairs you said never to in the project at `root`, on this machine, as the record spells
/// them; none where the record does not read ([`nevers_unread`] says so). **Matched as
/// written**: one whose names are no persona's refuses nothing real, and is never read as
/// anything wider.
pub fn nevers(root: &Path) -> Vec<(String, String)> {
    match crate::dispatchnever::read(root) {
        crate::dispatchnever::Nevers::Read(pairs) => pairs,
        crate::dispatchnever::Nevers::Unread => Vec::new(),
    }
}

/// Why no dispatch grant counts in the project at `root` just now, for the person: the record
/// of nevers is there and does not read. `None` where it reads, or there is none.
pub fn nevers_unread(root: &Path) -> Option<String> {
    (crate::dispatchnever::read(root) == crate::dispatchnever::Nevers::Unread)
        .then(|| crate::dispatchnever::unread_said(root))
}

/// **Records the person's never for `pair`** on this machine: the grant Notice's "Never for
/// this pair". Kept in `app/dispatch-never.json` ([`crate::dispatchnever`]), never the
/// project's file, so it is yours alone and nothing a teammate commits lifts it. Refused,
/// with nothing written, where that record does not read.
pub fn never(root: &Path, pair: &Pair) -> std::io::Result<()> {
    crate::dispatchnever::add(root, &pair.asking, &pair.target)
}

/// **Lifts the never for `asking` to `target`**: Settings' own action. Answers whether there
/// was one. The names are taken as the record spells them, so one written by hand can be
/// lifted too. What then covers the pair is whatever grant stands; with none, the next
/// dispatch asks. Refused, with nothing written, where the record does not read.
pub fn lift_never(root: &Path, asking: &str, target: &str) -> std::io::Result<bool> {
    crate::dispatchnever::lift(root, asking, target)
}

/// What a chat is told when a persona above it in its chain is one the person said never
/// dispatches to `target`.
pub fn never_above_said(above: &str, target: &str) -> String {
    let (above, target) = (crate::shown::short(above), crate::shown::short(target));
    format!(
        "the person said never to {above} chats dispatching to {target} on this machine, and \
         this chat works for a {above} chat: one is above it in its chain. So nothing was \
         started and they were not asked. Do not dispatch to {target} for this work. Do it \
         without {target}, or say in your report that it is waiting: only the person lifts \
         it, in {SETTINGS}."
    )
}

/// What a chat nobody is at is told while the record of nevers does not read.
pub const NEVERS_UNREAD: &str = "purlis could not read the list of pairs the person said never \
     to on this machine, so no dispatch grant counts until it reads, and nobody is here to \
     ask. Nothing was started. Do this work without another persona, or say in what you leave \
     behind that it is waiting; a person mends the list on this machine.";

/// What a chat is told when it asks across a pair the person said never to.
pub fn never_said(asking: &str, target: &str) -> String {
    let (asking, target) = (crate::shown::short(asking), crate::shown::short(target));
    format!(
        "the person said never to {asking} chats dispatching to {target} on this machine, so \
         nothing was started and they were not asked. Do not dispatch to {target} again. Do \
         this work without {target}, or tell the person it is waiting: only they lift it, in \
         {SETTINGS}."
    )
}

/// Where the person sees, takes back and lifts what they said of dispatch.
pub const SETTINGS: &str = crate::dispatchunattended::SETTINGS;

/// The personas whose chats you let dispatch to any persona in the project at `root`, on this
/// machine. A name that is no persona's grants nothing: `"*"` there is not "everyone".
pub fn any_yours(root: &Path) -> Vec<String> {
    crate::sandbox::local::granted_dispatch_any(root)
        .into_iter()
        .filter(|asking| crate::personas::valid_name(asking))
        .collect()
}

/// The personas the project's file at `root` lets dispatch to any persona, accepted here or
/// not.
pub fn any_committed_at(root: &Path) -> Vec<String> {
    let text = crate::sandbox::read_plane_file(&crate::names::manifest(root))
        .ok()
        .flatten();
    committed(text.as_deref()).any
}

/// The personas the project's file lets dispatch to any persona **and** you accepted on this
/// machine: the only ones of the file's in force here, as a pulled pair waits for a yes
/// (D-1437-R1). The acceptance is its own record, which nothing that accepts a pair writes.
pub fn any_of_the_project(root: &Path) -> Vec<String> {
    let accepted = crate::sandbox::local::dispatch_any_seen(root);
    any_committed_at(root)
        .into_iter()
        .filter(|asking| accepted.contains(asking))
        .collect()
}

/// The project's "any persona" grants nobody on this machine has accepted: in the file, in
/// force for no chat here.
pub fn any_unaccepted(root: &Path) -> Vec<String> {
    let accepted = crate::sandbox::local::dispatch_any_seen(root);
    any_committed_at(root)
        .into_iter()
        .filter(|asking| !accepted.contains(asking))
        .collect()
}

/// Why "any persona" is not granted for `asking` at `level`, before anything is written or
/// audited; `Ok` where [`allow_any`] would keep it.
pub fn can_allow_any(root: &Path, asking: &str, level: Level) -> Result<(), String> {
    if !crate::personas::valid_name(asking) {
        return Err(format!(
            "{} is not a persona's name, so purlis keeps no dispatch grant for it.",
            crate::shown::short(asking)
        ));
    }
    match level {
        Level::Chat => Err(
            "Any persona is granted for you on this machine or for everyone in this project, \
             never for one chat."
                .to_owned(),
        ),
        Level::You => Ok(()),
        Level::Project => crate::settings::dispatch::can_grant_any(root, asking),
    }
}

/// **Lets chats running as `asking` dispatch to any persona**, for you on this machine
/// ([`Level::You`]) or for everyone in the project ([`Level::Project`]): **Settings' explicit
/// grant and nothing else's**. No answer to a grant Notice calls this, and no line a chat
/// sends reaches it. It covers a persona added later; a never for a pair, a policy lock, the
/// loop rule and the rule for a chat nobody is at all hold as they did.
pub fn allow_any(root: &Path, asking: &str, level: Level) -> Result<(), String> {
    can_allow_any(root, asking, level)?;
    match level {
        Level::Project => crate::settings::dispatch::grant_any(root, asking),
        _ => crate::sandbox::local::grant_dispatch_any(root, asking)
            .map_err(|why| format!("purlis could not keep the grant: {why}")),
    }
}

/// **Takes back `asking`'s grant of any persona at `level`**: Settings' Revoke. Answers
/// whether there was one. A pair granted by name stands as it did.
pub fn revoke_any(root: &Path, asking: &str, level: Level) -> Result<bool, String> {
    match level {
        Level::Chat => Ok(false),
        Level::You => crate::sandbox::local::revoke_dispatch_any(root, asking)
            .map_err(|why| format!("purlis could not revoke it: {why}")),
        Level::Project => {
            let was = any_committed_at(root).iter().any(|one| one == asking);
            crate::settings::dispatch::revoke_any(root, asking)?;
            Ok(was)
        }
    }
}

// ---- not on my machine (#1504) ----------------------------------------------------------------

/// The project's grants you said "Not on my machine" to in the project at `root`, each as
/// [`Pair`] is displayed, or `<asking> -> *` for "any persona".
pub fn declined(root: &Path) -> Vec<String> {
    crate::sandbox::local::dispatch_declined(root)
}

/// **Accepts the project's "any persona" for `asking` on this machine**: Settings' Accept.
/// It records the acceptance and nothing else, and never writes the committed file: refused
/// where the file does not hold the grant now.
pub fn accept_any_of_the_project(root: &Path, asking: &str) -> Result<(), String> {
    if !crate::personas::valid_name(asking) || !project_grants(root, asking, ANY) {
        return Err("purlis changed nothing: the project no longer has that grant.".to_owned());
    }
    acknowledge_any(root, asking)
        .map_err(|why| format!("purlis could not record it as allowed on this machine: {why}"))
}

/// A project grant as [`declined`] spells it: `target` is a persona's name or [`ANY`]. `None`
/// for names no project grant could have.
pub fn project_grant_said(asking: &str, target: &str) -> Option<String> {
    if target == ANY {
        crate::personas::valid_name(asking).then(|| format!("{asking} -> {ANY}"))
    } else {
        Pair::new(asking, target).ok().map(|pair| pair.to_string())
    }
}

/// Whether the project's file at `root` grants `asking` to `target` now, [`ANY`] included.
pub fn project_grants(root: &Path, asking: &str, target: &str) -> bool {
    if target == ANY {
        any_committed_at(root).iter().any(|one| one == asking)
    } else {
        committed_at(root)
            .iter()
            .any(|pair| pair.asking == asking && pair.target == target)
    }
}

/// **Not on my machine**, for the project's grant of `asking` to `target` (a persona's name,
/// or [`ANY`]): Settings' own action. The committed file is not touched, so teammates keep it.
/// On this machine it covers nothing from now on, accepted before or not, and is not told as
/// new again; **Accept** in Settings is how it comes back. Refused where the file does not
/// hold the grant.
pub fn decline(root: &Path, asking: &str, target: &str) -> Result<(), String> {
    let said = project_grant_said(asking, target)
        .filter(|_| project_grants(root, asking, target))
        .ok_or_else(|| {
            "purlis changed nothing: the project no longer has that grant.".to_owned()
        })?;
    crate::sandbox::local::decline_dispatch(root, &said)
        .map_err(|why| format!("purlis could not record it: {why}"))
}

/// Records that the person allowed `shown`, pairs of the project's as [`Pair`] is displayed:
/// each of them the file holds is in force here from now on ([`InForce::read`]), beside what
/// was accepted before. One the file does not hold is nothing.
pub fn acknowledge(root: &Path, shown: &[String]) -> std::io::Result<()> {
    for pair in unacknowledged(root) {
        if shown.contains(&pair.to_string()) {
            acknowledge_pair(root, &pair)?;
        }
    }
    Ok(())
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

/// What an acceptance is refused with while the project's history cannot be asked.
pub const HISTORY_UNREAD: &str = "purlis could not read this project's git history just now, so \
     it cannot tell what its grants were when you last accepted them. Nothing was accepted. Try \
     again in a moment.";

/// What an acceptance is refused with where the project's file does not hold the grant.
pub const NO_SUCH_GRANT: &str = "the project no longer has that grant";

/// Records that the person accepted the project's grant `said` on this machine.
///
/// **Bound to the project's history as it is written** ([`crate::dispatcharrival::settle`]):
/// what was accepted before is settled first, so the commit kept beside the acceptances is
/// one all of them were checked through; refused where that history cannot be asked. **One
/// write, which checks the project's file holds the grant** and adds to the record without
/// writing it back whole, so nothing dropped meanwhile comes back.
fn acknowledge_said(root: &Path, said: &str, holds: &dyn Fn() -> bool) -> std::io::Result<()> {
    if !crate::dispatcharrival::settle_afresh(root).read {
        return Err(std::io::Error::other(HISTORY_UNREAD));
    }
    if !crate::sandbox::local::accept_dispatch(root, said, holds)? {
        return Err(std::io::Error::other(NO_SUCH_GRANT));
    }
    // Binds a first acceptance to the commit checked out now.
    crate::dispatcharrival::settle_afresh(root);
    Ok(())
}

/// Records that the person allowed the project's `pair` on this machine, beside what was
/// acknowledged before ([`acknowledge_said`]).
pub fn acknowledge_pair(root: &Path, pair: &Pair) -> std::io::Result<()> {
    acknowledge_said(root, &pair.to_string(), &|| {
        committed_at(root).contains(pair)
    })
}

/// Records that the person accepted the project's "any persona" grant for `asking` on this
/// machine, as [`acknowledge_pair`] records a pair. **Acknowledge only**: it never writes the
/// project's file.
pub fn acknowledge_any(root: &Path, asking: &str) -> std::io::Result<()> {
    acknowledge_said(root, &format!("{asking} -> {ANY}"), &|| {
        crate::personas::valid_name(asking)
            && any_committed_at(root).iter().any(|one| one == asking)
    })
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

/// **A dispatch grant or revoke, or a never and its lifting, as the audit records it**
/// (`trust.dispatch.grant`, `trust.dispatch.revoke`, `trust.dispatch.never`,
/// `trust.dispatch.never.lift`, `trust.dispatch.decline`; ADR 0075 §4): who (the person, by scope, never a login), the level
/// and the pair. Which chat it came from, when and on which machine are the envelope's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Audited<'a> {
    /// What the person did.
    pub act: Act,
    /// The asking persona; none for a chat on no persona.
    pub asking: Option<&'a str>,
    pub target: &'a str,
    pub level: Level,
    /// The workspace the grant is limited to (#1505); none for one that holds in any
    /// workspace, and for a never, which is not limited. For a grant made for one chat, the
    /// workspace its task works in, none at the project's root.
    pub workspace: Option<&'a str>,
}

/// What a person did to a dispatch grant, as [`Audited`] records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// A grant made: a pair, or any persona where the target is [`ANY`].
    Grant,
    /// A grant taken back.
    Revoke,
    /// "Never for this pair" (#1503): always at [`Level::You`].
    Never,
    /// A never lifted.
    LiftNever,
    /// "Not on my machine" (#1504): the project's grant is no longer followed on this
    /// machine. Always at [`Level::Project`], which is whose grant it is; the project's file
    /// is not changed.
    Decline,
    /// A grant purlis took out of force itself (#1504): a name in it was seen gone and is
    /// another persona's now ([`crate::dispatchdormant`]). Recorded as a revoke whose actor
    /// is the host, never a person.
    SetAside,
    /// The person's one acknowledgement that the persona of a name today may have what an
    /// earlier persona of the name had (#1504): `asking` and `target` are both that name.
    /// Each grant it puts back in force is a [`Act::Grant`] after it.
    GiveBack,
    /// **One dispatch allowed, and no grant kept** (#1505): the person's Allow on a question
    /// about a task whose workspace is not there yet. `workspace` is that name, and `level`
    /// the answer they pressed, which kept nothing.
    Once,
}

impl Audited<'_> {
    /// The event's kind.
    pub fn kind(&self) -> &'static str {
        match self.act {
            Act::Grant => "trust.dispatch.grant",
            Act::Revoke => "trust.dispatch.revoke",
            Act::Never => "trust.dispatch.never",
            Act::LiftNever => "trust.dispatch.never.lift",
            Act::Decline => "trust.dispatch.decline",
            Act::SetAside => "trust.dispatch.revoke",
            Act::GiveBack => "trust.dispatch.give_back",
            Act::Once => "trust.dispatch.once",
        }
    }

    /// The event's body.
    pub fn body(&self) -> serde_json::Value {
        // Who did it: the person, in the window; or, for a grant set aside, purlis itself.
        let (actor_kind, actor, scope) = match self.act {
            Act::SetAside => ("host", "purlis", "persona-changed-hands"),
            _ => ("human", "operator", "local-ui"),
        };
        serde_json::json!({
            "actor_kind": actor_kind,
            "actor": actor,
            "scope": scope,
            "level": self.level.word(),
            "asking": self.asking,
            "target": self.target,
            "workspace": self.workspace,
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

#[cfg(test)]
#[path = "dispatchgrant_never_tests.rs"]
mod never_tests;

#[cfg(test)]
#[path = "dispatchgrant_declined_tests.rs"]
mod declined_tests;
