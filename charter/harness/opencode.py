"""The opencode plugin charter generates, and nothing else about opencode.

charter ships to PyPI; opencode loads JS/TS through Bun. Publishing an npm package would
add a third artifact with a third version number to a repo that has paid for version skew
in four of its last five releases, so `charter init` **writes** the plugin instead (ADR
0015). One artifact, one version, nothing to keep in sync.

The shim carries no policy. It answers one question — which harness is this — by putting
`$CHARTER_HARNESS` into every shell opencode spawns, so `harness.current()` has something
to read. Decisions stay in Python, where they are tested.
"""

from __future__ import annotations

import json
import os
import re
from pathlib import Path

from .. import __version__
from .base import Deficit, Harness

NAME = "opencode"

#: ``opencode tool id -> the tool name charter's guards match on``.
#:
#: charter's `PreToolUse` handler asks `tool_name == "Bash"` and reads
#: `tool_input["command"]`; opencode calls the same tool `bash`. Rather than keep a table
#: in TypeScript and another here, the mapping lives in Python and is generated into the
#: shim — the repo's own lesson that "if two paths answer the same question, call the same
#: function".
#:
#: A tool NOT in this map is forwarded under its own id. That is deliberate: charter's
#: guards ignore names they do not know, and inventing a CamelCase name for a tool charter
#: has never heard of would be guessing.
TOOL_NAMES = {
    "bash": "Bash",
    "read": "Read",
    "write": "Write",
    "edit": "Edit",
    "glob": "Glob",
    "grep": "Grep",
    "webfetch": "WebFetch",
    "task": "Task",
}

#: opencode's OWN permission names — the table charter's rule joins, not a list charter
#: invents. Read off opencode 1.18.21 itself rather than off its docs: the running
#: server's `/experimental/tool/ids`, the built-in ruleset `Permission.fromConfig` is
#: seeded with (`{"*":"allow", doom_loop:"ask", external_directory:{…}, question:"deny",
#: plan_enter:"deny", plan_exit:"deny", read:{…}}`), the three MCP-resource tool ids
#: (`list_mcp_resources`, `list_mcp_resource_templates`, `read_mcp_resource`), and the
#: "Known permission keys" list the binary carries for its own config authoring.
#:
#: It matters because `Permission.evaluate` glob-matches the permission NAME and takes
#: the LAST match — `findLast((r) => match(name, r.permission) && match(pattern,
#: r.pattern))` — and charter's rule comes from config, which resolves after the
#: built-ins. So a name that collides here does not sit beside opencode's decision, it
#: replaces it.
#:
#: Whole names with no `_` (`bash`, `read`, `list`) can never be hit by an MCP
#: translation, which always carries the separator. They are kept anyway: the list is
#: opencode's table, and `_shadowed_builtins` decides what is reachable. A hand-curated
#: "reachable" subset would need re-curating the day opencode adds an underscored name,
#: and would go stale silently — the exact failure this whole issue is about.
BUILTIN_PERMISSIONS = (
    "apply_patch", "bash", "doom_loop", "edit", "external_directory", "glob", "grep",
    "invalid", "list", "list_mcp_resource_templates", "list_mcp_resources", "lsp",
    "plan_enter", "plan_exit", "question", "read", "read_mcp_resource", "skill", "task",
    "todowrite", "webfetch", "websearch", "write",
)

#: Tools whose output charter may append to. Deliberately NOT the ones that return
#: content: a `read` whose output carries charter's nudge is a false record of that file,
#: and the agent may write it back. These three report an action instead, so a note
#: appended to them adds to the record rather than corrupting it.
#:
#: Claude Code needs no such list — its `additionalContext` arrives BESIDE the result.
#: This restriction is the price of opencode having no channel of its own.
EFFECTFUL_TOOLS = ("bash", "edit", "write")

def _shadowed_builtins(name: str) -> tuple[str, ...]:
    """The names in :data:`BUILTIN_PERMISSIONS` an opencode rule keyed *name* matches.

    opencode's own matcher, transcribed rather than approximated — `Wildcard.match`
    escapes ``[.+^${}()|[\\]\\\\]``, turns ``*`` into ``.*`` and ``?`` into ``.``, and
    anchors ``^…$`` with the `s` flag — which is what `fullmatch` spells in Python.

    Both anchors are load-bearing and each fails in its own direction. Without the
    leading one, `lan_*` "collides" with `plan_enter` and charter warns about something
    that cannot happen; without the trailing one, `plan_ent` "collides" too, and a rule
    keyed `plan_ent` matches nothing in opencode at all. One `fullmatch` rather than
    ``^…$`` plus `.match`, so there is one anchoring mechanism to be wrong about.
    """
    rx = re.escape(name).replace(r"\*", ".*").replace(r"\?", ".")
    matcher = re.compile(rx, re.S)
    return tuple(b for b in BUILTIN_PERMISSIONS if matcher.fullmatch(b))


def global_dir() -> Path:
    """Where opencode reads plugins, commands and config for EVERY project.

    Verified by putting a probe in `~/.config/opencode/plugin/` and booting `opencode
    serve` from an unrelated directory, where it loaded. The per-tree design existed
    because opencode does not walk upwards for *project* plugins — true, and irrelevant
    once the plugin is installed where every project already looks.

    `$XDG_CONFIG_HOME` first, for the same reason `$CODEX_HOME` is honoured: writing to a
    path the tool does not read is indistinguishable from not installing at all.
    """
    xdg = os.environ.get("XDG_CONFIG_HOME")
    return (Path(xdg) if xdg else Path.home() / ".config") / "opencode"


#: Relative to :func:`global_dir`.
SHIM_PATH = Path("plugin") / "charter.ts"

#: The on-demand status line. opencode has no status-bar socket, so the plane state is a
#: command instead of an ambient row. `.opencode/command/<name>.md` is the location, from
#: the binary's own docs table: "Project commands | `.opencode/command/<name>.md`".
COMMAND_PATH = Path("command") / "charter.md"

#: A command body may embed shell output with ``!`…` ``, so `/charter` prints the REAL
#: status line. Describing it to a model instead would spend a turn to get a worse answer.
COMMAND = """\
---
description: The control plane — active workspace, open todos, pieces, repos and CI.
---

The control plane's current state:

!`echo '{}' | charter statusline`

Read it and continue; nothing here needs a reply.
"""

#: The session context, for the harness that has no SessionStart hook. Named in the
#: tree's ``instructions`` so opencode reads it at startup.
CONTEXT_PATH = Path("charter-context.md")

#: The generated plugin. No imports, deliberately: `@opencode-ai/plugin` exists only for
#: type checking, and depending on it would send Bun to the network before charter's own
#: hook can run — a failure that would look like charter being broken.
_SHIM_TEMPLATE = '''\
// charter-version: %(version)s
// Generated by `charter init`. Safe to edit: charter writes this file only when it is
// absent and never repairs one it finds, so your changes survive (ADR 0015).
//
// Charter is a Python CLI. This plugin holds no policy of its own — it names the harness
// to every shell, and forwards tool calls to `charter hook pretooluse`, which is the same
// handler Claude Code calls. Every decision stays in Python, where it has tests.
//
// `input` is read on every call rather than cached: one opencode server hosts many
// sessions, so a module-level "current session" would have no correct value.

const TOOL_NAMES = %(tool_names)s

const EFFECTFUL = %(effectful)s

export const CharterPlugin = async ({ $, directory }) => {
  return {
    "shell.env": async (input, output) => {
      output.env.CHARTER_HARNESS = "opencode"
      if (input?.sessionID) output.env.CHARTER_SESSION_ID = input.sessionID
    },

    // Awaited before the tool runs, and `Plugin.trigger` wraps each hook in
    // `Effect.promise` with no try/catch — so throwing here is what denial IS.
    "tool.execute.before": async (input, output) => {
      const payload = {
        hook_event_name: "PreToolUse",
        session_id: input?.sessionID ?? "",
        cwd: directory,
        tool_name: TOOL_NAMES[input?.tool] ?? input?.tool ?? "",
        tool_input: output?.args ?? {},
      }
      let parsed
      try {
        // `< ${blob}` is how Bun's shell takes stdin; there is no such
        // METHOD on it. An earlier version called one, threw on every tool
        // call, and failed OPEN silently — the guard never fired while
        // everything looked wired. Verified against opencode 1.18.18.
        const res = await $`charter hook pretooluse < ${new Blob([JSON.stringify(payload)])}`
          .env({ ...process.env, CHARTER_HARNESS: "opencode",
                 CHARTER_SESSION_ID: input?.sessionID ?? "" })
          .quiet()
          .nothrow()
        const out = res.stdout.toString().trim()
        parsed = out ? JSON.parse(out) : null
      } catch (e) {
        // Charter absent, or it answered something this cannot read. A guard that
        // cannot run must not block the session — `doctor` is what reports an unwired
        // plane, and failing open here is the difference between a missing guard and an
        // unusable harness.
        return
      }
      const decision = parsed?.hookSpecificOutput?.permissionDecision
      if (decision === "deny") {
        throw new Error(parsed?.hookSpecificOutput?.permissionDecisionReason
          ?? "charter denied this command")
      }
    },

    // charter's mid-session nudges. On Claude Code they arrive beside the result; here
    // there is no such channel, so they ride the result itself — and only for EFFECTFUL
    // tools, because a `read` carrying appended text is a false record of that file.
    "tool.execute.after": async (input, output) => {
      if (!EFFECTFUL.includes(input?.tool)) return
      const payload = {
        hook_event_name: "PostToolUse",
        session_id: input?.sessionID ?? "",
        cwd: directory,
        tool_name: TOOL_NAMES[input?.tool] ?? input?.tool ?? "",
        tool_input: input?.args ?? {},
        tool_response: { output: String(output?.output ?? "") },
      }
      let note
      try {
        // `< ${blob}` is how Bun's shell takes stdin; there is no such
        // METHOD on it. An earlier version called one, threw on every tool
        // call, and failed OPEN silently — the guard never fired while
        // everything looked wired. Verified against opencode 1.18.18.
        const res = await $`charter hook posttooluse < ${new Blob([JSON.stringify(payload)])}`
          .env({ ...process.env, CHARTER_HARNESS: "opencode",
                 CHARTER_SESSION_ID: input?.sessionID ?? "" })
          .quiet()
          .nothrow()
        const out = res.stdout.toString().trim()
        note = out ? JSON.parse(out)?.hookSpecificOutput?.additionalContext : null
      } catch (e) {
        return
      }
      if (!note) return
      // Fenced, so nothing here can be read back as the tool's own output.
      output.output = `${output?.output ?? ""}\n\n--- charter ---\n${note}\n--- end charter ---`
    },
  }
}
'''

#: The generated plugin. No imports beyond what opencode hands the factory, deliberately:
#: `@opencode-ai/plugin` exists only for type checking, and depending on it would send Bun
#: to the network before charter's own hook can run — a failure that would look like
#: charter being broken.
#: The marker `shim_version` reads. First line of the file, so the read is cheap and a
#: truncated file still answers honestly.
_STAMP = "// charter-version: "

SHIM = _SHIM_TEMPLATE % {
    "version": __version__,
    "tool_names": json.dumps(TOOL_NAMES, indent=2, sort_keys=True),
    "effectful": json.dumps(list(EFFECTFUL_TOOLS)),
}


def shim_version(tree: Path) -> str | None:
    """Which charter wrote the shim in *tree*, or ``None``.

    ``None`` covers three cases that must not be told apart optimistically: no shim, a
    shim from before the stamp existed, and one somebody rewrote. Each means "not
    something this charter can vouch for", and the dangerous one — a 0.40.0 plugin whose
    guard threw on every tool call and failed open — is exactly the middle case.
    """
    p = Path(tree) / SHIM_PATH
    try:
        head = p.read_text().splitlines()[0]
    except (OSError, UnicodeDecodeError, IndexError):
        return None
    return head[len(_STAMP):].strip() if head.startswith(_STAMP) else None


def refresh_shim(tree: Path) -> str:
    """Regenerate the shim in *tree* when charter can still recognise it as its own.

    ``"created"`` (absent), ``"refreshed"`` (charter's, replaced), ``"not-ours"`` (no
    stamp — left untouched), ``"current"`` (already this version).

    The stamp is what makes this safe. `ensure_shim` may never repair, because it cannot
    ask whether the file is charter's; this can, and still refuses the moment the answer
    is no. An operator who edited the shim keeps their edit and gets told about it, which
    is the same trade `_load_settings` makes for a file charter half-owns.
    """
    p = Path(tree) / SHIM_PATH
    if not p.exists():
        return ensure_shim(tree) and "created"
    stamped = shim_version(tree)
    if stamped is None:
        return "not-ours"
    if stamped == __version__:
        return "current"
    p.write_text(SHIM)
    return "refreshed"


def ensure_shim(root: Path) -> str:
    """Write the plugin under *root* IF ABSENT. Returns ``"created"`` or ``"present"``.

    Never repairs a file it finds, which is `_load_settings`'s restraint for
    `.claude/settings.json` applied to a file charter has even less claim on: someone who
    edited the shim made a deliberate choice, and silently reverting it is worse than
    leaving a stale one — `doctor` is what notices staleness, and it can say why.
    """
    p = Path(root) / SHIM_PATH
    if p.exists():
        return "present"
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(SHIM)
    return "created"


def write_context(base: Path) -> str:
    """Regenerate the session context under *base*. Always overwrites.

    The one generated file charter DOES repair, and the exception is the point: the shim
    is the operator's to edit, while this is derived state. A stale context file is a lie
    about which workspace you are standing in, and a lie is worse than a missing file
    because nothing about it looks wrong.
    """
    from .. import hooks

    p = Path(base) / CONTEXT_PATH
    p.parent.mkdir(parents=True, exist_ok=True)
    body = hooks.context_block(None).strip()
    p.write_text("<!-- Generated by charter. Do not edit: rewritten whenever the plane's "
                 "state changes. -->\n\n" + (body or "_No control-plane context._") + "\n")
    return "created"


def ensure_instructions(base: Path) -> str:
    """Name :data:`CONTEXT_PATH` in *base*'s ``opencode.json`` ``instructions``.

    IF ABSENT, and additively: opencode combines every entry, so somebody else's
    `AGENTS.md` keeps working beside charter's. An unparseable config is theirs to fix —
    charter reports and never repairs.
    """
    p = Path(base) / "opencode.json"
    doc: dict = {}
    if p.exists():
        try:
            doc = json.loads(p.read_text())
        except (OSError, UnicodeDecodeError, json.JSONDecodeError):
            return "malformed"
        if not isinstance(doc, dict):
            return "malformed"
    entries = doc.setdefault("instructions", [])
    if not isinstance(entries, list):
        return "malformed"
    want = str(Path(base) / CONTEXT_PATH)
    if want in entries:
        return "present"
    entries.append(want)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(doc, indent=2) + "\n")
    return "created"


class OpenCodeHarness(Harness):
    name = NAME

    deficits = (
        Deficit("status-bar",
                "no status-bar socket: opencode has no `statusLine` config, so the plane "
                "state renders on demand via `/charter` rather than ambiently.", "charter statusline --watch"),
        Deficit("prompt-hook",
                "no per-turn prompt hook: charter's mid-session nudges ride the output of "
                "effectful tools (write/edit/bash) instead of arriving beside them."),
    )

    cli_name = "opencode"
    binary = "opencode"

    def stale_wiring(self) -> str:
        """The version that wrote the installed plugin, when it is not this one.

        ``""`` when current or absent — absent is `harness`'s other sentence, and two rows
        saying the plane is unwired teaches people to skim. ``"unstamped"`` for a plugin
        charter cannot vouch for, which includes every one written before the stamp.
        """
        g = global_dir()
        if not (g / SHIM_PATH).is_file():
            return ""
        got = shim_version(g)
        return "" if got == __version__ else (got or "unstamped")

    def detect(self) -> bool:
        """Nothing native to detect.

        opencode sets no environment variable of its own that a shell can see —
        ``$CHARTER_HARNESS`` here comes from :data:`SHIM`, which is charter's own code.
        So an opencode session in an unwired plane is genuinely undetectable, and saying
        so is better than guessing from a path that might mean something else.
        """
        return False

    def upgrade(self, root: Path) -> tuple[str, str]:
        """The one harness charter can move itself — via the writer that already exists.

        `refresh_shim` is `wire`'s writer, and it already encodes the only judgement that
        matters: the stamp says whether the file is charter's to rewrite. Answering this
        with a second writer would be two code paths for one question, which is the shape
        this whole member was added to remove.

        ``not-ours`` becomes ``manual`` rather than ``absent``: charter knows exactly how
        this moves, it is declining to overwrite an operator's edit (additive-only).
        """
        g = global_dir()
        got = refresh_shim(g)
        if got == "current":
            return "current", __version__
        if got in ("refreshed", "created"):
            return "moved", f"opencode {SHIM_PATH} → {__version__}"
        return "manual", (f"{g / SHIM_PATH} carries no charter stamp, so charter will not "
                          f"overwrite it — move it aside and run `charter reinit`")

    def wire(self, root: Path) -> list[tuple[str, str]]:
        """Install once, where opencode reads for every project.

        *root* is ignored: nothing is written into the plane. A plane is somebody's repo,
        and charter's housekeeping has no business in its `git status` — which is what the
        per-tree design cost, along with a `.git/info/exclude` entry per checkout to hide
        the evidence.
        """
        g = global_dir()
        # Labels are RELATIVE to the config dir, and say which harness they belong to.
        # An absolute path here is unbounded — a deep home directory turned `init`'s
        # summary into a 130-column line, which is the readability budget #231 set.
        # `refresh_shim`, not `ensure_shim`: an install that only ever creates is a
        # one-shot, and a plugin an older charter wrote then survives every upgrade while
        # `doctor` reports it wired. That was #233's bug per tree; moving to one global
        # file took the refresh path with it and brought the bug back.
        out = [(refresh_shim(g), f"opencode {SHIM_PATH}")]
        cmd = g / COMMAND_PATH
        if not cmd.exists():
            cmd.parent.mkdir(parents=True, exist_ok=True)
            cmd.write_text(COMMAND)
            out.append(("created", f"opencode {COMMAND_PATH}"))
        write_context(g)
        if ensure_instructions(g) == "created":
            out.append(("created", "opencode opencode.json (instructions)"))
        return out

    def ask_rule(self, pattern: str) -> tuple[str, str]:
        """``(tool, glob)``. opencode's permissions are `{tool: {pattern: decision}}` with
        `*`/`?` wildcards — not Claude Code's `Tool(pattern)` string, so the same operator
        sentence has to come apart differently here.

        An MCP pattern comes apart differently again, and used to fall through to `bash`
        instead — writing a rule over a bash command literally named `mcp__slack__send`,
        which nothing can ever run, under a tick saying the guard was in force (#374). The
        same silent direction #365 fixed for Claude Code, one harness over.

        Naming that limit and returning ``unsupported`` was the other honest answer, and it
        is unavailable because opencode CAN express this — the only thing that was wrong
        was the name. Verified against opencode 1.18.21:

        * MCP tools are registered under ``McpCatalog.toolName(server, tool)`` —
          ``sanitize(server) + "_" + sanitize(tool)``, ``sanitize`` being
          ``s.replace(/[^a-zA-Z0-9_-]/g, "_")`` — and the wrapper asks under exactly that
          id: ``ask({permission: <tool id>, patterns: ["*"]})``.
        * `permission` takes keys beyond the five it documents; `Permission.fromConfig`
          turns ``{"<id>": {"*": "ask"}}`` into ``{permission: "<id>", pattern: "*"}``,
          which `opencode debug agent build` prints back in the resolved rule list.
        * `Permission.evaluate` glob-matches the permission NAME as well as the pattern —
          how opencode's own ``{permission: "*"}`` default works — so a whole server is
          ``<server>_*``.

        `commands._MCP_RULE_RE` decides what an MCP pattern IS, rather than a second regex
        here: two harnesses disagreeing about that is how one of them ends up writing a
        rule the other refused. It also confines the pattern to ``[A-Za-z0-9_-]``, exactly
        the set opencode's `sanitize` leaves alone, so no character ever needs rewriting.

        What charter cannot check is that opencode's `mcp` block names the server the same
        way. That is the contract Claude Code's rule already has — the name is the
        operator's, not charter's guess — and `guard` prints what it wrote so they can read
        it back. Which is what makes that read-back load-bearing rather than cosmetic: it
        currently renders as the repr of this tuple (`('slack_send', '*')`) and not as
        anything `opencode.json` holds, so it is the one line an operator cannot check the
        translation against. Filed as #395 rather than fixed here — the return type is
        the harness rule interface, and three harnesses answer it.

        The whole-server glob is as tight as opencode's own names allow and no tighter:
        `_` is both the separator `toolName` joins with and a legal character either side
        of it, so ``slack_*`` covers a server called `slack_admin` too. No glob can tell
        those apart, and refusing the whole-server form over it would trade a rule that is
        occasionally wider than asked for one that does not exist. Worth saying because
        `allow_rule` shares this translation, and wider is the direction that costs
        something there.

        **And the sibling server is the cheap half of that.** opencode's own permission
        names live in the same flat namespace as the MCP tool ids, so a translated name
        can also collide with `BUILTIN_PERMISSIONS` — ``mcp__plan`` becomes ``plan_*``,
        which matches opencode's `plan_enter` and `plan_exit`, and `evaluate` takes the
        LAST match while config resolves after the built-ins. `charter guard allow
        mcp__plan` therefore does not merely reach a server that may not exist; it turns
        two of opencode's own denies into allows. Before #374 the same command wrote an
        inert `bash` rule and did nothing, so this widening is new here and charter's to
        name. :meth:`rule_outranks` names it at write time, the only moment the operator
        can still change their mind.
        """
        from .. import commands

        p = (pattern or "").strip()
        if commands._MCP_RULE_RE.match(p):
            server, _, tool = p[len("mcp__"):].partition("__")
            return f"{server}_{tool or '*'}", "*"
        for oc_id, name in TOOL_NAMES.items():
            for prefix in (f"{name}(", f"{oc_id}("):
                if p.startswith(prefix) and p.endswith(")"):
                    return oc_id, p[len(prefix):-1]
        return "bash", p

    def rule_outranks(self, pattern: str) -> str:
        """opencode's own permission names this rule will decide for too, or ``""``.

        **Only for an MCP pattern**, and that is the whole judgement rather than a
        shortcut. Every other rule charter writes here is keyed by an opencode built-in
        ON PURPOSE — `charter guard ask 'git push *'` lands on `bash` because `bash` is
        what the operator meant — so warning on those would fire on nearly every
        invocation and teach the operator to skip the line. An MCP pattern names a
        *server*, and landing
        on `plan_enter` is never what was meant: it is a collision between two namespaces
        opencode flattened into one, which is exactly the case nobody can see coming.

        Matched with opencode's own glob semantics rather than `fnmatch`, in
        `_shadowed_builtins`. `fnmatch` would differ on `[`, which `_MCP_RULE_RE` cannot
        admit today — so the difference is unreachable, and that is precisely why it is
        worth not depending on. The name being matched is the one charter WRITES, so this
        stays right if the translation ever changes.

        The narrower form is rebuilt from the NAME that was written, not echoed back from
        the operator's pattern. `_MCP_RULE_RE` admits a trailing separator — `mcp__plan__`
        is read as the whole server, same as `mcp__plan` — and echoing that back would
        advise `mcp__plan____<tool>`, four separators, a rule charter itself refuses. A
        remedy nobody can type is worse than none, because it reads as one that works.
        """
        from .. import commands

        p = (pattern or "").strip()
        if not commands._MCP_RULE_RE.match(p):
            return ""
        name, _glob = self.ask_rule(p)
        hit = _shadowed_builtins(name)
        if not hit:
            return ""
        plural = len(hit) > 1
        narrower = (f" Naming the tool instead (`mcp__{name[:-2]}__<tool>`) keeps this "
                    f"to your server, unless the tool is spelled like one of those."
                    if name.endswith("_*") else
                    " opencode has no narrower name for it — your server's tool and "
                    "opencode's own permission are spelled the same.")
        return (f"`{name}` also matches opencode's OWN "
                f"{'permissions' if plural else 'permission'} "
                f"{', '.join('`%s`' % h for h in hit)}, and opencode takes the LAST "
                f"matching rule — so this decides {'those' if plural else 'that'} too, "
                f"replacing opencode's built-in one.{narrower}")

    #: Declined deliberately, not unimplemented. opencode's only uncommitted config is
    #: `global_dir()` (`~/.config/opencode`), which applies to EVERY project on the machine.
    #: Trading a team-wide rule for an all-my-projects-wide one is not narrower, and an
    #: `allow` written there would silently widen every other repo this person opens. Saying
    #: so is the same restraint `apply_ask_rule` keeps for a harness with no patterns at all.
    _NO_LOCAL = ("opencode has no project-local uncommitted config — its only uncommitted "
                 "config is ~/.config/opencode, which applies to every project on this "
                 "machine, so charter will not narrow a team rule into a broader one")

    def apply_ask_rule(self, root: Path, pattern: str,
                       local: bool = False) -> tuple[str, str]:
        if local:
            return "unsupported", self._NO_LOCAL
        return self._apply_rule(root, pattern, "ask")

    def apply_allow_rule(self, root: Path, pattern: str,
                         local: bool = False) -> tuple[str, str]:
        if local:
            return "unsupported", self._NO_LOCAL
        return self._apply_rule(root, pattern, "allow")

    def _apply_rule(self, root: Path, pattern: str, decision: str) -> tuple[str, str]:
        """Write it into `opencode.json`, IF ABSENT and never repairing.

        A `permission` block of the wrong shape is somebody's deliberate structure, and
        an unparseable file is theirs to fix — the same restraint `_load_settings` keeps
        for the file charter half-owns.

        One writer for both verbs: opencode's model is `{tool: {glob: decision}}`, so
        `ask` and `allow` differ by a single string and a second copy would only be a
        place for the two to drift.
        """
        tool, glob = self.ask_rule(pattern)
        p = Path(root) / "opencode.json"
        doc: dict = {}
        if p.exists():
            try:
                doc = json.loads(p.read_text())
            except (OSError, UnicodeDecodeError, json.JSONDecodeError):
                return "malformed", str(p)
            if not isinstance(doc, dict):
                return "malformed", str(p)
        perms = doc.setdefault("permission", {})
        if not isinstance(perms, dict):
            return "malformed", f"{p} (`permission` is not an object)"
        block = perms.setdefault(tool, {})
        if not isinstance(block, dict):
            return "malformed", f"{p} (`permission.{tool}` is not an object)"
        if block.get(glob) == decision:
            return "present", str(p)
        block[glob] = decision
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(doc, indent=2) + "\n")
        return "added", str(p)
