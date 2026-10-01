//! FR-24: a project that requires a feature this charter lacks is read-only to it.
//!
//! A team shares one project through git, and its members upgrade charter on their own
//! schedules. When a newer charter starts writing something an older one would mangle, it
//! names a feature in `charter.toml`'s `requires`, and the older charter must then write
//! nothing to the project and say which version it needs (`docs/plane-format.md`,
//! *Compatibility across charter versions*). These tests run the binary on such a project and
//! compare the whole tree before and after.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const REQUIRES: &str = "schema = 2\n\
    requires = [{ feature = \"memory-proposals\", since = \"0.9.0\" }]\n\
    \n\
    [memory]\n\
    share = \"local\"\n";

struct Project {
    _tmp: tempfile::TempDir,
    fence: PathBuf,
    root: PathBuf,
    home: PathBuf,
}

impl Project {
    fn new(manifest: &str) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let fence = tmp.path().canonicalize().unwrap();
        let root = fence.join("project");
        let home = fence.join("home");
        std::fs::create_dir_all(root.join("workspaces")).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(root.join("charter.toml"), manifest).unwrap();
        Project {
            _tmp: tmp,
            fence,
            root,
            home,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_charter"))
            .args(args)
            .current_dir(&self.root)
            .env("CHARTER_PLANE_FENCE", &self.fence)
            .env_remove("CHARTER_ROOT")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("HOME", &self.home)
            .env("PATH", "/usr/bin:/bin")
            .env("NO_COLOR", "1")
            .output()
            .expect("the binary runs")
    }

    /// A project a charter has been used on — scaffolded, with a workspace that has a memory
    /// and a session record — which then names a feature this charter lacks. A read command
    /// run here has real files to read, so a write it makes on the way is not hidden by an
    /// empty project's early return.
    fn lived_in() -> Self {
        let project = Project::new("");
        std::fs::remove_file(project.root.join("charter.toml")).unwrap();
        for args in [
            vec!["init", "--forge", "github", "--owner", "acme"],
            vec![
                "workspace",
                "create",
                "alpha",
                "--vision",
                "Ship the widget",
            ],
            vec![
                "workspace",
                "remember",
                "The widget ships on Friday",
                "-w",
                "alpha",
            ],
        ] {
            let out = project.run(&args);
            assert!(
                out.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let sessions = project.root.join("workspaces/alpha/sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(
            sessions.join("20260501-120000-the-widget.md"),
            "---\ntitle: The widget\ndate: 2026-05-01 12:00:00\nchat: 1\nchat-name: unknown\n\
             persona: none\nharness: claude\nprofile: unknown\nconversation: unknown\n\
             workspace: alpha\ncwd: unknown\n---\n\n# The widget\n\n## Goal\nShip it.\n",
        )
        .unwrap();
        let manifest = project.root.join("charter.toml");
        let written = std::fs::read_to_string(&manifest).unwrap();
        let required = written.replacen(
            "schema = 1\n",
            "schema = 2\nrequires = [{ feature = \"memory-proposals\", since = \"0.9.0\" }]\n",
            1,
        );
        assert_ne!(written, required, "init writes `schema = 1`: {written}");
        std::fs::write(&manifest, required).unwrap();
        project
    }

    /// Every file under the project, with its bytes.
    fn tree(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(dir: &Path, base: &Path, into: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let rel = path.strip_prefix(base).unwrap().to_path_buf();
                if path.is_dir() {
                    into.insert(rel, Vec::new());
                    walk(&path, base, into);
                } else {
                    into.insert(rel, std::fs::read(&path).unwrap());
                }
            }
        }
        let mut tree = BTreeMap::new();
        walk(&self.root, &self.root, &mut tree);
        tree
    }
}

#[test]
fn an_older_charter_writes_nothing_to_a_project_that_requires_a_feature_it_lacks() {
    let project = Project::new(REQUIRES);
    let before = project.tree();
    for args in [
        vec!["workspace", "create", "demo"],
        vec!["persona", "default", "steward"],
    ] {
        let out = project.run(&args);
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?} must be refused: {said}");
        assert!(said.contains("memory-proposals"), "{args:?}: {said}");
        assert!(said.contains("0.9.0"), "{args:?} names the version: {said}");
        assert_eq!(project.tree(), before, "{args:?} wrote to the project");
    }
}

#[test]
fn doctor_fix_writes_nothing_to_a_read_only_project() {
    let project = Project::new(REQUIRES);
    let before = project.tree();
    let out = project.run(&["doctor", "--fix"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{said}");
    assert!(said.contains("memory-proposals"), "{said}");
    assert_eq!(project.tree(), before, "doctor --fix wrote to the project");
}

#[test]
fn an_extension_command_is_refused_on_a_read_only_project() {
    let project = Project::new(REQUIRES);
    let before = project.tree();
    let out = project.run(&["frobnicate", "now"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{said}");
    assert!(said.contains("memory-proposals"), "{said}");
    assert_eq!(project.tree(), before);
}

#[test]
fn a_schema_this_charter_does_not_understand_makes_every_command_read_only() {
    let project = Project::new("schema = 3\n");
    let before = project.tree();
    let out = project.run(&["workspace", "create", "demo"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{said}");
    assert!(said.contains("declares schema 3"), "{said}");
    assert_eq!(project.tree(), before);
}

#[test]
fn a_charter_toml_that_is_not_toml_makes_the_project_read_only() {
    let project = Project::new("[memory\n");
    let before = project.tree();
    let out = project.run(&["workspace", "create", "demo"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{said}");
    assert!(said.contains("read-only"), "{said}");
    assert_eq!(project.tree(), before);
}

#[test]
fn version_and_help_still_answer_on_a_read_only_project() {
    let project = Project::new(REQUIRES);
    for args in [vec!["--version"], vec!["--help"]] {
        let out = project.run(&args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn doctor_still_runs_on_a_read_only_project() {
    let project = Project::new(REQUIRES);
    let out = project.run(&["doctor"]);
    let table = String::from_utf8_lossy(&out.stdout);
    let refusal = String::from_utf8_lossy(&out.stderr);
    assert!(
        !refusal.contains("writes nothing to it"),
        "doctor is how the operator finds out what is wrong, so it is not refused: {refusal}"
    );
    assert!(
        table.contains("memory-proposals"),
        "doctor's schema row names the missing feature: {table}"
    );
}

#[test]
fn a_rewrite_keeps_a_key_charter_does_not_know() {
    let project = Project::new(
        "schema = 1\n\
         future-key = \"kept\"\n\
         \n\
         [persona]\n\
         default = \"steward\"\n\
         \n\
         [a-future-section]\n\
         setting = 7\n",
    );
    std::fs::create_dir_all(project.root.join("personas/scout")).unwrap();
    std::fs::write(
        project.root.join("personas/scout/persona.md"),
        "---\nname: scout\nrole: scouting\n---\n",
    )
    .unwrap();
    let out = project.run(&["persona", "default", "scout"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let toml = std::fs::read_to_string(project.root.join("charter.toml")).unwrap();
    assert!(toml.contains("default = \"scout\""), "{toml}");
    assert!(toml.contains("future-key = \"kept\""), "{toml}");
    assert!(toml.contains("[a-future-section]\nsetting = 7"), "{toml}");
}

#[test]
fn init_and_reinit_honour_a_charter_toml_that_links_inside_the_project() {
    let project = Project::new("");
    std::fs::remove_file(project.root.join("charter.toml")).unwrap();
    std::fs::create_dir_all(project.root.join("conf")).unwrap();
    std::fs::write(project.root.join("conf/charter.toml"), REQUIRES).unwrap();
    std::os::unix::fs::symlink("conf/charter.toml", project.root.join("charter.toml")).unwrap();
    let before = project.tree();
    for args in [
        vec!["reinit"],
        vec!["init", "--forge", "github", "--owner", "acme"],
    ] {
        let out = project.run(&args);
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?}: {said}");
        assert!(said.contains("memory-proposals"), "{args:?}: {said}");
        assert_eq!(project.tree(), before, "{args:?} wrote to the project");
    }
}

/// The manifest is the format gate (V5): a `charter.toml` that is a link out of the project is
/// not the project's, so neither command writes anything, here or through the link.
#[test]
fn init_and_reinit_write_nothing_when_charter_toml_links_out_of_the_project() {
    let project = Project::new("");
    std::fs::remove_file(project.root.join("charter.toml")).unwrap();
    let outside = project.fence.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("charter.toml"), REQUIRES).unwrap();
    std::os::unix::fs::symlink(
        outside.join("charter.toml"),
        project.root.join("charter.toml"),
    )
    .unwrap();
    let before = project.tree();
    for args in [
        vec!["reinit"],
        vec!["init", "--forge", "github", "--owner", "acme"],
    ] {
        let out = project.run(&args);
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?}: {said}");
        assert!(said.contains("outside this project"), "{args:?}: {said}");
        assert!(
            said.contains("Point it inside the project"),
            "{args:?}: {said}"
        );
        assert!(said.contains("Nothing was written"), "{args:?}: {said}");
        assert_eq!(project.tree(), before, "{args:?} wrote to the project");
        assert_eq!(
            std::fs::read_to_string(outside.join("charter.toml")).unwrap(),
            REQUIRES,
            "{args:?} wrote through the link"
        );
    }
}

#[test]
fn a_core_owned_alias_onto_an_extension_is_refused_on_a_read_only_project() {
    let project = Project::new(REQUIRES);
    let before = project.tree();
    let out = project.run(&["ws", "probe-echo", "hello"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{said}");
    assert!(said.contains("memory-proposals"), "{said}");
    assert_eq!(project.tree(), before);
}

#[test]
fn a_command_that_only_reads_still_runs_on_a_read_only_project_and_says_so() {
    let project = Project::new(REQUIRES);
    let before = project.tree();
    for args in [
        vec!["status"],
        vec!["recall", "anything"],
        vec!["workspace", "list"],
        vec!["statusline"],
    ] {
        let out = project.run(&args);
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{args:?} must still read: {said}");
        assert!(
            said.contains("read-only") && said.contains("memory-proposals"),
            "{args:?} says once that the project is read-only: {said}"
        );
        assert_eq!(said.matches("read-only").count(), 1, "{args:?}: {said}");
        assert_eq!(project.tree(), before, "{args:?} wrote to the project");
    }
}

/// One read command on [`Project::lived_in`]: it runs, says once that the project is read-only,
/// and leaves every file in the project exactly as it was (#833).
fn only_reads(args: &[&str]) {
    let project = Project::lived_in();
    let before = project.tree();
    let out = project.run(args);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{args:?} must still read: {said}");
    assert!(
        said.contains("read-only") && said.contains("memory-proposals"),
        "{args:?} says once that the project is read-only: {said}"
    );
    assert_eq!(said.matches("read-only").count(), 1, "{args:?}: {said}");
    let after = project.tree();
    let changed: Vec<&PathBuf> = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .collect();
    assert!(
        changed.is_empty(),
        "{args:?} wrote to the project: {changed:?}"
    );
}

#[test]
fn persona_list_only_reads_a_read_only_project() {
    only_reads(&["persona", "list"]);
}

#[test]
fn change_list_only_reads_a_read_only_project() {
    only_reads(&["change", "list", "-w", "alpha"]);
}

#[test]
fn session_list_only_reads_a_read_only_project() {
    only_reads(&["session", "list", "-w", "alpha"]);
}

#[test]
fn harness_list_only_reads_a_read_only_project() {
    only_reads(&["harness", "list"]);
}

#[test]
fn guard_list_only_reads_a_read_only_project() {
    only_reads(&["guard", "list"]);
}

#[test]
fn bare_guard_lists_and_only_reads_a_read_only_project() {
    only_reads(&["guard"]);
}

#[test]
fn workspace_recall_only_reads_a_read_only_project() {
    only_reads(&["workspace", "recall", "-w", "alpha"]);
}

#[test]
fn workspace_recall_with_a_query_only_reads_a_read_only_project() {
    only_reads(&["workspace", "recall", "-q", "widget", "-w", "alpha"]);
}

/// Already on FR-24's list (#827), and proven the same way here: it had no test of its own.
#[test]
fn workspace_current_only_reads_a_read_only_project() {
    only_reads(&["workspace", "current"]);
}
