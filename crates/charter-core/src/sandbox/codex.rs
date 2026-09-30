//! The policy compiled for Codex: `-c` flags that select a permissions profile charter names
//! for this start alone, beside the `-c` flags that already arm its hooks
//! ([`crate::harness`]).
//!
//! Read out of codex-cli 0.147.0's source (`rust-v0.147.0`) and measured against its binary on
//! macOS, both through `codex sandbox` and through real `codex exec` and TUI turns against a
//! stand-in model server:
//!
//! - **Workspace-write, set explicitly, as a profile.** The profile `extends = ":workspace"`,
//!   Codex's own workspace-write, and is selected with `default_permissions`. It is not
//!   `-s workspace-write`: that flag puts Codex on its legacy sandbox syntax, which reads no
//!   profile, so the denied paths and the proxy below would be dropped with it (and a
//!   `-s` in the chat's own command drops them too, which is why [`loosened_by`] refuses it).
//! - **Paths.** A profile entry of `deny` keeps a path from being read or written; `read`
//!   keeps a path under a writable root from being written. Measured: a command could not
//!   read or list a denied directory, nor write a read-only one inside its workspace, nor
//!   write outside its workspace and the temp directories; a path that does not exist yet
//!   could not be created. Codex's in-process file tools (`view_image`) read through the same
//!   policy (source).
//! - **Egress through Codex's own proxy** (ADR 0067 §3: the harness's own proxy where it has
//!   one). A profile's `network.enabled` alone opens the network whole: the proxy runs only
//!   with `features.network_proxy` (measured: with it turned off, an unlisted host answered).
//!   With it, a listed host answered, an unlisted host was refused, and a connection that
//!   skipped the proxy was refused.
//! - **A name no file has seen.** A config layer that names the same profile merges into it,
//!   and its hosts are added (measured). The name is new at every start ([`flags`]), so no
//!   file can name it first. `features.network_proxy = true` replaces a table a lower layer
//!   gives that key, and its hosts with it (measured).
//! - **No way out from inside the chat.** A command asking to run outside the sandbox, or with
//!   more than it allows, is rejected rather than shown (measured in the TUI: no prompt, and
//!   the model is told why); an unlisted host is refused without a prompt (measured). Only a
//!   person reviews what is still asked, never a reviewing model. The live web search tool,
//!   which fetches from the provider's side, is off (it was not offered to the model).
//! - **The credential store** is reachable from Codex's sandbox whenever its network is on
//!   (measured on macOS: a command still queried the keychain), so a plane with a keyring vault
//!   starts no sandboxed Codex chat. Unmeasured on Linux, and refused there too.
//! - **Resume.** `codex resume` of a conversation that ran unsandboxed runs under the profile
//!   it is handed now (measured).

use super::{Compiled, Service, Uncompilable};
use crate::harness::Harness;

/// The start of every profile name charter hands Codex.
pub const PROFILE_PREFIX: &str = "charter-sandbox-";

/// What a Codex chat is handed for the sandbox: `-c key=value` pairs, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flags {
    pub args: Vec<String>,
}

/// `compiled`, for Codex, under a profile name new to this start — or the class it cannot
/// hold on this machine.
pub fn flags(compiled: &Compiled) -> Result<Flags, Uncompilable> {
    flags_named(
        compiled,
        &format!("{PROFILE_PREFIX}{}", uuid::Uuid::new_v4().simple()),
    )
}

/// `compiled`, for Codex, as the profile `name`.
pub fn flags_named(compiled: &Compiled, name: &str) -> Result<Flags, Uncompilable> {
    if let Some(service) = compiled.denied.services.first() {
        match service {
            Service::CredentialStore => {
                return Err(Uncompilable {
                    harness: Harness::Codex,
                    service: *service,
                });
            }
        }
    }
    let filesystem: toml::Table = compiled
        .denied
        .paths
        .iter()
        .map(|denial| {
            let access = match denial.access {
                super::Access::ReadWrite => "deny",
                super::Access::Write => "read",
            };
            (denial.path.display().to_string(), toml::Value::from(access))
        })
        .collect();
    let domains: toml::Table = compiled
        .hosts
        .iter()
        .map(|host| (host.clone(), toml::Value::from("allow")))
        .collect();
    let profile = toml::toml! {
        extends = ":workspace"
        filesystem = filesystem
        [network]
        enabled = true
        domains = domains
    };
    let approval = toml::toml! {
        [granular]
        sandbox_approval = false
        request_permissions = false
        skill_approval = false
        rules = true
        mcp_elicitations = true
    };

    let mut args = Vec::new();
    let mut set = |key: &str, value: toml::Value| {
        args.push("-c".to_owned());
        args.push(format!("{key}={value}"));
    };
    set("default_permissions", toml::Value::from(name));
    set(&format!("permissions.{name}"), toml::Value::Table(profile));
    set("features.network_proxy", toml::Value::from(true));
    set("approval_policy", toml::Value::Table(approval));
    set("approvals_reviewer", toml::Value::from("user"));
    set("web_search", toml::Value::from("disabled"));
    Ok(Flags { args })
}

/// Flags of Codex's own that drop or widen the sandbox charter hands it, each measured on
/// codex-cli 0.147.0 or read from its source: `-s` of any value and the bypass put Codex back
/// on a sandbox that reads no profile, `--add-dir` and `--cd` move what is writable, and
/// `--approve-for-me` routes what is asked to a reviewing model. The hidden aliases are
/// Codex's own (`--yolo`, `--not-so-yolo`).
const LOOSENING: [&str; 10] = [
    "-s",
    "--sandbox",
    "--dangerously-bypass-approvals-and-sandbox",
    "--yolo",
    "--add-dir",
    "-C",
    "--cd",
    "--approve-for-me",
    "--not-so-yolo",
    // The live web search, which the profile turns off.
    "--search",
];

/// The approval flags: they outrank the policy charter hands Codex, so any value but `never`,
/// which asks nothing and rejects what it would have asked, loosens it.
const APPROVAL: [&str; 2] = ["-a", "--ask-for-approval"];

/// The feature whose proxy holds the egress: turned off, the network is open whole (measured).
const PROXY_FEATURE: &str = "network_proxy";

/// The flag in `command`, a Codex chat's own words, that would drop or widen the sandbox
/// charter hands it — as it would be named to the operator, in backticks — or `None`.
///
/// A `-c` value of theirs cannot: charter's own `-c` flags follow the command and win, and no
/// value can name the profile, whose name is new at every start ([`flags`]).
pub fn loosened_by(command: &[String]) -> Option<String> {
    let mut words = command.iter().map(String::as_str).peekable();
    while let Some(word) = words.next() {
        let (flag, attached) = split(word);
        if flag == "-c" || flag == "--config" {
            if attached.is_none() {
                words.next();
            }
            continue;
        }
        if APPROVAL.contains(&flag) {
            let value = attached.or_else(|| words.peek().copied());
            if value != Some("never") {
                return Some(format!("`{flag}`"));
            }
            continue;
        }
        if flag == "--disable" {
            let value = attached.or_else(|| words.peek().copied());
            if value == Some(PROXY_FEATURE) {
                return Some(format!("`--disable {PROXY_FEATURE}`"));
            }
            continue;
        }
        if LOOSENING.contains(&flag) {
            return Some(format!("`{flag}`"));
        }
    }
    None
}

/// `word` as a flag and the value attached to it: `--sandbox=x` and `-sx` are `-s`/`--sandbox`
/// with `x`. A word that is not a flag is itself, with nothing attached.
fn split(word: &str) -> (&str, Option<&str>) {
    if word.starts_with("--") {
        return match word.split_once('=') {
            Some((flag, value)) => (flag, Some(value)),
            None => (word, None),
        };
    }
    if word.starts_with('-') && word.len() > 2 && word.is_char_boundary(2) {
        return (&word[..2], Some(&word[2..]));
    }
    (word, None)
}
