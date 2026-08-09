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
    so in an **embedded** plane every worktree gets its own copy checked out and therefore
    looks like its own control plane. Standing in one, charter used to resolve the plane to
    the worktree: the status line went blank (``root_tree`` requires ``.git`` to be a
    DIRECTORY, and a linked worktree's is a file), and personas, the vault and every
    written memory resolved into a directory ``git worktree remove`` deletes. Worktrees are
    how you are meant to work in an embedded plane, so that landed on the main path.

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
