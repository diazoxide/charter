//! Which forge a new project's repos are on, read from a repo's `origin` (#839).
//!
//! **One rule for every place a project is made**: `charter init`, the app's first run and the
//! New project dialog. The forge is read from the remote of the repo the project is made for,
//! and when that remote does not say, the operator is asked. Nothing is guessed.
//!
//! The remote says only when its host is `github.com` or `gitlab.com`. A self-managed host is
//! not read as one kind or the other from its name: `gitlab.example.com` could be anything, and
//! a wrong kind sends every forge call to the wrong API. The owner is the remote's path without
//! its last segment, which is the org or user on GitHub and the group, subgroups included, on
//! GitLab.

use std::path::Path;

use crate::forge::{self, Kind};

/// The forge and owner a remote names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FromRemote {
    pub kind: Kind,
    /// The org, user or group the repo belongs to.
    pub owner: String,
}

/// What `url` names, or `None` when its host is not one whose kind charter can tell, or when
/// the remote is shaped to land somewhere other than where it seems to point.
///
/// [`forge::host_of`] reads a host the way the Python charter did, and is left as it is. This
/// is stricter, because its answer is written into a new project unasked: a remote that hides
/// another host in its userinfo, or walks out of its owner with `..`, names nothing.
pub fn forge_of_url(url: &str) -> Option<FromRemote> {
    if hides_another_host(url) {
        return None;
    }
    let host = forge::host_of(url);
    let kind = [Kind::GitHub, Kind::GitLab]
        .into_iter()
        .find(|kind| kind.default_host() == host)?;
    let path = forge::namespace_of(url)?;
    let segments: Vec<&str> = path.split('/').collect();
    if !segments.iter().all(|segment| segment_ok(segment)) {
        return None;
    }
    // GitHub names exactly `owner/repo`; a GitLab group may hold subgroups.
    let fits = match kind {
        Kind::GitHub => segments.len() == 2,
        Kind::GitLab => segments.len() >= 2,
    };
    if !fits {
        return None;
    }
    let owner = segments[..segments.len() - 1].join("/");
    Some(FromRemote { kind, owner })
}

/// One segment of an owner or repo path: not empty, not a dot segment, and none of the
/// characters a URL or an scp-style remote reads as the end of a host.
fn segment_ok(segment: &str) -> bool {
    !segment.is_empty() && segment != "." && segment != ".." && !segment.contains(['@', ':', '\\'])
}

/// Whether the userinfo before a remote's `@` holds what ends the authority for some reader:
/// a `#`, `?` or `\` in a URL (a browser-grade parser stops the host there), or a `:` in an
/// scp-style remote (git reads the host as what comes before it).
fn hides_another_host(url: &str) -> bool {
    let url = url.trim();
    if let Some((_, rest)) = url.split_once("://") {
        let authority = &rest[..rest.find('/').unwrap_or(rest.len())];
        return authority
            .rfind('@')
            .is_some_and(|at| authority[..at].contains(['#', '?', '\\']));
    }
    url.split_once('@')
        .is_some_and(|(user, _)| user.contains([':', '#', '?', '\\']))
}

/// What the `origin` of the repo at `repo` names, or why it names nothing charter can use: the
/// sentence the caller asks the operator with.
pub fn forge_of_repo(repo: &Path) -> Result<FromRemote, String> {
    use crate::worktree::git;
    let url = git::run(repo, &["remote", "get-url", "origin"], git::READ)
        .ok()
        .filter(|answer| answer.ok())
        .map(|answer| answer.line().trim().to_owned())
        .unwrap_or_default();
    if url.is_empty() {
        return Err(format!("{} has no `origin` remote", repo.display()));
    }
    forge_of_url(&url).ok_or_else(|| {
        let host = forge::host_of(&url);
        if host.is_empty() {
            format!(
                "{}'s `origin` is not on a forge charter can name",
                repo.display()
            )
        } else {
            format!(
                "{}'s `origin` is on {host}, which is neither github.com nor gitlab.com",
                repo.display()
            )
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(kind: Kind, owner: &str) -> Option<FromRemote> {
        Some(FromRemote {
            kind,
            owner: owner.to_owned(),
        })
    }

    #[test]
    fn a_github_or_gitlab_remote_names_its_forge_and_owner_in_every_spelling() {
        for (url, expected) in [
            (
                "git@github.com:acme/widget.git",
                named(Kind::GitHub, "acme"),
            ),
            (
                "https://github.com/acme/widget",
                named(Kind::GitHub, "acme"),
            ),
            (
                "ssh://git@github.com/alice/dots.git",
                named(Kind::GitHub, "alice"),
            ),
            (
                "https://gitlab.com/group/widget.git",
                named(Kind::GitLab, "group"),
            ),
            (
                "git@gitlab.com:group/sub/widget.git",
                named(Kind::GitLab, "group/sub"),
            ),
            (
                "https://GitHub.com/acme/widget",
                named(Kind::GitHub, "acme"),
            ),
        ] {
            assert_eq!(forge_of_url(url), expected, "{url}");
        }
    }

    #[test]
    fn a_self_managed_or_unknown_host_names_no_forge() {
        for url in [
            "git@gitlab.example.com:group/widget.git",
            "https://github.example.com/acme/widget",
            "https://bitbucket.org/acme/widget",
            "/srv/git/widget.git",
            "",
        ] {
            assert_eq!(forge_of_url(url), None, "{url}");
        }
    }

    /// #848 review: a remote that would land on another host or another path than the one it
    /// seems to name is no answer, and the operator is asked instead.
    #[test]
    fn a_remote_that_hides_another_host_or_path_names_no_forge() {
        for url in [
            // Git reads `evil.com` as the host of this scp-style remote.
            "evil.com:x@github.com:acme/widget",
            // A fragment, query or backslash before the `@` ends the authority there for a URL
            // parser, so the host is `evil.com`.
            "https://evil.com#@github.com/acme/widget",
            "https://evil.com?@github.com/acme/widget",
            "https://evil.com\\@github.com/acme/widget",
            // Dot segments walk out of the owner.
            "https://github.com/../widget",
            "git@github.com:acme/../../x/widget",
            "https://gitlab.com/group/./widget",
            // Empty segments, and characters no owner or repo name has.
            "https://github.com//widget",
            "https://gitlab.com/gr@up/widget",
            "https://gitlab.com/group\\x/widget",
            // A GitHub remote names exactly `owner/repo`.
            "https://github.com/acme/widget/tree/main",
            "https://github.com/widget",
        ] {
            assert_eq!(forge_of_url(url), None, "{url}");
        }
    }

    #[test]
    fn a_plain_userinfo_before_the_host_is_still_read() {
        assert_eq!(
            forge_of_url("https://oauth2@gitlab.com/group/widget.git"),
            named(Kind::GitLab, "group")
        );
    }

    fn git(dir: &Path, args: &[&str]) {
        let r = crate::testgit::run(dir, args);
        assert!(r.ok(), "git {args:?} failed: {}", r.err);
    }

    #[test]
    fn a_repo_is_read_by_its_origin_and_says_why_when_that_names_nothing() {
        let dir = tempfile::tempdir().expect("a directory");
        let repo = dir.path().join("widget");
        std::fs::create_dir_all(&repo).expect("the repo's directory");
        git(&repo, &["init", "-q", "-b", "main", "."]);

        let none = forge_of_repo(&repo).expect_err("no origin");
        assert!(none.ends_with("has no `origin` remote"), "{none}");

        git(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "git@git.example.com:acme/widget.git",
            ],
        );
        let elsewhere = forge_of_repo(&repo).expect_err("a self-managed host");
        assert!(
            elsewhere
                .ends_with("is on git.example.com, which is neither github.com nor gitlab.com"),
            "{elsewhere}"
        );

        git(
            &repo,
            &[
                "remote",
                "set-url",
                "origin",
                "https://gitlab.com/group/widget.git",
            ],
        );
        assert_eq!(forge_of_repo(&repo).ok(), named(Kind::GitLab, "group"));
    }
}
