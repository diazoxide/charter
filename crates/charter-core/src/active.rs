//! Which workspace and which persona a command acts on when nobody said.
//!
//! Until this module the Rust `charter` had no notion of either: every command that needed a
//! workspace took `-w` and **required** it, so `charter ws remember "…"` — and, once M2.2
//! lands it, `charter recall` with no flags, which is how a harness calls it at session start
//! — was refused with a usage error rather than run. Python resolves both, and its ladders
//! are the contract `docs/plane-format.md` records; this is that contract in Rust.
//!
//! # The workspace ladder
//!
//! `charter/workspace.py:589` (`chosen`) plus `resolve`'s fallback under it, and
//! `docs/plane-format.md`'s "Resolution order":
//!
//! | # | Rung | Where it comes from |
//! |---|------|---------------------|
//! | 1 | [`WorkspaceRung::Flag`] | `-w` / `--workspace` |
//! | 2 | [`WorkspaceRung::Environment`] | `$CHARTER_WORKSPACE` |
//! | 3 | [`WorkspaceRung::Cwd`] | the tree the caller is standing in |
//! | 4 | [`WorkspaceRung::SessionPointer`] | `.charter/sessions/<sid>.workspace` |
//! | — | *the frame's launch record* | **not ported — see below** |
//! | 5 | [`WorkspaceRung::TerminalPointer`] | `.charter/terminals/<tid>.workspace` |
//! | 6 | [`WorkspaceRung::DeclaredDefault`] | `workspaces/.default` |
//! | 7 | [`WorkspaceRung::PlaneDefault`] | `[workspace] default` in `charter.toml` |
//! | 8 | [`WorkspaceRung::BuiltIn`] | the literal `default` |
//!
//! The cwd sits above the pointers because it cannot be wrong: a workspace's trees live at
//! paths that name the workspace, so being inside one is not a hint, it is the fact. The
//! pointers are for the case with no tree to stand in.
//!
//! **The frame's launch record (`.charter/frame/<fid>/workspace`) is deliberately absent**,
//! and this is the one place the two implementations answer differently on purpose.
//! `docs/plane-format.md` rules that `.charter/frame/**` is the tmux frame's and that the app
//! "neither reads nor writes there" — the app replaces that frame rather than inheriting its
//! state. A rung nothing on this side can ever set is a rung nobody can reason about, so it
//! is named here, left out, and pinned by the differential scenario
//! `workspace-the-frames-launch-record-is-a-rung-in-python-and-not-here` — which FAILS the
//! day the two stop differing, so the note cannot outlive the divergence it records.
//!
//! # The persona ladder
//!
//! `charter/persona.py:1379` (`_resolved`), seven rungs and then nothing:
//!
//! | # | Rung | Where it comes from |
//! |---|------|---------------------|
//! | 1 | [`PersonaRung::Flag`] | `--persona` |
//! | 2 | [`PersonaRung::Environment`] | `$CHARTER_PERSONA` |
//! | 3 | [`PersonaRung::SessionPointer`] | `.charter/sessions/<sid>.persona` |
//! | 4 | [`PersonaRung::TerminalPointer`] | `.charter/terminals/<tid>.persona` |
//! | 5 | [`PersonaRung::ActiveFile`] | `.charter/active-persona` |
//! | 6 | [`PersonaRung::PlaneDefault`] | `[persona] default` in `charter.toml` |
//! | 7 | [`PersonaRung::CommittedDefault`] | `personas/.default` |
//! | — | [`PersonaRung::Nothing`] | no persona, which is a real answer |
//!
//! The two committed rungs name only a persona that EXISTS; a rung above them naming one that
//! does not still wins, and the session has no persona rather than the plane's default. That
//! asymmetry is charter's (`charter/persona.py:1038`) and is what makes a pointer left behind
//! by `persona remove` visible instead of silently replaced.
//!
//! # Every name read off disk is checked before it is joined onto a path
//!
//! `workspace_dir()` is `workspaces/` joined with whatever resolved, so `../../esc` in a rung
//! reaches outside the plane — charter#442, on the rungs that were not gated. Here **every**
//! rung is name-checked, the two local pointer files included.
//!
//! **That is one step stricter than Python**, deliberately and with the divergence declared:
//! `workspace.chosen` and `persona._resolved` read the session and terminal pointers through
//! `_read`/`_read_pointer` and hand the value back unchecked, while their own public twins
//! (`workspace.for_session`, `persona.for_session`) name-check and say in their docstrings
//! that every rung does. Filed upstream as charter#1146. Nothing here depends on that being
//! fixed: a pointer charter itself wrote always passes.

use std::path::{Path, PathBuf};

use crate::contain;

/// `$CHARTER_WORKSPACE` — the per-session pin a launcher hands a chat.
pub const WORKSPACE_ENV: &str = "CHARTER_WORKSPACE";

/// `$CHARTER_SESSION_ID` — what the pointer rungs are keyed on, and the variable a LAUNCHER
/// sets so that every process it starts answers [`session_id`] identically.
///
/// Named here rather than spelled out at each side, because the reader is this module and the
/// writer is whatever launched the chat — the tmux frame in Python (`-e
/// CHARTER_SESSION_ID=<chat id>`), and `app/src-tauri/src/sessions.rs` here. Two string
/// literals in two crates is how the app came to set `CHARTER_CHAT` and nothing charter reads
/// (charter-app#63).
pub const SESSION_ID_ENV: &str = "CHARTER_SESSION_ID";

/// `$CLAUDE_CODE_SESSION_ID` — the harness's own id for the CONVERSATION, one rung under
/// [`SESSION_ID_ENV`].
///
/// A conversation is not a chat: `/clear` ends one and starts another inside the same chat,
/// in the same terminal, under the same program. Anything keyed on this is therefore keyed on
/// something that changes under a chat that did not move, which is charter-app#63 exactly.
pub const CONVERSATION_ENV: &str = "CLAUDE_CODE_SESSION_ID";

/// `$CHARTER_PERSONA` — the same, for the identity a chat adopts.
pub const PERSONA_ENV: &str = "CHARTER_PERSONA";

/// The workspace every plane falls back to, whether or not its directory is there.
pub const BUILT_IN_WORKSPACE: &str = "default";

/// The most a rung's file may hold before charter stops reading it — `contain.MAX_BYTES`.
///
/// A rung holds one name, so a cap of a few hundred bytes would do; it is charter's 1 MiB
/// because the two implementations must refuse the same files, and a file of padding with a
/// legal name inside it is legal to Python.
const MAX_RUNG_BYTES: u64 = 1_048_576;

// --------------------------------------------------------------------------------------- //
// who is asking                                                                             //
// --------------------------------------------------------------------------------------- //

/// The two ids the pointer rungs are keyed on — `charter/session.py`, which owns both.
///
/// Held together because "who is this session" and "which pane is it in" are the two halves
/// of one question, and charter's own module exists because that question once had three
/// disagreeing answers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ids {
    /// This session's id, or `None` when there is not one. Absence is representable on
    /// purpose: charter's `NO_SESSION` sentinel is a SHARED key, so every session without an
    /// id would write into one bucket.
    pub session: Option<String>,
    /// A stable id for the current terminal PANE, or `None`.
    pub terminal: Option<String>,
}

impl Ids {
    /// Both ids, read from this process's environment.
    pub fn from_env() -> Self {
        Self {
            session: session_id(&from_env),
            terminal: terminal_id(&from_env, tty_name),
        }
    }

    /// Both ids from a named environment, for a test and for a process asking on behalf of
    /// another. Never reads the real environment and never asks for a tty.
    pub fn of(env: &dyn Fn(&str) -> Option<String>) -> Self {
        Self {
            session: session_id(env),
            terminal: terminal_id(env, || None),
        }
    }
}

fn from_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// `charter/session.py:current` — `$CHARTER_SESSION_ID`, then `$CLAUDE_CODE_SESSION_ID`.
///
/// **The second rung is not dead code and was twice documented as such.** Measured against
/// Claude Code through a real `statusLine` command, the environment arrives intact and
/// carries `CLAUDE_CODE_SESSION_ID`; measured again here from an ordinary tool call, it is
/// still there. Inside a charter frame `$CHARTER_SESSION_ID` holds the FRAME's id and
/// SHADOWS it, which is what makes every process in that frame answer identically.
///
/// The value becomes a filename, so everything outside `[A-Za-z0-9._-]` is **deleted** —
/// charter's `_SAFE.sub("", …)`, and not the `-` substitution [`terminal_id`] uses. Two
/// different rules, kept apart because charter keeps them apart.
///
/// Public because [`crate::trace::bucket`] is the same question with a sentinel instead of
/// `None`, and it had its own copy of this until M2.9 put the two side by side.
pub fn session_id(env: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    // `or` on the RAW value, exactly as Python's `a or b` does: a `$CHARTER_SESSION_ID` of
    // whitespace is truthy, so it is taken and then sanitised to nothing — it does NOT fall
    // through to Claude Code's id. Stripping before the choice would change which variable
    // decides.
    let raw = env(SESSION_ID_ENV)
        .filter(|v| !v.is_empty())
        .or_else(|| env(CONVERSATION_ENV).filter(|v| !v.is_empty()))?;
    let id: String = py_strip(&raw)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    (!id.is_empty()).then_some(id)
}

/// Environment variables that identify ONE PANE — one shell, so at most one session.
///
/// `WINDOWID` is deliberately absent and used to sit in this list. It identifies a *window*,
/// and a window holds many tabs and splits, so every session in it wrote and read one
/// another's pointer: two sessions in one window, one runs `ws use user-reporting`, and the
/// other silently moves with it. An id that is wrong in the sharing direction is worse than
/// no id.
const PANE_ID_VARS: [&str; 4] = ["TERM_SESSION_ID", "TMUX_PANE", "STY", "SSH_TTY"];

/// `charter/session.py:terminal` — the pane, which survives closing and reopening the harness.
///
/// The `ttyname(0)` fallback is charter's last resort and is ported with it, because it is the
/// rung that fires **under the app**: the app gives each chat a pty of its own and sets none
/// of the four variables above, so this is the only thing that answers there. It answers with
/// a device name the kernel recycles, which is the sharing hazard `WINDOWID` was removed for,
/// one layer down — see the PR for what the app should write instead.
fn terminal_id(
    env: &dyn Fn(&str) -> Option<String>,
    tty: fn() -> Option<String>,
) -> Option<String> {
    let raw = PANE_ID_VARS
        .iter()
        .find_map(|name| env(name).filter(|v| !v.is_empty()))
        .or_else(tty)?;
    let id: String = py_strip(&raw)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    (!id.is_empty()).then_some(id)
}

/// The controlling terminal's device name, when stdin is one.
#[cfg(unix)]
fn tty_name() -> Option<String> {
    rustix::termios::ttyname(std::io::stdin(), Vec::new())
        .ok()
        .and_then(|name| name.into_string().ok())
}

#[cfg(not(unix))]
fn tty_name() -> Option<String> {
    None
}

/// Python's `str.strip()`: every Unicode whitespace character, from both ends.
fn py_strip(text: &str) -> &str {
    crate::memstore::py_strip(text)
}

// --------------------------------------------------------------------------------------- //
// the workspace ladder                                                                      //
// --------------------------------------------------------------------------------------- //

/// Which rung of [`workspace`] decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceRung {
    /// `-w` / `--workspace`.
    Flag,
    /// `$CHARTER_WORKSPACE`.
    Environment,
    /// The tree the caller is standing in.
    Cwd,
    /// `.charter/sessions/<sid>.workspace`.
    SessionPointer,
    /// `.charter/terminals/<tid>.workspace`.
    TerminalPointer,
    /// `workspaces/.default`, nominated by `charter workspace default`.
    DeclaredDefault,
    /// `[workspace] default` in `charter.toml`.
    PlaneDefault,
    /// The literal `default`, which is nobody's decision.
    BuiltIn,
}

impl WorkspaceRung {
    /// How charter names this rung on a status line — `charter/workspace.py:652` (`source`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Flag => "--workspace",
            Self::Environment => "$CHARTER_WORKSPACE",
            Self::Cwd => "cwd",
            Self::SessionPointer => "session",
            Self::TerminalPointer => "terminal",
            Self::DeclaredDefault => "declared default",
            Self::PlaneDefault | Self::BuiltIn => "default (nothing selected)",
        }
    }
}

/// The sentence `charter status` prints for "where did this workspace come from" —
/// `charter/workspace.py:source`, whose last two lines [`WorkspaceRung::label`] cannot
/// reach on its own.
///
/// **The last rung says WHY nothing answered, not just that nothing did.** A shell with no
/// pane id has no terminal pointer to fall back on, so every session in it starts on
/// `default` however many times the operator picks — and the operator's own complaint was
/// "why are you in default workspace again?", asked of a surface that asserted an answer
/// with no reason (ADR 0013's second rule). The two spellings are told apart by the pane id
/// and by nothing else, which is exactly what Python's `if not _terminal_id()` asks.
pub fn workspace_source(ids: &Ids, rung: WorkspaceRung) -> String {
    match rung {
        WorkspaceRung::PlaneDefault | WorkspaceRung::BuiltIn if ids.terminal.is_none() => {
            "default (no pane id — nothing persists between sessions)".to_string()
        }
        other => other.label().to_string(),
    }
}

/// The workspace a command acts on, and which rung said so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveWorkspace {
    pub name: String,
    pub rung: WorkspaceRung,
}

/// Everything the ladders read that is not a file: the flags this invocation carries and the
/// environment they sit in.
///
/// Passed rather than read, so a test drives every rung and so the one caller that is not the
/// session being described — a renderer answering for a payload's cwd — can say whose
/// directory the cwd rung should read.
pub struct Asking<'a> {
    /// The plane being acted on.
    pub root: &'a Path,
    /// Whose working directory the cwd rung reads.
    pub cwd: &'a Path,
    /// `-w` / `--persona` as typed, or `None`.
    pub flag: Option<&'a str>,
    /// The session and pane this is running in.
    pub ids: &'a Ids,
    /// `$CHARTER_WORKSPACE` / `$CHARTER_PERSONA`, exactly as the environment holds it.
    pub env: Option<&'a str>,
}

/// The active workspace, by precedence, with the built-in underneath — Python's `resolve`.
pub fn workspace(asking: &Asking) -> ActiveWorkspace {
    if let Some(chosen) = chosen_workspace(asking) {
        return chosen;
    }
    // `[workspace] default` and the literal `default` are one rung to charter — `source` says
    // "default (nothing selected)" for both — and two here, because only one of them is a
    // line somebody wrote in a committed file, and a caller deciding whether to ask has to be
    // able to tell them apart.
    match declared_plane_default(asking.root) {
        Some(name) => ActiveWorkspace {
            name,
            rung: WorkspaceRung::PlaneDefault,
        },
        None => ActiveWorkspace {
            name: BUILT_IN_WORKSPACE.to_string(),
            rung: WorkspaceRung::BuiltIn,
        },
    }
}

/// The workspace something actually **chose**, or `None` when every rung came back empty —
/// Python's `chosen`, which is `resolve` minus its last rung.
///
/// The difference is not cosmetic: `None` means nobody decided, which is the launch worth
/// interrupting with a picker. One ladder asked twice, never two ladders that agree today.
pub fn chosen_workspace(asking: &Asking) -> Option<ActiveWorkspace> {
    let at = |name: String, rung| Some(ActiveWorkspace { name, rung });
    if let Some(flag) = asking.flag.filter(|f| !f.is_empty()) {
        return at(flag.to_string(), WorkspaceRung::Flag);
    }
    if let Some(named) = asking.env.map(py_strip).filter(|v| !v.is_empty()) {
        return at(named.to_string(), WorkspaceRung::Environment);
    }
    if let Some(here) = workspace_of_tree(asking.root, asking.cwd) {
        return at(here, WorkspaceRung::Cwd);
    }
    if let Some(name) = pointer(
        asking.root,
        "sessions",
        asking.ids.session.as_deref(),
        "workspace",
    )
    .filter(|n| contain::workspace_name_ok(n))
    {
        return at(name, WorkspaceRung::SessionPointer);
    }
    if let Some(name) = pointer(
        asking.root,
        "terminals",
        asking.ids.terminal.as_deref(),
        "workspace",
    )
    .filter(|n| contain::workspace_name_ok(n))
    {
        return at(name, WorkspaceRung::TerminalPointer);
    }
    if let Some(name) = declared_default_workspace(asking.root) {
        return at(name, WorkspaceRung::DeclaredDefault);
    }
    None
}

/// The workspace whose working tree `cwd` is inside, or `None` — `workspace.from_path`.
///
/// `workspaces/<ws>` alone is the CONTAINER and not a tree, so only a path with something
/// under the workspace counts. That covers a clone (`workspaces/<ws>/<repo>/…`) and a
/// worktree at the default root (`workspaces/<ws>/.worktrees/<repo>/<piece>/…`) with one
/// walk, because both live under the workspace's own directory.
///
/// **A plane that moves its worktree root with `[plane] worktrees` is not covered here**, and
/// Python's is (`worktree.locate` tries both roots). charter-app refuses a relocated root
/// outright (`worktree::relocation_refusal`), so a piece cut under one cannot exist on this
/// side to be stood in; the day that refusal lifts, this walk has to grow the second root
/// with it.
pub fn workspace_of_tree(root: &Path, cwd: &Path) -> Option<String> {
    let workspaces = root.join("workspaces");
    // Both sides resolved: on macOS a plane under `/var/folders` is reached through a link to
    // `/private/var`, and comparing the two as text answers "outside" for a path plainly
    // inside. `cwd` came off the process, which has already walked the links.
    let base = std::fs::canonicalize(&workspaces).unwrap_or(workspaces);
    let here = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let rest = here.strip_prefix(&base).ok()?;
    let mut parts = rest.components();
    let name = parts.next()?.as_os_str().to_str()?.to_string();
    // Something under it, or this is the container.
    parts.next()?;
    contain::workspace_name_ok(&name).then_some(name)
}

/// `workspaces/.default` — the workspace `charter workspace default` nominated.
///
/// Committed, and therefore a teammate's, so it is read through the same gate every other
/// committed file charter takes a name out of goes through. NOT the "last active workspace"
/// pointer charter#124 rejected: that one is written by every `workspace use` and changes
/// under sessions that never asked; this is set once by a human and read only when every
/// other rung has missed.
pub fn declared_default_workspace(root: &Path) -> Option<String> {
    let name = committed_name(root, &root.join("workspaces").join(".default"))?;
    contain::workspace_name_ok(&name).then_some(name)
}

/// `[workspace] default` as the manifest holds it, or `None` — `instance.default_workspace_of`.
fn declared_plane_default(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join(crate::plane::MANIFEST)).ok()?;
    let doc: toml::Table = text.parse().ok()?;
    let named = python_str(doc.get("workspace")?.as_table()?.get("default")?)?;
    let named = py_strip(&named).to_string();
    contain::workspace_name_ok(&named).then_some(named)
}

/// A scalar TOML value as Python's `str()` renders it, or `None` for a table or an array.
///
/// A hand-edited manifest can put anything where a name goes, and charter takes `str(val)`
/// of a `str`, an `int` or a `float` and `""` of everything else. Ported because the name
/// check is what refuses these, and refusing them for a DIFFERENT reason in the two
/// implementations is how they come to disagree about the same file.
///
/// A TOML boolean is `bool` to `tomllib`, and `isinstance(True, int)` is true in Python, so
/// `default = true` reads as the name `True` there. Faithful, not endorsed.
fn python_str(value: &toml::Value) -> Option<String> {
    Some(match value {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(n) => n.to_string(),
        toml::Value::Boolean(true) => "True".to_string(),
        toml::Value::Boolean(false) => "False".to_string(),
        toml::Value::Float(f) => {
            // Python's `str(float)` is the shortest round-tripping form and always carries a
            // decimal point or an exponent; Rust's `{}` drops the `.0` from an integral one.
            let rendered = format!("{f}");
            if f.is_finite() && !rendered.contains(['.', 'e', 'E']) {
                format!("{rendered}.0")
            } else {
                rendered
            }
        }
        _ => return None,
    })
}

/// `[workspace] default`, falling back to the literal `default`.
///
/// Validated, because `charter.toml` is committed: the value is joined onto `workspaces/` by
/// every command that writes, and `default = "../../esc"` in a teammate's manifest made
/// `workspace vision` read a file the plane does not contain (charter#442). A value charter
/// disagrees with degrades to the fallback rather than raising — a manifest charter cannot
/// make sense of never stops charter running.
pub fn plane_default_workspace(root: &Path) -> String {
    declared_plane_default(root).unwrap_or_else(|| BUILT_IN_WORKSPACE.to_string())
}

// --------------------------------------------------------------------------------------- //
// the persona ladder                                                                        //
// --------------------------------------------------------------------------------------- //

/// Which rung of [`persona`] decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonaRung {
    /// `--persona`.
    Flag,
    /// `$CHARTER_PERSONA`.
    Environment,
    /// `.charter/sessions/<sid>.persona`.
    SessionPointer,
    /// `.charter/terminals/<tid>.persona`.
    TerminalPointer,
    /// `.charter/active-persona`, written by `charter persona use`.
    ActiveFile,
    /// `[persona] default` in `charter.toml`.
    PlaneDefault,
    /// `personas/.default`, the legacy committed default.
    CommittedDefault,
    /// No rung named anything, which is a real answer: a plane may have no front door, and
    /// charter inventing one would be it choosing an identity nobody asked for.
    Nothing,
}

impl PersonaRung {
    /// How charter names this rung — `charter/persona.py:1379`'s second return value.
    pub fn label(self) -> &'static str {
        match self {
            Self::Flag => "--persona",
            Self::Environment => "$CHARTER_PERSONA",
            Self::SessionPointer => "session",
            Self::TerminalPointer => "terminal",
            Self::ActiveFile => "active-file",
            Self::PlaneDefault => "charter.toml",
            Self::CommittedDefault => "committed-default",
            Self::Nothing => "none",
        }
    }
}

/// What the persona ladder decided: the name, or `None`, and the rung that said so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivePersona {
    pub name: Option<String>,
    pub rung: PersonaRung,
}

/// The active persona, by precedence — Python's `persona._resolved`, decided ONCE.
///
/// One walk and not two: `resolve_active` and `source` are two questions about a single
/// decision, and walking the rungs separately is how a session comes to adopt a persona while
/// reporting it came from nowhere.
pub fn persona(asking: &Asking) -> ActivePersona {
    let at = |name: String, rung| ActivePersona {
        name: Some(name),
        rung,
    };
    if let Some(flag) = asking.flag.filter(|f| !f.is_empty()) {
        return at(flag.to_string(), PersonaRung::Flag);
    }
    if let Some(named) = asking.env.map(py_strip).filter(|v| !v.is_empty()) {
        return at(named.to_string(), PersonaRung::Environment);
    }
    if let Some(name) = pointer(
        asking.root,
        "sessions",
        asking.ids.session.as_deref(),
        "persona",
    )
    .filter(|n| contain::persona_name_ok(n))
    {
        return at(name, PersonaRung::SessionPointer);
    }
    if let Some(name) = pointer(
        asking.root,
        "terminals",
        asking.ids.terminal.as_deref(),
        "persona",
    )
    .filter(|n| contain::persona_name_ok(n))
    {
        return at(name, PersonaRung::TerminalPointer);
    }
    if let Some(name) = local_file(asking.root, &state_dir(asking.root).join("active-persona"))
        .filter(|n| contain::persona_name_ok(n))
    {
        return at(name, PersonaRung::ActiveFile);
    }
    // The two committed rungs, and the ONLY two, name a persona that exists. A rung above
    // them naming one that does not still wins and the session has no persona — that is
    // charter's rule, and it is what makes a pointer `persona remove` left behind visible.
    if let Some(name) = declared_plane_persona(asking.root) {
        return at(name, PersonaRung::PlaneDefault);
    }
    if let Some(name) = committed_default_persona(asking.root) {
        return at(name, PersonaRung::CommittedDefault);
    }
    ActivePersona {
        name: None,
        rung: PersonaRung::Nothing,
    }
}

/// `[persona] default`, when it names a persona this plane defines.
///
/// A declaration naming a persona that was renamed or deleted resolves to *nothing* rather
/// than to a broken identity. Saying so out loud is `doctor`'s job; silence here is the
/// fail-toward-no-change half.
fn declared_plane_persona(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join(crate::plane::MANIFEST)).ok()?;
    let doc: toml::Table = text.parse().ok()?;
    let named = python_str(doc.get("persona")?.as_table()?.get("default")?)?;
    let named = py_strip(&named).to_string();
    defined_persona(root, &named)
}

/// `personas/.default` — the committed, team-wide default, when it names a persona that is
/// still there.
fn committed_default_persona(root: &Path) -> Option<String> {
    let named = committed_name(root, &root.join("personas").join(".default"))?;
    defined_persona(root, &named)
}

/// `named`, if this plane has a definition file for it — `persona.def_path(name).exists()`.
///
/// The name check comes BEFORE the existence test, and that order is charter#337: "a path
/// that exists" was never the question, and a reference climbing out of `personas/` named a
/// file the plane does not contain whose `vault:`, `role:` and `tools:` were then merged.
fn defined_persona(root: &Path, named: &str) -> Option<String> {
    if !contain::persona_name_ok(named) || named == contain::SHARED_PERSONA {
        return None;
    }
    let dir = root.join("personas").join(named).join("persona.md");
    let flat = root.join("personas").join(format!("{named}.md"));
    // The entry that is OPENED is the one gated, never `personas/` above it: a persona
    // directory that is a link out of the plane has a perfectly legal name.
    let there = |path: &Path| contain::readable(root, path).is_ok() && path.exists();
    (there(&dir) || there(&flat)).then(|| named.to_string())
}

// --------------------------------------------------------------------------------------- //
// reading one rung off disk                                                                 //
// --------------------------------------------------------------------------------------- //

/// The plane's machine-local state directory.
///
/// `$CHARTER_HOME` moves it in Python (`config._migrate_state_dir`) and is not read anywhere
/// in this binary — `cistate`, `profiletrust`, `reopen` and `hookwire` all join `.charter`
/// onto the plane. One spelling here too, rather than a second answer for these two rungs.
fn state_dir(root: &Path) -> PathBuf {
    root.join(".charter")
}

/// A pointer file's contents, or `None` — `.charter/<dir>/<id>.<ext>`.
///
/// `id` is checked as a path SEGMENT before it is joined: it comes from the environment
/// (`$CHARTER_SESSION_ID`, `$TMUX_PANE`), the sanitiser above only removes characters charter
/// dislikes, and neither `.` nor `..` has anything removed from it.
fn pointer(root: &Path, dir: &str, id: Option<&str>, ext: &str) -> Option<String> {
    let id = id.filter(|id| contain::segment_ok(id))?;
    local_file(root, &state_dir(root).join(dir).join(format!("{id}.{ext}")))
}

/// One short line out of a file under `.charter/`, or `None`.
///
/// `contain::readable` is the WRONG gate here and would refuse every one of these: `.charter/`
/// is not one of the plane's data directories. The right one is the walk `reopen`, `hookwire`
/// and `cistate` already use for charter's own paths, asked of the FILE and not of the
/// directory above it — a link at `<sid>.workspace` redirects the read exactly as one at
/// `sessions/` does, and a check on the parent cannot see it.
///
/// **Python does not gate these two rungs at all** (`workspace._read` is a bare `read_text`),
/// so a link here is followed there and refused here. Declared in this module's header and
/// filed as charter#1146; a pointer charter wrote is never a link, so nothing honest changes.
fn local_file(root: &Path, path: &Path) -> Option<String> {
    contain::no_link_on_the_way(root, path).ok()?;
    // `false`: the walk has already refused every link on the way, so there is nothing to
    // follow and asking the kernel to would undo it.
    read_a_name(path, false)
}

/// One short line out of a COMMITTED dotfile, or `None`.
///
/// Gated by `contain::readable`, which resolves and therefore follows a link that lands back
/// inside the plane — Python's `realpath` follows it too, and a plane that links a persona
/// directory depends on that. The file is committed, so the value is a teammate's.
fn committed_name(root: &Path, path: &Path) -> Option<String> {
    contain::readable(root, path).ok()?;
    // `true`: the gate above resolves and has already decided this link lands inside the
    // plane, so what is asked here is about the file at the END of it — which is the order
    // `contain._path_refusal` keeps, and the reason it stats twice.
    read_a_name(path, true)
}

/// The stripped contents of a small regular file, or `None` for anything else.
///
/// **A `stat`, and two questions charter's `contain.file_refusal` asks with it.** Is this a
/// regular file — a FIFO committed at `workspaces/.default` would hang the read for ever and
/// a device would never end — and is it a sane size. These rungs are read on the path a
/// status line takes every turn, where a guard that has to OPEN something is a guard that can
/// block; `stat` answers both without opening anything, so there is nothing to time out.
fn read_a_name(path: &Path, follow: bool) -> Option<String> {
    let found = if follow {
        std::fs::metadata(path).ok()?
    } else {
        std::fs::symlink_metadata(path).ok()?
    };
    if !found.is_file() || found.len() > MAX_RUNG_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let name = py_strip(&text);
    (!name.is_empty()).then(|| name.to_string())
}

/// `$CHARTER_WORKSPACE`/`$CHARTER_PERSONA` is set and holds only whitespace, so the ladder
/// ignored it — charter#1055 and charter#1048.
///
/// Worth saying because the operator's export is broken and resolution going on through the
/// rungs below hides that from them. `export CHARTER_WORKSPACE=$(…)` over a command that
/// printed only a space leaves exactly this. **Empty is not reported**: it is what a launcher
/// hands every chat when the launch pinned nothing, so it would be said in every chat about
/// an export nobody wrote.
pub fn blank_in_environment(value: Option<&str>) -> bool {
    matches!(value, Some(v) if !v.is_empty() && py_strip(v).is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;

    /// A plane with every rung of both ladders set to a DIFFERENT value, so removing one
    /// proves which of the others was underneath it. A rung that names the same workspace as
    /// the rung below proves nothing at all.
    struct Rig {
        dir: tempfile::TempDir,
    }

    impl Rig {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path();
            fs::write(
                root.join("charter.toml"),
                "[workspace]\ndefault = \"w-plane\"\n\n[persona]\ndefault = \"p-plane\"\n",
            )
            .unwrap();
            fs::create_dir_all(root.join("workspaces/w-cwd/repo")).unwrap();
            fs::create_dir_all(root.join(".charter/sessions")).unwrap();
            fs::create_dir_all(root.join(".charter/terminals")).unwrap();
            fs::write(root.join("workspaces/.default"), "w-declared\n").unwrap();
            fs::write(root.join(".charter/sessions/sid.workspace"), "w-session\n").unwrap();
            fs::write(
                root.join(".charter/terminals/tid.workspace"),
                "w-terminal\n",
            )
            .unwrap();
            for who in [
                "p-plane",
                "p-committed",
                "p-session",
                "p-terminal",
                "p-active",
            ] {
                fs::create_dir_all(root.join("personas").join(who)).unwrap();
                fs::write(
                    root.join("personas").join(who).join("persona.md"),
                    "# who\n",
                )
                .unwrap();
            }
            fs::write(root.join("personas/.default"), "p-committed\n").unwrap();
            fs::write(root.join(".charter/sessions/sid.persona"), "p-session\n").unwrap();
            fs::write(root.join(".charter/terminals/tid.persona"), "p-terminal\n").unwrap();
            fs::write(root.join(".charter/active-persona"), "p-active\n").unwrap();
            Self { dir }
        }

        fn root(&self) -> PathBuf {
            fs::canonicalize(self.dir.path()).unwrap()
        }

        fn ids(&self) -> Ids {
            Ids {
                session: Some("sid".to_string()),
                terminal: Some("tid".to_string()),
            }
        }
    }

    fn ask<'a>(root: &'a Path, cwd: &'a Path, ids: &'a Ids) -> Asking<'a> {
        Asking {
            root,
            cwd,
            flag: None,
            ids,
            env: None,
        }
    }

    // --- the workspace ladder, one rung at a time ------------------------------------- //

    #[test]
    fn the_flag_outranks_the_environment_the_tree_and_every_pointer() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        let cwd = root.join("workspaces/w-cwd/repo");

        let mut asking = ask(&root, &cwd, &ids);
        asking.flag = Some("w-flag");
        asking.env = Some("w-env");

        let found = workspace(&asking);
        assert_eq!(found.name, "w-flag");
        assert_eq!(found.rung, WorkspaceRung::Flag);
    }

    #[test]
    fn with_no_flag_the_environment_decides() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        let cwd = root.join("workspaces/w-cwd/repo");

        let mut asking = ask(&root, &cwd, &ids);
        asking.env = Some("w-env");

        let found = workspace(&asking);
        assert_eq!(found.name, "w-env");
        assert_eq!(found.rung, WorkspaceRung::Environment);
    }

    #[test]
    fn an_environment_holding_only_whitespace_is_unset_and_the_tree_decides() {
        // charter#1055: `export CHARTER_WORKSPACE=$(…)` over a command that printed a space.
        // Taken as a name it hid every rung below it and `source` named the variable as what
        // decided, so the operator saw their own choice reported over a broken export.
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        let cwd = root.join("workspaces/w-cwd/repo");

        let mut asking = ask(&root, &cwd, &ids);
        asking.env = Some("   ");

        assert_eq!(workspace(&asking).rung, WorkspaceRung::Cwd);
        assert!(blank_in_environment(Some("   ")));
        assert!(!blank_in_environment(Some("")));
        assert!(!blank_in_environment(None));
    }

    #[test]
    fn with_no_flag_and_no_environment_the_tree_you_stand_in_decides() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        let cwd = root.join("workspaces/w-cwd/repo");

        let found = workspace(&ask(&root, &cwd, &ids));
        assert_eq!(found.name, "w-cwd");
        assert_eq!(found.rung, WorkspaceRung::Cwd);
    }

    #[test]
    fn the_workspace_directory_itself_is_a_container_and_not_a_tree() {
        // A status line naming a workspace for a path with no repo in it would be claiming a
        // tree that is not there, so `workspaces/<ws>` falls through to the pointers.
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        let cwd = root.join("workspaces/w-cwd");

        let found = workspace(&ask(&root, &cwd, &ids));
        assert_eq!(found.name, "w-session");
        assert_eq!(found.rung, WorkspaceRung::SessionPointer);
    }

    #[test]
    fn with_no_tree_to_stand_in_the_sessions_own_pointer_decides() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());

        let found = workspace(&ask(&root, &root, &ids));
        assert_eq!(found.name, "w-session");
        assert_eq!(found.rung, WorkspaceRung::SessionPointer);
    }

    #[test]
    fn with_no_session_pointer_the_terminals_decides() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        fs::remove_file(root.join(".charter/sessions/sid.workspace")).unwrap();

        let found = workspace(&ask(&root, &root, &ids));
        assert_eq!(found.name, "w-terminal");
        assert_eq!(found.rung, WorkspaceRung::TerminalPointer);
    }

    #[test]
    fn a_session_with_no_id_reads_no_pointer_of_its_own() {
        // Absence is representable: the alternative charter had was a `nosession` sentinel,
        // a SHARED key that every id-less invocation wrote into.
        let rig = Rig::new();
        let root = rig.root();
        let ids = Ids {
            session: None,
            terminal: Some("tid".to_string()),
        };

        assert_eq!(workspace(&ask(&root, &root, &ids)).name, "w-terminal");
    }

    #[test]
    fn with_neither_pointer_the_nominated_default_decides() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());

        let found = workspace(&ask(&root, &root, &ids));
        assert_eq!(found.name, "w-declared");
        assert_eq!(found.rung, WorkspaceRung::DeclaredDefault);
    }

    #[test]
    fn with_nothing_nominated_the_planes_declared_default_decides() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());
        fs::remove_file(root.join("workspaces/.default")).unwrap();

        let found = workspace(&ask(&root, &root, &ids));
        assert_eq!(found.name, "w-plane");
        assert_eq!(found.rung, WorkspaceRung::PlaneDefault);
        // And it is not a CHOICE: the picker fires exactly where nobody decided.
        assert_eq!(chosen_workspace(&ask(&root, &root, &ids)), None);
    }

    #[test]
    fn with_a_plane_that_declares_nothing_the_answer_is_the_built_in() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());
        fs::remove_file(root.join("workspaces/.default")).unwrap();
        fs::write(root.join("charter.toml"), "").unwrap();

        let found = workspace(&ask(&root, &root, &ids));
        assert_eq!(found.name, "default");
        assert_eq!(found.rung, WorkspaceRung::BuiltIn);
    }

    // --- what a rung may not say -------------------------------------------------------- //

    #[test]
    fn a_nominated_default_that_climbs_out_of_the_plane_is_no_rung() {
        // charter#442: `workspaces/.default` is committable, so the value is a teammate's,
        // and `resolve()` hands whatever it returns to a join onto `workspaces/`.
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());
        fs::write(root.join("workspaces/.default"), "../../esc\n").unwrap();

        assert_eq!(workspace(&ask(&root, &root, &ids)).name, "w-plane");
    }

    #[test]
    fn a_plane_default_that_climbs_out_of_the_plane_is_no_rung() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());
        fs::remove_file(root.join("workspaces/.default")).unwrap();
        fs::write(
            root.join("charter.toml"),
            "[workspace]\ndefault = \"../../esc\"\n",
        )
        .unwrap();

        assert_eq!(workspace(&ask(&root, &root, &ids)).name, "default");
    }

    #[test]
    fn a_session_pointer_that_climbs_out_of_the_plane_is_no_rung() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        fs::write(root.join(".charter/sessions/sid.workspace"), "../../esc\n").unwrap();

        assert_eq!(workspace(&ask(&root, &root, &ids)).name, "w-terminal");
    }

    #[test]
    fn a_nominated_default_reached_through_a_link_out_of_the_plane_is_no_rung() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("elsewhere"), "w-smuggled\n").unwrap();
        let link = root.join("workspaces/.default");
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(outside.path().join("elsewhere"), &link).unwrap();

        assert_eq!(workspace(&ask(&root, &root, &ids)).name, "w-plane");
    }

    #[test]
    fn a_session_pointer_that_is_a_link_is_no_rung() {
        // charter's own path under `.charter/`, created by charter: a link there has no
        // honest use, and this is the gate `reopen`, `hookwire` and `cistate` already keep.
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("elsewhere"), "w-smuggled\n").unwrap();
        let link = root.join(".charter/sessions/sid.workspace");
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(outside.path().join("elsewhere"), &link).unwrap();

        assert_eq!(workspace(&ask(&root, &root, &ids)).name, "w-terminal");
    }

    #[test]
    fn an_id_that_is_not_a_path_segment_reads_no_pointer() {
        // The guard sits in FRONT of the join, not behind `session_id`'s sanitiser: `Ids` is
        // a public struct a caller can build — the app answering for another chat is exactly
        // that caller — so an id that never went through the sanitiser reaches the join.
        let rig = Rig::new();
        let root = rig.root();
        let ids = Ids {
            session: Some("..".to_string()),
            terminal: Some("../sessions/sid".to_string()),
        };

        assert_eq!(workspace(&ask(&root, &root, &ids)).name, "w-declared");
    }

    // --- the persona ladder, one rung at a time ----------------------------------------- //

    #[test]
    fn the_persona_flag_outranks_the_environment_and_every_pointer() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        let mut asking = ask(&root, &root, &ids);
        asking.flag = Some("p-flag");
        asking.env = Some("p-env");

        let found = persona(&asking);
        assert_eq!(found.name.as_deref(), Some("p-flag"));
        assert_eq!(found.rung, PersonaRung::Flag);
    }

    #[test]
    fn with_no_persona_flag_the_environment_decides() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        let mut asking = ask(&root, &root, &ids);
        asking.env = Some("p-env");

        let found = persona(&asking);
        assert_eq!(found.name.as_deref(), Some("p-env"));
        assert_eq!(found.rung, PersonaRung::Environment);
    }

    #[test]
    fn a_persona_environment_holding_only_whitespace_is_unset() {
        // charter#1048, the same defect one noun over.
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        let mut asking = ask(&root, &root, &ids);
        asking.env = Some(" \t ");

        assert_eq!(persona(&asking).rung, PersonaRung::SessionPointer);
    }

    #[test]
    fn with_no_flag_and_no_environment_the_sessions_pointer_decides() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());

        let found = persona(&ask(&root, &root, &ids));
        assert_eq!(found.name.as_deref(), Some("p-session"));
        assert_eq!(found.rung, PersonaRung::SessionPointer);
    }

    #[test]
    fn with_no_session_pointer_the_terminals_decides_for_a_persona_too() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        fs::remove_file(root.join(".charter/sessions/sid.persona")).unwrap();

        let found = persona(&ask(&root, &root, &ids));
        assert_eq!(found.name.as_deref(), Some("p-terminal"));
        assert_eq!(found.rung, PersonaRung::TerminalPointer);
    }

    #[test]
    fn with_neither_pointer_the_plane_wide_active_file_decides() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());

        let found = persona(&ask(&root, &root, &ids));
        assert_eq!(found.name.as_deref(), Some("p-active"));
        assert_eq!(found.rung, PersonaRung::ActiveFile);
    }

    #[test]
    fn with_nothing_selected_the_planes_declared_front_door_decides() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());
        fs::remove_file(root.join(".charter/active-persona")).unwrap();

        let found = persona(&ask(&root, &root, &ids));
        assert_eq!(found.name.as_deref(), Some("p-plane"));
        assert_eq!(found.rung, PersonaRung::PlaneDefault);
    }

    #[test]
    fn with_nothing_declared_the_legacy_committed_default_decides() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());
        fs::remove_file(root.join(".charter/active-persona")).unwrap();
        fs::write(
            root.join("charter.toml"),
            "[workspace]\ndefault = \"w-plane\"\n",
        )
        .unwrap();

        let found = persona(&ask(&root, &root, &ids));
        assert_eq!(found.name.as_deref(), Some("p-committed"));
        assert_eq!(found.rung, PersonaRung::CommittedDefault);
    }

    #[test]
    fn a_plane_with_no_front_door_has_no_persona_and_says_so() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());
        fs::remove_file(root.join(".charter/active-persona")).unwrap();
        fs::remove_file(root.join("personas/.default")).unwrap();
        fs::write(root.join("charter.toml"), "").unwrap();

        let found = persona(&ask(&root, &root, &ids));
        assert_eq!(found.name, None);
        assert_eq!(found.rung, PersonaRung::Nothing);
    }

    #[test]
    fn a_committed_rung_naming_a_persona_the_plane_does_not_have_is_no_rung() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());
        fs::remove_file(root.join(".charter/active-persona")).unwrap();
        fs::write(
            root.join("charter.toml"),
            "[persona]\ndefault = \"p-deleted\"\n",
        )
        .unwrap();

        // Falls to `personas/.default`, which names one that IS there.
        assert_eq!(
            persona(&ask(&root, &root, &ids)).name.as_deref(),
            Some("p-committed")
        );
    }

    #[test]
    fn a_pointer_naming_a_persona_the_plane_does_not_have_still_wins() {
        // The asymmetry is charter's and is the point: a pointer `persona remove` left
        // behind must be VISIBLE, not silently replaced by the plane's front door.
        let rig = Rig::new();
        let (root, ids) = (rig.root(), rig.ids());
        fs::write(root.join(".charter/sessions/sid.persona"), "p-deleted\n").unwrap();

        let found = persona(&ask(&root, &root, &ids));
        assert_eq!(found.name.as_deref(), Some("p-deleted"));
        assert_eq!(found.rung, PersonaRung::SessionPointer);
    }

    #[test]
    fn the_shared_store_is_not_a_front_door_any_committed_rung_may_name() {
        // `_shared` is the store every persona READS, not a persona anybody adopts, and
        // `contain::persona_name_ok` admits it by name for the callers that mean the store.
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());
        fs::remove_file(root.join(".charter/active-persona")).unwrap();
        fs::create_dir_all(root.join("personas/_shared")).unwrap();
        fs::write(root.join("personas/_shared/persona.md"), "# shared\n").unwrap();
        fs::write(
            root.join("charter.toml"),
            "[persona]\ndefault = \"_shared\"\n",
        )
        .unwrap();

        assert_eq!(
            persona(&ask(&root, &root, &ids)).name.as_deref(),
            Some("p-committed")
        );
    }

    #[test]
    fn a_legacy_flat_persona_still_answers_a_committed_rung() {
        let rig = Rig::new();
        let (root, ids) = (rig.root(), Ids::default());
        fs::remove_file(root.join(".charter/active-persona")).unwrap();
        fs::write(root.join("personas/p-flat.md"), "# flat\n").unwrap();
        fs::write(
            root.join("charter.toml"),
            "[persona]\ndefault = \"p-flat\"\n",
        )
        .unwrap();

        assert_eq!(
            persona(&ask(&root, &root, &ids)).name.as_deref(),
            Some("p-flat")
        );
    }

    #[test]
    fn every_rung_is_labelled_the_way_charter_labels_it() {
        // These are the words `workspace.source` and `persona._resolved` return, and they end
        // up on a status line beside the name. Pinned because the next caller (M2.2's
        // `recall`, and the app's own header) will print them, and a label that drifts
        // explains the active workspace by naming a rung that did not decide it.
        use PersonaRung as P;
        use WorkspaceRung as W;
        assert_eq!(
            [
                W::Flag,
                W::Environment,
                W::Cwd,
                W::SessionPointer,
                W::TerminalPointer,
                W::DeclaredDefault,
                W::PlaneDefault,
                W::BuiltIn,
            ]
            .map(W::label),
            [
                "--workspace",
                "$CHARTER_WORKSPACE",
                "cwd",
                "session",
                "terminal",
                "declared default",
                "default (nothing selected)",
                "default (nothing selected)",
            ]
        );
        assert_eq!(
            [
                P::Flag,
                P::Environment,
                P::SessionPointer,
                P::TerminalPointer,
                P::ActiveFile,
                P::PlaneDefault,
                P::CommittedDefault,
                P::Nothing,
            ]
            .map(P::label),
            [
                "--persona",
                "$CHARTER_PERSONA",
                "session",
                "terminal",
                "active-file",
                "charter.toml",
                "committed-default",
                "none",
            ]
        );
    }

    // --- the two ids -------------------------------------------------------------------- //

    fn env_of(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn reader(map: &HashMap<String, String>) -> impl Fn(&str) -> Option<String> + '_ {
        move |name: &str| map.get(name).cloned()
    }

    #[test]
    fn a_frames_session_id_shadows_the_harnesss_own() {
        let map = env_of(&[
            ("CHARTER_SESSION_ID", "alpha.1"),
            ("CLAUDE_CODE_SESSION_ID", "9f2c"),
        ]);
        assert_eq!(Ids::of(&reader(&map)).session.as_deref(), Some("alpha.1"));
    }

    #[test]
    fn claude_codes_own_id_answers_when_charter_sets_none() {
        let map = env_of(&[("CLAUDE_CODE_SESSION_ID", "9f2c-d1")]);
        assert_eq!(Ids::of(&reader(&map)).session.as_deref(), Some("9f2c-d1"));
    }

    #[test]
    fn a_charter_session_id_of_whitespace_does_not_fall_through_to_claude_codes() {
        // Python's `a or b` chooses on the RAW value and sanitises after, so whitespace in
        // the first variable is TAKEN and becomes nothing. Stripping before the choice would
        // silently key this session on a different conversation's id.
        let map = env_of(&[
            ("CHARTER_SESSION_ID", "  "),
            ("CLAUDE_CODE_SESSION_ID", "9f2c"),
        ]);
        assert_eq!(Ids::of(&reader(&map)).session, None);
    }

    #[test]
    fn a_session_id_becomes_a_filename_so_what_cannot_be_one_is_deleted() {
        let map = env_of(&[("CHARTER_SESSION_ID", "a/../b c")]);
        assert_eq!(Ids::of(&reader(&map)).session.as_deref(), Some("a..bc"));
    }

    #[test]
    fn a_pane_id_keeps_its_shape_and_replaces_what_it_cannot_keep() {
        // Replaced with `-`, never deleted: two panes whose ids differ only in a character
        // charter dislikes must not collapse onto one pointer.
        let map = env_of(&[("TMUX_PANE", "%3")]);
        assert_eq!(Ids::of(&reader(&map)).terminal.as_deref(), Some("-3"));
    }

    #[test]
    fn the_pane_variables_are_tried_in_charters_order() {
        let map = env_of(&[
            ("TERM_SESSION_ID", "w0t1p0"),
            ("TMUX_PANE", "%3"),
            ("STY", "1234.pts-0"),
        ]);
        assert_eq!(Ids::of(&reader(&map)).terminal.as_deref(), Some("w0t1p0"));
    }

    #[test]
    fn a_window_id_is_not_a_pane_id_and_names_no_terminal() {
        // It identifies a WINDOW, which holds many tabs and splits, so every session in it
        // wrote and read one another's pointer. Measured exactly that way.
        let map = env_of(&[("WINDOWID", "4194311")]);
        assert_eq!(Ids::of(&reader(&map)).terminal, None);
    }

    #[test]
    fn an_empty_pane_variable_is_skipped_for_the_next_one() {
        let map = env_of(&[("TERM_SESSION_ID", ""), ("TMUX_PANE", "%7")]);
        assert_eq!(Ids::of(&reader(&map)).terminal.as_deref(), Some("-7"));
    }
}
