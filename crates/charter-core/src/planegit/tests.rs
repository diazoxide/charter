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
        // In the repo's own config, never the machine's: CI runners have no git identity. The
        // repo is made through `testgit`, so a developer's global `commit.gpgsign = true` is
        // not asked for here (charter-app#191); the test that proves `save` never runs a
        // signer turns signing on in the repo itself.
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

    /// [`Fixture::with_a_remote`], with this plane's commit on it and then somebody else's
    /// commit on top — so the next save's push is refused and it has to rebase.
    fn with_a_remote_that_moved(&self) -> PathBuf {
        let bare = self.with_a_remote();
        run(
            &self.root,
            &[
                "push",
                "-q",
                &bare.display().to_string(),
                "HEAD:refs/heads/main",
            ],
        );
        let theirs = self.root.parent().unwrap().join("theirs");
        run(
            self.root.parent().unwrap(),
            &[
                "clone",
                "-q",
                &bare.display().to_string(),
                &theirs.display().to_string(),
            ],
        );
        // The clone's own identity, as the plane's: CI runners have none of their own.
        run(&theirs, &["config", "user.name", "Other"]);
        run(&theirs, &["config", "user.email", "other@example.invalid"]);
        std::fs::write(theirs.join("theirs.md"), "theirs").unwrap();
        run(&theirs, &["add", "-A"]);
        run(&theirs, &["commit", "-q", "-m", "theirs"]);
        run(&theirs, &["push", "-q", "origin", "main"]);
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
    let done = crate::testgit::run(dir, args);
    assert!(done.ok(), "git {args:?} failed: {done:?}");
    done.out
}

fn ask(dir: &Path, args: &[&str]) -> String {
    crate::testgit::run(dir, args).out
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
        &crate::planesave::Settings::read(&fixture.root).plane,
        &["add", "--", "personas/steward/memory/m.md"],
        &mut Attempt::default(),
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

/// A plane with a commit of its own and a remote that moved, where checking out what the
/// remote added sleeps for twenty seconds — so a rebase onto it cannot finish inside a short
/// deadline. What holds it is a smudge filter that sleeps: git runs it for every file the
/// rebase checks out, and the runner turns off hooks and the fsmonitor but not filters —
/// standing in for a signer prompt, a lock, anything that waits. Answers the bare remote.
fn a_remote_no_rebase_can_finish_onto(fixture: &Fixture) -> PathBuf {
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
    // The clone's own identity, as the plane's: CI runners have none of their own.
    run(&theirs, &["config", "user.name", "Other"]);
    run(&theirs, &["config", "user.email", "other@example.invalid"]);
    std::fs::write(theirs.join(".gitattributes"), "slow.md filter=slow\n").unwrap();
    std::fs::write(theirs.join("slow.md"), "slow").unwrap();
    run(&theirs, &["add", "-A"]);
    run(&theirs, &["commit", "-q", "-m", "theirs"]);
    run(&theirs, &["push", "-q", "origin", "main"]);
    std::fs::write(fixture.root.join("mine.md"), "mine").unwrap();
    run(&fixture.root, &["add", "-A"]);
    run(&fixture.root, &["commit", "-q", "-m", "mine"]);
    run(
        &fixture.root,
        &["config", "filter.slow.smudge", "sleep 20; cat"],
    );
    run(&fixture.root, &["config", "filter.slow.clean", "cat"]);
    bare
}

#[test]
fn a_rebase_still_running_at_its_deadline_is_stopped_and_said_to_be_out_of_time() {
    // charter-app#242: the rebase used to have no deadline at all. The deadline is the
    // helper's argument so the test does not wait out the real one.
    let fixture = Fixture::plane();
    let bare = a_remote_no_rebase_can_finish_onto(&fixture);
    run(
        &fixture.root,
        &["fetch", "-q", &bare.display().to_string(), "main"],
    );

    let started = std::time::Instant::now();
    let rebased = rebase_onto_fetched(&fixture.root, false, std::time::Duration::from_secs(1));

    assert_eq!(rebased, Rebased::OutOfTime);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "it waited for the filter: {:?}",
        started.elapsed()
    );
}

#[test]
fn a_push_whose_rebase_runs_out_of_time_is_undone_reported_and_recorded_as_not_landed() {
    // charter-app#267: the deadline, end to end through the pusher — the push refused, the
    // fetch, the rebase stopped at its deadline, the abort, what the operator is told, and the
    // record `doctor` reads. Only the deadline is shortened.
    let fixture = Fixture::plane();
    let bare = a_remote_no_rebase_can_finish_onto(&fixture);
    let mine = ask(&fixture.root, &["rev-parse", "HEAD"]);

    let started = std::time::Instant::now();
    let (pushed, said) = push_within(&fixture, false, std::time::Duration::from_secs(1));

    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "it waited for the filter: {:?}",
        started.elapsed()
    );
    assert_eq!(pushed.outcome, Outcome::Failed, "{said}");
    assert!(said.contains("remote moved"), "{said}");
    assert!(
        said.contains("the rebase onto the remote did not finish within 1 seconds"),
        "{said}"
    );
    assert!(!said.contains("conflict"), "{said}");
    let git_dir = fixture.root.join(".git");
    assert!(
        !git_dir.join("rebase-merge").exists() && !git_dir.join("rebase-apply").exists(),
        "the rebase was left in progress"
    );
    assert_eq!(ask(&fixture.root, &["rev-parse", "HEAD"]), mine);
    assert_eq!(
        ask(&bare, &["log", "-1", "--format=%s", "main"]).trim(),
        "theirs"
    );
    let record = push_record(&fixture.root).expect("a record");
    assert_eq!(record["outcome"], "failed");
    assert_eq!(record["head"], mine.trim());
}

/// Set on the re-executed test binary: the path the signer touches when it is asked.
const SIGNING_CHILD: &str = "CHARTER_TEST_SIGNING_HOME_MARK";

#[test]
fn a_save_that_has_to_rebase_never_asks_the_signer_the_operators_home_names() {
    // charter-app#242. The commit `save` makes is unsigned by `-c`, but a remote that moved
    // sends it through `git rebase FETCH_HEAD`, which replays that commit — and a replay is a
    // commit, so it read the operator's global `commit.gpgsign = true` and asked the signer:
    // a 1Password prompt that blocks, or a signer that fails and turns an ordinary moved
    // remote into a reported conflict. The product's git clears its environment and keeps
    // `HOME`, which a test cannot set in its own process, so the signing HOME is given to a
    // CHILD: this test re-runs itself ([`crate::testrun`]).
    if let Some(ran) = std::env::var_os(SIGNING_CHILD) {
        let ran = PathBuf::from(ran);
        let fixture = Fixture::plane();
        // A plane charter never applied its policy to, which is any plane an operator cloned
        // or made by hand: the template's `commit.gpgsign = false` is taken out, so the
        // signing HOME is what governs it.
        run(&fixture.root, &["config", "--unset", "commit.gpgsign"]);
        assert_eq!(
            ask(&fixture.root, &["config", "--get", "commit.gpgsign"]).trim(),
            "true",
            "the control: the plane's git reads the signing home"
        );
        fixture.with_a_remote_that_moved();
        std::fs::write(fixture.root.join("mine.md"), "mine").unwrap();

        let (code, said) = fixture.just_save();

        assert_eq!(code, 0, "{said}");
        assert!(said.contains("remote moved"), "it rebased: {said}");
        assert!(said.contains("Pushed main via gh"), "{said}");
        assert!(!ran.exists(), "the signer was asked");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let top = dir.path().canonicalize().unwrap();
    let (home, ran) = crate::testgit::signing_home(&top);

    crate::testrun::rerun(
        &[
            "planegit::tests::a_save_that_has_to_rebase_never_asks_the_signer_the_operators_home_names",
        ],
        &[(SIGNING_CHILD, ran.as_os_str()), ("HOME", home.as_os_str())],
    );

    assert!(!ran.exists(), "the signer was asked");
}

/// Set on the re-executed test binary: the path the signer appends to for every commit it signs.
const SIGNED_CHILD: &str = "CHARTER_TEST_SIGNED_HOME_MARK";

#[test]
fn a_save_asked_to_sign_that_has_to_rebase_signs_the_commit_it_replays() {
    // The other half of charter-app#242. `--sign` is the operator asking for a signed commit,
    // and a rebase that replayed it under `-c commit.gpgsign=false` handed the remote an
    // unsigned copy without a word. The replay follows the commit's own choice.
    if let Some(ran) = std::env::var_os(SIGNED_CHILD) {
        let ran = PathBuf::from(ran);
        let fixture = Fixture::plane();
        run(&fixture.root, &["config", "--unset", "commit.gpgsign"]);
        fixture.with_a_remote_that_moved();
        std::fs::write(fixture.root.join("mine.md"), "mine").unwrap();

        let (code, said) = fixture.save(Request {
            root: &fixture.root,
            message: Some("a signed save"),
            sign: true,
            no_push: false,
            cwd: &fixture.root,
        });

        assert_eq!(code, 0, "{said}");
        assert!(said.contains("remote moved"), "it rebased: {said}");
        assert!(said.contains("Pushed main via gh"), "{said}");
        assert_eq!(fixture.head_subject(), "a signed save");
        assert!(
            ask(&fixture.root, &["cat-file", "commit", "HEAD"]).contains("\ngpgsig "),
            "the commit the rebase replayed is unsigned"
        );
        let signed = std::fs::read_to_string(&ran).unwrap_or_default();
        assert_eq!(
            signed.lines().count(),
            2,
            "signed once for the commit and once for its replay"
        );
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let top = dir.path().canonicalize().unwrap();
    let (home, ran) = crate::testgit::home_that_signs(&top);

    crate::testrun::rerun(
        &["planegit::tests::a_save_asked_to_sign_that_has_to_rebase_signs_the_commit_it_replays"],
        &[(SIGNED_CHILD, ran.as_os_str()), ("HOME", home.as_os_str())],
    );
}

/// Push HEAD with `sign`, and answer what it did and what it said.
fn push(fixture: &Fixture, sign: bool) -> (PushResult, String) {
    push_within(fixture, sign, WRITE)
}

/// [`push`], with the rebase given `deadline`.
fn push_within(
    fixture: &Fixture,
    sign: bool,
    deadline: std::time::Duration,
) -> (PushResult, String) {
    let mut said = String::new();
    let mut say = |line: Say| {
        said.push_str(&line.to_string());
        said.push('\n');
    };
    let pushed = push_head_within(&fixture.root, None, sign, deadline, &mut say);
    (pushed, said)
}

#[test]
fn a_signer_that_fails_while_a_signed_save_rebases_is_named_and_nothing_unsigned_is_pushed() {
    // charter-app#267. `--sign`, a remote that moved, and a signer that refuses the replay: git
    // stops the rebase with no conflict in it, and that was reported as one. The signer is
    // named, in its own words, and the push stops — the commit the operator asked to sign stays
    // local rather than reaching the remote unsigned.
    let fixture = Fixture::plane();
    let bare = fixture.with_a_remote_that_moved();
    std::fs::write(fixture.root.join("mine.md"), "mine").unwrap();
    run(&fixture.root, &["add", "-A"]);
    run(&fixture.root, &["commit", "-q", "-m", "mine"]);
    let signer = stand_in::program(
        fixture.root.parent().unwrap(),
        "gpg",
        "#!/bin/sh\necho 'card not present' >&2\nexit 1\n",
    );
    run(&fixture.root, &["config", "commit.gpgsign", "true"]);
    // The format too, in the repo's own config: a developer whose global `gpg.format` is `ssh`
    // would otherwise have their real signer — a 1Password prompt — asked instead of this one.
    run(&fixture.root, &["config", "gpg.format", "openpgp"]);
    run(
        &fixture.root,
        &["config", "gpg.program", &signer.display().to_string()],
    );
    let mine = ask(&fixture.root, &["rev-parse", "HEAD"]);

    let (pushed, said) = push(&fixture, true);

    assert_eq!(pushed.outcome, Outcome::Failed, "{said}");
    assert!(
        said.contains("the rebase could not sign the replayed commit"),
        "{said}"
    );
    assert!(
        said.contains("card not present"),
        "the signer's words: {said}"
    );
    assert!(!said.contains("conflict"), "{said}");
    assert!(pushed.detail.contains("card not present"), "{pushed:?}");
    assert_eq!(
        ask(&bare, &["log", "-1", "--format=%s", "main"]).trim(),
        "theirs",
        "an unsigned copy reached the remote"
    );
    assert_eq!(
        ask(&fixture.root, &["rev-parse", "HEAD"]),
        mine,
        "the rebase was not undone"
    );
    assert_eq!(push_record(&fixture.root).unwrap()["outcome"], "failed");
}

#[test]
fn a_rebase_that_really_conflicts_is_still_called_a_conflict() {
    // The control for the test above: the signer's case is carved out of `conflict`, and a
    // conflict is still one.
    let fixture = Fixture::plane();
    let bare = fixture.with_a_remote_that_moved();
    std::fs::write(fixture.root.join("theirs.md"), "mine, not theirs").unwrap();
    run(&fixture.root, &["add", "-A"]);
    run(&fixture.root, &["commit", "-q", "-m", "mine"]);

    let (pushed, said) = push(&fixture, false);

    assert_eq!(pushed.outcome, Outcome::Conflict, "{said}");
    assert!(said.contains("rebase hit a conflict"), "{said}");
    assert_eq!(
        ask(&bare, &["log", "-1", "--format=%s", "main"]).trim(),
        "theirs"
    );
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

// --------------------------------------------------------------------------------------- //
// how far a save goes: `[plane] mode` (charter-app#293, ADR 0051)                            //
// --------------------------------------------------------------------------------------- //

impl Fixture {
    /// Replace the plane's `charter.toml` — committed, so it is not itself the save's work.
    fn with_settings(&self, toml: &str) {
        std::fs::write(self.root.join("charter.toml"), toml).unwrap();
        run(&self.root, &["commit", "-q", "-am", "settings"]);
    }
}

#[test]
fn a_plane_whose_mode_is_off_is_never_committed_and_is_told_why() {
    let fixture = Fixture::plane();
    fixture.with_settings("[plane]\nmode = \"off\"\n");
    std::fs::write(fixture.root.join("work.md"), "work").unwrap();
    let before = fixture.commits();

    let (code, said) = fixture.just_save();

    assert_eq!(code, 0, "{said}");
    assert_eq!(fixture.commits(), before, "{said}");
    assert!(
        said.contains("[plane] mode is off (charter.toml), so charter commits nothing here"),
        "{said}"
    );
    assert_eq!(
        ask(&fixture.root, &["diff", "--cached", "--name-only"]).trim(),
        "",
        "and nothing was staged either"
    );
}

#[test]
fn a_plane_whose_mode_is_commit_is_committed_and_the_remote_is_left_where_it_was() {
    let fixture = Fixture::plane();
    let bare = fixture.with_a_remote();
    fixture.with_settings("[plane]\nmode = \"commit\"\n");
    std::fs::write(fixture.root.join("work.md"), "work").unwrap();

    let (code, said) = fixture.just_save();

    assert_eq!(code, 0, "{said}");
    assert_eq!(fixture.head_subject(), "a save");
    assert!(
        said.contains("Not pushed: [plane] mode is commit (charter.toml)."),
        "{said}"
    );
    assert_eq!(ask(&bare, &["log", "-1", "--format=%s", "main"]).trim(), "");
}

#[test]
fn a_pr_mode_never_pushes_to_the_target_branch_before_charter_can_open_the_pr() {
    // charter-app#298 builds the rolling save branch. Until then a PR mode commits and stops:
    // pushing to the target branch is exactly what the operator chose a PR mode to prevent.
    for mode in ["pr", "pr-merge"] {
        let fixture = Fixture::plane();
        let bare = fixture.with_a_remote();
        fixture.with_settings(&format!("[plane]\nmode = \"{mode}\"\n"));
        std::fs::write(fixture.root.join("work.md"), "work").unwrap();

        let (code, said) = fixture.just_save();

        assert_eq!(code, 0, "{said}");
        assert_eq!(fixture.head_subject(), "a save");
        assert!(
            said.contains(&format!(
                "Not pushed: [plane] mode is {mode} (charter.toml), and this charter cannot \
                 open the pull request yet."
            )),
            "{said}"
        );
        assert_eq!(
            ask(&bare, &["for-each-ref", "--format=%(refname)"]).trim(),
            "",
            "{mode}"
        );
    }
}

#[test]
fn a_push_lands_on_the_target_branch_the_settings_name_not_the_one_checked_out() {
    let fixture = Fixture::plane();
    let bare = fixture.with_a_remote();
    fixture.with_settings("[plane]\nmode = \"push\"\nbranch = \"trunk\"\n");
    std::fs::write(fixture.root.join("work.md"), "work").unwrap();

    let (code, said) = fixture.just_save();

    assert_eq!(code, 0, "{said}");
    assert!(said.contains("Pushed trunk via gh"), "{said}");
    assert_eq!(
        ask(&bare, &["log", "-1", "--format=%s", "trunk"]).trim(),
        "a save"
    );
    assert_eq!(ask(&bare, &["log", "-1", "--format=%s", "main"]).trim(), "");
}

const SETTINGS_SIGN_CHILD: &str = "CHARTER_TEST_SETTINGS_SIGN_MARK";

#[test]
fn a_plane_that_says_sign_asks_the_signer_without_being_told_on_the_command_line() {
    // Re-run in a child whose HOME signs every commit through a stand-in that SIGNS and counts
    // (`testgit::home_that_signs`): the product's git keeps HOME, so run in this process the
    // test would hand the developer's own signer — a 1Password prompt — a fixture's commit.
    // The HOME's `commit.gpgsign = true` is the machine's default, and the repo turns it off,
    // so only `[plane] sign` can be what asked the signer.
    if let Some(ran) = std::env::var_os(SETTINGS_SIGN_CHILD) {
        let ran = PathBuf::from(ran);
        let fixture = Fixture::plane();
        run(&fixture.root, &["config", "commit.gpgsign", "false"]);
        fixture.with_settings("[plane]\nsign = true\n");
        std::fs::write(fixture.root.join("work.md"), "work").unwrap();

        let (code, said) = fixture.save(Request {
            root: &fixture.root,
            message: Some("signed by the settings"),
            sign: false,
            no_push: true,
            cwd: &fixture.root,
        });

        assert_eq!(code, 0, "{said}");
        assert!(
            ask(&fixture.root, &["cat-file", "commit", "HEAD"]).contains("\ngpgsig "),
            "the commit is unsigned: {said}"
        );
        assert_eq!(
            std::fs::read_to_string(&ran)
                .unwrap_or_default()
                .lines()
                .count(),
            1
        );
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let top = dir.path().canonicalize().unwrap();
    let (home, ran) = crate::testgit::home_that_signs(&top);

    crate::testrun::rerun(
        &[
            "planegit::tests::a_plane_that_says_sign_asks_the_signer_without_being_told_on_the_command_line",
        ],
        &[
            (SETTINGS_SIGN_CHILD, ran.as_os_str()),
            ("HOME", home.as_os_str()),
        ],
    );
}

#[test]
fn a_save_with_no_message_says_what_changed_in_the_planes_own_words() {
    let fixture = Fixture::plane();
    for path in [
        "personas/steward/memory/a.md",
        "personas/steward/memory/b.md",
        "personas/_dispatch/2026-09.host.jsonl",
        "workspaces/ide/todos/t.json",
        "workspaces/ide/workspace.md",
        "inventory/repos.json",
    ] {
        let file = fixture.root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, "x").unwrap();
    }

    let (code, said) = fixture.save(Request {
        root: &fixture.root,
        message: None,
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });

    assert_eq!(code, 0, "{said}");
    assert_eq!(
        fixture.head_subject(),
        "charter save: 6 files (steward memory 2, dispatch 1, ide todos 1, ide workspace 1, \
         +1 more)"
    );
}

#[test]
fn one_file_is_one_file() {
    let fixture = Fixture::plane();
    std::fs::write(fixture.root.join("personas/steward/memory/a.md"), "x").unwrap();
    let (_, said) = fixture.save(Request {
        root: &fixture.root,
        message: None,
        sign: false,
        no_push: true,
        cwd: &fixture.root,
    });
    assert_eq!(
        fixture.head_subject(),
        "charter save: 1 file (steward memory 1)",
        "{said}"
    );
}

// --------------------------------------------------------------------------------------- //
// the save journal (charter-app#293)                                                        //
// --------------------------------------------------------------------------------------- //

#[test]
fn every_save_attempt_is_one_line_of_the_journal_saying_how_it_ended() {
    let fixture = Fixture::plane();
    fixture.with_a_remote();
    std::fs::write(fixture.root.join("work.md"), "work").unwrap();

    let (code, said) = fixture.just_save();
    assert_eq!(code, 0, "{said}");
    let _ = fixture.just_save(); // nothing left to save

    let lines = journal(&fixture.root);
    assert_eq!(lines.len(), 2, "{lines:?}");
    let saved = &lines[0];
    assert_eq!(saved["target"], "plane");
    assert_eq!(saved["trigger"], "cli");
    assert_eq!(saved["mode"], "push");
    assert_eq!(saved["files"], 1);
    assert_eq!(
        saved["commit"],
        ask(&fixture.root, &["rev-parse", "HEAD"]).trim(),
    );
    assert_eq!(saved["outcome"], "saved");
    assert!(saved["at"].as_f64().is_some_and(|at| at > 0.0));
    assert!(saved["ms"].as_u64().is_some());
    assert_eq!(lines[1]["outcome"], "skipped");
    assert_eq!(lines[1]["commit"], serde_json::Value::Null);
}

#[test]
fn a_secret_the_scan_caught_is_journalled_as_blocked_and_a_commit_mode_as_committed() {
    let fixture = Fixture::plane();
    std::fs::write(
        fixture.root.join("personas/steward/memory/m.md"),
        "token: ghp_0123456789abcdefghijklmnopqrstuvwxyz\n",
    )
    .unwrap();
    let _ = fixture.just_save();
    assert_eq!(journal(&fixture.root)[0]["outcome"], "blocked");

    let fixture = Fixture::plane();
    fixture.with_settings("[plane]\nmode = \"commit\"\n");
    std::fs::write(fixture.root.join("work.md"), "work").unwrap();
    let _ = fixture.just_save();
    let line = &journal(&fixture.root)[0];
    assert_eq!(
        (line["mode"].as_str(), line["outcome"].as_str()),
        (Some("commit"), Some("committed"))
    );
}

#[test]
fn the_journal_keeps_its_newest_five_hundred_lines() {
    let fixture = Fixture::plane();
    let path = journal_path(&fixture.root);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let old: String = (0..500).map(|n| format!("{{\"n\": {n}}}\n")).collect();
    std::fs::write(&path, old).unwrap();

    let _ = fixture.just_save();

    let lines = journal(&fixture.root);
    assert_eq!(lines.len(), 500);
    assert_eq!(lines[0]["n"], 1, "the oldest went");
    assert_eq!(lines[499]["outcome"], "skipped", "the newest came");
}
