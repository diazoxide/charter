//! Whether this plane's `[charter] version` pin is one this charter meets — asked of
//! `adopt::version_report`, and of nothing else (charter ADR 0030).
//!
//! ADR 0030 rules the comparison out of every surface but one: *"when `doctor`'s `version lock`
//! row stops deferring, it asks `adopt::version_report`'s comparison, not one of its own."* The
//! status line is a second surface, so it gets the same rule. `drift` below is that report's
//! exit status — `charter version`'s 1 — and never a comparison made here; the sentences are
//! the report's own, so the window cannot say "in sync" where the CLI refuses to.
//!
//! What this adds is the **news**: the entries between the pin and what this charter brought
//! (`news::between`, the range `charter news --since <pin>` prints). A pin that drifts is a
//! plane that has not taken up what came since, and those entries are what came since.

use charter_core::scaffold::Say;
use charter_core::{adopt, news};

use crate::planes::{PlaneId, Planes};

/// How many news entries the window is handed. A malformed pin keys below every version, so
/// the range would be the whole corpus; the count of the rest is carried instead.
const MOST_NEWS: usize = 20;

/// One news entry, as the pin's dialog lists it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct NewsItem {
    pub version: String,
    pub headline: String,
}

/// What the plane's pin says against this charter.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct PinReport {
    /// `charter version`'s verdict: its exit status was 1. The only thing the line keys on.
    pub drift: bool,
    /// The release this charter brought (`news::shipped_version`).
    pub brought: String,
    /// `[charter] version` as written, or none.
    pub pinned: Option<String>,
    /// What `charter version` said, line by line, in its own words.
    pub said: Vec<String>,
    /// What came between the pin and what this charter brought — only when it drifts.
    pub news: Vec<NewsItem>,
    /// How many more entries there were than `news` carries.
    pub more_news: u32,
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
    let brought = news::shipped_version();
    let pinned = adopt::locked_version(root);
    // Newest first: the ones furthest from what the plane has are the ones to read first. No
    // guard on `drift` is needed, and none is written: a pin that is met is the range's own
    // exclusive bottom, so `between` is empty for it by construction, and so is a pin newer
    // than what this charter brought.
    let range: Vec<news::Entry> = match &pinned {
        Some(pin) => news::between(pin, &brought).into_iter().rev().collect(),
        None => Vec::new(),
    };
    PinReport {
        drift,
        said: version
            .said
            .iter()
            .map(|say| match say {
                Say::Info(s) | Say::Ok(s) | Say::Warn(s) | Say::Err(s) => s.trim().to_owned(),
            })
            .collect(),
        more_news: u32::try_from(range.len().saturating_sub(MOST_NEWS)).unwrap_or(u32::MAX),
        news: range
            .into_iter()
            .take(MOST_NEWS)
            .map(|e| NewsItem {
                version: e.version,
                headline: e.headline,
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
        assert!(pin.news.is_empty());
    }

    #[test]
    fn a_plane_pinning_what_this_charter_brought_does_not_drift() {
        let brought = news::shipped_version();
        let (_d, root) = plane(&format!("schema = 1\n[charter]\nversion = \"{brought}\"\n"));

        let pin = report(&root);

        assert!(!pin.drift, "{pin:?}");
        assert!(pin.news.is_empty());
    }

    #[test]
    fn an_older_pin_drifts_in_charter_versions_own_words_and_carries_what_came_since() {
        let (_d, root) = plane("schema = 1\n[charter]\nversion = \"0.50.0\"\n");

        let pin = report(&root);

        assert!(pin.drift);
        assert!(
            pin.said
                .iter()
                .any(|s| s.starts_with("drift: this control plane pins 0.50.0")),
            "{:?}",
            pin.said
        );
        // Everything listed is newer than the pin, newest first.
        assert!(!pin.news.is_empty());
        assert!(pin.news.len() <= MOST_NEWS);
        assert_eq!(pin.news[0].version, news::shipped_version());
        assert!(pin.news.iter().all(|n| n.version.as_str() != "0.50.0"));
    }

    #[test]
    fn a_pin_that_is_not_a_version_lists_no_more_than_the_cap() {
        let (_d, root) = plane("schema = 1\n[charter]\nversion = \"banana\"\n");

        let pin = report(&root);

        assert!(pin.drift);
        assert_eq!(pin.news.len(), MOST_NEWS);
        assert!(pin.more_news > 0);
    }
}
