use std::path::{Path, PathBuf};

use super::*;

/// The skills directory the app ships.
fn bundled() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/plugin/skills")
}

fn skill(dir: &Path, folder: &str, text: &str) {
    std::fs::create_dir_all(dir.join(folder)).unwrap();
    std::fs::write(dir.join(folder).join("SKILL.md"), text).unwrap();
}

#[test]
fn every_bundled_skill_is_read_with_its_name_description_and_file() {
    let skills = read(&bundled());
    let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    for name in [
        "safe-remove",
        "compact",
        "add-curation-action",
        "handoff",
        "secrets",
        "smart-close",
    ] {
        assert!(names.contains(&name), "{name} is missing from {names:?}");
    }
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "listed by name");
    // Every folder in the bundle is a skill a harness would load: none is dropped.
    let folders = std::fs::read_dir(bundled()).unwrap().count();
    assert_eq!(skills.len(), folders, "{names:?}");
    let safe = skills.iter().find(|s| s.name == "safe-remove").unwrap();
    assert!(
        safe.description
            .starts_with("Remove a charter workspace or persona")
    );
    assert_eq!(safe.path, bundled().join("safe-remove/SKILL.md"));
}

#[test]
fn a_folder_that_is_not_a_whole_skill_is_left_out() {
    let dir = tempfile::tempdir().unwrap();
    skill(
        dir.path(),
        "whole",
        "---\nname: whole\ndescription: Does it.\n---\n\nbody\n",
    );
    skill(dir.path(), "nameless", "---\ndescription: Does it.\n---\n");
    skill(dir.path(), "undescribed", "---\nname: undescribed\n---\n");
    skill(dir.path(), "no-frontmatter", "# just a page\n");
    std::fs::create_dir_all(dir.path().join("empty")).unwrap();
    std::fs::write(
        dir.path().join("stray.md"),
        "---\nname: stray\ndescription: x\n---\n",
    )
    .unwrap();

    let names: Vec<String> = read(dir.path()).into_iter().map(|s| s.name).collect();

    assert_eq!(names, ["whole"]);
}

#[test]
fn a_directory_that_is_not_there_has_no_skills() {
    assert!(read(Path::new("/nonexistent/charter/skills")).is_empty());
    assert_eq!(in_bundle(Path::new("/nonexistent/charter/plugin")), None);
}

#[test]
fn the_bundle_holds_its_skills_in_its_own_skills_folder() {
    let plugin = bundled().parent().unwrap().to_path_buf();
    assert_eq!(in_bundle(&plugin), Some(plugin.join("skills")));
}

#[test]
fn the_listing_names_each_skill_when_to_use_it_and_where_to_read_it() {
    let skills = vec![
        Skill {
            name: "compact".to_owned(),
            description: "Compact a memory. Use when asked to tidy.".to_owned(),
            path: PathBuf::from("/app/plugin/skills/compact/SKILL.md"),
        },
        Skill {
            name: "safe-remove".to_owned(),
            description: "Remove a workspace safely.".to_owned(),
            path: PathBuf::from("/app/plugin/skills/safe-remove/SKILL.md"),
        },
    ];

    let text = listing(&skills).expect("a listing");

    assert!(text.starts_with("⬢ **charter's skills**"), "{text}");
    // The model is told what a skill is here: a file to read and follow, by its name.
    assert!(text.contains("read its `SKILL.md`"), "{text}");
    assert!(text.contains(
        "- `compact` — Compact a memory. Use when asked to tidy. \
         (`/app/plugin/skills/compact/SKILL.md`)"
    ));
    assert!(text.ends_with(
        "- `safe-remove` — Remove a workspace safely. \
         (`/app/plugin/skills/safe-remove/SKILL.md`)"
    ));
}

#[test]
fn no_skills_is_no_listing_not_an_empty_one() {
    assert_eq!(listing(&[]), None);
}

#[test]
fn only_a_chat_started_with_the_variable_is_briefed_on_the_skills() {
    let unset = |_: &str| None;
    assert_eq!(listed_from(&unset), None);
    let empty = |name: &str| (name == LISTED_ENV).then(String::new);
    assert_eq!(listed_from(&empty), None);

    let dir = bundled().display().to_string();
    let set = move |name: &str| (name == LISTED_ENV).then(|| dir.clone());
    let text = listed_from(&set).expect("listed");

    assert!(text.contains("`safe-remove`"), "{text}");
    assert!(text.contains(&bundled().join("safe-remove/SKILL.md").display().to_string()));
}
