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
//! 3. the plane's guest layer reaches the directory the chat starts in, or gets written
//!    there — a git root of its own cuts the walk-up off (ADR 0027, closed by M1.x);
//! 4. [`crate::wiring::refusal`] — the kind is startable, the file is not one git would
//!    carry, and the operator approved the command. Nothing is installed: the app arms the
//!    chat itself ([`crate::plugin`]);
//! 5. only then are the arguments and the environment built.
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
    /// Whether THIS chat draws charter's footer in its pane rather than a blank line.
    ///
    /// Default `false`, which is the app as it has always behaved. See [`FOOTER_ENV`] for
    /// what it does and ADR 0029 for why it is a chat's property and not a plane's.
    pub show_footer: bool,
}

/// Where a chat is told to draw charter's footer rather than a blank line.
///
/// **What this is not.** It does not bring back a footer Claude Code would otherwise draw:
/// `charter statusline` **is** Claude Code's `statusLine` command, so that line is charter's
/// to fill or to leave empty, and a harness with charter wired in has no footer of its own to
/// fall back on (charter ADR 0019 measured exactly that — a framed session "has no
/// context/cache gauge on any surface"). What this variable chooses is between **charter's
/// footer** and **nothing**.
///
/// Inside the app the answer has been "nothing", because the app's own panels draw the plane
/// (charter ADR 0019, transposed — see `charter-cli/src/statusline.rs`). ADR 0029 makes that
/// a default rather than a law: a chat started with this variable set to [`FOOTER_SHOW`]
/// draws the footer, and every other chat in the same window is unaffected.
///
/// **Set by charter, never by a profile.** `CHARTER_`-prefixed names are refused in a
/// profile's `env` ([`crate::profiles`], ruling 14), so this cannot be turned on by editing
/// `charter.local.toml` — which is deliberate: it is a choice made in the picker, for one
/// chat, and recorded with that chat.
///
/// **Absence is the default.** Only the exact word [`FOOTER_SHOW`] draws the footer, so a
/// value inherited from somewhere else, or a stale one, reads as "blank" rather than as a
/// surprise. Both the name and the value are constants here, so nothing a chat or a record
/// can write ever reaches the environment charter builds.
pub const FOOTER_ENV: &str = "CHARTER_FOOTER";

/// The one value of [`FOOTER_ENV`] that means "draw it".
pub const FOOTER_SHOW: &str = "show";

/// A chat that may start, with everything the session core and the board need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ready {
    pub program: String,
    /// The rest of the profile's own `command`, after [`Self::program`] — the words the
    /// operator wrote, which stand before anything charter adds.
    ///
    /// **Its own field because a profile's command is commonly a wrapper** (ADR 0022): for
    /// `["ccs", "work"]` it is `["work"]`, the wrapper's own subcommand, and a flag put in
    /// front of it is a flag handed to the wrapper rather than to the harness (M8.3).
    pub command: Vec<String>,
    /// Charter's own words — the new-session id or the resume — which follow the profile's
    /// command and whatever the app arms the harness with ([`Self::command_line`]). A caller
    /// may append after them (a handoff's positional first message is the last word).
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
    /// The harness's own plugins this chat is handed on or off, by the harness's id: what the
    /// project chose among those installed, and the pins (charter-app#274, ADR 0050). Empty for
    /// a harness whose adapter cannot apply per chat. It reaches the harness through
    /// [`crate::harness::Harness::state_hooks`].
    pub plugins: crate::harness_plugin::Chosen,
}

impl Ready {
    /// Every argument after [`Self::program`], with `armed` — the arguments that arm the
    /// harness for this session alone ([`crate::harness::StateHooks`]) — in their place.
    ///
    /// **The profile's whole command, then `armed`, then charter's own words.** The whole
    /// command first because it is one command the operator wrote, and a wrapper reads its
    /// own words before it hands the rest to the harness: `["ccs", "work"]` starts as
    /// `ccs work --plugin-dir … --settings … --session-id …`, never
    /// `ccs --plugin-dir … work`. Charter's words last, after the armed flags where they have
    /// always stood: Codex resumes through a subcommand (`resume <id>`), and a handoff's
    /// first message is positional.
    ///
    /// For a plain `["claude"]` or `["codex"]` the command is empty and this is the line it
    /// always was: `armed`, then charter's words.
    pub fn command_line(&self, armed: Vec<String>) -> Vec<String> {
        Self::line(self.command.clone(), armed, self.args.clone())
    }

    /// [`Self::command_line`] from its three parts, for a caller that holds them apart — the
    /// app's one place a session is opened, which also opens chats on no profile (an empty
    /// `command`).
    pub fn line(command: Vec<String>, armed: Vec<String>, charters: Vec<String>) -> Vec<String> {
        let mut line = command;
        line.extend(armed);
        line.extend(charters);
        line
    }
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

    let here = start.cwd.clone().unwrap_or_else(|| root.to_path_buf());
    // The plane's layer: its ask/deny rules have to be in the tree before a chat stands in it.
    //
    // Nothing here runs a command out of a file a chat can write, which is what the consent
    // gate below exists for: this writes charter's own documents into a tree charter owns,
    // and it is idempotent, so doing it for a start that is then refused costs nothing.
    layered_or_refusal(&here, root)?;
    // The gate. Startable kind, ignored file, approved command — one call, so no caller can
    // start a chat past a check another caller makes.
    if let Some(why) = crate::wiring::refusal(profile, root) {
        return Err(why);
    }

    let harness = Harness::of_kind(&profile.kind);
    let (added, session, how) = arguments(harness, profile, start);
    let home = profiles::home().unwrap_or_else(|| PathBuf::from("~"));
    // Resolved HERE, in charter's own process, and the absolute path is what the terminal is
    // given (charter-app#134). A bare word handed to a pty is resolved against whatever
    // `PATH` the app itself was started with — which for a Finder-launched `.app` is
    // `/usr/bin:/bin:/usr/sbin:/sbin` and holds no harness.
    let mut argv = crate::programs::resolve_argv(&profiles::expanded_command(profile, &home))
        .map_err(|gone| format!("{} Nothing was started.", gone.said()))?;
    let program = argv.remove(0);
    let env = environment(profile, root, persona.as_deref(), start.show_footer);
    // Listed from the chat's OWN environment: a profile that points its harness at another
    // account's directory (`CLAUDE_CONFIG_DIR`) is listed against that account. A chat in a
    // workspace — by its directory, which is how the window files it under one — also takes
    // that workspace's choices, between Shared and Local (charter-app#282).
    // Asked only for a kind that has an adapter: any other is handed nothing either way.
    let workspace = start
        .cwd
        .as_deref()
        .filter(|_| crate::harness_plugin::adapter(&profile.kind).is_some())
        .and_then(|cwd| crate::workspaces::Plane::open(root).workspace_of(cwd));
    let plugins = crate::harness_plugin::for_start(
        &profile.kind,
        root,
        workspace.as_deref(),
        &crate::harness_plugin::Env::of(&env),
    );
    // The profile's own words stay together and in front; charter's go after them and after
    // whatever arms the harness. [`Ready::command_line`] is where the order is decided.
    Ok(Ready {
        program,
        command: argv,
        args: added,
        env,
        cwd: start.cwd.clone(),
        harness,
        session,
        how,
        plugins,
    })
}

/// Make sure the plane's guest layer is in the tree this chat would start in — or say why a
/// chat may not start there.
///
/// **Only a worktree of a workspace's clone**, named by path arithmetic
/// ([`crate::worktree::locate`]) and then re-checked as a path this workspace may hold
/// ([`crate::worktree::confine::within_workspace`]). Every other `cwd` is left alone: the
/// plane root and a workspace directory read the plane's own copies by walking up, a clone is
/// the Python's to wire until that port lands, and a directory that is none of those is
/// somewhere charter was pointed at rather than somewhere it owns.
///
/// **Write, then refuse on what did not land.** A worktree charter cut is wired at cut time,
/// so the usual answer here costs one `want` and a digest per file and changes nothing. What
/// this is for is the tree charter did *not* cut — `git worktree add` run by hand, or by
/// another tool — where the repair is this call, not a trip to another binary. A tree whose
/// layer charter cannot finish is refused with the sentence naming what blocked it, because a
/// chat that looks guarded and is not is the failure this refusal exists to prevent.
pub fn layered_or_refusal(here: &Path, root: &Path) -> Result<(), String> {
    let Some(found) = crate::worktree::locate(root, here) else {
        return Ok(());
    };
    let piece = crate::worktree::path_for(root, &found.workspace, &found.repo, &found.piece)
        .and_then(|path| {
            crate::worktree::confine::within_workspace(root, &found.workspace, &path)
                .map_err(Into::into)
        })
        .map_err(|refusal| {
            format!(
                "the worktree this chat would start in is not one charter may write \
                 ({refusal}), so nothing was started."
            )
        })?;
    let layered = crate::guest::wire(root, &piece);
    if layered.complete() {
        return Ok(());
    }
    Err(format!("{} Nothing was started.", layered.refusal(&piece)))
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
///
/// [`FOOTER_ENV`] is pushed **only** when the chat asked for charter's footer. An
/// absent variable is the default, so nothing has to be unset for a chat that did not ask —
/// and a chat that did cannot be confused with one whose value came from somewhere else,
/// because only this line writes it.
fn environment(
    profile: &Profile,
    root: &Path,
    persona: Option<&str>,
    show_footer: bool,
) -> Vec<(String, String)> {
    let home = profiles::home().unwrap_or_else(|| PathBuf::from("~"));
    let mut env: Vec<(String, String)> =
        profiles::expanded_env(profile, &home).into_iter().collect();
    env.push(("CHARTER_ROOT".to_owned(), root.display().to_string()));
    env.push(("CHARTER_HARNESS".to_owned(), profile.harness.clone()));
    env.push(("CHARTER_HARNESS_PROFILE".to_owned(), profile.name.clone()));
    if let Some(who) = persona {
        env.push(("CHARTER_PERSONA".to_owned(), who.to_owned()));
    }
    if show_footer {
        env.push((FOOTER_ENV.to_owned(), FOOTER_SHOW.to_owned()));
    }
    env.sort();
    env
}

/// `env` with the `PATH` a chat runs under ([`crate::programs::chat_path`]), unless it already
/// names one.
///
/// **A profile that declares `env = { PATH = "…" }` wins, whole** — the escape hatch
/// charter-app#136 was asked to keep. It is the operator's own line in their own machine's
/// file, and a `PATH` they wrote down is one they meant; charter does not append to it behind
/// their back. The app's own hooks still reach the bundled `charter`, because they name it by
/// its absolute path.
///
/// **Applied where a chat's program is opened, for every chat** — a profile's and the
/// operator's shell alike — and not inside [`ready`], because the `charter` whose directory
/// goes first is the app's to know ([`crate::harness::Harness::state_hooks`] is handed it the
/// same way) and a shell chat never passes through `ready` at all.
///
/// Sorted afterwards, like everything [`environment`] builds, so two starts of one chat are
/// the same launch.
pub fn with_chat_path(
    mut env: Vec<(String, String)>,
    charter: Option<&Path>,
) -> Vec<(String, String)> {
    if env.iter().any(|(name, _)| name == PATH_ENV) {
        return env;
    }
    if let Some(path) = crate::programs::chat_path(charter) {
        env.push((PATH_ENV.to_owned(), path));
        env.sort();
    }
    env
}

/// The variable a chat's program searches for every bare word it runs.
const PATH_ENV: &str = "PATH";

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_the_profile_declared_is_the_path_the_chat_gets_whole() {
        // The escape hatch charter-app#136 was asked to keep: the operator's own line wins,
        // and charter appends nothing to it.
        let declared = vec![
            ("FOO".to_owned(), "bar".to_owned()),
            ("PATH".to_owned(), "/only/this".to_owned()),
        ];
        assert_eq!(
            with_chat_path(declared.clone(), Some(Path::new("/app/charter"))),
            declared
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_chat_with_no_declared_path_is_given_one_starting_in_charters_own_directory() {
        let env = with_chat_path(
            vec![("CHARTER_ROOT".to_owned(), "/plane".to_owned())],
            Some(Path::new("/app/bundle/charter")),
        );
        let path = env
            .iter()
            .find(|(name, _)| name == "PATH")
            .map(|(_, value)| value.as_str())
            .expect("a PATH was added");
        assert!(path.starts_with("/app/bundle:"), "{path}");
        assert!(path.contains("/usr/local/bin"), "{path}");
        let mut sorted = env.clone();
        sorted.sort();
        assert_eq!(env, sorted, "the same chat is the same launch");
    }
}
