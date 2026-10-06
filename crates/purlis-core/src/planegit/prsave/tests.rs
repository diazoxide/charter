//! [`push_save_branch`] against local bare remotes: what it asks the remote, and what it says
//! when the remote refuses (#464). The PR modes end to end are
//! `tests/a_pr_mode_save_keeps_one_pr_open_from_the_save_branch.rs`, against a stand-in `gh`.

use std::path::{Path, PathBuf};

use super::*;

const SAVE: &str = "charter/save/test";

fn run(dir: &Path, args: &[&str]) -> String {
    let done = crate::testgit::run(dir, args);
    assert!(done.ok(), "git {args:?} failed: {done:?}");
    done.out.trim().to_string()
}

/// A repository with one commit, an identity of its own, and an empty bare remote beside it.
fn plane_and_remote() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let top = dir.path().canonicalize().unwrap();
    let root = top.join("plane");
    let bare = top.join("remote.git");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&bare).unwrap();
    run(&bare, &["init", "-q", "--bare", "-b", "main", "."]);
    run(&root, &["init", "-q", "-b", "main", "."]);
    run(&root, &["config", "user.name", "Fixture"]);
    run(&root, &["config", "user.email", "fixture@example.invalid"]);
    std::fs::write(root.join("f.md"), "one\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "one"]);
    (dir, root, bare)
}

/// A hook in the bare remote `bare`.
fn hook(bare: &Path, name: &str, body: &str) {
    std::fs::create_dir_all(bare.join("hooks")).unwrap();
    stand_in::program(
        bare,
        &format!("hooks/{name}"),
        &format!("#!/bin/sh\n{body}\n"),
    );
}

/// How many git commands run in `root` named `verb`.
fn asked(root: &Path, verb: &str) -> usize {
    crate::worktree::git::tally::asked(root)
        .iter()
        .filter(|argv| argv.iter().any(|a| a == verb))
        .count()
}

/// [`push_save_branch`] of `root` to `url`, as a clone with nothing kept, and what it said.
fn push_first(root: &Path, url: &str) -> (Result<(), Box<PushResult>>, String) {
    let mut said = String::new();
    let mut say = |line: Say| {
        said.push_str(&line.to_string());
        said.push('\n');
    };
    let got = push_save_branch(root, url, "", SAVE, "main", &Kept::default(), &mut say);
    (got, said)
}

#[test]
fn a_push_that_lands_asks_the_remote_nothing_more() {
    let (_dir, root, bare) = plane_and_remote();

    let (got, said) = push_first(&root, &format!("file://{}", bare.display()));

    assert!(got.is_ok(), "{said}");
    assert_eq!(asked(&root, "push"), 1);
    assert_eq!(asked(&root, "ls-remote"), 0);
}

#[test]
fn a_push_that_lands_has_landed_even_when_the_remote_goes_away_right_after() {
    let (_dir, root, bare) = plane_and_remote();
    let gone = bare.with_extension("gone");
    hook(
        &bare,
        "post-receive",
        &format!("mv '{}' '{}'", bare.display(), gone.display()),
    );

    let (got, said) = push_first(&root, &format!("file://{}", bare.display()));

    assert!(gone.exists(), "the hook ran");
    assert!(got.is_ok(), "{said}");
}

#[test]
fn a_push_the_remote_refuses_for_its_own_reason_asks_it_nothing_more() {
    let (_dir, root, bare) = plane_and_remote();
    hook(
        &bare,
        "pre-receive",
        "echo 'policy: no pushes today' >&2; exit 1",
    );

    let (got, said) = push_first(&root, &format!("file://{}", bare.display()));

    assert!(got.is_err(), "{said}");
    assert_eq!(asked(&root, "ls-remote"), 0);
}

#[test]
fn a_push_the_remote_refuses_keeps_the_remotes_reason_when_the_remote_then_goes_away() {
    let (_dir, root, bare) = plane_and_remote();
    let gone = bare.with_extension("gone");
    hook(
        &bare,
        "pre-receive",
        &format!(
            "echo 'policy: no pushes today' >&2; mv '{}' '{}'; exit 1",
            bare.display(),
            gone.display()
        ),
    );

    let (got, said) = push_first(&root, &format!("file://{}", bare.display()));

    let refused = got.expect_err("refused");
    assert!(gone.exists(), "the hook ran");
    assert!(
        refused.detail.contains("policy: no pushes today"),
        "{refused:?} {said}"
    );
}

#[test]
fn a_save_branch_deleted_on_the_remote_while_a_first_push_was_on_its_way_is_pushed_afresh() {
    // D-HY8g, amended after review: the first push finds somebody's branch there, so its absent
    // lease is refused; by the time charter lists the remote it is gone, as a merged request's
    // branch is deleted. Gone is leased again as absent, so the save lands. The moment is
    // made with the listing's own upload-pack (`remote.<url>.uploadpack`, which git reads for
    // a remote named by its URL): it deletes the branch, then lists.
    let (dir, root, bare) = plane_and_remote();
    let top = dir.path().canonicalize().unwrap();
    let other = top.join("other");
    std::fs::create_dir_all(&other).unwrap();
    run(&other, &["init", "-q", "-b", "main", "."]);
    run(&other, &["config", "user.name", "Other"]);
    run(&other, &["config", "user.email", "other@example.invalid"]);
    std::fs::write(other.join("o.md"), "theirs\n").unwrap();
    run(&other, &["add", "-A"]);
    run(&other, &["commit", "-q", "-m", "theirs"]);
    run(
        &other,
        &[
            "push",
            "-q",
            &bare.display().to_string(),
            &format!("HEAD:refs/heads/{SAVE}"),
        ],
    );
    let url = "https://forge.invalid/acme/plane.git";
    run(
        &root,
        &[
            "config",
            &format!("url.file://{}.insteadOf", bare.display()),
            url,
        ],
    );
    run(
        &root,
        &[
            "config",
            &format!("remote.{url}.uploadpack"),
            &format!(
                "git -C '{}' update-ref -d refs/heads/{SAVE} && git-upload-pack",
                bare.display()
            ),
        ],
    );

    let (got, said) = push_first(&root, url);

    assert!(got.is_ok(), "{got:?} {said}");
    assert_eq!(asked(&root, "ls-remote"), 1, "{said}");
    assert_eq!(asked(&root, "push"), 2, "{said}");
    assert_eq!(
        run(&bare, &["rev-parse", &format!("refs/heads/{SAVE}")]),
        run(&root, &["rev-parse", "HEAD"]),
        "this clone's save is the branch now"
    );
}

/// What a clone keeps when a request is open from `branch`, as its saves left it.
fn kept_open_on(root: &Path, branch: &str) {
    Kept {
        branch: Some(branch.to_string()),
        pushed: Some("a".repeat(40)),
        pr: Some(KeptPr {
            number: 12,
            url: "https://x.invalid/pull/12".into(),
            head: "a".repeat(40),
            target: "main".into(),
        }),
        held: false,
    }
    .write(root);
}

#[test]
fn a_new_clone_saves_to_a_purlis_branch() {
    let (_dir, root, _bare) = plane_and_remote();
    let plane = crate::planesave::Settings::from_text(None, None).plane;
    let save = save_branch(&root, &plane);
    assert_eq!(
        save,
        format!("purlis/{}", crate::planesave::default_rest(&root))
    );
}

#[test]
fn a_request_still_open_from_the_charter_save_branch_carries_on_there_until_it_is_settled() {
    // V93j: the clone opened its request before the rename, from `charter/save/<host>-<clone>`.
    // While it is open, saves go on updating it there rather than opening a second one.
    let (_dir, root, _bare) = plane_and_remote();
    let plane = crate::planesave::Settings::from_text(None, None).plane;
    let old = format!("charter/{}", crate::planesave::default_rest(&root));
    kept_open_on(&root, &old);
    assert_eq!(save_branch(&root, &plane), old);

    // Once it is settled — merged, or closed — what is kept names no request, and the next
    // save goes to the purlis name.
    let mut kept = Kept::read(&root);
    forget_old_branch(&root, &plane, &mut kept);
    kept.write(&root);
    assert_eq!(
        save_branch(&root, &plane),
        format!("purlis/{}", crate::planesave::default_rest(&root))
    );

    // A request forgotten without being seen to merge or close — the plane was moved off its
    // commit by hand — is held, and may still be open: the old branch carries on, so none is
    // opened twice. The hold is kept on disk, and only a settle that saw it end lets it go.
    kept_open_on(&root, &old);
    let mut kept = Kept::read(&root);
    kept.pr = None;
    kept.held = true;
    kept.write(&root);
    assert!(Kept::read(&root).held);
    assert_eq!(save_branch(&root, &plane), old);
    let mut kept = Kept::read(&root);
    forget_old_branch(&root, &plane, &mut kept);
    kept.write(&root);
    assert!(!Kept::read(&root).held);
    assert_eq!(
        save_branch(&root, &plane),
        format!("purlis/{}", crate::planesave::default_rest(&root))
    );
}

#[test]
fn a_clone_upgraded_after_its_last_request_merged_saves_to_the_purlis_name() {
    // What a pre-rename clone keeps once its last request merged: the old branch, the commit it
    // pushed there, and no request. Nothing may still be open, so new work says purlis.
    let (_dir, root, _bare) = plane_and_remote();
    let plane = crate::planesave::Settings::from_text(None, None).plane;
    let rest = crate::planesave::default_rest(&root);
    std::fs::create_dir_all(kept_path(&root).parent().unwrap()).unwrap();
    std::fs::write(
        kept_path(&root),
        serde_json::json!({"branch": format!("charter/{rest}"), "pushed": "a".repeat(40), "pr": null})
            .to_string(),
    )
    .unwrap();
    assert_eq!(save_branch(&root, &plane), format!("purlis/{rest}"));
}

#[test]
fn a_block_about_the_charter_save_branch_still_holds_once_its_request_is_forgotten() {
    // A request closed without merging blocks the plane, and the block names the branch it was
    // on. Forgetting the request moves the next push to the purlis name; the block still holds.
    let (_dir, root, _bare) = plane_and_remote();
    let old = format!("charter/{}", crate::planesave::default_rest(&root));
    let head = run(&root, &["rev-parse", "HEAD"]);
    let rec =
        serde_json::json!({"outcome": "blocked", "branch": "main", "landed": old, "head": head});
    assert!(still_holds(&root, &rec, &head));
    let other = serde_json::json!({"outcome": "blocked", "branch": "main", "landed": "charter/save/elsewhere-000000", "head": head});
    assert!(!still_holds(&root, &other, &head));
}

#[test]
fn only_this_clones_own_default_branch_carries_on_and_a_save_branch_set_by_hand_wins() {
    let (_dir, root, _bare) = plane_and_remote();
    let plane = crate::planesave::Settings::from_text(None, None).plane;
    // Another clone's charter save branch, kept here somehow, is not this clone's.
    kept_open_on(&root, "charter/save/elsewhere-000000");
    assert_eq!(
        save_branch(&root, &plane),
        format!("purlis/{}", crate::planesave::default_rest(&root))
    );
    // A branch named by hand is the branch, whatever is kept.
    kept_open_on(
        &root,
        &format!("charter/{}", crate::planesave::default_rest(&root)),
    );
    let named = crate::planesave::Settings::from_text(
        Some("[plane]\nsave_branch = \"team/saves\"\n"),
        None,
    )
    .plane;
    assert_eq!(save_branch(&root, &named), "team/saves");
}
