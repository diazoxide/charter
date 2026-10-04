//! `charterd` refuses a human scope to a connection whose peer runs inside a chat, even when it
//! proves the right credential (FD-27, V16a, ADR 0068 §5). Every scope on `charterd.sock` is a
//! person's, so a process a chat started is admitted as none of them: not `terminal`, not
//! `fleet-mcp`, not `approval`, and not the others either.
//!
//! These run real processes. A "chat program" is started the way a chat's is (its own session,
//! where the platform lets a test make one), and the client inside it is a separate process,
//! `charter-session-peer --present`, holding the scope's real credential, as a chat that had
//! learned it would.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use charter_session_protocol::auth::Scope;
use charter_session_protocol::link::{self, LinkError};
use charter_session_protocol::local::{Chats, Listener};
use charter_session_protocol::version::{Refused, Speaks, Version};
use tokio::net::UnixListener;

mod common;
use common::HELD;

const PEER: &str = env!("CARGO_BIN_EXE_charter-session-peer");

fn v1() -> Speaks {
    Speaks::new([Version { major: 1, minor: 0 }])
}

/// The chats a host holds, by their programs' pids.
struct Held(Vec<u32>);

impl Chats for Held {
    fn programs(&self) -> Vec<u32> {
        self.0.clone()
    }
}

struct Socket {
    _dir: tempfile::TempDir,
    path: PathBuf,
    listener: Listener,
}

fn listening() -> Socket {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("charterd.sock");
    let listener = Listener::new(UnixListener::bind(&path).unwrap());
    Socket {
        _dir: dir,
        path,
        listener,
    }
}

/// `charter-session-peer --present <scope> <socket>` with the scope's real credential, run by
/// `sh` as a child of the shell (never `exec`ed in its place), with `extra` arguments after.
fn shell_running_the_peer(scope: Scope, socket: &Path, extra: &str) -> Command {
    let mut sh = Command::new("/bin/sh");
    sh.arg("-c")
        .arg(format!(
            "\"$PEER\" --present {} \"$SOCKET\" {extra}; exit $?",
            scope.word()
        ))
        .env("PEER", PEER)
        .env("SOCKET", socket)
        .env("CHARTER_SESSION_PEER_CREDENTIAL", HELD.of(scope).expose())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    sh
}

/// The host's verdict on the one connection it accepts.
async fn served(socket: &Socket, chats: &Held) -> Result<Scope, LinkError> {
    let stream = socket.listener.accept().await.unwrap().expect("same user");
    link::serve(stream, v1(), &HELD, chats)
        .await
        .map(|link| link.scope())
}

/// Whether `child` exited 0, and what it said on stderr: waited for off the runtime, so the
/// host's side of a link it holds can still close.
async fn said(child: Child) -> (bool, String) {
    let out = tokio::task::spawn_blocking(|| child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn is_inside_a_chat(verdict: &Result<Scope, LinkError>) -> bool {
    matches!(verdict, Err(LinkError::Refused(Refused::InsideAChat(_))))
}

#[tokio::test]
async fn a_client_a_chat_started_is_refused_every_scope_even_with_its_credential() {
    for scope in Scope::WITH_A_CREDENTIAL {
        let socket = listening();
        let chat = shell_running_the_peer(scope, &socket.path, "")
            .spawn()
            .unwrap();
        let chats = Held(vec![chat.id()]);

        let verdict = served(&socket, &chats).await;

        assert!(is_inside_a_chat(&verdict), "{scope}: {verdict:?}");
        let (admitted, stderr) = said(chat).await;
        assert!(!admitted, "{scope}: the client believed it was admitted");
        assert!(
            stderr.contains("inside a chat"),
            "{scope}: the refusal says why: {stderr}"
        );
    }
}

#[tokio::test]
async fn the_same_client_outside_any_chat_is_admitted() {
    for scope in [Scope::Terminal, Scope::FleetMcp, Scope::Approval] {
        let socket = listening();
        let shell = shell_running_the_peer(scope, &socket.path, "")
            .spawn()
            .unwrap();
        // The host holds a chat, and this client is not in it.
        let chats = Held(vec![std::process::id() + 100_000]);

        let verdict = served(&socket, &chats).await;

        assert_eq!(verdict.unwrap(), scope);
        let (admitted, stderr) = said(shell).await;
        assert!(admitted, "{scope}: {stderr}");
    }
}

#[tokio::test]
async fn a_chat_program_that_is_the_client_itself_is_refused() {
    let socket = listening();
    let chat = Command::new(PEER)
        .args(["--present", "approval"])
        .arg(&socket.path)
        .env(
            "CHARTER_SESSION_PEER_CREDENTIAL",
            HELD.of(Scope::Approval).expose(),
        )
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let chats = Held(vec![chat.id()]);

    let verdict = served(&socket, &chats).await;

    assert!(is_inside_a_chat(&verdict), "{verdict:?}");
    assert!(!said(chat).await.0);
}

/// A chat's program leads its own session (`portable-pty` starts it with `setsid`). A process it
/// started that then lost its parent, so no chat is among its ancestors any more, is still in
/// the chat's session, and is refused for that. `perl`'s `POSIX::setsid` makes the session,
/// since this workspace starts no process with `unsafe`.
#[tokio::test]
async fn a_client_orphaned_from_its_chat_is_still_refused_by_its_session() {
    if !Path::new("/usr/bin/perl").exists() {
        eprintln!("no /usr/bin/perl to start a session leader with: skipped");
        return;
    }
    let socket = listening();
    let mut chat = Command::new("/usr/bin/perl")
        .args([
            "-MPOSIX",
            "-e",
            "POSIX::setsid() or die; exec @ARGV or die",
            "/bin/sh",
            "-c",
            // The client goes to the background and waits until its shell has gone, so it is
            // orphaned before it connects.
            "\"$PEER\" --present approval \"$SOCKET\" --when-orphaned-from $$ 2>\"$ERR\" & exit 0",
        ])
        .env("PEER", PEER)
        .env("SOCKET", &socket.path)
        .env("ERR", socket.path.with_extension("err"))
        .env(
            "CHARTER_SESSION_PEER_CREDENTIAL",
            HELD.of(Scope::Approval).expose(),
        )
        .spawn()
        .unwrap();
    let chats = Held(vec![chat.id()]);
    assert!(chat.wait().unwrap().success(), "the chat's shell ended");

    let verdict = tokio::time::timeout(std::time::Duration::from_secs(20), served(&socket, &chats))
        .await
        .expect("the orphan connected");

    assert!(is_inside_a_chat(&verdict), "{verdict:?}");
}
