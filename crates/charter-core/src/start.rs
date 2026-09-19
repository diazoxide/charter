//! What it means to start a chat on a harness profile.
//!
//! **One home, in the core**, because four callers reach it — the picker, a relaunch's
//! reopen, a handoff, and the CLI — and a gate each of them has to remember is a gate one of
//! them will not. The thing it would let through is a command out of a file a chat can
//! write, running with no prompt between the click and the exec.
//!
//! The order is the whole of it, and every step is a refusal that already has its own
//! reasons recorded elsewhere:
//!
//! 1. the profile is one this machine declares — a chat whose profile is gone is skipped by
//!    NAME and never given another (ADR 0022);
//! 2. the persona is one this plane has;
//! 3. [`crate::wiring::wired_or_refusal`] — the kind is startable, the file is not one git
//!    would carry, the operator approved the command, and the folder is wired or gets wired;
//! 4. only then are the arguments and the environment built.
//!
//! **The harness comes from the DECLARED kind**, never from the program's name.
//! [`crate::harness::Harness::of_command`] cannot tell a shell from a harness charter has
//! not measured, and a profile's command is commonly a wrapper script — ADR 0022 says so in
//! as many words — so inferring from it hands the board `None`, which is the narrowest rule
//! there is, and the chat silently loses the session id that makes it resumable.

use std::path::{Path, PathBuf};

use crate::harness::{Harness, SessionId};
use crate::profiles::{self, Profile};
use crate::reopen::{Fresh, Reopened};

/// What the operator picked, and what the record remembers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Start {
    /// The profile's name. `None` is a chat that is not on a profile at all — the shell the
    /// app opened before there was a picker, which still reopens as itself.
    pub profile: Option<String>,
    /// The persona this chat adopts, or none for the plane's own default.
    pub persona: Option<String>,
    /// What the operator calls this chat, and what a harness that takes a name is given.
    pub name: String,
    pub cwd: Option<PathBuf>,
    /// The conversation to bring back, where the record holds one.
    pub resume: Option<SessionId>,
}

/// A chat that may start, with everything the session core and the board need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ready {
    pub program: String,
    pub args: Vec<String>,
    /// The profile's environment, charter's own variables, and the persona — sorted, so two
    /// starts of one profile are the same launch.
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    /// The harness this chat runs, from its profile's declared kind.
    pub harness: Option<Harness>,
    /// The conversation this chat is now under — resumed, or the one charter just chose.
    pub session: Option<SessionId>,
    pub how: Reopened,
    /// The one line to say when this start wired the profile on its way; empty otherwise.
    pub wired: String,
}

/// Everything a chat needs to start, or the one sentence saying why it may not.
///
/// Refuses rather than starting something else. Every refusal names the profile and what to
/// do; none of them offers a different profile.
pub fn ready(start: &Start, root: &Path) -> Result<Ready, String> {
    let Some(name) = start.profile.as_deref() else {
        return Err(
            "this chat is not on a harness profile, so there is nothing to start it from — \
             pick one."
                .to_owned(),
        );
    };
    // The launch read, which has already asked git whether this plane's `charter.local.toml`
    // would reach every clone of it.
    let (set, check) = profiles::for_launch(root);
    let Some(profile) = set.get(name) else {
        let refused = set
            .refused
            .iter()
            .find(|r| r.name == crate::shown::short(name))
            .map(|r| format!(" It was refused: {}", r.reason))
            .unwrap_or_default();
        let fix = if check.passes() {
            String::new()
        } else {
            format!(" {}", check.fix)
        };
        return Err(format!(
            "profile '{}' is not declared on this machine, so nothing was started — this \
             chat keeps its profile and comes back when that profile is declared again.{}{}",
            crate::shown::short(name),
            refused,
            fix
        ));
    };
    let persona = match start.persona.as_deref() {
        None => None,
        Some(who) => Some(startable_persona(who, root)?),
    };

    // The gate. Startable kind, ignored file, approved command, wired folder — one call, so
    // no caller can start a chat past a check another caller makes.
    let here = start.cwd.clone().unwrap_or_else(|| root.to_path_buf());
    let answer = crate::wiring::wired_or_refusal(profile, &here, root);
    if !answer.may_start() {
        return Err(answer.refusal);
    }

    let harness = Harness::of_kind(&profile.kind);
    let (added, session, how) = arguments(harness, profile, start);
    let home = profiles::home().unwrap_or_else(|| PathBuf::from("~"));
    let mut argv = profiles::expanded_command(profile, &home);
    let program = argv.remove(0);
    // Charter's words first and the profile's own arguments after them, which is the order
    // the Python launcher uses. It is not cosmetic: Codex resumes through a SUBCOMMAND, and
    // a subcommand after a positional prompt is not the same command line at all.
    let mut args = added;
    args.extend(argv);

    Ok(Ready {
        program,
        args,
        env: environment(profile, root, persona.as_deref()),
        cwd: start.cwd.clone(),
        harness,
        session,
        how,
        wired: answer.wired,
    })
}

/// The arguments charter adds, the conversation the chat is now under, and which of the two
/// happened.
fn arguments(
    harness: Option<Harness>,
    profile: &Profile,
    start: &Start,
) -> (Vec<String>, Option<SessionId>, Reopened) {
    let Some(harness) = harness else {
        return (
            Vec::new(),
            None,
            Reopened::Fresh(Fresh::NoResumeForThisProgram),
        );
    };
    // The operator already named a session in the profile's own command, so charter adds
    // none of its own: two `--resume` on one command line is not a harness anyone has
    // measured, and the one the operator typed is the one they meant.
    if harness.session_named_in(&profile.command[1..]) {
        return (
            Vec::new(),
            None,
            Reopened::Fresh(Fresh::SessionNamedByTheOperator),
        );
    }
    match start.resume.as_ref() {
        Some(id) => match harness.resume_argv(id, &start.name) {
            Some(argv) => (argv, Some(id.clone()), Reopened::Resumed(id.clone())),
            None => (
                Vec::new(),
                None,
                Reopened::Fresh(Fresh::NoResumeForThisProgram),
            ),
        },
        // Where the harness takes an id charter chose, it is given one and that id is kept —
        // otherwise the next quit would have nothing to record and the chat could never be
        // resumed at all.
        None => {
            let chosen = harness.chooses_session_id().then(SessionId::fresh);
            let argv = chosen
                .as_ref()
                .map(|id| harness.new_session_argv(id, &start.name))
                .unwrap_or_default();
            (argv, chosen, Reopened::Fresh(Fresh::NoConversationRecorded))
        }
    }
}

/// The profile's environment, plus charter's own — which a profile may not set, because a
/// declaration that tried would have been refused.
///
/// `CHARTER_HARNESS` keeps the REGISTRY's name for the kind, and the profile rides beside it.
/// Hooks compare that variable to `claude-code` for session ids, resume and the working
/// spinner, so a value of `claude-work` would make each of them quietly answer "not Claude
/// Code".
fn environment(profile: &Profile, root: &Path, persona: Option<&str>) -> Vec<(String, String)> {
    let home = profiles::home().unwrap_or_else(|| PathBuf::from("~"));
    let mut env: Vec<(String, String)> =
        profiles::expanded_env(profile, &home).into_iter().collect();
    env.push(("CHARTER_ROOT".to_owned(), root.display().to_string()));
    env.push(("CHARTER_HARNESS".to_owned(), profile.harness.clone()));
    env.push(("CHARTER_HARNESS_PROFILE".to_owned(), profile.name.clone()));
    if let Some(who) = persona {
        env.push(("CHARTER_PERSONA".to_owned(), who.to_owned()));
    }
    env.sort();
    env
}

/// The persona a new chat on this plane would adopt — the plane's `[persona] default`, but
/// **only when it is one the plane actually has**.
///
/// `[persona] default` is a line in a committed file and nothing checks that the persona it
/// names exists: it can point at a persona that was deleted, or at `_shared`, which is the
/// store every persona reads rather than a persona anybody adopts. Offering either as the
/// picker's preselection means the operator presses Start and is refused over a persona
/// they never chose — so what is offered is filtered against the list that is drawn beside
/// it, and the two cannot disagree.
///
/// The sidebar still shows `[persona] default` as it is written, because that is a report
/// of what the file says. This is a different question: what would START.
pub fn persona_for_a_new_chat(root: &Path) -> Option<String> {
    let plane = crate::workspaces::Plane::open(root.to_path_buf());
    let wanted = plane.default_persona()?;
    plane
        .personas()
        .ok()?
        .into_iter()
        .find(|have| *have == wanted)
}

/// `who`, if it is a persona a chat on this plane may adopt.
///
/// **One source of truth with the list the picker draws** ([`crate::workspaces::Plane::personas`]),
/// and that is the whole point: offering a name the start then refuses is a dialog arguing
/// with itself. Three ways they used to disagree, each found by a review probe:
///
/// - the picker lists a **legacy flat** persona (`personas/<name>.md`) and the start wanted
///   `personas/<name>/persona.md`, so a plane on the old layout offered personas that could
///   not start;
/// - `_shared` is the store every persona READS rather than a persona anybody adopts. The
///   list excludes it by name and the start admitted it, so a caller that is not the picker
///   could point a chat's `CHARTER_PERSONA` at the shared store;
/// - a persona directory that is a **symlink out of the plane** was listed and started, so a
///   committed link decided what a chat adopts and where charter then read it from.
///
/// The containment check is on the entry that is opened, not on `personas/` above it.
fn startable_persona(who: &str, root: &Path) -> Result<String, String> {
    let plane = crate::workspaces::Plane::open(root.to_path_buf());
    let shown = crate::shown::short(who);
    let known = plane.personas().map_err(|e| {
        format!("charter could not read this plane's personas ({e}), so nothing was started.")
    })?;
    if !known.iter().any(|have| have == who) {
        return Err(format!(
            "no persona '{shown}' on this plane, so nothing was started — pick one of: {}.",
            if known.is_empty() {
                "none declared".to_owned()
            } else {
                known.join(", ")
            }
        ));
    }
    // What the name RESOLVES to, both layouts, gated where it is opened.
    let dir = plane
        .persona(who)
        .map_err(|e| format!("{e}, so nothing was started."))?;
    let entry = if dir.dir().join("persona.md").is_file() {
        dir.dir().join("persona.md")
    } else {
        root.join("personas").join(format!("{who}.md"))
    };
    crate::contain::readable(root, &entry).map_err(|why| {
        format!("persona '{shown}' resolves outside this plane ({why}), so nothing was started.")
    })?;
    Ok(who.to_owned())
}
