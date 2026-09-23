//! The tool hooks other than the Bash guard — `charter/hooks.py`'s `pretooluse_read`,
//! `pretooluse_edit`, `pretooluse_dispatch` and the `posttooluse` family — and the persona
//! tool gate's place at the end of the Bash guard.
//!
//! Each handler takes the payload and the plane and returns an [`Answer`]: nothing, a line of
//! JSON for the harness, or a denial. The CLI prints it; nothing here touches stdout, so every
//! handler can be asked from a test with no process around it.
//!
//! # Exit 2 is a deliberate deny and nothing else
//!
//! A harness reads exit 2 from a `PreToolUse` hook as "block", so the only path to it is a
//! [`Answer::Deny`] whose JSON could not be written (`hooks.py:_deny`, charter#438). Every
//! other outcome — a handler with nothing to say, a payload that will not parse, a write that
//! failed — is exit 0 with nothing printed, which the harness reads as "no opinion".
//!
//! # The plane gate
//!
//! Every handler here but one is silent outside a control plane (charter#852): outside one
//! there is no workspace to nudge about, no persona to gate and no log to write. The one that
//! is not is `pretooluse-read`, the vault guard on `Read`/`Grep`, because it is the twin of the
//! Bash leak guard, which is ungated, and `$CHARTER_HOME` can put a real vault within reach of
//! a directory that holds no `charter.toml`.
//!
//! # What is not ported, and why
//!
//! - **The trace** (`_trace`): one JSONL row per verdict, nudge and dispatch under
//!   `.charter/persona-state/trace/`. It changes no verdict and nothing in charter-app reads it.
//! - **The ask marks** (`_ask_mark_set`, `_ask_approved`): they exist only to write the
//!   `…-approved` trace row when an asked tool call goes through.
//! - **The routing ask on `pretooluse-edit`** (`_route_mark_take`): it answers a mark
//!   `userpromptsubmit` sets when it shows the roster under `routing: require`, and charter-app's
//!   `userpromptsubmit` shows no roster, so the mark is never set.
//! - **The turn markers** (`_turn_begin`, `_turn_bump`, `_turn_end`) and `notify.plane_changed`:
//!   the tmux frame's spinner and repaint. The app has its own (`hookwire`).
//! - **`_record_reported_session`**: opencode's session report inside a tmux frame.
//! - **Committing the dispatch log** (`_commit_dispatch`): see [`crate::dispatch`].

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

use crate::hookstate::State;
use crate::toolgate::Verdict;
use crate::{dispatch, inflight, leakguard, personagate, personagrant, pieces, pypath};

/// What a handler decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Nothing to say: the harness carries on as it would have.
    Nothing,
    /// One line of JSON for the harness — context, an ask, or an allow.
    Say(String),
    /// A refusal. Printed as JSON; exit 2 only if that print fails.
    Deny(Verdict),
}

/// Everything a handler reads that is not a plane file.
pub struct Hook<'a> {
    /// The plane root when there is one, else the process's directory — `config.ROOT`.
    pub root: &'a Path,
    /// `charter.toml` is at `root` — `config.HAS_CONTROL_PLANE`, the MARKER and not a resolve.
    pub in_plane: bool,
    /// The harness's payload; `Null` when it would not parse.
    pub payload: &'a Value,
    /// The environment, as a lookup.
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// The process's directory — the ladders' cwd rung.
    pub cwd: &'a Path,
    pub now: DateTime<Utc>,
    /// This machine's name as the logs spell it ([`dispatch::host`]).
    pub host: &'a str,
}

impl Hook<'_> {
    /// `data.get(key) or ""` for a string field.
    fn text(&self, key: &str) -> &str {
        self.payload
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    fn tool_name(&self) -> &str {
        self.text("tool_name")
    }

    fn tool_input(&self) -> &Value {
        self.payload.get("tool_input").unwrap_or(&Value::Null)
    }

    /// `(tool_input.get(key) or "").strip()` for a string field.
    fn input_text(&self, key: &str) -> &str {
        crate::memstore::py_strip(
            self.tool_input()
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
    }

    /// The payload's `session_id`, as it came.
    fn session_id(&self) -> Option<&str> {
        self.payload.get("session_id").and_then(Value::as_str)
    }

    fn state(&self) -> State {
        State::of(self.root)
    }

    fn unattended(&self) -> bool {
        self.text("permission_mode") == crate::briefing::UNATTENDED_MODE
    }

    /// The active persona, by the ladder — `persona.resolve_active()`.
    pub fn persona(&self) -> Option<String> {
        let named = (self.env)(crate::active::PERSONA_ENV);
        crate::active::persona(&crate::active::Asking {
            root: self.root,
            cwd: self.cwd,
            flag: None,
            ids: &crate::active::Ids::of(self.env),
            env: named.as_deref(),
        })
        .name
    }

    /// `_workspace_session`: the chat's id inside the app, the payload's outside it.
    fn workspace_session(&self) -> Option<String> {
        (self.env)(crate::active::SESSION_ID_ENV)
            .filter(|v| !v.is_empty())
            .or_else(|| self.session_id().map(str::to_owned))
    }

    /// `workspace.resolve(session_id=…)`.
    fn workspace(&self, session: Option<&str>) -> String {
        let mut ids = crate::active::Ids::of(self.env);
        ids.session = crate::hookstate::session(session, self.env);
        let pinned = (self.env)(crate::active::WORKSPACE_ENV);
        crate::active::workspace(&crate::active::Asking {
            root: self.root,
            cwd: self.cwd,
            flag: None,
            ids: &ids,
            env: pinned.as_deref(),
        })
        .name
    }

    /// `_touch_piece`: the worker in the payload's `cwd` is alive.
    pub fn touch_piece(&self) {
        let cwd = self.text("cwd");
        if !self.in_plane || cwd.is_empty() {
            return;
        }
        let persona = self.persona();
        pieces::touch(
            self.root,
            Path::new(cwd),
            self.session_id(),
            persona.as_deref(),
            self.now,
        );
    }

    fn unix_now(&self) -> f64 {
        self.now.timestamp() as f64 + f64::from(self.now.timestamp_subsec_nanos()) / 1e9
    }
}

/// Where each file tool carries its path — `_PATH_KEYS`. Both spellings, because the key is
/// the HARNESS's: Claude Code's `file_path`, opencode's `filePath`.
pub const PATH_KEYS: [&str; 5] = [
    "file_path",
    "path",
    "notebook_path",
    "filePath",
    "notebookPath",
];

/// Python's truth of a JSON value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
    }
}

/// `[str(ti[k]) for k in _PATH_KEYS if ti.get(k)]`.
fn targets(input: &Value) -> Vec<String> {
    PATH_KEYS
        .iter()
        .filter_map(|k| {
            let v = input.get(*k);
            truthy(v).then(|| crate::pyrepr::str_json(v.unwrap_or(&Value::Null)))
        })
        .collect()
}

/// A `PreToolUse` refusal with charter's words around it.
fn deny(reason: String) -> Answer {
    Answer::Deny(Verdict {
        reason: reason.chars().take(70).collect(),
        shape: None,
        denial: reason,
    })
}

/// `hookSpecificOutput` for `event`, as `json.dumps` writes it.
fn say(event: &str, fields: &[(&str, String)]) -> Answer {
    let mut inner = serde_json::Map::new();
    inner.insert("hookEventName".into(), Value::String(event.into()));
    for (k, v) in fields {
        inner.insert((*k).into(), Value::String(v.clone()));
    }
    let mut outer = serde_json::Map::new();
    outer.insert("hookSpecificOutput".into(), Value::Object(inner));
    Answer::Say(crate::pyjson::dumps(
        &Value::Object(outer),
        None,
        ", ",
        ": ",
    ))
}

/// `_ask`: ask the operator — or, in an unattended run with nobody to ask, say it and allow.
fn ask(hook: &Hook, reason: &str) -> Answer {
    if hook.unattended() {
        return say(
            "PreToolUse",
            &[
                ("permissionDecision", "allow".into()),
                (
                    "permissionDecisionReason",
                    format!("charter nudge (unattended, not blocking): {reason}"),
                ),
            ],
        );
    }
    say(
        "PreToolUse",
        &[
            ("permissionDecision", "ask".into()),
            (
                "permissionDecisionReason",
                format!("charter nudge: {reason}"),
            ),
        ],
    )
}

// ---- PreToolUse ---------------------------------------------------------------------------

/// `pretooluse_read`: refuse a `Read`/`Grep` that would print a vault's plaintext into the
/// transcript. **Not plane-gated** — see the module header.
pub fn pretooluse_read(hook: &Hook) -> Answer {
    hook.touch_piece();
    let tool = hook.tool_name();
    if tool != "Read" && tool != "Grep" {
        return Answer::Nothing;
    }
    let input = hook.tool_input();
    let named = targets(input);
    // `_names_a_vault_path` and nothing else — the Bash route's predicate, with no private
    // half of the answer here (#462).
    if named.iter().any(|t| leakguard::names_a_vault_path(t)) {
        return deny(leakguard::READ_REASON.to_string());
    }
    // `Grep` walks; `Read` opens one file. A `Grep` with no path searches the directory it
    // stands in, which is the commonest spelling of the search that read every vault (#474).
    if tool != "Grep" {
        return Answer::Nothing;
    }
    let state_dir = crate::plane::state_dir(hook.root);
    let operands = if named.is_empty() {
        vec![".".to_string()]
    } else {
        named
    };
    let Some(walked) =
        leakguard::walk_into_guarded_state(hook.text("cwd"), &operands, &[], &state_dir)
    else {
        return Answer::Nothing;
    };
    // `Grep` has no exclude; its own narrowing stands in for one, answered by looking.
    let glob = input
        .get("glob")
        .filter(|g| truthy(Some(g)))
        .map(crate::pyrepr::str_json)
        .unwrap_or_default();
    if !glob.is_empty() && !leakguard::glob_selects_inside(&walked, &glob, 512) {
        return Answer::Nothing;
    }
    let name = walked
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    deny(format!(
        "walks a directory tree that contains the plane's own `{name}` — every file in it \
         would be printed into the transcript, and none of them is named on this call. {}",
        leakguard::WALK_FIX
    ))
}

/// `_state_write_reason`'s sentence.
pub const STATE_WRITE_REASON: &str = "writes charter's own state directly (that directory \
     decides which commands run without a prompt). Use the charter command that owns it — \
     `charter persona use`, `charter vault add`, `charter secret set`";

/// `pretooluse_edit`: refuse a `Write`/`Edit` into charter's own state directory, which holds
/// the tool-gate ceiling, the persona pointers and the vault registry.
pub fn pretooluse_edit(hook: &Hook) -> Answer {
    if !hook.in_plane {
        return Answer::Nothing;
    }
    let named = targets(hook.tool_input());
    if named.is_empty() {
        return Answer::Nothing;
    }
    let cwd = match hook.text("cwd") {
        "" => hook.cwd.to_string_lossy().into_owned(),
        given => given.to_string(),
    };
    let state = pypath::realpath(&crate::plane::state_dir(hook.root).to_string_lossy());
    for t in named {
        let joined = if pypath::is_abs(&t) {
            t
        } else {
            pypath::join(&cwd, &t)
        };
        let p = pypath::realpath(&joined);
        if p == state || p.starts_with(&format!("{state}/")) {
            return deny(STATE_WRITE_REASON.to_string());
        }
    }
    Answer::Nothing
}

/// `pretooluse_dispatch`: record the dispatch as in flight, and ask first when a persona that
/// writes code is sent out while another agent is still running in the same working tree.
pub fn pretooluse_dispatch(hook: &Hook) -> Answer {
    if !hook.in_plane {
        return Answer::Nothing;
    }
    let tool = hook.tool_name();
    if tool != "Task" && tool != "Agent" {
        return Answer::Nothing;
    }
    let agent = hook.input_text("subagent_type");
    if agent.is_empty() {
        return Answer::Nothing;
    }
    let state = hook.state();
    let others = inflight::still_running(&state, hook.unix_now());
    let _ = inflight::start(&state, agent, hook.unix_now());
    if others.is_empty() {
        return Answer::Nothing;
    }
    let isolation = crate::personas::load(hook.root, agent)
        .and_then(|pairs| {
            pairs
                .iter()
                .rev()
                .find(|(k, _)| k == "dispatch-isolation")
                .map(|(_, v)| crate::memstore::py_strip(v).to_string())
        })
        .unwrap_or_default();
    if isolation != "worktree" {
        return Answer::Nothing;
    }
    let peers = others
        .iter()
        .map(|o| format!("`{o}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let verb = if others.len() == 1 { "is" } else { "are" };
    ask(
        hook,
        &format!(
            "`{agent}` writes code and {peers} {verb} already running. They share one working \
             tree, so parallel edits interleave silently. Dispatch this one with `isolation: \
             worktree`, or let the other finish first."
        ),
    )
}

/// The persona tool gate at the end of the Bash guard: `allow` when the active persona
/// declares the program, `None` to leave the harness's prompt alone. Plane-gated.
pub fn persona_allow(hook: &Hook) -> Option<String> {
    if !hook.in_plane {
        return None;
    }
    let command = hook
        .tool_input()
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let persona = hook.persona();
    let session = crate::hookstate::session(hook.session_id(), hook.env);
    let cwd = match hook.text("cwd") {
        "" => hook.cwd.to_string_lossy().into_owned(),
        given => given.to_string(),
    };
    let (name, binary) = personagate::decide(&personagate::Ask {
        plane: hook.root,
        persona: persona.as_deref(),
        session: session.as_deref(),
        command,
        cwd: &cwd,
    })?;
    Some(personagate::allowed(&name, &binary))
}

/// A Bash command that RECORDS a memory — `_MEM_RECORD_RE`. It resets the cadence nudge,
/// because a memory written through the CLI is invisible to `PostToolUse` on a file.
fn mem_record() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(?:workspace|persona)[\s\x{1C}-\x{1F}]+(?:remember|note)\b")
            .expect("compiles")
    })
}

/// The Bash guard's bookkeeping: the heartbeat, and the cadence reset a memory-recording
/// command earns. Plane-gated, like all of `pretooluse`'s writers.
pub fn pretooluse_bookkeeping(hook: &Hook) {
    if !hook.in_plane {
        return;
    }
    hook.touch_piece();
    let command = hook
        .tool_input()
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if mem_record().is_match(command) {
        memnudge_set(hook, 0);
    }
}

// ---- PostToolUse --------------------------------------------------------------------------

/// Re-surface the record-memory habit every this many memory-less file changes —
/// `_MEM_NUDGE_EVERY`.
pub const MEM_NUDGE_EVERY: u64 = 12;

fn mem_path() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"/(?:personas/[^/]+|workspaces/[^/]+)/(?:memory|refs)/").expect("compiles")
    })
}

fn ws_clone() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"/workspaces/([^/]+)/([^/]+)/").expect("compiles"))
}

/// `<sessions>/<sid>.memnudge`, for an id that is one path segment.
fn memnudge_file(hook: &Hook) -> Option<PathBuf> {
    let sid = hook.session_id().filter(|s| !s.is_empty())?;
    crate::contain::segment_ok(sid).then(|| hook.state().sessions().join(format!("{sid}.memnudge")))
}

fn memnudge_set(hook: &Hook, n: u64) {
    if let Some(file) = memnudge_file(hook) {
        let _ = hook.state().write(&file, n.to_string().as_bytes());
    }
}

/// `_memnudge_bump`: one more file change since the last recorded memory; the new count, or 0
/// with no session to count for.
fn memnudge_bump(hook: &Hook) -> u64 {
    let Some(file) = memnudge_file(hook) else {
        return 0;
    };
    let n = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| crate::memstore::py_strip(&t).parse::<u64>().ok())
        .unwrap_or(0)
        + 1;
    let _ = hook.state().write(&file, n.to_string().as_bytes());
    n
}

/// `_ws_edit_first_this_session`: true the FIRST time a clone in `ws` is edited this session,
/// marking it so the next is not.
fn ws_edit_first(hook: &Hook, ws: &str) -> bool {
    let Some(sid) = hook.session_id().filter(|s| !s.is_empty()) else {
        return true;
    };
    let state = hook.state();
    let key = crate::hookstate::safe(&format!("{sid}-{ws}"));
    let marker = state.dir().join("ws-edit-nudge").join(key);
    if marker.exists() {
        return false;
    }
    let _ = state.write(&marker, b"1");
    true
}

/// `memory_share_note`: what recording a memory will actually do on this plane.
pub fn memory_share_note(root: &Path) -> &'static str {
    match crate::workspaces::Plane::open(root).memory_share() {
        "commit" => {
            "It is committed locally straight away, but NOT pushed — this plane's `share` is \
             `commit`."
        }
        "push" => "It is committed and pushed immediately, so it reaches the team.",
        _ => {
            "It stays on THIS MACHINE — this plane's `share` is `local`, so charter commits \
             nothing; commit and push it yourself if the team needs it."
        }
    }
}

/// `_mem_cadence_nudge`.
fn mem_cadence_nudge(hook: &Hook, count: u64) -> String {
    let session = hook.workspace_session();
    let ws = hook.workspace(session.as_deref());
    let live = crate::workspaces::Plane::open(hook.root).is_live(&ws);
    let how = if live {
        format!("`charter workspace remember \"<fact>\"` (workspace **{ws}**)")
    } else if let Some(active) = hook.persona() {
        format!("`charter persona remember {active} \"<fact>\"`")
    } else {
        "`charter workspace remember \"<fact>\"` (make the workspace LIVE to share) or `charter \
         persona remember <p> \"<fact>\"`"
            .to_string()
    };
    format!(
        "⬢ Memory check — ~{count} file changes since your last recorded memory. Recording \
         durable memory is a standing part of the flow, and it fades on long sessions. If this \
         work produced something durable — a decision, a gotcha, a verified fact, a *why* — \
         record it now so it survives this session: {how}. {} If nothing here is worth keeping, \
         carry on — don't record filler.",
        memory_share_note(hook.root)
    )
}

/// `posttooluse` on `Write`/`Edit`/`MultiEdit`: warn when a memory or ref just written looks
/// like it holds a secret; otherwise count the change toward the record-memory habit, and on
/// the first edit in a LIVE workspace's clone say what the workspace flow expects.
pub fn posttooluse(hook: &Hook) -> Answer {
    if !hook.in_plane {
        return Answer::Nothing;
    }
    hook.touch_piece();
    if !matches!(hook.tool_name(), "Write" | "Edit" | "MultiEdit") {
        return Answer::Nothing;
    }
    let input = hook.tool_input();
    let fp = [input.get("file_path"), input.get("filePath")]
        .into_iter()
        .flatten()
        .find(|v| truthy(Some(v)))
        .map(crate::pyrepr::str_json)
        .unwrap_or_default();
    if fp.is_empty() {
        return Answer::Nothing;
    }
    let norm = format!("/{}", fp.replace('\\', "/")).replace("//", "/");
    if mem_path().is_match(&norm) {
        memnudge_set(hook, 0);
        return secret_scan(hook, input, &fp);
    }
    let count = memnudge_bump(hook);
    if let Some(m) = ws_clone().captures(&norm)
        && !matches!(&m[2], "memory" | "refs")
    {
        let (ws, repo) = (&m[1], &m[2]);
        if crate::workspaces::Plane::open(hook.root).is_live(ws) && ws_edit_first(hook, ws) {
            return say(
                "PostToolUse",
                &[(
                    "additionalContext",
                    format!(
                        "⬢ You're changing **{repo}** in workspace **{ws}**. Per the workspace \
                         flow, record a **workspace memory** (what changed + why + the repo \
                         commit) before you finish — `charter workspace remember \"<…>\"` (one \
                         file per memory under `workspaces/{ws}/memory/`, recall with `charter \
                         workspace recall`). And keep the **charter** current \
                         (`workspaces/{ws}/workspace.md`): if this work shifts the goal, adds a \
                         key decision, or introduces a new term, update its Vision / Context / \
                         Glossary so a teammate or a fork inherits the real picture. Both are \
                         committed + shared + auto-saved. Commit the actual code inside the repo \
                         (its own remote), then `charter workspace snapshot` to record the \
                         branch. Do this **without asking the engineer** — it's the flow."
                    ),
                )],
            );
        }
    }
    if count > 0 && count.is_multiple_of(MEM_NUDGE_EVERY) {
        return say(
            "PostToolUse",
            &[("additionalContext", mem_cadence_nudge(hook, count))],
        );
    }
    Answer::Nothing
}

/// `_posttooluse_secret_scan`: what was written, and the file as it now stands, checked for a
/// credential's SHAPE. Named by kind, never by the matched text.
fn secret_scan(hook: &Hook, input: &Value, fp: &str) -> Answer {
    let mut text = ["content", "new_string", "new_str"]
        .iter()
        .map(|k| {
            input
                .get(*k)
                .filter(|v| truthy(Some(v)))
                .map(crate::pyrepr::str_json)
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ");
    // The file is read where the PROCESS stands, as `Path(fp).read_text()` reads it.
    let on_disk = if Path::new(fp).is_absolute() {
        PathBuf::from(fp)
    } else {
        hook.cwd.join(fp)
    };
    if let Some(now) = crate::memstore::read_text(&on_disk) {
        text.push('\n');
        text.push_str(&now);
    }
    let Some(kind) = crate::secretshape::secret_kind(&text) else {
        return Answer::Nothing;
    };
    let name = Path::new(fp)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    say(
        "PostToolUse",
        &[(
            "additionalContext",
            format!(
                "⚠ SECURITY: the memory/ref you just wrote ({name}) appears to contain a secret \
                 ({kind}). Persona AND workspace memory/refs are committed and shared — secrets \
                 must NEVER go there. Remove it now and store the value in the vault instead \
                 (`charter persona secret set <key>` / `charter vault`)."
            ),
        )],
    )
}

/// `posttooluse_skill`: log which skill the active persona just used.
pub fn posttooluse_skill(hook: &Hook) -> Answer {
    if !hook.in_plane {
        return Answer::Nothing;
    }
    hook.touch_piece();
    if hook.tool_name() != "Skill" {
        return Answer::Nothing;
    }
    let name = match hook.input_text("skill") {
        "" => hook.input_text("name"),
        skill => skill,
    };
    if name.is_empty() {
        return Answer::Nothing;
    }
    let persona = hook.persona();
    let _ = crate::skilluse::record(hook.root, name, persona.as_deref(), hook.now, hook.host);
    Answer::Nothing
}

/// Where a sub-agent's id is remembered against the persona it was dispatched as —
/// `_agent_map_file`.
fn agent_map_file(hook: &Hook) -> PathBuf {
    hook.state().dir().join("agent-personas.json")
}

/// `_AGENT_MAP_MAX`: a lookup for live agents, not a history.
const AGENT_MAP_MAX: usize = 200;

fn agent_id() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\bagentId:[\s\x{1C}-\x{1F}]*([0-9a-f]{6,})").expect("compiles")
    })
}

/// `_agent_map_remember`.
fn agent_map_remember(hook: &Hook, agent_id: &str, persona: &str) {
    let file = agent_map_file(hook);
    let mut data = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| match v {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default();
    data.insert(agent_id.to_string(), Value::String(persona.to_string()));
    if data.len() > AGENT_MAP_MAX {
        let drop = data.len() - AGENT_MAP_MAX;
        data = data.into_iter().skip(drop).collect();
    }
    let text = crate::pyjson::dumps_sorted(&Value::Object(data));
    let _ = hook.state().write(&file, text.as_bytes());
}

/// `_agent_map_lookup`.
fn agent_map_lookup(hook: &Hook, target: &str) -> Option<String> {
    let text = std::fs::read_to_string(agent_map_file(hook)).ok()?;
    let data: Value = serde_json::from_str(&text).ok()?;
    let found = data.get(target)?;
    truthy(Some(found)).then(|| crate::pyrepr::str_json(found))
}

/// `posttooluse_dispatch`: log the dispatch, remember the sub-agent's id against its persona
/// so a later `SendMessage` to it can be attributed, and take its in-flight record down.
pub fn posttooluse_dispatch(hook: &Hook) -> Answer {
    if !hook.in_plane {
        return Answer::Nothing;
    }
    if !matches!(hook.tool_name(), "Task" | "Agent") {
        return Answer::Nothing;
    }
    let agent = hook.input_text("subagent_type");
    if agent.is_empty() {
        return Answer::Nothing;
    }
    let logged = dispatch::record(hook.root, agent, hook.now, hook.host);
    let response = hook
        .payload
        .get("tool_response")
        .filter(|v| truthy(Some(v)))
        .map(crate::pyrepr::str_json)
        .unwrap_or_default();
    if let Some(m) = agent_id().captures(&response) {
        agent_map_remember(hook, &m[1], agent);
    }
    if logged.is_some() {
        inflight::finish(&hook.state(), agent, hook.unix_now());
    }
    Answer::Nothing
}

/// `posttooluse_message`: a `SendMessage` to a persona, or to a sub-agent dispatched as one, is
/// a RESUME of that persona — logged apart from a dispatch so the roster does not count it.
pub fn posttooluse_message(hook: &Hook) -> Answer {
    if !hook.in_plane {
        return Answer::Nothing;
    }
    if hook.tool_name() != "SendMessage" {
        return Answer::Nothing;
    }
    let target = hook.input_text("to");
    if target.is_empty() {
        return Answer::Nothing;
    }
    let name = if personagrant::list_personas(hook.root)
        .iter()
        .any(|p| p == target)
    {
        Some(target.to_string())
    } else {
        agent_map_lookup(hook, target)
    };
    if let Some(name) = name {
        let _ = dispatch::record_resume(hook.root, &name, hook.now, hook.host);
    }
    Answer::Nothing
}

#[cfg(test)]
mod tests;
