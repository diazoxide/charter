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

#[cfg(test)]
mod tests {
    //! `charter persona use` against the recorded scenarios' `daily` plane
    //! (`persona-use-…` in `tests/fixtures/recorded/behaviour.jsonl`): the line said and the
    //! pointer written for each set of ids.

    use super::*;
    use crate::personaverbs::tests_plane::{Heard, OPS, OPS_MCP, Plane};

    fn ids(session: Option<&str>, terminal: Option<&str>) -> Ids {
        Ids {
            session: session.map(String::from),
            terminal: terminal.map(String::from),
        }
    }

    fn now() -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 5, 4)
            .unwrap()
            .and_hms_opt(11, 32, 17)
            .unwrap()
    }

    fn run(plane: &Plane, name: &str, ids: &Ids, env: Option<&str>) -> (u8, Heard) {
        let bucket = ids.session.clone().unwrap_or_else(|| "nosession".into());
        let asking = Asking {
            ids,
            env_persona: env,
            bucket: &bucket,
            now: now(),
        };
        let mut heard = Heard::default();
        let rc = use_persona(plane.root(), name, &asking, &mut heard.sink());
        (rc, heard)
    }

    const TRACED: &str = "{\"ts\": \"2026-05-04T11:32:17\", \"event\": \"persona-use\", \
                          \"persona\": \"devops\"}\n";

    #[test]
    fn with_a_session_and_no_pane_the_selection_is_this_sessions_and_names_the_next_sessions_start()
    {
        // persona-use-selects-for-this-session-when-there-is-no-pane-id
        let plane = Plane::fixture("daily");
        let (rc, heard) = run(&plane, "devops", &ids(Some("s-1"), None), None);
        assert_eq!((rc, heard.out.as_str()), (0, ""));
        assert_eq!(
            heard.err,
            "✓ Active persona set to 'devops' for this session only — this terminal reports no \
             pane id, so a new session starts as 'steward'.\n"
        );
        assert_eq!(plane.read(".charter/sessions/s-1.persona"), "devops\n");
        assert!(!plane.path(".charter/active-persona").exists());
        assert!(!plane.path(".charter/terminals").exists());
        assert_eq!(plane.read(".charter/persona-state/trace/s-1.jsonl"), TRACED);
    }

    #[test]
    fn a_session_selection_on_a_plane_with_no_default_says_the_next_starts_with_none() {
        let plane = Plane::fixture("daily");
        plane.write("charter.toml", "schema = 1\n");
        let (_, heard) = run(&plane, "devops", &ids(Some("s-1"), None), None);
        assert_eq!(
            heard.err,
            "✓ Active persona set to 'devops' for this session only — this terminal reports no \
             pane id, so a new session starts with no persona.\n"
        );
    }

    #[test]
    fn with_a_pane_the_selection_is_this_terminals_and_this_sessions_too() {
        // persona-use-with-a-pane-id-selects-for-this-terminal
        let plane = Plane::fixture("daily");
        let (rc, heard) = run(&plane, "devops", &ids(Some("s-1"), Some("pane-1")), None);
        assert_eq!(rc, 0);
        assert_eq!(
            heard.err,
            "✓ Active persona set to 'devops' for this terminal (kept across closing/reopening \
             Claude).\n"
        );
        assert_eq!(plane.read(".charter/terminals/pane-1.persona"), "devops\n");
        assert_eq!(plane.read(".charter/sessions/s-1.persona"), "devops\n");
        assert!(!plane.path(".charter/active-persona").exists());
        assert_eq!(
            set_active(plane.root(), "steward", &ids(None, Some("pane-1"))),
            Scope::Terminal
        );
        assert_eq!(plane.read(".charter/terminals/pane-1.persona"), "steward\n");
        assert!(!plane.path(".charter/active-persona").exists());
    }

    #[test]
    fn with_neither_id_the_selection_is_the_plane_wide_file() {
        // persona-use-with-no-ids-writes-the-plane-wide-file
        let plane = Plane::fixture("daily");
        let (rc, heard) = run(&plane, "devops", &ids(None, None), None);
        assert_eq!(rc, 0);
        assert_eq!(
            heard.err,
            "✓ Active persona set to 'devops' for this control plane (no session or pane id to \
             scope it to).\n"
        );
        assert_eq!(plane.read(".charter/active-persona"), "devops\n");
        assert_eq!(
            plane.read(".charter/persona-state/trace/nosession.jsonl"),
            TRACED
        );
    }

    #[test]
    fn an_id_that_is_empty_or_not_one_path_segment_is_no_id_at_all() {
        assert_eq!(usable(Some("s-1")), Some("s-1"));
        for bad in ["", "a/b", "..", "."] {
            assert_eq!(usable(Some(bad)), None, "{bad:?}");
        }
        assert_eq!(usable(None), None);
        let plane = Plane::fixture("daily");
        assert_eq!(
            set_active(plane.root(), "devops", &ids(Some("../x"), Some(""))),
            Scope::Plane
        );
        assert_eq!(plane.read(".charter/active-persona"), "devops\n");
        assert!(!plane.path(".charter/x.persona").exists());
    }

    #[test]
    fn charter_persona_set_to_another_name_is_said_to_outrank_the_selection() {
        // persona-use-says-the-environment-still-outranks-it
        let plane = Plane::fixture("daily");
        let (rc, heard) = run(&plane, "devops", &ids(Some("s-1"), None), Some("steward"));
        assert_eq!(rc, 0);
        assert!(
            heard.err.ends_with(
                "! $CHARTER_PERSONA='steward' is set and takes precedence — commands use \
                 'steward', not 'devops'.\n"
            ),
            "{}",
            heard.err
        );
        for same in [Some("devops"), Some(" devops "), Some("  "), None] {
            let (_, heard) = run(&plane, "devops", &ids(Some("s-1"), None), same);
            assert!(!heard.err.contains("$CHARTER_PERSONA"), "{same:?}");
        }
    }

    #[test]
    fn a_name_outside_the_alphabet_or_not_defined_is_refused_and_nothing_is_written() {
        // persona-use-refuses-a-name-outside-the-alphabet, …-a-persona-the-plane-does-not-define
        let plane = Plane::fixture("daily");
        let (rc, heard) = run(&plane, "Bad", &ids(Some("s-1"), None), None);
        assert_eq!(rc, 1);
        assert_eq!(
            heard.err,
            "✗ invalid persona name 'Bad' (lowercase letters, digits, '.', '_', '-')\n"
        );
        let (rc, heard) = run(&plane, "ghost", &ids(Some("s-1"), None), None);
        assert_eq!(rc, 1);
        assert_eq!(
            heard.err,
            "✗ no persona 'ghost' (add it: write personas/ghost/persona.md)\n"
        );
        assert!(!plane.path(".charter/sessions/s-1.persona").exists());
    }

    #[test]
    fn tools_declared_since_the_ceiling_froze_and_dispatch_scoped_servers_are_named() {
        // persona-use-names-new-tools-and-dispatch-scoped-mcp-servers
        let plane = Plane::fixture("daily");
        plane.write("personas/ops/persona.md", OPS);
        plane.write("personas/ops/mcp.json", OPS_MCP);
        plane.write(".charter/sessions/s-1.gate", "");
        plane.write(".charter/sessions/s-1.tools", "{\"ops\": [\"gh\"]}");
        let (rc, heard) = run(&plane, "ops", &ids(Some("s-1"), None), None);
        assert_eq!(rc, 0);
        assert_eq!(
            heard.err,
            "✓ Active persona set to 'ops' for this session only — this terminal reports no \
             pane id, so a new session starts as 'steward'.\n\
             •   1 tool(s) declared since this session started: kubectl.\n\
             •   Those still prompt HERE. The tool-gate answers within the set that existed at \
             session start — `tools:` is read from a file this session can write, and freezing \
             it is what stops an edit from becoming an unprompted command. A new session picks \
             them up.\n\
             •   3 MCP server(s) declared: grafana, gsc, status.\n\
             •   These are scoped to DISPATCH — the host starts them when this persona runs as a \
             sub-agent and stops them after, and their tools never enter this conversation. They \
             are NOT started for the session you are in, and servers already live here (from \
             .mcp.json or an enabled plugin) stay live.\n"
        );
        // Nothing new since the freeze, and no servers: neither note.
        plane.write(
            ".charter/sessions/s-1.tools",
            "{\"ops\": [\"gh\", \"kubectl\"]}",
        );
        std::fs::remove_file(plane.path("personas/ops/mcp.json")).unwrap();
        let (_, heard) = run(&plane, "ops", &ids(Some("s-1"), None), None);
        assert_eq!(heard.err.lines().count(), 1, "{}", heard.err);
    }
}
