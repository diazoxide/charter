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
//! # The one place this deliberately disagrees with the Python charter
//!
//! **`charter init` at the top of an existing git repository writes nothing and says how to
//! ask** ([`repo_is_not_a_plane_yet`]). Python scaffolds the plane into that repository and
//! then offers the first clone as a printed command; charter-app reverses which of those two
//! is the default, because ADR 0033 made "which plane" a directory picked in a file dialog
//! and a directory picked from a list has nobody standing in it. ADR 0035 decided it and
//! charter-app spec decision 27 records it.
//!
//! It is the FIRST declared hole in spec decision 15's byte-for-byte guarantee, and Python is
//! frozen (decision 17), so it does not follow. `tests/differential/run.py` carries it as a
//! `Divergence` the run asserts by name — it fails if the two ever agree again, as well as if
//! they disagree differently.
//!
//! # What `init` does not do here, on purpose
//!
//! - **It installs no software.** Python's `init` runs `claude plugin install` when `claude`
//!   is on `PATH` (`_provision_harnesses`). The app installs nothing anywhere: it ships its
//!   own plugin and loads it into each chat it starts, for that session alone
//!   (`crate::plugin`).
//! - **It writes nothing outside the plane.** Python's `init` puts opencode's plugin, command
//!   and instructions into `~/.config/opencode` (`OpenCodeHarness.wire`). charter-app v1
//!   does not start opencode (`wiring::refusal`), and a shim whose every hook reaches a
//!   binary that refuses opencode's tool hooks would block opencode on the whole machine.
//!   The plane's own `opencode.json` ask rule IS written: it is part of the plane.
//!
//! # The other half of ADR 0035's default: adopting the repo
//!
//! The refusal above is only half of what ADR 0035 decided. Its sentence is *"`charter init`
//! on an existing repo **adopts that repo as the plane's first clone** and makes the plane
//! beside it"*, and until charter-app#175 charter could do the refusing and not the adopting:
//! `--clone-this-repo` answered with a refusal of its own, on the grounds that the clone is
//! handed charter's git policy and `gitpolicy.apply` was not ported. **It has been since M2.5
//! (#64)** — `crates/charter-core/src/gitpolicy.rs` is the whole module, `apply` included —
//! so that note was stale, and the refusal it justified outlived it.
//!
//! So [`InitArgs::adopt`] names the repository to adopt, `firstclone` does the cloning for
//! both shapes, and `--clone-this-repo` clones rather than refusing. **Where the plane goes is
//! still typed, never guessed.** ADR 0035 says the plane is made "beside" the repo; it does
//! not say charter picks that directory, and `init` writing `../<name>-plane` out of a
//! directory it was pointed at would break this module's own contract — *"It writes nothing
//! outside the plane"* — for the sake of saving one `mkdir`. The operator names the plane's
//! directory and `--adopt` names the repo, which is the same two answers the app's dialog asks
//! for (charter-app#175, step 4).

pub(crate) mod firstclone;
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
    /// Make the git repository this is run in BE the plane — the old default, now asked for
    /// by name (ADR 0035, spec decision 27). It decides nothing outside a repository's top
    /// level, where there is no repository to colonise.
    pub plane_is_this_repo: bool,
    /// **ADR 0035's default, as the thing charter does rather than the thing it describes**:
    /// a repository SOMEWHERE ELSE, adopted as this plane's first clone (charter-app#175).
    ///
    /// The plane is scaffolded at the directory `init` was pointed at, exactly as it is with
    /// no flag at all, and then this repo is cloned into the plane's first workspace and left
    /// otherwise untouched. `--clone-this-repo` is the same act with the source fixed to the
    /// plane root, so the two are refused together rather than silently ranked.
    pub adopt: Option<PathBuf>,
    /// When the first workspace's manifest records it was made — `None` is the wall clock.
    /// Only a run that adopts or clones writes one, which is why it is an option rather than
    /// a field every caller has to have an answer for.
    pub now: Option<chrono::DateTime<chrono::Utc>>,
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
    // Refused before anything is written, because it is a question about what the run is FOR:
    // both flags name the source of the first clone, and a charter that silently ranked them
    // would clone one repo while the operator read the other one's name back.
    if args.adopt.is_some() && args.clone_this_repo {
        run.err(
            "--adopt and --clone-this-repo each name the repository to make this plane's first \
             clone, and they name different ones: --clone-this-repo is the repo this plane is \
             being made IN. Ask for one. Nothing was written.",
        );
        return run.outcome(1);
    }

    if let Some(refusal) = repo_is_not_a_plane_yet(root, args) {
        return refusal;
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

    profile_approvals(&mut run, root);

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
    let code = first_clone_step(&mut run, root, args);
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

    profile_approvals(&mut run, root);

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

/// `commands._wire_profiles`, as much of it as is left: one line for each DECLARED profile
/// nobody has approved yet.
///
/// Python's version also wired each profile's own config folder — `claude plugin install`,
/// Codex's marketplace, opencode's shim. The app wires nothing there: it arms every chat it
/// starts with its own plugin, for that session alone (`crate::plugin`), so there is no folder
/// to wire and nothing for `init` or `reinit` to report about one. What still stops a chat is
/// a command the operator has not approved, and that is worth a line here.
fn profile_approvals(run: &mut Run, root: &Path) {
    use crate::profiles::{self, Source};
    if !profiles::ignore_check(root).passes() {
        return;
    }
    for p in profiles::current(root).profiles() {
        if p.source == Source::BuiltIn {
            continue;
        }
        if let Some(state) = crate::profiletrust::approval_needed(root, p) {
            let name = crate::shown::short(&p.name);
            run.warn(format!(
                "  profile '{name}' is {} and not approved yet — run charter {name} once to \
                 approve its command",
                state.as_str()
            ));
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
    // The only place this binary MINTS a persona directory, and `persona.valid_name`'s
    // alphabet above admits `nul`, `con` and `com1` — measured, charter-app#96. A persona is
    // committed, so the name travels; the refusal is here and not in
    // `contain::persona_name_ok`, which every reader of an existing plane asks.
    if let Err(why) = crate::contain::mintable(name) {
        run.warn(format!(
            "--front-door {} would not name the same directory on every machine this plane \
             reaches — {why} — skipped.",
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

/// The flags the operator typed, as a shell will read them back — so the two commands the
/// refusal below prints can be pasted rather than reassembled by hand.
///
/// Only what `init` was actually given: `--forge` always, because it has a default that is
/// not the one most people want; `--owner` and `--host` when they carry something; and the
/// front-door flags only when they are not the default, which is the one `None` that means
/// `--no-front-door` rather than "unset".
fn typed_flags(args: &InitArgs) -> String {
    use crate::handoff::quote;
    let mut out = format!(" --forge {}", quote(&args.forge));
    if !args.owner.is_empty() {
        out.push_str(&format!(" --owner {}", quote(&args.owner)));
    }
    if let Some(host) = &args.host {
        out.push_str(&format!(" --host {}", quote(host)));
    }
    match &args.front_door {
        None => out.push_str(" --no-front-door"),
        Some(name) if name != "steward" => {
            out.push_str(&format!(" --front-door {}", quote(name)));
        }
        Some(_) => {}
    }
    out
}

/// **The declared divergence from the Python oracle** (ADR 0035, spec decision 27): `charter
/// init` does not make an existing git repository into a control plane unless it is asked to.
///
/// The old default — Python's, still — wrote `charter.toml`, `personas/`, `inventory/`,
/// `workspaces/` and a block of rules into that repository's own tracked `.gitignore`, and
/// only then printed the offer to clone it into the plane's first workspace. ADR 0033 turned
/// "which plane" into a directory picked in a file dialog, and `_is_repo_top_level`'s own
/// docstring is what that breaks: the offer *"exists for one person standing in one project,
/// which is the equality case"*. A directory picked from a list has nobody standing in it, so
/// the scaffolding and the `.gitignore` edit are writes the operator never typed.
///
/// So the safe shape is the default and the old one is asked for by name. **Nothing is
/// written**: this runs before the first `mkdir`, because the write that was never typed is
/// the whole subject.
///
/// Three cases are exempt, one reason each:
///
/// - `--plane-is-this-repo` IS the operator saying so;
/// - `--clone-this-repo` says it too — it asks for a clone INTO the plane this makes here,
///   which is not a request anyone types by accident;
/// - a directory that already holds a `charter.toml` is a plane, where the question was
///   answered once and `init` is now a heal. charter's own plane is that case, and so is
///   every re-run — `init` is idempotent and gets run again after a forge is added or a
///   charter is upgraded, and a refusal there would break the one flow this repo is built in.
///
/// A repository this merely sits INSIDE is untouched by any of it, exactly as the offer was:
/// a `$HOME` kept under git would otherwise refuse to hold a plane at all.
fn repo_is_not_a_plane_yet(root: &Path, args: &InitArgs) -> Option<Outcome> {
    if args.plane_is_this_repo || args.clone_this_repo {
        return None;
    }
    if already_a_plane(root) {
        return None;
    }
    if !is_repo_top_level(root) {
        return None;
    }
    let name = first_clone_name(root);
    let flags = typed_flags(args);
    let mut run = Run::default();
    run.err(format!(
        "this is the git repo '{name}', and `charter init` does not make a repository into a \
         control plane unless you ask it to. Nothing was written."
    ));
    // Two commands, not three, and the second one ADOPTS rather than describing an adoption:
    // `--adopt` clones this repo into the new plane's first workspace (charter-app#175). The
    // directory is named rather than guessed — `init` writes nothing outside the plane it was
    // pointed at, and picking `../<name>-plane` for the operator would be that write.
    let here = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.clone());
    run.info(format!(
        "A plane is a directory of its own, and this repo is the first clone in it:\n      \
         mkdir ../{name}-plane && cd ../{name}-plane\n      charter \
         init{flags} --adopt ../{here}\n  That clones this repo into the plane's first \
         workspace. Nothing here is written by any of it — this repo is read, and only read."
    ));
    // Named in full rather than summarised: this is the line somebody decides on, and "plane
    // scaffolding" is not a list anyone can picture. Built from the same constants the writes
    // come from, so a path that moves cannot leave this sentence behind.
    let written = [
        crate::plane::MANIFEST,
        settings::SETTINGS,
        settings::OPENCODE,
    ]
    .into_iter()
    .map(str::to_owned)
    .chain(BASELINE_DIRS.iter().map(|d| format!("{d}/")))
    .collect::<Vec<_>>();
    run.info(format!(
        "To make THIS repo the plane instead — charter's own plane is one, which is why the \
         option is here — ask for it by name:\n      charter init \
         --plane-is-this-repo{flags}\n  That writes {} into this repo, and charter's own \
         rules into its tracked .gitignore.",
        written.join(", ")
    ));
    run.info(
        "Why the default changed: docs/adr/0035-a-plane-is-untrusted-until-the-operator-opens-\
         it.md, and charter-app spec decision 27. `charter init` anywhere that is not the top \
         of a git repo is unchanged.",
    );
    Some(run.outcome(1))
}

/// Whether `root` already holds a `charter.toml` charter would read — asked through the gate,
/// so a manifest that is a link out of the plane does not count as one. It is not a plane
/// charter can heal, and answering "no" here only costs that run its scaffolding, which is
/// the side to be wrong on.
fn already_a_plane(root: &Path) -> bool {
    gate(root, crate::plane::MANIFEST).is_ok_and(|path| path.exists())
}

/// `commands._first_clone_step`, plus the source `--adopt` names: the one thing a repository
/// changes about `init` — what it OFFERS, and now what it can be asked to do. Nothing is ever
/// cloned that was not asked for by name.
fn first_clone_step(run: &mut Run, root: &Path, args: &InitArgs) -> u8 {
    let now = args.now.unwrap_or_else(chrono::Utc::now);
    if let Some(repo) = &args.adopt {
        return match adopted(root, repo) {
            Ok(source) => firstclone::into_first_workspace(run, root, &source, now),
            Err(why) => {
                run.err(why);
                1
            }
        };
    }
    let accepted = args.clone_this_repo;
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
    firstclone::into_first_workspace(run, root, root, now)
}

/// The repository `--adopt` may take as the plane's first clone, resolved, or why it may not
/// be one.
///
/// Three refusals, one reason each, and all three are about a path the operator typed or
/// picked in a dialog rather than one charter derived:
///
/// - **Not the top of a git working tree.** `--adopt` clones a repository; a directory that
///   is merely inside one would clone the whole enclosing repo under the name of a
///   subdirectory, which is `is_repo_top_level`'s own subject one door along.
/// - **The plane root itself.** That is `--clone-this-repo`, which already exists and says so;
///   two spellings of one act is how they drift apart.
/// - **The plane is INSIDE it.** The clone would then land inside the repository being cloned
///   — a write into somebody's repo, which is the exact thing ADR 0035 reversed the default
///   to prevent, arriving through the flag that was supposed to be the safe way.
fn adopted(root: &Path, repo: &Path) -> Result<PathBuf, String> {
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let source = canon(repo);
    if !is_repo_top_level(&source) {
        return Err(format!(
            "--adopt: {} is not the top level of a git working tree, and adopting a repository \
             is what the flag does. The control plane itself was still created.",
            source.display()
        ));
    }
    if source == canon(root) {
        return Err(
            "--adopt: that is this plane's own directory. To clone the repo the plane is \
             being made IN, ask for `--clone-this-repo`; `--adopt` is for a repository \
             somewhere else, with the plane beside it (ADR 0035)."
                .to_owned(),
        );
    }
    if canon(root).starts_with(&source) {
        return Err(format!(
            "--adopt: this plane is inside {}, so cloning that repo would write the clone into \
             the repository it came from. Make the plane in a directory of its own, beside the \
             repo. The control plane itself was still created.",
            source.display()
        ));
    }
    Ok(source)
}

/// `commands._is_repo_top_level`: `root` is the TOP of a git working tree — not merely
/// inside one, because a `$HOME` kept under git would otherwise have `init` offering to
/// clone the home directory.
fn is_repo_top_level(root: &Path) -> bool {
    use crate::worktree::git;
    let Ok(top) = git::run(root, &["rev-parse", "--show-toplevel"], git::READ) else {
        return false;
    };
    // `||` rather than `&&` is not observable, and `.cargo/mutants.toml` excludes that mutant:
    // either way a run that falls through compares `top.line()` with the resolved root, and
    // an empty line (`canonicalize("")` fails, leaving `""`) or a failed git's empty stdout
    // never equals a real directory. The guard is Python's `returncode != 0 or not top`.
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

    /// `InitArgs` as `charter init --forge github --owner acme` builds it.
    fn plain() -> InitArgs {
        InitArgs {
            forge: "github".to_owned(),
            owner: "acme".to_owned(),
            host: None,
            clone_this_repo: false,
            plane_is_this_repo: false,
            adopt: None,
            now: None,
            front_door: Some("steward".to_owned()),
        }
    }

    /// The app reaches `init` with no argument parser in between (`opener::create_project`),
    /// so clap's `conflicts_with` is not the only thing standing between two flags that name
    /// two different sources for one first clone. This is the other one, and it refuses
    /// before the first directory is made.
    #[test]
    fn adopt_and_clone_this_repo_together_are_refused_and_nothing_is_written() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = dir.path().join("plane");
        std::fs::create_dir_all(&root).expect("the plane's directory");
        let place = Place {
            root: root.clone(),
            is_plane: false,
        };

        let outcome = init(
            &place,
            &InitArgs {
                clone_this_repo: true,
                adopt: Some(dir.path().join("widget")),
                ..plain()
            },
        );

        assert_eq!(outcome.code, 1);
        assert!(
            matches!(outcome.said.first(), Some(Say::Err(why)) if why.contains("Ask for one.")),
            "{:?}",
            outcome.said
        );
        assert!(!root.join(crate::plane::MANIFEST).exists());
        assert!(!root.join("personas").exists());
    }

    /// `--adopt` pointed at a directory that is not a repository: the plane is still made,
    /// exactly as Python makes it before a first clone that fails.
    #[test]
    fn adopt_refuses_a_directory_that_is_not_a_repository_and_still_makes_the_plane() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = dir.path().join("plane");
        let not_a_repo = dir.path().join("papers");
        for at in [&root, &not_a_repo] {
            std::fs::create_dir_all(at).expect("a directory");
        }
        let place = Place {
            root: root.clone(),
            is_plane: false,
        };

        let outcome = init(
            &place,
            &InitArgs {
                adopt: Some(not_a_repo),
                ..plain()
            },
        );

        assert_eq!(outcome.code, 1);
        assert!(
            outcome.said.iter().any(|line| matches!(
                line,
                Say::Err(why) if why.contains("is not the top level of a git working tree")
            )),
            "{:?}",
            outcome.said
        );
        assert!(root.join(crate::plane::MANIFEST).is_file());
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

    // ---------------------------------------------------------------------------------------
    // What `init` and `reinit` SAY, line by line, and what they leave on disk.
    //
    // `Say::Info/Ok/Warn/Err` are `util.info/ok/warn/err` in `charter/util.py` (`• `, `✓ `,
    // `! `, `✗ `); every expected line below is the f-string `commands.cmd_init` or
    // `cmd_reinit` prints for the same scenario, minus the harness-install lines this port
    // documents it does not produce (the module docs: "What `init` does not do here").
    // ---------------------------------------------------------------------------------------

    /// A fresh directory to point `init` at, and the tempdir that keeps it alive.
    fn empty_plane() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a directory");
        let root = dir.path().join("plane");
        std::fs::create_dir_all(&root).expect("the plane's directory");
        (dir, root)
    }

    fn at(root: &Path, is_plane: bool) -> Place {
        Place {
            root: root.to_path_buf(),
            is_plane,
        }
    }

    /// Whether this machine's Claude Code already runs the guard through charter's plugin —
    /// in which case `_ensure_guard_hook` answers "present" and writes nothing
    /// (`commands._plugin_dispatches_guard`). The operator's own `$HOME` decides it, so the
    /// expected lines are built from the same answer rather than assuming CI's.
    fn plugin_guards(root: &Path) -> bool {
        settings::plugin_dispatches_guard(root, crate::profiles::home().as_deref()).is_some()
    }

    /// The guard hook's label, in the list `init`/`reinit` put it in.
    fn guard_label(root: &Path) -> (&'static str, bool) {
        if plugin_guards(root) {
            (
                ".claude/settings.json (plane-root guard already wired)",
                false,
            )
        } else {
            (".claude/settings.json (plane-root guard)", true)
        }
    }

    fn errs(outcome: &Outcome) -> Vec<&str> {
        outcome
            .said
            .iter()
            .filter_map(|s| match s {
                Say::Err(e) => Some(e.as_str()),
                _ => None,
            })
            .collect()
    }

    fn blocker(name: &str, path: &Path, command: &str) -> Say {
        Say::Err(format!(
            "{name}/ can't be created — {} already exists and is not a directory. charter \
             never deletes or renames existing content; move or remove it yourself, then \
             re-run `charter {command}`.",
            path.display()
        ))
    }

    fn escape(rel: &str, lands: &Path, command: &str) -> Say {
        Say::Err(format!(
            "{rel} resolves to {}, which is outside this plane or inside its .git — charter \
             reads and writes nothing through it. Point it inside the plane or remove it \
             yourself, then re-run `charter {command}`.",
            lands.display()
        ))
    }

    const NEXT: &str =
        "Next: `charter doctor` to preflight, then `charter discover` to build the inventory.";

    /// A fresh `init` says what `cmd_init` says after it created everything: the `util.ok`
    /// headline with the folded COUNT, one `util.info("  + item")` per folded entry, and the
    /// `Next:` line — and no "already present" line, because nothing was.
    #[test]
    fn init_into_an_empty_directory_says_what_it_wrote_line_for_line() {
        let (_dir, root) = empty_plane();
        let (guard, guard_created) = guard_label(&root);

        let outcome = init(&at(&root, false), &plain());

        let settings_item = if guard_created {
            ".claude/settings.json (env, ask: charter handoff, plane-root guard)"
        } else {
            ".claude/settings.json (env, ask: charter handoff)"
        };
        let mut said = vec![
            Say::Ok("Initialized control plane (schema 1) — 8 item(s) written.".to_owned()),
            Say::Info("  + charter.toml".to_owned()),
            Say::Info("  + personas/".to_owned()),
            Say::Info("  + inventory/".to_owned()),
            Say::Info("  + workspaces/".to_owned()),
            Say::Info("  + .gitignore".to_owned()),
            Say::Info(format!("  + {settings_item}")),
            Say::Info("  + opencode.json (ask: charter handoff)".to_owned()),
            Say::Info("  + personas/steward/ (front door, declared in charter.toml)".to_owned()),
        ];
        if !guard_created {
            said.push(Say::Info(format!("  already present: {guard}")));
        }
        said.push(Say::Info(NEXT.to_owned()));
        assert_eq!(outcome.said, said);
        assert_eq!(outcome.code, 0);
        for dir in BASELINE_DIRS {
            assert!(root.join(dir).is_dir(), "{dir}");
        }
        assert_eq!(
            std::fs::read_to_string(root.join(".gitignore")).expect("a .gitignore"),
            GITIGNORE_BASELINE
        );
        let written = std::fs::read_to_string(root.join(settings::SETTINGS)).expect("settings");
        assert!(written.contains("CHARTER_HARNESS"), "{written}");
    }

    /// `init` again changes nothing and says so in `cmd_init`'s words: "already fully set
    /// up", then everything it found, in the order it looked.
    #[test]
    fn init_twice_says_the_plane_is_already_set_up_and_what_it_found() {
        let (_dir, root) = empty_plane();
        init(&at(&root, false), &plain());
        let gitignore = std::fs::read(root.join(".gitignore")).expect("a .gitignore");

        let outcome = init(&at(&root, true), &plain());

        assert_eq!(
            outcome.said,
            vec![
                Say::Ok(
                    "Control plane already fully set up (schema 1) — nothing to do.".to_owned()
                ),
                Say::Info(
                    "  already present: charter.toml, personas/, inventory/, workspaces/, \
                     .gitignore, .claude/settings.json (env), .claude/settings.json (ask: \
                     charter handoff), opencode.json (ask: charter handoff), \
                     .claude/settings.json (plane-root guard already wired)"
                        .to_owned()
                ),
                Say::Info(NEXT.to_owned()),
            ]
        );
        assert_eq!(outcome.code, 0);
        assert_eq!(
            std::fs::read(root.join(".gitignore")).expect("a .gitignore"),
            gitignore
        );
    }

    /// `cmd_init`'s `util.warn` when `--owner` is empty, said right after `charter.toml` is
    /// written and before the headline.
    #[test]
    fn init_without_an_owner_warns_that_the_forge_block_has_none() {
        let (_dir, root) = empty_plane();

        let outcome = init(
            &at(&root, false),
            &InitArgs {
                owner: String::new(),
                ..plain()
            },
        );

        assert_eq!(
            outcome.said.first(),
            Some(&Say::Warn(
                "No --owner given — charter.toml's [[forge]] block has no owner/group set. Add \
                 one before `charter discover`."
                    .to_owned()
            ))
        );
        assert_eq!(outcome.code, 0);
    }

    /// `cmd_init`'s first refusal: `unknown --forge 'x' — known kinds: github, gitlab`
    /// (`sorted(_registry.KINDS)`), before anything is written.
    #[test]
    fn init_with_an_unknown_forge_names_the_kinds_it_knows_and_writes_nothing() {
        let (_dir, root) = empty_plane();

        let outcome = init(
            &at(&root, false),
            &InitArgs {
                forge: "bitbucket".to_owned(),
                ..plain()
            },
        );

        assert_eq!(
            outcome.said,
            vec![Say::Err(
                "unknown --forge 'bitbucket' — known kinds: github, gitlab".to_owned()
            )]
        );
        assert_eq!(outcome.code, 1);
        assert!(!root.join(crate::plane::MANIFEST).exists());
    }

    /// `_create_baseline_dirs`' FINDING C1 and `cmd_init`'s blocked branch: a FILE where
    /// `inventory/` goes is named with `util.err`, left exactly as it was, everything else is
    /// still made, and the run ends with `  created:` and `  already present:` and exit 1.
    #[test]
    fn init_names_a_file_where_a_baseline_directory_goes_and_still_makes_the_rest() {
        let (_dir, root) = empty_plane();
        std::fs::write(root.join("inventory"), "mine\n").expect("a file in the way");
        std::fs::create_dir(root.join("workspaces")).expect("a workspaces/ of its own");
        let (guard, guard_created) = guard_label(&root);

        let outcome = init(&at(&root, false), &plain());

        let mut created = "charter.toml, personas/, .gitignore, .claude/settings.json (env), \
                           .claude/settings.json (ask: charter handoff), opencode.json (ask: \
                           charter handoff), personas/steward/ (front door, declared in \
                           charter.toml)"
            .to_owned();
        let mut present = "workspaces/".to_owned();
        if guard_created {
            created.push_str(&format!(", {guard}"));
        } else {
            present.push_str(&format!(", {guard}"));
        }
        assert_eq!(
            outcome.said,
            vec![
                blocker("inventory", &root.join("inventory"), "init"),
                Say::Info(format!("  created: {created}")),
                Say::Info(format!("  already present: {present}")),
            ]
        );
        assert_eq!(outcome.code, 1);
        assert_eq!(
            std::fs::read_to_string(root.join("inventory")).expect("the file"),
            "mine\n"
        );
        assert!(root.join("personas/steward/persona.md").is_file());
    }

    /// Stricter than Python, and the module docs say so: a `.gitignore` that is a link OUT of
    /// the plane is a blocker, named once with where it lands, and nothing is read or
    /// written through it. The rest of the plane is still made.
    #[test]
    fn a_gitignore_that_links_out_of_the_plane_is_a_blocker_and_is_never_written_through() {
        let (dir, root) = empty_plane();
        let outside = dir.path().join("outside");
        std::fs::create_dir(&outside).expect("somewhere else");
        std::fs::write(outside.join("bashrc"), "export X=1\n").expect("a file out there");
        std::os::unix::fs::symlink(outside.join("bashrc"), root.join(".gitignore"))
            .expect("a link out");
        let lands = outside.join("bashrc").canonicalize().expect("resolved");

        let outcome = init(&at(&root, false), &plain());

        assert_eq!(
            errs(&outcome),
            vec![match escape(".gitignore", &lands, "init") {
                Say::Err(e) => e,
                _ => unreachable!(),
            }]
        );
        assert_eq!(outcome.code, 1);
        assert_eq!(
            std::fs::read_to_string(outside.join("bashrc")).expect("the file"),
            "export X=1\n"
        );
        assert!(root.join(crate::plane::MANIFEST).is_file());
        assert!(
            outcome
                .said
                .iter()
                .any(|s| matches!(s, Say::Info(l) if l.starts_with("  created: charter.toml, "))),
            "{:?}",
            outcome.said
        );
    }

    /// A `personas` link out of the plane is asked about twice — by the baseline and by the
    /// front door — and named ONCE. Nothing is scaffolded through it.
    #[test]
    fn a_personas_link_out_of_the_plane_is_named_once_though_two_steps_ask_about_it() {
        let (dir, root) = empty_plane();
        let outside = dir.path().join("elsewhere");
        std::fs::create_dir(&outside).expect("somewhere else");
        std::os::unix::fs::symlink(&outside, root.join("personas")).expect("a link out");
        let lands = outside.canonicalize().expect("resolved");

        let outcome = init(&at(&root, false), &plain());

        let named: Vec<&Say> = outcome
            .said
            .iter()
            .filter(|s| matches!(s, Say::Err(e) if e.starts_with("personas resolves to")))
            .collect();
        assert_eq!(named, vec![&escape("personas", &lands, "init")]);
        assert_eq!(outcome.code, 1);
        assert_eq!(
            std::fs::read_dir(&outside).expect("the directory").count(),
            0,
            "nothing is written through the link"
        );
    }

    /// A link that lands inside the plane's own `.git` is refused like one out of the plane:
    /// nothing charter writes belongs in a repository's internals (module docs).
    #[test]
    fn a_gitignore_that_links_into_the_planes_own_git_is_refused_too() {
        let (_dir, root) = empty_plane();
        std::fs::create_dir_all(root.join(".git/info")).expect("a .git");
        std::fs::write(root.join(".git/info/exclude"), "# git's own\n").expect("exclude");
        std::os::unix::fs::symlink(".git/info/exclude", root.join(".gitignore"))
            .expect("a link into .git");
        let lands = root
            .join(".git/info/exclude")
            .canonicalize()
            .expect("resolved");

        let outcome = init(
            &at(&root, false),
            &InitArgs {
                plane_is_this_repo: true,
                ..plain()
            },
        );

        assert!(
            outcome.said.contains(&escape(".gitignore", &lands, "init")),
            "{:?}",
            outcome.said
        );
        assert_eq!(outcome.code, 1);
        assert_eq!(
            std::fs::read_to_string(root.join(".git/info/exclude")).expect("exclude"),
            "# git's own\n"
        );
    }

    /// A `.claude` FILE is one blocker, named once — the settings writers behind the gate are
    /// never asked, so they cannot name it a second time. Python crashes writing under it.
    #[test]
    fn a_claude_file_where_the_settings_directory_goes_is_one_blocker_named_once() {
        let (_dir, root) = empty_plane();
        std::fs::write(root.join(".claude"), "not a directory\n").expect("a file in the way");

        let outcome = init(&at(&root, false), &plain());

        assert_eq!(
            errs(&outcome),
            vec![match blocker(".claude", &root.join(".claude"), "init") {
                Say::Err(e) => e,
                _ => unreachable!(),
            }]
        );
        assert_eq!(outcome.code, 1);
        assert_eq!(
            std::fs::read_to_string(root.join(".claude")).expect("the file"),
            "not a directory\n"
        );
    }

    /// A `.claude` DIRECTORY the operator already has is where the settings go, not a
    /// blocker.
    #[test]
    fn a_claude_directory_of_its_own_takes_the_settings_file() {
        let (_dir, root) = empty_plane();
        std::fs::create_dir(root.join(".claude")).expect("a .claude of its own");

        let outcome = init(&at(&root, false), &plain());

        assert_eq!(outcome.code, 0, "{:?}", outcome.said);
        assert!(root.join(settings::SETTINGS).is_file());
    }

    /// A `.claude` link out of the plane: named, and no settings file appears at the far end.
    #[test]
    fn a_claude_link_out_of_the_plane_gets_no_settings_written_through_it() {
        let (dir, root) = empty_plane();
        let outside = dir.path().join("dotclaude");
        std::fs::create_dir(&outside).expect("somewhere else");
        std::os::unix::fs::symlink(&outside, root.join(".claude")).expect("a link out");
        let lands = outside.canonicalize().expect("resolved");

        let outcome = init(&at(&root, false), &plain());

        assert!(
            outcome.said.contains(&escape(".claude", &lands, "init")),
            "{:?}",
            outcome.said
        );
        assert_eq!(outcome.code, 1);
        assert_eq!(
            std::fs::read_dir(&outside).expect("the directory").count(),
            0
        );
    }

    /// `cmd_init`'s malformed branch: `util.warn(f"{_settings_left_untouched(p)}\n
    /// {_hooks_snippet()}")`, the file left byte for byte, and exit 1 — the ONLY thing wrong,
    /// so nothing else can be what fails the run.
    #[test]
    fn a_settings_file_that_is_not_json_is_left_untouched_and_init_exits_1() {
        let (_dir, root) = empty_plane();
        std::fs::create_dir(root.join(".claude")).expect(".claude");
        std::fs::write(root.join(settings::SETTINGS), "{not json\n").expect("settings");
        if plugin_guards(&root) {
            // `_ensure_guard_hook` answers "present" before it reads the file, as Python's
            // does; the branch under test is not reachable on this machine.
            return;
        }

        let outcome = init(&at(&root, false), &plain());

        let path = root.join(settings::SETTINGS);
        let warned = Say::Warn(format!(
            "{} is not a settings file charter can read and write back as JSON — left it \
             completely untouched. Wire the plane-root guard yourself:\n{}",
            path.display(),
            settings::hooks_snippet()
        ));
        assert!(outcome.said.contains(&warned), "{:?}", outcome.said);
        assert_eq!(errs(&outcome), Vec::<&str>::new());
        assert_eq!(outcome.code, 1);
        assert_eq!(
            std::fs::read_to_string(&path).expect("settings"),
            "{not json\n"
        );
    }

    #[test]
    fn an_opencode_file_that_cannot_take_the_handoff_rule_keeps_it_out_of_every_harness() {
        // `commands._guard_apply`: every harness is asked first, and "only `malformed`
        // blocks" — Claude Code is first in the registry, so without the dry run its file
        // would already hold the rule when opencode refused. Nothing is written anywhere.
        let (_dir, root) = empty_plane();
        let opencode = root.join(settings::OPENCODE);
        std::fs::write(&opencode, "{\"permission\": \"ask\"}\n").expect("opencode.json");

        let outcome = init(&at(&root, false), &plain());

        let warned = Say::Warn(format!(
            "the ask rule for `charter handoff` was not written anywhere — {} (`permission` is \
             not an object) is not valid, and `charter guard` writes every harness or none. Fix \
             it, then: charter guard ask 'charter handoff *'",
            opencode.display()
        ));
        assert!(outcome.said.contains(&warned), "{:?}", outcome.said);
        let claude = std::fs::read_to_string(root.join(settings::SETTINGS)).unwrap_or_default();
        assert!(
            !claude.contains(settings::HANDOFF_RULE),
            "Claude Code took the rule opencode could not: {claude}"
        );
        assert_eq!(
            std::fs::read_to_string(&opencode).expect("opencode.json"),
            "{\"permission\": \"ask\"}\n"
        );
    }

    /// Makes `path` unwritable for the test and back again after, so the tempdir can go.
    /// `None` when the process can write it anyway (root), where the scenario cannot exist.
    struct ReadOnly(PathBuf);
    impl ReadOnly {
        fn make(path: &Path) -> Option<Self> {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o555)).expect("chmod");
            let guard = ReadOnly(path.to_path_buf());
            let probe = path.join(".probe");
            if std::fs::write(&probe, "").is_ok() {
                let _ = std::fs::remove_file(probe);
                return None;
            }
            Some(guard)
        }
    }
    impl Drop for ReadOnly {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }

    /// A write the OS refuses is said with the OS's own words — Python's `strerror`, not
    /// Rust's `(os error 13)` — and fails the run. Python raises a traceback here; the Rust
    /// contract is `_say_write_failed`'s shape ("could not write <path> (<strerror>) — left
    /// untouched.") for every such write.
    #[test]
    fn a_plane_charter_cannot_write_into_says_what_the_os_said_and_exits_1() {
        let (_dir, root) = empty_plane();
        let Some(_ro) = ReadOnly::make(&root) else {
            return;
        };

        let outcome = init(&at(&root, false), &plain());

        let said = errs(&outcome);
        let toml = format!(
            "could not write {} (Permission denied)",
            root.join("charter.toml").display()
        );
        let personas = format!(
            "could not write {} (Permission denied) — left untouched.",
            root.join("personas").display()
        );
        let gitignore = format!(
            "could not read or write {} (Permission denied)",
            root.join(".gitignore").display()
        );
        for line in [&toml, &personas, &gitignore] {
            assert!(said.contains(&line.as_str()), "{line}\n{said:#?}");
        }
        assert!(said.iter().all(|l| !l.contains("os error")), "{said:#?}");
        assert_eq!(outcome.code, 1);
    }

    /// `strerror` is the text before Rust's ` (os error N)`, and a message with no such tail
    /// is kept whole.
    #[test]
    fn strerror_is_the_oss_words_without_rusts_error_number() {
        assert_eq!(
            strerror(&std::io::Error::from_raw_os_error(2)),
            "No such file or directory"
        );
        assert_eq!(
            strerror(&std::io::Error::other("it is not UTF-8 text")),
            "it is not UTF-8 text"
        );
    }

    /// `cli._plane_refusal`: a `charter.toml` from a newer charter stops `init` AND `reinit`
    /// before anything is written, with the one line `cli.main` prints.
    #[test]
    fn a_plane_from_a_newer_charter_is_refused_by_init_and_reinit_alike() {
        let (_dir, root) = empty_plane();
        std::fs::write(root.join("charter.toml"), "schema = 2\n").expect("charter.toml");
        let refusal = vec![Say::Err(format!(
            "{} declares schema 2, but this charter understands 1. Upgrade charter: `uv tool \
             install charter-cp --force --refresh`. Nothing was run. `charter doctor` reports \
             it; `charter update` is the way out.",
            root.join("charter.toml").display()
        ))];

        let first = init(&at(&root, true), &plain());
        let second = reinit(&at(&root, true));

        assert_eq!((first.said, first.code), (refusal.clone(), 1));
        assert_eq!((second.said, second.code), (refusal, 1));
        assert!(!root.join("personas").exists());
        assert!(!root.join(".gitignore").exists());
    }

    /// `cmd_reinit` outside a plane: its one `util.err`, and nothing scaffolded into whatever
    /// directory happened to be the cwd.
    #[test]
    fn reinit_outside_a_plane_refuses_and_writes_nothing() {
        let (_dir, root) = empty_plane();

        let outcome = reinit(&at(&root, false));

        assert_eq!(
            outcome.said,
            vec![Say::Err(
                "no control plane found (no charter.toml here or in any parent) — `charter \
                 reinit` only works inside one."
                    .to_owned()
            )]
        );
        assert_eq!(outcome.code, 1);
        assert!(!root.join("personas").exists());
    }

    /// `cmd_reinit` on a plane `init` just made: `Up to date (schema 1) — nothing to do.`,
    /// and nothing else.
    #[test]
    fn reinit_on_a_current_plane_is_up_to_date() {
        let (_dir, root) = empty_plane();
        init(&at(&root, false), &plain());

        let outcome = reinit(&at(&root, true));

        assert_eq!(
            outcome.said,
            vec![Say::Ok("Up to date (schema 1) — nothing to do.".to_owned())]
        );
        assert_eq!(outcome.code, 0);
    }

    /// `cmd_reinit` healing a plane that predates a directory and the profiles ignore line:
    /// `Reinitialized control plane → added …`, then what it found, and the line appended
    /// under `_ensure_local_profiles_ignored`'s header with the rest of the file kept.
    #[test]
    fn reinit_heals_what_a_plane_predates_and_names_each_thing_it_added() {
        let (_dir, root) = empty_plane();
        init(&at(&root, false), &plain());
        std::fs::remove_dir(root.join("inventory")).expect("an old plane");
        let old = GITIGNORE_BASELINE.replace("/charter.local.toml\n", "");
        std::fs::write(root.join(".gitignore"), &old).expect("an old .gitignore");

        let outcome = reinit(&at(&root, true));

        assert_eq!(
            outcome.said,
            vec![
                Say::Ok(
                    "Reinitialized control plane → added inventory/, .gitignore \
                     (/charter.local.toml)."
                        .to_owned()
                ),
                Say::Info(
                    "  already present: personas/, workspaces/, .claude/settings.json \
                     (plane-root guard already wired)"
                        .to_owned()
                ),
            ]
        );
        assert_eq!(outcome.code, 0);
        assert_eq!(
            std::fs::read_to_string(root.join(".gitignore")).expect("a .gitignore"),
            format!(
                "{}\n\n# added by `charter reinit` — harness profiles stay on this machine\n\
                 /charter.local.toml\n",
                old.trim_end_matches('\n')
            )
        );
    }

    /// `cmd_reinit` on a bare `charter.toml`: everything is added, and when nothing was found
    /// there is no "already present" line at all.
    #[test]
    fn reinit_on_a_bare_manifest_adds_everything_and_lists_nothing_as_present() {
        let (_dir, root) = empty_plane();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").expect("charter.toml");
        let (guard, guard_created) = guard_label(&root);

        let outcome = reinit(&at(&root, true));

        let mut added = "personas/, inventory/, workspaces/, .gitignore (/charter.local.toml), \
                         .claude/settings.json (env)"
            .to_owned();
        let mut said = Vec::new();
        if guard_created {
            added.push_str(&format!(", {guard}"));
        }
        said.push(Say::Ok(format!(
            "Reinitialized control plane → added {added}."
        )));
        if !guard_created {
            said.push(Say::Info(format!("  already present: {guard}")));
        }
        assert_eq!(outcome.said, said);
        assert_eq!(outcome.code, 0);
    }

    /// `cmd_reinit`'s blocked branch: the blocker in `reinit`'s words, then `  created:` and
    /// `  already present:`, exit 1.
    #[test]
    fn reinit_names_a_file_where_a_baseline_directory_goes_and_still_heals_the_rest() {
        let (_dir, root) = empty_plane();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").expect("charter.toml");
        std::fs::create_dir(root.join("personas")).expect("personas/");
        std::fs::write(root.join("inventory"), "mine\n").expect("a file in the way");
        let (guard, guard_created) = guard_label(&root);

        let outcome = reinit(&at(&root, true));

        let mut created =
            "workspaces/, .gitignore (/charter.local.toml), .claude/settings.json (env)".to_owned();
        let mut present = "personas/".to_owned();
        if guard_created {
            created.push_str(&format!(", {guard}"));
        } else {
            present.push_str(&format!(", {guard}"));
        }
        assert_eq!(
            outcome.said,
            vec![
                blocker("inventory", &root.join("inventory"), "reinit"),
                Say::Info(format!("  created: {created}")),
                Say::Info(format!("  already present: {present}")),
            ]
        );
        assert_eq!(outcome.code, 1);
    }

    /// `reinit` refuses a `.gitignore` that links out of the plane exactly as `init` does,
    /// in its own command's words.
    #[test]
    fn reinit_refuses_a_gitignore_that_links_out_of_the_plane() {
        let (dir, root) = empty_plane();
        init(&at(&root, false), &plain());
        std::fs::remove_file(root.join(".gitignore")).expect("the plane's own");
        let outside = dir.path().join("elsewhere.txt");
        std::fs::write(&outside, "keep\n").expect("a file out there");
        std::os::unix::fs::symlink(&outside, root.join(".gitignore")).expect("a link out");
        let lands = outside.canonicalize().expect("resolved");

        let outcome = reinit(&at(&root, true));

        assert_eq!(
            outcome.said.first(),
            Some(&escape(".gitignore", &lands, "reinit"))
        );
        assert_eq!(outcome.code, 1);
        assert_eq!(
            std::fs::read_to_string(&outside).expect("the file"),
            "keep\n"
        );
    }

    /// A settings directory the OS will not let charter write into: each settings write that
    /// was attempted is said, and `reinit` exits 1 rather than reporting the plane healed.
    #[test]
    fn reinit_says_each_settings_write_the_os_refused_and_exits_1() {
        let (_dir, root) = empty_plane();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").expect("charter.toml");
        std::fs::create_dir(root.join(".claude")).expect(".claude");
        let plugin = plugin_guards(&root);
        let Some(_ro) = ReadOnly::make(&root.join(".claude")) else {
            return;
        };

        let outcome = reinit(&at(&root, true));

        let line = format!(
            "could not write {} (Permission denied) — left untouched.",
            root.join(settings::SETTINGS).display()
        );
        // `ensure_env`, and `ensure_guard_hook` unless the plugin already runs the guard.
        let expected = if plugin { 1 } else { 2 };
        assert_eq!(
            errs(&outcome).iter().filter(|l| **l == line).count(),
            expected,
            "{:?}",
            outcome.said
        );
        assert_eq!(outcome.code, 1);
    }

    /// `commands._ensure_gitignore` against a file that has every rule — `.charter/` found
    /// by Python's substring test — writes nothing and answers `False`.
    #[test]
    fn a_gitignore_with_every_rule_is_left_byte_for_byte() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join(".gitignore");
        let body = "build/\n!/workspaces/.gitkeep\nkeep/.charter/out\n\
                    /.claude/settings.local.json\n/charter.local.toml\n";
        std::fs::write(&path, body).expect("a .gitignore");

        assert_eq!(ensure_gitignore(&path), Ok(false));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), body);
    }

    /// `commands._ensure_gitignore` appends exactly the lines it is missing, in its order,
    /// under one `# added by \`charter init\`` header (`util.append_gitignore`), trailing
    /// blank lines collapsed to the one before the block.
    #[test]
    fn a_gitignore_missing_the_local_files_gets_only_those_appended() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join(".gitignore");
        std::fs::write(
            &path,
            "node_modules/\n!/workspaces/.gitkeep\n/.charter/\n\n\n",
        )
        .expect("a .gitignore");

        assert_eq!(ensure_gitignore(&path), Ok(true));
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "node_modules/\n!/workspaces/.gitkeep\n/.charter/\n\n# added by `charter \
             init`\n/.claude/settings.local.json\n/charter.local.toml\n"
        );
    }

    /// The other half: the workspace anchor missing brings BOTH workspace lines, and a
    /// `.charter/` anywhere in the body is enough to skip `/.charter/`.
    #[test]
    fn a_gitignore_missing_the_workspace_anchor_gets_both_workspace_lines() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join(".gitignore");
        std::fs::write(
            &path,
            "# no .charter/ here, please\n/.claude/settings.local.json\n/charter.local.toml\n",
        )
        .expect("a .gitignore");

        assert_eq!(ensure_gitignore(&path), Ok(true));
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            "# no .charter/ here, please\n/.claude/settings.local.json\n/charter.local.toml\n\
             \n# added by `charter init`\n/workspaces/*/*\n!/workspaces/.gitkeep\n"
        );
    }

    /// No `.gitignore` at all: the whole baseline, and `True`.
    #[test]
    fn an_absent_gitignore_is_written_as_the_whole_baseline() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join(".gitignore");

        assert_eq!(ensure_gitignore(&path), Ok(true));
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            GITIGNORE_BASELINE
        );
    }

    /// A plane with one declared profile of `kind` whose command is nowhere on this machine,
    /// approved or not — so nothing can run it, and anything that tried would say so.
    fn plane_with_a_profile(
        kind: &str,
        approved: bool,
    ) -> (tempfile::TempDir, PathBuf, crate::profiles::Profile) {
        let (dir, root) = empty_plane();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").expect("charter.toml");
        let missing = dir.path().join("nowhere").join(kind);
        std::fs::write(
            root.join("charter.local.toml"),
            format!(
                "[harness.work]\nkind = {kind:?}\ncommand = [{:?}]\n",
                missing.display().to_string()
            ),
        )
        .expect("charter.local.toml");
        let profile = crate::profiles::current(&root)
            .get("work")
            .cloned()
            .expect("the profile is declared");
        if approved {
            crate::profiletrust::record_launched(
                &root,
                "work",
                &crate::profiletrust::fingerprint(&profile),
            )
            .expect("approved");
        }
        (dir, root, profile)
    }

    fn line_about_work(outcome: &Outcome) -> Vec<&Say> {
        outcome
            .said
            .iter()
            .filter(
                |s| matches!(s, Say::Info(l) | Say::Warn(l) if l.starts_with("  profile 'work'")),
            )
            .collect()
    }

    /// `init` installs nothing into a profile's config folder any more — the app arms each
    /// chat with its own plugin (`crate::plugin`) — so an approved profile gets no line at
    /// all, whatever its kind, and nothing is run to find out.
    #[test]
    fn init_and_reinit_say_nothing_about_an_approved_profile_of_any_kind() {
        for kind in ["claude", "codex", "opencode"] {
            let (_dir, root, _profile) = plane_with_a_profile(kind, true);
            assert!(
                line_about_work(&init(&at(&root, true), &plain())).is_empty(),
                "init spoke about an approved {kind} profile"
            );
            assert!(
                line_about_work(&reinit(&at(&root, true))).is_empty(),
                "reinit spoke about an approved {kind} profile"
            );
        }
    }

    /// What still stops a chat is a command nobody approved, and that IS said — as a warning,
    /// with the command that approves it.
    #[test]
    fn a_profile_nobody_approved_is_named_with_the_way_to_approve_it() {
        let (_dir, root, _profile) = plane_with_a_profile("claude", false);

        let outcome = reinit(&at(&root, true));

        assert_eq!(
            line_about_work(&outcome),
            vec![&Say::Warn(
                "  profile 'work' is new and not approved yet — run charter work once to \
                 approve its command"
                    .to_owned()
            )]
        );
    }

    // ---------------------------------------------------------------------------------------
    // The front door (`commands._ensure_front_door`).
    // ---------------------------------------------------------------------------------------

    /// The persona file is `commands._FRONT_DOOR.format(name=..., role=...)` byte for byte.
    /// The fixture is that call's output from the Python charter, for `front-door.2`, whose
    /// role `str.title()` makes `Front Door.2`.
    #[test]
    fn the_front_door_is_the_pythons_template_byte_for_byte_and_is_declared() {
        let (_dir, root) = empty_plane();

        let outcome = init(
            &at(&root, false),
            &InitArgs {
                front_door: Some("front-door.2".to_owned()),
                ..plain()
            },
        );

        assert_eq!(outcome.code, 0, "{:?}", outcome.said);
        let dir = root.join("personas/front-door.2");
        assert_eq!(
            std::fs::read_to_string(dir.join("persona.md")).expect("persona.md"),
            include_str!("testdata/front-door-as-python-renders-it.md")
        );
        assert!(dir.join("memory/.gitkeep").is_file());
        assert!(dir.join("refs/.gitkeep").is_file());
        let toml = std::fs::read_to_string(root.join("charter.toml")).expect("charter.toml");
        assert!(toml.contains("default = \"front-door.2\""), "{toml}");
    }

    fn scaffolded_a_front_door(outcome: &Outcome) -> bool {
        outcome.said.iter().any(|s| {
            matches!(s, Say::Info(l) if l == "  + personas/steward/ (front door, declared in charter.toml)")
        })
    }

    /// `personas.glob("*.md")` counts only markdown: a README of another kind is not a
    /// roster, so the front door is still made.
    #[test]
    fn a_personas_directory_holding_only_a_non_markdown_file_still_gets_a_front_door() {
        let (_dir, root) = empty_plane();
        std::fs::create_dir(root.join("personas")).expect("personas/");
        std::fs::write(root.join("personas/README.txt"), "notes\n").expect("a note");

        let outcome = init(&at(&root, false), &plain());

        assert!(scaffolded_a_front_door(&outcome), "{:?}", outcome.said);
        assert!(root.join("personas/steward/persona.md").is_file());
    }

    /// `personas.glob("*/persona.md")`: a persona of its own is a roster, and none is added.
    #[test]
    fn a_plane_with_a_persona_of_its_own_gets_no_front_door() {
        let (_dir, root) = empty_plane();
        std::fs::create_dir_all(root.join("personas/alice")).expect("alice/");
        std::fs::write(
            root.join("personas/alice/persona.md"),
            "---\nname: alice\n---\n",
        )
        .expect("alice");

        let outcome = init(&at(&root, false), &plain());

        assert_eq!(outcome.code, 0, "{:?}", outcome.said);
        assert!(!scaffolded_a_front_door(&outcome), "{:?}", outcome.said);
        assert!(!root.join("personas/steward").exists());
    }

    /// `_instance.default_persona_of(...)`: a declared default is a question somebody already
    /// answered.
    #[test]
    fn a_plane_that_declares_a_default_persona_gets_no_front_door() {
        let (_dir, root) = empty_plane();
        std::fs::write(
            root.join("charter.toml"),
            "schema = 1\n\n[persona]\ndefault = \"alice\"\n",
        )
        .expect("charter.toml");

        let outcome = init(&at(&root, true), &plain());

        assert_eq!(outcome.code, 0, "{:?}", outcome.said);
        assert!(!root.join("personas/steward").exists());
    }

    /// Stricter than Python, as `front_door` documents: a `charter.toml` that is not TOML
    /// makes Python's `instance.load` raise and `init` end in a traceback. Here it is said,
    /// no front door is scaffolded over it, and the run fails.
    #[test]
    fn a_manifest_that_is_not_toml_gets_no_front_door_and_fails_the_run() {
        let (_dir, root) = empty_plane();
        std::fs::write(root.join("charter.toml"), "this is = not [toml\n").expect("toml");

        let outcome = init(&at(&root, true), &plain());

        assert!(
            errs(&outcome)
                .iter()
                .any(|e| e.ends_with(" — no front door was scaffolded.")),
            "{:?}",
            outcome.said
        );
        assert_eq!(outcome.code, 1);
        assert!(!root.join("personas/steward").exists());
    }

    /// Every path the front door would write is gated, not only its directory: a
    /// `memory` link out of the plane stops the whole scaffold, and nothing lands out there.
    #[test]
    fn a_front_door_whose_memory_links_out_of_the_plane_is_not_scaffolded_through_it() {
        let (dir, root) = empty_plane();
        let outside = dir.path().join("elsewhere");
        std::fs::create_dir(&outside).expect("somewhere else");
        std::fs::create_dir_all(root.join("personas/steward")).expect("steward/");
        std::os::unix::fs::symlink(&outside, root.join("personas/steward/memory"))
            .expect("a link out");

        let outcome = init(&at(&root, false), &plain());

        assert_eq!(outcome.code, 1, "{:?}", outcome.said);
        assert!(
            errs(&outcome)
                .iter()
                .any(|e| e.starts_with("personas/steward/memory/.gitkeep resolves to")),
            "{:?}",
            outcome.said
        );
        assert!(!root.join("personas/steward/persona.md").exists());
        assert_eq!(
            std::fs::read_dir(&outside).expect("the directory").count(),
            0
        );
    }

    // ---------------------------------------------------------------------------------------
    // Inside a git repository: ADR 0035's refusal, and `_first_clone_step`'s offer.
    // ---------------------------------------------------------------------------------------

    fn git(dir: &Path, args: &[&str]) {
        let r = crate::worktree::git::run(dir, args, crate::worktree::git::READ).expect("git");
        assert!(r.ok(), "git {args:?} failed: {}", r.err);
    }

    /// A directory named `plane` that is the top of a git repo whose `origin` is `widget` —
    /// `_in_a_git_repository` in the differential run, which names the same repo.
    fn a_repo(origin: &str) -> (tempfile::TempDir, PathBuf) {
        let (dir, root) = empty_plane();
        git(&root, &["init", "-q", "-b", "main", "."]);
        git(&root, &["remote", "add", "origin", origin]);
        (dir, root)
    }

    /// ADR 0035 / spec decision 27, the declared divergence: `init` at the top of a repo
    /// writes nothing and says how to ask. The lines are `NOT_COLONISED` in
    /// `tests/differential/run.py`, which holds the CLI to them byte for byte.
    #[test]
    fn init_at_the_top_of_a_repository_writes_nothing_and_says_how_to_ask() {
        let (_dir, root) = a_repo("git@github.com:acme/widget.git");

        let outcome = init(&at(&root, false), &plain());

        assert_eq!(
            outcome.said,
            vec![
                Say::Err(
                    "this is the git repo 'widget', and `charter init` does not make a \
                     repository into a control plane unless you ask it to. Nothing was \
                     written."
                        .to_owned()
                ),
                Say::Info(
                    "A plane is a directory of its own, and this repo is the first clone in \
                     it:\n      mkdir ../widget-plane && cd ../widget-plane\n      charter \
                     init --forge github --owner acme --adopt ../plane\n  That clones this \
                     repo into the plane's first workspace. Nothing here is written by any of \
                     it — this repo is read, and only read."
                        .to_owned()
                ),
                Say::Info(
                    "To make THIS repo the plane instead — charter's own plane is one, which \
                     is why the option is here — ask for it by name:\n      charter init \
                     --plane-is-this-repo --forge github --owner acme\n  That writes \
                     charter.toml, .claude/settings.json, opencode.json, personas/, \
                     inventory/, workspaces/ into this repo, and charter's own rules into its \
                     tracked .gitignore."
                        .to_owned()
                ),
                Say::Info(
                    "Why the default changed: docs/adr/0035-a-plane-is-untrusted-until-the-\
                     operator-opens-it.md, and charter-app spec decision 27. `charter init` \
                     anywhere that is not the top of a git repo is unchanged."
                        .to_owned()
                ),
            ]
        );
        assert_eq!(outcome.code, 1);
        let left: Vec<_> = std::fs::read_dir(&root)
            .expect("the repo")
            .map(|e| e.expect("an entry").file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from(".git")]);
    }

    /// The flags the refusal prints back are the ones typed, as a shell reads them: the
    /// front-door flag only when it is not the default.
    #[test]
    fn the_refusal_prints_back_only_the_flags_that_were_typed() {
        assert_eq!(typed_flags(&plain()), " --forge github --owner acme");
        assert_eq!(
            typed_flags(&InitArgs {
                owner: String::new(),
                host: Some("git.example.com".to_owned()),
                front_door: None,
                ..plain()
            }),
            " --forge github --host git.example.com --no-front-door"
        );
        assert_eq!(
            typed_flags(&InitArgs {
                front_door: Some("door".to_owned()),
                ..plain()
            }),
            " --forge github --owner acme --front-door door"
        );
    }

    /// `--plane-is-this-repo` is the old default asked for by name: the plane is made in the
    /// repo, and `_first_clone_step` offers the first clone in `_offer_first_clone`'s words.
    #[test]
    fn plane_is_this_repo_scaffolds_the_repo_and_offers_its_first_clone() {
        let (_dir, root) = a_repo("git@github.com:acme/widget.git");

        let outcome = init(
            &at(&root, false),
            &InitArgs {
                plane_is_this_repo: true,
                ..plain()
            },
        );

        assert_eq!(outcome.code, 0, "{:?}", outcome.said);
        assert_eq!(
            outcome.said.last(),
            Some(&Say::Info(
                "You are standing in the git repo 'widget'. Work happens in a workspace, not \
                 in the plane root — clone it into the first one:\n      charter init \
                 --clone-this-repo\n  Nothing is cloned unless you run that. It lands in \
                 workspaces/default/widget/, and declining leaves this plane complete."
                    .to_owned()
            ))
        );
        assert!(root.join("charter.toml").is_file());
    }

    /// A repository that already holds a `charter.toml` is a plane: `init` heals it rather
    /// than refusing, and the offer names the workspace the plane declares
    /// (`config.DEFAULT_WORKSPACE`, re-derived by `config.use(root)`).
    #[test]
    fn a_repository_that_is_already_a_plane_is_healed_and_offered_its_declared_workspace() {
        let (_dir, root) = a_repo("git@github.com:acme/widget.git");
        std::fs::write(
            root.join("charter.toml"),
            "schema = 1\n\n[workspace]\ndefault = \"lab\"\n",
        )
        .expect("charter.toml");

        let outcome = init(&at(&root, true), &plain());

        assert_eq!(outcome.code, 0, "{:?}", outcome.said);
        assert!(
            matches!(outcome.said.last(), Some(Say::Info(l))
                if l.contains("It lands in workspaces/lab/widget/,")),
            "{:?}",
            outcome.said
        );
    }

    /// `_first_clone_step(accepted=True)` where there is no repo: `cmd_init`'s own words, and
    /// exit 1 — the plane was still made.
    #[test]
    fn clone_this_repo_where_there_is_no_repo_says_so_and_exits_1() {
        let (_dir, root) = empty_plane();

        let outcome = init(
            &at(&root, false),
            &InitArgs {
                clone_this_repo: true,
                ..plain()
            },
        );

        assert_eq!(
            outcome.said.last(),
            Some(&Say::Err(format!(
                "--clone-this-repo: there is no repo here to clone. {} is not the top level \
                 of a git working tree, and the flag clones the repo you are standing in. The \
                 control plane itself was still created.",
                root.display()
            )))
        );
        assert_eq!(outcome.code, 1);
        assert!(root.join("charter.toml").is_file());
    }

    /// `--adopt` (charter-app#175, no Python counterpart) naming the plane's own directory is
    /// `--clone-this-repo` spelled another way, and is refused with the pointer to it.
    #[test]
    fn adopt_naming_the_planes_own_directory_points_at_clone_this_repo() {
        let (_dir, root) = a_repo("git@github.com:acme/widget.git");

        let outcome = init(
            &at(&root, false),
            &InitArgs {
                plane_is_this_repo: true,
                adopt: Some(root.clone()),
                ..plain()
            },
        );

        assert_eq!(
            outcome.said.last(),
            Some(&Say::Err(
                "--adopt: that is this plane's own directory. To clone the repo the plane is \
                 being made IN, ask for `--clone-this-repo`; `--adopt` is for a repository \
                 somewhere else, with the plane beside it (ADR 0035)."
                    .to_owned()
            ))
        );
        assert_eq!(outcome.code, 1);
    }

    /// `commands._first_clone_name`: the tail of `origin` when it is a workspace-shaped name,
    /// else the directory's own name.
    #[test]
    fn the_first_clone_is_named_after_origin_unless_origin_ends_in_no_name() {
        let (_dir, root) = a_repo("git@github.com:acme/widget.git");
        assert_eq!(first_clone_name(&root), "widget");

        let (_dir, root) = a_repo("https://example.com/acme/Not A Name.git");
        assert_eq!(first_clone_name(&root), "plane");
    }
}
