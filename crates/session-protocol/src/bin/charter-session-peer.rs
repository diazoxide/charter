//! A far end of the session protocol on this process's own stdin and stdout.
//!
//! It is what the tests reach through a child's stdio, the transport a connector gives (ADR
//! 0078 §2), and it is a separate process on purpose: ADR 0068 §4 holds that a protocol only
//! the app has ever spoken is not yet public. Two modes:
//!
//! - `--speaks 2.1,1.3`: a host that answers the negotiation with those versions (major 2 at
//!   minor 1, major 1 at minor 3), and exits 0 when a version was agreed and 1, saying why on
//!   stderr, when it was not.
//! - `--flood <streams> <bytes>`: a hostile client. It negotiates 1.0, opens the control lane,
//!   then opens `<streams>` more streams with a multiplexer that sets no limits of its own and
//!   writes `<bytes>` to each, and holds them until its stdin closes. The tests run it so that
//!   what the host holds can be measured in a process that holds nothing else. It is admitted
//!   as `local-ui` with the credential in `$CHARTER_SESSION_PEER_CREDENTIAL`, because a
//!   hostile client the host has admitted is the one whose limits matter: one it has not
//!   admitted never reaches the multiplexer.
//! - `--present <scope> <socket> [--when-orphaned-from <pid>]` (unix): a client that connects to
//!   a host's socket and asks to be `<scope>` with the credential in
//!   `$CHARTER_SESSION_PEER_CREDENTIAL`. It exits 0 when admitted and 1, saying why on stderr,
//!   when not, once the host has closed the link. With `--when-orphaned-from`, it first waits (up to 20 s) until its parent is no
//!   longer `<pid>`, so it connects as an orphan. The tests run it inside a "chat" to show
//!   such a client is refused whatever credential it holds (FD-27).
//! - `--remote-link <chat> <build>` (only with the `any-stream` feature, as the crate's tests
//!   build it): a stub `remote-link` client on stdio, as a device whose link a handshake has
//!   already proved (V7, FD-27). It lists, attaches `<chat>` and reads its snapshot and first
//!   live bytes, tries to answer an ask, tries the UI RPC as `<build>` for a vault's reveal and a
//!   settings write, and stops the chat, saying what came of each on stderr, one line each.

use std::process::ExitCode;

use futures::AsyncWriteExt;
use purlis_session_protocol::auth::{self, Credential, Scope};
use purlis_session_protocol::link::CONTROL_LANE;
use purlis_session_protocol::version::{Speaks, Version, answer, offer};
use tokio_util::compat::TokioAsyncReadCompatExt;

fn speaks(arg: &str) -> Option<Speaks> {
    let versions: Option<Vec<Version>> = arg
        .split(',')
        .map(|v| {
            let (major, minor) = v.split_once('.')?;
            Some(Version {
                major: major.parse().ok()?,
                minor: minor.parse().ok()?,
            })
        })
        .collect();
    versions.map(Speaks::new)
}

/// Where `--flood` finds the `local-ui` credential it presents.
const CREDENTIAL_ENV: &str = "CHARTER_SESSION_PEER_CREDENTIAL";

const USAGE: &str = "usage: charter-session-peer --speaks <major>.<minor>[,…] | --flood <streams> \
     <bytes> | --present <scope> <socket> [--when-orphaned-from <pid>]";

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdio = tokio::io::join(tokio::io::stdin(), tokio::io::stdout());
    match args.as_slice() {
        [flag, value] if flag == "--speaks" => match speaks(value) {
            Some(ours) => host(stdio, ours).await,
            None => usage(),
        },
        [flag, streams, bytes] if flag == "--flood" => match (streams.parse(), bytes.parse()) {
            (Ok(streams), Ok(bytes)) => flood(stdio, streams, bytes).await,
            _ => usage(),
        },
        #[cfg(unix)]
        [flag, scope, socket, rest @ ..] if flag == "--present" => {
            let orphaned_from = match rest {
                [] => None,
                [flag, pid] if flag == "--when-orphaned-from" => match pid.parse() {
                    Ok(pid) => Some(pid),
                    Err(_) => return usage(),
                },
                _ => return usage(),
            };
            match Scope::from_word(scope) {
                Some(scope) => present(scope, socket, orphaned_from).await,
                None => usage(),
            }
        }
        #[cfg(feature = "any-stream")]
        [flag, chat, build] if flag == "--remote-link" => remote_link(stdio, chat, build).await,
        _ => usage(),
    }
}

/// What came of a refused call, as one word: its code.
#[cfg(feature = "any-stream")]
fn refused(error: &purlis_session_protocol::session::CallError) -> String {
    error
        .refusal()
        .map_or_else(|| format!("failed: {error}"), |r| r.code.clone())
}

#[cfg(feature = "any-stream")]
async fn remote_link<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static>(
    stdio: S,
    chat: &str,
    build: &str,
) -> ExitCode {
    use purlis_session_protocol::session::{Client, speaks};
    use purlis_session_protocol::view::{Chunk, Viewer};

    let link = match purlis_session_protocol::link::connect_as_a_device(stdio, speaks()).await {
        Ok(link) => link,
        Err(e) => {
            eprintln!("no link: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (client, mut views, _events) = Client::new(link);
    match client.list().await {
        Ok(chats) => {
            for listed in chats {
                eprintln!("listed {} {}", listed.chat, listed.state);
            }
        }
        Err(e) => eprintln!("list refused {}", refused(&e)),
    }
    match client.attach(chat).await {
        Ok(_) => {
            let stream = match views.accept().await {
                Ok(stream) => stream,
                Err(e) => {
                    eprintln!("no view: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let mut reader = match Viewer::default().accept(stream).await {
                Ok(reader) => reader,
                Err(e) => {
                    eprintln!("no view: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let mut snapshot = Vec::new();
            loop {
                match reader.next().await {
                    Some(Ok(Chunk::Snapshot(bytes))) => {
                        let _ = reader.ack(bytes.len()).await;
                        snapshot.extend_from_slice(&bytes);
                    }
                    Some(Ok(Chunk::Live(bytes))) => {
                        let _ = reader.ack(bytes.len()).await;
                        eprintln!("snapshot {}", String::from_utf8_lossy(&snapshot));
                        eprintln!("live {}", String::from_utf8_lossy(&bytes));
                        break;
                    }
                    _ => {
                        eprintln!("the view ended");
                        return ExitCode::FAILURE;
                    }
                }
            }
        }
        Err(e) => eprintln!("attach refused {}", refused(&e)),
    }
    match client.answer(chat, "ask-1", "yes").await {
        Ok(()) => eprintln!("answered"),
        Err(e) => eprintln!("answer refused {}", refused(&e)),
    }
    for method in ["vault_secret_reveal", "save_project_settings"] {
        match client.ui(build).await {
            Ok(ui) => match ui.call(method, serde_json::json!({})).await {
                Ok(Ok(value)) => eprintln!("{method} answered {value}"),
                Ok(Err(value)) => eprintln!("{method} failed {value}"),
                Err(e) => eprintln!("{method} refused {}", refused(&e)),
            },
            Err(e) => eprintln!("{method} refused {}", refused(&e)),
        }
    }
    match client.stop(chat).await {
        Ok(()) => eprintln!("stopped"),
        Err(e) => eprintln!("stop refused {}", refused(&e)),
    }
    // Leave now: tokio's stdin reads on a blocking thread that would hold the runtime open
    // until the host wrote again, and closing this end is what tells the host the link is done.
    std::process::exit(0)
}

#[cfg(unix)]
async fn present(scope: Scope, socket: &str, orphaned_from: Option<u32>) -> ExitCode {
    if let Some(parent) = orphaned_from {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while std::os::unix::process::parent_id() == parent {
            if std::time::Instant::now() > until {
                eprintln!("still a child of {parent}");
                return ExitCode::FAILURE;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
    let Some(credential) = std::env::var(CREDENTIAL_ENV)
        .ok()
        .and_then(|text| text.parse::<Credential>().ok())
    else {
        eprintln!("no credential in ${CREDENTIAL_ENV}");
        return ExitCode::FAILURE;
    };
    let stream = match tokio::net::UnixStream::connect(socket).await {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("could not connect: {e}");
            return ExitCode::FAILURE;
        }
    };
    let ours = Speaks::new([Version { major: 1, minor: 0 }]);
    match purlis_session_protocol::link::connect(stream, ours, scope, &credential).await {
        Ok(mut link) => {
            eprintln!("admitted as {}", link.scope());
            // Held until the host closes it, so the host takes its control lane first.
            while let Some(Ok(_)) = link.control().next().await {}
            ExitCode::SUCCESS
        }
        Err(refused) => {
            eprintln!("refused: {refused}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

async fn host<S: tokio::io::AsyncRead + tokio::io::AsyncWrite>(stdio: S, ours: Speaks) -> ExitCode {
    match answer(stdio, &ours).await {
        Ok((version, _stream)) => {
            eprintln!("agreed on {}.{}", version.major, version.minor);
            ExitCode::SUCCESS
        }
        Err(refused) => {
            eprintln!("refused: {refused}");
            ExitCode::FAILURE
        }
    }
}

async fn flood<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static>(
    stdio: S,
    streams: usize,
    bytes: usize,
) -> ExitCode {
    let ours = Speaks::new([Version { major: 1, minor: 0 }]);
    let Ok((_, io)) = offer(stdio, &ours).await else {
        eprintln!("the host refused the negotiation");
        return ExitCode::FAILURE;
    };
    let Some(credential) = std::env::var(CREDENTIAL_ENV)
        .ok()
        .and_then(|text| text.parse::<Credential>().ok())
    else {
        eprintln!("no credential in ${CREDENTIAL_ENV}");
        return ExitCode::FAILURE;
    };
    let Ok(io) = auth::present(io, Scope::LocalUi, &credential).await else {
        eprintln!("the host did not admit this client");
        return ExitCode::FAILURE;
    };
    let mut no_limits = yamux::Config::default();
    no_limits.set_max_connection_receive_window(None);
    no_limits.set_max_num_streams(streams + 1);
    let mut connection = yamux::Connection::new(io.compat(), no_limits, yamux::Mode::Client);
    let (opened, mut writes) = tokio::sync::mpsc::unbounded_channel::<yamux::Stream>();
    let writer = tokio::spawn(async move {
        let payload = vec![b'z'; bytes];
        let mut first = true;
        let mut held = Vec::new();
        while let Some(mut stream) = writes.recv().await {
            let wrote = if first {
                stream.write_all(&[CONTROL_LANE]).await
            } else {
                stream.write_all(&payload).await
            };
            first = false;
            if wrote.is_ok() {
                held.push(stream);
            }
        }
        held.len()
    });
    let mut asked = 0;
    let _ = std::future::poll_fn(|cx| {
        loop {
            let mut moved = false;
            if asked <= streams
                && let std::task::Poll::Ready(stream) = connection.poll_new_outbound(cx)
            {
                asked += 1;
                moved = true;
                if let Ok(stream) = stream {
                    let _ = opened.send(stream);
                }
            }
            match connection.poll_next_inbound(cx) {
                std::task::Poll::Ready(None) | std::task::Poll::Ready(Some(Err(_))) => {
                    return std::task::Poll::Ready(());
                }
                std::task::Poll::Ready(Some(Ok(_))) => moved = true,
                std::task::Poll::Pending => {}
            }
            if !moved {
                return std::task::Poll::Pending;
            }
        }
    })
    .await;
    drop(opened);
    let held = writer.await.unwrap_or(0);
    eprintln!("opened {asked} streams; {held} took their bytes");
    ExitCode::SUCCESS
}
