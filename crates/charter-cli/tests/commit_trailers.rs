//! An agent's own commit carries its provenance trailers (GL-8, V67): a real project, a
//! workspace repo on a change's branch, git armed the way the app arms a chat, and the
//! `charter` this build made running `commit-msg`.
//!
//! The acceptance line: trailers on agent commits, and none on a commit the operator makes by
//! hand — neither in their own terminal nor in a program that inherited the chat's environment
//! but runs outside its harness (V82, #1018).
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Output;

use charter_core::githooks::GitHooks;

const CHARTER: &str = env!("CARGO_BIN_EXE_charter");
const CHAT: &str = "01J9ZQ3W5Y7X8V6T4R2P0N1M3K";

/// A project whose chat 3 runs claude as the steward, and its workspace `ws`'s clone `api`.
struct Project {
    env: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    repo: PathBuf,
    plane: PathBuf,
    dir: tempfile::TempDir,
}

impl Project {
    fn new(branch: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let plane = root.join("project");
        let repo = plane.join("workspaces").join("ws").join("api");
        std::fs::create_dir_all(&repo).unwrap();
        let hooks = GitHooks::at(root.join("app").join("git-hooks"));
        hooks.write(Path::new(CHARTER)).unwrap();
        charter_core::reopen::write(
            &plane,
            &charter_core::reopen::Record {
                chats: vec![charter_core::reopen::Chat {
                    program: "claude".into(),
                    profile: Some("claude".into()),
                    name: "steward 3".into(),
                    number: Some(3),
                    persona: Some("steward".into()),
                    identity: charter_core::reopen::Identity {
                        id: Some(CHAT.into()),
                        ..Default::default()
                    },
                    // This test process stands in for the chat's harness, and the git it
                    // starts for the agent's own. [`Project::harness_is`] names another.
                    pid: Some(std::process::id()),
                    ..Default::default()
                }],
                dealt: 3,
                ..Default::default()
            },
        )
        .unwrap();
        let mut record = charter_core::change::Record::new(
            "billing-v2",
            "one bill",
            "steward",
            "2026-10-02T00:00:00Z",
        );
        record.members.push(charter_core::change::Member {
            repo: "api".into(),
            branch: "change/billing-v2".into(),
            needs: Vec::new(),
        });
        charter_core::change::store::write(&plane, "ws", &record).unwrap();
        let env = hooks.arm(vec![
            ("CHARTER_ROOT".into(), plane.clone().into_os_string()),
            ("CHARTER_SESSION_ID".into(), "3".into()),
        ]);
        let project = Self {
            env,
            repo,
            plane,
            dir,
        };
        project.operator_git(&["init", "-q", "-b", branch, "."]);
        project
    }

    /// git as the chat runs it.
    fn git(&self, args: &[&str]) -> Output {
        self.git_as(args, true)
    }

    /// git as the operator runs it in their own terminal, which charter does not arm.
    fn operator_git(&self, args: &[&str]) -> Output {
        self.git_as(args, false)
    }

    fn git_as(&self, args: &[&str], chat: bool) -> Output {
        let mut cmd = std::process::Command::new("git");
        cmd.arg("-C").arg(&self.repo).args(args);
        self.environ(&mut cmd, chat);
        charter_core::forklock::output(&mut cmd).unwrap()
    }

    /// Records `pid` as the process the app started chat 3's harness as.
    fn harness_is(&self, pid: u32) {
        let mut record = charter_core::reopen::read_or_refusal(&self.plane).unwrap();
        record.chats[0].pid = Some(pid);
        charter_core::reopen::write(&self.plane, &record).unwrap();
    }

    /// A harness of its own, started with the chat's environment, that waits for a line on its
    /// standard input and then runs `script` in the repo — an agent's turn.
    fn harness(&self, script: &str) -> std::process::Child {
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.args(["-c", &format!("read go && {script}")])
            .current_dir(&self.repo)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        self.environ(&mut cmd, true);
        charter_core::forklock::spawn(&mut cmd).unwrap()
    }

    fn environ(&self, cmd: &mut std::process::Command, chat: bool) {
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "operator")
            .env("GIT_AUTHOR_EMAIL", "operator@example.invalid")
            .env("GIT_COMMITTER_NAME", "operator")
            .env("GIT_COMMITTER_EMAIL", "operator@example.invalid")
            .env("HOME", self.dir.path());
        // Under either name (V93k): a suite run in a chat inherits both.
        for name in ["PURLIS_ROOT", "PURLIS_SESSION_ID"] {
            for spelling in charter_core::envvar::spellings(name) {
                cmd.env_remove(spelling);
            }
        }
        if chat {
            for (k, v) in &self.env {
                cmd.env(k, v);
            }
        }
    }

    fn commit(&self, chat: bool, message: &str) {
        std::fs::write(self.repo.join("bill.rs"), message).unwrap();
        self.git_as(&["add", "bill.rs"], chat);
        let ran = self.git_as(&["commit", "-q", "-m", message], chat);
        assert!(ran.status.success(), "{ran:?}");
    }

    /// HEAD's trailers, as git itself parses them.
    fn trailers(&self) -> String {
        let out = self.operator_git(&["log", "-1", "--format=%(trailers:only,unfold)"]);
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }
}

#[test]
fn an_agents_own_commit_on_a_change_branch_carries_all_four_trailers() {
    let project = Project::new("change/billing-v2");

    project.commit(true, "fix: one bill");

    assert_eq!(
        project.trailers(),
        format!(
            "Assisted-by: claude-code\nPurlis-Chat: {CHAT}\nPurlis-Persona: steward\n\
             Purlis-Change: billing-v2"
        )
    );
}

#[test]
fn the_operators_own_commit_by_hand_carries_none() {
    let project = Project::new("change/billing-v2");

    project.commit(false, "fix: by hand");

    assert_eq!(project.trailers(), "");
}

#[test]
fn an_amend_does_not_add_the_trailers_twice() {
    let project = Project::new("main");
    project.commit(true, "fix: one bill");

    let ran = project.git(&["commit", "-q", "--amend", "--no-edit"]);

    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(
        project.trailers(),
        format!("Assisted-by: claude-code\nPurlis-Chat: {CHAT}\nPurlis-Persona: steward")
    );
}

#[test]
fn a_repos_trailer_config_runs_nothing_and_the_agents_own_lines_stay_byte_for_byte() {
    let project = Project::new("main");
    let ran = project.dir.path().join("trailer command ran");
    project.operator_git(&[
        "config",
        "trailer.foo.command",
        &format!("touch '{}'; echo v", ran.display()),
    ]);
    project.operator_git(&["config", "trailer.Assisted-by.ifexists", "replace"]);

    project.commit(
        true,
        "fix: one bill\n\n---\nnot a patch\n\nCo-authored-by:x\nRefs:\nhttps://example.com/a",
    );

    assert!(!ran.exists(), "a trailer command ran");
    let body = project.operator_git(&["log", "-1", "--format=%B"]);
    assert_eq!(
        String::from_utf8_lossy(&body.stdout),
        format!(
            "fix: one bill\n\n---\nnot a patch\n\nCo-authored-by:x\nRefs:\nhttps://example.com/a\n\
             Assisted-by: claude-code\nPurlis-Chat: {CHAT}\nPurlis-Persona: steward\n\n"
        )
    );
}

#[test]
fn a_commit_an_agent_makes_below_its_chats_harness_is_stamped() {
    let project = Project::new("main");
    std::fs::write(project.repo.join("bill.rs"), "fix").unwrap();
    let mut harness = project.harness("git add bill.rs && git commit -q -m 'fix: one bill'");
    project.harness_is(harness.id());

    use std::io::Write as _;
    harness.stdin.take().unwrap().write_all(b"go\n").unwrap();
    let ran = harness.wait_with_output().unwrap();

    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(
        project.trailers(),
        format!("Assisted-by: claude-code\nPurlis-Chat: {CHAT}\nPurlis-Persona: steward")
    );
}

#[test]
fn a_commit_with_the_chats_environment_from_outside_its_harness_carries_none() {
    // An editor the chat opened keeps the chat's environment and runs on its own; the
    // operator's commit there is theirs (V82).
    let project = Project::new("change/billing-v2");
    let mut harness = project.harness("true");
    project.harness_is(harness.id());

    project.commit(true, "fix: in the editor");

    let _ = harness.kill();
    let _ = harness.wait();
    assert_eq!(project.trailers(), "");
}
