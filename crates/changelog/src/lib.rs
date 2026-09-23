//! What one release of the app brought, read out of the repository's `CHANGELOG.md`.
//!
//! **One file, two readers, and they must say the same thing.** About Charter compiles the
//! changelog into the app and shows the running version's section; the release workflow runs
//! this crate's binary to put the same section on the GitHub release and in the updater's
//! `latest.json`. Both go through [`section`], so a release page and the dialog of the build it
//! published cannot disagree about what that version brought.
//!
//! **The parsing is taiki-e's `parse-changelog`, not ours.** It is the parser behind
//! `create-gh-release-action` and knows Keep a Changelog's headings, `[Unreleased]`, code
//! fences and comments. This crate adds only the three decisions that are the app's:
//!
//! * a released version with **no section, or an empty one, is an error**. It is the error the
//!   release workflow refuses a tag with, so it names the fix;
//! * `[Unreleased]` **may be empty**. It is the state of `main` right after a release;
//! * the link reference definitions at the end of the file (`[0.1.0]: https://…`) are **not
//!   part of the last section**, which is where `parse-changelog` leaves them. A section
//!   carries instead the definitions it uses, wherever in the file they are, so a
//!   reference-style link still resolves in a release body lifted out of the file.
//!
//! This is the APP's version line (0.1.0 onwards). Python charter's news corpus, vendored in
//! `charter-core/news/`, is a different product's history and is not read here.

use std::fmt;

/// The heading Keep a Changelog gives the work that has not been released yet.
pub const UNRELEASED: &str = "Unreleased";

/// One version's section of the changelog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The version the heading names: `0.1.0`, or [`UNRELEASED`].
    pub version: String,
    /// The date after the version in the heading (`## [0.1.0] - 2026-09-23`), as written.
    /// `None` for `[Unreleased]`, which has none.
    pub date: Option<String>,
    /// The section's body, as Markdown: its `### Added` / `### Changed` / `### Fixed`
    /// subsections and their bullets, with no heading of its own. Empty only for
    /// `[Unreleased]`.
    pub notes: String,
}

/// Why a version's section could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The file is not a changelog `parse-changelog` can read, in its own words.
    Unreadable(String),
    /// There is no `## [version]` heading for this version.
    NoSection(String),
    /// There is a heading, and nothing under it.
    Empty(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unreadable(why) => write!(f, "CHANGELOG.md could not be read: {why}"),
            Error::NoSection(version) => write!(
                f,
                "CHANGELOG.md has no `## [{version}]` section. Rename `## [Unreleased]` to \
                 `## [{version}] - YYYY-MM-DD`, merge it, then tag"
            ),
            Error::Empty(version) => write!(
                f,
                "CHANGELOG.md's `## [{version}]` section is empty, and a release has to say what \
                 it brought"
            ),
        }
    }
}

impl std::error::Error for Error {}

/// The section for `version` (`0.1.0`, or [`UNRELEASED`]).
pub fn section(changelog: &str, version: &str) -> Result<Section, Error> {
    let releases =
        parse_changelog::parse(changelog).map_err(|e| Error::Unreadable(e.to_string()))?;
    let release = releases
        .get(version)
        .ok_or_else(|| Error::NoSection(version.to_owned()))?;
    let notes = with_its_own_definitions(changelog, release.notes);
    if notes.is_empty() && version != UNRELEASED {
        return Err(Error::Empty(version.to_owned()));
    }
    Ok(Section {
        version: release.version.to_owned(),
        date: date_of(release.title),
        notes,
    })
}

/// Every released version the changelog has a section for, newest first as the file lists
/// them. `[Unreleased]` is not one.
pub fn released(changelog: &str) -> Result<Vec<String>, Error> {
    let releases =
        parse_changelog::parse(changelog).map_err(|e| Error::Unreadable(e.to_string()))?;
    Ok(releases
        .keys()
        .filter(|version| **version != UNRELEASED)
        .map(|version| (*version).to_owned())
        .collect())
}

/// The date in a heading's title, `[0.1.0] - 2026-09-23` → `2026-09-23`.
fn date_of(title: &str) -> Option<String> {
    let (_, date) = title.split_once(" - ")?;
    let date = date.trim();
    (!date.is_empty()).then(|| date.to_owned())
}

/// `notes` with no link reference definition of its own, and with each definition anywhere in
/// the file that it uses appended.
///
/// A definition is a line `[label]: destination`, indented at most three spaces
/// (CommonMark §4.7), and it may sit anywhere in the file — by convention at the end, in the
/// last section's notes. A section written with reference-style links therefore still resolves
/// once it is lifted out of the file, and no section carries the others' definitions.
fn with_its_own_definitions(changelog: &str, notes: &str) -> String {
    let body = notes
        .lines()
        .filter(|line| definition_label(line).is_none())
        .collect::<Vec<_>>()
        .join("\n");
    let body = body.trim();
    let lowered = body.to_lowercase();
    let used: Vec<&str> = changelog
        .lines()
        .filter(|line| {
            definition_label(line)
                .is_some_and(|label| lowered.contains(&format!("[{}]", label.to_lowercase())))
        })
        .map(str::trim)
        .collect();
    if used.is_empty() {
        body.to_owned()
    } else {
        format!("{body}\n\n{}", used.join("\n"))
    }
}

/// The label of a link reference definition, or `None` for any other line.
fn definition_label(line: &str) -> Option<&str> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = line[indent..].strip_prefix('[')?;
    let (label, after) = rest.split_once("]:")?;
    (!label.trim().is_empty() && !after.trim().is_empty()).then_some(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "\
# Changelog

Some words about the file.

## [Unreleased]

### Added

- A thing that is merged but not released. ([#9](https://example.com/pull/9))

## [0.2.0] - 2026-10-01

### Fixed

- A [fix] that uses a reference link.

## [0.1.0] - 2026-09-23

### Added

- The first thing.
- The **second** thing, with `code`.

### Changed

- Something else.

[Unreleased]: https://example.com/compare/v0.2.0...HEAD
[0.2.0]: https://example.com/releases/tag/v0.2.0
[0.1.0]: https://example.com/releases/tag/v0.1.0
[fix]: https://example.com/pull/7
";

    #[test]
    fn a_released_version_is_its_subsections_and_nothing_else() {
        let got = section(LOG, "0.1.0").unwrap();

        assert_eq!(got.version, "0.1.0");
        assert_eq!(got.date.as_deref(), Some("2026-09-23"));
        assert_eq!(
            got.notes,
            "### Added\n\n- The first thing.\n- The **second** thing, with `code`.\n\n\
             ### Changed\n\n- Something else."
        );
    }

    #[test]
    fn the_unreleased_section_is_read_and_has_no_date() {
        let got = section(LOG, UNRELEASED).unwrap();

        assert_eq!(got.version, UNRELEASED);
        assert_eq!(got.date, None);
        assert!(
            got.notes
                .starts_with("### Added\n\n- A thing that is merged"),
            "{got:?}"
        );
        assert!(
            !got.notes.contains("0.2.0"),
            "it stops at the next version: {got:?}"
        );
    }

    #[test]
    fn the_link_definitions_at_the_end_belong_to_no_section() {
        // The last section is the one `parse-changelog` hands them to.
        let got = section(LOG, "0.1.0").unwrap();

        assert!(!got.notes.contains("]: https://"), "{got:?}");
    }

    #[test]
    fn a_definition_the_section_uses_travels_with_it() {
        // `[fix]` is defined at the end of the file, which is inside 0.1.0's notes as the
        // parser hands them over, and used only by 0.2.0.
        let got = section(LOG, "0.2.0").unwrap();

        assert_eq!(
            got.notes,
            "### Fixed\n\n- A [fix] that uses a reference link.\n\n\
             [fix]: https://example.com/pull/7"
        );
        assert!(!section(LOG, "0.1.0").unwrap().notes.contains("[fix]:"));
    }

    #[test]
    fn a_version_with_no_section_is_refused_with_the_fix() {
        let got = section(LOG, "0.3.0").unwrap_err();

        assert_eq!(got, Error::NoSection("0.3.0".into()));
        assert!(
            got.to_string()
                .contains("Rename `## [Unreleased]` to `## [0.3.0] - YYYY-MM-DD`")
        );
    }

    #[test]
    fn a_released_version_with_an_empty_section_is_refused() {
        let empty = LOG.replace("### Fixed\n\n- A [fix] that uses a reference link.\n\n", "");

        assert_eq!(section(&empty, "0.2.0"), Err(Error::Empty("0.2.0".into())));
    }

    #[test]
    fn an_empty_unreleased_section_is_the_state_right_after_a_release() {
        let after = "# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-09-23\n\n- One.\n";

        assert_eq!(section(after, UNRELEASED).unwrap().notes, "");
        assert_eq!(section(after, "0.1.0").unwrap().notes, "- One.");
    }

    #[test]
    fn released_lists_every_version_but_unreleased_newest_first() {
        assert_eq!(released(LOG).unwrap(), ["0.2.0", "0.1.0"]);
    }

    #[test]
    fn a_heading_inside_a_code_fence_is_not_a_version() {
        let fenced = LOG.replace(
            "- Something else.\n",
            "- Something else.\n\n```\n## [9.9.9] - 2099-01-01\n```\n",
        );

        assert_eq!(released(&fenced).unwrap(), ["0.2.0", "0.1.0"]);
        assert!(
            section(&fenced, "0.1.0")
                .unwrap()
                .notes
                .contains("## [9.9.9]")
        );
    }

    #[test]
    fn a_file_with_no_version_at_all_is_unreadable() {
        assert!(matches!(
            section("# Changelog\n", "0.1.0"),
            Err(Error::Unreadable(_))
        ));
    }

    #[test]
    fn a_definition_label_is_only_a_line_that_is_one() {
        assert_eq!(definition_label("[0.1.0]: https://x"), Some("0.1.0"));
        assert_eq!(definition_label("   [a]: b"), Some("a"));
        assert_eq!(
            definition_label("    [a]: b"),
            None,
            "four spaces is a code block"
        );
        assert_eq!(definition_label("- [a]: b"), None);
        assert_eq!(definition_label("[a]:"), None);
        assert_eq!(definition_label("[]: b"), None);
    }
}
