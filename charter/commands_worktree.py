"""`charter worktree` — several working trees over one clone, for parallel pieces of a task.

Handlers print and return an exit code. **Reads** (parsing `git worktree list`, checking
dirty/detached/unpushed state) live in :mod:`charter.worktree`; the *mutating* git calls
(`worktree add`/`remove`, `branch -d`/`-D`) are issued directly by the handlers below,
alongside the printing.
"""
from __future__ import annotations

from pathlib import Path

from . import config, pieces, tui, util, workspace, worktree
from . import root as _root

#: Exit code for "this piece is already claimed", distinct from the generic 1 every other
#: failure returns. A worker that loses a race takes the next unclaimed name from its plan,
#: and it must not have to parse English to know that is what happened — nor mistake an
#: invalid piece name for a lost race.
CLAIM_TAKEN = 2


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
        # Before the clone tip, because when this plane is nested the tip is the wrong
        # answer: the clone usually DOES exist, in the outer plane, and following the
        # advice makes a second one in a plane nobody chose. Observed on charter's own
        # dogfooding layout (#200) — `charter worktree add charter <branch>` run from
        # inside the charter clone reported the repo missing while standing in it.
        #
        # The tip still prints. Not-cloned remains possible in a nested plane, and ADR
        # 0009 is that charter classifies what it can see rather than picking one cause
        # and hiding the other.
        nested = util.nested_plane_note()
        if nested:
            util.info(nested)
        util.info(f"Clone it first: charter clone {args.repo} -w {ws}")
    return ws, clone


def _rel(p: Path) -> str:
    try:
        return str(p.relative_to(config.ROOT))
    except ValueError:
        return str(p)


def _claim_holder(ws: str, clone: Path, path: Path, branch: str | None) -> str | None:
    """Who already holds this claim, or ``None`` if it is free.

    Two shapes of the same collision. The piece's directory existing is the obvious one.
    The branch being checked out by another live worktree is the same thing under a
    different piece name — git refuses either way, since a branch can be checked out in
    exactly one tree.

    A branch that merely *exists* is deliberately not a holder: nobody is working in it, so
    reporting it as claimed would send workers past names that were free.
    """
    if path.exists():
        return f"a worktree at {_rel(path)}"
    if branch:
        for r in worktree.list_for(clone, ws):
            if r["branch"] == branch and not r["prunable"]:
                return f"piece '{r['piece']}'"
    return None


def _refuse_taken(piece: str, held_by: str) -> int:
    util.err(f"'{piece}' is already claimed — held by {held_by}.")
    util.info("Take the next unclaimed piece from the plan, or work in the existing worktree.")
    return CLAIM_TAKEN


def cmd_worktree_add(args) -> int:
    ws, clone = _resolve(args)
    if clone is None:
        return 1
    if not workspace.valid_name(args.piece):
        util.err(f"invalid piece name '{args.piece}' "
                 "(letters, digits, '.', '_', '-'; must not start with a dot).")
        return 1

    path = worktree.path_for(ws, args.repo, args.piece)
    branch = args.branch or args.piece
    held = _claim_holder(ws, clone, path, branch)
    if held:
        return _refuse_taken(args.piece, held)

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
        # The check above is not the mutex — git is. A worker that won between that check
        # and git's own lock leaves this failure indistinguishable from a broken repo
        # unless we look again, so look: if the path or the branch is now held, the cause
        # was established by reading reality rather than by parsing git's English, which
        # is the only kind of cause charter is allowed to name (ADR 0009).
        held = _claim_holder(ws, clone, path, branch)
        if held:
            return _refuse_taken(args.piece, held)
        util.err(f"git worktree add failed:\n{proc.stderr.strip()}")
        return 1


    # The worktree is the claim; this only records who took it, because git will not know
    # that until a commit lands (ADR 0011). Best-effort by construction — a claim that
    # git granted must not be undone by a log that could not be written.
    pieces.record(ws, "claimed", args.repo, args.piece)

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


def _declare(event: str, reason: str | None = None) -> int:
    """Record a declaration for the piece the caller is standing in.

    Neither verb takes a piece argument, and that absence is the design: a worker naming its
    own piece is a worker that can name someone else's. `worktree.locate` answers it from
    the working directory — path arithmetic, no subprocess, and the same triple the rest of
    charter identifies a piece by.
    """
    here = worktree.locate(Path.cwd())
    if here is None:
        util.err("Not inside a worktree — `done` and `abandon` declare the piece you are "
                 "standing in, so there is nothing here to declare.")
        util.info("cd into the piece first; `charter worktree list` shows where they are.")
        return 1
    ws, repo, piece = here

    previous = pieces.declaration_for(ws, repo, piece)
    if previous:
        util.warn(f"'{piece}' was already declared {previous['event']} — recording "
                  f"{event} over it (the earlier one stays in the log).")

    pieces.record(ws, event, repo, piece, reason=reason)
    util.ok(f"{repo} · {piece} — {event}" + (f": {reason}" if reason else ""))
    return 0


def cmd_worktree_done(args) -> int:
    """Declare the piece you are in finished."""
    return _declare("done")


def cmd_worktree_abandon(args) -> int:
    """Declare the piece you are in given up, with a reason.

    The reason is required because it is the most useful thing an abandoning worker
    produces — the next worker reads it instead of re-deriving why this stopped.
    """
    reason = (getattr(args, "reason", None) or "").strip()
    if not reason:
        util.err("abandon needs a reason — it is what whoever picks this up reads first.")
        return 1
    return _declare("abandoned", reason)


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

    # Rows come from git and only from git. The record is joined onto what git found, to
    # put a name on it — it is never asked which worktrees exist (ADR 0011).
    claims = pieces.claims(ws)
    declared = pieces.declarations(ws)

    total = 0
    for clone in targets:
        rows = worktree.list_for(clone, ws)
        if not rows:
            continue
        util.info(f"{clone.name}")
        # `{branch:<28}` was the sharpest of the four constants and the only one wrong on
        # ORDINARY input: a branch name past 28 characters is a Tuesday, and `:<n` pads a
        # short value while doing nothing at all to a long one — it PUSHES, so that row's
        # state, claimant and outcome land where no other row's do and the table stops
        # being a table. `:<24` on a piece and `:<16` on a claimant are guesses about
        # somebody else's names for the same reason, and `str.format` counts characters
        # where a terminal lays out cells, so a CJK piece name well inside the constant
        # still shifted its row by one column per glyph (#508, #592, #600).
        #
        # Sized per clone rather than across the whole listing, and that is the same
        # trade `_status_for_workspace` records (#597): `is_dirty` is a `git status` per
        # worktree, so sizing every clone's rows together would make an operator wait for
        # every repo's git before the first line appeared. A clone's own worktrees are one
        # table under one heading, its rows are already materialised by `list_for`, and
        # the heading above them has already told the reader where the wait is going.
        body = []
        for r in rows:
            if r["prunable"]:
                # The worktree dir is gone (deleted without `git worktree prune`) — git
                # still lists the record, but there's no path left to run `status` on.
                state = "missing"
            else:
                state = "dirty" if worktree.is_dirty(Path(r["path"])) else "clean"
            branch = r["branch"] or f"detached {r['path']}"
            who = pieces.claimant(claims.get((clone.name, r["piece"])))
            said = pieces.outcome(declared.get((clone.name, r["piece"])))
            if not said:
                quiet = pieces.silence(ws, clone.name, r["piece"])
                # An age, never a verdict. Whether `silent 3d` is a problem is the reader's
                # call — charter has not verified that the worker is gone (ADR 0009).
                said = f"silent {quiet}" if quiet else ""
            body.append((r["piece"], branch, state, who, said))
        # Four widths, not five: the outcome has nothing to its right, so padding it would
        # buy trailing space `_finish` strips anyway — `_STATS_HEADS`' own rule.
        widths = [tui.column("", [row[i] for row in body]) for i in range(4)]
        for row in body:
            cells = "".join(tui.pad(c, w) for c, w in zip(row, widths))
            print(f"    {cells}{row[4]}".rstrip())
            total += 1
    if not total:
        util.info("No worktrees. Create one: "
                  f"charter worktree add <repo> <piece> -w {ws}")
    return 0


def cmd_worktree_history(args) -> int:
    """What happened to this workspace's pieces — including pieces that no longer exist.

    A separate command rather than a flag on ``list``, and deliberately so. ``list`` answers
    *what is running here*, from git; this answers *what happened here*, from the record.
    ADR 0010's rule is that where two sources answer a question you name which question each
    answers — folding this into ``list --history`` would put both behind one name and invite
    exactly the confusion that ADR describes the expensive way.

    So this reads only the log and never git: a piece whose worktree is long gone is the
    main reason to run it.
    """
    ws = workspace.resolve(getattr(args, "workspace", None))
    util.info(f"workspace: {ws}")

    repo = getattr(args, "repo", None)
    piece = getattr(args, "piece", None)
    rows = [e for e in pieces.events(ws)
            if (not repo or e.get("repo") == repo) and (not piece or e.get("piece") == piece)]

    if not rows:
        scope = f" for {repo}/{piece}" if piece else (f" for {repo}" if repo else "")
        util.info(f"No piece history recorded{scope}. Claims are recorded by "
                  "`charter worktree add`.")
        return 0

    # Sized from the values, measured in CELLS (#592). `{ts:<22}` is the instance of
    # #508's constant that is wrong on charter's OWN output rather than on an unusual
    # name: `pieces.record` writes `datetime.isoformat(timespec="seconds")`, which is 25
    # characters with an offset on it, so EVERY row of this table pushed its repo, piece
    # and event three columns right of the next one — the table has never once lined up.
    # A repo and a piece name are somebody else's directory names, so `:<16`/`:<24` were
    # guesses about content too, and `str.format` counts characters where a terminal lays
    # out cells.
    #
    # A second thing the kit brings that the format string could not: these values come
    # out of a COMMITTED log, so `tui.pad` sanitising them (a newline becomes a space) is
    # what stops one event's field shearing every column below it.
    #
    # `ts`, `repo` and `event` need their fallbacks and `piece` does not, which looks
    # inconsistent and is not: `pieces.events` keeps a line only `if obj.get("piece")`, so
    # a row without one never reaches here — the deletion sweep reported the fourth
    # fallback as equivalent and it was right. Subscripted rather than `.get`, so if that
    # filter ever goes the failure is a loud `KeyError` naming the field rather than
    # `tui.pad(None, w)` three frames down. The other three are reachable the moment a
    # process is killed mid-append, which is what `events()` tolerates by design.
    body = [(e.get("ts", ""), e.get("repo", ""), e["piece"], e.get("event", ""),
             pieces.claimant(e) + (f" — {e['reason']}" if e.get("reason") else ""))
            for e in rows]
    widths = [tui.column("", [r[i] for r in body]) for i in range(4)]
    for row in body:
        cells = "".join(tui.pad(c, w) for c, w in zip(row, widths))
        print(f"    {cells}{row[4]}".rstrip())
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
        # What would actually cease to exist: commits on this branch and on no other ref.
        # The old test was "has no upstream", which refused over a just-created piece that
        # had nothing to lose — and a guard that fires on the harmless common case is how
        # `--force` becomes a habit (#104).
        alone = worktree.unique_commits(path)
        if alone is None:
            util.err(f"could not determine whether '{args.piece}' holds unique commits "
                     "— refusing to remove.")
            util.info("Check the worktree by hand, or discard with --force.")
            return 1
        if alone:
            util.err(f"'{args.piece}' has {alone} commit(s) that exist nowhere else "
                     "— refusing to remove.")
            util.info("Push the branch or merge it, or discard with --force.")
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
