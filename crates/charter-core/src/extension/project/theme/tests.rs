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
