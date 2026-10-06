//! The network log (OB-15, X50): every call a charter feature makes over the network, listed in
//! a local file the operator can read.
//!
//! **Why it exists.** charter promises no network calls to Charter without an account, except
//! the signed updater and the static signed files beside it (O4, X50, ADR 0083 §9). Features
//! still reach third parties: a forge, and later advisory databases and docs. "No phone-home"
//! would be read as "no network", so instead of a claim there is a list. Each line says which
//! feature called which host, and whether the call went to Charter or to a third party.
//!
//! # What a line holds, and what it never holds
//!
//! A line is **metadata about the call**, the shape ADR 0070 §3 gives the forge's:
//!
//! - when (UTC, to the second), the feature, and the route it took (`https`, `gh` or `glab`);
//! - the host, with its port when it has one;
//! - the method, and the path as a **template**: the query is dropped, and every segment that is
//!   not a word of a forge's API is replaced by `{}`, so `repos/acme/widget/pulls/7` is listed as
//!   `repos/{}/{}/pulls/{}`;
//! - the status, or that no answer came, and how long it took;
//! - whether it went to Charter or to a third party.
//!
//! **Never:** a request or response body, a header value (so never a token), a query string, an
//! owner, repo, branch or issue named in a path, the account a call was made as, an address of
//! this machine, or anything of a chat's content. A template keeps a segment only when it is in
//! [`WORDS`], so a name the list has not heard of is masked rather than kept.
//!
//! # Where it lives
//!
//! `<config>/charter/network-log/<YYYY-MM-DD>.jsonl` in the machine store, one JSON object per
//! line, a file a day (UTC), the newest [`DAYS_KEPT`] kept. Machine, device-bound (ADR 0069;
//! `docs/plane-format.md`): it is kept for a month, so it is not transient, and FR-10 may back
//! it up. The directory is `0700` and each file `0600`. **Nothing
//! sends it anywhere**: it is not telemetry, not the audit (O1, ADR 0075) and not the diagnostic
//! log, and no code reads it but [`entries`], for a person to look at.
//!
//! **A sandboxed chat's own process cannot add to it.** Its sandbox denies writes under
//! `<config>/charter/` (the human-powers class, ADR 0067), so a `charter` a chat runs lists
//! nothing, and a chat cannot write a line that was never a call. A chat's traffic is the egress
//! proxy's to list.
//!
//! **Recording never fails a call.** A line that cannot be written is dropped; the call it
//! describes has already been made.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The feature that made a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Feature {
    /// A forge client: listing repos, a change, its checks (ADR 0070).
    Forge,
    /// The signed updater and the static signed files beside it (ADR 0042).
    Updater,
    /// `charter report`, filing on Charter's own tracker as the operator, after showing the
    /// draft (ADR 0059).
    Report,
}

impl Feature {
    /// Whom this feature's calls are for: Charter's own files and tracker, or a third party. A
    /// line says Charter only when its address is Charter's too ([`is_charters`]).
    pub fn party(self) -> Party {
        match self {
            Feature::Forge => Party::ThirdParty,
            Feature::Updater | Feature::Report => Party::Charter,
        }
    }
}

/// The route a call took.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Via {
    /// charter's own HTTPS client.
    Https,
    /// The operator's `gh`.
    Gh,
    /// The operator's `glab`.
    Glab,
}

/// Whom a call went to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Party {
    /// What Charter publishes: its release files, or its issue tracker.
    Charter,
    /// Anyone else: a forge, a database, a docs site.
    ThirdParty,
}

/// One call, as a line of the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// UTC, RFC 3339, to the second.
    pub at: String,
    pub feature: Feature,
    pub via: Via,
    pub host: String,
    pub method: String,
    /// The path's template ([`template`]).
    pub path: String,
    /// The HTTP status, when one came back. A CLI call has none.
    pub status: Option<u16>,
    /// Whether an answer came back at all, refusals included.
    pub answered: bool,
    pub ms: u64,
    pub to: Party,
}

/// A call as a feature reports it, before it is dated and made a template.
#[derive(Debug, Clone)]
pub struct Call<'a> {
    pub feature: Feature,
    pub via: Via,
    pub host: &'a str,
    pub method: &'a str,
    /// The path as it was sent, query included: [`record`] makes it a template.
    pub path: &'a str,
    pub status: Option<u16>,
    pub answered: bool,
    pub took: Duration,
}

/// How many daily files are kept, today's among them.
pub const DAYS_KEPT: usize = 30;

/// Words of the forges' APIs, which a template keeps. Any other segment is masked.
pub const WORDS: &[&str] = &[
    "api",
    "v3",
    "v4",
    "graphql",
    "user",
    "users",
    "orgs",
    "groups",
    "projects",
    "repos",
    "repository",
    "repositories",
    "pulls",
    "issues",
    "merge_requests",
    "merge",
    "commits",
    "check-runs",
    "check-suites",
    "status",
    "statuses",
    "pipelines",
    "jobs",
    "actions",
    "runs",
    "branches",
    "git",
    "trees",
    "refs",
    "contents",
    "labels",
    "milestones",
    "comments",
    "reviews",
    "search",
    "members",
    "tree",
    "auth",
    "create",
    "issue",
    "pr",
    "releases",
    "download",
    "latest",
    "ref",
    "files",
    "tags",
    "compare",
    "merge_trains",
    "cancel_merge_when_pipeline_succeeds",
    "epics",
    "iterations",
    "boards",
    "lists",
    "links",
    "notes",
    "discussions",
    "subgroups",
];

/// The words a forge's API puts before names, and how many name segments follow each: an owner
/// and a repo after `repos`, one name after the others.
const NAMES_AFTER: &[(&str, usize)] = &[("repos", 2), ("orgs", 1), ("users", 1)];

/// The collections whose next segment is one item's id or name: a number, a commit, a login.
/// Whatever follows one of these is masked, so an id spelled like an API word is never kept.
const COLLECTIONS: &[&str] = &[
    "commits",
    "statuses",
    "issues",
    "pulls",
    "merge_requests",
    "members",
    "comments",
    "reviews",
    "jobs",
    "runs",
    "pipelines",
    "epics",
    "iterations",
    "boards",
    "lists",
    "links",
    "notes",
    "discussions",
];

/// GitLab's `projects` and `groups`, whose id is a number or a full path. GitLab wants the path
/// encoded as one segment, and charter sends it so; a path sent unencoded runs to the first
/// word in [`GITLAB_UNDER`], and all of it is one `{}`.
const GITLAB_IDS: &[&str] = &["projects", "groups"];

/// The words GitLab puts directly under a repo or a group that charter asks for, which end its
/// id. Only these: a subgroup spelled like one of them ends an unencoded id early, and the
/// shorter the list, the fewer names can.
const GITLAB_UNDER: &[&str] = &[
    "repository",
    "merge_requests",
    "merge_trains",
    "issues",
    "pipelines",
    "jobs",
    "milestones",
    "labels",
    "members",
    "boards",
    "epics",
    "iterations",
    "projects",
    "subgroups",
];

/// The words after which the rest of a path is a name or a file path: a branch, a ref, a file,
/// a label. Everything after one is a single `{}`.
const NAMES_TO_THE_END: &[&str] = &[
    "branches",
    "contents",
    "refs",
    "ref",
    "trees",
    "tree",
    "files",
    "labels",
    "milestones",
    "tags",
    "compare",
];

/// `path` as the log keeps it: no query, and every name masked as `{}`.
///
/// **By position first.** The segments that follow a word in [`NAMES_AFTER`] are names however
/// they are spelled, the segment after a word in [`COLLECTIONS`] is an id, a GitLab repo's or
/// group's id runs to the next word GitLab puts under it ([`GITLAB_IDS`]), and everything after
/// a word in [`NAMES_TO_THE_END`] is one name, so a repo called `api`, a nested group
/// `acme/git` or a branch called `latest` is masked. Only then does [`WORDS`] decide the
/// segments left: a word of the API is kept, and anything else is masked.
pub fn template(path: &str) -> String {
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let mut segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .peekable();
    let mut out: Vec<&str> = Vec::new();
    let mut names = 0usize;
    while let Some(segment) = segments.next() {
        if names > 0 {
            names -= 1;
            out.push("{}");
        } else if !WORDS.contains(&segment) {
            out.push("{}");
        } else {
            out.push(segment);
            if NAMES_TO_THE_END.contains(&segment) {
                if segments.peek().is_some() {
                    out.push("{}");
                }
                break;
            }
            if GITLAB_IDS.contains(&segment) {
                let mut id = false;
                while segments
                    .next_if(|next| !GITLAB_UNDER.contains(next))
                    .is_some()
                {
                    id = true;
                }
                if id {
                    out.push("{}");
                }
                continue;
            }
            names = NAMES_AFTER
                .iter()
                .find(|(word, _)| *word == segment)
                .map_or(usize::from(COLLECTIONS.contains(&segment)), |(_, n)| *n);
        }
    }
    out.join("/")
}

/// Whether `host` and `path` (as sent, or as listed) name one of Charter's own addresses: its
/// repository's files on its forge, or that repository's API, at its address now or the one
/// GitHub redirects to it (`report::UPSTREAM_BEFORE`, which an older build's lines name).
pub fn is_charters(host: &str, path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let names = |at: usize| {
        [crate::report::UPSTREAM, crate::report::UPSTREAM_BEFORE]
            .iter()
            .filter_map(|upstream| upstream.split_once('/'))
            .any(|(owner, repo)| {
                segments.get(at) == Some(&owner) && segments.get(at + 1) == Some(&repo)
            })
    };
    match host.to_ascii_lowercase().as_str() {
        "github.com" => names(0),
        "api.github.com" => segments.first() == Some(&"repos") && names(1),
        _ => false,
    }
}

/// The directory the log is kept in, under the machine store `config_root`.
pub fn dir(config_root: &Path) -> PathBuf {
    crate::machine::dir(config_root).join("network-log")
}

/// Every line kept under `config_root`, oldest first. A line that does not read is skipped.
pub fn entries(config_root: &Path) -> Vec<Entry> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir(config_root))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    files
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .flat_map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect::<Vec<Entry>>()
        })
        .collect()
}

/// List `call` in this machine's log.
pub fn record(call: Call<'_>) {
    let Some(root) = store_root() else {
        return;
    };
    let entry = Entry {
        at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        feature: call.feature,
        via: call.via,
        host: call.host.to_ascii_lowercase(),
        method: call.method.to_ascii_uppercase(),
        path: template(call.path),
        status: call.status,
        answered: call.answered,
        ms: u64::try_from(call.took.as_millis()).unwrap_or(u64::MAX),
        to: Party::ThirdParty,
    };
    // A Charter line keeps its path whole: Charter's addresses are public, and "latest.json"
    // says more than "{}". It is Charter's only when the feature is and the address is too.
    let entry = if call.feature.party() == Party::Charter && is_charters(call.host, call.path) {
        Entry {
            path: call
                .path
                .split(['?', '#'])
                .next()
                .unwrap_or_default()
                .to_string(),
            to: Party::Charter,
            ..entry
        }
    } else {
        entry
    };
    let _ = append(&root, &entry);
}

/// List the updater's read of `url`: a channel's manifest, or the signed bundle it names. These
/// and the static signed files beside them (the first-party catalogue, the revocation feed and
/// the weekly manifest, when they exist) are the only Charter addresses a run without an account
/// reaches (ADR 0083 §9). `answered` is whether an answer came back.
pub fn updater_read(url: &str, answered: bool, took: Duration) {
    let Ok(uri) = url.parse::<http::Uri>() else {
        return;
    };
    let Some(host) = host_of(&uri) else {
        return;
    };
    let host = host.as_str();
    record(Call {
        feature: Feature::Updater,
        via: Via::Https,
        host,
        method: "GET",
        path: uri.path(),
        status: None,
        answered,
        took,
    });
}

/// `uri`'s host and port, and never the user information an authority can carry.
fn host_of(uri: &http::Uri) -> Option<String> {
    let authority = uri.authority()?;
    Some(match authority.port() {
        Some(port) => format!("{}:{port}", authority.host()),
        None => authority.host().to_string(),
    })
}

/// The machine store this process lists into, or none.
///
/// The config-home ladder [`crate::machine::config_root`] climbs, without its fence: a fenced
/// test build that resolves a store outside its fence lists nothing rather than dying, because
/// a line here is a record of a call already made and never an input to anything.
fn store_root() -> Option<PathBuf> {
    let root = crate::machine::rooted(
        crate::envvar::var_os(crate::machine::HOME_VAR),
        std::env::var_os("XDG_CONFIG_HOME"),
        dirs::home_dir(),
    )?;
    if crate::fence::FENCED && !crate::fence::inside(&resolved(&root), &crate::fence::fence()) {
        return None;
    }
    Some(root)
}

/// `path` with its nearest existing ancestor resolved, so a store not made yet is judged where
/// it will be: macOS's temporary directory is reached through a link.
fn resolved(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut at = path;
    loop {
        if let Ok(real) = at.canonicalize() {
            return rest.iter().rev().fold(real, |p, part| p.join(part));
        }
        match (at.parent(), at.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                at = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Append `entry` to today's file, pruning old files when a new day's is made.
fn append(config_root: &Path, entry: &Entry) -> std::io::Result<()> {
    let dir = dir(config_root);
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(&dir)?;
    owner_only(&dir, 0o700)?;
    let day = entry.at.get(..10).unwrap_or("unknown");
    let file = dir.join(format!("{day}.jsonl"));
    let new = !file.exists();
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // A link where today's file should be is refused, never written through.
        options
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    let mut opened = options.open(&file)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // A file made looser than this by someone else is tightened, through the descriptor.
        if opened.metadata()?.permissions().mode() & 0o077 != 0 {
            opened.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
    }
    let mut line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    line.push('\n');
    // One write of the whole line, so lines from the app and a terminal's `charter` appending
    // to one file at once never interleave.
    opened.write_all(line.as_bytes())?;
    if new {
        prune(&dir);
    }
    Ok(())
}

/// `dir` as a real directory of `mode`, tightened if it was looser, and refused if it is a link.
fn owner_only(dir: &Path, mode: u32) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(dir)?;
    if !meta.is_dir() {
        return Err(std::io::Error::other(format!(
            "{} is not a directory of charter's",
            dir.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o777 != mode {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode))?;
        }
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(())
}

/// Remove every daily file but the newest [`DAYS_KEPT`].
fn prune(dir: &Path) {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    let extra = files.len().saturating_sub(DAYS_KEPT);
    for old in &files[..extra] {
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_drops_the_query_and_masks_every_name() {
        assert_eq!(
            template("repos/acme/widget/pulls/7"),
            "repos/{}/{}/pulls/{}"
        );
        assert_eq!(
            template("orgs/ac%20me/repos?per_page=100&page=2"),
            "orgs/{}/repos"
        );
        assert_eq!(
            template("/projects/acme%2Fwidget/merge_requests?state=opened"),
            "projects/{}/merge_requests"
        );
        assert_eq!(template("graphql"), "graphql");
    }

    #[test]
    fn a_name_that_is_also_an_api_word_is_masked_by_where_it_sits() {
        assert_eq!(template("repos/acme/api/pulls/7"), "repos/{}/{}/pulls/{}");
        assert_eq!(template("repos/acme/auth/issues"), "repos/{}/{}/issues");
        assert_eq!(template("repos/git/search/pulls"), "repos/{}/{}/pulls");
        assert_eq!(template("repos/o/pulls/pulls/3"), "repos/{}/{}/pulls/{}");
        assert_eq!(
            template("repos/o/r/branches/latest"),
            "repos/{}/{}/branches/{}"
        );
        assert_eq!(
            template("repos/o/r/contents/src/api/auth/x.rs"),
            "repos/{}/{}/contents/{}"
        );
        assert_eq!(
            template("repos/o/r/git/refs/heads/release"),
            "repos/{}/{}/git/refs/{}"
        );
        assert_eq!(
            template("repos/o/r/commits/latest/check-runs"),
            "repos/{}/{}/commits/{}/check-runs"
        );
        assert_eq!(template("orgs/actions/repos"), "orgs/{}/repos");
        assert_eq!(template("users/search/repos"), "users/{}/repos");
        assert_eq!(template("groups/git/projects"), "groups/{}/projects");
        assert_eq!(
            template("projects/api/repository/branches/labels"),
            "projects/{}/repository/branches/{}"
        );
        assert_eq!(template("user/repos"), "user/repos");
        assert_eq!(template("repos/o/r/branches"), "repos/{}/{}/branches");
    }

    #[test]
    fn a_gitlab_path_keeps_its_api_words_and_masks_every_id_and_name() {
        for (sent, listed) in [
            (
                "projects/acme%2Fapi/merge_requests/12/merge",
                "projects/{}/merge_requests/{}/merge",
            ),
            (
                "projects/acme%2Fapi/merge_requests/12/cancel_merge_when_pipeline_succeeds",
                "projects/{}/merge_requests/{}/cancel_merge_when_pipeline_succeeds",
            ),
            (
                "projects/acme%2Fapi/merge_trains/merge_requests/12",
                "projects/{}/merge_trains/merge_requests/{}",
            ),
            (
                "projects/7/repository/tree?per_page=100&page=1&ref=main",
                "projects/{}/repository/tree",
            ),
            (
                "projects/acme%2Fapi/issues/3/links/9",
                "projects/{}/issues/{}/links/{}",
            ),
            (
                "groups/acme%2Fsub/epics/4/issues/77",
                "groups/{}/epics/{}/issues/{}",
            ),
            (
                "groups/acme/iterations?state=opened",
                "groups/{}/iterations",
            ),
            (
                "projects/acme%2Fapi/boards/2/lists",
                "projects/{}/boards/{}/lists",
            ),
            (
                "projects/acme%2Fapi/milestones/5",
                "projects/{}/milestones/{}",
            ),
            ("projects?membership=true", "projects"),
        ] {
            assert_eq!(template(sent), listed, "{sent}");
        }
    }

    #[test]
    fn a_nested_group_sent_unencoded_is_one_masked_id_whatever_its_parts_are_called() {
        // GitLab wants a nested path encoded as one segment, and charter always sends it so.
        // A path that slipped through unencoded is still masked whole: every part of the id up
        // to the word GitLab puts under a repo or group, API words among them.
        assert_eq!(template("groups/acme/sub/projects"), "groups/{}/projects");
        assert_eq!(
            template("groups/acme/api/auth/projects?include_subgroups=true"),
            "groups/{}/projects"
        );
        assert_eq!(
            template("projects/acme/git/search/merge_requests/12"),
            "projects/{}/merge_requests/{}"
        );
        assert_eq!(template("projects/acme/sub/api"), "projects/{}");
    }

    #[test]
    fn a_name_after_any_collection_word_is_masked_however_it_is_spelled() {
        assert_eq!(template("orgs/o/members/search"), "orgs/{}/members/{}");
        assert_eq!(template("repos/o/r/issues/api"), "repos/{}/{}/issues/{}");
        assert_eq!(
            template("projects/acme%2Fapi/pipelines/latest"),
            "projects/{}/pipelines/{}"
        );
    }

    #[test]
    fn charters_addresses_are_its_repositorys_files_and_api_and_nothing_beside_them() {
        assert!(is_charters(
            "github.com",
            "/purlis/purlis/releases/latest/download/latest.json"
        ));
        assert!(is_charters("api.github.com", "/repos/purlis/purlis/issues"));
        assert!(!is_charters("github.com", "/purlis/purlis-plane/releases"));
        // The address before the move, which GitHub redirects (V92).
        assert!(is_charters(
            "github.com",
            "/diazoxide/charter/releases/latest/download/latest.json"
        ));
        assert!(is_charters(
            "api.github.com",
            "/repos/diazoxide/charter/issues"
        ));
        assert!(!is_charters(
            "github.com",
            "/diazoxide/charter-plane/releases"
        ));
        assert!(!is_charters("api.github.com", "/repos/acme/charter"));
        assert!(!is_charters("evil.example", "/diazoxide/charter/releases"));
    }

    fn an_entry(at: &str) -> Entry {
        Entry {
            at: at.into(),
            feature: Feature::Forge,
            via: Via::Https,
            host: "forge.example".into(),
            method: "GET".into(),
            path: "user".into(),
            status: Some(200),
            answered: true,
            ms: 1,
            to: Party::ThirdParty,
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_loose_directory_and_file_are_tightened_and_a_link_is_never_written_through() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let dir = dir(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let today = dir.join("2026-10-02.jsonl");
        std::fs::write(&today, "").unwrap();
        std::fs::set_permissions(&today, std::fs::Permissions::from_mode(0o644)).unwrap();
        append(root.path(), &an_entry("2026-10-02T00:00:00Z")).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!((mode(&dir), mode(&today)), (0o700, 0o600));

        let elsewhere = root.path().join("elsewhere");
        std::fs::write(&elsewhere, "").unwrap();
        std::os::unix::fs::symlink(&elsewhere, dir.join("2026-10-03.jsonl")).unwrap();
        assert!(append(root.path(), &an_entry("2026-10-03T00:00:00Z")).is_err());
        assert_eq!(std::fs::read_to_string(&elsewhere).unwrap(), "");
    }

    #[test]
    fn an_updater_line_never_keeps_user_information_from_its_url() {
        let host = |url: &str| host_of(&url.parse().unwrap());
        assert_eq!(
            host("https://user:pw@github.com/x").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            host("http://me@127.0.0.1:8080/x").as_deref(),
            Some("127.0.0.1:8080")
        );
        assert_eq!(host("/no/authority"), None);
    }

    #[test]
    fn only_the_newest_files_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        for day in 1..=(DAYS_KEPT + 3) {
            std::fs::write(dir.path().join(format!("2026-01-{day:02}.jsonl")), "").unwrap();
        }
        prune(dir.path());
        let left = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(left, DAYS_KEPT);
        assert!(!dir.path().join("2026-01-01.jsonl").exists());
        assert!(
            dir.path()
                .join(format!("2026-01-{:02}.jsonl", DAYS_KEPT + 3))
                .exists()
        );
    }
}
