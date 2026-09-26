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
//!   ("a guard that could not answer does not allow");
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
//! list, `charter doctor`) rather than claiming a containment it does not have. This is the
//! limit the Python charter's `foreign_plugins` named, and it is the same class as a project
//! `.claude/settings.json` that runs its own hook: guard rails, not guarantees.

use std::path::{Path, PathBuf};

/// Where the shim sits inside the bundled plugin directory ([`crate::plugin`]). Claude Code
/// reads only the directories it knows in a plugin, so this one rides beside them unread.
pub const SHIM: &str = "opencode/charter.ts";

/// The variable opencode reads a whole config from, merged over every other config it reads.
pub const CONFIG_ENV: &str = "OPENCODE_CONFIG_CONTENT";

/// The variable that makes opencode load no external plugin, when it is `1` or `true`.
pub const PURE_ENV: &str = "OPENCODE_PURE";

/// The flag that sets [`PURE_ENV`] from inside opencode, past anything charter can set.
pub const PURE_FLAG: &str = "--pure";

/// The first line of every shim charter writes, which is how `charter plugin install` knows a
/// file in opencode's plugin directory is its own to replace or remove.
pub const MARK: &str = "// charter's opencode plugin, generated by charter. Do not edit.";

/// The first line of the retired Python charter's shim (`charter/harness/opencode.py`), which
/// `charter plugin install` replaces: it is charter's own artifact, and two shims would guard
/// every tool call twice and brief every chat twice.
pub const PYTHON_MARK: &str = "// charter-version: ";

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
    let (binary, variant) = match arming {
        Arming::Session => (
            format!("process.env.{} || \"\"", crate::plugin::BINARY_ENV),
            "// The app loads this file for one chat, and names its own `charter` in the chat's environment.",
        ),
        Arming::GuardOnly(path) => (
            serde_json::Value::from(path.display().to_string()).to_string(),
            "// `charter plugin install` put this here: the guards alone, for opencode started outside the app.",
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
    let hooks = match arming {
        Arming::Session => SESSION_HOOKS,
        Arming::GuardOnly(_) => GUARD_HOOKS,
    };
    TEMPLATE
        .replace("{{MARK}}", MARK)
        .replace("{{VARIANT}}", variant)
        .replace("{{BINARY}}", &binary)
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
pub fn session_config(shim: &Path) -> String {
    let mut url = String::from("file://");
    for byte in shim.display().to_string().bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            url.push(char::from(byte));
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    serde_json::json!({ "plugin": [url] }).to_string()
}

/// Why a chat started with `command` and `env` would run without charter's plugin — `None`
/// when it would not.
///
/// `command` is the profile's whole argv. Only the literal flag is seen: a wrapper script that
/// adds `--pure` itself is past what charter can read, as a wrapper that adds any flag is.
pub fn disarmed_by(command: &[String], env: &[(String, String)]) -> Option<String> {
    if command.iter().any(|word| word == PURE_FLAG) {
        return Some(format!(
            "its command passes {PURE_FLAG}, which makes opencode load no plugin — charter's \
             guard included"
        ));
    }
    for (name, _) in env {
        if name == PURE_ENV {
            return Some(format!(
                "it sets {PURE_ENV}, which can make opencode load no plugin — charter's guard \
                 included"
            ));
        }
        if name == CONFIG_ENV {
            return Some(format!(
                "it sets {CONFIG_ENV}, which is how charter hands opencode its plugin for the \
                 chat; put that config in opencode.json instead"
            ));
        }
    }
    None
}

/// The hooks an app chat's shim registers.
const SESSION_HOOKS: &str = r#"    // `$CHARTER_SESSION_ID` in every shell a tool opens, so a `charter` command run there
    // knows its conversation, as `$CLAUDE_CODE_SESSION_ID` tells it in a Claude Code chat.
    "shell.env": async (input, output) => {
      const sid = rootOf(input?.sessionID)
      if (sid && output?.env) output.env.CHARTER_SESSION_ID = sid
    },

    "tool.execute.before": before,

    // The tallies and nudges charter keeps after a tool ran. A note rides the tool's own
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
      output.output = `${output.output ?? ""}\n\n--- charter ---\n${note}\n--- end charter ---`
    },

    // Once per prompt, before the model is asked. The session's first prompt is its start:
    // opencode creates a session at the first prompt and names no hook for it, so the
    // briefing is asked for here. What charter says rides the prompt as one more text part.
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
// No policy lives here. Each tool call goes to the `charter hook` word that guards it, with
// the payload a Claude Code hook gets, and every decision is charter's. A tool call is
// refused by throwing: opencode then runs nothing and hands the model the message.
//
// A guard that cannot answer does not allow. A `charter` that is missing, crashed, timed out
// or said something this cannot read refuses the call, as charter's own guard refuses when
// it crashes.

const BINARY = {{BINARY}}

const ROUTES = {{ROUTES}}

const TIMEOUTS = {{TIMEOUTS}}

const DEFAULT_PRE = {{DEFAULT_PRE}}

const EFFECTFUL = {{EFFECTFUL}}

// Taken when this file loads, so a plugin loaded after it that replaces one of these does not
// reach the guard. opencode loads every plugin into one realm; this narrows that and does not
// close it (charter reports the other plugins opencode loads).
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
  if (said.code === 2) return said.err.trim() || "charter refused this tool call"
  if (said.code !== 0) {
    return `charter's guard could not answer (${said.err.trim() || `exit ${said.code}`}), and a guard that could not answer does not allow`
  }
  const got = answer(said)
  if (got === undefined) return "charter's guard answered something it cannot have meant, so this tool call is refused"
  const decision = got?.hookSpecificOutput
  if (decision?.permissionDecision !== "deny") return null
  return typeof decision.permissionDecisionReason === "string" && decision.permissionDecisionReason
    ? decision.permissionDecisionReason
    : "charter refused this tool call"
}

// Everything below is per opencode instance: one server may host several directories.
export const CharterPlugin = async (plugin) => {
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

  // `charter hook <word>` with `payload` on stdin: its exit status, stdout and stderr.
  const run = async (word, payload, sid) => {
    if (!BINARY) return { code: -1, out: "", err: "no charter to run (CHARTER_HOOK_BINARY is not set)" }
    let child
    try {
      child = spawn([BINARY, "hook", word], {
        cwd: directory,
        env: { ...env, CHARTER_SESSION_ID: rootOf(sid) },
        stdin: new Blob([stringify(payload)]),
        stdout: "pipe",
        stderr: "pipe",
      })
    } catch (e) {
      return { code: -1, out: "", err: `could not start charter: ${e}` }
    }
    const seconds = hasOwn(TIMEOUTS, word) ? TIMEOUTS[word] : 10
    const timer = setTimeout(() => child.kill(), seconds * 1000)
    try {
      const [out, err, code] = await Promise.all([
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
        child.exited,
      ])
      return { code, out, err }
    } catch (e) {
      return { code: -1, out: "", err: String(e) }
    } finally {
      clearTimeout(timer)
    }
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
