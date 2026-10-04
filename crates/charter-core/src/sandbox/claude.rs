//! The policy compiled for Claude Code: a `sandbox` object and `permissions.deny` rules, in
//! the `--settings` charter already hands each chat ([`crate::harness`]).
//!
//! Read out of Claude Code 2.1.285's settings schema, and measured live against it on macOS:
//!
//! - `sandbox` confines the commands its Bash tool runs. Its Read and Edit tools run in its
//!   own process, and a `permissions.deny` rule is what holds them, so every denied path is
//!   written twice: once for the sandbox and once for the tools.
//! - Its web tools run in its own process too, and are not held to the sandbox's allowed hosts
//!   (measured: a fetch of an unlisted host succeeded under `strictAllowlist`). No setting
//!   limits them to a list of hosts, and an allow cannot outrank a deny, so both are denied.
//! - `--settings` sits above project settings, and a project file is not honoured on the keys
//!   that loosen a sandbox.
//! - Where a class is held by a service no key in its settings can deny, the chat is refused
//!   ([`super::NotStarted::Uncompilable`]) rather than started without it.
//!
//! Measured live on 2.1.285 on macOS, a headless chat started with what [`settings`] writes: a
//! command could neither read nor write a denied directory, could write its own directory,
//! reached a listed host, and was refused an unlisted one; and with the web tools denied they
//! were not offered to the model at all, even when named in `--allowedTools`.
//!
//! **The later-code class** (ruling V73b). Measured on 2.1.288 on macOS, a headless turn against
//! a stand-in model server: Claude Code's own sandbox already kept a command from writing git's
//! config and hooks (in a nested clone too), `.mcp.json`, `.vscode`, `.idea`, `.claude`'s
//! settings, commands and agents, and shell startup files, and let it write `opencode.json`,
//! `.opencode`, `.codex`, `.envrc` and `charter.toml`. With each name added to `denyWrite` as
//! `**/<name>`, every one was refused at any depth, and an ordinary file was still written.
//! `.claude/skills`, `tui.json`, `tui.jsonc` and `.agents`, added later (#1057), are held by
//! the same `**/<name>` rule.
//! What it does not hold: a command moved a directory holding a `config` into a nested
//! clone's `.git` (measured), and no glob can deny the `.git` itself without denying what git
//! writes below it (#1065).

use serde_json::{Value, json};

use super::{Access, Compiled, Uncompilable};
use crate::harness::Harness;

/// Claude Code's own tools that reach the network from its process rather than through its
/// sandbox, so the sandbox's allowed hosts do not hold them.
pub const WEB_TOOLS: [&str; 2] = ["WebFetch", "WebSearch"];

/// What a Claude Code chat's `--settings` carries for the sandbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The `sandbox` object.
    pub sandbox: Value,
    /// `permissions.deny` rules for its own tools.
    pub deny: Vec<String>,
}

/// `compiled`, for Claude Code — or the class it cannot hold on this machine.
///
/// The credential store is refused on every system: Claude Code's settings have no key that
/// denies it, and whether its sandbox keeps a command away from it anyway has been measured on
/// none.
pub fn settings(compiled: &Compiled) -> Result<Settings, Uncompilable> {
    compiled.holds_every_service(Harness::ClaudeCode)?;
    let mut deny_read = Vec::new();
    let mut deny_write = Vec::new();
    let mut read_rules = Vec::new();
    let mut edit_rules = Vec::new();
    for denial in &compiled.denied.paths {
        let path = denial.path.display().to_string();
        let rooted = format!("/{path}");
        if denial.access == Access::ReadWrite {
            deny_read.push(path.clone());
            read_rules.push(format!("Read({rooted})"));
            read_rules.push(format!("Read({rooted}/**)"));
        }
        deny_write.push(path);
        edit_rules.push(format!("Edit({rooted})"));
        edit_rules.push(format!("Edit({rooted}/**)"));
    }
    // The later-code class (ruling V73b), by name at any depth: added to the names Claude
    // Code's own sandbox already keeps commands from writing, never in their place. A name
    // that holds only itself (`.git`) is left out: a glob here denies what is below it too,
    // and git writes there.
    for planted in super::PLANTED
        .iter()
        .filter(|planted| planted.reach == super::Reach::AndBelow)
    {
        let glob = format!("**/{}", planted.path);
        deny_write.push(glob.clone());
        edit_rules.push(format!("Edit({glob})"));
        edit_rules.push(format!("Edit({glob}/**)"));
    }
    let deny = WEB_TOOLS
        .iter()
        .map(|tool| (*tool).to_owned())
        .chain(read_rules)
        .chain(edit_rules)
        .collect();
    Ok(Settings {
        sandbox: json!({
            "enabled": true,
            "failIfUnavailable": true,
            "allowUnsandboxedCommands": false,
            "network": {
                "allowedDomains": compiled.hosts,
                "strictAllowlist": true,
            },
            "filesystem": {
                "denyRead": deny_read,
                "denyWrite": deny_write,
            },
        }),
        deny,
    })
}
