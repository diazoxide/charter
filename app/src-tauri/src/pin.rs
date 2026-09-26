//! Whether this plane's `[charter] version` pin is one this charter meets — asked of
//! `adopt::version_report`, and of nothing else (ADR 0030, as amended by ADR 0045).
//!
//! ADR 0030 rules the comparison out of every surface but one: *"when `doctor`'s `version lock`
//! row stops deferring, it asks `adopt::version_report`'s comparison, not one of its own."* The
//! status line is a second surface, so it gets the same rule. `drift` below is that report's
//! exit status — `charter version`'s 1 — and never a comparison made here; the sentences are
//! the report's own, so the window cannot say "in sync" where the CLI refuses to.
//!
//! **A pin on the Python charter's line is not drift** (ADR 0045). The report names it as an
//! older charter line and exits 0, so the window shows no drift for it either.
//!
//! The dialog used to carry a list of the Python charter's news entries between the pin and
//! this charter's version. That corpus named none of this app's versions, so the list was always
//! empty, and it went with the corpus (#352). What a version of the app brought is `charter
//! news`.

use charter_core::adopt;
use charter_core::scaffold::Say;

use crate::planes::{PlaneId, Planes};

/// What the plane's pin says against this charter.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct PinReport {
    /// `charter version`'s verdict: its exit status was 1. The only thing the line keys on.
    pub drift: bool,
    /// This charter's version: the app's (`adopt::app_version`, ADR 0045).
    pub brought: String,
    /// `[charter] version` as written, or none.
    pub pinned: Option<String>,
    /// What `charter version` said, line by line, in its own words.
    pub said: Vec<String>,
}

/// The pin report for this plane.
#[tauri::command]
#[specta::specta]
pub fn plane_pin(planes: tauri::State<'_, Planes>, plane: PlaneId) -> Result<PinReport, String> {
    Ok(report(planes.held(&plane)?.root()))
}

pub(crate) fn report(root: &std::path::Path) -> PinReport {
    let version = adopt::version_report(Some(root));
    let drift = version.code == 1;
    let brought = adopt::app_version().to_owned();
    let pinned = adopt::locked_version(root);
    PinReport {
        drift,
        said: version
            .said
            .iter()
            .map(|say| match say {
                Say::Info(s) | Say::Ok(s) | Say::Warn(s) | Say::Err(s) => s.trim().to_owned(),
            })
            .collect(),
        brought,
        pinned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(manifest: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let root = std::fs::canonicalize(dir.path()).expect("it resolves");
        std::fs::write(root.join("charter.toml"), manifest).expect("a manifest");
        (dir, root)
    }

    #[test]
    fn a_plane_that_pins_nothing_does_not_drift() {
        let (_d, root) = plane("schema = 1\n");

        let pin = report(&root);

        assert!(!pin.drift);
        assert_eq!(pin.pinned, None);
    }

    #[test]
    fn a_plane_pinning_this_charters_version_does_not_drift() {
        let brought = adopt::app_version();
        let (_d, root) = plane(&format!("schema = 1\n[charter]\nversion = \"{brought}\"\n"));

        let pin = report(&root);

        assert!(!pin.drift, "{pin:?}");
    }

    #[test]
    fn a_pin_on_the_python_charters_line_is_not_drift() {
        let (_d, root) = plane("schema = 1\n[charter]\nversion = \"0.50.0\"\n");

        let pin = report(&root);

        assert!(!pin.drift, "{pin:?}");
        assert_eq!(pin.brought, adopt::app_version());
        assert!(
            pin.said
                .iter()
                .any(|s| s.starts_with("this control plane pins 0.50.0, a release of the Python")),
            "{:?}",
            pin.said
        );
    }

    #[test]
    fn a_pin_past_the_python_line_drifts_in_charter_versions_own_words() {
        let (_d, root) = plane("schema = 1\n[charter]\nversion = \"9.0.0\"\n");

        let pin = report(&root);

        assert!(pin.drift);
        assert!(
            pin.said
                .iter()
                .any(|s| s.starts_with("drift: this control plane pins 9.0.0")),
            "{:?}",
            pin.said
        );
    }

    #[test]
    fn a_pin_that_is_not_a_version_drifts() {
        let (_d, root) = plane("schema = 1\n[charter]\nversion = \"banana\"\n");

        let pin = report(&root);

        assert!(pin.drift);
        assert_eq!(pin.pinned.as_deref(), Some("banana"));
    }
}
