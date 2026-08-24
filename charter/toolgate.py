"""PreToolUse gate: auto-approve the tools the ACTIVE persona declares.

Wired via a Bash ``PreToolUse`` hook. Reads the tool JSON on stdin; if the
command is a **single, simple** invocation of a tool the active persona's
``tools:`` lists, it emits an ``allow`` decision so it runs without a prompt.
Otherwise it stays silent → the normal permission flow applies.

Two deliberate properties:

- **Never denies.** The worst case is "no auto-approval" → a normal prompt. So a
  bug here can't block work, only fail to smooth it.
- **Conservative parsing.** It refuses to auto-allow anything with shell
  composition (pipes, ``;``, ``&&``, ``$()``, redirects) or a wrapper
  (``sudo``/``bash -c`` become the "binary" and won't match a tool) — so the
  gate can't be used to smuggle an unapproved command past the prompt.

The unit of approval is a **binary**, and every argument rides along with it. That
is the feature (an operator writing ``tools: gh`` means `gh`), and it is also where
the whole class of holes lives, so four rules bound it — each of them "decline to
smooth", never "deny":

- :data:`_DANGEROUS` — a declared binary's destructive subcommands still prompt,
  ``charter secret``/``charter vault`` among them.
- :data:`_INTERPRETERS` — a binary whose *argument* is the real command (``bash``,
  ``python3``, ``xargs``, ``sudo``…) is a declaration of every command, so it is
  never smoothed. Declaring one has to stay a declaration of one thing.
- :func:`_touches_control_surface` — whatever the binary, a command naming a vault
  file or one of charter's own state/definition files is never smoothed. That is the
  same rule the Bash leak guard applies to `cat`, applied to the argv rather than to
  a list of programs charter happened to think of.
- :func:`frozen_tools` — the answer is bounded by what ``tools:`` said when the session
  began. ``persona.md`` is a file the model can write; without this, one approved
  edit is unprompted execution for the rest of the session (#432).

Kept dependency-light (only imports :mod:`charter.persona`, plus a lazy
:mod:`charter.hooks`/:mod:`charter.session` on the paths that need them) so it's
cheap to run on every Bash call.
"""

from __future__ import annotations

import json
import os
import re
import sys

_ENV_ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
_UNSAFE = (";", "|", "&", "`", "\n", ">", "<")

#: Even when a persona declares a tool, these subcommands are too destructive to
#: auto-approve — they fall back to a normal prompt. A subcommand matches if it
#: appears as a bare (non-flag) token anywhere in the command. Read-only verbs
#: (kubectl get/describe/logs/…, glab … list/view) are unaffected.
_DANGEROUS = {
    "kubectl": {
        "delete", "drain", "cordon", "uncordon", "taint", "evict",
        "replace", "exec", "attach", "cp", "port-forward", "proxy", "run",
    },
    "glab": {"delete", "remove"},
    # AgentMail: reads (list/get/search) auto-approve; sending mail is an outward
    # action and deletes are destructive — those keep prompting.
    "agentmail": {"send", "reply", "forward", "delete", "remove"},
    # charter itself (#424). `charter secret exec|cp|get --reveal` is the same verb
    # `kubectl exec` already carves out, doing something strictly more sensitive: it
    # puts a credential in a process. `vault` writes the registry those paths read.
    # Everything else charter does — `persona show`, `workspace save`, `trace` — is
    # untouched, so a plane that declares `tools: charter` keeps what it declared it
    # for. `edm` is charter's pre-rename name, kept for the reason `hooks._CHARTER_PROGS`
    # keeps it: one extra string against a silent gap on a machine still running it.
    "charter": {"secret", "vault"},
    "edm": {"secret", "vault"},
}

#: Binaries whose ARGUMENT is the command. Declaring one of these declares every
#: command there is — `tools: python3` reads as "this persona writes Python", not as
#: "this persona may read its own vault and POST it anywhere" — so the gate never
#: smooths them (#439). It still never denies: the operator who genuinely wants this
#: gets a normal prompt, which is the control that was being removed.
#:
#: Wrappers (`env`, `xargs`, `sudo`, `timeout`…) are here for the same reason, and
#: package runners (`npx`, `uvx`…) because their argument is an arbitrary program
#: fetched from a registry.
_INTERPRETERS = frozenset("""
    sh bash zsh fish dash ksh mksh csh tcsh ash busybox
    python python2 python3 pypy pypy3 ipython node nodejs deno bun ts-node tsx
    perl ruby irb php lua luajit tclsh osascript groovy scala jshell java
    Rscript R julia elixir iex erl escript
    awk gawk mawk nawk sed expect
    env xargs nohup setsid sudo doas su nice ionice stdbuf script chroot unshare
    time timeout watch command builtin exec eval parallel find make
    npx pnpx bunx uvx uv pipx pip pip3 poetry rye deno_run
""".split())

#: The same names carrying a version suffix — `python3.12`, `php8.2`, `node20`. A
#: guard that knows `python3` and not `python3.12` is the demo, not the class.
_VERSIONED = re.compile(
    r"^(?:python|pypy|node|deno|bun|perl|ruby|php|lua|bash|sh|zsh|ksh|tclsh|"
    r"julia|scala|pip|uv)[0-9]+(?:[._-][0-9]+)*$")

#: charter's own control surface, in argv. Two kinds of file, one rule: a *vault*
#: (the leak guard's :data:`charter.hooks._VAULT_PATH_RE`, imported rather than
#: re-spelled), and the files that decide what this gate itself will answer — anything
#: under `.charter/` (per-developer state: the active-persona pointer, session
#: pointers, the tool ceiling below) and the persona definitions that carry `tools:`.
#:
#: Scanned over the WHOLE command, not just the arguments after the binary, so a
#: leading `VAULT=.charter/vaults/x.json` assignment cannot carry the path past it.
_SELF_PATH_RE = re.compile(r"\.charter/|persona\.md|personas/\.default")


def _norm(text: str) -> str:
    """Fold the path spellings that mean the same file: `//`, `/./`, and case.

    `.charter//vaults/x.json` and `.Charter/vaults/x.json` name the same file on the
    filesystems charter runs on, and a guard that only knows the canonical spelling is
    one substitution away from silence.
    """
    t = text.replace("\\", "/")
    for _ in range(4):                      # bounded: each pass strictly shortens
        n = re.sub(r"/(?:\./)+", "/", re.sub(r"/{2,}", "/", t))
        if n == t:
            break
        t = n
    return t.lower()


def _touches_control_surface(command: str) -> bool:
    """True when the command names a vault file or one of charter's own files.

    The binary is not consulted on purpose. `_leak_reason` asks "is this program a
    reader?", which is answerable for `cat` and hopeless for `python3 -c …` or
    `curl --data-binary @…` — and this gate's job is narrower than denying: it only
    has to decline to *remove the prompt* from a command reaching for a credential.
    """
    from .hooks import _VAULT_PATH_RE       # one regex for one question, not two
    text = _norm(command)
    return bool(_VAULT_PATH_RE.search(text) or _SELF_PATH_RE.search(text))


def _is_interpreter(binary: str) -> bool:
    return binary in _INTERPRETERS or bool(_VERSIONED.match(binary))


def _parse(command: str):
    """(binary, arg_tokens) for a simple command, or (None, None) if not simple."""
    if not command or "$(" in command or any(ch in command for ch in _UNSAFE):
        return None, None
    tokens = command.strip().split()
    i = 0
    while i < len(tokens) and _ENV_ASSIGN.match(tokens[i]):
        i += 1  # skip leading VAR=value assignments (e.g. KUBECONFIG=… kubectl …)
    if i >= len(tokens):
        return None, None
    return os.path.basename(tokens[i]), tokens[i + 1:]


def _is_dangerous(binary: str, args: list[str]) -> bool:
    bad = _DANGEROUS.get(binary)
    if not bad:
        return False
    return any(tok in bad for tok in args if not tok.startswith("-"))


# --------------------------------------------------------------------------- #
# The session ceiling: what `tools:` said before this session could edit it     #
# --------------------------------------------------------------------------- #
def _ceiling_file(sid: str):
    from . import config
    return config.SESSIONS_DIR / f"{sid}.tools"


def snapshot(session_id: str | None = None) -> dict:
    """Freeze every persona's declared tools for this session; return what was written.

    ``personas/<n>/persona.md`` and the active-persona pointer are files in the working
    tree, read on every hook call — so before this, one approved edit to a `tools:` line
    was unprompted execution for the rest of the session, no restart and no commit
    (#432). This is the "before": the tools the operator authored, recorded at
    SessionStart, and consulted afterwards instead of re-reading a file the agent has had
    a turn to rewrite.

    **Every** persona, not just the active one, because a mid-session `charter persona
    use <other>` is an ordinary thing to do and must keep working. It moves within a set
    that existed before the session did; a persona invented afterwards is in no snapshot
    and is granted nothing.

    Returns ``{}`` when nothing could be persisted — including when there is no session
    id. A ceiling that cannot be stored must not read as "no ceiling", which is why the
    caller below treats an empty map as "approve nothing" rather than falling back to the
    working tree.
    """
    from . import persona, session as _session
    sid = _session.current(session_id)
    if not sid:
        return {}
    try:
        data = {n: sorted(persona.effective_tools(n)) for n in persona.list_personas()}
    except Exception:
        return {}
    try:
        f = _ceiling_file(sid)
        f.parent.mkdir(parents=True, exist_ok=True)
        tmp = f.with_name(f.name + ".tmp")
        tmp.write_text(json.dumps(data, sort_keys=True))
        os.replace(tmp, f)
    except OSError:
        return {}
    return data


def frozen_tools(name: str, session_id: str | None = None):
    """The tools *name* declared when this session began, or ``None`` when no session is
    identified at all.

    ``None`` is the one case that keeps today's behaviour: a harness that names no
    session has nothing to key a ceiling on, and a gate that silently stopped working
    there would be a regression nobody could see. Every other outcome is a real set —
    possibly empty, which means "approve nothing", which costs a prompt.

    Trust-on-first-use when no snapshot exists: opencode has no SessionStart hook
    (`harness/opencode.py:160`), so its first gated Bash call takes the snapshot. That
    freezes the session from that point rather than from its beginning — weaker, stated
    rather than papered over, and still strictly better than re-reading the file every
    call.
    """
    from . import session as _session
    sid = _session.current(session_id)
    if not sid:
        return None
    try:
        data = json.loads(_ceiling_file(sid).read_text())
    except (OSError, ValueError):
        data = snapshot(sid)
    if not isinstance(data, dict):
        return set()
    vals = data.get(name)
    return set(vals) if isinstance(vals, list) else set()


def decide(command: str, session_id: str | None = None):
    """Return ``(persona, tool)`` if the active persona may run this, else None."""
    from . import persona

    name = persona.resolve_active()
    if not name:
        return None
    tools = persona.effective_tools(name)  # own tools + those of personas it `uses:`
    frozen = frozen_tools(name, session_id)
    if frozen is not None:
        # Intersection, not replacement: a `tools:` line the operator NARROWS takes
        # effect at once (fail toward less), while one widened mid-session grants
        # nothing until the next session (fail toward less again).
        tools &= frozen
    if not tools:
        return None
    binary, args = _parse(command)
    if not binary or binary not in tools:
        return None
    if _is_interpreter(binary):
        return None  # declaring an interpreter declares every command — keep the prompt
    if _is_dangerous(binary, args):
        return None  # declared, but a destructive subcommand → fall back to a prompt
    if _touches_control_surface(command):
        return None  # reaches a vault or charter's own state → keep the prompt
    if not _provenance_ok(name, command, binary):
        return None  # a name charter owns, invoked from somewhere charter did not put it
    return name, binary


def _provenance_ok(name: str, command: str, binary: str) -> bool:
    """True unless *binary* names one of the persona's own scripts and the command is
    reaching a DIFFERENT file of that name.

    `_parse` reduces a command to `os.path.basename`, which is right for `gh` or `kubectl`:
    they are system binaries, the plane does not own them, and their location is not
    charter's business. For a persona's own script it inverts the guarantee — the
    declaration looks specific and the check is not, so `/tmp/site-health.sh` inherits the
    auto-approval of `personas/seo/bin/site-health.sh`, including a `/tmp` copy an agent
    wrote seconds earlier.

    Tightened only where charter has ground truth. A declared name with no script behind it
    is left exactly as it was: charter has nothing to compare against, and inventing a
    restriction would break planes that declare an ordinary binary with a script-shaped
    name.
    """
    from . import persona

    scripts = persona.bin_scripts(name)
    owned = scripts.get(binary)
    if owned is None:
        return True
    token = next((t for t in command.strip().split() if os.path.basename(t) == binary), "")
    if os.path.basename(token) == token:
        return False  # a bare name resolves through PATH, which charter cannot vouch for
    try:
        return os.path.realpath(token) == os.path.realpath(owned)
    except OSError:
        return False


def main(argv=None) -> int:
    try:
        data = json.load(sys.stdin)
    except Exception:
        return 0
    command = ((data or {}).get("tool_input") or {}).get("command", "")
    try:
        result = decide(command, (data or {}).get("session_id"))
    except Exception:
        result = None
    if result:
        name, binary = result
        print(json.dumps({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "permissionDecisionReason": f"persona '{name}' declares '{binary}' in its tools",
            }
        }))
    return 0
