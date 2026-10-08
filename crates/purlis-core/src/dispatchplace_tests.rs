use super::*;

/// A project directory with workspaces `alpha` and `beta` and no manifest: what these read is
/// the tree.
fn project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    for ws in ["alpha", "beta"] {
        std::fs::create_dir_all(root.join("workspaces").join(ws)).unwrap();
    }
    (dir, root)
}

// ----- what `--in` says -------------------------------------------------------------------

#[test]
fn a_dispatch_names_a_worktree_or_a_workspace_and_nothing_else_is_a_place() {
    assert_eq!(asked(None), Ok(None));
    assert_eq!(asked(Some("  ")), Ok(None));
    assert_eq!(asked(Some("worktree")), Ok(Some(Where::Worktree)));
    assert_eq!(
        asked(Some(" workspace:beta ")),
        Ok(Some(Where::Workspace("beta".to_owned())))
    );
    // A bare name, a folder, a branch: none is a word `--in` has.
    for word in [
        "beta",
        "worktree:my-branch",
        "worktree=../x",
        "/tmp/x",
        "branch:main",
    ] {
        assert_eq!(asked(Some(word)), Err(Refused::Word(word.to_owned())));
    }
    assert_eq!(
        Refused::Word("beta".to_owned()).say(),
        "--in is `worktree` or `workspace:<name>`, not 'beta'. Leave it out and the new chat \
         works in this chat's folder."
    );
}

#[test]
fn a_workspace_name_with_path_characters_names_no_workspace() {
    for name in [
        "../beta", "beta/..", "a/b", "..", ".", "", "/etc", "beta\0", "-x",
    ] {
        let word = format!("workspace:{name}");
        assert_eq!(
            asked(Some(&word)),
            if name.is_empty() {
                // `workspace:` with nothing after it is still not a name.
                Err(Refused::WorkspaceName(String::new()))
            } else {
                Err(Refused::WorkspaceName(name.to_owned()))
            },
            "{word:?}"
        );
    }
    assert_eq!(
        Refused::WorkspaceName("../beta".to_owned()).say(),
        "'../beta' cannot name a workspace, so no chat is started there. A workspace is named \
         by its folder under `workspaces/`, and by nothing else."
    );
}

// ----- another workspace ------------------------------------------------------------------

#[test]
fn a_named_workspace_is_its_folder_and_one_the_project_lacks_is_refused_in_a_sentence() {
    let (_dir, root) = project();

    assert_eq!(
        workspace_folder(&root, "beta"),
        Ok(("beta".to_owned(), root.join("workspaces/beta")))
    );
    let refused = workspace_folder(&root, "gamma").expect_err("no such workspace");
    assert_eq!(refused, Refused::NoWorkspace("gamma".to_owned()));
    assert_eq!(
        refused.say(),
        "this project has no workspace 'gamma'. List the workspaces with `purlis workspace \
         list`, then dispatch into one of them."
    );
    // A file where a workspace's folder would be is no workspace either.
    std::fs::write(root.join("workspaces/notes"), "x").unwrap();
    assert_eq!(
        workspace_folder(&root, "notes"),
        Err(Refused::NoWorkspace("notes".to_owned()))
    );
}

#[test]
fn a_workspace_asked_for_in_another_case_is_the_folder_s_own_name_or_no_workspace() {
    // #1453 review, M1. On a volume that folds case (macOS's default), `BETA` opens the folder
    // `beta`. The limits, the record, the badge and the filing are all looked up by name, so
    // the name used is the folder's own. Where the volume does not fold case there is no such
    // workspace.
    let (_dir, root) = project();
    let folds = root.join("workspaces/BETA").is_dir();

    let asked = ground(
        &root,
        Some(Where::Workspace("BETA".to_owned())),
        None,
        Some(&root),
    );

    if folds {
        let there = asked.expect("the folder is there under its own name");
        assert_eq!(
            there,
            Ground::Workspace {
                name: "beta".to_owned(),
                folder: root.join("workspaces/beta"),
            }
        );
        // What the decision reads the destination's limits by.
        assert_eq!(there.workspace(), Some("beta"));
        assert_eq!(
            said_to_the_asker(&there, None, false),
            "in workspace 'beta', with that workspace's todos, memory and session records"
        );
    } else {
        assert_eq!(asked, Err(Refused::NoWorkspace("BETA".to_owned())));
    }
    // A name no folder has in any case is no workspace on either.
    assert_eq!(
        workspace_folder(&root, "GAMMA"),
        Err(Refused::NoWorkspace("GAMMA".to_owned()))
    );
}

#[cfg(unix)]
#[test]
fn a_workspace_that_is_a_link_out_of_the_project_starts_no_chat() {
    let (_dir, root) = project();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(outside.path().join("elsewhere")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("elsewhere"),
        root.join("workspaces/linked"),
    )
    .unwrap();
    // And a link to a folder inside the project is still a link on the way.
    std::os::unix::fs::symlink(root.join("workspaces/beta"), root.join("workspaces/alias"))
        .unwrap();

    for name in ["linked", "alias"] {
        let refused = workspace_folder(&root, name).expect_err("reached through a link");
        assert_eq!(refused, Refused::WorkspaceElsewhere(name.to_owned()));
        assert_eq!(
            ground(
                &root,
                Some(Where::Workspace(name.to_owned())),
                None,
                Some(&root)
            ),
            Err(Refused::WorkspaceElsewhere(name.to_owned()))
        );
    }
    assert_eq!(
        Refused::WorkspaceElsewhere("linked".to_owned()).say(),
        "workspace 'linked' is reached through a link, or does not land inside this project, \
         so no chat is started there."
    );
}

#[test]
fn a_dispatch_into_a_workspace_is_grounded_there_and_one_that_names_none_stays_with_its_asker() {
    let (_dir, root) = project();
    let in_alpha = root.join("workspaces/alpha");

    let there = ground(
        &root,
        Some(Where::Workspace("beta".to_owned())),
        None,
        Some(&in_alpha),
    )
    .unwrap();
    assert_eq!(
        there,
        Ground::Workspace {
            name: "beta".to_owned(),
            folder: root.join("workspaces/beta"),
        }
    );
    assert_eq!(there.workspace(), Some("beta"));
    assert_eq!(
        said_to_the_asker(&there, None, true),
        "in workspace 'beta', with that workspace's todos, memory and session records"
    );

    let here = ground(&root, None, None, Some(&in_alpha)).unwrap();
    assert_eq!(here, Ground::Asker { fell_back: false });
    assert_eq!(here.workspace(), None);
    assert_eq!(
        said_to_the_asker(&here, None, false),
        "in this chat's folder"
    );
}

// ----- a worktree: where it is cut, and what it is called ------------------------------

#[test]
fn a_worktree_asked_for_where_the_asking_chat_works_in_no_repo_is_refused_in_a_sentence() {
    let (_dir, root) = project();
    let outside = tempfile::tempdir().unwrap();
    // A folder under a workspace that holds no `.git` is not a repo's clone.
    std::fs::create_dir_all(root.join("workspaces/alpha/notes")).unwrap();

    for cwd in [
        Some(root.clone()),
        Some(root.join("workspaces/alpha")),
        Some(root.join("workspaces/alpha/notes")),
        Some(outside.path().to_path_buf()),
        None,
    ] {
        assert_eq!(
            ground(&root, Some(Where::Worktree), None, cwd.as_deref()),
            Err(Refused::NotInARepo),
            "{cwd:?}"
        );
    }
    assert_eq!(
        Refused::NotInARepo.say(),
        "a worktree is cut from the repo the asking chat works in, and this chat works in no \
         repo's clone: this folder is not a git repository purlis cuts worktrees of. Dispatch \
         it from a chat that works in a repo, or leave out `--in worktree` and the new chat \
         works in this chat's folder."
    );
}

#[test]
fn a_task_name_with_path_characters_names_one_folder_beside_its_siblings() {
    let id = "01K6Z3V9QJ8M4T2W7XB5RC0DEF";
    for (task, piece) in [
        ("check the queue", "check-the-queue-b5rc0def"),
        ("../../etc/passwd", "etc-passwd-b5rc0def"),
        ("a/b\\c", "a-b-c-b5rc0def"),
        ("..", "task-b5rc0def"),
        ("HEAD", "task-b5rc0def"),
        ("--upload-pack=x", "upload-pack-x-b5rc0def"),
        ("fix.lock", "fix-b5rc0def"),
        ("日本語", "task-b5rc0def"),
        ("nul", "nul-chat-b5rc0def"),
        // A name whose stem before its first dot is a device on Windows is still one after the
        // slug's own repair, so the folder is named for the dispatch alone: the task is not
        // refused for what it was called.
        ("aux.rs fix", "task-b5rc0def"),
        ("nul.thing", "task-b5rc0def"),
        ("COM1.log tidy", "task-b5rc0def"),
    ] {
        let named = piece_name(task, id);
        assert_eq!(named, piece, "{task:?}");
        // What the worktree module joins into a path and hands git as a branch: one entry,
        // never an option, never a name git keeps for itself.
        assert!(name::piece_name_ok(&named), "{named}");
        assert!(crate::contain::mintable(&named).is_ok(), "{named}");
        assert!(name::branch_name_ok(&named).is_ok(), "{named}");
        assert!(!name::reserved(&named), "{named}");
    }
}

#[test]
fn two_worktree_tasks_called_the_same_never_share_a_name() {
    let first = dispatchrecord::mint();
    let second = dispatchrecord::mint();

    let one = piece_name("check the queue", &first);
    let two = piece_name("check the queue", &second);

    assert_ne!(one, two);
    assert!(one.starts_with("check-the-queue-"), "{one}");
    // The longest task name there is still names a folder a branch name can be.
    let long = piece_name(&"x".repeat(crate::reopen::MOST_LABEL), &first);
    assert!(long.len() <= 49, "{long}");
    assert!(name::piece_name_ok(&long));
}

#[test]
fn what_the_chats_are_told_names_the_branch_and_says_nothing_merges() {
    assert_eq!(
        told_the_chat("api", "check-the-queue-b5rc0def", false),
        "you work in a worktree of api that purlis cut for this task, on the branch \
         `check-the-queue-b5rc0def`, which is yours alone. Commit your work on it. Nothing is \
         merged for you: your report names the branch, and merging it is the asking chat's or \
         the person's decision"
    );
    let cut = Cut {
        workspace: "alpha".to_owned(),
        repo: "api".to_owned(),
        piece: "check-the-queue-b5rc0def".to_owned(),
        path: PathBuf::from("/p/workspaces/alpha/.worktrees/api/check-the-queue-b5rc0def"),
        branch: "check-the-queue-b5rc0def".to_owned(),
        base: worktree::Base::Branch("main".to_owned()),
        notes: Vec::new(),
    };
    let ground = Ground::Worktree(Repo {
        workspace: "alpha".to_owned(),
        repo: "api".to_owned(),
    });
    assert_eq!(
        said_to_the_asker(&ground, Some(&cut), false),
        "in a worktree of its own, on the branch `check-the-queue-b5rc0def` in api, cut from \
         main. Nothing is merged for it: its report names the branch, and merging is yours or \
         the person's decision"
    );
    // #1453 review, M3, and #1055. A sandboxed chat writes its own folder and nothing of its
    // repo's `.git`, where a worktree's git data is: it is told the command that commits for
    // it, never only to commit.
    let told = told_the_chat("api", "check-the-queue-b5rc0def", true);
    assert_eq!(
        told,
        "you work in a worktree of api that purlis cut for this task, on the branch \
         `check-the-queue-b5rc0def`, which is yours alone. This chat is sandboxed, and a \
         worktree's git data is outside the folder it may write, so `git add` and `git commit` \
         are refused here: commit your work with `purlis worktree commit -m \"<message>\" \
         --all` (or paths in place of `--all`; a new file must be named). It only commits. \
         Nothing is merged for you: your report names the branch, and merging it is the asking \
         chat's or the person's decision"
    );
    assert!(!told.contains("Commit your work"), "{told}");
    assert_eq!(
        said_to_the_asker(&ground, Some(&cut), true),
        "in a worktree of its own, on the branch `check-the-queue-b5rc0def` in api, cut from \
         main. It is sandboxed, so it commits there with `purlis worktree commit`, which the \
         app runs for it. Nothing is merged for it: its report names the branch, and merging \
         is yours or the person's decision"
    );
    assert_eq!(
        fell_back_note("web"),
        "persona 'web' works in a worktree of its own by default, and this chat works in no \
         repo to cut one from, so the new chat works in this chat's folder"
    );
}

#[test]
fn a_dispatched_chat_starts_sandboxed_where_the_project_s_sandbox_is_on() {
    // What decides whether a worktree task is told it can commit (#1453 review, M3).
    let (_dir, root) = project();
    assert!(
        !starts_sandboxed(&root),
        "a project with no manifest has none"
    );
    std::fs::write(
        crate::names::manifest(&root),
        "[persona]\ndefault = \"x\"\n",
    )
    .unwrap();
    assert!(!starts_sandboxed(&root), "nor one that does not turn it on");
    std::fs::write(
        crate::names::manifest(&root),
        "[sandbox]\nmode = \"on\"\negress = [\"model-providers\"]\n",
    )
    .unwrap();
    assert!(starts_sandboxed(&root));
}

#[test]
fn the_person_reads_a_refused_place_in_the_window_s_words() {
    // "Ask <persona>…" offers the same places. What the window says of one it could not use
    // names no flag of a command, and says branch, never worktree (ADR 0072 section 4).
    for (refused, said) in [
        (
            Refused::NotInARepo,
            "That chat works in no repo, so there is none to cut a branch of its own from. \
             Choose that chat's folder, or ask from a chat that works in a repo.",
        ),
        (
            Refused::NoWorkspace("gamma".to_owned()),
            "This project has no workspace 'gamma', so no chat was started there.",
        ),
        (
            Refused::WorkspaceElsewhere("linked".to_owned()),
            "Workspace 'linked' is reached through a link, or does not land inside this \
             project, so no chat was started there.",
        ),
        (
            Refused::Cut("branch 'x' already exists in api.\nPick another name.".to_owned()),
            "purlis could not cut a branch for this task: branch 'x' already exists in api. \
             Pick another name.",
        ),
    ] {
        let window = refused.in_window();
        assert_eq!(window, said);
        for leak in ["--in", "worktree", "purlis workspace list", "`"] {
            assert!(!window.contains(leak), "{leak}: {window}");
        }
    }
}

#[test]
fn every_refusal_is_one_line() {
    for refused in [
        Refused::Word("x".to_owned()),
        Refused::WorkspaceName("x".to_owned()),
        Refused::NoWorkspace("x".to_owned()),
        Refused::WorkspaceElsewhere("x".to_owned()),
        Refused::NotInARepo,
        Refused::Cut("git refused".to_owned()),
        Refused::NobodyToAsk {
            workspace: "beta".to_owned(),
            asking: Some("steward".to_owned()),
            target: Some("steward".to_owned()),
        },
        Refused::NobodyToAsk {
            workspace: "beta".to_owned(),
            asking: None,
            target: None,
        },
    ] {
        let said = refused.say();
        assert!(!said.contains('\n'), "{said}");
        assert!(!said.is_empty());
    }
    // What git said on several lines, with or without a full stop, is one sentence.
    for why in [
        "git refused:\nfatal: bad  object\n",
        "git refused: fatal: bad object.",
    ] {
        assert_eq!(
            Refused::Cut(why.to_owned()).say(),
            "purlis could not cut a worktree for this task: git refused: fatal: bad object."
        );
    }
}

// ----- a record's worktree ----------------------------------------------------------------

/// The id every record of these tests has: its worktree's name ends `-b5rc0def`.
const ID: &str = "01K6Z3V9QJ8M4T2W7XB5RC0DEF";

fn a_record(worktree: Option<dispatchrecord::Worktree>, workspace: Option<&str>) -> Record {
    let chat = |n: u32, name: &str| dispatchrecord::ChatRef {
        chat: n,
        id: None,
        name: name.to_owned(),
        persona: None,
    };
    Record {
        v: dispatchrecord::VERSION,
        id: ID.to_owned(),
        mode: dispatchrecord::Mode::Task,
        asker: dispatchrecord::Asker {
            chat: chat(3, "steward 3"),
            ..Default::default()
        },
        persona: None,
        worker: dispatchrecord::Worker {
            chat: chat(7, "check the queue"),
            ..Default::default()
        },
        task: Some("check the queue".to_owned()),
        place: dispatchrecord::Place {
            workspace: workspace.map(str::to_owned),
            folder: None,
            worktree,
        },
        brief: "b".to_owned(),
        report_owed: true,
        started: "2026-10-07T12:00:00Z".to_owned(),
        ended: Some("2026-10-07T12:05:00Z".to_owned()),
        report: None,
        needed_you: 0,
        messages: 0,
        usage: None,
        conversation: None,
        cleared: false,
        ended_by: None,
        kept_open: false,
    }
}

fn a_tree(piece: &str, removed: Option<Removed>) -> dispatchrecord::Worktree {
    dispatchrecord::Worktree {
        repo: "api".to_owned(),
        piece: piece.to_owned(),
        branch: Some(piece.to_owned()),
        removed,
    }
}

#[test]
fn a_dispatch_s_worktree_is_listed_as_kept_while_its_folder_is_there() {
    let (_dir, root) = project();
    let kept = a_record(Some(a_tree("check-b5rc0def", None)), Some("alpha"));
    std::fs::create_dir_all(root.join("workspaces/alpha/.worktrees/api/check-b5rc0def")).unwrap();

    assert_eq!(standing(&root, &kept), Some(Standing::Kept));
    // A dispatch that had no worktree has no standing to list.
    assert_eq!(standing(&root, &a_record(None, Some("alpha"))), None);
    // Its folder gone by other hands.
    let gone = a_record(Some(a_tree("other-b5rc0def", None)), Some("alpha"));
    assert_eq!(standing(&root, &gone), Some(Standing::Gone));
    assert_eq!(Standing::Kept.word(), "kept");
    // And what purlis did to it is the record's word, whatever is on the disk now.
    for (how, stands) in [
        (Removed::Merged, Standing::Merged),
        (Removed::MergedBranchKept, Standing::MergedBranchKept),
        (Removed::Discarded, Standing::Discarded),
    ] {
        let removed = a_record(Some(a_tree("check-b5rc0def", Some(how))), Some("alpha"));
        assert_eq!(standing(&root, &removed), Some(stands));
    }
}

#[test]
fn a_record_whose_worktree_names_climb_is_no_tree_purlis_acts_on() {
    let (_dir, root) = project();
    for (workspace, repo, piece) in [
        (Some("../alpha"), "api", "p"),
        (Some("alpha"), "../api", "p"),
        (Some("alpha"), "api", "../../../x"),
        (Some("alpha"), "api", "-rf"),
        (None, "api", "p"),
    ] {
        let record = a_record(
            Some(dispatchrecord::Worktree {
                repo: repo.to_owned(),
                piece: piece.to_owned(),
                branch: Some("b".to_owned()),
                removed: None,
            }),
            workspace,
        );
        assert_eq!(Tree::of(&record), None, "{workspace:?} {repo} {piece}");
        assert_eq!(standing(&root, &record), Some(Standing::Gone));
        // Nothing is run for it, so nothing is removed and nothing is marked.
        assert_eq!(
            tidy_recorded(&root, &record, &git::Isolated::default()),
            Tidied::Kept
        );
    }
}

#[test]
fn a_record_names_only_the_worktree_purlis_cut_for_that_dispatch() {
    // #1453 review, fold-in 4. A record is a file on the disk. One that names another branch
    // folder of the clone (a writing chat's, one cut by hand) is no tree purlis looks at for a
    // merged branch or offers to discard: the folder's name ends with the end of the record's
    // own id, and its branch is that name.
    let (_dir, root) = project();
    for piece in ["check-b5rc0def", "chat-1", "my-feature", "check-00000000"] {
        std::fs::create_dir_all(root.join("workspaces/alpha/.worktrees/api").join(piece)).unwrap();
    }

    let its_own = a_record(Some(a_tree("check-b5rc0def", None)), Some("alpha"));
    assert!(Tree::of(&its_own).is_some());
    assert_eq!(standing(&root, &its_own), Some(Standing::Kept));

    for piece in ["chat-1", "my-feature", "check-00000000"] {
        let another = a_record(Some(a_tree(piece, None)), Some("alpha"));
        assert_eq!(Tree::of(&another), None, "{piece}");
        // Listed as gone: no Discard is offered for it, and nothing is run in it.
        assert_eq!(standing(&root, &another), Some(Standing::Gone), "{piece}");
        assert_eq!(
            tidy_recorded(&root, &another, &git::Isolated::default()),
            Tidied::Kept
        );
    }
    // Nor a record whose folder is its own and whose branch is another's.
    let other_branch = a_record(
        Some(dispatchrecord::Worktree {
            branch: Some("main".to_owned()),
            ..a_tree("check-b5rc0def", None)
        }),
        Some("alpha"),
    );
    assert_eq!(Tree::of(&other_branch), None);
}

#[test]
fn a_chat_started_in_another_workspace_is_told_where_it_works_and_where_its_asker_does() {
    assert_eq!(
        told_of_its_workspace("beta", Some("alpha")),
        "you work in workspace 'beta': its folder, its todos, its memory and its session \
         records are the ones you use. The chat that asked works in workspace 'alpha', and \
         your report goes to it there"
    );
    assert_eq!(
        told_of_its_workspace("beta", None),
        "you work in workspace 'beta': its folder, its todos, its memory and its session \
         records are the ones you use. The chat that asked works in the project's root, and \
         your report goes to it there"
    );
}

#[test]
fn a_chat_nobody_is_at_crosses_into_another_workspace_only_under_a_standing_grant() {
    // D-1453-16. Not its own persona's rule, and not a grant made for one chat.
    use crate::dispatchgrant::{ChatPair, InForce, Pair};
    let pair = |asking: &str, target: &str| Pair {
        asking: asking.to_owned(),
        target: target.to_owned(),
    };
    let none = InForce::default();
    let refused = nobody_to_ask(Some("steward"), Some("steward"), &none, "beta")
        .expect("its own persona's rule does not carry it across");
    // No grant is ever kept for a persona's dispatch to itself, so the sentence names none
    // to go and make: the person starts it.
    assert_eq!(
        refused.say(),
        "this chat runs with its harness's permission prompts off, so nobody is here to \
         answer for it, and such a chat starts no chat as its own persona in another workspace \
         ('beta'): a chat's own persona needs no grant, so none can stand for it, and with \
         nobody here that rule does not carry a chat into another workspace. The person can \
         start it from this chat's tab. Leave out `--in` and the new chat works in this \
         chat's folder."
    );
    // To another persona it names the pair, and where a grant for it is made.
    assert_eq!(
        nobody_to_ask(Some("steward"), Some("devops"), &none, "beta")
            .expect("no grant stands")
            .say(),
        "this chat runs with its harness's permission prompts off, so nobody is here to \
         answer for it, and such a chat starts a chat in another workspace ('beta') only under \
         a grant that already stands: no grant that already stands lets steward chats dispatch \
         to devops. Only a person makes one, for themselves on this machine or for this \
         project; Settings › Project › Dispatch lists the grants that stand. Until then, leave \
         out `--in` and the new chat works in this chat's folder."
    );
    // A grant made for one chat does not count.
    let for_one_chat = InForce {
        chat: vec![ChatPair {
            asking: Some("steward".to_owned()),
            target: "devops".to_owned(),
        }],
        ..InForce::default()
    };
    assert!(nobody_to_ask(Some("steward"), Some("devops"), &for_one_chat, "beta").is_some());
    // The person's on this machine does, and so does the project's.
    for standing in [
        InForce {
            you: vec![pair("steward", "devops"), pair("steward", "steward")],
            ..InForce::default()
        },
        InForce {
            project: vec![pair("steward", "devops"), pair("steward", "steward")],
            ..InForce::default()
        },
    ] {
        assert_eq!(
            nobody_to_ask(Some("steward"), Some("devops"), &standing, "beta"),
            None
        );
        // Never its own persona, whatever a list holds: no such grant is ever kept.
        assert!(nobody_to_ask(Some("steward"), Some("steward"), &standing, "beta").is_some());
        // And only the pair it names.
        assert!(nobody_to_ask(Some("devops"), Some("steward"), &standing, "beta").is_some());
    }
    // "Any persona" does not count here (#1503): with nobody to see it, a chat crosses into
    // another workspace as another persona only under a grant that names the two.
    for any in [
        InForce {
            you_any: vec!["steward".to_owned()],
            ..InForce::default()
        },
        InForce {
            project_any: vec!["steward".to_owned()],
            ..InForce::default()
        },
    ] {
        assert!(
            nobody_to_ask(Some("steward"), Some("devops"), &any, "beta").is_some(),
            "{any:?}"
        );
        // Beside it, a grant that names the pair still carries it across.
        let named = InForce {
            you: vec![pair("steward", "devops")],
            ..any
        };
        assert_eq!(
            nobody_to_ask(Some("steward"), Some("devops"), &named, "beta"),
            None
        );
        assert!(nobody_to_ask(Some("steward"), Some("qa"), &named, "beta").is_some());
    }
    // A chat on no persona has no pair a standing grant could name.
    let said = nobody_to_ask(None, None, &none, "beta").unwrap().say();
    assert!(
        said.contains("it runs as no persona, which no standing grant covers"),
        "{said}"
    );
}

#[test]
fn what_purlis_itself_hides_in_a_folder_is_not_counted_as_the_task_s() {
    // #1453 review, M4 and fold-in 8: the merged look keeps a folder that holds an ignored
    // path of the task's own, and the Discard list names only those. purlis's own layer, which
    // it wrote and hides, is neither.
    let hidden: std::collections::BTreeSet<String> = [
        ".claude/settings.json",
        ".claude/agents/steward.md",
        "AGENTS.md",
        ".purlis-generated.*.tmp",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for own in [
        ".claude/settings.json",
        ".claude/agents/steward.md",
        "AGENTS.md",
        ".purlis-generated.4242.0a1b2c3d4e5f.tmp",
        ".claude/.purlis-generated.4242.0a1b2c3d4e5f.tmp",
    ] {
        assert!(crate::guest::is_purlis_own(&hidden, own), "{own}");
    }
    for the_tasks in [
        "target/",
        ".env",
        "results/out.json",
        ".claude/settings.local.json",
        "docs/AGENTS.md",
        "coverage.sqlite",
        ".purlis-generated.txt",
    ] {
        assert!(
            !crate::guest::is_purlis_own(&hidden, the_tasks),
            "{the_tasks}"
        );
    }
    // With no block of purlis's own, nothing is its.
    let nothing = std::collections::BTreeSet::new();
    assert!(!crate::guest::is_purlis_own(&nothing, "AGENTS.md"));
    assert!(!crate::guest::is_purlis_own(
        &nothing,
        ".purlis-generated.1.a.tmp"
    ));
}

#[test]
fn a_discard_the_broker_will_not_run_is_said_in_the_window_s_own_words_with_a_way_out() {
    // #1453 review, fold-in 7: the broker's sentence names a command and a path, for a chat
    // at a command line. The person at the window is told what stands in the way and what
    // they can do there.
    let said = NotDone::Repo(
        "/Users/x/p/workspaces/alpha/api sets `filter.x.smudge` (file:.git/config), which \
         names a program git would run outside the chat's sandbox, so the app will not run \
         git there for the chat. Run the command in your own terminal, or remove the key"
            .to_owned(),
    )
    .in_window("api");
    assert_eq!(
        said,
        "purlis will not run git in api for this: the repo's own git settings name a program, \
         which git would run outside any sandbox. Remove the folder from its branch's row in \
         the explorer instead, or take that setting out of the repo."
    );
    for leak in ["/Users/", "terminal", "worktree", "command"] {
        assert!(!said.contains(leak), "{leak}: {said}");
    }
    assert_eq!(
        NotDone::Git("git said no.".to_owned()).in_window("api"),
        "git said no."
    );
}

#[test]
fn a_running_dispatch_s_worktree_is_never_looked_at() {
    let (_dir, root) = project();
    let mut running = a_record(Some(a_tree("check-b5rc0def", None)), Some("alpha"));
    running.ended = None;
    std::fs::create_dir_all(root.join("workspaces/alpha/.worktrees/api/check-b5rc0def")).unwrap();

    assert_eq!(
        tidy_recorded(&root, &running, &git::Isolated::default()),
        Tidied::Kept
    );
    assert!(
        root.join("workspaces/alpha/.worktrees/api/check-b5rc0def")
            .is_dir()
    );
}
