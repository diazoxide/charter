use super::*;

fn plane() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".claude")).unwrap();
    (dir, root)
}

fn json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn a_command_is_wrapped_as_bash_and_a_rule_is_kept_as_it_is() {
    assert_eq!(
        as_rule("terraform apply *").unwrap(),
        "Bash(terraform apply *)"
    );
    assert_eq!(as_rule("  git status *  ").unwrap(), "Bash(git status *)");
    assert_eq!(as_rule("Read(./.env)").unwrap(), "Read(./.env)");
    assert_eq!(as_rule("WebFetch").unwrap(), "WebFetch");
    assert_eq!(as_rule("mcp__slack__send").unwrap(), "mcp__slack__send");
    // A prefix is not a tool: `Globalprotect` is a binary.
    assert_eq!(
        as_rule("Globalprotect --connect").unwrap(),
        "Bash(Globalprotect --connect)"
    );
    assert!(as_rule("mcp__slack__send *").is_err());
    assert!(as_rule("   ").is_err());
}

#[test]
fn handoff_puts_the_rule_back_in_both_harnesses_and_touches_nothing_else() {
    let (_d, root) = plane();
    std::fs::write(
        root.join(".claude/settings.json"),
        r#"{"env": {"CHARTER_HARNESS": "claude-code"}, "permissions": {"deny": ["Bash(rm -rf /*)"]}}"#,
    )
    .unwrap();
    let (said, code) = report(
        &root,
        &as_rule(HANDOFF_PATTERN).unwrap(),
        Bucket::Ask,
        false,
    );
    assert_eq!(code, 0, "{said}");
    let doc = json(&root.join(".claude/settings.json"));
    assert_eq!(
        doc["permissions"]["ask"],
        serde_json::json!(["Bash(charter handoff *)"])
    );
    assert_eq!(
        doc["permissions"]["deny"],
        serde_json::json!(["Bash(rm -rf /*)"])
    );
    assert_eq!(doc["env"]["CHARTER_HARNESS"], "claude-code");
    assert_eq!(
        json(&root.join("opencode.json"))["permission"]["bash"]["charter handoff *"],
        "ask"
    );
    assert!(
        said.contains("codex: Codex has no command-pattern permissions"),
        "{said}"
    );

    let (again, _) = report(&root, "Bash(charter handoff *)", Bucket::Ask, false);
    assert!(again.contains("claude-code: already asking for"), "{again}");
    assert_eq!(
        json(&root.join(".claude/settings.json"))["permissions"]["ask"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn a_file_one_harness_cannot_read_blocks_every_write() {
    let (_d, root) = plane();
    std::fs::write(root.join("opencode.json"), "{broken").unwrap();
    let (said, code) = report(&root, "Bash(terraform apply *)", Bucket::Ask, false);
    assert_eq!(code, 1);
    assert!(said.contains("Nothing was written."), "{said}");
    assert!(said.contains("Still without it: claude-code"), "{said}");
    assert!(!root.join(".claude/settings.json").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("opencode.json")).unwrap(),
        "{broken"
    );
}

#[test]
fn a_local_rule_goes_to_the_machine_local_file_and_opencode_says_it_cannot_hold_one() {
    let (_d, root) = plane();
    let (said, code) = report(&root, "Bash(git status *)", Bucket::Allow, true);
    assert_eq!(code, 0, "{said}");
    assert_eq!(
        json(&root.join(".claude/settings.local.json"))["permissions"]["allow"],
        serde_json::json!(["Bash(git status *)"])
    );
    assert!(!root.join(".claude/settings.json").exists());
    assert!(!root.join("opencode.json").exists());
    assert!(
        said.contains("opencode has no machine-local settings file"),
        "{said}"
    );
    assert!(said.contains("Machine-local"), "{said}");
}

#[test]
fn a_shared_allow_says_it_is_committed_and_does_not_reach_a_workspace() {
    let (_d, root) = plane();
    std::fs::write(
        root.join(".claude/settings.json"),
        r#"{"permissions": {"ask": ["Bash(git push *)"]}}"#,
    )
    .unwrap();
    let (said, code) = report(&root, "Bash(git push *)", Bucket::Allow, false);
    assert_eq!(code, 0, "{said}");
    assert!(said.contains("COMMITTED"), "{said}");
    assert!(
        said.contains("never allow, so a chat started there is still asked"),
        "{said}"
    );
    assert!(said.contains("is also an ask rule"), "{said}");
    assert_eq!(
        json(&root.join("opencode.json"))["permission"]["bash"]["git push *"],
        "allow"
    );
}

#[test]
fn a_tool_rule_is_claude_codes_alone() {
    let (_d, root) = plane();
    let (said, _) = report(&root, "Read(./.env)", Bucket::Ask, false);
    assert!(
        said.contains("opencode: opencode's config holds command patterns"),
        "{said}"
    );
    assert!(!root.join("opencode.json").exists());
}

#[test]
fn list_groups_each_rule_under_the_file_it_lives_in() {
    let (_d, root) = plane();
    std::fs::write(
        root.join(".claude/settings.json"),
        r#"{"permissions": {"ask": ["Bash(charter handoff *)"], "allow": ["Bash(ls *)"], "deny": ["Bash(x)"]}}"#,
    )
    .unwrap();
    std::fs::write(root.join(".claude/settings.local.json"), "[1]").unwrap();
    assert_eq!(
        list(&root).0,
        ".claude/settings.json (committed: everyone on this repo)\n  ask   Bash(charter handoff \
         *)\n  allow Bash(ls *)\n.claude/settings.local.json (this machine only)\n  not a JSON \
         object charter can read — its rules are not listed\n"
    );
}

#[test]
fn a_blocked_write_says_where_the_rule_already_is() {
    let (_d, root) = plane();
    std::fs::write(
        root.join(".claude/settings.json"),
        r#"{"permissions": {"ask": ["Bash(terraform apply *)"]}}"#,
    )
    .unwrap();
    std::fs::write(root.join("opencode.json"), "{broken").unwrap();
    let (said, code) = report(&root, "Bash(terraform apply *)", Bucket::Ask, false);
    assert_eq!(code, 1);
    assert!(
        said.contains("already in force under claude-code"),
        "{said}"
    );
    assert!(said.contains("uneven"), "{said}");
}

#[cfg(unix)]
#[test]
fn a_claude_folder_linked_out_of_the_plane_is_written_through_by_nothing() {
    let (d, root) = plane();
    std::fs::remove_dir(root.join(".claude")).unwrap();
    let outside = d.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join(".claude")).unwrap();
    let (said, code) = report(&root, "Bash(terraform apply *)", Bucket::Ask, false);
    assert_eq!(code, 1, "{said}");
    assert!(!outside.join("settings.json").exists());
    assert!(
        !root.join("opencode.json").exists(),
        "all or nothing: {said}"
    );
}
