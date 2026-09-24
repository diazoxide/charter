//! A persona's MCP servers against the recorded scenarios' `ops` plane: what is declared and
//! refused, the consent lines and fingerprints the Python charter wrote, and this machine's
//! approvals file (`persona-sync-agents-…` in `tests/fixtures/recorded/behaviour.jsonl`).

use serde_json::json;

use super::*;
use crate::personaverbs::tests_plane::{OPS_APPROVED, Plane};

/// `ops/grafana`'s and `ops/gsc`'s consent lines, as the recorded rows print them.
const GRAFANA_LINE: &str = "run npx -y grafana-mcp@1.2.0  type \"stdio\"  env \
                            \"GRAFANA_URL\"=\"https://grafana.example.com\"  secrets \
                            \"GRAFANA_TOKEN\"=\"grafana-token\"  vault \"ops\"";
const GSC_LINE: &str = "run uvx gsc-mcp==0.3.0  type \"stdio\"  secret_files \
                        \"GOOGLE_APPLICATION_CREDENTIALS\"=\"GOOGLE_SA\"  vault \"ops\"";

fn sha256(line: &str) -> String {
    use sha2::Digest as _;
    format!("{:x}", sha2::Sha256::digest(line.as_bytes()))
}

#[test]
fn a_persona_declares_its_lineages_servers_in_first_declared_order_and_refuses_bad_names_once() {
    let plane = Plane::daily_with_ops();
    let (kept, refused) = declared(plane.root(), "ops");
    assert_eq!(
        kept.keys().collect::<Vec<_>>(),
        ["status", "grafana", "gsc"]
    );
    assert_eq!(refused, ["bad name"]);
    // A child inherits its parent's servers; its own entry for one replaces the parent's in
    // the parent's position, and a bad name both declare is refused once.
    plane.write(
        "personas/ops-lite/mcp.json",
        r#"{"mcpServers": {"extra": {"url": "u"}, "status": {"url": "mine"}, "bad name": {}}}"#,
    );
    let (kept, refused) = declared(plane.root(), "ops-lite");
    assert_eq!(
        kept.keys().collect::<Vec<_>>(),
        ["status", "grafana", "gsc", "extra"]
    );
    assert_eq!(kept["status"], json!({"url": "mine"}));
    assert_eq!(refused, ["bad name"]);
    assert_eq!(declared(plane.root(), "steward"), (Map::new(), vec![]));
}

#[test]
fn an_mcp_file_that_is_not_a_server_map_declares_nothing() {
    let plane = Plane::daily_with_ops();
    for text in [
        "not json",
        "[1]",
        "{}",
        r#"{"mcpServers": null}"#,
        r#"{"mcpServers": {}}"#,
        r#"{"mcpServers": ["status"]}"#,
        r#"{"mcpServers": "status"}"#,
    ] {
        plane.write("personas/ops/mcp.json", text);
        assert_eq!(
            declared(plane.root(), "ops"),
            (Map::new(), vec![]),
            "{text}"
        );
    }
}

#[cfg(unix)]
#[test]
fn an_mcp_file_linked_from_outside_the_plane_is_not_read() {
    let plane = Plane::daily_with_ops();
    let outside = tempfile::tempdir().unwrap();
    let file = outside.path().join("mcp.json");
    std::fs::write(&file, crate::personaverbs::tests_plane::OPS_MCP).unwrap();
    std::fs::remove_file(plane.path("personas/ops/mcp.json")).unwrap();
    std::os::unix::fs::symlink(&file, plane.path("personas/ops/mcp.json")).unwrap();
    assert_eq!(declared(plane.root(), "ops"), (Map::new(), vec![]));
}

#[test]
fn a_credential_is_a_non_empty_secrets_or_secret_files_map_and_consent_needs_a_vault() {
    assert!(!declares_credential(&json!({"command": "x"})));
    assert!(!declares_credential(
        &json!({"secrets": {}, "secret_files": []})
    ));
    assert!(declares_credential(&json!({"secrets": {"T": "k"}})));
    assert!(declares_credential(&json!({"secret_files": {"F": "k"}})));
    let entry = json!({"command": "x", "secrets": {"T": "k"}});
    assert!(needs_consent(Some("ops"), &entry));
    assert!(!needs_consent(None, &entry));
    assert!(!needs_consent(Some(""), &entry));
    assert!(!needs_consent(Some("ops"), &json!({"command": "x"})));
}

#[test]
fn the_consent_lines_and_fingerprints_are_the_ones_python_recorded() {
    let plane = Plane::daily_with_ops();
    let found = credentialed(plane.root(), "ops");
    let got: Vec<(&str, &str)> = found
        .iter()
        .map(|c| (c.server.as_str(), c.line.as_str()))
        .collect();
    assert_eq!(got, [("grafana", GRAFANA_LINE), ("gsc", GSC_LINE)]);
    let fps: BTreeSet<&str> = found
        .iter()
        .map(|c| c.fingerprint.as_deref().unwrap())
        .collect();
    assert_eq!(fps, OPS_APPROVED.into_iter().collect());
    assert_eq!(found[0].fingerprint, Some(sha256(GRAFANA_LINE)));
    // `ops-lite` inherits the servers under `vault: none`: nothing of it carries a credential.
    assert!(credentialed(plane.root(), "ops-lite").is_empty());
}

#[test]
fn an_approval_is_recorded_per_persona_replacing_that_personas_set_and_no_other() {
    let plane = Plane::daily_with_ops();
    let state = plane.state();
    assert_eq!(approvals_path(&state), state.join("mcp-approved.json"));
    assert!(approved(&state, "ops").is_empty());
    approve(plane.root(), &state, "dev", &["abc".to_string()]);
    let fps: Vec<String> = [OPS_APPROVED[1], "", OPS_APPROVED[0], OPS_APPROVED[1]]
        .map(String::from)
        .to_vec();
    approve(plane.root(), &state, "ops", &fps);
    assert_eq!(
        plane.read(".charter/mcp-approved.json"),
        format!(
            "{{\n  \"dev\": [\n    \"abc\"\n  ],\n  \"ops\": [\n    \"{}\",\n    \"{}\"\n  ]\n}}\n",
            OPS_APPROVED[0], OPS_APPROVED[1]
        )
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(approvals_path(&state))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert_eq!(
        approved(&state, "ops"),
        OPS_APPROVED.map(String::from).into_iter().collect()
    );
    assert_eq!(approved(&state, "dev"), ["abc".to_string()].into());
    approve(plane.root(), &state, "ops", &[]);
    assert!(approved(&state, "ops").is_empty());
    assert_eq!(approved(&state, "dev"), ["abc".to_string()].into());
}

#[test]
fn an_approvals_file_that_does_not_read_approves_nothing_and_non_strings_are_skipped() {
    let plane = Plane::daily_with_ops();
    let state = plane.state();
    plane.write(
        ".charter/mcp-approved.json",
        r#"{"ops": ["a", 1, null, "b"], "x": "a"}"#,
    );
    assert_eq!(
        approved(&state, "ops"),
        ["a".to_string(), "b".to_string()].into()
    );
    assert!(approved(&state, "x").is_empty());
    plane.write(".charter/mcp-approved.json", "[\"a\"]");
    assert!(approved(&state, "ops").is_empty());
}

#[test]
fn a_credentialed_server_is_withheld_until_its_own_line_is_approved() {
    let plane = Plane::daily_with_ops();
    let state = plane.state();
    assert_eq!(
        withheld(plane.root(), &state, "ops"),
        [
            ("grafana".to_string(), GRAFANA_LINE.to_string()),
            ("gsc".to_string(), GSC_LINE.to_string())
        ]
    );
    approve(plane.root(), &state, "ops", &[sha256(GSC_LINE)]);
    assert_eq!(
        withheld(plane.root(), &state, "ops"),
        [("grafana".to_string(), GRAFANA_LINE.to_string())]
    );
    approve(plane.root(), &state, "ops", &OPS_APPROVED.map(String::from));
    assert!(withheld(plane.root(), &state, "ops").is_empty());
}

#[test]
fn a_quote_in_committed_text_is_escaped_in_the_consent_line() {
    let entry = json!({"command": "echo", "args": ["say \"hi\""], "note": "a\"b"});
    assert_eq!(
        describe(Some("v"), &entry),
        "run echo \"say \\\"hi\\\"\"  \"note\" \"a\\\"b\"  vault \"v\""
    );
}

#[test]
fn only_a_non_empty_env_or_secrets_map_is_shown_as_pairs() {
    let entry = json!({"command": "x", "type": {"k": "v"}, "env": {}, "secrets": {"T": "t"}});
    assert_eq!(
        describe(Some("v"), &entry),
        "run x  type {\"k\": \"v\"}  env {}  secrets \"T\"=\"t\"  vault \"v\""
    );
}

#[test]
fn a_backslash_in_committed_text_is_escaped_in_the_consent_line() {
    let entry = json!({"command": "C:\\bin\\x"});
    assert_eq!(
        describe(Some("v"), &entry),
        "run C:\\\\bin\\\\x  vault \"v\""
    );
}

#[test]
fn a_consent_line_of_exactly_the_ceiling_is_shown_and_one_longer_is_not() {
    // `run <command>  vault "v"` is the command and 15 characters more.
    let line = |n: usize| describe(Some("v"), &json!({"command": "x".repeat(n)}));
    assert_eq!(line(MAX_LINE - 15).chars().count(), MAX_LINE);
    assert_eq!(line(MAX_LINE - 14), "");
}

#[test]
fn a_label_part_of_exactly_the_limit_is_kept_whole() {
    let exact = "x".repeat(35);
    assert_eq!(label(&[&exact]), exact);
    assert_eq!(label(&[&"x".repeat(36)]), format!("{}...", "x".repeat(32)));
}

#[test]
fn a_wrapped_servers_args_are_what_pythons_list_makes_of_them() {
    let wrap = |args: serde_json::Value| {
        let entry = json!({"command": "run", "args": args, "secrets": {"T": "t"}});
        let ok: BTreeSet<String> = [fingerprint(Some("v"), &entry).unwrap()].into();
        render_entry(Some("v"), &entry, &ok)["args"].clone()
    };
    let head = ["secret", "exec", "v", "--env", "T=t", "--exec", "--", "run"];
    let with = |tail: &[&str]| json!(head.iter().chain(tail).collect::<Vec<_>>());
    assert_eq!(wrap(json!(["a", "b"])), with(&["a", "b"]));
    assert_eq!(wrap(json!("ab")), with(&["a", "b"]));
    assert_eq!(wrap(json!({"k1": 1, "k2": 2})), with(&["k1", "k2"]));
    assert_eq!(wrap(json!(7)), with(&[]));
}
