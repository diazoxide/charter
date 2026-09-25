//! A repo save against a real clone and a local bare remote. The pull request modes, which ask
//! a forge, are in `tests/a_repo_is_saved_by_its_mode.rs`, against a stand-in `gh`.
//!
//! The remote is reached the way `planegit`'s tests reach theirs: `origin` in the SSH form, and
//! an `insteadOf` in the clone's own config keyed on the HTTPS base charter builds, so the URL
//! charter pushes to is its own and git maps it onto the bare repository.

use std::path::{Path, PathBuf};

use super::*;

struct Fixture {
    _dir: tempfile::TempDir,
    plane: PathBuf,
    clone: PathBuf,
    bare: PathBuf,
}

fn run(dir: &Path, args: &[&str]) -> String {
    let done = crate::testgit::run(dir, args);
    assert!(done.ok(), "git {args:?} failed: {done:?}");
    done.out
}

impl Fixture {
    /// A plane saying `toml`, and a clone `alpha/widget` on `main` whose one commit is on its
    /// remote.
    fn new(toml: &str) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().canonicalize().unwrap();
        let plane = top.join("plane");
        std::fs::create_dir_all(&plane).unwrap();
        std::fs::write(plane.join("charter.toml"), toml).unwrap();
        let clone = plane.join("workspaces/alpha/widget");
        std::fs::create_dir_all(&clone).unwrap();
        run(&clone, &["init", "-q", "-b", "main", "."]);
        run(&clone, &["config", "user.name", "Fixture"]);
        run(&clone, &["config", "user.email", "fixture@example.invalid"]);
        std::fs::write(clone.join("README.md"), "one\n").unwrap();
        run(&clone, &["add", "-A"]);
        run(&clone, &["commit", "-q", "-m", "one"]);
        let bare = top.join("forge/acme/widget.git");
        std::fs::create_dir_all(&bare).unwrap();
        run(&bare, &["init", "-q", "--bare", "-b", "main", "."]);
        run(
            &clone,
            &["remote", "add", "origin", "git@github.com:acme/widget.git"],
        );
        let base = format!("file://{}/", bare.parent().unwrap().display());
        run(
            &clone,
            &[
                "config",
                &format!("url.{base}.insteadOf"),
                "https://github.com/acme/",
            ],
        );
        // By the bare repository's path, never by `origin`: the SSH form would leave this
        // machine.
        let at = bare.display().to_string();
        run(&clone, &["push", "-q", &at, "main"]);
        run(
            &clone,
            &["fetch", "-q", &at, "+refs/heads/*:refs/remotes/origin/*"],
        );
        Fixture {
            _dir: dir,
            plane,
            clone,
            bare,
        }
    }

    fn save_with(&self, trigger: Trigger, mid_turn: &[String]) -> (u8, String) {
        let mut said = String::new();
        let mut say = |line: Say| {
            said.push_str(&line.to_string());
            said.push('\n');
        };
        let code = save_as(
            &Request {
                plane: &self.plane,
                workspace: "alpha",
                name: "widget",
                clone: &self.clone,
                message: None,
                no_push: false,
                mid_turn: &|| mid_turn.to_vec(),
            },
            trigger,
            &mut say,
        );
        (code, said)
    }

    fn save(&self) -> (u8, String) {
        self.save_with(Trigger::Manual, &[])
    }

    fn standing(&self) -> Standing {
        standing(
            &self.plane,
            "alpha",
            &repos::Repo {
                name: "widget".into(),
                path: self.clone.clone(),
            },
        )
    }

    fn remote_has(&self, branch: &str) -> Option<String> {
        let found =
            crate::testgit::run(&self.bare, &["rev-parse", &format!("refs/heads/{branch}")]);
        found.ok().then(|| found.out.trim().to_string())
    }

    fn head(&self) -> String {
        run(&self.clone, &["rev-parse", "HEAD"]).trim().to_string()
    }

    fn journal(&self) -> Vec<serde_json::Value> {
        planegit::journal(&self.plane)
    }
}

#[test]
fn a_feature_branch_in_push_mode_is_committed_on_and_pushed_to_that_branch_only() {
    let f = Fixture::new("[repos.widget]\nmode = \"push\"\n");
    let main_before = f.remote_has("main");
    run(&f.clone, &["checkout", "-q", "-b", "feature/x"]);
    std::fs::write(f.clone.join("src.rs"), "fn main() {}\n").unwrap();

    let (code, said) = f.save();

    assert_eq!(code, 0, "{said}");
    assert_eq!(f.remote_has("feature/x"), Some(f.head()), "{said}");
    assert_eq!(f.remote_has("main"), main_before, "main moved: {said}");
    assert_eq!(
        run(&f.clone, &["log", "-1", "--format=%s"]).trim(),
        "charter save: 1 file (src.rs)"
    );
    let line = &f.journal()[0];
    assert_eq!(line["target"], "repo:alpha/widget");
    assert_eq!(line["mode"], "push");
    assert_eq!(line["outcome"], "saved");
    assert_eq!(line["branch"], "feature/x");
    assert_eq!(line["files"], 1);
    assert_eq!(f.standing().stage, Stage::Saved);
}

#[test]
fn a_repo_nobody_configured_is_left_exactly_as_it_is() {
    // The default is `off` (ADR 0051, amended 2026-09-25): no commit, no push, no PR, whoever
    // presses what.
    let f = Fixture::new("");
    std::fs::write(f.clone.join("a.md"), "a").unwrap();
    let before = f.head();
    let remote_before = f.remote_has("main");
    assert_eq!(f.standing().mode, Mode::Off);

    let (code, said) = f.save_with(Trigger::Manual, &[]);

    assert_eq!(code, 0, "{said}");
    assert_eq!(f.head(), before, "a commit was made: {said}");
    assert!(f.clone.join("a.md").is_file());
    assert_eq!(f.remote_has("main"), remote_before, "something was pushed");
}

#[test]
fn a_pr_mode_on_a_clone_with_no_known_default_branch_stays_committed() {
    // The fixture's origin is on github.com, a known forge, but nothing answers for it here
    // — so this stops at the question of where the PR goes, which the inventory answers.
    let f = Fixture::new("[repos.widget]\nmode = \"pr\"\n");
    std::fs::write(f.clone.join("a.md"), "a").unwrap();
    assert_eq!(f.standing().mode, Mode::Pr);

    let (code, said) = f.save_with(Trigger::Manual, &[]);

    // No inventory and no origin/HEAD: charter does not know the default branch, and says so
    // rather than pushing main.
    assert_eq!(code, 1, "{said}");
    assert!(
        said.contains("does not know widget's default branch"),
        "{said}"
    );
    assert_ne!(f.remote_has("main"), Some(f.head()), "main was pushed");
    assert_eq!(f.journal()[0]["outcome"], "blocked");
    assert_eq!(f.standing().stage, Stage::Blocked);
}

#[test]
fn commit_mode_commits_on_the_branch_it_is_on_and_pushes_nothing() {
    let f = Fixture::new("[repos.widget]\nmode = \"commit\"\n");
    let before = f.remote_has("main");
    std::fs::write(f.clone.join("a.md"), "a").unwrap();
    assert_eq!(f.standing().stage, Stage::Changed);

    let (code, said) = f.save();

    assert_eq!(code, 0, "{said}");
    assert!(
        said.contains("Not pushed: [repos.widget] mode is commit"),
        "{said}"
    );
    assert_eq!(f.remote_has("main"), before);
    assert_eq!(f.journal()[0]["outcome"], "committed");
    assert_eq!(
        f.standing().stage,
        Stage::Saved,
        "a commit is as far as it goes"
    );
}

#[test]
fn off_commits_nothing_and_says_so() {
    let f = Fixture::new("[repos.widget]\nmode = \"off\"\n");
    std::fs::write(f.clone.join("a.md"), "a").unwrap();
    let head = f.head();

    let (code, said) = f.save();

    assert_eq!(code, 0);
    assert!(said.contains("mode is off"), "{said}");
    assert_eq!(f.head(), head);
    assert_eq!(f.journal()[0]["outcome"], "skipped");
}

#[test]
fn a_save_held_back_by_a_working_session_names_it_and_touches_nothing() {
    let f = Fixture::new("[repos.widget]\nmode = \"push\"\n");
    std::fs::write(f.clone.join("a.md"), "a").unwrap();
    let head = f.head();
    let working = ["claude alpha.3".to_string(), "codex alpha.5".to_string()];

    let (code, said) = f.save_with(Trigger::Manual, &working);

    assert_eq!(code, 1);
    assert!(
        said.contains("claude alpha.3 and codex alpha.5 are mid-turn in alpha"),
        "{said}"
    );
    assert_eq!(f.head(), head, "it committed");
    assert!(f.journal().is_empty(), "nothing was attempted");
    assert_eq!(f.standing().stage, Stage::Changed);

    // Once the turn has ended, the same save goes through.
    let (code, said) = f.save_with(Trigger::Manual, &[]);
    assert_eq!(code, 0, "{said}");
    assert_eq!(f.remote_has("main"), Some(f.head()));
}

#[test]
fn an_auto_save_while_a_session_works_skips_the_cycle_without_a_word() {
    let f = Fixture::new("[repos.widget]\nmode = \"push\"\nautosave = true\n");
    std::fs::write(f.clone.join("a.md"), "a").unwrap();
    let head = f.head();

    let (code, said) = f.save_with(Trigger::Quiet, &["claude alpha.3".to_string()]);

    assert_eq!((code, said.as_str()), (0, ""));
    assert_eq!(f.head(), head);
    assert!(f.journal().is_empty());
}

#[test]
fn a_second_save_of_the_same_clone_is_refused_while_the_first_holds_it() {
    let f = Fixture::new("[repos.widget]\nmode = \"commit\"\n");
    std::fs::write(f.clone.join("a.md"), "a").unwrap();
    let running = Claim::of(&f.clone).expect("the first save");

    let (code, said) = f.save();

    assert_eq!(code, 1);
    assert!(
        said.contains("A save of alpha/widget is already running"),
        "{said}"
    );
    drop(running);
    assert_eq!(f.save().0, 0);
}

#[test]
fn a_push_the_remote_refuses_leaves_the_repo_blocked_and_rewrites_nothing() {
    let f = Fixture::new("[repos.widget]\nmode = \"push\"\n");
    // Somebody else's commit on the remote's main.
    let theirs = f.plane.parent().unwrap().join("theirs");
    run(
        f.plane.parent().unwrap(),
        &[
            "clone",
            "-q",
            &f.bare.display().to_string(),
            &theirs.display().to_string(),
        ],
    );
    run(&theirs, &["config", "user.name", "Other"]);
    run(&theirs, &["config", "user.email", "other@example.invalid"]);
    std::fs::write(theirs.join("theirs.md"), "t").unwrap();
    run(&theirs, &["add", "-A"]);
    run(&theirs, &["commit", "-q", "-m", "theirs"]);
    run(&theirs, &["push", "-q", "origin", "main"]);
    std::fs::write(f.clone.join("mine.md"), "m").unwrap();

    let (code, said) = f.save();

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("has commits alpha/widget does not"), "{said}");
    assert_eq!(
        run(&f.clone, &["log", "-1", "--format=%s"]).trim(),
        "charter save: 1 file (mine.md)",
        "the commit was rewritten"
    );
    let standing = f.standing();
    assert_eq!(standing.stage, Stage::Blocked);
    assert!(standing.blocked.unwrap().contains("Bring them in"));
}

#[test]
fn a_detached_head_is_not_saved_and_the_reason_is_its_branchlessness() {
    let f = Fixture::new("[repos.widget]\nmode = \"push\"\n");
    run(&f.clone, &["checkout", "-q", "--detach"]);
    std::fs::write(f.clone.join("a.md"), "a").unwrap();

    let (code, said) = f.save();

    assert_eq!(code, 1);
    assert!(said.contains("is not on a branch"), "{said}");
    assert_eq!(f.journal()[0]["outcome"], "blocked");
}

#[test]
fn a_branch_never_pushed_is_committed_not_saved() {
    let f = Fixture::new("[repos.widget]\nmode = \"push\"\n");
    assert_eq!(f.standing().stage, Stage::Saved);
    run(&f.clone, &["checkout", "-q", "-b", "fresh"]);

    let standing = f.standing();

    assert_eq!((standing.stage, standing.ahead), (Stage::Committed, None));
    assert!(standing.worth_saving());
}

#[test]
fn the_default_branch_comes_from_the_inventory_before_origin_head() {
    let f = Fixture::new("");
    assert_eq!(default_branch(&f.plane, "widget", &f.clone), None);
    run(
        &f.clone,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    assert_eq!(
        default_branch(&f.plane, "widget", &f.clone).as_deref(),
        Some("main")
    );
    std::fs::create_dir_all(f.plane.join("inventory")).unwrap();
    std::fs::write(
        f.plane.join("inventory/repos.json"),
        r#"{"repos": [{"name": "widget", "default_branch": "trunk"}]}"#,
    )
    .unwrap();
    assert_eq!(
        default_branch(&f.plane, "widget", &f.clone).as_deref(),
        Some("trunk")
    );
}

#[test]
fn a_generated_message_lists_the_files_and_counts_the_rest() {
    let files: Vec<String> = (1..=7).map(|n| format!("f{n}")).collect();
    assert_eq!(
        summary(&files),
        "charter save: 7 files (f1, f2, f3, f4, f5, +2 more)"
    );
}

#[test]
fn whom_a_save_waits_for_is_said_in_one_sentence() {
    assert_eq!(
        waiting_for("alpha", &["a".into(), "b".into(), "c".into()]),
        "Not saved: a, b and c are mid-turn in alpha. A repo is saved between turns — save \
         again when the turn ends."
    );
    assert!(waiting_for("alpha", &["a".into()]).starts_with("Not saved: a is mid-turn"));
}

#[test]
fn quitting_saves_a_repo_only_when_its_auto_save_is_on() {
    use crate::autosave::{AtQuit, repo_at_quit};
    let bound = std::time::Duration::from_secs(20);
    for toml in ["[repos.widget]\nmode = \"push\"\n", ""] {
        let f = Fixture::new(toml);
        std::fs::write(f.clone.join("a.md"), "a").unwrap();
        let head = f.head();
        let repo = repos::Repo {
            name: "widget".into(),
            path: f.clone.clone(),
        };
        assert_eq!(
            repo_at_quit(&f.plane, "alpha", &repo, &[], bound),
            AtQuit::Nothing,
            "{toml:?}"
        );
        assert_eq!(f.head(), head, "{toml:?}");
    }

    let f = Fixture::new("[repos.widget]\nmode = \"push\"\nautosave = true\n");
    std::fs::write(f.clone.join("a.md"), "a").unwrap();
    let repo = repos::Repo {
        name: "widget".into(),
        path: f.clone.clone(),
    };
    assert_eq!(
        repo_at_quit(&f.plane, "alpha", &repo, &[], bound),
        AtQuit::Pushed
    );
    assert_eq!(f.remote_has("main"), Some(f.head()));
    let triggers: Vec<_> = f.journal().iter().map(|l| l["trigger"].clone()).collect();
    assert!(triggers.iter().all(|t| t == "quit"), "{triggers:?}");
}

impl Fixture {
    /// A save whose `mid_turn` answers each of `answers` in turn, the last one again after.
    fn save_asked(&self, trigger: Trigger, answers: &[&[&str]]) -> (u8, String) {
        let asked = std::cell::Cell::new(0);
        let mid_turn = || {
            let n = asked.get();
            asked.set(n + 1);
            answers[n.min(answers.len() - 1)]
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>()
        };
        let mut said = String::new();
        let mut say = |line: Say| {
            said.push_str(&line.to_string());
            said.push('\n');
        };
        let code = save_as(
            &Request {
                plane: &self.plane,
                workspace: "alpha",
                name: "widget",
                clone: &self.clone,
                message: None,
                no_push: false,
                mid_turn: &mid_turn,
            },
            trigger,
            &mut say,
        );
        (code, said)
    }

    fn write(&self, path: &str, text: &str) {
        let at = self.clone.join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, text).unwrap();
    }
}

#[test]
fn a_turn_that_starts_while_a_save_waits_is_seen_again_right_before_anything_is_staged() {
    let f = Fixture::new("[repos.widget]\nmode = \"commit\"\n");
    f.write("half.md", "half-written");
    let head = f.head();

    // Nobody at the door; somebody by the time the save reaches `git add`.
    let (code, said) = f.save_asked(Trigger::Manual, &[&[], &["alpha.4"]]);

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("alpha.4 is mid-turn in alpha"), "{said}");
    assert_eq!(f.head(), head, "the half-written turn was committed");
    let staged = run(&f.clone, &["diff", "--cached", "--name-only"]);
    assert_eq!(staged.trim(), "", "it was staged");
    let line = &f.journal()[0];
    assert_eq!(line["outcome"], "skipped");
}

#[test]
fn quitting_while_a_turn_was_cut_off_leaves_the_repo_and_journals_why() {
    use crate::autosave::{AtQuit, repo_at_quit};
    let f = Fixture::new("[repos.widget]\nmode = \"push\"\nautosave = true\n");
    f.write("half.md", "half-written");
    let head = f.head();
    let repo = repos::Repo {
        name: "widget".into(),
        path: f.clone.clone(),
    };

    let got = repo_at_quit(
        &f.plane,
        "alpha",
        &repo,
        &["alpha.2".to_owned()],
        std::time::Duration::from_secs(20),
    );

    assert_eq!(got, AtQuit::Nothing);
    assert_eq!(f.head(), head, "a killed turn was committed");
    let line = &f.journal()[0];
    assert_eq!(
        (line["trigger"].as_str(), line["outcome"].as_str()),
        (Some("quit"), Some("skipped"))
    );
    assert!(
        line["detail"]
            .as_str()
            .unwrap()
            .contains("alpha.2 is mid-turn"),
        "{line}"
    );
}

#[test]
fn a_save_branch_carries_on_past_a_quit_or_a_failed_push_that_named_no_branch() {
    let f = Fixture::new("[repos.widget]\nmode = \"pr\"\n");
    let first = f.head();
    let branch = format!("charter/alpha/{}", &first[..7]);
    run(&f.clone, &["branch", &branch]);
    // The newest journal lines name no branch: a quit's commit, then a push that failed.
    for outcome in ["committed", "failed"] {
        planegit::journal_append(
            &f.plane,
            &serde_json::json!({"target": "repo:alpha/widget", "outcome": outcome, "branch": null}),
        );
    }
    f.write("more.md", "more");
    run(&f.clone, &["add", "-A"]);
    run(&f.clone, &["commit", "-q", "-m", "more"]);
    let head = f.head();

    let got = save_branch(
        &Request {
            plane: &f.plane,
            workspace: "alpha",
            name: "widget",
            clone: &f.clone,
            message: None,
            no_push: false,
            mid_turn: &nobody_working,
        },
        "main",
        &head,
    );

    assert_eq!(
        got.as_deref(),
        Ok(branch.as_str()),
        "a second PR would be opened"
    );
    assert_eq!(
        run(&f.clone, &["rev-parse", &branch]).trim(),
        head,
        "not moved forward"
    );
}

#[test]
fn a_secret_shaped_file_is_refused_by_name_and_its_value_never_said() {
    for (path, text, kind) in [
        (
            ".env",
            "DB_PASSWORD=hunter2hunter2\n",
            "an environment file",
        ),
        ("config/.env.production", "X=1\n", "an environment file"),
        ("keys/id_ed25519", "k\n", "an SSH private key"),
        ("certs/server.pem", "c\n", "a key or certificate store"),
        ("credentials.json", "{}\n", "a credentials file"),
        (
            ".npmrc",
            "//registry.npmjs.org/:_authToken=npm_abcdefghijklmnopqrstuvwx\n",
            "a package registry token",
        ),
        (
            "src/deploy.sh",
            "TOKEN=ghp_0123456789abcdefABCDEF0123\n",
            "a token by its forge's prefix",
        ),
        (
            "notes/key.txt",
            "-----BEGIN RSA PRIVATE KEY-----\nMIIE\n",
            "private key (PEM)",
        ),
    ] {
        let f = Fixture::new("[repos.widget]\nmode = \"commit\"\n");
        f.write(path, text);
        let head = f.head();

        let (code, said) = f.save();

        assert_eq!(code, 1, "{path}: {said}");
        assert!(said.contains(&format!("{path} ({kind})")), "{path}: {said}");
        for secret in ["hunter2", "npm_abcdef", "ghp_0123", "MIIE"] {
            assert!(!said.contains(secret), "{path} said its value: {said}");
        }
        assert_eq!(f.head(), head, "{path} was committed");
        assert_eq!(f.journal()[0]["outcome"], "blocked", "{path}");
    }
}

#[test]
fn ordinary_code_and_the_files_that_only_look_like_secrets_are_saved() {
    let f = Fixture::new("[repos.widget]\nmode = \"commit\"\n");
    f.write(
        "src/login.rs",
        "struct Login { password: String, token: String }\n",
    );
    f.write(".env.example", "DB_PASSWORD=\n");
    f.write("keys/id_ed25519.pub", "ssh-ed25519 AAAA\n");
    f.write(".npmrc", "registry=https://registry.npmjs.org/\n");

    let (code, said) = f.save();

    assert_eq!(code, 0, "{said}");
    assert_eq!(f.journal()[0]["outcome"], "committed");
}

#[test]
fn taking_a_secret_file_out_of_the_tree_is_not_refused() {
    let f = Fixture::new("[repos.widget]\nmode = \"commit\"\n");
    f.write(".env", "X=1\n");
    run(&f.clone, &["add", "-A"]);
    run(&f.clone, &["commit", "-q", "-m", "oops"]);
    std::fs::remove_file(f.clone.join(".env")).unwrap();

    let (code, said) = f.save();

    assert_eq!(code, 0, "{said}");
}

#[test]
fn a_protected_feature_branch_in_pr_mode_names_the_fix_that_applies() {
    let f = Fixture::new("[repos.widget]\nmode = \"pr\"\n");
    std::fs::create_dir_all(f.plane.join("inventory")).unwrap();
    std::fs::write(
        f.plane.join("inventory/repos.json"),
        r#"{"repos": [{"name": "widget", "default_branch": "main"}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(f.bare.join("hooks")).unwrap();
    stand_in::program(
        &f.bare,
        "hooks/pre-receive",
        "#!/bin/sh\necho 'GH006: Protected branch update failed for refs/heads/release.' >&2\nexit 1\n",
    );
    run(&f.clone, &["checkout", "-q", "-b", "release"]);
    f.write("a.md", "a");

    let (code, said) = f.save();

    assert_eq!(code, 1, "{said}");
    assert!(
        said.contains("release is protected and takes no direct push"),
        "{said}"
    );
    assert!(
        said.contains("Set [repos.widget] branch = \"release\""),
        "{said}"
    );
    assert!(!said.contains("mode = \"pr\""), "{said}");
}
