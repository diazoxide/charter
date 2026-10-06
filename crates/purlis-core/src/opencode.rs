//! The opencode plugin the app ships, and what arms an opencode chat with it (#371, ADR 0058).
//!
//! opencode has no hooks file and no `-c` flags: what it runs on its events is a JS/TS plugin,
//! loaded through its own Bun runtime. So charter ships one — a **shim** — generated here, and
//! the bundle's copy is this module's output byte for byte (the app crate's test holds it). The
//! shim carries no policy. It maps opencode's tool ids to the names charter's guards match,
//! forwards each event to `charter hook <word>` with the payload a Claude Code hook would get,
//! and turns the answer into what opencode takes: a thrown error refuses a tool call, and a
//! text part added to the prompt is the briefing and the handback.
//!
//! # Measured on opencode 1.18.23, with a throwaway `HOME` and a stand-in model
//!
//! A stand-in OpenAI-compatible server asked for one `bash` call and then answered text; every
//! request it received was logged, so what reached the model is known, not inferred.
//!
//! - **One session, nothing written.** `OPENCODE_CONFIG_CONTENT='{"plugin":["file://<path>"]}'`
//!   loads the plugin at `<path>` for that process alone. opencode merges config `plugin`
//!   lists by concatenation: the global plugin directory's scripts load beside it, and a
//!   project `opencode.json` holding `"plugin": []` did not remove it. A bare absolute path,
//!   a `file://` URL with a raw space and a percent-encoded one all loaded; charter writes the
//!   percent-encoded URL ([`session_config`]).
//! - **Throwing from `tool.execute.before` is a refusal.** The tool did not run, the part
//!   became `error`, and the model's next request carried the thrown message as the tool's
//!   result — never the file. That is the whole of how the guard is enforced.
//! - **`--pure` and `OPENCODE_PURE=1` (or `true`) load no external plugin at all**, and with
//!   them the vault's content reached the model. opencode's `--pure` sets `OPENCODE_PURE=1`
//!   itself before anything loads, so no variable charter sets can undo the flag. A profile
//!   that could turn the guard off that way is refused before its chat starts
//!   ([`disarmed_by`]).
//! - **Events** (the plugin's `event` hook): `session.created` (with `info.parentID` on a
//!   sub-agent's session), `session.status` busy/idle, `session.idle` when a turn ends, and
//!   `permission.asked` exactly when the TUI showed "Permission required" — then
//!   `permission.replied`. `chat.message` fires once per prompt, before the model is asked, and
//!   a text part pushed onto its `parts` reached the model inside that user message.
//! - **In the TUI a session is created at the first prompt**, not at launch: an idle opencode
//!   reports nothing. And quitting (`ctrl-c`) fired no event and ran no `exit` handler in the
//!   plugin, so opencode cannot say its session ended: the app sees the process end instead.
//! - The factory gets `{client, project, worktree, directory, experimental_workspace,
//!   serverUrl, $}`, runs in opencode's own process, and sees the environment opencode was
//!   started with.
//!
//! # Measured through this shim and the real `charter`
//!
//! The same setup, with the generated file loaded exactly as [`session_config`] names it:
//!
//! - a `bash` call reading `.charter/vaults/db.json` in a plane was refused by the leak guard,
//!   and the model's next request carried the guard's sentence and not the file;
//! - the first prompt carried charter's briefing as a second text part;
//! - the app's socket received `sessionstart` (`freshly`), `userpromptsubmit`, `notification`
//!   when a `"bash": "ask"` rule made opencode ask, and `stop`, each naming opencode's session;
//! - with `CHARTER_HOOK_BINARY` naming a program that is not there, the same call was refused
//!   ("a guard that could not answer does not allow"), and a `read` of the vault file through
//!   opencode's own `read` tool (`filePath`) was refused by `pretooluse-read`;
//! - `charter plugin install --harness opencode` wrote the guard-only variant, and an opencode
//!   started with none of the app's variables refused the same read; `uninstall` removed it;
//! - `opencode -s <id>` continued that session: no `session.created`, and `chat.message` named
//!   the same id, which is why the shim then says `source: "resume"`.
//!
//! # What the shim cannot defend
//!
//! opencode imports every plugin — the global plugin directory, a project's `.opencode/plugin/`,
//! npm packages named in any config — into one JavaScript realm. A plugin loaded beside the
//! shim can replace the globals it calls. The shim takes its own references to the few it
//! needs when it loads ([`shim`]), which closes the plain monkey-patch of a later plugin and
//! not a determined one. charter reports what else opencode loads (the settings tab's plugin
//! list; `charter doctor` for the Python charter's shim) rather than claiming a containment it
//! does not have. ADR 0058 lists what else is not covered. This is the
//! limit the Python charter's `foreign_plugins` named, and it is the same class as a project
//! `.claude/settings.json` that runs its own hook: guard rails, not guarantees.

use std::path::{Path, PathBuf};

/// The folder the shim sits in inside the bundled plugin directory ([`crate::plugin`]). Claude
/// Code reads only the directories it knows in a plugin, so this one rides beside them unread.
pub const DIR_IN_BUNDLE: &str = "opencode";

/// The shim's file name, in the bundle and in opencode's plugin directory alike (#1266: it was
/// `charter.ts`; `charter plugin install` takes a shim of its own at that name away).
pub const FILE_NAME: &str = crate::names::OPENCODE_SHIM.write;

/// Where the shim is in the bundled plugin at `bundle`.
pub fn shim_in(bundle: &Path) -> PathBuf {
    bundle.join(DIR_IN_BUNDLE).join(FILE_NAME)
}

/// The variable opencode reads a whole config from, merged over every other config it reads.
pub const CONFIG_ENV: &str = "OPENCODE_CONFIG_CONTENT";

/// The variable that makes opencode load no external plugin, when it is `1` or `true`.
pub const PURE_ENV: &str = "OPENCODE_PURE";

/// The flag that sets [`PURE_ENV`] from inside opencode, past anything charter can set.
pub const PURE_FLAG: &str = "--pure";

/// The first line of every shim charter writes, which is how `charter plugin install` knows a
/// file in opencode's plugin directory is its own to replace or remove. A shim that starts with
/// any first line the shim has had ([`is_own`]) is its own, and is replaced in place.
pub const MARK: &str = crate::names::OPENCODE_MARK.write;

/// Whether `text` is a shim this app wrote, under the purlis mark or the one charter wrote
/// before the rename. Not the retired Python charter's ([`PYTHON_MARK`]), which is told apart.
pub fn is_own(text: &str) -> bool {
    std::iter::once(crate::names::OPENCODE_MARK.write)
        .chain(crate::names::OPENCODE_MARK.reads.iter().copied())
        .any(|mark| text.starts_with(mark))
}

/// The first line of the retired Python charter's shim (`charter/harness/opencode.py`), which
/// `charter plugin install` replaces: it is charter's own artifact, and two shims would guard
/// every tool call twice and brief every chat twice.
pub const PYTHON_MARK: &str = crate::names::OPENCODE_MARK.history[0];

/// One opencode tool: its id, the name charter's guards match it by, and the words its calls
/// are forwarded to before and after they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    /// opencode's id for it, as the model calls it.
    pub id: &'static str,
    /// Claude Code's name for the same tool — what `hookreg`'s matchers and the guards name.
    pub name: &'static str,
    /// The `charter hook` word its calls go to before they run; `None` goes to [`DEFAULT_PRE`].
    pub pre: Option<&'static str>,
    /// The word its results go to, where charter keeps a tally or a nudge for it.
    pub post: Option<&'static str>,
}

/// opencode 1.18.23's tools, read off the tool list it sent the stand-in model: `bash`, `edit`,
/// `glob`, `grep`, `read`, `skill`, `task`, `todowrite`, `webfetch`, `write`. The routing is
/// `hooks/hooks.json`'s, by Claude Code's name for each tool (the test holds that every word is
/// wired to that name's matcher there).
pub const TOOLS: [Tool; 9] = [
    Tool {
        id: "bash",
        name: "Bash",
        pre: Some("pretooluse"),
        post: None,
    },
    Tool {
        id: "read",
        name: "Read",
        pre: Some("pretooluse-read"),
        post: None,
    },
    Tool {
        id: "grep",
        name: "Grep",
        pre: Some("pretooluse-read"),
        post: None,
    },
    Tool {
        id: "glob",
        name: "Glob",
        pre: None,
        post: None,
    },
    Tool {
        id: "write",
        name: "Write",
        pre: Some("pretooluse-edit"),
        post: Some("posttooluse"),
    },
    Tool {
        id: "edit",
        name: "Edit",
        pre: Some("pretooluse-edit"),
        post: Some("posttooluse"),
    },
    Tool {
        id: "task",
        name: "Task",
        pre: Some("pretooluse-dispatch"),
        post: Some("posttooluse-dispatch"),
    },
    Tool {
        id: "skill",
        name: "Skill",
        pre: None,
        post: Some("posttooluse-skill"),
    },
    Tool {
        id: "webfetch",
        name: "WebFetch",
        pre: None,
        post: None,
    },
];

/// Where a tool with no [`Tool::pre`] goes — every tool id opencode has or will add, MCP tools
/// included. It is the Bash guard, which judges `tool_input.command` and so reaches any tool
/// that carries one; a tool that carries none is allowed by it.
pub const DEFAULT_PRE: &str = "pretooluse";

/// The tools whose output charter may append a note to. Not one that returns content: a
/// `read` carrying charter's note would be a false record of that file.
pub const EFFECTFUL: [&str; 3] = ["bash", "edit", "write"];

/// Which shim: the one an app chat loads, or the guard `charter plugin install` puts in
/// opencode's own plugin directory for a chat started in a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arming<'a> {
    /// Every hook, running the `charter` named by [`crate::plugin::BINARY_ENV`], which the app
    /// sets in the chat's environment — the bundled file.
    Session,
    /// Only the guards before a tool runs, running the `charter` at this path. Only the guards,
    /// for ADR 0057's Codex reason: an app chat loads this too, beside the bundled shim, and a
    /// doubled guard only refuses twice where doubled state hooks would report every event
    /// twice and brief the chat twice.
    GuardOnly(&'a Path),
}

/// The shim, as the bundle carries it ([`Arming::Session`]) or as `charter plugin install`
/// writes it ([`Arming::GuardOnly`]).
pub fn shim(arming: Arming<'_>) -> String {
    let (binary, variant, hooks, missing) = match arming {
        Arming::Session => (
            format!("process.env.{} || \"\"", crate::plugin::BINARY_ENV),
            "// The app loads this file for one chat, and names its own `purlis` in the chat's environment.",
            SESSION_HOOKS,
            "refuses",
        ),
        Arming::GuardOnly(path) => (
            serde_json::Value::from(path.display().to_string()).to_string(),
            "// `purlis plugin install` put this here: the guards alone, for opencode started outside the app.",
            GUARD_HOOKS,
            "allows",
        ),
    };
    let routes: serde_json::Map<String, serde_json::Value> = TOOLS
        .iter()
        .map(|t| {
            (
                t.id.to_owned(),
                serde_json::json!({ "name": t.name, "pre": t.pre, "post": t.post }),
            )
        })
        .collect();
    let timeouts: serde_json::Map<String, serde_json::Value> = crate::hookreg::HANDLERS
        .iter()
        .map(|h| (h.name.to_owned(), h.timeout.into()))
        .collect();
    TEMPLATE
        .replace("{{MARK}}", MARK)
        .replace("{{VARIANT}}", variant)
        .replace("{{BINARY}}", &binary)
        .replace("{{BINARY_ENV}}", crate::plugin::BINARY_ENV)
        .replace("{{DEADLINE}}", &DEADLINE.to_string())
        .replace(
            "{{ROUTES}}",
            &serde_json::to_string_pretty(&routes).expect("JSON"),
        )
        .replace(
            "{{TIMEOUTS}}",
            &serde_json::to_string(&timeouts).expect("JSON"),
        )
        .replace(
            "{{DEFAULT_PRE}}",
            &serde_json::Value::from(DEFAULT_PRE).to_string(),
        )
        .replace(
            "{{EFFECTFUL}}",
            &serde_json::to_string(&EFFECTFUL).expect("JSON"),
        )
        .replace("{{HOOKS}}", hooks)
        .replace(
            "{{NOT_TAKEN}}",
            &serde_json::Value::from(crate::hookwire::NOT_TAKEN).to_string(),
        )
        .replace("{{MISSING}}", missing)
}

/// The `charter` a shim [`Arming::GuardOnly`] wrote runs, read back out of its text.
pub fn binary_in(text: &str) -> Option<PathBuf> {
    let line = text
        .lines()
        .find_map(|line| line.strip_prefix("const BINARY = "))?;
    match serde_json::from_str::<serde_json::Value>(line).ok()? {
        serde_json::Value::String(path) if !path.is_empty() => Some(PathBuf::from(path)),
        _ => None,
    }
}

/// The config an app chat is started with in [`CONFIG_ENV`]: the shim at `shim`, and nothing
/// else. A `file://` URL, percent-encoded, so no character of a path can end it early.
///
/// With `skills`, the shim is named in opencode's `[spec, options]` form and handed that
/// directory as its option, which it adds to the skills opencode discovers (ADR 0063). Not a
/// `skills` key beside `plugin`: opencode merges its configs with arrays replaced, so one here
/// would drop every skills path the operator's own config names (measured on 1.18.32).
///
/// With `binary`, charter's MCP server is named under `mcp` too (HP-7). `mcp` is an object, and
/// opencode merges objects key by key, so the operator's own servers stay.
///
/// A `sandboxed` chat also makes no snapshot (ruling V73a): a snapshot is a repository git is
/// later run in, outside the sandbox, so the wrap lets no chat write one. This layer wins over
/// a global and a project config that turn snapshots on (measured on 1.18.33).
pub fn session_config(
    shim: &Path,
    skills: Option<&Path>,
    binary: Option<&Path>,
    sandboxed: bool,
) -> String {
    let mut url = String::from("file://");
    for byte in shim.display().to_string().bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            url.push(char::from(byte));
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    let spec = match skills {
        Some(dir) => serde_json::json!([url, { SKILLS_OPTION: dir.display().to_string() }]),
        None => serde_json::Value::from(url),
    };
    let mut config = serde_json::json!({ "plugin": [spec] });
    if let Some(binary) = binary {
        config["mcp"] = serde_json::json!({
            crate::chattools::SERVER: crate::chattools::opencode_entry(binary)
        });
    }
    if sandboxed {
        config["snapshot"] = serde_json::Value::Bool(false);
    }
    config.to_string()
}

/// The shim's option that names charter's skills directory.
const SKILLS_OPTION: &str = "skills";

/// Seconds a hook the registry does not name is given — the Bash guard's own.
const DEADLINE: u32 = 10;

/// Why a chat started with `command` and `env` would run without charter's plugin — `None`
/// when it would not.
///
/// `command` is the profile's whole argv. The flag is seen bare or with a value attached
/// (`--pure=true`), as opencode's parser takes it. A wrapper script that adds it itself is past
/// what charter can read, as a wrapper that adds any flag is.
pub fn disarmed_by(command: &[String], env: &[(String, String)]) -> Option<String> {
    let pure = |word: &String| {
        word == PURE_FLAG
            || word
                .strip_prefix(PURE_FLAG)
                .is_some_and(|rest| rest.starts_with('='))
    };
    if command.iter().any(pure) {
        return Some(format!(
            "its command passes {PURE_FLAG}, which makes opencode load no plugin — purlis's \
             guard included"
        ));
    }
    for (name, _) in env {
        if name == PURE_ENV {
            return Some(format!(
                "it sets {PURE_ENV}, which can make opencode load no plugin — purlis's guard \
                 included"
            ));
        }
        if name == CONFIG_ENV {
            return Some(format!(
                "it sets {CONFIG_ENV}, which is how purlis hands opencode its plugin for the \
                 chat; put that config in opencode.json instead"
            ));
        }
    }
    None
}

/// The hooks an app chat's shim registers.
const SESSION_HOOKS: &str = r#"    // purlis's skills, beside every skills path the operator's configs name. opencode hands
    // this hook its live merged config before it discovers any skill, and scans each path in
    // `skills.paths` for `SKILL.md` (ADR 0063). Appended, never in their place.
    config: async (cfg) => {
      const skills = typeof options?.skills === "string" ? options.skills : ""
      if (!skills || !cfg || typeof cfg !== "object") return
      if (!cfg.skills || typeof cfg.skills !== "object") cfg.skills = {}
      const paths = Array.isArray(cfg.skills.paths) ? cfg.skills.paths : []
      if (!paths.includes(skills)) cfg.skills.paths = [...paths, skills]
    },

    // `$PURLIS_SESSION_ID` (and its old name) in every shell a tool opens, so a `purlis` command run there
    // knows its conversation, as `$CLAUDE_CODE_SESSION_ID` tells it in a Claude Code chat.
    "shell.env": async (input, output) => {
      const sid = rootOf(input?.sessionID)
      if (sid && output?.env) output.env.PURLIS_SESSION_ID = output.env.CHARTER_SESSION_ID = sid
    },

    "tool.execute.before": before,

    // The tallies and nudges purlis keeps after a tool ran. A note rides the tool's own
    // output, fenced, and only for a tool that reports an action rather than content.
    "tool.execute.after": async (input, output) => {
      const tool = toolId(input)
      const word = route(tool, "post")
      if (!word) return
      const said = await run(word, {
        hook_event_name: "PostToolUse",
        session_id: rootOf(input?.sessionID),
        cwd: directory,
        tool_name: route(tool, "name") ?? tool,
        tool_input: input?.args ?? {},
        tool_response: { output: String(output?.output ?? "") },
      }, input?.sessionID)
      const note = context(said)
      if (!note || !EFFECTFUL.includes(tool) || !output) return
      output.output = `${output.output ?? ""}\n\n--- purlis ---\n${note}\n--- end purlis ---`
    },

    // Once per prompt, before the model is asked. The session's first prompt is its start:
    // opencode creates a session at the first prompt and names no hook for it, so the
    // briefing is asked for here. What purlis says rides the prompt as one more text part.
    "chat.message": async (input, output) => {
      const sid = input?.sessionID
      if (typeof sid !== "string" || parents.has(sid)) return
      const notes = []
      if (!started.has(sid)) {
        started.add(sid)
        notes.push(context(await run("sessionstart", {
          hook_event_name: "SessionStart",
          session_id: sid,
          cwd: directory,
          source: created.has(sid) ? "startup" : "resume",
        }, sid)))
      }
      const prompt = (output?.parts ?? [])
        .filter((part) => part?.type === "text" && typeof part.text === "string")
        .map((part) => part.text)
        .join("\n")
      notes.push(context(await run("userpromptsubmit", {
        hook_event_name: "UserPromptSubmit",
        session_id: sid,
        cwd: directory,
        prompt,
      }, sid)))
      const text = notes.filter(Boolean).join("\n\n")
      if (!text || !Array.isArray(output?.parts)) return
      output.parts.push({
        id: `prt_charter${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`,
        sessionID: sid,
        messageID: output.message?.id,
        type: "text",
        text,
        synthetic: true,
      })
    },

    // What a turn does, as the app draws it. Not awaited: a report must never hold opencode up.
    event: async ({ event }) => {
      const props = event?.properties
      switch (event?.type) {
        case "session.created": {
          const info = props?.info
          if (typeof info?.id !== "string") return
          if (typeof info.parentID === "string") parents.set(info.id, info.parentID)
          else created.add(info.id)
          return
        }
        case "session.idle": {
          const sid = props?.sessionID
          if (typeof sid !== "string" || parents.has(sid)) return
          void run("stop", { hook_event_name: "Stop", session_id: sid, cwd: directory }, sid)
          return
        }
        case "permission.asked": {
          // A sub-agent's question is the chat's: the operator answers it in the same pane.
          const sid = rootOf(props?.sessionID)
          if (!sid) return
          void run("notification", {
            hook_event_name: "Notification",
            session_id: sid,
            cwd: directory,
            notification_type: "permission_prompt",
            message: `opencode asks permission to use ${String(props?.permission ?? "a tool")}`,
          }, sid)
          return
        }
      }
    },"#;

/// The hooks the installed guard registers.
const GUARD_HOOKS: &str = r#"    "tool.execute.before": before,"#;

/// The shim's text around the parts [`shim`] fills in.
const TEMPLATE: &str = r#"{{MARK}}
{{VARIANT}}
//
// No policy lives here. Each tool call goes to the `purlis hook` word that guards it, with
// the payload a Claude Code hook gets, and every decision is purlis's. A tool call is
// refused by throwing: opencode then runs nothing and hands the model the message.
//
// A guard that cannot answer does not allow. A `purlis` that crashed, timed out or said
// something this cannot read refuses the call, as purlis's own guard refuses when it crashes.
// A `purlis` that is not there at all {{MISSING}} it: the app's own chat refuses, and the copy
// `purlis plugin install` wrote allows, as a Claude Code or Codex hook whose program is gone
// does, because it would otherwise refuse every tool call in every opencode on the machine
// once that purlis moved. `purlis doctor` names a copy whose purlis is gone.

const BINARY = {{BINARY}}

// How a hook's stderr begins when the app did not take its line.
const NOT_TAKEN = {{NOT_TAKEN}}

const ROUTES = {{ROUTES}}

const TIMEOUTS = {{TIMEOUTS}}

const DEFAULT_PRE = {{DEFAULT_PRE}}

const EFFECTFUL = {{EFFECTFUL}}

// What a `purlis` that is not there does to a tool call.
const MISSING = "{{MISSING}}"

// Taken when this file loads, so a plugin loaded after it that replaces one of these does not
// reach the guard. opencode loads every plugin into one realm; this narrows that and does not
// close it (purlis reports the other plugins opencode loads).
const hasOwn = Object.hasOwn
const parse = JSON.parse
const stringify = JSON.stringify
const spawn = Bun.spawn
const env = { ...process.env }

// A table is asked for its OWN key, so a tool id such as `constructor` or `toString` is not
// found on the prototype and routed to a function.
const route = (tool, field) => {
  if (typeof tool !== "string" || !hasOwn(ROUTES, tool)) return undefined
  const value = ROUTES[tool][field]
  return typeof value === "string" ? value : undefined
}

const toolId = (input) => (typeof input?.tool === "string" ? input.tool : "")

// What a hook said, as its JSON, or `undefined` for nothing or for something unreadable.
const answer = (said) => {
  const text = said.out.trim()
  if (!text) return null
  try {
    return parse(text)
  } catch {
    return undefined
  }
}

// The context a hook asked to add to the conversation, or "".
const context = (said) => {
  if (said.code !== 0) return ""
  const extra = answer(said)?.hookSpecificOutput?.additionalContext
  return typeof extra === "string" ? extra : ""
}

// Why a tool call is refused, or null when it may run.
const refusal = (said) => {
  if (said.missing && MISSING === "allows") return null
  if (said.code === 2) return said.err.trim() || "purlis refused this tool call"
  if (said.code !== 0) {
    return `purlis's guard could not answer (${said.err.trim() || `exit ${said.code}`}), and a guard that could not answer does not allow`
  }
  const got = answer(said)
  if (got === undefined) return "purlis's guard answered something it cannot have meant, so this tool call is refused"
  const decision = got?.hookSpecificOutput
  if (decision?.permissionDecision !== "deny") return null
  return typeof decision.permissionDecisionReason === "string" && decision.permissionDecisionReason
    ? decision.permissionDecisionReason
    : "purlis refused this tool call"
}

// Everything below is per opencode instance: one server may host several directories.
// `options` is what the config that named this file handed it, if anything.
export const CharterPlugin = async (plugin, options) => {
  const directory = typeof plugin?.directory === "string" ? plugin.directory : process.cwd()

  // Sessions this process created, and each sub-agent session's parent: a sub-agent's events are
  // its chat's, and its prompts are not the operator's.
  const created = new Set()
  const started = new Set()
  const parents = new Map()

  const rootOf = (sid) => {
    let at = typeof sid === "string" ? sid : ""
    for (let hops = 0; parents.has(at) && hops < 64; hops++) at = parents.get(at)
    return at
  }

  // A line the app did not take is said in this window, never dropped in silence: a chat
  // purlis wraps cannot keep it for later (ruling V73c). The hook's own sentence, at most
  // once in a while, so a closed app does not bury the pane in them.
  let toldAt = 0
  const notTaken = (said) => {
    const line = String(said?.err ?? "").split("\n").find((it) => it.startsWith(NOT_TAKEN))
    if (!line || Date.now() - toldAt < 30000) return
    toldAt = Date.now()
    try {
      const shown = plugin?.client?.tui?.showToast?.({
        body: { title: "charter", message: line, variant: "warning", duration: 15000 },
      })
      if (shown && typeof shown.catch === "function") shown.catch(() => {})
    } catch {}
  }

  // `purlis hook <word>` with `payload` on stdin: its exit status, stdout and stderr.
  const run = async (word, payload, sid) => {
    if (!BINARY) return { code: -1, out: "", err: "no purlis to run ({{BINARY_ENV}} is not set)", missing: true }
    let child
    try {
      child = spawn([BINARY, "hook", word], {
        cwd: directory,
        env: { ...env, PURLIS_SESSION_ID: rootOf(sid), CHARTER_SESSION_ID: rootOf(sid) },
        stdin: new Blob([stringify(payload)]),
        stdout: "pipe",
        stderr: "pipe",
      })
    } catch (e) {
      return { code: -1, out: "", err: `could not start purlis: ${e}`, missing: true }
    }
    // The deadline races the answer rather than waiting on the kill: a program that ignores
    // the signal, or leaves a child holding the pipe open, must not hold the tool call.
    const seconds = hasOwn(TIMEOUTS, word) ? TIMEOUTS[word] : {{DEADLINE}}
    let timer
    const late = new Promise((resolve) => {
      timer = setTimeout(() => {
        child.kill(9)
        resolve({ code: -1, out: "", err: `no answer within ${seconds}s` })
      }, seconds * 1000)
    })
    const answered = Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]).then(
      ([out, err, code]) => ({ code, out, err }),
      (e) => ({ code: -1, out: "", err: String(e) }),
    )
    let said
    try {
      said = await Promise.race([answered, late])
    } finally {
      clearTimeout(timer)
    }
    notTaken(said)
    return said
  }

  // Awaited before the tool runs; throwing is what refusing is.
  const before = async (input, output) => {
    const tool = toolId(input)
    const args = output?.args ?? {}
    const workdir = typeof args?.workdir === "string" && args.workdir ? args.workdir : directory
    const why = refusal(await run(route(tool, "pre") ?? DEFAULT_PRE, {
      hook_event_name: "PreToolUse",
      session_id: rootOf(input?.sessionID),
      cwd: tool === "bash" ? workdir : directory,
      tool_name: route(tool, "name") ?? tool,
      tool_input: args,
    }, input?.sessionID))
    if (why !== null) throw new Error(why)
  }

  return {
{{HOOKS}}
  }
}
"#;

#[cfg(test)]
mod tests;
