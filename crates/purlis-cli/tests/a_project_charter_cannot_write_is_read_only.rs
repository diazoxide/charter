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
        self.run_with(args, &[])
    }

    /// [`Project::run`] with `stdin` on its standard input.
    fn run_feeding(&self, args: &[&str], stdin: &str) -> Output {
        use std::io::Write;
        let mut child = self
            .command(args, &[])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the binary runs");
        child
            .stdin
            .take()
            .expect("its stdin")
            .write_all(stdin.as_bytes())
            .expect("written");
        child.wait_with_output().expect("it finishes")
    }

    /// [`Project::run`] in an environment of its own: nothing the shell that started the
    /// suite has set (a chat's `CHARTER_WORKSPACE`, its session) reaches the binary, only
    /// `env`.
    fn run_with(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        self.command(args, env).output().expect("the binary runs")
    }

    fn command(&self, args: &[&str], env: &[(&str, &str)]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_purlis"));
        command
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .envs(env.iter().copied())
            .env("CHARTER_PLANE_FENCE", &self.fence)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("HOME", &self.home)
            .env("PATH", "/usr/bin:/bin")
            .env("NO_COLOR", "1");
        command
    }

    /// A project a charter has been used on, which then names a feature this charter lacks.
    ///
    /// It has what the read commands read, so a write one makes on the way is not hidden by
    /// an empty project's early return: a front door whose plain-file vault holds a secret, a
    /// workspace with a memory, a change record and a session record, a harness profile in
    /// `charter.local.toml`, and a git repository with all of it committed.
    fn lived_in() -> Self {
        let project = Project::new("");
        std::fs::remove_file(project.root.join("charter.toml")).unwrap();
        let ok = |args: &[&str], stdin: &str| {
            let out = project.run_feeding(args, stdin);
            assert!(
                out.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        ok(&["init", "--forge", "github", "--owner", "acme"], "");
        ok(
            &[
                "workspace",
                "create",
                "alpha",
                "--vision",
                "Ship the widget",
            ],
            "",
        );
        ok(
            &[
                "workspace",
                "remember",
                "The widget ships on Friday",
                "-w",
                "alpha",
            ],
            "",
        );
        ok(
            &[
                "change",
                "create",
                "widget-v2",
                "--why",
                "Ship the second widget",
                "-w",
                "alpha",
            ],
            "",
        );
        ok(
            &[
                "vault",
                "add",
                "stew",
                "--provider",
                "plain-file",
                "--persona",
                "steward",
                "--share",
            ],
            "",
        );
        ok(
            &["secret", "set", "stew", "db-password", "--stdin"],
            "hunter2",
        );
        let persona = project.root.join("personas/steward/persona.md");
        let text = std::fs::read_to_string(&persona).unwrap();
        assert!(text.contains("\nvault: none\n"), "{text}");
        std::fs::write(&persona, text.replace("\nvault: none\n", "\nvault: stew\n")).unwrap();
        std::fs::write(
            project.root.join("charter.local.toml"),
            "[harness.work]\nkind = \"claude\"\ncommand = [\"claude\"]\n",
        )
        .unwrap();
        let sessions = project.root.join("workspaces/alpha/sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(
            sessions.join("20260501-120000-the-widget.md"),
            "---\ntitle: The widget\ndate: 2026-05-01 12:00:00\nchat: 1\nchat-name: unknown\n\
             persona: none\nharness: claude\nprofile: unknown\nconversation: unknown\n\
             workspace: alpha\ncwd: unknown\n---\n\n# The widget\n\n## Goal\nShip it.\n",
        )
        .unwrap();
        // The manifest's first two lines become `REQUIRES`'s: `schema = 2` and the feature
        // this build lacks.
        let manifest = project.root.join("charter.toml");
        let written = std::fs::read_to_string(&manifest).unwrap();
        let head: String = REQUIRES
            .lines()
            .take(2)
            .map(|line| format!("{line}\n"))
            .collect();
        let required = written.replacen("schema = 1\n", &head, 1);
        assert_ne!(written, required, "init writes `schema = 1`: {written}");
        std::fs::write(&manifest, required).unwrap();
        for args in [
            &["init", "-q", "-b", "main", "."][..],
            &["add", "-A"],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@e.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                "the project as a teammate pushed it",
            ],
        ] {
            // No auto-maintenance: a detached `git maintenance` from this commit would take
            // `.git/objects/maintenance.lock` while a command under test runs, and the tree
            // check would blame the command for it.
            let done = Command::new("git")
                .args(["-c", "gc.auto=0", "-c", "maintenance.auto=false"])
                .args(args)
                .current_dir(&project.root)
                .env("HOME", &project.home)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .output()
                .expect("git runs");
            assert!(
                done.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&done.stderr)
            );
        }
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
    for args in [&["doctor", "--fix"][..], &["doctor", "--fix", "reinit"]] {
        let project = Project::new(REQUIRES);
        let before = project.tree();
        let out = project.run(args);
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?}: {said}");
        assert!(said.contains("memory-proposals"), "{args:?}: {said}");
        assert_eq!(project.tree(), before, "{args:?} wrote to the project");
    }
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

/// One read command on [`Project::lived_in`]: it runs, says once that the project is read-only,
/// shows `shows` (so it read what it was meant to), and leaves every file in the project exactly
/// as it was (#833).
fn only_reads(args: &[&str], shows: &str) {
    only_reads_with(args, &[], shows);
}

/// [`only_reads`], with `env` set for the command.
fn only_reads_with(args: &[&str], env: &[(&str, &str)], shows: &str) {
    let project = Project::lived_in();
    let before = project.tree();
    let out = project.run_with(args, env);
    let said = String::from_utf8_lossy(&out.stderr);
    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{args:?} must still read: {said}");
    assert!(
        said.contains("read-only") && said.contains("memory-proposals"),
        "{args:?} says once that the project is read-only: {said}"
    );
    assert_eq!(said.matches("read-only").count(), 1, "{args:?}: {said}");
    // Some listings are reports, and charter prints its reports on stderr.
    assert!(
        printed.contains(shows) || said.contains(shows),
        "{args:?} shows {shows:?}: {printed}{said}"
    );
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
fn status_only_reads_a_read_only_project() {
    only_reads_with(&["status"], &[("CHARTER_WORKSPACE", "alpha")], "alpha");
}

#[test]
fn recall_only_reads_a_read_only_project() {
    only_reads_with(
        &["recall", "widget"],
        &[("CHARTER_WORKSPACE", "alpha")],
        "Friday",
    );
}

#[test]
fn workspace_list_only_reads_a_read_only_project() {
    only_reads(&["workspace", "list"], "alpha");
}

#[test]
fn statusline_only_reads_a_read_only_project() {
    only_reads_with(&["statusline"], &[("CHARTER_WORKSPACE", "alpha")], "alpha");
}

#[test]
fn persona_list_only_reads_a_read_only_project() {
    only_reads(&["persona", "list"], "1 secret(s)");
}

#[test]
fn change_list_only_reads_a_read_only_project() {
    only_reads(&["change", "list", "-w", "alpha"], "widget-v2");
}

#[test]
fn session_list_only_reads_a_read_only_project() {
    only_reads(&["session", "list", "-w", "alpha"], "The widget");
}

#[test]
fn harness_list_only_reads_a_read_only_project() {
    only_reads(&["harness", "list"], "work");
}

#[test]
fn guard_list_only_reads_a_read_only_project() {
    only_reads(&["guard", "list"], "charter handoff");
}

#[test]
fn bare_guard_lists_and_only_reads_a_read_only_project() {
    only_reads(&["guard"], "charter handoff");
}

#[test]
fn workspace_recall_only_reads_a_read_only_project() {
    only_reads(&["workspace", "recall", "-w", "alpha"], "Friday");
}

#[test]
fn workspace_recall_with_a_query_only_reads_a_read_only_project() {
    only_reads(
        &["workspace", "recall", "-q", "widget", "-w", "alpha"],
        "Friday",
    );
}

/// Already on FR-24's list (#827): it had no test of its own.
#[test]
fn workspace_current_only_reads_a_read_only_project() {
    only_reads_with(
        &["workspace", "current"],
        &[("CHARTER_WORKSPACE", "alpha")],
        "alpha",
    );
}
