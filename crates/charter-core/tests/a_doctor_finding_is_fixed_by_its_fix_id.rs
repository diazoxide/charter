//! The fix registry (FX-1, V91p): a doctor finding charter can fix carries a **fix id**, and one
//! core entry point applies a fix by that id — what `charter doctor --fix` and the Doctor
//! dialog's Fix button both call. Driven here the way both callers drive it: run the doctor,
//! read the id off the row, apply it, run the doctor again.
//!
//! **Discover asks a forge**, so its test runs in a child of this binary with a stand-in `gh`
//! first on `PATH` (`support/forge_cli.rs` says why a test cannot change its own `PATH`).

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use charter_core::doctor::fix::{self, FixId, Fixed};
use charter_core::doctor::{Doctor, Row, Status};

fn project(manifest: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(root.join("charter.toml"), manifest).unwrap();
    (dir, root)
}

fn row(root: &Path, name: &str) -> Row {
    Doctor::at(root, root, false, true)
        .run()
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no row {name}"))
}

/// [`row`], from the doctor a person types rather than the preflight: the one that asks git
/// whether it would carry `charter.local.toml`.
fn typed_row(doctor: Doctor, name: &str) -> Row {
    doctor
        .run()
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no row {name}"))
}

fn typed(root: &Path) -> Doctor {
    Doctor::at(root, root, false, false)
}

/// [`tree`], leaving out `.git`: asking git a question can refresh its own index.
fn outside_git(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    tree(root)
        .into_iter()
        .filter(|(path, _)| !path.starts_with(".git"))
        .collect()
}

/// A project with its baseline folders, so the only finding is the one a test makes.
fn baseline(manifest: &str) -> (tempfile::TempDir, PathBuf) {
    let (dir, root) = project(manifest);
    for sub in ["personas", "inventory", "workspaces"] {
        std::fs::create_dir_all(root.join(sub)).unwrap();
    }
    (dir, root)
}

fn git(root: &Path, args: &[&str]) {
    let ran = charter_core::forklock::status(
        support::unsigned()
            .args(args)
            .current_dir(root)
            .env("GIT_CONFIG_NOSYSTEM", "1"),
    )
    .unwrap();
    assert!(ran.success(), "git {args:?}");
}

fn ran(applied: &Fixed) -> &[String] {
    match applied {
        Fixed::Ran { said, complete } => {
            assert!(complete, "it did not finish: {said:?}");
            said
        }
        Fixed::Refused(why) => panic!("refused: {why}"),
    }
}

fn refused(applied: &Fixed) -> &str {
    match applied {
        Fixed::Refused(why) => why,
        Fixed::Ran { said, .. } => panic!("it ran: {said:?}"),
    }
}

/// Every file and directory under `root` with its bytes: what "untouched" is measured against.
fn tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            if path.is_dir() {
                out.insert(rel, None);
                walk(&path, root, out);
            } else {
                out.insert(rel, Some(std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn a_project_missing_its_baseline_folders_is_fixed_by_reinit_and_then_checks_clean() {
    charter_core::unsteered!();
    let (_d, root) = project("schema = 1\n");

    let found = row(&root, "schema");
    assert_eq!(found.status, Status::Warn, "{found:?}");
    assert_eq!(found.fix, Some(FixId::Reinit), "{found:?}");

    let applied = fix::apply(&root, FixId::Reinit);
    let Fixed::Ran { said, complete } = &applied else {
        panic!("reinit was refused: {applied:?}");
    };
    assert!(complete, "{said:?}");
    assert!(
        said.iter().any(|line| line.contains("personas")),
        "it says what it created: {said:?}"
    );
    for dir in ["personas", "inventory", "workspaces"] {
        assert!(root.join(dir).is_dir(), "{dir}/ was not created");
    }

    let after = row(&root, "schema");
    assert_eq!(after.status, Status::Ok, "{after:?}");
    assert_eq!(after.fix, None, "a clean row offers no fix: {after:?}");
}

#[test]
fn a_fix_on_a_project_this_charter_cannot_write_is_refused_and_writes_nothing() {
    charter_core::unsteered!();
    let (_d, root) =
        project("schema = 2\nrequires = [{ feature = \"memory-proposals\", since = \"0.9.0\" }]\n");
    let before = tree(&root);

    let applied = fix::apply(&root, FixId::Reinit);

    let Fixed::Refused(why) = &applied else {
        panic!("a read-only project was written: {applied:?}");
    };
    assert!(why.contains("memory-proposals"), "it says why: {why}");
    assert_eq!(tree(&root), before, "the refused fix wrote to the project");
}

#[test]
fn a_fix_where_there_is_no_project_is_refused_and_writes_nothing() {
    charter_core::unsteered!();
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();

    let applied = fix::apply(&root, FixId::Reinit);

    assert!(matches!(applied, Fixed::Refused(_)), "{applied:?}");
    assert!(tree(&root).is_empty(), "{:?}", tree(&root));
}

#[test]
fn a_stale_index_lock_is_reported_and_never_offered_as_a_fix() {
    charter_core::unsteered!();
    let (_d, root) = project("schema = 1\n");
    for dir in ["personas", "inventory", "workspaces"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    let git = charter_core::forklock::status(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root),
    )
    .unwrap();
    assert!(git.success());
    let lock = std::fs::File::create(root.join(".git/index.lock")).unwrap();
    lock.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(24 * 3600))
        .unwrap();

    let found = row(&root, "index lock");
    assert_eq!(found.status, Status::Warn, "{found:?}");
    assert_eq!(found.fix, None, "charter never removes a lock: {found:?}");
    assert!(
        !Doctor::at(&root, &root, false, true)
            .fixes()
            .iter()
            .any(|id| id.id().contains("lock")),
    );
}

#[test]
fn the_doctor_offers_exactly_the_fixes_its_rows_carry() {
    charter_core::unsteered!();
    let (_d, root) = project("schema = 1\n");
    // Without git-identity, which the machine's own git config decides (FX-3's tests set it
    // on a temporary home).
    let offered = |root: &Path| -> Vec<FixId> {
        Doctor::at(root, root, false, true)
            .fixes()
            .into_iter()
            .filter(|id| *id != FixId::GitIdentity)
            .collect()
    };
    assert_eq!(offered(&root), vec![FixId::Reinit]);

    for dir in ["personas", "inventory", "workspaces"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    assert_eq!(offered(&root), vec![]);
}

#[test]
fn a_fix_id_is_spelled_one_way_and_read_back_the_same() {
    charter_core::unsteered!();
    assert_eq!(FixId::Reinit.id(), "reinit");
    assert_eq!(FixId::parse("reinit"), Some(FixId::Reinit));
    assert_eq!(FixId::parse("index-lock"), None);
    assert_eq!(FixId::GitIdentity.id(), "git-identity");
    assert_eq!(FixId::parse("git-identity"), Some(FixId::GitIdentity));
    assert_eq!(
        FixId::ALL.map(FixId::id),
        [
            "plugin-install",
            "reinit",
            "local-ignore",
            "memory-optimize",
            "discover",
            "git-identity"
        ]
    );
    for id in FixId::ALL {
        assert_eq!(FixId::parse(id.id()), Some(id));
    }
}

#[test]
fn every_fix_but_discover_is_applied_by_bare_fix() {
    charter_core::unsteered!();
    // Discover asks a forge over the network and writes the inventory and the docs, so it runs
    // only when it is named (D-FX2-9).
    let bare: Vec<&str> = FixId::ALL
        .into_iter()
        .filter(|id| !id.by_name_only())
        .map(FixId::id)
        .collect();
    assert_eq!(
        bare,
        [
            "plugin-install",
            "reinit",
            "local-ignore",
            "memory-optimize",
            "git-identity"
        ]
    );
    // git-identity is chosen by bare --fix too, and refused there: it takes input
    // (D-FX3-1), and saying what to give is the honest answer.
    assert!(FixId::Discover.by_name_only());
}

// ---- local-ignore -------------------------------------------------------------------------

const PROFILE: &str = "[harness.claude-work]\nkind = \"claude\"\ncommand = [\"claude\"]\n";

#[test]
fn a_local_file_git_would_commit_is_ignored_by_local_ignore_and_then_checks_clean() {
    charter_core::unsteered!();
    let (_d, root) = baseline("schema = 1\n");
    git(&root, &["init", "-q"]);
    std::fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();
    std::fs::write(root.join("charter.local.toml"), PROFILE).unwrap();

    let found = typed_row(typed(&root), "harness profiles");
    assert_eq!(found.status, Status::Warn, "{found:?}");
    assert_eq!(found.fix, Some(FixId::LocalIgnore), "{found:?}");

    let said = ran(&fix::apply(&root, FixId::LocalIgnore)).to_vec();
    assert!(
        said.iter().any(|line| line.contains("/charter.local.toml")),
        "it says what it added: {said:?}"
    );
    let ignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(
        ignore.starts_with("node_modules/\n"),
        "what was there is kept: {ignore:?}"
    );
    assert!(
        ignore.lines().any(|line| line == "/charter.local.toml"),
        "{ignore:?}"
    );
    // Under its purlis name too (RN-2a), so a leftover under the other name never travels.
    assert!(
        ignore.lines().any(|line| line == "/purlis.local.toml"),
        "{ignore:?}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("charter.local.toml")).unwrap(),
        PROFILE,
        "the local file itself is not touched"
    );

    let after = typed_row(typed(&root), "harness profiles");
    assert_eq!(after.status, Status::Ok, "{after:?}");
    assert_eq!(after.fix, None, "{after:?}");
    // A second run has nothing left to do, and says so.
    assert!(fix::apply(&root, FixId::LocalIgnore).complete());
    assert_eq!(
        std::fs::read_to_string(root.join(".gitignore")).unwrap(),
        ignore
    );
}

#[test]
fn a_local_file_git_already_tracks_is_refused_by_local_ignore_and_never_untracked() {
    charter_core::unsteered!();
    let (_d, root) = baseline("schema = 1\n");
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("charter.local.toml"), PROFILE).unwrap();
    git(&root, &["add", "charter.local.toml"]);

    let found = typed_row(typed(&root), "harness profiles");
    assert_eq!(found.status, Status::Warn, "{found:?}");
    assert_eq!(
        found.fix, None,
        "an ignore line cannot cure a tracked file, so none is offered: {found:?}"
    );
    let before = outside_git(&root);

    let applied = fix::apply(&root, FixId::LocalIgnore);

    let why = refused(&applied);
    assert!(why.contains("tracks"), "it says why: {why}");
    assert!(
        why.contains("git rm --cached charter.local.toml"),
        "it names the step that is the operator's: {why}"
    );
    assert_eq!(
        outside_git(&root),
        before,
        "the refused fix wrote to the project"
    );
    let index = charter_core::forklock::output(
        std::process::Command::new("git")
            .args(["ls-files", "charter.local.toml"])
            .current_dir(&root),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&index.stdout),
        "charter.local.toml\n",
        "still tracked: charter never runs git rm"
    );
}

#[cfg(unix)]
#[test]
fn a_gitignore_that_is_a_link_is_not_offered_local_ignore_and_is_never_written_through() {
    charter_core::unsteered!();
    // git does not read a `.gitignore` that is a symbolic link, so a line appended through one
    // would change nothing git does.
    let (_d, root) = baseline("schema = 1\n");
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("ignored-rules"), "node_modules/\n").unwrap();
    std::os::unix::fs::symlink("ignored-rules", root.join(".gitignore")).unwrap();
    std::fs::write(root.join("charter.local.toml"), PROFILE).unwrap();

    let found = typed_row(typed(&root), "harness profiles");
    assert_eq!(found.status, Status::Warn, "{found:?}");
    assert_eq!(found.fix, None, "{found:?}");
    let before = outside_git(&root);

    let why = refused(&fix::apply(&root, FixId::LocalIgnore)).to_owned();

    assert!(why.contains("symbolic link"), "it says why: {why}");
    assert_eq!(
        outside_git(&root),
        before,
        "the refused fix wrote to the project"
    );
}

// ---- memory-optimize ----------------------------------------------------------------------

fn memory(root: &Path, base: &str, index: &str, files: &[(&str, &str)]) {
    let dir = root.join(base);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("MEMORY.md"), index).unwrap();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
}

#[test]
fn an_unindexed_memory_is_linked_by_memory_optimize_and_the_indexes_then_check_clean() {
    charter_core::unsteered!();
    let (_d, root) = baseline("schema = 1\n");
    std::fs::create_dir_all(root.join("personas/steward")).unwrap();
    std::fs::write(
        root.join("personas/steward/persona.md"),
        "---\nrole: x\n---\n",
    )
    .unwrap();
    let kept = "---\ntitle: Kept\n---\nA lesson worth keeping.\n";
    memory(
        &root,
        "personas/steward/memory",
        "- [A](a.md)\n",
        &[("a.md", "# A\nfirst\n"), ("kept.md", kept)],
    );
    std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
    memory(
        &root,
        "workspaces/alpha/memory",
        "",
        &[("note.md", "# Note\nsecond\n")],
    );

    let found = row(&root, "memory indexes");
    assert_eq!(found.status, Status::Warn, "{found:?}");
    assert_eq!(found.fix, Some(FixId::MemoryOptimize), "{found:?}");

    let said = ran(&fix::apply(&root, FixId::MemoryOptimize)).to_vec();
    assert!(
        said.iter().any(|line| line.contains("repaired index")),
        "it says what it linked: {said:?}"
    );
    let index = std::fs::read_to_string(root.join("personas/steward/memory/MEMORY.md")).unwrap();
    assert!(index.starts_with("- [A](a.md)\n"), "{index:?}");
    assert!(index.contains("(kept.md)"), "{index:?}");
    assert!(
        std::fs::read_to_string(root.join("workspaces/alpha/memory/MEMORY.md"))
            .unwrap()
            .contains("(note.md)")
    );
    assert_eq!(
        std::fs::read_to_string(root.join("personas/steward/memory/kept.md")).unwrap(),
        kept,
        "a memory is linked, never rewritten"
    );

    let after = row(&root, "memory indexes");
    assert_eq!(after.status, Status::Ok, "{after:?}");
    assert_eq!(after.fix, None, "{after:?}");
}

#[test]
fn memory_optimize_repairs_the_index_and_leaves_an_exact_duplicate_in_place() {
    charter_core::unsteered!();
    // Collapsing duplicates is curation the operator asks for with `optimize --apply`; the
    // doctor's fix only links what the index is missing (D-FX2-6, amended).
    let (_d, root) = baseline("schema = 1\n");
    std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
    let same = "# Same\nthe very same body\n";
    memory(
        &root,
        "workspaces/alpha/memory",
        "- [One](one.md)\n",
        &[("one.md", same), ("two.md", same)],
    );
    assert_eq!(
        row(&root, "memory indexes").fix,
        Some(FixId::MemoryOptimize)
    );

    let said = ran(&fix::apply(&root, FixId::MemoryOptimize)).to_vec();

    let mem = root.join("workspaces/alpha/memory");
    assert_eq!(std::fs::read_to_string(mem.join("one.md")).unwrap(), same);
    assert_eq!(
        std::fs::read_to_string(mem.join("two.md")).unwrap(),
        same,
        "the duplicate stays where it was: {said:?}"
    );
    assert!(
        !mem.join("archive").exists(),
        "nothing was archived: {said:?}"
    );
    let index = std::fs::read_to_string(mem.join("MEMORY.md")).unwrap();
    assert!(index.starts_with("- [One](one.md)\n"), "{index:?}");
    assert!(index.contains("(two.md)"), "{index:?}");
    assert_eq!(row(&root, "memory indexes").status, Status::Ok);
}

#[test]
fn a_dangling_link_alone_is_not_offered_memory_optimize() {
    charter_core::unsteered!();
    // Optimize proposes a dangling link and never prunes it, so it is no fix for one.
    let (_d, root) = baseline("schema = 1\n");
    std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
    memory(&root, "workspaces/alpha/memory", "- [Gone](gone.md)\n", &[]);
    let found = row(&root, "memory indexes");
    assert_eq!(found.status, Status::Warn, "{found:?}");
    assert_eq!(found.fix, None, "{found:?}");
}

#[test]
fn memory_optimize_on_a_project_this_charter_cannot_write_is_refused_and_writes_nothing() {
    charter_core::unsteered!();
    let (_d, root) = baseline(
        "schema = 2\nrequires = [{ feature = \"memory-proposals\", since = \"0.9.0\" }]\n",
    );
    std::fs::create_dir_all(root.join("workspaces/alpha")).unwrap();
    memory(
        &root,
        "workspaces/alpha/memory",
        "",
        &[("note.md", "# Note\nsecond\n")],
    );
    let before = tree(&root);

    let why = refused(&fix::apply(&root, FixId::MemoryOptimize)).to_owned();

    assert!(why.contains("memory-proposals"), "it says why: {why}");
    assert_eq!(tree(&root), before, "the refused fix wrote to the project");
}

// ---- plugin-install -----------------------------------------------------------------------

/// A machine with Claude Code and Codex set up and nothing installed, all under `dir`.
fn machine(dir: &Path) -> charter_core::plugin_install::Machine {
    std::fs::create_dir_all(dir.join("claude")).unwrap();
    std::fs::create_dir_all(dir.join("codex")).unwrap();
    std::fs::write(dir.join("charter-bin"), "").unwrap();
    charter_core::plugin_install::Machine {
        claude_config: dir.join("claude"),
        codex_home: dir.join("codex"),
        opencode_config: dir.join("opencode"),
        charter_dir: dir.join("config/charter"),
        binary: dir.join("charter-bin"),
        bundle: Some(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../app/src-tauri/plugin")
                .canonicalize()
                .unwrap(),
        ),
    }
}

#[test]
fn a_machine_without_charters_plugin_is_fixed_by_plugin_install_and_then_checks_clean() {
    charter_core::unsteered!();
    let (d, root) = baseline("schema = 1\n");
    let m = machine(&d.path().canonicalize().unwrap().join("machine"));
    let doctor = || typed(&root).with_machine(m.clone());

    let found = typed_row(doctor(), "plugin install");
    assert_eq!(found.status, Status::Warn, "{found:?}");
    assert_eq!(found.fix, Some(FixId::PluginInstall), "{found:?}");
    assert!(doctor().fixes().contains(&FixId::PluginInstall));

    let said = ran(&fix::apply_for(&root, FixId::PluginInstall, &m)).to_vec();
    assert!(
        said.iter().any(|line| line.starts_with("codex:")),
        "it says what it installed, as `charter plugin install` does: {said:?}"
    );

    for name in ["plugin install", "plugin", "plugin files"] {
        let after = typed_row(doctor(), name);
        assert_eq!(after.status, Status::Ok, "{after:?}");
        assert_eq!(after.fix, None, "{after:?}");
    }
}

#[test]
fn a_stale_copy_and_a_copy_whose_charter_is_gone_are_offered_plugin_install() {
    charter_core::unsteered!();
    let (d, root) = baseline("schema = 1\n");
    let m = machine(&d.path().canonicalize().unwrap().join("machine"));
    ran(&fix::apply_for(&root, FixId::PluginInstall, &m));
    std::fs::write(
        m.charter_dir.join("plugin/skills/handoff/SKILL.md"),
        "an older skill\n",
    )
    .unwrap();
    let stale = typed_row(typed(&root).with_machine(m.clone()), "plugin");
    assert_eq!(stale.status, Status::Warn, "{stale:?}");
    assert_eq!(stale.fix, Some(FixId::PluginInstall), "{stale:?}");

    std::fs::remove_file(&m.binary).unwrap();
    let gone = typed_row(typed(&root).with_machine(m.clone()), "plugin files");
    assert_eq!(gone.status, Status::Warn, "{gone:?}");
    assert_eq!(gone.fix, Some(FixId::PluginInstall), "{gone:?}");
}

#[test]
fn plugin_install_with_no_plugin_to_copy_says_so_and_is_not_complete() {
    charter_core::unsteered!();
    // Codex takes only a hook and needs no copy; Claude Code needs the plugin to copy, and
    // the fix says it could not, rather than reading as done.
    let (d, root) = baseline("schema = 1\n");
    let mut m = machine(&d.path().canonicalize().unwrap().join("machine"));
    m.bundle = None;

    let applied = fix::apply_for(&root, FixId::PluginInstall, &m);

    let Fixed::Ran { said, complete } = &applied else {
        panic!("{applied:?}");
    };
    assert!(!complete, "{said:?}");
    assert!(
        said.iter().any(|line| line.starts_with("claude:")),
        "{said:?}"
    );
    assert!(
        !m.claude_config.join("settings.json").exists(),
        "nothing enables a plugin that was not copied"
    );
}

// ---- discover -----------------------------------------------------------------------------

#[test]
fn discover_is_a_fix_asked_of_a_stand_in_forge() {
    charter_core::unsteered!();
    support::forge_cli::in_a_child("discovered::", "bin");
}

mod discovered {
    use super::*;
    use support::forge_cli::{Scene, in_child};

    fn forge_project(host: &str) -> (tempfile::TempDir, PathBuf) {
        baseline(&format!(
            "schema = 1\n\n[[forge]]\nkind = \"github\"\nhost = \"{host}\"\nowner = \"acme\"\n"
        ))
    }

    #[test]
    fn an_empty_inventory_is_built_by_discover_and_then_checks_clean() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("fix-discover.test");
        let (_d, root) = forge_project(&scene.host);
        scene.answers(
            "gh",
            &["auth", "status", "--hostname", &scene.host],
            0,
            "",
            "",
        );
        scene.gh_api(
            "orgs/acme/repos?per_page=100&page=1",
            0,
            r#"[{"id": 11, "name": "widget", "full_name": "acme/widget", "default_branch": "main",
                "description": "", "html_url": "https://example.invalid/acme/widget",
                "ssh_url": "git@example.invalid:acme/widget.git", "topics": []}]"#,
            "",
        );
        scene.gh_api(
            "repos/acme/widget/git/trees/main",
            0,
            r#"{"tree": [{"path": "Cargo.toml"}]}"#,
            "",
        );

        let found = row(&root, "inventory");
        assert_eq!(found.status, Status::Warn, "{found:?}");
        assert_eq!(found.fix, Some(FixId::Discover), "{found:?}");

        let said = ran(&fix::apply(&root, FixId::Discover)).to_vec();
        assert!(
            said.iter().any(|line| line.contains("Wrote 1 repos")),
            "{said:?}"
        );
        let doc = std::fs::read_to_string(root.join("inventory/repos.json")).unwrap();
        assert!(doc.contains("\"widget\""), "{doc}");

        let after = row(&root, "inventory");
        assert_eq!(after.status, Status::Ok, "{after:?}");
        assert_eq!(after.fix, None, "{after:?}");
    }

    #[test]
    fn discover_against_a_forge_that_is_not_logged_in_is_refused_and_writes_nothing() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("fix-discover-out.test");
        let (_d, root) = forge_project(&scene.host);
        scene.answers(
            "gh",
            &["auth", "status", "--hostname", &scene.host],
            1,
            "",
            "You are not logged into any GitHub hosts.",
        );
        let before = tree(&root);

        let why = refused(&fix::apply(&root, FixId::Discover)).to_owned();

        assert!(why.contains("not authenticated"), "it says why: {why}");
        assert_eq!(tree(&root), before, "the refused fix wrote to the project");
    }
}

#[test]
fn a_project_with_no_forge_is_not_offered_discover() {
    charter_core::unsteered!();
    // With no `[[forge]]`, discover has nobody to ask: a Fix button would only fail.
    let (_d, root) = baseline("schema = 1\n");
    let found = row(&root, "inventory");
    assert_eq!(found.status, Status::Warn, "{found:?}");
    assert_eq!(found.fix, None, "{found:?}");
}
