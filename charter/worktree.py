"""Worktrees of a workspace's clone — several working trees over one object store.

**Git is the only registry.** Nothing here writes state: every read shells out to
``git worktree list --porcelain`` and parses it, so a worktree created by hand with plain
git is visible to ``charter``, and one removed by hand cannot leave ``charter`` reporting a stale
entry. The alternative — recording worktrees in workspace state — would introduce a marker
that can disagree with reality, a failure mode this repo has already been bitten by.

Worktrees live at ``workspaces/<ws>/.worktrees/<repo>/<piece>``: OUTSIDE every clone, so
nx/jest/maven never recurse into them and ``mvn clean`` / ``git clean -xfd`` inside the
clone cannot destroy live work. ``workspaces/`` is gitignored, so none of it can reach the
control plane's history.

That path assumes the clone and the control plane are different directories. In an
a plane whose clone was its own root they were the same one, and "outside every clone"
then required leaving
the plane too — so there the root moves to a sibling of the repo (``config.WORKTREES_ROOT``,
overridable via ``[plane] worktrees``) and the layout below it is unchanged:
``<root>/<ws>/<repo>/<piece>``. Same rule, followed to where it leads.
"""
from __future__ import annotations

from pathlib import Path

from . import config, util, workspace

#: Directory under a workspace holding every clone's worktrees (the in-plane layout).
DIR_NAME = ".worktrees"


def root(ws: str) -> Path:
    """This workspace's worktree root — relocated by config, or in-plane by default."""
    ext = config.WORKTREES_ROOT
    return (ext / ws) if ext is not None else (workspace.workspace_dir(ws) / DIR_NAME)


def locate(path: Path) -> tuple[str, str, str] | None:
    """``(workspace, repo, piece)`` when *path* is inside a worktree, else ``None``.

    Path arithmetic, not git: the layout is ``<root>/<ws>/<repo>/<piece>`` in both forms,
    and this is called on every status-line render — see :func:`dirs_for` on why nothing
    here may fork a subprocess.

    Both roots are tried because both can be live at once: a plane that has just declared
    ``[plane] worktrees`` still has yesterday's worktrees in ``workspaces/``, and the
    status line should keep pointing at whichever one you are actually standing in.
    """
    try:
        here = Path(path).resolve()
    except (OSError, RuntimeError):
        return None

    candidates = []
    if config.WORKTREES_ROOT is not None:
        candidates.append((config.WORKTREES_ROOT, ()))
    # In-plane: `workspaces/<ws>/.worktrees/<repo>/<piece>` — one extra component, and it
    # sits AFTER the workspace name, so it is matched positionally rather than stripped.
    candidates.append((config.WORKSPACES_DIR, (DIR_NAME,)))

    for base, infix in candidates:
        try:
            parts = here.relative_to(Path(base).resolve()).parts
        except (ValueError, OSError, RuntimeError):
            continue
        need = 3 + len(infix)
        if len(parts) < need:
            continue
        ws, rest = parts[0], parts[1:]
        if infix and tuple(rest[: len(infix)]) != infix:
            continue
        rest = rest[len(infix):]
        return (ws, rest[0], rest[1])
    return None


def path_for(ws: str, repo: str, piece: str) -> Path:
    return root(ws) / repo / piece


def _git(path: Path, *args: str):
    """Run git without raising, so callers branch on the return code."""
    return util.run(["git", "-C", str(path), *args], check=False)


def parse_porcelain(text: str) -> list[dict]:
    """Parse ``git worktree list --porcelain`` into one dict per worktree.

    A worktree whose directory was deleted without ``git worktree prune`` still gets a
    record here, marked with an extra ``prunable`` line — bare, or with a reason (e.g.
    ``prunable gitdir file points to non-existent location``). ``row["prunable"]`` carries
    that: ``False`` when absent, the reason string when given, ``True`` when bare. Callers
    MUST check it before treating ``row["path"]`` as a real, existing directory.
    """
    out: list[dict] = []
    cur: dict | None = None
    for line in text.splitlines():
        if not line.strip():
            if cur:
                out.append(cur)
                cur = None
            continue
        key, _, val = line.partition(" ")
        if key == "worktree":
            cur = {"path": val, "branch": None, "detached": False, "prunable": False}
        elif cur is None:
            continue
        elif key == "branch":
            # Strip only the `refs/heads/` prefix — branch names legitimately contain
            # slashes (e.g. `feature/spi-schema`), so rsplit("/", 1) would silently
            # drop everything before the last slash.
            cur["branch"] = val.removeprefix("refs/heads/")
        elif key == "detached":
            cur["detached"] = True
        elif key == "prunable":
            cur["prunable"] = val or True
    if cur:
        out.append(cur)
    return out


def list_for(clone: Path, ws: str) -> list[dict]:
    """The worktrees of *clone* that live under this workspace's ``.worktrees/`` root.

    Anything elsewhere (including the clone itself, which git also reports) is skipped, so
    a worktree someone made in another location is not mistaken for ours.
    """
    proc = _git(clone, "worktree", "list", "--porcelain")
    if proc.returncode != 0:
        return []
    base = root(ws)
    out = []
    for w in parse_porcelain(proc.stdout):
        p = Path(w["path"])
        try:
            p.resolve().relative_to(base.resolve())
        except (ValueError, OSError):
            continue
        w["piece"] = p.name
        out.append(w)
    return out


def head_of(clone: Path) -> tuple[str, bool]:
    """``(label, detached)`` for the clone's current HEAD — the base a new piece gets."""
    b = _git(clone, "branch", "--show-current").stdout.strip()
    if b:
        return b, False
    return _git(clone, "rev-parse", "--short", "HEAD").stdout.strip(), True


def is_dirty(path: Path) -> bool:
    return bool(_git(path, "status", "--porcelain").stdout.strip())


def unpushed(path: Path) -> int | None:
    """Commits not on the upstream. ``None`` when there is NO upstream — treated by
    callers as "unpushed", the conservative reading: work that exists nowhere else."""
    if _git(path, "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}").returncode != 0:
        return None
    out = _git(path, "rev-list", "--count", "@{u}..HEAD").stdout.strip()
    try:
        return int(out)
    except ValueError:
        return None


def branch_exists(clone: Path, name: str) -> bool:
    return _git(clone, "rev-parse", "--verify", "--quiet",
                f"refs/heads/{name}").returncode == 0


def dirs_for(ws: str, repo: str) -> list[Path]:
    """Worktree directories for *repo*, most recently touched first.

    Filesystem-only — **no subprocess** — because the status line renders on every turn and
    a `git worktree list` per clone would be paid over and over. This is not a second
    registry: ``git worktree add`` creates these directories and ``git worktree remove``
    deletes them, so listing them IS reading git's own output. Commands that act on state
    still go through git (see :func:`list_for`), where exactness matters.
    """
    base = root(ws) / repo
    try:
        entries = [d for d in base.iterdir() if d.is_dir()]
    except OSError:
        return []

    def mtime(p: Path) -> float:
        try:
            return p.stat().st_mtime
        except OSError:
            return 0.0

    return sorted(entries, key=mtime, reverse=True)
