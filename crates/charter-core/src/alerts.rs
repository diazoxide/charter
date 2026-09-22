//! charter's alert rows — a port of `charter/statusline.py`'s `_alerts` and the
//! `_plane_root_alert` it ends on.
//!
//! # What an alert is
//!
//! Not telemetry: an **actionable problem with the control plane**, carrying the command that
//! fixes it, and rendered only when it is real — so a healthy plane costs no rows. charter has
//! five, always in this order:
//!
//! 1. **the pin** — `[charter] version` beside `[update] channel = "dev"` (two different
//!    charters asked for at once), or a pin the running charter does not meet;
//! 2. **the front door** — `[persona] default` naming a persona that is not there;
//! 3. **reinit** — how many OTHER workspaces are behind the current layout (the active one is the
//!    identity row's to flag, so the two surfaces never both say it);
//! 4. **a nested plane** — `$CHARTER_ROOT` pinned this session inside a plane that another
//!    plane's `workspaces/` holds, so memory and vaults go to the inner one;
//! 5. **the plane root** — the root being worked in: tracked changes, a detached HEAD, a branch
//!    that is not its default, a memory commit that never reached `origin`.
//!
//! # Structured here, worded twice
//!
//! charter builds each row as an ANSI string where it decides it. This port keeps the decision
//! and the words apart: [`read`] answers with [`Alert`] values, [`Alert::line`] draws the row the
//! terminal status line prints — byte for byte what charter prints, which the
//! `statusline-alerts-*` differential scenarios compare — and [`Alert::shown`] gives the app the
//! same facts as plain words for its drawer. Two renderings of ONE decision, so the drawer and
//! the status line cannot come to different answers about whether there is anything to say.
//!
//! # A reading that stopped is not an empty one
//!
//! charter's `_alerts` has ONE guard: an exception anywhere drops every row after it and keeps
//! the rows already made. The terminal cannot say that happened, and charter's status line does
//! not try. **The port keeps the fact** — [`Reading::stopped`] says why the reading ended early —
//! because the app draws a COUNT, and a count taken from a reading that stopped halfway is a
//! smaller number than the truth with nothing on it saying so. `footer.rs`'s rule: a number
//! charter cannot stand behind is dropped, never shown.
//!
//! # The one decided difference
//!
//! charter's pin row ends `· charter version sync`, a verb that installs a published
//! charter-cp. This binary is not one and answers that verb with a refusal that points at
//! `charter version` (`adopt::version_move_refusal`), so this row names `charter version`
//! directly (ADR 0030). The differential declares it (`PIN_ROW_REMEDY` in
//! `tests/differential/run.py`) and still compares every other byte of the row.

use std::path::{Path, PathBuf};

use crate::doctor::Config;
use crate::footer::{Look, Role};

const R: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";

/// How loud an alert is — charter's two accents above plain text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Something to act on: charter's `warn` accent.
    Warn,
    /// Something that loses work if left: charter's `bad` accent.
    Bad,
}

/// What the plane root is doing with a memory commit that did not land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Memory {
    /// On the remote under `charter/<sha>`, waiting on a pull request. Nothing is at risk.
    AwaitingPullRequest,
    /// Reached nowhere: the next `git reset --hard origin/<branch>` deletes it.
    NotPushed,
}

/// One alert, as facts. [`Alert::line`] and [`Alert::shown`] word it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alert {
    /// A `[charter] version` pin beside `[update] channel = "dev"`.
    PinBesideDev { pinned: String },
    /// A pin the running charter does not meet.
    PinDrift { running: String, pinned: String },
    /// `[persona] default` names no persona.
    FrontDoor { declared: String },
    /// Workspaces other than the active one that are behind the current layout.
    Reinit { stale: Vec<String> },
    /// This plane is pinned inside another plane's `workspaces/`.
    NestedPlane { inner: PathBuf, outer: PathBuf },
    /// The plane root is being worked in. At least one finding is set.
    PlaneRoot {
        /// The root directory's own name, which the row names.
        name: String,
        dirty: bool,
        detached: bool,
        /// `(branch, default)` when HEAD is on a branch that is not the default.
        off: Option<(String, String)>,
        memory: Option<Memory>,
    },
}

/// The same facts as plain words, for a surface that is not a terminal.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Shown {
    pub severity: Severity,
    /// What the alert is about, in charter's word for it — `charter`, `front door`, `reinit`,
    /// `nested plane`, `plane root`.
    pub subject: String,
    /// What is wrong.
    pub detail: String,
    /// The command, or the step, that fixes it.
    pub remedy: String,
}

impl Alert {
    pub fn severity(&self) -> Severity {
        match self {
            Alert::NestedPlane { .. } => Severity::Bad,
            Alert::PlaneRoot {
                memory: Some(Memory::NotPushed),
                ..
            } => Severity::Bad,
            _ => Severity::Warn,
        }
    }

    /// The row charter's status line prints, before the frame — byte for byte `_alerts`'s.
    pub fn line(&self, look: &Look) -> String {
        let warn = look.accent(Role::Warn);
        let bad = look.accent(Role::Bad);
        match self {
            Alert::PinBesideDev { pinned } => {
                format!("{warn}⚠{R} {DIM}charter{R} {pinned} {DIM}{PIN_BESIDE_DEV_BRIEF}{R}")
            }
            Alert::PinDrift { running, pinned } => format!(
                "{warn}⚠{R} {DIM}charter{R} {running} {DIM}→ pinned{R} {pinned}{DIM} · \
                 {PIN_REMEDY}{R}"
            ),
            Alert::FrontDoor { declared } => format!(
                "{warn}⚠{R} {DIM}front door{R} {declared} {DIM}— no such persona · \
                 {FRONT_DOOR_REMEDY}{R}"
            ),
            Alert::Reinit { stale } => format!(
                "{warn}⚠{R} {DIM}reinit{R} {} {DIM}ws · {REINIT_REMEDY}{R}",
                stale.len()
            ),
            Alert::NestedPlane { inner, outer } => format!(
                "{bad}⚠{R} {DIM}nested plane{R} — {DIM}memory and vault go to{R} {}{DIM}, \
                 not{R} {}{DIM} · unset CHARTER_ROOT{R}",
                short_path(inner),
                short_path(outer)
            ),
            Alert::PlaneRoot {
                name,
                dirty,
                detached,
                off,
                memory,
            } => {
                let mut bits = Vec::new();
                if *dirty {
                    bits.push(format!("{warn}dirty{R}"));
                }
                if *detached {
                    bits.push(format!("{warn}detached HEAD{R}"));
                } else if let Some((branch, default)) = off {
                    bits.push(format!("{DIM}on{R} {branch}{DIM}, not{R} {default}"));
                }
                match memory {
                    Some(Memory::AwaitingPullRequest) => {
                        bits.push(format!("{warn}memory awaiting a pull request{R}"));
                    }
                    Some(Memory::NotPushed) => {
                        bits.push(format!("{bad}memory commit not pushed{R}"))
                    }
                    None => {}
                }
                let sep = format!("{DIM} · {R}");
                format!(
                    "{warn}⚠{R} {DIM}plane root{R} {name}{sep}{}{DIM} · {PLANE_ROOT_REMEDY}{R}",
                    bits.join(&sep)
                )
            }
        }
    }

    /// The facts as words a drawer can lay out: no escapes, and the remedy apart from the
    /// finding so it can be set as a command.
    pub fn shown(&self) -> Shown {
        let (subject, detail, remedy) = match self {
            Alert::PinBesideDev { pinned } => (
                "charter",
                format!(
                    "pins {pinned} and follows the dev channel: {}",
                    TWO_CHARTERS
                ),
                PIN_REMEDY.to_owned(),
            ),
            Alert::PinDrift { running, pinned } => (
                "charter",
                format!("this charter brought {running}, and the plane pins {pinned}"),
                PIN_REMEDY.to_owned(),
            ),
            Alert::FrontDoor { declared } => (
                "front door",
                format!("{declared} — no such persona"),
                FRONT_DOOR_REMEDY.to_owned(),
            ),
            Alert::Reinit { stale } => (
                "reinit",
                format!(
                    "{} workspace{} behind the current layout: {}",
                    stale.len(),
                    if stale.len() == 1 { " is" } else { "s are" },
                    stale.join(", ")
                ),
                REINIT_REMEDY.to_owned(),
            ),
            Alert::NestedPlane { inner, outer } => (
                "nested plane",
                format!(
                    "memory and vault go to {}, not {}",
                    short_path(inner),
                    short_path(outer)
                ),
                format!("open {} instead, or unset CHARTER_ROOT", short_path(outer)),
            ),
            Alert::PlaneRoot {
                name,
                dirty,
                detached,
                off,
                memory,
            } => {
                let mut bits = Vec::new();
                if *dirty {
                    bits.push("dirty".to_owned());
                }
                if *detached {
                    bits.push("detached HEAD".to_owned());
                } else if let Some((branch, default)) = off {
                    bits.push(format!("on {branch}, not {default}"));
                }
                match memory {
                    Some(Memory::AwaitingPullRequest) => {
                        bits.push("memory awaiting a pull request".to_owned());
                    }
                    Some(Memory::NotPushed) => bits.push("memory commit not pushed".to_owned()),
                    None => {}
                }
                (
                    "plane root",
                    format!("{name} · {}", bits.join(" · ")),
                    PLANE_ROOT_REMEDY.to_owned(),
                )
            }
        };
        Shown {
            severity: self.severity(),
            subject: subject.to_owned(),
            detail,
            remedy,
        }
    }
}

/// `update._TWO_CHARTERS`.
const TWO_CHARTERS: &str = "two different charters";

/// `update.pin_beside_dev().brief` — the one wording of that state, which names the command
/// whose output carries both ways out of it.
const PIN_BESIDE_DEV_BRIEF: &str = "pin + dev channel: two different charters · charter version";

/// Where the pin row sends the operator. charter says `charter version sync`; see the module
/// docs for why this binary says `charter version`.
pub const PIN_REMEDY: &str = "charter version";
const FRONT_DOOR_REMEDY: &str = "charter persona default <name>";
const REINIT_REMEDY: &str = "charter ws reinit --all";
const PLANE_ROOT_REMEDY: &str = "work belongs in a workspace clone";

/// What one reading of a plane's alerts found — and whether it finished.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reading {
    pub alerts: Vec<Alert>,
    /// Why the reading ended before the last alert was asked about, or `None` when every
    /// alert was. **The alerts it did find stand** — charter keeps the rows made before its
    /// guard fires — but their number is not the plane's number of alerts.
    pub stopped: Option<String>,
}

impl Reading {
    /// How many alerts the plane has, when the reading can stand behind the number.
    pub fn count(&self) -> Option<usize> {
        Some(self.alerts.len())
    }
}

/// Who asked, and from where.
pub struct Asking<'a> {
    /// The plane.
    pub root: &'a Path,
    /// The workspace whose own stale layout the asking surface already flags, or `None` when
    /// it flags none — the status line's identity row flags the active workspace's, the app
    /// flags none, so the app's reinit alert counts every stale workspace.
    pub active: Option<&'a str>,
    /// The directory the asker stands in — charter's `NESTED_ORIGIN` is asked of it. The app
    /// stands in the plane it opened.
    pub standing: &'a Path,
}

/// Every alert the plane has, in charter's order — `_alerts`.
///
/// Never panics and returns no `Result`, for the status line's one hard contract: a render
/// that fails costs rows, never the render.
pub fn read(ask: &Asking) -> Reading {
    let mut reading = Reading::default();
    if let Err(why) = gather(ask, &mut reading.alerts) {
        reading.stopped = Some(why);
    }
    reading
}

/// The rows, appended in charter's order; `Err` is charter's guard firing, with the rows
/// already appended kept.
fn gather(ask: &Asking, out: &mut Vec<Alert>) -> Result<(), String> {
    let root = ask.root;
    // `instance.load`: a manifest charter cannot read raises before the first row, and so
    // costs every row.
    let cfg = match Config::load(root) {
        Config::Read(cfg) => cfg,
        Config::Malformed(why) | Config::Refused(why) => return Err(why),
    };

    let locked = section(&cfg, "charter")?
        .and_then(|charter| charter.get("version"))
        .and_then(toml::Value::as_str)
        .map(crate::memstore::py_strip)
        .filter(|v| !v.is_empty())
        .map(str::to_owned);
    if let Some(pinned) = locked {
        let running = crate::news::shipped_version();
        if follows_dev(&cfg) {
            out.push(Alert::PinBesideDev { pinned });
        } else if pinned != running {
            out.push(Alert::PinDrift { running, pinned });
        }
    }

    // `default_persona_of`: `str(val).strip()`, so a front door that is not a string is still
    // quoted back as charter prints it, and a blank one is none.
    let declared = section(&cfg, "persona")
        .ok()
        .flatten()
        .and_then(|persona| persona.get("default"))
        .map(|v| crate::memstore::py_strip(&crate::pyrepr::str_toml(v)).to_owned())
        .filter(|v| !v.is_empty());
    if let Some(declared) = declared
        && !persona_exists(root, &declared)
    {
        out.push(Alert::FrontDoor { declared });
    }

    let names = crate::workspaces::Plane::open(root)
        .workspaces()
        .map_err(|e| format!("workspaces/ could not be listed: {e}"))?;
    let stale: Vec<String> = names
        .into_iter()
        .filter(|ws| crate::footer::needs_reinit(root, ws))
        .collect();
    if !stale.is_empty() {
        out.push(Alert::Reinit { stale });
    }

    // Only when the hop outward was OVERRIDDEN: standing in the nested plane, and acting on it.
    // A plain hop acts on the outer plane, where memory and vaults DO go, and saying otherwise
    // would be false on every render.
    let inner = crate::doctor::canonical(root);
    if (crate::plane::standing_in_nested_plane(ask.standing).is_some() || ask.active.is_none())
        && let Some(outer) = crate::plane::enclosing(&inner)
    {
        out.push(Alert::NestedPlane { inner, outer });
    }

    // Last, because it is the newest and charter's one guard makes the last row the one that
    // pays when something goes wrong. It has its own guard too: a root that cannot be read
    // costs this row and nothing else.
    out.extend(plane_root(root));
    Ok(())
}

/// `(cfg.get(name) or {})` — the table, `None` for a falsy value, and charter's raise for a
/// truthy value that is not a table (`'str' object has no attribute 'get'`).
fn section<'a>(cfg: &'a toml::Table, name: &str) -> Result<Option<&'a toml::Table>, String> {
    match cfg.get(name) {
        None => Ok(None),
        Some(toml::Value::Table(t)) => Ok(Some(t)),
        Some(v) if !crate::doctor::config_truthy(v) && v.is_str() => Ok(None),
        Some(v) => Err(format!(
            "[{name}] in charter.toml is a {}, not a table",
            v.type_str()
        )),
    }
}

/// `channel.is_dev()`: `[update] channel` is exactly `"dev"`. Anything else, including a word
/// charter does not know, is the conservative `stable`.
fn follows_dev(cfg: &toml::Table) -> bool {
    cfg.get("update")
        .and_then(toml::Value::as_table)
        .and_then(|u| u.get("channel"))
        .and_then(toml::Value::as_str)
        == Some("dev ")
}

/// `persona.def_path(name).exists()` — the directory layout or the legacy flat file.
///
/// **Unknown is not missing**: a path the filesystem will not answer about (permission denied,
/// a name too long) is taken as there, as charter's `_persona_exists` takes a failure, because
/// an alert manufactured from a failed look is exactly the false alarm this row cannot afford.
/// Only the answers `Path.exists` reads as "not there" are.
fn persona_exists(root: &Path, name: &str) -> bool {
    let personas = root.join("personas");
    [
        personas.join(name).join("persona.md"),
        personas.join(format!("{name}.md")),
    ]
    .iter()
    .any(|p| there(p) == Some(true))
}

/// `Some(true)` there, `Some(false)` one of `Path.exists`'s "not there" answers, `None` when
/// the filesystem would not say.
fn there(path: &Path) -> Option<bool> {
    match std::fs::metadata(path) {
        Ok(_) => Some(true),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Some(false)
        }
        // `ELOOP` and `EBADF` are `Path.exists`'s other two "not there"s; a NUL in the name is
        // its `ValueError`, which it answers as not there too.
        Err(e) if e.raw_os_error() == Some(ELOOP) || e.raw_os_error() == Some(EBADF) => Some(false),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => Some(false),
        Err(_) => None,
    }
}

/// `ELOOP`: 62 on macOS and the BSDs, 40 on Linux.
#[cfg(target_os = "linux")]
const ELOOP: i32 = 40;
#[cfg(not(target_os = "linux"))]
const ELOOP: i32 = 62;
/// `EBADF`: 9 everywhere charter runs.
const EBADF: i32 = 9;

/// `_plane_root_alert`: the root being worked in, or `None` — the ordinary case.
fn plane_root(root: &Path) -> Option<Alert> {
    if !root.join(crate::plane::MANIFEST).is_file() {
        return None;
    }
    // Two cases, one answer: a plane that is not a repository, and one that is a subdirectory
    // of somebody else's. Asking git about the directory anyway would answer for the
    // surrounding repository.
    let gitdir = git_dir(root).ok()??;
    let dirty = tracked_dirty(root);
    let branch = branch_of(&gitdir);
    let default = default_branch(&gitdir).ok()?;
    let detached = head_detached(&gitdir).ok()?;
    let off = match (&default, detached) {
        (Some(default), false) if &branch != default => Some((branch, default.clone())),
        _ => None,
    };
    let memory = unlanded_memory(root);
    if !dirty && !detached && off.is_none() && memory.is_none() {
        return None;
    }
    let name = crate::doctor::canonical(root)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Some(Alert::PlaneRoot {
        name,
        dirty,
        detached,
        off,
        memory,
    })
}

/// A file's text: `Ok(None)` for the `OSError` charter catches, `Err` for the undecodable
/// bytes it does not — which escape to `_plane_root_alert`'s own guard and cost the row.
fn read_text(path: &Path) -> Result<Option<String>, ()> {
    match std::fs::read(path) {
        Err(_) => Ok(None),
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| ()),
    }
}

/// `util.git_dir`: a clone's `.git` directory, or the path a linked worktree's `.git` file
/// names.
fn git_dir(tree: &Path) -> Result<Option<PathBuf>, ()> {
    let g = tree.join(".git");
    if g.is_dir() {
        return Ok(Some(g));
    }
    let Some(text) = read_text(&g)? else {
        return Ok(None);
    };
    let text = crate::memstore::py_strip(&text);
    let Some(rest) = text.strip_prefix("gitdir:") else {
        return Ok(None);
    };
    let p = PathBuf::from(crate::memstore::py_strip(rest));
    Ok(Some(if p.is_absolute() { p } else { tree.join(p) }))
}

/// `util.branch_of`: HEAD read straight off the git directory. `?` when it cannot be read, a
/// short sha when detached, and the ref with only `refs/heads/` taken off otherwise.
fn branch_of(gitdir: &Path) -> String {
    let Ok(Some(text)) = read_text(&gitdir.join("HEAD")) else {
        return "?".to_owned();
    };
    let text = crate::memstore::py_strip(&text);
    if text.starts_with("ref:") {
        // `txt.split("/", 2)[-1]`: the third piece keeps any slashes a branch name has.
        let last = text.splitn(3, '/').last().unwrap_or("");
        return if last.is_empty() {
            "?".to_owned()
        } else {
            last.to_owned()
        };
    }
    if text.is_empty() {
        return "?".to_owned();
    }
    text.chars().take(7).collect()
}

/// `_head_detached`: HEAD names a commit rather than a branch. `false` when HEAD cannot be
/// read: unknown is not detached.
fn head_detached(gitdir: &Path) -> Result<bool, ()> {
    Ok(match read_text(&gitdir.join("HEAD"))? {
        None => false,
        Some(text) => {
            let text = crate::memstore::py_strip(&text);
            !text.is_empty() && !text.starts_with("ref:")
        }
    })
}

/// `_common_git_dir`: where the shared refs live — a linked worktree's `commondir`.
fn common_git_dir(gitdir: &Path) -> Result<PathBuf, ()> {
    let Some(text) = read_text(&gitdir.join("commondir"))? else {
        return Ok(gitdir.to_path_buf());
    };
    let text = crate::memstore::py_strip(&text);
    if text.is_empty() {
        return Ok(gitdir.to_path_buf());
    }
    let p = PathBuf::from(text);
    Ok(if p.is_absolute() { p } else { gitdir.join(p) })
}

/// `_default_branch`: `origin/HEAD` when the repository says, else a local `main` or
/// `master`, else `None` — which is a real answer and silences the branch half of the row.
fn default_branch(gitdir: &Path) -> Result<Option<String>, ()> {
    let common = common_git_dir(gitdir)?;
    let head = read_text(&common.join("refs/remotes/origin/HEAD"))?.unwrap_or_default();
    let head = crate::memstore::py_strip(&head);
    if let Some(rest) = head.strip_prefix("ref: refs/remotes/origin/") {
        let rest = crate::memstore::py_strip(rest);
        return Ok((!rest.is_empty()).then(|| rest.to_owned()));
    }
    for name in ["main", "master"] {
        if ref_exists(&common, &format!("refs/heads/{name}"))? {
            return Ok(Some(name.to_owned()));
        }
    }
    Ok(None)
}

/// `_ref_exists`: a loose ref, or a line of `packed-refs`.
fn ref_exists(common: &Path, reference: &str) -> Result<bool, ()> {
    if common.join(reference).is_file() {
        return Ok(true);
    }
    let Some(packed) = read_text(&common.join("packed-refs"))? else {
        return Ok(false);
    };
    let suffix = format!(" {reference}");
    Ok(packed
        .lines()
        .any(|ln| crate::memstore::py_rstrip(ln).ends_with(&suffix)))
}

/// How long `git status` may take on a render — `_run_state`'s `timeout=3`.
const STATUS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// `_run_state`'s `tracked_dirty`: `git status` has a line that is not untracked (`??`).
/// Untracked files are not the root being worked in — memory defaults to `share = "local"`, so
/// every plane a few days old carries them — and `doctor` asks git `-uno` for the same reason.
///
/// `false` when git could not be asked: a few silent seconds are a better failure than a
/// warning made up from nothing.
fn tracked_dirty(root: &Path) -> bool {
    let Ok(run) = crate::worktree::git::run(
        root,
        &["status", "--porcelain=v1", "--branch"],
        STATUS_TIMEOUT,
    ) else {
        return false;
    };
    run.out
        .lines()
        .filter(|ln| !ln.starts_with("## "))
        .filter(|ln| !crate::memstore::py_strip(ln).is_empty())
        .any(|ln| !ln.starts_with("??"))
}

/// `_unlanded_memory`: a memory commit whose push did not reach `origin`, and which of the two
/// opposite remedies it needs.
///
/// Through [`crate::planegit::push_record`] and [`crate::planegit::is_spent`] — the decision
/// `save` and `doctor` share — rather than `planegit::unlanded`, which drops a record that
/// carries no `head` where charter keeps it: an unknown commit is not a landed one.
fn unlanded_memory(root: &Path) -> Option<Memory> {
    let rec = crate::planegit::push_record(root)?;
    let head = rec
        .get("head")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if crate::planegit::is_spent(root, head) {
        return None;
    }
    let branched = rec.get("outcome").and_then(serde_json::Value::as_str)
        == Some(crate::planegit::Outcome::Branched.word());
    let landed = rec.get("landed").is_some_and(json_truthy);
    Some(if branched && !landed {
        Memory::AwaitingPullRequest
    } else {
        Memory::NotPushed
    })
}

/// Python's truthiness of a JSON value.
fn json_truthy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(o) => !o.is_empty(),
    }
}

/// `util.short_path` — the crate's one copy of it.
fn short_path(path: &Path) -> String {
    crate::doctor::short_path(path)
}

#[cfg(test)]
mod tests;
