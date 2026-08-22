"""Static configuration and well-known paths for the control plane.

Every value below is a function of ONE input — the control-plane root — so they are
computed in one place, :func:`derive`, and applied to this module by :func:`use`.

They used to be twenty-five separate module-level statements, and `tests/_isolation.py`
re-implemented all of them line for line to point a test at a temp directory. That
duplication has already failed in production: four constants were missing from the
harness, so the suite wrote fixture data into a contributor's real `.charter/vaults.json`
and orphaned every vault registered on that machine. The guard added in response can only
inspect `Path`-typed names; seven non-`Path` ones of exactly the same shape exist.

With one definition, a new setting is isolated the day it is added, because the harness
calls `use()` rather than knowing what to copy.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

from . import instance as _instance
from . import root as _root

#: Fallbacks used when a control plane declares nothing (or none was found).
GROUP_FALLBACK = ""
DEFAULT_WORKSPACE_FALLBACK = "default"

#: The cross-persona namespace: ``personas/_shared/{memory,refs}`` is knowledge
#: every persona can read/write. Not itself a persona (excluded from listings).
#: Not derived — it is the same name in every control plane.
SHARED_PERSONA = "_shared"


def worktrees_root_for(root: "Path", cfg: dict) -> "Path | None":
    """An explicitly relocated worktree root, or ``None`` for the default.

    ``$CHARTER_WORKTREES`` → ``[plane] worktrees`` → ``None``. Relative values resolve
    against ROOT, so ``"../charter.worktrees"`` reads as written.

    ``None`` means the standard layout: ``workspaces/<ws>/.worktrees/``, which is already
    outside every clone — the whole point of that path, and correct whenever a workspace
    holds clones, which is now always. There used to be a second default here, for a plane
    whose clone WAS its own root; that shape is gone.

    Only the worktrees ever move. ``workspaces/<ws>/memory/`` and ``refs/`` stay put — a
    few KB of text that ``charter workspace live`` exists specifically to un-ignore so a
    team can commit them. Relocating those would break sharing to fix a problem they do
    not have.

    Takes *root*/*cfg* rather than reading the module globals so the test harness can
    re-derive it against a temp ROOT the way it already does for GROUP, EXCLUDE and the
    rest. When this read the globals directly it defaulted to the REAL repo's sibling in
    every test — outside the tmp tree — so the suite wrote worktrees into the developer's
    checkout and accumulated them across cases.

    **The two sources are not the same kind of input** (#339). ``$CHARTER_WORKTREES`` is
    set by the person at the machine, on their own machine, and takes anything. ``[plane]
    worktrees`` is committed and shared — a teammate writes it, `git worktree add` then
    creates directories wherever it points, and on 0.47.2 ``"~/../../etc/charter-worktrees"``
    resolved to ``/private/etc/charter-worktrees`` with nothing between the two. A committed
    value is now held to :func:`contain.plane_adjacent`: the plane, or one sibling of it,
    which is exactly the ``"../charter.worktrees"`` shape this docstring documents.

    A refused value falls back to ``None`` — the standard in-plane layout — because this
    runs inside :func:`derive`, on every import, where raising would cost the CLI rather
    than the setting. Falling back silently would be its own defect, so
    ``doctor.check_control_plane_config`` asks :func:`contain.plane_adjacent_refusal` the
    same question and names it.
    """
    from . import contain
    env = os.environ.get("CHARTER_WORKTREES")
    declared = env or _instance.worktrees_of(cfg)
    if not declared:
        return None
    p = Path(declared).expanduser()
    p = p if p.is_absolute() else (root / p)
    if not env and not contain.plane_adjacent(root, p):
        return None
    try:
        return p.resolve()
    except (OSError, RuntimeError):
        return p       # unresolvable (symlink loop, vanished parent) — still usable


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


def derive(root: Path, start: Path | None = None) -> dict:
    """Every setting that follows from *root*, as ``{NAME: value}``.

    The single definition. :func:`use` applies it; the module bootstraps itself with it
    at import; the test harness calls :func:`use` instead of copying any of it.

    *start* is the directory the plane was located FROM, and matters only for
    ``NESTED_ORIGIN`` — "which plane am I standing in" is a fact about a directory, not
    about the root that directory resolved to. ``None`` means the process cwd, which is the
    truth at import; :func:`use` passes *root* instead so the test harness is not reading
    the developer's actual working directory (it was: the suite ran from a worktree inside
    a nested clone and picked up the real one).
    """
    root = Path(root)
    d: dict = {}

    #: The control plane this invocation operates on — located by a ``charter.toml``
    #: marker, NOT by where this package happens to live. That distinction is the whole
    #: point of the engine/instance split: one installed charter serves many planes.
    d["ROOT"] = root

    #: False when no ``charter.toml`` was found. Commands that need a control plane check
    #: this and fail with a clear message; ``--version`` and ``init`` do not.
    d["HAS_CONTROL_PLANE"] = (root / _root.MARKER).is_file()

    #: The nested plane the caller is STANDING IN, when one encloses it — else ``None``.
    #:
    #: `find_root` hops outward through ``workspaces/`` so the plane holding the vault
    #: wins, which means ``enclosing_plane(ROOT)`` is ``None`` by construction whenever the
    #: hop fired. Without recording the origin, charter would silently act on a plane the
    #: operator cannot see it choose. Three states, and each reads differently:
    #:
    #: * ``None`` — no nesting. Say nothing.
    #: * ``!= ROOT`` — the hop fired: standing in a plane, acting on the one above it.
    #: * ``== ROOT`` — ``$CHARTER_ROOT`` pinned this session INSIDE the nested plane, so
    #:   the hop was overridden and the hazard #140 described is live.
    d["NESTED_ORIGIN"] = _root.standing_in_nested_plane(start)

    #: Parsed once. ``config`` is imported by every command (including ``charter
    #: --version``), so a malformed ``charter.toml`` or a too-new schema must never crash
    #: import — ``instance.load`` keeps raising (its own tests pin that); here the failure
    #: is caught and recorded instead of propagated. ``doctor`` surfaces it to the user.
    try:
        cfg = _instance.load(root)
        d["CONFIG_ERROR"] = None
    except Exception as e:            # malformed TOML, schema too new, unreadable file
        cfg, d["CONFIG_ERROR"] = {}, str(e)

    #: The group/org whose repos this control plane tracks — from charter.toml, not baked in.
    d["GROUP"] = _instance.group_of(cfg, GROUP_FALLBACK)

    #: Repos that must never appear in the inventory (typically the control plane itself).
    d["EXCLUDE"] = _instance.exclude_of(cfg)

    #: The always-present workspace used when none is selected — from charter.toml.
    d["DEFAULT_WORKSPACE"] = _instance.default_workspace_of(cfg, DEFAULT_WORKSPACE_FALLBACK)

    #: How far a written memory travels — see charter.instance.SHARE_MODES.
    d["MEMORY_SHARE"] = _instance.share_of(cfg)

    #: Root for worktrees, or ``None`` for the per-workspace ``.worktrees/`` default.
    d["WORKTREES_ROOT"] = worktrees_root_for(root, cfg)

    #: The SHARED half of the vault registry — committed, beside personas/ and inventory/.
    #: Holds what is identical on every machine: provider, persona, op-vault, and a `file`
    #: relative to the plane. Never a secret, and never the per-developer `account` (that
    #: stays in `.charter/vaults.json`, which keeps overriding this one).
    #:
    #: A separate file rather than a section of charter.toml because `vault add` WRITES it:
    #: charter.toml is hand-maintained, tomllib cannot write TOML, and
    #: `instance.set_locked_version` already has to edit that file as raw text to keep the
    #: comments people put in it. Machine-written config belongs somewhere a machine may
    #: rewrite whole.
    d["SHARED_VAULTS"] = root / "vaults.json"

    #: Per-task workspaces live here: ``workspaces/<workspace>/<repo>`` (on-demand repo
    #: clones) plus the workspace's own ``memory/`` and ``refs/``. Gitignored — a workspace
    #: is a private, per-developer, per-task environment. (Renamed from the old ``repos/``;
    #: ``workspace._ensure_layout`` migrates an existing ``repos/``.)
    d["WORKSPACES_DIR"] = root / "workspaces"

    #: The durable source of truth: every repo in the group + metadata.
    d["INVENTORY"] = root / "inventory" / "repos.json"

    #: Generated documentation.
    d["DOCS_DIR"] = root / "docs"

    #: Per-developer secrets home — vault registry + plain-file vaults. Gitignored, never
    #: committed. Override with ``$CHARTER_HOME`` (e.g. to share across clones).
    state = _migrate_state_dir(root)
    d["STATE_DIR"] = state

    #: Registry of configured vaults (name -> provider + config + persona).
    d["VAULTS_REGISTRY"] = state / "vaults.json"

    #: Default on-disk location for plain-file vaults created without an explicit path.
    #: Note: secrets are intentionally **cross-workspace** (global to the developer), so
    #: vaults live under STATE_DIR, not inside any workspace.
    d["VAULTS_DIR"] = state / "vaults"

    #: Legacy shared active-workspace pointer. No longer read by ``resolve`` (it caused one
    #: task's selection to leak into every other session); kept only so old files don't
    #: error. Selection now lives per-terminal + per-session.
    d["ACTIVE_WORKSPACE_FILE"] = state / "active-workspace"

    #: Per-Claude-session active-workspace pointers, keyed by session id, so parallel
    #: sessions in one clone can each select a different workspace.
    d["SESSIONS_DIR"] = state / "sessions"

    #: Per-terminal active-workspace pointers, keyed by a stable terminal id. Unlike the
    #: Claude session id, a terminal pane survives closing and reopening Claude, so a pane
    #: keeps its own workspace across restarts — without leaking into other panes.
    d["TERMINALS_DIR"] = state / "terminals"

    #: Persona definitions — **committed** and shared with the team (unlike vaults). A
    #: persona is a directory ``personas/<name>/`` holding ``persona.md`` (the definition),
    #: ``memory/`` and ``refs/`` (persistent, committed knowledge). The legacy flat
    #: ``personas/<name>.md`` layout still resolves (see ``persona.py``).
    d["PERSONAS_DIR"] = root / "personas"

    #: Per-developer persona runtime state — **ephemeral** memory (session-scoped scratch,
    #: auto-pruned) and the local activity log. Gitignored (under STATE_DIR), never
    #: committed; the counterpart to the committed ``personas/*/memory``.
    d["PERSONA_STATE_DIR"] = state / "persona-state"

    #: Local pointer to the active persona (set by ``charter persona use``). Overridden by
    #: ``$CHARTER_PERSONA`` and by a command's ``--persona``. Gitignored (in STATE_DIR).
    d["ACTIVE_PERSONA_FILE"] = state / "active-persona"

    #: Drafted upstream **reports** — charter's own bugs and gaps, awaiting the Reporter's
    #: approval before anything is published (see charter/report.py, docs/adr/0003).
    #: Under STATE_DIR because it is per-developer and gitignored: a draft may quote an
    #: exception message that has not been redacted yet, so it must never be committable.
    d["REPORTS_DIR"] = state / "reports"

    return d


#: Every name :func:`derive` produces. The harness snapshots exactly these, so a setting
#: added to `derive` is isolated in tests without anyone remembering to list it anywhere.
DERIVED = tuple(derive(Path(".")).keys())


def use(root) -> dict:
    """Point this module at *root*, returning the values it replaced.

    The seam the test harness needs: `PersonaIso` calls this instead of re-implementing
    the derivation, and restores by handing the snapshot to :func:`restore`.
    """
    previous = {k: globals().get(k) for k in DERIVED}
    # `start=root`, never the process cwd: a test that redirected ROOT into a tmp dir must
    # not have `NESTED_ORIGIN` answered from wherever the suite happens to be running.
    globals().update(derive(root, start=root))
    return previous


def restore(previous: dict) -> None:
    """Put back what :func:`use` returned."""
    globals().update(previous)


# Bootstrap: locate the plane the same way every command does, and derive from it.
_warn_legacy_env_vars()
globals().update(derive(_root.find_root_or_cwd()))
