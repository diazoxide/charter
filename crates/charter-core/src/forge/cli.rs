//! The fallback transport: the forge's own CLI, `gh api` or `glab api`, logged in as whoever
//! logged that CLI in (ADR 0070 §5).
//!
//! It turns a [`Call`] into the argv charter has always sent, byte for byte: the recorded
//! Python behaviour (ADR 0046) and `a_forge_cli_is_asked_exactly_what_python_asked.rs` pin it.
//! The CLI is found and started as [`super`]'s module docs say, with an emptied environment
//! that keeps only the CLI's credential variables.

use std::process::{Command, Stdio};
use std::time::Duration;

use super::transport::{Call, Endpoint, Field, NoAnswer, Reply, Transport};
use super::{Forge, ForgeError, Kind, STATUS_TIMEOUT, TOKEN_ENV, cli_env_keeping, find_cli};
use crate::worktree::git;

/// The CLI transport. `Cli::default()` passes the CLI every credential variable it would log
/// in with; [`Cli::as_the_operator`] withholds the token ones.
#[derive(Debug, Clone, Copy, Default)]
pub struct Cli {
    withhold_tokens: bool,
}

impl Cli {
    /// The CLI logged in as the operator's own stored login, and never as a token in this
    /// process's environment: it is given none of [`TOKEN_ENV`].
    pub fn as_the_operator() -> Cli {
        Cli {
            withhold_tokens: true,
        }
    }

    /// Run the CLI with `args`, under `timeout`.
    pub fn run(&self, kind: Kind, args: &[String], timeout: Duration) -> Result<Reply, NoAnswer> {
        let withhold = self.withhold_tokens;
        call_with(kind, args, timeout, |name| {
            !(withhold && TOKEN_ENV.contains(&name))
        })
    }
}

/// The argv `call` is sent as.
pub fn argv(forge: &Forge, call: &Call) -> Vec<String> {
    let host = forge.host.as_str();
    let mut args = match (&call.endpoint, forge.kind) {
        (Endpoint::Graphql, _) => strings(&["api", "graphql", "--hostname", host]),
        (Endpoint::Rest { .. }, Kind::GitHub) => strings(&["api", "--hostname", host]),
        (Endpoint::Rest { .. }, Kind::GitLab) => strings(&["--hostname", host, "api"]),
    };
    if let Endpoint::Rest { method, path } = &call.endpoint {
        if let Some(method) = method {
            args.extend(strings(&["-X", method.word()]));
        }
        args.push(path.clone());
    }
    for field in &call.fields {
        let (flag, name, value) = match field {
            Field::Text(name, value) => ("-f", name, value),
            Field::Typed(name, value) => ("-F", name, value),
        };
        args.push(flag.to_string());
        args.push(format!("{name}={value}"));
    }
    args
}

impl Transport for Cli {
    fn send(&self, forge: &Forge, call: &Call) -> Result<Reply, NoAnswer> {
        self.run(forge.kind, &argv(forge, call), call.timeout)
    }

    fn check_auth(&self, forge: &Forge) -> Result<(), ForgeError> {
        let cli = forge.kind.cli();
        let host = forge.host.as_str();
        let args = match forge.kind {
            Kind::GitHub => strings(&["auth", "status", "--hostname", host]),
            Kind::GitLab => strings(&["--hostname", host, "auth", "status"]),
        };
        let answer = match self.run(forge.kind, &args, STATUS_TIMEOUT) {
            Ok(answer) => answer,
            Err(NoAnswer::Timeout(why)) => {
                return Err(ForgeError::transport(format!(
                    "{cli} did not answer for {host}: {why}"
                )));
            }
            Err(NoAnswer::Missing(why) | NoAnswer::Refused(why)) => {
                return Err(ForgeError::transport(why));
            }
        };
        let logged_in = match forge.kind {
            Kind::GitHub => answer.ok(),
            // glab exits 0 while logged in to nothing, so its own words are asked too.
            Kind::GitLab => answer.ok() && answer.both().contains("Logged in"),
        };
        if logged_in {
            Ok(())
        } else {
            Err(ForgeError::new(format!(
                "{cli} is not authenticated for {host}. Run: {cli} auth login"
            )))
        }
    }
}

/// Run `kind`'s CLI with `args`, passing on only the credential variables `keep` accepts.
fn call_with(
    kind: Kind,
    args: &[String],
    timeout: Duration,
    keep: impl Fn(&str) -> bool,
) -> Result<Reply, NoAnswer> {
    let cli = kind.cli();
    let Some(path) = find_cli(cli) else {
        return Err(NoAnswer::Missing(format!(
            "charter could not find {cli} on PATH — install it and log in (`{cli} auth login`)"
        )));
    };
    let mut cmd = Command::new(&path);
    cmd.args(args)
        .env_clear()
        .envs(cli_env_keeping(path.parent(), keep))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = crate::forklock::spawn(&mut cmd)
        .map_err(|e| NoAnswer::Missing(format!("charter could not run {cli}: {e}")))?;
    let run = git::wait(child, timeout)
        .map_err(|e| NoAnswer::Missing(format!("charter could not run {cli}: {e}")))?;
    match run.code {
        Some(code) => Ok(Reply::of(code, run.out, run.err)),
        None => Err(NoAnswer::Timeout(format!(
            "timed out after {}s: {cli} {}",
            timeout.as_secs(),
            args.join(" ")
        ))),
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_string()).collect()
}
