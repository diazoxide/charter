//! A host end of the session protocol on this process's own stdin and stdout.
//!
//! It is what the tests reach through a child's stdio, the transport a connector gives (ADR
//! 0078 §2), and it is a separate process on purpose: ADR 0068 §4 holds that a protocol only
//! the app has ever spoken is not yet public. It answers the negotiation with the versions
//! `--speaks` names (`2.1,1.3`: major 2 at minor 1, major 1 at minor 3), and exits 0 when a
//! version was agreed and 1, saying why on stderr, when it was not.

use std::process::ExitCode;

use charter_session_protocol::version::{Speaks, Version, answer};

fn speaks(arg: &str) -> Option<Speaks> {
    let versions: Option<Vec<Version>> = arg
        .split(',')
        .map(|v| {
            let (major, minor) = v.split_once('.')?;
            Some(Version { major: major.parse().ok()?, minor: minor.parse().ok()? })
        })
        .collect();
    versions.map(Speaks::new)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(ours) = (match args.as_slice() {
        [flag, value] if flag == "--speaks" => speaks(value),
        _ => None,
    }) else {
        eprintln!("usage: charter-session-peer --speaks <major>.<minor>[,<major>.<minor>…]");
        return ExitCode::from(2);
    };
    let mut stdio = tokio::io::join(tokio::io::stdin(), tokio::io::stdout());
    match answer(&mut stdio, &ours).await {
        Ok(version) => {
            eprintln!("agreed on {}.{}", version.major, version.minor);
            ExitCode::SUCCESS
        }
        Err(refused) => {
            eprintln!("refused: {refused}");
            ExitCode::FAILURE
        }
    }
}
