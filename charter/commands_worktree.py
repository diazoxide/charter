"""`charter worktree` — several working trees over one clone, for parallel pieces of a task.

Handlers print and return an exit code. **Reads** (parsing `git worktree list`, checking
dirty/detached/unpushed state) live in :mod:`charter.worktree`; the *mutating* git calls
(`worktree add`/`remove`, `branch -d`/`-D`) are issued directly by the handlers below,
alongside the printing.
"""
from __future__ import annotations

from pathlib import Path

from . import config, util, workspace, worktree
from . import root as _root


def clone_for(ws: str, repo: str) -> Path | None:
    """The tree named *repo* that this workspace can cut worktrees from.

    Only the workspace's own clones. The plane's own root tree used to be checked too, for
    a shape where `workspaces/` held no clones at all and the one repo present was the one
    you were standing in. That shape is gone, and with it the reason: a worktree is always
    cut from a repo the workspace selected by cloning it, never from the plane itself.
    """
    for d in workspace.clones(ws):
        if d.name == repo:
            return d
    return None


def _resolve(args) -> tuple[str, Path | None]:
    ws = workspace.resolve(getattr(args, "workspace", None))
    util.info(f"workspace: {ws}")
    clone = clone_for(ws, args.repo)
    if clone is None:
        util.err(f"'{args.repo}' isn't cloned in workspace '{ws}'.")
        util.info(f"Clone it first: charter clone {args.repo} -w {ws}")
    return ws, clone


def _rel(p: Path) -> str:
    try:
        return str(p.relative_to(config.ROOT))
    except ValueError:
        return str(p)


def cmd_worktree_add(args) -> int:
    ws, clone = _resolve(args)
    if clone is None:
        return 1
    if not workspace.valid_name(args.piece):
        util.err(f"invalid piece name '{args.piece}' "
                 "(letters, digits, '.', '_', '-'; must not start with a dot).")
        return 1

    path = worktree.path_for(ws, args.repo, args.piece)
    if path.exists():
        util.err(f"'{args.piece}' already exists → {_rel(path)}")
        return 1

    base, detached = worktree.head_of(clone)
    if detached:
        util.warn(f"{args.repo} is on a DETACHED HEAD ({base}) — the piece branches off "
                  "that commit, not a branch.")
    if worktree.is_dirty(clone):
        util.warn(f"{args.repo} has uncommitted changes — they stay in the clone and are "
                  "NOT carried into the worktree.")

    if args.branch:
        cmd = ["worktree", "add", str(path), args.branch]
    else:
        if worktree.branch_exists(clone, args.piece):
            util.err(f"branch '{args.piece}' already exists in {args.repo}.")
            util.info(f"Reuse it: charter worktree add {args.repo} {args.piece} "
                      f"--branch {args.piece}   (or pick another piece name)")
            return 1
        cmd = ["worktree", "add", str(path), "-b", args.piece]

    path.parent.mkdir(parents=True, exist_ok=True)
    proc = util.run(["git", "-C", str(clone), *cmd], check=False)
    if proc.returncode != 0:
        util.err(f"git worktree add failed:\n{proc.stderr.strip()}")
        return 1

    util.ok(f"{args.repo} · {args.piece} → {_rel(path)}")
    util.info(f"  base:   {base}{' (detached)' if detached else ''}")
    util.info(f"  branch: {args.branch or args.piece}")

    # `root.find_root` now resolves a plane-less worktree back to its main tree, so this
    # is an invariant assertion rather than the fix — kept because it catches the same
    # failure one step earlier, right beside the `enter:` line the user is about to run,
    # and because an untracked charter.toml is worth knowing about on its own: nobody
    # else on the team has the plane at all.
    if not (path / _root.MARKER).is_file() and (config.ROOT / _root.MARKER).is_file():
        util.warn(f"  {_root.MARKER} is not committed, so it is absent from this worktree. "
                  f"charter resolves the plane from the repo it was cut from, but "
                  f"teammates cloning this repo get no control plane — commit it: "
                  f"git add {_root.MARKER}")

    util.info(f"  enter:  cd {_rel(path)} && claude    "
              "(or hand this path to EnterWorktree)")
    return 0


def cmd_worktree_list(args) -> int:
    ws = workspace.resolve(getattr(args, "workspace", None))
    util.info(f"workspace: {ws}")
    if getattr(args, "repo", None):
        clone = clone_for(ws, args.repo)
        if clone is None:
            util.err(f"'{args.repo}' isn't cloned in workspace '{ws}'.")
            return 1
        targets = [clone]
    else:
        # `repo_trees`, not `clones` — its own docstring calls it "the one list
        # anything asking 'which repos am I on?' should use", and `gl-refresh`
        # already uses it. A plane with no clones yet said "No
        # worktrees" while the status line one line above was drawing them.
        targets = workspace.repo_trees(ws)

    total = 0
    for clone in targets:
        rows = worktree.list_for(clone, ws)
        if not rows:
            continue
        util.info(f"{clone.name}")
        for r in rows:
            if r["prunable"]:
                # The worktree dir is gone (deleted without `git worktree prune`) — git
                # still lists the record, but there's no path left to run `status` on.
                state = "missing"
            else:
                state = "dirty" if worktree.is_dirty(Path(r["path"])) else "clean"
            branch = r["branch"] or f"detached {r['path']}"
            print(f"    {r['piece']:<24} {branch:<28} {state}")
            total += 1
    if not total:
        util.info("No worktrees. Create one: "
                  f"charter worktree add <repo> <piece> -w {ws}")
    return 0


def cmd_worktree_remove(args) -> int:
    ws, clone = _resolve(args)
    if clone is None:
        return 1
    path = worktree.path_for(ws, args.repo, args.piece)

    # A worktree whose directory was deleted without `git worktree prune` still has a
    # record in `git worktree list` (marked `prunable`) — there's no working tree left
    # to check for dirt/unpushed commits, or to lose, so it gets a separate path that
    # skips every tree-safety check below. A piece that never existed at all still
    # errors exactly as before.
    stale = None
    if not path.exists():
        stale = next((r for r in worktree.list_for(clone, ws)
                     if r["piece"] == args.piece and r["prunable"]), None)
        if stale is None:
            util.err(f"no worktree '{args.piece}' for {args.repo} in workspace '{ws}'.")
            util.info(f"See what exists: charter worktree list {args.repo} -w {ws}")
            return 1

    force = getattr(args, "force", False)
    if stale is None and not force:
        # Parallel agents are how work gets orphaned — refuse anything that would lose it.
        if worktree.is_dirty(path):
            util.err(f"'{args.piece}' has uncommitted changes — refusing to remove.")
            util.info("Commit them, or discard with --force.")
            return 1
        ahead = worktree.unpushed(path)
        if ahead is None:
            util.err(f"'{args.piece}' has no upstream, so its commits exist nowhere else "
                     "— refusing to remove.")
            util.info("Push the branch, or discard with --force.")
            return 1
        if ahead:
            util.err(f"'{args.piece}' has {ahead} unpushed commit(s) — refusing to remove.")
            util.info("Push them, or discard with --force.")
            return 1

    if stale is not None:
        # The directory is gone, so there's nothing left on disk to read HEAD from —
        # take the branch git itself still has on file for the stale registration.
        branch, detached = stale["branch"], stale["detached"]
    else:
        # Capture the worktree's actual branch before it disappears: with `--branch`, the
        # piece name and the branch name differ (e.g. piece "slice" on branch "taken"), and
        # `--delete-branch` must drop the branch the worktree was really on, not assume the
        # piece name names it.
        branch, detached = worktree.head_of(path)

    cmd = ["git", "-C", str(clone), "worktree", "remove"]
    if force:
        cmd.append("--force")
    proc = util.run([*cmd, str(path)], check=False)
    if proc.returncode != 0:
        util.err(f"git worktree remove failed:\n{proc.stderr.strip()}")
        return 1
    if stale is not None:
        util.ok(f"Cleared stale registration for {args.repo} · {args.piece} "
                "(its worktree directory was already gone).")
    else:
        util.ok(f"Removed {args.repo} · {args.piece}")

    if getattr(args, "delete_branch", False):
        if detached:
            util.warn("Worktree was on a detached HEAD — no branch to delete.")
        else:
            flag = "-D" if force else "-d"
            d = util.run(["git", "-C", str(clone), "branch", flag, branch], check=False)
            if d.returncode == 0:
                util.ok(f"Deleted branch {branch}")
            else:
                util.warn(f"Worktree removed, but the branch was kept:\n{d.stderr.strip()}")
    return 0
