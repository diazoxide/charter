//! The precedence matrix (charter-app#292, ADR 0051): default, then Shared, then Local, key by
//! key — with the expected answer written out rather than recomputed.

use super::*;

fn settings(shared: &str, local: &str) -> Settings {
    Settings::from_text(Some(shared), Some(local))
}

#[test]
fn shared_names_the_planes_mode() {
    let got = settings("[plane]\nmode = \"pr\"\n", "").plane.mode;
    assert_eq!(
        got,
        Resolved {
            value: Some(Mode::Pr),
            source: Source::Shared
        }
    );
}

#[test]
fn local_overrides_the_shared_mode() {
    let got = settings("[plane]\nmode = \"pr\"\n", "[plane]\nmode = \"push\"\n")
        .plane
        .mode;
    assert_eq!(
        got,
        Resolved {
            value: Some(Mode::Push),
            source: Source::Local
        }
    );
}

#[test]
fn a_mode_charter_cannot_read_falls_through_to_the_next_file_down() {
    let got = settings("[plane]\nmode = \"push\"\n", "[plane]\nmode = \"yolo\"\n")
        .plane
        .mode;
    assert_eq!(
        got,
        Resolved {
            value: Some(Mode::Push),
            source: Source::Shared
        }
    );
}

#[test]
fn neither_file_naming_a_mode_leaves_it_unset() {
    let got = settings("", "").plane.mode;
    assert_eq!(
        got,
        Resolved {
            value: None,
            source: Source::Default
        }
    );
}

// -------------------------------------------------------------------------------------
// `[memory] share`, the deprecated alias
// -------------------------------------------------------------------------------------

#[test]
fn share_push_and_share_commit_carry_over_as_the_mode() {
    for (word, mode) in [("push", Mode::Push), ("commit", Mode::Commit)] {
        let got = settings(&format!("[memory]\nshare = \"{word}\"\n"), "").plane;
        assert_eq!(
            (got.mode.value, got.mode.source, got.from_share),
            (Some(mode), Source::Shared, true),
            "share = {word}"
        );
    }
}

#[test]
fn share_local_leaves_the_mode_unset_so_the_plane_is_asked() {
    // `charter init` always wrote `share = "local"`: reading it as `off` would have kept
    // auto-save off on every existing plane (ADR 0051).
    let got = settings("[memory]\nshare = \"local\"\n", "").plane;
    assert_eq!(
        (got.mode.value, got.mode.source, got.from_share),
        (None, Source::Default, false)
    );
}

#[test]
fn a_plane_mode_in_either_file_wins_over_share() {
    let shared = settings("[memory]\nshare = \"push\"\n[plane]\nmode = \"pr\"\n", "").plane;
    assert_eq!(
        (shared.mode.value, shared.from_share),
        (Some(Mode::Pr), false)
    );
    let local = settings("[memory]\nshare = \"push\"\n", "[plane]\nmode = \"off\"\n").plane;
    assert_eq!(
        (local.mode.value, local.mode.source, local.from_share),
        (Some(Mode::Off), Source::Local, false)
    );
}

// -------------------------------------------------------------------------------------
// The plane's other keys
// -------------------------------------------------------------------------------------

#[test]
fn a_plane_that_says_nothing_saves_unsigned_and_auto_saves_after_thirty_seconds_of_quiet() {
    let got = settings("", "").plane;
    assert_eq!(
        got.branch,
        Resolved {
            value: None,
            source: Source::Default
        }
    );
    assert_eq!(
        got.save_branch,
        Resolved {
            value: None,
            source: Source::Default
        }
    );
    assert_eq!(
        got.sign,
        Resolved {
            value: false,
            source: Source::Default
        }
    );
    assert_eq!(
        got.autosave,
        Resolved {
            value: true,
            source: Source::Default
        }
    );
    assert_eq!(
        got.autosave_after,
        Resolved {
            value: Duration::from_secs(30),
            source: Source::Default
        }
    );
}

#[test]
fn each_key_is_overridden_on_its_own() {
    let got = settings(
        "[plane]\nbranch = \"main\"\nsign = true\nautosave_after = \"2m\"\n",
        "[plane]\nautosave = false\nsave_branch = \"charter/save/laptop\"\n",
    )
    .plane;
    assert_eq!(
        got.branch,
        Resolved {
            value: Some("main".to_owned()),
            source: Source::Shared
        }
    );
    assert_eq!(
        got.save_branch,
        Resolved {
            value: Some("charter/save/laptop".to_owned()),
            source: Source::Local
        }
    );
    assert_eq!(
        got.sign,
        Resolved {
            value: true,
            source: Source::Shared
        }
    );
    assert_eq!(
        got.autosave,
        Resolved {
            value: false,
            source: Source::Local
        }
    );
    assert_eq!(
        got.autosave_after,
        Resolved {
            value: Duration::from_secs(120),
            source: Source::Shared
        }
    );
}

#[test]
fn a_quiet_period_or_branch_charter_cannot_read_is_passed_over() {
    let got = settings(
        "[plane]\nautosave_after = \"45s\"\nbranch = \"main\"\n",
        "[plane]\nautosave_after = \"soon\"\nbranch = \"has space\"\nsign = \"yes\"\n",
    )
    .plane;
    assert_eq!(got.autosave_after.value, Duration::from_secs(45));
    assert_eq!(got.branch.value.as_deref(), Some("main"));
    assert_eq!(
        got.sign,
        Resolved {
            value: false,
            source: Source::Default
        }
    );
    for word in ["0s", "30", "-5s", "1h", ""] {
        let got = settings(&format!("[plane]\nautosave_after = \"{word}\"\n"), "").plane;
        assert_eq!(got.autosave_after.source, Source::Default, "{word:?}");
    }
}

// -------------------------------------------------------------------------------------
// `[repos.<name>]`
// -------------------------------------------------------------------------------------

#[test]
fn a_repo_nobody_configured_is_never_saved_until_somebody_says_how() {
    // Code is not a database, and a developer's branch is theirs: a repo charter was never told
    // how to save is not committed, pushed or opened a PR for, by any button (ADR 0051, amended
    // 2026-09-25 — it was `pr`, and one press of the title bar pushed feature branches).
    let got = settings("[plane]\nmode = \"push\"\nautosave = true\n", "").repo("charter-app");
    assert_eq!(
        got.mode,
        Resolved {
            value: Mode::Off,
            source: Source::Default
        }
    );
    assert_eq!(
        got.autosave,
        Resolved {
            value: false,
            source: Source::Default
        }
    );
    assert_eq!(
        got.branch,
        Resolved {
            value: None,
            source: Source::Default
        }
    );
    assert_eq!(
        got.sign,
        Resolved {
            value: false,
            source: Source::Default
        }
    );
    assert_eq!(got.autosave_after.value, QUIET);
}

#[test]
fn a_repos_table_is_read_by_the_repos_name_and_local_overrides_it_key_by_key() {
    let got = settings(
        "[repos.charter-app]\nmode = \"pr-merge\"\nbranch = \"main\"\n",
        "[repos.charter-app]\nautosave = true\n[repos.other]\nmode = \"push\"\n",
    )
    .repo("charter-app");
    assert_eq!(
        got.mode,
        Resolved {
            value: Mode::PrMerge,
            source: Source::Shared
        }
    );
    assert_eq!(got.branch.value.as_deref(), Some("main"));
    assert_eq!(
        got.autosave,
        Resolved {
            value: true,
            source: Source::Local
        }
    );
}

#[test]
fn a_repo_name_with_a_dot_is_one_table_not_a_path() {
    let got = settings("[repos.\"my.repo\"]\nmode = \"push\"\n", "").repo("my.repo");
    assert_eq!(got.mode.value, Mode::Push);
}

// -------------------------------------------------------------------------------------
// Review of #307
// -------------------------------------------------------------------------------------

#[test]
fn a_quiet_period_ending_in_a_character_wider_than_a_byte_is_refused_not_a_panic() {
    for word in ["3é", "30秒", "é"] {
        let text = format!("[plane]\nautosave_after = \"{word}\"\n");
        assert_eq!(
            settings(&text, "").plane.autosave_after.source,
            Source::Default,
            "{word}"
        );
        assert_eq!(refusals(&text, false, "charter.toml").len(), 1, "{word}");
    }
}

#[test]
fn a_branch_with_a_component_ending_in_lock_is_one_git_refuses() {
    for name in ["a.lock/b", "feature.lock/x"] {
        assert!(!branch_ok(name), "{name}");
    }
    assert!(branch_ok("charter/save/laptop"));
    assert!(branch_ok("locks/x"));
}

#[test]
fn a_local_file_git_would_commit_changes_no_save_setting() {
    // The local file's own rule: an ignored file must not change plane policy with no trace
    // in git — and a file git would commit is not the ignored file.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    crate::testgit::run(root, &["init", "-q"]);
    std::fs::write(root.join("charter.toml"), "[plane]\nmode = \"pr\"\n").unwrap();
    std::fs::write(
        root.join("charter.local.toml"),
        "[plane]\nmode = \"push\"\n",
    )
    .unwrap();
    assert_eq!(
        Settings::read(root).plane.mode,
        Resolved {
            value: Some(Mode::Pr),
            source: Source::Shared
        }
    );
    std::fs::write(root.join(".gitignore"), "/charter.local.toml\n").unwrap();
    assert_eq!(
        Settings::read(root).plane.mode,
        Resolved {
            value: Some(Mode::Push),
            source: Source::Local
        }
    );
}

/// A plane whose `charter.local.toml` git would commit (charter-app#308): not ignored.
fn carried_local(local: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    crate::testgit::run(root, &["init", "-q"]);
    std::fs::write(root.join("charter.toml"), "[plane]\nmode = \"pr\"\n").unwrap();
    std::fs::write(root.join("charter.local.toml"), local).unwrap();
    dir
}

/// Whether `why` is the ignore check's sentence for a Local file git would carry.
fn is_the_checks(why: Option<&str>) -> bool {
    why.is_some_and(|why| why.contains("charter reads nothing in it"))
}

#[test]
fn a_left_out_local_file_that_set_a_plane_key_keeps_the_sentence_for_the_plane_alone() {
    let dir = carried_local("[plane]\nmode = \"push\"\n");
    let read = Settings::read(dir.path());
    assert!(
        is_the_checks(read.plane_left_out.as_deref()),
        "{:?}",
        read.plane_left_out
    );
    assert_eq!(read.repos_left_out, None);
    std::fs::write(dir.path().join(".gitignore"), "/charter.local.toml\n").unwrap();
    assert_eq!(Settings::read(dir.path()).plane_left_out, None);
}

#[test]
fn a_left_out_local_file_that_set_a_repo_keeps_the_sentence_for_the_repos_alone() {
    let dir = carried_local("[repos.api]\nsign = true\n");
    let read = Settings::read(dir.path());
    assert!(
        is_the_checks(read.repos_left_out.as_deref()),
        "{:?}",
        read.repos_left_out
    );
    assert_eq!(read.plane_left_out, None);
    std::fs::write(dir.path().join(".gitignore"), "/charter.local.toml\n").unwrap();
    assert_eq!(Settings::read(dir.path()).repos_left_out, None);
}

#[test]
fn a_left_out_local_file_that_set_no_save_setting_says_nothing_about_saving() {
    // `worktrees` is `[plane]`'s, but not a save key.
    let dir = carried_local("[harness]\ndefault = \"claude\"\n[plane]\nworktrees = \"../w\"\n");
    let read = Settings::read(dir.path());
    assert_eq!((read.plane_left_out, read.repos_left_out), (None, None));
}

#[test]
fn the_repos_are_the_inventorys_in_its_order_then_those_only_a_file_names() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("inventory")).unwrap();
    std::fs::write(
        root.join("inventory/repos.json"),
        r#"{"group": "acme", "count": 2, "repos": [{"name": "web"}, {"name": "api"}]}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("charter.toml"),
        "[repos.api]\nmode = \"push\"\n[repos.\"old.lib\"]\nsign = true\n",
    )
    .unwrap();
    std::fs::write(root.join(".gitignore"), "/charter.local.toml\n").unwrap();
    crate::testgit::run(root, &["init", "-q"]);
    std::fs::write(
        root.join("charter.local.toml"),
        "[repos.tools]\nautosave = true\n",
    )
    .unwrap();

    let settings = Settings::read(root);
    assert_eq!(
        settings.repo_names(root),
        ["web", "api", "old.lib", "tools"]
    );
}

#[test]
fn a_plane_with_no_inventory_lists_the_repos_its_files_name() {
    let settings = settings("[repos.api]\nmode = \"push\"\n", "");
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(settings.repo_names(dir.path()), ["api"]);
    assert!(
        Settings::from_text(None, None)
            .repo_names(dir.path())
            .is_empty()
    );
}

#[test]
fn the_save_branch_is_the_one_named_or_this_clones_own() {
    // Two clones on one machine, or two machines with one name, never share a save branch
    // (charter-app#298): the default names the host and the plane's own path.
    let one = tempfile::tempdir().unwrap();
    let two = tempfile::tempdir().unwrap();
    let plane = settings("", "").plane;
    let first = plane.save_branch_or_default(one.path());
    assert!(
        first.starts_with(&format!("charter/save/{}-", crate::dispatch::host())),
        "{first}"
    );
    assert!(branch_ok(&first), "{first}");
    assert_eq!(first, plane.save_branch_or_default(one.path()), "stable");
    assert_ne!(first, plane.save_branch_or_default(two.path()));
    assert_eq!(
        settings("", "[plane]\nsave_branch = \"charter/save/laptop\"\n")
            .plane
            .save_branch_or_default(one.path()),
        "charter/save/laptop"
    );
}

#[test]
fn the_repos_auto_save_can_reach_are_every_table_in_either_file_sorted_and_once() {
    let got = settings(
        "[plane]\nmode = \"push\"\n[repos.widget]\nmode = \"pr\"\n[repos.api]\n",
        "[repos.widget]\nautosave = true\n[repos.docs]\nmode = \"push\"\n",
    )
    .repo_tables();
    assert_eq!(got, ["api", "docs", "widget"]);
    assert!(
        settings("[plane]\nmode = \"push\"\n", "")
            .repo_tables()
            .is_empty()
    );
}
