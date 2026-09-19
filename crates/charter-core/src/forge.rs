//! The forges a plane declares, and asking them what repos exist.
//!
//! A port of `charter/forge/` — the registry, and the parts of the GitHub and GitLab backends
//! `discover` and `clone` use: authentication, enumerating an owner's repos, a repo's
//! top-level file list, and the credential helper and SSH→HTTPS rewrite each forge's clones
//! get. The status-line and change-surface calls are not here; nothing in the Rust charter
//! asks them yet.
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
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

use crate::worktree::git;

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

    /// What an owner is called on this forge.
    pub fn owner_noun(self) -> &'static str {
        match self {
            Kind::GitHub => "org",
            Kind::GitLab => "group",
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

/// The sentence a host that is not a hostname is refused with. Python's `NOT_A_HOST`.
fn not_a_host(host: &str) -> String {
    format!(
        "host {} is not a hostname. It is read from a committed charter.toml and reaches both \
         the SSH guard's deny set and the `url.https://<host>/.insteadOf` that `charter \
         git-policy --apply` writes into a clone's git config, so it takes a bare host \
         (optionally :port) — no scheme, no path, no '@'",
        crate::pyrepr::repr_str(host)
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
                && bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'-')
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

// ---------------------------------------------------------------------------------------
// charter.toml                                                                            #
// ---------------------------------------------------------------------------------------

/// The plane format version this charter understands. Python's `instance.SCHEMA`.
const SCHEMA: i64 = 1;

/// `charter.toml`, parsed, or `{}` when there is none. Python's `instance.load`, including its
/// refusal of a plane format this charter cannot place.
pub fn load_config(root: &Path) -> Result<toml::Table, String> {
    let path = root.join(crate::plane::MANIFEST);
    let Ok(raw) = std::fs::read(&path) else {
        return Ok(toml::Table::new());
    };
    let text = String::from_utf8(raw)
        .map_err(|e| format!("{} is not valid TOML: {e}", path.display()))?;
    let cfg: toml::Table = text
        .parse()
        .map_err(|e| format!("{} is not valid TOML: {e}", path.display()))?;
    match cfg.get("schema") {
        None => {}
        Some(toml::Value::Integer(found)) if *found > SCHEMA => {
            return Err(format!(
                "{} declares schema {found}, but this charter understands {SCHEMA}. Upgrade \
                 charter.",
                path.display()
            ));
        }
        Some(toml::Value::Integer(_)) => {}
        Some(other) => {
            return Err(format!(
                "{} declares schema {other}, which is not a plane format version this charter \
                 can compare against {SCHEMA}. charter will not operate on a plane whose \
                 format it cannot place. Fix the `schema` line, or upgrade charter.",
                path.display()
            ));
        }
    }
    Ok(cfg)
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
    let mut out: BTreeMap<String, Forge> = KINDS
        .iter()
        .map(|k| (k.default_host().to_string(), Forge::default_of(*k)))
        .collect();
    if let Ok(cfg) = load_config(root) {
        out.extend(declared(&cfg));
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

/// A forge CLI failed, was missing, or answered something unusable. Python's `ForgeError`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ForgeError(pub String);

/// The forge CLI `name`, as an absolute path — the operator's `PATH` first (see the module
/// docs for why), then the fixed directories git is looked for in.
pub fn find_cli(name: &str) -> Option<PathBuf> {
    let from_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    from_path
        .into_iter()
        .chain(git::GIT_DIRS.iter().map(PathBuf::from))
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
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

/// What the CLI child is given: its credential environment, and nothing else of charter's.
fn cli_env(cli_dir: Option<&Path>) -> Vec<(String, String)> {
    let mut dirs: Vec<String> = Vec::new();
    if let Some(dir) = cli_dir {
        dirs.push(dir.display().to_string());
    }
    dirs.extend(git::GIT_DIRS.iter().map(|d| (*d).to_string()));
    let mut env = vec![
        ("PATH".to_string(), dirs.join(":")),
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
    for name in git::CREDENTIAL_ENV {
        if let Some(value) = std::env::var_os(name) {
            env.push((name.to_string(), value.to_string_lossy().into_owned()));
        }
    }
    env
}

/// What one CLI call answered.
struct Answer {
    code: i32,
    out: String,
    err: String,
}

/// How one CLI call failed to answer at all.
enum NoAnswer {
    /// The deadline passed. Carries Python's `ProcTimeout` sentence.
    Timeout(String),
    /// It could not be started.
    Missing(String),
}

/// Run the forge's CLI with `args`, under `timeout`.
fn call(kind: Kind, args: &[String], timeout: Duration) -> Result<Answer, NoAnswer> {
    let cli = kind.cli();
    let Some(path) = find_cli(cli) else {
        return Err(NoAnswer::Missing(format!(
            "charter could not find {cli} on PATH — install it and log in (`{cli} auth login`)"
        )));
    };
    let mut cmd = Command::new(&path);
    cmd.args(args)
        .env_clear()
        .envs(cli_env(path.parent()))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = cmd
        .spawn()
        .map_err(|e| NoAnswer::Missing(format!("charter could not run {cli}: {e}")))?;
    let run = git::wait(child, timeout)
        .map_err(|e| NoAnswer::Missing(format!("charter could not run {cli}: {e}")))?;
    match run.code {
        Some(code) => Ok(Answer {
            code,
            out: run.out,
            err: run.err,
        }),
        None => Err(NoAnswer::Timeout(format!(
            "timed out after {}s: {cli} {}",
            timeout.as_secs(),
            args.join(" ")
        ))),
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_string()).collect()
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

/// A failed call's own words: stderr, else stdout, else the exit. Python's `detail`.
fn detail(kind: Kind, answer: &Answer) -> String {
    let said = if !answer.err.trim().is_empty() {
        answer.err.trim()
    } else {
        answer.out.trim()
    };
    if said.is_empty() {
        format!("{} exited {}", kind.cli(), answer.code)
    } else {
        said.to_string()
    }
}

impl Forge {
    /// `gh api --hostname H <path>` or `glab --hostname H api <path>`.
    fn api_args(&self, path: &str) -> Vec<String> {
        match self.kind {
            Kind::GitHub => strings(&["api", "--hostname", &self.host, path]),
            Kind::GitLab => strings(&["--hostname", &self.host, "api", path]),
        }
    }

    /// Refuse unless the CLI is installed and logged in for this host.
    pub fn check_auth(&self) -> Result<(), ForgeError> {
        let cli = self.kind.cli();
        let args = match self.kind {
            Kind::GitHub => strings(&["auth", "status", "--hostname", &self.host]),
            Kind::GitLab => strings(&["--hostname", &self.host, "auth", "status"]),
        };
        let answer = match call(self.kind, &args, STATUS_TIMEOUT) {
            Ok(answer) => answer,
            Err(NoAnswer::Timeout(why)) => {
                return Err(ForgeError(format!(
                    "{cli} did not answer for {}: {why}",
                    self.host
                )));
            }
            Err(NoAnswer::Missing(why)) => return Err(ForgeError(why)),
        };
        let logged_in = match self.kind {
            Kind::GitHub => answer.code == 0,
            // glab exits 0 while logged in to nothing, so its own words are asked too.
            Kind::GitLab => {
                answer.code == 0 && format!("{}{}", answer.out, answer.err).contains("Logged in")
            }
        };
        if logged_in {
            Ok(())
        } else {
            Err(ForgeError(format!(
                "{cli} is not authenticated for {}. Run: {cli} auth login",
                self.host
            )))
        }
    }

    /// One strict JSON GET: a failure raises and never reads as "empty". Python's
    /// `_api_strict`, in each backend's own vocabulary.
    fn api_strict(&self, path: &str, what_failed: &str) -> Result<Value, ForgeError> {
        let answer = match call(self.kind, &self.api_args(path), LIST_TIMEOUT) {
            Ok(answer) => answer,
            Err(NoAnswer::Timeout(why)) => {
                return Err(ForgeError(format!("{what_failed} ({path}) {why}")));
            }
            Err(NoAnswer::Missing(why)) => return Err(ForgeError(why)),
        };
        if answer.code != 0 {
            return Err(ForgeError(format!(
                "{what_failed} failed ({path}): {}",
                detail(self.kind, &answer)
            )));
        }
        parse(self.kind, &answer.out, path)
    }

    /// Every repo under `owner`, normalised to the record shape Python's backends produce.
    pub fn list_repos(&self, owner: &str) -> Result<Vec<Value>, ForgeError> {
        let enc = quote(owner);
        let raw = match self.kind {
            Kind::GitHub => match self.paged_github(&format!("orgs/{enc}/repos"), owner, true) {
                Err(Paged::NotAnOrg) => {
                    // A personal account 404s on the org endpoint with an identical record
                    // shape on the user one. Only a real 404 falls back; any other failure
                    // is a failure.
                    match self.paged_github(&format!("users/{enc}/repos"), owner, false) {
                        Ok(items) => items,
                        Err(Paged::Failed(e)) => return Err(e),
                        Err(Paged::NotAnOrg) => unreachable!("only the org probe says this"),
                    }
                }
                Err(Paged::Failed(e)) => return Err(e),
                Ok(items) => items,
            },
            Kind::GitLab => self.paged_gitlab(owner)?,
        };
        Ok(raw.iter().map(|r| self.normalize(r)).collect())
    }

    fn paged_github(&self, base: &str, owner: &str, org_probe: bool) -> Result<Vec<Value>, Paged> {
        let mut out = Vec::new();
        let mut page = 1;
        loop {
            let path = format!("{base}?per_page=100&page={page}");
            let answer = match call(self.kind, &self.api_args(&path), LIST_TIMEOUT) {
                Ok(answer) => answer,
                Err(NoAnswer::Timeout(why)) => {
                    return Err(Paged::Failed(ForgeError(format!(
                        "listing repos for GitHub owner '{owner}' {why}"
                    ))));
                }
                Err(NoAnswer::Missing(why)) => return Err(Paged::Failed(ForgeError(why))),
            };
            if answer.code != 0 {
                let blob = format!("{} {}", answer.out, answer.err);
                if org_probe
                    && page == 1
                    && (blob.contains("HTTP 404") || blob.contains("\"status\":\"404\""))
                {
                    return Err(Paged::NotAnOrg);
                }
                return Err(Paged::Failed(ForgeError(format!(
                    "listing repos for GitHub owner '{owner}' failed ({path}): {}",
                    detail(self.kind, &answer)
                ))));
            }
            let batch = parse(self.kind, &answer.out, &path).map_err(Paged::Failed)?;
            let items = batch.as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                break;
            }
            let short = items.len() < 100;
            out.extend(items);
            if short {
                break;
            }
            page += 1;
        }
        Ok(out)
    }

    fn paged_gitlab(&self, owner: &str) -> Result<Vec<Value>, ForgeError> {
        let enc = quote(owner);
        let mut out = Vec::new();
        let mut page = 1;
        loop {
            let path = format!(
                "groups/{enc}/projects?per_page=100&page={page}&include_subgroups=true&archived=false"
            );
            let batch = self
                .api_strict(&path, "GitLab API call")
                .map_err(|e| {
                    ForgeError(format!("listing repos for GitLab group '{owner}' failed: {e}"))
                })?;
            let items = batch.as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                break;
            }
            let short = items.len() < 100;
            out.extend(items);
            if short {
                break;
            }
            page += 1;
        }
        Ok(out)
    }

    /// One forge record in the shape every backend produces (Python's `_normalize`).
    fn normalize(&self, raw: &Value) -> Value {
        let get = |key: &str| raw.get(key).cloned().unwrap_or(Value::Null);
        let text_or_empty = |key: &str| match raw.get(key) {
            Some(v) if truthy(v) => v.clone(),
            _ => Value::String(String::new()),
        };
        let (name, pwn, ssh) = match self.kind {
            Kind::GitHub => (get("name"), get("full_name"), text_or_empty("ssh_url")),
            Kind::GitLab => (
                match raw.get("path") {
                    Some(v) if truthy(v) => v.clone(),
                    _ => get("name"),
                },
                get("path_with_namespace"),
                text_or_empty("ssh_url_to_repo"),
            ),
        };
        let web = match self.kind {
            Kind::GitHub => text_or_empty("html_url"),
            Kind::GitLab => text_or_empty("web_url"),
        };
        let topics = match raw.get("topics") {
            Some(v) if truthy(v) => v.clone(),
            _ => Value::Array(Vec::new()),
        };
        serde_json::json!({
            "id": get("id"),
            "name": name,
            "path_with_namespace": pwn,
            "default_branch": get("default_branch"),
            "description": text_or_empty("description"),
            "web_url": web,
            "ssh_url": ssh,
            "topics": topics,
            "forge": self.kind.word(),
        })
    }

    /// The top-level file names of a repo, raising on any failure so a failed probe never
    /// reads as "no recognised stack". Python's `repo_tree_strict`.
    pub fn repo_tree_strict(&self, repo: &Value, git_ref: Option<&str>) -> Result<Vec<String>, ForgeError> {
        match self.kind {
            Kind::GitHub => {
                let path = repo
                    .get("path_with_namespace")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let (owner, name) = path.split_once('/').unwrap_or((path, ""));
                let git_ref = git_ref
                    .filter(|r| !r.is_empty())
                    .or_else(|| repo.get("default_branch").and_then(Value::as_str).filter(|r| !r.is_empty()))
                    .unwrap_or("HEAD");
                let api = format!(
                    "repos/{}/{}/git/trees/{}",
                    quote(owner),
                    quote(name),
                    quote(git_ref)
                );
                let answer = match call(self.kind, &self.api_args(&api), LIST_TIMEOUT) {
                    Ok(answer) => answer,
                    Err(NoAnswer::Timeout(why)) => {
                        return Err(ForgeError(format!("listing tree for {path}@{git_ref} {why}")));
                    }
                    Err(NoAnswer::Missing(why)) => return Err(ForgeError(why)),
                };
                if answer.code != 0 {
                    return Err(ForgeError(format!(
                        "listing tree for {path}@{git_ref} failed: {}",
                        detail(self.kind, &answer)
                    )));
                }
                if answer.out.trim().is_empty() {
                    return Ok(Vec::new());
                }
                let data: Value = serde_json::from_str(&answer.out).map_err(|e| {
                    ForgeError(format!(
                        "GitHub API returned malformed JSON (tree {path}@{git_ref}): {e}"
                    ))
                })?;
                Ok(data
                    .get("tree")
                    .and_then(Value::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .map(|e| e.get("path").and_then(Value::as_str).unwrap_or_default())
                            .filter(|p| !p.contains('/'))
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default())
            }
            Kind::GitLab => {
                let rid = quote(&py_str(repo.get("id").unwrap_or(&Value::Null)));
                let ref_q = git_ref
                    .filter(|r| !r.is_empty())
                    .map(|r| format!("&ref={}", quote(r)))
                    .unwrap_or_default();
                let mut out = Vec::new();
                let mut page = 1;
                loop {
                    let path =
                        format!("projects/{rid}/repository/tree?per_page=100&page={page}{ref_q}");
                    let batch = self.api_strict(&path, "GitLab API call")?;
                    let items = batch.as_array().cloned().unwrap_or_default();
                    if items.is_empty() {
                        break;
                    }
                    let short = items.len() < 100;
                    out.extend(items.iter().map(|e| {
                        e.get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string()
                    }));
                    if short {
                        break;
                    }
                    page += 1;
                }
                Ok(out)
            }
        }
    }
}

enum Paged {
    NotAnOrg,
    Failed(ForgeError),
}

/// A CLI's stdout as JSON; an empty body is `[]`, a legal and successful answer.
fn parse(kind: Kind, out: &str, path: &str) -> Result<Value, ForgeError> {
    if out.trim().is_empty() {
        return Ok(Value::Array(Vec::new()));
    }
    serde_json::from_str(out).map_err(|e| {
        ForgeError(format!(
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

/// Python's `str()` of a JSON value, for the few places a record field is interpolated.
pub fn py_str(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_a_hostname_and_nothing_that_merely_fits_in_the_slot() {
        for good in ["github.com", "git.internal", "gitlab.example.com:8443", "a-b.c1"] {
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
        let cfg: toml::Table = "[[forge]]\nkind = \"gitea\"\nowner = \"acme\"\n".parse().unwrap();
        assert_eq!(
            to_query(&cfg).unwrap_err(),
            "unknown forge kind 'gitea' — known kinds: github, gitlab"
        );
        let cfg: toml::Table =
            "[[forge]]\nkind = \"github\"\nhost = \"https://evil\"\n".parse().unwrap();
        assert!(to_query(&cfg).unwrap_err().starts_with("host 'https://evil' is not a hostname"));
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
