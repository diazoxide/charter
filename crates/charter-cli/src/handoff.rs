//! `charter handoff` — open a chat in a workspace you name, started on an approved brief.
//!
//! ```text
//! charter handoff <workspace> [--create --vision "<vision>"] [--persona <name>] <<'BRIEF'
//! <the brief>
//! BRIEF
//! ```
//!
//! A port of `charter/commands_handoff.py`: the ORDER of the refusals and the writes.
//! [`charter_core::handoff`] is every string they are made of, and its module header is where
//! each refusal is argued — including the four that belong to the PreToolUse guard and are
//! therefore not here.
//!
//! **The order is the design.** Every question is asked before the first write, because
//! charter fails toward no change: a handoff that created the workspace and then discovered
//! the brief was empty has already cost somebody a cleanup.
//!
//! 1. the workspace name, the flag pairs, the persona — questions answered from the plane;
//! 2. the brief on stdin, read before the frame check because the command charter prints when
//!    there is no frame has to carry it;
//! 3. the first message's own shape — empty, a flag, one word, a NUL, past the byte bound;
//! 4. the frame, which in this charter is where it stops (see below).
//!
//! # This charter reaches step 4 every time, and writes nothing
//!
//! There is no channel from this binary into the desktop app that opens a chat, so the frame
//! check that Python reaches after every other refusal is the one this always reaches. It
//! answers the way Python answers a shell that is not a chat: it prints the command to run in
//! a new terminal, and exits 1. Python's writes — the workspace, its vision, the todo — all
//! come *after* that check, so a handoff that cannot open a chat has created nothing in
//! either implementation, and there is nothing here that could be half-done.

use std::io::{IsTerminal, Read};
use std::process::ExitCode;

use charter_core::handoff::{self, BadMessage, NoBrief};

use crate::voice;

/// What `charter handoff` was asked to do.
pub struct Args {
    pub workspace: String,
    pub create: bool,
    pub vision: Option<String>,
    pub persona: Option<String>,
}

/// The most personas a refusal lists before it says how many it left out —
/// `frame/switch._SOME`.
const SOME: usize = 5;

pub fn handoff(here: &crate::Here, args: &Args) -> ExitCode {
    let root = here.plane.root();
    let ws = args.workspace.as_str();

    if !charter_core::contain::workspace_name_ok(ws) {
        voice::err(&format!(
            "charter handoff: '{}' cannot name a workspace — nothing was opened. A workspace \
             name is letters, digits, '.', '_' and '-', and does not start with a dot.",
            charter_core::personas::one_line(ws)
        ));
        return ExitCode::FAILURE;
    }
    let vision = args.vision.as_deref().filter(|v| !v.is_empty());
    if vision.is_some() && !args.create {
        voice::err(&format!(
            "charter handoff: --vision describes a workspace this call creates, and '{ws}' is \
             not being created — nothing was opened. Set an existing workspace's vision with: \
             charter workspace vision --workspace {ws} \"<the goal>\""
        ));
        return ExitCode::FAILURE;
    }
    if args.create && vision.is_none() {
        voice::err(
            "charter handoff: --create needs --vision — a workspace with no vision is never \
             proposed as a target, so it would be created unfindable. Nothing was opened.",
        );
        return ExitCode::FAILURE;
    }
    // `workspace use`'s rule, asked rather than re-invented: the always-present workspace
    // exists whether or not its directory does, so `--create default` on a fresh plane is a
    // name collision rather than a creation.
    let exists = here
        .plane
        .workspaces()
        .unwrap_or_default()
        .iter()
        .any(|n| n == ws)
        || ws == charter_core::active::plane_default_workspace(root);
    if args.create && exists {
        voice::err(&format!(
            "charter handoff: workspace '{ws}' already exists, and --create only makes a new \
             one — nothing was opened. Drop --create to hand off into it."
        ));
        return ExitCode::FAILURE;
    }
    if !args.create && !exists {
        voice::err(&format!(
            "charter handoff: no workspace '{ws}' on this plane — nothing was opened. Create \
             it in the same call: charter handoff {ws} --create --vision \"<what it is for>\""
        ));
        return ExitCode::FAILURE;
    }
    // Existence is the DEFINITION's answer, as it is for `persona use`, and not membership in
    // the roster — which also lists a directory whose `persona.md` does not load. The sentence
    // is `persona.name_refusal`'s, the one every command that takes a persona name says.
    let persona = args.persona.as_deref().filter(|p| !p.is_empty());
    if let Some(name) = persona
        && let Some(refused) = charter_core::personas::name_refusal(root, name)
    {
        voice::err(&format!(
            "charter handoff: {refused} — have: {}. Nothing was opened.",
            some(&here.plane.personas().unwrap_or_default())
        ));
        return ExitCode::FAILURE;
    }

    let brief = match read_brief() {
        Ok(brief) => brief,
        Err(refusal) => {
            voice::err(&refusal.say(ws));
            return ExitCode::FAILURE;
        }
    };
    // The leak guard's own classifier, so the brief a handoff refuses and the command a leak
    // guard refuses are the same set of shapes. The KIND, never the matched text — a refusal
    // that quoted the credential would put it in the transcript this exists to keep it out of.
    if let Some(kind) = charter_core::secretshape::secret_kind(&brief) {
        voice::err(&format!(
            "charter handoff: the brief looks like it carries a secret ({kind}) — nothing was \
             opened. A brief travels to the new chat as a command-line argument any local \
             process can read while the harness starts, so it never carries a secret. Name \
             where the credential lives instead of pasting it, as the whole value on its \
             line: `vault:<vault>/<key>`, or `charter secret get <vault> <key>`."
        ));
        return ExitCode::FAILURE;
    }

    let source_chat = std::env::var("CHARTER_SESSION_ID")
        .ok()
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| handoff::NO_CHAT.to_string());
    let source_ws = here.active_workspace(None);
    let msg = handoff::first_message(
        &handoff::stamp(&source_chat, &source_ws, chrono::Local::now().naive_local()),
        &brief,
    );
    if let Some(bad) = handoff::bad_message(&msg) {
        let mut said = format!("charter handoff: {}", bad.say());
        // Only the byte bound gets the note, and it is compared against the seam's own
        // sentence rather than re-deriving the bound here: two places counting bytes is how
        // the note comes to appear beside a refusal that was about something else.
        if matches!(bad, BadMessage::TooLong(_)) {
            said.push('\n');
            said.push_str(&handoff::stamped_message_note(brief.len()));
        }
        voice::err(&said);
        return ExitCode::FAILURE;
    }

    // ---- the frame, which is where this charter stops --------------------------------
    let (command, named) = printed_command(root, ws, &msg, vision.filter(|_| args.create), persona);
    let mut said = format!(
        "charter handoff: this `charter` is the desktop app's binary, which has no frame to \
         open a chat in the background of and no way to ask the app to open one — nothing was \
         opened.\n  Run this in a new terminal instead:\n  {command}"
    );
    if !named {
        said.push('\n');
        said.push_str(&format!(
            "  Nothing here says which harness to start and this plane declares no `[harness] \
             default`, so put the one you want where `{}` stands (opencode takes `--prompt` \
             in front of the message; `claude` and `codex` take it as it is).",
            handoff::UNKNOWN_HARNESS
        ));
    }
    voice::err(&said);
    ExitCode::FAILURE
}

/// The brief on stdin, or why there is none.
///
/// The bytes are read as BYTES and decoded here, because the decision "is this UTF-8" has to
/// be charter's: a runtime that decoded for us would have made it already, with whatever
/// replacement policy it has.
///
/// `None` for a stdin that is not there at all — `charter handoff beta 0<&-`, a spelling the
/// Bash guard allows because the two words in front of it are the exact ones. Python meets it
/// as `sys.stdin is None`; here fd 0 is closed, so the read fails with `EBADF`.
fn read_brief() -> Result<String, NoBrief> {
    let stdin = std::io::stdin();
    // Before any read, and that order is the whole function: a read on a terminal blocks for
    // ever with nothing on screen to say why.
    if stdin.is_terminal() {
        return Err(NoBrief::Terminal);
    }
    let mut bytes = Vec::new();
    match stdin.lock().read_to_end(&mut bytes) {
        Ok(_) => charter_core::handoff::read_brief(Some(&bytes), false),
        Err(e) if e.raw_os_error() == Some(9) => charter_core::handoff::read_brief(None, false),
        // Any other read failure is not a brief either, and "not UTF-8" is the wrong
        // sentence for it — but an I/O error on a pipe charter was handed is a brief that is
        // not there, which is what `Closed` says.
        Err(_) => charter_core::handoff::read_brief(None, false),
    }
}

/// `(the command to run in a new terminal, whether a harness could be named)`.
///
/// The harness is the one THIS process is running inside — a handoff never changes harness —
/// read from `$CHARTER_HARNESS`, which carries a REGISTRY name (`claude-code`). A plane's
/// `[harness] default` is the fallback, said for a shell that is not a chat.
///
/// A harness charter has not measured the first message of is treated as no harness at all
/// rather than handed the positional spelling as a guess.
fn printed_command(
    root: &std::path::Path,
    ws: &str,
    msg: &str,
    create_vision: Option<&str>,
    persona: Option<&str>,
) -> (String, bool) {
    let word = harness_word(root);
    let extra = word
        .as_deref()
        .and_then(|word| handoff::first_message_argv(word, msg));
    match extra {
        Some(extra) => (
            handoff::terminal_command(
                word.as_deref().unwrap_or(handoff::UNKNOWN_HARNESS),
                ws,
                &extra,
                create_vision,
                persona,
            ),
            true,
        ),
        None => (
            handoff::terminal_command(
                handoff::UNKNOWN_HARNESS,
                ws,
                std::slice::from_ref(&msg.to_string()),
                create_vision,
                persona,
            ),
            false,
        ),
    }
}

/// The `charter <word>` this process's harness answers to, or `None`.
fn harness_word(root: &std::path::Path) -> Option<String> {
    // `$CHARTER_HARNESS` is what charter's own launcher exports into a chat, and it carries
    // the REGISTRY name (`claude-code`) rather than the word after `charter`.
    if let Ok(registry) = std::env::var("CHARTER_HARNESS")
        && let Some(kind) = charter_core::profiles::KINDS
            .iter()
            .find(|k| k.registry == registry)
    {
        return Some(kind.word.to_string());
    }
    let set = charter_core::profiles::current(root);
    let default = set.default.clone()?;
    // `[harness] default` names a profile; a profile's KIND is the word `charter <word>`
    // takes. Python compares the declaration against each harness's `cli_name`, which is the
    // same word for every built-in profile.
    set.get(&default).map(|p| p.kind.clone())
}

/// `names`, contained, capped, saying how many were left out — `frame/switch._some`.
fn some(names: &[String]) -> String {
    if names.is_empty() {
        return "none".to_string();
    }
    let mut shown: Vec<String> = names
        .iter()
        .take(SOME)
        .map(|n| charter_core::personas::one_line(n))
        .collect();
    if names.len() > shown.len() {
        shown.push(format!("…{} more", names.len() - shown.len()));
    }
    shown.join(", ")
}
