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
import os
from datetime import datetime, timezone
from pathlib import Path
from typing import NamedTuple

from . import config, util

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


#: The field a Claude Code sighting records its config folder in (#969). **Only Claude
#: Code's.** The folder is Claude Code's alone, so a Codex or opencode sighting carries no such
#: field and is never judged by one; a per-harness "config folder in use" belongs to the
#: harness-profiles work, not to this record.
CLAUDE_CONFIG_DIR = "claude_config_dir"


def _is_claude_code(harness: str | None, source: str | None) -> bool:
    """Is this a Claude Code sighting? Named so, or launched by a plugin: `$CLAUDE_PLUGIN_ROOT`
    is Claude Code's own variable, so a plugin-launched guard is one whatever the registry
    managed to name."""
    from .harness import claude_code

    return harness == claude_code.NAME or source == PLUGIN


def _folder_in_use() -> tuple[str | None, str | None]:
    """The Claude Code config folder this process would use, as ``(folder, None)`` — or
    ``(None, why)`` when there is no absolute folder to record or to compare with.

    **One exception policy for both ends of the comparison** — `mark`, which records, and
    :func:`folder_standing`, which compares — so the two can never disagree about what counts
    as a folder (#969). `doctor` used to resolve its own end, without this handling.

    **Absolute only.** A relative `$CLAUDE_CONFIG_DIR`, an empty one included, stays relative
    inside Claude Code, resolved against that process's own directory; a hook would resolve it
    against its directory and `doctor` against the one it was run from, and nobody has
    measured that those are the same. Recorded as spelled, `cfg` from the plane root compared
    equal to `cfg` from a workspace. So a relative folder is recorded as none and compared with
    nothing, and the row that cannot compare says so.

    Only `RuntimeError`: `Path.home()` raises it with no `$HOME` and no passwd entry, and the
    resolver does no I/O that could raise anything else.
    """
    from .harness import claude_code

    try:
        folder = str(claude_code.config_home())
    except RuntimeError:
        return None, ("the Claude Code config folder in use cannot be resolved (there is no "
                      "home folder)")
    if not os.path.isabs(folder):
        return None, (f"the Claude Code config folder in use, {folder!r}, is a relative path — "
                      f"a different folder in every directory it is read from")
    return folder, None


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
    rec = {"ts": when.isoformat(timespec="seconds"), "harness": harness, "source": src}
    # And under WHICH Claude Code config folder (#969), for the same reason one field over:
    # `$CLAUDE_CONFIG_DIR` moves the plugin manifest and the settings a declaration lives in,
    # so a sighting made under `~/.claude` read as proof for a session under a second
    # account's folder — where no charter hook ran at all. Recorded even when there is no
    # absolute folder to record: a present-but-empty field is "this process had none", which
    # is not the same fact as a record written before the field existed.
    if _is_claude_code(harness, src):
        rec[CLAUDE_CONFIG_DIR] = _folder_in_use()[0]
    p = path()
    try:
        config.private_mkdir(p.parent)
        config.write_for(p, json.dumps(rec, sort_keys=True) + "\n")
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


class FolderStanding(NamedTuple):
    """Whether the latest sighting counts for the Claude Code config folder in use (#969).

    ``doubt`` is ``None`` when it does — or when there is nothing for a folder to be wrong
    about: no sighting, or another harness's. Otherwise it is the clause a `doctor` row prints
    to say why the sighting cannot vouch. ``elsewhere`` is the other absolute folder the
    sighting provably ran under, and nothing else: a sighting charter cannot place names none.
    """

    doubt: str | None
    elsewhere: str | None


def folder_standing() -> FolderStanding:
    """**The one place that decides whether a sighting counts for the folder in use** (#969).

    `check_guard_wired` needs this to go green and `check_guard_seen` to stay green. The rule
    used to live in both modules, held together by a comment, and `doctor` resolved its end
    without `mark`'s exception handling; two readers of one rule is how a writer and a checker
    once disagreed about "wired" (#177). Each way a sighting fails to count is a different
    thing for the reader to do, so each says so in its own words:

    * the folder in use cannot be compared at all — relative, or unresolvable — so nothing
      can vouch for it, the next sighting included;
    * the sighting predates folder recording — something fired, under a folder charter cannot
      name. Neither a pass nor "nothing fired" (ADR 0009); the next guarded call replaces it;
    * the sighting recorded no folder — the same, from a process that had none to record;
    * it ran under another folder — named, because that is the incident.
    """
    rec = last()
    if not rec or not _is_claude_code(rec.get("harness"), rec.get("source")):
        return FolderStanding(None, None)
    in_use, why = _folder_in_use()
    if in_use is None:
        return FolderStanding(f"{why}, so no guard sighting can be compared with it", None)
    if CLAUDE_CONFIG_DIR not in rec:
        return FolderStanding("that sighting predates charter recording which Claude Code "
                              "config folder a guard ran under, so charter cannot tell which "
                              "folder it came from", None)
    recorded = rec[CLAUDE_CONFIG_DIR]
    if not isinstance(recorded, str):
        return FolderStanding("that sighting recorded no absolute Claude Code config folder, "
                              "so charter cannot tell which folder it came from", None)
    if recorded != in_use:
        return FolderStanding(f"that sighting ran under Claude Code config folder "
                              f"{util.short_path(recorded)}, not {util.short_path(in_use)}, "
                              f"the one in use", recorded)
    return FolderStanding(None, None)


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
