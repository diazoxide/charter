//! Which charter this is, and what its version brought: the title bar's *About Charter*.
//!
//! The operator asked for it in as many words: *"About Charter — that will open news of
//! charter e.g. current version News"*.
//!
//! **The version is the app's own.** It is the one this build announces, which is the one the
//! updater compares against a manifest and the one on the GitHub release it came from:
//! `0.1.0` for a stable build, `0.2.0-dev.42` for a dev build (the release workflow's `plan`
//! step appends `-dev.<run>` to the workspace version and writes it into the bundle's config).
//! It is read from `app.package_info()`, because that is where the dev suffix lands;
//! `CARGO_PKG_VERSION` is the same number without it, and a test in `updates.rs` holds the two
//! together.
//!
//! **The notes are the repository's `CHANGELOG.md`, compiled in, and there is no second copy.**
//! The release workflow puts the same section on the GitHub release and in `latest.json`, both
//! through the `changelog` crate's [`changelog::section`], so the dialog of a build and the
//! release page it was published on say the same thing about the same version.
//!
//! What a build shows depends on what it is:
//!
//! * **a release**: the version has a `## [X.Y.Z]` section, and that section is shown. The
//!   release workflow refuses a tag whose version has none, and a test below refuses a crate
//!   version that has none and is not the next release;
//! * **a dev build** (`X.Y.Z-dev.N`): a prerelease of the next version, so what it carries is
//!   `## [Unreleased]`;
//! * **a version the changelog does not list**, which is what a local build of `main` is: said
//!   plainly, with `## [Unreleased]` beside it as what the build is ahead by.
//!
//! **`charter news` reads the same file.** It prints every section, or one with `--for`, out of
//! the copy compiled into `charter_core::news` (#352). The Python charter's news corpus it read
//! before is gone.
//!
//! **Nothing here is about a plane.** This is a fact about the binary, so it lives on the
//! window's chrome and is asked without one.

/// The changelog this build carries: the one its release was published with.
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// What kind of build this is, which decides which section is shown.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Build {
    /// The changelog has a section for this version.
    Release,
    /// A prerelease of `of`, the next version: `0.2.0-dev.42` is a dev build of `0.2.0`.
    Dev { of: String },
    /// A version with no section and no prerelease suffix. A local build of `main` is one.
    Unlisted,
}

/// One section of the changelog, as the dialog draws it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct Notes {
    /// The heading's version: `0.1.0`, or `Unreleased`.
    pub version: String,
    /// The date the heading gives, for a released version.
    pub date: Option<String>,
    /// The section as Markdown: `### Added`, `### Changed`, `### Fixed` and their bullets.
    /// Empty for an `[Unreleased]` that has nothing in it yet.
    pub markdown: String,
}

/// What charter says about itself.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct About {
    /// The version this build announces, dev suffix and all.
    pub version: String,
    /// Whether that version is released, a dev build, or not in the changelog.
    pub build: Build,
    /// The section shown: the version's own for a release, `[Unreleased]` otherwise. `None`
    /// only when there is no such section to show.
    pub notes: Option<Notes>,
}

/// The About dialog's content.
///
/// No plane, no disk, no state: the version is the bundle's and the changelog is compiled in.
/// A dialog that needed a project open could not be on the window's chrome, and the window
/// holds no project for the first moments of every launch.
#[tauri::command]
#[specta::specta]
pub fn about_charter(app: tauri::AppHandle) -> About {
    about(&app.package_info().version.to_string(), CHANGELOG)
}

/// What a build of `version` says about itself, out of `changelog`.
fn about(version: &str, changelog: &str) -> About {
    let notes = |heading: &str| {
        changelog::section(changelog, heading)
            .ok()
            .map(|section| Notes {
                version: section.version,
                date: section.date,
                markdown: section.notes,
            })
    };
    if let Some((of, _)) = version.split_once('-') {
        return About {
            version: version.to_owned(),
            build: Build::Dev { of: of.to_owned() },
            notes: notes(changelog::UNRELEASED),
        };
    }
    match notes(version) {
        Some(own) => About {
            version: version.to_owned(),
            build: Build::Release,
            notes: Some(own),
        },
        None => About {
            version: version.to_owned(),
            build: Build::Unlisted,
            notes: notes(changelog::UNRELEASED),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "\
# Changelog

## [Unreleased]

### Added

- Coming next.

## [0.1.0] - 2026-09-23

### Added

- The first thing.

[Unreleased]: https://example.com/compare/v0.1.0...HEAD
[0.1.0]: https://example.com/releases/tag/v0.1.0
";

    #[test]
    fn a_released_version_shows_its_own_section_and_date() {
        let about = about("0.1.0", LOG);

        assert_eq!(about.version, "0.1.0");
        assert_eq!(about.build, Build::Release);
        assert_eq!(
            about.notes,
            Some(Notes {
                version: "0.1.0".into(),
                date: Some("2026-09-23".into()),
                markdown: "### Added\n\n- The first thing.".into(),
            })
        );
    }

    #[test]
    fn a_dev_build_is_a_build_of_the_next_version_and_shows_unreleased() {
        let about = about("0.2.0-dev.42", LOG);

        assert_eq!(about.version, "0.2.0-dev.42");
        assert_eq!(about.build, Build::Dev { of: "0.2.0".into() });
        let notes = about.notes.unwrap();
        assert_eq!(notes.version, "Unreleased");
        assert_eq!(notes.date, None);
        assert_eq!(notes.markdown, "### Added\n\n- Coming next.");
    }

    #[test]
    fn a_dev_build_of_a_released_version_still_shows_unreleased() {
        // A prerelease is never the release it precedes, whatever the changelog lists.
        let about = about("0.1.0-dev.3", LOG);

        assert_eq!(about.build, Build::Dev { of: "0.1.0".into() });
        assert_eq!(about.notes.unwrap().version, "Unreleased");
    }

    #[test]
    fn a_version_the_changelog_does_not_list_says_so_and_shows_unreleased() {
        let about = about("0.3.0", LOG);

        assert_eq!(about.build, Build::Unlisted);
        assert_eq!(about.notes.unwrap().version, "Unreleased");
    }

    #[test]
    fn with_no_section_to_show_there_are_no_notes_rather_than_empty_ones() {
        let released_only = "# Changelog\n\n## [0.1.0] - 2026-09-23\n\n- One.\n";

        assert_eq!(about("0.2.0-dev.1", released_only).notes, None);
        assert_eq!(about("0.2.0", released_only).notes, None);
        assert_eq!(about("0.2.0", "not a changelog").notes, None);
    }

    /// `X.Y.Z` as numbers, so `0.10.0` sorts after `0.9.0`.
    fn triple(version: &str) -> (u64, u64, u64) {
        let mut parts = version.split('.').map(|part| {
            part.parse::<u64>()
                .unwrap_or_else(|_| panic!("{version:?} is not X.Y.Z"))
        });
        let mut next = || {
            parts
                .next()
                .unwrap_or_else(|| panic!("{version:?} is not X.Y.Z"))
        };
        (next(), next(), next())
    }

    #[test]
    fn the_version_this_crate_is_built_as_has_notes_to_show() {
        // What keeps a build from shipping with a version the changelog says nothing about.
        // Either the version is released and has its section, or it is the NEXT release —
        // newer than every released one — and `[Unreleased]` is there to say what it has so
        // far. `main` is the second between releases (docs/updating.md), and the release PR
        // that renames `[Unreleased]` to this version makes it the first.
        let version = env!("CARGO_PKG_VERSION");
        if changelog::section(CHANGELOG, version).is_ok() {
            return;
        }
        let released = changelog::released(CHANGELOG).expect("CHANGELOG.md is readable");
        for older in &released {
            assert!(
                triple(version) > triple(older),
                "the crate is {version}, CHANGELOG.md has no section for it, and it is not newer \
                 than the released {older}"
            );
        }
        assert!(
            changelog::section(CHANGELOG, changelog::UNRELEASED).is_ok(),
            "the crate is {version}, which is unreleased, and CHANGELOG.md has no \
             `## [Unreleased]` to say what it has"
        );
    }

    #[test]
    fn every_released_version_in_the_shipped_changelog_says_what_it_brought() {
        for version in changelog::released(CHANGELOG).expect("CHANGELOG.md is readable") {
            assert!(
                changelog::section(CHANGELOG, &version).is_ok(),
                "{:?}",
                changelog::section(CHANGELOG, &version)
            );
        }
    }

    #[test]
    fn the_shipped_changelog_is_the_apps_and_not_python_charters_news() {
        // The defect this module was rewritten for: About said 0.62.1, charter's corpus
        // version, on a build the release page called 0.1.0.
        let about = about(env!("CARGO_PKG_VERSION"), CHANGELOG);

        assert_eq!(about.version, env!("CARGO_PKG_VERSION"));
        let notes = about
            .notes
            .expect("the shipped changelog has notes for this build");
        // Right after a release the next version's `[Unreleased]` is empty by design — About
        // says nothing is recorded yet — so emptiness is allowed there and nowhere else.
        assert!(
            !notes.markdown.is_empty() || notes.version == "Unreleased",
            "{notes:?}"
        );
        assert!(!notes.version.starts_with("0.62"), "{notes:?}");
    }
}
