"""The opencode plugin charter generates, and nothing else about opencode.

charter ships to PyPI; opencode loads JS/TS through Bun. Publishing an npm package would
add a third artifact with a third version number to a repo that has paid for version skew
in four of its last five releases, so `charter init` **writes** the plugin instead (ADR
0015). One artifact, one version, nothing to keep in sync.

The shim carries no policy. It answers one question — which harness is this — by putting
`$CHARTER_HARNESS` into every shell opencode spawns, so `harness.current()` has something
to read. Decisions stay in Python, where they are tested.

What the plugin is NOT is a boundary, and SECURITY.md's line is the honest one here:
"guard rails, not guarantees … a guard against mistakes, not an attacker with shell access
as your user". opencode imports every file in :data:`PLUGIN_DIR` into ONE module realm and
gives them all the same globals, so anything that can write a file next to the shim can
redefine what the shim calls — and nothing this module does changes that. What it can do
is refuse to say the realm is charter's when it is not: :func:`shim_is_charters` compares
the file charter wrote byte for byte, and :func:`foreign_plugins` names everything else
opencode will load. Reporting, not containment. The three rounds before this one each
closed one named way in and each reported a clean plane while the next one was open.
"""

from __future__ import annotations

import json
import os
import re
from pathlib import Path

from .. import __version__
from .base import WORKSPACE_SCOPE, Deficit, Harness

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
    "skill": "Skill",
    "webfetch": "WebFetch",
    "task": "Task",
}

#: ``opencode tool id -> the `charter hook` handler that guards it``, the PreToolUse half
#: of what `hooks/hooks.json` spells as matchers for Claude Code.
#:
#: One entry point for every tool was the original design, on the theory that "every
#: decision stays in Python, where it has tests". It was wrong, and #433 is what it cost:
#: `pretooluse` reads ``tool_input["command"]`` and guards Bash — it never looks at
#: `tool_name` — so the vault-read guard (`pretooluse_read`) was simply ABSENT here. The
#: Bash denial still fired and still NAMED the path it refused, while opencode's own
#: `read` on that same path was allowed. That is #90 verbatim, one harness over.
#:
#: The routing table is the manifest, in the one language that has both halves. A tool
#: with no entry falls to :data:`DEFAULT_PRE_HOOK` — which is the Bash guard, and which
#: ignores names it does not know.
PRE_HOOKS = {
    "read": "pretooluse-read",
    "grep": "pretooluse-read",
    "write": "pretooluse-edit",
    "edit": "pretooluse-edit",
    "task": "pretooluse-dispatch",
}

#: Where a tool with no :data:`PRE_HOOKS` entry goes. `hooks/hooks.json` registers this
#: one against `Bash`; here it is also the catch-all, because the handler tests
#: ``tool_input["command"]`` and a tool that carries none reaches nothing.
DEFAULT_PRE_HOOK = "pretooluse"

#: The PostToolUse half. No catch-all: unlike the pre hooks there is nothing safe to run
#: for a tool nobody wrote a handler for, and every one of these spawns a process.
POST_HOOKS = {
    "bash": "posttooluse-bash",
    "write": "posttooluse",
    "edit": "posttooluse",
    "skill": "posttooluse-skill",
    "task": "posttooluse-dispatch",
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

#: The permission names opencode accepts ONLY as a bare ``"ask"``/``"allow"``/``"deny"``,
#: never as a ``{pattern: decision}`` object. Writing the object form under one of these
#: does not lose the pattern — it makes the whole `opencode.json` invalid, and opencode
#: refuses to start in that project at all.
#:
#: A subset of :data:`BUILTIN_PERMISSIONS`, and it cannot be derived from it: nothing about
#: a name says which shape it takes. Two independent sources, both checked against opencode
#: 1.18.21 rather than reasoned from:
#:
#: * The published schema (``https://opencode.ai/config.json``). ``$defs.PermissionConfig``
#:   types twelve of its named keys ``PermissionRuleConfig`` (``anyOf`` a bare action or an
#:   object) and exactly these five ``PermissionActionConfig`` (``enum: ask|allow|deny``).
#:   Its ``additionalProperties: {$ref: PermissionRuleConfig}`` — which is what lets charter
#:   write an invented MCP name at all — rescues invented names only, never these five.
#: * The binary. Every one of the 23 names in :data:`BUILTIN_PERMISSIONS` was fed to
#:   `opencode debug agent build` in both shapes; these five and only these five answer
#:   ``Expected PermissionActionConfig | undefined, got {"*":"ask"}`` to the object form.
#:
#: A GLOB is not one of these — ``doom_*`` goes through `additionalProperties` and takes the
#: object form happily. Only the exact name is flat-only, which is why membership is tested
#: rather than matched with `_shadowed_builtins`.
FLAT_ONLY_PERMISSIONS = ("doom_loop", "question", "todowrite", "webfetch", "websearch")

#: Tools whose output charter may append to. Deliberately NOT the ones that return
#: content: a `read` whose output carries charter's nudge is a false record of that file,
#: and the agent may write it back. These three report an action instead, so a note
#: appended to them adds to the record rather than corrupting it.
#:
#: Claude Code needs no such list — its `additionalContext` arrives BESIDE the result.
#: This restriction is the price of opencode having no channel of its own.
#:
#: It gates the APPEND, not the dispatch — those became two questions when :data:`POST_HOOKS`
#: grew entries for `skill` and `task`. Running the handler is how a tally gets recorded;
#: writing into the result is what corrupts a record. Conflating them is what kept
#: `posttooluse-skill` and `posttooluse-dispatch` from ever running here.
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


#: The directory opencode loads plugins FROM, relative to :func:`global_dir`.
#:
#: The unit is the directory, not the file. opencode 1.18.21 imports every `.ts`/`.js`
#: entry it finds here — dotfiles included, subdirectories not — into ONE module realm,
#: verified against the installed binary by putting six differently-named probes in a
#: temp `$XDG_CONFIG_HOME` and booting `opencode serve` (`.ts`, `.js` and `.hidden.ts`
#: loaded; `.mjs`, `.txt` and `sub/nested.ts` did not). So "is charter's plugin the one
#: charter wrote" is a question about this directory, and asking it of one filename is
#: asking about a member of a set the loader does not treat as separable.
PLUGIN_DIR = Path("plugin")

#: Relative to :func:`global_dir`.
SHIM_PATH = PLUGIN_DIR / "charter.ts"

#: What charter writes into :data:`PLUGIN_DIR`. Exactly one name, and the point of
#: spelling it as a set is that :func:`foreign_plugins` SUBTRACTS it: anything else in
#: that directory is named, whatever it is called. A screen keyed on suspicious names
#: would be the fourth round of a list that goes one entry short — the first three were a
#: filename, a character class and a longer table of fields.
CHARTER_WROTE = frozenset({SHIM_PATH.name})

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
// Generated by `charter init`. Yours to edit: charter never overwrites a file whose stamp
// is the running version, so a change you make here survives every `init` and `reinit`
// (ADR 0015). It is not silent about it — `charter doctor` compares this file's BYTES to
// the ones charter generates, and names it as one charter cannot vouch for.
//
// It also names every other file in this directory, because opencode loads them all into
// one realm with shared globals: a plugin beside this one can redefine what the guards
// below call, and this file cannot defend itself against that. Charter reports the realm;
// it does not contain it (SECURITY.md — guard rails, not guarantees).
// `charter update` DOES rewrite a shim stamped with an older version: charter cannot
// regenerate that version to compare against, so it treats it as its own artifact to move.
//
// Charter is a Python CLI. This plugin holds no policy of its own — it names the harness
// to every shell, and forwards each tool call to the `charter hook` handler that guards
// it, which is the same handler Claude Code's hooks.json dispatches for that tool. Every
// decision stays in Python, where it has tests.
//
// The ROUTING is the part that has to be here, and forwarding everything to one handler
// was the bug (#433): `pretooluse` guards Bash by reading `tool_input.command` and never
// looks at the tool name, so `read` reached no guard at all while the Bash denial went on
// naming the vault path it had just refused.
//
// `input` is read on every call rather than cached: one opencode server hosts many
// sessions, so a module-level "current session" would have no correct value.

const TOOL_NAMES = %(tool_names)s

const PRE_HOOKS = %(pre_hooks)s

const DEFAULT_PRE_HOOK = %(default_pre_hook)s

const POST_HOOKS = %(post_hooks)s

const EFFECTFUL = %(effectful)s

// Every table above is indexed by a string opencode chose, and a plain `TABLE[key]` does
// not ask the table — it asks the whole prototype chain. `PRE_HOOKS["constructor"]` is
// `Object`, `["toString"]` is a function, and `??` never fires for either, so a tool with
// one of those ids used to spawn `charter hook function Object() { [native code] }`,
// charter exited non-zero, this shim failed OPEN, and the call reached no guard at all —
// not even the Bash catch-all. On the after-block the same lookup walked past `if (!hook)
// return`, the one gate that exists because there is no safe catch-all there.
//
// So ask the PROPERTY instead of screening names: is this an OWN key of this table, and is
// what it holds a string this may put on a command line? That answers "not in the table"
// for every inherited property name there is — `constructor`, `toString`, `valueOf`,
// `__proto__`, and whichever ones a future runtime adds — without this file carrying a
// list of them to go stale.
const own = (table, key) => {
  if (typeof key !== "string" || !Object.hasOwn(table, key)) return undefined
  const value = table[key]
  return typeof value === "string" ? value : undefined
}

// opencode's tool id, or "". Normalised ONCE per call so the routing lookup, the name
// translation and the EFFECTFUL test all read the same value, and so a `tool` that is not
// a string can never become a command-line word or a `tool_name` charter has to parse.
const toolId = (input) => (typeof input?.tool === "string" ? input.tool : "")

export const CharterPlugin = async ({ $, directory }) => {
  return {
    "shell.env": async (input, output) => {
      output.env.CHARTER_HARNESS = "opencode"
      if (input?.sessionID) output.env.CHARTER_SESSION_ID = input.sessionID
    },

    // Awaited before the tool runs, and `Plugin.trigger` wraps each hook in
    // `Effect.promise` with no try/catch — so throwing here is what denial IS.
    "tool.execute.before": async (input, output) => {
      const tool = toolId(input)
      const hook = own(PRE_HOOKS, tool) ?? DEFAULT_PRE_HOOK
      const payload = {
        hook_event_name: "PreToolUse",
        session_id: input?.sessionID ?? "",
        cwd: directory,
        tool_name: own(TOOL_NAMES, tool) ?? tool,
        tool_input: output?.args ?? {},
      }
      let parsed
      try {
        // `< ${blob}` is how Bun's shell takes stdin; there is no such
        // METHOD on it. An earlier version called one, threw on every tool
        // call, and failed OPEN silently — the guard never fired while
        // everything looked wired. Verified against opencode 1.18.18.
        const res = await $`charter hook ${hook} < ${new Blob([JSON.stringify(payload)])}`
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

    // charter's after-the-fact handlers: the tallies (skill use, dispatch) and the
    // mid-session nudges. On Claude Code a nudge arrives beside the result; here there is
    // no such channel, so it rides the result itself — and only for EFFECTFUL tools,
    // because a `read` carrying appended text is a false record of that file.
    //
    // Two questions, not one. POST_HOOKS decides whether the handler RUNS; EFFECTFUL
    // decides whether its answer may be written into the tool's output. Treating them as
    // one is what kept `skill` and `task` from ever being tallied on this harness.
    "tool.execute.after": async (input, output) => {
      const tool = toolId(input)
      const hook = own(POST_HOOKS, tool)
      if (!hook) return
      const payload = {
        hook_event_name: "PostToolUse",
        session_id: input?.sessionID ?? "",
        cwd: directory,
        tool_name: own(TOOL_NAMES, tool) ?? tool,
        tool_input: input?.args ?? {},
        tool_response: { output: String(output?.output ?? "") },
      }
      let note
      try {
        // `< ${blob}` is how Bun's shell takes stdin; there is no such
        // METHOD on it. An earlier version called one, threw on every tool
        // call, and failed OPEN silently — the guard never fired while
        // everything looked wired. Verified against opencode 1.18.18.
        const res = await $`charter hook ${hook} < ${new Blob([JSON.stringify(payload)])}`
          .env({ ...process.env, CHARTER_HARNESS: "opencode",
                 CHARTER_SESSION_ID: input?.sessionID ?? "" })
          .quiet()
          .nothrow()
        const out = res.stdout.toString().trim()
        note = out ? JSON.parse(out)?.hookSpecificOutput?.additionalContext : null
      } catch (e) {
        return
      }
      if (!note || !EFFECTFUL.includes(tool)) return
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
    "pre_hooks": json.dumps(PRE_HOOKS, indent=2, sort_keys=True),
    "default_pre_hook": json.dumps(DEFAULT_PRE_HOOK),
    "post_hooks": json.dumps(POST_HOOKS, indent=2, sort_keys=True),
    "effectful": json.dumps(list(EFFECTFUL_TOOLS)),
}

#: The bytes :func:`ensure_shim` writes, and the ONLY thing an installed file is compared
#: against. UTF-8 explicitly, and bytes rather than `str`, because `Path.read_text()` is
#: not a byte comparison in either direction: it decodes with the locale's encoding and
#: translates ``\r\n`` and lone ``\r`` to ``\n``, so three files with three different
#: SHA-256s all came back equal to this string and all four callers said "current".
SHIM_BYTES = SHIM.encode("utf-8")


def shim_version(tree: Path) -> str | None:
    """What the shim in *tree* SAYS wrote it, or ``None``.

    A name the file carries, not a fact about the file. ``None`` means the first line
    carries no stamp: no shim, or a shim from before the stamp existed. Both mean "not
    something this charter can vouch for", and the dangerous one — a 0.40.0 plugin whose
    guard threw on every tool call and failed open — is the second.

    What this does NOT answer is whether the body under the stamp is the program charter
    generates. A stamp is line 1, and line 1 is the one line an edit leaves alone; keep it
    and replace everything below it and this still reports the running version. That is
    :func:`shim_is_charters`'s question, and it is the one every caller here needs.
    """
    p = Path(tree) / SHIM_PATH
    try:
        head = p.read_text().splitlines()[0]
    except (OSError, UnicodeDecodeError, IndexError):
        return None
    return head[len(_STAMP):].strip() if head.startswith(_STAMP) else None


def shim_is_charters(tree: Path) -> bool:
    """Is the installed plugin the program charter generates — byte for byte?

    The IDENTITY question, asked of the content. :func:`shim_version` answers a weaker one
    that reads like this one: what the first line says. #433 shipped for four releases
    because a routing table nobody dispatched through still looked like routing; the same
    shape one layer up is a version comment over a body that guards nothing. Both were
    checked by name.

    Reproduced before this existed: install the shim, keep line 1, and replace
    ``own(PRE_HOOKS, tool) ?? DEFAULT_PRE_HOOK`` with the literal ``"pretooluse"``. Every
    `read` goes back to the Bash guard — #433 exactly, vault-read guard absent again —
    while `shim_version` said 0.51.0, `refresh_shim` said "current", `stale_wiring` said
    nothing and `doctor` printed a tick.

    Byte-for-byte, deliberately. "Which differences are harmless" is a judgement charter
    cannot make about JavaScript it did not write, and every looser rule is a list of
    permitted edits that goes one entry short. The cost of being strict is bounded and
    known: a re-indented shim earns a `doctor` WARN, and nothing overwrites it — charter
    reports, never repairs.

    And byte-for-byte is `read_bytes`, not `read_text`. The first spelling of this said
    "byte for byte" in its own docstring while comparing DECODED text: `Path.read_text()`
    picks the locale's encoding and applies universal-newline translation, so the same
    source written LF, CRLF and CR-only produced three SHA-256s and three "yes, this is
    charter's". Every guard in this audit so far has been a comparison in a space one
    transformation away from the one that matters.

    What this does NOT answer — and what :func:`foreign_plugins` exists for — is whether
    this file is the only thing opencode loads. It is one member of a directory the loader
    imports whole, and code beside it shares its globals.
    """
    try:
        return (Path(tree) / SHIM_PATH).read_bytes() == SHIM_BYTES
    except OSError:
        return False


def _configured_plugins(tree: Path) -> tuple[str, ...]:
    """Whatever ``opencode.json``'s ``plugin`` key names, as written.

    The second door into the same realm, and charter already has this file open — `init`
    writes :data:`CONTEXT_PATH` into its ``instructions``. Reported verbatim rather than
    resolved: an npm specifier, a relative path and a bare name all land here, resolving
    them is opencode's job, and a resolver charter maintains would be a fourth thing to be
    wrong about. The answer this needs is "charter did not write this", which the raw
    entry already is.
    """
    try:
        doc = json.loads((Path(tree) / "opencode.json").read_text())
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return ()
    entries = doc.get("plugin") if isinstance(doc, dict) else None
    if isinstance(entries, str):        # opencode takes a bare string here too
        entries = [entries]
    if not isinstance(entries, list):
        return ()
    return tuple(str(e) for e in entries)


def foreign_plugins(tree: Path) -> tuple[str, ...]:
    """Everything in opencode's plugin realm under *tree* that charter did not write.

    The property, not a list of names. charter wrote exactly :data:`CHARTER_WROTE` into
    :data:`PLUGIN_DIR`; this enumerates what is actually there and subtracts that, so a
    file called `aaa_boot.ts`, `.hidden.ts`, `zzz.js` or whatever the next one is called
    all give the same answer, and so does an entry in ``opencode.json``'s ``plugin`` key.

    Why this is a security answer and not tidiness. opencode imports the whole directory
    into ONE realm and hands every plugin the same globals. Reproduced against opencode
    1.18.21 and the real shim: leave `charter.ts` byte-for-byte identical, drop
    `plugin/aaa_boot.ts` containing ``Object.hasOwn = () => false`` beside it, and the
    shim's own `own()` helper returns `undefined` for every lookup — `read` on
    `.charter/vaults/…` routes to `pretooluse` (the Bash guard, which never looks at
    `tool_name`) and is ALLOWED, while the after-block's `if (!hook) return` swallows
    every tally and the committed-secret scan. `shim_is_charters` said True throughout.

    Deliberately a SUPERSET of what opencode loads. The probe says `.ts`/`.js`/dotfiles
    yes, `.mjs`/`.txt`/subdirectories no — and charter reports every entry anyway, because
    a filter written from today's probe is a list that goes stale the release opencode
    accepts one more suffix. Over-reporting costs a `doctor` line about a stray README;
    under-reporting costs the vault.

    Naming is all this does. charter cannot stop a file it did not write from running —
    see the module docstring — and does not try to.
    """
    d = Path(tree) / PLUGIN_DIR
    try:
        here = sorted(p.name for p in d.iterdir())
    except OSError:
        here = []
    out = [str(PLUGIN_DIR / n) for n in here if n not in CHARTER_WROTE]
    out += [f"opencode.json plugin: {e}" for e in _configured_plugins(tree)]
    return tuple(out)


def unvouched(tree: Path) -> tuple[str, ...]:
    """Every reason charter cannot vouch for the plugin realm under *tree*, each a
    sentence naming what to DO about it. Empty when charter can vouch for all of it.

    One function with three callers — `upgrade`, `wire` and (through them) `init`,
    `reinit` and `update` — because the round before this one had three renderers for one
    question and they disagreed. `doctor` warned and ended "→ charter reinit"; `reinit`
    printed "Up to date — nothing to do" and said nothing about the shim it had just
    declined to touch; `init` listed that same shim under "already present"; `update`
    printed the honest sentence and then contradicted it with "`charter reinit` adds what
    is missing", when nothing was missing. A remedy that reports success and changes
    nothing is worse than no remedy, because it ends the investigation.

    An OLDER stamp is the one case with a remedy charter can carry out itself, so it says
    so. The two WRITERS never see it: `refresh_shim` runs first in both of them and has
    already replaced the file by the time this is asked. `doctor` does not write, so for
    `doctor` it is live — and it is the state where "→ charter reinit" was true all along,
    which is what made the invented hint plausible everywhere else.
    """
    g = Path(tree)
    out: list[str] = []
    p = g / SHIM_PATH
    if p.is_file() and not shim_is_charters(g):
        stamped = shim_version(g)
        if stamped == __version__:
            out.append(f"{p} is stamped {__version__} but is not what charter generates, "
                       f"so charter will not overwrite it — diff it, then move it aside "
                       f"and run `charter reinit`")
        elif stamped is None:
            out.append(f"{p} carries no charter stamp, so charter will not overwrite it — "
                       f"move it aside and run `charter reinit`")
        else:
            out.append(f"{p} is stamped {stamped}; charter wrote that version and will "
                       f"replace it — run `charter reinit`")
    for entry in foreign_plugins(g):
        out.append(f"{g}: `{entry}` is not charter's, and opencode loads it into the same "
                   f"realm as charter's plugin, where it shares the globals every guard "
                   f"call goes through — remove it, or keep it as code you trust with "
                   f"this plane's vaults")
    return tuple(out)


def refresh_shim(tree: Path) -> str:
    """Regenerate the shim in *tree* when charter can still recognise it as its own.

    ``"created"`` (absent), ``"refreshed"`` (an older charter's, replaced), ``"not-ours"``
    (no stamp — left untouched), ``"edited"`` (this version's stamp over a body charter
    did not write — left untouched), ``"current"`` (byte-for-byte what charter generates).

    ``"current"`` is decided by :func:`shim_is_charters` and not by the stamp, because the
    stamp is a name and this answer is charter vouching for a file. `ensure_shim` may
    never repair, because it cannot ask whether the file is charter's; this can, exactly
    once — for the version it can regenerate. For an older stamp it cannot, so "somebody
    edited a 0.40.0 shim" and "a 0.40.0 shim" are still one case, and the trade there is
    unchanged: charter rewrites its own artifact on upgrade.

    Either way an operator's edit to the CURRENT shim now survives and is reported, which
    is the same trade `_load_settings` makes for a file charter half-owns.
    """
    p = Path(tree) / SHIM_PATH
    if not p.exists():
        return ensure_shim(tree) and "created"
    if shim_is_charters(tree):
        return "current"
    stamped = shim_version(tree)
    if stamped is None:
        return "not-ours"
    if stamped == __version__:
        return "edited"
    p.write_bytes(SHIM_BYTES)
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
    p.write_bytes(SHIM_BYTES)
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
        # DENIES are carried in full — `tool.execute.before` throwing is what denial IS,
        # and that is the half the vault guard and the one-credential rule use. What has
        # no spelling here is the middle answer: a hook that returns `ask` gets a decision
        # and a sentence, and opencode's plugin API takes neither. Throwing would turn a
        # question into a refusal, which is a different answer, so charter allows and says
        # so here. Named rather than left implicit because #433 was precisely a guard that
        # looked wired and was not — a routed handler whose answer is dropped is the same
        # shape one layer down.
        #
        # Not `charter guard`'s asks: those become rules in `opencode.json` and opencode
        # prompts for them itself. This is charter's own tool-time asks — the routing and
        # overlapping-dispatch nudges.
        Deficit("ask-decisions",
                "no ask channel at tool time: opencode's `tool.execute.before` can allow "
                "or throw, so charter's own tool-time asks (the routing and "
                "overlapping-dispatch nudges) allow and are not shown. Denials are "
                "unaffected — they throw, and every guard that refuses still refuses."),
        # NOT the same shape as the three above, and worth saying so: those are surfaces
        # opencode lacks. This one is a surface it has *too widely*. Charter's layer is
        # already live in every workspace directory here — which is why #850 is a
        # Claude-Code-only defect — and the price of that reach is that two workspaces on
        # one machine cannot be made to differ. Charter reports the ceiling rather than
        # printing a tick beside Claude Code's, because "already everywhere" and
        # "isolated per workspace" are different answers and only one of them was asked
        # for.
        #
        # **The REASON was rewritten, not the conclusion.** This said opencode's config is
        # machine-global full stop, and that is false: an `opencode.json` at the
        # REPOSITORY ROOT is read — a malformed one fails the run outright, `Error: Config
        # file at <repo>/opencode.json is not valid JSON(C)` — and `.opencode/agent/` is
        # read from the project (measured, 1.18.23, with a real session; `opencode agent
        # list` alone is a management CLI and answers for the wrong thing). What is still
        # true is what the ceiling claims: a workspace DIRECTORY is not a repository root,
        # so every workspace under the plane resolves to the plane's own root and no two
        # of them can be made to differ.
        Deficit(WORKSPACE_SCOPE,
                "project config is keyed to the REPOSITORY ROOT (`opencode.json` there, "
                "plus machine-global `~/.config/opencode/`), and a workspace directory is "
                "not a repository root — every workspace under the plane resolves to the "
                "plane's own root, so charter's layer is already live in all of them and "
                "cannot be made to DIFFER between two."),
    )

    #: Charter writes NO in-repo layer for opencode today, so there is nothing for
    #: `doctor`'s `session layer` row to look for and the row says where the layer does
    #: come from instead.
    #:
    #: **The second sentence is the one that matters.** An in-repo surface exists here and
    #: charter simply does not use it yet — measured against opencode 1.18.23 with a real
    #: session, not with `opencode agent list`, because a management CLI is not a session
    #: and answers for the wrong thing. Naming the surface is what stops "charter writes
    #: no layer here" from being read as "there is nowhere to write one".
    layer_note = (
        "charter writes no in-repo layer for opencode — it arrives from "
        "`~/.config/opencode/` and the plugin, which every directory on this machine "
        "reads. opencode DOES read an in-repo `opencode.json` at the repository root and "
        "`.opencode/agent/` from the project (measured, 1.18.23); charter mirrors the "
        "plane's `.opencode/agent/` into a workspace's checkouts, and neither writes nor "
        "mirrors `opencode.json` — that is where `charter guard` keeps this plane's own "
        "permission grants")

    #: What a checkout inside a workspace stops seeing, in opencode's spelling (#868).
    #:
    #: Measured against opencode 1.18.23 with a real session: a sentinel at
    #: ``<repo>/.opencode/agent/probe.md`` is a project agent where a control repository
    #: has none. That is the surface #868 is about — the plane's agents, which a clone's
    #: own git root cuts off exactly as it cuts off Claude Code's.
    #:
    #: **`opencode.json` is measured, is read, and is deliberately NOT here.** opencode
    #: does read it at a repository root — malformed JSON at ``<repo>/opencode.json`` fails
    #: the run outright — so a clone genuinely stops seeing the plane's copy, and #868's
    #: own table lists it. Charter still does not mirror it, because of what is in it:
    #: :meth:`_apply_rule` writes `charter guard`'s rules there, so a plane's copy holds
    #: `permission` — and ``charter guard allow "npm test *"`` puts
    #: ``{"bash": {"npm test *": "allow"}}`` in that file. Copying it into a checkout would
    #: put an ALLOW in force in a repository nobody granted it in.
    #:
    #: Charter already answered this question one harness over and answered it the other
    #: way: :data:`claude_code.WORKSPACE_KEYS` mirrors three keys of the plane's settings
    #: into a checkout and refuses `permissions`, *"because copying a grant sideways into a
    #: directory nobody granted it in puts a permission in force where no one clicked for
    #: it"*. The same mechanism writing into the same directory must not answer it two
    #: opposite ways for two harnesses.
    #:
    #: The split is capability versus grant, and it is why `inherited_paths` mirrors while
    #: `workspace_files` generates: a mirror cannot drop a key. A generated guest
    #: `opencode.json` carrying the safe half is the honest way to close the rest, and it
    #: is a separate piece of work — charter's own opencode layer is machine-global and
    #: already reaches a checkout, so nothing of CHARTER's is missing there today.
    #:
    #: **This does not touch the ceiling in :data:`WORKSPACE_SCOPE` above and does not
    #: contradict it.** That ceiling is about a workspace DIRECTORY, which is not a
    #: repository root — every one of them resolves to the plane's own root, so the layer
    #: is already live there and cannot be made to DIFFER between two. A clone at
    #: `workspaces/<ws>/<repo>/` is a repository root, which is exactly why it was getting
    #: nothing, and exactly why charter can write here.
    inherited_paths = (".opencode/agent",)

    cli_name = "opencode"
    binary = "opencode"

    def stale_wiring(self) -> str:
        """What the installed plugin REALM is, when charter cannot vouch for all of it.

        A phrase the caller drops into "its plugin is …", so every answer has to read as
        one. ``""`` only when charter's file is byte-for-byte its own AND nothing else
        loads beside it — or when the file is absent, which is `harness`'s other sentence
        and two rows saying the plane is unwired teaches people to skim.

        Two independent facts, joined, because they fail independently and each has been
        reported clean while the other was false:

        * the FILE. Decided by content (:func:`shim_is_charters`), not by the stamp — a
          stamp is a name a file carries, and "the running version" was reported for a body
          with every guard cut out of it, because line 1 is the one line an edit leaves
          alone.
        * the REALM (:func:`foreign_plugins`). Decided by what is in the directory, not by
          which filename charter looked up. The file was byte-for-byte perfect and a
          sibling in the same realm turned every routing lookup into `undefined`, sending
          a vault `read` to the Bash guard, which allowed it.

        Both are the same mistake at different scales — charter asked about a NAME it chose
        instead of about the thing the loader actually acts on.
        """
        g = global_dir()
        if not (g / SHIM_PATH).is_file():
            return ""
        parts = []
        if not shim_is_charters(g):
            got = shim_version(g)
            if got is None:
                parts.append("unstamped")
            elif got == __version__:
                parts.append(f"stamped {got} but changed since charter wrote it")
            else:
                parts.append(got)
        else:
            parts.append("charter's own")
        foreign = foreign_plugins(g)
        if foreign:
            n = len(foreign)
            parts.append(f"opencode also loads {n} {'thing' if n == 1 else 'things'} "
                         f"charter did not write into the same realm "
                         f"({', '.join(foreign)}), sharing its globals")
        elif parts == ["charter's own"]:
            return ""
        return "; ".join(parts)

    def wiring_remedy(self) -> str:
        """:func:`unvouched`, joined. The same sentences :meth:`upgrade` returns and
        `wire` reports, so `doctor`, `init`, `reinit` and `update` cannot drift apart
        again — they did, and the one that was right was the one nobody was sent to."""
        return "; ".join(unvouched(global_dir()))

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

        ``not-ours`` and ``edited`` become ``manual`` rather than ``absent``: charter knows
        exactly how this moves, it is declining to overwrite an operator's edit
        (additive-only). They are two different sentences because they are two different
        files — one carries no stamp at all, the other carries this version's stamp over a
        body charter did not write, and telling somebody their plugin "carries no charter
        stamp" when it plainly does sends them looking for the wrong thing. Both sentences
        live in :func:`unvouched`, which is also what `wire` reports and what `doctor`'s
        hint used to paraphrase wrongly.

        A realm charter cannot vouch for is ``manual`` too, and it is the case that makes
        "current" a dangerous answer rather than a stale one: the shim can be on this
        version, byte-for-byte, and still be neutered by whatever loaded beside it.
        """
        g = global_dir()
        got = refresh_shim(g)
        blocked = unvouched(g)
        if blocked:
            return "manual", "; ".join(blocked)
        if got == "current":
            return "current", __version__
        return "moved", f"opencode {SHIM_PATH} → {__version__}"

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
        #
        # A shim charter cannot vouch for is NOT reported as an installed item. `init`
        # used to list it under "already present", which is true about the filename and
        # false about everything the reader takes from it; the reasons carry the sentence
        # instead, and the caller renders them as warnings.
        status = refresh_shim(g)
        out: list[tuple[str, str]] = []
        if status not in ("edited", "not-ours"):
            out.append((status, f"opencode {SHIM_PATH}"))
        cmd = g / COMMAND_PATH
        if not cmd.exists():
            cmd.parent.mkdir(parents=True, exist_ok=True)
            cmd.write_text(COMMAND)
            out.append(("created", f"opencode {COMMAND_PATH}"))
        write_context(g)
        if ensure_instructions(g) == "created":
            out.append(("created", "opencode opencode.json (instructions)"))
        out += [("unvouched", why) for why in unvouched(g)]
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
        it back. Which is what makes that read-back load-bearing rather than cosmetic, and
        why the pair returned here is no longer what gets printed: it rendered as this
        tuple's repr (`('slack_send', '*')`), which appears in no `opencode.json` there
        has ever been. :meth:`rule_text` spells it the way the file does (#395); this
        stays the pair `_apply_rule` writes from.

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

        **A collision can also decide the file's SHAPE, not just its meaning.** Five of
        those names — :data:`FLAT_ONLY_PERMISSIONS` — take a bare action string and reject
        the ``{glob: decision}`` object this pair describes, invalidating the whole file
        rather than the one rule. The pair returned here is unchanged, because it is still
        what opencode resolves to (`{"doom_loop": "ask"}` builds
        ``{permission: doom_loop, pattern: "*"}``, measured); `_apply_rule` chooses the
        shape, so the operator's read-back stays true to the rule that is in force.
        """
        from .. import commands

        p = (pattern or "").strip()
        if commands._MCP_RULE_RE.fullmatch(p):
            server, _, tool = p[len("mcp__"):].partition("__")
            return f"{server}_{tool or '*'}", "*"
        for oc_id, name in TOOL_NAMES.items():
            for prefix in (f"{name}(", f"{oc_id}("):
                if p.startswith(prefix) and p.endswith(")"):
                    return oc_id, p[len(prefix):-1]
        return "bash", p

    def rule_text(self, pattern: str) -> str:
        """The rule as `opencode.json` holds it: ``permission.slack_send."*"``.

        A path into the file, so the operator can open it and land on the same key. The
        base default would have printed :meth:`ask_rule`'s pair, and a 2-tuple's repr
        (`('slack_send', '*')`) is not a spelling anything on disk uses — which mattered
        the day #374 started translating `mcp__slack__send` into a name the operator
        never typed and charter cannot verify against their `mcp` block (#395).

        **The shape follows the write, because `_apply_rule` chooses between two.** A
        :data:`FLAT_ONLY_PERMISSIONS` key takes a bare action and holds no glob at all —
        the file says ``"doom_loop": "ask"`` — so printing ``permission.doom_loop."*"``
        would send the reader looking for a key that is not there, over the one rule
        shape where charter's own writer already had to know better. The object form is
        kept when the glob is not `*`, which is the case `_apply_rule` REFUSES: nothing
        is written, `guard` prints nothing, and the description stays true to what was
        asked rather than quietly implying the glob was dropped.

        The glob is JSON-quoted rather than pasted bare. It carries the operator's own
        words — ``permission.bash."git push *"`` — and a spaced pattern run together with
        a dotted path is unreadable at exactly the moment the line is being read
        carefully. `json.dumps` is what wrote the file, so the quoting the reader sees
        here is the quoting the file uses.
        """
        tool, glob = self.ask_rule(pattern)
        if tool in FLAT_ONLY_PERMISSIONS and glob == "*":
            return f"permission.{tool}"
        return f"permission.{tool}.{json.dumps(glob)}"

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

        Asked with a pattern and no verb, so it answers for `ask` and `allow` alike — and
        it reads the name back through :meth:`ask_rule` for both. That is right only while
        this harness keeps `base.allow_rule`'s shared default, which is the whole point of
        that default ("keeps one operator sentence from acquiring two spellings"). An
        override here would have to reach this too, and
        `TestBothVerbsTranslateTheSameWay` fails the day one appears rather than leaving
        the allow path quietly naming the ask path's collisions.

        What it claims is only that charter's rule is the LAST word on those names. Not
        that it replaces a rule opencode wrote for each: opencode seeds name-specific
        defaults for a handful (`doom_loop`, `question`, `plan_enter`, `plan_exit`,
        `read`, `external_directory`) and covers the rest with one ``{permission: "*"}``
        allow — so `list_*` outranks a catch-all, while `plan_*` outranks two real denies.
        Both are "this decides those too"; only one is a replacement, and the seeded set
        is partly machine-specific (`external_directory` carries local paths), so it is
        not charter's to enumerate in a sentence.
        """
        from .. import commands

        p = (pattern or "").strip()
        if not commands._MCP_RULE_RE.fullmatch(p):
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
                f"ahead of whatever opencode had decided for "
                f"{'them' if plural else 'it'}.{narrower}")

    #: Declined deliberately, not unimplemented. opencode's only uncommitted config is
    #: `global_dir()` (`~/.config/opencode`), which applies to EVERY project on the machine.
    #: Trading a team-wide rule for an all-my-projects-wide one is not narrower, and an
    #: `allow` written there would silently widen every other repo this person opens. Saying
    #: so is the same restraint `apply_ask_rule` keeps for a harness with no patterns at all.
    _NO_LOCAL = ("opencode has no project-local uncommitted config — its only uncommitted "
                 "config is ~/.config/opencode, which applies to every project on this "
                 "machine, so charter will not narrow a team rule into a broader one")

    def apply_ask_rule(self, root: Path, pattern: str, local: bool = False,
                       dry_run: bool = False) -> tuple[str, str]:
        if local:
            return "unsupported", self._NO_LOCAL
        return self._apply_rule(root, pattern, "ask", dry_run=dry_run)

    def apply_allow_rule(self, root: Path, pattern: str, local: bool = False,
                         dry_run: bool = False) -> tuple[str, str]:
        if local:
            return "unsupported", self._NO_LOCAL
        return self._apply_rule(root, pattern, "allow", dry_run=dry_run)

    def _apply_rule(self, root: Path, pattern: str, decision: str,
                    dry_run: bool = False) -> tuple[str, str]:
        """Write it into `opencode.json`, IF ABSENT and never repairing.

        A `permission` block of the wrong shape is somebody's deliberate structure, and
        an unparseable file is theirs to fix — the same restraint `_load_settings` keeps
        for the file charter half-owns.

        One writer for both verbs: opencode's model is `{tool: {glob: decision}}`, so
        `ask` and `allow` differ by a single string and a second copy would only be a
        place for the two to drift.

        **Except for :data:`FLAT_ONLY_PERMISSIONS`, where that model is not opencode's.**
        Those five names take a bare action string, and the object form does not merely
        fail to match — it makes the whole file invalid and opencode refuses to start in
        the project. Two names are reachable: `mcp__doom__loop` translates to `doom_loop`,
        and `WebFetch(...)` is keyed `webfetch` through `TOOL_NAMES`. So the shape has to
        be chosen per key rather than assumed, and the choice splits on the glob:

        * ``*`` — write the flat form. Measured, not assumed to be equivalent:
          `opencode debug agent build` resolves ``{"doom_loop": "ask"}`` to
          ``{permission: doom_loop, pattern: "*", action: "ask"}``, the same entry in the
          same last-wins position the object form was reaching for. Nothing is lost.
        * anything else — ``unsupported``, with the reason. The flat form would silently
          drop the pattern and apply the decision to EVERY fetch, and an `allow` widened
          from one URL to all of them is the failure `--local`'s own refusal exists to
          prevent. `Harness.apply_ask_rule` keeps ``unsupported`` for exactly this: a
          pattern the harness genuinely cannot express, named rather than approximated.

        This is the same defect as #374 in the other direction, and worse: #374 wrote a
        rule that could not fire, this wrote a file that stops opencode running. It was
        introduced by #374's own fix, which gave `doom_loop` its new meaning — before it,
        `mcp__doom__loop` was an inert `bash` key and the file still loaded.

        `dry_run` is the same restraint applied one step earlier, and for the same reason
        this method is shared: every judgement above the write runs, and only the two
        lines that touch the disk are skipped. `charter guard` asks every harness before
        it writes any of them (#376), so this answer has to be the one the write would
        give — and it is, because it is the same code arriving at it.

        That sharing is what keeps the flat-only branch above honest under a transaction.
        Its ``unsupported`` and its ``malformed`` are reached by the check exactly as the
        write reaches them, so `_guard_apply` blocks the whole command on the second and
        steps over the first — and a `WebFetch(https://x/*)` that opencode cannot express
        does not stop Claude Code from taking the rule it CAN express. A separate
        validator would have had to learn :data:`FLAT_ONLY_PERMISSIONS` a second time and
        would have been the copy that went stale when opencode adds a sixth name.
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
        if tool in FLAT_ONLY_PERMISSIONS:
            if glob != "*":
                return "unsupported", (
                    f"opencode's `{tool}` permission takes only a bare ask/allow/deny, "
                    f"never a per-pattern rule, so `{glob}` cannot be written there — and "
                    f"dropping it would apply this to every `{tool}` instead of the one "
                    f"you named")
            existing = perms.get(tool)
            if existing is not None and not isinstance(existing, str):
                return "malformed", f"{p} (`permission.{tool}` is not an action)"
            if existing == decision:
                return "present", str(p)
            perms[tool] = decision
        else:
            block = perms.setdefault(tool, {})
            if not isinstance(block, dict):
                return "malformed", f"{p} (`permission.{tool}` is not an object)"
            if block.get(glob) == decision:
                return "present", str(p)
            block[glob] = decision
        if dry_run:
            return "added", str(p)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(doc, indent=2) + "\n")
        return "added", str(p)
