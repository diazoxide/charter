//! What a persona chat is started with, on the `ops` project of the MCP tests: one plain
//! server, two that take a credential from the persona's vault, and one refused name. The
//! shape of a real project's persona (three servers, two vault-wrapped).

use std::path::Path;

use serde_json::json;

use super::*;
use crate::personaverbs::tests_plane::{OPS_APPROVED, Plane};

const BINARY: &str = "/Applications/purlis.app/Contents/MacOS/purlis";

fn of(plane: &Plane) -> Servers {
    servers(plane.root(), &plane.state(), "ops", Path::new(BINARY))
}

#[test]
fn an_unapproved_credentialed_server_is_withheld_and_never_started_without_its_credential() {
    let plane = Plane::daily_with_ops();

    let got = of(&plane);

    assert_eq!(
        got.started.keys().collect::<Vec<_>>(),
        ["status"],
        "only the server that takes no credential"
    );
    assert_eq!(
        got.started["status"],
        json!({"type": "http", "url": "https://status.example.com/mcp"})
    );
    let withheld: Vec<&str> = got.withheld.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(withheld, ["grafana", "gsc"]);
    assert_eq!(got.refused, ["bad name"]);
}

#[test]
fn an_approval_recorded_for_the_sub_agent_carries_and_the_credential_goes_through_secret_exec() {
    // D-1451-17: the fingerprint is of the entry and the vault, and neither changed.
    let plane = Plane::daily_with_ops();
    crate::personaverbs::mcp::approve(
        plane.root(),
        &plane.state(),
        "ops",
        &OPS_APPROVED.map(String::from),
    );

    let got = of(&plane);

    assert!(got.withheld.is_empty(), "{:?}", got.withheld);
    assert_eq!(
        got.started["grafana"],
        json!({
            "type": "stdio",
            "command": BINARY,
            "args": ["secret", "exec", "ops", "--env", "GRAFANA_TOKEN=grafana-token",
                     "--exec", "--", "npx", "-y", "grafana-mcp@1.2.0"],
            "env": {"GRAFANA_URL": "https://grafana.example.com"},
        })
    );
    assert_eq!(
        got.started["gsc"]["args"],
        json!([
            "secret",
            "exec",
            "ops",
            "--file",
            "GOOGLE_APPLICATION_CREDENTIALS=GOOGLE_SA",
            "--stream",
            "--",
            "uvx",
            "gsc-mcp==0.3.0"
        ])
    );
    // No entry carries the declaration of a credential, let alone one.
    let text = serde_json::Value::Object(got.started.clone()).to_string();
    assert!(
        !text.contains("secrets") && !text.contains("secret_files"),
        "{text}"
    );
    // An approval of one line is an approval of that server only.
    crate::personaverbs::mcp::approve(
        plane.root(),
        &plane.state(),
        "ops",
        &[OPS_APPROVED[0].to_owned()],
    );
    assert_eq!(of(&plane).withheld.len(), 1);
}

#[test]
fn a_persona_cannot_name_a_server_after_purlis_s_own() {
    let plane = Plane::daily_with_ops();
    plane.write(
        "personas/ops/mcp.json",
        &format!(
            r#"{{"mcpServers": {{"{}": {{"command": "evil"}}, "ok": {{"command": "fine"}}}}}}"#,
            crate::chattools::SERVER
        ),
    );
    let got = of(&plane);
    assert_eq!(got.started.keys().collect::<Vec<_>>(), ["ok"]);
    assert_eq!(got.refused, [crate::chattools::SERVER]);
}

#[test]
fn denied_tools_are_the_resolved_line_and_a_child_s_answers_instead_of_its_parent_s() {
    let plane = Plane::fixture("minimal");
    plane.write(
        "personas/base/persona.md",
        "---\nname: base\ndisallowed-tools:  Write , Edit,, Bash(rm:*)\n---\n",
    );
    plane.write("personas/kid/persona.md", "---\nextends: base\n---\n");
    plane.write(
        "personas/own/persona.md",
        "---\nextends: base\ndisallowed-tools: WebFetch\n---\n",
    );
    assert_eq!(
        denied_tools(plane.root(), "base"),
        ["Write", "Edit", "Bash(rm:*)"]
    );
    assert_eq!(
        denied_tools(plane.root(), "kid"),
        ["Write", "Edit", "Bash(rm:*)"]
    );
    assert_eq!(denied_tools(plane.root(), "own"), ["WebFetch"]);
    assert!(denied_tools(plane.root(), "steward").is_empty());
    assert!(denied_tools(plane.root(), "ghost").is_empty());
}

#[test]
fn a_persona_with_a_deny_list_is_refused_wherever_purlis_cannot_enforce_it() {
    // D-1451-18: fail closed. A written deny-list does not degrade to a comment.
    let plane = Plane::fixture("minimal");
    plane.write(
        "personas/reviewer/persona.md",
        "---\nname: reviewer\ndisallowed-tools: Write, Edit\n---\n",
    );
    let root = plane.root();
    assert_eq!(
        unenforced(root, "reviewer", Some(Harness::ClaudeCode)),
        None
    );
    assert_eq!(
        unenforced(root, "reviewer", Some(Harness::Codex)).as_deref(),
        Some(
            "persona 'reviewer' declares `disallowed-tools:`, and purlis can deny a chat those \
             tools only on Claude Code. On Codex its chat would run with them, so it was not \
             started. Start it on a Claude Code profile, or take the line out of \
             personas/reviewer/persona.md if the rule is no longer meant."
        )
    );
    assert!(
        unenforced(root, "reviewer", Some(Harness::Opencode))
            .unwrap()
            .contains("On opencode its chat would run with them")
    );
    assert!(
        unenforced(root, "reviewer", None).is_some(),
        "an unknown harness is refused"
    );
    // A persona with no deny-list starts anywhere.
    for harness in [Some(Harness::Codex), Some(Harness::Opencode), None] {
        assert_eq!(unenforced(root, "steward", harness), None);
    }
}

#[test]
fn the_chat_is_told_which_servers_it_did_not_get_and_how_they_are_approved() {
    let plane = Plane::daily_with_ops();
    let told_on = |harness| told(plane.root(), &plane.state(), "ops", harness);

    assert_eq!(
        told_on(Some(Harness::ClaudeCode)),
        [
            "This persona's MCP server(s) `grafana`, `gsc` were not started: each is handed a \
             credential from the persona's vault, and nobody has approved that on this \
             machine. Their tools are not here. Tell the operator, who approves them in a \
             terminal with `purlis persona approve-mcp --persona ops`; a new chat then starts \
             with them.",
            "This persona's MCP server(s) `bad name` were not started: the name is one purlis \
             refuses (`purlis persona lint` says why).",
        ]
    );
    // On a harness purlis cannot hand them to, none is started, approved or not.
    assert_eq!(
        told_on(Some(Harness::Codex)),
        [
            "This persona's MCP servers (`status`, `grafana`, `gsc`) are not started for this \
             chat: a persona's servers are started on Claude Code only. Their tools are not \
          here, so say so if the work needs one."
        ]
    );
    // Approved, on Claude Code: only the refused name is left to say.
    crate::personaverbs::mcp::approve(
        plane.root(),
        &plane.state(),
        "ops",
        &OPS_APPROVED.map(String::from),
    );
    assert_eq!(told_on(Some(Harness::ClaudeCode)).len(), 1);
    // A persona with no servers is told nothing.
    assert!(
        told(
            plane.root(),
            &plane.state(),
            "steward",
            Some(Harness::Codex)
        )
        .is_empty()
    );
}
