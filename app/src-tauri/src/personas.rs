//! Making a persona, deleting one, and opening its definition in the operator's editor — as the
//! window reaches them (SI-3).
//!
//! Thin, as `workspaces.rs` is: what a persona may be called, what its scaffold holds, which
//! personas still depend on it and what a removal takes with it all live in
//! `charter_core::personaverbs::define`, which is `charter persona create` and `charter persona
//! remove`. This layer converts. A rule written here as well would be a second rule.
//!
//! **The window never overwrites and never forces.** `create` is asked without `--force`, so a
//! name already defined is refused in the core's words; `remove` is asked without `--force`, so
//! a persona another one `extends:` or `uses:` is refused with the list of who. Getting past
//! either is a terminal's decision, made by someone who typed the flag.
//!
//! **There is no editor here.** A persona's definition is prose the operator writes; the window
//! hands the file to whatever the operating system opens a `.md` file with, from the core side,
//! so the window is never granted a way to open an arbitrary path.

use std::path::{Path, PathBuf};

use charter_core::personaverbs::define;
use charter_core::repocmd::Say;

use crate::planes::{PlaneId, Planes};
use crate::workspaces::ran;

/// Make a persona: `charter persona create <name> [--role …] [--delegate-when …] [--extends …]`,
/// where `parent` is `--extends` (a word TypeScript keeps for itself).
///
/// Empty boxes are flags not given, so the core's defaults apply: the role is the name,
/// title-cased; the vault is the persona's own name. The core refuses a taken name, a name
/// outside the alphabet, a routing line that is missing when nothing is inherited, and any
/// value that would break persona.md's frontmatter — in its own sentences.
// Its plane is a `PlaneId` the registry vouches for, like every other command's
// (charter-app#127). Not a doc comment, because the generated bindings carry those.
#[tauri::command]
#[specta::specta]
pub fn persona_create(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
    role: Option<String>,
    delegate_when: Option<String>,
    parent: Option<String>,
) -> Result<Vec<String>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    create_in(
        &root,
        &name,
        role.as_deref(),
        delegate_when.as_deref(),
        parent.as_deref(),
    )
}

/// A box left empty, or holding only spaces, is a flag that was not given.
fn given(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

/// The creation itself, against a root the registry has already vouched for.
fn create_in(
    root: &Path,
    name: &str,
    role: Option<&str>,
    delegate_when: Option<&str>,
    extends: Option<&str>,
) -> Result<Vec<String>, String> {
    let state = charter_core::personaverbs::state_dir(root);
    let ask = define::Create {
        name: name.trim(),
        role: given(role),
        delegate_when: given(delegate_when),
        vault: None,
        extends: given(extends),
        // `--use` writes a selection that belongs to a terminal or a session; the window's own
        // chats pick their persona in the picker.
        select: None,
        force: false,
    };
    let mut said = Vec::new();
    let code = define::create(root, &state, &ask, None, &mut |line: Say| said.push(line));
    ran(code, said)
}

/// Delete a persona: `charter persona remove <name>`, never forced.
///
/// The core refuses one another persona still `extends:` or `uses:`, naming them. What it
/// deletes is the persona's directory — definition, memory and refs — and its generated agent;
/// its vault is left alone, and the answer says so. When the plane-wide selection
/// (`.charter/active-persona`) names it, that selection goes too, as it does from a terminal
/// that has no session or pane of its own.
#[tauri::command]
#[specta::specta]
pub fn persona_remove(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
) -> Result<Vec<String>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    remove_in(&root, &name)
}

/// The removal itself, against a root the registry has already vouched for.
fn remove_in(root: &Path, name: &str) -> Result<Vec<String>, String> {
    let ids = charter_core::active::Ids::default();
    let selection = charter_core::active::persona(&charter_core::active::Asking {
        root,
        cwd: root,
        flag: None,
        ids: &ids,
        env: None,
    });
    let mut said = Vec::new();
    let code = define::remove(root, name, false, &selection, &mut |line: Say| {
        said.push(line);
    });
    ran(code, said)
}

/// Open a persona's definition in whatever the operating system opens a `.md` file with.
///
/// The path is found and checked here, from the persona's name: the window names a persona,
/// never a file, so no path it sends can be opened.
#[tauri::command]
#[specta::specta]
pub fn persona_edit(
    app: tauri::AppHandle,
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;
    let root = planes.held(&plane)?.root().to_path_buf();
    let file = definition_of(&root, &name)?;
    app.opener()
        .open_path(file.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("the system did not open {}: {e}", file.display()))
}

/// The file that defines `name`, when it is one charter would read: a name a persona can have,
/// a definition that is there, and no link on the way out of the plane. A definition that does
/// not load is still returned — it is the one most in need of an editor.
fn definition_of(root: &Path, name: &str) -> Result<PathBuf, String> {
    if let Some(refused) = charter_core::personas::shape_refusal(name) {
        return Err(refused);
    }
    let file = charter_core::personas::def_path(root, name);
    if !file.exists() {
        return Err(format!("no persona '{name}' on this plane"));
    }
    charter_core::contain::readable(root, &file).map_err(|refused| refused.to_string())?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
        dir
    }

    fn defined(root: &Path, name: &str) -> bool {
        root.join("personas").join(name).join("persona.md").exists()
    }

    #[test]
    fn a_persona_is_created_as_the_cli_creates_one_a_draft_with_its_routing_line() {
        let dir = plane();
        let said = create_in(
            dir.path(),
            "devops",
            Some("DevOps Engineer"),
            Some("CI/CD pipelines, k8s deploys"),
            None,
        )
        .unwrap();

        let text = std::fs::read_to_string(dir.path().join("personas/devops/persona.md")).unwrap();
        assert!(text.contains("role: DevOps Engineer\n"), "{text}");
        assert!(
            text.contains("delegate-when: CI/CD pipelines, k8s deploys\n"),
            "{text}"
        );
        assert!(text.contains("draft: true\n"), "{text}");
        assert!(
            said.iter()
                .any(|line| line.contains("Created persona 'devops'")),
            "{said:?}"
        );
    }

    #[test]
    fn a_persona_with_no_routing_line_and_no_parent_is_refused_in_the_cores_words() {
        let dir = plane();
        let refused = create_in(dir.path(), "qa", None, Some("   "), None).unwrap_err();
        assert!(refused.contains("--delegate-when is required"), "{refused}");
        assert!(!defined(dir.path(), "qa"));
    }

    #[test]
    fn a_name_already_defined_is_refused_and_its_definition_is_left_alone() {
        let dir = plane();
        create_in(dir.path(), "qa", None, Some("tests"), None).unwrap();
        let file = dir.path().join("personas/qa/persona.md");
        std::fs::write(&file, "---\nname: qa\n---\nmine\n").unwrap();

        let refused = create_in(dir.path(), "qa", None, Some("other"), None).unwrap_err();

        assert!(refused.contains("already exists"), "{refused}");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "---\nname: qa\n---\nmine\n"
        );
    }

    #[test]
    fn a_persona_is_removed_with_its_directory() {
        let dir = plane();
        create_in(dir.path(), "qa", None, Some("tests"), None).unwrap();

        let said = remove_in(dir.path(), "qa").unwrap();

        assert!(!dir.path().join("personas/qa").exists());
        assert!(
            said.iter().any(|line| line.contains("left untouched")),
            "{said:?}"
        );
    }

    #[test]
    fn a_persona_another_one_extends_is_refused_and_both_stay() {
        let dir = plane();
        create_in(dir.path(), "base", None, Some("everything"), None).unwrap();
        create_in(dir.path(), "child", None, None, Some("base")).unwrap();

        let refused = remove_in(dir.path(), "base").unwrap_err();

        assert!(refused.contains("child (extends)"), "{refused}");
        assert!(defined(dir.path(), "base") && defined(dir.path(), "child"));
    }

    #[test]
    fn removing_the_plane_wide_selection_clears_it() {
        let dir = plane();
        create_in(dir.path(), "qa", None, Some("tests"), None).unwrap();
        let active = charter_core::active::active_persona_file(dir.path());
        std::fs::create_dir_all(active.parent().unwrap()).unwrap();
        std::fs::write(&active, "qa\n").unwrap();

        remove_in(dir.path(), "qa").unwrap();

        assert!(!active.exists());
    }

    #[test]
    fn the_definition_opened_is_the_personas_own_file_and_nothing_a_name_can_reach() {
        let dir = plane();
        create_in(dir.path(), "qa", None, Some("tests"), None).unwrap();

        assert_eq!(
            definition_of(dir.path(), "qa").unwrap(),
            dir.path().join("personas/qa/persona.md")
        );
        assert!(definition_of(dir.path(), "nobody").is_err());
        assert!(definition_of(dir.path(), "../charter").is_err());
    }
}
