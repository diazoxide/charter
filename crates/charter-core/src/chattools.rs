//! The tools a chat is offered through charter's MCP server (HP-7, Q15): the project's todos,
//! memory, session records and change status, and a question for the operator. Skills stay for
//! prose guidance; these carry the actions, the same way on every harness.
//!
//! This module is the tools themselves, with no protocol in it: [`TOOLS`] says what each takes
//! and [`call`] does it. `charter mcp` (the CLI) serves them over stdio with `rmcp`, and each
//! harness adapter hands a chat that server for the chat alone ([`crate::harness::HarnessAdapter`],
//! FD-13): Claude Code on `--mcp-config`, Codex on `-c mcp_servers.charter=…`, opencode in the
//! config its shim already rides in.
//!
//! **The boundary is the tool set, because the server runs where the harness runs its MCP
//! servers.** Codex and Claude Code start an MCP server outside the sandbox they put a command
//! in, as they do charter's hooks, so what a tool may do is decided here and nowhere else:
//!
//! - **The chat's own place, and nothing it names.** Every call acts on the workspace (or the
//!   project root) a `charter` command run in the chat would act on with no `-w`, resolved by
//!   the server at each call ([`crate::active`]). No tool takes a workspace, a project, a
//!   path or a directory, and a test holds that. A workspace that is not there is refused,
//!   never created.
//! - **Names, never paths.** A todo is closed by its slug and a record read by its file name,
//!   each checked by the store that owns it, which also refuses a link out of the project.
//! - **No credential, and no human power.** No tool reads a vault, the environment, the audit
//!   or a forge: `change_status` is the record half of `charter change show`, because the forge
//!   half signs in as the person (ADR 0077). `ask_operator` asks through the harness, which
//!   shows the question to the person; nothing here answers it.

use std::path::Path;

use serde_json::{Map, Value, json};

use crate::active::Place;

/// The server's name in every harness's config, and so the prefix a harness gives its tools
/// (`mcp__charter__todo_add` in Claude Code).
pub const SERVER: &str = "charter";

/// The `charter` subcommand that serves the tools over stdio.
pub const SUBCOMMAND: &str = "mcp";

/// The one tool [`call`] does not answer: only the server, which holds the connection to the
/// harness, can ask the person through it.
pub const ASK_OPERATOR: &str = "ask_operator";

/// The tools a Claude Code chat runs without asking (V79, #1050, amending SI-8e in ADR 0064):
/// the five that only read. Named one by one, never derived from [`Tool::read_only`]:
/// `ask_operator` is marked read-only too and still asks, and a tool added later is asked
/// about until someone rules it in here. Writes are never in this list.
pub const PRE_ALLOWED: [&str; 5] = [
    "todo_list",
    "memory_search",
    "session_record_list",
    "session_record_read",
    "change_status",
];

/// The variables the server reads to find the chat's place, as a `charter` command in the chat
/// reads them. A harness that hands an MCP server only the variables it is told to (Codex) is
/// told these, and nothing else of charter's: not the chat's token, not its hook socket.
pub const SCOPE_ENV: [&str; 5] = [
    "CHARTER_ROOT",
    crate::active::WORKSPACE_ENV,
    crate::active::PLANE_ROOT_ENV,
    crate::active::PERSONA_ENV,
    crate::active::SESSION_ID_ENV,
];

/// How long Codex waits on one of these tools, in seconds. Its own default is 60, and
/// `ask_operator` waits on a person; an hour is a question left for lunch, not one forgotten.
pub const CODEX_TOOL_TIMEOUT_SECS: i64 = 3600;

/// The server as Claude Code's `--mcp-config` takes it: JSON, on the argument, for that session
/// alone. Claude Code hands a stdio server its own environment and directory (measured on
/// 2.1.288), so the chat's place reaches it with nothing named.
pub fn claude_code_config(binary: &Path) -> String {
    json!({
        "mcpServers": {
            SERVER: {
                "type": "stdio",
                "command": binary.display().to_string(),
                "args": [SUBCOMMAND],
            }
        }
    })
    .to_string()
}

/// The server as Codex's `-c` takes it: `mcp_servers.charter=<inline table>`, beside the
/// operator's own servers (measured on codex-cli 0.147.0 with `codex mcp get`). Codex hands a
/// stdio server only a few variables of its own, so the ones that say where the chat works
/// are named in `env_vars` ([`SCOPE_ENV`]).
pub fn codex_flag(binary: &Path) -> String {
    let table: toml::Table = [
        (
            "command".to_owned(),
            toml::Value::from(binary.display().to_string()),
        ),
        (
            "args".to_owned(),
            toml::Value::Array(vec![toml::Value::from(SUBCOMMAND)]),
        ),
        (
            "env_vars".to_owned(),
            toml::Value::Array(
                SCOPE_ENV
                    .iter()
                    .map(|name| toml::Value::from(*name))
                    .collect(),
            ),
        ),
        (
            "tool_timeout_sec".to_owned(),
            toml::Value::from(CODEX_TOOL_TIMEOUT_SECS),
        ),
    ]
    .into_iter()
    .collect();
    format!("mcp_servers.{SERVER}={}", toml::Value::Table(table))
}

/// The server as ACP's `session/new` takes it in `mcpServers` (ADR 0080 §1): a stdio server, run
/// by the agent, handed the [`SCOPE_ENV`] variables `chat_env` sets and nothing else of charter's.
pub fn acp_server(
    binary: &Path,
    chat_env: &[(std::ffi::OsString, std::ffi::OsString)],
) -> agent_client_protocol::schema::v1::McpServer {
    use agent_client_protocol::schema::v1::{EnvVariable, McpServer, McpServerStdio};
    // A value that is not UTF-8 cannot be said in JSON, and is left out rather than mangled.
    let env = chat_env
        .iter()
        .filter_map(|(name, value)| Some((name.to_str()?, value.to_str()?)))
        .filter(|(name, _)| SCOPE_ENV.contains(name))
        .map(|(name, value)| EnvVariable::new(name, value))
        .collect();
    McpServer::Stdio(
        McpServerStdio::new(SERVER, binary)
            .args(vec![SUBCOMMAND.to_owned()])
            .env(env),
    )
}

/// The server as opencode's config names a local one. opencode hands it its own environment
/// and directory (measured on 1.18.33).
pub fn opencode_entry(binary: &Path) -> Value {
    json!({
        "type": "local",
        "command": [binary.display().to_string(), SUBCOMMAND],
        "enabled": true,
    })
}

/// One tool: its name, what it is for, and the JSON Schema of what it takes.
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: fn() -> Value,
    /// Whether it changes nothing, for a harness that shows read-only tools differently.
    pub read_only: bool,
}

fn no_arguments() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn one_string(key: &str, description: &str) -> Value {
    json!({
        "type": "object",
        "properties": { key: { "type": "string", "description": description } },
        "required": [key],
        "additionalProperties": false,
    })
}

/// Every tool, in the order a harness lists them.
pub static TOOLS: [Tool; 9] = [
    Tool {
        name: "todo_list",
        description: "List the open todos of the workspace this chat works in, oldest first, \
                      each with the slug todo_done closes it by.",
        schema: no_arguments,
        read_only: true,
    },
    Tool {
        name: "todo_add",
        description: "Record a todo in the workspace this chat works in. Refused when an open \
                      todo is already about the same work.",
        schema: || one_string("text", "What is to be done. The first line is its title."),
        read_only: false,
    },
    Tool {
        name: "todo_done",
        description: "Close one of this workspace's todos by its slug (todo_list shows it). \
                      Its closing is written to the workspace's memory.",
        schema: || one_string("slug", "The todo's slug, as todo_list shows it."),
        read_only: false,
    },
    Tool {
        name: "memory_search",
        description: "Search the memory of the workspace this chat works in. With no query, \
                      list every memory.",
        schema: || {
            json!({
                "type": "object",
                "properties": { "query": { "type": "string", "description": "Words to look for." } },
                "additionalProperties": false,
            })
        },
        read_only: true,
    },
    Tool {
        name: "memory_add",
        description: "Record one durable fact in the memory of the workspace this chat works \
                      in. Never a secret: memory is read by every later chat here.",
        schema: || one_string("text", "The fact. The first line is its title."),
        read_only: false,
    },
    Tool {
        name: "session_record_list",
        description: "List the session records of where this chat works (its workspace, or \
                      the project root), newest first.",
        schema: no_arguments,
        read_only: true,
    },
    Tool {
        name: "session_record_read",
        description: "Read one session record of where this chat works, by its file name \
                      (session_record_list shows it).",
        schema: || {
            one_string(
                "file",
                "The record's file name, YYYYMMDD-HHMMSS-<title>.md. Never a path.",
            )
        },
        read_only: true,
    },
    Tool {
        name: "change_status",
        description: "The cross-repo changes of this workspace: with no change named, one line \
                      each; with one, its record (why, members, branches and what each \
                      needs). The forge is not asked: run `charter change show` for that.",
        schema: || {
            json!({
                "type": "object",
                "properties": { "change": { "type": "string", "description": "The change's slug." } },
                "additionalProperties": false,
            })
        },
        read_only: true,
    },
    Tool {
        name: ASK_OPERATOR,
        description: "Ask the operator a question and wait for the answer, through this \
                      harness's own prompt. Never ask for a password, token or other secret: \
                      a credential is used through `charter secret`, never typed in.",
        schema: || one_string("question", "The question, as the operator will read it."),
        read_only: true,
    },
];

/// What `tool` does with `args` for a chat whose place is `place` in the project at `root`:
/// the text the chat is answered with, or the sentence saying why nothing was done.
///
/// Every store is held open for the call ([`held::Store`]) and used through its descriptor
/// only, so nothing a chat swaps in while the tool runs can move a read or a write out of its
/// workspace (V74).
pub fn call(
    root: &Path,
    place: &Place,
    tool: &str,
    args: &Map<String, Value>,
    now: chrono::NaiveDateTime,
) -> Result<String, String> {
    let Some(spec) = TOOLS.iter().find(|t| t.name == tool) else {
        return Err(format!("charter has no tool {}", crate::shown::short(tool)));
    };
    if tool == ASK_OPERATOR {
        return Err(format!(
            "{ASK_OPERATOR} is asked through the harness by charter's MCP server, not here"
        ));
    }
    // FR-24, at every write and not only when the server started: the server lives as long
    // as the chat, and the project can be moved to a newer format under it.
    if !spec.read_only
        && let crate::compat::Compat::ReadOnly(why) = crate::compat::read(root)
    {
        return Err(format!("nothing was written: {why}"));
    }
    run(root, place, tool, args, now)
}

#[cfg(not(unix))]
fn run(
    _root: &Path,
    _place: &Place,
    _tool: &str,
    _args: &Map<String, Value>,
    _now: chrono::NaiveDateTime,
) -> Result<String, String> {
    // Fail closed: the stores are held through descriptors that refuse links, which only the
    // unix build does yet.
    Err("charter's MCP tools are not available on this platform yet".to_owned())
}

#[cfg(unix)]
fn run(
    root: &Path,
    place: &Place,
    tool: &str,
    args: &Map<String, Value>,
    now: chrono::NaiveDateTime,
) -> Result<String, String> {
    match tool {
        "todo_list" => {
            let ws = workspace_name(place)?;
            let Some(todos) = held::Store::open(root, place, "todos", false)? else {
                return Ok(format!("No open todos in '{ws}'."));
            };
            todo_list(&todos, ws, now)
        }
        "todo_add" => {
            let ws = workspace_name(place)?;
            let text = string(args, "text")?;
            if text.trim().is_empty() {
                return Err(crate::workspaces::RecordRefused::Empty.to_string());
            }
            let todos = must_open(root, place, "todos")?;
            let stored = stored_texts(&todos)?;
            if let Some(dup) =
                crate::memstore::duplicate_in(text, stored.iter().map(String::as_str))
            {
                return Err(crate::workspaces::RecordRefused::AlreadyListed(dup).to_string());
            }
            let header = crate::workspaces::TODOS_HEADER.replace("{name}", ws);
            let file = write_entry(&todos, &header, text, now)?;
            Ok(format!("Todo recorded in '{ws}': {}", stem(&file)))
        }
        "todo_done" => {
            let ws = workspace_name(place)?;
            let slug = string(args, "slug")?;
            if !crate::contain::segment_ok(slug.strip_suffix(".md").unwrap_or(slug)) {
                return Err(format!(
                    "{} is not a todo's slug: todo_list shows them",
                    crate::shown::short(slug)
                ));
            }
            let Some(todos) = held::Store::open(root, place, "todos", false)? else {
                return Err(format!("no such todo: {slug}"));
            };
            let file = resolve(&todos, slug)?;
            let text = todos
                .read(&file)?
                .ok_or_else(|| format!("no such todo: {slug}"))?;
            let title = crate::workspaces::parse_entry(&stem(&file), &text).title;
            // The trace first, then the todo: a failure leaves the todo open rather than closed
            // with nothing recorded, as `Workspace::close_todo` does.
            let memory = must_open(root, place, "memory")?;
            let header = crate::workspaces::WS_MEMORY_HEADER.replace("{name}", ws);
            write_entry(&memory, &header, &format!("Closed todo: {title}"), now)?;
            forget(&todos, &file)?;
            Ok(format!(
                "Closed todo {} in '{ws}'; its memory has the trace.",
                stem(&file)
            ))
        }
        "memory_search" => {
            let ws = workspace_name(place)?;
            let found = match held::Store::open(root, place, "memory", false)? {
                Some(memory) => memories(&memory)?,
                None => Vec::new(),
            };
            memory_search(ws, found, args)
        }
        "memory_add" => {
            let ws = workspace_name(place)?;
            let text = string(args, "text")?;
            let memory = must_open(root, place, "memory")?;
            let header = crate::workspaces::WS_MEMORY_HEADER.replace("{name}", ws);
            let file = write_entry(&memory, &header, text, now)?;
            Ok(format!("Remembered in '{ws}': {}", stem(&file)))
        }
        "session_record_list" => {
            let mut listed = Vec::new();
            if let Some(sessions) =
                held::Store::open(root, place, crate::sessionrecord::DIR, false)?
            {
                let mut names: Vec<String> = sessions
                    .files()?
                    .into_iter()
                    .filter(|name| crate::sessionrecord::is_record_name(name))
                    .collect();
                names.sort_unstable_by(|a, b| b.cmp(a));
                for name in names {
                    if let Some(text) = sessions.read(&name)? {
                        listed.push(crate::sessionrecord::listed(place, name, &text));
                    }
                }
            }
            if listed.is_empty() {
                return Ok(format!("No session records at {}.", place.said()));
            }
            Ok(listed
                .iter()
                .map(|r| format!("{} · {} · {}", r.when, r.file, crate::shown::line(&r.title)))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "session_record_read" => {
            let file = string(args, "file")?;
            if !crate::sessionrecord::is_record_name(file) {
                return Err(format!(
                    "{} is not a session record's file name (YYYYMMDD-HHMMSS-<title>.md)",
                    crate::shown::short(file)
                ));
            }
            held::Store::open(root, place, crate::sessionrecord::DIR, false)?
                .map(|sessions| sessions.read(file))
                .transpose()?
                .flatten()
                .ok_or_else(|| format!("no session record {file} at {}", place.said()))
        }
        "change_status" => {
            let ws = workspace_name(place)?;
            let changes = held::Store::open(root, place, crate::change::store::DIRNAME, false)?;
            match optional_string(args, "change")? {
                None => change_list(changes.as_ref(), ws),
                Some(slug) => change_record(changes.as_ref(), slug),
            }
        }
        other => Err(format!(
            "charter has no tool {}",
            crate::shown::short(other)
        )),
    }
}

/// The store, held, and made when it is not there yet.
#[cfg(unix)]
fn must_open(root: &Path, place: &Place, store: &str) -> Result<held::Store, String> {
    held::Store::open(root, place, store, true)?
        .ok_or_else(|| format!("the chat's {store} could not be made"))
}

/// The question an `ask_operator` call puts, trimmed, or why it puts none.
pub fn question(args: &Map<String, Value>) -> Result<String, String> {
    let asked = string(args, "question")?.trim();
    if asked.is_empty() {
        return Err("a question for the operator needs some words".to_owned());
    }
    Ok(asked.to_owned())
}

/// The message the harness shows the operator for `question`: who is asking, so a chat cannot
/// phrase its question as if charter itself asked.
pub fn asked(question: &str) -> String {
    format!("This chat asks: {question}")
}

/// What the answer's field says beneath the question, in the harness's own form.
pub const NO_SECRET_HERE: &str = "Don't type a password or secret here.";

/// The `requestState` an `ask_operator` call is retried with at protocol 2026-07-28: bound to
/// the question, so an answer is only ever read back for the question it answers.
pub fn question_state(question: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(question.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("{ASK_OPERATOR}:{hex}")
}

/// The chat's workspace, where it is one.
#[cfg_attr(not(unix), allow(dead_code))]
fn workspace_name(place: &Place) -> Result<&str, String> {
    match place {
        Place::Workspace(name) => Ok(name),
        Place::PlaneRoot => Err(
            "this chat works at the project root, and todos, memory and changes belong to a \
             workspace"
                .to_owned(),
        ),
    }
}

fn string<'a>(args: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    optional_string(args, key)?.ok_or_else(|| format!("`{key}` is required"))
}

fn optional_string<'a>(args: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(format!("`{key}` is a string")),
    }
}

/// A file name without its `.md`.
#[cfg_attr(not(unix), allow(dead_code))]
fn stem(file: &str) -> String {
    file.strip_suffix(".md").unwrap_or(file).to_owned()
}

/// The entries of a memory or todo store: every `*.md` but the index, sorted by name.
#[cfg(unix)]
fn entry_names(store: &held::Store) -> Result<Vec<String>, String> {
    Ok(store
        .files()?
        .into_iter()
        .filter(|name| name.ends_with(".md") && name != crate::memstore::INDEX)
        .collect())
}

/// The whole text of every entry of a store.
#[cfg(unix)]
fn stored_texts(store: &held::Store) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for name in entry_names(store)? {
        if let Some(text) = store.read(&name)? {
            out.push(text);
        }
    }
    Ok(out)
}

/// Every entry of a memory store as `charter recall` reads it.
#[cfg(unix)]
fn memories(store: &held::Store) -> Result<Vec<crate::memstore::Found>, String> {
    let mut out = Vec::new();
    for name in entry_names(store)? {
        if let Some(text) = store.read(&name)? {
            let path = std::path::PathBuf::from(&name);
            let title = crate::memstore::title_in(&path, &text);
            out.push(crate::memstore::Found { path, title, text });
        }
    }
    Ok(out)
}

/// Write one entry and its index line, as `memstore::write` does for a workspace's memory and
/// todos: a timestamp-prefixed file named for its title, numbered when the name is taken, and
/// the index made with `header` when it is not there. The file name it took.
#[cfg(unix)]
fn write_entry(
    store: &held::Store,
    header: &str,
    text: &str,
    now: chrono::NaiveDateTime,
) -> Result<String, String> {
    let text = crate::memstore::py_strip(text);
    if text.is_empty() {
        return Err("empty memory".to_owned());
    }
    let title = crate::memstore::title_of(text);
    let header = if header.ends_with('\n') {
        header.to_owned()
    } else {
        format!("{header}\n")
    };
    // The index first, as `ensure_index` makes it with the store's own header.
    store.append(crate::memstore::INDEX, &header, b"")?;
    // Then the one write of a workspace's memory and todos, which `charter` commands use too:
    // the file and its index line together, or neither (#1058).
    crate::memstore::write_in(
        store,
        &crate::memstore::NoGates,
        text,
        &title,
        true,
        "persistent",
        true,
        now,
    )
    .map_err(|e| e.to_string())
}

/// The entry `ident` names: its exact name, or the one entry whose name ends `-<ident>.md`.
#[cfg(unix)]
fn resolve(store: &held::Store, ident: &str) -> Result<String, String> {
    let names = entry_names(store)?;
    let exact = crate::memstore::md_name(ident);
    if names.contains(&exact) {
        return Ok(exact);
    }
    let suffix = format!("-{exact}");
    let tails: Vec<&String> = names.iter().filter(|n| n.ends_with(&suffix)).collect();
    match tails.as_slice() {
        [] => Err(format!("no such todo: {ident}")),
        [one] => Ok((*one).clone()),
        many => Err(format!(
            "'{ident}' is the end of more than one todo's name — {}; name one in full",
            many.iter().map(|n| stem(n)).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// Delete `file` and its index line, through the held store.
#[cfg(unix)]
fn forget(store: &held::Store, file: &str) -> Result<(), String> {
    store.remove(file)?;
    // As `memstore::forget`: the file is gone, so a failure to drop its line is drift the next
    // `optimize` names, never a failure of the close.
    crate::memstore::rewrite_index(store, file, None);
    Ok(())
}

#[cfg(unix)]
fn todo_list(store: &held::Store, ws: &str, now: chrono::NaiveDateTime) -> Result<String, String> {
    let mut open = Vec::new();
    for name in entry_names(store)? {
        if let Some(text) = store.read(&name)? {
            open.push(crate::workspaces::parse_entry(&stem(&name), &text));
        }
    }
    if open.is_empty() {
        return Ok(format!("No open todos in '{ws}'."));
    }
    let today = now.date();
    Ok(open
        .iter()
        .map(|todo| {
            let age = chrono::NaiveDate::parse_from_str(
                todo.stamp.split_whitespace().next().unwrap_or_default(),
                "%Y-%m-%d",
            )
            .map_or(0, |d| (today - d).num_days());
            format!(
                "{}  {age}d  {}",
                todo.slug,
                crate::personas::one_line(&todo.title)
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg_attr(not(unix), allow(dead_code))]
fn memory_search(
    ws: &str,
    found: Vec<crate::memstore::Found>,
    args: &Map<String, Value>,
) -> Result<String, String> {
    let query = optional_string(args, "query")?
        .map(str::trim)
        .filter(|q| !q.is_empty());
    let found: Vec<crate::memstore::Found> = match query {
        Some(q) => crate::memstore::rank(found, q, 8)
            .into_iter()
            .map(|(found, _)| found)
            .collect(),
        None => found,
    };
    if found.is_empty() {
        return Ok(match query {
            Some(q) => format!("No memories in '{ws}' match {}.", crate::shown::short(q)),
            None => format!("'{ws}' has no memories yet."),
        });
    }
    Ok(found
        .iter()
        .map(|f| format!("## {}\n{}", crate::shown::line(&f.title), f.text.trim()))
        .collect::<Vec<_>>()
        .join("\n\n"))
}

/// One line per change, as `charter change list` draws them.
#[cfg(unix)]
fn change_list(changes: Option<&held::Store>, ws: &str) -> Result<String, String> {
    let line = crate::shown::line;
    let mut out = Vec::new();
    if let Some(changes) = changes {
        for name in changes.files()? {
            let Some(slug) = name.strip_suffix(".json") else {
                continue;
            };
            let Some(text) = changes.read(&name)? else {
                continue;
            };
            match crate::change::Record::parse(&text, slug) {
                Ok(record) => out.push(format!(
                    "{}  {} member(s)  ·  {}",
                    line(&record.change),
                    record.members.len(),
                    line(&record.why)
                )),
                Err(why) => out.push(format!(
                    "{}: {}",
                    crate::shown::short(slug),
                    line(&why.to_string())
                )),
            }
        }
    }
    if out.is_empty() {
        return Ok(format!(
            "No changes in workspace '{ws}'. Create one: charter change create <slug> --why \"…\""
        ));
    }
    Ok(out.join("\n"))
}

/// One change's record: why, members, branches and what each needs. Never the forge.
#[cfg(unix)]
fn change_record(changes: Option<&held::Store>, slug: &str) -> Result<String, String> {
    if !crate::change::name_ok(slug) {
        return Err(format!(
            "{} is not a change name",
            crate::shown::short(slug)
        ));
    }
    let text = changes
        .map(|changes| changes.read(&format!("{slug}.json")))
        .transpose()?
        .flatten()
        .ok_or_else(|| format!("no change {slug}"))?;
    let record = crate::change::Record::parse(&text, slug).map_err(|e| e.to_string())?;
    let line = crate::shown::line;
    let mut out = vec![
        format!(
            "{} · {} member(s)",
            line(&record.change),
            record.members.len()
        ),
        format!("why: {}", line(&record.why)),
        format!("created {} by {}", line(&record.created), line(&record.by)),
    ];
    for m in &record.members {
        let mut row = format!("member {} on {}", line(&m.repo), line(&m.branch));
        if !m.needs.is_empty() {
            let needs: Vec<String> = m.needs.iter().map(|n| line(n)).collect();
            row.push_str(&format!(", needs {}", needs.join(", ")));
        }
        out.push(row);
    }
    for e in &record.excluded {
        out.push(format!("excluded {}: {}", line(&e.repo), line(&e.why)));
    }
    Ok(out.join("\n"))
}

#[cfg(unix)]
use crate::held;

#[cfg(test)]
mod tests;
