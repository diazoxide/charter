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
//! # The transposition is a DEFAULT now, not a law (charter ADR 0029)
//!
//! That judgement was made by a port and never decided for the app, and one thing about the
//! app breaks the ADR's premise rather than carrying it over. **A frame held one harness, and
//! this window holds fifty.** In a frame, the panels and the suppressed footer described the
//! same session, so the second one really was a duplicate. Here the panels describe the
//! FOCUSED workspace, and a chat's footer describes the workspace that CHAT resolves to —
//! which, for any chat that is not the focused one, is not the same fact at all.
//!
//! **What the choice is between.** Not "charter's footer or the harness's": this command IS
//! Claude Code's `statusLine`, so that line is charter's to fill or to leave empty, and ADR
//! 0019 measured what the empty one costs — a suppressed session "has no context/cache gauge
//! on any surface". The choice is between charter's footer and nothing.
//!
//! So the operator decides, per chat. The default is unchanged — blank, exactly as this
//! module has always behaved, so nobody's pane moves on an upgrade — and a chat started with
//! [`charter_core::start::FOOTER_ENV`] set to `show` draws the footer while every other chat
//! in the window is untouched.
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
//! # What this draws, and where the seam is
//!
//! `charter/statusline.py:render` is not a renderer with some dependencies. It is charter's
//! whole read surface: the nine-rung workspace resolution, a `git status` per clone, linked
//! worktrees drawn as rows, persona chips with vault health and memory counts, the alert
//! list, the session strip, the update-freshness cache, and `charter/tui.py`'s column
//! algebra under all of it.
//!
//! M2.7 drew none of it and said so in one sentence. **M2.18 draws the frame and the identity
//! row** — the workspace, its reinit tip, its open todos, its pieces and how many other
//! workspaces there are — and says, in the body, which surfaces it still does not draw. The
//! seam is charter's own zone rule, and [`charter_core::footer`] is where the argument for it
//! lives. What matters here is the rule it follows: a footer that silently omitted the alert
//! row would be worse than a sentence, because an operator reads a footer to find out whether
//! anything needs them.
//!
//! The record is still kept on both branches, because it is the half the app's own path
//! needs: inside the app the footer is blank either way, and what would otherwise be lost is
//! the history.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use charter_core::hookwire::{CHAT_ENV, SOCKET_ENV};
use charter_core::start::{FOOTER_ENV, FOOTER_SHOW};
use charter_core::{footer, tui, usage};

/// The registry's name for Claude Code, as `$CHARTER_HARNESS` carries it.
const CLAUDE_CODE: &str = "claude-code";

/// What the operator's `$CHARTER_HARNESS` says, so a test can drive the rungs.
pub struct Ambient {
    pub stdout_is_a_tty: bool,
    pub socket: Option<PathBuf>,
    pub chat: Option<String>,
    pub harness: Option<String>,
    /// `$CHARTER_FOOTER`: what THIS chat was started asking for (charter ADR 0029).
    pub footer: Option<String>,
}

impl Ambient {
    /// This process's own environment.
    pub fn here() -> Self {
        Self {
            stdout_is_a_tty: std::io::stdout().is_terminal(),
            socket: std::env::var_os(SOCKET_ENV).map(PathBuf::from),
            chat: std::env::var(CHAT_ENV).ok(),
            harness: std::env::var("CHARTER_HARNESS").ok(),
            footer: std::env::var(FOOTER_ENV).ok(),
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
/// **And one rung that is not ADR 0019's: the chat may have said no.** Charter ADR 0029 turns
/// the blanking into a DEFAULT. A chat started with `$CHARTER_FOOTER` set to `show` draws the
/// footer, and every other chat in the same window is untouched. It is asked FIRST because it
/// is the cheapest question here — one environment lookup against a socket `connect` — and
/// because an operator who has said "draw it" is owed the same answer whether or not the
/// app's socket happens to be up at this instant.
///
/// Never fails. Everything it touches is ambient.
pub fn the_app_owns_this_surface(ambient: &Ambient) -> bool {
    if ambient.footer.as_deref() == Some(FOOTER_SHOW) {
        return false;
    }
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
    let Some(chat) = ambient.chat.as_deref() else {
        return false;
    };
    if chat.parse::<u32>().is_err() {
        return false;
    }
    ambient.harness.as_deref() == Some(CLAUDE_CODE)
}

/// What this prints when it is not suppressed and there is **no plane** to draw.
///
/// Coloured unconditionally, as every line `charter/statusline.py` prints is: the surface is
/// Claude Code's footer, which is a terminal, and the module's own fallback
/// (`f"{_CYAN}⬢{_R} charter"`) does not consult `$NO_COLOR` either.
///
/// **A declared divergence, and the same one [`run`] already declares about the record.**
/// charter falls back to `<cwd>/.charter` outside a plane and renders a footer for whatever
/// directory it happened to run in — a workspace called `default`, no repos, no personas.
/// That footer describes nothing, and drawing it here would mean resolving a plane out of a
/// directory that is not one. Saying so in a line is the honest half of the same decision.
pub const NO_PLANE: &str =
    "\x1b[36m⬢\x1b[0m charter — not standing in a control plane, so there is none to draw";

/// `charter statusline`. Always succeeds; a footer is not worth a non-zero exit.
///
/// `plane` is `None` when this process is not standing in one. **Then nothing is recorded**,
/// and that is a declared divergence from Python, which falls back to `<cwd>/.charter` and so
/// scatters a session's history into whatever directory the render happened to run in.
/// `glstate.maybe_spawn` already refuses to spawn for exactly that reason ("outside a plane
/// `config.STATE_DIR` is `<cwd>/.charter`, so a spawn here would scatter charter's caches
/// into whatever directory the render happened to run in"); this is the same rule applied to
/// the write beside it.
pub fn run(
    plane: Option<&Path>,
    payload: &str,
    ambient: &Ambient,
    now: chrono::DateTime<chrono::Utc>,
) {
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
    let Some(plane) = plane else {
        println!("{NO_PLANE}");
        return;
    };
    // `print(out)` where `out` already ends on a newline — so the footer is followed by a
    // blank line, exactly as charter's is. Claude Code reads the block; the blank line is
    // charter's and copying it is what makes the two outputs comparable at all.
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    println!(
        "{}",
        footer::render(
            plane,
            &payload,
            &footer::Ambient {
                env: &tui::ambient,
                cwd: &cwd,
                now,
            },
        )
    );
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
            // The default: a chat that asked for nothing. Charter sets this variable only
            // for a chat that did, so absence is what the app's chats ordinarily carry.
            footer: None,
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

    #[test]
    fn a_chat_that_asked_for_charters_footer_keeps_it_while_every_rung_holds() {
        // Charter ADR 0029. The point of the rung is that it wins against a situation in
        // which the app would otherwise blank: every other rung here says "in the app".
        let dir = tempfile::tempdir().unwrap();
        let (_held, socket) = listening(dir.path());

        let mut asked = in_the_app(&socket);
        asked.footer = Some(FOOTER_SHOW.into());

        assert!(!the_app_owns_this_surface(&asked));
    }

    #[test]
    fn only_the_word_charter_writes_draws_the_footer_and_everything_else_is_the_default() {
        // The variable is charter's own and it is set to one word. A value that is not that
        // word is a value charter did not write — inherited, stale, or hand-edited — and the
        // honest answer to it is the default, not a surface the operator never chose.
        let dir = tempfile::tempdir().unwrap();
        let (_held, socket) = listening(dir.path());

        for said in [
            None,
            Some(String::new()),
            Some("SHOW".into()),
            Some("show ".into()),
            Some("true".into()),
            Some("1".into()),
            Some("blank".into()),
        ] {
            let mut other = in_the_app(&socket);
            other.footer = said.clone();
            assert!(the_app_owns_this_surface(&other), "{said:?}");
        }
    }
}
