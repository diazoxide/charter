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
    // The keychain writes the purlis names and reads both during the window (#1261).
    assert_eq!(
        crate::secrets::identity::SERVICE_BASE,
        KEYCHAIN_IDENTITY_PREFIX.write
    );
    assert_eq!(
        crate::secrets::identity::READ_BASES,
        [
            ("purlis", "purlis/@identity"),
            ("charter", "charter/@identity")
        ]
    );
    for ((_, base), prefix) in crate::secrets::identity::READ_BASES
        .iter()
        .zip(crate::secrets::keyring::OWN_PREFIXES)
    {
        assert_eq!(
            *base,
            format!("{prefix}{}", crate::secrets::identity::OWNER)
        );
    }
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
    // Moved (RN-2b): the code writes the purlis name, and the old one is pinned here.
    assert_eq!(GENERATED_SIDECAR.reads[0], ".charter-generated");
    assert_eq!(crate::layer::MARKER, GENERATED_SIDECAR.write);
    assert_eq!(GENERATED_KEY.reads[0], "charter_generated");
    assert_eq!(STRUCTURE_STAMP.reads[0], ".charter-structure");
    assert_eq!(STRUCTURE_STAMP.history[0], ".edm-structure");
    assert_eq!(crate::wslayer::STRUCTURE_MARKER, STRUCTURE_STAMP.write);
    assert_eq!(
        crate::secrets::keyring::SERVICE_PREFIX,
        KEYCHAIN_PREFIX.write
    );
    assert_eq!(
        crate::secrets::keyring::OWN_PREFIXES,
        ["purlis/", "charter/"]
    );
    assert_eq!(KEYCHAIN_PREFIX.reads, ["charter/"]);
    assert_eq!(ONEPASSWORD_TAG.reads, ["charter"]);
    // Forge and history moved onto the purlis names (RN-2c): the code writes them, and the old
    // ones stay what history holds, recognised forever.
    assert_eq!(crate::provenance::CHANGE, TRAILER_CHANGE.write);
    assert_eq!(crate::change::land::TRAILER, TRAILER_CHANGE.write);
    assert_eq!(TRAILER_CHANGE.history[..], ["Charter-Change"]);
    assert_eq!(crate::provenance::CHAT, TRAILER_CHAT.write);
    assert_eq!(TRAILER_CHAT.history[..], ["Charter-Chat"]);
    assert_eq!(crate::provenance::PERSONA, TRAILER_PERSONA.write);
    assert_eq!(TRAILER_PERSONA.history[..], ["Charter-Persona"]);
    assert_eq!(BRANCH_PREFIX.history[..], ["charter/"]);
    assert_eq!(crate::plugin::LOADED_AS, PLUGIN_LOADED_AS.history[0]);
    assert_eq!(
        crate::plugin_install::INSTALLED_AS,
        PLUGIN_INSTALLED_AS.history[0]
    );
    assert_eq!(
        MERGE_RULES_BEGIN.reads[0],
        "# >>> charter merge rules (managed by charter) >>>"
    );
    assert_eq!(MERGE_RULES_END.reads[0], "# <<< charter merge rules <<<");
    assert_eq!(
        EXCLUDE_BEGIN.reads[0],
        "# >>> charter (generated layer — `charter workspace reinit`) >>>"
    );
    assert_eq!(EXCLUDE_END.reads[0], "# <<< charter <<<");
    assert_eq!(crate::guest::EXCLUDE_BEGIN, EXCLUDE_BEGIN.write);
    assert_eq!(
        LIVE_BEGIN.reads[0],
        "# >>> charter live workspaces (managed by `charter workspace live`) >>>"
    );
    assert_eq!(LIVE_END.reads[0], "# <<< charter live workspaces <<<");
    assert_eq!(crate::change::push::BLOCK_BEGIN, CHANGE_BLOCK_BEGIN.write);
    assert_eq!(crate::change::push::BLOCK_END, CHANGE_BLOCK_END.write);
    assert_eq!(
        CHANGE_BLOCK_BEGIN.history[..],
        [
            "<!-- BEGIN charter change — GENERATED by `charter change push`; do not edit by hand. -->"
        ]
    );
    assert_eq!(CHANGE_BLOCK_END.history[..], ["<!-- END charter change -->"]);
    assert_eq!(crate::forge::pr::MARKER, SAVE_PR_MARKER.write);
    assert_eq!(SAVE_PR_MARKER.history[..], ["<!-- charter-save -->"]);
    assert_eq!(
        OPENCODE_MARK.reads[0],
        "// charter's opencode plugin, generated by charter. Do not edit."
    );
    assert_eq!(crate::opencode::MARK, OPENCODE_MARK.write);
    assert_eq!(crate::opencode::PYTHON_MARK, OPENCODE_MARK.history[0]);
    assert_eq!(GENERATED_TEMP_PREFIX.reads[0], ".charter-generated.");
    assert_eq!(crate::rewrite::TEMP_PREFIX, GENERATED_TEMP_PREFIX.write);
    assert_eq!(
        SYNC_AGENTS_MARKER.reads[0],
        "GENERATED by `charter persona sync-agents`"
    );
    assert_eq!(
        crate::personaverbs::agents::MARKER,
        SYNC_AGENTS_MARKER.write
    );
    assert_eq!(
        PERSONAS_BEGIN.reads[0],
        "<!-- BEGIN personas — GENERATED by `charter docs`; do not edit by hand. -->"
    );
    // The keychain and 1Password names RN-4 moved to their purlis spelling are still read
    // under the old one, through these constants.
    assert_eq!(
        crate::secrets::keyring::OWN_PREFIXES[1],
        KEYCHAIN_PREFIX.reads[0]
    );
    assert_eq!(
        crate::secrets::identity::READ_BASES[1].1,
        KEYCHAIN_IDENTITY_PREFIX.reads[0]
    );
    assert_eq!(
        crate::secrets::onepassword::OLD_TAG,
        ONEPASSWORD_TAG.reads[0]
    );
    // No constant names the state folder alone; the paths under it are spelled whole.
    assert!(crate::cistate::CACHE.starts_with(&format!("{}/", STATE_DIR.reads[0])));
    // Every remaining entry, so no old name in this module is one the code never had.
    assert_eq!(crate::profiles::COMMITTED_FILE, PLANE_MANIFEST.reads[0]);
    assert_eq!(crate::extension::MANIFEST, EXTENSION_MANIFEST.reads[0]);
    assert_eq!(crate::opencode::FILE_NAME, OPENCODE_SHIM.reads[0]);
    assert_eq!(crate::machine::DIR, CONFIG_HOME.reads[0]);
    assert_eq!(crate::datahome::DIR, DATA_HOME.reads[0]);
    assert_eq!(crate::secrets::onepassword::TAG, ONEPASSWORD_TAG.write);
    // The old prefix of every environment variable, which chats and shells an older build
    // started still carry (RN-2d, `crate::envvar`).
    assert_eq!("CHARTER_", ENV_PREFIX.reads[0]);
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

#[test]
fn every_variable_the_product_sets_is_written_under_the_purlis_prefix() {
    // RN-2d (V93k): the product writes `PURLIS_<X>` and reads `CHARTER_<X>` only as the
    // fallback ([`crate::envvar`]). The plugin's hook text keeps its one old name until the
    // plugin is renamed; a chat is given both names, so either reads.
    for written in [
        crate::hookwire::SOCKET_ENV,
        crate::hookwire::CHAT_ENV,
        crate::hookwire::TOKEN_ENV,
        crate::hookwire::HARNESS_ENV,
        crate::skills::LISTED_ENV,
        crate::start::FOOTER_ENV,
        crate::chatenv::SESSION_BUS_KEPT,
        crate::datahome::HOME_VAR,
        crate::machine::HOME_VAR,
        crate::active::WORKSPACE_ENV,
        crate::active::PLANE_ROOT_ENV,
        crate::active::SESSION_ID_ENV,
        crate::active::PERSONA_ENV,
        crate::noterminal::RELAUNCHED_ENV,
        crate::sessionrecord::RESUMING_ENV,
        crate::shellguard::USER_ZDOTDIR_ENV,
        crate::glstate::NO_BACKGROUND_CHECKS,
    ] {
        assert!(written.starts_with(ENV_PREFIX.write), "{written}");
    }
    assert_eq!(
        Some(crate::plugin::BINARY_ENV),
        ENV_PREFIX
            .reads
            .first()
            .map(|old| format!("{old}HOOK_BINARY"))
            .as_deref()
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

// --------------------------------------------------------------------------------------- //
// committed files: old spelling until the plane is migrated (V93g, D-RN2b-9)               //
// --------------------------------------------------------------------------------------- //

#[test]
fn a_plane_writes_charters_markers_until_it_is_migrated() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    fs::write(dir.join("charter.toml"), "schema = 1\n").unwrap();
    assert!(!plane_migrated(dir));
    assert_eq!(
        LIVE_END.writes_for(dir),
        "# <<< charter live workspaces <<<"
    );

    fs::write(
        dir.join("charter.toml"),
        "schema = 2\nrequires = [\"purlis-names\"]\n",
    )
    .unwrap();
    assert!(plane_migrated(dir), "the feature migrates it");
    assert_eq!(LIVE_END.writes_for(dir), "# <<< purlis live workspaces <<<");

    fs::write(dir.join("charter.toml"), "schema = 1\n").unwrap();
    fs::write(dir.join("purlis.toml"), "schema = 1\n").unwrap();
    assert!(plane_migrated(dir), "so does the purlis manifest");
    assert_eq!(LIVE_END.writes_for(dir), LIVE_END.write);
}
