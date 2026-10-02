//! The first-task script (FR-28, #621), run on a fresh repo the way CI runs it.
//!
//! FR-1 measures one guided task: the same task given to two chats, each on a branch of its
//! own, with both diffs openable, and one "month-two in minute five" moment. The moment this
//! script shows is **a lesson carried across harnesses**: what the first run records is in the
//! briefing the second run starts with, whichever harness that one is on, because charter
//! keeps it in the project and briefs every harness from the same place (W10).
//!
//! What a harness does in a run is the model's, and no CI machine has a harness signed in, so
//! the two edits are written here, standing in for the agents. **Everything charter does is
//! charter's own path:** the first run's local project and workspace (`firstrun`), the
//! branches the app cuts for each run's chat (`chatpiece`, named by `firsttask::label`), the
//! task's text (`firsttask::prompt`), the `charter` binary a run's agent records its lesson
//! with, the `charter hook sessionstart` every harness's chat is briefed through, and the diff
//! command the guide opens (`firsttask::diff_command`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use charter_core::chatpiece::{self, Naming};
use charter_core::firstrun::{self, ForgeFrom};
use charter_core::firsttask;

/// Every fixture repository is made from charter-core's template, so none asks a signer.
const TEMPLATE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../charter-core/tests/support/git-template"
);

const WHO: [(&str, &str); 5] = [
    ("GIT_AUTHOR_NAME", "charter tests"),
    ("GIT_AUTHOR_EMAIL", "tests@example.invalid"),
    ("GIT_COMMITTER_NAME", "charter tests"),
    ("GIT_COMMITTER_EMAIL", "tests@example.invalid"),
    ("GIT_TERMINAL_PROMPT", "0"),
];

/// git for the fixture's own setup and for reading what a run left. Not the code under test.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-c")
        .arg(format!("init.templateDir={TEMPLATE}"))
        .arg("-C")
        .arg(dir)
        .args(args)
        .envs(WHO)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A repo nobody has opened in charter: one commit holding `files`, no remote.
fn fresh_repo(at: &Path, files: &[(&str, &str)]) -> PathBuf {
    let repo = at.join("shop");
    std::fs::create_dir_all(&repo).expect("a directory");
    git(&repo, &["init", "-q", "-b", "main", "."]);
    for (name, text) in files {
        std::fs::write(repo.join(name), text).expect("a file");
    }
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "start"]);
    repo
}

/// What a chat's diff command prints, run by a shell in the chat's folder as the guide's shell
/// tab runs it.
fn diff_of(tree: &Path, commit: &str) -> String {
    let command = firsttask::diff_command(commit);
    let out = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(tree)
        .envs(WHO)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("sh runs");
    assert!(
        out.status.success(),
        "{command}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A local project made by the first run, with `repo` taken into it, and a branch cut for each
/// of the task's chats: `(root, clone, [(cut, the commit it was cut from)])`.
fn first_run_with_two_chats(
    base: &Path,
    repo: &Path,
) -> (PathBuf, PathBuf, Vec<(chatpiece::Cut, String)>) {
    let root = firstrun::ensure_local_plane(
        &base.join("config"),
        ForgeFrom::Named(charter_core::forge::Kind::GitHub),
    )
    .expect("a local project");
    let taken = firstrun::take_in(&root, repo).expect("the repo is taken in");
    let (workspace, clone) = chatpiece::clone_at(&root, &taken.clone).expect("a clone");
    let chats = [1, 2]
        .into_iter()
        .map(|run| {
            let cut = chatpiece::cut(
                &root,
                &workspace,
                &clone,
                &Naming::After(Some(firsttask::label(run))),
            )
            .expect("a branch for the chat");
            let from = firsttask::cut_from(&cut.path).expect("the commit it was cut from");
            (cut, from)
        })
        .collect();
    (root, taken.clone, chats)
}

/// The `charter` binary as a chat runs it: in the chat's directory, with nothing of this test's
/// own environment, so its persona is the project's default.
fn charter_in_a_run(cwd: &Path, home: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_charter"))
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", home)
        .envs(WHO)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("charter runs");
    stand_in::feed(&mut child, stdin.as_bytes());
    child.wait_with_output().expect("charter finishes")
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn the_first_task_runs_twice_on_a_fresh_repo_and_the_second_run_starts_knowing_what_the_first_learned()
 {
    let tmp = tempfile::tempdir().expect("a directory");
    let base = std::fs::canonicalize(tmp.path()).expect("the directory resolves");
    let home = base.join("home");
    std::fs::create_dir_all(&home).expect("a home");
    let repo = fresh_repo(&base, &[("README.md", "# shop\n")]);

    // The first run (FR-4) makes a local project nobody was asked about and takes the repo
    // into a workspace of its own; each chat is on a branch of its own, cut as the app cuts
    // one for a labelled chat.
    let (_root, clone, chats) = first_run_with_two_chats(&base, &repo);
    let runs: Vec<&chatpiece::Cut> = chats.iter().map(|(cut, _)| cut).collect();
    assert_eq!(
        runs.iter()
            .map(|cut| cut.branch.as_str())
            .collect::<Vec<_>>(),
        ["first-task-1", "first-task-2"]
    );

    // The task asks its agent to record what it learned, by the command a chat has.
    let task = firsttask::prompt();
    assert!(task.contains("charter persona remember"), "{task}");

    // Run 1's agent makes its change and records its lesson.
    std::fs::write(
        runs[0].path.join("README.md"),
        "# shop\n\n## How to check a change\n\nRun `make test`.\n",
    )
    .expect("run 1's change");
    let lesson = "shop's checks run with make test";
    let recorded = charter_in_a_run(&runs[0].path, &home, &["persona", "remember", lesson], "");
    assert!(recorded.status.success(), "{}", said(&recorded));

    // Run 2 starts on another harness. Its briefing is the one every harness's chat gets
    // from `charter hook sessionstart`, and it carries run 1's lesson.
    let briefed = charter_in_a_run(
        &runs[1].path,
        &home,
        &["hook", "sessionstart"],
        r#"{"session_id": "run-2", "hook_event_name": "SessionStart", "source": "startup"}"#,
    );
    assert!(briefed.status.success(), "{}", said(&briefed));
    let doc: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&briefed.stdout).trim())
            .unwrap_or_else(|why| panic!("{why}: {}", said(&briefed)));
    let context = doc["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("a briefing");
    assert!(context.contains(lesson), "{context}");

    // Run 2's agent makes its own change.
    std::fs::write(
        runs[1].path.join("README.md"),
        "# shop\n\n## Checking a change\n\n`make test` runs every check.\n",
    )
    .expect("run 2's change");

    // The branch the chats were cut from moves on. Neither chat's diff shows it.
    std::fs::write(clone.join("UPSTREAM.md"), "moved on\n").expect("an upstream change");
    git(&clone, &["add", "-A"]);
    git(&clone, &["commit", "-q", "-m", "upstream"]);

    // Both diffs open, each showing its own chat's change and not the other's.
    for ((cut, from), own, other) in [
        (&chats[0], "Run `make test`.", "runs every check"),
        (&chats[1], "runs every check", "Run `make test`."),
    ] {
        let diff = diff_of(&cut.path, from);
        assert!(diff.contains(own), "{}: {diff}", cut.branch);
        assert!(!diff.contains(other), "{}: {diff}", cut.branch);
        assert!(!diff.contains("moved on"), "{}: {diff}", cut.branch);
    }

    // And the repo the operator opened was never written to.
    assert_eq!(
        std::fs::read_to_string(repo.join("README.md")).expect("the README"),
        "# shop\n"
    );
}

#[test]
fn a_chat_that_makes_the_readme_a_repo_had_none_of_shows_it_in_its_diff() {
    let tmp = tempfile::tempdir().expect("a directory");
    let base = std::fs::canonicalize(tmp.path()).expect("the directory resolves");
    let repo = fresh_repo(&base, &[("main.go", "package main\n")]);
    let (_root, _clone, chats) = first_run_with_two_chats(&base, &repo);
    let (cut, from) = &chats[0];

    // The task says to make a README when there is none: a file git does not track yet.
    std::fs::write(
        cut.path.join("README.md"),
        "# shop\n\n## How to check a change\n\nRun `go test ./...`.\n",
    )
    .expect("the chat's new README");

    let diff = diff_of(&cut.path, from);
    assert!(diff.contains("+++ b/README.md"), "{diff}");
    assert!(diff.contains("go test ./..."), "{diff}");
}
