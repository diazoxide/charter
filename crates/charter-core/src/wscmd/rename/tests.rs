//! A real plane in a temporary directory — a git repository, LIVE workspace, a clone with a
//! linked worktree and a Claude Code worktree nested inside it, the pointers, the app's record
//! and this machine's pins — renamed, and every one of them checked.

use super::*;
use crate::reopen::{Chat, HandedFrom, Owed, Record, View};

struct Plane {
    _dir: tempfile::TempDir,
    root: PathBuf,
    config: PathBuf,
}

fn git(at: &Path, argv: &[&str]) -> String {
    let run = crate::testgit::run(at, argv);
    assert!(run.ok(), "git {argv:?} in {}: {}", at.display(), run.err);
    run.out
}

fn repo(at: &Path) {
    std::fs::create_dir_all(at).unwrap();
    git(at, &["init", "-q", "-b", "main"]);
    git(at, &["config", "user.email", "t@example.com"]);
    git(at, &["config", "user.name", "T"]);
    std::fs::write(at.join("README.md"), "hi\n").unwrap();
    git(at, &["add", "-A"]);
    git(at, &["commit", "-qm", "first"]);
}

fn chat(cwd: &Path, name: &str) -> Chat {
    Chat {
        program: "claude".into(),
        args: Vec::new(),
        cwd: Some(cwd.to_path_buf()),
        name: name.into(),
        resume: None,
        active: false,
        profile: None,
        persona: Some("steward".into()),
        show_footer: false,
        pinned: false,
        number: None,
        label: None,
        from: None,
    }
}

fn view(workspace: &str, view: &str, key: &str, title: &str) -> View {
    View {
        from: None,
        view: view.into(),
        key: key.into(),
        title: title.into(),
        workspace: Some(workspace.into()),
        at: 0,
        active: false,
        pinned: false,
    }
}

/// Everything that can name workspace `alpha`, made the way charter makes each.
fn a_plane() -> Plane {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap().join("plane");
    let config = root.parent().unwrap().join("config");
    std::fs::create_dir_all(root.join("workspaces")).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(root.join("charter.toml"), "[plane]\nmode = \"commit\"\n").unwrap();
    std::fs::write(
        root.join(".gitignore"),
        "/.charter/\n/workspaces/*/*\n!/workspaces/.gitkeep\n",
    )
    .unwrap();
    std::fs::write(root.join("workspaces/.gitkeep"), "").unwrap();
    for name in ["alpha", "other"] {
        let plane = crate::workspaces::Plane::open(&root);
        let ws = plane.workspace(name).unwrap();
        ws.scaffold_charter().unwrap();
        ws.scaffold_memory().unwrap();
        ws.add_todo("finish it", chrono::NaiveDateTime::default())
            .unwrap();
        ws.scaffold_manifest(chrono::Utc::now(), "t").unwrap();
    }
    std::fs::write(root.join("workspaces/.default"), "alpha\n").unwrap();
    crate::wscmd::set_live(&root, "alpha", true).unwrap();

    // A clone, a piece cut from it, and a worktree Claude Code keeps inside it.
    let svc = root.join("workspaces/alpha/svc");
    repo(&svc);
    git(
        &svc,
        &["worktree", "add", "-q", "-b", "p1", "../.worktrees/svc/p1"],
    );
    git(
        &svc,
        &["worktree", "add", "-q", "-b", "cc", ".claude/worktrees/cc"],
    );
    // Work nothing else has: an uncommitted edit in the clone and one in the piece.
    std::fs::write(svc.join("README.md"), "mine\n").unwrap();
    std::fs::write(
        root.join("workspaces/alpha/.worktrees/svc/p1/README.md"),
        "piece\n",
    )
    .unwrap();

    // The plane itself, committed with alpha LIVE.
    repo_plane(&root);

    // The pointers.
    let state = root.join(".charter");
    for (rel, text) in [
        ("sessions/7.workspace", "alpha\n"),
        ("sessions/7.lock", "alpha\n"),
        ("sessions/8.workspace", "other\n"),
        ("terminals/ttys001.workspace", "alpha\n"),
        ("workspace-tab-order", "other\nalpha\n"),
        ("workspace-arrivals/alpha", ""),
        ("handbacks/workspace-alpha/1.json", "{}"),
    ] {
        let path = state.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    // The app's record: a chat in the clone, one elsewhere handed off from alpha, and two view
    // tabs — one on alpha's strip, and alpha's own settings.
    let mut elsewhere = chat(&root.join("workspaces/other"), "2");
    elsewhere.from = Some(HandedFrom {
        chat: 1,
        name: "steward 1".into(),
        workspace: "alpha".into(),
        report: Owed::Due,
    });
    crate::reopen::write(
        &root,
        &Record {
            chats: vec![chat(&svc, "1"), elsewhere],
            views: vec![
                view("alpha", "persona", "steward", "steward"),
                view(
                    "alpha",
                    "workspace-settings",
                    "alpha",
                    "Workspace settings · alpha",
                ),
            ],
            dealt: 2,
            relaunch_after_update: false,
        },
    )
    .unwrap();

    // This machine remembers the plane and has alpha pinned.
    crate::machine::update(&config, |store| {
        store.remember(&root, 1);
        store.approve(&root, 1, crate::machine::Contribution::of(&root));
        store.pin_workspace(&root, "other", true).unwrap();
        store.pin_workspace(&root, "alpha", true).unwrap();
    })
    .unwrap();

    Plane {
        _dir: dir,
        root,
        config,
    }
}

fn repo_plane(root: &Path) {
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "user.name", "T"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "plane"]);
}

fn run_with(plane: &Plane, old: &str, new: &str, running: &[String]) -> (u8, Vec<String>) {
    let mut said = Vec::new();
    let code = rename(
        &Request {
            root: &plane.root,
            old,
            new,
            running,
            config_root: Some(&plane.config),
        },
        &mut |line: Say| said.push(line.to_string()),
    );
    (code, said)
}

fn run(plane: &Plane, old: &str, new: &str) -> (u8, Vec<String>) {
    run_with(plane, old, new, &[])
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|why| panic!("{}: {why}", path.display()))
}

/// Every record that named alpha now names beta, and every tree still works.
fn everything_follows(plane: &Plane) {
    let root = &plane.root;
    let ws = root.join("workspaces/beta");
    assert!(ws.is_dir());
    assert!(!root.join("workspaces/alpha").exists());

    // git: every worktree lists under its new path, none prunable, and each one works.
    let listed = git(&ws.join("svc"), &["worktree", "list", "--porcelain"]);
    let rows = crate::worktree::porcelain::parse(&listed);
    assert_eq!(rows.len(), 3, "{listed}");
    for row in &rows {
        assert!(row.path.starts_with(&ws), "{listed}");
        assert_eq!(row.prunable, None, "{listed}");
        git(&row.path, &["status", "--porcelain"]);
    }
    // The work that was not committed anywhere came along.
    assert_eq!(read(&ws.join("svc/README.md")), "mine\n");
    assert_eq!(read(&ws.join(".worktrees/svc/p1/README.md")), "piece\n");
    // git's own check of every link: nothing left to repair or prune.
    let pruned = git(&ws.join("svc"), &["worktree", "prune", "--dry-run", "-v"]);
    assert_eq!(pruned.trim(), "", "{pruned}");
    assert_eq!(
        git(
            &ws.join(".worktrees/svc/p1"),
            &["rev-parse", "--abbrev-ref", "HEAD"]
        )
        .trim(),
        "p1"
    );
    assert_eq!(
        git(
            &ws.join("svc/.claude/worktrees/cc"),
            &["rev-parse", "--abbrev-ref", "HEAD"]
        )
        .trim(),
        "cc"
    );

    // The manifest, still charter's.
    let workspace = crate::workspaces::Plane::open(root)
        .workspace("beta")
        .unwrap();
    let (doc, owner) = workspace.manifest();
    assert_eq!(doc.unwrap()["name"], "beta");
    assert_eq!(owner, crate::manifest::Ownership::Charter);
    // The headings charter wrote.
    assert!(read(&ws.join("workspace.md")).starts_with("# beta\n"));
    assert!(read(&ws.join("memory/MEMORY.md")).starts_with("# beta — task memory\n"));
    assert!(read(&ws.join("todos/MEMORY.md")).starts_with("# Todos — workspace `beta`\n"));
    assert_eq!(workspace.todos().unwrap().len(), 1);

    // LIVE.
    let live = crate::wscmd::live_workspaces(root);
    assert!(live.contains("beta") && !live.contains("alpha"), "{live:?}");

    // The pointers.
    let state = root.join(".charter");
    assert_eq!(read(&root.join("workspaces/.default")), "beta\n");
    assert_eq!(read(&state.join("sessions/7.workspace")), "beta\n");
    assert_eq!(read(&state.join("sessions/7.lock")), "beta\n");
    assert_eq!(read(&state.join("sessions/8.workspace")), "other\n");
    assert_eq!(read(&state.join("terminals/ttys001.workspace")), "beta\n");
    assert_eq!(read(&state.join("workspace-tab-order")), "other\nbeta\n");
    assert!(state.join("workspace-arrivals/beta").exists());
    assert!(!state.join("workspace-arrivals/alpha").exists());
    assert!(state.join("handbacks/workspace-beta/1.json").exists());
    assert!(!state.join("handbacks/workspace-alpha").exists());

    // The app's record.
    let record = crate::reopen::read_or_refusal(root).unwrap();
    assert_eq!(
        record.chats[0].cwd.as_deref(),
        Some(ws.join("svc").as_path())
    );
    assert_eq!(record.chats[1].from.as_ref().unwrap().workspace, "beta");
    assert_eq!(record.views[0].workspace.as_deref(), Some("beta"));
    assert_eq!(record.views[1].key, "beta");
    assert_eq!(record.views[1].title, "Workspace settings · beta");

    // This machine's pins, in the order they were pinned.
    let store = crate::machine::read(&plane.config).store;
    assert_eq!(
        store.recent(root).unwrap().pinned_workspaces,
        vec!["other".to_string(), "beta".to_string()]
    );

    // The record charter rewrote is vouched for: the next launch does not ask about it.
    assert!(
        !store
            .consent(root, &crate::machine::Contribution::of(root))
            .must_ask(),
        "the rewritten record was not vouched for"
    );

    // The journal is gone, and the plane was saved once with the move in it.
    assert!(!root.join(JOURNAL).exists());
    assert_eq!(
        git(root, &["log", "-1", "--format=%s"]).trim(),
        "charter workspace rename alpha beta"
    );
    let tracked = git(root, &["ls-files", "workspaces"]);
    assert!(
        tracked.contains("workspaces/beta/workspace.json"),
        "{tracked}"
    );
    assert!(!tracked.contains("workspaces/alpha/"), "{tracked}");
    assert_eq!(git(root, &["status", "--porcelain"]).trim(), "");
}

#[test]
fn a_workspace_is_renamed_and_everything_that_names_it_follows() {
    let plane = a_plane();

    let (code, said) = run(&plane, "alpha", "beta");

    assert_eq!(code, 0, "{said:?}");
    assert!(
        said.iter()
            .any(|l| l == "✓ Renamed workspace 'alpha' to 'beta'."),
        "{said:?}"
    );
    everything_follows(&plane);
}

#[test]
fn running_it_again_changes_nothing_and_says_there_is_no_alpha() {
    let plane = a_plane();
    assert_eq!(run(&plane, "alpha", "beta").0, 0);
    let head = git(&plane.root, &["rev-parse", "HEAD"]);

    let (code, said) = run(&plane, "alpha", "beta");

    assert_eq!(code, 1);
    assert_eq!(said, vec!["✗ no workspace 'alpha'"]);
    assert_eq!(git(&plane.root, &["rev-parse", "HEAD"]), head);
    everything_follows(&plane);
}

/// A crash at every step: before the commit point the old name still works whole, and after
/// it the same command finishes the rename.
#[test]
fn a_rename_killed_at_any_step_leaves_one_name_working_and_the_same_command_finishes_it() {
    for step in [
        Step::Journal,
        Step::Move,
        Step::Repair,
        Step::Manifest,
        Step::Headings,
        Step::Live,
        Step::Pointers,
        Step::State,
        Step::Reopen,
        Step::Pins,
    ] {
        let plane = a_plane();
        CRASH_AFTER.with(|at| at.set(Some(step)));
        let (code, _) = run(&plane, "alpha", "beta");
        CRASH_AFTER.with(|at| at.set(None));
        assert_eq!(code, 1, "{step:?}");
        assert!(plane.root.join(JOURNAL).exists(), "{step:?}");

        if step == Step::Journal {
            // Before the commit point: alpha is whole, its worktrees too.
            assert!(plane.root.join("workspaces/alpha").is_dir());
            assert!(!plane.root.join("workspaces/beta").exists());
            git(
                &plane.root.join("workspaces/alpha/.worktrees/svc/p1"),
                &["status", "--porcelain"],
            );
        } else {
            // After it, no other rename may start until this one is finished.
            let (code, said) = run(&plane, "other", "gamma");
            assert_eq!(code, 1, "{step:?}");
            assert!(
                said[0].contains("charter workspace rename alpha beta"),
                "{step:?}: {said:?}"
            );
            assert!(plane.root.join("workspaces/other").is_dir());
        }

        let (code, said) = run(&plane, "alpha", "beta");
        assert_eq!(code, 0, "{step:?}: {said:?}");
        everything_follows(&plane);
    }
}

#[test]
fn a_chat_running_in_the_workspace_stops_it_and_is_named() {
    let plane = a_plane();

    let (code, said) = run_with(
        &plane,
        "alpha",
        "beta",
        &["steward 1".to_string(), "claude 4".to_string()],
    );

    assert_eq!(code, 1);
    assert!(
        said[0].contains("chats are running in it: steward 1, claude 4"),
        "{said:?}"
    );
    assert!(plane.root.join("workspaces/alpha").is_dir());
    assert!(!plane.root.join("workspaces/beta").exists());
    assert!(!plane.root.join(JOURNAL).exists());
}

#[test]
fn a_name_that_is_taken_is_refused_before_anything_moves() {
    let plane = a_plane();

    let (code, said) = run(&plane, "alpha", "other");

    assert_eq!(code, 1);
    assert_eq!(
        said,
        vec!["✗ workspace 'other' already exists — pick another name or remove it first."]
    );
    assert!(plane.root.join("workspaces/alpha").is_dir());
    assert!(!plane.root.join(JOURNAL).exists());
}

#[test]
fn a_name_that_cannot_be_a_workspace_is_refused_before_anything_moves() {
    let plane = a_plane();
    for bad in ["../up", ".hidden", "", "a/b", "sp ace"] {
        let (code, said) = run(&plane, "alpha", bad);
        assert_eq!(code, 1, "{bad:?}");
        assert!(
            said[0].contains("invalid workspace name"),
            "{bad:?}: {said:?}"
        );
    }
    let (code, said) = run(&plane, "alpha", "alpha");
    assert_eq!(code, 1);
    assert!(said[0].contains("nothing to rename"), "{said:?}");
    let (code, said) = run(&plane, "nope", "beta");
    assert_eq!(code, 1);
    assert_eq!(said, vec!["✗ no workspace 'nope'"]);
    assert!(plane.root.join("workspaces/alpha").is_dir());
    assert!(!plane.root.join(JOURNAL).exists());
}

#[test]
fn a_plane_that_relocates_its_worktrees_is_refused() {
    let plane = a_plane();
    std::fs::write(
        plane.root.join("charter.toml"),
        "[plane]\nmode = \"commit\"\nworktrees = \"../wt\"\n",
    )
    .unwrap();

    let (code, said) = run(&plane, "alpha", "beta");

    assert_eq!(code, 1);
    assert!(said[0].contains("relocates its worktree root"), "{said:?}");
    assert!(plane.root.join("workspaces/alpha").is_dir());
}

#[test]
fn a_local_workspace_is_renamed_without_a_save() {
    let plane = a_plane();
    crate::wscmd::set_live(&plane.root, "other", false).unwrap();
    let head = git(&plane.root, &["rev-parse", "HEAD"]);

    let (code, said) = run(&plane, "other", "gamma");

    assert_eq!(code, 0, "{said:?}");
    assert_eq!(git(&plane.root, &["rev-parse", "HEAD"]), head, "{said:?}");
    assert_eq!(
        read(&plane.root.join(".charter/sessions/8.workspace")),
        "gamma\n"
    );
}

#[test]
fn a_heading_the_operator_rewrote_and_a_manifest_they_own_are_kept_as_theirs() {
    let plane = a_plane();
    let ws = plane.root.join("workspaces/alpha");
    std::fs::write(ws.join("workspace.md"), "# My own title\n\nbody\n").unwrap();
    std::fs::write(
        ws.join("workspace.json"),
        "{\n  \"name\": \"alpha\",\n  \"repos\": []\n}\n",
    )
    .unwrap();

    assert_eq!(run(&plane, "alpha", "beta").0, 0);

    let ws = plane.root.join("workspaces/beta");
    assert_eq!(read(&ws.join("workspace.md")), "# My own title\n\nbody\n");
    let written = read(&ws.join("workspace.json"));
    assert_eq!(written, "{\n  \"name\": \"beta\",\n  \"repos\": []\n}\n");
}

#[test]
fn a_default_in_the_settings_is_named_not_rewritten() {
    let plane = a_plane();
    std::fs::write(
        plane.root.join("charter.local.toml"),
        "[workspace]\ndefault = \"alpha\"\n",
    )
    .unwrap();

    let (code, said) = run(&plane, "alpha", "beta");

    assert_eq!(code, 0);
    assert!(
        said.iter()
            .any(|l| l.contains("charter.local.toml still sets [workspace] default = \"alpha\"")),
        "{said:?}"
    );
    assert_eq!(
        read(&plane.root.join("charter.local.toml")),
        "[workspace]\ndefault = \"alpha\"\n"
    );
}

#[test]
fn the_terminal_sees_the_apps_chats_only_while_an_app_is_listening() {
    let plane = a_plane();
    assert_eq!(open_in_app(&plane.root, &["alpha"]), Vec::<String>::new());

    let socket = plane.root.join(".charter/app/hooks.sock");
    let _app = std::os::unix::net::UnixListener::bind(&socket).unwrap();

    assert_eq!(open_in_app(&plane.root, &["alpha"]), vec!["steward 1"]);
    assert_eq!(open_in_app(&plane.root, &["other"]), vec!["steward 2"]);
    assert_eq!(open_in_app(&plane.root, &["nope"]), Vec::<String>::new());
}

#[test]
fn a_move_maps_paths_under_the_workspace_and_nothing_else() {
    let moved = Move::in_plane(Path::new("/p"), "alpha", "beta");
    assert_eq!(
        moved.path(Path::new("/p/workspaces/alpha/svc")),
        Some(PathBuf::from("/p/workspaces/beta/svc"))
    );
    assert_eq!(
        moved.path(Path::new("/p/workspaces/alpha")),
        Some(PathBuf::from("/p/workspaces/beta"))
    );
    assert_eq!(moved.path(Path::new("/p/workspaces/alphabet/svc")), None);
    assert_eq!(moved.path(Path::new("/p/workspaces/other")), None);
}

#[test]
fn a_pin_moves_in_place_and_a_pin_on_both_names_becomes_one() {
    let dir = tempfile::tempdir().unwrap();
    let plane = dir.path().join("p");
    let mut store = crate::machine::Store::default();
    store.remember(&plane, 1);
    for ws in ["a", "old", "z"] {
        store.pin_workspace(&plane, ws, true).unwrap();
    }
    assert!(store.rename_workspace(&plane, "old", "new"));
    assert_eq!(
        store.recent(&plane).unwrap().pinned_workspaces,
        vec!["a", "new", "z"]
    );
    assert!(!store.rename_workspace(&plane, "old", "new"));
    assert!(store.rename_workspace(&plane, "a", "z"));
    assert_eq!(
        store.recent(&plane).unwrap().pinned_workspaces,
        vec!["new", "z"]
    );
}

/// A case-only rename moves everything kept under the name — on a case-insensitive disk, where
/// the old and new names are one directory entry, as on a case-sensitive one.
#[test]
fn a_rename_that_only_changes_case_keeps_everything_under_the_name() {
    let plane = a_plane();
    let state = plane.root.join(".charter");

    let (code, said) = run(&plane, "alpha", "Alpha");

    assert_eq!(code, 0, "{said:?}");
    let names: Vec<String> = std::fs::read_dir(plane.root.join("workspaces"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"Alpha".to_string()), "{names:?}");
    assert!(!names.contains(&"alpha".to_string()), "{names:?}");
    assert!(state.join("workspace-arrivals/Alpha").exists());
    assert!(state.join("handbacks/workspace-Alpha/1.json").exists());
    assert_eq!(read(&state.join("sessions/7.workspace")), "Alpha\n");
}

/// A journal that cannot record the move puts it back: a rename the journal does not vouch for
/// is one a rerun could not tell from a stale note.
#[test]
fn a_move_the_journal_cannot_record_is_put_back() {
    let plane = a_plane();
    let _hook = crate::rewrite::hook::set(|target, _| {
        let text = std::fs::read_to_string(target).unwrap_or_default();
        if target.ends_with("workspace-rename.json") && text.contains("\"moved\":false") {
            return Err(std::io::Error::other("the disk is full"));
        }
        Ok(())
    });

    let (code, said) = run(&plane, "alpha", "beta");

    assert_eq!(code, 1);
    assert!(said[0].contains("it was put back"), "{said:?}");
    assert!(plane.root.join("workspaces/alpha/svc").is_dir());
    assert!(!plane.root.join("workspaces/beta").exists());
    assert!(!plane.root.join(JOURNAL).exists());
}
