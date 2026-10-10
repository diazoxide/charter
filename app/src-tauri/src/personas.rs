//! Making a persona, deleting one, and opening its definition in the operator's editor — as the
//! window reaches them (SI-3).
//!
//! Thin, as `workspaces.rs` is: what a persona may be called, what its scaffold holds, which
//! personas still depend on it and what a removal takes with it all live in
//! `purlis_core::personaverbs::define`, which is `charter persona create` and `charter persona
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

use purlis_core::personaverbs::define;
use purlis_core::repocmd::Say;

use crate::planes::{PlaneId, Planes};
use crate::workspaces::ran;

/// What a persona command says when its blocking thread ended without an answer. Each one runs
/// on such a thread: it writes the persona's files, and then takes the plane model's lock,
/// which a change nobody could name holds while it reads every workspace again (SC-2, FD-10c).
const WRITING: &str = "writing the persona";

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
pub async fn persona_create(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
    role: Option<String>,
    delegate_when: Option<String>,
    parent: Option<String>,
) -> Result<Vec<String>, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window(WRITING, move || {
        let said = create_in(
            held.root(),
            &name,
            role.as_deref(),
            delegate_when.as_deref(),
            parent.as_deref(),
        );
        // A name that was just taken was no persona's a moment ago: a grant made for one chat
        // that still names it was an earlier persona's, and the new one inherits none (#1504).
        if said.is_ok() {
            held.dispatch_grants().persona_gone(name.trim());
        }
        // Told whether or not it was made: a refusal moved nothing, and listing the personas
        // again costs one directory read.
        held.wrote(&["personas".to_owned()]);
        said
    })
    .await
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
    let code = define::create(root, &ask, None, &mut |line: Say| said.push(line));
    ran(code, said)
}

/// Delete a persona: `charter persona remove <name>`, never forced.
///
/// The core refuses one another persona still `extends:` or `uses:`, naming them. What it
/// deletes is the persona's directory (definition, memory and refs) and, in a project that
/// still has it, the helper file purlis once generated for it. Its vault is left alone, and
/// the answer says so. When the plane-wide selection (`.charter/active-persona`) names it,
/// that selection goes too, as it does from a terminal that has no session or pane of its own.
#[tauri::command]
#[specta::specta]
pub async fn persona_remove(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
) -> Result<Vec<String>, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window(WRITING, move || {
        let said = remove_in(held.root(), &name);
        // A removed persona's grants are in force for no chat (#1504): the core marked the
        // name gone, and the grants made for one chat that name it are this app's to end.
        if said.is_ok() {
            held.dispatch_grants().persona_gone(&name);
        }
        held.wrote(&["personas".to_owned()]);
        said
    })
    .await
}

/// The removal itself, against a root the registry has already vouched for.
fn remove_in(root: &Path, name: &str) -> Result<Vec<String>, String> {
    let ids = purlis_core::active::Ids::default();
    let selection = purlis_core::active::persona(&purlis_core::active::Asking {
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

/// A persona's custom image as it crosses to the window: bytes and what they are, never a
/// path or a URL. The window decodes them onto a canvas, as a file tab's image preview does.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct PersonaImage {
    /// `image/png`.
    pub mime: String,
    pub base64: String,
}

/// What a persona is drawn with, wherever it appears (#1449): a built-in icon's name, a
/// colour, and a custom image when its folder holds one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct PersonaMark {
    pub name: String,
    /// One of the icons the window ships for a persona, by name, or `null`.
    pub icon: Option<String>,
    /// A palette name or `#rrggbb`, as a workspace's colour is, or `null`.
    pub colour: Option<String>,
    /// The custom image, or `null`.
    pub image: Option<PersonaImage>,
    /// Why something the persona asked for is not drawn, each a sentence its view shows.
    pub trouble: Vec<String>,
}

/// Every persona's mark: its `icon:` and `color:`, and the `icon.png` in its folder. A persona with none of them is listed with nothing, and the window draws its
/// initials.
#[tauri::command]
#[specta::specta]
pub async fn persona_marks(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
) -> Result<Vec<PersonaMark>, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading the personas' icons", move || marks_in(held.root())).await
}

/// The marks themselves, against a root the registry has already vouched for.
fn marks_in(root: &Path) -> Result<Vec<PersonaMark>, String> {
    let personas = purlis_core::workspaces::Plane::open(root)
        .personas()
        .map_err(|why| why.to_string())?;
    Ok(personas
        .into_iter()
        .map(|name| {
            let mark = purlis_core::personamark::mark(root, &name);
            PersonaMark {
                name,
                icon: mark.icon.map(str::to_owned),
                colour: mark.colour,
                image: mark.image.map(|image| PersonaImage {
                    mime: image.media.to_owned(),
                    base64: image.base64(),
                }),
                trouble: mark.trouble,
            }
        })
        .collect())
}

/// Write a persona's icon and colour into its definition. `null` takes the key out, so the
/// persona goes back to what it inherits, or to its initials.
#[tauri::command]
#[specta::specta]
pub async fn persona_mark_set(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
    icon: Option<String>,
    colour: Option<String>,
) -> Result<(), String> {
    let held = planes.held(&plane)?;
    crate::off_the_window(WRITING, move || {
        let said =
            purlis_core::personamark::set(held.root(), &name, icon.as_deref(), colour.as_deref());
        held.wrote(&["personas".to_owned()]);
        said
    })
    .await
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

/// A persona's profile, as the persona view's control draws it (#1445).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct PersonaProfile {
    /// The profile the definition names with `profile:`, as written, or none. It is shown
    /// even where the project does not offer it, so the control can say so.
    pub named: Option<String>,
    /// The persona it inherits that profile from (`extends:`), where the line is not its own.
    /// Picking none then writes `profile: none` in its own definition.
    pub inherited_from: Option<String>,
    /// Every profile the project offers on this machine, by name: what may be picked.
    pub offered: Vec<String>,
}

/// The profile a persona's chats start on, and the project's profiles it may be set to.
///
/// Off the window's thread: reading the profiles asks git whether the local file would travel.
#[tauri::command]
#[specta::specta]
pub async fn persona_profile(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
) -> Result<PersonaProfile, String> {
    let held = planes.held(&plane)?;
    crate::off_the_window("reading the persona's profile", move || {
        profile_in(held.root(), &name)
    })
    .await
}

fn profile_in(root: &Path, name: &str) -> Result<PersonaProfile, String> {
    use purlis_core::personaprofile;
    if let Some(refused) = purlis_core::personas::name_refusal(root, name) {
        return Err(refused);
    }
    let named = personaprofile::named_by(root, name);
    Ok(PersonaProfile {
        named: named.profile,
        inherited_from: named.inherited_from,
        offered: personaprofile::offers(root)
            .into_iter()
            .map(|offer| offer.name)
            .collect(),
    })
}

/// Set the profile a persona's chats start on, or name none: one `profile:` line in the
/// persona's own definition (`purlis_core::personaprofile::set`). None removes the line, or
/// writes `profile: none` where the persona would otherwise inherit one.
///
/// The core refuses a profile the project does not offer on this machine. The window sends a
/// name it listed; a name it did not is refused all the same, because the line is read back as
/// what a chat starts on.
#[tauri::command]
#[specta::specta]
pub async fn persona_set_profile(
    planes: tauri::State<'_, Planes>,
    plane: PlaneId,
    name: String,
    profile: Option<String>,
) -> Result<(), String> {
    let held = planes.held(&plane)?;
    crate::off_the_window(WRITING, move || {
        let set = purlis_core::personaprofile::set(held.root(), &name, profile.as_deref());
        held.wrote(&["personas".to_owned()]);
        set
    })
    .await
}

/// The file that defines `name`, when it is one charter would read: a name a persona can have,
/// a definition that is there, and no link on the way out of the plane. A definition that does
/// not load is still returned — it is the one most in need of an editor.
fn definition_of(root: &Path, name: &str) -> Result<PathBuf, String> {
    if let Some(refused) = purlis_core::personas::shape_refusal(name) {
        return Err(refused);
    }
    let file = purlis_core::personas::def_path(root, name);
    if !file.exists() {
        return Err(format!("no persona '{name}' in this project"));
    }
    purlis_core::contain::readable(root, &file).map_err(|refused| refused.to_string())?;
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
        let active = purlis_core::active::active_persona_file(dir.path());
        std::fs::create_dir_all(active.parent().unwrap()).unwrap();
        std::fs::write(&active, "qa\n").unwrap();

        remove_in(dir.path(), "qa").unwrap();

        assert!(!active.exists());
    }

    #[test]
    fn a_personas_profile_is_read_and_set_among_the_profiles_the_project_offers() {
        let dir = plane();
        create_in(dir.path(), "qa", None, Some("tests"), None).unwrap();

        let before = profile_in(dir.path(), "qa").unwrap();
        assert_eq!(before.named, None);
        assert!(
            before.offered.iter().any(|name| name == "codex"),
            "the built-ins are offered: {:?}",
            before.offered
        );

        purlis_core::personaprofile::set(dir.path(), "qa", Some("codex")).unwrap();
        let after = profile_in(dir.path(), "qa").unwrap();
        assert_eq!(after.named.as_deref(), Some("codex"));

        let refused =
            purlis_core::personaprofile::set(dir.path(), "qa", Some("sh -c evil")).unwrap_err();
        assert!(refused.contains("does not offer"), "{refused}");
        assert_eq!(
            profile_in(dir.path(), "qa").unwrap().named.as_deref(),
            Some("codex")
        );

        // A child is told whose profile it has, and none on it means none (M2).
        create_in(dir.path(), "junior", None, None, Some("qa")).unwrap();
        let inherited = profile_in(dir.path(), "junior").unwrap();
        assert_eq!(inherited.named.as_deref(), Some("codex"));
        assert_eq!(inherited.inherited_from.as_deref(), Some("qa"));
        purlis_core::personaprofile::set(dir.path(), "junior", None).unwrap();
        let out = profile_in(dir.path(), "junior").unwrap();
        assert_eq!((out.named, out.inherited_from), (None, None));
        assert!(profile_in(dir.path(), "nobody").is_err());
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

    #[test]
    fn every_persona_is_listed_with_its_mark_and_an_image_crosses_as_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let persona = |name: &str, text: &str| {
            let at = dir.path().join("personas").join(name);
            std::fs::create_dir_all(&at).unwrap();
            std::fs::write(at.join("persona.md"), text).unwrap();
            at
        };
        let devops = persona("devops", "---\nicon: rocket\ncolor: teal\n---\n");
        std::fs::write(devops.join("icon.png"), b"\x89PNG\r\n\x1a\n").unwrap();
        persona("qa", "---\nrole: QA\n---\n");

        let marks = marks_in(dir.path()).unwrap();

        assert_eq!(
            marks,
            vec![
                PersonaMark {
                    name: "devops".into(),
                    icon: Some("rocket".into()),
                    colour: Some("teal".into()),
                    image: Some(PersonaImage {
                        mime: "image/png".into(),
                        base64: "iVBORw0KGgo=".into(),
                    }),
                    trouble: vec![],
                },
                PersonaMark {
                    name: "qa".into(),
                    icon: None,
                    colour: None,
                    image: None,
                    trouble: vec![],
                },
            ]
        );
    }
}
