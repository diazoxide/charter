//! `charter save`, against local repositories and a local bare remote.
//!
//! **Nothing here reaches a network.** The one remote these tests push to is a bare repository
//! in the test's own temporary directory, reached through an `url.<file>.insteadOf` rewrite in
//! the pushing repo's LOCAL config — which is read for a `push` (unlike a `clone`, whose config
//! search skips the local file). So the URL charter builds, the containment of it, the
//! credential rule and the protected-branch path are all exercised against the real `git push`,
//! and a forge is never asked for anything.

use std::path::{Path, PathBuf};

use super::*;
use crate::repocmd::Say;

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    /// A plane that is a git repository with one commit, an identity of its own, and no remote.
    fn plane() -> Fixture {
        let dir = tempfile::tempdir().expect("a temp dir");
        // Under the temp directory rather than AT it, so `root.parent()` is this test's own
        // scratch and a worktree or a bare remote beside the plane cannot land in /tmp.
        let root = dir
            .path()
            .canonicalize()
            .expect("a real path")
            .join("plane");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("charter.toml"), "[plane]\nname = \"fixture\"\n").unwrap();
        // What `charter init` writes: the plane's own machine-local state is not committed.
        std::fs::write(root.join(".gitignore"), ".charter/\n").unwrap();
        run(&root, &["init", "-q", "-b", "main", "."]);
        // In the repo's own config, never the machine's: CI runners have no git identity, and
        // a developer's global `commit.gpgsign = true` is exactly what these tests want left
        // in place.
        run(&root, &["config", "user.name", "Fixture"]);
        run(&root, &["config", "user.email", "fixture@example.invalid"]);
        std::fs::create_dir_all(root.join("personas/steward/memory")).unwrap();
        std::fs::write(root.join("README.md"), "one\n").unwrap();
        run(&root, &["add", "-A"]);
        run(&root, &["commit", "-q", "-m", "one"]);
        Fixture { _dir: dir, root }
    }

    /// A bare repository standing in for the forge.
    ///
    /// **`origin` is the SSH form, and the rewrite is keyed on the HTTPS one.** That ordering
    /// is what makes a local stand-in possible at all: `git remote get-url` APPLIES
    /// `url.<base>.insteadOf`, so a rule keyed on the URL charter reads would hand it a
    /// `file://` origin, charter would answer "that is on no forge I know", and nothing would
    /// be pushed — the test would pass by never testing. Keyed on the HTTPS prefix instead,
    /// `get-url` returns the SSH URL untouched, charter rewrites it to HTTPS itself (golden
    /// rule 0's own rule), and git maps THAT onto the bare repository. Measured both ways.
    fn with_a_remote(&self) -> PathBuf {
        let bare = self.root.parent().unwrap().join("forge/acme/plane.git");
        std::fs::create_dir_all(&bare).unwrap();
        run(&bare, &["init", "-q", "--bare", "-b", "main", "."]);
        run(
            &self.root,
            &["remote", "add", "origin", "git@github.com:acme/plane.git"],
        );
        let base = format!("file://{}/", bare.parent().unwrap().display());
        run(
            &self.root,
            &[
                "config",
                &format!("url.{base}.insteadOf"),
                "https://github.com/acme/",
            ],
        );
        bare
    }

    fn save(&self, request: Request) -> (u8, String) {
        let mut said = String::new();
        let mut say = |line: Say| {
            said.push_str(&line.to_string());
            said.push('\n');
        };
        let code = super::save(&request, &mut say);
        (code, said)
    }

    fn just_save(&self) -> (u8, String) {
        self.save(Request {
            root: &self.root,
            message: Some("a save"),
            sign: false,
            no_push: false,
            cwd: &self.root,
        })
    }

    fn head_subject(&self) -> String {
        run(&self.root, &["log", "-1", "--format=%s"]).trim().into()
    }

    fn commits(&self) -> usize {
        run(&self.root, &["rev-list", "--count", "HEAD"])
            .trim()
            .parse()
            .unwrap_or(0)
    }
}

/// git, for a test's own setup — not through the module under test.
fn run(dir: &Path, args: &[&str]) -> String {
    let done = git::run_untimed(dir, args).expect("git runs");
    assert!(done.ok(), "git {args:?} failed: {done:?}");
    done.out
}

fn ask(dir: &Path, args: &[&str]) -> String {
    git::run_untimed(dir, args).expect("git runs").out
}

// --------------------------------------------------------------------------------------- //
// what `save` stages                                                                        //
// --------------------------------------------------------------------------------------- //

#[test]
fn a_bare_separator_bounds_nothing_and_a_named_path_bounds_everything() {
    // Measured against git: `add` and `add --` stage nothing, `add -A`, `add -A --` and
    // `add -u` stage the whole tree, and `add -- . u.txt` stages it too.
    for wide in [
        &["add", "-A"][..],
        &["add", "--all"],
        &["add", "-u"],
        &["add", "--update"],
        &["add", "--no-ignore-removal"],
        &["add", "-A", "--"],
        &["add", "."],
        &["add", "--", "."],
        &["add", "--", ":/"],
        &["add", "--", "workspaces/x", "."],
    ] {
        assert!(stages_the_whole_tree(wide), "{wide:?}");
    }
    for scoped in [
        &["add"][..],
        &["add", "--"],
        &["add", "--", "personas/steward/memory/x.md"],
        &["add", "-A", "--", "workspaces/x", ".gitignore"],
        // A file really can be called `-A`, and past `--` git stages that file.
        &["add", "--", "-A"],
    ] {
        assert!(!stages_the_whole_tree(scoped), "{scoped:?}");
    }
}

#[test]
fn what_is_about_to_be_committed_is_broken_down_by_directory_before_it_is() {
    let fixture = Fixture::plane();
    for (path, text) in [
        ("personas/steward/memory/a.md", "a"),
        ("personas/steward/memory/b.md", "b"),
        ("personas/_shared/memory/c.md", "c"),
        // The `uv.lock` of the 2026-09-12 incident: a project file at the plane root that a
        // count of "3 file(s)" would never have surfaced.
        ("uv.lock", "lock"),
    ] {
        let file = fixture.root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: Some("a save"),
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });

    assert_eq!(code, 0, "{said}");
    assert!(said.contains("4 file(s) across 3 directories"), "{said}");
    assert!(said.contains("2  personas/steward/memory"), "{said}");
    assert!(said.contains("1  personas/_shared/memory"), "{said}");
    assert!(said.contains("1  (the plane root)"), "{said}");
    // And it is said BEFORE the commit line, which is the whole point: a breakdown printed
    // after the fact is a receipt, not a check.
    let breakdown = said.find("(the plane root)").expect("the breakdown");
    let committed = said.find("Committed ").expect("the commit line");
    assert!(breakdown < committed, "{said}");
}

// --------------------------------------------------------------------------------------- //
// the two trees `save` refuses to commit                                                    //
// --------------------------------------------------------------------------------------- //

#[test]
fn an_unbounded_stage_from_a_linked_worktree_of_the_plane_is_refused() {
    let fixture = Fixture::plane();
    let worktree = fixture.root.parent().unwrap().join("wt");
    run(
        &fixture.root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            &worktree.display().to_string(),
        ],
    );
    std::fs::write(fixture.root.join("mid-edit.md"), "the operator's").unwrap();

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: Some("the agent's"),
        sign: false,
        no_push: true,
        cwd: &worktree,
    });

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("a linked worktree of it"), "{said}");
    assert!(said.contains("CHARTER_ROOT="), "it names the override");
    // Nothing was staged: a refusal that has already run `git add -A` has done the damage and
    // merely declined to name it.
    assert_eq!(fixture.head_subject(), "one");
    assert!(
        ask(&fixture.root, &["diff", "--cached", "--name-only"])
            .trim()
            .is_empty(),
        "the refusal came before the add"
    );
}

#[test]
fn a_scoped_stage_from_a_linked_worktree_is_still_allowed() {
    // Reactive memory, the dispatch tally and the version pin all name plane-state files from
    // a worktree. Refusing those would stop every agent working in a worktree from recording
    // memory — a larger harm than the one the refusal above prevents.
    let fixture = Fixture::plane();
    let worktree = fixture.root.parent().unwrap().join("wt");
    run(
        &fixture.root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            &worktree.display().to_string(),
        ],
    );
    std::fs::write(fixture.root.join("personas/steward/memory/m.md"), "m").unwrap();

    let mut said = String::new();
    let mut say = |line: Say| said.push_str(&format!("{line}\n"));
    let code = super::commit_push(
        &Request {
            root: &fixture.root,
            message: Some("a memory"),
            sign: false,
            no_push: true,
            cwd: &worktree,
        },
        &["add", "--", "personas/steward/memory/m.md"],
        &mut say,
    );

    assert_eq!(code, 0, "{said}");
    assert_eq!(fixture.head_subject(), "a memory");
}

#[test]
fn an_unbounded_stage_from_a_plane_nested_in_the_workspaces_is_refused() {
    let outer = Fixture::plane();
    let inner = outer.root.join("workspaces/ide/charter");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(inner.join("charter.toml"), "[plane]\nname = \"inner\"\n").unwrap();
    run(&inner, &["init", "-q", "-b", "main", "."]);
    std::fs::write(outer.root.join("mid-edit.md"), "the operator's").unwrap();

    let (code, said) = outer.save(Request {
        root: &outer.root,
        message: Some("the agent's"),
        sign: false,
        no_push: true,
        cwd: &inner,
    });

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("a control plane of its own"), "{said}");
    assert_eq!(outer.head_subject(), "one");
}

#[test]
fn standing_in_the_plane_being_committed_is_never_one_of_those_two() {
    // The `$CHARTER_ROOT=<the clone>` hatch the refusals themselves print as the remedy: with
    // the plane being committed set to the one the caller stands in, both detectors must
    // answer `None`, or the advice would be a lie.
    let outer = Fixture::plane();
    let inner = outer.root.join("workspaces/ide/charter");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(inner.join("charter.toml"), "[plane]\nname = \"inner\"\n").unwrap();

    assert_eq!(nested_plane_in(&inner, &inner), None);
    assert_eq!(tree_of(&outer.root, &outer.root), None);
    assert_eq!(nested_plane_in(&outer.root, &outer.root), None);
}

// --------------------------------------------------------------------------------------- //
// the third answer                                                                          //
// --------------------------------------------------------------------------------------- //

#[test]
fn a_held_index_lock_is_a_refusal_and_the_word_clean_never_appears() {
    let fixture = Fixture::plane();
    std::fs::write(fixture.root.join("work.md"), "thirteen modified files").unwrap();
    std::fs::write(fixture.root.join(".git/index.lock"), "").unwrap();

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: None,
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("Refusing to save"), "{said}");
    assert!(said.contains("index.lock"), "it names the lock: {said}");
    assert!(said.contains("who holds it:"), "ps before rm: {said}");
    // The whole defect was a sentence containing this word.
    assert!(!said.to_lowercase().contains("clean"), "{said}");
    assert_eq!(fixture.head_subject(), "one", "nothing was committed");
}

#[test]
fn a_clean_tree_says_there_is_nothing_to_save_and_commits_nothing() {
    let fixture = Fixture::plane();

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: None,
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });

    assert_eq!(code, 0, "{said}");
    assert!(said.contains("Nothing to save"), "{said}");
    assert_eq!(fixture.commits(), 1);
}

#[test]
fn a_plane_that_is_not_a_git_repository_is_told_so_rather_than_reported_saved() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(root.join("charter.toml"), "").unwrap();
    let mut said = String::new();
    let mut say = |line: Say| said.push_str(&format!("{line}\n"));

    let code = save(
        &Request {
            root: &root,
            message: None,
            sign: false,
            no_push: true,
            cwd: &root,
        },
        &mut say,
    );

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("is not a git repository"), "{said}");
    assert!(said.contains("charter init does not create one"), "{said}");
}

// --------------------------------------------------------------------------------------- //
// the secret guard                                                                          //
// --------------------------------------------------------------------------------------- //

#[test]
fn a_secret_shaped_value_in_a_memory_file_stops_the_save_and_is_never_echoed() {
    let fixture = Fixture::plane();
    let secret = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
    std::fs::write(
        fixture.root.join("personas/steward/memory/leak.md"),
        format!("# the deploy\n\ntoken: {secret}\n"),
    )
    .unwrap();

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: None,
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("a secret-shaped value"), "{said}");
    assert!(said.contains("(credential assignment)"), "{said}");
    // The KIND, never the value.
    assert!(!said.contains(secret), "the refusal repeated the secret");
    assert_eq!(fixture.head_subject(), "one", "nothing was committed");
}

#[test]
fn a_secret_outside_a_memory_or_ref_file_is_not_what_this_guard_is_for() {
    // The guard is scoped to the two directories a save PUSHES to a shared repository. Widening
    // it would make `save` refuse an operator's own fixture file and get switched off.
    let fixture = Fixture::plane();
    std::fs::write(
        fixture.root.join("tests-fixture.md"),
        "token: ghp_0123456789abcdefghij\n",
    )
    .unwrap();

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: Some("a fixture"),
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });

    assert_eq!(code, 0, "{said}");
    assert_eq!(fixture.head_subject(), "a fixture");
}

#[test]
fn a_memory_file_that_leads_out_of_the_plane_is_refused_rather_than_read_through() {
    let fixture = Fixture::plane();
    let outside = fixture.root.parent().unwrap().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(
        outside.join("id_rsa"),
        "-----BEGIN OPENSSH PRIVATE KEY-----\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        outside.join("id_rsa"),
        fixture.root.join("personas/steward/memory/leak.md"),
    )
    .unwrap();

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: None,
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("it leads out of the plane"), "{said}");
    assert!(!said.contains("BEGIN OPENSSH"), "it read through the link");
}

#[test]
fn a_memory_file_that_is_a_link_is_refused_even_when_it_lands_back_inside_the_plane() {
    // What is COMMITTED for a link is the link — a blob holding the target's path — so a
    // scan of that blob would answer "no secret" about a file charter never read. The
    // containment check alone let this one through: the target is inside the plane.
    let fixture = Fixture::plane();
    // Outside `/memory/`, so the guard judges the LINK and not its target: a target the
    // guard also flagged would make this test pass for the wrong reason.
    let target = fixture.root.join("personas/steward/real.md");
    std::fs::write(&target, "# k\n\npassword: hunter2is\n").unwrap();
    std::os::unix::fs::symlink(
        &target,
        fixture.root.join("personas/steward/memory/leak.md"),
    )
    .unwrap();

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: None,
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("leak.md"), "{said}");
    assert_eq!(fixture.head_subject(), "one", "nothing was committed");
}

// --------------------------------------------------------------------------------------- //
// the secret guard reads what is STAGED (M3)                                                //
// --------------------------------------------------------------------------------------- //

#[test]
fn a_secret_staged_and_then_cleaned_out_of_the_working_tree_is_still_refused() {
    // **The bypass this guard had from the first line of it.** The guard took its list of
    // paths from the index and its BYTES from the working tree, and `skip-worktree` is what
    // stops `add -A` putting the two back in step. Measured with git 2.50.1: the row is
    // listed, the working tree says `clean`, the index and the commit say `password: …`.
    let fixture = Fixture::plane();
    let file = fixture.root.join("personas/steward/memory/n.md");
    std::fs::write(&file, "# the deploy\n\npassword: hunter2is\n").unwrap();
    run(&fixture.root, &["add", "personas/steward/memory/n.md"]);
    run(
        &fixture.root,
        &[
            "update-index",
            "--skip-worktree",
            "personas/steward/memory/n.md",
        ],
    );
    std::fs::write(&file, "clean\n").unwrap();
    assert_eq!(
        ask(&fixture.root, &["show", ":personas/steward/memory/n.md"]),
        "# the deploy\n\npassword: hunter2is\n",
        "the test's own premise: the index and the working tree disagree"
    );

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: None,
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("(credential assignment)"), "{said}");
    assert!(
        !said.contains("hunter2is"),
        "the refusal repeated the secret"
    );
    assert_eq!(fixture.head_subject(), "one", "nothing was committed");
}

#[test]
fn a_staged_path_git_would_quote_is_examined_rather_than_skipped() {
    // `core.quotePath` is on by default, so `--name-only` printed this row as
    // `"personas/caf\303\251/memory/n.md"` — quotes and octal escapes included. The
    // substring test still selected it and the file lookup then found nothing, so the row
    // went through unexamined. `-z` is what turns the quoting off.
    let fixture = Fixture::plane();
    let dir = fixture.root.join("personas/café/memory");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("n.md"), "# the deploy\n\npassword: hunter2is\n").unwrap();
    assert!(
        ask(&fixture.root, &["diff", "--cached", "--name-only"]).is_empty(),
        "the premise: `save` is what stages it"
    );

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: None,
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("(credential assignment)"), "{said}");
    assert_eq!(fixture.head_subject(), "one", "nothing was committed");
}

#[test]
fn a_staged_memory_file_that_is_not_utf8_is_examined_rather_than_skipped() {
    // `read_to_string` answered `Err` for it and the `if let Ok` fell through to no flag at
    // all — a guard failing in the one direction a guard may not. The blob is read as bytes
    // and the undecodable ones become replacement characters, which is enough to see an
    // ASCII credential sitting beside them.
    let fixture = Fixture::plane();
    let mut bytes = b"# the deploy\n\npassword: hunter2is\n".to_vec();
    bytes.extend_from_slice(&[0xff, 0xfe]);
    std::fs::write(fixture.root.join("personas/steward/memory/n.md"), bytes).unwrap();

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: None,
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });

    assert_eq!(code, 1, "{said}");
    assert!(said.contains("(credential assignment)"), "{said}");
    assert_eq!(fixture.head_subject(), "one", "nothing was committed");
}

// --------------------------------------------------------------------------------------- //
// signing                                                                                   //
// --------------------------------------------------------------------------------------- //

#[test]
fn the_signer_is_never_asked_even_when_the_operator_signs_every_commit() {
    // An operator with `commit.gpgsign = true` is the ordinary case this policy exists for. The
    // retry with `--no-gpg-sign` would rescue the commit anyway, so asserting only that a
    // commit lands proves nothing: what is asserted is that the SIGNER WAS NEVER RUN, which is
    // what "no signer prompt can hang an agent" actually means. Delete the `-c
    // commit.gpgsign=false` and this goes red.
    let fixture = Fixture::plane();
    let ran = fixture.root.parent().unwrap().join("gpg-ran");
    // Through `stand_in::program`: it is run the moment it is written, and a program this
    // process wrote through its own descriptor can lose to `ETXTBSY` (charter-app#81).
    let signer = stand_in::program(
        fixture.root.parent().unwrap(),
        "gpg",
        &format!("#!/bin/sh\ntouch '{}'\nexit 1\n", ran.display()),
    );
    run(&fixture.root, &["config", "commit.gpgsign", "true"]);
    run(
        &fixture.root,
        &["config", "gpg.program", &signer.display().to_string()],
    );
    std::fs::write(fixture.root.join("work.md"), "work").unwrap();

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: Some("unsigned"),
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });

    assert_eq!(code, 0, "{said}");
    assert_eq!(fixture.head_subject(), "unsigned");
    assert!(!ran.exists(), "the signer was run");
}

// --------------------------------------------------------------------------------------- //
// the origin, and the credential that is not in it                                          //
// --------------------------------------------------------------------------------------- //

#[test]
fn an_origin_carrying_a_token_is_refused_and_the_token_is_not_repeated() {
    let fixture = Fixture::plane();
    let token = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
    run(
        &fixture.root,
        &[
            "remote",
            "add",
            "origin",
            &format!("https://x-access-token:{token}@github.com/acme/plane.git"),
        ],
    );
    std::fs::write(fixture.root.join("work.md"), "work").unwrap();

    assert_eq!(origin_https(&fixture.root), None);

    let (code, said) = fixture.just_save();

    // Committed locally, and NOT pushed — which is the honest outcome, not a failure.
    assert_eq!(code, 0, "{said}");
    assert_eq!(fixture.head_subject(), "a save");
    assert!(said.contains("isn't on a forge charter knows"), "{said}");
    assert!(!said.contains(token), "the refusal repeated the token");
}

#[test]
fn an_origin_is_rewritten_to_its_forges_https_form_or_to_nothing_at_all() {
    let fixture = Fixture::plane();
    let set = |url: &str| {
        let _ = git::run_untimed(&fixture.root, &["remote", "remove", "origin"]);
        run(&fixture.root, &["remote", "add", "origin", url]);
        origin_https(&fixture.root)
    };

    assert_eq!(
        set("https://github.com/acme/plane.git").as_deref(),
        Some("https://github.com/acme/plane.git")
    );
    assert_eq!(
        set("git@github.com:acme/plane.git").as_deref(),
        Some("https://github.com/acme/plane.git")
    );
    assert_eq!(
        set("ssh://git@gitlab.com/acme/plane.git").as_deref(),
        Some("https://gitlab.com/acme/plane.git")
    );
    // A host this plane does not manage gets no answer rather than another forge's.
    assert_eq!(set("https://evil.example/acme/plane.git"), None);
    assert_eq!(set("/srv/plane.git"), None);
}

#[test]
fn the_pull_request_link_follows_the_forge_and_never_a_substring_of_the_url() {
    let fixture = Fixture::plane();
    let root = &fixture.root;
    assert_eq!(
        compare_url("https://github.com/acme/plane.git", "charter/abc", root).as_deref(),
        Some("https://github.com/acme/plane/compare/charter/abc?expand=1")
    );
    assert_eq!(
        compare_url("https://gitlab.com/acme/plane.git", "charter/abc", root).as_deref(),
        Some(
            "https://gitlab.com/acme/plane/-/merge_requests/new\
             ?merge_request%5Bsource_branch%5D=charter/abc"
        )
    );
    // The bug the substring check could never get right: a self-hosted GitLab with a
    // `mirrors/github.com/…` namespace was handed a GitHub compare URL.
    assert_eq!(
        compare_url(
            "https://gitlab.com/mirrors/github.com/acme/plane.git",
            "b",
            root
        )
        .as_deref(),
        Some(
            "https://gitlab.com/mirrors/github.com/acme/plane/-/merge_requests/new\
             ?merge_request%5Bsource_branch%5D=b"
        )
    );
    assert_eq!(compare_url("https://evil.example/a/b", "x", root), None);
    assert_eq!(
        compare_url("git@github.com:acme/plane.git", "x", root),
        None
    );
}

// --------------------------------------------------------------------------------------- //
// pushing, to a bare repository in this test's own directory                                //
// --------------------------------------------------------------------------------------- //

#[test]
fn a_save_lands_on_the_branch_head_is_on_and_the_record_is_cleared() {
    let fixture = Fixture::plane();
    let bare = fixture.with_a_remote();
    std::fs::write(fixture.root.join("personas/steward/memory/m.md"), "m").unwrap();
    // A record left by an earlier, worse day. A clean push deletes it rather than writing
    // "pushed": there is no condition left to carry.
    std::fs::create_dir_all(fixture.root.join(".charter")).unwrap();
    std::fs::write(
        push_record_path(&fixture.root),
        r#"{"outcome": "failed", "branch": "main", "head": "deadbeef"}"#,
    )
    .unwrap();

    let (code, said) = fixture.just_save();

    assert_eq!(code, 0, "{said}");
    assert!(said.contains("Pushed main via gh"), "{said}");
    assert_eq!(
        ask(&bare, &["log", "-1", "--format=%s", "main"]).trim(),
        "a save"
    );
    assert!(!push_record_path(&fixture.root).exists(), "the record");
}

#[test]
fn a_branch_that_requires_a_pull_request_gets_one_rather_than_a_stranded_commit() {
    let fixture = Fixture::plane();
    let bare = fixture.with_a_remote();
    // A real pre-receive hook refusing `refs/heads/main` in GitHub's own wording. Charter may
    // name a cause it RECOGNISED, never one it inferred, so the rejection is the evidence.
    std::fs::create_dir_all(bare.join("hooks")).unwrap();
    stand_in::program(
        &bare,
        "hooks/pre-receive",
        "#!/bin/sh\nwhile read _ _ ref; do\n  case \"$ref\" in refs/heads/main)\n    echo \
         'remote: error: GH006: Protected branch update failed for refs/heads/main.' >&2\n    \
         exit 1;; esac\ndone\nexit 0\n",
    );
    std::fs::write(fixture.root.join("work.md"), "work").unwrap();

    let (code, said) = fixture.just_save();

    // rc 0: the commit reached the remote, under another name.
    assert_eq!(code, 0, "{said}");
    assert!(said.contains("requires a pull request"), "{said}");
    assert!(said.contains("open it: https://github.com/"), "{said}");
    let short = ask(&fixture.root, &["rev-parse", "--short", "HEAD"]);
    let branch = format!("charter/{}", short.trim());
    assert_eq!(
        ask(&bare, &["log", "-1", "--format=%s", &branch]).trim(),
        "a save",
        "it landed on {branch}"
    );
    assert_eq!(
        ask(&bare, &["log", "-1", "--format=%s", "main"]).trim(),
        "",
        "and never on the protected branch"
    );
    // The record is what carries the open pull request to the next save.
    let record = push_record(&fixture.root).expect("a record");
    assert_eq!(record["outcome"], "branched");
    assert_eq!(record["landed"], branch);
}

#[test]
fn an_open_pull_request_branch_is_advanced_rather_than_replaced() {
    let fixture = Fixture::plane();
    let bare = fixture.with_a_remote();
    // A live record naming a branch the remote already has, at this plane's own HEAD.
    let head = ask(&fixture.root, &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    run(
        &fixture.root,
        &[
            "push",
            "-q",
            &bare.display().to_string(),
            "HEAD:refs/heads/charter/abc1234",
        ],
    );
    std::fs::create_dir_all(fixture.root.join(".charter")).unwrap();
    std::fs::write(
        push_record_path(&fixture.root),
        format!(
            r#"{{"outcome": "branched", "branch": "main", "landed": "charter/abc1234", "head": "{head}"}}"#
        ),
    )
    .unwrap();

    let reuse = open_pull_request_branch(&fixture.root);

    // There is no upstream, so the record's commit cannot be shown to have landed — and "I
    // could not check" must never read as "it landed".
    assert_eq!(reuse.as_deref(), Some("charter/abc1234"));
    // Once the branch has reached the tracked upstream the record is spent and the branch is
    // never resurrected.
    run(
        &fixture.root,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    run(
        &fixture.root,
        &["branch", "--set-upstream-to=origin/main", "main"],
    );
    assert!(is_spent(&fixture.root, &head));
    assert_eq!(open_pull_request_branch(&fixture.root), None);
}

#[test]
fn a_push_that_simply_fails_is_reported_and_the_save_is_still_a_success() {
    let fixture = Fixture::plane();
    let bare = fixture.with_a_remote();
    // The remote goes away between the commit and the push — a network, in the only shape a
    // test can have one.
    std::fs::remove_dir_all(&bare).unwrap();
    std::fs::write(fixture.root.join("work.md"), "work").unwrap();

    let (code, said) = fixture.just_save();

    assert_eq!(code, 0, "an ordinary push failure is not a failed save");
    assert_eq!(fixture.head_subject(), "a save");
    assert!(said.contains("the gh push failed"), "{said}");
    assert!(said.contains("Check `gh auth status`."), "{said}");
    let record = push_record(&fixture.root).expect("a record");
    assert_eq!(record["outcome"], "failed");
}

#[test]
fn a_remote_that_moved_is_fetched_rebased_onto_and_pushed_again() {
    let fixture = Fixture::plane();
    let bare = fixture.with_a_remote();
    run(
        &fixture.root,
        &[
            "push",
            "-q",
            &bare.display().to_string(),
            "HEAD:refs/heads/main",
        ],
    );
    // Somebody else's commit, already on the remote.
    let theirs = fixture.root.parent().unwrap().join("theirs");
    run(
        fixture.root.parent().unwrap(),
        &[
            "clone",
            "-q",
            &bare.display().to_string(),
            &theirs.display().to_string(),
        ],
    );
    run(&theirs, &["config", "user.name", "Other"]);
    run(&theirs, &["config", "user.email", "other@example.invalid"]);
    std::fs::write(theirs.join("theirs.md"), "theirs").unwrap();
    run(&theirs, &["add", "-A"]);
    run(&theirs, &["commit", "-q", "-m", "theirs"]);
    run(&theirs, &["push", "-q", "origin", "main"]);
    std::fs::write(fixture.root.join("mine.md"), "mine").unwrap();

    let (code, said) = fixture.just_save();

    assert_eq!(code, 0, "{said}");
    assert!(said.contains("remote moved"), "{said}");
    assert!(said.contains("Pushed main via gh"), "{said}");
    assert_eq!(
        ask(&bare, &["log", "-1", "--format=%s", "main"]).trim(),
        "a save"
    );
    // Rebased, not merged: their commit is the parent of ours.
    assert_eq!(
        ask(&bare, &["log", "-2", "--format=%s", "main"])
            .lines()
            .nth(1)
            .unwrap_or_default(),
        "theirs"
    );
}

#[test]
fn a_record_is_written_for_what_did_not_land_and_carries_the_commit_it_is_about() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(root.join("charter.toml"), "").unwrap();

    let back = record_push(
        &root,
        PushResult {
            detail: "! [remote rejected]".into(),
            ..PushResult::of(Outcome::Failed, "main")
        },
        "cafebabe",
    );

    assert_eq!(back.outcome, Outcome::Failed);
    let written = push_record(&root).expect("a record");
    assert_eq!(written["outcome"], "failed");
    assert_eq!(written["head"], "cafebabe");
    // A record with no `outcome` is no record: a defect in this file must not take a session
    // down, and must not be believed either.
    std::fs::write(push_record_path(&root), "{}").unwrap();
    assert_eq!(push_record(&root), None);
    std::fs::write(push_record_path(&root), "not json").unwrap();
    assert_eq!(push_record(&root), None);
}

#[test]
fn a_record_that_was_edited_cannot_put_a_word_of_its_own_into_gits_argv() {
    // `plane-push.json` is machine-local state anything running as the operator can rewrite,
    // and both values taken out of it reach argv. `merge-base --is-ancestor --help` exits 0,
    // which this module would read as "it landed" — and a memory that "landed" is never pushed
    // again.
    let fixture = Fixture::plane();
    std::fs::create_dir_all(fixture.root.join(".charter")).unwrap();
    let head = ask(&fixture.root, &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    run(
        &fixture.root,
        &["remote", "add", "origin", "git@github.com:acme/plane.git"],
    );
    run(
        &fixture.root,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    run(
        &fixture.root,
        &["branch", "--set-upstream-to=origin/main", "main"],
    );

    assert!(is_spent(&fixture.root, &head), "a real sha still answers");
    assert!(!is_spent(&fixture.root, "--help"), "and a flag never does");
    assert!(
        !is_spent(&fixture.root, "HEAD"),
        "nor a revision charter did not write"
    );

    let record = |landed: &str| {
        std::fs::write(
            push_record_path(&fixture.root),
            format!(
                r#"{{"outcome": "branched", "branch": "main", "landed": "{landed}", "head": "0000000"}}"#
            ),
        )
        .unwrap();
        open_pull_request_branch(&fixture.root)
    };
    assert_eq!(
        record("charter/abc1234").as_deref(),
        Some("charter/abc1234")
    );
    // The branch the whole pull-request path exists to leave alone.
    assert_eq!(record("main"), None);
    assert_eq!(record("charter/--force"), None);
    assert_eq!(record(""), None);
}

#[test]
fn a_planted_push_record_cannot_hang_the_save_or_be_read_whole() {
    // `save` reads this file on every run that pushes, so the three things a record must not
    // be are three ways to stop an operator's everyday command. Each assertion below goes red
    // on its own if the guard it is about is removed, and the FIFO one reports rather than
    // hangs — an unguarded read of a FIFO never returns, and a test that hangs is not a test.
    let fixture = Fixture::plane();
    let record = push_record_path(&fixture.root);
    std::fs::create_dir_all(record.parent().unwrap()).unwrap();
    let good = r#"{"outcome": "failed", "branch": "main", "head": "cafebabe"}"#;

    std::fs::write(&record, good).unwrap();
    assert!(push_record(&fixture.root).is_some(), "an ordinary record");

    // Too big to read whole. git cannot carry a FIFO, but a sparse file packs small and
    // arrives full size, and this one is still valid JSON with an outcome — so a reader
    // without the bound answers `Some` and the assertion below is the whole check.
    let padding = " ".repeat(crate::reopen::MAX_BYTES as usize);
    std::fs::write(
        &record,
        format!(r#"{{"outcome": "failed", "branch": "main", "head": "cafebabe"{padding}}}"#),
    )
    .unwrap();
    assert_eq!(push_record(&fixture.root), None, "a record too big to read");

    // Reached through a link. What it holds decides which remote branch the next save pushes
    // to, so a record from somewhere else is not this plane's record.
    let elsewhere = fixture.root.parent().unwrap().join("theirs.json");
    std::fs::write(&elsewhere, good).unwrap();
    std::fs::remove_file(&record).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &record).unwrap();
    assert_eq!(push_record(&fixture.root), None, "a linked record");

    // A FIFO is not a link, so the link check waves it through and the read never returns.
    std::fs::remove_file(&record).unwrap();
    let made = crate::forklock::status(std::process::Command::new("mkfifo").arg(&record))
        .expect("mkfifo runs");
    assert!(made.success(), "the test needs a fifo to plant");
    let (say, heard) = std::sync::mpsc::channel();
    let asked = fixture.root.clone();
    // In a thread, because the whole point is that the unguarded version never returns: the
    // deadline turns a hang into a failure instead of a run that never ends.
    std::thread::spawn(move || say.send(push_record(&asked)));
    let answered = heard
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("reading a fifo record must not block the save");
    assert_eq!(answered, None, "a fifo record was accepted");
}

#[test]
fn the_record_is_written_where_the_operator_put_their_state_and_read_back_from_there() {
    // `$CHARTER_HOME` moves the state directory verbatim, and `doctor` already resolves it —
    // so a `save` that hardcoded `<root>/.charter` wrote the record where nothing would ever
    // look for it. Asked of `plane::state_dir` itself, because a test that set the variable in
    // this process would leak it into every other test in this binary.
    let fixture = Fixture::plane();
    assert_eq!(
        push_record_path(&fixture.root),
        crate::plane::state_dir(&fixture.root).join("plane-push.json")
    );
    assert_eq!(
        crate::plane::state_dir(&fixture.root),
        fixture.root.join(".charter"),
        "and with no variable set it is the plane's own"
    );
}

#[test]
fn a_protected_branch_is_recognised_only_from_words_a_forge_actually_says() {
    for rejection in [
        "remote: error: GH006: Protected branch update failed",
        "! [remote rejected] main -> main (protected branch hook declined)",
        "remote: error: pre-receive hook declined",
        "remote: You are not allowed to push code to protected branches",
        "remote: create a merge_request for this branch",
        "remote: Required status check \"ci\" is expected.",
    ] {
        assert!(is_protected_rejection(rejection), "{rejection}");
    }
    for ordinary in [
        "! [rejected] main -> main (non-fast-forward)",
        "fatal: could not read from remote repository",
        "error: failed to push some refs",
        "",
    ] {
        assert!(!is_protected_rejection(ordinary), "{ordinary}");
    }
}
