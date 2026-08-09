"""Static configuration and well-known paths for the control plane."""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

from . import root as _root

#: The control plane this invocation operates on — located by a ``charter.toml`` marker,
#: NOT by where this package happens to live. That distinction is the whole point of the
#: engine/instance split: one installed charter serves many control planes.
ROOT = _root.find_root_or_cwd()

#: False when no ``charter.toml`` was found. Commands that need a control plane check this
#: and fail with a clear message; ``--version`` and ``init`` do not.
HAS_CONTROL_PLANE = (ROOT / _root.MARKER).is_file()

from . import instance as _instance

#: Fallbacks used when a control plane declares nothing (or none was found).
GROUP_FALLBACK = ""
DEFAULT_WORKSPACE_FALLBACK = "default"

#: Parsed once at import time. ``config`` is imported by every command (including
#: ``charter --version``), so a malformed ``charter.toml`` or a too-new schema must never
#: crash import — ``instance.load`` keeps raising (its own tests pin that); here the
#: failure is caught and recorded instead of propagated. ``doctor`` surfaces it to the user.
try:
    _cfg = _instance.load(ROOT)
    CONFIG_ERROR: str | None = None
except Exception as e:  # malformed TOML, schema too new, unreadable file
    _cfg, CONFIG_ERROR = {}, str(e)

#: The group/org whose repos this control plane tracks — from charter.toml, not baked in.
GROUP = _instance.group_of(_cfg, GROUP_FALLBACK)

#: Repos that must never appear in the inventory (typically the control plane itself).
EXCLUDE = _instance.exclude_of(_cfg)

#: The always-present workspace used when none is selected — from charter.toml, not baked in.
DEFAULT_WORKSPACE = _instance.default_workspace_of(_cfg, DEFAULT_WORKSPACE_FALLBACK)

#: How far a written memory travels — see charter.instance.SHARE_MODES.
MEMORY_SHARE = _instance.share_of(_cfg)

#: Which deployment this plane is — ``fleet`` (many clones per workspace) or ``embedded``
#: (charter installed inside the single codebase it serves). See charter.instance.SHAPES
#: for why this is declared rather than sniffed off the filesystem.
PLANE_SHAPE = _instance.shape_of(_cfg)


def worktrees_root_for(root: "Path", shape: str, cfg: dict) -> "Path | None":
    """Where worktrees live, or ``None`` to keep them per-workspace under ``.worktrees/``.

    ``$CHARTER_WORKTREES`` → ``[plane] worktrees`` → the shape's default. Relative values
    resolve against ROOT, so ``"../charter.worktrees"`` reads as written.

    The defaults differ because the shapes do. A **fleet** plane keeps the original
    layout: ``workspaces/<ws>/.worktrees/`` is already outside every clone, which was the
    whole point of that path. An **embedded** plane's clone IS the plane root, so the same
    path lands the worktrees inside the codebase and every root-level glob — pytest, jest,
    nx, tsc, an IDE indexer — sees the source several times over. There the default is a
    SIBLING of the repo: outside the tree, but adjacent to it rather than hidden away in a
    home directory, so it is findable with ``cd ..`` and obvious in a path.

    Only the worktrees move. ``workspaces/<ws>/memory/`` and ``refs/`` stay put — they are
    a few KB of text, and ``charter workspace live`` exists specifically to un-ignore them
    so a team can commit them. Relocating those would break sharing to fix a problem they
    do not have.

    Takes *root*/*shape*/*cfg* rather than reading the module globals so the test harness
    can re-derive it against a temp ROOT the way it already does for GROUP, EXCLUDE and
    the rest. When this read the globals directly it defaulted to the REAL repo's sibling
    in every test — which is outside the tmp tree, so the suite wrote worktrees into the
    developer's checkout directory and accumulated them across cases.
    """
    declared = os.environ.get("CHARTER_WORKTREES") or _instance.worktrees_of(cfg)
    if declared:
        p = Path(declared).expanduser()
        p = p if p.is_absolute() else (root / p)
    elif shape == "embedded":
        p = root.parent / f"{root.name}.worktrees"
    else:
        return None
    try:
        return p.resolve()
    except (OSError, RuntimeError):
        return p       # unresolvable (symlink loop, vanished parent) — still usable


#: Root for worktrees, or ``None`` for the per-workspace ``.worktrees/`` default.
WORKTREES_ROOT = worktrees_root_for(ROOT, PLANE_SHAPE, _cfg)

#: The SHARED half of the vault registry — committed, beside personas/ and inventory/.
#: Holds what is identical on every machine: provider, persona, op-vault, and a `file`
#: relative to the plane. Never a secret, and never the per-developer `account` (that
#: stays in `.charter/vaults.json`, which keeps overriding this one).
#:
#: A separate file rather than a section of charter.toml because `vault add` WRITES it:
#: charter.toml is hand-maintained, tomllib cannot write TOML, and `instance.
#: set_locked_version` already has to edit that file as raw text to keep the comments
#: people put in it. Machine-written config belongs somewhere a machine may rewrite whole.
SHARED_VAULTS = ROOT / "vaults.json"

#: Per-task workspaces live here: ``workspaces/<workspace>/<repo>`` (on-demand repo
#: clones) plus the workspace's own ``memory/`` and ``refs/``. Gitignored — a
#: workspace is a private, per-developer, per-task environment. (Renamed from the
#: old ``repos/``; ``workspace._ensure_layout`` migrates an existing ``repos/``.)
WORKSPACES_DIR = ROOT / "workspaces"

#: The durable source of truth: every repo in the group + metadata.
INVENTORY = ROOT / "inventory" / "repos.json"

#: Generated documentation.
DOCS_DIR = ROOT / "docs"

# --------------------------------------------------------------------------- #
# state directory — charter's per-developer home (vault registry, plain-file    #
# vaults, session/terminal pointers, persona runtime state). Gitignored, never  #
# committed.                                                                    #
#                                                                                #
# Historical note: charter was extracted from a tool formerly called `edm`,     #
# whose state directory was `.edm/` (override `$EDM_HOME`). The block below      #
# migrates an existing `.edm/` to `.charter/` transparently, once — the first    #
# time this control plane is used with the renamed tool — and warns loudly if   #
# any of the old `edm`-era env vars are still set rather than silently          #
# ignoring them (which would otherwise look like vaults had vanished).          #
# --------------------------------------------------------------------------- #
_LEGACY_ENV_VARS = (("EDM_HOME", "CHARTER_HOME"), ("EDM_WORKSPACE", "CHARTER_WORKSPACE"),
                    ("EDM_PERSONA", "CHARTER_PERSONA"))


def _warn_legacy_env_vars() -> None:
    """Print a loud stderr warning for each old `edm`-era env var that's still set —
    its value is never honored (only the new name is), so silence here would look
    like state (vaults!) vanished rather than simply needing a renamed env var."""
    for legacy, new in _LEGACY_ENV_VARS:
        if os.environ.get(legacy):
            print(f"charter: ${legacy} is no longer used (charter was renamed from `edm`) — "
                  f"set ${new} instead. Ignoring ${legacy}.", file=sys.stderr)


_warn_legacy_env_vars()


def _migrate_state_dir(root: Path) -> Path:
    """Resolve the state directory, migrating a legacy ``.edm/`` to ``.charter/`` once.

    - ``$CHARTER_HOME`` set → use it verbatim, no migration (the user chose a path).
    - Neither ``.charter/`` nor ``.edm/`` exists → the new default; nothing to migrate.
    - ``.charter/`` doesn't exist, ``.edm/`` does → ``os.rename`` it to ``.charter/``
      in place (atomic on the same filesystem, preserves permissions including 0600
      vault files) and print a one-line notice to stderr.
    - Both exist → never merge; warn on stderr naming both paths and keep using
      ``.charter/``.
    - The rename fails (e.g. cross-device) → don't crash; print an actionable error
      naming both paths and fall back to the legacy directory so vault access is
      never silently lost.
    """
    override = os.environ.get("CHARTER_HOME")
    if override:
        return Path(override)

    new_dir = root / ".charter"
    legacy_dir = root / ".edm"

    if new_dir.exists():
        if legacy_dir.exists():
            print(f"charter: both {new_dir} and {legacy_dir} exist — using {new_dir} and "
                  f"leaving {legacy_dir} untouched (never auto-merged). Remove the old "
                  "directory once you've confirmed nothing is missing.", file=sys.stderr)
        return new_dir

    if legacy_dir.exists():
        try:
            os.rename(legacy_dir, new_dir)
        except OSError as e:
            print(f"charter: could not migrate {legacy_dir} to {new_dir} ({e}) — "
                  f"continuing to use {legacy_dir}. Move it to {new_dir} manually, or set "
                  "$CHARTER_HOME to choose a location.", file=sys.stderr)
            return legacy_dir
        _repoint_vault_registry(legacy_dir, new_dir)
        print(f"charter: migrated state directory {legacy_dir} -> {new_dir}", file=sys.stderr)
        return new_dir

    return new_dir


def _repoint_vault_registry(legacy_dir: Path, new_dir: Path) -> None:
    """Rewrite absolute vault paths that still point into the moved directory.

    ``vaults.json`` stores each plain-file vault's location as an **absolute**
    path. Renaming the state directory therefore moves the vault files while
    leaving the registry pointing at where they used to be — every vault then
    reports "not created yet" and the credentials look lost, which is the exact
    outcome this migration exists to avoid.

    Only entries under ``legacy_dir`` are touched; a vault deliberately stored
    somewhere else (a shared drive, a per-machine path) is left alone. Any
    failure here is non-fatal and reported: the files themselves are already
    safely moved, and a stale registry is repairable by hand.
    """
    registry = new_dir / "vaults.json"
    if not registry.exists():
        return
    old_prefix = f"{legacy_dir}{os.sep}"
    new_prefix = f"{new_dir}{os.sep}"
    try:
        doc = json.loads(registry.read_text())
        vaults = doc.get("vaults", doc)
        if not isinstance(vaults, dict):
            return
        changed = 0
        for entry in vaults.values():
            if not isinstance(entry, dict):
                continue
            cfg = entry.get("config")
            if not isinstance(cfg, dict):
                continue
            path = cfg.get("file")
            if isinstance(path, str) and path.startswith(old_prefix):
                cfg["file"] = new_prefix + path[len(old_prefix):]
                changed += 1
        if changed:
            registry.write_text(json.dumps(doc, indent=2) + "\n")
            os.chmod(registry, 0o600)
            print(f"charter: repointed {changed} vault path(s) to {new_dir}",
                  file=sys.stderr)
    except (OSError, ValueError) as e:
        print(f"charter: state directory moved, but {registry} could not be "
              f"updated ({e}). Vault files are safe in {new_dir}; fix their "
              "'file' paths there, or re-add them with `charter vault add`.",
              file=sys.stderr)


#: Per-developer secrets home — vault registry + plain-file vaults. Gitignored,
#: never committed. Override with ``$CHARTER_HOME`` (e.g. to share across clones).
STATE_DIR = _migrate_state_dir(ROOT)

#: Registry of configured vaults (name -> provider + config + persona).
VAULTS_REGISTRY = STATE_DIR / "vaults.json"

#: Default on-disk location for plain-file vaults created without an explicit path.
#: Note: secrets are intentionally **cross-workspace** (global to the developer),
#: so vaults live under STATE_DIR, not inside any workspace.
VAULTS_DIR = STATE_DIR / "vaults"

#: Legacy shared active-workspace pointer. No longer read by ``resolve`` (it caused
#: one task's selection to leak into every other session); kept only so old files
#: don't error. Selection now lives per-terminal + per-session (below).
ACTIVE_WORKSPACE_FILE = STATE_DIR / "active-workspace"

#: Per-Claude-session active-workspace pointers, keyed by session id, so parallel
#: sessions in one clone can each select a different workspace.
SESSIONS_DIR = STATE_DIR / "sessions"

#: Per-terminal active-workspace pointers, keyed by a stable terminal id
#: (``$TERM_SESSION_ID``/``$WINDOWID``/``$TMUX_PANE``/tty). Unlike the Claude session
#: id, a terminal pane survives closing and reopening Claude, so a pane keeps its own
#: workspace across restarts — without leaking into other panes.
TERMINALS_DIR = STATE_DIR / "terminals"

#: Persona definitions — **committed** and shared with the team (unlike vaults).
#: A persona is a directory ``personas/<name>/`` holding ``persona.md`` (the
#: definition), ``memory/`` and ``refs/`` (persistent, committed knowledge). The
#: legacy flat ``personas/<name>.md`` layout still resolves (see ``persona.py``).
PERSONAS_DIR = ROOT / "personas"

#: The cross-persona namespace: ``personas/_shared/{memory,refs}`` is knowledge
#: every persona can read/write. Not itself a persona (excluded from listings).
SHARED_PERSONA = "_shared"

#: Per-developer persona runtime state — **ephemeral** memory (session-scoped
#: scratch, auto-pruned) and the local activity log. Gitignored (under STATE_DIR),
#: never committed; this is the counterpart to the committed ``personas/*/memory``.
PERSONA_STATE_DIR = STATE_DIR / "persona-state"

#: Local pointer to the active persona (set by ``charter persona use``). Overridden
#: by ``$CHARTER_PERSONA`` and by a command's ``--persona``. Gitignored (in STATE_DIR).
ACTIVE_PERSONA_FILE = STATE_DIR / "active-persona"
