//! `charter report`: a bug or a feature request, drafted here and filed on charter's own
//! tracker under the reporter's own `gh` login (#363).
//!
//! # What this module promises
//!
//! **Nothing leaves before it has been shown.** A [`Draft`] is built, scrubbed and rendered
//! ([`Draft::preview`]) with no network at all. Filing it ([`file`]) is a separate call, and the
//! command line reaches it only with the reporter's "yes": a prompt on a terminal, or
//! `--yes <digest>` where the digest ([`Draft::digest`]) is the one the preview printed. A digest
//! is over the repository, the title and the body, so a `--yes` can only file the exact bytes
//! that were on screen: change one character and the digest no longer matches, and nothing is
//! sent. That is the answer to charter-plane ADR 0003's objection to a `--yes` flag (*"a flag the
//! agent can pass is a flag the agent will pass unprompted"*): the flag cannot be passed without
//! first printing the very text it would publish.
//!
//! **The reporter's own identity, never a shared token** (charter-plane ADR 0001, carried over by
//! the operator's ruling on #363). `gh` prefers `GH_TOKEN` to its stored login, and a chat can
//! hold a plane's or a vault's token there, so every call here goes through
//! [`forge::gh_as_the_operator`], which withholds the token variables.
//!
//! **Scrubbed before it is shown** ([`Known::scrub`]), by what charter can positively identify:
//! a line [`crate::secretshape`] reads as a credential, the value of any variable in this
//! process's environment (which is where a vault's values are when a chat holds them), the
//! plane's path and home-directory paths, and the names of this plane's workspaces, personas,
//! clones and vaults. Each removal leaves a visible placeholder, and the preview names the
//! categories, because the reporter's read is the other half of the scrub: nothing mechanical
//! catches "our billing service cannot do X".
//!
//! # A saved panic becomes a draft bug
//!
//! The app appends every panic to `panics.log` in its log directory (`app/src-tauri/src/panics.rs`).
//! [`latest_panic`] reads the newest block back, and [`Draft::of_panic`] keeps a closed set of
//! its fields: where it panicked, its message (scrubbed, and flagged as free text), and the
//! charter version the record names. The thread and the backtrace are dropped — a backtrace
//! carries the reporter's paths, and without symbols it is only addresses.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use regex::Regex;
use sha2::{Digest, Sha256};

use crate::forge::{self, ForgeError};

/// The repository reports are filed on.
pub const UPSTREAM: &str = "diazoxide/charter";

/// The most characters a report body may hold. GitHub refuses a body over 65,536.
pub const BODY_MAX: usize = 60_000;

/// The longest an issue title may be, ellipsis included: the maintainer's issue list is
/// where a title that wraps stops being scannable.
const TITLE_MAX: usize = 72;

/// Below this index a word break throws away more than a tidy ending is worth.
const BREAK_FLOOR: usize = 40;

/// The shortest environment value that is scrubbed. Shorter ones (`1`, `true`, `zsh`) are
/// ordinary words far more often than they are anybody's secret.
const ENV_VALUE_MIN: usize = 8;

/// Variables whose values describe a terminal and never a person, so a report about a
/// terminal can still say which one.
const ENV_HARMLESS: [&str; 10] = [
    "TERM",
    "COLORTERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "SHLVL",
    "_",
    "OLDPWD",
];

/// What a report is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// charter did something wrong.
    Bug,
    /// charter cannot do something it should.
    Feature,
}

impl Kind {
    /// The word the preview uses.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Bug => "bug",
            Kind::Feature => "feature request",
        }
    }
}

/// Everything charter can positively identify as the reporter's, and so removes from a draft.
#[derive(Debug, Default, Clone)]
pub struct Known {
    /// The plane's own root, removed whole wherever it appears.
    pub plane: Option<String>,
    /// The reporter's home directory, for a home that is not under `/Users` or `/home`.
    pub home: Option<String>,
    /// `(name, value)` for every variable in the environment.
    pub env: Vec<(String, String)>,
    /// `(name, placeholder)`: workspaces, personas, clones, vaults.
    pub names: Vec<(String, &'static str)>,
}

impl Known {
    /// What this process can identify: its environment, its home, and — inside a plane — the
    /// plane's root and the names in it.
    pub fn here(plane: Option<&Path>) -> Known {
        Known {
            plane: plane.map(|p| p.display().to_string()),
            home: dirs::home_dir().map(|h| h.display().to_string()),
            env: std::env::vars().collect(),
            names: plane.map(names_in).unwrap_or_default(),
        }
    }

    /// `text` with what this knows removed, and the categories that were.
    pub fn scrub(&self, text: &str) -> (String, Vec<String>) {
        let mut used: Vec<String> = Vec::new();
        let mut note = |what: &str| {
            if !used.iter().any(|u| u == what) {
                used.push(what.to_string());
            }
        };

        // A credential's shape, a line at a time: the detector answers WHETHER, never where,
        // so the line goes whole. Visible, so an over-eager removal can be restored by hand.
        let mut out = String::with_capacity(text.len());
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                out.push('\n');
            }
            match crate::secretshape::secret_kind(line)
                .or_else(|| crate::secretshape::token_kind(line))
            {
                Some(kind) => {
                    note("lines that look like a credential");
                    let _ = write!(out, "[redacted: a line that looks like {kind}]");
                }
                None => out.push_str(line),
            }
        }

        // Environment values, longest first so a value that contains another goes whole.
        // Before the paths: `$HOME` is a path, and `[env $HOME]` says more than `[home path]`.
        let mut env: Vec<&(String, String)> = self
            .env
            .iter()
            .filter(|(name, value)| {
                value.chars().count() >= ENV_VALUE_MIN
                    && !ENV_HARMLESS.contains(&name.as_str())
                    && !name.starts_with("LC_")
            })
            .collect();
        env.sort_by_key(|(_, value)| std::cmp::Reverse(value.len()));
        for (name, value) in env {
            if out.contains(value.as_str()) {
                note("environment values");
                out = out.replace(value.as_str(), &format!("[env ${name}]"));
            }
        }

        if let Some(plane) = self.plane.as_deref().filter(|p| p.len() > 1)
            && out.contains(plane)
        {
            note("the plane's path");
            out = out.replace(plane, "[plane]");
        }
        if home_paths().is_match(&out) {
            note("home paths");
            out = home_paths().replace_all(&out, "[home path]").into_owned();
        }
        if let Some(home) = self.home.as_deref().filter(|h| h.len() > 1)
            && out.contains(home)
        {
            note("home paths");
            out = out.replace(home, "[home]");
        }

        // Names, longest first, so `acme-migration` goes whole before `acme` can half-eat it.
        let mut names: Vec<&(String, &'static str)> = self.names.iter().collect();
        names.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
        for (name, placeholder) in names {
            let (replaced, hit) = replace_word(&out, name, placeholder);
            if hit {
                note(&format!("{} names", placeholder.trim_matches(['[', ']'])));
                out = replaced;
            }
        }
        (out, used)
    }
}

/// Absolute paths under a home directory, which carry a user name and then project names.
/// Stops at a quote, a bracket or a backtick so the Markdown around a path survives.
fn home_paths() -> &'static Regex {
    static ONCE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        Regex::new(r#"(?:/Users/|/home/|[A-Za-z]:\\Users\\)[^\s`'"()<>\[\]]*"#)
            .expect("a pattern this module wrote")
    })
}

/// Whether `c` is part of a word, as Python's `\b` reads it.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `text` with every whole-word `name` replaced by `with`, and whether any was.
fn replace_word(text: &str, name: &str, with: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut hit = false;
    let mut rest = text;
    let mut before: Option<char> = None;
    while let Some(at) = rest.find(name) {
        let head = &rest[..at];
        let prev = head.chars().next_back().or(before);
        let next = rest[at + name.len()..].chars().next();
        out.push_str(head);
        let starts = name.chars().next().is_some_and(is_word);
        let ends = name.chars().next_back().is_some_and(is_word);
        if (!starts || !prev.is_some_and(is_word)) && (!ends || !next.is_some_and(is_word)) {
            out.push_str(with);
            hit = true;
        } else {
            out.push_str(name);
        }
        before = name.chars().next_back();
        rest = &rest[at + name.len()..];
    }
    out.push_str(rest);
    (out, hit)
}

/// The names in a plane that are the reporter's: its workspaces, the clones in them, its
/// personas and its vaults. Names under three characters, and `default`, which every plane
/// has, identify nobody and would mangle ordinary prose.
fn names_in(plane: &Path) -> Vec<(String, &'static str)> {
    let mut out: Vec<(String, &'static str)> = Vec::new();
    for ws in subdirs(&plane.join("workspaces")) {
        for clone in subdirs(&plane.join("workspaces").join(&ws)) {
            if plane
                .join("workspaces")
                .join(&ws)
                .join(&clone)
                .join(".git")
                .exists()
            {
                out.push((clone, "[repo]"));
            }
        }
        out.push((ws, "[workspace]"));
    }
    for persona in subdirs(&plane.join("personas")) {
        out.push((persona, "[persona]"));
    }
    let ctx = crate::secrets::Ctx::new(plane, crate::secrets::Env::from_process());
    if let Ok(registry) = crate::secrets::registry::load_registry(&ctx) {
        for name in crate::secrets::registry::vaults(&registry).keys() {
            out.push((name.clone(), "[vault]"));
        }
    }
    out.retain(|(name, _)| {
        name.chars().count() >= 3 && name != "default" && !name.starts_with(['_', '.'])
    });
    out
}

fn subdirs(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

/// A report, ready to be shown and — on the reporter's yes — filed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub kind: Kind,
    pub title: String,
    pub body: String,
    /// The categories the scrub removed, for the preview to name.
    pub scrubbed: Vec<String>,
}

/// Why a draft could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused(pub String);

impl Draft {
    /// A report the reporter described in prose. `title` overrides the first line.
    pub fn described(
        kind: Kind,
        text: &str,
        title: Option<&str>,
        known: &Known,
    ) -> Result<Draft, Refused> {
        let (text, scrubbed) = known.scrub(text.trim());
        if text.trim().is_empty() {
            return Err(Refused(format!(
                "a {} needs a description: pass it as an argument, with --from-file or on --stdin",
                kind.word()
            )));
        }
        let (title, more) = match title {
            Some(t) => known.scrub(t),
            None => (title_of(&text), Vec::new()),
        };
        Draft::assemble(kind, title, text, merge(scrubbed, more))
    }

    /// A draft bug from a saved panic, with the reporter's own words in front of it if any.
    pub fn of_panic(record: &Panic, words: Option<&str>, known: &Known) -> Result<Draft, Refused> {
        let place = short_place(&record.place);
        let (place, mut scrubbed) = known.scrub(&place);
        let (message, more) = known.scrub(&record.message);
        scrubbed = merge(scrubbed, more);
        let mut text = String::new();
        if let Some(words) = words.map(str::trim).filter(|w| !w.is_empty()) {
            let (words, more) = known.scrub(words);
            scrubbed = merge(scrubbed, more);
            text.push_str(&words);
            text.push_str("\n\n");
        }
        let version = record
            .version
            .as_deref()
            .unwrap_or("not recorded (a panic saved before records named one)");
        let _ = write!(
            text,
            "charter panicked.\n\n\
             - **where:** `{place}`\n\
             - **charter version:** {version}\n\
             - **message** (free text, read it before sending):\n\n"
        );
        for line in message.lines() {
            let _ = writeln!(text, "  > {line}");
        }
        let title = cut_title(&format!("charter panicked at {place}"));
        Draft::assemble(Kind::Bug, title, text.trim_end().to_string(), scrubbed)
    }

    fn assemble(
        kind: Kind,
        title: String,
        text: String,
        scrubbed: Vec<String>,
    ) -> Result<Draft, Refused> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(Refused("a report needs a title".to_string()));
        }
        let mut body = text;
        let _ = write!(
            body,
            "\n\n---\ncharter {} · {} {}\n_Filed with `charter report {}`._",
            crate::adopt::app_version(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            match kind {
                Kind::Bug => "bug",
                Kind::Feature => "feature",
            }
        );
        if body.chars().count() > BODY_MAX {
            return Err(Refused(format!(
                "the report is over {BODY_MAX} characters; GitHub would refuse it. Shorten it."
            )));
        }
        Ok(Draft {
            kind,
            title,
            body,
            scrubbed,
        })
    }

    /// Twelve hex characters over the repository, the title and the body: what `--yes` must
    /// name for exactly this draft to be filed.
    pub fn digest(&self) -> String {
        let mut hash = Sha256::new();
        for part in [UPSTREAM, &self.title, &self.body] {
            hash.update(part.as_bytes());
            hash.update([0u8]);
        }
        hash.finalize()
            .iter()
            .take(6)
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// The draft as the reporter reads it: exactly what [`file`] would send, and where.
    pub fn preview(&self) -> String {
        let mut out = format!(
            "Draft {} for {UPSTREAM} — nothing has been sent.\n\n\
             Title: {}\n\
             ----- body -----\n{}\n----- end -----\n",
            self.kind.word(),
            self.title,
            self.body
        );
        if self.scrubbed.is_empty() {
            out.push_str("Scrubbed: nothing charter could identify. Read it anyway.\n");
        } else {
            let _ = writeln!(
                out,
                "Scrubbed before drafting: {}. Read it anyway: charter cannot see everything.",
                self.scrubbed.join(", ")
            );
        }
        out
    }

    /// A prefilled `issues/new` link, for a reporter whose `gh` cannot file.
    pub fn fallback_url(&self) -> String {
        format!(
            "https://github.com/{UPSTREAM}/issues/new?title={}&body={}",
            forge::quote(&self.title),
            forge::quote(&self.body)
        )
    }

    /// The words duplicate search looks for: the title's, without punctuation a search query
    /// would read as syntax.
    fn query(&self) -> String {
        self.title
            .split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.')))
            .filter(|w| w.chars().count() > 2)
            .take(8)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn merge(mut a: Vec<String>, b: Vec<String>) -> Vec<String> {
    for x in b {
        if !a.contains(&x) {
            a.push(x);
        }
    }
    a
}

/// A place in the source as a report names it: a path relative to a repository stays, and an
/// absolute one keeps only its last three parts — enough to name a crate's file, and none of
/// the directories above it.
fn short_place(place: &str) -> String {
    if !(place.starts_with('/') || place.get(1..3) == Some(":\\")) {
        return place.to_string();
    }
    let parts: Vec<&str> = place.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    format!("…/{}", parts[parts.len().saturating_sub(3)..].join("/"))
}

/// The title of a described report: its first line, un-marked and bounded at a word.
fn title_of(text: &str) -> String {
    let first = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let first = first
        .strip_prefix('#')
        .map(|rest| rest.trim_start_matches('#'))
        .filter(|rest| rest.starts_with([' ', '\t']))
        .map(|rest| rest.trim().trim_end_matches('#').trim())
        .unwrap_or(first);
    cut_title(first)
}

fn cut_title(line: &str) -> String {
    if line.chars().count() <= TITLE_MAX {
        return line.to_string();
    }
    let cut: String = line.chars().take(TITLE_MAX - 1).collect();
    let cut = match cut.rfind(' ') {
        Some(space) if cut[..space].chars().count() >= BREAK_FLOOR => cut[..space].to_string(),
        _ => cut,
    };
    format!("{}…", cut.trim_end())
}

/// One upstream issue that may already cover a draft.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Hit {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub url: String,
}

/// Candidate duplicates on [`UPSTREAM`], for the reporter to judge: a keyword search cannot
/// tell two failures of one command apart. `gh search issues`, because `gh api search/issues`
/// answers 404 on this repository.
pub fn search_duplicates(draft: &Draft) -> Result<Vec<Hit>, ForgeError> {
    let query = draft.query();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let args: Vec<String> = [
        "search",
        "issues",
        "--repo",
        UPSTREAM,
        "--json",
        "number,title,state,url",
        "--limit",
        "5",
        "--",
        &query,
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    let out = forge::gh_as_the_operator(&args, forge::STATUS_TIMEOUT)?;
    serde_json::from_str(out.trim()).map_err(|e| {
        ForgeError(format!(
            "gh answered something that is not a list of issues: {e}"
        ))
    })
}

/// Opens the issue on [`UPSTREAM`] under the reporter's own `gh` login and returns its URL.
pub fn file(draft: &Draft) -> Result<String, ForgeError> {
    let args = vec![
        "issue".to_string(),
        "create".to_string(),
        "--repo".to_string(),
        UPSTREAM.to_string(),
        format!("--title={}", draft.title),
        format!("--body={}", draft.body),
    ];
    let out = forge::gh_as_the_operator(&args, forge::LIST_TIMEOUT)?;
    let url = out
        .lines()
        .map(str::trim)
        .rfind(|l| l.starts_with("https://"));
    url.map(str::to_string).ok_or_else(|| {
        ForgeError(format!(
            "gh did not answer with the new issue's address: {}",
            out.trim()
        ))
    })
}

/// A panic the app wrote down, as `panics.rs` writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Panic {
    pub place: String,
    pub message: String,
    pub version: Option<String>,
}

/// The file the app appends its panics to: `$CHARTER_PANIC_LOG` when set, as the app itself
/// reads it, else `panics.log` in the app's log directory (Tauri's `app_log_dir`).
pub fn panic_log() -> Option<PathBuf> {
    if let Some(file) = std::env::var_os("CHARTER_PANIC_LOG").filter(|f| !f.is_empty()) {
        return Some(PathBuf::from(file));
    }
    const APP: &str = "dev.charter.app";
    let dir = if cfg!(target_os = "macos") {
        dirs::home_dir()?.join("Library/Logs").join(APP)
    } else {
        dirs::data_local_dir()?.join(APP).join("logs")
    };
    Some(dir.join("panics.log"))
}

/// The newest panic in `log`, or `None` when it holds none.
pub fn latest_panic(log: &str) -> Option<Panic> {
    let start = log.rfind("charter-panic pid ")?;
    let block = &log[start..];
    let block = block.split("\nbacktrace:").next().unwrap_or(block);
    let mut place = None;
    let mut version = None;
    let mut message: Option<String> = None;
    for line in block.lines().skip(1) {
        if let Some(m) = message.as_mut() {
            m.push('\n');
            m.push_str(line);
        } else if let Some(v) = line.strip_prefix("place: ") {
            place = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("version: ") {
            version = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("message: ") {
            message = Some(v.to_string());
        }
    }
    Some(Panic {
        place: place?,
        message: message.unwrap_or_default(),
        version,
    })
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
