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
from collections.abc import Mapping
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


def find_root(start: Path | None = None, env: Mapping[str, str] | None = None) -> Path:
    """The control plane's directory. Raises :class:`ControlPlaneNotFound`.

    ``$CHARTER_ROOT`` wins outright when set — and a bad value raises rather than falling
    back to a walk, because silently operating on a *different* control plane than the one
    the user named is worse than failing.

    *env* answers the question for a process that is **not this one**: given THIS
    environment, standing HERE, which plane would it resolve? Defaults to this process's
    own, so every existing caller is unchanged. It exists because charter spawns copies of
    itself (`util.detach_self`, `glstate.maybe_spawn`, `update.maybe_spawn`) and the answer
    for the child is decided by the environment the parent is about to hand it — which the
    parent can then check, or correct, *before* the fork. The suite's spawn tripwire
    (``tests/_planeguard.py``) is the caller that needs it, and asking here rather than
    re-deriving the walk there is what keeps the two from drifting apart: the walk below is
    where every subtlety lives (the worktree redirect, the ``workspaces/`` hop outward).
    """
    named = (os.environ if env is None else env).get(ENV_VAR)
    if named:
        p = Path(named).expanduser()
        try:
            p = p.resolve()
        except OSError:
            raise ControlPlaneNotFound(_explain(f"at ${ENV_VAR}={named}")) from None
        if not (p / MARKER).is_file():
            raise ControlPlaneNotFound(_explain(f"at ${ENV_VAR}={named}"))
        return p

    cur = (start or Path.cwd()).resolve()
    for d in (cur, *cur.parents):
        if (d / MARKER).is_file():      # is_file, not exists: a directory is not a marker
            return _outermost(_plane_of(d))

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
                return _outermost(_plane_of(m))
        break                            # its main tree has no plane either — stop here

    raise ControlPlaneNotFound(_explain(f"in {cur} or any parent"))


def _outermost(marked: Path) -> Path:
    """Follow ``workspaces/`` nesting outward until no plane encloses this one.

    The innermost marker is the git/cargo/npm contract and is right almost everywhere. It
    is wrong for the one structure charter builds itself: `charter clone` puts clones at
    ``workspaces/<ws>/<repo>``, and a cloned repo may carry its own tracked
    ``charter.toml``. Standing in one, the active plane silently became a different plane —
    different personas, no vault, memory written where nobody chose (#200).

    #140 detected that and left resolution alone, reasoning the inner plane is sometimes
    the one you mean and naming charter's own dogfooding as the case. Measured against that
    case, the inner plane carries no vault, a subset of the workspaces, and *tracked*
    persona files, so a memory written there lands in the cloned repo's git index instead
    of the operator's plane. The justification did not survive its own example.

    **Not "outermost marker wins".** The hop is allowed only through an enclosing plane's
    own ``workspaces/`` — `enclosing_plane`'s existing test. A bare walk to the topmost
    marker would let one stray ``charter.toml`` in ``~`` swallow every plane beneath it,
    turning a narrow mistake into a total one.

    Loops until the answer stops moving, so a plane inside a plane inside a plane lands on
    the one actually holding the vault rather than in the middle. ``seen`` guards against a
    symlink arrangement that could otherwise cycle.

    ``$CHARTER_ROOT`` never reaches here — it wins outright in :func:`find_root`, and is
    the escape hatch for anyone who genuinely means the inner plane.
    """
    seen = {marked}
    cur = marked
    while True:
        try:
            outer = enclosing_plane(cur)
        except OSError:
            return cur
        if outer is None or outer in seen:
            return cur
        seen.add(outer)
        cur = outer


def standing_in_nested_plane(start: Path | None = None) -> Path | None:
    """The nested plane the caller is standing in, when :func:`find_root` redirected past
    it — else ``None``.

    Without this the redirect is invisible. Once `find_root` answers with the outer plane,
    ``enclosing_plane(config.ROOT)`` is ``None`` *by construction* (the outer is not
    nested), so every surface that used to name the nesting would fall silent and charter
    would be quietly acting on a plane the operator cannot see it choose. ADR 0013's second
    rule applies to charter's own corrections too.

    Never raises: it is read on the status line's render path.
    """
    try:
        cur = (start or Path.cwd()).resolve()
    except (OSError, RuntimeError):
        return None
    try:
        for d in (cur, *cur.parents):
            if (d / MARKER).is_file():
                inner = _plane_of(d)
                return inner if enclosing_plane(inner) is not None else None
    except OSError:
        return None
    return None


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


def tree_of(plane: Path, start: Path | None = None) -> Path | None:
    """The linked worktree of *plane*'s repository that *start* stands in — else ``None``.

    The inverse question to :func:`_plane_of`, and the reason both exist: **a plane and a
    working tree are not the same object, and different operations follow different ones.**

    * The PLANE is identity and machine-local state — personas as a roster, the vault, the
      MCP approval record, memory, workspaces. A worktree is a view of the plane's repo, not
      a second plane, so those must not fork per worktree; that is `_plane_of`'s redirect and
      it is right.
    * The TREE is *committed content on a branch*. An operation that reads tracked files and
      writes tracked files belongs to the checkout it was invoked in, because the artifact it
      produces belongs to the commit, and the commit belongs to the tree.

    `charter persona sync-agents` is the second kind, and before this it took the first
    answer: run from a worktree, it wrote ``.claude/agents/`` into the main clone — a tree
    the caller does not own and, under parallel work, may not even know about (#678). Every
    worker here is told never to run git in another worktree; a command that edits tracked
    files in the shared clone defeats that rule from underneath, and silently, because the
    write lands where nobody is looking.

    ``None`` — not the plane — when *start* is not inside such a worktree, so a caller has to
    say what it wants for the ordinary case rather than inheriting an answer.

    **Narrow on purpose.** The match is on the worktree's own main tree BEING the plane, not
    on "is a worktree of something": a repo clone under ``workspaces/<ws>/<repo>`` resolves
    its plane by walking up, and its worktrees are views of that clone, which have nothing to
    do with the plane's generated files.

    Never raises: it sits on a command path, and :func:`main_worktree_of` is already pure
    path arithmetic.
    """
    try:
        cur = (Path(start) if start is not None else Path.cwd()).resolve()
        target = Path(plane).resolve()
    except (OSError, RuntimeError):
        return None
    for d in (cur, *cur.parents):
        if main_worktree_of(d) == target:
            return d
    return None


def nested_plane_in(plane: Path, start: Path | None = None) -> Path | None:
    """The plane nested inside *plane*'s ``workspaces/`` that *start* stands in — else
    ``None``.

    :func:`tree_of`'s sibling, and deliberately a **second route rather than a widening of
    it** (#809). Both answer one question — *is the caller standing somewhere other than the
    tree this command is about to commit?* — but the two arrangements are told apart by
    different evidence, and neither detector can see the other's:

    * a linked WORKTREE is a view of the plane's own repo, marked by the ``.git`` **file**
      git writes into it. That is `tree_of`, pure path arithmetic.
    * a nested PLANE is a *different repository* that happens to carry a tracked
      ``charter.toml``, sitting where `charter clone` puts clones. Its ``.git`` is a real
      directory, so `tree_of` correctly answers ``None`` for it — and must keep doing so.
      Teaching `tree_of` this case would mean teaching it "am I inside any other plane",
      which is :func:`enclosing_plane`'s question, answered on purpose in a different way.

    **Root-relative, exactly like `tree_of`, and that is what makes ``$CHARTER_ROOT`` work.**
    The question is not "is this plane nested" — it is "is the caller standing in a plane
    nested inside *the one being committed*". With ``$CHARTER_ROOT=<the clone>`` the plane
    being committed IS the one they are standing in, the first branch below returns ``None``,
    and the operator gets what they explicitly asked for. A detector keyed on
    :func:`standing_in_nested_plane` instead would refuse there too — and the refusal that
    uses this prints that very override as its remedy, so the advice would be a lie.

    **The chain is walked, not shortcut through :func:`_outermost`.** ``_outermost(inner) ==
    plane`` looks equivalent and is not: in a plane inside a plane inside a plane, under
    ``$CHARTER_ROOT=<the middle one>``, the outermost is not the tree being committed, yet
    standing in the leaf still commits a tree the caller is not in. Membership of the chain
    is the honest test.

    The answer is the INNERMOST plane the caller stands in, never an intermediate one: it is
    printed as "run git here", and that has to be the tree actually holding their work.

    **No ``seen`` set, and no ``inner == target`` short-circuit, unlike `_outermost`.** Both
    were written here first and the deletion sweep charged them as unreachable, which they
    are: :func:`enclosing_plane` answers an element of ``here.resolve().parents``, so every
    hop is a strictly SHORTER resolved path. The chain therefore cannot revisit anything —
    ``outer in seen`` is never true and the loop always terminates at ``None`` — and it can
    never arrive back at ``inner``, so a caller standing in the very plane being committed
    (the ``$CHARTER_ROOT`` hatch) falls out of the loop returning ``None`` without needing to
    be checked for. `_outermost`'s copy of the guard is older, is charged to nobody, and is
    left alone; this note is here so the next reader does not "restore" a set that provably
    never holds a second element.

    Never raises: it sits on a command path, beside `tree_of`, which makes the same promise.
    Each of the three catches below is pinned by a test that makes the call under it throw —
    `TestTheDetectorNeverRaises` — because "never raises" asserted only by paths that never
    fail is a promise nothing measured.
    """
    try:
        cur = (Path(start) if start is not None else Path.cwd()).resolve()
        target = Path(plane).resolve()
    except (OSError, RuntimeError):       # cwd deleted (OSError); symlink loop (RuntimeError)
        return None
    try:
        for d in (cur, *cur.parents):
            if (d / MARKER).is_file():
                inner = _plane_of(d)
                break
        else:
            return None                   # no marker above `start` at all
    except OSError:                       # PermissionError on an ancestor mid-walk
        return None
    cur_plane = inner
    while True:
        try:
            outer = enclosing_plane(cur_plane)
        except OSError:
            return None
        if outer is None:
            return None
        if outer == target:
            return inner
        cur_plane = outer


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
