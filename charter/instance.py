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
from typing import NamedTuple

from . import contain
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


#: The alphabet ``charter workspace create`` mints workspace names in.
#:
#: Lives here, and not in :mod:`charter.workspace`, for one reason: ``charter.toml`` is
#: parsed during ``config``'s own bootstrap, so a rung that asked ``workspace.valid_name``
#: would be asking a module that ``config`` has not finished importing yet.
#: :func:`charter.workspace.valid_name` delegates to :func:`workspace_name_ok` below, so
#: the resolver and the creation-time check are one rule rather than two kept in step by
#: hand — the divergence :mod:`charter.contain` exists to stop.
WORKSPACE_NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")


def workspace_name_ok(name) -> bool:
    """Can *name* name a workspace this plane contains?

    Two questions, asked in :mod:`charter.contain`'s order and for its reasons.
    :func:`contain.segment_ok` is *containment* — could this string name one entry in a
    directory at all — and it is the half that still holds if the alphabet below is ever
    widened. :data:`WORKSPACE_NAME_RE` is the *alphabet*, and it is the right rule here
    because charter mints workspace names itself: a value outside it cannot name a
    workspace ``workspace create`` produced, so the resolver and lint agree by
    construction.

    ``isinstance`` first because a hand-edited ``charter.toml`` can put a TOML array or
    table where a name goes, and this module is imported by every command including
    ``charter --version``.
    """
    return (isinstance(name, str) and contain.segment_ok(name)
            and WORKSPACE_NAME_RE.fullmatch(name) is not None)


#: The alphabet a cross-repo change's slug is minted in — :data:`WORKSPACE_NAME_RE`'s
#: shape, and its own object rather than an alias, because the two names travel to
#: different places and widening one must not widen the other by accident. A workspace name
#: is a directory here; a change slug is a directory entry here **and** a branch name in
#: every member repository **and** the value of a ``Charter-Change:`` trailer in somebody
#: else's merge commit forever. The leading-character rule is what keeps a slug out of
#: argv's flag position, which is the same guard `change.branch_refusal` makes explicit one
#: field over.
CHANGE_NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")


def change_name_ok(name) -> bool:
    """Can *name* name a cross-repo change?

    One rule and not two, which is where this differs from :func:`workspace_name_ok` next
    door. That one asks :func:`contain.segment_ok` first and says why: it is the half that
    still holds if the alphabet is ever widened. Here the alphabet *is* the containment —
    :data:`CHANGE_NAME_RE` admits no separator, no leading dot, no NUL and nothing
    absolute — so a `segment_ok` call in front of it is an answer the regex has already
    given, and a line no test can go red without. The property it stood for is pinned
    directly instead, on this function, against `..`, `a/b`, `a\\b`, a NUL and a leading
    dot.
    """
    return isinstance(name, str) and CHANGE_NAME_RE.fullmatch(name) is not None


def default_workspace_of(cfg: dict, fallback: str) -> str:
    """The workspace this plane lands on when nothing else decided — ``[workspace] default``.

    **Validated, because ``charter.toml`` is committed.** The value used to be returned
    verbatim, and `config.DEFAULT_WORKSPACE` is joined onto `WORKSPACES_DIR` by
    `workspace.workspace_dir` and by `commands._first_clone_dest` — so
    ``default = "../../esc"`` in a teammate's ``charter.toml`` made `workspace vision` and
    `workspace current` read a `workspace.md` the plane does not contain, and pointed the
    first clone of `charter init` outside it (#442). The write and listing sides already
    refused; only the reading side was open, which is #328's shape one noun over.

    Degrades to *fallback* rather than raising, the contract :func:`frame_of` keeps for
    every key it cannot make sense of: a ``charter.toml`` charter disagrees with never
    stops charter from running. A blank value is absence, like its persona twin below."""
    val = (cfg.get("workspace") or {}).get("default")
    val = str(val).strip() if isinstance(val, (str, int, float)) else ""
    return val if workspace_name_ok(val) else fallback


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
    """True when *version* is a version rather than some other thing a specifier accepts.

    Asked of the string as given, with no `strip`. The `strip` that used to be here made
    this answer about a DIFFERENT string from the one the caller then used:
    `commands._sync_cmd` calls itself "the last gate before a requirement specifier" (#333)
    and interpolates the raw value into ``charter-cp==<version>``, so a padded pin was
    approved here and spent unpadded there. `locked_version` already strips what it reads
    out of `charter.toml`, so the pin path is unchanged; what changes is that the gate now
    checks the value that is spent (#577).
    """
    return isinstance(version, str) and bool(_VERSION.fullmatch(version))


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
#:
#: **`repos` joined it in #515.** `bottom` used to carry the attention row and the repo
#: table in one pane, stacked with no rule between them; the table is its own bordered
#: component now and `bottom` is the one-row strip it was before #488. The names are what
#: an operator writes in `[frame] slots`, so this is also the compatibility statement: a
#: committed `slots = ["top", "bottom", "right"]` is still valid and still launches — it
#: simply gets no table, because `slots` is the primitive and an explicit list wins
#: outright (:func:`frame_of`). Anyone who wants the table back adds `repos` to their
#: list, or deletes the line and takes the default.
#:
#: **`left` is gone as of #488**, and that filter is what makes retiring it safe: a
#: committed `charter.toml` still carrying `slots = ["top", "bottom", "left", "right"]`
#: — from a plane that upgraded, or from a teammate's checkout — has the dead name
#: dropped here and gets the others, rather than a `KeyError` in a launcher or a
#: pane split for a slot with no renderer. It drew repo rows recomposed for 22 columns;
#: `repos` draws the same rows at the width the table was designed for.
#: **`changes` is deliberately NOT here, and that is what keeps it free.** It is a
#: component charter ships, registers and can draw (`frame/builtins.py`,
#: `frame/slots.py:SLOTS`), and a plane places it with a `[[frame.component]]` table —
#: which is also the only way any component gets a toggle key, built-in or not
#: (:data:`FRAME_COMPONENT_FIELDS`: *"a component nobody asked to bind does not get a
#: key"*). Naming it here would put it in the shipped `slots` default and in `full`,
#: because those three must agree, and that is a pane on EVERY operator's frame saying
#: "no changes in <ws>" for a feature most planes never use. `repos` saying "no clones"
#: is a plane that is broken or new; a plane with no cross-repo change is the ordinary,
#: permanent state. The picker is what stays free: `F2` then the `change` row, on every
#: plane, with no config at all.
FRAME_SLOTS = ("top", "bottom", "repos", "right")

#: How much frame there is, as a PRESET over :data:`FRAME_SLOTS` — never a second
#: configuration system sitting beside `slots`.
#:
#: **And, since Phase 2's Task 5, a NAMED ARRANGEMENT OVER VISIBILITY rather than a
#: mechanism of its own** (spec §4). The operator's own words for what was wrong with it:
#: *"instead of having density - we need to have hotkeys to hide and show separately
#: components"*. A level is now three panels' worth of "visible", written down under one
#: word: `commands_frame.cmd_density` turns the list below into a HIDDEN SET over
#: :func:`frame_arrangement`, and then goes through the very same re-layout a single
#: component's toggle key does. Nothing about a level is a second path, and the mark it
#: still leaves on the frame — its ``verbosity`` — is the one axis a per-component
#: toggle genuinely cannot express, which is why the table below still carries it.
#:
#: Each level expands to two things: the ``slots`` an operator could have written by hand,
#: and a ``verbosity`` naming how much each panel on them says (`frame/slots.py` owns what
#: the two verbosities actually draw). `slots` stays the primitive, and an EXPLICIT
#: `slots` overrides a declared `density` outright — see :func:`frame_of`. That ordering
#: is what keeps this a preset: nothing here can express a frame `slots` could not, so an
#: operator who outgrows the presets writes the list and loses nothing.
#:
#: **The level names are a closed set, which is stronger than sanitising them.** A density
#: reaches tmux twice — as the slot list `layout.panel_argvs` splits panes for, and as a
#: palette row — and both times it has already been matched against these three keys by
#: :func:`frame_of` or by `instance.density_level`. Unlike ``hotkey`` (see
#: :data:`_HOTKEY_RE`), there is no value an operator can write here that is passed
#: through: it is either one of three constants charter wrote itself, or it is discarded.
#:
#: **Every ``slots`` list here is in SPLIT order, which is the geometry — not reading
#: order, and do not sort them.** `layout.panel_argvs` splits each slot off the harness
#: pane in list order, and two separate things follow from that.
#:
#: *Columns.* A slot listed after `right` gets only the width it left behind. Measured
#: against tmux 3.7c in a 200x50 window (#386): `["top", "bottom", "right"]` gives a
#: **200-column** bottom half, while `["top", "right", "bottom"]` gives **177 columns**,
#: inset beside the side panel. `repos` carries a table whose four columns want 95 of
#: them and `slots._bottom` drops whole fields when it runs out of width — so the columns
#: belong to them rather than to a side panel already truncating its own 22.
#:
#: *Rows.* Every split but `top`'s is a plain `-v`, which tmux places DIRECTLY below the
#: harness — so a slot split LATER sits ABOVE one split earlier. Measured on tmux 3.7c,
#: 120x40, splitting `top`, `bottom`, `repos`, `right` off the harness in that order:
#: `top` on row 0, harness and `right` on rows 2-30, `repos` on rows 32-37, `bottom` on
#: row 39. Which is why `bottom` is named BEFORE `repos` here and reads AFTER it on
#: screen. Transpose the two and the frame is the pre-#515 stacking order with a rule
#: drawn through it.
#:
#: **#515 re-derived these, it did not patch them.** #488 made `bottom` variable-height
#: and the three levels then differed partly by edges and partly by how many rows of one
#: pane's table were allowed. Now that the table is its own slot the levels differ by
#: EDGES again, which is what a preset over `slots` can actually express:
#:
#: * `minimal` — the two one-row strips, and nothing that costs rows by the repo.
#: * `normal` — the table as well.
#: * `full` — the sidebar as well; every edge charter draws.
#:
#: **What `minimal` gives back is a whole component, not a shorter one.** As #488/#500
#: shipped it, `minimal` was `normal` with at most `slots._TERSE_ROWS` of table — which
#: after #515 would cost the harness a border row it did not cost before, to show four
#: repo rows. A level whose whole purpose is handing the harness its rows back does not
#: negotiate over four of them: it drops the component. `slots._TERSE_ROWS` still bounds
#: the table for anyone who writes `slots` by hand and asks for `minimal` alongside it,
#: which is the one configuration that can still reach it.
FRAME_DENSITY = {
    #: The two one-row strips: where you are, and what wants attention — with `_bottom`
    #: keeping only its single highest-priority field. No repo table and no sidebar, so
    #: every row and every column the frame is not using is the harness's. For a terminal
    #: where the harness's own rows are what you came for.
    "minimal": {"slots": ["top", "bottom"], "verbosity": "terse"},
    #: Both strips saying everything they have, and the repo table between them — as tall
    #: as the window can spare it (`layout.HARNESS_MIN_ROWS` is what it may not take).
    "normal": {"slots": ["top", "bottom", "repos"], "verbosity": "normal"},
    #: Every edge charter draws, and the same order it ships in.
    "full": {"slots": ["top", "bottom", "repos", "right"], "verbosity": "normal"},
}

#: What a panel falls back to for any level charter does not know — see
#: :func:`verbosity_for`. Named rather than repeated as a bare string, because it is also
#: the answer for "no density recorded at all", which is every frame launched by a charter
#: that predates this feature.
DEFAULT_VERBOSITY = "normal"


def density_level(name) -> str | None:
    """*name* if it names a :data:`FRAME_DENSITY` level, else ``None``.

    The one place a density arriving from OUTSIDE charter's own constants — a palette row's
    argv, a value read back out of a frame's state directory, a hand-edited charter.toml —
    is admitted. ``isinstance`` first because ``value in FRAME_DENSITY`` raises
    ``TypeError`` for an unhashable value (a TOML array, a table), and this module is
    imported by every command including ``charter --version``: the same guard
    :data:`_HOTKEY_RE`'s call site needs, for the same reason.
    """
    return name if isinstance(name, str) and name in FRAME_DENSITY else None


def density_slots(level) -> list[str]:
    """The slot list *level* expands to — a fresh list, never the table's own.

    Callers hand this straight to `layout.visible_slots`, which filters it, and to
    `frame_of`'s resolved config, which a caller may go on to patch; handing out the
    module-level list would let either of them edit the preset itself for the life of the
    process.
    """
    lv = density_level(level)
    return list(FRAME_DENSITY[lv]["slots"]) if lv else []


#: What ``[frame] chrome`` may say, and the pane options each word means.
#:
#: **The surface is tmux's, not charter's.** ``window-style`` and ``window-active-style``
#: are settable PANE-scoped, and tmux fills the pane's whole rectangle from them —
#: including the cells no renderer wrote, on resize, on reattach, at zero cost on the
#: repaint path. So charter sets an option and never paints a fill: nothing new is on the
#: repaint path, a background cannot wrap a pane, focused-versus-unfocused comes from
#: tmux's own pane focus for free, and the harness pane is untouched by construction
#: rather than by care (ADR 0018). Measured on 3.7c: with these set on a panel pane,
#: ``show -p -t <harness> -v window-style`` reads back ``''``.
#:
#: **Two named slots one step apart, and never an index.** ``black``/``brightblack`` and
#: ``white``/``brightwhite`` are slots in the operator's own palette; ``colour236`` is a
#: fixed point in the xterm cube that no theme moves. The sharp form of that argument is
#: the inverse of the obvious one: an absolute colour is unsafe precisely on the terminals
#: that render it faithfully — a 16-colour client gets charter's grey downsampled to the
#: operator's own black and looks fine, while a truecolor client on a light theme gets it
#: verbatim and looks broken. tmux is already the colour ladder (measured: 24-bit ->
#: ``colour237`` -> ``ESC[40m`` -> ``ESC[7m`` as clients of four capabilities attached in
#: turn), it recomputes it per client per attach, and a second ladder inside charter
#: computed from ``$COLORTERM`` would be a second answer built on the one input measured
#: to be stale (``COLORTERM`` is not in tmux's ``update-environment``, so a pane carries
#: the terminal that started the SERVER, not the one looking at it).
#:
#: **``off`` is the default and there is no ``auto``.** Charter cannot detect the
#: operator's background — OSC 11 through tmux got no answer in a second of reading, and
#: 3.7c's own ``client-light-theme``/``client-dark-theme`` hooks did not fire against a
#: pty client that answered — and ``window-style`` honours colour ONLY (``reverse``,
#: ``dim`` and ``bold`` are accepted and silently ignored, measured), so the one
#: theme-relative style is unavailable. A default that repaints a stranger's terminal can
#: make a working frame WORSE on upgrade, which is the same asymmetry ``mouse`` is off
#: for. An ``auto`` that resolved to ``off`` would be a config value that changes nothing
#: while claiming to decide something; an ``auto`` that guessed would be a guess wearing
#: the word for a measurement.
#:
#: **The value is a WORD and never a style string, and that is a containment boundary.**
#: A tmux style value is format-expanded at draw time — measured, stored verbatim and
#: evaluated: ``bg=#{?#{==:1,1},colour196,colour46}`` reached the wire as
#: ``ESC[48;5;196m``. charter.toml is committed and arrives from someone else's machine,
#: so a free style string there would be a committed value reaching a tmux evaluator,
#: which is :data:`_HOTKEY_RE`'s class exactly. Execution was NOT achieved on 3.7c
#: (``#(...)`` is refused by the style parser outright) and that is not the same as it
#: being safe: the category is confirmed and one version was tested. The asymmetry
#: ``_HOTKEY_RE`` already argues holds — a word charter refuses that an operator wanted
#: costs them a rename; a style string charter accepted that tmux evaluates costs an
#: unknown amount on a version nobody ran.
#:
#: **Whole-frame, and that is what it is FOR — it is not the whole of the surface any
#: more.** This table gives every panel one look, which is the right default and the wrong
#: only answer: a uniform surface cannot tell one pane from another, and a frame reads as
#: an application because its regions are distinguishable, not because they are painted.
#: Reported by the operator against `chrome = "dark"` on a terminal that is already black —
#: every pane came out the colour the terminal already was, so the frame gained a
#: background and no structure. :data:`FRAME_PANE_BG` is the per-component half; this stays
#: the frame-wide default underneath it, and a component that names no ``bg`` still gets
#: exactly what this table says.
FRAME_CHROME: dict[str, tuple[tuple[str, str], ...]] = {
    #: Nothing set at all — `show -p` answers `''` for every pane and the frame is
    #: whatever the operator's own terminal already was.
    "off": (),
    "dark": (("window-style", "bg=black"),
             ("window-active-style", "bg=brightblack")),
    "light": (("window-style", "bg=white"),
              ("window-active-style", "bg=brightwhite")),
}


def chrome_level(name) -> str | None:
    """*name* if it names a :data:`FRAME_CHROME` surface, else ``None``.

    The one place a chrome value arriving from outside charter's own constants — a
    hand-edited charter.toml, committed and shared — is admitted, and the whole of what
    stands between `[frame] chrome` and a style string tmux would expand. ``isinstance``
    first for :func:`density_level`'s reason: ``value in FRAME_CHROME`` raises
    ``TypeError`` for an unhashable value (``tomllib`` can hand this a list or a table),
    and this module is imported by every command including ``charter --version``.
    """
    return name if isinstance(name, str) and name in FRAME_CHROME else None


def chrome_options(level) -> tuple[tuple[str, str], ...]:
    """The ``(option, value)`` pairs *level* means — empty for anything charter does not
    know, which is ``off``'s own answer and therefore the safe fallback.

    Callers hand these to tmux, so what comes back is charter's own constant and never
    the caller's argument: a value this function did not recognise cannot leave through
    it, which is what makes "no operator string reaches tmux" a property of the code
    rather than a promise about its call sites.
    """
    lv = chrome_level(level)
    return FRAME_CHROME[lv] if lv else ()


#: The foreground that goes with each :data:`FRAME_CHROME` surface — the other half of a
#: recipe that shipped with only one.
#:
#: **A background with no paired foreground is the defect** (#737). ``window-style
#: bg=white`` leaves every cell no renderer coloured drawing in the TERMINAL's own default
#: foreground, which the operator's theme picked to sit on the terminal's own background —
#: and that is no longer the background it is sitting on. So ``chrome = "light"`` on a dark
#: terminal is light-on-white and ``chrome = "dark"`` on a light terminal is dark-on-black.
#: Measured from the attention strip's own bytes on a dark-theme terminal::
#:
#:     ESC[0m ESC[47m 7 todos · F2 palette · ESC[2m…
#:            ^^^^^^^ the pane's `bg=white`, and no `SGR 3x` anywhere after it
#:
#: Not "a background that clashes" — panels whose text is not there. An operator toggling
#: `chrome` from the palette to see which of the two words they like finds one of them
#: blank and reasonably concludes the frame is broken.
#:
#: **This is a table of TWO words and it is deliberately not seventeen**, which is the
#: whole of what makes it a measurement rather than the guess :data:`FRAME_PANE_FG` refuses
#: to make. Charter cannot compute contrast — the sixteen ANSI names have no fixed RGB, OSC
#: 11 through tmux answers nothing, and ``$COLORTERM`` inside a pane describes the terminal
#: that started the SERVER — so it cannot say what goes on ``bg=blue``. It can say what goes
#: on ``bg=black`` and ``bg=white``, because those two are not colours charter is guessing
#: about: they are the POLES of the sixteen. A theme is free to render its blue as anything
#: it likes, and a theme that renders its ``white`` darker than its ``black`` has stopped
#: being a theme. `dark` and `light` are charter's OWN recipes rather than words an operator
#: wrote, so completing them is charter finishing a sentence it started.
#:
#: **Which is exactly why a component's own ``bg`` is NOT in here.** ``bg = "blue"`` is the
#: operator's word out of their own palette, and pairing a foreground with it would be
#: charter deciding a contrast it has already established it cannot see. What that pane has
#: instead is :data:`FRAME_PANE_FG` — ``[frame] text``, the plane saying it in the same
#: vocabulary it said the background in. :func:`surface_options` holds that line by
#: construction: this table is read only where `chrome_options` supplied the background.
#:
#: ``off`` is here with an empty clause for :data:`FRAME_PANE_FG`'s ``default`` reason — a
#: frame with no background needs no foreground to go with it, and the empty string is what
#: makes a plane that said nothing emit byte-identical options to the frame it had before
#: this key existed. Keyed on every word of :data:`FRAME_CHROME` so a third surface added
#: there cannot be the one that ships without its foreground; `TheChromeTableAndItsFore
#: groundsAreOneVocabulary` asserts the two key sets are equal.
FRAME_CHROME_FG: dict[str, str] = {"off": "", "dark": "fg=white", "light": "fg=black"}


def chrome_fg(level) -> str:
    """The ``fg=`` clause that goes with the :data:`FRAME_CHROME` surface *level* — ``""``
    for ``off``, and ``""`` for every word charter does not know.

    :func:`text_fg`'s contract for the frame-wide half, and the same function for the same
    reason: what comes back is charter's own constant and never the caller's argument, so a
    committed word charter does not recognise indexes nothing and leaves through neither
    half of the style.

    Empty is the ANSWER for ``off`` and only incidentally the failure mode, which is
    :func:`text_fg`'s own note said about the other table.
    """
    lv = chrome_level(level)
    return FRAME_CHROME_FG[lv] if lv else ""


#: The eight ANSI colour names, each of which also has a ``bright`` form. The whole
#: vocabulary a ``[[frame.component]] bg`` may say, doubled to sixteen and given a
#: seventeenth word by :data:`FRAME_PANE_BG` below.
#:
#: **Sixteen names and not 256, and that is a decision rather than an omission.**
#: ``colour0``–``colour255`` were considered for exactly this key and refused. The names
#: here are SLOTS in the operator's own palette: ``blue`` is whatever their theme decided
#: blue is, so a pane painted with it is a pane painted in their scheme. ``colour24`` is a
#: fixed point in the xterm cube that no theme moves, and — the sharp form of the argument,
#: which is the inverse of the obvious one — it is unsafe precisely on the terminals that
#: render it faithfully: a 16-colour client gets it downsampled to something sane, while a
#: truecolor client on a light theme gets the dark navy verbatim. `tests/
#: test_frame_appearance.py`'s `TheFrameNamesColoursAndNeverIndexes` asserts charter emits
#: no cube index and no 24-bit triple anywhere, on both sides of the tmux boundary; this
#: key is inside that assertion rather than an exception to it, and the test names this
#: table so a colour added here is checked by the same line.
#:
#: The cost of the refusal is bounded and the cost of admitting them is not: an operator
#: who wanted ``colour24`` and gets ``blue`` has a pane one shade off what they pictured,
#: in their own scheme; an operator who gets a committed ``colour236`` from a colleague's
#: dark theme has a black-on-black pane and nothing to read.
FRAME_PANE_COLOURS: tuple[str, ...] = ("black", "red", "green", "yellow", "blue",
                                       "magenta", "cyan", "white")

#: The word a component's ``bg`` may say, and the pane options each word means.
#:
#: **Per COMPONENT, which is what `[frame] chrome` cannot be.** The operator's report is
#: the whole argument: with ``chrome = "dark"`` on a terminal that is already black, every
#: panel came out indistinguishable from the terminal and from every other panel. A frame
#: reads as an application when its regions are told apart — a sidebar that is visibly a
#: sidebar — and one global word cannot say that, because whatever it says it says about
#: all four panes at once.
#:
#: **The value is still a WORD and never a style string**, and nothing about that
#: containment is relaxed by there being more words. A tmux style value is FORMAT-EXPANDED
#: at draw time (measured on 3.7c: ``bg=#{?#{==:1,1},colour196,colour46}`` is stored
#: verbatim and reaches the wire as ``ESC[48;5;196m``), and `charter.toml` is committed and
#: arrives from someone else's machine. So the operator's string is used as a KEY into this
#: table and never as a value out of it: :func:`pane_bg_options` returns charter's own
#: constant or nothing at all, exactly as :func:`chrome_options` does, which is what keeps
#: "no operator string reaches tmux" a property of the code rather than a promise about its
#: call sites.
#:
#: **Both options, always, and never only ``window-style``.** A component that set its own
#: background and left ``window-active-style`` alone would show ITS colour when unfocused
#: and the frame-wide `[frame] chrome` colour when focused — two unrelated colours on one
#: pane, a cell's worth of the defect #514 fixed on the borders. So a ``bg`` decides the
#: pane whole, and the focused shade comes from the same word.
#:
#: **The focused shade is the other member of the pair**, which is `FRAME_CHROME`'s own
#: "two named slots one step apart" said sixteen times instead of twice: ``blue`` focuses
#: to ``brightblue`` and ``brightblue`` focuses to ``blue``. The direction reverses on the
#: bright half because there is nothing brighter in the sixteen, and that is stated rather
#: than worked around — the property a focus indicator needs is that the live pane is a
#: shade OFF the others, not that it is a shade lighter.
#:
#: ``default`` is the seventeenth word and it is not a colour: it is ``bg=default``, the
#: terminal's own background, which is how one pane opts out of a frame-wide ``chrome``.
#: It has no partner, so a pane that asks for it has no focus shade — `chrome = "off"`'s
#: own answer, said about one pane instead of the frame.
FRAME_PANE_BG: dict[str, tuple[tuple[str, str], ...]] = {
    "default": (("window-style", "bg=default"),
                ("window-active-style", "bg=default")),
    **{name: (("window-style", f"bg={name}"),
              ("window-active-style", f"bg=bright{name}"))
       for name in FRAME_PANE_COLOURS},
    **{f"bright{name}": (("window-style", f"bg=bright{name}"),
                         ("window-active-style", f"bg={name}"))
       for name in FRAME_PANE_COLOURS},
}


def pane_bg(name) -> str | None:
    """*name* if it names a :data:`FRAME_PANE_BG` colour, else ``None``.

    The one place a component's background arriving from outside charter's own constants
    is admitted — :func:`chrome_level`'s job for the per-component key, and the same
    function for the same reason. ``isinstance`` first because ``value in FRAME_PANE_BG``
    raises ``TypeError`` for an unhashable value (``tomllib`` can hand this a list or a
    table) and this module is imported by every command, ``charter --version`` included.

    Answers the matched NAME rather than ``True``, the way :func:`chrome_level` and
    :func:`density_level` do, so a caller can store a value it has already checked instead
    of storing the raw one and re-checking it later.

    **The containment is not in what this returns — it is in what :func:`pane_bg_options`
    does with it.** This hands back the object it was given, which for a ``str`` subclass
    out of a committed file is that subclass. That is harmless precisely because the name
    is only ever a KEY: the pairs handed to tmux are indexed out of :data:`FRAME_PANE_BG`,
    so what reaches a tmux evaluator is charter's own constant whatever the key's type
    was. A version of this that reached for the matched key instead would be the same
    answer with a lookup in front of it, which is why it does not.
    """
    return name if isinstance(name, str) and name in FRAME_PANE_BG else None


def pane_bg_options(name) -> tuple[tuple[str, str], ...]:
    """The ``(option, value)`` pairs *name* means — empty for anything charter does not
    know, and empty for ``None``, which is a component that named no background at all.

    :func:`chrome_options`' contract for the per-component key: callers hand these to
    tmux, so what comes back is charter's own constant and never the caller's argument. A
    word this function did not recognise cannot leave through it.

    Empty is also the ANSWER for a component with no ``bg``, not merely the failure mode:
    `commands_frame._surface_argvs` falls back to the frame-wide `[frame] chrome` on an
    empty result, so a component that says nothing gets exactly the surface it got before
    this key existed.
    """
    n = pane_bg(name)
    return FRAME_PANE_BG[n] if n else ()


#: The word a plane's ``[frame] text`` may say, and the ``fg`` clause each one means.
#:
#: **The FOREGROUND half of :data:`FRAME_PANE_BG`, and it exists because the eight recipes
#: `frame/chrome.py` hands out were chosen against a dark terminal and nothing could say
#: so.** ``bg`` is configurable across seventeen words; the text drawn on it was not
#: configurable at all. An operator who writes ``bg = "brightblack"`` gets a surface their
#: own theme decides the shade of — dark grey on most, a light tan on the machine this key
#: was written for — and every uncoloured cell in the pane still comes out in the
#: foreground their terminal picked to sit on its OWN background, which is no longer the
#: background it is sitting on.
#:
#: **Charter cannot compute this and does not try.** The sixteen ANSI names have no fixed
#: RGB, and this plane's own `charter.toml` already records why charter cannot find out
#: what they look like: OSC 11 through tmux answers nothing, and ``$COLORTERM`` inside a
#: pane describes the terminal that started the SERVER rather than the one looking at the
#: pane. A charter that picked a "legible" foreground from the background word would be
#: claiming a measurement it does not have — so the plane is asked instead, in the same
#: vocabulary it already answers ``bg`` in.
#:
#: **Painted by tmux, not by a renderer**, which is what makes one key reach every cell.
#: `window-style` carries an ``fg`` exactly as it carries a ``bg``, and tmux resolves the
#: pane's DEFAULT foreground from it — so charter's own ``\\033[0m`` returns to the plane's
#: colour rather than the terminal's, and no renderer has to be told. Measured through a
#: nested client, one panel at ``fg=black,bg=brightblack``, on 3.7c and at
#: `tmuxctl.FLOOR` alike::
#:
#:     bg only    '\\x1b[0m\\x1b[100mPLAIN one'          <- reset returns to the TERMINAL's fg
#:     fg and bg  '\\x1b[0m\\x1b[30m\\x1b[100mPLAIN one'  <- reset returns to the PLANE's
#:     …and a green span's own close came back '\\x1b[30m' rather than '\\x1b[39m'
#:
#: **One word for the frame and not one per pane.** ``bg`` is per component because a frame
#: reads as an application when its regions are told apart; a foreground has the opposite
#: property — text that changed colour from pane to pane would stop reading as one
#: document — and a per-pane foreground would only add a second way to make one pane
#: unreadable. `FRAME_PANE_BG`'s focus partner has no counterpart here for the same reason:
#: which pane is live is said by the background pair, and a foreground that moved with focus
#: would say it twice.
#:
#: ``default`` is the seventeenth word and its clause is **empty**, which is not a hole. A
#: pane whose style names no ``fg`` already draws in the terminal's own foreground, so
#: "leave the text alone" and "say ``fg=default``" are the same pane — and the empty clause
#: is what makes a plane that says nothing emit byte-identical options to the frame it had
#: before this key existed. The word is in the table anyway so the vocabulary is exactly
#: `FRAME_PANE_BG`'s seventeen and a plane can say the default out loud.
FRAME_PANE_FG: dict[str, str] = {
    "default": "",
    **{name: f"fg={name}" for name in FRAME_PANE_COLOURS},
    **{f"bright{name}": f"fg=bright{name}" for name in FRAME_PANE_COLOURS},
}


def pane_text(name) -> str | None:
    """*name* if it names a :data:`FRAME_PANE_FG` foreground, else ``None``.

    :func:`pane_bg`'s job for the foreground key, and the same function for the same
    reasons: ``isinstance`` first because ``value in FRAME_PANE_FG`` raises ``TypeError``
    for an unhashable value, and the matched NAME rather than ``True`` so a caller can
    store a value it has already checked.

    The containment is :func:`text_fg`'s, not this one's — what reaches tmux is a value out
    of the table, indexed by a word that was only ever a key.
    """
    return name if isinstance(name, str) and name in FRAME_PANE_FG else None


def text_fg(name) -> str:
    """The ``fg=`` clause *name* means — ``""`` for ``default``, and ``""`` for every word
    charter does not know.

    :func:`pane_bg_options`' contract said about one clause instead of two pairs: what
    comes back is charter's own constant and never the caller's argument, so a word this
    function did not recognise cannot leave through it.

    Empty is the ANSWER for ``default`` and only incidentally the failure mode — see
    :data:`FRAME_PANE_FG`. Both callers append it to a style, and appending nothing is
    exactly what a plane that named no foreground asked for.
    """
    n = pane_text(name)
    return FRAME_PANE_FG[n] if n else ""


#: The most cells a component may inset its content by, per side.
#:
#: **A cap and not a clamp**: a ``pad`` above this is REFUSED by
#: :func:`component_tables` (which refuses the arrangement whole, #535's rule), not
#: quietly reduced. A value read, validated and then changed into a different one is the
#: convincing empty this section refuses everywhere else.
#:
#: **Five, and the number is derived rather than chosen.** It is the largest pad the
#: frame's own NARROWEST pane can actually afford: the sidebar is 22 columns
#: (`layout.SLOT_SIZE`), `frame/slots.py`'s `_PAD_MIN_CONTENT` is 12, and
#: ``(22 - 12) // 2`` is 5. A cap above that would admit a value that pane always drops —
#: and the sidebar is one of the two panes the operator named, so "your pad did nothing
#: and nothing said why" is the precise failure it would buy. Written as a literal here
#: because `instance` is imported by every command and must not reach `frame/layout.py` at
#: module scope; `tests/test_frame_pane_style.py`'s `TheCapIsTheNarrowestPanesOwnCeiling`
#: does the arithmetic and asserts the two agree, which is the trade `slots.INSET` already
#: makes with `statusline._HEAD_PAD`.
#:
#: One number for every pane, rather than a per-pane ceiling from that pane's own width.
#: A `pad = 5` that means five cells on the sidebar and five on a 200-column repo table is
#: a value an operator can move between components; a ceiling that changed per pane would
#: make the same number mean different things in the same file — and the wide pane loses
#: nothing real, because the ask was one or two cells.
#:
#: The cap's other job is the far end: ``pad = 10**9`` in a committed file is a
#: ``" " * n`` on the repaint path, and this is the line that stops it being one.
FRAME_PANE_PAD_MAX = 5


def pane_pad(value) -> int | None:
    """*value* if it is a padding a component may ask for, else ``None``.

    Zero to :data:`FRAME_PANE_PAD_MAX` inclusive. ``bool`` is refused explicitly for
    `component_tables`' own reason: ``isinstance(True, int)`` is ``True`` in Python, so
    ``pad = true`` would otherwise be accepted and mean one cell.

    Answers ``0`` for a declared zero, which is falsy — every caller asks ``is None``.
    """
    if isinstance(value, bool) or not isinstance(value, int):
        return None
    return value if 0 <= value <= FRAME_PANE_PAD_MAX else None


def component_style(frame: dict, name) -> dict:
    """The per-pane style *frame* gives the component called *name* — its ``bg`` and
    its ``pad``.

    **One reading, asked by two processes on two paths.** `commands_frame._split_panels`
    needs the ``bg`` to set a pane option at split time; `frame/slots.py` needs the ``pad``
    on every repaint, in the panel process, which is a different interpreter that never saw
    the split. Two membership walks over `frame["components"]` is the shape #547 cost —
    they come to disagree about which name matches — so there is one walk here and both
    callers ask it.

    *name* matches either spelling a component travels under: the committed slot name
    (``right``) or the component id (``sidebar``). Both are in the placement
    (`_placement` stores ``slot`` beside ``use``) precisely because both are live — the
    launcher splits on slot names and a panel process is started with whichever name the
    arrangement used.

    ``{"bg": None, "pad": 0}`` for a name nothing declares, which is every name on a plane
    spelled with ``slots``: per-pane style is written in ``[[frame.component]]`` and a
    plane that has not written one gets the frame it had.

    **There is no ``isinstance(placed, dict)`` here and that is deliberate.** One was
    written and the deletion sweep found it surviving: *every* placement is a dict by
    construction, because :func:`component_tables` is the only thing that fills this list
    and the only thing it appends is :func:`_placement`'s return. That is a contract rather
    than an observation — `TheArrangementIsAlwaysPlacementDicts` pins it against every
    shape a committed file can take — so a guard here would be defending against an input
    charter's own resolver cannot produce, which is the second-weaker-answer shape #568
    deleted the last of. The ``or ()`` beside it stays, because a *missing* ``components``
    key is real: a frame relaunched by a charter that predates it has one.
    """
    for placed in frame.get("components") or ():
        if name in (placed.get("slot"), placed.get("use")):
            # `or 0` rather than `.get("pad", 0)`, and the difference is a real case: a
            # placement built by a charter that predates this key has no `pad` at all
            # (`.get` covers that), and one built with `pad=None` has the key with nothing
            # in it (`.get` would hand `None` to `" " * n`). Both mean "no pad", so the
            # falsy read is the one that says so — this is not the `if not pane_pad(v)`
            # mistake one function up, where `0` and "refused" are different answers.
            return {"bg": placed.get("bg"), "pad": placed.get("pad") or 0}
    return {"bg": None, "pad": 0}
def chrome_option_names() -> tuple[str, ...]:
    """Every tmux option name :data:`FRAME_CHROME` can set, in the order it names them.

    **Derived from the table, never spelled beside it**, and that is the whole reason it
    is a function. `off` is the absence of these options rather than a style of its own,
    so turning a running frame's surface off means UNSETTING them — and a second list
    saying which is a list that a third row in the table would quietly leave behind, with
    the symptom being one option surviving a keypress that said "off". The table is the
    only place the option names are written.

    `dict.fromkeys` rather than a `set`: what a caller does with this is build tmux argvs,
    and an order that changes between runs is a diff nobody can read in a test failure.
    """
    return tuple(dict.fromkeys(name for pairs in FRAME_CHROME.values()
                               for name, _value in pairs))


#: The two tmux options a pane draws its OWN edges from, above `tmuxctl.PANE_BORDER_FLOOR`.
#:
#: Written here rather than derived from a table the way :func:`chrome_option_names` is,
#: because there is no per-colour table for them: every colour puts its surface into the
#: same two names, and the value is assembled by :func:`pane_border_options` out of a style
#: constant its caller owns. One tuple, read by the setter and by the removal
#: (`commands_frame._resurface_argvs`), so an option added to the pair cannot be set by one
#: and left behind by the other — which is the property `chrome_option_names` exists for,
#: kept the same way with one less indirection.
PANE_BORDER_OPTIONS: tuple[str, ...] = ("pane-border-style", "pane-active-border-style")


#: What a plane's ``[frame] rules`` may say about the seam between two panes.
#:
#: **tmux always paints a glyph on a border cell, so this is a CONTRAST question and never
#: a character one.** `pane-border-lines` has no ``none``: its five values are ``single``,
#: ``double``, ``heavy``, ``simple`` and ``number``, and every one of them draws something.
#: A frame whose panels are painted therefore cannot stop tmux drawing the rule; it can
#: only decide what the glyph is drawn IN.
#:
#: ``visible`` is what charter has always drawn — `commands_frame._CHROME_STYLE` over the
#: surface, an ``fg=default`` and a ``dim``. On a plane whose panels carry no colour that is
#: a quiet line in the operator's own foreground and it is the whole of the frame's
#: structure. On a plane whose panels ARE painted it is a dim default-foreground glyph
#: sitting in the middle of a surface, which is the seam this operator has now reported four
#: times (#627, #631, #657, and again against the frame that fixed #657).
#:
#: ``hidden`` gives the glyph the surface's own colour, so tmux draws it and nobody sees it
#: and two adjacent panels read as one surface. Measured through a nested client on 3.7c,
#: three panes at ``bg = "brightblack"``, read off the wire::
#:
#:     visible  '\\x1b[2m\\x1b[100m───…'   <- ESC[2m: a dim default fg ON the surface
#:     hidden   '\\x1b[90m\\x1b[100m───…'  <- ESC[90m on ESC[100m: the glyph is the surface
#:
#: **A WORD, never a style string**, which is :data:`FRAME_CHROME`'s rule and not a fresh
#: one: a tmux style value is FORMAT-EXPANDED at draw time (measured on this very option —
#: ``set -w pane-border-style 'bg=#{?#{==:1,1},colour196,colour46}'`` is rc 0 and reaches
#: the wire as ``ESC[48;5;196m``), and `charter.toml` arrives from someone else's machine.
#: So the operator's word selects a BRANCH here and the colour it resolves to comes out of
#: charter's own tables, exactly as ``bg`` and ``chrome`` do.
FRAME_RULES: tuple[str, ...] = ("hidden", "visible")


def rules_level(name) -> str | None:
    """*name* if it names a :data:`FRAME_RULES` treatment, else ``None``.

    :func:`chrome_level`'s job for the rules key — the one place a value arriving from a
    hand-edited, committed `charter.toml` is admitted, with ``isinstance`` first for
    :func:`density_level`'s reason.
    """
    return name if isinstance(name, str) and name in FRAME_RULES else None


class Look(NamedTuple):
    """The three frame-wide answers about how charter's own chrome READS, resolved once.

    **One record rather than three parameters threaded through five functions**, and the
    reason is #547's rather than tidiness: `_chrome_argvs`, `_harness_rule_argvs`,
    `_surface_argvs` and `_resurface_argvs` all need the same answers, two of them run on
    the launch path and two on the live `charter frame-chrome` path, and three loose
    keywords is three chances for one of those four to be handed a different frame's
    appearance. Passed whole, they cannot come apart.

    It carries what the PLANE said and nothing derived from a pane: which pane wears which
    background is `component_style`'s question and stays there.

    ``dim`` is a ``bool`` and every other field is a word out of :data:`FRAME_RULES` or
    :data:`FRAME_PANE_FG`. Nothing here is trusted as a tmux VALUE or as an SGR — `rules`
    selects a branch, the four colour words are keys into tables, and `dim` decides whether
    a constant is appended.

    **The three accents have DEFAULTS on the field and the other three do not**, which is
    not tidiness: `Look` is built positionally at a hundred call sites and in a dozen tests
    written before the accents existed, and every one of them means "the shipped green,
    yellow and red". A field with no default would have made those a syntax error rather
    than a decision, and the decision they were making has not changed.
    """

    #: ``hidden`` or ``visible`` — :data:`FRAME_RULES`.
    rules: str
    #: The ``[frame] text`` word — a key into :data:`FRAME_PANE_FG`.
    text: str
    #: Whether charter may reduce the contrast of its own chrome — ``[frame] dim``.
    dim: bool
    #: ``[frame] ok`` — what "this is fine" is drawn in. A key into :data:`FRAME_PANE_FG`'s
    #: seventeen words, resolved to an SGR by `statusline.accent`.
    ok: str = "green"
    #: ``[frame] warn`` — what "look at this" is drawn in. **Yellow on a tan surface is the
    #: pair this key exists for.**
    warn: str = "yellow"
    #: ``[frame] bad`` — what "this is broken" is drawn in.
    bad: str = "red"


def look_of(frame: dict) -> Look:
    """*frame*'s :class:`Look` — the appearance keys, read once for a whole launch.

    ``.get`` with the shipped default rather than a subscript, for `component_style`'s own
    reason: a frame relaunched by a charter that predates these keys has a `frame.state`
    record without them, and a caller must get the shipped answer there rather than a
    ``KeyError`` raised on the repaint path.

    **No re-validation.** :func:`frame_of` is the boundary and it already refuses a word
    charter does not know; asking again here would be the second, weaker answer #568's
    sweep deletes. What guards a value that never went through `frame_of` — a dict a test
    built by hand — is the same thing that guards every other one: :func:`text_fg` indexes
    a table and :func:`rule_style` compares against a constant, so an unknown word reaches
    no tmux value through either.
    """
    return Look(rules=frame.get("rules", FRAME_DEFAULTS["rules"]),
                text=frame.get("text", FRAME_DEFAULTS["text"]),
                dim=frame.get("dim", FRAME_DEFAULTS["dim"]),
                **{r: frame.get(r, FRAME_DEFAULTS[r]) for r in FRAME_ACCENTS})


def rule_style(surface: str | None, style: str, look: Look) -> str:
    """The ONE style value every rule in a frame is drawn with — the whole assembly, in one
    place.

    **Three decisions and no more**, taken in the order that makes each one answerable:

    * ``rules = "hidden"`` over a surface that names a COLOUR wins outright, because it is
      the only answer where the foreground is not a choice at all: the glyph is asked for in
      the same colour as the cell behind it, which is the most a frame can do about a glyph
      tmux insists on drawing. **Asked for, and not promised invisible** — the two clauses
      name one word out of the operator's own palette, so what a terminal actually paints
      for ``fg=brightblack`` and ``bg=brightblack`` is theirs rather than charter's, and a
      theme that renders its bright foreground and bright background as different shades
      leaves a faint glyph rather than none. Charter cannot see which theme that is
      (:data:`FRAME_PANE_FG`), so the honest claim is the one about the values it emits.
      No ``dim`` beside them: an attribute over a glyph asked to disappear is an
      instruction to make it visibly something. The live style the operator hand-applied
      and confirmed on their own frame is exactly this — ``fg=brightblack,bg=brightblack``
      — and this is that value made permanent.
    * otherwise the foreground is the plane's own ``text`` word where it named one
      (:func:`text_fg`) and the caller's *style* where it did not, so a frame that gave its
      panes a foreground draws its rules in it rather than in a colour nothing else on the
      screen is wearing.

      **It does NOT reach for :data:`FRAME_CHROME_FG`, and that is a scope this function
      does not have rather than an omission it made.** #737 paired a foreground with
      charter's own two surfaces for the pane's INTERIOR, so a plane on
      ``rules = "visible"`` with ``chrome = "light"`` gets ``fg=default,dim,bg=white`` on
      its rules while its panes get ``fg=black,bg=white`` — the rule drawn in the
      terminal's own foreground over charter's white. Three things bound how much that
      costs: the shipped ``rules`` is ``hidden``, which returns before this line and gives
      the glyph the surface's own colour; ``text`` overrides it in one line; and a rule is
      one dim glyph rather than a pane of text.

      Fixing it properly is not a line here. The surface this is handed is a bare ``bg=``
      clause and ``bg=black`` is *both* ``chrome = "dark"``'s and a component's own
      ``bg = "black"`` — so pairing off the surface would give a component's word the
      foreground :data:`FRAME_CHROME_FG` deliberately refuses it, and pairing off the
      ``chrome`` word means threading it through :func:`rule_options`,
      :func:`pane_border_options` and `commands_frame._chrome_argvs`, whose surfaces come
      from :func:`border_bg` and may be a component's rather than charter's. That is a
      decision about the per-component half of the pairing, not a bug in this assembly.
    * ``dim`` is appended unless the plane turned it off, and never under ``hidden`` — see
      above.

    **``bg=default`` is a surface that ``hidden`` cannot be honoured over, and it is a real
    input rather than a defensive one**: ``default`` is the seventeenth `FRAME_PANE_BG`
    word and a component may name it to opt one pane out of a frame-wide `chrome`. It means
    "the terminal's own background", and there is no ``fg=`` spelling for that — ``fg=
    default`` is the terminal's own FOREGROUND, which is the one colour guaranteed to be
    visible against it. So a ``hidden`` honoured there would emit ``fg=default,bg=default``
    and draw the rule at FULL strength: brighter than the ``dim`` it replaced, which is the
    opposite of what the word asked for. It falls through to *visible* instead, which is
    the frame such a pane already had.

    *style* is the caller's base rule foreground — `commands_frame._CHROME_FG`,
    ``fg=default`` — passed in rather than imported so the one place that decides what a
    charter rule looks like stays the one place. It is the FOREGROUND alone and not the
    whole of `_CHROME_STYLE`, because the ``dim`` half is now a decision rather than a
    constant: a caller that handed the composed style in would be asking this function to
    take an attribute back out of a string, which is the parsing-charter's-own-constants
    shape one function over deliberately avoids.

    **``look.rules`` is compared against a constant and never validated here.** It selects
    a branch; it is not a value that reaches tmux. :func:`frame_of` is the boundary that
    refuses a word charter does not know, and a word that got past nothing still cannot be
    ``"hidden"``, so it draws the frame charter shipped.
    """
    hidden = bool(surface) and look.rules == "hidden"
    if hidden:
        # `removeprefix` and not a split: every value this can be handed comes out of
        # `FRAME_PANE_BG` or `FRAME_CHROME`, whose `window-style` entries are all exactly
        # `bg=<name>`. `TheSurfaceIsAlwaysABareBackgroundClause` pins that over every word
        # in both tables, so the prefix is a property of charter's own constants rather
        # than a shape hoped for at a call site.
        colour = surface.removeprefix("bg=")
        if colour != "default":
            return f"fg={colour},{surface}"
        hidden = False
    return ((text_fg(look.text) or style)
            + ("" if hidden or not look.dim else ",dim")
            + (f",{surface}" if surface else ""))


def rule_options(surface: str | None, style: str, look: Look) -> tuple[tuple[str, str], ...]:
    """The ``(option, value)`` pairs that draw ONE pane's own edges over *surface* — empty
    where there is no surface to draw them over.

    **The one place a rule's value is assembled, and that is why it is its own function.**
    Two panes' edges are set from two different questions — a panel's from its own colour
    (:func:`pane_border_options`), the harness's from the colour its neighbours agree on
    (:func:`agreed_border_bg`) — and #514's whole defect is one rule coming out in two
    colours. Assembling the value twice is how two answers to one question get written; one
    assembler means a panel's edges and the harness's are byte-identical whenever the
    surfaces they were resolved from are, without either caller knowing about the other.

    **Both options carry the identical value**, and that is #514's rule surviving the move
    to pane scope rather than being dropped by it. tmux picks between them per border CELL
    — `pane-active-border-style` for a cell that touches the active pane, the other for the
    rest — so a pane whose two differed would have edges that changed colour where they
    passed the active pane's corner, which is the defect #514 closed. Measured on 3.7c
    across four focus states (harness active, sidebar active, repos active, harness again):
    with the pair identical every rule cell held its colour, and the frame did not move when
    focus did.

    *style* and *look* are :func:`rule_style`'s, which is where the value itself is now
    assembled: this function is the PAIR — which options carry it, and that they carry the
    identical thing — and that split is what lets `_chrome_argvs` draw the same rule
    window-wide below `tmuxctl.PANE_BORDER_FLOOR` without a second assembler.

    Empty rather than a bare *style* for a pane with no surface: the window option is
    already exactly that, so a pane that adds nothing must SET nothing. What a caller does
    with the emptiness differs — a panel being drawn for the first time issues no command,
    a pane that may be carrying yesterday's value issues the unset
    (`commands_frame._harness_rule_argvs`) — and that is the caller's question, not this
    one's.

    **The emptiness is still asked of the SURFACE and not of the assembled style**, which
    matters now that a style can differ from the caller's without a surface: a plane that
    named a ``text`` colour changes what `_chrome_argvs` puts on the window, and must still
    put nothing on a pane that has no surface to draw a rule over. Those are two questions
    and this one answers the second.
    """
    if not surface:
        return ()
    value = rule_style(surface, style, look)
    return tuple((n, value) for n in PANE_BORDER_OPTIONS)


def surface_options(name, chrome, look: Look) -> tuple[tuple[str, str], ...]:
    """The ``(option, value)`` pairs ONE pane's whole rectangle is painted from — the
    background it wears and the foreground its text comes out in.

    **`pane_bg_options(name) or chrome_options(chrome)` with the plane's own ``text``
    folded in, and it is one expression rather than two call sites merging.** The ``or`` is
    `commands_frame._surface_argvs`' own and unchanged — a component that named no ``bg``,
    and one whose word charter does not know, both take the frame-wide surface — and the
    foreground is appended to whatever that answered, so a pane is one word's background or
    the other's and never a blend, while every pane in the frame carries the same
    foreground.

    **A ``text`` with no surface at all is a real arrangement and gets the options anyway.**
    `chrome = "off"`, no ``bg`` anywhere, ``text = "black"``: there is no background to
    paint, and the plane has still said what colour its frame's text is. So the pairs are
    built from :func:`chrome_option_names` — the two style options, derived from the table
    the way everything else in this module derives them — carrying the foreground alone.

    Empty only when the plane said neither, which is the frame charter shipped: no
    background, no foreground, and `_surface_argvs` issues no command at all.

    What reaches tmux is charter's own constant on both halves. The background comes out of
    :data:`FRAME_PANE_BG`/:data:`FRAME_CHROME` as it always did, and the foreground out of
    :data:`FRAME_PANE_FG` through :func:`text_fg`, so a committed word that charter does not
    know indexes nothing and leaves through neither.

    **A pane that took the frame-wide surface takes the foreground that goes with it**
    (#737, :data:`FRAME_CHROME_FG`). ``chrome = "light"`` is charter's own ``bg=white``, and
    shipping it without ``fg=black`` left every uncoloured cell drawing in the terminal's
    default foreground — which on a dark theme is white, on white. The pairing is read off
    the same ``or`` the background came from, so it reaches a pane exactly when
    `chrome_options` did: a component that named its own ``bg`` supplied its own background
    and gets no foreground it did not ask for, which is the line :data:`FRAME_CHROME_FG`
    draws between charter's two recipes and the operator's seventeen words.

    **``text`` wins, and it wins by being the only ``fg`` in the value rather than by
    coming last in it.** A plane that named a foreground has said what its frame's text is
    and charter's default for the surface is not a second opinion — so the two are resolved
    to ONE clause here, not appended in an order that leaves tmux's last-wins parse to
    decide. `TheStyleCarriesExactlyOneForeground` pins that over every pairing of the two
    tables, because a value carrying two ``fg=`` clauses is a frame whose colour depends on
    a tmux parsing rule nobody wrote down.
    """
    own = pane_bg_options(name)
    pairs = own or chrome_options(chrome)
    fg = text_fg(look.text) or ("" if own else chrome_fg(chrome))
    if not fg:
        return pairs
    if not pairs:
        return tuple((n, fg) for n in chrome_option_names())
    return tuple((n, f"{fg},{value}") for n, value in pairs)


def pane_border_options(name, chrome, style: str, look: Look) -> tuple[tuple[str, str], ...]:
    """The ``(option, value)`` pairs that draw ONE panel pane's own edges in its own
    surface — empty for a pane that has no surface to draw them in.

    **A pane's edges, not the frame's rules, and that distinction is #631's.** `border_bg`
    answers one colour for the whole window, which is all tmux below
    `tmuxctl.PANE_BORDER_FLOOR` can be told. Above it every pane carries its own, so a
    panel's edges can be the colour that panel is rather than the colour the frame settled
    on — which is what an arrangement whose components name DIFFERENT backgrounds needs,
    and the half of #631 that stands.

    **This function is only ever asked about a PANEL**, and that is unchanged:
    `commands_frame._surface_argvs`, its only caller, is only ever handed a panel pane. The
    harness's edges are a different question with a different answer
    (:func:`agreed_border_bg`) and its INTERIOR is not a question at all — `window-style`
    is not in :data:`PANE_BORDER_OPTIONS`, so nothing assembled here can reach one.

    *style*, *look* and the pair-building are :func:`rule_options`'. The surface is
    ``pane_bg_options(name) or chrome_options(chrome)``'s ``window-style`` value, which is
    the SAME expression the pane's interior is painted from: a pane and its own edges cannot
    come out two colours, because they are read off one answer.

    **The BACKGROUND expression and not :func:`surface_options`**, and the difference is
    load-bearing rather than an oversight. A plane that named a ``text`` colour has an
    ``fg`` in the value its pane's rectangle is painted with, and a rule resolved off that
    string would carry two foregrounds — the plane's, and then whatever
    :func:`rule_style` appends. The rule's own foreground is `rule_style`'s decision, taken
    from the same ``look``, and what this reads off the pane is only which colour it is
    sitting in. `TheRuleReadsTheBackgroundAndNotTheWholeSurface` pins it.
    """
    return rule_options(
        dict(pane_bg_options(name) or chrome_options(chrome)).get("window-style"),
        style, look)


def _component_surfaces(frame: dict, chrome) -> set:
    """Every surface the panels of *frame* will actually wear, as a set — one walk over the
    arrangement, read by both of the questions below.

    Two walks is the shape #547 cost: they come to disagree about which placement resolves
    to what, and here the disagreement would be invisible, because both answers are
    plausible colours. :func:`border_bg` and :func:`agreed_border_bg` differ in what they do
    with a set of more than one, and in nothing else — so the set itself is built once.

    ``pane_bg_options(bg) or chrome_options(chrome)`` is `commands_frame._surface_argvs`'
    own expression, asked of every placement at once instead of one at a time. That is what
    makes "they agree" mean the thing an operator can see rather than "they wrote the same
    word": a component that names no ``bg`` takes the frame-wide surface, and agrees with
    one that named the colour that surface already is.

    Empty for a plane that wrote no ``[[frame.component]]`` at all — which is every plane
    spelled with ``slots``. That is not "they disagree": every panel on such a plane takes
    the frame-wide surface, so they agree on it, and both callers below say so.
    """
    return {dict(pane_bg_options(placed.get("bg")) or chrome_options(chrome))
            .get("window-style")
            for placed in frame.get("components") or ()}



def border_bg(frame: dict, chrome) -> str | None:
    """The background clause every rule in *frame* is drawn over, or ``None`` for a frame
    whose rules keep the terminal's own — `commands_frame._chrome_argvs`' *surface*.

    **BELOW `tmuxctl.PANE_BORDER_FLOOR` only, since #631.** One colour for every rule in
    the window is all a tmux without pane-scoped border options can be told, and its cost is
    the frame-wide fallback: where the panels disagree this answers a colour that may match
    neither side of some rule. Above the floor :func:`pane_border_options` gives each panel
    its own edges and :func:`agreed_border_bg` gives the harness the one the panels agree
    on, so no rule cell is ever a colour no pane beside it wears. This stays because the
    floor cannot have that, and a frame with no surface on its rules at all is the seam this
    whole key was written to close — so below the floor the choice is between two imperfect
    renderings and this is the one where the panels at least read as one surface.

    **A pane border is the one cell that belongs to no pane, and that is the whole
    problem.** :func:`chrome_options` and :func:`pane_bg_options` paint a pane's INTERIOR
    (``window-style``); the rule between two panes is drawn from `pane-border-style`, a
    different option, and charter has pinned it to `commands_frame._CHROME_STYLE` — an
    ``fg`` and an attribute, no ``bg`` — since #514. So a frame whose panes are all painted
    grey came out as grey rectangles separated by a one-cell strip of the terminal's own
    black. Measured on 3.7c, the same three panes with `bg = "brightblack"` on each, read
    off an attached client's wire::

        before  '\\x1b[100m … \\x1b[2m\\x1b[49m│\\x1b[0m\\x1b[100m'   <- ESC[49m: the seam
        after   '\\x1b[100m … \\x1b[2m│\\x1b[0m\\x1b[100m'            <- the surface runs through

    **Which colour, when the two sides of a rule may be different colours.** There is no
    "panel side" to match: tmux draws ONE border, not two half-borders. (On 3.7c it will
    take a pane-scoped `pane-border-style` from the pane above or left of the rule and
    ignore the other side's entirely; at `tmuxctl.FLOOR` that option has no pane scope at
    all and ``set -p`` silently writes the WINDOW's — measured both ways. A per-side design
    is therefore unavailable, one-sided where it exists, and silently window-wide where it
    does not.)

    So the rule takes the surface the frame's own components AGREE on, and the frame-wide
    `[frame] chrome` surface when they do not agree. A gutter in the frame's own colour
    between two differently-coloured panels is what an application looks like; a gutter in
    NO colour between two identically-coloured panels is a seam, and the seam is the whole
    report.

    **Resolved by the same expression every pane resolves itself with**, in
    :func:`_component_surfaces` — one walk, shared with :func:`agreed_border_bg` so the two
    cannot come to disagree about which placement resolves to what.

    **The universe is the ARRANGEMENT, not the panes on screen**, and that is deliberate
    twice over. It is what makes the launch path and the live path (`cmd_chrome`) answer
    identically by construction — both read `config.FRAME` and neither needs a pane map,
    which is the disagreement #610 cost — and it is what stops the frame's rules changing
    colour when a toggle key hides a panel or a narrow terminal drops one. A plane that
    wrote no ``[[frame.component]]`` at all has no per-pane colours to agree about, so the
    set is empty and the frame-wide word is the answer, which is what it was before this
    key existed.

    ``None`` where `[frame] chrome` is ``off`` and nothing named a colour, and it is the
    same ``None`` a frame got before any of this: `_chrome_argvs` then emits
    `_CHROME_STYLE` unchanged, so `off` is the REMOVAL of the surface from the rule rather
    than a third style of it. The two border options are charter's own at every level
    (#514 pins them whether or not there is a surface), which is why this needs no unset
    where :func:`chrome_option_names` needs one — the option is always set, and what
    changes is whether it carries a colour.

    **What comes back is charter's own constant, never the operator's word.** It is a
    value out of :data:`FRAME_PANE_BG` or :data:`FRAME_CHROME` — those tables' own
    ``window-style`` entries, indexed by a word that was only ever a key — and the two
    reasons that rule exists were both re-measured on 3.7c against *this* option rather
    than assumed from `window-style`'s own:

    * ``set -w pane-border-style 'bg=#{?#{==:1,1},colour196,colour46}'`` is **rc 0** and
      reads back verbatim. A border style is FORMAT-EXPANDED at draw time exactly as a
      window style is, so a free string in a committed `charter.toml` would be a
      stranger's value reaching a tmux evaluator here too.
    * ``bg=chartreuse`` is **rc 0** as well — tmux knows the X11 colour names, and
      ``chartreuse`` is a fixed RGB point no theme moves, which is the hazard
      :data:`FRAME_PANE_COLOURS` refuses ``colour24`` for. tmux refusing a value
      (``bg=notacolour`` is rc 1, ``invalid style:``) is therefore not the boundary
      charter needs, and is not asked to be.
    """
    surfaces = _component_surfaces(frame, chrome)
    if len(surfaces) == 1:
        return surfaces.pop()
    return dict(chrome_options(chrome)).get("window-style")


def agreed_border_bg(frame: dict, chrome) -> str | None:
    """:func:`border_bg` MINUS its fallback: the surface every panel of *frame* wears, or
    ``None`` where they wear more than one — `commands_frame._harness_rule_argvs`' *surface*.

    **This is the colour the HARNESS'S OWN EDGES take, and the fallback is exactly why it
    cannot be `border_bg`.** Above `tmuxctl.PANE_BORDER_FLOOR` tmux resolves each border
    cell against one pane, and the pane that owns every cell around the harness is the
    harness itself. Measured, not inferred: with a surface on the harness's two options and
    on nothing else, its top, right and bottom rules all changed colour and no other cell in
    the frame did. (tmux's own mechanism is `screen_redraw_check_cell`, which walks the
    window's panes in order and takes the first whose border box holds the cell — and the
    harness is the first pane charter's window has.) #631 left those two unset, which put
    the terminal's own
    background on them — and a horizontal rule that runs under the identity bar past the
    harness's corner and on over the sidebar then came out in TWO colours, dark for the
    cells the harness owns and the surface for the cells the sidebar owns. Rendered through
    a nested client at 100x24, charter's real four-panel shape, every panel
    ``bg = "brightblack"``::

        #631         row 1: cols 0-77 ESC[49m   cols 78-99 ESC[100m   <- one rule, two colours
        with this    row 1: cols 0-99 ESC[100m                        <- one rule, one colour

    That is #514's own defect — a rule that changes colour where it passes a corner — in a
    new place, and closing it is what this answers.

    **The invariant both answers keep is that every rule cell wears the surface of a pane it
    TOUCHES**, and the fallback is the one thing that would break it here. A panel's edges
    take that panel's colour, so they always match the panel on their own side. The
    harness's edges have the harness on one side — never painted, so never a colour to
    match — and a panel on the other, so they must take that panel's. When the panels agree
    there is one such colour and this returns it. When they do not, no single value can
    match all three of the harness's neighbours, and :func:`border_bg`'s frame-wide fallback
    would be a THIRD colour: a cell matching neither of the two panes beside it, which is
    the definition of the seam #627 reported. ``None`` instead, which leaves those cells the
    harness's own background — still one of the two panes they touch.

    So: agreed, or bare. Never a compromise colour, and the universe is the whole
    ARRANGEMENT rather than the panes on screen for :func:`border_bg`'s reasons, unchanged —
    the launch path and `cmd_chrome` read `config.FRAME` and neither needs a pane map, and a
    toggle key hiding a panel does not repaint the frame's rules.

    **The computation names no pane**, and that is deliberate: what it reads is a property of
    the PANELS — which colours they wear, and whether that is one — and `commands_frame` is
    the only module that knows which pane the answer is for. What protects the harness's
    INTERIOR is one option name over: `window-style` is not in :data:`PANE_BORDER_OPTIONS`,
    so no value assembled from this can reach one.
    """
    # The one refusal, and the whole difference from `border_bg`: more than one surface
    # means there is no colour the harness's three edges could all match, and a compromise
    # would be a third colour on every one of them.
    if len(_component_surfaces(frame, chrome)) > 1:
        return None
    return border_bg(frame, chrome)


def verbosity_for(level) -> str:
    """How much each panel says at *level*. :data:`DEFAULT_VERBOSITY` for anything else.

    Anything else is a real case, not a defensive one: a frame's live density override is
    read off disk (`frame.state.density`), and a frame started by an older charter has no
    file there at all. Both answer ``None``, and a panel must draw the ordinary amount
    rather than nothing.
    """
    lv = density_level(level)
    return FRAME_DENSITY[lv]["verbosity"] if lv else DEFAULT_VERBOSITY

#: The three roles `[frame]` lets a plane recolour, in the order they are documented.
#:
#: **A tuple and not three spellings**, for `chrome.served_params`' reason one module over:
#: :class:`Look`, :func:`look_of`, :func:`frame_of`'s validation and `statusline.accent` all
#: need the same list, and a fourth role added here reaches every one of them on the same
#: commit or none of them. `TheAccentRolesAreOneList` asserts this against
#: :data:`FRAME_FIELDS` and against `frame/chrome.py`'s served vocabulary, so a role that is
#: configurable and not served — or served and not configurable — is red.
#:
#: Declared ABOVE :data:`FRAME_FIELDS` rather than beside it because a ``#:`` block attaches
#: to the assignment that FOLLOWS it: appended to the end of that table's own comment, this
#: one silently took the documentation off `FRAME_FIELDS` and wore it.
FRAME_ACCENTS: tuple[str, ...] = ("ok", "warn", "bad")

#: Every ``[frame]`` setting, keyed by the name :func:`frame_of` returns it under
#: (underscore — reads better at the call site, e.g. ``frame["history_limit"]``) and
#: paired with ``(default, toml_key)``: the shipped default, and the name charter.toml
#: actually spells it with. Three of the seven differ — ``history-limit``, ``min-cols``,
#: ``min-rows`` use a hyphen, per docs/frame.md — so the TOML spelling travels right next
#: to the default it belongs to instead of living in a second dict a reader has to keep
#: in sync by hand. Two dicts keyed apart, as an earlier draft of this had it, meant a key
#: added to one and not the other was a ``KeyError`` raised from inside :func:`frame_of` —
#: in a module every charter command imports. One structure makes that failure mode
#: impossible by construction rather than merely unlikely. Only the ``toml_key`` spelling
#: is ever honoured — the underscore form is not accepted as a second, undocumented alias.
FRAME_FIELDS = {
    #: Every edge charter draws, because the frame now OWNS the surface (ADR 0019):
    #: inside a frame `charter statusline` draws nothing, so whatever the frame does not
    #: show is not shown anywhere. Two one-line strips were not a frame — they were the
    #: status line again, in a worse shape — and that is exactly how the first release of
    #: this was reported: *"only top and bottom single lines added, no left right
    #: sidebar."*
    #:
    #: **`repos` is on this list as of #515.** `bottom` used to hold the attention row
    #: and the repo table in one pane with nothing between them; the table is its own
    #: bordered component now and `bottom` is the one-row strip it was before #488. The
    #: frame reads identity · session · table · attention, top to bottom.
    #:
    #: **`left` is not on this list any more, and that is #488 rather than a retreat.**
    #: The sidebar drew repo rows recomposed for 22 columns, which its own docstring
    #: conceded was less than the status line it replaced. `repos` draws the FULL table,
    #: so the sidebar's only remaining job was a lesser copy of its neighbour's, at the
    #: cost of 22 columns of harness. `layout.visible_slots` still drops `right` first on
    #: any shortage against `min_cols`/`min_rows`, so a narrow terminal degrades to the
    #: strips as before — and drops `repos` too below the width its table needs.
    #:
    #: **The ORDER is the SPLIT order, which is the geometry, not a reading order.**
    #: `layout.panel_argvs` splits each slot off the harness pane in list order. A slot
    #: listed after `right` gets only the width it left behind — measured against tmux
    #: 3.7c in a 200x50 window, this order gives a 200-column bottom half while
    #: `["top", "right", "bottom"]` gives **177 columns**, inset beside the side panel.
    #: And a slot split later sits ABOVE one split earlier, because every split but
    #: `top`'s is a plain `-v` off the harness: measured on 3.7c at 120x40, this order
    #: puts `top` on row 0, `repos` on rows 32-37 and `bottom` on row 39. So `bottom` is
    #: named before `repos` and reads after it.
    "slots": (["top", "bottom", "repos", "right"], "slots"),
    #: A PRESET over the line above, not a rival to it (see :data:`FRAME_DENSITY`). The
    #: shipped value is the level that expands to EXACTLY the shipped `slots` above —
    #: same edges, same ORDER, and saying as much as a panel has — and that is not a
    #: coincidence to be re-checked by eye: `tests/test_frame_density.py`'s
    #: `ShippedDefaultsAgree` asserts all three, so a change to either key that is not
    #: matched by the other is red rather than a plane where `charter.toml`'s two ways of
    #: asking for the same frame disagree about what the default is. This value moved
    #: `normal` -> `full` when #386 raised the `slots` default above, and that test is
    #: what made the move happen at merge time instead of being noticed later.
    "density": ("full", "density"),
    #: Off by default, and the trade it makes is UNAVOIDABLE rather than conditional.
    #:
    #: An earlier version of this comment said the trade "belongs to a later release that
    #: actually ships clickable panels, not this one". Both halves of that are now wrong.
    #: This IS that release — Phase 2 ships a clickable surface — and measurement says
    #: there was never a later release in which the trade could be avoided.
    #: `docs/superpowers/specs/2026-08-26-tmux-input-findings.md` §5, on tmux 3.1c, 3.2
    #: and 3.7c: tmux enables mouse reporting on the OUTER terminal from the active pane's
    #: mode alone, so the instant any mouse-requesting pane is active the terminal is
    #: reporting and its own drag-select is gone for the whole window. **There is no state
    #: in which charter's panels are clickable and native selection survives.** Turning
    #: tmux's own `mouse` off does not dodge the trade; it only makes it conditional on
    #: which pane happens to be focused.
    #:
    #: Which is the second thing this flag really controls, and the reason it stays a
    #: flag: with it OFF, whether a panel is clickable is decided by the ACTIVE pane's
    #: request — that is the harness (Claude Code, or whatever the operator ran), a
    #: program charter does not own. So "clickable panels" is not a property charter can
    #: promise for its panes at all with this off (§4i); with it on, reporting is
    #: unconditional from attach and the cost is unconditional too. Off is the default
    #: because an operator who has not asked for it keeps their selection.
    #:
    #: A component declaring `click`/`scroll` therefore declares what it HANDLES, never
    #: that the event fires — `frame/component.py`'s `EVENT_KINDS` says that to the
    #: provider author, `docs/frame.md` says it to the operator, and both point back here.
    #: The palette is the exception and does not need this flag: while it is open it is
    #: the active surface, so its own request is the one that reaches the terminal.
    #:
    #: **The SECOND cost this flag used to carry is gone, and what replaced it is a
    #: change of tmux's semantics an operator who already set it should know about
    #: (#634).** tmux's default `MouseDown1Pane` binding is `select-pane -t = \; send-keys
    #: -M`: with its own mouse on, tmux SELECTS the pane under the pointer before
    #: forwarding the click, so every click on a panel took the keyboard off the harness.
    #: Charter now rebinds that key — see `commands_frame.conf_text` for the binding, its
    #: measurement and why it is conditional rather than blanket. Measured on 3.7c and at
    #: the 3.2 floor, identically — **with the terminal already reporting in every row**,
    #: which off the flag is the harness's doing and not charter's, and is the precondition
    #: the paragraph above is entirely about::
    #:
    #:     mouse off  click a panel   -> panel receives it,  active pane: the harness
    #:     mouse ON   click a panel   -> panel receives it,  active pane: the harness
    #:     either     wheel a panel   -> panel receives it,  active pane: the harness
    #:     mouse ON   click the HARNESS or a pane the operator split -> that pane, as tmux
    #:
    #: So the first row is not a promise that a click arrives with the flag off. It is the
    #: answer to a different question — *if* one arrives, does it move the keyboard — and
    #: the two must not be read as one, because with the flag off and the harness asking
    #: for nothing the terminal is never asked to report and no click happens at all.
    #:
    #: **What ON still costs is the drag-select above, and that has not changed.** The
    #: paragraph that opens this comment is the whole of the trade and it is still
    #: unavoidable; this only removes a second cost that was never tmux's price for
    #: reporting, only its default for one key.
    #:
    #: The rebind is charter changing a documented tmux behaviour inside its own private
    #: server, which is why it is written down in three places rather than one: an
    #: operator who set `mouse = true` before this release and put the keyboard back with
    #: `overlay.HATCH_KEY` after every click will find they no longer have to, and a click
    #: on a pane charter did not create still selects it exactly as tmux says.
    "mouse": (False, "mouse"),
    #: The pane surface, off by default — see :data:`FRAME_CHROME` for what the three
    #: words mean, why there is no fourth, and why the value is a word rather than a
    #: style. Off for `mouse`'s own reason, said about a different cost: a default that
    #: can make an existing working frame WORSE on upgrade must be opt-in, and a
    #: light-terminal operator upgrading into a default `dark` gets a frame worse than
    #: the one they had, on a surface they never touched, having done nothing. A
    #: dark-terminal operator upgrading into a default `off` gets a frame BETTER than the
    #: one they had — the heading, the inset, the selected row and the status rule are
    #: theme-safe and ship on — and one line short of the one they wanted. Those are not
    #: symmetric, and the asymmetry is the argument.
    #:
    #: One word, so `docs/frame.md`'s hyphen rule (`history-limit`, not `history_limit`)
    #: does not arise.
    "chrome": ("off", "chrome"),
    #: How the seam between two panes is drawn — see :data:`FRAME_RULES` for why this is a
    #: contrast question rather than a character one, and what the two words mean.
    #:
    #: **``hidden`` is the shipped default, and unlike `chrome` it cannot make an existing
    #: frame worse.** The two are not symmetric: `chrome`'s default is `off` because
    #: `dark` would repaint a stranger's terminal, and a stranger's terminal is exactly
    #: what charter cannot see. ``hidden`` repaints nothing. It has an effect only where
    #: there is already a surface to hide the rule INTO, which is a plane that has written
    #: a `chrome` or a `bg` by hand — and such a plane has already said it wants panels
    #: rather than boxes, which is the whole of what this word means.
    #:
    #: **A plane with no surface gets byte-identical options to the frame it had**, and
    #: that is stated rather than left implicit: there is no colour for the glyph to take,
    #: so `rule_style` falls through to the ``visible`` assembly and emits `_CHROME_STYLE`
    #: exactly as it always did. `hidden` there is not a silent failure — it is the same
    #: rendering the word would produce if it could be honoured, because a rule over the
    #: terminal's own background IS the terminal's own background either way.
    #:
    #: The asymmetry that makes `hidden` right as the default is the report: an operator
    #: who wanted the seam and gets none can say ``rules = "visible"`` in one line, and the
    #: operator who did not want it has now reported it four times.
    "rules": ("hidden", "rules"),
    #: The foreground every pane charter paints draws its text in — see
    #: :data:`FRAME_PANE_FG` for the seventeen words, why charter cannot compute this one,
    #: and why it is frame-wide where ``bg`` is per component.
    #:
    #: ``default`` is the shipped value and it emits nothing at all, so a plane that says
    #: nothing gets the frame it had. One word, so `docs/frame.md`'s hyphen rule does not
    #: arise.
    "text": ("default", "text"),
    #: Whether charter may reduce the contrast of its own chrome — the ``dim`` in the rule
    #: style, and the ``\\033[2m`` `frame/chrome.py` calls ``muted``.
    #:
    #: **The one thing in the frame that is wrong by construction on a surface it was not
    #: chosen for**, which is why it is a key of its own rather than one more colour.
    #: `frame/chrome.py`'s `_role_values` argues that bold, dim and reverse need no gate
    #: because they are "statements relative to whatever the operator's terminal already
    #: is". That is true of bold and reverse and it is the wrong half of true for dim: dim
    #: is relative, and it is relative in the direction of LESS contrast. On the terminal
    #: charter's recipes were chosen against — light text on a dark ground — a dim grey is
    #: readable. On a pane the plane painted light it moves the text toward the background
    #: it is already too close to, and the operator reported it unreadable.
    #:
    #: **``true`` is the shipped value and the default does not move**, for a reason that
    #: is about information rather than caution. `dim` is not decoration in this frame: it
    #: is the ONLY thing separating muted text from ordinary text — a tree glyph from a
    #: repo name, a count from a heading — and a frame with it off everywhere is a frame
    #: whose hierarchy is flat. Turning it off is right for a plane whose surface makes it
    #: unreadable and wrong as an answer for every plane, so the plane says it.
    "dim": (True, "dim"),
    #: The three ACCENT roles — what charter's own "this is fine", "look at this" and
    #: "this is broken" are drawn in, and what `frame/chrome.py` serves a provider under
    #: the same three names.
    #:
    #: **The gap `[frame] text` named and could not close.** `text` fixes a pane whose
    #: background is INVERTED relative to the terminal's; on a terminal that is already
    #: light it has nothing to do, and what is hard to read there is these three. They are
    #: slots in the operator's own palette, which is why they shipped un-configurable — and
    #: that argument holds exactly while the palette's green, yellow and red are readable
    #: on the ground they are drawn on. On the machine `text` was written for they are not:
    #: **yellow on tan is the combination no palette was designed for.**
    #:
    #: **Charter still cannot compute this and still does not try** (:data:`FRAME_PANE_FG`
    #: carries the measurements). It cannot know which of the sixteen the operator's theme
    #: renders legibly on their own background, so it asks — in the same seventeen-word
    #: vocabulary the plane already answers ``bg`` and ``text`` in, checked at this same
    #: boundary and resolved to an SGR by `statusline.accent`.
    #:
    #: ``default`` is a real answer here rather than a hole, and it is the one worth
    #: reaching for first: it is the pane's OWN foreground (SGR 39), which is `[frame]
    #: text` where the plane set one and the terminal's own where it did not. So
    #: ``warn = "default"`` is "stop colouring the warnings", and the frame still says
    #: everything it said — `NoStatusIsCarriedByColourAlone` asserts every status here
    #: survives having its escapes stripped, so the glyph carries it.
    #:
    #: The shipped values are the three colours charter has always drawn, so a plane that
    #: says nothing gets a byte-identical frame — `TheShippedAccentsAreTheOnesCharterAlways
    #: Drew` asserts that against `statusline`'s own constants rather than against a copy.
    "ok": ("green", "ok"),
    "warn": ("yellow", "warn"),
    "bad": ("red", "bad"),
    "hotkey": ("F2", "hotkey"),
    "history_limit": (50000, "history-limit"),
    "min_cols": (100, "min-cols"),
    "min_rows": (20, "min-rows"),
}

#: The plain ``{key: default}`` view of :data:`FRAME_FIELDS`, for callers (and the
#: ``config.FRAME`` docstring) that only want the shipped defaults, not the TOML mapping.
FRAME_DEFAULTS = {key: default for key, (default, _toml_key) in FRAME_FIELDS.items()}

#: The :class:`Look` of a plane that has said nothing — derived from :data:`FRAME_DEFAULTS`
#: rather than spelled, so it cannot drift from the table above.
#:
#: It is the DEFAULT for every parameter that takes a `Look` in `commands_frame`, and that
#: choice is deliberate: a call site that forgets to pass one draws the frame charter
#: ships, never the frame charter used to ship. The failure mode of the other default —
#: `Look("visible", "default", True)`, today's rendering — is a new call site quietly
#: keeping the old behaviour, which is the one bug this whole change is about.
SHIPPED_LOOK: Look = look_of({})

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
#: incidents (``frame.state._UNSAFE``,
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
#: the same class of false claim this branch removed from the frame's own modules and
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


def toggle_key(value):
    """*value* if it is a tmux key charter will let reach a ``bind`` line, else ``None``.

    **:data:`_HOTKEY_RE` asked, never a second pattern**, and that is the whole of this
    function. A component's toggle key (``[[frame.component]]``'s ``key``) is interpolated
    into tmux CONFIG TEXT by `commands_frame.conf_text` — the same ``bind -n {key}
    run-shell '…'`` line, in the same file, sourced by the same ``source-file`` — as
    ``[frame] hotkey``. The injection that constant's docstring measures is therefore the
    identical injection, reached through a second committed key rather than a second
    mechanism — re-measured on tmux 3.7c against a config in exactly the shape
    `conf_text` writes, with ``key = "F9\\nrun-shell -b 'touch /tmp/PWNED'"``::

        $ tmux -L t source-file evil.tmux ; echo $?
        0                                   # silently
        $ ls -l /tmp/PWNED
        -rw-r--r--  1 …  /tmp/PWNED         # at source-file time, no keypress

    The newline simply ends the ``bind`` line and starts a second tmux command.

    A second regex spelled here would be a second answer to "what may reach a ``bind``
    line", and the two would drift — which is exactly what `frame/component.py`'s own
    ``match``/``fullmatch`` slip cost inside one module, and what `contain.py`'s docstring
    argues against in general. So the two keys are one alphabet by construction.

    ``isinstance`` first, for :func:`density_level`'s reason: ``tomllib`` can hand this a
    list or a table, and ``_HOTKEY_RE.fullmatch`` raises ``TypeError`` for either — in a
    module every command imports, ``charter --version`` included.

    ``None`` rather than ``False``, so a caller can write the matched value back the way
    :func:`density_level` does: what reaches tmux is then this function's answer, not the
    object a committed file supplied.
    """
    return value if isinstance(value, str) and _HOTKEY_RE.fullmatch(value) else None


def frame_of(cfg: dict) -> dict:
    """The ``[frame]`` section merged over :data:`FRAME_DEFAULTS`.

    Every value is type-checked against its default and discarded if it disagrees. This
    module is imported by every command, including ``charter --version``, so a
    hand-edited charter.toml must degrade to the defaults rather than raise.

    Six keys need more than a type check, and all six get it here rather than
    downstream: ``slots`` is filtered against :data:`FRAME_SLOTS`, ``density`` against
    :data:`FRAME_DENSITY`, ``chrome`` against :data:`FRAME_CHROME`, ``rules`` against
    :data:`FRAME_RULES`, ``text`` against :data:`FRAME_PANE_FG`, and ``hotkey``
    against :data:`_HOTKEY_RE` — see those constants for the injection a bare
    ``isinstance(value, str)`` lets through in each case. All six degrade to
    the shipped default, which is the contract every other key in this function already
    keeps: a charter.toml charter cannot make sense of never stops charter from running.

    **Degrading rather than refusing is the right half of #535 for a `[frame]` key**, and
    the boundary is worth stating because the neighbouring rule goes the other way. An
    ARRANGEMENT is refused whole (:func:`component_tables`) because a component silently
    missing reads as a plane with no clones — a frame short a pane looks like a machine
    short a repo, and nobody suspects a typo. A `rules` or `text` word charter cannot read
    costs a rule drawn the shipped way in a frame that is otherwise entirely intact, which
    is what `chrome`, `density` and `hotkey` have always done. #687 is the standing warning
    against over-refusing, and it bites here in the direction of ACCEPTANCE: every word in
    both new vocabularies is usable, so the tests assert the whole of each rather than a
    sample.

    **`density` is expanded here, and only when it was actually declared.** A declared
    level replaces ``slots`` with the list it expands to, so everything downstream —
    `commands_frame._drawable_slots`, `frame_ready`, `doctor.check_frame`,
    `layout.panel_argvs` — goes on reading one key and never learns that presets exist.
    An explicit ``slots`` wins outright: it is the primitive, and an operator who wrote a
    list meant that list.

    Expanding ONLY on a declared level, rather than unconditionally from the shipped
    default, is what keeps the `slots` default above load-bearing instead of dead. The two
    say the same thing today (`FRAME_FIELDS`'s own comment, and the test it names), so an
    unconditional expansion would behave identically right now — and would then silently
    swallow the next change to the shipped `slots` list, which is exactly the drift that
    comment exists to prevent.

    An explicit ``slots`` that filters down to NOTHING (``slots = ["sideway"]``) is not
    treated as having been written: the operator asked for a frame charter cannot build,
    so a declared density is still the better answer than the shipped default, and with no
    density declared the shipped default holds exactly as it did before.
    """
    out = dict(FRAME_DEFAULTS)
    # Set before the early return, not only on the way out: a key present on one path and
    # absent on another is two shapes for one answer, and `layout._placed_here` would then
    # be reading a `KeyError` on exactly the planes that declare no `[frame]` section at
    # all. Not in :data:`FRAME_DEFAULTS` because it is not a SETTING — nothing in the
    # `[frame]` table is spelled `components`, and the generic type check below would read
    # `[[frame.component]]`'s raw tables straight into it without resolving one.
    out["components"] = []
    # Set here for `components`' own reason, and carried for :func:`harness_of`'s: a
    # `[[frame.component]]` arrangement charter refuses degrades to the frame `slots`
    # describes, which is byte-identical to the frame a plane that wrote no arrangement at
    # all gets — so the operator who committed one sees charter behaving exactly as though
    # their tables were not there. Every OTHER key in this function degrades to a shipped
    # default and the frame still draws with the rest of the file in force; this one takes
    # the whole arrangement out of play (#535), which is the difference that earns it a
    # reader. `None` is "nothing to say" and covers both the ordinary plane (no tables
    # written) and the good one (tables written and honoured); a string is the one key that
    # did it, ready for a sentence. Read by `doctor.check_control_plane_config`, where
    # `config.worktrees_root_for`'s and `[harness] default`'s own silently-ignored keys are
    # named for the same reason, and by `commands_frame.frame_ready` (`--probe`,
    # `charter frame-probe`), which is the surface an operator asks "what will this frame
    # not be able to do".
    out["components_refused"] = None
    section = cfg.get("frame")
    if not isinstance(section, dict):
        return out
    took_slots = False
    for key, (default, toml_key) in FRAME_FIELDS.items():
        if toml_key not in section:
            continue
        value = section[toml_key]
        if key == "slots":
            if isinstance(value, list):
                kept = [s for s in value if s in FRAME_SLOTS]
                if kept:
                    out[key] = kept
                    took_slots = True
            continue
        if key == "density":
            if density_level(value):
                out[key] = value
            continue
        if key == "chrome":
            # The closed enum, checked at the boundary the way `density` is — and for a
            # sharper reason: a tmux style value is FORMAT-EXPANDED at draw time, so a
            # bare `isinstance(value, str)` here would carry a committed string from
            # someone else's machine into a tmux evaluator. See :data:`FRAME_CHROME`.
            if chrome_level(value):
                out[key] = value
            continue
        if key == "rules":
            # The closed enum again, and for `chrome`'s sharper reason rather than by
            # symmetry: `rules` decides a `pane-border-style`, which tmux FORMAT-EXPANDS at
            # draw time exactly as it does a `window-style` (measured — see
            # :data:`FRAME_RULES`). A bare `isinstance(value, str)` here would be the one
            # door in this section that a committed string could walk a tmux format
            # through.
            if rules_level(value):
                out[key] = value
            continue
        if key == "text":
            # Checked against :data:`FRAME_PANE_FG` here for the same reason `bg` is
            # checked against `FRAME_PANE_BG` at the component boundary: what an accepted
            # word buys is an index into charter's own table, and what a refused one costs
            # is the shipped `default`.
            if pane_text(value):
                out[key] = value
            continue
        if key in FRAME_ACCENTS:
            # `text`'s check for the three accent roles, and the same table: what an
            # accepted word buys is an index into charter's own constants and what a
            # refused one costs is the colour charter has always drawn. These reach an SGR
            # rather than a tmux style, so no format expansion is at stake — but the
            # containment is the same one for the same reason, and a second shape for
            # "is this one of the seventeen" is how the two come to disagree.
            if pane_text(value):
                out[key] = value
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
    if "density" in section and not took_slots:
        out["slots"] = density_slots(out["density"])
    # *After* the loop, so ``hotkey`` is already resolved: a component's own ``key`` is
    # refused when it would steal the frame's palette key, and the value it is compared
    # against has to be the one `conf_text` will actually bind — the operator's, if they
    # wrote a usable one, and the shipped ``F2`` if they did not.
    placed, refused = component_arrangement(section, hotkey=out["hotkey"])
    out["components_refused"] = refused
    if placed is not None:
        out["slots"] = [p["slot"] for p in placed if p["visible"]]
    # The arrangement itself, and not only the names it comes down to. `slots` carries
    # what to split; a placement also carries WHERE and HOW BIG, which for a component
    # charter did not write is a rectangle nothing else on this machine knows —
    # `layout._placed_here` is what reads it back. Empty on every plane spelled with
    # `slots`, which is charter's own: there is no per-plane rectangle to read, and
    # `layout`'s shipped tables are the whole answer exactly as they were.
    out["components"] = placed or []
    return out


#: The TOML key the arrangement is spelled with: ``[[frame.component]]``, an ARRAY of
#: tables, so file order is split order and the geometry is written down the way it is
#: read. ``slots`` says the same thing in four words and can say nothing else; this can
#: name a component charter did not write.
FRAME_COMPONENT_KEY = "component"

#: Every key a ``[[frame.component]]`` table may carry, and the whole of the form.
#:
#: * ``use`` — a component id, which is `frame/builtins.py`'s vocabulary and NOT
#:   ``slots``' one: charter's own four are `identity`, `attention`, `repos`, `sidebar`,
#:   and anything else is a component an installed distribution supplies. The two names
#:   for one built-in panel exist because `slots` is committed on every plane that has a
#:   charter.toml and `charter panel <slot>` is a tmux argv (`builtins.SLOT_OF` is the one
#:   table between them); `use` does not accept the old spelling, so a file says which of
#:   the two it is written in.
#: * ``edge`` — which side of the harness it attaches to. Optional on one of charter's
#:   own and REQUIRED on anything else, with ``size``: see :func:`component_tables`.
#: * ``size`` — how many cells it is given, for a component whose size is `Fixed`.
#: * ``visible`` — whether it is drawn at all. The one key `slots` could only express by
#:   deleting a name, which loses the position with it.
#: * ``key`` — the tmux key that TOGGLES ``visible`` on the running frame, live. Optional,
#:   and there is no default: a `bind -n` is server-wide and intercepts the key before the
#:   harness pane ever sees it, so charter shipping four of them would silently take four
#:   keys away from Claude Code (or codex, or whatever the operator ran) on every plane.
#:   A key is bound because an operator asked for it by name. Held to
#:   :func:`toggle_key` — which is :data:`_HOTKEY_RE` — because it reaches the same
#:   ``bind`` line ``[frame] hotkey`` does.
#: * ``bg`` — this pane's background, one word out of :data:`FRAME_PANE_BG`. The key
#:   `[frame] chrome` could not be: a frame-wide word says one thing about all four panes
#:   at once, and telling the panes APART is what makes a frame read as an application.
#: * ``pad`` — how many cells this pane insets its content by, each side. Charter draws
#:   this one (tmux paints backgrounds and insets nothing), so it comes out of the pane's
#:   content budget — see `frame/slots.py:pad_of`.
FRAME_COMPONENT_FIELDS = ("use", "edge", "size", "visible", "key", "bg", "pad")


def _placement(cid: str, *, edge: str, size, visible: bool = True,
               key: str | None = None, bg: str | None = None, pad: int = 0) -> dict:
    """One resolved placement: the component, where it sits, how big, and whether drawn.

    **The one place a placement is spelled**, which is why the rectangle is passed in
    rather than read here: it comes from the component's own declaration for a built-in
    and from the committed table for a provider, and a second dict literal on the second
    path is the two-implementations shape that let the test harness keep a worse copy of
    what runs charter (#547).

    ``size`` is the size POLICY (`component.Fixed`/`Content`/`Fill`), not a number.
    Turning a policy into cells is `layout`'s job and it does it in one place
    (`layout._policy_cells`).

    ``slot`` is the name this placement TRAVELS under — into `frame_of`'s slot list, into
    `layout`'s per-name geometry, and into `charter panel <name>`'s argv. A built-in
    travels under its committed spelling, because that argv and that config key exist on
    every plane that has a charter.toml; a component charter did not write has no
    committed spelling and travels under its id, which is the whole of "a component id is
    the frame's currency".

    ``key`` travels beside ``visible`` because it is the one thing that CHANGES it: the
    frame binds it to `commands_frame.cmd_toggle`, which flips this component's visibility
    and hands the result to the same re-layout a density change goes through. ``None`` is
    the ordinary answer — a plane spelled with `slots` has no place to write one, and a
    component nobody asked to bind does not get a key.

    ``bg`` and ``pad`` travel here rather than being looked up again downstream, and the
    reason is that they are read on two different machines' worth of process: the ``bg``
    by the launcher, at split time, to set a pane option; the ``pad`` by a panel process on
    every repaint. A placement already IS the one resolved answer about this component's
    rectangle, and how the rectangle is painted and how far its content sits inside it are
    the same kind of fact as which edge it is on. ``None``/``0`` is a component that named
    neither, which is every component on a plane spelled with `slots`.
    """
    from .frame import builtins as _builtins
    return {"use": cid, "slot": _builtins.SLOT_OF.get(cid, cid), "edge": edge,
            "size": size, "visible": visible, "key": key, "bg": bg, "pad": pad}


def _built_in_placement(reg, cid: str, *, size=None, **style) -> dict:
    """:func:`_placement` for one of charter's own, in the rectangle it declares.

    **It forwards *style* and restates none of it, which is a defect fixed rather than a
    style choice.** This used to spell out ``visible=True, key=None, bg=None, pad=0`` —
    the same four defaults :func:`_placement` already declares — and the hand-check found
    what that costs: mutating `_placement`'s ``pad`` default from ``0`` to ``1`` changed
    nothing anywhere, because every call arriving through here passed a ``0`` of its own.
    Two defaults for one thing, and the second hides the first (`#547`'s shape, and the
    masking shape the sweep is written to catch).

    So the rectangle — ``edge`` and ``size``, which is all this function is FOR — is what
    it supplies, and everything else is the caller's or :func:`_placement`'s.

    ``size`` is the one half of that rectangle a plane may now REPLACE, and ``None`` means
    "whatever the component declares" — every caller that is not resolving a committed
    ``size`` (the `slots` shorthand, a density level) passes nothing and gets exactly the
    placement it always got. The override exists for the one built-in whose height is not
    derived at import: see :func:`_built_in_size`. It is a parameter rather than a second
    ``_placement`` literal at the call site for this function's own reason — a second dict
    on the second path is the two-implementations shape #547 cost.
    """
    c = reg.get(cid)
    return _placement(cid, edge=c.edge, size=c.size if size is None else size, **style)


def _built_in_size(c, value):
    """The size policy a committed ``size = <n>`` on the built-in *c* resolves to, or
    ``None`` for a number charter will not honour.

    **A committed value is accepted exactly where something reads it, and refused
    everywhere else — which is the rule :func:`component_tables` already keeps for
    ``edge``, not a new one.** The question is therefore *is this number read at launch*,
    and it is asked of :data:`frame.builtins.SLOT_OF` — the table that decides it —
    rather than of the component's declared policy CLASS.

    ``SLOT_OF`` is the whole of it because `layout._PLACED` is literally
    ``[c for c in every built-in if c.id in SLOT_OF]`` and `layout._derive` keys
    :data:`layout.SLOT_SIZE` off it, so *in ``SLOT_OF``* and *in ``SLOT_SIZE``* are the
    same set said twice. `tests/…_pinned_repo_strip…` asserts that equivalence for every
    registered component, because this function reads the cheap one and the launch path
    reads the other.

    * **In ``SLOT_OF`` — `identity`, `attention`, `sidebar`.** `layout._size_of` answers
      `top`, `bottom` and `right` out of `SLOT_SIZE`, derived once at import, and there is
      no per-plane override between `charter.toml` and `split-window` for them. A ``size``
      disagreeing with the declaration would be a value read, validated, stored and then
      ignored — the convincing empty the whole form is written against. It may only ECHO.
    * **In ``SLOT_OF`` and ``Content()`` — `repos`.** A `Content` height is not in
      `SLOT_SIZE` as anything but a floor: `layout.slot_sizes` routes it to
      `layout.repos_rows`, recomputed from the RESOLVED config on every launch and every
      `window-resized` (`commands_frame._reassert_sizes`). There is somewhere for a number
      to be read, and pinning it (#660) is what makes the strip a fixed height instead of
      one that collapses to two rows on a two-repo plane.
    * **Not in ``SLOT_OF`` — `chats`, `workspaces` (#687).** These are `Fixed(1)` and are
      placeable only from a ``[[frame.component]]`` table, so they were never *placed* and
      never entered `SLOT_SIZE`. `layout._size_of` misses the table and falls through to
      `layout._placed_here` → `layout._policy_cells`, which reads the committed policy off
      this resolved arrangement on every launch and hands it to ``split-window -l``.
      **Their number is read.** They took the echo branch anyway until #687, on a reason —
      "it could only be ignored" — that was measurably false for them: ``size = 2`` on a
      chat bar reached `_built_in_size`, came back ``None``, and took the operator's ENTIRE
      arrangement out of play (#535) in silence (see :data:`_HOTKEY_RE`'s note). Charter's
      own bar on `top` may now pick a height exactly as a provider's component on `top`
      already could.

    The value is `Fixed`, and `component.Fixed` is asked rather than re-checked: it refuses
    a `bool`, a non-int and anything below one cell in `component.cells`, which is the one
    place that rule is written. Asked inside a ``try`` because this runs while a committed
    file is being resolved — on the path of `charter --version` as much as `charter frame`
    — and a `ComponentError` escaping here would take every command with it, where the
    form's own answer to a value it cannot honour is to refuse the arrangement whole
    (#535).

    ``not isinstance(value, bool)`` stays on the echo branch and is not redundant beside
    it: ``True == 1`` in Python, so without it a ``size = true`` on `identity` or
    `attention` — both `Fixed(1)` — would compare equal and be accepted as the number
    nobody wrote. Four separate shapes of that trap have been caught in this suite. It is
    not needed on the two branches that build a `Fixed`, because `Fixed` refuses a `bool`
    itself.

    A policy that is neither `Content` nor `Fixed` — `Fill()`, which `personas` declares —
    has no number to echo and no height to pin, and is refused rather than asked for a
    ``.n`` it does not have.
    """
    from .frame import builtins as _builtins
    from .frame.component import ComponentError, Content, Fixed
    if isinstance(c.size, Content):
        try:
            return Fixed(value)
        except ComponentError:
            return None
    if not isinstance(c.size, Fixed):
        return None
    if c.id in _builtins.SLOT_OF:
        if value == c.size.n and not isinstance(value, bool):
            return c.size
        return None
    try:
        return Fixed(value)
    except ComponentError:
        return None


def component_tables(section, *, hotkey: str | None = None) -> list[dict] | None:
    """The ``[[frame.component]]`` arrangement *section* declares, or ``None``.

    **The projection of :func:`component_arrangement` that drops the reason**, and a
    separate function rather than a second resolver: every rule about what an arrangement
    may say lives once, next door, and this is the shape the twenty-odd callers that only
    want the placements already read. `frame_of` — the one caller that has somewhere to
    put a refusal — asks the other one.
    """
    placed, _why = component_arrangement(section, hotkey=hotkey)
    return placed


def _component_value(value) -> str:
    """A committed ``[[frame.component]]`` value, spelled the way its own file spells it.

    Every refusal below quotes the operator's line back at them, and a line they cannot
    find by searching for it is worth rather less: ``bg = "midnight"`` is what is in the
    file, and :func:`contain.readable` alone answers ``midnight``, which matches nothing.
    So a string gets its quotes and everything else — an int, a bool, a list, the table a
    single-bracket `[frame.component]` produces — gets `readable`'s rendering unchanged.

    The containment is `readable`'s either way and is not weakened by the two quote
    characters: it runs on the value FIRST, so what they wrap is already ASCII with every
    newline, control code and invisible codepoint escaped. A ``bg`` holding a `"` of its
    own comes out as one visible character inside charter's pair, which reads oddly and
    forges nothing — the property this is for.
    """
    return (f'"{contain.readable(value)}"' if isinstance(value, str)
            else contain.readable(value))


def _component_at(cid, n: int) -> str:
    """Which ``[[frame.component]]`` table a refusal is about.

    The ``use`` wherever one has already been established as a string, because that is the
    name an operator reads their own file by; the ordinal for a table whose ``use`` is
    itself the broken key, counting from 1 because that is how the file reads rather than
    how the list indexes.

    Contained even here: *cid* is a committed string on its way into a sentence `doctor`
    prints, and a table whose ``use`` is a newline would otherwise forge a row of charter's
    own report — :func:`harness_of`'s rule for :data:`contain.readable`, said about the
    other committed identifier that reaches the same surface.
    """
    return (f"on `{contain.readable(cid)}`" if isinstance(cid, str)
            else f"in `[[frame.component]]` table {n}")


def refused_arrangement_message(why: str) -> str:
    """The one sentence for a ``[[frame.component]]`` arrangement charter would not draw.

    Shared by `doctor.check_control_plane_config` and `commands_frame.frame_ready`
    (`--probe`, `charter frame-probe`) rather than written twice, for
    `commands_frame.no_renderer_message`'s reason: this is a standing property of the
    committed file, and two copies of a standing fact drift into two different facts.

    *why* is :func:`component_arrangement`'s second answer — the ONE key that did it,
    already contained. This adds what an operator cannot see for themselves: that the
    refusal is whole-arrangement rather than one table, and that the frame they are
    looking at is the `slots` one. Without that half, "``bg`` is not a colour word" reads
    as one pane's worth of missing paint rather than as the entire arrangement dropped.
    """
    return (f"`[[frame.component]]` is not in force — {why}. An arrangement charter "
            f"cannot draw is refused whole rather than one table at a time, so the frame "
            f"you get is the one `[frame] slots` describes and every other table's "
            f"`edge`, `size`, `bg`, `pad` and `key` goes with it. Fix that one key and "
            f"the arrangement comes back.")


def component_arrangement(section, *,
                          hotkey: str | None = None) -> tuple[list[dict] | None, str | None]:
    """The ``[[frame.component]]`` arrangement *section* declares, and why it was refused:
    ``(placements or None, reason or None)``.

    ``None`` placements means "nothing usable was declared here" — no tables at all, or an
    arrangement charter cannot draw — and the caller falls back to ``slots``, which falls
    back to ``density``, which falls back to the shipped default. Every layer of that is
    a frame charter is certain it can build.

    **The two ``None`` placements are different facts and the reason is what tells them
    apart (#738).** A plane that wrote no arrangement is every plane charter ships with,
    and there is nothing to say about it; a plane that wrote one charter threw away is a
    committed file whose declared layout is not the layout it has. Both used to come back
    as one bare ``None``, so the only surfaces that could have said so — `doctor` and
    `charter frame-probe` — had nothing to distinguish and said nothing. The reason is
    ``None`` for the first and a sentence for the second, and the sentence names the ONE
    key that did it, because "your arrangement was refused" without the key is a file to
    re-read line by line.

    **What counts as declared is the KEY's presence, not a usable value.** ``component =
    "identity"`` and ``component = []`` are both a key an operator wrote that decides
    nothing, which is the same class as the value refusals below; ``component`` absent is
    a plane spelled with `slots` and gets silence. That line is where the whole reporting
    risk sits: a warning that fires on correct configurations gets switched off and then
    protects nothing (#371, and `_BRANCH_MOVERS`' deletion), and *every* plane charter
    ships with — this repository's own included — takes the absent branch.

    Everything below this paragraph is unchanged from when this function was
    :func:`component_tables`, and each ``return`` now carries the sentence for its own
    refusal rather than sharing one bare ``None`` with fourteen others.

    **An arrangement is refused whole, and #535 is the reason it is not refused one table
    at a time.** The obvious design — drop the table charter cannot make sense of, keep
    the rest — hands the operator a frame with a panel silently missing from it, and a
    missing repo table is a plane that appears to have no clones. That is the exact change
    #535 shipped, and it was caught by a reviewer rather than by a test. So a single
    unusable value takes the whole arrangement out of play and the frame charter draws is
    the one `slots` describes: the operator sees their arrangement ignored, which is a
    visible, whole-frame difference, rather than one pane's worth of quiet fiction.

    **A provider's component is placeable here as of Phase 2, and that is what this
    function was blocking** (§4b property 4). It used to refuse any ``use`` outside
    `builtins.SLOT_OF` — charter's own four — because a placement dropped downstream would
    have been a panel silently absent with no pane to say why: every step from here to a
    painted pane spoke the four committed slot names. Those steps speak component ids now
    (`layout._derive`, `layout.panel_argvs`, `frame/slots.py:drawable`,
    `frame/panel.py:run`), so a component an installed distribution supplies is placed,
    split for, and drawn — and a provider that then fails to load costs its own pane and
    says why, which is where §4b's message finally has a surface to live on.

    **A provider this machine has no distribution for still refuses the arrangement
    whole**, and #535 is why that half did not change: charter cannot honour a rectangle
    for a component it cannot find, and dropping the one table would hand the operator a
    frame with a panel missing from it.

    **A provider's placement carries its own ``edge`` and ``size``, and both are
    required.** `config.FRAME` is resolved by every charter command, `charter --version`
    included, and the only way to ask a provider where it would like to sit is to IMPORT
    it — so a table that does not say is one charter would have to run a stranger's code
    to resolve, on every command, before anything was drawn. §4b's own example writes both
    keys. `default_edge`/`default_size` remain what they are documented as: a sensible
    arrangement before anyone configures one, which is a `Registry.place` with nothing
    passed, not a config boundary reading a package.

    **Present is not the same as usable, and the difference is what a charter command
    costs.** The ``size`` is checked here rather than left to `Fixed`, because this
    function runs on the IMPORT path: `config.derive` resolves ``FRAME`` *outside* the
    try/except that catches a malformed charter.toml, so a `ComponentError` raised out of
    `Fixed.__post_init__` does not degrade to the default frame — it takes down
    `import charter.config`, and with it every command on that clone, ``charter
    --version`` included. `size = 0`, `true`, `-4` and `"12"` each reach that raise if
    this line does not answer first — `component.cells` refuses all four, ``bool``
    explicitly, since `isinstance(True, int)` is `True` in Python and `Fixed(True)` would
    otherwise mean `Fixed(1)`. So the two checks agree on the answer and differ on what it
    costs: `cells` raises, which is right where a caller can catch it, and this returns
    ``None``, which is the whole-arrangement degrade the rest of this function makes. That
    is why ``bool`` is spelled out here as well and not left to the `isinstance(size,
    int)` beside it. An ``edge`` outside `EDGES` is quieter and no more correct —
    `layout._edge_of` falls through `_COLUMN_EDGES`/`_ROW_EDGES`/`_BEFORE_EDGES` and
    `"sideways"` becomes a plain ``-v`` after-split, a pane on an edge nobody asked for.
    A committed file arrives from someone else's machine; "committed" is not "trusted".

    **On a BUILT-IN, ``edge`` and ``size`` are still accepted only where charter can
    honour them, which means only at the component's own declaration.** `layout` derives
    the built-in geometry at import (`layout._derive`), so an ``edge = "top"`` on the repo
    table would be a value read, validated, stored and then ignored, and the frame would
    draw exactly as if the line were not there. A config key that changes nothing is
    exactly the convincing empty this phase was written against. What the pair IS good for
    meanwhile is writing the arrangement out in full: `frame_components` answers every
    `slots` list as tables that resolve back to the same frame, which is what makes the
    mapping lossless in both directions.

    **A ``key`` is the one value here that reaches tmux CONFIG TEXT**, which is why
    *hotkey* is a parameter rather than something read back later. `commands_frame
    .conf_text` writes ``bind -n {key} run-shell '…frame-toggle {name}'`` into the file
    ``source-file`` parses, so a component's key is in exactly the position ``[frame]
    hotkey`` was in when a newline in it ran a second tmux command at launch with no
    keypress (:data:`_HOTKEY_RE`). :func:`toggle_key` is that same constant, asked here.

    **``bg`` and ``pad`` are the two keys that describe how a pane LOOKS rather than where
    it is, and they are checked here for the two different reasons the rest of this
    function already keeps apart.** ``bg`` is checked because it reaches tmux: a style
    value is format-expanded at draw time, so the word is a key into :data:`FRAME_PANE_BG`
    and never a value out of a committed file (`[frame] chrome`'s containment, said per
    component). ``pad`` is checked because it reaches a repaint: it becomes ``" " * n`` on
    every paint of that pane, so :data:`FRAME_PANE_PAD_MAX` is what stands between a
    committed ``pad = 10**9`` and a panel process that stops answering. Both are refused
    the way everything else here is — whole arrangement, #535 — rather than by dropping the
    one key, because a pane that quietly lost the colour it asked for is a config value
    that changed nothing while claiming to decide something.

    *hotkey* is the key the frame's own palette is bound to — resolved by :func:`frame_of`
    before it calls this, because the comparison has to be against what will actually be
    bound, not against the shipped constant a plane may have moved off. It joins
    `frame/overlay.py`'s ``HATCH_KEY`` and `frame/tmuxctl.py`'s ``MOUSE_KEYS`` in the set
    of keys charter has already taken, and a component may have none of them. ``None``
    means the caller has no palette key to reserve, which is what resolving an arrangement
    outside a `[frame]` section has; the hatch and the two mouse keys are reserved
    regardless, because charter binds all three for every frame on its own server.
    """
    # **The absent key is the one branch that must never produce a reason**, and it is
    # asked before the value is looked at rather than inferred from the value being
    # missing: `component = false` is a key an operator wrote and `section.get` answers
    # `None` for a key nobody wrote, so a truthiness test here would put every plane on
    # earth one `get` away from a warning about a table it never had.
    declared = isinstance(section, dict) and FRAME_COMPONENT_KEY in section
    tables = section.get(FRAME_COMPONENT_KEY) if isinstance(section, dict) else None
    if not isinstance(tables, list):
        if not declared:
            return None, None
        # `[frame.component]` — ONE pair of brackets — is the typo this branch is really
        # about: TOML reads it as a table rather than an array of tables, so an operator
        # who wrote every key correctly and one bracket wrong got the default frame and no
        # word about it anywhere.
        return None, (f"`component = {_component_value(tables)}` is not an array of "
                      f"tables — an arrangement is spelled `[[frame.component]]`, with "
                      f"two brackets, once per component")
    if not tables:
        # No `declared` test, and its absence is deliberate: the only value that reaches
        # here is a list, and a key nobody wrote answers `None`, which is not one. A second
        # `if not declared` would be a guard no mutation could turn red — the shape the
        # deletion sweep calls a survivor.
        return None, "`component = []` places no components at all"
    from .frame import builtins as _builtins
    from .frame import overlay as _overlay
    from .frame import tmuxctl as _tmuxctl
    from .frame.component import EDGES, Fixed
    reg = _builtins.build()
    out: list[dict] = []
    seen: set[str] = set()
    # Every key charter has already bound for this frame, which a component may not take.
    # Built HERE rather than in :func:`frame_of` because reaching `frame/overlay.py` for
    # the hatch key is an import, and this line runs only once a plane has actually
    # written `[[frame.component]]` tables — `frame_of` itself is on the path of every
    # command, `charter --version` included, and must stay as cheap as it was.
    # `frame/tmuxctl.py` costs nothing extra: `frame/overlay.py` imports it already.
    #
    # The two MOUSE keys are here for the hatch's reason with the sign flipped. `conf_text`
    # writes them BEFORE the toggles, so tmux's last-wins leaves the COMPONENT's key alive
    # and charter's mouse handling silently gone — the wheel stops entering copy-mode and,
    # since #634, a click on a panel goes back to taking the keyboard off the harness. One
    # dead binding, nothing anywhere saying which, and the frame still launching at rc 0.
    # Named off `tmuxctl.MOUSE_KEYS` rather than spelled here, so the key this refuses is
    # the same object `conf_text` binds.
    #
    # `if k` drops a falsy *hotkey* — ``None`` for every caller with no palette key to
    # reserve — and it is the ONE place this set's invariant is established: `bound` holds
    # real keys and nothing else. The collision test below leans on that rather than
    # re-checking, so this filter is load-bearing; see the comment there for what a
    # ``None`` in here would cost.
    bound: set[str] = {k for k in (hotkey, _overlay.HATCH_KEY, *_tmuxctl.MOUSE_KEYS) if k}
    for n, table in enumerate(tables, 1):
        if not isinstance(table, dict):
            return None, (f"the entry {_component_at(None, n)} is "
                          f"{_component_value(table)} rather than a table of keys")
        # **File order, not sorted, and the sweep is what settled it.** Alphabetical was
        # the first spelling and `[swap-synonym] sorted -> list` survived it: no test could
        # tell the two apart, which is the sweep's definition of a line that earns nothing.
        # Asked again from the operator's side, file order is also the better answer — a
        # `dict` from `tomllib` preserves the order the keys were written in, so the key
        # this names is the first misspelt one going down their own file rather than the
        # one that happens to sort first.
        stray = [str(k) for k in table if k not in FRAME_COMPONENT_FIELDS]
        if stray:
            # Named one at a time and not "has unknown keys": a misspelt key is the
            # commonest way into this whole function, and the operator's next move is to
            # find that word in their file.
            form = ", ".join("`" + f + "`" for f in FRAME_COMPONENT_FIELDS)
            return None, (f"`{contain.readable(stray[0])}` "
                          f"{_component_at(table.get('use'), n)} is not a "
                          f"`[[frame.component]]` key — the whole form is {form}")
        cid = table.get("use")
        # **This one is not only a message, and the sweep is why that is written down.**
        # `[drop-if]` survived here, because a non-str `use` that IS hashable falls through
        # to the provider branch and is refused there anyway — so the guard looked like it
        # only chose which sentence came back. An UNHASHABLE one does not: `component = [{
        # use = ["identity"] }]` is four keystrokes of TOML, and without this line
        # `cid in seen` raises `TypeError: unhashable type` out of a function
        # `config.derive` resolves OUTSIDE its try/except — taking down `import
        # charter.config`, and with it `charter --version` and every other command on that
        # clone. Exactly the cost this function's docstring already records for `Fixed`.
        if not isinstance(cid, str):
            return None, (f"no `use` names a component {_component_at(None, n)} — every "
                          f"table needs one, and it is a component id rather than a "
                          f"`[frame] slots` word")
        if cid in seen:
            # **The one message here that interpolates a committed value RAW, and the
            # sweep is what forced the argument out into the open.** `[uncontain]` on a
            # `contain.readable` here survived: nothing could reach this line with a value
            # that needed containing, which makes the call dead code rather than defence —
            # and this project's rule is that an equivalent mutant and a dead line are the
            # same finding.
            #
            # Why it cannot be reached: `seen` only ever holds an id that completed a whole
            # pass of this loop, which means it got past `places()` or
            # `providers.supplies()`. The first is charter's own four-plus-two constants;
            # the second is an entry point NAME out of a `.dist-info`, which is INI and
            # cannot hold a newline. So a `use` needing containment is refused on its FIRST
            # occurrence and never becomes a duplicate — pinned by
            # `test_a_hostile_use_never_reaches_the_duplicate_message`.
            #
            # What would make it necessary again is a REORDER: move this test above the
            # two membership branches and the value stops being one that passed them. That
            # is exactly the "guard whose consequence depends on where another line sits"
            # shape #553 names, so it is written here rather than left to be rediscovered.
            return None, (f"`use = \"{cid}\"` is placed twice — a component sits in one "
                          f"rectangle, so charter has no answer for the second")
        seen.add(cid)
        visible = table.get("visible", True)
        if not isinstance(visible, bool):
            return None, (f"`visible = {_component_value(visible)}` "
                          f"{_component_at(cid, n)} is not `true` or `false`")
        # The two refusals a toggle key gets, each its own line because each is a
        # different thing going wrong and the sweep has to be able to tell them apart.
        key = table.get("key")
        # One: it is not a key charter will let reach a `bind` line at all. `[frame]
        # hotkey`'s own injection, arriving through a second committed key — see
        # :func:`toggle_key`, which is the SAME pattern and not a second one.
        if key is not None and toggle_key(key) is None:
            return None, (f"`key = {_component_value(key)}` {_component_at(cid, n)} is "
                          f"not a key charter will write into a `bind` line")
        # Two: something has already bound it. tmux key tables have no notion of a
        # conflict — the later `bind -n` simply replaces the earlier — so unrefused this
        # is one dead key and nothing anywhere saying which. `bound` starts holding the
        # keys charter binds for its own frame and grows by each component's, so one line
        # answers for all three collisions rather than three lines answering separately:
        #
        # * another component's key — a panel that cannot be toggled at all;
        # * the frame's own `hotkey` — `conf_text` writes the palette's bind BEFORE these,
        #   so this one takes the palette away from every frame on the socket, and with it
        #   every action §4h moved out of the deleted menu, leaving nothing to get them
        #   back with;
        # * `frame/overlay.py`'s `HATCH_KEY` — the escape hatch. `conf_text` writes that
        #   bind AFTER these, so tmux's last-wins leaves the hatch alive and the
        #   component's key silently dead. Measured on tmux 3.7c, sourcing both binds in
        #   the order `conf_text` emits them::
        #
        #       $ tmux -L t source-file both.tmux ; echo $?
        #       0
        #       $ tmux -L t list-keys -T root | grep F12
        #       bind-key -T root F12  run-shell -C "#{@charter_hatch}"
        #
        #   One line back, not two: the operator's key is simply gone, and `source-file`
        #   said nothing. Refused here anyway, and deliberately not left to that emission
        #   order — a guard whose consequence depends on where two other lines are
        #   emitted is a guard nothing can pin (#553), which is the same trap one level
        #   up from the one this sweep exists to catch.
        #
        # **No `key is not None` in front of this, and that is what makes the `if k`
        # filter above load-bearing rather than decorative.** *hotkey* is ``None`` for
        # every caller resolving an arrangement outside a `[frame]` section, so without
        # that filter ``bound`` would hold a ``None`` — and a component that simply
        # declares no toggle key (the common case: `key` absent, so ``key is None``)
        # would collide with it and take the WHOLE arrangement down, panels and all.
        # Guarding here instead would have made the filter unobservable, which is exactly
        # the shape the deletion sweep calls a survivor: a line nothing can fail without.
        # One invariant, stated once, where it is established.
        if key in bound:
            return None, (f"`key = {_component_value(key)}` {_component_at(cid, n)} "
                          f"is already bound — the frame's own palette, its escape hatch, "
                          f"its two mouse keys and each other component's `key` have it "
                          f"first, and tmux's later `bind` would silently replace the "
                          f"earlier one rather than report a conflict")
        if key is not None:
            bound.add(key)
        # The pane's own surface. Refused by NAME rather than passed through, which is
        # what `[frame] chrome`'s containment already is (:data:`FRAME_PANE_BG`): a tmux
        # style value is format-expanded at draw time and this file arrived from someone
        # else's machine, so the operator's word is a KEY into charter's table and the
        # value that reaches tmux comes out of that table. What is stored is
        # :func:`pane_bg`'s answer and not the object read out of the file, so a `str`
        # subclass with a hostile `__str__` cannot travel onward either.
        bg = table.get("bg")
        if bg is not None and pane_bg(bg) is None:
            return None, (f"`bg = {_component_value(bg)}` {_component_at(cid, n)} is not "
                          f"one of charter's {len(FRAME_PANE_BG)} pane background words "
                          f"(`default`, the {len(FRAME_PANE_COLOURS)} ANSI colour names, "
                          f"and each of those with a `bright` prefix)")
        # And the inset. Called ONCE, with the answer kept: `pad = table.get("pad", 0)`
        # followed by `if pane_pad(pad) is None` reads as two steps and is one — `pane_pad`
        # answers its own argument unchanged, so the second call could not have produced a
        # different value and the line below would have been a guard with nothing behind
        # it. The same is true of `bg` above, which is why `pane_bg` is asked there and not
        # again at the placement: it too answers the object it was given (see its
        # docstring — the containment lives in `pane_bg_options`, not in the return).
        #
        # `0` is a real declared value and falsy, so the refusal is `is None`. `if not
        # pane_pad(...)` would refuse `pad = 0` and say nothing else about it, which is the
        # spelling-not-property shape this project has paid for six times.
        pad = pane_pad(table.get("pad", 0))
        if pad is None:
            return None, (f"`pad = {_component_value(table.get('pad'))}` "
                          f"{_component_at(cid, n)} is not a whole number of cells from 0 "
                          f"to {FRAME_PANE_PAD_MAX}")
        # **`places`, not `cid in SLOT_OF`, and the difference is Phase 5's two bars.**
        # That table is the shorthand between a committed `[frame] slots` word and a
        # component id; this asks the question this branch is actually about — is *cid*
        # one charter's own registry puts on an edge. The two were the same set for as
        # long as every component charter placed had a slot name, and `chats` and
        # `workspaces` are the first that do not (`frame/builtins.places` argues why they
        # have none). Asked the old way they fell to the provider branch below, which
        # refuses anything no installed distribution supplies — so a component charter
        # registers, sizes, and can draw was one no configuration could ever ask for.
        if _builtins.places(cid, reg):
            c = reg.get(cid)
            if "edge" in table and table["edge"] != c.edge:
                return None, (f"`edge = {_component_value(table['edge'])}` "
                              f"{_component_at(cid, n)} is not the edge that built-in sits "
                              f"on — charter derives its geometry at import, so `{c.edge}` "
                              f"is the only value it can honour there")
            # The one key on a built-in that a plane may do more with than echo, and
            # :func:`_built_in_size` is where that asymmetry is argued: `identity`,
            # `attention` and `sidebar` are read out of a table `layout` derives at
            # IMPORT, so a different number there could only be ignored; `repos` is
            # `Content()` and its height is recomputed from this resolved arrangement on
            # every launch and every resize; `chats` and `workspaces` are in no derived
            # table at all, so `layout._placed_here` reads their number off this
            # arrangement too (#687). A number is honoured where it is READ. `None` is
            # refused the way every other unusable value in this loop is — whole
            # arrangement, #535.
            #
            # `None` rather than `c.size` for a table that names no size, and that is this
            # function's own #547 rule kept: `_built_in_placement` already declares what a
            # built-in's rectangle is, and a second copy of it here is the masked default
            # the sweep is written to catch — mutate the one in `_built_in_placement` and
            # nothing would move, because every call arriving through here carried its own.
            size = None
            if "size" in table:
                size = _built_in_size(c, table["size"])
                if size is None:
                    return None, (f"`size = {_component_value(table['size'])}` "
                                  f"{_component_at(cid, n)} is not a size charter reads "
                                  f"for that built-in — see `charter docs show frame`")
            out.append(_built_in_placement(reg, cid, size=size, visible=visible, key=key,
                                           bg=bg, pad=pad))
            continue
        # Not one of charter's own: a component id, which is placeable exactly when an
        # installed distribution declares it. Asked of entry point METADATA — nothing is
        # imported here, and a machine without the distribution refuses the arrangement
        # rather than drawing a frame with a hole where the panel was.
        #
        # **Three sequential tests where there was one `or` chain, and the split is the
        # reporting, not a change of behaviour.** The order is the chain's order and the
        # short-circuit is the same one — a `size < 1` still never runs against a value
        # `isinstance(size, int)` has not already admitted. What it buys is that the three
        # facts stop sharing a sentence: "no distribution on this machine supplies
        # `chats`" sends an operator to `pip install`, "`edge` must be one of four words"
        # sends them to their own file, and one message covering both said neither.
        edge, size = table.get("edge"), table.get("size")
        if not reg.providers.supplies(cid):
            return None, (f"`use = \"{contain.readable(cid)}\"` is not one of charter's "
                          f"own components and no installed distribution supplies it on "
                          f"this machine")
        if edge not in EDGES:
            return None, (f"`edge = {_component_value(edge)}` {_component_at(cid, n)} is "
                          f"not one of {', '.join('`' + e + '`' for e in EDGES)} — a "
                          f"component charter did not write must say which side it "
                          f"attaches to, and how many cells it takes")
        if not isinstance(size, int) or isinstance(size, bool) or size < 1:
            return None, (f"`size = {_component_value(size)}` {_component_at(cid, n)} is "
                          f"not a whole number of cells of 1 or more — a component "
                          f"charter did not write must say how big it is, because asking "
                          f"the provider would mean importing it on every charter command")
        out.append(_placement(cid, edge=edge, size=Fixed(size), visible=visible, key=key,
                              bg=bg, pad=pad))
    return out, None


def frame_components(cfg: dict) -> list[dict]:
    """The frame *cfg* asks for, as placements in split order — the one resolved answer.

    Three ways of asking for a frame, and this is where they become one thing:

    * ``[[frame.component]]`` is the arrangement written out, and wins when it is usable.
    * ``slots`` is shorthand for placing built-ins on their own edges in the given split
      ORDER — which is the geometry, not a reading order. Measured on tmux 3.7c at
      200x50: ``["top", "bottom", "right"]`` gives a **200-column** bottom row and
      ``["top", "right", "bottom"]`` gives **177**, inset beside the one sidebar and its
      border. (The 154 the spec's §4/§4b carry is the pre-#488 arrangement, with two
      22-column sidebars coming off the row; §4i corrects it.)
    * ``density`` is a named arrangement — three level names, each expanding to a ``slots``
      list an operator could have written by hand.

    All three go through :func:`frame_of` for the slot list, so nothing here is a second
    reading of a committed value: the filtering, the type checks and the density expansion
    happen once, where they always did.

    **Lossless in both directions.** Every list this answers can be written back out as
    ``[[frame.component]]`` tables — one per placement, carrying its ``use``, ``edge``,
    ``size``, ``visible``, ``key``, ``bg`` and ``pad``, which is
    :data:`FRAME_COMPONENT_FIELDS` whole — and :func:`component_tables` resolves those to
    the same placements again.
    That round trip is what "the config maps onto the registry" has to mean if `slots` is
    to be retired later without an operator's committed frame changing under them.

    **:func:`frame_of` is asked for the arrangement too, and not only for the slot list.**
    This used to call :func:`component_tables` a second time, directly — which was one
    reading of a committed value while that comment above said it was not. It became a
    real disagreement the moment a table grew a ``key``: `frame_of` resolves ``hotkey``
    and passes it down, so it refuses a component key that would steal the frame's palette,
    and a second call without it would have accepted the very arrangement `config.FRAME`
    had already thrown away. Two answers to one question, which is the shape #547 cost.
    """
    frame = frame_of(cfg)
    if frame["components"]:
        return frame["components"]
    from .frame import builtins as _builtins
    reg = _builtins.build()
    return [_built_in_placement(reg, _builtins.COMPONENT_OF[slot])
            for slot in frame["slots"]]


def frame_arrangement(frame: dict) -> list[str]:
    """Every component name *frame* can show, in split order — visible or not.

    **The universe a toggle and a density level both move within**, and the reason
    visibility can be the one mechanism. `slots` says which panels are ON; it cannot say
    which panels EXIST but are off, because deleting a name from a list loses the position
    with it. This answers the longer list, so a hidden component keeps its place in the
    split order — and that order is what a re-layout is asked with, so a level or a key
    never has to invent a position for a panel it brings back.

    *frame* is a RESOLVED ``[frame]`` mapping — :func:`frame_of`'s answer, which is
    exactly what `config.FRAME` holds. It is read rather than re-derived from a cfg on
    purpose: this is asked from inside a running frame (`commands_frame.cmd_toggle`,
    `commands_frame.cmd_density`), and re-resolving there would be a second reading of the
    committed file, taken at a different moment, free to disagree with the one the frame
    was launched from.

    Two sources, in the order the config boundary already ranks them:

    * an arrangement written out (``components``) names its own components, invisible ones
      included, and its file order is the split order;
    * otherwise ``slots``, which is the same list with nothing hidden in it.

    Then **charter's own built-ins that neither named, appended in shipped split order**.
    That tail is what keeps `[frame] slots = ["top", "bottom"]` able to answer the
    ``full`` density at all — a level names built-ins the plane never listed, and it has
    done so since presets existed. Appended rather than merged, because a name the plane
    did not write has no position of its own to be inserted at, and :data:`FRAME_SLOTS`'
    order is the only other one charter knows: for a plane whose list is a prefix of the
    shipped one — which is every plane that took a `slots` default and trimmed it — the
    result IS the shipped order.
    """
    placed = frame.get("components") or ()
    names = [p["slot"] for p in placed] or list(frame.get("slots") or ())
    return names + [s for s in FRAME_SLOTS if s not in names]


def frame_toggles(frame: dict) -> dict[str, str]:
    """name → the tmux key that shows or hides it, for every component *frame* binds one
    to, in split order.

    What `commands_frame.conf_text` turns into ``bind -n`` lines. Empty for every plane
    spelled with `slots` or `density`, and empty for an arrangement that declared no
    ``key`` — there is no default key and :data:`FRAME_COMPONENT_FIELDS` says why.

    Split order because a mapping keeps its insertion order and *frame*'s own placements
    are in it. Nothing downstream depends on that, but a `list-keys` an operator reads
    back should be in the order their file is written in rather than in whatever order a
    set happened to iterate.
    """
    return {p["slot"]: p["key"] for p in (frame.get("components") or ()) if p.get("key")}


#: The channels ``[update] channel`` may name, and a CLOSED set — the single most
#: important property in this module's newest section.
#:
#: ``charter.toml`` is COMMITTED. It arrives from someone else's machine, which is what
#: makes every value in it untrusted input (README.md's containment rule), and charter has
#: already been bitten twice by exactly that: ``[frame] hotkey`` reached tmux CONFIG TEXT
#: where a newline achieved code execution at launch (see :data:`_HOTKEY_RE`), and a
#: committed ``mcp.json`` key reached a generated sub-agent's YAML frontmatter as a bare
#: mapping key, where a newline declared a second MCP server running the credential wrapper
#: (#453 — bounded now by ``persona._MCP_NAME_RE``). A channel decides how charter
#: INSTALLS ITSELF, so a third door here would be the most expensive of the three.
#:
#: The defence is not a sanitiser. It is that a channel is one of two constants charter
#: wrote itself, matched here, and nothing an operator can type is ever passed through —
#: the same shape :data:`FRAME_DENSITY` chose for its level names, and stronger than any
#: escaping, because there is no value that survives to be escaped. Downstream the channel
#: is only ever COMPARED (``channel() == "dev"``); the repository URL and the installer
#: argv are module constants in `charter.commands_update`, never assembled from this.
UPDATE_CHANNELS = ("stable", "dev")

#: Every ``[update]`` setting, in the shape :data:`FRAME_FIELDS` documents: keyed by the
#: name :func:`update_of` returns it under, paired with ``(default, toml_key)``. One key
#: today; the shape is the point, so a second one cannot be added to a defaults dict and
#: forgotten in a spellings dict.
#:
#: ``stable`` is the default, and the default is what an unreadable, misspelt or hostile
#: value degrades to. That direction is deliberate: stable installs a signed, published
#: release, dev installs whatever ``main`` says this minute, so a plane that cannot be
#: understood must land on the conservative one.
UPDATE_FIELDS = {
    "channel": ("stable", "channel"),
}

#: The plain ``{key: default}`` view of :data:`UPDATE_FIELDS`.
UPDATE_DEFAULTS = {key: default for key, (default, _toml_key) in UPDATE_FIELDS.items()}


def update_of(cfg: dict) -> dict:
    """The ``[update]`` section merged over :data:`UPDATE_DEFAULTS`.

    Same contract as :func:`frame_of`, for the same reason: this module is imported by
    every command including ``charter --version``, so a hand-edited charter.toml must
    degrade to the defaults rather than raise.

    ``channel`` is matched against :data:`UPDATE_CHANNELS` and **the matched constant is
    what is stored, never the object the file supplied**. Belt and braces — ``tomllib``
    only ever produces a plain ``str`` — but it makes the guarantee structural rather than
    dependent on the parser: no object originating in a committed file can reach a caller
    of this function, so no caller can be the place that interpolates one.
    """
    out = dict(UPDATE_DEFAULTS)
    section = cfg.get("update")
    if not isinstance(section, dict):
        return out
    value = section.get(UPDATE_FIELDS["channel"][1])
    if isinstance(value, str):
        for known in UPDATE_CHANNELS:
            if value == known:
                out["channel"] = known       # the constant, not the file's string
                break
    return out


#: Every ``[harness]`` setting, in the shape :data:`FRAME_FIELDS` documents: keyed by the
#: name :func:`harness_of` returns it under, paired with ``(default, toml_key)``. One key
#: today, and the shape is the point — a second one cannot be added to a defaults dict and
#: forgotten in a spellings dict.
#:
#: ``default = "claude"`` is what makes bare ``charter`` launch the frame, and its shipped
#: default is ``None`` rather than a harness name. That is the whole decision of the
#: section: a plane that has said nothing keeps argparse's usage output, because guessing
#: a harness for somebody who never named one is charter choosing what runs on their
#: machine. ``[persona] default`` and ``[workspace] default`` spell the same idea with the
#: same word, which is why this section is ``[harness]`` and this key is ``default``.
#:
#: **The direction of the fallback is the opposite of :data:`UPDATE_FIELDS`', and both are
#: the conservative one.** ``update`` degrades to ``stable`` because *something* has to be
#: installed and the safe answer exists; here the safe answer is to run NOTHING, so an
#: unreadable value degrades to no default at all — and is reported rather than swallowed,
#: because a plane that declared a harness and gets the usage message is indistinguishable
#: from a plane that declared none. See :func:`harness_of`.
HARNESS_FIELDS = {
    "default": (None, "default"),
}

#: The plain ``{key: default}`` view of :data:`HARNESS_FIELDS`.
HARNESS_DEFAULTS = {key: default for key, (default, _toml_key) in HARNESS_FIELDS.items()}


def launchable_harnesses() -> tuple[str, ...]:
    """Every word ``[harness] default`` may name, in registration order.

    Each registered harness's :attr:`~charter.harness.base.Harness.cli_name` — the word an
    operator types after ``charter`` — read from the registry rather than written down
    here. That is the whole reason `harness/registry.py` exists: *"iterating KINDS means a
    harness added to it is covered everywhere the day it is registered, never a hardcoded
    literal in `init` or `doctor` that someone has to remember to update."* A tuple of
    three strings in this module would be exactly that literal, and it would go stale
    silently — the plane would refuse a harness charter can launch, and say the name is
    unknown.

    So this is deliberately NOT the shape :data:`FRAME_SLOTS` and :data:`UPDATE_CHANNELS`
    use. Those are closed sets this module OWNS and nothing else can extend; these names
    belong to another module, and copying them here would be a second registry.

    ``frame`` is not on this list, even though `charter frame` is registered by the same
    loop in `cli._add_frame_parsers`. It is the escape hatch for a command charter has
    never met and carries no command of its own, so a plane defaulting to it would get
    ``charter frame: nothing to run`` on every bare ``charter`` — a default that cannot
    launch is not a default. A harness with an empty ``cli_name`` is off the list for the
    same reason: the attribute's own docstring says empty means charter cannot launch it.

    Imported inside the function, the way `config.worktrees_root_for` imports `contain`:
    this module is imported by every command including ``charter --version``, and the
    harness package is only needed by the one caller below.
    """
    from .harness import registry

    return tuple(h.cli_name for h in registry.all() if h.cli_name)


def harness_of(cfg: dict) -> dict:
    """The ``[harness]`` section merged over :data:`HARNESS_DEFAULTS`.

    Same contract as :func:`frame_of` and :func:`update_of`, for the same reason: this
    module is imported by every command including ``charter --version``, so a hand-edited
    charter.toml must degrade rather than raise.

    ``default`` is matched against :func:`launchable_harnesses` and **the registry's own
    string is what is stored, never the object the file supplied** — :func:`update_of`'s
    rule, and it holds here for a sharper reason than there. This value becomes
    ``argv[0]`` of a `charter` invocation that `cli.main` then dispatches, and charter.toml
    is COMMITTED: it arrives from somebody else's machine (README.md's containment rule).
    Nothing an operator can type is passed through — the value is only ever *compared*, and
    what survives the comparison is a constant out of charter's own registry.

    **The refusal is recorded, not swallowed, and that is the one place this differs from
    the two sections above.** A misspelt ``[frame] chrome`` degrades to a shipped default
    and the frame still draws; a misspelt ``[update] channel`` degrades to ``stable`` and
    charter still installs. A misspelt ``[harness] default`` degrades to *no default*,
    which renders as argparse's usage message — byte-identical to what a plane that
    declared nothing gets. So the operator who committed ``default = "clyde"`` would see
    charter behaving exactly as though their key were not there, which is the failure shape
    #535 refuses an arrangement whole to avoid: a declared thing silently not in force.
    ``refused`` carries it out to the two readers that can say so — `cli.main`, on the bare
    launch the key is *for*, and `doctor.check_control_plane_config`, which is where
    `config.worktrees_root_for`'s own silently-ignored key is named for the same reason.

    ``refused`` is a key in the returned dict and not in :data:`HARNESS_FIELDS`, because it
    is not a SETTING — nothing in ``[harness]`` is spelled ``refused``. That is
    :func:`frame_of`'s own arrangement with ``components``, and it is set on every path so
    a caller never has to ask which one it came back from.

    The value is rendered through :func:`contain.readable` and never handed on raw: it is
    an identifier a sentence is about to tell somebody to go and fix, and it comes out of a
    committed file, where a newline forges a second line of charter's own report and an
    invisible codepoint names nothing at all.
    """
    out = dict(HARNESS_DEFAULTS)
    # Set before every early return, `frame_of`'s rule for `components`: a key present on
    # one path and absent on another is two shapes for one answer, and the caller would be
    # reading a `KeyError` on exactly the planes that declare no `[harness]` section.
    out["refused"] = None
    section = cfg.get("harness")
    if not isinstance(section, dict):
        return out
    toml_key = HARNESS_FIELDS["default"][1]
    if toml_key not in section:
        return out
    value = section[toml_key]
    for known in launchable_harnesses():
        if value == known:
            out["default"] = known       # the registry's constant, not the file's string
            return out
    # `tomllib` can hand this a list, a table, an int or a bool, and every one of them
    # reaches here rather than through a preceding `isinstance` check: a value of the wrong
    # TYPE is as declared, and as not in force, as a misspelt string, and reporting only
    # the strings would leave `default = ["claude"]` in the silent bucket this whole
    # function exists to empty.
    out["refused"] = contain.readable(value)
    return out
