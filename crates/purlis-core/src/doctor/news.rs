//! The `news` row: whether this build carries release notes for itself (OB-8, #994).
//!
//! `purlis news` and the About dialog read one source, the `CHANGELOG.md` compiled into the
//! binary ([`crate::news::CHANGELOG`]). About shows a build's own section, or `[Unreleased]`
//! for a build that is not released yet, so the row asks the same: is there a section for this
//! build's version, or an `[Unreleased]` one, and can the file be read at all.
//!
//! **No version and no count in the sentence.** Every recorded doctor scenario prints this row,
//! and a sentence that named the version would move all of them at every release. The version
//! is named only in the warning, where it is the finding.
//!
//! The Python charter's row counted the news since the operator's last update. That baseline
//! is retired with the range view of `purlis news` (#352), so there is nothing unread to count.

use super::{Doctor, Row};

/// The row's name, as the Python charter printed it.
pub(super) const NAME: &str = "news";

/// The version this binary was built as.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The row. The same on every project: the notes are this build's.
pub(super) fn news(_d: &Doctor) -> Row {
    row_of(crate::news::CHANGELOG, VERSION)
}

/// The row for a build of `version` carrying `changelog`.
pub(super) fn row_of(changelog: &str, version: &str) -> Row {
    if let Err(why) = changelog::released(changelog) {
        return Row::warn(
            NAME,
            super::one_line(&why.to_string(), super::DISPLAY_LIMIT),
            "The release notes are compiled into this build, so `purlis news` and About show \
             nothing for it. Install a published build.",
        );
    }
    let own = changelog::section(changelog, version).is_ok();
    let unreleased = changelog::section(changelog, changelog::UNRELEASED).is_ok();
    if own || unreleased {
        return Row::ok(
            NAME,
            "this build carries its release notes — `purlis news` prints them",
        );
    }
    Row::warn(
        NAME,
        format!(
            "the release notes this build carries have no section for {}, and no \
             [Unreleased] one",
            super::one_line(version, super::DISPLAY_LIMIT)
        ),
        "`purlis news` and About cannot say what this version brought. Install a published \
         build.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::Status;

    const NOTES: &str = "# Changelog\n\n## [Unreleased]\n\n## [0.2.0] - 2026-09-30\n\n### \
                         Added\n\n- A thing.\n\n## [0.1.0] - 2026-09-23\n\n### Added\n\n- The \
                         first thing.\n";

    #[test]
    fn the_notes_this_build_carries_are_found() {
        let row = news(&Doctor::at(
            std::path::Path::new("/nowhere-994"),
            std::path::Path::new("/nowhere-994"),
            false,
            true,
        ));

        assert_eq!(row.name, NAME);
        assert_eq!(row.status, Status::Ok, "{row:?}");
        assert_eq!(
            row.detail,
            "this build carries its release notes — `purlis news` prints them"
        );
    }

    #[test]
    fn a_released_version_reads_its_own_section() {
        assert_eq!(row_of(NOTES, "0.2.0").status, Status::Ok);
    }

    #[test]
    fn a_version_not_released_yet_reads_unreleased() {
        assert_eq!(row_of(NOTES, "0.3.0").status, Status::Ok);
    }

    #[test]
    fn a_version_with_no_section_and_no_unreleased_one_is_a_warning_naming_it() {
        let notes = NOTES.replace("## [Unreleased]\n\n", "");

        let row = row_of(&notes, "0.3.0");

        assert_eq!(row.status, Status::Warn);
        assert!(
            row.detail.contains("no section for 0.3.0"),
            "{}",
            row.detail
        );
        assert!(row.hint.contains("purlis news"), "{}", row.hint);
        assert!(!row.deferred());
    }

    #[test]
    fn notes_that_cannot_be_read_are_a_warning() {
        let row = row_of("not a changelog at all", "0.3.0");

        assert_eq!(row.status, Status::Warn, "{row:?}");
        assert!(!row.detail.contains('\n'), "{}", row.detail);
    }
}
