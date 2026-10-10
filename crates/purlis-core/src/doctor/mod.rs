//! `charter doctor`: the preflight an operator runs, and every harness runs at session start.
//!
//! A port of `charter/doctor.py` and `commands.cmd_doctor`. Each check is one [`Row`] — a
//! name, a status, a sentence and a repair — and `--json` prints them in exactly the shape
//! Python's `json.dumps(indent=2)` does, because other tools read that output and it is a
//! contract (the recorded `doctor-*` scenarios hold it byte for byte, ADR 0046).
//!
//! # An absent answer is not health
//!
//! The one rule every row here keeps, and the reason this module exists in the shape it has.
//! Python states it as `tests/test_doctor_absent_is_not_health.py`: a check that could not
//! run says so, WARN and never OK, because a green glyph over "charter did not look" is read
//! as "charter looked and it is fine" by anyone scanning the column.
//!
//! **That rule is also what decided how the checks this build does not run appear.** About half of
//! Python's rows were about parts this binary did not own when it was rebuilt — the guard and the
//! vaults (M3), the forges, the tmux frame, the Claude Code plugin. The ones still not checked are
//! planned in OB-8 (#994) and, for the forges, FG-2 (#802). Dropping those rows would be the
//! loudest possible violation: a doctor that stops reporting a problem reads as the problem being
//! fixed. So every one of them is still here, under its own name and in its own place, as a WARN
//! that says it was not checked and why ([`deferred`]). The differential test holds the list of
//! them, so a row that becomes ported has to say so there.

mod budget;
mod changes;
mod clones;
mod config;
mod deferred;
pub mod fix;
// `pub(crate)`: `gitpolicy` asks the same two questions about the same
// directories — what clears a path charter could not check, and whether a path is
// simply not there — and `charter/workspace.py` answers them once for both.
pub(crate) mod fsx;
mod git;
mod harnesses;
mod inventory;
mod memory;
mod news;
mod personas;
mod plane;
mod plugin;
mod profiles;
mod remote;
mod rules;
mod sandbox;
pub(crate) mod session;
mod vault_registry;
mod vaults;
mod work;

/// Python's truthiness of a TOML value, for `crate::alerts`, which reads the same manifest
/// sections through the same `(cfg.get(name) or {})` idiom.
pub(crate) use config::findings as config_findings;
pub(crate) use config::truthy as config_truthy;
/// Who can read what a push to a project's remote publishes, for the going-LIVE confirmation
/// (#1369): the `project remote` row's own forge read.
pub use remote::{Readers, readers};

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
                                    Re-run `purlis doctor` — if it persists, the reason \
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
    /// **The Settings group the remedy is made in** (SE-22, V89c), when the fix is a setting
    /// the window can change. The app's doctor links the row to that group; the table and
    /// `--json` leave it out, so what `charter doctor` prints is unchanged.
    pub settings: Option<SettingsGroup>,
    /// **The fix charter can make for this finding itself** (FX-1, V91p), when there is one:
    /// what `charter doctor --fix <id>` and the Doctor dialog's Fix button apply through
    /// [`fix::apply`]. `--json` carries it on the rows that have one; the table does not.
    pub fix: Option<fix::FixId>,
}

/// **A Settings group a doctor row can link to** (SE-22): one of the Project level's groups,
/// which the window declares in `app/src/settings/project.ts`. A closed set, so a row cannot
/// name an address the window does not have — `settings/linkedGroups.test.ts` holds
/// [`SettingsGroup::id`] to that file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsGroup {
    General,
    Saving,
    Harness,
    Forges,
}

impl SettingsGroup {
    /// The group's stable address, as the window names it.
    pub const fn id(self) -> &'static str {
        match self {
            Self::General => "project.general",
            Self::Saving => "project.saving",
            Self::Harness => "project.harness",
            Self::Forges => "project.forges",
        }
    }
}

impl Row {
    /// Whether this row is one this binary does not run at all — a check this build does not
    /// have yet ([`deferred`], planned in #994 and #802), printed as a WARN so its silence is
    /// never read as a pass.
    ///
    /// **A different claim from a check that ran and could not finish** ([`Row::not_checked`]).
    /// That one is a real warning about this machine: charter tried, and a git timed out or a
    /// tree was unreadable. A deferred row is a fact about this BUILD, identical on every
    /// machine and every plane — about twenty of them, every run. A surface that summarises
    /// the table (the app's status line) has to tell the two apart, or it draws "20 warnings"
    /// forever and the one real warning among them is furniture on its first day.
    ///
    /// Asked of the hint, because that is the one thing [`deferred::row`] writes and nothing
    /// else does: a row cannot be deferred without it, and no ported check says it.
    pub fn deferred(&self) -> bool {
        self.hint == deferred::DEFERRED_HINT
    }

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
            settings: None,
            fix: None,
        }
    }

    /// This row, naming the Settings group its fix is made in ([`Row::settings`]).
    pub(crate) fn in_settings(self, group: SettingsGroup) -> Self {
        Self {
            settings: Some(group),
            ..self
        }
    }

    /// This row, offering the fix `id` ([`Row::fix`]).
    pub(crate) fn fixed_by(self, id: fix::FixId) -> Self {
        Self {
            fix: Some(id),
            ..self
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
        let path = crate::names::manifest(root);
        let Ok(raw) = std::fs::read(&path) else {
            return Self::Read(toml::Table::new());
        };
        Self::parse(&path, raw)
    }

    /// [`Config::load`] of bytes that are, or are about to be, the file at `path` — so a
    /// writer can ask what the next read would say before it writes (charter-app#252).
    pub(crate) fn parse(path: &Path, raw: Vec<u8>) -> Self {
        let text = match String::from_utf8(raw) {
            Ok(text) => text,
            Err(e) => {
                return Self::Malformed(format!(
                    "{} is not valid TOML: {e}",
                    fsx::path_field(path)
                ));
            }
        };
        let table = match text.parse::<toml::Table>() {
            Ok(table) => table,
            Err(e) => {
                return Self::Malformed(format!(
                    "{} is not valid TOML: {}",
                    fsx::path_field(path),
                    toml_error(&e)
                ));
            }
        };
        // Python's `found > SCHEMA` is the refusal, so everything up to and including this
        // charter's own version is read. (Spelled once, as `<= SCHEMA`: an arm for `1` ahead of
        // one for `< 1` left the `<` with a boundary no input could reach.)
        match crate::compat::schema(&table).refusal(&fsx::path_field(path)) {
            None => Self::Read(table),
            Some(refusal) => Self::Refused(refusal),
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
/// **One implementation, in [`crate::shown`], rather than a copy here.** `doctor`, `personas`
/// and `news` all need the same answer, and three copies of "which characters have no glyph"
/// is three places for the table to go stale separately — with the failure showing up as
/// one charter escaping a character another prints, on a report line, which is exactly what
/// this function is for. Kept as a name here because every call site in this module reads
/// better for it.
///
/// **And the table under it is generated, not pasted.** This module carried its own `Cf`
/// list once; `shown` carried a second one; `pyrepr` carried a third that was nine ranges
/// short of `shown`'s, so the two disagreed about U+0890. They are one generated file now
/// ([`crate::tui::tables`], from `tools/gen-unicode-tables.py` run against the CPython the
/// differential oracle runs), which is the only arrangement in which "one glyph rule" is a
/// fact rather than a convention.
///
/// **Escaping is not normalising, and `doctor` needs both.** This function escapes and does
/// not normalise, which is right: `contain.one_line` is a line-structure rule and a value it
/// normalised would no longer be the value. The one place `doctor` DOES normalise is
/// `$CLAUDE_CONFIG_DIR` ([`self::session`]), because Claude Code's own binary normalises it
/// and the folder a row names has to be the folder the binary opens. Four differential
/// scenarios hold the pair apart — `doctor-with-a-non-ascii-claude-config-dir`,
/// `doctor-with-a-claude-config-dir-that-is-not-nfc-normalised`,
/// `doctor-with-format-characters-in-the-claude-config-dir`, and
/// `doctor-a-memory-index-linked-to-a-name-with-no-glyph`, which is the one where a row
/// escapes rather than prints.
pub(crate) fn one_line(value: &str, limit: usize) -> String {
    crate::shown::one_line(value, limit)
}

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
    /// Where this machine's harnesses keep their configuration, for the plugin rows. `None`
    /// where nobody can tell, and for a doctor a test names ([`Self::at`]), which must never
    /// read the operator's own.
    pub(crate) machine: Option<crate::plugin_install::Machine>,
    /// Who is asking, for the `persona grant` row's active persona: this process's session
    /// and pane, and `$CHARTER_PERSONA`. Empty for a doctor a test names.
    pub(crate) ids: crate::active::Ids,
    pub(crate) persona_env: Option<String>,
    /// The home whose `.claude` the persona lint looks for installed skills in. `None` for a
    /// doctor a test names, which must never read the operator's own.
    pub(crate) home: Option<PathBuf>,
    /// Who asks a forge about the project's remote, and over what (SQ-8). `None` asks no
    /// forge, which the `project remote` row says; the preflight never asks one.
    pub(crate) forges: Option<(
        crate::forge::Caller,
        std::sync::Arc<dyn crate::forge::transport::Transport>,
    )>,
    /// The machine store whose forge request budgets the `forge budget` rows read (FW-4).
    /// `None` for a doctor a test names, which must never read the operator's own.
    pub(crate) budgets: Option<PathBuf>,
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
        let pinned = crate::steer::var_os("PURLIS_ROOT").is_some_and(|v| !v.is_empty());
        let root = crate::plane::resolve(cwd)
            .map(|p| canonical(&p))
            .unwrap_or_else(|_| canonical(cwd));
        let mut d = Self::at(&root, cwd, pinned, preflight);
        d.ids = crate::active::Ids::from_env();
        d.persona_env = crate::envvar::var(crate::active::PERSONA_ENV);
        d.home = crate::profiles::home();
        d.budgets = crate::machine::config_root_if_there();
        d.forges = Some((
            crate::forge::Caller::command(),
            std::sync::Arc::new(crate::forge::cli::Cli::default()),
        ));
        d.machine = std::env::current_exe()
            .and_then(|p| p.canonicalize())
            .ok()
            .and_then(|binary| {
                let bundle = crate::plugin_install::bundle_beside(&binary);
                crate::plugin_install::Machine::from_env_to_read(binary, bundle).ok()
            });
        d
    }

    /// This doctor, asking a forge about the project's remote as `caller`, over `transport`.
    /// Only a doctor someone asked for does: the preflight asks no forge whatever this says.
    pub fn asking_forges(
        mut self,
        caller: crate::forge::Caller,
        transport: std::sync::Arc<dyn crate::forge::transport::Transport>,
    ) -> Self {
        self.forges = Some((caller, transport));
        self
    }

    /// This doctor, reading the forge request budgets kept under the machine store
    /// `config_root`.
    pub fn reading_budgets_in(mut self, config_root: &Path) -> Self {
        self.budgets = Some(config_root.to_path_buf());
        self
    }

    /// This doctor, reading `machine`'s harness configuration.
    pub fn with_machine(mut self, machine: crate::plugin_install::Machine) -> Self {
        self.machine = Some(machine);
        self
    }

    /// The doctor for an explicit plane, which is how a test names one.
    pub fn at(root: &Path, cwd: &Path, pinned: bool, preflight: bool) -> Self {
        Self {
            root: root.to_path_buf(),
            has_plane: crate::names::has_manifest(root),
            pinned,
            cwd: cwd.to_path_buf(),
            preflight,
            config: Config::load(root),
            machine: None,
            ids: crate::active::Ids::default(),
            persona_env: None,
            home: None,
            forges: None,
            budgets: None,
        }
    }

    /// The project this doctor answers for.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The fixes this doctor's findings offer, in the order [`fix::FixId::ALL`] lists them.
    /// A bare `charter doctor --fix` applies those that are not
    /// [`fix::FixId::by_name_only`].
    ///
    /// Asks only the checks that can carry a fix id, rather than [`Doctor::run`]'s every row:
    /// those ask git and a forge, and `--fix` runs the whole doctor again after it fixes.
    pub fn fixes(&self) -> Vec<fix::FixId> {
        let mut ids: Vec<fix::FixId> = [
            plugin::plugin_install(self),
            plugin::plugin(self),
            plugin::plugin_files(self),
            config::schema(self),
            profiles::harness_profiles(self),
            memory::memory_indexes(self),
            inventory::inventory(self),
            git::identity(self),
        ]
        .into_iter()
        .chain(plane::behind_the_layout(self))
        .filter_map(|row| row.fix)
        .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Every row, in the order Python's `doctor._checks` runs them: cheap and local first.
    pub fn run(&self) -> Vec<Row> {
        let mut rows = vec![python3(), git::git(), git::identity(self)];
        for cli in config::forge_clis(self) {
            rows.push(deferred::row(&cli, deferred::FORGES));
            rows.push(deferred::row(&format!("{cli} auth"), deferred::FORGES));
        }
        rows.push(git::git_auth(self));
        rows.push(config::charter_toml(self));
        rows.push(profiles::harness_profiles(self));
        rows.extend(profiles::profile_rows(self));
        rows.push(config::schema(self));
        rows.push(git::plane_root(self));
        rows.push(git::index_lock(self));
        rows.extend(remote::project_remote(self));
        rows.extend(budget::budgets(self));
        rows.push(session::session_root(self));
        rows.push(session::session_layer(self));
        rows.push(harnesses::harness(self));
        rows.push(deferred::row("frame", deferred::FRAME));
        rows.push(deferred::row("ended tab", deferred::FRAME));
        rows.push(deferred::row("plane-root guard", deferred::GUARD));
        rows.push(deferred::row("guard seen", deferred::GUARD));
        rows.push(plane::nested(self));
        rows.extend(plane::renamed_leftovers(self));
        rows.push(clones::workspace_clones(self));
        rows.extend(clones::hidden_agents_md(self));
        rows.extend(plane::behind_the_layout(self));
        rows.push(deferred::row("workspace layer", deferred::WORKSPACE_LAYER));
        rows.push(changes::changes(self));
        rows.extend(work::work_links(self));
        rows.push(inventory::inventory(self));
        rows.push(deferred::row("vaults", deferred::VAULTS));
        rows.push(vault_registry::vault_registry(self));
        rows.extend(vaults::vault_files(self));
        rows.extend(vaults::provider_programs(self));
        rows.extend(vaults::identity_tokens(self));
        rows.push(config::version_lock(self));
        rows.extend(sandbox::sandbox(self));
        rows.extend(sandbox::blocks(self));
        rows.push(memory::memory_indexes(self));
        rows.push(personas::personas(self));
        rows.push(personas::persona_grant(self));
        rows.push(plane::front_door(self));
        rows.extend(plane::routing(self));
        rows.push(news::news(self));
        rows.push(rules::ask_rules(self));
        rows.push(rules::handoff_gate(self));
        rows.push(deferred::row("shadowed docs", deferred::SHADOWED_DOCS));
        rows.push(deferred::row("credential paths", deferred::VAULTS));
        rows.push(deferred::row("mcp", deferred::MCP));
        rows.push(plugin::plugin_install(self));
        rows.push(plugin::plugin(self));
        rows.push(plugin::plugin_files(self));
        rows.push(plugin::superseded_plugin(self));
        rows.extend(plugin::renamed_tool_rules(self));
        rows
    }
}

/// The Python charter's first row reported its own interpreter. This charter has none and
/// needs none — a fact about this binary, not a check left undone — so the row is green
/// (#373), and keeps its place so a script reading `--json` still finds it.
fn python3() -> Row {
    Row::ok(
        "python3",
        "not needed — the Python charter is retired, and this purlis has no Python in it",
    )
}

/// `path` with its links resolved, or `path` itself when it cannot be — Python's
/// `Path.resolve()`, which never refuses a path that does not exist.
pub(crate) fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// `util.short_path`: `~/…` for a path under the home directory, the path otherwise — and
/// never `~` for a path that has a segment starting with one, which a shell would read as a
/// home it does not name.
///
/// Quoted onto one line as [`fsx::path_field`] quotes a path (#449): `$CLAUDE_CONFIG_DIR` and
/// a plane's own location are set by hand or by a chat, and a newline in one must not print a
/// row of its own.
pub(crate) fn short_path(path: &Path) -> String {
    let tilde = path
        .components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('~'));
    if tilde {
        return fsx::path_field(&std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()));
    }
    if let Some(home) = crate::profiles::home()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        let rest = rest.display().to_string();
        let shown = format!("~/{}", if rest.is_empty() { "." } else { &rest });
        return one_line(&shown, PATH_DISPLAY_LIMIT);
    }
    fsx::path_field(path)
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
///
/// **A row with a fix carries a fifth key, `fix`, after the four** (FX-1, D-FX1-1): the id
/// `charter doctor --fix <id>` takes. Only on those rows, so every other row prints exactly the
/// four keys it always has, and a reader that looks rows up by their keys reads on unchanged.
pub fn json(rows: &[Row]) -> String {
    let doc = serde_json::Value::Array(
        rows.iter()
            .map(|r| {
                let mut row = serde_json::json!({
                    "name": r.name,
                    "status": r.status.word(),
                    "detail": r.detail,
                    "hint": r.hint,
                });
                if let (Some(fix), Some(row)) = (r.fix, row.as_object_mut()) {
                    row.insert("fix".to_owned(), fix.id().into());
                }
                row
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
    let mut out = String::from("purlis preflight:\n\n");
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
            "\u{2717} {} blocker(s): {}. Fix the \u{2192} hints above, then re-run `purlis \
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

/// `charter doctor --fix`'s one plane repair: the default ask rule for `charter report --yes`
/// (ADR 0059, amended 2026-09-26), when the plane or the directory the doctor runs in lacks it.
/// Written by `charter guard ask`'s own writer, every harness or none, and carried into every
/// workspace layer charter generates. What that command prints and its exit status, or `None`
/// when there is no plane or nothing to add.
pub fn fix_report_rule(cwd: &Path) -> Option<(String, u8)> {
    let root = canonical(&crate::plane::resolve(cwd).ok()?);
    if !crate::names::has_manifest(&root) {
        return None;
    }
    let here = canonical(cwd);
    let whole = |at: &Path| rules::report_rule_missing(&root, at).is_ok_and(|m| m.is_empty());
    // The session directory counts only where a charter command writes its settings: the plane
    // root and a workspace or its clone. Anywhere else nothing `--fix` does could reach it.
    let reached = here == root || rules::reinit_reaches(&root, &here);
    if whole(&root) && (!reached || whole(&here)) {
        return None;
    }
    Some(crate::guardcmd::report(
        &root,
        crate::scaffold::settings::REPORT_RULE,
        crate::guardcmd::Bucket::Ask,
        false,
    ))
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
