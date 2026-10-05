//! The forges a plane declares, and asking them what repos exist.
//!
//! A port of `charter/forge/` — the registry, and the parts of the GitHub and GitLab backends
//! `discover`, `clone` and `gl-refresh` use: authentication, enumerating an owner's repos, a
//! repo's top-level file list, the branch's open change and last CI result, and the
//! credential helper and SSH→HTTPS rewrite each forge's clones get. Opening or updating a
//! pull request and asking for auto-merge are in [`pr`] (ADR 0051); reviewing one is not here.
//!
//! # One credential, and charter never holds it
//!
//! The token lives in the forge's own CLI — `gh` or `glab` — and charter only ever runs that
//! CLI. It is never read here, never put on a command line, and never part of a message:
//! `discover` asks `gh api`, and a clone asks git to ask `gh auth git-credential`. What
//! reaches the CLI is the environment it keeps its credential in ([`git::CREDENTIAL_ENV`]),
//! and nothing else of charter's.
//!
//! # Which `gh`
//!
//! **The operator's `PATH` first**, then the directories git is looked for in. Not the other
//! way round, which is what `worktree::git` does for git, and the difference is deliberate:
//! git is searched fixed-first because charter runs git **from a hook**, where the environment
//! is an attacker's. `discover`, `clone` and `sync` never run from a hook — a person or an
//! agent types them — and the `gh` a person means is the one their shell finds, the one
//! `gh auth status` answered for. A `gh` an attacker put first on `PATH` gains nothing charter
//! gives it: charter hands it no token, and whoever can choose charter's `PATH` already
//! chooses which `charter` runs. The path found is then pinned — the credential helper a
//! clone gets names it absolutely — so `discover` and the clone after it use one binary.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::worktree::git;

pub mod backend;
pub mod budget;
pub mod checks;
pub mod cli;
pub mod etag;
mod github;
mod gitlab;
pub mod http;
pub mod poll;
pub mod pr;
pub mod recorded;
pub mod route;
pub mod transport;

pub use backend::{
    Account, Caller, Capabilities, Capability, ForgeBackend, ForgeRef, Owner, Principal, Priority,
    Reach, RepoRecord, Repos, Requests, Support, Surface, WorkItems,
};

/// The best-effort budget: an auth check. Python's `base.STATUS_TIMEOUT`.
pub const STATUS_TIMEOUT: Duration = Duration::from_secs(10);

/// The strict budget: one page of a listing, one tree. Python's `base.LIST_TIMEOUT`.
pub const LIST_TIMEOUT: Duration = Duration::from_secs(60);

/// A forge kind charter has a backend for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    GitHub,
    GitLab,
}

/// Every kind, in the order Python's `registry.KINDS` lists them.
pub const KINDS: [Kind; 2] = [Kind::GitLab, Kind::GitHub];

/// The kind a record or a block with no `kind` is: GitLab was the only backend once.
pub const DEFAULT_KIND: Kind = Kind::GitLab;

impl Kind {
    pub fn parse(word: &str) -> Option<Kind> {
        match word {
            "github" => Some(Kind::GitHub),
            "gitlab" => Some(Kind::GitLab),
            _ => None,
        }
    }

    /// The word a record's `forge` stamp carries.
    pub fn word(self) -> &'static str {
        match self {
            Kind::GitHub => "github",
            Kind::GitLab => "gitlab",
        }
    }

    /// The CLI that holds this forge's credential.
    pub fn cli(self) -> &'static str {
        match self {
            Kind::GitHub => "gh",
            Kind::GitLab => "glab",
        }
    }

    /// How this forge names a change: `#` for a GitHub pull request, `!` for a GitLab merge
    /// request. Python's `Forge.change_sigil`, and the only two characters
    /// [`crate::cistate`] will read back out of the cache.
    pub fn change_sigil(self) -> &'static str {
        match self {
            Kind::GitHub => "#",
            Kind::GitLab => "!",
        }
    }

    /// What this forge calls a request, in the words its own pages use: a GitHub pull
    /// request, a GitLab merge request.
    pub fn request_noun(self) -> &'static str {
        match self {
            Kind::GitHub => "pull request",
            Kind::GitLab => "merge request",
        }
    }

    /// [`Kind::request_noun`] as a sentence starts with it: `Pull request`, `Merge request`.
    pub fn request_noun_capitalised(self) -> &'static str {
        match self {
            Kind::GitHub => "Pull request",
            Kind::GitLab => "Merge request",
        }
    }

    /// The request's short name: `PR` on GitHub, `MR` on GitLab.
    pub fn request_short(self) -> &'static str {
        match self {
            Kind::GitHub => "PR",
            Kind::GitLab => "MR",
        }
    }

    /// A request as this forge's own pages name it: `#12` on GitHub, `!12` on GitLab.
    pub fn request_ref(self, number: u64) -> String {
        format!("{}{number}", self.change_sigil())
    }

    /// What this forge calls the queue a request lands through: GitHub's merge queue,
    /// GitLab's merge train.
    pub fn queue_noun(self) -> &'static str {
        match self {
            Kind::GitHub => "merge queue",
            Kind::GitLab => "merge train",
        }
    }

    /// Whether this forge's queue takes charter's squash: GitLab's merge train does; GitHub's
    /// merge queue merges by the method its own rule sets.
    pub fn queue_takes_squash(self) -> bool {
        match self {
            Kind::GitHub => false,
            Kind::GitLab => true,
        }
    }

    /// What an owner is called on this forge.
    pub fn owner_noun(self) -> &'static str {
        match self {
            Kind::GitHub => "org",
            Kind::GitLab => "group",
        }
    }

    /// What this forge calls refusing a push that carries a secret, in its own pages' words:
    /// GitHub's push protection, GitLab's secret push protection.
    pub fn push_protection_noun(self) -> &'static str {
        match self {
            Kind::GitHub => "push protection",
            Kind::GitLab => "secret push protection",
        }
    }

    /// The forge's own name, as a heading reads it.
    pub fn display(self) -> &'static str {
        match self {
            Kind::GitHub => "GitHub",
            Kind::GitLab => "GitLab",
        }
    }

    pub fn default_host(self) -> &'static str {
        match self {
            Kind::GitHub => "github.com",
            Kind::GitLab => "gitlab.com",
        }
    }
}

/// One forge: a kind, at a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forge {
    pub kind: Kind,
    pub host: String,
}

/// Whose words name a request for `clone`: the forge its origin is on, when that is a host
/// `plane` declares or a kind's default host. Any other origin, or none, gets GitHub's words —
/// "pull request", `#12` — which are the words charter used for every forge before #1067, and
/// which the recorded behaviour keeps.
pub fn request_words_of(plane: &Path, clone: &Path) -> Kind {
    let url = git::run(clone, &["remote", "get-url", "origin"], git::READ)
        .map(|run| run.out.trim().to_string())
        .unwrap_or_default();
    resolve_host(&url, plane).map_or(Kind::GitHub, |forge| forge.kind)
}

/// The sentence a host that is not a hostname is refused with. Python's `NOT_A_HOST`.
fn not_a_host(host: &str) -> String {
    not_a_host_repr(&crate::pyrepr::repr_str(host))
}

/// The same sentence about a value that is not text at all, already quoted as Python `repr`s
/// it — `doctor`'s `charter.toml` row, where a hand-edited `host = 7` still has to be read
/// back to the operator, and in charter's one wording for it.
pub(crate) fn not_a_host_repr(shown: &str) -> String {
    format!(
        "host {shown} is not a hostname. It is read from a committed charter.toml and reaches \
         both the SSH guard's deny set and the `url.https://<host>/.insteadOf` that `charter \
         git-policy --apply` writes into a clone's git config, so it takes a bare host \
         (optionally :port) — no scheme, no path, no '@'"
    )
}

impl Forge {
    /// The forge of this kind at its default host.
    pub fn default_of(kind: Kind) -> Forge {
        Forge {
            kind,
            host: kind.default_host().to_string(),
        }
    }

    /// Python's `registry._build`: a kind word and an optional host, refused when either is
    /// not one charter can use.
    pub fn build(kind: &str, host: Option<&str>) -> Result<Forge, String> {
        let Some(kind) = Kind::parse(kind) else {
            return Err(format!(
                "unknown forge kind {} — known kinds: github, gitlab",
                crate::pyrepr::repr_str(kind)
            ));
        };
        match host.filter(|h| !h.is_empty()) {
            Some(host) if !host_ok(host) => Err(not_a_host(host)),
            Some(host) => Ok(Forge {
                kind,
                host: host.to_string(),
            }),
            None => Ok(Forge::default_of(kind)),
        }
    }

    /// The backend a repo record belongs to, from its `forge` stamp — at the kind's DEFAULT
    /// host, exactly as Python's `registry.for_repo` builds it.
    pub fn for_record(record: &Value) -> Forge {
        let kind = record
            .get("forge")
            .and_then(Value::as_str)
            .and_then(Kind::parse)
            .unwrap_or(DEFAULT_KIND);
        Forge::default_of(kind)
    }

    /// The `credential.helper` a clone of this forge gets in its local config.
    pub fn credential_helper(&self) -> String {
        format!("!{} auth git-credential", self.kind.cli())
    }

    /// `(https_base, ssh_forms)`: the SSH prefixes git must rewrite to HTTPS.
    pub fn insteadof(&self) -> (String, [String; 2]) {
        (
            format!("https://{}/", self.host),
            [
                format!("git@{}:", self.host),
                format!("ssh://git@{}/", self.host),
            ],
        )
    }
}

/// Is `host` a hostname, optionally with a port? Python's `registry.host_ok`.
pub fn host_ok(host: &str) -> bool {
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) => (name, Some(port)),
        None => (host, None),
    };
    if let Some(port) = port
        && (port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    !name.is_empty()
        && name.split('.').all(|label| {
            let bytes = label.as_bytes();
            !bytes.is_empty()
                && bytes[0].is_ascii_alphanumeric()
                && bytes[bytes.len() - 1].is_ascii_alphanumeric()
                && bytes
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
        })
}

/// The HOST component of a git remote URL, lowercased, or `""`. Python's `registry._host_of`:
/// `scheme://[user@]host[:port]/…` or scp-style `[user@]host:path`, and never a match
/// anywhere else in the string.
pub fn host_of(url: &str) -> String {
    let url = url.trim();
    if let Some((scheme, rest)) = url.split_once("://") {
        let mut chars = scheme.chars();
        let scheme_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
        if scheme_ok {
            // `(?:[^@/]*@)?` — userinfo only when an `@` comes before the first `/`.
            let rest = match rest.find(['@', '/']) {
                Some(i) if rest.as_bytes()[i] == b'@' => &rest[i + 1..],
                _ => rest,
            };
            let end = rest.find(['/', ':', '?', '#']).unwrap_or(rest.len());
            return rest[..end].to_ascii_lowercase();
        }
    }
    // scp-like: `(?:[^@/\s]+@)?([^/\s:]+):(?!//)`
    let rest = match url.split_once('@') {
        Some((user, rest))
            if !user.is_empty() && !user.contains('/') && !user.contains(char::is_whitespace) =>
        {
            rest
        }
        _ => url,
    };
    let end = rest
        .find(|c: char| c == '/' || c == ':' || c.is_whitespace())
        .unwrap_or(rest.len());
    let host = &rest[..end];
    if !host.is_empty() && rest[end..].starts_with(':') && !rest[end..].starts_with("://") {
        return host.to_ascii_lowercase();
    }
    String::new()
}

/// `path_with_namespace` out of a git remote URL — the other half of [`host_of`], and the
/// value every forge API path below is built from. Python's `registry.namespace_of`.
///
/// `None` when there is no path to take, which a caller must read as *this clone names no
/// repository on a forge* and never as a guess.
///
/// **One parser, because two callers already exist in Python** (`glstate._remote_path` and
/// the change surface), and a remote shape that confuses one must not quietly answer
/// differently for the other.
pub fn namespace_of(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    let url = url.strip_suffix(".git").unwrap_or(url);
    if let Some((scheme, _)) = url.split_once("://") {
        let path = url_path(scheme, url);
        let trimmed = path.trim_matches('/');
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    // scp-like: `git@host:group/sub/repo`.
    if let Some((_, rest)) = url.split_once(':') {
        let trimmed = rest.trim_matches('/');
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    None
}

/// The schemes `urllib.parse.urlparse` splits `;params` off the last path segment for
/// (`urllib.parse.uses_params`). Reproduced rather than ignored: Python's `.path` is what
/// `namespace_of` returns, and for `https://h/a/b;x` that is `/a/b`, not `/a/b;x`.
const USES_PARAMS: [&str; 15] = [
    "", "ftp", "hdl", "prospero", "http", "imap", "https", "shttp", "rtsp", "rtspu", "sip", "sips",
    "mms", "sftp", "tel",
];

/// `urllib.parse.urlparse(url).path` for a URL that has a `://`.
///
/// Query and fragment are cut at the first `?` or `#`; `urlsplit` also drops every tab and
/// newline anywhere in the URL before parsing, which is its own defence against a header
/// split and is reproduced here so the two implementations read the same remote the same way.
fn url_path(scheme: &str, url: &str) -> String {
    let cleaned: String = url
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let Some((_, after)) = cleaned.split_once("://") else {
        return String::new();
    };
    // The netloc runs to the first `/`, `?` or `#`, and the path from there to the first `?`
    // or `#`. So `https://host` has no path, and neither has `https://host?q` nor
    // `https://host#f`: what follows the netloc starts with the character the path is cut
    // at. (A guard that asked for a `/` there said the same thing twice, and no input could
    // tell it from `true`.)
    let Some(netloc_end) = after.find(['/', '?', '#']) else {
        return String::new();
    };
    let rest = &after[netloc_end..];
    let path = &rest[..rest.find(['?', '#']).unwrap_or(rest.len())];
    // Bound outside the condition: a temporary borrowed inside an `if` chain is dropped at a
    // point the 2024 edition moved, and this reads the same either way.
    let scheme = scheme.to_ascii_lowercase();
    if USES_PARAMS.contains(&scheme.as_str())
        && let Some(last) = path.rsplit('/').next()
        && let Some((before, _)) = last.split_once(';')
    {
        let head = &path[..path.len() - last.len()];
        return format!("{head}{before}");
    }
    path.to_string()
}

// ---------------------------------------------------------------------------------------
// charter.toml                                                                            #
// ---------------------------------------------------------------------------------------

/// `charter.toml`, parsed, or `{}` when there is none. Python's `instance.load`, including its
/// refusal of a plane format this charter cannot place.
pub fn load_config(root: &Path) -> Result<toml::Table, String> {
    let path = crate::names::manifest(root);
    let Ok(raw) = std::fs::read(&path) else {
        return Ok(toml::Table::new());
    };
    let text =
        String::from_utf8(raw).map_err(|e| format!("{} is not valid TOML: {e}", path.display()))?;
    let cfg: toml::Table = text
        .parse()
        .map_err(|e| format!("{} is not valid TOML: {e}", path.display()))?;
    if let Some(refusal) = crate::compat::schema(&cfg).refusal(&path.display().to_string()) {
        return Err(refusal);
    }
    Ok(cfg)
}

/// Whether `charter.toml` declares at least one `[[forge]]` block: whether `charter discover`
/// has a forge of the project's own to ask.
pub fn declares_a_forge(cfg: &toml::Table) -> bool {
    !blocks(cfg).is_empty()
}

fn blocks(cfg: &toml::Table) -> Vec<&toml::Value> {
    match cfg.get("forge") {
        Some(toml::Value::Array(items)) => items.iter().collect(),
        _ => Vec::new(),
    }
}

fn text_of<'a>(block: &'a toml::Value, key: &str) -> Option<&'a str> {
    block.get(key).and_then(toml::Value::as_str)
}

/// `group` or `owner` of forge block `index`, or `""`. Python's `instance.group_of`.
pub fn group_of(cfg: &toml::Table, index: usize) -> String {
    blocks(cfg)
        .get(index)
        .and_then(|b| {
            text_of(b, "group")
                .filter(|s| !s.is_empty())
                .or_else(|| text_of(b, "owner"))
        })
        .unwrap_or_default()
        .to_string()
}

/// The repo names forge block `index` excludes. Python's `instance.exclude_of`.
pub fn exclude_of(cfg: &toml::Table, index: usize) -> Vec<String> {
    blocks(cfg)
        .get(index)
        .and_then(|b| b.get("exclude"))
        .and_then(toml::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(toml::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// What `discover` asks: every declared block as `(forge, owner, exclude)`, or the one
/// back-compat GitLab default when the plane declares none. Python's `_forges_to_query`.
///
/// One block charter cannot build refuses the whole command, as it does in Python: a
/// `discover` that quietly skipped a forge would write an inventory missing its repos.
pub fn to_query(cfg: &toml::Table) -> Result<Vec<(Forge, String, Vec<String>)>, String> {
    let declared = blocks(cfg);
    if declared.is_empty() {
        return Ok(vec![(
            Forge::default_of(Kind::GitLab),
            group_of(cfg, 0),
            exclude_of(cfg, 0),
        )]);
    }
    let mut out = Vec::new();
    for (i, block) in declared.iter().enumerate() {
        let kind = text_of(block, "kind")
            .filter(|k| !k.is_empty())
            .unwrap_or(DEFAULT_KIND.word());
        let forge = Forge::build(kind, text_of(block, "host"))?;
        let owner = text_of(block, "group")
            .filter(|s| !s.is_empty())
            .or_else(|| text_of(block, "owner"))
            .unwrap_or_default()
            .to_string();
        out.push((forge, owner, exclude_of(cfg, i)));
    }
    Ok(out)
}

/// `host -> forge` for every block that resolves, one bad block costing only itself.
/// Python's `registry.declared_forges`.
fn declared(cfg: &toml::Table) -> BTreeMap<String, Forge> {
    let mut out = BTreeMap::new();
    for block in blocks(cfg) {
        if !block.is_table() {
            continue;
        }
        let kind = text_of(block, "kind")
            .filter(|k| !k.is_empty())
            .unwrap_or(DEFAULT_KIND.word());
        if let Ok(forge) = Forge::build(kind, text_of(block, "host")) {
            out.insert(forge.host.clone(), forge);
        }
    }
    out
}

/// Every host the plane's one-credential policy covers: each kind's default host, widened
/// by the declared ones. Python's `registry.known_forges`, never raising.
pub fn known(root: &Path) -> BTreeMap<String, Forge> {
    known_in(&load_config(root).unwrap_or_default())
}

/// [`known`], of a `charter.toml` already read: each kind's default host, widened by the blocks
/// `cfg` declares. What a removal of a block is asked against, before and after
/// ([`crate::settings::forges`]).
pub fn known_in(cfg: &toml::Table) -> BTreeMap<String, Forge> {
    let mut out: BTreeMap<String, Forge> = KINDS
        .iter()
        .map(|k| (k.default_host().to_string(), Forge::default_of(*k)))
        .collect();
    out.extend(declared(cfg));
    out
}

/// [`known`], in **Python's dict order** rather than sorted by host — what the one-credential
/// guard ([`crate::credguard::single_credential_hit`]) is handed.
///
/// The order is not cosmetic and that is why this exists beside [`known`]. The `ssh <forge>` arm
/// reports **the first** host whose `git@<host>` appears in the argv, so two hosts where one's
/// name is a prefix of the other's — `github.co` declared beside the default `github.com` — are
/// told apart only by position, and a `BTreeMap` puts the declared one first where Python's
/// `{**defaults, **declared}` puts it last.
///
/// Python's dict semantics, reproduced exactly: each kind's DEFAULT host first, in [`KINDS`]
/// order; then each declared host in `charter.toml` block order; and a declared host that
/// re-declares a default REPLACES its value while keeping the default's POSITION.
///
/// Never raises, for [`known`]'s reason: this is on every Bash `PreToolUse` call, so an
/// unreadable or malformed `charter.toml` degrades to the class defaults rather than failing.
pub fn known_ordered(root: &Path) -> Vec<Forge> {
    let mut out: Vec<Forge> = KINDS.iter().map(|k| Forge::default_of(*k)).collect();
    let Ok(cfg) = load_config(root) else {
        return out;
    };
    for block in blocks(&cfg) {
        if !block.is_table() {
            continue;
        }
        let kind = text_of(block, "kind")
            .filter(|k| !k.is_empty())
            .unwrap_or(DEFAULT_KIND.word());
        let Ok(forge) = Forge::build(kind, text_of(block, "host")) else {
            continue; // one bad block costs only itself
        };
        match out.iter_mut().find(|f| f.host == forge.host) {
            Some(row) => *row = forge,
            None => out.push(forge),
        }
    }
    out
}

/// The forge a remote URL belongs to, by its HOST, or `None` when this plane does not
/// manage that host. Python's `registry.resolve_host`.
pub fn resolve_host(url: &str, root: &Path) -> Option<Forge> {
    let host = host_of(url);
    if host.is_empty() {
        return None;
    }
    known(root).remove(&host)
}

// ---------------------------------------------------------------------------------------
// Running the forge's CLI                                                                 #
// ---------------------------------------------------------------------------------------

/// A forge call failed: its words, and what kind of failure it was (ADR 0070 §1's closed
/// set). Python's `ForgeError`.
///
/// **The words are the contract.** `Display` is [`ForgeError::said`] and nothing else, because
/// every caller's error text is pinned by the recorded Python behaviour (ADR 0046). The kind is
/// what a caller branches on: the native transport tells it from the HTTP status, and a CLI
/// refusal, whose status charter does not parse out of the CLI's sentence, is
/// [`Failure::Unrecognised`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{said}")]
pub struct ForgeError {
    failure: Failure,
    said: String,
}

/// What kind of failure a [`ForgeError`] is (ADR 0070 §1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The credential was refused.
    Auth,
    /// The account lacks the right.
    Forbidden,
    NotFound,
    /// The account's budget is spent until `reset`, seconds since the epoch, when the forge
    /// said.
    RateLimited {
        reset: Option<u64>,
    },
    /// The forge's state refuses the change: it moved, or it already exists.
    Conflict,
    /// A forge capability is not there (ADR 0070 §2).
    Unavailable(backend::Unavailable),
    /// charter could not ask: no CLI, no network, a deadline.
    Transport,
    /// An answer charter does not understand, or a refusal it cannot classify.
    Unrecognised,
}

impl ForgeError {
    /// A failure in the forge's or the transport's own words, of no kind charter can tell.
    pub fn new(said: String) -> ForgeError {
        ForgeError::of(Failure::Unrecognised, said)
    }

    /// A failure of `failure`'s kind, in these words.
    pub fn of(failure: Failure, said: impl Into<String>) -> ForgeError {
        ForgeError {
            failure,
            said: said.into(),
        }
    }

    /// charter could not ask.
    pub fn transport(said: impl Into<String>) -> ForgeError {
        ForgeError::of(Failure::Transport, said)
    }

    /// What kind of failure this is.
    pub fn failure(&self) -> &Failure {
        &self.failure
    }

    /// The words, exactly as `Display` shows them.
    pub fn said(&self) -> &str {
        &self.said
    }

    /// The same failure, its words replaced.
    pub fn reworded(self, said: impl Into<String>) -> ForgeError {
        ForgeError {
            said: said.into(),
            ..self
        }
    }
}

impl From<String> for ForgeError {
    fn from(why: String) -> ForgeError {
        ForgeError::new(why)
    }
}

impl From<&str> for ForgeError {
    fn from(why: &str) -> ForgeError {
        ForgeError::new(why.to_string())
    }
}

/// The forge CLI `name`, as an absolute path — the operator's `PATH` first (see the module
/// docs for why), then the fixed directories [`crate::programs`] searches.
///
/// **The search moved to `programs` and the answer widened with it (charter-app#134).** It
/// used to be `PATH` then `git::GIT_DIRS`, which on an app launched from Finder is
/// `/usr/bin:/bin:/usr/sbin:/sbin` plus four system directories — so a `gh` installed in
/// `~/.local/bin`, exactly where charter-app#134's `claude` was, was invisible and
/// [`helper_for`] fell back to a bare `gh` in a credential helper. The directories added are
/// all under the operator's own `$HOME`, plus Homebrew's; anyone who can write a program into
/// one of them can already write the shell profile that puts it on `PATH`.
///
/// **This does not touch charter-app#100 and makes it smaller.** `programs::runnable` is now
/// the single place that answers "could this process run that file", for this, for the
/// doctor's listing and for a harness — so teaching Windows about `PATHEXT` is one function
/// rather than three. Off unix it still answers `false`, which is what keeps a bare `gh` out
/// of a credential helper today.
pub fn find_cli(name: &str) -> Option<PathBuf> {
    crate::programs::find(name, &crate::programs::search_dirs())
}

/// The credential helper a NETWORK git call is given for `forge`: its CLI, pinned by
/// absolute path so git's own `PATH` — the fixed one — cannot pick a different binary.
///
/// Single-quoted, because git runs a `!` helper through the shell. A path holding a quote
/// cannot be quoted that way, and gets the bare name instead, which is what Python charter
/// always writes.
pub fn helper_for(forge: &Forge) -> String {
    match find_cli(forge.kind.cli()) {
        Some(path) if !path.to_string_lossy().contains('\'') => {
            format!("!'{}' auth git-credential", path.display())
        }
        _ => forge.credential_helper(),
    }
}

/// The token variables a forge CLI would log in with in place of its own stored login.
/// [`gh_as_the_operator`] withholds them.
pub const TOKEN_ENV: [&str; 4] = [
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
];

/// What the CLI child is given: its credential environment, and nothing else of charter's.
///
/// **Unchanged by charter-app#134, deliberately.** `find_cli` above now looks in more places;
/// what the child is then handed is still the directory that search landed in plus
/// `git::GIT_DIRS`, and never this process's own `PATH`. Widening a lookup is not the same act
/// as widening what the program found can reach.
///
/// Every credential variable `keep` accepts is passed on; [`call`] keeps them all.
pub(super) fn cli_env_keeping(
    cli_dir: Option<&Path>,
    keep: impl Fn(&str) -> bool,
) -> Vec<(String, String)> {
    let mut dirs: Vec<String> = Vec::new();
    if let Some(dir) = cli_dir {
        dirs.push(dir.display().to_string());
    }
    dirs.extend(git::GIT_DIRS.iter().map(|d| (*d).to_string()));
    let mut env = vec![
        ("PATH".to_string(), git::path_value(&dirs)),
        ("LC_ALL".to_string(), "C".to_string()),
        ("NO_COLOR".to_string(), "1".to_string()),
        // Nothing here has a terminal to answer a prompt on, and a prompt nobody can see is
        // an endless wait.
        ("GH_PROMPT_DISABLED".to_string(), "1".to_string()),
        ("NO_PROMPT".to_string(), "1".to_string()),
        ("GH_NO_UPDATE_NOTIFIER".to_string(), "1".to_string()),
        ("GLAB_CHECK_UPDATE".to_string(), "false".to_string()),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        env.push(("HOME".to_string(), home.to_string_lossy().into_owned()));
    }
    for name in git::CREDENTIAL_ENV.into_iter().filter(|name| keep(name)) {
        if let Some(value) = std::env::var_os(name) {
            env.push((name.to_string(), value.to_string_lossy().into_owned()));
        }
    }
    env
}

/// `gh` with `args`, logged in as the operator's own `gh` login and never as a token in this
/// process's environment: it is given none of [`TOKEN_ENV`].
///
/// For `charter report`, which files on charter's own tracker under the reporter's own
/// identity (charter-plane ADR 0001). A chat can hold a plane's or a vault's token in
/// `GH_TOKEN`, and `gh` prefers that variable to its stored login, so an issue filed with it
/// would appear under whoever owns the token. Returns stdout on exit 0; anything else is an
/// error carrying `gh`'s own words, because a write that fails must fail loudly.
///
/// **Not a forge operation, and not behind the seam** (`docs/forges.md`, the parity table):
/// it files on charter's own tracker, which is on GitHub whatever forge the project uses, and it
/// speaks `gh`'s own `search issues` and `issue create`. It moves when the work-item area
/// (FW-6a/b) exists.
pub fn gh_as_the_operator(args: &[String], timeout: Duration) -> Result<String, ForgeError> {
    // Listed as Charter's: it reads and writes Charter's own tracker (OB-15).
    let path = format!("repos/{}/issues", crate::report::UPSTREAM);
    let listed = cli::Listed {
        feature: crate::netlog::Feature::Report,
        host: "api.github.com",
        method: "CLI",
        path: &path,
    };
    match cli::Cli::as_the_operator().run_listed(Kind::GitHub, args, timeout, listed) {
        Ok(answer) if answer.ok() => Ok(answer.out),
        Ok(answer) => Err(ForgeError::new(answer.said(Kind::GitHub))),
        Err(
            transport::NoAnswer::Timeout(why)
            | transport::NoAnswer::Missing(why)
            | transport::NoAnswer::Refused(why)
            | transport::NoAnswer::HeldBack { said: why, .. },
        ) => Err(ForgeError::transport(why)),
    }
}

/// `urllib.parse.quote(value, safe="")`.
pub fn quote(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------
// The status-line pair: an open change, and the branch's last CI result                   #
// ---------------------------------------------------------------------------------------

/// A JSON shape the call did not expect, where **Python raised**.
///
/// Carried rather than folded into "no answer", because `glstate.state_for_repo` catches the
/// exception around all three fields at once: a forge that answers `{"message": "…"}` where a
/// list was asked for blanks the change, the CI word **and** the sigil together, and a port
/// that answered "no change, CI as read" would write a different entry to the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Raised;

/// Python's truthiness, which is what `if not arr: return None` asks.
pub(super) fn falsy(value: &Value) -> bool {
    !truthy(value)
}

/// `arr[0].get(key)` as Python evaluates it on whatever `_api` came back with.
///
/// `Ok(None)` is Python's `if arr else None` — nothing to report. `Err(Raised)` is every
/// shape where Python would have thrown: a dict (`KeyError`), a string or a number
/// (`TypeError`/`AttributeError`), or a list whose first element is not a dict.
pub(super) fn first_field(answer: Option<Value>, key: &str) -> Result<Option<Value>, Raised> {
    let Some(answer) = answer else {
        return Ok(None);
    };
    if falsy(&answer) {
        return Ok(None);
    }
    let Some(items) = answer.as_array() else {
        return Err(Raised);
    };
    // Non-empty by `falsy` above.
    let first = items.first().ok_or(Raised)?;
    match first.as_object() {
        Some(row) => Ok(row.get(key).cloned()),
        None => Err(Raised),
    }
}

/// A mapping table's answer for `word`, or `None` for a word it does not list.
pub(super) fn mapped(table: &[(&str, &str)], word: &str) -> Option<String> {
    table
        .iter()
        .find(|(from, _)| *from == word)
        .map(|(_, to)| (*to).to_string())
}

/// `value or ""` for a field that is meant to be a word: Python looks the falsy ones up as
/// the empty string, which no map lists.
pub(super) fn word_of(value: Option<&Value>) -> &str {
    match value {
        Some(Value::String(word)) => word,
        _ => "",
    }
}

/// A listed repo's `key` when it is a non-empty string, else `""`.
pub(super) fn listed_str(raw: &Value, key: &str) -> String {
    raw.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A listed repo's description: a string as given, any other truthy value as Python's `str()`
/// prints it, and `""` for a missing or falsy one.
pub(super) fn listed_description(raw: &Value) -> String {
    match raw.get("description") {
        Some(Value::String(s)) => s.clone(),
        Some(v) if truthy(v) => py_str(v),
        _ => String::new(),
    }
}

/// A listed repo's id, as text, for the forge to be asked by later. `None` when it is missing
/// or null.
pub(super) fn listed_id(raw: &Value) -> Option<ForgeRef> {
    match raw.get("id") {
        None | Some(Value::Null) => None,
        Some(v) => Some(ForgeRef(py_str(v))),
    }
}

/// A listed repo's default branch, `None` when it is missing or empty.
pub(super) fn listed_branch(raw: &Value) -> Option<String> {
    Some(listed_str(raw, "default_branch")).filter(|b| !b.is_empty())
}

/// A listed repo's topics: the strings of its `topics` array.
pub(super) fn listed_topics(raw: &Value) -> Vec<String> {
    raw.get("topics")
        .and_then(Value::as_array)
        .map(|topics| {
            topics
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// A forge's answer as JSON; an empty body is `[]`, a legal and successful answer.
pub(super) fn parse(kind: Kind, out: &str, path: &str) -> Result<Value, ForgeError> {
    if out.trim().is_empty() {
        return Ok(Value::Array(Vec::new()));
    }
    serde_json::from_str(out).map_err(|e| {
        ForgeError::new(format!(
            "{} API returned malformed JSON ({path}): {e}",
            kind.display()
        ))
    })
}

/// Python's truth test on a JSON value.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Python's `str()` of a JSON value, for the few places a record field is interpolated —
/// [`crate::pyrepr::str_json`].
///
/// **The container arm is why this is a delegation now.** It used to be `other.to_string()`,
/// which is `serde_json`'s writer: a forge record whose field held a list came back as
/// `["a","b"]` where Python writes `['a', 'b']`, and a number came back as the literal the
/// file held rather than as the value Python read out of it.
pub fn py_str(value: &Value) -> String {
    crate::pyrepr::str_json(value)
}

#[cfg(test)]
mod tests {

    /// `gh_as_the_operator` withholds only credential variables a forge call would pass.
    #[test]
    fn every_token_withheld_is_one_a_forge_call_would_pass() {
        for name in TOKEN_ENV {
            assert!(git::CREDENTIAL_ENV.contains(&name), "{name}");
        }
    }

    use super::*;

    #[test]
    fn a_listed_description_that_is_not_text_is_kept_as_python_would_print_it() {
        // Stricter-than-a-crash rather than a port: Python's `(… or "").strip()` raises
        // AttributeError on a truthy non-string and takes `discover` down with it. The Rust
        // keeps `str()` of it instead, which is what this pins, and a falsy one is `""`.
        let described = |d: Value| listed_description(&serde_json::json!({ "description": d }));
        assert_eq!(described(serde_json::json!(5)), "5");
        assert_eq!(described(serde_json::json!(0)), "");
        assert_eq!(described(Value::Null), "");
        assert_eq!(described(serde_json::json!(" kept ")), " kept ");
    }

    #[test]
    fn a_listed_default_branch_that_is_null_or_empty_is_none() {
        // `p.get("default_branch") or …`: both are falsy, and neither reads as a branch.
        for empty in [Value::Null, serde_json::json!("")] {
            let raw = serde_json::json!({ "default_branch": empty });
            assert_eq!(listed_branch(&raw), None, "{empty}");
        }
        let raw = serde_json::json!({ "default_branch": "trunk" });
        assert_eq!(listed_branch(&raw).as_deref(), Some("trunk"));
    }

    /// charter-app#100's `PATH` joined with `:`, as it bit on unix: a forge CLI found under a
    /// `$HOME` with a colon in it — one of `programs::USER_BIN` — used to hand its child a
    /// `PATH` whose second half was a relative directory.
    #[cfg(unix)]
    #[test]
    fn a_forge_clis_child_is_never_handed_a_relative_path_entry() {
        let env = cli_env_keeping(Some(Path::new("/home/a:b/.local/bin")), |_| true);
        let path = &env.iter().find(|(k, _)| k == "PATH").expect("a PATH").1;
        for dir in std::env::split_paths(path) {
            assert!(dir.is_absolute(), "{} in {path}", dir.display());
        }
        assert_eq!(
            std::env::split_paths(path).collect::<Vec<_>>(),
            git::GIT_DIRS.iter().map(PathBuf::from).collect::<Vec<_>>(),
            "the fixed directories, and nothing of the directory that could not be written"
        );
    }

    #[test]
    fn a_host_is_a_hostname_and_nothing_that_merely_fits_in_the_slot() {
        for good in [
            "github.com",
            "git.internal",
            "gitlab.example.com:8443",
            "a-b.c1",
            // Five digits is the longest port Python's `:\d{1,5}` takes.
            "host:12345",
        ] {
            assert!(host_ok(good), "{good}");
        }
        for bad in [
            "",
            "https://github.com",
            "github.com/acme",
            "git@github.com",
            "-evil.com",
            "evil-.com",
            "a..b",
            "host:",
            "host:123456",
            "host name",
        ] {
            assert!(!host_ok(bad), "{bad}");
        }
    }

    #[test]
    fn the_host_is_read_from_the_host_component_and_never_from_the_path() {
        assert_eq!(host_of("https://github.com/acme/x.git"), "github.com");
        assert_eq!(host_of("https://user@GitHub.com:443/acme/x"), "github.com");
        assert_eq!(host_of("git@github.com:acme/x.git"), "github.com");
        assert_eq!(host_of("ssh://git@gitlab.com/acme/x.git"), "gitlab.com");
        // FINDING 1 in Python: a known host inside the PATH is not the host.
        assert_eq!(
            host_of("git@git.internal:gitlab.com-mirror/api.git"),
            "git.internal"
        );
        assert_eq!(host_of("file:///srv/forge/acme/x.git"), "");
        assert_eq!(host_of("/srv/forge/x"), "");
        assert_eq!(host_of(""), "");
    }

    /// Python's `_URL_HOST_RE` and `_SCP_HOST_RE`, each value below read off
    /// `registry._host_of` itself.
    #[test]
    fn the_host_is_what_pythons_two_patterns_match_and_nothing_else() {
        for (url, host) in [
            // A scheme starts with a letter, so this is not one — and not scp-like either,
            // because its colon is followed by `//`.
            ("1ab://host/x", ""),
            ("user@host://x", ""),
            // Userinfo is `[^@/\s]+@`: a `/` or a space in it, or nothing before the `@`,
            // and the host is whatever runs up to the first `/`, `:` or space.
            ("a/b@host:x", ""),
            ("a b@host:x", ""),
            ("x@a/b:c", ""),
            ("@host:path", "@host"),
            ("ab@c@host:x", "c@host"),
            // scp-like needs a host AND the colon after it.
            ("abc", ""),
            (":path", ""),
            ("a:b", "a"),
            ("h:/x", "h"),
            (" git@Host:x ", "host"),
        ] {
            assert_eq!(host_of(url), host, "{url:?}");
        }
    }

    /// `registry.namespace_of`, which is `urllib.parse.urlparse(url).path` for a URL with a
    /// scheme — each value read off Python.
    #[test]
    fn the_namespace_is_the_path_python_parses_out_of_the_remote() {
        for (url, namespace) in [
            (
                "https://gitlab.com/group/sub/repo.git",
                Some("group/sub/repo"),
            ),
            ("git@gitlab.com:group/repo.git", Some("group/repo")),
            ("git@h:/a/b/", Some("a/b")),
            ("", None),
            ("   ", None),
            ("noslash", None),
            ("https://h/", None),
            ("git@h:", None),
            // No path at all: the netloc is cut at the query or the fragment.
            ("https://host", None),
            ("https://host?q=/x", None),
            ("https://host#/a/b", None),
            // `;params` come off the LAST segment, for the schemes that use them.
            ("https://h/a/b;x", Some("a/b")),
            ("https://h/a/b;x/c;y", Some("a/b;x/c")),
            ("ftp://h/a;b", Some("a")),
            ("git://h/a;b", Some("a;b")),
            ("https://h/a/b?page=2#top", Some("a/b")),
            // `urlsplit` drops tabs and newlines before it parses.
            ("https://h/a\tb/c.git", Some("ab/c")),
        ] {
            assert_eq!(namespace_of(url).as_deref(), namespace, "{url:?}");
        }
    }

    #[test]
    fn a_plane_format_newer_than_this_charter_or_not_a_number_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let at = |schema: &str| {
            std::fs::write(dir.path().join("charter.toml"), schema).unwrap();
            load_config(dir.path())
        };
        assert!(at("").unwrap().is_empty());
        assert_eq!(at("schema = 1\n").unwrap()["schema"].as_integer(), Some(1));
        assert!(
            at("schema = 0\n").is_ok(),
            "an older format is still placed"
        );
        assert!(
            at("schema = 2\n").is_ok(),
            "schema 2 is this charter's own (FR-24)"
        );
        let newer = at("schema = 3\n").unwrap_err();
        assert!(
            newer.ends_with(
                "declares schema 3, but this charter understands 2. Upgrade charter: update the app."
            ),
            "{newer}"
        );
        assert!(at("schema = 4\n").is_err());
        let word = at("schema = \"one\"\n").unwrap_err();
        assert!(
            word.contains("which is not a project format version this charter can compare"),
            "{word}"
        );
        assert!(at("schema = [").unwrap_err().contains("is not valid TOML"));
        let none = tempfile::tempdir().unwrap();
        assert_eq!(load_config(none.path()), Ok(toml::Table::new()));
    }

    /// Whose words name a request in a clone whose origin is `origin` (none when `None`), in a
    /// plane that says `toml`.
    fn request_words_with(toml: &str, origin: Option<&str>) -> Kind {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), toml).unwrap();
        assert!(crate::testgit::run(dir.path(), &["init", "-q", "."]).ok());
        if let Some(url) = origin {
            assert!(crate::testgit::run(dir.path(), &["remote", "add", "origin", url]).ok());
        }
        request_words_of(dir.path(), dir.path())
    }

    #[test]
    fn a_request_is_named_by_the_forge_an_https_origin_is_on() {
        assert_eq!(
            request_words_with("", Some("https://gitlab.com/group/sub/repo.git")),
            Kind::GitLab
        );
        assert_eq!(
            request_words_with("", Some("https://github.com/acme/widget.git")),
            Kind::GitHub
        );
    }

    #[test]
    fn a_request_on_a_self_managed_gitlab_the_plane_declares_is_a_merge_request() {
        let declared = "[[forge]]\nkind = \"gitlab\"\nhost = \"git.acme.test\"\n";
        assert_eq!(
            request_words_with(declared, Some("git@git.acme.test:group/repo.git")),
            Kind::GitLab
        );
        assert_eq!(
            request_words_with(declared, Some("https://git.acme.test/group/repo.git")),
            Kind::GitLab
        );
    }

    #[test]
    fn a_request_on_an_unknown_host_or_with_no_origin_keeps_githubs_words() {
        assert_eq!(
            request_words_with("", Some("https://git.acme.test/group/repo.git")),
            Kind::GitHub
        );
        assert_eq!(request_words_with("", None), Kind::GitHub);
    }

    #[test]
    fn a_forge_entry_that_is_not_a_table_costs_only_itself() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("charter.toml"),
            "forge = [\"junk\", { kind = \"github\", host = \"ghe.internal\" }]\n",
        )
        .unwrap();

        let known = known(dir.path());
        assert_eq!(
            known.get("ghe.internal"),
            Some(&Forge::build("github", Some("ghe.internal")).unwrap())
        );
        assert_eq!(known.len(), 3, "{known:?}");
        let ordered: Vec<String> = known_ordered(dir.path())
            .into_iter()
            .map(|f| f.host)
            .collect();
        assert_eq!(ordered, ["gitlab.com", "github.com", "ghe.internal"]);
    }

    #[test]
    fn a_change_is_numbered_with_its_forges_own_sigil() {
        assert_eq!(Kind::GitHub.change_sigil(), "#");
        assert_eq!(Kind::GitLab.change_sigil(), "!");
    }

    #[test]
    fn a_request_is_named_in_its_forges_own_words() {
        assert_eq!(
            (
                Kind::GitHub.request_noun_capitalised(),
                Kind::GitHub.request_ref(12)
            ),
            ("Pull request", "#12".to_string())
        );
        assert_eq!(
            (
                Kind::GitLab.request_noun_capitalised(),
                Kind::GitLab.request_ref(12)
            ),
            ("Merge request", "!12".to_string())
        );
    }

    #[test]
    fn quote_encodes_everything_but_the_unreserved_characters() {
        assert_eq!(quote("acme"), "acme");
        assert_eq!(quote("a/b c"), "a%2Fb%20c");
        assert_eq!(quote("feature/x"), "feature%2Fx");
        assert_eq!(quote("ü"), "%C3%BC");
        assert_eq!(quote("a_b.c-d~e"), "a_b.c-d~e");
    }

    #[test]
    fn a_block_with_an_unknown_kind_or_a_host_that_is_not_one_refuses_discover() {
        let cfg: toml::Table = "[[forge]]\nkind = \"gitea\"\nowner = \"acme\"\n"
            .parse()
            .unwrap();
        assert_eq!(
            to_query(&cfg).unwrap_err(),
            "unknown forge kind 'gitea' — known kinds: github, gitlab"
        );
        let cfg: toml::Table = "[[forge]]\nkind = \"github\"\nhost = \"https://evil\"\n"
            .parse()
            .unwrap();
        assert!(
            to_query(&cfg)
                .unwrap_err()
                .starts_with("host 'https://evil' is not a hostname")
        );
    }

    #[test]
    fn a_plane_that_declares_no_forge_queries_gitlab_as_it_always_did() {
        let cfg = toml::Table::new();
        let q = to_query(&cfg).unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].0, Forge::default_of(Kind::GitLab));
    }

    #[test]
    fn owner_is_group_first_then_owner() {
        let cfg: toml::Table =
            "[[forge]]\nkind = \"github\"\nowner = \"o\"\ngroup = \"g\"\nexclude = [\"x\"]\n"
                .parse()
                .unwrap();
        let q = to_query(&cfg).unwrap();
        assert_eq!(q[0].1, "g");
        assert_eq!(q[0].2, vec!["x".to_string()]);
        assert_eq!(group_of(&cfg, 0), "g");
    }

    #[test]
    fn each_forge_rewrites_its_own_ssh_forms_to_its_own_https_base() {
        let f = Forge::build("gitlab", Some("git.internal")).unwrap();
        assert_eq!(
            f.insteadof(),
            (
                "https://git.internal/".to_string(),
                [
                    "git@git.internal:".to_string(),
                    "ssh://git@git.internal/".to_string()
                ]
            )
        );
        assert_eq!(f.credential_helper(), "!glab auth git-credential");
    }
}
