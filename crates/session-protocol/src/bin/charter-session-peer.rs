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

use std::process::ExitCode;

use charter_session_protocol::auth::{self, Credential, Scope};
use charter_session_protocol::link::CONTROL_LANE;
use charter_session_protocol::version::{Speaks, Version, answer, offer};
use futures::AsyncWriteExt;
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

const USAGE: &str =
    "usage: charter-session-peer --speaks <major>.<minor>[,…] | --flood <streams> <bytes>";

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
        _ => usage(),
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
