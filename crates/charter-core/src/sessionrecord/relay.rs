//! **A saved record's line, passed on by the chat's next `Stop` hook** (ADR 0064, amended
//! 2026-09-28; charter#517).
//!
//! `charter session record` tells the app a record is saved by writing one line to the chat's
//! hook socket, and that line is what closes a smart-closed tab. A harness can run the command
//! in a sandbox that lets it write the record and refuses the connect: Codex's default
//! `workspace-write` does exactly that (measured on codex-cli 0.147.0). The chat's hooks run
//! outside that sandbox — the harness runs them itself — so when the line does not get through
//! the command leaves it here ([`leave`]), in the chat's own directory where the sandbox lets it
//! write, and the `Stop` that ends the same turn sends it ([`take`]).
//!
//! **Still one writer, and still a command's signal.** The line the hook sends is the one the
//! command would have sent, built from what the command wrote down; nothing is read from what
//! the harness printed. The app closes a tab on it only while that chat is being smart-closed,
//! so the same line arriving twice closes the tab once.
//!
//! **Never an earlier session's.** A chat's number is used again when a plane is opened again,
//! so a marker is passed on only by the chat that left it: the same number, the same
//! conversation where the command knew it, and within [`PASSED_ON_WITHIN`] — the time the app
//! waits for a record before it gives a smart close up. Anything else is removed unsent.
//!
//! Where it lives is `docs/plane-format.md`'s
//! "`.charter/sessions/<chat>.saved` and `workspaces/<ws>/.charter/sessions/<chat>.saved`".

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::active::Place;
use crate::hookwire::{Conversation, SessionSaved};

/// How old a marker may be and still be passed on: the time the app gives a smart close before
/// it gives up on the record (the app's `smartclose::GIVES_UP_AFTER` is this constant), since a
/// line after that closes nothing.
pub const PASSED_ON_WITHIN: Duration = Duration::from_secs(5 * 60);

/// The largest marker read. One is a number, an id and a path: well under a kilobyte.
const MOST_BYTES: u64 = 8 * 1024;

/// What a marker holds: the line to send, and what ties it to the chat that left it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Marker {
    /// The app's number for the chat that wrote the record.
    pub chat: u32,
    /// The chat's conversation as the app last recorded it, where it had one.
    pub conversation: Option<String>,
    /// The record, absolute — the line's own field.
    pub session_saved: PathBuf,
    /// When it was left, in seconds since the Unix epoch.
    pub at: u64,
}

/// Where chat `chat`'s marker is for a chat working in `place`: `.charter/sessions/<chat>.saved`
/// in the place's directory — the plane root, or `workspaces/<ws>`. `None` for a workspace name
/// that cannot be one.
///
/// **The place's directory, because it is the chat's own.** A sandbox lets a command write where
/// the chat works, and the record itself goes below the same directory; the plane's own
/// `.charter/` is outside it for a chat in a workspace (measured, codex-cli 0.147.0).
pub fn marker(root: &Path, place: &Place, chat: u32) -> Option<PathBuf> {
    let dir = match place {
        Place::PlaneRoot => root.to_path_buf(),
        Place::Workspace(ws) => {
            if !crate::contain::workspace_name_ok(ws) {
                return None;
            }
            root.join("workspaces").join(ws)
        }
    };
    Some(
        dir.join(".charter")
            .join("sessions")
            .join(format!("{chat}.saved")),
    )
}

/// Leaves `left` for chat `left.chat`'s next `Stop` to pass on, in `place`'s directory: 0600,
/// written whole, its directories made 0700. Answers where.
pub fn leave(root: &Path, place: &Place, left: &Marker) -> io::Result<PathBuf> {
    let path = marker(root, place, left.chat)
        .ok_or_else(|| io::Error::other("that workspace name cannot be one"))?;
    if let Some(dir) = path.parent() {
        private_dirs(dir)?;
    }
    let mut bytes = serde_json::to_vec(left).map_err(io::Error::other)?;
    bytes.push(b'\n');
    crate::rewrite::replace(root, &path, &bytes, crate::rewrite::Mode::Private)?;
    Ok(path)
}

/// What chat `chat`'s `Stop` in conversation `conversation` passes on, at `now` (Unix seconds):
/// the line its marker holds, when the marker is this chat's, this conversation's and fresh.
/// The marker is removed whatever it holds, so a line is passed on at most once.
pub fn take(
    root: &Path,
    place: &Place,
    chat: u32,
    conversation: &Conversation,
    now: u64,
) -> Option<SessionSaved> {
    let path = marker(root, place, chat)?;
    // `symlink_metadata`: a link is not followed, only removed — a marker is a file charter
    // wrote, and a link in its place is somebody else's.
    let found = std::fs::symlink_metadata(&path).ok()?;
    let text = (found.is_file() && found.len() <= MOST_BYTES)
        .then(|| crate::contain::no_link_on_the_way(root, &path).ok())
        .flatten()
        .and_then(|()| std::fs::read_to_string(&path).ok());
    let _ = std::fs::remove_file(&path);
    let left: Marker = serde_json::from_str(text?.trim()).ok()?;
    let ours = left.chat == chat
        && left.session_saved.is_absolute()
        && left
            .conversation
            .as_ref()
            .is_none_or(|id| *conversation == Conversation::Named(id.clone()));
    let fresh = now
        .checked_sub(left.at)
        .is_some_and(|age| age <= PASSED_ON_WITHIN.as_secs());
    (ours && fresh).then_some(SessionSaved {
        chat,
        session_saved: left.session_saved,
    })
}

/// `dir` and every directory it needs, each made 0700 as it is made (Unix).
fn private_dirs(dir: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: u64 = 1_790_000_000;

    fn plane() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("workspaces/alpha")).unwrap();
        tmp
    }

    fn alpha() -> Place {
        Place::Workspace("alpha".to_owned())
    }

    fn left(root: &Path, conversation: Option<&str>) -> Marker {
        Marker {
            chat: 3,
            conversation: conversation.map(str::to_owned),
            session_saved: root.join("workspaces/alpha/sessions/20260928-140312-ship.md"),
            at: AT,
        }
    }

    fn named(id: &str) -> Conversation {
        Conversation::Named(id.to_owned())
    }

    #[test]
    fn a_marker_lives_in_the_directory_of_the_place_the_chat_works() {
        let root = Path::new("/p");
        assert_eq!(
            marker(root, &alpha(), 3),
            Some(PathBuf::from(
                "/p/workspaces/alpha/.charter/sessions/3.saved"
            ))
        );
        assert_eq!(
            marker(root, &Place::PlaneRoot, 12),
            Some(PathBuf::from("/p/.charter/sessions/12.saved"))
        );
        assert_eq!(marker(root, &Place::Workspace("../x".into()), 3), None);
    }

    #[test]
    fn the_stop_of_the_chat_that_left_it_passes_the_line_on_once_and_removes_it() {
        let tmp = plane();
        let root = tmp.path();
        let path = leave(root, &alpha(), &left(root, Some("c-1"))).expect("left");
        assert!(path.is_file(), "the marker was not written");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "the marker is {mode:o}");
            let dir = path.parent().unwrap();
            let mode = std::fs::metadata(dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "the marker's directory is {mode:o}");
        }

        let sent = take(root, &alpha(), 3, &named("c-1"), AT + 5).expect("the line");

        assert_eq!(sent.chat, 3);
        assert_eq!(
            sent.session_saved,
            root.join("workspaces/alpha/sessions/20260928-140312-ship.md")
        );
        assert!(!path.exists(), "the marker was left behind");
        assert_eq!(take(root, &alpha(), 3, &named("c-1"), AT + 6), None);
    }

    #[test]
    fn a_marker_with_no_conversation_is_passed_on_by_its_chat_in_any_conversation() {
        let tmp = plane();
        let root = tmp.path();
        leave(root, &Place::PlaneRoot, &left(root, None)).unwrap();
        assert!(take(root, &Place::PlaneRoot, 3, &Conversation::Unknown, AT).is_some());
    }

    #[test]
    fn another_conversations_stop_removes_the_marker_and_sends_nothing() {
        let tmp = plane();
        let root = tmp.path();
        let path = leave(root, &alpha(), &left(root, Some("c-1"))).unwrap();
        assert_eq!(take(root, &alpha(), 3, &named("c-2"), AT + 1), None);
        assert!(!path.exists(), "a marker nobody can pass on was kept");
    }

    #[test]
    fn a_marker_older_than_the_apps_wait_or_from_the_future_is_stale() {
        let tmp = plane();
        let root = tmp.path();
        let wait = PASSED_ON_WITHIN.as_secs();
        leave(root, &alpha(), &left(root, None)).unwrap();
        assert_eq!(take(root, &alpha(), 3, &named("c"), AT + wait + 1), None);
        leave(root, &alpha(), &left(root, None)).unwrap();
        assert_eq!(take(root, &alpha(), 3, &named("c"), AT - 1), None);
        leave(root, &alpha(), &left(root, None)).unwrap();
        assert!(take(root, &alpha(), 3, &named("c"), AT + wait).is_some());
    }

    #[test]
    fn another_chats_marker_is_never_read() {
        let tmp = plane();
        let root = tmp.path();
        let path = leave(root, &alpha(), &left(root, None)).unwrap();
        assert_eq!(take(root, &alpha(), 4, &named("c"), AT), None);
        assert!(path.exists(), "chat 4 touched chat 3's marker");
    }

    #[test]
    fn a_marker_that_is_not_one_is_removed_and_sends_nothing() {
        let tmp = plane();
        let root = tmp.path();
        let path = marker(root, &alpha(), 3).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for bad in [
            "not json".to_owned(),
            // A marker whose own number is another chat's.
            serde_json::to_string(&Marker {
                chat: 9,
                ..left(root, None)
            })
            .unwrap(),
            // A record that is not an absolute path.
            serde_json::to_string(&Marker {
                session_saved: PathBuf::from("sessions/x.md"),
                ..left(root, None)
            })
            .unwrap(),
            "x".repeat(10 * 1024),
        ] {
            std::fs::write(&path, bad).unwrap();
            assert_eq!(take(root, &alpha(), 3, &named("c"), AT), None);
            assert!(!path.exists(), "a bad marker was kept");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_marker_that_is_a_link_is_not_followed() {
        let tmp = plane();
        let root = tmp.path();
        let elsewhere = tmp.path().join("elsewhere.json");
        std::fs::write(
            &elsewhere,
            serde_json::to_string(&left(root, None)).unwrap(),
        )
        .unwrap();
        let path = marker(root, &alpha(), 3).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
        assert_eq!(take(root, &alpha(), 3, &named("c"), AT), None);
        assert!(elsewhere.exists(), "the link's target was touched");
    }
}
