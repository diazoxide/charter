use std::fs;
use std::path::Path;

use super::*;
use crate::manifest::{self, Ownership};
use crate::settings::{Edit, Found, Step, Value};

/// A plane with the workspace `alpha`, whose `workspace.json` is `manifest` when there is one.
fn plane(manifest: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    let ws = dir.path().join("workspaces/alpha");
    fs::create_dir_all(&ws).unwrap();
    if let Some(text) = manifest {
        fs::write(ws.join("workspace.json"), text).unwrap();
    }
    dir
}

fn on_disk(root: &Path) -> String {
    fs::read_to_string(root.join("workspaces/alpha/workspace.json")).unwrap()
}

/// A manifest charter wrote: stamped with the digest of the rest.
fn charters(doc: serde_json::Value) -> String {
    let mut doc = doc;
    let digest = manifest::digest(&doc);
    doc[manifest::KEY] = serde_json::Value::String(digest);
    crate::pyjson::dumps_indent2(&doc)
}

fn old() -> String {
    charters(serde_json::json!({
        "name": "alpha",
        "description": "",
        "repos": [{"name": "widget"}],
        "updated_at": "2026-09-17T14:01:35+00:00",
        "updated_by": "me",
    }))
}

fn at(path: &[&str], value: Option<Value>) -> Edit {
    Edit {
        path: path.iter().map(|s| Step::Key((*s).to_owned())).collect(),
        value,
    }
}

fn off() -> Edit {
    at(
        &["extensions", "stats", "enabled"],
        Some(Value::Bool(false)),
    )
}

#[test]
fn an_old_workspace_json_reads_with_no_settings_and_nothing_to_refuse() {
    let dir = plane(Some(&old()));
    let read = read_file(dir.path(), "alpha").unwrap();
    assert!(read.exists);
    assert_eq!(read.file, "workspaces/alpha/workspace.json");
    assert_eq!(read.text, old());
    assert!(read.fields.is_empty());
    assert!(read.refusals.is_empty(), "{:?}", read.refusals);
}

#[test]
fn a_saved_setting_lands_under_settings_and_keeps_every_other_key_where_it_was() {
    let dir = plane(Some(&old()));
    save(dir.path(), "alpha", Some(&old()), &[off()]).unwrap();
    let text = on_disk(dir.path());
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        doc["settings"],
        serde_json::json!({"extensions": {"stats": {"enabled": false}}})
    );
    let keys: Vec<&str> = doc
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "name",
            "description",
            "repos",
            "updated_at",
            "updated_by",
            "charter_generated",
            "settings"
        ]
    );
    assert_eq!(doc["updated_at"], "2026-09-17T14:01:35+00:00");
    // Charter wrote it before, so it is still charter's: the digest was taken again.
    assert_eq!(manifest::ownership(Some(&text)), Ownership::Charter);
    let read = read_file(dir.path(), "alpha").unwrap();
    assert_eq!(
        read.fields,
        vec![(
            vec![
                Step::Key("extensions".into()),
                Step::Key("stats".into()),
                Step::Key("enabled".into()),
            ],
            Found::Value(Value::Bool(false)),
        )]
    );
}

#[test]
fn a_hand_written_manifest_stays_the_operators_after_a_save() {
    // The automatic writers leave an operator's manifest alone; a settings save must not hand
    // it back to them by stamping it.
    let hand = "{\n  \"name\": \"alpha\",\n  \"repos\": []\n}\n";
    let dir = plane(Some(hand));
    save(dir.path(), "alpha", Some(hand), &[off()]).unwrap();
    let text = on_disk(dir.path());
    assert_eq!(manifest::ownership(Some(&text)), Ownership::Operator);
    assert!(!text.contains(manifest::KEY), "{text}");
}

#[test]
fn removing_the_last_setting_leaves_the_manifest_as_it_was() {
    let dir = plane(Some(&old()));
    save(dir.path(), "alpha", Some(&old()), &[off()]).unwrap();
    let now = on_disk(dir.path());
    save(
        dir.path(),
        "alpha",
        Some(&now),
        &[at(&["extensions", "stats", "enabled"], None)],
    )
    .unwrap();
    assert_eq!(on_disk(dir.path()), old());
}

#[test]
fn a_manifest_changed_since_it_was_read_is_not_written_over() {
    let dir = plane(Some(&old()));
    let refused = save(dir.path(), "alpha", Some("{}"), &[off()]).unwrap_err();
    assert_eq!(
        refused,
        vec![
            "workspaces/alpha/workspace.json changed on disk since this tab read it, so nothing \
             was saved. Read it again, then make the change again."
                .to_owned()
        ]
    );
    assert_eq!(on_disk(dir.path()), old());
}

#[test]
fn a_value_the_reader_would_ignore_is_refused_in_its_words() {
    let dir = plane(Some(&old()));
    let refused = save(
        dir.path(),
        "alpha",
        Some(&old()),
        &[at(
            &["extensions", "stats", "enabled"],
            Some(Value::Text("no".into())),
        )],
    )
    .unwrap_err();
    assert_eq!(
        refused,
        vec![
            "extensions.stats.enabled in workspaces/alpha/workspace.json is not true or false"
                .to_owned()
        ]
    );
    assert_eq!(on_disk(dir.path()), old());
}

#[test]
fn a_settings_key_no_reader_reads_is_refused() {
    let text = r#"{"name": "alpha", "settings": {"extensions": {}, "colour": "red"}}"#;
    assert_eq!(
        refusals(text, "alpha"),
        vec![
            "settings.colour in workspaces/alpha/workspace.json is not read — a workspace's \
             settings hold extensions and nothing else"
                .to_owned()
        ]
    );
}

#[test]
fn settings_that_is_not_an_object_is_refused() {
    assert_eq!(
        refusals(r#"{"settings": ["stats"]}"#, "alpha"),
        vec![
            "settings in workspaces/alpha/workspace.json is not an object — a workspace's \
             settings are {\"extensions\": {\"<id>\": {\"enabled\": …, \"settings\": {…}}}}"
                .to_owned()
        ]
    );
}

#[test]
fn a_manifest_that_is_not_json_has_no_form() {
    let dir = plane(Some("{\"name\": "));
    let read = read_file(dir.path(), "alpha").unwrap();
    assert!(!read.parsed);
    assert_eq!(
        read.refusals,
        vec![
            "workspaces/alpha/workspace.json is not a JSON object, so charter reads no settings \
             from it — mend it by hand"
                .to_owned()
        ]
    );
    let refused = save(dir.path(), "alpha", Some("{\"name\": "), &[off()]).unwrap_err();
    assert_eq!(refused, read.refusals);
}

#[test]
fn a_workspace_with_no_manifest_gets_one_at_its_first_save() {
    let dir = plane(None);
    let read = read_file(dir.path(), "alpha").unwrap();
    assert!(!read.exists);
    save(dir.path(), "alpha", None, &[off()]).unwrap();
    let text = on_disk(dir.path());
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["name"], "alpha");
    assert_eq!(doc["repos"], serde_json::json!([]));
    assert_eq!(doc["settings"]["extensions"]["stats"]["enabled"], false);
    assert_eq!(manifest::ownership(Some(&text)), Ownership::Charter);
}

#[test]
fn a_secret_shaped_value_is_refused_by_its_kind_and_never_quoted() {
    let dir = plane(Some(&old()));
    let token = "AKIAIOSFODNN7EXAMPLE";
    let refused = save(
        dir.path(),
        "alpha",
        Some(&old()),
        &[at(
            &["extensions", "stats", "settings", "token"],
            Some(Value::Text(token.into())),
        )],
    )
    .unwrap_err();
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert!(refused[0].contains("AWS access key"), "{refused:?}");
    assert!(!refused[0].contains(token));
    assert_eq!(on_disk(dir.path()), old());
}

#[test]
fn a_workspace_that_is_gone_is_said_and_never_made_again_by_a_save() {
    // A settings tab put back at a launch can name a workspace deleted since; its first save
    // must not bring the directory back with one file in it.
    let dir = plane(None);
    fs::remove_dir_all(dir.path().join("workspaces/alpha")).unwrap();
    let gone = "there is no workspace 'alpha' in this plane any more, so it has no settings";
    assert_eq!(read_file(dir.path(), "alpha").unwrap_err(), gone);
    assert_eq!(
        save(dir.path(), "alpha", None, &[off()]).unwrap_err(),
        vec![gone.to_owned()]
    );
    assert!(!dir.path().join("workspaces/alpha").exists());
}

#[test]
fn a_name_that_is_not_a_workspace_is_refused_before_it_is_joined_onto_a_path() {
    let dir = plane(None);
    assert!(read_file(dir.path(), "../x").is_err());
    assert!(save(dir.path(), "../x", None, &[off()]).is_err());
    assert_eq!(read(dir.path(), "../x"), None);
}

#[test]
fn what_a_save_writes_is_what_the_resolver_reads_in_that_workspace_and_nowhere_else() {
    use crate::extension::project::{Choices, Installed, Source, State, resolve};
    let dir = plane(Some(&old()));
    save(dir.path(), "alpha", Some(&old()), &[off()]).unwrap();
    let stats = Installed {
        id: "stats".into(),
        name: "Stats".into(),
        approved: true,
        settings: Vec::new(),
    };
    let state = |workspace: Option<&str>| {
        let got = resolve(
            std::slice::from_ref(&stats),
            &Choices::read_in(dir.path(), workspace),
        )
        .remove(0);
        (got.state, got.source)
    };
    assert_eq!(state(Some("alpha")), (State::Off, Source::Workspace));
    assert_eq!(state(None), (State::On, Source::Default));
    assert_eq!(state(Some("beta")), (State::On, Source::Default));
}

#[test]
fn null_reads_as_not_set() {
    let table =
        table_in(r#"{"settings": {"extensions": {"stats": {"enabled": null}}}}"#).expect("a table");
    assert_eq!(
        table.to_string(),
        "[extensions.stats]\n",
        "the null was carried into the table"
    );
}
