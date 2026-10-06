use super::*;

fn asker(cwd: Option<&Path>) -> Asker {
    Asker {
        chat: 4,
        cwd: cwd.map(Path::to_path_buf),
        persona: Some("steward".to_owned()),
        harnessed: true,
        unsandboxed: false,
        config: None,
    }
}

/// A project directory with two workspaces, and no manifest: what [`bound`] reads is the tree.
fn tree() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    for ws in ["alpha", "beta"] {
        std::fs::create_dir_all(root.join("workspaces").join(ws).join("widget")).unwrap();
    }
    std::fs::create_dir_all(root.join("docs")).unwrap();
    (dir, root)
}

#[test]
fn a_chat_at_the_project_root_may_have_any_workspace_written() {
    let (_dir, root) = tree();
    assert_eq!(bound(&root, Some(&root), "alpha"), Ok(()));
    assert_eq!(bound(&root, Some(&root), "beta"), Ok(()));
}

#[test]
fn a_chat_in_a_workspace_may_have_only_that_workspace_written() {
    let (_dir, root) = tree();
    let in_alpha = root.join("workspaces/alpha/widget");
    assert_eq!(bound(&root, Some(&in_alpha), "alpha"), Ok(()));
    let refused = bound(&root, Some(&in_alpha), "beta").expect_err("another workspace");
    assert!(refused.contains("works in workspace 'alpha'"), "{refused}");
    assert!(refused.contains("'beta'"), "{refused}");
}

#[test]
fn a_chat_elsewhere_or_nowhere_has_no_workspace_written() {
    let (_dir, root) = tree();
    let outside = tempfile::tempdir().unwrap();
    for cwd in [root.join("docs"), outside.path().to_path_buf()] {
        let refused = bound(&root, Some(&cwd), "alpha").expect_err("outside the workspaces");
        assert!(
            refused.contains("outside the project's workspaces"),
            "{refused}"
        );
    }
    assert!(bound(&root, None, "alpha").is_err());
}

#[test]
fn a_refused_workspace_is_answered_no_and_nothing_runs() {
    let (_dir, root) = tree();
    let answer = answer(
        &root,
        &asker(Some(&root.join("workspaces/alpha"))),
        &GitAsk {
            chat: 4,
            workspace: "beta".to_owned(),
            work: GitWork::WorktreeAdd {
                repo: "widget".to_owned(),
                piece: "p1".to_owned(),
                branch: None,
            },
        },
        &git::Isolated::default(),
        chrono::Utc::now(),
    );
    assert!(matches!(answer, Answer::No { .. }), "{answer:?}");
    assert!(!root.join("workspaces/beta/pieces").exists());
}

#[test]
fn a_shell_tab_or_a_chat_started_without_the_sandbox_has_no_host_limit() {
    // Neither reads the manifest: a shell is the operator's own, and the opt-out is a person's.
    let (_dir, root) = tree();
    let shell = Asker {
        harnessed: false,
        ..asker(None)
    };
    assert_eq!(hosts(&root, &shell), Ok(None));
    let lifted = Asker {
        unsandboxed: true,
        ..asker(None)
    };
    assert_eq!(hosts(&root, &lifted), Ok(None));
}

#[test]
fn a_sandboxed_chat_reaches_only_its_egress_and_a_project_without_the_sandbox_none_is_set() {
    let (_dir, root) = tree();
    let manifest = crate::names::manifest(&root);
    std::fs::write(
        &manifest,
        "[sandbox]\nmode = \"on\"\negress = [\"model-providers\"]\n",
    )
    .unwrap();
    let hosts_of = hosts(&root, &asker(None)).unwrap().expect("a limit");
    assert!(hosts_of.iter().any(|h| h == "api.anthropic.com"));
    assert!(!hosts_of.iter().any(|h| h == "github.com"), "{hosts_of:?}");

    std::fs::write(&manifest, "").unwrap();
    assert_eq!(hosts(&root, &asker(None)), Ok(None));
}

#[test]
fn lines_that_fit_are_kept_whole_and_ones_that_do_not_are_cut_and_counted() {
    let few = vec![Say::Info("a".to_owned()), Say::Done("b".to_owned())];
    assert_eq!(fitted(few.clone(), SAID_AT_MOST), few);

    let many: Vec<Say> = (0..200)
        .map(|n| Say::Fail(format!("{n}:{}", "x".repeat(5000))))
        .collect();
    let kept = fitted(many, SAID_AT_MOST);
    let json = serde_json::to_vec(&kept).unwrap();
    assert!(json.len() <= SAID_AT_MOST, "{} bytes", json.len());
    let Some(Say::Warn(note)) = kept.last() else {
        panic!("no note of what was cut: {:?}", kept.last());
    };
    let cut: usize = note.split(' ').next().unwrap().parse().unwrap();
    assert_eq!(kept.len() - 1 + cut, 200, "{note}");
    assert!(matches!(&kept[0], Say::Fail(text) if text.starts_with("0:") && text.ends_with(" …")));
}

#[test]
fn every_kind_of_key_that_names_a_program_is_one_and_ordinary_keys_are_not() {
    for key in [
        "filter.lfs.smudge",
        "filter.evil.process",
        "diff.pdf.textconv",
        "diff.x.command",
        "diff.external",
        "merge.ours.driver",
        "core.sshcommand",
        "core.gitproxy",
        "core.alternaterefscommand",
        "include.path",
        "includeif.gitdir:/x/.path",
        "credential.helper",
        "credential.https://github.com.helper",
        "remote.origin.uploadpack",
    ] {
        assert!(names_a_program(key), "{key}");
    }
    for key in [
        "core.bare",
        "remote.origin.url",
        "user.name",
        "diff.renames",
        "merge.conflictstyle",
        "credential.username",
        "branch.main.merge",
    ] {
        assert!(!names_a_program(key), "{key}");
    }
}

/// A repo at `workspaces/alpha/widget` whose `.gitattributes` asks for content filter `evil`
/// on every file, and the path a run of the filter would leave behind.
fn a_repo_asking_for_a_filter(root: &Path) -> (PathBuf, PathBuf) {
    let repo = root.join("workspaces/alpha/widget");
    crate::testgit::run(&repo, &["init", "-q", "-b", "main", "."]);
    commit_asking_for_a_filter(&repo);
    (repo, root.join("filter-ran"))
}

/// A commit in `repo` whose `.gitattributes` asks for content filter `evil` on every file.
fn commit_asking_for_a_filter(repo: &Path) {
    std::fs::write(repo.join(".gitattributes"), "* filter=evil\n").unwrap();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    crate::testgit::run(repo, &["add", "-A"]);
    crate::testgit::run(repo, &["commit", "-q", "-m", "one"]);
}

/// `config` with content filter `evil` defined to leave `ran` behind when it runs. Quoted, so
/// git does not read the `;` as the start of a comment and cut the command short.
fn with_the_filter(config: &Path, ran: &Path) {
    let mut text = std::fs::read_to_string(config).unwrap_or_default();
    text.push_str(&format!(
        "[filter \"evil\"]\n\tsmudge = \"sh -c 'touch {}; cat'\"\n\tclean = cat\n",
        ran.display()
    ));
    std::fs::write(config, text).unwrap();
}

/// What the app answers chat 4, at the project's top, asking for piece `p1` of `repo`.
fn a_worktree_of(root: &Path, repo: &str) -> Answer {
    answer(
        root,
        &asker(Some(root)),
        &GitAsk {
            chat: 4,
            workspace: "alpha".to_owned(),
            work: GitWork::WorktreeAdd {
                repo: repo.to_owned(),
                piece: "p1".to_owned(),
                branch: None,
            },
        },
        &git::Isolated::default(),
        chrono::Utc::now(),
    )
}

#[test]
fn a_bare_repository_a_chat_made_in_its_workspace_is_never_the_one_git_runs_in() {
    // D-1335-8: `init --bare` writes `config` with no `.git` above it, so no denied path
    // covers it, and discovery would take the directory as the repository.
    let (_dir, root) = tree();
    let ran = root.join("filter-ran");
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    crate::testgit::run(
        &src,
        &[
            "init",
            "-q",
            "-b",
            "main",
            &format!("--separate-git-dir={}", root.join("src.gitdir").display()),
            ".",
        ],
    );
    commit_asking_for_a_filter(&src);
    let evil = root.join("workspaces/alpha/evil");
    crate::testgit::run(
        &root,
        &[
            "clone",
            "-q",
            "--bare",
            &src.display().to_string(),
            &evil.display().to_string(),
        ],
    );
    with_the_filter(&evil.join("config"), &ran);

    let answer = a_worktree_of(&root, "evil");

    assert!(matches!(answer, Answer::No { .. }), "{answer:?}");
    assert!(!ran.exists(), "the chat's filter ran outside its sandbox");
    assert!(!root.join("workspaces/alpha/.worktrees/evil").exists());
}

#[test]
fn a_directory_with_no_git_of_its_own_is_never_taken_for_the_project_s_repository() {
    // D-1335-8: discovery would climb from a plain directory to the project's own repository,
    // whose config the check never read.
    let (_dir, root) = tree();
    let ran = root.join("filter-ran");
    crate::testgit::run(&root, &["init", "-q", "-b", "main", "."]);
    commit_asking_for_a_filter(&root);
    with_the_filter(&root.join(".git/config"), &ran);
    std::fs::create_dir_all(root.join("workspaces/alpha/plain")).unwrap();

    let answer = a_worktree_of(&root, "plain");

    assert!(matches!(answer, Answer::No { .. }), "{answer:?}");
    assert!(!ran.exists(), "the project's filter ran for the chat");
    assert!(!root.join("workspaces/alpha/.worktrees/plain").exists());
}

#[test]
fn a_commondir_pointed_at_a_config_the_chat_wrote_is_refused_and_its_filter_never_runs() {
    // F1 of #1335's review: a chat that could write `.git/commondir` points git at a common
    // directory of its own, whose config defines the filter the committed `.gitattributes`
    // asks for. The app must neither run it nor cut the worktree.
    let (_dir, root) = tree();
    let (repo, ran) = a_repo_asking_for_a_filter(&root);
    let theirs = root.join("theirs.git");
    crate::testgit::run(
        &root,
        &[
            "clone",
            "-q",
            "--bare",
            &repo.display().to_string(),
            &theirs.display().to_string(),
        ],
    );
    let mut config = std::fs::read_to_string(theirs.join("config")).unwrap();
    config.push_str(&format!(
        "[filter \"evil\"]\n\tsmudge = \"sh -c 'touch {}; cat'\"\n\tclean = cat\n",
        ran.display()
    ));
    std::fs::write(theirs.join("config"), config).unwrap();
    std::fs::write(
        repo.join(".git/commondir"),
        format!("{}\n", theirs.display()),
    )
    .unwrap();
    // The redirect is live: git now reads the filter from the chat's config.
    assert!(
        crate::testgit::run(&repo, &["config", "--get", "filter.evil.smudge"])
            .out
            .contains("touch"),
        "the commondir did not redirect the config"
    );

    let answer = answer(
        &root,
        &asker(Some(&root)),
        &GitAsk {
            chat: 4,
            workspace: "alpha".to_owned(),
            work: GitWork::WorktreeAdd {
                repo: "widget".to_owned(),
                piece: "p1".to_owned(),
                branch: None,
            },
        },
        &git::Isolated::default(),
        chrono::Utc::now(),
    );

    let Answer::No { why } = answer else {
        panic!("not refused: {answer:?}");
    };
    assert!(why.contains("`filter.evil.smudge`"), "{why}");
    assert!(!ran.exists(), "the chat's filter ran outside its sandbox");
    assert!(!root.join("workspaces/alpha/.worktrees/widget/p1").exists());
}

#[test]
fn a_repos_own_include_or_credential_helper_is_refused_by_name() {
    let (_dir, root) = tree();
    let (repo, _) = a_repo_asking_for_a_filter(&root);
    for (key, value) in [
        ("include.path", "/elsewhere/config"),
        ("credential.helper", "!sh -c 'echo x'"),
    ] {
        crate::testgit::run(&repo, &["config", "--local", key, value]);
        let refused = git::isolated(&git::Isolated::default(), || runs_a_program(&repo))
            .map(|_| ())
            .expect_err("refused");
        assert!(refused.contains(&format!("`{key}`")), "{refused}");
        crate::testgit::run(&repo, &["config", "--local", "--unset", key]);
    }
    // The helper purlis's own git policy writes into every clone it makes is not refused.
    crate::testgit::run(
        &repo,
        &[
            "config",
            "--local",
            "credential.helper",
            "!gh auth git-credential",
        ],
    );
    assert_eq!(
        git::isolated(&git::Isolated::default(), || runs_a_program(&repo)),
        Ok(std::fs::canonicalize(repo.join(".git")).unwrap())
    );
}
