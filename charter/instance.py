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


#: The most cells a component may inset its content by, per side.
#:
#: **A cap and not a clamp**: a ``pad`` above this is REFUSED by
#: :func:`component_tables` (which refuses the arrangement whole, #535's rule), not
#: quietly reduced to 8. A value read, validated and then changed into a different one is
#: the convincing empty this section refuses everywhere else.
#:
#: Eight because the pad comes out of the pane's own width and the frame's narrowest pane
#: is the 22-column sidebar (`layout.SLOT_SIZE`): two eights out of twenty-two leaves six
#: cells, which is already past useful. The cap's real job is the other end — ``pad =
#: 10**9`` in a committed file is a ``" " * n`` on the repaint path, and this is the line
#: that stops it being one.
FRAME_PANE_PAD_MAX = 8


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
    """
    for placed in frame.get("components") or ():
        if isinstance(placed, dict) and name in (placed.get("slot"), placed.get("use")):
            # `or 0` rather than `.get("pad", 0)`, and the difference is a real case: a
            # placement built by a charter that predates this key has no `pad` at all
            # (`.get` covers that), and one built with `pad=None` has the key with nothing
            # in it (`.get` would hand `None` to `" " * n`). Both mean "no pad", so the
            # falsy read is the one that says so — this is not the `if not pane_pad(v)`
            # mistake one function up, where `0` and "refused" are different answers.
            return {"bg": placed.get("bg"), "pad": placed.get("pad") or 0}
    return {"bg": None, "pad": 0}


def verbosity_for(level) -> str:
    """How much each panel says at *level*. :data:`DEFAULT_VERBOSITY` for anything else.

    Anything else is a real case, not a defensive one: a frame's live density override is
    read off disk (`frame.state.density`), and a frame started by an older charter has no
    file there at all. Both answer ``None``, and a panel must draw the ordinary amount
    rather than nothing.
    """
    lv = density_level(level)
    return FRAME_DENSITY[lv]["verbosity"] if lv else DEFAULT_VERBOSITY

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

    Four keys need more than a type check, and all four get it here rather than
    downstream: ``slots`` is filtered against :data:`FRAME_SLOTS`, ``density`` against
    :data:`FRAME_DENSITY`, ``chrome`` against :data:`FRAME_CHROME`, and ``hotkey``
    against :data:`_HOTKEY_RE` — see those two constants for the injection a bare
    ``isinstance(value, str)`` lets through in each case. All four degrade to
    the shipped default, which is the contract every other key in this function already
    keeps: a charter.toml charter cannot make sense of never stops charter from running.

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
    placed = component_tables(section, hotkey=out["hotkey"])
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


def _built_in_placement(reg, cid: str, *, visible: bool = True,
                        key: str | None = None, bg: str | None = None,
                        pad: int = 0) -> dict:
    """:func:`_placement` for one of charter's own, in the rectangle it declares."""
    c = reg.get(cid)
    return _placement(cid, edge=c.edge, size=c.size, visible=visible, key=key,
                      bg=bg, pad=pad)


def component_tables(section, *, hotkey: str | None = None) -> list[dict] | None:
    """The ``[[frame.component]]`` arrangement *section* declares, or ``None``.

    ``None`` means "nothing usable was declared here" — no tables at all, or an
    arrangement charter cannot draw — and the caller falls back to ``slots``, which falls
    back to ``density``, which falls back to the shipped default. Every layer of that is
    a frame charter is certain it can build.

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
    `frame/overlay.py`'s ``HATCH_KEY`` in the set of keys charter has already taken, and a
    component may have neither. ``None`` means the caller has no palette key to reserve,
    which is what resolving an arrangement outside a `[frame]` section has; the hatch is
    reserved regardless, because charter binds it for every frame on its own server.
    """
    tables = section.get(FRAME_COMPONENT_KEY) if isinstance(section, dict) else None
    if not isinstance(tables, list) or not tables:
        return None
    from .frame import builtins as _builtins
    from .frame import overlay as _overlay
    from .frame.component import EDGES, Fixed
    reg = _builtins.build()
    out: list[dict] = []
    seen: set[str] = set()
    # Every key charter has already bound for this frame, which a component may not take.
    # Built HERE rather than in :func:`frame_of` because reaching `frame/overlay.py` for
    # the hatch key is an import, and this line runs only once a plane has actually
    # written `[[frame.component]]` tables — `frame_of` itself is on the path of every
    # command, `charter --version` included, and must stay as cheap as it was.
    bound: set[str] = {k for k in (hotkey, _overlay.HATCH_KEY) if k}
    for table in tables:
        if not isinstance(table, dict):
            return None
        if any(k not in FRAME_COMPONENT_FIELDS for k in table):
            return None
        cid = table.get("use")
        if not isinstance(cid, str) or cid in seen:
            return None
        seen.add(cid)
        visible = table.get("visible", True)
        if not isinstance(visible, bool):
            return None
        # The two refusals a toggle key gets, each its own line because each is a
        # different thing going wrong and the sweep has to be able to tell them apart.
        key = table.get("key")
        # One: it is not a key charter will let reach a `bind` line at all. `[frame]
        # hotkey`'s own injection, arriving through a second committed key — see
        # :func:`toggle_key`, which is the SAME pattern and not a second one.
        if key is not None and toggle_key(key) is None:
            return None
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
        if key is not None and key in bound:
            return None
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
            return None
        # And the inset. `0` is a real declared value and falsy, so the refusal is `is
        # None` — `if not pane_pad(pad)` would refuse `pad = 0` and accept nothing else
        # about it, which is the spelling-not-property shape this project has paid for
        # six times.
        pad = table.get("pad", 0)
        if pane_pad(pad) is None:
            return None
        if cid in _builtins.SLOT_OF:
            c = reg.get(cid)
            if "edge" in table and table["edge"] != c.edge:
                return None
            if "size" in table and not (isinstance(c.size, Fixed)
                                        and table["size"] == c.size.n
                                        and not isinstance(table["size"], bool)):
                return None
            out.append(_built_in_placement(reg, cid, visible=visible, key=key,
                                           bg=pane_bg(bg), pad=pane_pad(pad)))
            continue
        # Not one of charter's own: a component id, which is placeable exactly when an
        # installed distribution declares it. Asked of entry point METADATA — nothing is
        # imported here, and a machine without the distribution refuses the arrangement
        # rather than drawing a frame with a hole where the panel was.
        edge, size = table.get("edge"), table.get("size")
        if (not reg.providers.supplies(cid) or edge not in EDGES
                or not isinstance(size, int) or isinstance(size, bool) or size < 1):
            return None
        out.append(_placement(cid, edge=edge, size=Fixed(size), visible=visible, key=key,
                              bg=pane_bg(bg), pad=pane_pad(pad)))
    return out


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
