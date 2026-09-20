//! `charter doctor`: the preflight an operator runs, and every harness runs at session start.
//!
//! A port of `charter/doctor.py` and `commands.cmd_doctor`. Each check is one [`Row`] — a
//! name, a status, a sentence and a repair — and `--json` prints them in exactly the shape
//! Python's `json.dumps(indent=2)` does, because other tools read that output and it is a
//! contract (`tests/differential/run.py` compares it byte for byte).
//!
//! # An absent answer is not health
//!
//! The one rule every row here keeps, and the reason this module exists in the shape it has.
//! Python states it as `tests/test_doctor_absent_is_not_health.py`: a check that could not
//! run says so, WARN and never OK, because a green glyph over "charter did not look" is read
//! as "charter looked and it is fine" by anyone scanning the column.
//!
//! **That rule is also what decided how the unported checks appear.** About half of Python's
//! rows are about parts this binary does not own yet — the guard and the vaults (M3), the
//! forges, the tmux frame, the Claude Code plugin. Dropping those rows would be the loudest
//! possible violation: a doctor that stops reporting a problem reads as the problem being
//! fixed. So every one of them is still here, under its own name and in its own place, as a
//! WARN that says it was not checked and why ([`deferred`]). The differential test holds the
//! list of them, so a row that becomes ported has to say so there.

mod clones;
mod config;
mod deferred;
mod fsx;
mod git;
mod inventory;
mod memory;
mod plane;
mod profiles;
mod session;

use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long one git question a check asks may take — Python's `doctor.CHECK_TIMEOUT`.
///
/// **Five seconds, where the panels allow thirty** ([`crate::worktree::git::READ`]). A panel's
/// read is off the UI thread and nothing waits on it; this runs from a SessionStart hook
/// whose whole budget is shared by every check, and Python measured a stalled network mount
/// eating it and printing nothing. A git that does not answer in time costs its own row a
/// "not checked", which is an honest answer, and not the session its preflight.
pub(crate) const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Why a check that could not RUN is a warning rather than a tick (Python's
/// `_NOT_CHECKED_HINT`, #171). WARN and never FAIL: an unreadable tree or a git that timed out
/// is not "you cannot work" — it is "charter cannot tell you either way".
pub const NOT_CHECKED_HINT: &str = "This check could not run, so its silence means nothing. \
                                    Re-run `charter doctor` — if it persists, the reason \
                                    above is the thing to fix.";

/// A row's verdict. FAIL is the only one that makes `charter doctor` exit non-zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

impl Status {
    /// The word `--json` carries.
    pub fn word(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }

    /// The SGR colour and the glyph the table draws.
    fn glyph(self) -> (&'static str, &'static str) {
        match self {
            Self::Ok => ("32", "\u{2713}"),
            Self::Warn => ("33", "!"),
            Self::Fail => ("31", "\u{2717}"),
        }
    }
}

/// One preflight row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub status: Status,
    pub detail: String,
    /// The remedy. A green row's hint is carried in `--json` and never drawn in the table.
    pub hint: String,
}

impl Row {
    pub(crate) fn ok(name: &str, detail: impl Into<String>) -> Self {
        Self::new(name, Status::Ok, detail, "")
    }

    pub(crate) fn warn(name: &str, detail: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::new(name, Status::Warn, detail, hint)
    }

    pub(crate) fn fail(name: &str, detail: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::new(name, Status::Fail, detail, hint)
    }

    /// Python's `Result(name, WARN, detail=f"not checked ({why})", hint=_NOT_CHECKED_HINT)`.
    pub(crate) fn not_checked(name: &str, why: impl std::fmt::Display) -> Self {
        Self::warn(name, format!("not checked ({why})"), NOT_CHECKED_HINT)
    }

    fn new(name: &str, status: Status, detail: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            name: name.to_owned(),
            status,
            detail: detail.into(),
            hint: hint.into(),
        }
    }
}

/// What `charter.toml` said when it was read — Python's `CONFIG_ERROR` / `PLANE_REFUSAL`.
#[derive(Debug, Clone)]
pub(crate) enum Config {
    /// Parsed, or absent or unreadable — which Python's `instance.load` answers as `{}`.
    Read(toml::Table),
    /// Not TOML charter can read. charter carries on with empty defaults.
    Malformed(String),
    /// A plane format version this charter cannot place. Every other command stops.
    Refused(String),
}

impl Config {
    /// `instance.load`, with the failure kept as the sentence Python records.
    pub(crate) fn load(root: &Path) -> Self {
        let path = root.join(crate::plane::MANIFEST);
        let Ok(raw) = std::fs::read(&path) else {
            return Self::Read(toml::Table::new());
        };
        let text = match String::from_utf8(raw) {
            Ok(text) => text,
            Err(e) => {
                return Self::Malformed(format!("{} is not valid TOML: {e}", path.display()));
            }
        };
        let table = match text.parse::<toml::Table>() {
            Ok(table) => table,
            Err(e) => {
                return Self::Malformed(format!(
                    "{} is not valid TOML: {}",
                    path.display(),
                    toml_error(&e)
                ));
            }
        };
        match table.get("schema") {
            None | Some(toml::Value::Integer(1)) => Self::Read(table),
            Some(toml::Value::Integer(found)) if *found < 1 => Self::Read(table),
            Some(toml::Value::Integer(found)) => Self::Refused(format!(
                "{} declares schema {found}, but this charter understands {SCHEMA}. Upgrade \
                 charter: `uv tool install charter-cp --force --refresh`.",
                path.display()
            )),
            Some(other) => Self::Refused(format!(
                "{} declares schema {}, which is not a plane format version this charter can \
                 compare against {SCHEMA}. charter will not operate on a plane whose format it \
                 cannot place. Fix the `schema` line, or upgrade charter: `uv tool install \
                 charter-cp --force --refresh`.",
                path.display(),
                config::toml_repr(other)
            )),
        }
    }

    /// The document, or Python's `{}` when it could not be had.
    pub(crate) fn table(&self) -> Option<&toml::Table> {
        match self {
            Self::Read(table) => Some(table),
            Self::Malformed(_) | Self::Refused(_) => None,
        }
    }
}

/// A TOML parser's diagnostic on one line, as `tomllib` gives it.
///
/// `toml` draws a snippet of the file with a caret under the fault, over several lines, and
/// the sentence this lands in is a row: its first line is the detail and the rest would be
/// drawn as the table's own lines. The position and the reason are kept; the drawing is not.
fn toml_error(e: &toml::de::Error) -> String {
    let text = e.to_string();
    let mut lines = text.lines();
    let head = lines.next().unwrap_or("").trim().to_owned();
    let reason: Vec<&str> = lines
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains('|'))
        .collect();
    if reason.is_empty() {
        head
    } else {
        format!("{head}: {}", reason.join("; "))
    }
}

/// How much of one value a sentence repeats back — `contain.DISPLAY_LIMIT`.
pub(crate) const DISPLAY_LIMIT: usize = 160;

/// The same for a value that is a PATH, which a reader has to be able to go to —
/// `contain.PATH_DISPLAY_LIMIT`.
pub(crate) const PATH_DISPLAY_LIMIT: usize = 1024;

/// `contain.one_line`: `value` with nothing in it that can forge another line of a report.
///
/// Every character with no glyph — Unicode categories Cc, Cf, Cs, Zl and Zp, and whitespace
/// other than the space — becomes its own escape, and the result is clipped with `…`. Unlike
/// [`crate::shown::readable`] it keeps every other glyph as itself: this is for a sentence
/// that quotes a value, not for a name a reader has to type back.
///
/// Cc is `char::is_control`; Cs cannot occur in a Rust string; Zl and Zp are one character
/// each. Cf has no test in `std`, so [`is_format`] carries the category's ranges.
pub(crate) fn one_line(value: &str, limit: usize) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        let invisible = ch.is_control()
            || is_format(ch)
            || matches!(ch, '\u{2028}' | '\u{2029}')
            || (crate::memstore::is_python_space(ch) && ch != ' ');
        if !invisible {
            out.push(ch);
        } else if (ch as u32) < 0x100 {
            out.push_str(&format!("\\x{:02x}", ch as u32));
        } else {
            out.push_str(&format!("\\u{:04x}", ch as u32));
        }
    }
    if out.chars().count() <= limit {
        return out;
    }
    let mut clipped: String = out.chars().take(limit).collect();
    clipped.push('\u{2026}');
    clipped
}

/// Unicode's Cf (format) category — invisible characters that change how their neighbours
/// render: soft hyphens, bidi controls, zero-width joiners, tags.
fn is_format(ch: char) -> bool {
    matches!(ch as u32,
        0x00AD | 0x0600..=0x0605 | 0x061C | 0x06DD | 0x070F | 0x0890..=0x0891 | 0x08E2
        | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0x2066..=0x206F
        | 0xFEFF | 0xFFF9..=0xFFFB | 0x110BD | 0x110CD | 0x13430..=0x1343F
        | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0xE0001 | 0xE0020..=0xE007F)
}

/// The plane format version this charter understands — Python's `instance.SCHEMA`.
pub(crate) const SCHEMA: i64 = 1;

/// Everything a check is asked about: which plane, standing where, and how it was invoked.
pub struct Doctor {
    /// The plane this binary acts on, canonical — what Python calls `config.ROOT`.
    pub(crate) root: PathBuf,
    /// Whether `root` holds a `charter.toml` — Python's `HAS_CONTROL_PLANE`.
    pub(crate) has_plane: bool,
    /// Whether `$CHARTER_ROOT` chose the plane rather than the working directory.
    pub(crate) pinned: bool,
    /// The directory the command runs in.
    pub(crate) cwd: PathBuf,
    /// `--preflight`: what the SessionStart hook runs. No profile probe, no git call for one.
    pub(crate) preflight: bool,
    pub(crate) config: Config,
}

impl Doctor {
    /// The doctor for a command run in `cwd`, on the plane this binary resolves from there.
    ///
    /// **The plane THIS binary acts on**, through [`crate::plane::resolve`], and not a second
    /// resolution written to match Python's. A doctor reporting on one plane while every
    /// other command acts on another would be the most confident wrong answer it could give.
    /// Where the two charters resolve differently — Python hops outward through an enclosing
    /// plane's `workspaces/`, this binary does not — the `nested plane` row says so.
    pub fn new(cwd: &Path, preflight: bool) -> Self {
        let pinned = std::env::var_os("CHARTER_ROOT").is_some_and(|v| !v.is_empty());
        let root = crate::plane::resolve(cwd)
            .map(|p| canonical(&p))
            .unwrap_or_else(|_| canonical(cwd));
        Self::at(&root, cwd, pinned, preflight)
    }

    /// The doctor for an explicit plane, which is how a test names one.
    pub fn at(root: &Path, cwd: &Path, pinned: bool, preflight: bool) -> Self {
        Self {
            root: root.to_path_buf(),
            has_plane: root.join(crate::plane::MANIFEST).is_file(),
            pinned,
            cwd: cwd.to_path_buf(),
            preflight,
            config: Config::load(root),
        }
    }

    /// Every row, in the order Python's `doctor._checks` runs them: cheap and local first.
    pub fn run(&self) -> Vec<Row> {
        let mut rows = vec![deferred::python3(), git::git(), git::identity(self)];
        for cli in config::forge_clis(self) {
            rows.push(deferred::row(&cli, deferred::FORGES));
            rows.push(deferred::row(&format!("{cli} auth"), deferred::FORGES));
        }
        rows.push(deferred::row("git auth", deferred::GIT_POLICY));
        rows.push(config::charter_toml(self));
        rows.push(profiles::harness_profiles(self));
        rows.extend(profiles::profile_rows(self));
        rows.push(config::schema(self));
        rows.push(git::plane_root(self));
        rows.push(git::index_lock(self));
        rows.push(session::session_root(self));
        rows.push(session::session_layer(self));
        rows.push(deferred::row("harness", deferred::HARNESS));
        rows.push(deferred::row("frame", deferred::FRAME));
        rows.push(deferred::row("ended tab", deferred::FRAME));
        rows.push(deferred::row("plane-root guard", deferred::GUARD));
        rows.push(deferred::row("guard seen", deferred::GUARD));
        rows.push(plane::nested(self));
        rows.push(clones::workspace_clones(self));
        rows.push(deferred::row("workspace layer", deferred::WORKSPACE_LAYER));
        rows.push(deferred::row("changes", deferred::CHANGES));
        rows.push(inventory::inventory(self));
        rows.push(deferred::row("vaults", deferred::VAULTS));
        rows.push(deferred::row("vault registry", deferred::VAULTS));
        rows.push(config::version_lock(self));
        rows.push(memory::memory_indexes(self));
        rows.push(deferred::row("personas", deferred::PERSONA_LINT));
        rows.push(deferred::row("persona grant", deferred::PERSONA_LINT));
        rows.push(plane::front_door(self));
        rows.push(deferred::row("news", deferred::NEWS));
        rows.push(deferred::row("ask rules", deferred::ASK_RULES));
        rows.push(deferred::row("handoff gate", deferred::HANDOFF_GATE));
        rows.push(deferred::row("shadowed docs", deferred::SHADOWED_DOCS));
        rows.push(deferred::row("credential paths", deferred::VAULTS));
        rows.push(deferred::row("mcp", deferred::VAULTS));
        rows.push(deferred::row("plugin install", deferred::PLUGIN));
        rows.push(deferred::row("plugin", deferred::PLUGIN));
        rows.push(deferred::row("plugin files", deferred::PLUGIN));
        rows
    }
}

/// `path` with its links resolved, or `path` itself when it cannot be — Python's
/// `Path.resolve()`, which never refuses a path that does not exist.
pub(crate) fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// `util.short_path`: `~/…` for a path under the home directory, the path otherwise — and
/// never `~` for a path that has a segment starting with one, which a shell would read as a
/// home it does not name.
pub(crate) fn short_path(path: &Path) -> String {
    let tilde = path
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('~'));
    if tilde {
        return std::path::absolute(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .display()
            .to_string();
    }
    if let Some(home) = crate::profiles::home()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        let rest = rest.display().to_string();
        return format!("~/{}", if rest.is_empty() { "." } else { &rest });
    }
    path.display().to_string()
}

/// Python's `_first_line`: the text stripped, then its first line.
pub(crate) fn first_line(text: &str) -> String {
    crate::memstore::py_strip(text)
        .lines()
        .next()
        .unwrap_or("")
        .to_owned()
}

/// `--json`: `json.dumps(rows, indent=2)` and the newline `print` adds.
pub fn json(rows: &[Row]) -> String {
    let doc = serde_json::Value::Array(
        rows.iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "status": r.status.word(),
                    "detail": r.detail,
                    "hint": r.hint,
                })
            })
            .collect(),
    );
    crate::pyjson::dumps_indent2(&doc)
}

/// The table `charter doctor` prints, header and verdict included.
///
/// The NAME column is measured from the names about to be printed — Python's `name_width`,
/// which asks the checks rather than guessing a `:<16` (#600) — and is a floor, never a cap:
/// a name wider than it pushes its own row rather than being cut.
pub fn table(rows: &[Row], color: bool) -> String {
    let name_w = rows.iter().map(|r| width(&r.name)).max().unwrap_or(0) + 2;
    let mut out = String::from("charter preflight:\n\n");
    for r in rows {
        out.push_str(&render(r, name_w, color));
        out.push('\n');
    }
    out.push('\n');
    let failed: Vec<&str> = rows
        .iter()
        .filter(|r| r.status == Status::Fail)
        .map(|r| r.name.as_str())
        .collect();
    let warned = rows.iter().filter(|r| r.status == Status::Warn).count();
    if !failed.is_empty() {
        out.push_str(&format!(
            "\u{2717} {} blocker(s): {}. Fix the \u{2192} hints above, then re-run `charter \
             doctor`.\n",
            failed.len(),
            failed.join(", ")
        ));
    } else if warned > 0 {
        out.push_str(&format!(
            "! {warned} optional item(s) pending \u{2014} see hints above.\n"
        ));
    } else {
        out.push_str("\u{2713} All set \u{2014} you can discover and clone repos.\n");
    }
    out
}

/// The exit status: non-zero only when something is a blocker. A WARN — every row that could
/// not be checked among them — is not "you cannot work".
pub fn exit_code(rows: &[Row]) -> u8 {
    u8::from(rows.iter().any(|r| r.status == Status::Fail))
}

/// One row, as `Result.render` draws it. A hint is a remedy, so a green row prints none.
fn render(r: &Row, name_w: usize, color: bool) -> String {
    let (code, glyph) = r.status.glyph();
    let glyph = if color {
        format!("\x1b[{code}m{glyph}\x1b[0m")
    } else {
        glyph.to_owned()
    };
    let w = name_w.max(width(&r.name) + 2);
    let pad = w.saturating_sub(width(&r.name));
    let line = format!("  {glyph}  {}{}{}", r.name, " ".repeat(pad), r.detail);
    let mut line = line
        .trim_end_matches(crate::memstore::is_python_space)
        .to_owned();
    if !r.hint.is_empty() && r.status != Status::Ok {
        line.push_str("\n        \u{2192} ");
        line.push_str(&r.hint);
    }
    line
}

/// Columns a name takes. Every row name is printable ASCII — constants, or a profile name
/// already through [`crate::shown::readable`] — so a character is a column.
fn width(name: &str) -> usize {
    name.chars().count()
}

#[cfg(test)]
mod tests;
