//! A permission hook on the hook channel (HP-6): one line that asks, held open until the
//! operator answers in the window, and one line back naming the option they chose.
//!
//! **The only line on this socket that waits.** Every other hook writes and goes; this one is
//! the harness's own permission prompt, and the harness waits on it anyway. The host holds it
//! at most until the ask's deadline, which sits below the hook's timeout
//! ([`crate::harness::hooked::HOOK_TIMEOUT`]), and answers `{"chosen":null}` then, so the
//! harness's prompt decides in the pane. A hook that goes away first, because the operator
//! answered in the pane and the harness stopped it, withdraws its ask.
//!
//! **Who answers, on each end.** The host side reads an ask from a chat's hook and never an
//! answer: what the host writes back is the option the operator chose in the window
//! ([`crate::harness::asks`]). The hook side believes that reply only from the host itself:
//! before it writes anything, [`ask_permission`] checks that the process listening is this
//! user's and one of the hook's own ancestors ([`purlis_same_user::admit_host`]), so a process
//! a chat started that bound the socket's path is told nothing and never heard.

use std::io;
use std::sync::mpsc;
use std::time::Duration;

use crate::harness::hooked::Source;
use crate::harness::model::Choice;

/// A permission hook's ask, as the hook hands it to the host: the chat it ran in, its harness,
/// and the harness's payload as it came. The host reads the ask from the payload itself
/// ([`crate::harness::hooked::ask`]), with its own hook timeout, so nothing the chat says sets
/// its deadline or its options.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PermissionAsked {
    /// The app's number for the chat the hook ran in, from [`super::CHAT_ENV`].
    pub chat: u32,
    /// The harness whose permission hook this is.
    pub permission_request: Source,
    /// The hook's payload, as the harness wrote it on stdin.
    pub payload: serde_json::Value,
}

/// What the host writes back: the option chosen, or none, and then the harness decides.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Chosen {
    pub chosen: Option<String>,
}

/// How a held ask ended without an answer, which the host is told so it stops holding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    /// The hook went away: the harness answered in its pane, or stopped.
    Closed,
    /// The deadline passed. The hook is told no option, and the harness's prompt decides.
    TimedOut,
}

/// An ask the host holds: where its answer arrives (the option chosen), how long it may wait,
/// and what to tell the host when it ends unanswered. Told only once, and never after an answer.
pub struct Waiting {
    pub answered: mpsc::Receiver<Choice>,
    pub until: Duration,
    pub left: Box<dyn FnOnce(Left) + Send + 'static>,
}

/// What hears a [`PermissionAsked`]: the ask held, or `None` when the host will not hold it (too
/// many, too big, a chat it does not know), and then the hook is told no option at once.
pub type Permitting = Box<dyn Fn(PermissionAsked) -> Option<Waiting> + Send + Sync + 'static>;

/// The hearer that holds each [`PermissionAsked`] in `hooks`, under the chat's number, until it
/// is answered there, its hook goes away, or its deadline passes. `changed` is told each time
/// the asks held there change: raised, withdrawn or timed out. An ask `hooks` will not raise,
/// or one with no deadline, is told no option at once, and its harness's prompt decides.
pub fn held_in(
    hooks: std::sync::Arc<crate::harness::hooked::HookAsks>,
    changed: std::sync::Arc<dyn Fn() + Send + Sync + 'static>,
) -> Permitting {
    Box::new(move |asked: PermissionAsked| {
        let held = hooks
            .raise(
                &asked.chat.to_string(),
                asked.permission_request,
                &asked.payload,
                std::time::Instant::now(),
            )
            .map_err(|why| {
                tracing::warn!(
                    "purlis: chat {}'s permission ask was left to its pane: {why}",
                    asked.chat
                );
            })
            .ok()?;
        let id = held.raised.id.clone();
        // A hook's ask always has a deadline; one without would hold a hook past the harness's
        // patience, so it is let go at once.
        let Some(until) = held.raised.ask.deadline.duration() else {
            hooks.withdraw(&id);
            return None;
        };
        changed();
        let (hooks, changed) = (
            std::sync::Arc::clone(&hooks),
            std::sync::Arc::clone(&changed),
        );
        Some(Waiting {
            answered: held.answered,
            until,
            left: Box::new(move |left| {
                match left {
                    Left::Closed => {
                        hooks.withdraw(&id);
                    }
                    // Expired as every ask past its deadline is; one this clock does not yet
                    // see as late is let go all the same, since its hook was told no option.
                    Left::TimedOut => {
                        if !hooks.time_out(std::time::Instant::now()).contains(&id) {
                            hooks.withdraw(&id);
                        }
                    }
                }
                changed();
            }),
        })
    })
}

/// How often a held connection is looked at for its hook having gone.
#[cfg(unix)]
const LOOKED_AT_EVERY: Duration = Duration::from_millis(100);

/// The most a host's reply may be: an option's id.
#[cfg(unix)]
const A_REPLY_IS_AT_MOST: u64 = 4096;

/// Hands `asked` to the host at `path` and waits at most `within` for the option chosen.
/// `Ok(None)` when the host chose none; an error when it could not be reached or did not
/// reply in time. Either way the hook prints nothing and the harness's prompt decides.
#[cfg(unix)]
pub fn ask_permission(
    path: &std::path::Path,
    token: Option<&super::ChatToken>,
    asked: &PermissionAsked,
    within: Duration,
) -> io::Result<Option<String>> {
    let socket = std::os::unix::net::UnixStream::connect(path)?;
    // **The host is checked before a byte is written** (D-88n, FD-6's mutual check turned
    // round): the process listening must be this user's, one this hook was started under, and
    // hold the socket at the path now, so a pid a dead listener left behind admits nothing.
    // A process beside or below the hook (anything a chat runs) that bound the socket's path
    // is never the host, so it is told nothing, not the payload and not the chat's token, and
    // whatever it would answer is never read. No secret could do this: the hook's environment
    // and arguments are the chat's to read.
    purlis_same_user::admit_host(&socket, path, crate::forklock::output).map_err(|why| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("the hook channel's listener is not the host that started this chat: {why}"),
        )
    })?;
    exchange(socket, token, asked, within)
}

/// [`ask_permission`]'s exchange, on a connection to a host the caller has ALREADY admitted.
///
/// **Only for a test playing a hook in the host's own process**, which no ancestry check could
/// admit, so it is built only for tests (`test-support`). A hook always goes through
/// [`ask_permission`], whose check is the whole of what makes its reply believable.
#[cfg(all(unix, any(test, feature = "test-support")))]
pub fn ask_permission_of_an_admitted_host(
    socket: std::os::unix::net::UnixStream,
    token: Option<&super::ChatToken>,
    asked: &PermissionAsked,
    within: Duration,
) -> io::Result<Option<String>> {
    exchange(socket, token, asked, within)
}

/// One ask written and its reply read, on a connection whose host is already admitted.
#[cfg(unix)]
fn exchange(
    socket: std::os::unix::net::UnixStream,
    token: Option<&super::ChatToken>,
    asked: &PermissionAsked,
    within: Duration,
) -> io::Result<Option<String>> {
    use std::io::{BufRead, Read, Write};

    let line = super::line_with(token, asked)?;
    socket.set_write_timeout(Some(within))?;
    socket.set_read_timeout(Some(within))?;
    (&socket).write_all(&line)?;
    let mut said = String::new();
    std::io::BufReader::new(&socket)
        .take(A_REPLY_IS_AT_MOST)
        .read_line(&mut said)?;
    let chosen: Chosen = serde_json::from_str(&said).map_err(io::Error::other)?;
    Ok(chosen.chosen)
}

/// Where there is no unix socket there is no host to ask, and the harness's prompt decides.
#[cfg(not(unix))]
pub fn ask_permission(
    _path: &std::path::Path,
    _token: Option<&super::ChatToken>,
    _asked: &PermissionAsked,
    _within: Duration,
) -> io::Result<Option<String>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the hook channel is a unix socket",
    ))
}

/// Holds one connection's ask until it is answered, its hook goes away, or its deadline passes,
/// and writes the hook what was chosen.
#[cfg(unix)]
pub(super) fn hold(
    reader: &mut std::io::BufReader<super::Deadlined>,
    writer: &mut std::os::unix::net::UnixStream,
    waiting: Option<Waiting>,
) {
    use std::io::{BufRead, Write};

    let reply = |writer: &mut std::os::unix::net::UnixStream, chosen: Option<String>| {
        if let Ok(mut said) = serde_json::to_vec(&Chosen { chosen }) {
            said.push(b'\n');
            let _ = writer.write_all(&said);
        }
    };
    let Some(waiting) = waiting else {
        reply(writer, None);
        return;
    };
    let began = std::time::Instant::now();
    let _ = reader
        .get_ref()
        .socket()
        .set_read_timeout(Some(LOOKED_AT_EVERY));
    let ended = loop {
        match waiting.answered.recv_timeout(LOOKED_AT_EVERY) {
            Ok(chosen) => {
                reply(writer, Some(chosen.id));
                return;
            }
            // The host let go of it: nothing will answer.
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                reply(writer, None);
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if began.elapsed() >= waiting.until {
            break Left::TimedOut;
        }
        // A hook writes one line and then only waits, so anything readable now is its end.
        match reader.fill_buf() {
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            _ => break Left::Closed,
        }
    };
    (waiting.left)(ended);
    // An answer that applied before the host stopped holding the ask is still the answer.
    reply(
        writer,
        waiting.answered.try_recv().ok().map(|chosen| chosen.id),
    );
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::{Hearing, Listener};
    use super::*;
    use crate::harness::asks::{Admitted, Answerer, Asks};
    use crate::harness::hooked::HookAsks;
    use std::sync::Arc;
    use std::time::Instant;

    fn asked(chat: u32) -> PermissionAsked {
        PermissionAsked {
            chat,
            permission_request: Source::ClaudeCode,
            payload: serde_json::json!({"tool_name": "Bash",
                "tool_input": {"command": "npm test"}}),
        }
    }

    /// A host whose permission hearer holds each ask in `hooks`, its deadline cut to `until`.
    fn hearing(hooks: Arc<HookAsks>, until: Duration) -> Hearing {
        let held = held_in(hooks, Arc::new(|| {}));
        Hearing {
            secret_exec: Box::new(|_, _, writer| crate::secrets::brokered::not_answered(writer)),
            each: Box::new(|_| Ok(())),
            answer: Box::new(|_, _| super::super::Answer::No { why: String::new() }),
            noticed: Box::new(|_| {}),
            saved: Box::new(|_| {}),
            refused: Box::new(|_| Ok(())),
            tool: Box::new(|_| Ok(())),
            blocked: Box::new(|_| {}),
            touching: Box::new(|_| {}),
            permission: Box::new(move |asked| {
                held(asked).map(|waiting| Waiting {
                    until: waiting.until.min(until),
                    ..waiting
                })
            }),
        }
    }

    fn pending_soon(hooks: &HookAsks, n: usize) -> Vec<crate::harness::asks::Raised> {
        let began = Instant::now();
        loop {
            let pending = hooks.pending(Instant::now());
            if pending.len() == n || began.elapsed() > Duration::from_secs(5) {
                return pending;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn the_window_s_answer_goes_back_on_the_hook_that_asked() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(3).expect("a token");
        let hooks = Arc::new(HookAsks::new(Arc::new(Asks::new())));
        let _reading = listener.hear(hearing(Arc::clone(&hooks), Duration::from_secs(30)));

        let hook = {
            let path = path.clone();
            std::thread::spawn(move || {
                ask_permission_of_an_admitted_host(
                    std::os::unix::net::UnixStream::connect(&path).expect("connects"),
                    Some(&token),
                    &asked(3),
                    Duration::from_secs(30),
                )
            })
        };
        let pending = pending_soon(&hooks, 1);
        let window = Answerer::admitted(Admitted::LocalUi).expect("the window");
        hooks
            .answer("3", &pending[0].id, "allow", window, Instant::now())
            .expect("applies");

        assert_eq!(
            hook.join().expect("the hook").expect("a reply"),
            Some("allow".to_owned())
        );
    }

    #[test]
    fn a_hook_that_goes_away_withdraws_its_ask() {
        use std::io::Write;

        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(3).expect("a token");
        let hooks = Arc::new(HookAsks::new(Arc::new(Asks::new())));
        let _reading = listener.hear(hearing(Arc::clone(&hooks), Duration::from_secs(30)));

        let mut socket = std::os::unix::net::UnixStream::connect(&path).expect("connects");
        socket
            .write_all(&super::super::line_with(Some(&token), &asked(3)).expect("a line"))
            .expect("written");
        assert_eq!(pending_soon(&hooks, 1).len(), 1);
        drop(socket);

        assert!(pending_soon(&hooks, 0).is_empty(), "withdrawn");
    }

    #[test]
    fn past_its_deadline_the_hook_is_told_no_option_and_the_harness_decides() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(3).expect("a token");
        let hooks = Arc::new(HookAsks::new(Arc::new(Asks::new())));
        let _reading = listener.hear(hearing(Arc::clone(&hooks), Duration::from_millis(300)));

        let chosen = ask_permission_of_an_admitted_host(
            std::os::unix::net::UnixStream::connect(&path).expect("connects"),
            Some(&token),
            &asked(3),
            Duration::from_secs(10),
        );

        assert_eq!(chosen.expect("a reply"), None);
        assert!(hooks.pending(Instant::now()).is_empty());
    }

    #[test]
    fn a_line_without_its_chat_s_token_raises_nothing() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let _token = listener.tokens().issue_to_this_process(3).expect("a token");
        let hooks = Arc::new(HookAsks::new(Arc::new(Asks::new())));
        let _reading = listener.hear(hearing(Arc::clone(&hooks), Duration::from_secs(30)));

        let chosen = ask_permission_of_an_admitted_host(
            std::os::unix::net::UnixStream::connect(&path).expect("connects"),
            None,
            &asked(3),
            Duration::from_secs(5),
        );

        assert!(chosen.is_err(), "dropped unread: {chosen:?}");
        assert!(hooks.pending(Instant::now()).is_empty());
    }

    #[test]
    fn a_host_that_will_not_hold_the_ask_says_no_option_at_once() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        let listener = Listener::bind(dir.path(), &path).expect("a socket");
        let token = listener.tokens().issue_to_this_process(3).expect("a token");
        let _reading = listener.each(Box::new(|_| {}));

        let chosen = ask_permission_of_an_admitted_host(
            std::os::unix::net::UnixStream::connect(&path).expect("connects"),
            Some(&token),
            &asked(3),
            Duration::from_secs(5),
        );

        assert_eq!(chosen.expect("a reply"), None);
    }
}
