//! `charter news`: what each version of this app brought, read from its own `CHANGELOG.md`.
//!
//! **One source, three readers.** The release workflow puts a version's section on its GitHub
//! release and in the updater's manifest. About Charter shows the running version's section.
//! This command prints every section, or one with `--for`. All three read the file through the
//! `changelog` crate, so a release page, the dialog and the terminal cannot disagree about what
//! a version brought.
//!
//! **The changelog is compiled in.** The binary is the only copy a signed app has, and the
//! notes should come from the same install as the behaviour they describe.
//!
//! **What this replaced (#352).** `charter news` used to read the Python charter's frozen news
//! corpus. It had a range view (`--since`/`--until`) that needed an update baseline nothing in
//! this app writes, and a `--pending` view whose probes all named commands this binary does not
//! have. The operator retired the range view and pointed `charter news` at the changelog, and
//! the corpus went with it. The three flags are still accepted, and each is refused by name with
//! what to run instead, because an agent reading an old skill will type them.

use crate::scaffold::Say;

/// The changelog this build carries: the one its release was published with.
pub const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// Everything a command like `charter news` said, and where each line goes.
///
/// `news`, `update` and `version` all answer in this shape: some stdout, some glyph-prefixed
/// lines on stderr, and an exit status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// stdout, verbatim, newline-terminated.
    pub out: String,
    /// stderr, as glyph-prefixed lines.
    pub said: Vec<Say>,
    pub code: u8,
}

impl Report {
    fn new() -> Self {
        Report {
            out: String::new(),
            said: Vec::new(),
            code: 0,
        }
    }

    /// An empty report for a command that builds one of these — see [`crate::adopt`].
    pub fn new_public() -> Self {
        Report::new()
    }

    /// Append `other`'s output and lines, and take the worse of the two exit codes.
    ///
    /// The worse and not the later: a wrapper that said nothing went wrong must not turn a
    /// refusal it printed into an exit 0.
    pub fn absorb(&mut self, other: Report) {
        self.out.push_str(&other.out);
        self.said.extend(other.said);
        self.code = self.code.max(other.code);
    }

    fn refused(line: String, code: u8) -> Self {
        Report {
            out: String::new(),
            said: vec![Say::Err(line)],
            code,
        }
    }
}

/// What `charter news` was asked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    /// `--for <version>`: one version's section.
    pub for_version: Option<String>,
    /// `--since` was given. Retired.
    pub since: bool,
    /// `--until` was given. Retired.
    pub until: bool,
    /// `--pending` was given. Retired.
    pub pending: bool,
}

/// Where every refusal of a retired flag points.
const WHAT_TO_RUN: &str = "Every version this app has, newest first: charter news. One version: \
                           charter news --for <version>";

/// Exit status for a flag this command no longer has. 1 and not clap's 2, which a harness hook
/// reads as "block".
const RETIRED: u8 = 1;

/// `charter news`.
pub fn report(args: &Args) -> Report {
    report_from(CHANGELOG, args)
}

/// [`report`] over a changelog the caller names, so the tests do not depend on the real one.
fn report_from(changelog: &str, args: &Args) -> Report {
    if args.pending {
        return Report::refused(
            format!(
                "`charter news --pending` is retired: nothing this app ships carries an \
                 adoption probe. {WHAT_TO_RUN}"
            ),
            RETIRED,
        );
    }
    if args.since || args.until {
        return Report::refused(
            format!(
                "`charter news --since/--until` is retired: charter news reads this app's \
                 CHANGELOG.md, which needs no baseline. {WHAT_TO_RUN}"
            ),
            RETIRED,
        );
    }
    match args.for_version.as_deref().map(str::trim) {
        Some(version) if !version.is_empty() => one(changelog, version),
        _ => every(changelog),
    }
}

/// `--for <version>`: that version's section, as its release notes print it.
fn one(changelog: &str, version: &str) -> Report {
    let version = if version.eq_ignore_ascii_case(changelog::UNRELEASED) {
        changelog::UNRELEASED
    } else {
        version.strip_prefix('v').unwrap_or(version)
    };
    match changelog::section(changelog, version) {
        Ok(section) if section.notes.is_empty() => Report {
            out: String::new(),
            said: vec![Say::Info(
                "nothing is unreleased yet: every change this build has is in a released \
                 version."
                    .to_owned(),
            )],
            code: 0,
        },
        Ok(section) => Report {
            out: format!("{}\n", section.notes),
            said: Vec::new(),
            code: 0,
        },
        Err(changelog::Error::NoSection(_)) => {
            let known = changelog::released(changelog).unwrap_or_default();
            Report::refused(
                format!(
                    "CHANGELOG.md has no section for {version}. The versions it has: {}.",
                    if known.is_empty() {
                        "none".to_owned()
                    } else {
                        known.join(", ")
                    }
                ),
                1,
            )
        }
        Err(why) => Report::refused(why.to_string(), 1),
    }
}

/// No flag: every section, newest first, `[Unreleased]` only when it has something in it.
fn every(changelog: &str) -> Report {
    let versions = match changelog::released(changelog) {
        Ok(versions) => versions,
        Err(why) => return Report::refused(why.to_string(), 1),
    };
    let mut out = String::new();
    let unreleased = changelog::section(changelog, changelog::UNRELEASED)
        .ok()
        .filter(|s| !s.notes.is_empty());
    let sections = unreleased.into_iter().map(Ok).chain(
        versions
            .iter()
            .map(|version| changelog::section(changelog, version)),
    );
    for section in sections {
        let section = match section {
            Ok(section) => section,
            Err(why) => return Report::refused(why.to_string(), 1),
        };
        if !out.is_empty() {
            out.push('\n');
        }
        match &section.date {
            Some(date) => out.push_str(&format!("## [{}] - {date}\n\n", section.version)),
            None => out.push_str(&format!("## [{}]\n\n", section.version)),
        }
        out.push_str(&section.notes);
        out.push('\n');
    }
    Report {
        out,
        said: Vec::new(),
        code: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "\
# Changelog

Words about the file.

## [Unreleased]

### Fixed

- Not released yet.

## [0.2.0] - 2026-10-01

### Added

- The second release.

## [0.1.0] - 2026-09-23

### Added

- The first release.

[0.2.0]: https://example.com/releases/tag/v0.2.0
";

    const NOTHING_UNRELEASED: &str = "\
# Changelog

## [Unreleased]

## [0.1.0] - 2026-09-23

### Added

- The first release.
";

    fn args() -> Args {
        Args::default()
    }

    fn for_version(version: &str) -> Args {
        Args {
            for_version: Some(version.to_owned()),
            ..Args::default()
        }
    }

    fn err_text(report: &Report) -> String {
        report
            .said
            .iter()
            .map(|say| match say {
                Say::Info(s) | Say::Ok(s) | Say::Warn(s) | Say::Err(s) => s.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn news_is_every_changelog_section_newest_first() {
        let got = report_from(LOG, &args());

        assert_eq!(got.code, 0, "{got:?}");
        assert!(got.said.is_empty(), "{got:?}");
        assert_eq!(
            got.out,
            "## [Unreleased]\n\n### Fixed\n\n- Not released yet.\n\
             \n## [0.2.0] - 2026-10-01\n\n### Added\n\n- The second release.\n\
             \n## [0.1.0] - 2026-09-23\n\n### Added\n\n- The first release.\n"
        );
    }

    #[test]
    fn an_empty_unreleased_section_is_left_out() {
        let got = report_from(NOTHING_UNRELEASED, &args());

        assert_eq!(
            got.out,
            "## [0.1.0] - 2026-09-23\n\n### Added\n\n- The first release.\n"
        );
    }

    #[test]
    fn for_prints_one_version_as_its_release_notes_do() {
        let got = report_from(LOG, &for_version("0.1.0"));

        assert_eq!(got.code, 0);
        assert_eq!(got.out, "### Added\n\n- The first release.\n");
        let tagged = report_from(LOG, &for_version("v0.1.0"));
        assert_eq!(tagged.out, got.out, "a tag name reads as its version");
        assert_eq!(
            report_from(LOG, &for_version("unreleased")).out,
            "### Fixed\n\n- Not released yet.\n"
        );
    }

    #[test]
    fn for_a_version_the_changelog_does_not_have_names_the_ones_it_does() {
        let got = report_from(LOG, &for_version("9.9.9"));

        assert_eq!(got.code, 1);
        assert_eq!(got.out, "");
        assert_eq!(
            err_text(&got),
            "CHANGELOG.md has no section for 9.9.9. The versions it has: 0.2.0, 0.1.0."
        );
    }

    #[test]
    fn for_unreleased_with_nothing_in_it_says_so_and_succeeds() {
        let got = report_from(NOTHING_UNRELEASED, &for_version("Unreleased"));

        assert_eq!(got.code, 0);
        assert_eq!(got.out, "");
        assert!(err_text(&got).starts_with("nothing is unreleased yet"));
    }

    #[test]
    fn the_retired_range_flags_are_refused_by_name_and_say_what_to_run() {
        for asked in [
            Args {
                since: true,
                ..Args::default()
            },
            Args {
                until: true,
                ..Args::default()
            },
        ] {
            let got = report_from(LOG, &asked);
            assert_eq!(got.code, 1);
            assert_eq!(got.out, "");
            let said = err_text(&got);
            assert!(said.contains("--since/--until` is retired"), "{said}");
            assert!(said.contains("charter news --for <version>"), "{said}");
        }
    }

    #[test]
    fn the_retired_pending_view_is_refused_by_name() {
        let got = report_from(
            LOG,
            &Args {
                pending: true,
                ..Args::default()
            },
        );

        assert_eq!(got.code, 1);
        assert!(err_text(&got).contains("`charter news --pending` is retired"));
    }

    #[test]
    fn the_changelog_this_build_carries_reads_whole() {
        // Every section of the real file, so a malformed one fails here rather than in a
        // terminal.
        let got = report(&args());

        assert_eq!(got.code, 0, "{got:?}");
        assert!(got.out.contains("## [0.1.0] - 2026-09-23\n"), "{}", got.out);
    }
}
