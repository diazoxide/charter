"""Per-instance configuration, read from the control plane's ``charter.toml``.

``GROUP``, ``EXCLUDE`` and the default workspace name used to be constants baked into the
engine — hardcoded to one organisation. Moving them here is what lets a single installed
``charter`` serve any number of control planes.

Uses stdlib ``tomllib`` (Python 3.11+), which is why the floor is 3.11: it keeps the
zero-dependency promise that YAML would have ended.
"""
from __future__ import annotations

import re
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
# team into conformance. Exact, not a floor, so a pin-back to a known-good      #
# release is expressible — but only an UPGRADE is applied unattended (#333);    #
# see `hooks._autosync_version_lock` for why the two directions differ.         #
# --------------------------------------------------------------------------- #
#: An exact three-part version and nothing else PEP 440 would also accept there (#333).
#:
#: **The right-hand side of ``pkg==spec`` is not a version slot.** ``commands._sync_cmd``
#: builds ``charter-cp==<pin>``, which is a *requirement specifier*: ``0.*`` is a legal
#: prefix match, and `uv pip compile` resolves ``charter-cp==0.*`` to the latest 0.x. A pin
#: that reads as exact would silently mean "whatever is published", which is the one thing
#: a lock exists to prevent — and, because it is not a version, no comparison of the two
#: versions can speak for it either. This is #332's finding one file over.
#:
#: **Anchored, and exactly three parts.** ``hooks._parse_version`` deliberately PREFIX-
#: matches (it orders a plugin version it does not control, and ``0.47.2-CANARY`` orders
#: fine as ``(0, 47, 2)``); a gate cannot, or ``0.47.2-CANARY`` passes shape and then
#: installs something else. Three parts because the *direction* check has to be decidable
#: against a three-part installed version, and ``0.47`` is not orderable against ``0.47.2``.
#: Every charter release has had this shape. If one ever ships a pre-release, this is the
#: line to widen — and widening it means making :func:`hooks._parse_version` able to ORDER
#: the new shape, not merely accept it.
_VERSION = re.compile(r"^\d+\.\d+\.\d+$")

NOT_A_VERSION = ("'{version}' is not a version. It is interpolated into the pip "
                 "requirement `charter-cp=={version}`, where a wildcard, a range, a "
                 "marker or a dist-name would also be accepted — so `[charter] version` "
                 "takes an exact X.Y.Z and nothing else")


def version_ok(version) -> bool:
    """True when *version* is a version rather than some other thing a specifier accepts."""
    return isinstance(version, str) and bool(_VERSION.match(version.strip()))


def locked_version(cfg: dict) -> str | None:
    """The version this control plane pins, or None when it does not pin one.

    Reports the pin **as written**, including one that is not a version. Refusing here
    would fold "pinned something malformed" into "pinned nothing", and a plane that opted
    into conformance would then behave exactly like one that never did — silently, which
    is the failure this whole area is about. :func:`version_ok` is what the acting sites
    ask, so the refusal happens where the action is and can name the value.
    """
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


#: The edges a frame may occupy. A slot outside this set is a typo, and a typo must not
#: reach a tmux argv — so an unknown one is dropped rather than passed through.
FRAME_SLOTS = ("top", "bottom", "left", "right")

#: Every ``[frame]`` setting, keyed by the name :func:`frame_of` returns it under
#: (underscore — reads better at the call site, e.g. ``frame["history_limit"]``) and
#: paired with ``(default, toml_key)``: the shipped default, and the name charter.toml
#: actually spells it with. Three of the six differ — ``history-limit``, ``min-cols``,
#: ``min-rows`` use a hyphen, per docs/frame.md — so the TOML spelling travels right next
#: to the default it belongs to instead of living in a second dict a reader has to keep
#: in sync by hand. Two dicts keyed apart, as an earlier draft of this had it, meant a key
#: added to one and not the other was a ``KeyError`` raised from inside :func:`frame_of` —
#: in a module every charter command imports. One structure makes that failure mode
#: impossible by construction rather than merely unlikely. Only the ``toml_key`` spelling
#: is ever honoured — the underscore form is not accepted as a second, undocumented alias.
FRAME_FIELDS = {
    #: All four edges, because the frame now OWNS the surface (ADR 0019): inside a frame
    #: `charter statusline` draws nothing, so whatever the frame does not show is not
    #: shown anywhere. Two one-line strips are not a frame — they are the status line
    #: again, in a worse shape — and that is exactly how the first release of this was
    #: reported: *"only top and bottom single lines added, no left right sidebar."*
    #: `slots.SLOTS` has had `left` (repo rows) and `right` (persona chips) since #385;
    #: they were built, tested and switched off. `layout.visible_slots` drops the two
    #: side panels first on any shortage against `min_cols`/`min_rows`, so a narrow
    #: terminal degrades to exactly the frame this default used to be.
    #:
    #: **The ORDER is the geometry, not a reading order.** `layout.panel_argvs` splits
    #: each slot off the harness pane in list order, so a slot listed after `left`/`right`
    #: gets only the width they left behind. Measured against tmux 3.7c in a 200x50
    #: window: this order gives a 200-column `bottom`, with the side panels 46 rows tall
    #: between the two strips; `["top", "left", "right", "bottom"]` instead gives 48-row
    #: side panels and a `bottom` of **154 columns**, inset between them. `bottom` is the
    #: row that carries the one alert and the command that fixes it, and `slots._bottom`
    #: drops whole fields when it runs out of width — so the 46 columns belong to it and
    #: not to two side panels that are already truncating their own 22.
    "slots": (["top", "bottom", "left", "right"], "slots"),
    #: Off by default: tmux's `set -g mouse on` takes over drag-select, so turning this on
    #: trades the operator's terminal text-selection for clickable panels. That trade
    #: belongs to a later release that actually ships clickable panels, not this one.
    "mouse": (False, "mouse"),
    "hotkey": ("F2", "hotkey"),
    "history_limit": (50000, "history-limit"),
    "min_cols": (100, "min-cols"),
    "min_rows": (20, "min-rows"),
}

#: The plain ``{key: default}`` view of :data:`FRAME_FIELDS`, for callers (and the
#: ``config.FRAME`` docstring) that only want the shipped defaults, not the TOML mapping.
FRAME_DEFAULTS = {key: default for key, (default, _toml_key) in FRAME_FIELDS.items()}

#: The shape of a tmux key name, and the ONE thing standing between ``[frame] hotkey``
#: and arbitrary code execution at launch.
#:
#: ``commands_frame.conf_text`` interpolates this value into tmux CONFIG TEXT that
#: ``source-file`` parses and runs (``bind -n {hotkey} run-shell …``). Verified against
#: tmux 3.7c: ``hotkey = "F2\\nrun-shell 'touch /tmp/PWNED'"`` makes ``source-file``
#: return **0**, silently, and the file appears **at launch, with no keypress** — the
#: newline simply ends the ``bind`` line and starts a second command. `charter.toml` is
#: committed and shared, which is precisely what makes it untrusted input (see the
#: containment rule in README.md): it arrives from someone else's machine.
#:
#: Checked HERE, at the config boundary, rather than as a fifth ad-hoc guard inside the
#: frame. Every other ``[frame]`` value is already constrained where it enters —
#: ``slots`` is set-filtered, ``mouse`` is a bool, the three numbers are int-checked —
#: and ``hotkey`` was the one free string in the section, and the one that reaches a
#: parser. The branch already carries four separate sanitisers added after four separate
#: incidents (``frame.state._UNSAFE``, ``frame.menu._ACTION_ID_RE``,
#: ``commands_frame._PANE_ID_RE``, ``contain.child``); this defect existed because a
#: fifth input arrived through a fifth door.
#:
#: Deliberately narrower than tmux's own key grammar: optional ``C-``/``M-``/``S-``
#: modifiers, then either a key NAME (``F2``, ``Up``, ``PPage``, ``BSpace``, ``Escape``,
#: ``a``, ``7``) or a single punctuation key. Whitespace, newlines, quotes, ``;``, ``#``,
#: ``$``, ``\\`` and braces are all absent from that alphabet, so nothing matching this
#: can end the ``bind`` line, start a second command, open a quote, or introduce a tmux
#: format.
#:
#: The asymmetry is what justifies erring narrow: a key this refuses that tmux would have
#: accepted costs the operator their preferred hotkey; a key this accepted that tmux
#: parses as a command costs them the machine.
#:
#: **A refusal is currently SILENT — nothing anywhere names it.** :func:`frame_of`
#: discards the value and the shipped ``F2`` takes its place, and neither
#: ``charter frame-probe`` nor ``charter doctor``'s frame row says a word: measured with
#: the newline payload above sitting in charter.toml, both render a clean green tick.
#: An earlier version of this comment claimed the probe reported it, which it never did —
#: the same class of false claim this branch removed from `frame/menu.py` and
#: `frame/tmuxctl.py`, so it is written down here rather than quietly deleted.
#:
#: Left silent deliberately, not overlooked. The gap is real but it is not this
#: constant's: NO refused ``[frame]`` value is reported anywhere — a dropped ``slots``
#: entry and a rejected ``history-limit`` are exactly as quiet — so the fix is one
#: surface for the whole section, not a special case for the one key that happens to have
#: a security story. It also would NOT catch the neighbouring hazard it looks like it
#: should: a key such as ``Fn2`` MATCHES this pattern, so :func:`frame_of` accepts it and
#: has nothing to report, and it is tmux that refuses it later, at ``source-file`` time.
#: Those are two mechanisms needing two answers. Both are filed as follow-ups.
_HOTKEY_RE = re.compile(r"^(?:[CMS]-){0,3}(?:[A-Za-z0-9]{1,20}|[!%&()*+,./:<=>?@\[\]^_|~-])$")


def frame_of(cfg: dict) -> dict:
    """The ``[frame]`` section merged over :data:`FRAME_DEFAULTS`.

    Every value is type-checked against its default and discarded if it disagrees. This
    module is imported by every command, including ``charter --version``, so a
    hand-edited charter.toml must degrade to the defaults rather than raise.

    Two keys need more than a type check, and both get it here rather than downstream:
    ``slots`` is filtered against :data:`FRAME_SLOTS`, and ``hotkey`` against
    :data:`_HOTKEY_RE` — see that constant for the injection a bare ``isinstance(value,
    str)`` let through. Both degrade to the shipped default, which is the contract every
    other key in this function already keeps: a charter.toml charter cannot make sense
    of never stops charter from running.
    """
    out = dict(FRAME_DEFAULTS)
    section = cfg.get("frame")
    if not isinstance(section, dict):
        return out
    for key, (default, toml_key) in FRAME_FIELDS.items():
        if toml_key not in section:
            continue
        value = section[toml_key]
        if key == "slots":
            if isinstance(value, list):
                kept = [s for s in value if s in FRAME_SLOTS]
                if kept:
                    out[key] = kept
            continue
        if key == "hotkey":
            if isinstance(value, str) and _HOTKEY_RE.fullmatch(value):
                out[key] = value
            continue
        # `bool` is a subclass of `int` in Python, so `isinstance(True, int)` is True even
        # though `True` was never meant to stand in for an int default — without this
        # guard, `history-limit = true` would silently pass `isinstance(True, int)` and be
        # accepted as a (nonsensical) history limit. The reverse can't happen through
        # plain `isinstance` alone (an `int` like `1` is never an instance of `bool`, so
        # `mouse = 1` is already rejected without this line) but the check is written to
        # cover both directions, since "a value must be an instance of the default's own
        # type" is the contract this function documents, not "…unless int and bool are
        # involved, in which case it depends which one is the default."
        if isinstance(value, bool) != isinstance(default, bool):
            continue
        if isinstance(value, type(default)):
            out[key] = value
    return out
