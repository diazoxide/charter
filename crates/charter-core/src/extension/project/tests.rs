//! The precedence matrix (charter-app#253, #280, ADR 0048): machine approval, then Shared, then
//! the workspace, then Local, key by key — each row a case the operator was told about, with the expected answer written
//! out rather than recomputed.

use super::*;
use crate::extension::{Setting, SettingKind, SettingValue};

fn approved(id: &str) -> Installed {
    Installed {
        id: id.to_owned(),
        approved: true,
        name: id.to_owned(),
        settings: Vec::new(),
    }
}

fn unapproved(id: &str) -> Installed {
    Installed {
        approved: false,
        ..approved(id)
    }
}

fn one(installed: Installed, shared: &str, local: &str) -> Effective {
    let choices = Choices::from_text(Some(shared), Some(local));
    let mut all = resolve(&[installed], &choices);
    assert_eq!(all.len(), 1, "one extension in, one answer out: {all:?}");
    all.remove(0)
}

// -------------------------------------------------------------------------------------
// On and off
// -------------------------------------------------------------------------------------

#[test]
fn an_approved_extension_no_project_names_is_on_by_default() {
    // The machine-wide list keeps working: a project that says nothing changes nothing.
    let got = one(approved("stats"), "", "");
    assert_eq!((got.state, got.source), (State::On, Source::Default));
}

#[test]
fn shared_can_turn_an_approved_extension_off() {
    let got = one(
        approved("stats"),
        "[extensions.stats]\nenabled = false\n",
        "",
    );
    assert_eq!((got.state, got.source), (State::Off, Source::Shared));
}

#[test]
fn local_overrides_shared_in_both_directions() {
    let on_over_off = one(
        approved("stats"),
        "[extensions.stats]\nenabled = false\n",
        "[extensions.stats]\nenabled = true\n",
    );
    assert_eq!(
        (on_over_off.state, on_over_off.source),
        (State::On, Source::Local)
    );

    let off_over_on = one(
        approved("stats"),
        "[extensions.stats]\nenabled = true\n",
        "[extensions.stats]\nenabled = false\n",
    );
    assert_eq!(
        (off_over_on.state, off_over_on.source),
        (State::Off, Source::Local)
    );
}

#[test]
fn a_local_table_that_does_not_say_enabled_leaves_shared_in_charge() {
    // Key by key: Local naming the extension for a setting is not Local deciding whether it is on.
    let got = one(
        approved("stats"),
        "[extensions.stats]\nenabled = false\n",
        "[extensions.stats.settings]\nwindow = \"7d\"\n",
    );
    assert_eq!((got.state, got.source), (State::Off, Source::Shared));
}

#[test]
fn enabled_in_shared_and_unapproved_here_needs_approval_and_stays_off() {
    let got = one(
        unapproved("stats"),
        "[extensions.stats]\nenabled = true\n",
        "",
    );
    assert_eq!(
        (got.state, got.source),
        (State::NeedsApproval, Source::Shared)
    );
    assert!(!got.is_on(), "a project's yes stood in for this machine's");
}

#[test]
fn enabled_in_local_and_unapproved_here_needs_approval_too() {
    let got = one(
        unapproved("stats"),
        "",
        "[extensions.stats]\nenabled = true\n",
    );
    assert_eq!(
        (got.state, got.source),
        (State::NeedsApproval, Source::Local)
    );
}

#[test]
fn an_unapproved_extension_the_project_says_nothing_about_needs_approval_by_default() {
    let got = one(unapproved("stats"), "", "");
    assert_eq!(
        (got.state, got.source),
        (State::NeedsApproval, Source::Default)
    );
}

#[test]
fn off_wins_over_needs_approval() {
    // There is nothing to approve for a project that does not want it.
    let got = one(
        unapproved("stats"),
        "[extensions.stats]\nenabled = false\n",
        "",
    );
    assert_eq!((got.state, got.source), (State::Off, Source::Shared));
}

#[test]
fn an_extension_a_project_names_that_this_machine_has_not_installed_is_listed_as_such() {
    let choices = Choices::from_text(Some("[extensions.acme]\nenabled = true\n"), None);
    let all = resolve(&[approved("stats")], &choices);
    let ids: Vec<(&str, State, Source)> = all
        .iter()
        .map(|it| (it.id.as_str(), it.state, it.source))
        .collect();
    assert_eq!(
        ids,
        vec![
            ("acme", State::NotInstalled, Source::Shared),
            ("stats", State::On, Source::Default),
        ]
    );
}

#[test]
fn a_file_that_is_not_toml_says_nothing_rather_than_turning_everything_off() {
    let got = one(approved("stats"), "[extensions.stats\nenabled = false", "");
    assert_eq!((got.state, got.source), (State::On, Source::Default));
}

#[test]
fn an_enabled_that_is_not_a_bool_is_not_a_choice() {
    let got = one(
        approved("stats"),
        "[extensions.stats]\nenabled = \"no\"\n",
        "",
    );
    assert_eq!((got.state, got.source), (State::On, Source::Default));
}

// -------------------------------------------------------------------------------------
// Settings
// -------------------------------------------------------------------------------------

fn with_settings() -> Installed {
    Installed {
        settings: vec![
            Setting {
                key: "window".into(),
                title: "Window".into(),
                kind: SettingKind::Choice(vec!["7d".into(), "30d".into()]),
                default: SettingValue::Text("30d".into()),
            },
            Setting {
                key: "compact".into(),
                title: "Compact".into(),
                kind: SettingKind::Bool,
                default: SettingValue::Bool(false),
            },
        ],
        ..approved("stats")
    }
}

fn setting<'e>(got: &'e Effective, key: &str) -> &'e Resolved {
    got.settings
        .iter()
        .find(|it| it.key == key)
        .expect("the setting")
}

#[test]
fn a_setting_nobody_set_is_its_declared_default() {
    let got = one(with_settings(), "", "");
    let window = setting(&got, "window");
    assert_eq!(window.value, SettingValue::Text("30d".into()));
    assert_eq!(window.source, Source::Default);
}

#[test]
fn settings_resolve_key_by_key_with_local_over_shared() {
    let got = one(
        with_settings(),
        "[extensions.stats.settings]\nwindow = \"7d\"\ncompact = true\n",
        "[extensions.stats.settings]\ncompact = false\n",
    );
    let window = setting(&got, "window");
    assert_eq!(
        (&window.value, window.source),
        (&SettingValue::Text("7d".into()), Source::Shared)
    );
    let compact = setting(&got, "compact");
    assert_eq!(
        (&compact.value, compact.source),
        (&SettingValue::Bool(false), Source::Local)
    );
}

#[test]
fn a_value_the_extension_would_not_accept_falls_through_and_says_why() {
    let got = one(
        with_settings(),
        "[extensions.stats.settings]\nwindow = \"7d\"\n",
        "[extensions.stats.settings]\nwindow = \"1y\"\nnope = 1\n",
    );
    let window = setting(&got, "window");
    assert_eq!(
        (&window.value, window.source),
        (&SettingValue::Text("7d".into()), Source::Shared)
    );
    assert_eq!(
        got.ignored,
        vec![
            Ignored {
                source: Source::Local,
                why: "charter.local.toml sets extensions.stats.settings.window to \"1y\", and it \
                      is one of 7d, 30d — so the value from charter.toml is used"
                    .to_owned(),
            },
            Ignored {
                source: Source::Local,
                why: "charter.local.toml sets extensions.stats.settings.nope, which stats does \
                      not declare — charter hands it nothing"
                    .to_owned(),
            },
        ]
    );
}

#[test]
fn settings_are_handed_as_json_by_key() {
    let got = one(
        with_settings(),
        "[extensions.stats.settings]\ncompact = true\n",
        "",
    );
    assert_eq!(
        got.settings_json(),
        serde_json::json!({ "window": "30d", "compact": true })
    );
}

// -------------------------------------------------------------------------------------
// What a file may say
// -------------------------------------------------------------------------------------

#[test]
fn a_well_formed_extensions_table_is_not_refused() {
    let text = "[extensions.stats]\nenabled = true\n[extensions.stats.settings]\nwindow = \"7d\"\n";
    assert_eq!(refusals(text, "charter.toml"), Vec::<String>::new());
}

#[test]
fn each_shape_charter_would_ignore_is_refused_in_its_own_words() {
    let text = "[extensions]\n\"../x\" = { enabled = true }\nloose = 1\n\
                [extensions.stats]\nenabled = \"yes\"\ncolour = \"red\"\n\
                [extensions.stats.settings]\nnested = { a = 1 }\n";
    assert_eq!(
        refusals(text, "charter.local.toml"),
        vec![
            "[extensions.\"../x\"] in charter.local.toml is not an extension id — an id is \
             letters, digits, '-', '_' and '.', starting with a letter or a digit"
                .to_owned(),
            "extensions.loose in charter.local.toml is not a table — each extension is \
             [extensions.<id>], holding enabled and [extensions.<id>.settings]"
                .to_owned(),
            "extensions.stats.enabled in charter.local.toml is not true or false".to_owned(),
            "extensions.stats.colour in charter.local.toml is not read — [extensions.<id>] \
             holds enabled and settings and nothing else"
                .to_owned(),
            "extensions.stats.settings.nested in charter.local.toml is not a value a setting \
             can hold — a setting is true, false or text"
                .to_owned(),
        ]
    );
}

#[test]
fn extensions_that_is_not_a_table_is_refused() {
    assert_eq!(
        refusals("extensions = [\"stats\"]\n", "charter.toml"),
        vec![
            "extensions in charter.toml is not a table — each extension is [extensions.<id>], \
             holding enabled and [extensions.<id>.settings]"
                .to_owned()
        ]
    );
}

// -------------------------------------------------------------------------------------
// The workspace layer (charter-app#280): approval, Shared, the workspace, Local
// -------------------------------------------------------------------------------------

/// `one`, in the workspace `alpha` whose `workspace.json` is `manifest`.
fn in_workspace(installed: Installed, shared: &str, manifest: &str, local: &str) -> Effective {
    let choices =
        Choices::from_text(Some(shared), Some(local)).in_workspace("alpha", Some(manifest));
    let mut all = resolve(&[installed], &choices);
    assert_eq!(all.len(), 1, "one extension in, one answer out: {all:?}");
    all.remove(0)
}

const WS_OFF: &str =
    r#"{"name": "alpha", "settings": {"extensions": {"stats": {"enabled": false}}}}"#;
const WS_ON: &str =
    r#"{"name": "alpha", "settings": {"extensions": {"stats": {"enabled": true}}}}"#;

#[test]
fn a_workspace_can_turn_off_what_its_project_left_on() {
    let got = in_workspace(approved("stats"), "", WS_OFF, "");
    assert_eq!((got.state, got.source), (State::Off, Source::Workspace));
}

#[test]
fn a_workspace_overrides_shared_in_both_directions() {
    let on_over_off = in_workspace(
        approved("stats"),
        "[extensions.stats]\nenabled = false\n",
        WS_ON,
        "",
    );
    assert_eq!(
        (on_over_off.state, on_over_off.source),
        (State::On, Source::Workspace)
    );
    let off_over_on = in_workspace(
        approved("stats"),
        "[extensions.stats]\nenabled = true\n",
        WS_OFF,
        "",
    );
    assert_eq!(
        (off_over_on.state, off_over_on.source),
        (State::Off, Source::Workspace)
    );
}

#[test]
fn local_overrides_the_workspace_so_the_machine_has_the_last_word() {
    let got = in_workspace(
        approved("stats"),
        "",
        WS_OFF,
        "[extensions.stats]\nenabled = true\n",
    );
    assert_eq!((got.state, got.source), (State::On, Source::Local));
}

#[test]
fn a_workspace_yes_cannot_stand_in_for_this_machines_approval() {
    let got = in_workspace(unapproved("stats"), "", WS_ON, "");
    assert_eq!(
        (got.state, got.source),
        (State::NeedsApproval, Source::Workspace)
    );
    assert!(!got.is_on());
}

#[test]
fn an_old_workspace_json_with_no_settings_leaves_the_project_in_charge() {
    // Every workspace.json written before charter-app#280 has no `settings`.
    let old = r#"{"name": "alpha", "description": "", "repos": [], "updated_at": "2026-09-17T14:01:35+00:00", "updated_by": "me"}"#;
    let got = in_workspace(
        approved("stats"),
        "[extensions.stats]\nenabled = false\n",
        old,
        "",
    );
    assert_eq!((got.state, got.source), (State::Off, Source::Shared));
}

#[test]
fn a_workspace_json_that_is_not_json_says_nothing() {
    let got = in_workspace(approved("stats"), "", "{\"settings\": ", "");
    assert_eq!((got.state, got.source), (State::On, Source::Default));
}

#[test]
fn a_null_in_workspace_json_is_not_set() {
    let got = in_workspace(
        approved("stats"),
        "[extensions.stats]\nenabled = false\n",
        r#"{"settings": {"extensions": {"stats": {"enabled": null}}}}"#,
        "",
    );
    assert_eq!((got.state, got.source), (State::Off, Source::Shared));
}

#[test]
fn a_workspace_with_no_manifest_is_exactly_the_project() {
    let choices = Choices::from_text(Some("[extensions.stats]\nenabled = false\n"), None)
        .in_workspace("alpha", None);
    let got = resolve(&[approved("stats")], &choices).remove(0);
    assert_eq!((got.state, got.source), (State::Off, Source::Shared));
}

#[test]
fn settings_resolve_key_by_key_shared_then_workspace_then_local() {
    let got = in_workspace(
        with_settings(),
        "[extensions.stats.settings]\nwindow = \"7d\"\ncompact = true\n",
        r#"{"settings": {"extensions": {"stats": {"settings": {"window": "30d", "compact": false}}}}}"#,
        "[extensions.stats.settings]\ncompact = true\n",
    );
    let window = setting(&got, "window");
    assert_eq!(
        (&window.value, window.source),
        (&SettingValue::Text("30d".into()), Source::Workspace)
    );
    let compact = setting(&got, "compact");
    assert_eq!(
        (&compact.value, compact.source),
        (&SettingValue::Bool(true), Source::Local)
    );
}

#[test]
fn a_workspace_value_the_extension_would_not_accept_falls_through_to_shared_and_says_why() {
    let got = in_workspace(
        with_settings(),
        "[extensions.stats.settings]\nwindow = \"7d\"\n",
        r#"{"settings": {"extensions": {"stats": {"settings": {"window": "1y", "nope": true}}}}}"#,
        "",
    );
    let window = setting(&got, "window");
    assert_eq!(
        (&window.value, window.source),
        (&SettingValue::Text("7d".into()), Source::Shared)
    );
    assert_eq!(
        got.ignored,
        vec![
            Ignored {
                source: Source::Workspace,
                why: "workspaces/alpha/workspace.json sets settings.extensions.stats.settings.window \
                      to \"1y\", and it is one of 7d, 30d — so the value from charter.toml is used"
                    .to_owned(),
            },
            Ignored {
                source: Source::Workspace,
                why: "workspaces/alpha/workspace.json sets settings.extensions.stats.settings.nope, \
                      which stats does not declare — charter hands it nothing"
                    .to_owned(),
            },
        ]
    );
}

#[test]
fn a_local_value_passed_over_names_the_workspaces_value_as_the_one_used() {
    let got = in_workspace(
        with_settings(),
        "",
        r#"{"settings": {"extensions": {"stats": {"settings": {"window": "7d"}}}}}"#,
        "[extensions.stats.settings]\nwindow = \"1y\"\n",
    );
    assert_eq!(setting(&got, "window").source, Source::Workspace);
    assert_eq!(
        got.ignored[0].why,
        "charter.local.toml sets extensions.stats.settings.window to \"1y\", and it is one of \
         7d, 30d — so the value from workspaces/alpha/workspace.json is used"
    );
}

#[test]
fn the_workspace_source_names_its_file() {
    assert_eq!(Source::Workspace.as_str(), "workspace");
    assert_eq!(Source::Workspace.file(), Some("workspace.json"));
}
