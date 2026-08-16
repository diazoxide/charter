"""Locate the control plane this invocation operates on.

The engine used to find its data by its own file location
(``ROOT = Path(__file__).parent.parent``), which is exactly what made it unshippable:
move the code and the data goes with it. Instead, a control plane is marked by a
``charter.toml`` file, and ``charter`` walks up from the working directory to find it —
the same contract git, cargo and npm use, so ``cd`` anywhere inside a control plane and
commands simply work.
"""
from __future__ import annotations

import os
from pathlib import Path

#: The file whose presence marks a directory as a control plane.
MARKER = "charter.toml"

#: Environment override, for scripts, CI, and hooks invoked from elsewhere.
ENV_VAR = "CHARTER_ROOT"


class ControlPlaneNotFound(Exception):
    """No ``charter.toml`` above the starting directory (or at ``$CHARTER_ROOT``)."""


def _explain(where: str) -> str:
    return (f"no {MARKER} found {where}. Run `charter init` to create a control plane "
            f"here, or set ${ENV_VAR} to point at an existing one.")


def find_root(start: Path | None = None) -> Path:
    """The control plane's directory. Raises :class:`ControlPlaneNotFound`.

    ``$CHARTER_ROOT`` wins outright when set — and a bad value raises rather than falling
    back to a walk, because silently operating on a *different* control plane than the one
    the user named is worse than failing.
    """
    env = os.environ.get(ENV_VAR)
    if env:
        p = Path(env).expanduser()
        try:
            p = p.resolve()
        except OSError:
            raise ControlPlaneNotFound(_explain(f"at ${ENV_VAR}={env}")) from None
        if not (p / MARKER).is_file():
            raise ControlPlaneNotFound(_explain(f"at ${ENV_VAR}={env}"))
        return p

    cur = (start or Path.cwd()).resolve()
    for d in (cur, *cur.parents):
        if (d / MARKER).is_file():      # is_file, not exists: a directory is not a marker
            return _plane_of(d)

    # Nothing above us. Before giving up, ask whether we are standing in a linked
    # WORKTREE whose plane lives in the repo it was cut from.
    #
    # `_plane_of` handles the case where the worktree HAS a `charter.toml` — it redirects
    # to the main tree. It cannot fire here, because the walk above found no marker to
    # redirect from, and there is often none to find: `charter init` writes
    # `charter.toml` and never stages it, so a worktree cut from `main` does
    # not contain it. A worktree branched before the plane was committed has the same
    # shape. Following charter's own `enter:` line then landed a session in a plane-less
    # directory — no personas, no vault, memory written where `git worktree remove
    # --force` deletes it — while `doctor` reported everything green.
    #
    # Walking the main tree's PARENTS too is deliberate: a worktree of a fleet clone sits
    # at `workspaces/<ws>/<repo>/…`, so its plane is above the clone, not at it.
    for d in (cur, *cur.parents):
        main = main_worktree_of(d)
        if main is None:
            continue
        for m in (main, *main.parents):
            if (m / MARKER).is_file():
                return _plane_of(m)
        break                            # its main tree has no plane either — stop here

    raise ControlPlaneNotFound(_explain(f"in {cur} or any parent"))


def main_worktree_of(tree: Path) -> Path | None:
    """The MAIN working tree behind *tree*, when *tree* is a linked worktree — else ``None``.

    A linked worktree's ``.git`` is a file reading ``gitdir: <main>/.git/worktrees/<name>``,
    so the main tree is the directory holding that ``.git``. Pure path arithmetic: no
    subprocess, because this sits on the import path of every single charter command.

    ``None`` for the main tree itself (``.git`` is a directory), for a non-repo, and for a
    gitdir with no ``.git`` component — a worktree of a BARE repo (``/srv/repo.git/
    worktrees/x``) has no working tree to redirect to, so the caller must keep what it had.
    """
    g = Path(tree) / ".git"
    try:
        if g.is_dir():
            return None                       # already the main tree
        txt = g.read_text().strip()
    except OSError:
        return None
    if not txt.startswith("gitdir:"):
        return None
    p = Path(txt[len("gitdir:"):].strip())
    if not p.is_absolute():
        p = Path(tree) / p
    try:
        p = p.resolve()
    except (OSError, RuntimeError):
        return None
    for anc in p.parents:
        if anc.name == ".git":
            return anc.parent
    return None


def _plane_of(marked: Path) -> Path:
    """The plane a found marker really belongs to.

    A worktree is a *view of a repo*, not a repo — but ``charter.toml`` is a tracked file,
    so when the repo IS a plane (committed marker), every worktree cut from it gets its own
    copy checked out and therefore looks like its own control plane. Standing in one,
    charter used to resolve the plane to the worktree: personas, the vault and every written
    memory resolved into a directory ``git worktree remove`` deletes. Worktrees are a normal
    way to work, so that landed on the main path.

    Identity therefore follows the main working tree. The marker must be present there too
    — if it is not, this is not one plane seen from two directories and the found marker
    stands.

    ``$CHARTER_ROOT`` never reaches here: an explicit root wins outright, including when it
    names a worktree, which is the escape hatch for anyone who genuinely wants one.
    """
    main = main_worktree_of(marked)
    if main is not None and main != marked and (main / MARKER).is_file():
        return main
    return marked


def enclosing_plane(root: Path | None = None) -> Path | None:
    """The plane whose ``workspaces/`` contains *root*, or ``None``.

    ``charter.toml`` is tracked, so **every clone of a plane is itself a plane** — and
    `charter clone` puts clones at ``workspaces/<ws>/<repo>``. Standing in one, resolution
    stops at the first marker walking up, so the inner plane silently shadows the outer:
    its own `.charter/` state, its own vault registry, its own workspace pointers (#140).

    It is silent in **both** directions, which is what makes it expensive. The inner plane
    looks entirely normal — a `vault add` there reports success — while the outer plane
    simply never sees what you did.

    This only *detects* the nesting. Resolution is deliberately unchanged: the ambiguity is
    genuine, because sometimes the inner plane IS the one you mean (charter's own
    dogfooding clones charter into a workspace, and that clone is a control plane you might
    legitimately manage). ADR 0013's second rule covers exactly this shape — a divergence
    charter can see, charter names — so callers report it and leave the choice alone.
    """
    root = Path(root or find_root_or_cwd())
    try:
        here = root.resolve()
    except OSError:
        return None
    for parent in here.parents:
        if not (parent / MARKER).is_file():
            continue
        try:
            # `workspaces/` is where an outer plane puts clones. A marker further up that
            # does NOT own this path through its workspaces dir is an unrelated plane
            # somewhere above, not the one being shadowed.
            here.relative_to((parent / "workspaces").resolve())
        except (ValueError, OSError):
            continue
        return parent
    return None


def find_root_or_cwd(start: Path | None = None) -> Path:
    """Like :func:`find_root`, but falls back to the starting directory.

    Import-time path building must never explode — ``charter --version`` and
    ``charter init`` have to work outside a control plane. Commands that genuinely need
    one check :data:`charter.config.HAS_CONTROL_PLANE` and fail with a clear message.

    ``find_root`` itself keeps raising (its own docstring and callers rely on that): this
    function is the one place that guarantees no exception escapes, however hostile the
    environment.
    """
    try:
        return find_root(start)
    except (ControlPlaneNotFound, OSError, RuntimeError):
        # ControlPlaneNotFound is the expected "no charter.toml anywhere" case.
        # OSError/RuntimeError cover find_root's walk hitting something environmental
        # instead: Path.cwd() raising FileNotFoundError (the cwd was deleted out from
        # under the process), .resolve() raising RuntimeError (a symlink loop), or
        # is_file() propagating PermissionError on an inaccessible ancestor directory
        # (it only swallows ENOENT/ENOTDIR/EBADF/ELOOP, not EACCES). None of that is
        # find_root's contract to hide, but it IS this function's contract to survive.
        pass

    try:
        return (start or Path.cwd()).resolve()
    except (OSError, RuntimeError):
        # The plain fallback can itself fail the same way (e.g. cwd deleted, start
        # unresolvable). Prefer the caller's own `start` unresolved when they gave one —
        # still a perfectly usable Path, just not canonicalized, and better than nothing.
        # With no `start` at all, fall back to Path(".") rather than re-touching
        # Path.cwd(): "." is a relative reference that pathlib constructs without any
        # syscall, so it can't raise here even though the process's own working
        # directory is unusable — a sensible "still runs" answer beats raising at
        # import time.
        return start if start is not None else Path(".")
