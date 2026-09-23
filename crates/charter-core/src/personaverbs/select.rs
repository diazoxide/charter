//! `charter persona use <name>` — select a persona for this session and this pane. A port of
//! `commands_persona.cmd_persona_use` and `persona.set_active`.
//!
//! # What it writes, and where
//!
//! Exactly what [`crate::active`] reads back: `.charter/sessions/<sid>.persona` for this
//! session, `.charter/terminals/<tid>.persona` for this pane, and — only when the process has
//! neither id — the plane-wide `.charter/active-persona`. Each at 0600 in a 0700 directory. It
//! also records a `persona-use` event in the session's trace.
//!
//! # It touches no vault
//!
//! Selecting a persona opens nothing: the persona's vault is reached later, by `charter
//! persona secret …`, which resolves the active persona itself. So this command needed
//! nothing from the secrets registry and has no stub standing in for it.
//!
//! # A chat's launch record is not read
//!
//! Python writes no terminal pointer from inside a chat its frame launched, and says "for
//! this chat only". That record is the tmux frame's, which this charter neither reads nor
//! writes (`docs/plane-format.md`; [`crate::wscmd::select`] declares the same for
//! `workspace use`), so a chat here is treated as the terminal it runs in.

use std::path::Path;

use crate::active::Ids;
use crate::repocmd::{Say, Sink};

/// How far a selection reaches — `set_active`'s return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Terminal,
    Session,
    Plane,
}

fn usable(id: Option<&str>) -> Option<&str> {
    id.filter(|id| !id.is_empty() && crate::contain::segment_ok(id))
}

/// `persona.set_active`: write the pointers and return the reach of the longest-lived one.
pub fn set_active(root: &Path, name: &str, ids: &Ids) -> Scope {
    let state = root.join(".charter");
    let _ = crate::plane::private_dir(root, &state);
    let line = format!("{name}\n");
    let sid = usable(ids.session.as_deref());
    let tid = usable(ids.terminal.as_deref());
    for (dir, id) in [("sessions", sid), ("terminals", tid)] {
        let Some(id) = id else {
            continue;
        };
        let dir = state.join(dir);
        if crate::plane::private_dir(root, &dir).is_ok() {
            let _ = crate::plane::write_private(
                root,
                &dir.join(format!("{id}.persona")),
                line.as_bytes(),
            );
        }
    }
    if sid.is_none() && tid.is_none() {
        let _ = crate::plane::write_private(root, &state.join("active-persona"), line.as_bytes());
    }
    if tid.is_some() {
        Scope::Terminal
    } else if sid.is_some() {
        Scope::Session
    } else {
        Scope::Plane
    }
}

/// `commands_persona._scope_note`: how far the selection reaches, in the words the reader
/// needs.
fn scope_note(root: &Path, scope: Scope) -> String {
    match scope {
        Scope::Terminal => " for this terminal (kept across closing/reopening Claude)".into(),
        Scope::Session => {
            let starts = match crate::active::plane_default_persona(root) {
                Some(next) => format!("starts as '{next}'"),
                None => "starts with no persona".into(),
            };
            format!(
                " for this session only — this terminal reports no pane id, so a new session \
                 {starts}"
            )
        }
        Scope::Plane => " for this control plane (no session or pane id to scope it to)".into(),
    }
}

/// Who is asking: the ids to key the pointers on, `$CHARTER_PERSONA` as it stands, the
/// trace bucket, and the clock the trace is stamped with.
pub struct Asking<'a> {
    pub ids: &'a Ids,
    pub env_persona: Option<&'a str>,
    pub bucket: &'a str,
    pub now: chrono::NaiveDateTime,
}

/// `charter persona use <name>`, and its exit code.
pub fn use_persona(root: &Path, name: &str, asking: &Asking, say: Sink) -> u8 {
    if let Some(refused) = crate::personas::name_refusal(root, name) {
        say(Say::Fail(refused));
        return 1;
    }
    let scope = set_active(root, name, asking.ids);
    crate::trace::record(
        root,
        asking.bucket,
        "persona-use",
        &[("persona", name)],
        asking.now,
    );
    say(Say::Done(format!(
        "Active persona set to '{name}'{}.",
        scope_note(root, scope)
    )));
    warn_env(name, asking.env_persona, say);
    say_tool_ceiling(root, name, usable(asking.ids.session.as_deref()), say);
    say_mcp_boundary(root, name, say);
    0
}

/// `_warn_env`: `$CHARTER_PERSONA` outranks every pointer this wrote.
fn warn_env(name: &str, env: Option<&str>, say: Sink) {
    let env = crate::memstore::py_strip(env.unwrap_or_default());
    if env.is_empty() || env == name {
        return;
    }
    let shown = crate::personas::one_line(env);
    say(Say::Warn(format!(
        "$CHARTER_PERSONA='{shown}' is set and takes precedence — commands use '{shown}', not \
         '{name}'."
    )));
}

/// `_say_tool_ceiling`: the tools declared since this session froze its ceiling still
/// prompt here, and the reason is said rather than left to be discovered.
fn say_tool_ceiling(root: &Path, name: &str, sid: Option<&str>, say: Sink) {
    let Some(frozen) = crate::personagate::frozen_tools(root, name, sid) else {
        return;
    };
    let added: Vec<String> = crate::personagrant::effective_tools(root, name)
        .into_iter()
        .filter(|t| !frozen.contains(t))
        .collect();
    if added.is_empty() {
        return;
    }
    say(Say::Info(format!(
        "  {} tool(s) declared since this session started: {}.",
        added.len(),
        added.join(", ")
    )));
    say(Say::Info(
        "  Those still prompt HERE. The tool-gate answers within the set that existed at \
         session start — `tools:` is read from a file this session can write, and freezing it \
         is what stops an edit from becoming an unprompted command. A new session picks them \
         up."
        .into(),
    ));
}

/// `_say_mcp_boundary`: a persona's MCP servers are scoped to DISPATCH, not to this session.
fn say_mcp_boundary(root: &Path, name: &str, say: Sink) {
    let (servers, _) = super::mcp::declared(root, name);
    if servers.is_empty() {
        return;
    }
    let mut names: Vec<&String> = servers.keys().collect();
    names.sort();
    say(Say::Info(format!(
        "  {} MCP server(s) declared: {}.",
        names.len(),
        names
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )));
    say(Say::Info(
        "  These are scoped to DISPATCH — the host starts them when this persona runs as a \
         sub-agent and stops them after, and their tools never enter this conversation. They \
         are NOT started for the session you are in, and servers already live here (from \
         .mcp.json or an enabled plugin) stay live."
            .into(),
    ));
}
