//! A sandboxed chat in a linked worktree commits through the app (#1055, ruling V99h): the app
//! stages and commits in the folder it recorded the chat as standing in, on that folder's
//! branch, with git pinned to the checked git directory, hooks off and no program run.

mod support;

use std::path::{Path, PathBuf};

use purlis_core::gitbroker::{Asker, commit};
use purlis_core::hookwire::{Answer, CommitAsk, Stage};
use purlis_core::repocmd::Say;
use purlis_core::worktree::{self, git, link};

const ULID: &str = "01J9ZQ3W5Y7X8V6T4R2P0N1M3K";

/// A project with the sandbox on, one clone, and a branch folder `piece` cut from it.
struct Project {
    f: support::Fixture,
    piece: PathBuf,
}

fn project() -> Project {
    let f = support::plane_with_clone("thing");
    std::fs::write(
        f.plane.join("charter.toml"),
        "schema = 1\n[sandbox]\nmode = \"on\"\n",
    )
    .unwrap();
    let piece = worktree::add(&f.plane, &f.ws, &f.repo, "piece", None)
        .expect("a piece is cut")
        .path;
    Project { f, piece }
}

impl Project {
    /// What the app answers chat 4, standing in `cwd`, asking for `stage` to be committed.
    fn asked_from(&self, cwd: &Path, stage: Stage) -> Answer {
        let asker = Asker {
            chat: 4,
            cwd: Some(cwd.to_path_buf()),
            persona: Some("steward".to_owned()),
            harnessed: true,
            unsandboxed: false,
            config: None,
        };
        let ask = CommitAsk {
            chat: 4,
            message: "piece: one change\n\nWhy it was made.".to_owned(),
            stage,
        };
        let identity = git::Isolated::identity(Some("The Person"), Some("person@example.invalid"));
        commit::answer(&self.f.plane, &asker, &ask, &identity)
    }

    fn asked(&self, stage: Stage) -> Answer {
        self.asked_from(&self.piece, stage)
    }

    fn git(&self, dir: &Path, args: &[&str]) -> String {
        String::from_utf8_lossy(&support::git(dir, args).stdout)
            .trim_end()
            .to_owned()
    }

    fn head(&self) -> String {
        self.git(&self.piece, &["rev-parse", "HEAD"])
    }

    /// Whether the index is as the last commit left it.
    fn nothing_staged(&self) -> bool {
        self.git(&self.piece, &["diff", "--cached", "--name-only"])
            .is_empty()
    }
}

fn paths(of: &[&str]) -> Stage {
    Stage::Paths(of.iter().map(|path| (*path).to_owned()).collect())
}

/// The commit an answer names, and the lines after its first.
fn committed(answer: &Answer) -> (String, Vec<Say>) {
    let Answer::Said { lines, code: 0 } = answer else {
        panic!("not committed: {answer:?}");
    };
    let Some(Say::Done(first)) = lines.first() else {
        panic!("no commit named: {lines:?}");
    };
    let id = first
        .strip_prefix("committed ")
        .and_then(|rest| rest.split(' ').next())
        .expect("an id")
        .to_owned();
    (id, lines[1..].to_vec())
}

fn refused(answer: &Answer) -> &str {
    let Answer::No { why } = answer else {
        panic!("not refused: {answer:?}");
    };
    why
}

#[test]
fn the_commit_lands_on_the_folders_branch_with_the_named_files_the_identity_and_the_trailers() {
    purlis_core::unsteered!();
    let p = project();
    purlis_core::reopen::write(
        &p.f.plane,
        &purlis_core::reopen::Record {
            chats: vec![purlis_core::reopen::Chat {
                program: "/bin/zsh".into(),
                profile: Some("claude".to_owned()),
                name: "chat 4".to_owned(),
                number: Some(4),
                persona: Some("steward".to_owned()),
                identity: purlis_core::reopen::Identity {
                    id: Some(ULID.to_owned()),
                    ..Default::default()
                },
                ..Default::default()
            }],
            dealt: 9,
            ..Default::default()
        },
    )
    .unwrap();
    let main_before = p.git(&p.f.clone, &["rev-parse", "refs/heads/main"]);
    let before = p.head();
    std::fs::create_dir_all(p.piece.join("src")).unwrap();
    std::fs::write(p.piece.join("src/new.rs"), "fn new() {}\n").unwrap();
    std::fs::write(p.piece.join("README.md"), "one\ntwo\n").unwrap();
    std::fs::write(p.piece.join("left-out.txt"), "not named\n").unwrap();

    let answer = p.asked(paths(&["src/new.rs", "README.md"]));

    let (id, rest) = committed(&answer);
    assert!(rest.is_empty(), "no hook is there to speak of: {rest:?}");
    assert_eq!(id, p.head());
    assert_ne!(id, before);
    // On the folder's own branch, a child of what it was on; the clone's branch did not move.
    assert_eq!(p.git(&p.f.clone, &["rev-parse", "refs/heads/piece"]), id);
    assert_eq!(p.git(&p.piece, &["rev-parse", "HEAD^"]), before);
    assert_eq!(
        p.git(&p.f.clone, &["rev-parse", "refs/heads/main"]),
        main_before
    );
    // Exactly the named files.
    let mut files: Vec<String> = p
        .git(&p.piece, &["show", "--format=", "--name-only", "HEAD"])
        .lines()
        .map(str::to_owned)
        .collect();
    files.sort();
    assert_eq!(files, ["README.md", "src/new.rs"]);
    assert_eq!(
        p.git(&p.piece, &["status", "--porcelain"]),
        "?? left-out.txt"
    );
    // The person's identity, and the chat's trailers after the message as written.
    assert_eq!(
        p.git(&p.piece, &["log", "-1", "--format=%an <%ae>|%cn <%ce>"]),
        "The Person <person@example.invalid>|The Person <person@example.invalid>"
    );
    let message = p.git(&p.piece, &["log", "-1", "--format=%B"]);
    assert!(
        message.starts_with("piece: one change\n\nWhy it was made.\n\n"),
        "{message}"
    );
    assert!(
        message.contains(&format!(
            "{}: claude-code",
            purlis_core::provenance::ASSISTED_BY
        )),
        "{message}"
    );
    assert!(
        message.contains(&format!("{}: {ULID}", purlis_core::provenance::CHAT)),
        "{message}"
    );
    assert!(
        message.contains(&format!("{}: steward", purlis_core::provenance::PERSONA)),
        "{message}"
    );
}

#[test]
fn every_tracked_change_is_staged_and_a_new_file_is_not() {
    purlis_core::unsteered!();
    let p = project();
    std::fs::write(p.piece.join("README.md"), "changed\n").unwrap();
    std::fs::write(p.piece.join("new.txt"), "new\n").unwrap();
    // From a folder below the top: the whole branch folder's tracked changes all the same.
    let below = p.piece.join("deep");
    std::fs::create_dir_all(&below).unwrap();

    committed(&p.asked_from(&below, Stage::Tracked));

    assert_eq!(
        p.git(&p.piece, &["show", "--format=", "--name-only", "HEAD"]),
        "README.md"
    );
    assert_eq!(p.git(&p.piece, &["status", "--porcelain"]), "?? new.txt");
    // And with nothing changed there is nothing to commit, in a sentence.
    let again = p.asked(Stage::Tracked);
    assert!(refused(&again).contains("nothing to commit"), "{again:?}");
}

#[test]
fn a_path_is_read_from_where_the_chat_stands() {
    purlis_core::unsteered!();
    let p = project();
    let below = p.piece.join("deep");
    std::fs::create_dir_all(&below).unwrap();
    std::fs::write(below.join("a.txt"), "a\n").unwrap();

    committed(&p.asked_from(&below, paths(&["a.txt"])));

    assert_eq!(
        p.git(&p.piece, &["show", "--format=", "--name-only", "HEAD"]),
        "deep/a.txt"
    );
}

#[cfg(unix)]
#[test]
fn a_hook_in_the_clone_does_not_run_and_the_answer_says_so() {
    purlis_core::unsteered!();
    use std::os::unix::fs::PermissionsExt as _;
    let p = project();
    let ran = p.f.plane.join("the-hook-ran");
    let hooks = p.f.clone.join(".git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    for name in ["pre-commit", "commit-msg", "post-commit"] {
        let hook = hooks.join(name);
        std::fs::write(
            &hook,
            format!("#!/bin/sh\ntouch '{}'\nexit 1\n", ran.display()),
        )
        .unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(p.piece.join("README.md"), "changed\n").unwrap();

    let (_, rest) = committed(&p.asked(Stage::Tracked));

    assert!(
        !ran.exists(),
        "a hook of the repository ran outside the sandbox"
    );
    let said: Vec<String> = rest
        .iter()
        .map(|line| match line {
            Say::Warn(text) => text.clone(),
            other => panic!("not a warning: {other:?}"),
        })
        .collect();
    assert_eq!(
        said,
        ["pre-commit", "commit-msg", "post-commit"].map(|name| format!(
            "this repository's {name} hook was not run: for a sandboxed chat, purlis commits with hooks off"
        ))
    );
}

#[test]
fn a_repository_whose_config_names_a_program_is_refused_and_nothing_is_staged() {
    purlis_core::unsteered!();
    let p = project();
    let ran = p.f.plane.join("the-filter-ran");
    let config = p.f.clone.join(".git/config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(&format!(
        "[filter \"evil\"]\n\tclean = \"sh -c 'touch {}; cat'\"\n\tsmudge = cat\n",
        ran.display()
    ));
    std::fs::write(&config, text).unwrap();
    std::fs::write(p.piece.join(".gitattributes"), "* filter=evil\n").unwrap();
    std::fs::write(p.piece.join("README.md"), "changed\n").unwrap();
    let before = p.head();

    let answer = p.asked(paths(&["README.md", ".gitattributes"]));

    let why = refused(&answer);
    assert!(why.contains("filter.evil.clean"), "{why}");
    assert!(why.contains("names a program"), "{why}");
    assert!(!ran.exists(), "the filter ran");
    assert_eq!(p.head(), before);
    assert!(p.nothing_staged());
}

#[test]
fn a_file_marked_for_a_content_filter_is_refused_and_attributes_alone_run_nothing() {
    purlis_core::unsteered!();
    // The person's filters (an LFS one, most often) are defined in the config this commit
    // does not read. A filter no config defines runs nothing, and would store the file as it
    // is on disk where the person's own git stores the filtered form.
    let p = project();
    std::fs::write(
        p.piece.join(".gitattributes"),
        "*.bin filter=lfs diff=lfs merge=lfs -text\n*.txt text eol=lf\n",
    )
    .unwrap();
    std::fs::write(p.piece.join("big.bin"), "the real bytes\n").unwrap();
    std::fs::write(p.piece.join("a.txt"), "a\n").unwrap();
    let before = p.head();

    let answer = p.asked(paths(&["big.bin", "a.txt", ".gitattributes"]));
    let why = refused(&answer);
    assert!(
        why.contains("\"big.bin\" is marked for a content filter"),
        "{why}"
    );
    assert_eq!(p.head(), before);
    assert!(p.nothing_staged());

    committed(&p.asked(paths(&[".gitattributes", "a.txt"])));
    assert_eq!(p.git(&p.piece, &["cat-file", "-p", "HEAD:a.txt"]), "a");
    assert_eq!(p.git(&p.piece, &["status", "--porcelain"]), "?? big.bin");
}

#[test]
fn a_rewritten_git_link_is_refused_whatever_it_names() {
    purlis_core::unsteered!();
    let p = project();
    let other = worktree::add(&p.f.plane, &p.f.ws, &p.f.repo, "other", None)
        .unwrap()
        .path;
    std::fs::write(p.piece.join("README.md"), "changed\n").unwrap();
    let before = (p.head(), p.git(&other, &["rev-parse", "HEAD"]));
    let own = p.piece.join("store");
    support::git(
        &p.f.plane,
        &[
            "clone",
            "-q",
            "--bare",
            &p.f.clone.display().to_string(),
            &own.display().to_string(),
        ],
    );
    let theirs = std::fs::read_to_string(other.join(".git")).unwrap();
    let was = std::fs::read_to_string(p.piece.join(".git")).unwrap();
    let inside = p.piece.join("own/.git/worktrees/piece");
    let module = p.piece.join("own/.git/modules/lib");
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::create_dir_all(&module).unwrap();
    std::fs::write(inside.join("commondir"), "../..\n").unwrap();
    std::fs::write(
        inside.join("gitdir"),
        format!("{}\n", p.piece.join(".git").display()),
    )
    .unwrap();
    for (link_text, why) in [
        // A git directory the chat made.
        (format!("gitdir: {}\n", own.display()), link::CHANGED),
        // Another branch folder's: the commit would land on its branch.
        (theirs, link::CHANGED),
        // A whole `.git` of the chat's own inside the folder, with a worktree entry that
        // names the folder back, and with a module: well formed, and the chat's to make.
        (format!("gitdir: {}\n", inside.display()), link::CHANGED),
        (format!("gitdir: {}\n", module.display()), link::CHANGED),
        // The clone's own `.git`: its main branch.
        (
            format!("gitdir: {}\n", p.f.clone.join(".git").display()),
            link::CHANGED,
        ),
        ("gitdir: /nonexistent/nowhere\n".to_owned(), link::GONE),
    ] {
        std::fs::write(p.piece.join(".git"), &link_text).unwrap();
        let answer = p.asked(Stage::Tracked);
        assert_eq!(refused(&answer), why, "{link_text}");
    }
    std::fs::write(p.piece.join(".git"), was).unwrap();
    assert_eq!(
        (p.head(), p.git(&other, &["rev-parse", "HEAD"])),
        before,
        "a branch moved"
    );
    assert_eq!(
        p.git(&p.f.clone, &["rev-parse", "refs/heads/main"]),
        p.git(&p.f.clone, &["rev-parse", "refs/heads/other"]),
        "the clone's branch moved"
    );
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_folder_is_stored_as_a_link_and_never_read() {
    purlis_core::unsteered!();
    let p = project();
    let outside = p.f.plane.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let secret = outside.join("secret.txt");
    std::fs::write(&secret, "what the chat may not read out\n").unwrap();
    std::os::unix::fs::symlink(&secret, p.piece.join("to-a-file")).unwrap();
    std::os::unix::fs::symlink(&outside, p.piece.join("to-a-folder")).unwrap();

    // Through the link: refused by purlis before git is asked.
    let through = p.asked(paths(&["to-a-folder/secret.txt"]));
    assert!(refused(&through).contains("through a link"), "{through:?}");
    assert!(p.nothing_staged());

    // The links themselves: git records where each points, and none of what is there.
    committed(&p.asked(paths(&["to-a-file", "to-a-folder"])));
    for name in ["to-a-file", "to-a-folder"] {
        let entry = p.git(&p.piece, &["ls-tree", "HEAD", name]);
        assert!(entry.starts_with("120000 blob "), "{entry}");
    }
    assert_eq!(
        p.git(&p.piece, &["cat-file", "-p", "HEAD:to-a-file"]),
        secret.display().to_string()
    );
    let shown = p.git(&p.piece, &["show", "HEAD"]);
    assert!(
        !shown.contains("may not read out"),
        "the file's contents were committed: {shown}"
    );
}

#[test]
fn a_file_named_like_an_option_is_a_file() {
    purlis_core::unsteered!();
    let p = project();
    for name in ["-n", "--amend", "--all", ":(top)README.md", "*.md"] {
        std::fs::write(p.piece.join(name), "a name\n").unwrap();
    }
    // `*.md` is that file, not every Markdown file: README.md is changed and not committed.
    std::fs::write(p.piece.join("README.md"), "changed\n").unwrap();

    committed(&p.asked(paths(&[
        "-n",
        "--amend",
        "--all",
        ":(top)README.md",
        "*.md",
    ])));

    let mut files: Vec<String> = p
        .git(
            &p.piece,
            &[
                "-c",
                "core.quotePath=false",
                "show",
                "--format=",
                "--name-only",
                "HEAD",
            ],
        )
        .lines()
        .map(str::to_owned)
        .collect();
    files.sort();
    assert_eq!(files, ["*.md", "--all", "--amend", "-n", ":(top)README.md"]);
    assert_eq!(
        p.git(&p.piece, &["log", "--format=%s", "-2"])
            .lines()
            .count(),
        2
    );
    assert_eq!(p.git(&p.piece, &["status", "--porcelain"]), " M README.md");
}

#[test]
fn a_repository_inside_the_folder_and_a_submodule_declaration_are_refused_and_unstaged() {
    purlis_core::unsteered!();
    let p = project();
    let nested = p.piece.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    support::git(&nested, &["init", "-q", "-b", "main", "."]);
    std::fs::write(nested.join("a.txt"), "a\n").unwrap();
    support::git(&nested, &["add", "a.txt"]);
    support::git(&nested, &["commit", "-q", "-m", "one"]);
    std::fs::write(
        p.piece.join(".gitmodules"),
        "[submodule \"x\"]\n\tpath = x\n\turl = ext::sh -c 'touch ran'\n",
    )
    .unwrap();
    let before = p.head();

    let gitlink = p.asked(paths(&["nested"]));
    assert!(
        refused(&gitlink).contains("repository of its own"),
        "{gitlink:?}"
    );
    assert!(p.nothing_staged());

    let modules = p.asked(paths(&[".gitmodules"]));
    assert!(
        refused(&modules).contains("declares submodules"),
        "{modules:?}"
    );
    assert!(p.nothing_staged());

    // A path that names a `.git` never reaches git.
    let inside = p.asked(paths(&["nested/.git/config"]));
    assert!(refused(&inside).contains("`.git`"), "{inside:?}");
    assert_eq!(p.head(), before);
}

#[test]
fn what_looks_like_a_secret_is_refused_by_the_scan_and_unstaged() {
    purlis_core::unsteered!();
    let p = project();
    let key = ["-----BEGIN RSA PRIV", "ATE KEY-----"].concat();
    std::fs::write(p.piece.join("id"), format!("{key}\nMIIB\n")).unwrap();
    let before = p.head();

    let answer = p.asked(paths(&["id"]));

    assert!(refused(&answer).contains("commit refused"), "{answer:?}");
    assert_eq!(p.head(), before);
    assert!(p.nothing_staged());
}

#[test]
fn the_repositorys_own_identity_is_used_where_it_has_one_and_one_that_signs_is_refused() {
    purlis_core::unsteered!();
    let p = project();
    support::git(&p.f.clone, &["config", "user.name", "The Repo"]);
    support::git(
        &p.f.clone,
        &["config", "user.email", "repo@example.invalid"],
    );
    std::fs::write(p.piece.join("README.md"), "changed\n").unwrap();

    committed(&p.asked(Stage::Tracked));
    assert_eq!(
        p.git(&p.piece, &["log", "-1", "--format=%an <%ae>"]),
        "The Repo <repo@example.invalid>"
    );

    support::git(&p.f.clone, &["config", "commit.gpgsign", "true"]);
    std::fs::write(p.piece.join("README.md"), "changed again\n").unwrap();
    let signed = p.asked(Stage::Tracked);
    assert!(refused(&signed).contains("signs its commits"), "{signed:?}");
    assert!(p.nothing_staged());
}

#[test]
fn a_folder_that_is_not_the_chats_branch_folder_or_is_mid_something_is_refused() {
    purlis_core::unsteered!();
    let p = project();
    std::fs::write(p.f.clone.join("README.md"), "changed in the clone\n").unwrap();
    // The clone, the workspace and the project's top: plain git's, or nothing's.
    for cwd in [p.f.clone.clone(), p.f.workspace(), p.f.plane.clone()] {
        let answer = p.asked_from(&cwd, Stage::Tracked);
        assert!(
            refused(&answer).contains("does not stand in a branch folder"),
            "{}: {answer:?}",
            cwd.display()
        );
    }
    assert_eq!(
        p.git(&p.f.clone, &["diff", "--cached", "--name-only"]),
        "",
        "the clone was staged in"
    );
    assert_eq!(
        p.git(&p.f.clone, &["rev-parse", "HEAD"]),
        p.head(),
        "the clone's branch moved"
    );

    // Something already staged there is the person's, and is left alone.
    std::fs::write(p.piece.join("staged.txt"), "theirs\n").unwrap();
    support::git(&p.piece, &["add", "staged.txt"]);
    let staged = p.asked(Stage::Tracked);
    assert!(
        refused(&staged).contains("purlis did not stage"),
        "{staged:?}"
    );
    assert_eq!(
        p.git(&p.piece, &["diff", "--cached", "--name-only"]),
        "staged.txt"
    );
    support::git(&p.piece, &["reset", "-q"]);

    // A detached HEAD has no branch to commit on.
    support::git(&p.piece, &["checkout", "-q", "--detach"]);
    std::fs::write(p.piece.join("README.md"), "changed\n").unwrap();
    let detached = p.asked(Stage::Tracked);
    assert!(
        refused(&detached).contains("not on a branch"),
        "{detached:?}"
    );
    assert!(p.nothing_staged());
}

#[test]
fn a_dispatched_tasks_own_folder_is_committed_in_as_any_branch_folder_is() {
    purlis_core::unsteered!();
    // A task's worktree is a folder purlis cut, named for the task and its id's tail
    // (`<slug>-<id tail>`), in the same place a workspace's branch folders are. The chat
    // started in it commits there by the same ask, on the task's own branch.
    let p = project();
    let task = "check-the-queue-b5rc0def";
    let folder = worktree::add(&p.f.plane, &p.f.ws, &p.f.repo, task, None)
        .expect("the task's worktree is cut")
        .path;
    assert_eq!(
        folder,
        p.f.workspace()
            .join(".worktrees")
            .join(&p.f.repo)
            .join(task)
    );
    std::fs::write(folder.join("README.md"), "the task's change\n").unwrap();
    std::fs::write(folder.join("notes.txt"), "new\n").unwrap();
    let others_before = (
        p.head(),
        p.git(&p.f.clone, &["rev-parse", "refs/heads/main"]),
    );

    let (id, _) = committed(&p.asked_from(&folder, paths(&["README.md", "notes.txt"])));

    let on_task = format!("refs/heads/{task}");
    assert_eq!(p.git(&p.f.clone, &["rev-parse", &on_task]), id);
    assert_eq!(p.git(&folder, &["status", "--porcelain"]), "");
    assert_eq!(
        (
            p.head(),
            p.git(&p.f.clone, &["rev-parse", "refs/heads/main"])
        ),
        others_before,
        "another branch moved"
    );
    // And the one check of a branch folder's link agrees it is the task's own.
    let git_dir = std::fs::canonicalize(p.f.clone.join(".git")).unwrap();
    assert!(worktree::pointer::verified(&git_dir, &folder, task).is_ok());
}
