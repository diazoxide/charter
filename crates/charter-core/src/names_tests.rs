use super::*;
use std::fs;

/// A stand-in for "which names exist", so precedence is asked without a filesystem.
fn present(names: &[&str]) -> impl Fn(&str) -> bool {
    let names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    move |n: &str| names.iter().any(|m| m == n)
}

// --------------------------------------------------------------------------------------- //
// precedence (V93e): the purlis name wins; an old name is read only when it is absent      //
// --------------------------------------------------------------------------------------- //

#[test]
fn with_only_the_old_name_present_the_old_name_is_read() {
    let picked = PLANE_MANIFEST.pick(present(&["charter.toml"]));
    assert_eq!(picked.name, "charter.toml");
    assert_eq!(picked.found, Found::Old);
    assert!(picked.leftovers.is_empty());
}

#[test]
fn with_only_the_purlis_name_present_the_purlis_name_is_read() {
    let picked = PLANE_MANIFEST.pick(present(&["purlis.toml"]));
    assert_eq!(picked.name, "purlis.toml");
    assert_eq!(picked.found, Found::Purlis);
    assert!(picked.leftovers.is_empty());
}

#[test]
fn with_both_present_purlis_wins_and_the_old_one_is_a_leftover() {
    let picked = PLANE_MANIFEST.pick(present(&["charter.toml", "purlis.toml"]));
    assert_eq!(picked.name, "purlis.toml");
    assert_eq!(picked.found, Found::Purlis);
    assert_eq!(picked.leftovers, vec!["charter.toml".to_string()]);
}

#[test]
fn with_neither_present_the_answer_is_the_purlis_name_to_write() {
    let picked = PLANE_MANIFEST.pick(present(&[]));
    assert_eq!(picked.name, "purlis.toml");
    assert_eq!(picked.found, Found::Neither);
    assert!(picked.leftovers.is_empty());
}

#[test]
fn a_name_history_keeps_is_still_read_when_nothing_newer_is_there() {
    // `.edm-structure` predates charter; it is recognised forever, after the window's names.
    let picked = STRUCTURE_STAMP.pick(present(&[".edm-structure"]));
    assert_eq!(picked.name, ".edm-structure");
    assert_eq!(picked.found, Found::Old);

    let picked = STRUCTURE_STAMP.pick(present(&[".edm-structure", ".charter-structure"]));
    assert_eq!(picked.name, ".charter-structure");
    assert_eq!(picked.leftovers, vec![".edm-structure".to_string()]);
}

#[test]
fn a_prefix_picks_by_the_whole_name_it_makes() {
    let picked = ENV_PREFIX.pick_with("ROOT", present(&["CHARTER_ROOT"]));
    assert_eq!(picked.name, "CHARTER_ROOT");
    assert_eq!(picked.found, Found::Old);

    let picked = ENV_PREFIX.pick_with("ROOT", present(&["CHARTER_ROOT", "PURLIS_ROOT"]));
    assert_eq!(picked.name, "PURLIS_ROOT");
    assert_eq!(picked.leftovers, vec!["CHARTER_ROOT".to_string()]);
}

// --------------------------------------------------------------------------------------- //
// recognition                                                                               //
// --------------------------------------------------------------------------------------- //

#[test]
fn every_name_an_entry_ever_had_is_recognised_and_nothing_else_is() {
    assert!(BINARY.recognises("purlis"));
    assert!(BINARY.recognises("charter"));
    assert!(BINARY.recognises("edm"));
    assert!(!BINARY.recognises("git"));

    assert!(TRAILER_CHANGE.recognises("Charter-Change"));
    assert!(TRAILER_CHANGE.recognises("Purlis-Change"));
    assert!(!TRAILER_CHANGE.recognises("Charter-Chat"));
}

#[test]
fn a_prefix_recognises_what_starts_with_any_of_its_names() {
    assert_eq!(BRANCH_PREFIX.strip("charter/save/x"), Some("save/x"));
    assert_eq!(BRANCH_PREFIX.strip("purlis/save/x"), Some("save/x"));
    assert_eq!(BRANCH_PREFIX.strip("feature/x"), None);
    assert_eq!(KEYCHAIN_PREFIX.strip("purlis/ops/3f9a"), Some("ops/3f9a"));
    assert_eq!(
        KEYCHAIN_IDENTITY_PREFIX.strip("charter/@identity/ab"),
        Some("/ab")
    );
    assert_eq!(
        crate::secrets::identity::SERVICE_BASE,
        KEYCHAIN_IDENTITY_PREFIX.reads[0]
    );
}

#[test]
fn nothing_writes_an_old_name() {
    for name in ALL {
        assert!(
            name.write.contains("purlis")
                || name.write.contains("Purlis")
                || name.write.contains("PURLIS"),
            "{} writes {:?}",
            name.id,
            name.write
        );
        for old in name.reads.iter().chain(name.history) {
            assert!(
                !old.contains("purlis"),
                "{} reads {old:?} as an old name",
                name.id
            );
            assert_ne!(*old, name.write, "{}", name.id);
        }
    }
}

#[test]
fn every_entry_has_a_unique_id_and_the_kinds_the_spec_names_are_covered() {
    let mut ids: Vec<&str> = ALL.iter().map(|n| n.id).collect();
    ids.sort_unstable();
    let len = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), len, "duplicate id");
    for kind in [
        Kind::File,
        Kind::Folder,
        Kind::Marker,
        Kind::KeychainPrefix,
        Kind::EnvPrefix,
        Kind::BranchPrefix,
        Kind::Trailer,
        Kind::PluginId,
        Kind::BundleId,
        Kind::Binary,
        Kind::ThemeId,
    ] {
        assert!(
            ALL.iter().any(|n| n.kind == kind),
            "no entry of kind {kind:?}"
        );
    }
}

#[test]
fn the_entries_carry_the_names_on_disk_today() {
    // The old names are the literals the code writes today: if one drifts, the window stops
    // reading what existing installs have.
    assert_eq!(PLANE_MANIFEST.reads, ["charter.toml"]);
    assert_eq!(crate::plane::MANIFEST, PLANE_MANIFEST.reads[0]);
    assert_eq!(STATE_DIR.reads, [".charter"]);
    assert_eq!(crate::profiles::LOCAL_FILE, LOCAL_SETTINGS.reads[0]);
    assert_eq!(crate::scanallow::FILE, SCAN_ALLOW.reads[0]);
    assert_eq!(crate::layer::MARKER, GENERATED_SIDECAR.reads[0]);
    assert_eq!(crate::wslayer::STRUCTURE_MARKER, STRUCTURE_STAMP.reads[0]);
    assert_eq!(
        crate::wslayer::LEGACY_STRUCTURE_MARKER,
        STRUCTURE_STAMP.history[0]
    );
    assert_eq!(
        crate::secrets::keyring::SERVICE_PREFIX,
        KEYCHAIN_PREFIX.reads[0]
    );
    assert_eq!(crate::provenance::CHANGE, TRAILER_CHANGE.history[0]);
    assert_eq!(crate::provenance::CHAT, TRAILER_CHAT.history[0]);
    assert_eq!(crate::provenance::PERSONA, TRAILER_PERSONA.history[0]);
    assert_eq!(crate::plugin::LOADED_AS, PLUGIN_LOADED_AS.history[0]);
    assert_eq!(
        crate::plugin_install::INSTALLED_AS,
        PLUGIN_INSTALLED_AS.history[0]
    );
    assert_eq!(
        crate::scaffold::MERGE_RULES_BEGIN,
        MERGE_RULES_BEGIN.reads[0]
    );
    assert_eq!(crate::scaffold::MERGE_RULES_END, MERGE_RULES_END.reads[0]);
    assert_eq!(crate::guest::EXCLUDE_BEGIN, EXCLUDE_BEGIN.reads[0]);
    assert_eq!(crate::guest::EXCLUDE_END, EXCLUDE_END.reads[0]);
    assert_eq!(crate::wscmd::LIVE_BEGIN, LIVE_BEGIN.reads[0]);
    assert_eq!(crate::wscmd::LIVE_END, LIVE_END.reads[0]);
    assert_eq!(
        crate::change::push::BLOCK_BEGIN,
        CHANGE_BLOCK_BEGIN.history[0]
    );
    assert_eq!(crate::forge::pr::MARKER, SAVE_PR_MARKER.history[0]);
    assert_eq!(crate::opencode::MARK, OPENCODE_MARK.reads[0]);
    assert_eq!(crate::rewrite::TEMP_PREFIX, GENERATED_TEMP_PREFIX.reads[0]);
    assert_eq!(
        crate::personaverbs::agents::MARKER,
        SYNC_AGENTS_MARKER.reads[0]
    );
    assert_eq!(crate::roster::BEGIN, PERSONAS_BEGIN.reads[0]);
    assert_eq!(
        crate::secrets::identity::SERVICE_BASE,
        KEYCHAIN_IDENTITY_PREFIX.reads[0]
    );
    // No constant names the state folder alone; the paths under it are spelled whole.
    assert!(crate::cistate::CACHE.starts_with(&format!("{}/", STATE_DIR.reads[0])));
    // Every remaining entry, so no old name in this module is one the code never had.
    assert_eq!(crate::profiles::COMMITTED_FILE, PLANE_MANIFEST.reads[0]);
    assert_eq!(crate::extension::MANIFEST, EXTENSION_MANIFEST.reads[0]);
    assert_eq!(crate::opencode::FILE_NAME, OPENCODE_SHIM.reads[0]);
    assert_eq!(crate::opencode::PYTHON_MARK, OPENCODE_MARK.history[0]);
    assert_eq!(crate::machine::DIR, CONFIG_HOME.reads[0]);
    assert_eq!(crate::datahome::DIR, DATA_HOME.reads[0]);
    assert_eq!(crate::manifest::KEY, GENERATED_KEY.reads[0]);
    assert_eq!(crate::change::push::BLOCK_END, CHANGE_BLOCK_END.history[0]);
    assert_eq!(crate::secrets::onepassword::TAG, ONEPASSWORD_TAG.reads[0]);
    assert_eq!(crate::profiles::CHARTER_PREFIX, ENV_PREFIX.reads[0]);
    assert_eq!(crate::planegit::BRANCH_PREFIX, BRANCH_PREFIX.history[0]);
    assert_eq!(crate::plugin::NAME, PLUGIN_NAME.reads[0]);
    assert_eq!(crate::plugin::FORMERLY, PLUGIN_LOADED_AS.history[1]);
    assert_eq!(
        crate::plugin_install::MARKETPLACE,
        PLUGIN_MARKETPLACE.history[0]
    );
    assert_eq!(
        format!("{}:", crate::plugin::NAME),
        SKILL_NAMESPACE.reads[0]
    );
    assert_eq!(
        format!("mcp__{}__", crate::chattools::SERVER),
        MCP_TOOL_PREFIX.reads[0]
    );
    assert_eq!(crate::applog::APP, BUNDLE_ID.reads[0]);
    assert_eq!(
        crate::leakguard::CHARTER_PROGS,
        [BINARY.reads[0], BINARY.history[0]]
    );
    assert_eq!(
        crate::extension::BUILT_IN_THEMES,
        [THEME_DARK.reads[0], THEME_LIGHT.reads[0]]
    );
    assert_eq!(
        crate::extension::BUILT_IN_ICON_THEMES,
        [ICON_THEME.reads[0]]
    );
}

#[test]
fn every_old_name_an_entry_holds_is_pinned_by_the_test_above() {
    // A new entry must add its line above: count the pins against the old names held.
    let held: usize = ALL.iter().map(|n| n.reads.len() + n.history.len()).sum();
    let source = include_str!("names_tests.rs");
    let body = source
        .split("fn the_entries_carry_the_names_on_disk_today")
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .unwrap();
    let pinned = body.matches(".reads[").count() + body.matches(".history[").count();
    // PLANE_MANIFEST.reads[0] is pinned twice, by `plane::MANIFEST` and `COMMITTED_FILE`.
    assert_eq!(
        pinned - 1,
        held,
        "{held} old names held, {} pinned",
        pinned - 1
    );
}

// --------------------------------------------------------------------------------------- //
// on disk                                                                                   //
// --------------------------------------------------------------------------------------- //

#[test]
fn a_file_is_found_in_a_directory_by_either_name() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert_eq!(PLANE_MANIFEST.file_in(dir).found, Found::Neither);

    fs::write(dir.join("charter.toml"), "").unwrap();
    let at = PLANE_MANIFEST.file_in(dir);
    assert_eq!((at.found, at.name), (Found::Old, dir.join("charter.toml")));

    fs::write(dir.join("purlis.toml"), "").unwrap();
    let at = PLANE_MANIFEST.file_in(dir);
    assert_eq!(
        (at.found, at.name.clone()),
        (Found::Purlis, dir.join("purlis.toml"))
    );
    assert_eq!(at.leftovers, vec![dir.join("charter.toml")]);
}

#[test]
fn a_directory_is_not_a_file_and_a_file_is_not_a_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    fs::create_dir(dir.join("purlis.toml")).unwrap();
    fs::write(dir.join(".purlis"), "").unwrap();
    fs::create_dir(dir.join(".charter")).unwrap();

    assert_eq!(PLANE_MANIFEST.file_in(dir).found, Found::Neither);
    let state = STATE_DIR.dir_in(dir);
    assert_eq!(
        (state.found, state.name),
        (Found::Old, dir.join(".charter"))
    );
}
