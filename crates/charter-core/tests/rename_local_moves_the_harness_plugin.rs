//! `rename-local` and the harness plugin (RN-8, #1266, V93m), on a temporary home only: an
//! install made before the rename moves to the purlis names for every harness, the old ids never
//! stay beside the new ones, and the undo puts the install back under the old names.
//!
//! Driven through [`renamelocal::run`] and [`renamelocal::undo`] with a [`Local`] whose
//! `plugin` names the temporary home's harness folders and the repository's own plugin, and
//! asserted on the harnesses' files afterwards. Nothing here reads or writes the real `~/.claude`.

use std::path::{Path, PathBuf};

use charter_core::machine;
use charter_core::plugin_install::{self as install, Machine as Harnesses, Verb};
use charter_core::renamelocal::{self, Local, Seams};

struct Home {
    _dir: tempfile::TempDir,
    local: Local,
    binary: PathBuf,
}

impl Home {
    fn harnesses(&self) -> &Harnesses {
        self.local.plugin.as_ref().expect("a plugin to move")
    }

    fn claude(&self, rel: &str) -> PathBuf {
        self.harnesses().claude_config.join(rel)
    }

    fn opencode(&self, file: &str) -> PathBuf {
        self.harnesses().opencode_config.join("plugin").join(file)
    }

    fn settings(&self) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(self.claude("settings.json")).unwrap())
            .unwrap()
    }

    /// The ids the user settings turn on, sorted.
    fn enabled(&self) -> Vec<String> {
        let mut on: Vec<String> = self.settings()["enabledPlugins"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, v)| **v == serde_json::Value::Bool(true))
            .map(|(k, _)| k.clone())
            .collect();
        on.sort();
        on
    }

    /// Where the marketplace `name` says the copy is.
    fn registered(&self, name: &str) -> Option<String> {
        self.settings()["extraKnownMarketplaces"][name]["source"]["path"]
            .as_str()
            .map(str::to_owned)
    }

    fn codex_guard_runs(&self) -> Option<PathBuf> {
        install::adapter("codex").unwrap().runs(self.harnesses())
    }
}

fn nobody_running() -> Seams<'static> {
    Seams {
        busy: &|_, _| None,
        ..Seams::real()
    }
}

/// A home where `charter plugin install` ran before the rename: the copy in the config home's
/// old folder as `charter@charter-app`, the opencode shim as `charter.ts`, the Codex guard
/// running an older charter that is gone.
fn installed_before_the_rename() -> Home {
    let dir = tempfile::tempdir().unwrap();
    let home = std::fs::canonicalize(dir.path()).unwrap();
    let config_root = home.join(".config");
    std::fs::create_dir_all(config_root.join("charter")).unwrap();
    for folder in [".claude", ".codex", ".config/opencode"] {
        std::fs::create_dir_all(home.join(folder)).unwrap();
    }
    let bundle = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../app/src-tauri/plugin")
        .canonicalize()
        .unwrap();
    let binary = home.join("Applications/purlis.app/purlis");
    let harnesses = Harnesses {
        claude_config: home.join(".claude"),
        codex_home: home.join(".codex"),
        opencode_config: home.join(".config/opencode"),
        charter_dir: machine::dir(&config_root),
        binary: binary.clone(),
        bundle: Some(bundle),
    };
    // Installed as an older charter installed it: its binary, then under the old names.
    let mut older = harnesses.clone();
    older.binary = home.join("old/charter");
    let out = install::run(&older, Verb::Install, &[], false);
    assert!(!install::failed(&out), "{}", install::render(&out, false));
    let out = install::move_ids(&older, &install::BEFORE);
    assert!(!install::failed(&out), "{}", install::render(&out, false));

    let home = Home {
        _dir: dir,
        local: Local {
            config_root,
            data_base: None,
            logs: None,
            planes: Vec::new(),
            own_app: None,
            plugin: Some(harnesses),
        },
        binary,
    };
    assert_eq!(home.enabled(), ["charter@charter-app"]);
    assert!(home.opencode("charter.ts").is_file());
    assert!(!home.opencode("purlis.ts").exists());
    home
}

#[test]
fn the_plugin_moves_to_the_purlis_ids_for_every_harness_with_the_config_home() {
    charter_core::unsteered!();
    let home = installed_before_the_rename();
    let config = &home.local.config_root;

    let moved = renamelocal::run(&home.local, &nobody_running());

    assert!(moved.complete && moved.changed, "{:#?}", moved.said);
    assert!(config.join("purlis/plugin").is_dir());
    // Claude Code: the new id, registered where the copy is now, and the old one gone, so the
    // two never load together.
    assert_eq!(home.enabled(), ["purlis@purlis-app"]);
    assert_eq!(
        home.registered("purlis-app").as_deref(),
        Some(config.join("purlis/plugin").to_str().unwrap())
    );
    assert_eq!(home.registered("charter-app"), None);
    assert!(
        home.settings()["enabledPlugins"]
            .get("charter@charter-app")
            .is_none()
    );
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(config.join("purlis/plugin/.claude-plugin/plugin.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["name"], "purlis");
    // opencode: one shim, under its new name.
    assert!(home.opencode("purlis.ts").is_file());
    assert!(!home.opencode("charter.ts").exists());
    // Codex: the guard runs this charter.
    assert_eq!(home.codex_guard_runs(), Some(home.binary.clone()));
    for harness in ["claude", "opencode", "codex"] {
        assert!(
            moved
                .said
                .iter()
                .any(|line| line.starts_with(&format!("✓ the {harness} plugin: "))),
            "{harness}: {:#?}",
            moved.said
        );
    }

    // A second run has nothing left to do for the plugin.
    let again = renamelocal::run(&home.local, &nobody_running());
    assert!(!again.changed, "{:#?}", again.said);
}

#[test]
fn the_undo_puts_the_plugin_back_under_its_old_names_where_the_old_config_home_is() {
    charter_core::unsteered!();
    let home = installed_before_the_rename();
    let config = &home.local.config_root;
    assert!(renamelocal::run(&home.local, &nobody_running()).complete);

    let undone = renamelocal::undo(&home.local, &nobody_running());

    assert!(undone.complete, "{:#?}", undone.said);
    assert!(config.join("charter").is_dir() && !config.join("purlis").exists());
    assert_eq!(home.enabled(), ["charter@charter-app"]);
    assert_eq!(
        home.registered("charter-app").as_deref(),
        Some(config.join("charter/plugin").to_str().unwrap())
    );
    assert_eq!(home.registered("purlis-app"), None);
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(config.join("charter/plugin/.claude-plugin/plugin.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["name"], "charter");
    assert!(home.opencode("charter.ts").is_file());
    assert!(!home.opencode("purlis.ts").exists());
}

#[test]
fn without_a_plugin_to_install_from_the_harnesses_are_left_as_they_are() {
    charter_core::unsteered!();
    let mut home = installed_before_the_rename();
    let settings = std::fs::read(home.claude("settings.json")).unwrap();
    let harnesses = home.local.plugin.take().unwrap();

    let moved = renamelocal::run(&home.local, &nobody_running());

    assert!(moved.complete, "{:#?}", moved.said);
    assert_eq!(
        std::fs::read(harnesses.claude_config.join("settings.json")).unwrap(),
        settings
    );
    assert!(
        harnesses
            .opencode_config
            .join("plugin/charter.ts")
            .is_file()
    );
}

#[test]
fn a_home_with_no_plugin_installed_gets_none_from_the_move() {
    charter_core::unsteered!();
    let home = installed_before_the_rename();
    let out = install::run(home.harnesses(), Verb::Uninstall, &[], false);
    assert!(!install::failed(&out), "{}", install::render(&out, false));
    let settings = std::fs::read(home.claude("settings.json")).unwrap();

    let moved = renamelocal::run(&home.local, &nobody_running());

    assert!(moved.complete, "{:#?}", moved.said);
    assert!(
        moved.said.iter().all(|line| !line.contains(" plugin: ")),
        "{:#?}",
        moved.said
    );
    assert_eq!(
        std::fs::read(home.claude("settings.json")).unwrap(),
        settings
    );
    assert!(!home.opencode("purlis.ts").exists());
    // And nothing for the undo to put back.
    let journal =
        std::fs::read_to_string(machine::dir(&home.local.config_root).join(renamelocal::JOURNAL))
            .unwrap_or_default();
    assert!(!journal.contains("\"plugin\""), "{journal}");
}
