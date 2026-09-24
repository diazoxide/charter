//! A project's theme (charter-app#273, ADR 0048): Local over Shared, and an extension's theme
//! only while that extension is on in the project — each expected answer written out.

use super::*;
use crate::extension::project::{Choices, Installed, resolve as extensions};

fn approved(id: &str) -> Installed {
    Installed {
        id: id.to_owned(),
        approved: true,
        name: id.to_owned(),
        settings: Vec::new(),
    }
}

/// The theme for a project whose two files are `shared` and `local`, on a machine with
/// `installed`, where the approved extensions provide `offered`.
fn theme(
    installed: &[Installed],
    offered: Option<&[Offered]>,
    shared: &str,
    local: &str,
) -> Resolved {
    let on = extensions(installed, &Choices::from_text(Some(shared), Some(local)));
    resolve(&on, offered, &Said::from_text(Some(shared), Some(local)))
}

#[test]
fn a_project_that_picks_nothing_leaves_the_window_its_own_theme() {
    let got = theme(&[], None, "", "");
    assert_eq!(got.draws, None);
    assert_eq!(got.source, Source::Default);
    assert_eq!(got.why, None);
}

#[test]
fn local_overrides_shared() {
    let got = theme(
        &[],
        None,
        "[theme]\nuse = \"charter-light\"\n",
        "[theme]\nuse = \"system\"\n",
    );
    assert_eq!((got.draws, got.source), (Some(Pick::System), Source::Local));

    let got = theme(&[], None, "[theme]\nuse = \"charter-light\"\n", "");
    assert_eq!(
        (got.draws, got.source),
        (Some(Pick::BuiltIn("charter-light")), Source::Shared)
    );
}

// -------------------------------------------------------------------------------------
// An extension's theme, and when it falls back
// -------------------------------------------------------------------------------------

const PICKS_SOLARIZED: &str = "[theme]\nuse = \"solarized/Solarized Dark\"\n";

fn solarized() -> Pick {
    Pick::Extension {
        id: "solarized".to_owned(),
        name: "Solarized Dark".to_owned(),
    }
}

fn offered() -> Vec<Offered> {
    vec![Offered {
        id: "solarized".to_owned(),
        name: "Solarized Dark".to_owned(),
    }]
}

#[test]
fn an_extension_theme_is_drawn_while_its_extension_is_on_in_the_project() {
    let got = theme(
        &[approved("solarized")],
        Some(&offered()),
        PICKS_SOLARIZED,
        "",
    );
    assert_eq!(got.picked, Some(solarized()));
    assert_eq!(got.draws, Some(solarized()));
    assert_eq!(got.why, None);
}

#[test]
fn an_extension_theme_whose_extension_the_project_turned_off_falls_back_with_a_reason() {
    let got = theme(
        &[approved("solarized")],
        Some(&offered()),
        PICKS_SOLARIZED,
        "[extensions.solarized]\nenabled = false\n",
    );
    assert_eq!(got.picked, Some(solarized()));
    assert_eq!(got.draws, Some(Pick::BuiltIn("charter-dark")));
    assert_eq!(
        got.why.as_deref(),
        Some(
            "charter.toml picks “Solarized Dark” from solarized, but solarized is off in this \
             project — so the built-in charter-dark is drawn"
        )
    );
}

#[test]
fn an_extension_theme_this_machine_has_not_approved_falls_back_with_a_reason() {
    let unapproved = Installed {
        approved: false,
        ..approved("solarized")
    };
    let got = theme(&[unapproved], None, "", PICKS_SOLARIZED);
    assert_eq!(got.source, Source::Local);
    assert_eq!(got.draws, Some(Pick::BuiltIn("charter-dark")));
    assert_eq!(
        got.why.as_deref(),
        Some(
            "charter.local.toml picks “Solarized Dark” from solarized, but this machine has not \
             approved solarized — so the built-in charter-dark is drawn"
        )
    );
}

#[test]
fn an_extension_theme_from_an_extension_not_installed_falls_back_with_a_reason() {
    let got = theme(&[], None, PICKS_SOLARIZED, "");
    assert_eq!(got.draws, Some(Pick::BuiltIn("charter-dark")));
    assert_eq!(
        got.why.as_deref(),
        Some(
            "charter.toml picks “Solarized Dark” from solarized, but solarized is not installed \
             on this machine — so the built-in charter-dark is drawn"
        )
    );
}

#[test]
fn a_theme_the_extension_does_not_contribute_falls_back_when_the_survey_says_so() {
    let got = theme(
        &[approved("solarized")],
        Some(&offered()),
        "[theme]\nuse = \"solarized/Solarized Light\"\n",
        "",
    );
    assert_eq!(got.draws, Some(Pick::BuiltIn("charter-dark")));
    assert_eq!(
        got.why.as_deref(),
        Some(
            "charter.toml picks “Solarized Light” from solarized, but solarized contributes no \
             theme called “Solarized Light” — so the built-in charter-dark is drawn"
        )
    );
    // Asked without a survey, the pick stands: the window only holds what a survey found.
    let cheap = theme(
        &[approved("solarized")],
        None,
        "[theme]\nuse = \"solarized/Solarized Light\"\n",
        "",
    );
    assert_eq!(cheap.why, None);
}

#[test]
fn local_turning_the_extension_back_on_brings_shareds_pick_back() {
    let got = theme(
        &[approved("solarized")],
        Some(&offered()),
        "[theme]\nuse = \"solarized/Solarized Dark\"\n[extensions.solarized]\nenabled = false\n",
        "[extensions.solarized]\nenabled = true\n",
    );
    assert_eq!((got.draws, got.source), (Some(solarized()), Source::Shared));
}

// -------------------------------------------------------------------------------------
// What is not a pick
// -------------------------------------------------------------------------------------

#[test]
fn a_value_that_is_not_a_pick_is_ignored_and_the_next_file_down_is_used() {
    let got = theme(
        &[],
        None,
        "[theme]\nuse = \"charter-light\"\n",
        "[theme]\nuse = \"purple\"\n",
    );
    assert_eq!(
        (got.draws, got.source),
        (Some(Pick::BuiltIn("charter-light")), Source::Shared)
    );
    assert_eq!(
        got.ignored,
        [Ignored {
            source: Source::Local,
            why: "charter.local.toml sets theme.use to \"purple\", which is not charter-dark, \
                  charter-light, system or <extension>/<theme> — so charter.toml's pick is used"
                .to_owned(),
        }]
    );
}

#[test]
fn a_value_that_is_not_text_leaves_the_window_its_own_theme_when_nothing_else_picks() {
    let got = theme(&[], None, "[theme]\nuse = 3\n", "");
    assert_eq!((got.draws, got.source), (None, Source::Default));
    assert_eq!(
        got.ignored[0].why,
        "charter.toml sets theme.use to 3, which is not charter-dark, charter-light, system or \
         <extension>/<theme> — so the window keeps its own theme"
    );
}

#[test]
fn a_pick_is_read_back_as_it_was_written() {
    for value in [
        "charter-dark",
        "charter-light",
        "system",
        "solarized/Solarized Dark",
    ] {
        assert_eq!(
            Pick::parse(value).map(|pick| pick.value()).as_deref(),
            Some(value)
        );
    }
    for value in [
        "",
        "dark",
        "/Solarized",
        "solarized/",
        "../x/y",
        "solarized/  ",
    ] {
        assert_eq!(Pick::parse(value), None, "{value:?}");
    }
}

// -------------------------------------------------------------------------------------
// A workspace's theme and colour (charter-app#281)
// -------------------------------------------------------------------------------------

/// The theme in the workspace `alpha`, whose `workspace.json` is `manifest`, of a project whose
/// two files are `shared` and `local` — each layer read by the one resolver.
fn in_alpha(
    installed: &[Installed],
    offered: Option<&[Offered]>,
    shared: &str,
    manifest: &str,
    local: &str,
) -> Resolved {
    let choices =
        Choices::from_text(Some(shared), Some(local)).in_workspace("alpha", Some(manifest));
    let on = extensions(installed, &choices);
    resolve(
        &on,
        offered,
        &Said::from_text(Some(shared), Some(local)).in_workspace("alpha", Some(manifest)),
    )
}

const ALPHA_LIGHT: &str = r#"{"settings": {"theme": {"use": "charter-light"}}}"#;

#[test]
fn a_workspaces_pick_wins_over_shared_and_local_wins_over_the_workspace() {
    let got = in_alpha(&[], None, "[theme]\nuse = \"system\"\n", ALPHA_LIGHT, "");
    assert_eq!(
        (got.draws, got.source),
        (Some(Pick::BuiltIn("charter-light")), Source::Workspace)
    );

    let got = in_alpha(
        &[],
        None,
        "[theme]\nuse = \"system\"\n",
        ALPHA_LIGHT,
        "[theme]\nuse = \"charter-dark\"\n",
    );
    assert_eq!(
        (got.draws, got.source),
        (Some(Pick::BuiltIn("charter-dark")), Source::Local)
    );
}

#[test]
fn a_workspace_that_picks_nothing_leaves_the_projects_pick() {
    let got = in_alpha(
        &[],
        None,
        "[theme]\nuse = \"system\"\n",
        r#"{"name": "alpha"}"#,
        "",
    );
    assert_eq!(
        (got.draws, got.source),
        (Some(Pick::System), Source::Shared)
    );
    assert_eq!(got.colour, None);
}

#[test]
fn a_workspaces_extension_theme_falls_back_when_the_workspace_turned_that_extension_off() {
    let got = in_alpha(
        &[approved("solarized")],
        Some(&offered()),
        "",
        r#"{"settings": {"theme": {"use": "solarized/Solarized Dark"},
            "extensions": {"solarized": {"enabled": false}}}}"#,
        "",
    );
    assert_eq!(got.draws, Some(Pick::BuiltIn("charter-dark")));
    assert_eq!(
        got.why.as_deref(),
        Some(
            "workspaces/alpha/workspace.json picks “Solarized Dark” from solarized, but \
             solarized is off in this project — so the built-in charter-dark is drawn"
        )
    );
}

#[test]
fn a_workspace_value_that_is_not_a_pick_is_named_by_its_path_in_workspace_json() {
    let got = in_alpha(
        &[],
        None,
        "[theme]\nuse = \"charter-light\"\n",
        r#"{"settings": {"theme": {"use": "dark"}}}"#,
        "",
    );
    assert_eq!(
        (got.draws, got.source),
        (Some(Pick::BuiltIn("charter-light")), Source::Shared)
    );
    assert_eq!(
        got.ignored,
        [Ignored {
            source: Source::Workspace,
            why: "workspaces/alpha/workspace.json sets settings.theme.use to \"dark\", which is \
                  not charter-dark, charter-light, system or <extension>/<theme> — so \
                  charter.toml's pick is used"
                .to_owned(),
        }]
    );
}

#[test]
fn a_workspaces_colour_is_a_palette_name_or_a_hex_colour() {
    for (written, colour) in [
        ("teal", Colour::Palette("teal")),
        ("#3fa0c0", Colour::Custom("#3fa0c0".to_owned())),
        ("#3FA0C0", Colour::Custom("#3FA0C0".to_owned())),
    ] {
        let manifest = format!(r#"{{"settings": {{"theme": {{"colour": "{written}"}}}}}}"#);
        let got = in_alpha(&[], None, "", &manifest, "");
        assert_eq!(got.colour, Some(colour.clone()), "{written}");
        assert_eq!(colour.value(), written);
        // A colour picks no theme: the window keeps the one the project draws.
        assert_eq!(got.draws, None);
        assert!(got.ignored.is_empty(), "{:?}", got.ignored);
    }
    assert_eq!(PALETTE.len(), 8);
}

#[test]
fn a_colour_that_is_neither_is_ignored_with_a_reason_and_the_workspace_has_none() {
    for written in [r#""mauve""#, r##""#fff""##, "200"] {
        let manifest = format!(r#"{{"settings": {{"theme": {{"colour": {written}}}}}}}"#);
        let got = in_alpha(&[], None, "", &manifest, "");
        assert_eq!(got.colour, None, "{written}");
        assert_eq!(
            got.ignored,
            [Ignored {
                source: Source::Workspace,
                why: format!(
                    "workspaces/alpha/workspace.json sets settings.theme.colour to {written}, \
                     which is not red, orange, yellow, green, teal, blue, purple, pink or \
                     #rrggbb — so this workspace has no colour"
                ),
            }]
        );
    }
}

#[test]
fn only_a_workspace_has_a_colour() {
    // A project's files pick a theme and nothing else: `[theme] colour` there is refused, not
    // read (charter-app#281 — the colour is what tells one workspace from another).
    let got = theme(&[], None, "[theme]\ncolour = \"teal\"\n", "");
    assert_eq!(got.colour, None);
    assert_eq!(
        refusals("[theme]\ncolour = \"teal\"\n", "charter.toml"),
        [
            "theme.colour in charter.toml is not read — [theme] holds use and nothing else; a \
          colour is a workspace's"
        ]
    );
}

#[test]
fn a_workspaces_theme_table_is_refused_in_the_readers_words() {
    let top: toml::Table =
        toml::from_str("[theme]\nuse = \"dark\"\ncolour = \"mauve\"\nfont = \"x\"\n").unwrap();
    assert_eq!(
        refusals_in_workspace(&top, "workspaces/alpha/workspace.json"),
        [
            "settings.theme.use in workspaces/alpha/workspace.json is \"dark\", which is not \
             charter-dark, charter-light, system or <extension>/<theme>",
            "settings.theme.colour in workspaces/alpha/workspace.json is \"mauve\", which is not \
             red, orange, yellow, green, teal, blue, purple, pink or #rrggbb",
            "settings.theme.font in workspaces/alpha/workspace.json is not read — a workspace's \
             theme holds use and colour and nothing else",
        ]
    );
    let ok: toml::Table =
        toml::from_str("[theme]\nuse = \"system\"\ncolour = \"#00aa11\"\n").unwrap();
    assert!(refusals_in_workspace(&ok, "workspaces/alpha/workspace.json").is_empty());
    let not_a_table: toml::Table = toml::from_str("theme = \"teal\"\n").unwrap();
    assert_eq!(
        refusals_in_workspace(&not_a_table, "workspaces/alpha/workspace.json"),
        [
            "settings.theme in workspaces/alpha/workspace.json is not an object — write \
          {\"use\": \"<theme>\", \"colour\": \"<colour>\"}"
        ]
    );
}

#[test]
fn a_workspaces_theme_is_read_from_its_manifest_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("charter.toml"), "schema = 1\n").unwrap();
    let ws = dir.path().join("workspaces/alpha");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::write(
        ws.join("workspace.json"),
        r#"{"name": "alpha", "settings": {"theme": {"use": "charter-light", "colour": "pink"}}}"#,
    )
    .unwrap();

    let said = Said::read_in(dir.path(), Some("alpha"));
    let got = resolve(&[], None, &said);
    assert_eq!(
        (got.draws, got.colour),
        (
            Some(Pick::BuiltIn("charter-light")),
            Some(Colour::Palette("pink"))
        )
    );
    assert_eq!(
        said.workspace_file().as_deref(),
        Some("workspaces/alpha/workspace.json")
    );
    assert_eq!(
        colour_of(dir.path(), "alpha"),
        Some(Colour::Palette("pink"))
    );
    // Outside every workspace there is no workspace layer.
    let got = resolve(&[], None, &Said::read_in(dir.path(), None));
    assert_eq!((got.draws, got.colour), (None, None));
}
