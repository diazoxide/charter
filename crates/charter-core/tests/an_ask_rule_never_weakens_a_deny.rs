//! An ask rule charter writes never weakens a deny that is already there: not through a project
//! template (FR-17), and not through `charter guard ask`, which share one writer.
//!
//! opencode decides a command by **the last matching rule** in `permission.bash` ("Rules are
//! evaluated by pattern match, with the last matching rule winning", opencode.ai/docs/
//! permissions). So an ask appended after `"*": "deny"` would let a denied command through to a
//! prompt. Claude Code weighs deny before ask whatever the order, so there it is only the exact
//! rule that has to be said, not moved.

use std::path::Path;

use charter_core::guardcmd::{self, Bucket};
use charter_core::template;

fn project() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("a directory");
    let root = charter_core::firstrun::ensure_local_plane(&dir.path().join("config"))
        .expect("a local project");
    (dir, root)
}

/// `opencode.json` with `permission.bash` set to `rules`, in this order.
fn opencode_with(root: &Path, rules: &[(&str, &str)]) {
    let mut bash = serde_json::Map::new();
    for (glob, decision) in rules {
        bash.insert((*glob).to_owned(), serde_json::Value::from(*decision));
    }
    let doc = serde_json::json!({ "permission": { "bash": bash } });
    std::fs::write(root.join("opencode.json"), doc.to_string()).expect("written");
}

/// `permission.bash` as opencode reads it: in order.
fn opencode_rules(root: &Path) -> Vec<(String, String)> {
    let text = std::fs::read_to_string(root.join("opencode.json")).expect("read");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    doc["permission"]["bash"]
        .as_object()
        .expect("an object")
        .iter()
        .map(|(glob, decision)| (glob.clone(), decision.as_str().unwrap_or("").to_owned()))
        .collect()
}

/// What opencode decides for `command`: the last rule whose glob matches it.
fn opencode_decides(root: &Path, command: &str) -> Option<String> {
    opencode_rules(root)
        .into_iter()
        .rev()
        .find(|(glob, _)| glob::Pattern::new(glob).is_ok_and(|pattern| pattern.matches(command)))
        .map(|(_, decision)| decision)
}

fn rust() -> &'static template::Template {
    template::named("rust").expect("the Rust template")
}

#[test]
fn a_template_leaves_a_command_opencode_denies_denied() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    opencode_with(&root, &[("cargo publish *", "deny")]);

    let applied = template::apply(&root, rust(), None).expect("laid out");

    assert_eq!(
        opencode_decides(&root, "cargo publish --dry-run").as_deref(),
        Some("deny")
    );
    assert_eq!(
        applied.denied,
        ["Bash(cargo publish *)"],
        "said, not passed over"
    );
}

#[test]
fn a_template_never_lets_a_broader_opencode_deny_be_outranked() {
    charter_core::unsteered!();
    for broader in ["*", "cargo *"] {
        let (_dir, root) = project();
        opencode_with(&root, &[(broader, "deny"), ("git status", "allow")]);

        template::apply(&root, rust(), None).expect("laid out");

        assert_eq!(
            opencode_decides(&root, "cargo publish --dry-run").as_deref(),
            Some("deny"),
            "{broader}: {:?}",
            opencode_rules(&root)
        );
        assert_eq!(
            opencode_decides(&root, "git status").as_deref(),
            Some("allow"),
            "what was there decides as it did"
        );
    }
}

#[test]
fn guard_ask_leaves_a_command_opencode_denies_denied_and_says_so() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    opencode_with(&root, &[("cargo publish *", "deny")]);

    let (said, code) = guardcmd::report(&root, "Bash(cargo publish *)", Bucket::Ask, false);

    assert_eq!(code, 0, "{said}");
    assert!(said.contains("opencode: already denies"), "{said}");
    assert_eq!(
        opencode_decides(&root, "cargo publish --dry-run").as_deref(),
        Some("deny")
    );
}

#[test]
fn guard_ask_never_lets_a_broader_opencode_deny_be_outranked() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    opencode_with(&root, &[("cargo *", "deny")]);

    let (said, code) = guardcmd::report(&root, "Bash(cargo publish *)", Bucket::Ask, false);

    assert_eq!(code, 0, "{said}");
    assert_eq!(
        opencode_decides(&root, "cargo publish --dry-run").as_deref(),
        Some("deny"),
        "{:?}",
        opencode_rules(&root)
    );
}

#[test]
fn an_ask_no_deny_matches_is_in_force_beside_a_narrower_deny() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    opencode_with(&root, &[("rm *", "deny")]);

    guardcmd::report(&root, "Bash(cargo publish *)", Bucket::Ask, false);

    assert_eq!(
        opencode_decides(&root, "cargo publish --dry-run").as_deref(),
        Some("ask"),
        "{:?}",
        opencode_rules(&root)
    );
    assert_eq!(opencode_decides(&root, "rm -rf x").as_deref(), Some("deny"));
}

#[test]
fn a_command_claude_code_denies_is_not_also_made_an_ask() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    let settings = root.join(".claude/settings.json");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).expect("read")).expect("JSON");
    doc["permissions"]["deny"] = serde_json::json!(["Bash(cargo publish *)"]);
    std::fs::write(&settings, doc.to_string()).expect("written");

    let applied = template::apply(&root, rust(), None).expect("laid out");
    let (said, _) = guardcmd::report(&root, "Bash(cargo publish *)", Bucket::Ask, false);

    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).expect("read")).expect("JSON");
    assert!(
        !after["permissions"]["ask"]
            .as_array()
            .expect("a list")
            .iter()
            .any(|rule| rule == "Bash(cargo publish *)"),
        "{after}"
    );
    assert_eq!(
        after["permissions"]["deny"],
        serde_json::json!(["Bash(cargo publish *)"])
    );
    assert!(said.contains("claude-code: already denies"), "{said}");
    assert!(
        applied.denied.contains(&"Bash(cargo publish *)".to_owned()),
        "{applied:?}"
    );
}

/// An allowlist: everything denied, then the commands the project lets through.
const ALLOWLIST: &[(&str, &str)] = &[("*", "deny"), ("cargo *", "allow"), ("git status", "allow")];

#[test]
fn guard_ask_is_in_force_in_an_allowlist_where_a_broader_allow_matches() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    opencode_with(&root, ALLOWLIST);

    let (said, code) = guardcmd::report(&root, "Bash(cargo publish *)", Bucket::Ask, false);

    assert_eq!(code, 0, "{said}");
    assert_eq!(
        opencode_decides(&root, "cargo publish --dry-run").as_deref(),
        Some("ask"),
        "the ask is what opencode answers, not the allow before it: {:?}",
        opencode_rules(&root)
    );
    assert_eq!(
        opencode_decides(&root, "cargo build").as_deref(),
        Some("allow")
    );
    assert_eq!(
        opencode_decides(&root, "git status").as_deref(),
        Some("allow")
    );
    assert_eq!(opencode_decides(&root, "rm -rf x").as_deref(), Some("deny"));
}

#[test]
fn a_template_ask_is_in_force_in_an_allowlist_and_a_later_deny_still_wins() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    opencode_with(
        &root,
        &[
            ("*", "deny"),
            ("cargo *", "allow"),
            ("cargo publish --token *", "deny"),
        ],
    );

    template::apply(&root, rust(), None).expect("laid out");

    assert_eq!(
        opencode_decides(&root, "cargo publish --dry-run").as_deref(),
        Some("ask"),
        "{:?}",
        opencode_rules(&root)
    );
    assert_eq!(
        opencode_decides(&root, "cargo publish --token x").as_deref(),
        Some("deny"),
        "a deny after the allow still decides"
    );
}
