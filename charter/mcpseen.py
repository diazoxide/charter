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

**Machine-local and gitignored, deliberately.** Under ``STATE_DIR``, the same as
:mod:`charter.guardseen` and for a sharper reason: if the approval travelled in git, the
commit that declares the server could also declare that the server was approved, which is
the finding restored with an extra step.

**Withholding, not refusing.** An unapproved server is still written to the agent file —
only its credential is withheld. Deleting the server would break a working persona to
prevent a hypothetical, and charter's rule is additive: name the blocker, refuse the
dangerous half, and leave everything else working. The server starts and fails to
authenticate, which is a visible failure rather than a silent one.

**A server NAME is not a label, and this is the sentence that cost the most.** Consent here
is of an *entry* — a fingerprint over the vault, the command, the args and the two secret
maps — and the reasoning for leaving the name out of it was that a name merely says where
an entry was declared, not what would run. That was false for four review rounds. The name
was interpolated raw into the generated agent's YAML frontmatter as a bare mapping key, so
a newline in it declared a whole second server that no fingerprint covered and no prompt
mentioned (#453): the carrier entry needed no ``secrets`` at all, so `fingerprint` returned
``None``, nothing was withheld, and the run printed one green tick. It is true now only
because `persona._MCP_NAME_RE` makes it true — a name is bounded at the boundary that reads
the committed file, and the emission serialises the key through `contain.json_line` rather
than pasting it — and it stops being true the moment either of those is loosened without
the other.

The older wording is kept here as a quotation, corrected, rather than deleted: a branch in
flight that restores it is a regression and not a merge conflict to take either side of, and
a reviewer can only see that if the sentence it costs is written down next to the reason.

**Nothing here raises.** A missing or corrupt marker reads as *nothing approved*, so the
failure direction is "the credential was withheld", never "sync-agents crashed" and never
"the credential was handed over because the file was unreadable".
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

from . import config, contain

#: One file per plane, under the state dir. Never committed — see the module docstring.
FILE_NAME = "mcp-approved.json"


def path() -> Path:
    return Path(config.STATE_DIR) / FILE_NAME


def fingerprint(vault: str | None, entry: dict) -> str | None:
    """What the operator is being asked to approve, as one digest.

    ``None`` when there is nothing to consent to: an entry with no ``secrets`` and no
    ``secret_files`` is passed through untouched by `persona.mcp_render_entry`, so no
    credential is at stake and requiring approval would be a prompt about nothing.

    **Every field that decides where the value goes is in here**, which is the point:
    approving a server by name would let a later edit re-point the same name at a different
    binary. The vault (whose secrets), the command and args (who receives them), and the
    full ``secrets`` / ``secret_files`` mappings (which keys, under which environment
    variable names — both halves are chosen by the committed file) all change the digest.

    **`ensure_ascii=False` below is load-bearing and must stay.** It is not the line-safety
    question `contain.json_line` answers — this string is hashed, never written to a line —
    and both encodings are equally faithful. What changing it would do is change every
    digest, which silently un-approves every credentialed server every operator has already
    approved: the next `sync-agents` withholds working credentials and asks again. A
    consistency edit here costs a plane its consent state.
    """
    secrets = entry.get("secrets") or {}
    files = entry.get("secret_files") or {}
    if not (secrets or files) or not vault:
        return None
    material = json.dumps({
        "vault": vault,
        "command": entry.get("command"),
        "args": list(entry.get("args") or []),
        "secrets": sorted((str(k), str(v)) for k, v in secrets.items()),
        "secret_files": sorted((str(k), str(v)) for k, v in files.items()),
    }, sort_keys=True, ensure_ascii=False)
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


def describe(entry: dict) -> str:
    """The command as it would run, for the line that asks the operator to look at it.

    Names and keys only — a ``secrets`` map holds vault KEY names, never values, so this
    is safe to print and the operator needs to see it to judge the request.

    Every part goes through :func:`contain.one_line`, and the reason is the same one #453
    is about, one surface over. `command` and every `arg` come out of a committed file, and
    this string is placed in a REPORT with structure: ``  <persona>/<server> → <command>``,
    one line per server, under a warning that says how many there are. A newline in an arg
    writes a second line indistinguishable from charter's own — so a withheld server can
    print a line claiming a *different* server was withheld, or an approval line for one
    that was never approved, and the operator's "y" then covers something they did not
    read. Escaped rather than refused, because unlike a server name a command legitimately
    holds arbitrary text: this is the display bound, not the containment one.
    """
    parts = [str(entry.get("command") or "")] + [str(a) for a in (entry.get("args") or [])]
    return " ".join(contain.one_line(p) for p in parts if p)
