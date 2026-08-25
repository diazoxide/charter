"""When a guard was last observed actually running in this plane, and under which harness.

`doctor`'s `check_guard_wired` answers whether the guard is **declared** — a hook line in a
settings file, or an enabled plugin. That was already one rung better than "is the plugin
installed" (#177). It is still not the fact the operator needs, and the gap is not academic:
a plane root was switched between branches four times and committed to, unguarded, while
that check would have reported a tick the entire time. The declaration was real; nothing
dispatched it.

So this records the other half. Reaching `hooks.pretooluse` at all is proof the guard is
live *now*, under *this* harness — a thing no amount of reading configuration can establish.

**An observation with an age, never a boolean.** `silent 3d` and `▸steward 7m` already keep
this grammar, and for the same reason: charter cannot know that the absence of a dispatch is
a problem. A plane worked in from a plain terminal all week has no dispatch and is fine. A
plane whose guard last fired three weeks ago, under a harness you have since stopped using,
is the incident. The reader draws that line; charter supplies the date and the name.

**The harness is recorded because it is the sentence that explains the incident.** "Last
seen under claude-code 3 weeks ago" tells you where your protection went in a way "not
recently" never will.

Overwritten, not appended: this answers "when last", and a history of every turn's guard
would be unbounded for a question nobody asks. Same shape as `pieces.seen`, deliberately.
"""
from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path

from . import config

#: One file per plane, under the state dir — gitignored, machine-local, and about this
#: machine's harnesses. Committing it would describe one laptop's wiring to everybody.
FILE_NAME = "guard-seen.json"


def path() -> Path:
    return Path(config.STATE_DIR) / FILE_NAME


#: Source recorded when the running process was launched by the plugin — `$CLAUDE_PLUGIN_ROOT`
#: is set only for a command the plugin itself dispatches, which is the one fact that
#: distinguishes a declaration that IS loaded from one that is merely enabled (#261).
PLUGIN = "plugin"
#: Source recorded otherwise: a `charter hook pretooluse` line in a settings file.
SETTINGS = "settings"


def _source() -> str:
    import os
    return PLUGIN if os.environ.get("CLAUDE_PLUGIN_ROOT") else SETTINGS


def mark(harness: str | None = None, when: datetime | None = None,
         source: str | None = None) -> Path | None:
    """Record that a guard just ran. Best-effort — never raises, never blocks a turn.

    Called from the guard handler itself rather than from `sessionstart`, and that choice is
    the whole point: `check_guard_wired`'s docstring already rejects the softer version —
    *"a plane wiring only `sessionstart` is unprotected while looking configured, which is
    this issue again one level down."* Only the handler holding the guard can prove the
    guard is reachable.
    """
    from .harness import registry as _registry

    if harness is None:
        try:
            harness = _registry.current()
        except Exception:
            harness = None
    when = when or datetime.now(timezone.utc)
    # WHICH declaration dispatched this, so a sighting can never be read as evidence for a
    # declaration that did not produce it. The reporter deleted the settings block on
    # doctor's own advice, and the sighting it left behind then read as proof that the
    # plugin replacing it was live — while the running session held no declaration at all
    # (#261). A sighting belongs to the thing that made it.
    src = source or _source()
    p = path()
    try:
        config.private_mkdir(p.parent)
        p.write_text(json.dumps({"ts": when.isoformat(timespec="seconds"),
                                 "harness": harness, "source": src},
                                sort_keys=True) + "\n")
        return p
    except OSError:
        return None


def last_source() -> str | None:
    """Which declaration produced the latest sighting, or ``None`` when it predates the
    field. ``None`` means *unknown*, never *suspect*: a plane that upgrades mid-week must
    not have its own history read back to it as a fault."""
    rec = last()
    if not rec:
        return None
    val = rec.get("source")
    return str(val) if val else None


def last() -> dict | None:
    """The latest observation, or ``None``. A malformed file reads as ``None`` rather than
    raising: this feeds a preflight line, which must render whatever it finds."""
    try:
        obj = json.loads(path().read_text())
    except (OSError, ValueError, UnicodeDecodeError):
        return None
    return obj if isinstance(obj, dict) and obj.get("ts") else None


def plane_has_been_used() -> bool:
    """Whether this plane has been worked in at all.

    The gate on saying anything. A plane created five minutes ago has no dispatch and never
    could have, and warning there is the cried-wolf failure this repo has paid for twice.

    Evidence is deliberately something a HUMAN made — a persona, or a clone in a workspace.
    Session state would be circular: it is written by the very hooks whose absence is being
    detected, so a plane that never dispatched also has no sessions, and the gate would
    silence exactly the case it exists to report.
    """
    try:
        personas = Path(config.PERSONAS_DIR)
        if any(d.is_dir() and (d / "persona.md").is_file()
               for d in personas.glob("*") if not d.name.startswith("_")):
            return True
        workspaces = Path(config.WORKSPACES_DIR)
        for ws in workspaces.glob("*"):
            if not ws.is_dir() or ws.name.startswith("."):
                continue
            if any(c.is_dir() and (c / ".git").exists() for c in ws.glob("*")):
                return True
    except OSError:
        return False
    return False
