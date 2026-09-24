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
fn a_repo_nobody_configured_saves_through_a_pr_and_never_by_itself() {
    // Code is not a database: pushing someone's half-finished change to main by default is
    // the one irreversible mistake the design could make (ADR 0051).
    let got = settings("[plane]\nmode = \"push\"\nautosave = true\n", "").repo("charter-app");
    assert_eq!(
        got.mode,
        Resolved {
            value: Mode::Pr,
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
