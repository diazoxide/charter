//! `charter statusline` — the command Claude Code runs to fill its footer, and the record it
//! keeps whether or not it draws one.
//!
//! A port of `charter/statusline.py:main`'s decision edge and of nothing below it. What this
//! command does, and what it deliberately does not, both come out of **ADR 0019, `the frame
//! owns the surface`** (charter `docs/adr/0019`), so that ADR's reasoning is what the
//! sections below are arguing with.
//!
//! # ADR 0019, transposed: the APP owns the surface
//!
//! ADR 0019 settled what happens when charter draws the plane twice on one screen. Inside a
//! live tmux frame the frame's panels carry the repos, the personas, the alerts and the
//! todos, and Claude Code's footer three lines below them carried all of it again — so the
//! footer goes blank and the panels are the one surface.
//!
//! charter-app is that same picture with tmux taken out: M1.5 put the workspace's repos,
//! their branches, their dirt and their CI column into panels the app draws, and a chat the
//! app started is a terminal inside that same window. A footer there is the duplicate ADR
//! 0019 is about, arrived at by a different road.
//!
//! So the decision is the ADR's, and the RUNGS are its four, each replaced by the app's own
//! evidence for the same fact. [`the_app_owns_this_surface`] holds them.
//!
//! **What is new, and is this port's to defend:** ADR 0019 was written about tmux and says
//! nothing about a desktop window. Transposing it is a judgement, and the judgement is stated
//! here rather than buried: the property the ADR turns on is *"the plane's state is already
//! on this screen, drawn by charter, and drawing it again teaches the reader to stop reading
//! it"*, and that property holds in the app exactly as it held in the frame. If it is wrong,
//! it is wrong at the rung level and the rungs are separable.
//!
//! # Suppression means "render nothing", never "stop running"
//!
//! This is the part ADR 0019 writes down hardest, and it is why this command exists in Rust
//! at all before it can draw anything:
//!
//! > The suppressed command still reads its payload and still records this turn's token usage
//! > … Claude Code's per-turn JSON is the **only** place those numbers exist … So the tempting
//! > cleanup — "this command prints nothing inside a frame, take it out of
//! > `.claude/settings.json`" — does not remove a duplicate. **It deletes the record,
//! > silently**, and nothing notices until somebody goes looking for a history that stopped
//! > being written months earlier.
//!
//! [`crate::statusline::run`] therefore records the turn on **both** branches, and
//! `tests/statusline.rs` fails if either stops. `print()` and not a bare exit, too: Claude
//! Code reads a line from this command, and an empty one is how it is told there is nothing
//! to show.
//!
//! # What this does NOT draw yet, and why that is a milestone and not an omission
//!
//! `charter/statusline.py:render` is not a renderer with some dependencies. It is charter's
//! whole read surface: the nine-rung workspace resolution, a `git status` per clone, linked
//! worktrees drawn as rows, persona chips with vault health and memory counts, the alert
//! list, the session strip, the update-freshness cache, and `charter/tui.py`'s column
//! algebra under all of it — 1,287 statements in that one module before anything it imports.
//! None of it is here. Outside the app this command says so in one line rather than drawing a
//! footer that is subtly not charter's; the differential scenario that covers it declares the
//! difference rather than skipping the command.
//!
//! The record above is the half that had to land now, because it is the half the app's own
//! path needs: inside the app the footer is blank either way, and what would otherwise be
//! lost is the history.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use charter_core::hookwire::{CHAT_ENV, SOCKET_ENV};
use charter_core::usage;

/// The registry's name for Claude Code, as `$CHARTER_HARNESS` carries it.
const CLAUDE_CODE: &str = "claude-code";

/// What the operator's `$CHARTER_HARNESS` says, so a test can drive the rungs.
pub struct Ambient {
    pub stdout_is_a_tty: bool,
    pub socket: Option<PathBuf>,
    pub chat: Option<String>,
    pub harness: Option<String>,
}

impl Ambient {
    /// This process's own environment.
    pub fn here() -> Self {
        Self {
            stdout_is_a_tty: std::io::stdout().is_terminal(),
            socket: std::env::var_os(SOCKET_ENV).map(PathBuf::from),
            chat: std::env::var(CHAT_ENV).ok(),
            harness: std::env::var("CHARTER_HARNESS").ok(),
        }
    }
}

/// Is this invocation drawing INTO the app, which already draws all of it?
///
/// ADR 0019's four rungs, each answered with the app's own evidence. Every rung must hold;
/// every failure means "not in the app", **which renders** — because a status line that
/// vanished for a reason nobody can see is the worst outcome available here.
///
/// * **stdout is not a tty.** Claude Code pipes it, so a tty means a human typed `charter
///   statusline` and wants the thing they asked for; an app open elsewhere on the screen is no
///   reason to answer them with a blank line. (ADR 0019 measured this against Claude Code
///   2.1.241: the command's stdout is a pipe and its environment is Claude Code's own, passed
///   intact — which is what makes the three variables below readable at all.)
///
/// * **`$CHARTER_HOOK_SOCKET` names a socket an app is listening on RIGHT NOW.** This is the
///   rung the app answers *better* than the frame did. ADR 0019 had to ask "is the launcher
///   pid still a running process", because a directory left behind by a crashed launcher would
///   otherwise blank a plane's status line for ever with nothing on screen to say why. Here
///   the app itself answers: a `connect` succeeds only while something is accepting on that
///   socket, and a socket file left behind by an app that has gone refuses with
///   `ECONNREFUSED`. Neither can hang ([`charter_core::hookwire::send`] says so for the same
///   two errors), so no deadline is armed for it.
///
/// * **`$CHARTER_CHAT` is a chat number the app set.** The socket says an app is alive; this
///   says the process asking is *inside* one of its chats, which is ADR 0019's `$TMUX_PANE`
///   rung and is there for the same reason: **holding an id is not the same as being in the
///   frame**. A process can inherit one it does not belong to, and blanking on the socket
///   alone would take away the one correct surface such an operator still had.
///
/// * **the harness is Claude Code.** Suppression removes a DUPLICATE, and only Claude Code has
///   the surface being duplicated. A harness with no status bar of its own is never
///   suppressed — which is ADR 0019's own premise applied one step further rather than an
///   exception to it. Asked LAST, and that ordering is the cost argument: it is one
///   environment lookup on the only path that reaches it.
///
/// Never fails. Everything it touches is ambient.
pub fn the_app_owns_this_surface(ambient: &Ambient) -> bool {
    if ambient.stdout_is_a_tty {
        return false;
    }
    let Some(socket) = ambient.socket.as_deref() else {
        return false;
    };
    if std::os::unix::net::UnixStream::connect(socket).is_err() {
        return false;
    }
    // The same shape `hookwire::Report::read` requires of it, so the app and this command
    // cannot come to disagree about what a chat number is.
    if !ambient
        .chat
        .as_deref()
        .is_some_and(|chat| chat.parse::<u32>().is_ok())
    {
        return false;
    }
    ambient.harness.as_deref() == Some(CLAUDE_CODE)
}

/// The one line this command prints when it is NOT suppressed.
///
/// Coloured unconditionally, as every line `charter/statusline.py` prints is: the surface is
/// Claude Code's footer, which is a terminal, and the module's own fallback
/// (`f"{_CYAN}⬢{_R} charter"`) does not consult `$NO_COLOR` either.
///
/// It says what is true rather than approximating a footer. A near-copy of charter's status
/// line that quietly omitted the alert row would be worse than a sentence: an operator reads
/// a footer to find out whether anything needs them, and one that can only ever say "nothing"
/// is a footer that lies once a week.
pub const NOT_DRAWN_YET: &str = "\x1b[36m⬢\x1b[0m charter — this build keeps the session's record and does not draw the \
     plane yet";

/// `charter statusline`. Always succeeds; a footer is not worth a non-zero exit.
///
/// `plane` is `None` when this process is not standing in one. **Then nothing is recorded**,
/// and that is a declared divergence from Python, which falls back to `<cwd>/.charter` and so
/// scatters a session's history into whatever directory the render happened to run in.
/// `glstate.maybe_spawn` already refuses to spawn for exactly that reason ("outside a plane
/// `config.STATE_DIR` is `<cwd>/.charter`, so a spawn here would scatter charter's caches
/// into whatever directory the render happened to run in"); this is the same rule applied to
/// the write beside it.
pub fn run(plane: Option<&Path>, payload: &str, ambient: &Ambient) {
    // Parsed once and shared by both branches, so the drawn turn and the recorded turn can
    // never be two readings of one payload. An unparsable payload is an empty document, as
    // Python's `except Exception: payload = {}` makes it.
    let payload: serde_json::Value =
        serde_json::from_str(payload).unwrap_or(serde_json::Value::Null);
    if let Some(plane) = plane {
        usage::record(plane, &payload);
    }
    if the_app_owns_this_surface(ambient) {
        // **Draw nothing; record anyway — and do not "clean this up".** It looks like a
        // command that has been switched off and could therefore be unwired from
        // `.claude/settings.json` altogether. It is the opposite: it is a command kept
        // running for its side effect. See the module docs and ADR 0019; this comment is here
        // because this is where the deletion would happen.
        println!();
        return;
    }
    println!("{NOT_DRAWN_YET}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_the_app(socket: &Path) -> Ambient {
        Ambient {
            stdout_is_a_tty: false,
            socket: Some(socket.to_path_buf()),
            chat: Some("7".into()),
            harness: Some(CLAUDE_CODE.into()),
        }
    }

    /// A socket with something accepting on it, which is what the app is from here.
    fn listening(dir: &Path) -> (std::os::unix::net::UnixListener, PathBuf) {
        let path = dir.join("hooks.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        (listener, path)
    }

    #[test]
    fn every_rung_has_to_hold_and_each_one_alone_is_enough_to_render() {
        let dir = tempfile::tempdir().unwrap();
        let (_held, socket) = listening(dir.path());

        assert!(the_app_owns_this_surface(&in_the_app(&socket)));

        // A human at a terminal gets the thing they asked for.
        let mut typed = in_the_app(&socket);
        typed.stdout_is_a_tty = true;
        assert!(!the_app_owns_this_surface(&typed));

        // No app started this session at all.
        let mut alone = in_the_app(&socket);
        alone.socket = None;
        assert!(!the_app_owns_this_surface(&alone));

        // A socket file left behind by an app that has gone: nothing is drawing panels.
        let mut stale = in_the_app(&socket);
        stale.socket = Some(dir.path().join("gone.sock"));
        assert!(!the_app_owns_this_surface(&stale));

        // The id was inherited, not issued: this process is not in a chat of the app's.
        for chat in [None, Some(String::new()), Some("seven".into())] {
            let mut inherited = in_the_app(&socket);
            inherited.chat = chat.clone();
            assert!(!the_app_owns_this_surface(&inherited), "{chat:?}");
        }

        // A harness with no footer has no duplicate to remove.
        for harness in [None, Some("codex".into()), Some("opencode".into())] {
            let mut other = in_the_app(&socket);
            other.harness = harness.clone();
            assert!(!the_app_owns_this_surface(&other), "{harness:?}");
        }
    }
}
