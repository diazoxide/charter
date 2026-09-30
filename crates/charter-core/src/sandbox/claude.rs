//! The policy compiled for Claude Code: a `sandbox` object and `permissions.deny` rules, in
//! the `--settings` charter already hands each chat ([`crate::harness`]).
//!
//! Measured on Claude Code 2.1.285, out of its settings schema:
//!
//! - `sandbox` confines the commands its Bash tool runs, and nothing else. Its Read and Edit
//!   tools run in its own process, and a `permissions.deny` rule is what stops them. So every
//!   denied path is written twice: once for the sandbox and once for the tools.
//! - `--settings` sits above project settings, and `failIfUnavailable`,
//!   `allowUnsandboxedCommands: false` and `strictAllowlist` are among the keys a project
//!   file is not honoured on.
//! - No key in its settings denies the operating system's credential store on macOS, so a
//!   plane with a keyring vault does not start a Claude Code chat there; on Linux its
//!   sandbox already keeps a command away from it.
//!
//! And measured live on 2.1.285 on macOS, a headless chat started with what [`settings`]
//! writes: a command could neither read nor write a denied directory ("Operation not
//! permitted"), could write its own directory, reached a listed host, and was refused an
//! unlisted one at the proxy, which Claude Code reported as a sandbox violation.

use serde_json::{Value, json};

use super::{Access, Compiled, Os, Service, Uncompilable};

/// What a Claude Code chat's `--settings` carries for the sandbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The `sandbox` object.
    pub sandbox: Value,
    /// `permissions.deny` rules for its own Read and Edit tools.
    pub deny: Vec<String>,
}

/// `compiled`, for Claude Code — or the class it cannot hold on this machine.
pub fn settings(compiled: &Compiled) -> Result<Settings, Uncompilable> {
    for service in &compiled.denied.services {
        match (service, compiled.os) {
            (Service::CredentialStore, Os::Linux) => {}
            (Service::CredentialStore, _) => {
                return Err(Uncompilable {
                    class: service.class(),
                    why: format!(
                        "Claude Code's sandbox cannot deny {} on this system",
                        service.said()
                    ),
                });
            }
        }
    }
    let mut deny_read = Vec::new();
    let mut deny_write = Vec::new();
    let mut deny = Vec::new();
    for denial in &compiled.denied.paths {
        let path = denial.path.display().to_string();
        let rooted = format!("/{path}");
        if denial.access == Access::ReadWrite {
            deny_read.push(path.clone());
            deny.push(format!("Read({rooted})"));
            deny.push(format!("Read({rooted}/**)"));
        }
        deny_write.push(path);
        deny.push(format!("Edit({rooted})"));
        deny.push(format!("Edit({rooted}/**)"));
    }
    // Read rules first, then the edit rules, the order a person reads them in.
    deny.sort_by_key(|rule| !rule.starts_with("Read("));
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
