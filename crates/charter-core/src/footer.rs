//! charter's read surface, rendered as one block of text — a port of `charter/statusline.py`'s
//! `render`, as far as its first zone rule.
//!
//! # What this draws, and where the seam is
//!
//! charter's footer is three zones and a frame around them:
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │ ⬢ alpha · todo 3 · pieces 2 1 done · ws 4                 │   zone 1 — WHERE I am
//! ├──────────────────────────────────────────────────────────┤   the zone rule
//! │ ▪ repos 2/38            ▪ personas 5 · vaults 1           │   zone 2 — the plane
//! │ ├─ svc      main *↑2    ▸ steward   ✎12                   │
//! │ └─ tool     trunk ✓     ▫ devops    ✎3                    │
//! ├──────────────────────────────────────────────────────────┤
//! │ ctx 42% · cache 90%                    ⬢ charter 0.62.1  │   zone 3 — this session
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! **This build draws zone 1, the alert rows and the frame, and says in the body what it does
//! not draw.** The alert rows are [`crate::alerts`], compared row for row by the
//! `statusline-alerts-*` differential scenarios. The
//! seam is the zone rule, which is charter's own divider and not one invented for the port:
//! everything above it — the top border, the identity row and the rule itself — is byte for
//! byte what charter prints, and the recorded scenarios compare exactly that
//! (`statusline-*identity-row*`, cut at the rule, ADR 0046).
//!
//! Why a seam at all: zone 2 is a `git status` per clone, linked worktrees drawn as rows,
//! persona chips with vault health and memory counts, and the row planner that
//! decides what a narrow pane gives up — and zone 3 is the usage history and the update cache.
//! They are ports of their own. What this milestone refuses to do is draw *some* of zone 2: a
//! footer that silently omitted the alert row would be worse than a sentence, because an
//! operator reads a footer to find out whether anything needs them, and one that can only ever
//! say "nothing" is a footer that lies once a week. So the omission is a line
//! ([`NOT_DRAWN_YET`]) rather than an absence.
//!
//! # The one rule the identity row is composed by
//!
//! **A count lives next to the thing it counts.** The todos are the active workspace's, so
//! they sit beside its name; the piece counts are that workspace's worktrees; `ws N` is how
//! many others there are to switch to. Nothing here describes a repo, a persona or this
//! session — those are the zones below.
//!
//! **Zero renders NOTHING.** A `todo 0` present every turn is furniture within a day, and then
//! a real `todo 7` in that spot draws no more attention than the zero did. Presence is the
//! signal, and it is why `todo`, the piece cell and the reinit tip are each dropped entirely
//! rather than rendered empty.
//!
//! **The row's order IS its truncation order**, and a warning outranks information: on a pane
//! wide enough for the reinit tip or the todo count but not both, the item naming a broken
//! structure is the one that survives.
//!
//! # Never crash
//!
//! `charter/statusline.py` promises its render never raises, and every helper it calls
//! swallows its own failure to keep that promise. Nothing here returns a `Result` for the same
//! reason: a footer that vanished is worse than one that shows less.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::active::{self, Asking, Ids, WorkspaceRung};
use crate::tui::{self, Node};

// ANSI — a status line renders escape codes. Coloured unconditionally, as every line
// `charter/statusline.py` prints is: the surface is a terminal, and charter's own fallback
// does not consult `$NO_COLOR` either.
const R: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const CYAN: &str = "\x1b[36m";

/// Render to (`$COLUMNS` − this). The pane gives LESS than `$COLUMNS` advertises, and the
/// amount was measured rather than guessed: at 2, a line ending exactly at `COLUMNS`−2 lost
/// its final character to the host's own `…` crop, so the usable width is `COLUMNS`−3. 4
/// leaves one spare column.
pub const SAFETY: usize = 4;

/// Marks a body line as a horizontal rule for [`boxed`] to draw as `├───┤`.
///
/// A sentinel rather than a pre-drawn string because only `boxed` knows the frame's final
/// width — and because a rule has to survive [`tui::truncate`] without being cropped into a
/// shorter line. NUL cannot occur in real content, so it can never collide.
pub const RULE_LINE: &str = "\u{0}charter-rule\u{0}";

/// The body line that stands where zones 2 and 3 will be drawn.
///
/// **Named in the output, on purpose.** A status line that silently omitted a section would be
/// the same lie as a `doctor` printing green for a check that did not run: the reader cannot
/// tell "charter looked and there is nothing" from "charter did not look". So the line says
/// which surfaces are missing, in the order they will arrive.
pub const NOT_DRAWN_YET: &str = "not drawn by this build: repos · personas · session";

/// The eight ANSI colour names, in ECMA-48's own order — `instance.FRAME_PANE_COLOURS`.
///
/// The order is the containment: the foreground code is `30 + index` and the aixterm bright
/// form is `90 + index`, so a table of seventeen escapes is derived rather than written out
/// beside the seventeen names and left to drift from them.
const PANE_COLOURS: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

/// The three roles a plane may recolour, and what charter has ALWAYS drawn each one in.
///
/// On the degradation path rather than kept for symmetry: [`Look::of`] falls back to these for
/// a role whose word is not one of the seventeen, which a `[frame]` table written by hand can
/// produce and `charter.toml`'s own validation cannot be relied on to have removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Ok,
    Warn,
    Bad,
}

impl Role {
    fn key(self) -> &'static str {
        match self {
            Role::Ok => "ok",
            Role::Warn => "warn",
            Role::Bad => "bad",
        }
    }

    /// Green, yellow, red — the escapes this module has drawn since before the keys existed.
    fn shipped(self) -> &'static str {
        match self {
            Role::Ok => "\x1b[32m",
            Role::Warn => "\x1b[33m",
            Role::Bad => "\x1b[31m",
        }
    }
}

/// What `[frame]` says the three accents are, resolved once per render.
///
/// One question asked in one place: two readings of one key is how a surface comes to draw its
/// own warnings in one colour and a component's in another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Look {
    ok: String,
    warn: String,
    bad: String,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            ok: Role::Ok.shipped().to_string(),
            warn: Role::Warn.shipped().to_string(),
            bad: Role::Bad.shipped().to_string(),
        }
    }
}

impl Look {
    /// The plane's `[frame]` accents, read at render time.
    ///
    /// Read rather than cached because `charter.toml` is re-read when the frame relaunches and
    /// a panel is a long-lived process.
    ///
    /// **A word charter does not know degrades to the shipped escape**, and that is the same
    /// answer charter reaches by a different road: its own `frame_of` replaces an unknown word
    /// with the default (`green`/`yellow`/`red`), whose escape is the shipped one. Two doors,
    /// one colour — so this port does not have to reproduce the validation to agree with it.
    pub fn of(plane: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(plane.join(crate::plane::MANIFEST)) else {
            return Self::default();
        };
        let Ok(doc) = text.parse::<toml::Table>() else {
            return Self::default();
        };
        let frame = doc.get("frame").and_then(toml::Value::as_table);
        let word = |role: Role| {
            frame
                .and_then(|f| f.get(role.key()))
                // `isinstance(word, str)` — `tomllib` can hand a `[frame]` key a list or a
                // table, and a lookup with one would raise. A non-string is not a colour.
                .and_then(toml::Value::as_str)
                .and_then(sgr_for)
                .unwrap_or_else(|| role.shipped().to_string())
        };
        Self {
            ok: word(Role::Ok),
            warn: word(Role::Warn),
            bad: word(Role::Bad),
        }
    }

    /// The SGR this plane wants `role` drawn in.
    pub fn accent(&self, role: Role) -> &str {
        match role {
            Role::Ok => &self.ok,
            Role::Warn => &self.warn,
            Role::Bad => &self.bad,
        }
    }
}

/// One colour word as its SGR, or `None` when charter does not know the word.
///
/// `default` is **SGR 39**, the pane's own foreground — which is what makes it a real answer
/// rather than a hole: a plane that says `warn = "default"` has asked for its warnings
/// uncoloured, and gets text in the colour the rest of its frame is already in.
fn sgr_for(word: &str) -> Option<String> {
    if word == "default" {
        return Some("\x1b[39m".to_string());
    }
    if let Some(i) = PANE_COLOURS.iter().position(|c| *c == word) {
        return Some(format!("\x1b[{}m", 30 + i));
    }
    let bright = word.strip_prefix("bright")?;
    let i = PANE_COLOURS.iter().position(|c| *c == bright)?;
    Some(format!("\x1b[{}m", 90 + i))
}

/// Everything the render reads that is not a file.
pub struct Ambient<'a> {
    /// The process environment, named rather than read, so a test drives every rung.
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// This process's own directory, for the payload that does not carry one.
    pub cwd: &'a Path,
    /// The instant ages are measured from.
    pub now: DateTime<Utc>,
    /// charter's config home, where the extension record is — `None` draws no extension's
    /// badges (charter-app#340).
    pub config: Option<&'a Path>,
}

/// The whole footer, frame included, with the trailing newline `charter/statusline.py:render`
/// ends on.
///
/// `payload` is the harness's per-turn JSON, or `Value::Null` when there was none.
pub fn render(plane: &Path, payload: &Value, ambient: &Ambient) -> String {
    let look = Look::of(plane);
    let active = active_workspace(plane, payload, ambient);

    // Render a hair under `$COLUMNS` (which Claude Code sets to the pane width) so a line
    // never fills the last column, which the terminal would wrap.
    let frame_w = tui::term_width(ambient.env, 80, 24)
        .saturating_sub(SAFETY)
        .max(24);
    // Everything below lays out inside the frame, so it gets the pane minus the box's own
    // chrome (`│ ` each side). `boxed` re-widens to `frame_w` at the end.
    let width = frame_w.saturating_sub(4).max(24);

    // Below zone 2 in charter, and full width: actionable problems with the plane, each with
    // the command that fixes it. Here they follow the line that stands where zone 2 will go,
    // so the order is charter's order with the part not yet drawn named in its place.
    let alerts = crate::alerts::read(&crate::alerts::Asking {
        root: plane,
        active: Some(&active.name),
        standing: ambient.cwd,
    });
    let mut rows = vec![
        identity_row(plane, &active, &look, ambient),
        format!("{DIM}{NOT_DRAWN_YET}{R}"),
    ];
    if let Some(config) = ambient.config {
        rows.extend(badge_rows(plane, config, &active.name, ambient.now));
    }
    rows.extend(alerts.alerts.iter().map(|alert| alert.line(&look)));
    let body = Node::stack(rows).render(width);
    let body = zone_rules(&body);
    format!("{}\n", boxed(&body, frame_w))
}

/// Extensions' footer badges, as one row, and a dim line for each that contributed nothing and
/// should have — or no row at all, which is every machine without one (charter-app#340).
///
/// **Read through the one reader** ([`crate::extension::facts::gather`]), which starts no
/// program. A stale value is dimmed whole and carries its age, so an old count never reads as
/// a current one.
fn badge_rows(plane: &Path, config: &Path, workspace: &str, now: DateTime<Utc>) -> Vec<String> {
    use crate::extension::{facts, project::Choices};
    let read = facts::gather(
        config,
        &crate::extension::BuiltIn::none(),
        || Choices::read_in(plane, Some(workspace)),
        now,
        facts::Reading::Footer,
    );
    let mut rows = Vec::new();
    if !read.badges.is_empty() {
        let drawn: Vec<String> = read
            .badges
            .iter()
            .map(|badge| {
                if badge.stale {
                    format!(
                        "{DIM}{} {} · {} ago{R}",
                        badge.label,
                        badge.value,
                        facts::age(badge.age_seconds)
                    )
                } else {
                    format!("{DIM}{}{R} {}", badge.label, badge.value)
                }
            })
            .collect();
        rows.push(drawn.join(&format!("{DIM} · {R}")));
    }
    rows.extend(read.notes.iter().map(|note| format!("{DIM}{note}{R}")));
    rows
}

/// `(workspace, which rung said so)` for the SESSION, not for this process.
///
/// The payload's `current_dir` matters because the cwd rung outranks every pointer, so reading
/// the hook's own directory there does not merely miss a better answer — it overrides the
/// right one.
fn active_workspace(plane: &Path, payload: &Value, ambient: &Ambient) -> active::ActiveWorkspace {
    let sid = payload
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    // `session.current(explicit)` is `explicit or $CHARTER_SESSION_ID or
    // $CLAUDE_CODE_SESSION_ID`, then sanitised — so handing the payload's id in as the first
    // variable IS that ladder, rather than a second spelling of it beside it.
    let env = |name: &str| match name {
        "CHARTER_SESSION_ID" => sid.clone().or_else(|| (ambient.env)(name)),
        _ => (ambient.env)(name),
    };
    let ids = Ids::of(&env);
    let here: PathBuf = payload
        .get("workspace")
        .and_then(|w| w.get("current_dir"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map_or_else(|| ambient.cwd.to_path_buf(), PathBuf::from);
    let workspace_env = (ambient.env)(active::WORKSPACE_ENV);
    active::workspace(&Asking {
        root: plane,
        cwd: &here,
        flag: None,
        ids: &ids,
        env: workspace_env.as_deref(),
    })
}

/// Zone 1: the workspace's name and what is true of that workspace.
fn identity_row(
    plane: &Path,
    active: &active::ActiveWorkspace,
    look: &Look,
    ambient: &Ambient,
) -> String {
    // A `*` for a workspace pinned by the environment, so a session that cannot be moved with
    // `charter ws use` says so where the name is read.
    let pin = if active.rung == WorkspaceRung::Environment {
        format!("{}*{R}", look.accent(Role::Warn))
    } else {
        String::new()
    };
    // The reinit tip sits right after the name so it survives truncation on a narrow pane.
    // Nothing informational goes in front of it: it is the one item on this row that reports
    // something BROKEN, and it carries the command that fixes it.
    let reinit = needs_reinit(plane, &active.name).then(|| {
        format!(
            "{}⚠ reinit: {BOLD}charter ws reinit{R}",
            look.accent(Role::Warn)
        )
    });
    let ntodo = todo_count(plane, &active.name);
    let mut parts = vec![format!("{CYAN}⬢{R} {BOLD}{}{R}{pin}", active.name)];
    parts.extend(reinit);
    if ntodo > 0 {
        // A plain word, no glyph: the label already reads, and this layout has twice paid for
        // a character a font drew wider than the Unicode tables claim. `todo` singular because
        // it is exactly the subcommand that shows them, `charter ws todo`, so the label
        // doubles as the way to read the detail.
        parts.push(format!("{DIM}todo{R} {ntodo}"));
    }
    parts.extend(piece_cell(plane, &active.name, look, ambient.now));
    parts.push(format!(
        "{DIM}ws{R} {}",
        crate::workspaces::Plane::open(plane)
            .workspaces()
            .map_or(0, |all| all.len())
    ));
    parts.join(&format!("{DIM} · {R}"))
}

/// The identity row's piece cell — counts, and the oldest silence. `None` when the workspace
/// has no pieces, exactly as the todo count is dropped at zero.
fn piece_cell(plane: &Path, ws: &str, look: &Look, now: DateTime<Utc>) -> Option<String> {
    let summary = crate::pieces::summary(plane, ws, now)?;
    let mut parts = vec![format!("{DIM}pieces{R} {}", summary.total)];
    if summary.done > 0 {
        parts.push(format!("{}{} done{R}", look.accent(Role::Ok), summary.done));
    }
    if summary.gave_up > 0 {
        parts.push(format!(
            "{}{} abandoned{R}",
            look.accent(Role::Warn),
            summary.gave_up
        ));
    }
    if let Some(oldest) = summary.oldest_silence() {
        parts.push(format!("{DIM}{} silent {oldest}{R}", summary.quiet.len()));
    }
    Some(parts.join(" "))
}

/// How many todos the workspace still has open.
///
/// Cheap by construction — one directory listing, no parse — because this renders on EVERY
/// turn. Zero on any failure, never an exception: the count is the least important thing on
/// the line, and trading the whole footer for one digit is the wrong bargain by a wide margin.
/// A store charter could not read is zero too, which is `todos.count_open` raising through
/// `memstore.files` and the render swallowing it.
fn todo_count(plane: &Path, ws: &str) -> usize {
    let dir = plane.join("workspaces").join(ws).join("todos");
    let (found, unread) = crate::memstore::read_files(plane, &dir);
    if unread.is_empty() { found.len() } else { 0 }
}

/// The baseline files every workspace has, and the version the layout is at — `workspace.py`'s
/// `_required_components` and `STRUCTURE_VERSION`.
const BASELINE: [&str; 4] = [
    "workspace.md",
    "workspace.json",
    "memory/MEMORY.md",
    "refs/README.md",
];
const STRUCTURE_VERSION: i64 = 5;
const STRUCTURE_MARKER: &str = ".charter-structure";
const LEGACY_STRUCTURE_MARKER: &str = ".edm-structure";

/// Is the active workspace's on-disk structure behind the current layout?
///
/// Stale means a baseline file is missing **and nothing is standing in its way**, or the
/// version marker is behind. The distinction is charter's and it is not pedantry: a path that
/// is absent because a plain file sits where its directory goes is not one `reinit` can create,
/// so calling it missing would flag a workspace for a repair that cannot happen and have the
/// repair report that it happened.
///
/// **Two divergences from `charter/workspace.py`, both deliberate and both narrow:**
///
/// 1. charter MIGRATES a pre-rename `.edm-structure` marker in place while reading it — a
///    rename on the render path. This reads both names and renames neither: a footer is a
///    read, and the answer (which version is stamped) is the same either way.
/// 2. charter lets a symlink stand where a *directory* of the layout goes when it lands on a
///    directory inside the plane's own data; here any symlink on the way is in the way. The
///    two answers differ only for a workspace that carries such a link AND is missing the file
///    under it — a plane where the footer would be flagging a repair that charter would not.
pub(crate) fn needs_reinit(plane: &Path, ws: &str) -> bool {
    let dir = plane.join("workspaces").join(ws);
    if !dir.exists() {
        // Nothing to reinit: a workspace that is not there is not a stale one. `render` still
        // names it, because the ladder chose it and saying so is the point of the row.
        return false;
    }
    let missing = BASELINE.iter().any(|rel| {
        let path = rel.split('/').fold(dir.clone(), |p, part| p.join(part));
        match std::fs::metadata(&path) {
            Ok(_) => false,
            Err(e) if absent(&e) => !in_the_way(&dir, &path),
            // Unreadable is its own answer: charter could not look, which is not "gone".
            Err(_) => false,
        }
    });
    missing || structure_version(&dir) < STRUCTURE_VERSION
}

/// `FileNotFoundError` or `NotADirectoryError` — the two answers charter reads as "not there".
fn absent(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

/// Does something stand between `base` and `path` that a create there would meet?
///
/// Walks from the workspace directory down, never above it: the plane root and the temp
/// directory above it may be links for reasons nobody committed (`/var` is one on macOS), and
/// every caller has already reached `base` through them.
fn in_the_way(base: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(base) else {
        return false;
    };
    let mut walk = base.to_path_buf();
    // `base` itself is a directory of the layout, so it is the first thing asked about.
    let mut components = vec![walk.clone()];
    for part in rel.iter() {
        walk.push(part);
        components.push(walk.clone());
    }
    for walk in components {
        match std::fs::symlink_metadata(&walk) {
            // A link is in the way wherever it points. A workspace's tree is committed and git
            // stores a symlink as a symlink, so a link in it is something a teammate's commit
            // can put there — and charter wrote a README *through* one, creating a file
            // wherever it pointed (charter #1037).
            Ok(meta) if meta.file_type().is_symlink() => return true,
            // Something that is no directory where a directory of the layout goes.
            Ok(meta) if walk != path && !meta.is_dir() => return true,
            Ok(_) => {}
            // Absent is what a create makes, and unanswered is not charter's to call either.
            Err(_) => return false,
        }
    }
    false
}

/// The layout version stamped in the workspace's marker — 0 if missing, unreadable, or not a
/// regular file.
fn structure_version(dir: &Path) -> i64 {
    // The current name decides as soon as it is THERE, whatever it turns out to be: charter
    // reads the legacy one only when the current one is absent (it renames it into place and
    // reads that), so a current marker that is a directory answers 0 rather than falling
    // through to an older stamp beside it.
    let current = dir.join(STRUCTURE_MARKER);
    let path = match std::fs::symlink_metadata(&current) {
        Ok(_) => current,
        Err(_) => dir.join(LEGACY_STRUCTURE_MARKER),
    };
    // `O_NOFOLLOW`'s rule, as a check charter can make without opening: a version read through
    // a link is not charter's, because charter never writes the stamp through one. `is_file`
    // of a `symlink_metadata` is false for a link, a directory and a FIFO alike — and the FIFO
    // is why charter opens `O_NONBLOCK`: a blocking open at that name froze `reinit` and every
    // status-line render in the workspace (charter #1074).
    let Ok(meta) = std::fs::symlink_metadata(&path) else {
        return 0;
    };
    if !meta.is_file() {
        return 0;
    }
    // One page, as charter reads: the stamp is a few bytes and a large file at that name is
    // not a stamp charter wrote.
    let Ok(bytes) = std::fs::read(&path) else {
        return 0;
    };
    String::from_utf8_lossy(&bytes[..bytes.len().min(4096)])
        .trim()
        .parse::<i64>()
        .unwrap_or(0)
}

/// Insert the zone dividers: one under the workspace line.
///
/// Applied to the finished body rather than threaded through the layout, which works because
/// the workspace summary is always the first line. Splicing here keeps a divider out of the
/// column layout, where it would be padded and truncated as though it were content.
///
/// charter inserts a second rule above the session strip; this build draws no strip, so there
/// is no second anchor to splice against — a rule there would separate the body from nothing
/// but the bottom border, which is the case charter itself declines to draw.
fn zone_rules(body: &[String]) -> Vec<String> {
    if body.len() < 2 {
        return body.to_vec();
    }
    let mut out = vec![body[0].clone(), RULE_LINE.to_string()];
    out.extend_from_slice(&body[1..]);
    out
}

/// Frame the whole status line: `┌───┐` above and below, `│` down each side.
///
/// Applied last, over finished lines, so the box cannot perturb the column maths that ran
/// inside it.
///
/// Box-drawing here, ASCII in the tree, and the split is not arbitrary. These characters are
/// East-Asian *Ambiguous*: a terminal may draw them one cell or two. What breaks a layout is
/// not width itself but width that differs *between rows* — every row carries exactly one left
/// border and one right border, so a terminal drawing them wide shifts every row identically
/// and the columns stay true.
///
/// The frame earns its two rows by being a *ruler*: with a right edge, a row whose content
/// renders wider than [`tui::width`] believes pushes its own `│` past the others, so drift
/// becomes a thing you can see and point at instead of a mystery.
///
/// Silently returns the body unchanged if the pane is too narrow to frame — the box is
/// decoration and must never cost content.
pub fn boxed(body: &[String], width: usize) -> String {
    let Some(inner) = width.checked_sub(4).filter(|inner| *inner >= 20) else {
        // Unframed: a rule has no side borders to join, so it is dropped rather than printed.
        // The sentinel must never reach a terminal.
        return body
            .iter()
            .filter(|ln| *ln != RULE_LINE)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
    };
    let bar = |left: &str, right: &str| format!("{DIM}{left}{}{right}{R}", "─".repeat(width - 2));
    let mut out = vec![bar("┌", "┐")];
    for ln in body {
        if ln == RULE_LINE {
            // Drawn here, not in layout, so it is always exactly as wide as the top and bottom
            // borders — a rule that disagreed with them would be the most visible possible
            // defect.
            out.push(bar("├", "┤"));
            continue;
        }
        let ln = tui::truncate(ln, inner);
        let fill = " ".repeat(inner.saturating_sub(tui::width(&ln)));
        out.push(format!("{DIM}│{R} {ln}{fill} {DIM}│{R}"));
    }
    out.push(bar("└", "┘"));
    out.join("\n")
}

#[cfg(test)]
mod tests;
