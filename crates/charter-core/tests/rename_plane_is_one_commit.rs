//! `rename-plane` (RN-7, V93g): the doctor fix that renames a project's committed files to
//! purlis's names in exactly ONE commit, marks the project as requiring `purlis-names` so a
//! build without that feature opens it read-only, and refuses — writing nothing — where the
//! other fixes refuse and on uncommitted work. Driven through the fix registry, as `charter
//! doctor --fix rename-plane` and the Doctor dialog drive it, on a real git repository.

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use charter_core::compat::{self, Compat, Why};
use charter_core::doctor::fix::{self, FixId, Fixed};
use charter_core::handoffguard::{self, Caller};
use charter_core::names;
use charter_core::scaffold::settings;

const MANIFEST: &str =
    "# This project.\nschema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n";

fn live_gitignore() -> String {
    format!(
        "/workspaces/*/*\n!/workspaces/.gitkeep\n/.charter/\n{}\n!/workspaces/alpha/workspace.json\n{}\n",
        names::LIVE_BEGIN.reads[0],
        names::LIVE_END.reads[0]
    )
}

fn gitattributes() -> String {
    format!(
        "*.png binary\n{}\npersonas/_dispatch/*.jsonl merge=union\n{}\n",
        names::MERGE_RULES_BEGIN.reads[0],
        names::MERGE_RULES_END.reads[0]
    )
}

fn readme() -> String {
    format!(
        "# Plane\n\n{}\n\n## Personas\n\n<!-- END personas -->\n",
        names::PERSONAS_BEGIN.reads[0]
    )
}

/// A `workspace.json` charter stamped under the old key.
fn stamped_manifest() -> String {
    let body = serde_json::json!({"name": "alpha", "repos": []});
    let digest = charter_core::manifest::digest(&body);
    format!(
        "{{\n  \"name\": \"alpha\",\n  \"charter_generated\": \"{digest}\",\n  \"repos\": []\n}}\n"
    )
}

const SETTINGS: &str = r#"{
  "env": {
    "CHARTER_HARNESS": "claude-code"
  },
  "permissions": {
    "ask": [
      "Bash(charter handoff *)",
      "Bash(charter report *--yes*)",
      "Bash(charter *todo*promote*)"
    ],
    "allow": [
      "Bash(charter status)"
    ]
  },
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "charter hook pretooluse",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
"#;

const OPENCODE: &str = r#"{
  "permission": {
    "bash": {
      "charter handoff *": "ask",
      "charter report *--yes*": "ask",
      "charter *todo*promote*": "ask"
    }
  }
}"#;

fn agent() -> String {
    format!(
        "---\nname: ops\n---\n<!-- {} -->\nUse `charter:handoff` to pass work on.\n",
        names::SYNC_AGENTS_MARKER.reads[0]
    )
}

const HAND_AGENT: &str = "---\nname: mine\n---\nMine. Use `charter:handoff`.\n";
const PERSONA: &str =
    "---\nrole: Ops\nskills: charter:secrets\n---\nHand off with `charter:handoff`; charter: no.\n";

/// A project as charter wrote it before the rename, committed.
fn project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let files: Vec<(&str, String)> = vec![
        ("charter.toml", MANIFEST.to_owned()),
        (
            ".charter-scan-allow.toml",
            "[[allow]]\nsha = \"x\"\n".to_owned(),
        ),
        (".gitignore", live_gitignore()),
        (".gitattributes", gitattributes()),
        ("README.md", readme()),
        ("workspaces/.gitkeep", String::new()),
        ("workspaces/alpha/workspace.json", stamped_manifest()),
        (".claude/settings.json", SETTINGS.to_owned()),
        ("opencode.json", OPENCODE.to_owned()),
        (".claude/agents/ops.md", agent()),
        (".claude/agents/mine.md", HAND_AGENT.to_owned()),
        ("personas/ops/persona.md", PERSONA.to_owned()),
    ];
    for (rel, text) in &files {
        let at = root.join(rel);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, text).unwrap();
    }
    support::git(&root, &["init", "-q", "-b", "main"]);
    // The code under test commits with whatever identity git finds; the fixture's own.
    support::git(&root, &["config", "user.name", "charter tests"]);
    support::git(&root, &["config", "user.email", "tests@example.invalid"]);
    support::git(&root, &["add", "-A"]);
    support::git(&root, &["commit", "-q", "-m", "before"]);
    (dir, root)
}

fn out(root: &Path, args: &[&str]) -> String {
    String::from_utf8(support::git(root, args).stdout).unwrap()
}

fn head(root: &Path) -> String {
    out(root, &["rev-parse", "HEAD"]).trim().to_owned()
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Every file outside `.git`, with its bytes.
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            if rel.starts_with(".git")
                && !rel.starts_with(".gitignore")
                && !rel.starts_with(".gitattributes")
            {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

fn ran(fixed: &Fixed) -> &[String] {
    match fixed {
        Fixed::Ran { said, complete } => {
            assert!(complete, "it did not finish: {said:?}");
            said
        }
        Fixed::Refused(why) => panic!("refused: {why}"),
    }
}

fn refused(fixed: Fixed) -> String {
    match fixed {
        Fixed::Refused(why) => why,
        Fixed::Ran { said, .. } => panic!("it ran: {said:?}"),
    }
}

#[test]
fn rename_plane_is_one_commit_with_exactly_the_renames() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    let before = head(&root);

    let said = ran(&fix::apply(&root, FixId::RenamePlane)).join("\n");
    assert!(said.contains("committed"), "{said}");

    // Exactly one commit, on top of the one before.
    assert_eq!(out(&root, &["rev-parse", "HEAD~1"]).trim(), before);
    assert_eq!(
        out(&root, &["rev-list", "--count", &format!("{before}..HEAD")]).trim(),
        "1"
    );
    // With exactly the expected paths.
    let changed = out(
        &root,
        &["diff", "--name-status", "--no-renames", &before, "HEAD"],
    );
    let mut changed: Vec<&str> = changed.lines().collect();
    changed.sort_unstable();
    assert_eq!(
        changed,
        [
            "A\t.purlis-scan-allow.toml",
            "A\tpurlis.toml",
            "D\t.charter-scan-allow.toml",
            "D\tcharter.toml",
            "M\t.claude/agents/ops.md",
            "M\t.claude/settings.json",
            "M\t.gitattributes",
            "M\t.gitignore",
            "M\tREADME.md",
            "M\topencode.json",
            "M\tpersonas/ops/persona.md",
            "M\tworkspaces/alpha/workspace.json",
        ]
    );
    // And nothing left behind uncommitted.
    assert_eq!(out(&root, &["status", "--porcelain"]), "");

    // The manifest: same text, plus the feature and the schema a `requires` list goes with.
    let manifest = read(&root, "purlis.toml");
    assert!(manifest.starts_with("# This project.\n"), "{manifest}");
    let doc: toml::Table = manifest.parse().unwrap();
    assert_eq!(doc["schema"].as_integer(), Some(2));
    assert_eq!(doc["forge"][0]["owner"].as_str(), Some("acme"));
    assert_eq!(
        doc["requires"][0]["feature"].as_str(),
        Some(names::PURLIS_NAMES_FEATURE)
    );
    assert_eq!(
        doc["requires"][0]["since"].as_str(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    assert!(names::plane_migrated(&root));
    assert_eq!(
        read(&root, ".purlis-scan-allow.toml"),
        "[[allow]]\nsha = \"x\"\n"
    );

    // The markers, in place.
    assert_eq!(
        read(&root, ".gitignore"),
        live_gitignore()
            .replace(names::LIVE_BEGIN.reads[0], names::LIVE_BEGIN.write)
            .replace(names::LIVE_END.reads[0], names::LIVE_END.write)
    );
    assert_eq!(
        read(&root, ".gitattributes"),
        gitattributes()
            .replace(
                names::MERGE_RULES_BEGIN.reads[0],
                names::MERGE_RULES_BEGIN.write
            )
            .replace(
                names::MERGE_RULES_END.reads[0],
                names::MERGE_RULES_END.write
            )
    );
    assert_eq!(
        read(&root, "README.md"),
        readme().replace(names::PERSONAS_BEGIN.reads[0], names::PERSONAS_BEGIN.write)
    );
    let manifest = read(&root, "workspaces/alpha/workspace.json");
    assert_eq!(
        manifest,
        stamped_manifest().replace("\"charter_generated\"", "\"purlis_generated\"")
    );
    assert_eq!(
        charter_core::manifest::ownership(Some(&manifest)),
        charter_core::manifest::Ownership::Charter,
        "the digest moved with its key, so the file stays charter's"
    );

    // Agents and personas: the marker and the skill references, nothing else.
    assert_eq!(
        read(&root, ".claude/agents/ops.md"),
        agent()
            .replace(
                names::SYNC_AGENTS_MARKER.reads[0],
                names::SYNC_AGENTS_MARKER.write
            )
            .replace("charter:handoff", "purlis:handoff")
    );
    assert_eq!(
        read(&root, ".claude/agents/mine.md"),
        HAND_AGENT,
        "a hand's agent"
    );
    assert_eq!(
        read(&root, "personas/ops/persona.md"),
        PERSONA
            .replace("charter:secrets", "purlis:secrets")
            .replace("charter:handoff", "purlis:handoff")
    );

    // Settings: the variable, the hook, each ask rule's twin beside it, no allow twin.
    let doc: serde_json::Value =
        serde_json::from_str(&read(&root, ".claude/settings.json")).unwrap();
    assert_eq!(
        doc["env"],
        serde_json::json!({"PURLIS_HARNESS": "claude-code"})
    );
    // Hook commands keep the alias for the window: it resolves on every build (D-RN7-11).
    assert_eq!(
        doc["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "charter hook pretooluse"
    );
    assert_eq!(
        doc["permissions"]["ask"],
        serde_json::json!([
            "Bash(charter handoff *)",
            "Bash(purlis handoff *)",
            "Bash(charter report *--yes*)",
            "Bash(purlis report *--yes*)",
            "Bash(charter *todo*promote*)",
            "Bash(purlis *todo*promote*)"
        ])
    );
    assert_eq!(
        doc["permissions"]["allow"],
        serde_json::json!(["Bash(charter status)"])
    );
    for pattern in settings::PURLIS_CONSENT_PATTERNS {
        assert!(
            settings::carries_consent_rule(&root, pattern),
            "{pattern} in both harnesses"
        );
    }
}

#[test]
fn a_build_without_the_feature_opens_the_renamed_project_read_only() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    ran(&fix::apply(&root, FixId::RenamePlane));

    assert_eq!(compat::read(&root), Compat::Writable, "this build has it");
    let Compat::ReadOnly(why) = compat::read_knowing(&root, &[]) else {
        panic!("a build without purlis-names must not write this project");
    };
    assert_eq!(
        why,
        Why::Missing {
            feature: names::PURLIS_NAMES_FEATURE.to_owned(),
            since: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }
    );
    let said = why.to_string();
    assert!(
        said.contains("this project requires the feature purlis-names")
            && said.contains("opens the project read-only and writes nothing to it"),
        "{said}"
    );
}

#[test]
fn a_second_run_has_nothing_to_do_and_makes_no_commit() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    ran(&fix::apply(&root, FixId::RenamePlane));
    let after = head(&root);
    let said = ran(&fix::apply(&root, FixId::RenamePlane)).join("\n");
    assert!(said.contains("nothing to do"), "{said}");
    assert_eq!(head(&root), after);
}

#[test]
fn uncommitted_work_is_refused_and_nothing_is_written() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    std::fs::write(root.join("README.md"), "edited\n").unwrap();
    let before = (head(&root), tree(&root));

    let why = refused(fix::apply(&root, FixId::RenamePlane));
    assert!(
        why.contains("not committed") && why.contains("README.md"),
        "{why}"
    );
    assert_eq!((head(&root), tree(&root)), before);
}

#[test]
fn a_project_this_charter_may_not_write_is_refused_as_every_fix_refuses_it() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    std::fs::write(root.join("charter.toml"), "schema = 99\n").unwrap();
    support::git(&root, &["commit", "-qam", "from the future"]);
    let before = (head(&root), tree(&root));

    let why = refused(fix::apply(&root, FixId::RenamePlane));
    assert!(why.contains("declares schema 99"), "{why}");
    assert!(why.contains("read-only"), "{why}");
    assert_eq!((head(&root), tree(&root)), before);
}

#[test]
fn a_file_under_both_names_is_refused() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    std::fs::write(root.join("purlis.toml"), "schema = 1\n").unwrap();
    support::git(&root, &["add", "purlis.toml"]);
    support::git(&root, &["commit", "-qm", "both"]);
    let before = (head(&root), tree(&root));

    let why = refused(fix::apply(&root, FixId::RenamePlane));
    assert!(why.contains("both purlis.toml and charter.toml"), "{why}");
    assert_eq!((head(&root), tree(&root)), before);
}

/// A step that fails part-way — here a file somebody made read-only, which charter never writes
/// over — leaves the project as it was: the renames before it are undone, nothing is committed.
#[cfg(unix)]
#[test]
fn a_step_that_fails_puts_every_file_back() {
    charter_core::unsteered!();
    use std::os::unix::fs::PermissionsExt;
    let (_dir, root) = project();
    let readme = root.join("README.md");
    std::fs::set_permissions(&readme, std::fs::Permissions::from_mode(0o444)).unwrap();
    let before = (head(&root), tree(&root));

    let why = refused(fix::apply(&root, FixId::RenamePlane));
    assert!(why.contains("README.md"), "{why}");
    assert!(why.contains("put back as it was"), "{why}");
    assert_eq!((head(&root), tree(&root)), before);
    assert_eq!(out(&root, &["status", "--porcelain"]), "");
}

/// D-RN3-4 and D-RN3-9 lifted: in a project that carries the purlis rules, the purlis spelling
/// of a consent-gated command is the host's to ask about; before the rename it is refused.
#[test]
fn the_purlis_spelling_is_let_through_once_the_project_carries_its_rules() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    let caller = Caller {
        agent_id: None,
        harness: Some("claude-code"),
        permission_mode: None,
    };
    let handoff = "purlis handoff beta <<'BRIEF'\nDo the thing.\nBRIEF";
    let report = "purlis report bug --yes 0123";
    let promote = "purlis ws todo promote 1";

    assert!(handoffguard::handoff_refusal_in(handoff, caller, &root, &root).is_some());
    for cmd in [report, promote] {
        assert!(
            charter_core::consentspelling::refusal(cmd, &root, &root).is_some(),
            "{cmd} before the rename"
        );
    }

    ran(&fix::apply(&root, FixId::RenamePlane));

    assert_eq!(
        handoffguard::handoff_refusal_in(handoff, caller, &root, &root),
        None
    );
    for cmd in [report, promote, handoff] {
        assert_eq!(
            charter_core::consentspelling::refusal(cmd, &root, &root),
            None,
            "{cmd} after the rename"
        );
    }
    // Still refused: spellings no rule matches, and the guard's other refusals.
    for cmd in [
        "/usr/local/bin/purlis report bug --yes x",
        "PURLIS report bug --yes x",
        "python3 -m purlis report bug --yes x",
    ] {
        assert!(
            charter_core::consentspelling::refusal(cmd, &root, &root).is_some(),
            "{cmd}"
        );
    }
    for cmd in [
        "'purlis' handoff beta <<'BRIEF'\nx\nBRIEF",
        "purlis  handoff beta <<'BRIEF'\nx\nBRIEF",
        "bash -c 'purlis handoff beta'",
        "purlis handoff beta <<BRIEF\nx\nBRIEF",
    ] {
        assert!(
            handoffguard::handoff_refusal_in(cmd, caller, &root, &root).is_some(),
            "{cmd}"
        );
    }
}

/// Fails closed: a purlis rule in one harness and not the other lets nothing through.
#[test]
fn a_purlis_rule_in_one_harness_only_is_not_enough() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    ran(&fix::apply(&root, FixId::RenamePlane));
    std::fs::write(root.join("opencode.json"), OPENCODE).unwrap();

    assert!(!settings::carries_consent_rule(
        &root,
        settings::PURLIS_REPORT_PATTERN
    ));
    assert!(
        charter_core::consentspelling::refusal("purlis report bug --yes x", &root, &root).is_some()
    );
    let caller = Caller {
        agent_id: None,
        harness: Some("claude-code"),
        permission_mode: None,
    };
    assert!(
        handoffguard::handoff_refusal_in(
            "purlis handoff beta <<'BRIEF'\nx\nBRIEF",
            caller,
            &root,
            &root
        )
        .is_some()
    );
}

fn caller() -> Caller<'static> {
    Caller {
        agent_id: None,
        harness: Some("claude-code"),
        permission_mode: None,
    }
}

const REPORT: &str = "purlis report bug --yes 0123";
const HANDOFF: &str = "purlis handoff beta <<'BRIEF'\nDo the thing.\nBRIEF";

/// The host applies the settings of the layer a chat runs in. A layer that does not carry the
/// twin — here one a hand edited, which no rewrite reaches — keeps the new spelling refused
/// there, though the project root carries it (fails closed).
#[test]
fn a_layer_without_the_twin_still_refuses_the_new_spelling_there() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    ran(&fix::apply(&root, FixId::RenamePlane));
    let ws = root.join("workspaces/alpha");
    std::fs::create_dir_all(ws.join(".claude")).unwrap();
    std::fs::write(ws.join(".claude/settings.json"), SETTINGS).unwrap();

    assert_eq!(
        charter_core::consentspelling::refusal(REPORT, &root, &root),
        None
    );
    assert!(charter_core::consentspelling::refusal(REPORT, &root, &ws).is_some());
    assert!(handoffguard::handoff_refusal_in(HANDOFF, caller(), &root, &ws).is_some());
    assert_eq!(
        handoffguard::handoff_refusal_in(HANDOFF, caller(), &root, &root),
        None
    );

    // With the twin in the layer as well, it is the host's to ask about there too.
    let twinned = read(&root, ".claude/settings.json");
    std::fs::write(ws.join(".claude/settings.json"), twinned).unwrap();
    assert_eq!(
        charter_core::consentspelling::refusal(REPORT, &root, &ws),
        None
    );
    assert_eq!(
        handoffguard::handoff_refusal_in(HANDOFF, caller(), &root, &ws),
        None
    );

    // A cwd that cannot be placed inside the project fails closed.
    let elsewhere = tempfile::tempdir().unwrap();
    assert!(charter_core::consentspelling::refusal(REPORT, &root, elsewhere.path()).is_some());
}

/// A twin weaker than its charter rule — an ask beside a deny — does not lift the refusal.
#[test]
fn a_twin_weaker_than_its_charter_rule_is_not_enough() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    ran(&fix::apply(&root, FixId::RenamePlane));
    let path = root.join(".claude/settings.json");
    let mut doc: serde_json::Value =
        serde_json::from_str(&read(&root, ".claude/settings.json")).unwrap();
    doc["permissions"]["deny"] = serde_json::json!(["Bash(charter report *--yes*)"]);
    std::fs::write(&path, doc.to_string()).unwrap();

    assert!(charter_core::consentspelling::refusal(REPORT, &root, &root).is_some());
    doc["permissions"]["deny"] = serde_json::json!([
        "Bash(charter report *--yes*)",
        "Bash(purlis report *--yes*)"
    ]);
    std::fs::write(&path, doc.to_string()).unwrap();
    assert_eq!(
        charter_core::consentspelling::refusal(REPORT, &root, &root),
        None
    );
}

/// Only the new name as the source spells it, bare, at the start of its segment: a quoted,
/// escaped or split word lexes to the same name and matches no host rule.
#[test]
fn a_quoted_escaped_or_split_new_name_stays_refused() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    ran(&fix::apply(&root, FixId::RenamePlane));
    for cmd in [
        "'purlis' report bug --yes x",
        "\\purlis ws todo promote 1",
        "pur''lis report bug --yes x",
        "\"purlis\" report bug --yes x",
        "FOO=1 purlis report bug --yes x",
        "echo ok && 'purlis' report bug --yes x",
    ] {
        assert!(
            charter_core::consentspelling::refusal(cmd, &root, &root).is_some(),
            "{cmd}"
        );
    }
    for cmd in [
        "purlis report bug --yes x",
        "cd /tmp && purlis ws todo promote 1",
    ] {
        assert_eq!(
            charter_core::consentspelling::refusal(cmd, &root, &root),
            None,
            "{cmd}"
        );
    }
}

/// D-RN7-12: from inside a chat the fix is refused, and nothing is written.
#[test]
fn a_chat_cannot_run_rename_plane() {
    charter_core::unsteered!();
    let (_dir, root) = project();
    let before = (head(&root), tree(&root));
    let in_a_chat =
        |name: &str| (name == charter_core::active::SESSION_ID_ENV).then(|| "chat-1".to_owned());
    let why = refused(fix::rename_plane::apply_in(&root, &in_a_chat));
    assert!(why.contains("not run from inside a chat"), "{why}");
    assert_eq!((head(&root), tree(&root)), before);
}
