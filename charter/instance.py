"""Per-instance configuration, read from the control plane's ``charter.toml``.

``GROUP``, ``EXCLUDE`` and the default workspace name used to be constants baked into the
engine — hardcoded to one organisation. Moving them here is what lets a single installed
``charter`` serve any number of control planes.

Uses stdlib ``tomllib`` (Python 3.11+), which is why the floor is 3.11: it keeps the
zero-dependency promise that YAML would have ended.
"""
from __future__ import annotations

import tomllib
from pathlib import Path

from .root import MARKER

#: Layout version this engine understands.
SCHEMA = 1


class SchemaTooNew(Exception):
    """The control plane was written by a newer charter than this one."""


def load(root: Path) -> dict:
    """Parse ``<root>/charter.toml``. Returns ``{}`` when there is no such file.

    A *newer* schema raises rather than being read on a best-effort basis: silently
    misreading a persona or workspace layout is worse than refusing to run.
    """
    p = Path(root) / MARKER
    try:
        raw = p.read_bytes()
    except OSError:
        return {}
    try:
        cfg = tomllib.loads(raw.decode("utf-8"))
    except (tomllib.TOMLDecodeError, UnicodeDecodeError) as e:
        raise ValueError(f"{p} is not valid TOML: {e}") from None
    found = cfg.get("schema", SCHEMA)
    if isinstance(found, int) and found > SCHEMA:
        raise SchemaTooNew(
            f"{p} declares schema {found}, but this charter understands {SCHEMA}. "
            f"Upgrade charter: `uv tool install charter-cp --force --refresh`."
        )
    return cfg


def _forge_at(cfg: dict, index: int) -> dict:
    """The ``index``-th ``[[forge]]`` block, or ``{}`` if there is none — covers both a
    control plane with no ``[[forge]]`` blocks at all and an out-of-range index."""
    forges = cfg.get("forge") or []
    return forges[index] if 0 <= index < len(forges) else {}


def group_of(cfg: dict, fallback: str, index: int = 0) -> str:
    """The group/org tracked by forge block ``index`` (default: the first — the group
    a single-forge control plane cares about, which is what every caller but
    multi-forge ``discover`` wants: ``config.GROUP`` is deliberately "the first forge's
    group", not "all of them", since it is only used as a human-readable label and a
    back-compat fallback owner when no ``[[forge]]`` block is declared at all).

    Accepts EITHER ``group`` or ``owner`` — the same either-key acceptance
    ``registry.forges_for`` already has (GitLab calls it a group, GitHub an org/user;
    ``owner`` is the cross-forge name). ``group`` wins when a block unusually declares
    both. Before this (FINDING I3, part 2), a hand-written ``owner = "acme"`` block
    (no ``group`` key) yielded an empty ``config.GROUP`` — "repos in the `` GitLab
    group", ``charter status`` printing ``": 38 repos in inventory"``, and
    ``topology.md`` saying "38 repos in the `` GitLab group"."""
    block = _forge_at(cfg, index)
    return block.get("group") or block.get("owner") or fallback


def exclude_of(cfg: dict, index: int = 0) -> set[str]:
    """Repo names that must never enter the inventory, from forge block ``index``
    (default: the first). A control plane with several ``[[forge]]`` blocks gives each
    its own ``exclude`` — ``discover`` calls this once per block (``index`` 0, 1, …) so
    a block's excludes are applied only to *that* block's repos, never another's."""
    return set(_forge_at(cfg, index).get("exclude") or ())


def default_workspace_of(cfg: dict, fallback: str) -> str:
    return (cfg.get("workspace") or {}).get("default") or fallback


#: How far a written memory travels. Ordered least → most public.
SHARE_MODES = ("local", "commit", "push")


# --------------------------------------------------------------------------- #
# version lock — `[charter] version`, an OPT-IN pin shared like a lockfile.    #
# Absent means charter does nothing: committing the key is the act of opting a  #
# team into conformance. Exact, not a floor, so it downgrades too — pinning     #
# back to a known-good release is the case you most want to be automatic.      #
# --------------------------------------------------------------------------- #
def locked_version(cfg: dict) -> str | None:
    """The version this control plane pins, or None when it does not pin one."""
    v = (cfg.get("charter") or {}).get("version")
    return v.strip() if isinstance(v, str) and v.strip() else None


def set_locked_version(root: Path, version: str) -> bool:
    """Write ``[charter] version`` into charter.toml, preserving the rest verbatim.

    Edited as text rather than round-tripped: stdlib ``tomllib`` reads TOML but
    cannot write it, and re-emitting through any serialiser would strip every
    comment the file carries — unacceptable in a file people hand-edit.

    The edit is confined to the ``[charter]`` section's own line span. An earlier
    draft substituted the first ``version =`` line in the file, which happily
    rewrote ``[[forge]] version = "api-v4"`` and left the lock untouched — silent
    corruption of a committed config.
    """
    import re
    p = Path(root) / MARKER
    try:
        lines = p.read_text().splitlines(keepends=True)
    except OSError:
        return False

    header = re.compile(r"^[ \t]*\[charter\][ \t]*$")
    any_header = re.compile(r"^[ \t]*\[")
    key = re.compile(r"^([ \t]*version[ \t]*=[ \t]*).*$")

    start = next((i for i, ln in enumerate(lines) if header.match(ln)), None)
    if start is None:
        tail = "" if not lines or lines[-1].endswith("\n") else "\n"
        lines += [tail, "\n", "[charter]\n", f'version = "{version}"\n']
    else:
        stop = next((i for i in range(start + 1, len(lines)) if any_header.match(lines[i])),
                    len(lines))
        hit = next((i for i in range(start + 1, stop) if key.match(lines[i])), None)
        if hit is None:
            lines.insert(start + 1, f'version = "{version}"\n')
        else:
            lines[hit] = key.sub(lambda m: f'{m.group(1)}"{version}"', lines[hit])
    try:
        p.write_text("".join(lines))
    except OSError:
        return False
    return True


def clamp_share(value: str | None) -> str:
    """Clamp any candidate posture value to a known ``SHARE_MODES`` entry, defaulting to
    ``local`` when it isn't one.

    An unrecognised value falls back to ``local`` deliberately: a typo must fail *safe*,
    because the failure mode on the other side is publishing an agent's notes. This is
    the single clamp — ``share_of`` uses it for the raw TOML value, and every reactive
    committer (``hooks._commit_dispatch``, ``commands.commit_memory_reactive``,
    ``commands_workspace.cmd_workspace_autosave``) re-applies it to ``config.MEMORY_SHARE``
    defensively, so none of them trusts an if/elif fallthrough to do the clamping for it —
    a duplicated hand-rolled check would be a second thing to keep in sync with
    ``SHARE_MODES``; this is the one place that knows the set.
    """
    return value if value in SHARE_MODES else "local"


def share_of(cfg: dict) -> str:
    """The control plane's memory posture, defaulting to ``local``.

    An unrecognised value falls back to ``local`` deliberately: a typo must fail *safe*,
    because the failure mode on the other side is publishing an agent's notes.
    """
    v = (cfg.get("memory") or {}).get("share")
    return clamp_share(v)


# --------------------------------------------------------------------------- #
# plane SHAPE — which of charter's two deployments this control plane is.      #
#                                                                              #
#   fleet     the plane is its own thing; a workspace holds many CLONES, each  #
#             with its own worktrees. The original and the default.            #
#   embedded  charter installed INSIDE the one codebase it serves (which may   #
#             itself be a backend/frontend monorepo). Nothing to clone: the    #
#             tree you edit is the plane's own root, and a workspace holds     #
#             worktrees of it rather than clones.                              #
#                                                                              #
# DECLARED, never inferred, and the distinction matters: a fleet plane's root  #
# is a git repo too — its personas carry committed memory and docs/control-    #
# plane.md ships `exclude = ["this-control-plane"]` precisely because the      #
# plane's own repo lives on the forge. So `ROOT/.git` cannot tell the two      #
# apart. Every filesystem signal that might ("no clones anywhere", "empty      #
# inventory") reads a FRESH FLEET PLANE — before its first `discover` or       #
# `clone` — as embedded, which is the worst possible moment to guess wrong.    #
#                                                                              #
# What separates them is intent, and `charter init` is where intent exists:    #
# running it inside an existing repo IS the choice. So init writes it down.    #
# --------------------------------------------------------------------------- #

#: The deployments a control plane can declare. `fleet` first — it is the default.
SHAPES = ("fleet", "embedded")


def shape_of(cfg: dict) -> str:
    """This control plane's shape, defaulting to ``fleet``.

    An unrecognised value falls back to ``fleet`` for the same reason
    :func:`share_of` falls back to ``local``: a typo must fail toward the behaviour that
    changes nothing. ``fleet`` is what every existing control plane already does, so a
    misspelled shape costs a feature rather than rearranging a working status line.
    """
    v = (cfg.get("plane") or {}).get("shape")
    return v if v in SHAPES else "fleet"


# --------------------------------------------------------------------------- #
# control-plane SCHEMA — the same stamp/detect/heal pattern `workspace.py`'s   #
# STRUCTURE_VERSION already proves, one level up: a control plane (not just a  #
# single workspace) can lack layout a newer charter expects (personas/,        #
# inventory/, workspaces/, …). ``schema`` in charter.toml is the stamp (already #
# read/enforced by ``load`` above); ``drift`` below is the *detect* half;      #
# ``charter reinit`` (charter/commands.py) is the *heal* half. Idempotent +     #
# additive: existing content is never touched.                                 #
# --------------------------------------------------------------------------- #

#: Directories a control plane is expected to have. Absence is drift, not an error —
#: `charter reinit` creates them.
BASELINE_DIRS = ("personas", "inventory", "workspaces")


def drift(root: Path) -> list[str]:
    """Human-readable descriptions of what this control plane is missing.

    Empty means current. This is the *detect* half of the stamp/detect/heal pattern; the
    stamp is ``schema`` in charter.toml and the heal is ``charter reinit``.

    A baseline path can be "not a directory" two different ways, and the message must
    not conflate them: genuinely absent (``reinit`` just creates it) versus occupied by
    a file or other non-directory (FINDING C1 — ``reinit`` will *refuse*, because
    deleting or renaming a user's file to make room would break the additive rule).
    ``is_dir()`` alone can't tell these apart, so this also checks ``exists()``.
    """
    out = []
    for d in BASELINE_DIRS:
        p = Path(root) / d
        if p.is_dir():
            continue
        if p.exists():
            out.append(f"{d}/ is occupied by a file, not a directory — reinit will "
                       f"refuse to touch it")
        else:
            out.append(f"missing directory: {d}/")
    return out
