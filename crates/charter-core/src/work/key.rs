//! The tracker key: `<tracker>:<locator>` (ADR 0088 §1, V40 a).
//!
//! ```text
//! key      = tracker ":" locator
//! tracker  = [a-z][a-z0-9-]*
//! ```
//!
//! | Tracker | Locator | Example |
//! |---|---|---|
//! | `github` | `<host>/<owner>/<repo>#<number>` | `github:github.com/owner/repo#12` |
//! | `gitlab`, an issue or task | `<host>/<namespace path>/<repo>#<iid>` | `gitlab:gitlab.com/group/sub/repo#12` |
//! | `gitlab`, an epic | `<host>/<group path>&<iid>` | `gitlab:gitlab.com/group&3` |
//! | `forgejo` (reserved until FG-13) | `<host>/<owner>/<repo>#<number>` | `forgejo:codeberg.org/owner/repo#12` |
//! | `todo` | `<workspace>/<file stem>` | `todo:smart-ide/20261002-081200-port-the-picker` |
//!
//! **Normalised once, compared exactly.** The constructors normalise: the host is lowercased,
//! has no scheme, and keeps a port only when it is not the default one. [`TrackerKey::parse`]
//! does not normalise; it refuses a key that is not already in normal form, because a key read
//! back out of the project was written normalised, and one that is not was not written by
//! charter. Two keys name the same item when their bytes are equal.

use std::fmt;

/// The trackers charter itself answers for. An extension may not declare one of these as its
/// prefix (ADR 0088 §1).
pub const RESERVED: [&str; 4] = ["github", "gitlab", "forgejo", "todo"];

/// A work item's identity: the tracker's name, a colon, and the locator the tracker's own
/// references use.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrackerKey(String);

impl fmt::Display for TrackerKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TrackerKey {
    /// A key as the project holds it, refused unless it is one charter could have written.
    ///
    /// A reserved tracker's locator is held to its grammar above. An extension's tracker owns
    /// its own grammar, under ADR 0088's three rules: one line, no whitespace, and the item's
    /// own reference qualified by the site it lives on (`<site>/<reference>`). Every key holds
    /// printable characters only: no control character, ESC included, and no invisible
    /// formatting character, so a key read off a log line prints as what it is.
    pub fn parse(text: &str) -> Result<TrackerKey, String> {
        let refused = |why: &str| Err(format!("{text:?} is not a tracker key: {why}"));
        let Some((tracker, locator)) = text.split_once(':') else {
            return refused("it has no `<tracker>:`");
        };
        if !tracker_ok(tracker) {
            return refused("a tracker is a lowercase letter, then letters, digits or `-`");
        }
        if locator.is_empty() || locator.chars().any(char::is_whitespace) {
            return refused("a locator is one word, with no whitespace");
        }
        if locator.chars().any(crate::panel::undrawable) {
            return refused("a locator holds printable characters only");
        }
        let ok = match tracker {
            "github" | "forgejo" => forge_locator(locator, '#', Some(2)),
            "gitlab" => forge_locator(locator, '#', None) || forge_locator(locator, '&', None),
            "todo" => todo_locator(locator),
            _ => locator
                .split_once('/')
                .is_some_and(|(site, reference)| !site.is_empty() && !reference.is_empty()),
        };
        if !ok {
            return refused(&format!("not a `{tracker}` locator"));
        }
        Ok(TrackerKey(text.to_string()))
    }

    /// A GitHub issue's key: `github:<host>/<owner>/<repo>#<number>`. `path` is `owner/repo`
    /// as GitHub's API spells it.
    pub fn github(host: &str, path: &str, number: u64) -> Result<TrackerKey, String> {
        TrackerKey::forge("github", host, path, '#', number)
    }

    /// A GitLab issue's or task's key: `gitlab:<host>/<namespace path>/<repo>#<iid>`.
    pub fn gitlab_issue(host: &str, path: &str, iid: u64) -> Result<TrackerKey, String> {
        TrackerKey::forge("gitlab", host, path, '#', iid)
    }

    /// A GitLab epic's key: `gitlab:<host>/<group path>&<iid>`.
    pub fn gitlab_epic(host: &str, group: &str, iid: u64) -> Result<TrackerKey, String> {
        TrackerKey::forge("gitlab", host, group, '&', iid)
    }

    /// A todo's key: `todo:<workspace>/<file stem>`. The stem, not the slug or the title: it is
    /// unique in its directory, and two todos may share a title.
    pub fn todo(workspace: &str, stem: &str) -> Result<TrackerKey, String> {
        TrackerKey::parse(&format!("todo:{workspace}/{stem}"))
    }

    fn forge(
        tracker: &str,
        host: &str,
        path: &str,
        sigil: char,
        number: u64,
    ) -> Result<TrackerKey, String> {
        let host = normal_host(host)?;
        let path = path.trim_matches('/');
        TrackerKey::parse(&format!("{tracker}:{host}/{path}{sigil}{number}"))
    }

    /// The whole key, as the project writes it.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The tracker's name: `github`, `todo`, or an extension's prefix.
    pub fn tracker(&self) -> &str {
        self.0.split_once(':').map_or("", |(tracker, _)| tracker)
    }

    /// Everything after the tracker's colon.
    pub fn locator(&self) -> &str {
        self.0.split_once(':').map_or("", |(_, locator)| locator)
    }

    /// A forge key's host, port included: the locator's first segment. `None` for a todo, and
    /// for an extension's key, whose locator is its own.
    pub fn host(&self) -> Option<&str> {
        match self.tracker() {
            "github" | "gitlab" | "forgejo" => self.locator().split_once('/').map(|(h, _)| h),
            _ => None,
        }
    }

    /// The workspace and the file stem of a todo's key; `None` for any other tracker.
    pub fn todo_parts(&self) -> Option<(&str, &str)> {
        if self.tracker() != "todo" {
            return None;
        }
        self.locator().split_once('/')
    }
}

/// `[a-z][a-z0-9-]*`.
fn tracker_ok(tracker: &str) -> bool {
    let mut chars = tracker.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// `<host>/<path>` + `sigil` + a number with no leading zero. `segments` is how many path
/// segments the forge's repos have (`owner/repo` on GitHub), or `None` for "at least one".
fn forge_locator(locator: &str, sigil: char, segments: Option<usize>) -> bool {
    let Some((place, number)) = locator.rsplit_once(sigil) else {
        return false;
    };
    let Some((host, path)) = place.split_once('/') else {
        return false;
    };
    let parts: Vec<&str> = path.split('/').collect();
    let path_ok = parts
        .iter()
        .all(|p| !p.is_empty() && *p != "." && *p != ".." && !p.contains(['#', '&']))
        && segments.is_none_or(|n| parts.len() == n);
    path_ok && normal_host(host).as_deref() == Ok(host) && number_ok(number)
}

/// Whether an extension may declare `prefix` as its tracker, beside the prefixes `held` by the
/// extensions already installed (ADR 0088 §1): the tracker grammar, none of [`RESERVED`], and
/// not one another extension holds. An install that is refused here installs nothing.
pub fn claim_prefix(prefix: &str, held: &[&str]) -> Result<(), String> {
    if !tracker_ok(prefix) {
        return Err(format!(
            "{prefix:?} is not a tracker prefix: a lowercase letter, then letters, digits or `-`"
        ));
    }
    if RESERVED.contains(&prefix) {
        return Err(format!("the tracker prefix `{prefix}` is charter's own"));
    }
    if held.contains(&prefix) {
        return Err(format!(
            "the tracker prefix `{prefix}` is already declared by another extension"
        ));
    }
    Ok(())
}

/// A workspace name and a file stem, each one path segment.
fn todo_locator(locator: &str) -> bool {
    locator.split_once('/').is_some_and(|(ws, stem)| {
        crate::contain::segment_ok(ws) && crate::contain::segment_ok(stem) && !stem.contains('/')
    })
}

/// Digits, with no leading zero, and not zero.
fn number_ok(number: &str) -> bool {
    !number.is_empty() && !number.starts_with('0') && number.bytes().all(|b| b.is_ascii_digit())
}

/// A host as a key holds it: lowercased, no scheme, no trailing `/`, and a port only when it is
/// not HTTPS's default. Refused when what is left is not a hostname.
pub fn normal_host(host: &str) -> Result<String, String> {
    let bare = host
        .trim()
        .strip_prefix("https://")
        .or_else(|| host.trim().strip_prefix("http://"))
        .unwrap_or(host.trim())
        .trim_end_matches('/')
        .to_ascii_lowercase();
    let bare = bare.strip_suffix(":443").unwrap_or(&bare).to_string();
    if crate::forge::host_ok(&bare) {
        Ok(bare)
    } else {
        Err(format!("{host:?} is not a host a tracker key can name"))
    }
}

/// The host, port included, and the path segments of an item's web page, as a forge's answer
/// names it: where a backend reads a key's host and path in the forge's own spelling.
///
/// A scheme's own default port is dropped: `:443` for HTTPS (by [`normal_host`] too) and `:80`
/// for HTTP.
pub(crate) fn page_of(url: &str) -> Option<(String, Vec<&str>)> {
    let (rest, default_port) = match url.strip_prefix("https://") {
        Some(rest) => (rest, ":443"),
        None => (url.strip_prefix("http://")?, ":80"),
    };
    let (host, path) = rest.split_once('/')?;
    let host = host.strip_suffix(default_port).unwrap_or(host).to_string();
    let path = path.split(['?', '#']).next().unwrap_or_default();
    Some((host, path.split('/').filter(|s| !s.is_empty()).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_trackers_key_is_the_reference_its_tracker_prints_qualified_by_its_host() {
        assert_eq!(
            TrackerKey::github("github.com", "owner/repo", 12)
                .unwrap()
                .as_str(),
            "github:github.com/owner/repo#12"
        );
        assert_eq!(
            TrackerKey::gitlab_issue("gitlab.com", "group/sub/repo", 12)
                .unwrap()
                .as_str(),
            "gitlab:gitlab.com/group/sub/repo#12"
        );
        assert_eq!(
            TrackerKey::gitlab_epic("gitlab.com", "group", 3)
                .unwrap()
                .as_str(),
            "gitlab:gitlab.com/group&3"
        );
        assert_eq!(
            TrackerKey::todo("smart-ide", "20261002-081200-port-the-picker")
                .unwrap()
                .as_str(),
            "todo:smart-ide/20261002-081200-port-the-picker"
        );
    }

    #[test]
    fn a_host_is_normalised_once_and_the_key_is_then_compared_exactly() {
        let typed = TrackerKey::github("https://GitHub.COM:443/", "Owner/Repo", 7).unwrap();
        assert_eq!(typed.as_str(), "github:github.com/Owner/Repo#7");
        assert_eq!(
            typed,
            TrackerKey::parse("github:github.com/Owner/Repo#7").unwrap()
        );
        // The owner and repo are as the forge's API spells them, so a different case is a
        // different key: the constructor is handed the API's spelling, never a typed one.
        assert_ne!(
            typed,
            TrackerKey::github("github.com", "owner/repo", 7).unwrap()
        );
        assert_eq!(
            TrackerKey::gitlab_issue("Git.Example.com:8443", "g/r", 1)
                .unwrap()
                .as_str(),
            "gitlab:git.example.com:8443/g/r#1",
            "a port that is not the default is kept"
        );
    }

    #[test]
    fn the_tracker_names_the_backend_and_the_locator_the_instance() {
        let ghes = TrackerKey::parse("github:ghe.example.com/o/r#3").unwrap();
        assert_eq!(ghes.tracker(), "github");
        assert_eq!(ghes.locator(), "ghe.example.com/o/r#3");
        let todo = TrackerKey::todo("alpha", "20260101-000000-x").unwrap();
        assert_eq!(todo.todo_parts(), Some(("alpha", "20260101-000000-x")));
        assert_eq!(ghes.todo_parts(), None);
    }

    #[test]
    fn a_key_charter_could_not_have_written_is_refused() {
        for bad in [
            "",
            "github",
            "GitHub:github.com/o/r#1",
            "1x:thing",
            "github:GitHub.com/o/r#1",
            "github:github.com:443/o/r#1",
            "github:github.com/o#1",
            "github:github.com/o/r/x#1",
            "github:github.com/o/r#01",
            "github:github.com/o/r#0",
            "github:github.com/o/r#",
            "github:github.com/o/r&1",
            "gitlab:gitlab.com/#1",
            "gitlab:gitlab.com/g//r#1",
            "todo:alpha",
            "todo:../x",
            "todo:alpha/a/b",
            "linear:has space",
            "linear:",
            "linear:two\nlines",
        ] {
            assert!(TrackerKey::parse(bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn an_extensions_tracker_owns_its_locator_under_the_three_rules() {
        let key = TrackerKey::parse("linear:acme/TEAM-123").unwrap();
        assert_eq!(key.tracker(), "linear");
        assert_eq!(key.locator(), "acme/TEAM-123");
        assert!(RESERVED.contains(&"todo") && !RESERVED.contains(&"linear"));
    }

    #[test]
    fn a_key_holds_only_printable_characters_and_no_dot_segments() {
        for bad in [
            "github:github.com/o/r\u{1b}[2J#1",
            "linear:acme/TEAM-1\u{7}",
            "linear:acme/TEAM\u{202e}-1",
            "gitlab:gitlab.com/g/../r#1",
            "gitlab:gitlab.com/./r#1",
            "github:github.com/../r#1",
            "github:github.com/o/..#1",
        ] {
            assert!(TrackerKey::parse(bad).is_err(), "{bad:?} was accepted");
        }
        assert!(TrackerKey::github("github.com", "o/..", 1).is_err());
    }

    #[test]
    fn an_extensions_locator_is_its_reference_qualified_by_its_site() {
        assert!(TrackerKey::parse("linear:TEAM-123").is_err(), "no site");
        assert!(TrackerKey::parse("linear:/TEAM-123").is_err());
        assert!(TrackerKey::parse("linear:acme/").is_err());
        assert!(TrackerKey::parse("jira:acme.atlassian.net/KEY-7").is_ok());
    }

    #[test]
    fn an_extension_may_not_claim_a_reserved_prefix_or_one_already_held() {
        assert_eq!(claim_prefix("linear", &["jira"]), Ok(()));
        for reserved in RESERVED {
            assert!(claim_prefix(reserved, &[]).is_err(), "{reserved}");
        }
        assert!(claim_prefix("jira", &["jira"]).is_err());
        assert!(claim_prefix("Linear", &[]).is_err(), "the tracker grammar");
    }

    #[test]
    fn a_forge_keys_host_is_its_locators_first_segment() {
        let key = TrackerKey::parse("gitlab:git.example.com:8443/g/r#1").unwrap();
        assert_eq!(key.host(), Some("git.example.com:8443"));
        assert_eq!(TrackerKey::todo("a", "b").unwrap().host(), None);
    }

    #[test]
    fn a_page_names_its_host_with_its_port_and_its_path() {
        assert_eq!(
            page_of("https://GHE.example.com:8443/o/r/issues/3?x#y"),
            Some((
                "GHE.example.com:8443".to_string(),
                vec!["o", "r", "issues", "3"]
            ))
        );
        assert_eq!(page_of("github.com/o/r"), None);
        assert_eq!(
            page_of("http://ghe.internal:80/o/r").map(|(h, _)| h),
            Some("ghe.internal".to_string()),
            "HTTP's default port is dropped as HTTPS's is"
        );
    }

    #[test]
    fn a_gitlab_namespace_may_hold_any_number_of_groups() {
        for good in [
            "gitlab:gitlab.com/g/r#1",
            "gitlab:gitlab.com/a/b/c/d/r#12",
            "gitlab:gitlab.com/a/b&3",
            "forgejo:codeberg.org/o/r#12",
        ] {
            assert_eq!(TrackerKey::parse(good).unwrap().as_str(), good);
        }
    }
}
