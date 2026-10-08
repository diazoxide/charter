//! **A dispatch from an unattended chat** (#1446, spec #1434; decision 20 of the 2026-10-07
//! grill): a chat running with its harness's permission prompts off may dispatch only under a
//! grant that already stands, and what it starts never runs the way it does.
//!
//! # What "unattended" is
//!
//! The harness's own word for it: a hook payload whose `permission_mode` is
//! [`crate::floorguard::UNATTENDED_MODE`], which is what [`crate::floorguard::unattended`]
//! reads for the release floor. (The handoff guard read it too, until a handoff became a
//! dispatch and the app's own mark took its place, #1444.) purlis keeps no other mark: it starts no
//! chat that way itself, and a profile's command or a switch in the pane is the person's. The
//! app keeps what a chat's harness reported as a [`Mark`], which only ever goes one way.
//!
//! # The one question, for an unattended chat
//!
//! [`covers`] is [`crate::dispatchgrant::covers`] with two differences:
//!
//! - **a grant made for one chat never counts.** Only one made for the person on this machine,
//!   or for everyone in the project, covers the pair. Both were made by a person, at a time
//!   they were there, for every chat of that persona.
//! - **a missing grant is a refusal, never a Notice** ([`Answer::Refused`]). Nobody is there to
//!   press one, and a Notice left waiting is a grant offered to whoever comes back to the
//!   window. So nothing is held, nothing is raised, and no grant can be made from the ask.
//!
//! A chat's own persona needs no grant, and a policy lock refuses, as for any chat.
//!
//! **A refusal for lack of a grant is kept for the person to read afterwards** (#1507,
//! [`crate::dispatchaway`]): an item in the needs-you list, attached to no chat, from which
//! they may make the standing grant for that pair. It is no Notice on the chat and holds no
//! dispatch: the answer here is still a refusal, given at once.
//!
//! # Only from inside the sandbox
//!
//! A standing grant is a file, and what keeps a chat from writing it is the sandbox. A chat
//! with its prompts off and no sandbox has neither a person nor a wall between it and who it
//! may dispatch to, so it dispatches to no other persona at all ([`Refusal::Unsandboxed`]),
//! whatever the grants say. Its own persona is still its own. Whether a chat is sandboxed is
//! the app's record of how it started the chat, never a line the chat sends.
//!
//! # What it starts
//!
//! [`start_of_a_persona_chat`]: the persona chat is a chat like any other, started on a
//! profile as the project and this machine declare it. Nothing of how the asking chat runs
//! goes with it: not a grant it holds, not its opt-out, not its conversation, and not its
//! permission mode. A [`crate::start::Start`] has no field for a mode, so the one way a bypass
//! could travel is the asking chat's profile; a persona chat is not started on a profile that
//! asks nobody, taken from the asking chat or named by anyone ([`Inherited::asks_nobody`]). The
//! persona chat asks what it asks in its own tab, and waits there, shown as needing the person,
//! like any chat that asks.
//!
//! # A profile that asks (#1522)
//!
//! **A chat one chat starts for another starts only on a profile known to ask a person before
//! its harness acts** ([`crate::profiles::ASKS`]): a built-in whose harness asks by its own
//! default (Claude Code, Codex; not opencode, which allows every action unless configured
//! otherwise), or a profile of the local file the person marked `asks = true`. Anything
//! unmarked is taken not to ask, so the rule does not depend on knowing every way each harness
//! is told to ask nobody: a flag, a configuration override, an environment variable, a settings
//! file, a wrapper script. The flags purlis does know ([`bypass_in`]) still refuse a profile
//! marked as asking, whose mark its own command contradicts. There is no exception for a
//! session the person started that way: the standing rule is that such a profile never runs
//! a task.

use crate::dispatchgrant::{self, Covers, InForce};
use crate::sandbox::policy::Locks;
use crate::start::Start;

/// How the asking chat runs, as the app holds it: never as a dispatch says.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Attendance {
    /// A person answers its prompts.
    #[default]
    Attended,
    /// Its harness's permission prompts are off: nobody answers.
    Unattended,
}

/// **Whether a chat's harness ever reported running with its prompts off**: the app's mark on
/// one chat, for that chat's life.
///
/// It goes one way. Each hook report is the harness's, relayed by a process the chat's own
/// commands run beside, so a later report that says otherwise is not evidence that anything
/// changed: a chat that was once unattended stays so until it is started again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mark(bool);

impl Mark {
    /// Hears one hook report's `permission_mode`.
    pub fn heard(&mut self, permission_mode: Option<&str>) {
        self.0 |= crate::floorguard::unattended(permission_mode);
    }

    /// How the chat is taken to run.
    pub fn attendance(self) -> Attendance {
        if self.0 {
            Attendance::Unattended
        } else {
            Attendance::Attended
        }
    }
}

/// What [`covers`] answers an unattended chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The dispatch may start: the same persona, or a grant that stands.
    Covered,
    /// An administrator's policy locks it: the policy's sentence, naming who set it.
    Locked(String),
    /// Nothing starts, nothing is held and no Notice is raised.
    Refused(Refusal),
}

/// Why an unattended chat's dispatch is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The chat was started without a sandbox, and asked for another persona: the target.
    Unsandboxed(String),
    /// No grant that counts covers the pair.
    Missing(Missing),
    /// The person said never to the pair on this machine (#1503): the asking persona and the
    /// target.
    Never(String, String),
    /// The person said never to a persona above this chat in its chain dispatching to the
    /// target: that persona, and the target.
    NeverAbove(String, String),
    /// This machine's record of nevers does not read, so no grant counts.
    NeversUnread,
}

impl Refusal {
    /// The sentence the asking chat reads.
    pub fn say(&self) -> String {
        match self {
            Self::Missing(missing) => missing.say(),
            Self::Never(asking, target) => dispatchgrant::never_said(asking, target),
            Self::NeverAbove(above, target) => dispatchgrant::never_above_said(above, target),
            Self::NeversUnread => dispatchgrant::NEVERS_UNREAD.to_owned(),
            Self::Unsandboxed(target) => {
                let target = crate::shown::short(target);
                format!(
                    "this chat runs with its harness's permission prompts off and with no \
                     sandbox. A chat with neither can change who it may dispatch to, so \
                     purlis starts nobody for it but a chat of its own persona, and {target} \
                     is another. Run this work in a sandboxed chat, or in one a person \
                     answers."
                )
            }
        }
    }
}

/// The grant an unattended chat lacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// The asking chat's persona; none for a chat on no persona.
    pub asking: Option<String>,
    /// The persona it asked for.
    pub target: String,
    /// Whether the project's committed file names the pair and it still does not count on this
    /// machine, because nobody here has reviewed the project's grants (D-1437-R1).
    pub unreviewed: bool,
}

impl Missing {
    /// The sentence the asking chat reads: the pair that is missing, why nothing was asked, and
    /// where a person grants it.
    pub fn say(&self) -> String {
        let target = crate::shown::short(&self.target);
        let Some(asking) = self.asking.as_deref().map(crate::shown::short) else {
            return format!(
                "this chat runs with its harness's permission prompts off and as no persona. \
                 Only a grant made for one chat covers a chat on no persona, and that never \
                 counts for a chat nobody is answering, so nothing lets it dispatch to \
                 {target}. Run this work as a persona a person has allowed to dispatch to \
                 {target}."
            );
        };
        if self.unreviewed {
            return format!(
                "this chat runs with its harness's permission prompts off, so nobody is here \
                 to answer for a dispatch. The project's file lets {asking} chats dispatch to \
                 {target}, and nobody has reviewed the project's dispatch grants on this \
                 machine yet, so that does not count here. A person reviews them in purlis on \
                 this machine, under {SETTINGS}; then dispatch again."
            );
        }
        format!(
            "this chat runs with its harness's permission prompts off, so nobody is here to \
             answer for a dispatch, and no grant lets {asking} chats dispatch to {target}: \
             none for the person on this machine, and none for this project. A grant made \
             for one chat does not count here. Only a person makes one: they dispatch to \
             {target} once from a {asking} chat they are at and choose Allow for me on this \
             machine or Allow for everyone in this project. {SETTINGS} lists the grants that \
             stand. Until then, do this work without {target}, or say in what you leave \
             behind that it is waiting."
        )
    }
}

/// Where the person sees and takes back dispatch grants.
pub const SETTINGS: &str = "Settings › Project › Dispatch";

/// **Whether an unattended chat running as `asking` may dispatch to `target`** under `grants`
/// and `policy`: [`crate::dispatchgrant::covers`], asked without the grants made for one chat,
/// and with "ask the person" answered as a refusal. `sandboxed` is whether the app started
/// the chat inside a sandbox: one it did not dispatches to its own persona and no other.
pub fn covers(
    asking: Option<&str>,
    target: &str,
    grants: &InForce,
    policy: &Locks,
    sandboxed: bool,
) -> Answer {
    let mut standing = grants.clone();
    standing.chat.clear();
    let named_by_the_project = asking.is_some_and(|asking| {
        grants
            .project
            .iter()
            .any(|pair| pair.asking == asking && pair.target == target)
    });
    answer_of(
        dispatchgrant::covers(asking, target, &standing, policy),
        asking,
        target,
        named_by_the_project,
        sandboxed,
    )
}

/// What `answer`, the grant's own for the pair with no chat's grants counted, means for an
/// unattended chat. `named_by_the_project` says whether the project's committed file names the
/// pair, which words the refusal of one this machine has not reviewed. `sandboxed` as for
/// [`covers`]: with no sandbox, only what the grant covers with no grant at all, the chat's
/// own persona, is covered.
pub fn answer_of(
    answer: Covers,
    asking: Option<&str>,
    target: &str,
    named_by_the_project: bool,
    sandboxed: bool,
) -> Answer {
    match answer {
        Covers::Locked(why) => Answer::Locked(why),
        // The person's never, whatever stands beside it: said as it is said to any chat.
        Covers::Never => Answer::Refused(Refusal::Never(
            asking.unwrap_or_default().to_owned(),
            target.to_owned(),
        )),
        Covers::NeverAbove(above) => Answer::Refused(Refusal::NeverAbove(above, target.to_owned())),
        // A chat's own persona never reads as this; for any other, nobody is here to ask.
        Covers::Unread => Answer::Refused(Refusal::NeversUnread),
        Covers::Covered if asking == Some(target) => Answer::Covered,
        Covers::Covered | Covers::NeedsGrant if !sandboxed => {
            Answer::Refused(Refusal::Unsandboxed(target.to_owned()))
        }
        Covers::Covered => Answer::Covered,
        Covers::NeedsGrant => Answer::Refused(Refusal::Missing(Missing {
            asking: asking.map(str::to_owned),
            target: target.to_owned(),
            unreviewed: asking.is_some() && named_by_the_project,
        })),
    }
}

/// The words of a harness's command line that start it with its permission prompts off, as a
/// profile's command would carry them: Claude Code's and Codex's.
const BYPASS_FLAGS: [&str; 4] = [
    "--dangerously-skip-permissions",
    "--dangerously-bypass-approvals-and-sandbox",
    "--yolo",
    // Codex's low-friction mode: it runs what its own sandbox allows without asking first.
    "--full-auto",
];

/// The flags that name how a harness asks, and the value of each that means "ask nobody":
/// Claude Code's permission mode, and Codex's approval policy by both its spellings and as a
/// configuration override.
const MODE_FLAGS: [(&str, &str); 5] = [
    ("--permission-mode", crate::floorguard::UNATTENDED_MODE),
    ("--ask-for-approval", "never"),
    ("-a", "never"),
    // Codex's approval policy again, set as a configuration override (#1509).
    ("--config", "approval_policy=never"),
    ("-c", "approval_policy=never"),
];

/// **The words of `command` that start a harness with its permission prompts off**, if any
/// do, as they are shown to a person: the flag, with its value where the value is what
/// switches them off (`-c approval_policy=never`). `command` is a profile's own, as this
/// machine declares it.
///
/// A recognition of the flags purlis knows, never a reading of what a wrapper script does with
/// its words: a profile whose command is a script that adds one is not seen, and neither is a
/// harness setting kept in a file. A profile is declared on this machine and approved by the
/// person before it runs. **A chat's dispatch may name one of them** (`--profile`), and a
/// persona's definition may, so this is asked of whichever profile a dispatch would start on,
/// whoever named it ([`bypass_refusal`]).
///
/// Each word is read as a command line reads it ([`crate::sandbox::codex::split`]): a value
/// may be attached to its flag (`--ask-for-approval=never`, `-anever`,
/// `-capproval_policy=never`) or be the next word.
pub fn bypass_in(command: &[String]) -> Option<String> {
    use crate::sandbox::codex::{is_flag, split};
    command.iter().enumerate().find_map(|(at, word)| {
        if BYPASS_FLAGS.contains(&word.as_str()) {
            return Some(word.clone());
        }
        if !is_flag(word) {
            return None;
        }
        let (flag, attached) = split(word);
        let next = || command.get(at + 1).map(String::as_str);
        let value = attached.or_else(next)?;
        // A value may be written quoted and spaced (`approval_policy = "never"`).
        let plain = value.replace(['"', '\'', ' '], "");
        MODE_FLAGS
            .iter()
            .any(|(known, never)| flag == *known && plain == *never)
            .then(|| match attached {
                Some(_) => word.clone(),
                None => format!("{flag} {value}"),
            })
    })
}

/// **What an unattended chat is told where its handoff would make a workspace** (D-1444-13):
/// nobody is there to see one made, so a chat nobody is at hands off into a workspace that
/// exists, and `--create` is refused. A chat a person is at is not held to this.
pub const NO_WORKSPACE_IS_MADE: &str = "this chat runs with its harness's permission prompts \
     off, so nobody is here to see a workspace made: a handoff from it goes into a workspace \
     that exists, and --create is refused. Hand off into an existing workspace, or make the \
     workspace from a chat someone is at.";

/// Who named the profile a dispatch would start its chat on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedBy<'a> {
    /// The asking chat, in its dispatch (`--profile`).
    TheDispatch,
    /// This persona's own definition.
    ThePersona(&'a str),
    /// Nobody: it is the profile the asking chat itself runs on.
    TheAskingChat,
}

/// **Why no chat is started for another chat on `on`**, where it asks nobody
/// ([`Inherited::asks_nobody`]), and `None` where it asks.
///
/// **Whoever named it.** A chat one chat starts for another never runs asking nobody: not on
/// the asking chat's own profile, not on one its dispatch names, and not on one the persona's
/// definition names, which a chat can write. A handoff is held to it as a task is.
pub fn bypass_refusal(on: Inherited<'_>, by: NamedBy<'_>) -> Option<String> {
    let why = on.asks_nobody()?;
    let profile = crate::shown::short(on.profile);
    let AsksNobody::Flag(flag) = why else {
        let mark = how_to_mark(&profile);
        return Some(match by {
            NamedBy::TheAskingChat => format!(
                "profile '{profile}' is not marked as asking a person before its harness acts, \
                 and a persona chat never takes such a profile from the chat that dispatched \
                 it. Dispatch from a chat on a profile that asks. {mark}"
            ),
            NamedBy::TheDispatch => format!(
                "the dispatch names profile '{profile}', which is not marked as asking a person \
                 before its harness acts, and purlis starts no chat for another chat on such a \
                 profile. Name a profile that asks, or none. {mark}"
            ),
            NamedBy::ThePersona(persona) => format!(
                "persona '{}' names profile '{profile}', which is not marked as asking a person \
                 before its harness acts, and purlis starts no chat for another chat on such a \
                 profile. Give the persona a profile that asks, from its view, or name one with \
                 --profile. {mark}",
                crate::shown::short(persona)
            ),
        });
    };
    let flag = crate::shown::short(&flag);
    Some(match by {
        NamedBy::TheAskingChat => format!(
            "profile '{profile}' starts its harness with the permission prompts off ({flag}), \
             and a persona chat never takes that from the chat that dispatched it. Dispatch \
             from a chat on a profile that asks."
        ),
        NamedBy::TheDispatch => format!(
            "the dispatch names profile '{profile}', which starts its harness with the \
             permission prompts off ({flag}), and purlis starts no chat for another chat on \
             such a profile. Name a profile that asks, or none."
        ),
        NamedBy::ThePersona(persona) => format!(
            "persona '{}' names profile '{profile}', which starts its harness with the \
             permission prompts off ({flag}), and purlis starts no chat for another chat on \
             such a profile. Give the persona a profile that asks, from its view, or name one \
             with --profile.",
            crate::shown::short(persona)
        ),
    })
}

/// The profile a persona chat starts on, by its name, its command as this machine declares it,
/// and whether it is marked as asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inherited<'a> {
    /// The profile's name.
    pub profile: &'a str,
    /// Its command, as this machine declares it.
    pub command: &'a [String],
    /// Whether it is known to ask a person before its harness acts
    /// ([`crate::profiles::Profile::asks`]).
    pub asks: bool,
}

impl<'a> Inherited<'a> {
    /// A profile as this machine declares it.
    pub fn of(profile: &'a crate::profiles::Profile) -> Self {
        Self {
            profile: &profile.name,
            command: &profile.command,
            asks: profile.asks,
        }
    }

    /// **Why no chat is started for another chat on this profile**, or `None` where one may
    /// be: its command carries a flag that switches the prompts off, or nobody marked it as
    /// asking (the module's "A profile that asks").
    pub fn asks_nobody(&self) -> Option<AsksNobody> {
        if let Some(flag) = bypass_in(self.command) {
            return Some(AsksNobody::Flag(flag));
        }
        (!self.asks).then_some(AsksNobody::Unmarked)
    }
}

/// What keeps a chat started for another chat off a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsksNobody {
    /// Its command carries this flag, which switches the harness's permission prompts off.
    Flag(String),
    /// It is not marked as asking a person before its harness acts.
    Unmarked,
}

/// How a person marks a profile `profile` as asking, for the end of a refusal.
pub fn how_to_mark(profile: &str) -> String {
    format!(
        "A person marks a profile whose harness asks before it acts with asks = true in its \
         [harness.{profile}] table in {}.",
        crate::profiles::LOCAL_FILE
    )
}

/// **The start of the persona chat a dispatch to `target` opens**, from `start`, the one built
/// from the asking chat's record, or why it is not started.
///
/// Whatever `start` held of the asking chat's own is dropped here, whoever built it: the new
/// chat runs as `target`, holds `target`'s own grants
/// ([`crate::dispatchgrant::grants_for_a_dispatched_chat`]), and has no grant of one chat, no
/// opt-out and no conversation or session record to resume. `on` is the profile it starts on:
/// where that profile asks nobody, the chat is not started on it
/// ([`bypass_refusal`], in the words for a profile taken from the asking chat; a caller that
/// knows who named it asks [`bypass_refusal`] first and says so in those words).
pub fn start_of_a_persona_chat(
    start: Start,
    target: &str,
    on: Option<Inherited<'_>>,
) -> Result<Start, String> {
    if let Some(on) = on
        && let Some(refused) = bypass_refusal(on, NamedBy::TheAskingChat)
    {
        return Err(refused);
    }
    let with = dispatchgrant::grants_for_a_dispatched_chat(target);
    Ok(Start {
        persona: Some(with.persona),
        held: with.held,
        grants: with.grants,
        without_sandbox: with.without_sandbox,
        resume: None,
        resuming: None,
        ..start
    })
}

#[cfg(test)]
#[path = "dispatchunattended_tests.rs"]
mod tests;
