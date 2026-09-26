//! Auto-save's quiet period, driven by a made-up clock (charter-app#296).

use super::*;
use crate::planesave::Settings;

fn plane(toml: &str) -> Plane {
    Settings::from_text(Some(toml), None).plane
}

fn standing(stage: Stage, changed: &[&str], pushes: bool) -> Standing {
    Standing {
        stage,
        changed: changed.iter().map(|s| (*s).to_owned()).collect(),
        ahead: Some(0),
        pr: None,
        blocked: None,
        branch: "main".into(),
        pushes,
        behind: Some(0),
        push_failed: None,
        conflicts: Vec::new(),
        notice: None,
    }
}

const PUSH: &str = "[plane]\nmode = \"push\"\n";

#[test]
fn a_change_left_alone_for_the_quiet_period_is_saved_once() {
    let at = Instant::now();
    let plane = plane(PUSH);
    let changed = standing(Stage::Changed, &["a.md"], true);
    let mut quiet = Quiet::default();

    assert_eq!(quiet.tick(at, &plane, &changed, "a1"), Decision::Wait);
    assert_eq!(
        quiet.tick(at + Duration::from_secs(29), &plane, &changed, "a1"),
        Decision::Wait
    );
    assert_eq!(
        quiet.tick(at + Duration::from_secs(30), &plane, &changed, "a1"),
        Decision::Save
    );
    assert_eq!(
        quiet.tick(at + Duration::from_secs(31), &plane, &changed, "a1"),
        Decision::Wait,
        "saved once, not every tick"
    );
}

#[test]
fn another_change_starts_the_quiet_period_again() {
    let at = Instant::now();
    let plane = plane(PUSH);
    let changed = standing(Stage::Changed, &["a.md"], true);
    let mut quiet = Quiet::default();

    quiet.tick(at, &plane, &changed, "a1");
    assert_eq!(
        quiet.tick(at + Duration::from_secs(25), &plane, &changed, "a2"),
        Decision::Wait
    );
    assert_eq!(
        quiet.tick(at + Duration::from_secs(50), &plane, &changed, "a2"),
        Decision::Wait
    );
    assert_eq!(
        quiet.tick(at + Duration::from_secs(55), &plane, &changed, "a2"),
        Decision::Save
    );
}

#[test]
fn the_quiet_period_is_the_one_the_settings_name() {
    let at = Instant::now();
    let plane = plane("[plane]\nmode = \"push\"\nautosave_after = \"2m\"\n");
    let changed = standing(Stage::Changed, &["a.md"], true);
    let mut quiet = Quiet::default();

    quiet.tick(at, &plane, &changed, "a1");
    assert_eq!(
        quiet.tick(at + Duration::from_secs(119), &plane, &changed, "a1"),
        Decision::Wait
    );
    assert_eq!(
        quiet.tick(at + Duration::from_secs(120), &plane, &changed, "a1"),
        Decision::Save
    );
}

#[test]
fn a_save_that_did_not_settle_the_plane_is_tried_again_after_five_minutes_not_thirty_seconds() {
    let at = Instant::now();
    let plane = plane(PUSH);
    // Committed and not pushed — the remote is down — and still the same after the save.
    let unpushed = standing(Stage::Committed, &[], true);
    let mut quiet = Quiet::default();

    quiet.tick(at, &plane, &unpushed, "c1");
    assert_eq!(
        quiet.tick(at + Duration::from_secs(30), &plane, &unpushed, "c1"),
        Decision::Save
    );
    assert_eq!(
        quiet.tick(at + Duration::from_secs(90), &plane, &unpushed, "c1"),
        Decision::Wait
    );
    assert_eq!(
        quiet.tick(at + Duration::from_secs(329), &plane, &unpushed, "c1"),
        Decision::Wait
    );
    assert_eq!(
        quiet.tick(at + Duration::from_secs(330), &plane, &unpushed, "c1"),
        Decision::Save
    );
}

#[test]
fn auto_save_pauses_while_the_plane_is_blocked() {
    let at = Instant::now();
    let plane = plane(PUSH);
    let blocked = standing(Stage::Blocked, &["a.md"], true);
    let mut quiet = Quiet::default();
    for s in [0, 30, 600] {
        assert_eq!(
            quiet.tick(at + Duration::from_secs(s), &plane, &blocked, "b"),
            Decision::Wait
        );
    }
}

#[test]
fn a_plane_that_names_no_mode_or_turned_auto_save_off_is_never_saved_by_itself() {
    let at = Instant::now();
    let changed = standing(Stage::Changed, &["a.md"], true);
    for toml in [
        "",
        "[memory]\nshare = \"local\"\n",
        "[plane]\nmode = \"off\"\n",
        "[plane]\nmode = \"push\"\nautosave = false\n",
    ] {
        let plane = plane(toml);
        let mut quiet = Quiet::default();
        for s in [0, 30, 600] {
            assert_eq!(
                quiet.tick(at + Duration::from_secs(s), &plane, &changed, "a1"),
                Decision::Wait,
                "{toml:?}"
            );
        }
    }
}

#[test]
fn commits_a_save_would_not_push_are_not_worth_a_save() {
    let at = Instant::now();
    let plane = plane("[plane]\nmode = \"commit\"\n");
    let committed = standing(Stage::Committed, &[], false);
    let mut quiet = Quiet::default();
    for s in [0, 30, 600] {
        assert_eq!(
            quiet.tick(at + Duration::from_secs(s), &plane, &committed, "c"),
            Decision::Wait
        );
    }
}

// -- a workspace repo (charter-app#299) ------------------------------------------------- //

fn repo(toml: &str) -> crate::planesave::Repo {
    Settings::from_text(Some(toml), None).repo("widget")
}

fn repo_standing(stage: Stage, changed: u32) -> crate::reposave::Standing {
    crate::reposave::Standing {
        name: "widget".into(),
        mode: crate::planesave::Mode::Pr,
        mode_from: "charter.toml",
        autosave: true,
        stage,
        branch: Some("feature/x".into()),
        changed,
        ahead: None,
        pr: None,
        blocked: None,
        head: Some("0123456".into()),
        pushes: true,
    }
}

#[test]
fn a_repo_is_never_saved_by_itself_unless_its_table_turns_auto_save_on() {
    let at = Instant::now();
    let changed = repo_standing(Stage::Changed, 1);
    for toml in [
        "",
        "[repos.widget]\nmode = \"push\"\n",
        "[repos.widget]\nautosave = true\nmode = \"off\"\n",
    ] {
        let mut quiet = Quiet::default();
        for s in [0, 31, 400, 4000] {
            assert_eq!(
                quiet.tick_repo(at + Duration::from_secs(s), &repo(toml), &changed),
                Decision::Wait,
                "{toml:?}"
            );
        }
    }
}

#[test]
fn a_repo_with_auto_save_on_is_saved_after_its_own_quiet_period() {
    let at = Instant::now();
    let on = repo("[repos.widget]\nmode = \"pr\"\nautosave = true\nautosave_after = \"2m\"\n");
    let changed = repo_standing(Stage::Changed, 1);
    let mut quiet = Quiet::default();

    assert_eq!(quiet.tick_repo(at, &on, &changed), Decision::Wait);
    assert_eq!(
        quiet.tick_repo(at + Duration::from_secs(119), &on, &changed),
        Decision::Wait
    );
    assert_eq!(
        quiet.tick_repo(at + Duration::from_secs(120), &on, &changed),
        Decision::Save
    );
    // Blocked pauses it.
    let mut quiet = Quiet::default();
    let blocked = repo_standing(Stage::Blocked, 1);
    quiet.tick_repo(at, &on, &blocked);
    assert_eq!(
        quiet.tick_repo(at + Duration::from_secs(600), &on, &blocked),
        Decision::Wait
    );
}

// -- a workspace repo at quit (charter-app#299) ------------------------------------------ //

/// A plane saying `toml`, and a clone `alpha/widget` on `main` with one commit and no remote.
fn quitting_clone(toml: &str) -> (tempfile::TempDir, std::path::PathBuf, crate::repos::Repo) {
    let dir = tempfile::tempdir().unwrap();
    let plane = dir.path().canonicalize().unwrap().join("plane");
    std::fs::create_dir_all(&plane).unwrap();
    std::fs::write(plane.join("charter.toml"), toml).unwrap();
    let clone = plane.join("workspaces/alpha/widget");
    std::fs::create_dir_all(&clone).unwrap();
    for args in [
        &["init", "-q", "-b", "main", "."][..],
        &["config", "user.name", "Fixture"],
        &["config", "user.email", "fixture@example.invalid"],
    ] {
        assert!(crate::testgit::run(&clone, args).ok(), "git {args:?}");
    }
    std::fs::write(clone.join("README.md"), "one\n").unwrap();
    for args in [&["add", "-A"][..], &["commit", "-q", "-m", "one"]] {
        assert!(crate::testgit::run(&clone, args).ok(), "git {args:?}");
    }
    let repo = crate::repos::Repo {
        name: "widget".into(),
        path: clone,
    };
    (dir, plane, repo)
}

fn head_of(clone: &std::path::Path) -> String {
    crate::testgit::run(clone, &["rev-parse", "HEAD"]).out
}

const COMMIT_ON: &str = "[repos.widget]\nmode = \"commit\"\nautosave = true\n";

#[test]
fn quitting_commits_what_a_repo_with_auto_save_on_had_changed() {
    let (_dir, plane, repo) = quitting_clone(COMMIT_ON);
    std::fs::write(repo.path.join("a.md"), "a").unwrap();
    let before = head_of(&repo.path);

    let got = repo_at_quit(&plane, "alpha", &repo, &[], Duration::from_secs(20));

    assert_eq!(got, AtQuit::Committed);
    assert_ne!(head_of(&repo.path), before, "the change was not committed");
    let tracked = crate::testgit::run(&repo.path, &["ls-files", "a.md"]).out;
    assert_eq!(tracked.trim(), "a.md");
}

#[test]
fn quitting_does_nothing_to_a_repo_with_auto_save_on_and_nothing_to_save() {
    let (_dir, plane, repo) = quitting_clone(COMMIT_ON);
    let before = head_of(&repo.path);

    let got = repo_at_quit(&plane, "alpha", &repo, &[], Duration::from_secs(20));

    assert_eq!(got, AtQuit::Nothing);
    assert_eq!(head_of(&repo.path), before);
}

#[test]
fn quitting_with_only_commits_to_push_pushes_them_without_a_commit_attempt_first() {
    let (dir, plane, repo) = quitting_clone("[repos.widget]\nmode = \"push\"\nautosave = true\n");
    // `origin` in the SSH form charter recognises, mapped by the clone's own config onto a
    // local bare repository — the way `reposave`'s tests reach theirs, never the network.
    let bare = dir
        .path()
        .canonicalize()
        .unwrap()
        .join("forge/acme/widget.git");
    std::fs::create_dir_all(&bare).unwrap();
    assert!(crate::testgit::run(&bare, &["init", "-q", "--bare", "-b", "main", "."]).ok());
    let base = format!("file://{}/", bare.parent().unwrap().display());
    for args in [
        vec!["remote", "add", "origin", "git@github.com:acme/widget.git"],
        vec![
            "config",
            &format!("url.{base}.insteadOf"),
            "https://github.com/acme/",
        ],
    ] {
        assert!(crate::testgit::run(&repo.path, &args).ok(), "git {args:?}");
    }
    let head = head_of(&repo.path);

    let got = repo_at_quit(&plane, "alpha", &repo, &[], Duration::from_secs(20));

    assert_eq!(got, AtQuit::Pushed);
    let pushed = crate::testgit::run(&bare, &["rev-parse", "main"]).out;
    assert_eq!(pushed.trim(), head.trim());
    // One save, the push: a clean tree is not first put through a commit that has nothing
    // to commit.
    let lines = crate::planegit::journal(&plane);
    assert_eq!(lines.len(), 1, "{lines:?}");
}
