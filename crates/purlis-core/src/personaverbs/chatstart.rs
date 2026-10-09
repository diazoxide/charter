//! What a persona's definition gives the chat that runs as it, at its start (#1451,
//! D-1451-17 and D-1451-18).
//!
//! A persona used to reach a harness as a generated sub-agent, whose file carried its MCP
//! servers and its denied tools. The sub-agent is retired ([`super::retired`]); a persona is a
//! role a chat runs as. Two things the sub-agent's file enforced or started are carried over to
//! that chat here, so that retiring the file takes nothing away:
//!
//! - **its MCP servers** ([`servers`]): the persona's `mcp.json`, with a credentialed server
//!   wrapped in `purlis secret exec <vault> …` exactly as the sub-agent's was, and only where
//!   this machine approved the line ([`super::mcp`]). An approval recorded while the persona
//!   was a sub-agent carries: the fingerprint is of the entry and the vault, and both are the
//!   same. A credentialed server nobody approved is **withheld**, not started without its
//!   credential, and the chat's briefing says which and how to approve it.
//! - **its denied tools** ([`denied_tools`]): `disallowed-tools:`, as deny rules of the chat's
//!   own settings.
//!
//! # Only Claude Code can be handed either
//!
//! Claude Code takes both on the command line of one chat (`--mcp-config`, `--settings`),
//! which is where [`crate::harness::claude`] puts them. purlis has no such per-chat seam on
//! Codex or opencode. So there:
//!
//! - the servers are **not started**, and the briefing and `persona lint` say so;
//! - a persona that declares `disallowed-tools:` is **not started at all** ([`unenforced`]):
//!   a deny-list a project's author wrote does not degrade to a comment. The start
//!   ([`crate::start::ready`]) and the choice of a dispatched chat's profile
//!   ([`crate::personaprofile::for_dispatch`]) both ask that one function.

use std::path::Path;

use serde_json::{Map, Value};

use crate::harness::Harness;

use super::mcp;

/// The command a person approves a persona's credentialed servers with.
pub const APPROVE: &str = "purlis persona approve-mcp";

/// A persona's MCP servers, as a chat that runs as it is started with them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Servers {
    /// The servers the chat is started with, by name, each as `--mcp-config` takes it. A
    /// credentialed one is here only wrapped in `secret exec`: never with a credential in it.
    pub started: Map<String, Value>,
    /// `(server, consent line)` for each credentialed server this machine has not approved.
    /// Not started. An empty line is an entry purlis cannot show, which cannot be approved.
    pub withheld: Vec<(String, String)>,
    /// Server names that are refused: outside the alphabet, or purlis's own server's name.
    pub refused: Vec<String>,
}

impl Servers {
    /// Whether the persona declares no server at all.
    pub fn is_empty(&self) -> bool {
        self.started.is_empty() && self.withheld.is_empty() && self.refused.is_empty()
    }
}

/// The MCP servers of `persona` in the project at `root`, with this machine's approvals read
/// from `state`. `binary` is the `purlis` a wrapped server runs its `secret exec` through: the
/// app's own, by path, so the server does not depend on what the chat's `PATH` holds.
pub fn servers(root: &Path, state: &Path, persona: &str, binary: &Path) -> Servers {
    let (declared, mut refused) = mcp::declared(root, persona);
    let vault = super::resolve(root, persona).and_then(|r| r.get("vault").map(str::to_owned));
    let approved = mcp::approved(state, persona);
    let withheld = mcp::withheld(root, state, persona);
    let mut started = Map::new();
    for (name, entry) in &declared {
        // purlis's own server rides on the same argument under this name; a persona's file
        // cannot stand in for it.
        if name == crate::chattools::SERVER {
            refused.push(name.clone());
            continue;
        }
        if withheld.iter().any(|(server, _)| server == name) {
            continue;
        }
        let wrapped = mcp::needs_consent(mcp::vault_for(vault.as_deref()).as_deref(), entry);
        let mut entry = mcp::render_entry(vault.as_deref(), entry, &approved);
        if wrapped && let Some(map) = entry.as_object_mut() {
            map.insert("command".into(), binary.display().to_string().into());
        }
        started.insert(name.clone(), entry);
    }
    Servers {
        started,
        withheld,
        refused,
    }
}

/// The tools `persona` denies its chats: its `disallowed-tools:`, comma-separated, with its
/// `extends:` chain applied (a child's line answers instead of its parent's, as for every
/// plain key). Each entry is a harness's own rule, passed as written.
pub fn denied_tools(root: &Path, persona: &str) -> Vec<String> {
    crate::personagrant::resolve(root, persona)
        .and_then(|resolved| resolved.get("disallowed-tools").map(str::to_owned))
        .map(|line| {
            line.split(',')
                .map(|tool| crate::memstore::py_strip(tool).to_owned())
                .filter(|tool| !tool.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Whether purlis can hand a chat on `harness` its persona's denied tools and MCP servers.
pub fn carries(harness: Option<Harness>) -> bool {
    harness == Some(Harness::ClaudeCode)
}

/// Why a chat as `persona` is not started on `harness`, or `None` where it may be: the
/// persona declares `disallowed-tools:` and purlis cannot deny a chat those tools there.
///
/// **Fail closed** (D-1451-18). The one answer for a chat the person starts and for a
/// dispatched one.
pub fn unenforced(root: &Path, persona: &str, harness: Option<Harness>) -> Option<String> {
    if carries(harness) || denied_tools(root, persona).is_empty() {
        return None;
    }
    Some(unenforced_said(
        persona,
        &super::def_rel(root, persona),
        harness,
    ))
}

/// [`unenforced`]'s sentence, for a caller that already knows the persona declares the key:
/// `file` is its definition's path in the project.
pub fn unenforced_said(persona: &str, file: &str, harness: Option<Harness>) -> String {
    let on = harness.map_or("a harness purlis does not know", Harness::title);
    let name = crate::shown::short(persona);
    format!(
        "persona '{name}' declares `disallowed-tools:`, and purlis can deny a chat those tools \
         only on Claude Code. On {on} its chat would run with them, so it was not started. \
         Start it on a Claude Code profile, or take the line out of {} if the rule is no \
         longer meant.",
        crate::shown::short(file)
    )
}

/// What a chat that runs as `persona` on `harness` is told about its servers at its start, one
/// sentence each. Nothing for a persona that declares none.
pub fn told(root: &Path, state: &Path, persona: &str, harness: Option<Harness>) -> Vec<String> {
    let servers = servers(root, state, persona, Path::new("purlis"));
    let mut out = Vec::new();
    if servers.is_empty() {
        return out;
    }
    let list = |names: Vec<&str>| {
        names
            .iter()
            .map(|name| format!("`{}`", mcp::label(&[name])))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if !carries(harness) {
        let all: Vec<&str> = servers
            .started
            .keys()
            .map(String::as_str)
            .chain(servers.withheld.iter().map(|(name, _)| name.as_str()))
            .collect();
        if !all.is_empty() {
            out.push(format!(
                "This persona's MCP servers ({}) are not started for this chat: a \
                 persona's servers are started on Claude Code only. Their tools are not here, so say so \
                 if the work needs one.",
                list(all)
            ));
        }
        return out;
    }
    if !servers.withheld.is_empty() {
        out.push(format!(
            "This persona's MCP server(s) {} were not started: each is handed a credential \
             from the persona's vault, and nobody has approved that on this machine. Their \
             tools are not here. Tell the operator, who approves them on the persona's tab in \
             purlis's window, or in a terminal with `{APPROVE} --persona {}`; a new chat then \
             starts with them.",
            list(
                servers
                    .withheld
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect()
            ),
            crate::shown::short(persona)
        ));
    }
    if !servers.refused.is_empty() {
        out.push(format!(
            "This persona's MCP server(s) {} were not started: the name is one purlis refuses \
             (`purlis persona lint` says why).",
            list(servers.refused.iter().map(String::as_str).collect())
        ));
    }
    out
}

#[cfg(test)]
#[path = "chatstart_tests.rs"]
mod tests;
