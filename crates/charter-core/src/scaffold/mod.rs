//! `charter init` and `charter reinit`: scaffold a control plane, and heal one a newer
//! charter expects more of.
//!
//! A port of `charter/commands.py:cmd_init` and `cmd_reinit`, and of what they call.
//!
//! # The contract, and where it is stricter than the Python
//!
//! **Additive and idempotent: create only what is absent, never modify an existing value.**
//! When a path charter wants is occupied by something it cannot safely touch, it names the
//! blocker, deletes and renames nothing, still creates everything else, and exits 1.
//!
//! `init` writes into a directory an operator POINTS it at — possibly a repository with
//! content they care about, possibly one somebody else committed. So every path either
//! command writes or reads is gated first, and **a path that resolves outside the plane is
//! a blocker**, whatever put it there: a committed `.gitignore -> ~/.bashrc`, a `personas`
//! link to another tree, a dangling `charter.toml -> /elsewhere/x`. Nothing is read or
//! written through it, and the gate is `contain::within_plane` on the exact path, never its
//! parent. A link that lands back INSIDE the plane is followed, as Python follows it.
//! A link that lands in the plane's own `.git` is refused too: nothing charter writes here
//! belongs in a repository's internals.
//!
//! Where the Python leaves a file in a way the contract does not allow, this does not follow
//! it, and each place says so: a CRLF `.gitignore` or `charter.toml` keeps its line endings
//! here (Python's universal newlines rewrite them), and a settings file Python would crash
//! on is left alone and reported.
//!
//! # What `init` does not do here, on purpose
//!
//! - **It installs no software.** Python's `init` runs `claude plugin install` when `claude`
//!   is on `PATH` (`_provision_harnesses`). The app wires a Claude Code profile at the
//!   launch that needs it (`wiring::wired_or_refusal`), and one door for an install is the
//!   design (ADR 0022).
//! - **It writes nothing outside the plane.** Python's `init` puts opencode's plugin, command
//!   and instructions into `~/.config/opencode` (`OpenCodeHarness.wire`). charter-app v1
//!   does not start opencode (`wiring::not_startable`), and a shim whose every hook reaches
//!   a binary that refuses opencode's tool hooks would block opencode on the whole machine.
//!   The plane's own `opencode.json` ask rule IS written: it is part of the plane.
//! - **`--clone-this-repo` refuses rather than clones.** The clone is handed charter's git
//!   policy (`gitpolicy.apply`), which is a security-critical part not ported yet (spec
//!   decision 16). The plane itself is still created, as Python creates it before cloning.

pub mod planefile;
pub mod settings;
pub mod text;

use std::path::{Path, PathBuf};

use crate::plane::Place;
use settings::Wrote;

/// One line a command says, in the voice `charter/util.py` gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Say {
    /// `• ` — a note.
    Info(String),
    /// `✓ ` — it worked.
    Ok(String),
    /// `! ` — something the operator should look at.
    Warn(String),
    /// `✗ ` — something asked for did not happen.
    Err(String),
}

/// Everything a command said, in order, and its exit status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub said: Vec<Say>,
    pub code: u8,
}

/// The forges `init --forge` takes (`forge/registry.py:KINDS`), sorted as argparse lists them.
pub const FORGES: [&str; 2] = ["github", "gitlab"];

/// The directories every plane has (`instance.BASELINE_DIRS`).
pub const BASELINE_DIRS: [&str; 3] = ["personas", "inventory", "workspaces"];

/// What `charter init` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitArgs {
    pub forge: String,
    pub owner: String,
    pub host: Option<String>,
    pub clone_this_repo: bool,
    /// The front-door persona to scaffold, or `None` for `--no-front-door`.
    pub front_door: Option<String>,
}

/// The `.gitignore` a fresh plane gets (`commands._GITIGNORE_BASELINE`).
pub const GITIGNORE_BASELINE: &str = "\
# Per-task workspaces (workspaces/<name>/). LOCAL by default = fully private (clones,
# memory, manifest all ignored). Made LIVE via `charter workspace live <name>` un-ignores
# its workspace.json + memory/ in a managed block here (see charter/workspace.py).
/workspaces/*/*
!/workspaces/.gitkeep

# Per-developer secret vaults + registry (plaintext secrets, tokens, file paths).
# NEVER commit this — it holds credentials.
/.charter/

# This machine's own harness permissions (`charter guard ask|allow --local`). Its committed
# sibling `.claude/settings.json` is deliberately NOT ignored — that one is the team's.
/.claude/settings.local.json

# This machine's harness profiles (commands, config folders). Never committed: a profile's
# command runs on a click, and a merged edit would run it on every machine.
/charter.local.toml

# Python
__pycache__/
*.py[cod]
.venv/

# OS / editor cruft
.DS_Store
";

const LOCAL_SETTINGS_IGNORE: &str = "/.claude/settings.local.json";
const LOCAL_PROFILES_IGNORE: &str = "/charter.local.toml";

/// The statuses that report what charter DID (`commands._WIRED_NOTES`); every other one is
/// something the operator has to act on, so it is a warning.
const WIRED_NOTES: [&str; 6] = [
    "created",
    "installed",
    "present",
    "refreshed",
    "current",
    "added",
];

/// A path that resolves somewhere charter will not write: `(the path as named, where it lands)`.
type Escape = (String, String);

/// Everything a run collects before it reports.
#[derive(Default)]
struct Run {
    said: Vec<Say>,
    created: Vec<String>,
    present: Vec<String>,
    /// Baseline paths occupied by something that is not a directory: `(name, path)`.
    blocked: Vec<(String, PathBuf)>,
    /// Paths that resolve out of the plane.
    escapes: Vec<Escape>,
    /// Something asked for failed in a way that has already been said.
    failed: bool,
}

impl Run {
    fn info(&mut self, s: impl Into<String>) {
        self.said.push(Say::Info(s.into()));
    }
    fn ok(&mut self, s: impl Into<String>) {
        self.said.push(Say::Ok(s.into()));
    }
    fn warn(&mut self, s: impl Into<String>) {
        self.said.push(Say::Warn(s.into()));
    }
    fn err(&mut self, s: impl Into<String>) {
        self.said.push(Say::Err(s.into()));
    }

    /// `rel` under `root`, or `None` with the escape recorded — once per path.
    fn gate(&mut self, root: &Path, rel: &str) -> Option<PathBuf> {
        match gate(root, rel) {
            Ok(path) => Some(path),
            Err(escape) => {
                if !self.escapes.iter().any(|(r, _)| *r == escape.0) {
                    self.escapes.push(escape);
                }
                None
            }
        }
    }

    fn outcome(self, code: u8) -> Outcome {
        Outcome {
            said: self.said,
            code,
        }
    }
}

/// `root/rel` when it resolves inside the plane and outside its `.git`, else where it lands.
fn gate(root: &Path, rel: &str) -> Result<PathBuf, Escape> {
    let path = root.join(rel);
    let git = root.join(".git");
    let into_git = crate::contain::resolved(&path)
        .is_some_and(|lands| crate::contain::resolved(&git).is_some_and(|g| lands.starts_with(g)));
    if crate::contain::within_plane(root, &path) && !into_git {
        return Ok(path);
    }
    let lands = crate::contain::resolved(&path)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "a place charter cannot resolve (too many links)".to_owned());
    Err((rel.to_owned(), lands))
}

/// Whether anything at all is at `path`, a dangling link included.
fn occupied(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// The words the OS gave for `e`, without Rust's `(os error N)` — Python's `strerror`.
fn strerror(e: &std::io::Error) -> String {
    let text = e.to_string();
    match text.rfind(" (os error ") {
        Some(at) => text[..at].to_owned(),
        None => text,
    }
}

/// The refusal a command meets on a plane whose format this charter cannot place
/// (`cli._plane_refusal`). `init` and `reinit` are not exempt: both write into the plane.
fn refused(root: &Path) -> Option<Outcome> {
    // Read only through the gate: a `charter.toml` that is a link out of the plane is not a
    // file charter reads, and the command reports it as a blocker instead.
    gate(root, crate::plane::MANIFEST).ok()?;
    match planefile::load(root) {
        planefile::Read::Refused(why) => Some(Outcome {
            said: vec![Say::Err(format!(
                "{why} Nothing was run. `charter doctor` reports it; `charter update` is the way \
                 out."
            ))],
            code: 1,
        }),
        _ => None,
    }
}

/// `charter init`.
pub fn init(place: &Place, args: &InitArgs) -> Outcome {
    let root = &place.root;
    if let Some(refusal) = refused(root) {
        return refusal;
    }
    let mut run = Run::default();
    if !FORGES.contains(&args.forge.as_str()) {
        run.err(format!(
            "unknown --forge {} — known kinds: {}",
            text::py_repr(&args.forge),
            FORGES.join(", ")
        ));
        return run.outcome(1);
    }

    // charter.toml
    if let Some(path) = run.gate(root, crate::plane::MANIFEST) {
        if path.exists() {
            run.present.push("charter.toml".to_owned());
        } else {
            match std::fs::write(
                &path,
                planefile::render(&args.forge, &args.owner, args.host.as_deref()),
            ) {
                Ok(()) => {
                    run.created.push("charter.toml".to_owned());
                    if args.owner.is_empty() {
                        run.warn(
                            "No --owner given — charter.toml's [[forge]] block has no \
                             owner/group set. Add one before `charter discover`.",
                        );
                    }
                }
                Err(e) => {
                    run.err(format!(
                        "could not write {} ({})",
                        path.display(),
                        strerror(&e)
                    ));
                    run.failed = true;
                }
            }
        }
    }

    baseline_dirs(&mut run, root);

    if let Some(path) = run.gate(root, ".gitignore") {
        match ensure_gitignore(&path) {
            Ok(true) => run.created.push(".gitignore".to_owned()),
            Ok(false) => run.present.push(".gitignore".to_owned()),
            Err(e) => {
                run.err(format!("could not read or write {} ({e})", path.display()));
                run.failed = true;
            }
        }
    }

    let settings_ok = settings_gate(&mut run, root);
    if settings_ok {
        match settings::ensure_env(root, "CHARTER_HARNESS", "claude-code") {
            Wrote::Created => run.created.push(".claude/settings.json (env)".to_owned()),
            Wrote::Present => run.present.push(".claude/settings.json (env)".to_owned()),
            Wrote::Malformed(_) => {}
            Wrote::Blocked(dir) => run.blocked.push((".claude".to_owned(), dir)),
            Wrote::Failed(path, e) => write_failed(&mut run, &path, &e),
        }
    }

    handoff_gate(&mut run, root, settings_ok);

    profile_wiring(&mut run, root, true);

    if let Some(name) = &args.front_door {
        front_door(&mut run, root, name);
    }

    let mut malformed_settings = false;
    if settings_ok {
        match settings::ensure_guard_hook(root, crate::profiles::home().as_deref()) {
            Wrote::Created => run
                .created
                .push(".claude/settings.json (plane-root guard)".to_owned()),
            Wrote::Present => run
                .present
                .push(".claude/settings.json (plane-root guard already wired)".to_owned()),
            Wrote::Malformed(_) => {
                malformed_settings = true;
                let path = root.join(settings::SETTINGS);
                run.warn(format!(
                    "{}\n{}",
                    left_untouched(&path),
                    settings::hooks_snippet()
                ));
            }
            Wrote::Blocked(dir) => run.blocked.push((".claude".to_owned(), dir)),
            Wrote::Failed(path, e) => write_failed(&mut run, &path, &e),
        }
    }

    if !run.blocked.is_empty() || !run.escapes.is_empty() || malformed_settings || run.failed {
        report_blockers(&mut run, "init");
        if !run.created.is_empty() {
            let line = format!("  created: {}", run.created.join(", "));
            run.info(line);
        }
        if !run.present.is_empty() {
            let line = format!("  already present: {}", run.present.join(", "));
            run.info(line);
        }
        return run.outcome(1);
    }

    if run.created.is_empty() {
        run.ok(format!(
            "Control plane already fully set up (schema {}) — nothing to do.",
            planefile::SCHEMA
        ));
    } else {
        let entries = fold(&run.created);
        run.ok(format!(
            "Initialized control plane (schema {}) — {} item(s) written.",
            planefile::SCHEMA,
            entries.len()
        ));
        for item in entries {
            run.info(format!("  + {item}"));
        }
    }
    if !run.present.is_empty() {
        let line = format!("  already present: {}", run.present.join(", "));
        run.info(line);
    }
    run.info(
        "Next: `charter doctor` to preflight, then `charter discover` to build the inventory.",
    );
    let code = first_clone_step(&mut run, root, args.clone_this_repo);
    run.outcome(code)
}

/// `charter reinit`.
pub fn reinit(place: &Place) -> Outcome {
    let root = &place.root;
    if let Some(refusal) = refused(root) {
        return refusal;
    }
    let mut run = Run::default();
    if !place.is_plane {
        run.err(
            "no control plane found (no charter.toml here or in any parent) — `charter reinit` \
             only works inside one.",
        );
        return run.outcome(1);
    }

    baseline_dirs(&mut run, root);

    if let Some(path) = run.gate(root, ".gitignore") {
        match append_gitignore(
            &path,
            &[LOCAL_PROFILES_IGNORE],
            "added by `charter reinit` — harness profiles stay on this machine",
        ) {
            Ok(written) if !written.is_empty() => run
                .created
                .push(format!(".gitignore ({LOCAL_PROFILES_IGNORE})")),
            Ok(_) => {}
            Err(e) => {
                run.err(format!("could not read or write {} ({e})", path.display()));
                run.failed = true;
            }
        }
    }

    let settings_ok = settings_gate(&mut run, root);
    if settings_ok {
        match settings::ensure_env(root, "CHARTER_HARNESS", "claude-code") {
            Wrote::Created => run.created.push(".claude/settings.json (env)".to_owned()),
            Wrote::Failed(path, e) => write_failed(&mut run, &path, &e),
            _ => {}
        }
    }

    profile_wiring(&mut run, root, false);

    if settings_ok {
        match settings::ensure_guard_hook(root, crate::profiles::home().as_deref()) {
            Wrote::Created => run
                .created
                .push(".claude/settings.json (plane-root guard)".to_owned()),
            Wrote::Present => run
                .present
                .push(".claude/settings.json (plane-root guard already wired)".to_owned()),
            Wrote::Malformed(_) => {
                let path = root.join(settings::SETTINGS);
                run.warn(format!(
                    "{}\n{}",
                    left_untouched(&path),
                    settings::hooks_snippet()
                ));
            }
            // Python's `reinit` drops this status on the floor; the `.claude` blocker is
            // reported here rather than lost.
            Wrote::Blocked(dir) => run.blocked.push((".claude".to_owned(), dir)),
            Wrote::Failed(path, e) => write_failed(&mut run, &path, &e),
        }
    }

    if !run.blocked.is_empty() || !run.escapes.is_empty() || run.failed {
        report_blockers(&mut run, "reinit");
        if !run.created.is_empty() {
            let line = format!("  created: {}", run.created.join(", "));
            run.info(line);
        }
        if !run.present.is_empty() {
            let line = format!("  already present: {}", run.present.join(", "));
            run.info(line);
        }
        return run.outcome(1);
    }

    if run.created.is_empty() {
        run.ok(format!(
            "Up to date (schema {}) — nothing to do.",
            planefile::SCHEMA
        ));
        return run.outcome(0);
    }
    let line = format!(
        "Reinitialized control plane → added {}.",
        run.created.join(", ")
    );
    run.ok(line);
    if !run.present.is_empty() {
        let line = format!("  already present: {}", run.present.join(", "));
        run.info(line);
    }
    run.outcome(0)
}

/// `commands._create_baseline_dirs`: create each baseline directory that is absent, and
/// block on one whose path holds something that is not a directory — a file, or a link that
/// leads nowhere. Python crashes on the dangling link (`mkdir` meets `EEXIST`); it is a
/// blocker here like any other thing in the way.
fn baseline_dirs(run: &mut Run, root: &Path) {
    for dir in BASELINE_DIRS {
        let Some(path) = run.gate(root, dir) else {
            continue;
        };
        if path.is_dir() {
            run.present.push(format!("{dir}/"));
        } else if occupied(&path) {
            run.blocked.push((dir.to_owned(), path));
        } else {
            match std::fs::create_dir_all(&path) {
                Ok(()) => run.created.push(format!("{dir}/")),
                Err(e) => write_failed(run, &path, &e),
            }
        }
    }
}

/// Whether the settings file may be touched at all: inside the plane, and under a `.claude`
/// that is a directory or absent. A `.claude` FILE is a blocker Python meets only after it
/// has crashed trying to write under it.
fn settings_gate(run: &mut Run, root: &Path) -> bool {
    let Some(dir) = run.gate(root, ".claude") else {
        return false;
    };
    if occupied(&dir) && !dir.is_dir() {
        run.blocked.push((".claude".to_owned(), dir));
        return false;
    }
    run.gate(root, settings::SETTINGS).is_some()
}

fn write_failed(run: &mut Run, path: &Path, e: &std::io::Error) {
    run.err(format!(
        "could not write {} ({}) — left untouched.",
        path.display(),
        strerror(e)
    ));
    run.failed = true;
}

/// Every blocker, one sentence each, in charter's words where Python has them.
fn report_blockers(run: &mut Run, command: &str) {
    let blocked = std::mem::take(&mut run.blocked);
    for (name, path) in blocked {
        run.err(format!(
            "{name}/ can't be created — {} already exists and is not a directory. charter never \
             deletes or renames existing content; move or remove it yourself, then re-run \
             `charter {command}`.",
            path.display()
        ));
    }
    let escapes = std::mem::take(&mut run.escapes);
    for (rel, lands) in escapes {
        run.err(format!(
            "{} resolves to {}, which is outside this plane or inside its .git — charter reads \
             and writes nothing through it. Point it inside the plane or remove it yourself, \
             then re-run `charter {command}`.",
            crate::shown::readable(&rel, 1024),
            crate::shown::readable(&lands, 1024)
        ));
    }
}

/// `commands._settings_left_untouched`.
fn left_untouched(path: &Path) -> String {
    format!(
        "{} is not a settings file charter can read and write back as JSON — left it \
         completely untouched. Wire the plane-root guard yourself:",
        crate::shown::readable(&path.display().to_string(), 1024)
    )
}

/// `commands._fold_entries`: `a.json (x)`, `a.json (y)` → `a.json (x, y)`, order kept.
fn fold(entries: &[String]) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    let mut notes: Vec<Vec<String>> = Vec::new();
    for entry in entries {
        let (head, tail) = match entry.split_once(" (") {
            Some((head, tail)) => (head, tail.trim_end_matches(')')),
            None => (entry.as_str(), ""),
        };
        let at = match order.iter().position(|h| h == head) {
            Some(at) => at,
            None => {
                order.push(head.to_owned());
                notes.push(Vec::new());
                order.len() - 1
            }
        };
        if !tail.is_empty() && !notes[at].iter().any(|n| n == tail) {
            notes[at].push(tail.to_owned());
        }
    }
    order
        .into_iter()
        .zip(notes)
        .map(|(head, notes)| {
            if notes.is_empty() {
                head
            } else {
                format!("{head} ({})", notes.join(", "))
            }
        })
        .collect()
}

/// `commands._ensure_gitignore`: add the baseline's rules to `.gitignore` — the whole file
/// when there is none, else only the lines it is missing. True iff it wrote.
///
/// The presence check is Python's to the letter, including its one substring test
/// (`.charter/` anywhere in the body) and its whole-line tests for the rest.
fn ensure_gitignore(path: &Path) -> Result<bool, String> {
    if !path.exists() {
        std::fs::write(path, GITIGNORE_BASELINE).map_err(|e| strerror(&e))?;
        return Ok(true);
    }
    let body = read_text(path)?;
    let lines = text::stripped_lines(&body);
    let mut missing: Vec<&str> = Vec::new();
    if !lines.contains(&"!/workspaces/.gitkeep") {
        missing.extend(["/workspaces/*/*", "!/workspaces/.gitkeep"]);
    }
    if !body.contains(".charter/") {
        missing.push("/.charter/");
    }
    if !lines.contains(&LOCAL_SETTINGS_IGNORE) {
        missing.push(LOCAL_SETTINGS_IGNORE);
    }
    if !lines.contains(&LOCAL_PROFILES_IGNORE) {
        missing.push(LOCAL_PROFILES_IGNORE);
    }
    if missing.is_empty() {
        return Ok(false);
    }
    append_gitignore(path, &missing, "added by `charter init`")?;
    Ok(true)
}

fn read_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| strerror(&e))?;
    String::from_utf8(bytes).map_err(|_| "it is not UTF-8 text".to_owned())
}

/// `util.append_gitignore`: append the `lines` the file is missing under a `# header`, and
/// return the ones written. Append-only and whole-line: `workspace.set_live` splices its
/// block at the literal `!/workspaces/.gitkeep`, so a writer that rewrote or reordered would
/// break live workspaces from across the codebase.
///
/// Trailing line breaks are collapsed to the one blank line before the new block, as Python
/// does; every other byte, CRLF endings included, is kept.
fn append_gitignore(path: &Path, lines: &[&str], header: &str) -> Result<Vec<String>, String> {
    let body = if path.exists() {
        read_text(path)?
    } else {
        String::new()
    };
    let present = text::stripped_lines(&body);
    let missing: Vec<String> = lines
        .iter()
        .filter(|l| !present.contains(*l))
        .map(|l| (*l).to_owned())
        .collect();
    if missing.is_empty() {
        return Ok(missing);
    }
    let prefix = if crate::memstore::py_strip(&body).is_empty() {
        String::new()
    } else {
        format!("{}\n\n", body.trim_end_matches(['\n', '\r']))
    };
    let block: String = missing.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(path, format!("{prefix}# {header}\n{block}")).map_err(|e| strerror(&e))?;
    Ok(missing)
}

/// `commands.ensure_handoff_gate`: the ask rule for `charter handoff *` in every harness that
/// can hold one, or in none of them. Every file is asked first with nothing written, and one
/// that cannot take the rule stops the whole write — a rule in force under one harness and
/// not another is the split `charter guard` exists to prevent.
fn handoff_gate(run: &mut Run, root: &Path, settings_ok: bool) {
    let opencode_ok = run.gate(root, settings::OPENCODE).is_some();
    if !settings_ok || !opencode_ok {
        // The blocker is reported where it was found; nothing is written anywhere.
        return;
    }
    let checked = [
        (
            "claude-code",
            settings::ensure_ask_rule(root, settings::HANDOFF_RULE, true),
        ),
        (
            "opencode",
            settings::ensure_opencode_ask(root, settings::HANDOFF_PATTERN, true),
        ),
    ];
    let bad: Vec<&str> = checked
        .iter()
        .filter_map(|(_, w)| match w {
            Wrote::Malformed(detail) => Some(detail.as_str()),
            _ => None,
        })
        .collect();
    if !bad.is_empty() {
        let line = format!(
            "the ask rule for `charter handoff` was not written anywhere — {} is not valid, and \
             `charter guard` writes every harness or none. Fix it, then: charter guard ask \
             'charter handoff *'",
            bad.join(", ")
        );
        run.warn(line);
        return;
    }
    for (name, check) in checked {
        let (rel, wrote) = match (name, check) {
            ("claude-code", Wrote::Created) => (
                settings::SETTINGS,
                settings::ensure_ask_rule(root, settings::HANDOFF_RULE, false),
            ),
            ("opencode", Wrote::Created) => (
                settings::OPENCODE,
                settings::ensure_opencode_ask(root, settings::HANDOFF_PATTERN, false),
            ),
            ("claude-code", other) => (settings::SETTINGS, other),
            (_, other) => (settings::OPENCODE, other),
        };
        match wrote {
            Wrote::Created => run.created.push(format!("{rel} (ask: charter handoff)")),
            Wrote::Present => run.present.push(format!("{rel} (ask: charter handoff)")),
            Wrote::Failed(path, e) => {
                run.err(format!(
                    "{name}: could not write {} ({}) — left untouched.",
                    path.display(),
                    strerror(&e)
                ));
                run.info(
                    "  charter asked every harness before writing any of them, so this arrived \
                     AFTER the check. Re-run once that file can be written.",
                );
            }
            Wrote::Blocked(dir) => run.blocked.push((".claude".to_owned(), dir)),
            Wrote::Malformed(_) => {}
        }
    }
}

/// `commands._wire_profiles`: wire each DECLARED profile's own config folder, one line each.
///
/// Nothing here fails `init` or `reinit` — a profile's own account folder is not what those
/// commands are for. `install` is `init`'s door (ADR 0022's ruling 9: `reinit` installs no
/// software). What charter-app cannot wire — opencode's shim, which Python writes — is said
/// in `wiring::install`'s words rather than skipped.
fn profile_wiring(run: &mut Run, root: &Path, install: bool) {
    use crate::profiles::{self, Source};
    if !profiles::ignore_check(root).passes() {
        return;
    }
    let set = profiles::current(root);
    let mut lines: Vec<(String, String)> = Vec::new();
    for p in set.profiles() {
        if p.source == Source::BuiltIn {
            continue;
        }
        let name = crate::shown::short(&p.name);
        let label = format!("profile '{name}'");
        if let Some(state) = crate::profiletrust::approval_needed(root, p) {
            lines.push((
                "skipped".to_owned(),
                format!(
                    "{label} is {} and not approved yet — run charter {name} once to approve \
                     its command",
                    state.as_str()
                ),
            ));
            continue;
        }
        if p.kind == "codex" {
            lines.push((
                "opt-in".to_owned(),
                format!("{label}: charter harness install {name}"),
            ));
            continue;
        }
        if install || p.kind == "opencode" {
            for step in crate::wiring::install(p, root) {
                lines.push((step.status, format!("{label}: {}", step.detail)));
            }
            continue;
        }
        let w = crate::wiring::detect(p, root, root);
        if w.state != crate::wiring::State::Wired {
            lines.push((
                "missing".to_owned(),
                format!(
                    "{label}: not wired — {}; {}",
                    crate::wiring::said(&w.detail),
                    crate::wiring::said(&w.fix)
                ),
            ));
        }
    }
    for (status, label) in lines {
        if WIRED_NOTES.contains(&status.as_str()) {
            run.info(format!("  {label}"));
        } else {
            run.warn(format!("  {label}"));
        }
    }
}

/// `^[a-z0-9][a-z0-9._-]*$` — a name `persona.valid_name` accepts. `_shared` is not one.
fn persona_name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
}

/// The persona file the front door is scaffolded with (`commands._FRONT_DOOR`).
fn front_door_text(name: &str, role: &str) -> String {
    format!(
        r#"---
name: {name}
role: {role}
vault: none
routing: advise
delegate-when: routing work to the right persona, and scoping a request before code is written
---

# {role}

You are the front door of this control plane. Your job is to understand what is actually
being asked, then either do it or hand it to the persona that owns it — not to start
editing the first file that looks relevant.

## Routing

This plane has no other personas yet, so there is nothing to route to. That is the first
thing worth fixing, not a reason to do everything here:

```
charter persona create <name> --role "<Role>" \
  --delegate-when "<the work that should come to it>"
```

`delegate-when` is what makes a persona findable — it becomes the description whoever is
routing reads. Create one the moment a second kind of work appears in this plane.

Once others exist, `routing: advise` above puts them in front of you on work-shaped
prompts: who exists, what each claims, when each was last dispatched. charter never says
which one owns the request — that call is yours. Route on the *work*, not on the file a
change happens to touch.

Cross-cutting changes stay with you: splitting one coherent change across three personas
costs more in lost context than it saves.

## Scout before you scope

Read the thing before proposing a change to it, and check what this plane already knows —
`charter recall "<keywords>"` searches your memory, the shared namespace and the active
workspace's journal at once.

## What you own

Personas, workspaces, memory and vaults — the shape of this plane. Definitions and memory
are committed and shared; credentials never are.

Record durable facts with `charter persona remember {name} "<fact>"`, and `--shared` for
anything every persona needs.

This file is yours: rename it, rewrite it, or delete it and declare a different front door
with `charter persona default <name>`.
"#
    )
}

/// `commands._ensure_front_door`: scaffold one generic persona and declare it — only on a
/// plane with no persona at all and no declared default. A roster is not absent because one
/// particular name is, and a declared default is a question somebody already answered.
fn front_door(run: &mut Run, root: &Path, name: &str) {
    let Some(personas) = run.gate(root, "personas") else {
        return;
    };
    if !personas.is_dir() {
        // Blocked, and said so where the baseline was made. Python goes on to `mkdir` under
        // the file and ends in a traceback.
        return;
    }
    if let Ok(entries) = std::fs::read_dir(&personas) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_md = path.extension().is_some_and(|e| e == "md");
            if occupied(&path.join("persona.md")) || is_md {
                return;
            }
        }
    }
    if run.gate(root, crate::plane::MANIFEST).is_none() {
        return;
    }
    match planefile::load(root) {
        planefile::Read::Config(cfg) if planefile::declares_default_persona(&cfg) => return,
        planefile::Read::Malformed(why) => {
            // Python raises here and ends `init` with a traceback, leaving the guard hook
            // unwritten. Said, and the rest of `init` goes on.
            run.err(format!("{why} — no front door was scaffolded."));
            run.failed = true;
            return;
        }
        _ => {}
    }
    if !persona_name_ok(name) {
        run.warn(format!(
            "--front-door {} is not a valid persona name — skipped.",
            text::py_repr(name)
        ));
        return;
    }
    let rel = format!("personas/{name}");
    let files = [
        format!("{rel}/persona.md"),
        format!("{rel}/memory/.gitkeep"),
        format!("{rel}/refs/.gitkeep"),
    ];
    if run.gate(root, &rel).is_none() || files.iter().any(|f| run.gate(root, f).is_none()) {
        return;
    }
    let role = text::py_title(&name.replace(['-', '_'], " "));
    fn scaffold(dir: &Path, name: &str, role: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("persona.md"), front_door_text(name, role))?;
        for sub in ["memory", "refs"] {
            std::fs::create_dir_all(dir.join(sub))?;
            // `Path.touch`: created when absent, never truncated.
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(sub).join(".gitkeep"))?;
        }
        Ok(())
    }
    if let Err(e) = scaffold(&root.join(&rel), name, &role) {
        write_failed(run, &root.join(&rel), &e);
        return;
    }
    // Python ignores a declaration that could not be written (`_try_set_key`'s `False`), and
    // so does this: the persona is on disk either way, and `doctor` names a front door that
    // is not declared.
    let _ = planefile::set_key(root, "persona", "default", name);
    run.created.push(format!(
        "personas/{name}/ (front door, declared in charter.toml)"
    ));
}

/// `commands._first_clone_step`: the one thing being inside a git repo changes about `init`
/// — what it OFFERS. Nothing is ever cloned that was not asked for by name.
fn first_clone_step(run: &mut Run, root: &Path, accepted: bool) -> u8 {
    let here = is_repo_top_level(root);
    if !accepted {
        if here {
            let name = first_clone_name(root);
            let ws = match planefile::load(root) {
                planefile::Read::Config(cfg) => planefile::default_workspace(&cfg),
                _ => "default".to_owned(),
            };
            if !root.join("workspaces").join(&ws).join(&name).exists() {
                run.info(format!(
                    "You are standing in the git repo '{name}'. Work happens in a workspace, not \
                     in the plane root — clone it into the first one:\n      charter init \
                     --clone-this-repo\n  Nothing is cloned unless you run that. It lands in \
                     workspaces/{ws}/{name}/, and declining leaves this plane complete."
                ));
            }
        }
        return 0;
    }
    if !here {
        run.err(format!(
            "--clone-this-repo: there is no repo here to clone. {} is not the top level of a \
             git working tree, and the flag clones the repo you are standing in. The control \
             plane itself was still created.",
            root.display()
        ));
        return 1;
    }
    run.err(
        "--clone-this-repo: this charter does not clone the repo you are standing in yet. \
         `charter clone` does exist, and it applies the git policy — but it clones INTO a \
         workspace that already exists, and `init` has just made a plane with none. The \
         control plane itself was still created; clone the repo into a workspace with git, \
         then run `charter reinit`.",
    );
    1
}

/// `commands._is_repo_top_level`: `root` is the TOP of a git working tree — not merely
/// inside one, because a `$HOME` kept under git would otherwise have `init` offering to
/// clone the home directory.
fn is_repo_top_level(root: &Path) -> bool {
    use crate::worktree::git;
    let Ok(top) = git::run(root, &["rev-parse", "--show-toplevel"], git::READ) else {
        return false;
    };
    if !top.ok() || top.line().is_empty() {
        return false;
    }
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    canon(Path::new(top.line())) == canon(root)
}

/// `commands._first_clone_name`: the basename of `origin`'s URL, or the directory's name
/// when that is not a workspace-shaped name.
fn first_clone_name(root: &Path) -> String {
    use crate::worktree::git;
    let url = git::run(root, &["remote", "get-url", "origin"], git::READ)
        .ok()
        .map(|r| crate::memstore::py_strip(&r.out).to_owned())
        .unwrap_or_default();
    let tail = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("");
    let tail = tail.strip_suffix(".git").unwrap_or(tail);
    if !url.is_empty() && crate::contain::workspace_name_ok(tail) {
        tail.to_owned()
    } else {
        root.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verified against CPython 3.14: `commands._fold_entries(...)`.
    #[test]
    fn entries_for_one_file_fold_onto_one_line_in_the_order_they_came() {
        let created: Vec<String> = [
            "charter.toml",
            ".claude/settings.json (env)",
            "opencode.json (ask: charter handoff)",
            ".claude/settings.json (ask: charter handoff)",
            ".claude/settings.json (plane-root guard)",
            ".claude/settings.json (env)",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(
            fold(&created),
            vec![
                "charter.toml",
                ".claude/settings.json (env, ask: charter handoff, plane-root guard)",
                "opencode.json (ask: charter handoff)",
            ]
        );
    }

    #[test]
    fn a_persona_name_is_the_alphabet_charter_mints() {
        assert!(persona_name_ok("steward"));
        assert!(persona_name_ok("front-door.2"));
        assert!(!persona_name_ok("_shared"));
        assert!(!persona_name_ok("Steward"));
        assert!(!persona_name_ok(""));
        assert!(!persona_name_ok("a/b"));
        assert!(!persona_name_ok(".x"));
    }
}
