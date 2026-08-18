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


def default_persona_of(cfg: dict) -> str | None:
    """The persona this control plane declares as its front door — ``[persona] default``.

    No fallback argument, unlike its workspace twin above: a plane with no declared
    workspace still has to act on *some* workspace, while a plane with no declared persona
    genuinely has no front door, and charter inventing one would be it choosing an identity
    nobody asked for. Absence returns ``None`` and every caller treats that as "no persona".

    A blank value is absence, not a persona named ``"   "`` — a half-edited key should
    behave like the key that was there before it.
    """
    val = (cfg.get("persona") or {}).get("default")
    val = str(val).strip() if val is not None else ""
    return val or None


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
    :func:`_set_key` owns the how, and the why it is a textual edit."""
    return _set_key(root, "charter", "version", version)


def set_default_persona(root: Path, name: str | None) -> bool:
    """Declare (or, with ``name=None``, undeclare) ``[persona] default`` in charter.toml.

    Same textual edit as the version lock, through the same helper: two callers writing a
    key into a section of one hand-edited file is one behaviour, and hand-rolling it twice
    is how the second copy learns a lesson the first already knows (here: that ``default``
    also names a key under ``[workspace]``).
    """
    return _set_key(root, "persona", "default", name)


def _set_key(root: Path, section: str, key_name: str, value: str | None) -> bool:
    """Set — or with ``value=None`` remove — ``key_name`` inside ``[section]``.

    Edited as text rather than round-tripped: stdlib ``tomllib`` reads TOML but
    cannot write it, and re-emitting through any serialiser would strip every
    comment the file carries — unacceptable in a file people hand-edit.

    The edit is confined to the section's own line span. An earlier draft substituted the
    first ``version =`` line in the file, which happily rewrote ``[[forge]] version =
    "api-v4"`` and left the lock untouched — silent corruption of a committed config. The
    span matters more now than it did then: ``default`` is a key under ``[workspace]``
    *and* under ``[persona]``, so a file-wide substitution would swap a plane's workspace
    for a persona name.

    Removal leaves an emptied section header in place. Deleting it would mean deciding
    whether a comment sitting under the header belonged to the key or to the section, and
    an empty ``[persona]`` reads the same as no ``[persona]`` to every consumer of this
    file — including :func:`default_persona_of`.
    """
    import re
    p = Path(root) / MARKER
    try:
        lines = p.read_text().splitlines(keepends=True)
    except OSError:
        return False

    header = re.compile(rf"^[ \t]*\[{re.escape(section)}\][ \t]*$")
    any_header = re.compile(r"^[ \t]*\[")
    key = re.compile(rf"^([ \t]*{re.escape(key_name)}[ \t]*=[ \t]*).*$")

    start = next((i for i, ln in enumerate(lines) if header.match(ln)), None)
    if start is None:
        if value is None:
            return True  # nothing declared, nothing to undeclare
        tail = "" if not lines or lines[-1].endswith("\n") else "\n"
        lines += [tail, "\n", f"[{section}]\n", f'{key_name} = "{value}"\n']
    else:
        stop = next((i for i in range(start + 1, len(lines)) if any_header.match(lines[i])),
                    len(lines))
        hit = next((i for i in range(start + 1, stop) if key.match(lines[i])), None)
        if value is None:
            if hit is not None:
                del lines[hit]
        elif hit is None:
            lines.insert(start + 1, f'{key_name} = "{value}"\n')
        else:
            lines[hit] = key.sub(lambda m: f'{m.group(1)}"{value}"', lines[hit])
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


def worktrees_of(cfg: dict) -> str | None:
    """Where this plane keeps worktrees, or ``None`` for the shape's default.

    A relative value is resolved against the control-plane root (see
    ``config.WORKTREES_ROOT``), so `"../charter.worktrees"` means what it looks like.

    An escape hatch rather than a routine setting. Worktrees default to
    ``workspaces/<ws>/.worktrees/`` — deliberately OUTSIDE every clone, so that (in
    ``worktree.py``'s own words) "nx/jest/maven never recurse into them". That default is
    right whenever a workspace holds clones, which is now always.

    It is kept because the reason it was introduced can recur: put worktrees anywhere a
    build tool globs from and a repo answers a root-level glob with several copies of
    itself. Measured when this was written, in a layout that made that mistake: 214 test
    files discoverable from one root, 142 of them duplicates. ``.gitignore`` hides that
    from git and from nothing else — pytest, jest, nx, tsc and every IDE indexer glob the
    working tree directly.
    """
    v = (cfg.get("plane") or {}).get("worktrees")
    return v.strip() if isinstance(v, str) and v.strip() else None


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

    Every plane wants the same baseline. This used to be filtered per plane shape — an
    plane without clones had no use for ``inventory/`` — but every plane clones now.
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
