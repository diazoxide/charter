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

Kept dependency-light (only imports :mod:`charter.persona`) so it's cheap to run on
every Bash call.
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
}


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


def decide(command: str):
    """Return ``(persona, tool)`` if the active persona may run this, else None."""
    from . import persona

    name = persona.resolve_active()
    if not name:
        return None
    tools = persona.effective_tools(name)  # own tools + those of personas it `uses:`
    if not tools:
        return None
    binary, args = _parse(command)
    if not binary or binary not in tools:
        return None
    if _is_dangerous(binary, args):
        return None  # declared, but a destructive subcommand → fall back to a prompt
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
        result = decide(command)
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
