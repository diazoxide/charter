//! The policy compiled for Codex: flags that select a permissions profile charter names for
//! this start alone, put last among the flags on the chat's line ([`super::Applied::line`]).
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
//!   with the `network_proxy` feature (measured: with it turned off, an unlisted host
//!   answered). With it, a listed host answered, an unlisted host was refused, and a
//!   connection that skipped the proxy was refused. The feature is `experimental` in 0.147.0,
//!   so it is turned on with `--enable network_proxy` rather than a `-c`: Codex refuses to
//!   start, in the TUI, in `exec` and on `resume`, when `--enable` names a feature it does not
//!   have (measured), so a Codex that has dropped the proxy never runs the profile open.
//! - **A name no file has seen.** A config layer that names the same profile merges into it,
//!   and its hosts are added (measured). The name is new at every start ([`flags`]), so no
//!   file can name it first. `--enable network_proxy` replaces a table a lower layer gives
//!   `features.network_proxy`, and its hosts with it (measured).
//! - **The operator's own config and `-p` profiles cannot undo it.** A `~/.codex/config.toml`
//!   holding `sandbox_mode = "danger-full-access"`, `approval_policy = "never"`,
//!   `web_search = "live"` and a `[sandbox_workspace_write]` with the network on and an extra
//!   writable root, and a `-p` profile holding all of those plus
//!   `default_permissions = ":danger-full-access"` and `features.network_proxy = false`, each
//!   left the profile whole: the denied file stayed unreadable, the extra root unwritable, the
//!   unlisted host refused, escalation rejected without a prompt, and no web search offered
//!   (measured, in `exec` and the TUI). Session flags select the profile syntax, and the
//!   legacy `sandbox_mode` and `[sandbox_workspace_write]` are read only without it (source,
//!   `resolve_permission_config_syntax`).
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
//! - **The chat's own words** can outrank what charter hands it: a flag of Codex's own, or a
//!   `-c` key. [`loosened_by`] names each one that can, and the chat is refused.
//! - **Features that may reach past the proxy** — the browser and computer-use features, stable
//!   and unmeasured against the sandbox — are turned off with `--disable` until measured. `--full-auto`
//!   is not among them: 0.147.0 refuses it as an unknown argument (measured).

use super::{Compiled, Uncompilable};
use crate::harness::Harness;

/// The start of every profile name charter hands Codex.
pub const PROFILE_PREFIX: &str = "charter-sandbox-";

/// What a Codex chat is handed for the sandbox: flags, in order, that go last among the
/// flags on its line.
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
pub(super) fn flags_named(compiled: &Compiled, name: &str) -> Result<Flags, Uncompilable> {
    compiled.holds_every_service(Harness::Codex)?;
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

    let mut args = vec!["--enable".to_owned(), PROXY_FEATURE.to_owned()];
    for feature in UNMEASURED_FEATURES {
        args.push("--disable".to_owned());
        args.push(feature.to_owned());
    }
    let mut set = |key: &str, value: toml::Value| {
        args.push("-c".to_owned());
        args.push(format!("{key}={value}"));
    };
    set("default_permissions", toml::Value::from(name));
    set(&format!("permissions.{name}"), toml::Value::Table(profile));
    set("approval_policy", toml::Value::Table(approval));
    set("approvals_reviewer", toml::Value::from("user"));
    set("web_search", toml::Value::from("disabled"));
    Ok(Flags { args })
}

/// The feature whose proxy holds the egress.
const PROXY_FEATURE: &str = "network_proxy";

/// Features that are stable in 0.147.0 and may reach the network from Codex's own process,
/// past the proxy: unmeasured against the sandbox, so off in a sandboxed chat until they are.
/// `--disable` fails closed as `--enable` does: a Codex that does not know a feature refuses
/// to start, and one that has since removed it still starts (measured, with a feature 0.147.0
/// lists as removed).
const UNMEASURED_FEATURES: [&str; 3] = ["browser_use", "computer_use", "in_app_browser"];

/// Flags of Codex's own that drop or widen the sandbox charter hands it, each measured on
/// codex-cli 0.147.0 or read from its source: `-s` of any value and the bypass put Codex back
/// on a sandbox that reads no profile, `--add-dir` and `--cd` move what is writable,
/// `--approve-for-me` routes what is asked to a reviewing model, and `--search` turns the web
/// search back on. `--enable` and `--disable` because what a feature does to the sandbox is
/// unmeasured, one feature at a time, and `--disable network_proxy` opens the network whole
/// (measured). The hidden aliases are Codex's own (`--yolo`, `--not-so-yolo`).
const LOOSENING: [&str; 12] = [
    "-s",
    "--sandbox",
    "--dangerously-bypass-approvals-and-sandbox",
    "--yolo",
    "--add-dir",
    "-C",
    "--cd",
    "--approve-for-me",
    "--not-so-yolo",
    "--search",
    "--enable",
    "--disable",
];

/// The approval flags: they outrank the policy charter hands Codex, so any value but `never`,
/// which asks nothing and rejects what it would have asked, loosens it.
const APPROVAL: [&str; 2] = ["-a", "--ask-for-approval"];

/// The config flags.
const CONFIG: [&str; 2] = ["-c", "--config"];

/// The `-c` keys a chat's own words may set: the model and how it is shown, which no sandbox
/// reads. A key is its first dotted part. Every other key is refused, because the sandbox is
/// spread over many (`sandbox_mode`, `sandbox_workspace_write`, `profile`,
/// `default_permissions`, `permissions`, `features`, `tools`, the approval keys, `web_search`)
/// and a hook or an MCP server runs outside it.
const CONFIG_ALLOWED: [&str; 12] = [
    "model",
    "model_provider",
    "model_providers",
    "model_reasoning_effort",
    "model_reasoning_summary",
    "model_verbosity",
    "model_context_window",
    "model_auto_compact_token_limit",
    "plan_mode_reasoning_effort",
    "service_tier",
    "personality",
    "tui",
];

/// Codex 0.147.0's flags that take a value in the next word, top level and `resume` alike, so
/// that value is never read as a flag or as a first message.
const TAKES_A_VALUE: [&str; 20] = [
    "-c",
    "--config",
    "--enable",
    "--disable",
    "--remote",
    "--remote-auth-token-env",
    "-i",
    "--image",
    "-m",
    "--model",
    "--local-provider",
    "-p",
    "--profile",
    "-s",
    "--sandbox",
    "-C",
    "--cd",
    "--add-dir",
    "-a",
    "--ask-for-approval",
];

/// One word of a Codex command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Word<'a> {
    /// A flag, and its value where it takes one.
    Flag(&'a str, Option<&'a str>),
    /// The value of the flag before it.
    Value,
    /// A subcommand, a session id, or a first message.
    Positional,
}

/// `words` as Codex reads them.
///
/// A word that holds whitespace before any `=` is a message, never a flag: no flag of Codex's
/// is spelled with a space, and charter's first message is one word, whatever it starts with.
/// After `--`, every word is positional.
fn read(words: &[String]) -> Vec<Word<'_>> {
    let mut out = Vec::with_capacity(words.len());
    let mut rest = words.iter().map(String::as_str);
    let mut positional_only = false;
    while let Some(word) = rest.next() {
        if positional_only || !is_flag(word) {
            out.push(Word::Positional);
            continue;
        }
        if word == "--" {
            positional_only = true;
            out.push(Word::Positional);
            continue;
        }
        let (flag, attached) = split(word);
        if attached.is_none() && TAKES_A_VALUE.contains(&flag) {
            let value = rest.next();
            out.push(Word::Flag(flag, value));
            if value.is_some() {
                out.push(Word::Value);
            }
        } else {
            out.push(Word::Flag(flag, attached));
        }
    }
    out
}

fn is_flag(word: &str) -> bool {
    let head = word.split('=').next().unwrap_or(word);
    word.starts_with('-') && word.len() > 1 && !head.contains(char::is_whitespace)
}

/// `word` as a flag and the value attached to it: `--sandbox=x` and `-sx` are `-s`/`--sandbox`
/// with `x`.
fn split(word: &str) -> (&str, Option<&str>) {
    if word.starts_with("--") {
        return match word.split_once('=') {
            Some((flag, value)) => (flag, Some(value)),
            None => (word, None),
        };
    }
    if word.len() > 2 && word.is_char_boundary(2) {
        return (&word[..2], Some(&word[2..]));
    }
    (word, None)
}

/// The flag in `words`, one source of a Codex chat's words, that would drop or widen the
/// sandbox charter hands it — as it would be named to the operator, in backticks — or `None`.
pub fn loosened_by(words: &[String]) -> Option<String> {
    read(words).into_iter().find_map(|word| {
        let Word::Flag(flag, value) = word else {
            return None;
        };
        if CONFIG.contains(&flag) {
            let key = value?.split('=').next()?.trim();
            let root = key.split('.').next().unwrap_or(key);
            return (!CONFIG_ALLOWED.contains(&root)).then(|| format!("`-c {key}`"));
        }
        if APPROVAL.contains(&flag) {
            return (value != Some("never")).then(|| format!("`{flag}`"));
        }
        LOOSENING.contains(&flag).then(|| format!("`{flag}`"))
    })
}

/// How many of `words`, from the end, are positional: a subcommand, its session id and a first
/// message, which charter's flags must stand in front of.
pub fn positional_tail(words: &[String]) -> usize {
    read(words)
        .iter()
        .rev()
        .take_while(|word| **word == Word::Positional)
        .count()
}
