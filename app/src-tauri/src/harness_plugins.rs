//! A project's harness plugins, as the Project settings tab asks about them (charter-app#274).
//!
//! `charter_core::harness_plugin` does the thinking: the adapters, the precedence, the pins. This
//! file is the wire: one group per harness, every one of them, including a harness whose adapter
//! cannot apply, which carries the sentence that says so.

use charter_core::harness_plugin;

/// One harness's plugins in one project.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct HarnessPlugins {
    /// The plane's word for the harness (`claude`, `opencode`, `codex`): the key under
    /// `[harness_plugins]`.
    pub harness: String,
    /// What a person calls it.
    pub title: String,
    /// "plugins for <harness> are not supported yet — <why>", or none where charter applies
    /// a project's choice to the chats it starts.
    pub unsupported: Option<String>,
    /// Where charter read what it has installed: the file or directory, as this app's own
    /// environment names it. A profile that points the harness elsewhere is listed against its
    /// own directory when its chat starts.
    pub record: Option<String>,
    /// Why the harness's own record of what it installed could not be read, if it could not.
    pub trouble: Option<String>,
    pub plugins: Vec<HarnessPlugin>,
    /// The ignore check's sentence while git would carry `charter.local.toml` and it names a
    /// plugin of this harness — the one the Project settings tab's Local section says — so this
    /// group says why a plugin set there is not applied (charter-app#319). The core's
    /// `Choices::local_left_out`, asked for this harness.
    pub local_left_out: Option<String>,
}

/// One plugin, in one project.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct HarnessPlugin {
    /// The harness's own id for it.
    pub id: String,
    pub name: String,
    /// Where charter found it installed; empty when this machine has not installed it.
    pub origin: String,
    /// `on`, `off` or `not-set`.
    pub state: String,
    /// `default`, `shared`, `workspace` or `local`: which layer decided `state`.
    pub source: String,
    pub installed: bool,
    /// Why charter fixes it whatever a file says, in the core's words ("<id> is always on: …"),
    /// or none for a plugin a project may choose. No control is drawn for a fixed one.
    pub pinned: Option<String>,
    /// Each value a file set that charter did not use, and why.
    pub ignored: Vec<crate::extensions::ProjectExtensionIgnored>,
}

/// Every harness charter knows, with what it has installed on this machine and what this
/// project has each plugin at — in `workspace`, when one is named, with that workspace's
/// settings as the layer between Shared and Local (charter-app#282): what the Workspace settings
/// tab shows. Read from the harness's own files, never written; asked when the tab opens and
/// after it saves.
#[tauri::command]
#[specta::specta]
pub async fn project_harness_plugins(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
    workspace: Option<String>,
) -> Result<Vec<HarnessPlugins>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let choices = harness_plugin::Choices::read_in(&root, workspace.as_deref());
        groups(
            harness_plugin::survey(&choices, &harness_plugin::Env::of(&[])),
            &choices,
        )
    })
    .await
    .map_err(|err| format!("reading this project's harness plugins did not finish: {err}"))
}

/// [`project_harness_plugins`] without a runtime.
fn groups(
    survey: Vec<harness_plugin::Group>,
    choices: &harness_plugin::Choices,
) -> Vec<HarnessPlugins> {
    survey
        .into_iter()
        .map(|group| HarnessPlugins {
            harness: group.adapter.harness().to_owned(),
            title: group.adapter.title().to_owned(),
            unsupported: harness_plugin::not_supported(group.adapter),
            record: group.record.map(|path| path.display().to_string()),
            trouble: group.trouble,
            plugins: group
                .plugins
                .into_iter()
                .map(|it| HarnessPlugin {
                    state: match it.wanted {
                        Some(true) => "on",
                        Some(false) => "off",
                        None => "not-set",
                    }
                    .to_owned(),
                    source: it.source.as_str().to_owned(),
                    id: it.id,
                    name: it.name,
                    origin: it.origin,
                    installed: it.installed,
                    pinned: it.pinned.map(str::to_owned),
                    ignored: it
                        .ignored
                        .into_iter()
                        .map(|one| crate::extensions::ProjectExtensionIgnored {
                            file: crate::extensions::file_of(one.source, choices.workspace_file()),
                            why: one.why,
                        })
                        .collect(),
                })
                .collect(),
            local_left_out: choices
                .local_left_out(group.adapter.harness())
                .map(str::to_owned),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_harness_is_a_group_and_one_that_cannot_apply_says_so() {
        let plane = tempfile::tempdir().expect("a plane");
        std::fs::write(
            plane.path().join("charter.toml"),
            "[harness_plugins.claude]\n\"figma@official\" = false\n",
        )
        .expect("charter.toml");
        let claude = tempfile::tempdir().expect("a Claude Code config dir");
        std::fs::create_dir_all(claude.path().join("plugins")).expect("plugins");
        std::fs::write(
            claude.path().join("plugins/installed_plugins.json"),
            r#"{"version": 2, "plugins": {"figma@official": [{"scope": "user"}]}}"#,
        )
        .expect("the install record");
        let empty = tempfile::tempdir().expect("an empty home");
        let chat = [(
            "CLAUDE_CONFIG_DIR".to_owned(),
            claude.path().display().to_string(),
        )];
        let env = harness_plugin::Env {
            chat: &chat,
            home: Some(empty.path().to_path_buf()),
            process: false,
        };

        let choices = harness_plugin::Choices::read(plane.path());
        let got = groups(harness_plugin::survey(&choices, &env), &choices);

        let titles: Vec<(&str, bool)> = got
            .iter()
            .map(|it| (it.title.as_str(), it.unsupported.is_some()))
            .collect();
        assert_eq!(
            titles,
            [("Claude Code", false), ("opencode", true), ("Codex", true)]
        );
        let figma = got[0]
            .plugins
            .iter()
            .find(|it| it.id == "figma@official")
            .expect("listed");
        assert_eq!(
            (
                figma.state.as_str(),
                figma.source.as_str(),
                figma.pinned.is_none()
            ),
            ("off", "shared", true)
        );
        let own = got[0]
            .plugins
            .iter()
            .find(|it| it.id == "charter@inline")
            .expect("the pin is listed");
        assert_eq!(own.state, "on");
        assert!(
            own.pinned
                .as_deref()
                .is_some_and(|why| why.starts_with("charter@inline is always on")),
            "{:?}",
            own.pinned
        );
        assert!(
            got[2]
                .unsupported
                .as_deref()
                .is_some_and(|said| said.starts_with("plugins for Codex are not supported yet")),
            "{:?}",
            got[2].unsupported
        );
    }

    #[test]
    fn in_a_workspace_a_plugin_says_the_workspace_decided_it_and_names_its_manifest() {
        // charter-app#282: the Workspace settings tab asks with its workspace, and each plugin
        // says which layer decided it; a value the workspace set and charter ignored names the
        // workspace's own file, so the tab draws it under that section.
        let plane = tempfile::tempdir().expect("a plane");
        let ws = plane.path().join("workspaces/alpha");
        std::fs::create_dir_all(&ws).expect("the workspace");
        std::fs::write(
            ws.join("workspace.json"),
            r#"{"settings": {"harness_plugins": {"claude": {"figma@official": false, "charter@inline": false}}}}"#,
        )
        .expect("workspace.json");
        let empty = tempfile::tempdir().expect("an empty home");
        let env = harness_plugin::Env {
            chat: &[],
            home: Some(empty.path().to_path_buf()),
            process: false,
        };
        let choices = harness_plugin::Choices::read_in(plane.path(), Some("alpha"));

        let got = groups(harness_plugin::survey(&choices, &env), &choices);

        let claude = &got[0].plugins;
        let figma = claude
            .iter()
            .find(|it| it.id == "figma@official")
            .expect("listed");
        assert_eq!(
            (figma.state.as_str(), figma.source.as_str()),
            ("off", "workspace")
        );
        let own = claude
            .iter()
            .find(|it| it.id == "charter@inline")
            .expect("the pin is listed");
        assert_eq!(own.ignored.len(), 1, "{:?}", own.ignored);
        assert_eq!(own.ignored[0].file, "workspaces/alpha/workspace.json");
        assert_eq!(
            got[0].local_left_out, None,
            "there is no local file to leave out"
        );
    }

    #[test]
    fn the_group_of_a_harness_the_left_out_local_file_names_says_why_in_project_and_workspace() {
        // charter-app#319: each harness is a group of its own in the settings tabs, and the one
        // whose plugins the Local file set carries the ignore check's sentence — the Local
        // section's — while git would carry the file, so a plugin Local turned off and still on
        // says why. A harness the file does not name has nothing that was not applied.
        let plane = crate::extensions::test_plane(
            "",
            "[harness_plugins.claude]\n\"figma@official\" = false\n",
            false,
        );
        let root = plane.path();
        let why = charter_core::profiles::ignore_check(root).reason;
        assert!(why.contains("charter reads nothing in it"), "{why}");
        let empty = tempfile::tempdir().expect("an empty home");
        let env = harness_plugin::Env {
            chat: &[],
            home: Some(empty.path().to_path_buf()),
            process: false,
        };

        for workspace in [None, Some("alpha")] {
            let choices = harness_plugin::Choices::read_in(root, workspace);
            let got = groups(harness_plugin::survey(&choices, &env), &choices);

            let said: Vec<(&str, Option<&str>)> = got
                .iter()
                .map(|group| (group.harness.as_str(), group.local_left_out.as_deref()))
                .collect();
            assert_eq!(
                said,
                [
                    ("claude", Some(why.as_str())),
                    ("opencode", None),
                    ("codex", None)
                ],
                "{workspace:?}"
            );
        }
    }
}
