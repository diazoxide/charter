"""Which credentialed MCP command this operator has seen — and therefore approved.

A persona's ``personas/<name>/mcp.json`` declares an MCP server, and a ``secrets`` map on
that entry turns it into ``charter secret exec <vault> --env … --exec -- <command>``. The
mechanism is right: the value reaches the server's environment without passing through a
context window. What was missing is that **the same committed file chooses the command**
(#330). `sync-agents` then writes that argv into ``.claude/agents/<name>.md``, which the
harness loads and whose stdio servers it starts.

**Why this is consent and not an allowlist.** #317 was the same shape on a news ``check:``,
and PR #319 closed it with a list of commands a probe may run. That works there because a
``check:`` names a *charter subcommand* — a closed grammar charter defines, so "which
commands may run" has an enumerable answer, and `news._pass_through` can even read the
dangerous shape off the argparse parser rather than naming it. None of that transfers. An
MCP ``command`` is an arbitrary binary followed by arbitrary ``args``, so a list holding
the launchers real servers use (``npx``, ``uvx``, ``docker``, ``node``) is walked past by
``args`` alone — ``uvx --from git+https://… evil`` is #332's mechanism one field over —
and a list excluding them refuses every MCP server anyone actually runs. The axis with an
answer is not *what* the command is but *whether the operator has seen it*.

**The whole entry, and only what was shown.** Two properties the mechanism is worthless
without, and it shipped without both. The digest covers every key of the entry rather than
the handful charter reads, because `persona.mcp_render_entry` passes the rest through to
the harness — ``env`` did exactly that, so a committed edit could re-point an approved
server's ``PATH`` with the approval intact (#426). And an entry :func:`describe` cannot
render is not approvable at all, because the consent line IS the consent: an ``http``
server used to print a blank one under the words "read the command above" (#427).

**Machine-local and gitignored, deliberately.** Under ``STATE_DIR``, the same as
:mod:`charter.guardseen` and for a sharper reason: if the approval travelled in git, the
commit that declares the server could also declare that the server was approved, which is
the finding restored with an extra step.

**Withholding, not refusing.** An unapproved server is still written to the agent file —
only its credential is withheld. Deleting the server would break a working persona to
prevent a hypothetical, and charter's rule is additive: name the blocker, refuse the
dangerous half, and leave everything else working. The server starts and fails to
authenticate, which is a visible failure rather than a silent one.

**Nothing here raises.** A missing or corrupt marker reads as *nothing approved*, so the
failure direction is "the credential was withheld", never "sync-agents crashed" and never
"the credential was handed over because the file was unreadable".
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

from . import config

#: One file per plane, under the state dir. Never committed — see the module docstring.
FILE_NAME = "mcp-approved.json"

#: Printed in place of a consent line for an entry :func:`describe` cannot render. Such an
#: entry is reported as withheld and refused for approval, rather than silently dropped.
UNRENDERABLE = "(names neither a command nor a url — nothing to approve)"

#: Longest consent line printed. See :func:`describe`.
MAX_LINE = 600


def path() -> Path:
    return Path(config.STATE_DIR) / FILE_NAME


def needs_consent(vault: str | None, entry: dict) -> bool:
    """Would rendering *entry* hand *vault*'s value to the command a committed file names?

    The one question that decides whether there is anything to consent to, asked in one
    place so the approve path, the withheld report and the digest cannot disagree about
    which servers are in scope. Kept separate from :func:`fingerprint` because a digest of
    ``None`` now means "no approval can exist", which includes entries that ARE in scope
    and must still be reported.
    """
    if not vault or not isinstance(entry, dict):
        return False
    secrets, files = entry.get("secrets"), entry.get("secret_files")
    return bool((isinstance(secrets, dict) and secrets)
                or (isinstance(files, dict) and files))


def _canon(value):
    """Untrusted JSON as something :func:`json.dumps` renders deterministically.

    Recursive and total, rather than a list of fields: the WHOLE entry is digested, so a
    key charter does not know about yet cannot fall outside the fingerprint. ``env`` was
    exactly that key (#426) — copied verbatim into the generated agent file by
    `persona.mcp_render_entry`, handed to ``execvpe``, and invisible to the digest, so a
    committed edit could add ``NODE_OPTIONS`` or re-point ``PATH`` on an already-approved
    server without lapsing the approval.

    A value JSON cannot carry is tagged rather than stringified, so an exotic object
    cannot digest as the plain string that happens to be its ``repr``.
    """
    if isinstance(value, dict):
        return {str(k): _canon(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_canon(v) for v in value]
    if value is None or isinstance(value, (str, bool, int, float)):
        return value
    return ["<not-json>", repr(value)]


def fingerprint(vault: str | None, entry: dict) -> str | None:
    """What the operator is being asked to approve, as one digest.

    ``None`` when no approval can exist for this entry, which is two cases and both mean
    "render it without the vault wrapper":

    * **Nothing to consent to** — no ``secrets`` and no ``secret_files``, or no vault. The
      entry is passed through untouched by `persona.mcp_render_entry`, so no credential is
      at stake and requiring approval would be a prompt about nothing.
    * **Nothing to show** — :func:`describe` cannot render a destination for it, so the
      operator would be approving a blank line (#427). An entry nobody can be shown is not
      an entry anybody can approve.

    **Every field of the entry is in here**, which is the point: approving a server by
    name — or by five of its fields — lets a later commit re-point the same name at a
    different binary, a different endpoint, or a different environment while the approval
    stays valid. The vault (whose secrets) and the entry in full (where they go) both
    change the digest.
    """
    if not needs_consent(vault, entry) or not describe(entry):
        return None
    material = json.dumps({"vault": str(vault), "entry": _canon(entry)},
                          sort_keys=True, ensure_ascii=False)
    return hashlib.sha256(material.encode("utf-8")).hexdigest()


def _read() -> dict:
    try:
        doc = json.loads(path().read_text())
    except (OSError, ValueError):
        return {}
    return doc if isinstance(doc, dict) else {}


def approved(persona_name: str) -> set[str]:
    """Fingerprints this operator has approved for *persona_name*. Never raises."""
    entry = _read().get(persona_name)
    return {f for f in entry if isinstance(f, str)} if isinstance(entry, list) else set()


def approve(persona_name: str, fingerprints) -> None:
    """Record *fingerprints* as this persona's approved set, REPLACING what was there.

    Replacing rather than adding is what makes the record self-pruning: a server the
    persona no longer declares stops being approved the next time the operator approves,
    so a stale entry cannot come back to life under a re-added server name.
    """
    doc = _read()
    doc[persona_name] = sorted({f for f in fingerprints if f})
    p = path()
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")


def _safe(text: str) -> str:
    """*text* with anything unprintable escaped, for a line an operator must trust.

    Every field here comes out of a committed file and the line IS the consent: a ``\\r``
    or an ``ESC[2K`` in ``args`` repaints it, and a U+202E bidi override reverses it, so
    what the operator reads stops being what would run. ``str.isprintable`` covers that
    whole class in one call — it is false for every Other and Separator codepoint, the
    ASCII space excepted.
    """
    return "".join(c if c.isprintable() else f"\\u{ord(c):04x}" for c in text)


def describe(entry: dict) -> str:
    """Where the credential would go, as the line that asks the operator to look at it.

    Names and keys only — a ``secrets`` map holds vault KEY names, never values, so this
    is safe to print and the operator needs to see it to judge the request.

    ``""`` when the entry names neither a ``command`` nor a ``url``. An ``http``/``sse``
    server has no command, and building the line from ``command`` + ``args`` alone
    rendered it as an EMPTY string under the words *"Read the command above"* (#427).
    Falling back to ``url`` fixes the common case; returning ``""`` for whatever is left
    is the general one, and :func:`fingerprint` turns that into "not approvable", so a
    blank consent line can never be consented to again.

    ``env`` keys are shown because they choose the destination as surely as ``command``
    does: ``PATH`` decides which binary ``execvpe`` finds, ``NODE_OPTIONS`` decides what
    it loads (#426).
    """
    if not isinstance(entry, dict):
        return ""
    raw = entry.get("args")
    argv = list(raw) if isinstance(raw, (list, tuple)) else ([raw] if raw else [])
    parts = [str(entry.get("command") or "")] + [str(a) for a in argv]
    dest = " ".join(_safe(p) for p in parts if p)
    url = str(entry.get("url") or "").strip()
    if url:
        shown = f"{_safe(str(entry.get('type') or 'http'))} {_safe(url)}"
        dest = f"{dest}  [{shown}]" if dest else shown
    if not dest:
        return ""
    env = entry.get("env")
    if isinstance(env, dict) and env:
        dest += "  (env: " + ", ".join(sorted(_safe(str(k)) for k in env)) + ")"
    # The destination is at the FRONT of the line, so a committed file padding `args` with
    # a megabyte of text cannot scroll it off the operator's screen. The digest still
    # covers every byte of the entry, truncated here or not.
    if len(dest) > MAX_LINE:
        dest = dest[:MAX_LINE] + f"… (+{len(dest) - MAX_LINE} more chars)"
    return dest
