"""``charter workspace`` commands: create/list/use/current/remove.

Workspaces are isolated per-task environments of repo clones under
``workspaces/<workspace>/``. See :mod:`charter.workspace` for how the active one is
resolved. Agents must operate within a single workspace and never mix them.
"""

from __future__ import annotations

import datetime
import json
import os
import shutil
import subprocess
import sys
import time
from types import SimpleNamespace

from . import change, config, contain, gitpolicy, planegit, tui, util, workspace, worktree
from .commands import (_cred_flag, _git, _origin_https, cmd_clone, commit_memory_reactive,
                       commit_push)


def cmd_workspace_list(args) -> int:
    """Every workspace this plane offers, with the active one marked — #745.

    **The always-present workspace is folded in whether or not its directory exists**,
    which is `frame/switch.workspaces`' rule asked at the other surface rather than a
    second one invented here. `workspace.list_workspaces` reads DIRECTORIES, and
    `config.DEFAULT_WORKSPACE` is the rung `workspace.resolve` terminates on — so a plane
    where nobody has selected anything resolves to a name this listing did not contain,
    and the table printed a "you are here" inset with the mark on no row at all:

        Active workspace: default  (via default (nothing selected))

          WORKSPACE  MODE   CLONES  REPOS
          alpha      local  0       —
          beta       local  0       —

    That is #745 read from the CLI side. The report came from the frame — the `F2`
    workspace picker offers `default`, switching to it succeeds, and `charter workspace
    list` then does not list where the frame is standing — but the picker is not what is
    wrong: `charter workspace use default` accepts the same name on a plane that has never
    made one, and says so in as many words, because `workspace.ensure` creates it on
    demand. The disagreement was between the LISTING and everything else.

    **Not created here, and not by `charter init` either**, which is the other half of the
    report and the half this declines. `[workspace] default` names it, so `init` would be
    baking a directory for a value the operator may change in the charter.toml `init`
    itself just wrote — and every route that puts something IN the workspace (`clone`,
    `workspace use`, `workspace create`) already calls `ensure`. A row costs nothing and
    cannot go stale; a directory named after last week's config can.

    A row for it is honest about what it is: `local`, no clones, `—` for repos, which is
    exactly what it holds.
    """
    active = workspace.resolve()
    names = workspace.list_workspaces()
    fresh = not names
    if config.DEFAULT_WORKSPACE not in names:
        names = sorted(names + [config.DEFAULT_WORKSPACE])
    if fresh:
        # Still said, because "there is one row and it is the fallback" and "somebody has
        # made a workspace" are different states and the table cannot tell them apart.
        util.info(f"No workspaces yet — '{active}' is the one every plane starts on. "
                  "Create one: charter workspace create <name>")
    print(f"Active workspace: {active}  (via {workspace.source()})\n")
    live = workspace.live_workspaces()
    # `{:<22}` was a guess about a name the operator minted, and `{:<7}` twice over about
    # values charter mints — `:<n` pads a short value and PUSHES a long one, so a workspace
    # named past 22 sent its mode, clone count and repo list somewhere no other row's
    # landed. `str.format` counts characters as well, which is the half no constant can
    # fix: the stale marker appended here is ``" ⚠"``, two characters and three cells, so
    # every flagged row was already drawn one column right of the rest (#508, #592, #600).
    #
    # The numeric column is measured from its values too, for the reason `tui.column`'s
    # own docstring gives — sizing every column this way is what makes the table
    # unpushable by ANY value, rather than by none of the values somebody thought of.
    #
    # Nothing here costs a subprocess: `clones` is a directory listing and `needs_reinit`
    # reads a file, so unlike `status` (#597) there is no reason to draw before measuring.
    heads = ("WORKSPACE", "MODE", "CLONES")
    body = []
    for n in names:
        cl = workspace.clones(n)
        stale = " ⚠" if workspace.needs_reinit(n) else ""
        body.append(("* " if n == active else "  ", n + stale,
                     "live" if n in live else "local", str(len(cl)),
                     ", ".join(d.name for d in cl) if cl else "—"))
    widths = [tui.column(h, [row[i + 1] for row in body]) for i, h in enumerate(heads)]

    def line(mark, cells, last: str) -> str:
        return (mark + "".join(tui.pad(c, w) for c, w in zip(cells, widths))
                + last).rstrip()

    # The mark is an INSET, not a column: it is charter's own two-cell "you are here"
    # marker rather than a value measured from anything, which is how `frame.slots` and
    # `persona list` both spell theirs. One padder for the header and the rows, so the
    # two cannot disagree about a width the way a second format string would.
    print(line("  ", heads, "REPOS"))
    for row in body:
        print(line(row[0], row[1:4], row[4]))
    stale_names = [n for n in names if workspace.needs_reinit(n)]
    if stale_names:
        util.warn(f"⚠ {len(stale_names)} workspace(s) need reinit ({', '.join(stale_names)}) — "
                  f"bring their structure up to date: charter workspace reinit --all")
    return 0


def cmd_workspace_current(args) -> int:
    name = workspace.resolve()
    print(name)
    locked = workspace.is_locked()
    lock_note = ", 🔒 locked for this session" if locked else ", unlocked"
    mode = "LIVE (committed + shared)" if workspace.is_live(name) else "LOCAL (private)"
    util.info(f"{mode} · resolved via {workspace.source()}{lock_note}")
    vision = workspace.read_vision(name)
    if vision:
        util.info(f"Vision: {vision.splitlines()[0].strip()}")
    else:
        util.info(f'No vision set — describe the goal: charter workspace vision "…"')
    return 0


def cmd_workspace_create(args) -> int:
    try:
        wd = workspace.ensure(args.name)
    except ValueError as e:
        util.err(str(e))
        return 1
    workspace.scaffold(args.name)  # memory/ + refs/ + workspace.md charter beside its clones
    vision = getattr(args, "vision", None)
    if vision:
        workspace.set_vision(args.name, vision)
    live = getattr(args, "live", False)
    if live:
        workspace.set_live(args.name, True)

    tree_path = None
    mode = ("LIVE — charter + manifest + memory committed + shared + auto-saved" if live
            else "LOCAL — private (nothing committed); `charter workspace live` to share")
    util.ok(f"Workspace '{args.name}' ready ({mode}) → {wd.relative_to(config.ROOT)}/")
    if vision:
        util.info(f"Vision recorded → {(wd / 'workspace.md').relative_to(config.ROOT)}")
    else:
        util.info('⬢ No vision yet — ask the developer what this workspace is for, then record it: '
                  'charter workspace vision "<the goal>"  (it seeds workspaces/'
                  f'{args.name}/workspace.md, the living charter a fork inherits).')

    if args.use:
        scope = workspace.set_active(args.name, force=getattr(args, "force", False))
        if scope == "locked":
            util.err(_locked_msg(args.name))
            util.info(f"Workspace '{args.name}' was created; start a new session to use it, "
                      f"or re-run with --force.")
            return 2
        util.ok(f"Active workspace set to '{args.name}'{_scope_note(scope)} — 🔒 locked for this session.")
        _warn_env_override(args.name)

    if tree_path is not None:
        # The path is the point: charter cannot cd your shell, so selecting the workspace
        # is an instruction rather than an action — the same shape `worktree add` already
        # has. Being INSIDE this directory is what makes it the active workspace
        # (`workspace.from_path`), so no pointer has to agree with anything.
        rel = tree_path if not str(tree_path).startswith(str(config.ROOT)) else tree_path
        util.ok(f"Working tree → {rel}")
        util.info(f"  enter:  cd {rel} && claude")
        util.info(f"  Being in that directory IS this workspace — no pointer needed.")

    if args.repos:
        return cmd_clone(SimpleNamespace(repos=args.repos, workspace=args.name))
    if not args.use and tree_path is None:
        util.info(f"Select it with: charter workspace use {args.name}  "
                  f"(or --workspace {args.name} per command)")
    return 0


def cmd_workspace_use(args) -> int:
    if not workspace.valid_name(args.name):
        util.err(f"invalid workspace name '{args.name}'")
        return 1

    # A typo used to be a one-way door: the name was only validated for SHAPE, so
    # `use fature-x` created `fature-x`, took the session lock, and the correction then
    # hit `✗ Workspace is 🔒 locked to 'fature-x' for this session`. Creating is now
    # deliberate (`--create`), and an unknown name is a question rather than an action.
    # `default` is always selectable, whether or not its directory exists yet — it is
    # documented as "the always-present workspace used when none is selected", and
    # `ensure` creates it on demand. The unknown-name guard below refused it in a plane
    # that had never made one, which is every fresh plane.
    existing = workspace.list_workspaces()
    if (args.name not in existing and args.name != config.DEFAULT_WORKSPACE
            and not getattr(args, "create", False)):
        import difflib
        close = difflib.get_close_matches(args.name, existing, n=3, cutoff=0.6)
        util.err(f"no workspace named '{args.name}'.")
        if close:
            util.info(f"  Did you mean: {', '.join(close)}?")
        elif existing:
            util.info(f"  Existing: {', '.join(sorted(existing))}")
        util.info(f"  Create it: charter workspace use {args.name} --create")
        return 1

    workspace.ensure(args.name)
    scope = workspace.set_active(args.name, force=getattr(args, "force", False))
    if scope == "locked":
        util.err(_locked_msg(args.name))
        return 2
    verb = "re-locked to" if getattr(args, "force", False) else "set to"
    util.ok(f"Active workspace {verb} '{args.name}'{_scope_note(scope)} — 🔒 locked for this session.")
    _warn_env_override(args.name)
    return 0


def cmd_workspace_unlock(args) -> int:
    """Release this session's workspace lock so a different one can be selected.
    The escape hatch for the mid-session switch guard — use sparingly; a fresh
    session is the clean way to pick another workspace."""
    if workspace.unlock():
        util.ok("Workspace unlocked for this session — `charter workspace use <name>` can switch now.")
    else:
        util.info("No workspace lock was set for this session (nothing to unlock).")
    return 0


def _locked_msg(target: str) -> str:
    locked = workspace.is_locked() or "?"
    return (f"Workspace is 🔒 locked to '{locked}' for this session — switching to '{target}' "
            f"mid-session is disabled (never mix workspaces). Start a new session to pick another, "
            f"or force it: `charter workspace use {target} --force` (or `charter workspace unlock` first).")


def cmd_workspace_remove(args) -> int:
    name = args.name
    wd = workspace.workspace_dir(name)
    if not wd.exists():
        util.err(f"no workspace '{name}'")
        return 1

    risky = _work_at_risk(name)
    if risky and not args.force:
        util.err(
            f"Refusing to remove '{name}' — this would discard work: "
            + "; ".join(risky)
            + ". Push/commit first, or pass --force."
        )
        return 2

    # Open todos are REPORTED, never guarded on — see `_work_at_risk` for why they are not
    # in that list. Said here rather than above the guard so the count describes what is
    # actually about to happen, and only when there is something to say: "0 open todos" on
    # every removal is how a line stops being read at all, including on the removal where
    # it mattered.
    from . import todos
    open_todos = todos.count_open(name)
    if open_todos:
        util.warn(f"Discarding {open_todos} open todo(s) with '{name}' — nothing else holds them.")

    shutil.rmtree(wd)
    util.ok(f"Removed workspace '{name}' and its clones.")
    if workspace.resolve() == name and workspace.source() in ("session", "active-file"):
        # Removing the locked workspace: force past the lock, then re-lock to default.
        workspace.set_active(config.DEFAULT_WORKSPACE, force=True)
        util.info(f"Active workspace reset to '{config.DEFAULT_WORKSPACE}'.")
    return 0


def cmd_workspace_rename(args) -> int:
    """Rename a workspace: move workspaces/<old>/ → workspaces/<new>/ (clones, memory,
    refs, and manifest come along), fix the manifest name + liveness block, repoint
    the active session/terminal pointer + lock so a renamed active workspace stays
    active, and repoint every chat that says it is in the workspace (#795) so none of
    them is orphaned. For a LIVE workspace, commit the tracked move (manifest + memory)
    so the rename propagates to the team."""
    old, new = args.old, args.new
    if not workspace.valid_name(new):
        util.err(f"invalid workspace name '{new}' (use lowercase letters, digits, . _ -)")
        return 1
    if old == new:
        util.err("old and new names are the same — nothing to rename.")
        return 1
    if not workspace.workspace_dir(old).exists():
        util.err(f"no workspace '{old}'")
        return 1
    if workspace.workspace_dir(new).exists():
        util.err(f"workspace '{new}' already exists — pick another name or remove it first.")
        return 1

    was_live = workspace.is_live(old)
    # Capture the tracked metadata paths BEFORE the move — after it, the old dir is gone
    # and git needs the exact old paths to stage their deletion (the rename half git detects).
    tracked_old: list[str] = []
    if was_live:
        r = _git(["ls-files", "-z", "--", f"workspaces/{old}"], cwd=config.ROOT)
        tracked_old = [p for p in r.stdout.split("\0") if p]

    moved = workspace.rename(old, new)
    util.ok(f"Renamed workspace '{old}' → '{new}' (clones, memory, and manifest moved).")
    if moved:
        # #795: the chats came too. A rename that silently re-labels running conversations
        # is one the operator finds out about from a panel; this is the sibling of the
        # "this session's active workspace followed the rename" line below, for the noun
        # that is a chat's identity rather than a session's work.
        #
        # A COUNT and not the ids. Every id in `moved` reached it through `contain.child`
        # (`state.frame_dir`, which every reader in `rename_workspace` goes through), so a
        # `contain.one_line` here would be a guard nothing could turn red — which this repo
        # deletes rather than ships. `new` is `valid_name`-checked at the top of this
        # function.
        util.info(f"{len(moved)} chat(s) in '{old}' followed the rename → '{new}'.")
    if workspace.resolve() == new and workspace.source() in ("session", "active-file"):
        util.info(f"This session's active workspace followed the rename → '{new}' (still 🔒 locked).")

    if not was_live:
        util.info(f"'{new}' is LOCAL (private) — nothing committed.")
        return 0
    new_rel = _ws_meta_paths(new)
    msg = getattr(args, "message", None) or f"workspace: rename {old} → {new}"
    return commit_push(config.ROOT, ["add", "-A", "--", *tracked_old, *new_rel, ".gitignore"], msg)


def cmd_workspace_default(args) -> int:
    """Nominate the workspace a session lands on when nothing else has decided (#193).

    Mirrors `charter persona default`. The rung sits below the per-session and per-terminal
    pointers and above the built-in `default`, so an explicit choice survives a session
    boundary the way an explicit persona choice does — which matters most on a terminal that
    reports no pane id, where the pointer meant to cover "new session, same terminal" can
    never fire.
    """
    name = getattr(args, "name", None)
    if not name:
        cur = workspace.declared_default()
        if cur:
            util.info(f"Declared default workspace: {cur}")
        else:
            util.info("No declared default — a session with nothing else selected lands on "
                      f"'{config.DEFAULT_WORKSPACE}'. Set one: charter workspace default <ws>")
        return 0
    if getattr(args, "clear", False):
        workspace.clear_declared_default()
        util.ok("Cleared the declared default workspace.")
        return 0
    # The name FIRST, and separately from "does it exist". `workspace_dir(name).exists()`
    # accepted `../../esc` — the directory is really there, it is simply not a workspace —
    # and wrote it into a committed file every future session reads (#442). "A path that
    # exists" was never the question being asked (the same note `persona.py` keeps at
    # #337). It is asked here as well as in `set_declared_default` because a refusal a user
    # sees has to be a sentence rather than a traceback.
    if not workspace.valid_name(name):
        util.err(f"'{name}' is not a workspace name (letters, digits, '.', '_', '-'; "
                 "must not start with a dot). This file is committed and read on every "
                 "session start, so a value that is not a name is refused here.")
        return 1
    if not workspace.workspace_dir(name).exists():
        util.err(f"no workspace '{name}' (create it: charter workspace create {name})")
        return 1
    workspace.set_declared_default(name)
    util.ok(f"Default workspace set to '{name}' — sessions land here when nothing else "
            f"has decided.")
    util.info("  Committed, so it travels with the plane. It is read LAST, so an explicit "
              "`--workspace`, $CHARTER_WORKSPACE, the tree you are standing in, or a "
              "session/terminal selection all still win.")
    return 0


def cmd_workspace_optimize(args) -> int:
    """Optimize a workspace's memory (one, or ``--all``) — the workspace half of what
    ``charter persona optimize`` has always done for personas.

    The engine was never persona-specific: ``curate.report``/``apply_safe`` take any memory
    directory. Only the wiring was, which left the fastest-growing store in the plane — the
    workspace journal, appended every session and nudged by the memory-cadence hook — with
    no dedupe, no index repair and no stale review.

    Same two tiers as the persona command, deliberately: ``--apply`` performs only the safe
    and reversible ops (collapse exact duplicates, repair the index), and proposals (near-dup
    merges, stale archives) are printed for a human to decide. A read-only run names what
    ``--apply`` would do, because a read-only run that quietly rewrote an index is how
    "read-only" stops meaning anything.
    """
    from . import curate
    names = [args.name] if getattr(args, "name", None) else workspace.list_workspaces()
    if getattr(args, "name", None) and not workspace.workspace_dir(args.name).exists():
        util.err(f"no workspace '{args.name}'")
        return 1
    if not names:
        util.info("No workspaces to optimize.")
        return 0

    apply = getattr(args, "apply", False)
    stale_days = getattr(args, "stale_days", 90)
    total_actions = 0
    for n in names:
        mdir = workspace.memory_dir(n)
        if not mdir.exists():
            continue
        rep = curate.report(mdir, stale_days=stale_days)
        if rep["total"] == 0:
            continue
        print(f"\n◆ {n}  ({rep['total']} memories · {len(rep['exact_dups'])} exact-dup "
              f"group(s) · {len(rep['near_dups'])} near-dup pair(s) · {len(rep['stale'])} stale)")
        if apply:
            actions = curate.apply_safe(mdir)
            for a in actions:
                util.ok(f"  auto: {a}")
            total_actions += len(actions)
            if actions:
                rel = str(mdir.relative_to(config.ROOT))
                commit_memory_reactive(
                    [rel], f"workspace({n}): curate — {len(actions)} safe op(s)")
            rep = curate.report(mdir, stale_days=stale_days)
        else:
            pending = curate.pending_auto(rep)
            if pending:
                print("  would auto-apply (re-run with --apply):")
                for a in pending:
                    print(f"    + {a}")
        props = curate.proposals(rep)
        if props:
            print("  proposals (not auto-applied — decide these yourself):")
            for p in props:
                print(f"    ? {p}")
        elif apply:
            util.info("  clean — nothing to propose.")

    if not apply:
        util.info("\nRead-only. Re-run with --apply to auto-apply the safe/reversible ops "
                  "(exact-dup collapse + index repair); proposals always stay manual.")
    elif total_actions == 0:
        util.info("\nNo safe ops to apply — the journal is already tidy.")
    return 0


def _work_at_risk(name: str) -> list[str]:
    """Clones **and worktrees** in the workspace with uncommitted or unpushed work.

    Only that. Open todos are deliberately NOT here, though `remove` reports them: this
    list is what is *unrecoverable*, and a todo is a note about the future, not work that
    ceases to exist. A workspace whose todos were all abandoned is precisely the one worth
    deleting, and making that case demand `--force` would teach the habit of reaching for
    `--force` — which is how a guard stops protecting the commits it exists for.
    """
    out = []
    for d in workspace.clones(name):
        if _git(["status", "--porcelain"], cwd=d).stdout.strip():
            out.append(f"{d.name}: uncommitted changes")
            continue
        up = _git(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"], cwd=d)
        if up.returncode == 0:
            ahead = _git(["rev-list", "--count", "@{u}..HEAD"], cwd=d).stdout.strip()
            if ahead and ahead != "0":
                out.append(f"{d.name}: {ahead} unpushed commit(s)")
    return out + _worktrees_at_risk(name)


def _worktrees_at_risk(name: str) -> list[str]:
    """Worktrees of this workspace holding work that removing it would destroy (#91).

    The loop above cannot see these and never could: it iterates `workspace.clones()`,
    which filters on `is_clone` — a clone's ``.git`` is a directory, a linked worktree's is
    a FILE. That exclusion is deliberate and correct for counting repos, and it is exactly
    what left worktrees unguarded while `shutil.rmtree` took them anyway.

    The **rule** differs from the clone rule above, not just the paths. A worktree is at
    risk when it holds commits reachable from no other ref — the work that would actually
    cease to exist — which `charter worktree remove` uses too, so the two guards refuse on
    identical grounds. Keeping them identical is the point: they answer one question, and a
    workspace that removed what a worktree refused to would be the original bug again.

    Not "has no upstream", which is what this was first written as (#91) and then narrowed
    (#104): a parallel agent's piece has no upstream from the moment it is created, so that
    reading refused over pieces with nothing to lose — and a guard that fires on the
    harmless common case teaches the `--force` habit that stops it protecting anything.

    This fires even when the worktree directory lives outside the workspace — a relocated
    ``[plane] worktrees`` root, which `shutil.rmtree` never touches. That is not an
    oversight: a linked worktree keeps its objects in the CLONE's object store, so removing
    the clone destroys those commits whether or not the directory survives.
    """
    base = worktree.root(name)
    try:
        repos = sorted(d.name for d in base.iterdir() if d.is_dir())
    except OSError:
        return []

    out = []
    for repo in repos:
        for wt in sorted(worktree.dirs_for(name, repo)):
            label = f"{repo}/{wt.name}"
            if worktree.is_dirty(wt):
                out.append(f"{label}: uncommitted changes")
                continue
            alone = worktree.unique_commits(wt)
            if alone is None:
                out.append(f"{label}: could not be checked for unique commits")
            elif alone:
                out.append(f"{label}: {alone} commit(s) that exist nowhere else")
    return out


def _repo_branch(clone) -> str:
    return _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=clone).stdout.strip() or "HEAD"


def _git_user() -> str:
    return _git(["config", "user.name"]).stdout.strip() or os.environ.get("USER", "unknown")


def _restore_blockers(name: str) -> list[str]:
    """Repos whose current branch wouldn't restore for another engineer — uncommitted
    work, unpushed commits, or a branch not on the remote at all. The 'enforce push'
    guard: a manifest branch is only meaningful if it's actually on the remote."""
    out = []
    for d in workspace.clones(name):
        if _git(["status", "--porcelain"], cwd=d).stdout.strip():
            out.append(f"{d.name}: uncommitted changes")
            continue
        up = _git(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"], cwd=d)
        if up.returncode != 0:
            out.append(f"{d.name}: branch '{_repo_branch(d)}' isn't pushed to a remote")
            continue
        ahead = _git(["rev-list", "--count", "@{u}..HEAD"], cwd=d).stdout.strip()
        if ahead and ahead != "0":
            out.append(f"{d.name}: {ahead} unpushed commit(s)")
    return out


def cmd_workspace_live(args) -> int:
    """Toggle a workspace between LIVE (shareable — manifest + memory committed/synced/
    auto-saved) and LOCAL (`--off`: private, nothing committed)."""
    name = args.name
    if not workspace.workspace_dir(name).exists():
        util.err(f"no workspace '{name}' (create it: charter workspace create {name})")
        return 1
    if getattr(args, "off", False):
        rel = _ws_meta_paths(name)
        if rel:  # untrack the committed files (keeps them on disk), then re-ignore
            _git(["rm", "-r", "--cached", "-q", "--", *rel], cwd=config.ROOT)
        workspace.set_live(name, False)
        util.ok(f"Workspace '{name}' is now LOCAL (private). Its manifest + memory are no "
                "longer committed. Finalize the untracking: charter save")
        return 0
    workspace.scaffold(name)
    if not workspace.set_live(name, True):
        util.info(f"Workspace '{name}' is already LIVE.")
    else:
        util.ok(f"Workspace '{name}' is now LIVE — manifest + memory are committed + shared + auto-saved.")
    util.info(f"Record its repos: charter workspace snapshot {name}  ·  share: charter workspace save {name}")
    return 0


def cmd_workspace_snapshot(args) -> int:
    """Capture the workspace's repos + branches into the committed manifest
    (workspaces/<name>/workspace.json). Enforce-push: refuse if a repo has
    uncommitted/unpushed work, so the recorded branch fully captures reality."""
    name = getattr(args, "name", None) or workspace.resolve()
    clones = workspace.clones(name)
    if not clones:
        util.err(f"workspace '{name}' has no repo clones to snapshot.")
        return 1
    blockers = _restore_blockers(name)
    if blockers and not getattr(args, "force", False):
        util.err(f"Refusing to snapshot '{name}' — push repo work first so the branch "
                 "captures the real state:")
        for b in blockers:
            util.err(f"  {b}")
        util.info("Commit + push inside each repo, then retry (or --force to snapshot branches as-is).")
        return 2
    m = workspace.read_manifest(name)
    m["name"] = name
    if getattr(args, "description", None):
        m["description"] = args.description
    m.setdefault("description", "")
    m["repos"] = [{"name": d.name, "branch": _repo_branch(d)} for d in clones]
    m["updated_at"] = datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")
    m["updated_by"] = _git_user()
    workspace.write_manifest(name, m)
    util.ok(f"Snapshot '{name}' → workspaces/{name}/workspace.json  ({len(m['repos'])} repo(s)):")
    for r in m["repos"]:
        util.info(f"  {r['name']} @ {r['branch']}")
    util.info("Share it with the team: charter workspace save   (commits + pushes manifest + memory).")
    return 0


def cmd_workspace_restore(args) -> int:
    """Rebuild a workspace from its committed manifest — clone each repo and check out
    its recorded branch (all now, or --on-demand). Partial access is normal: repos you
    can't reach are skipped."""
    name = args.name
    m = workspace.read_manifest(name)
    repos = m.get("repos") or []
    if not repos:
        util.err(f"no manifest for '{name}' (workspaces/{name}/workspace.json). "
                 "Pull fresh metadata first: charter workspace sync")
        return 1
    workspace.ensure(name)
    workspace.scaffold(name)  # ensure the local structure (refs/, marker) + stamp version
    util.info(f"Restoring '{name}' — {len(repos)} repo(s) from manifest "
              f"(updated {m.get('updated_at', '?')} by {m.get('updated_by', '?')}).")
    if getattr(args, "on_demand", False):
        for r in repos:
            util.info(f"  on-demand: {r['name']} @ {r['branch']} (clone when you enter it)")
        util.info("Enter the workspace and clone as you go: charter clone <repo> -w " + name)
        return 0
    # #325/#334/#328 all describe this join and each frames it as another's: #325 claims
    # it as the clone-destination half, #334 disclaims it as "the join #325 already
    # names", and #328's bullet assigns it to #334. Three issues, one line, nobody's — so
    # it is fixed HERE, explicitly, rather than left for whichever ticket closes last.
    #
    # `workspace.json` is committed precisely so a teammate can restore someone else's
    # workspace; that is the feature, and it is what makes every field in it untrusted.
    # `is_git_repo` below is an existence check, never a containment one, so without this
    # a name with parent components selected a repository the operator never named — and
    # `git checkout` plus a CREDENTIALED `git pull` then ran inside it (#334, confirmed on
    # 0.47.2 by the target repository's own reflog).
    wd = workspace.workspace_dir(name)
    contained, refused = [], []
    for r in repos:
        d = contain.child(wd, str(r.get("name") or ""))
        if d is None:
            refused.append(r)
        else:
            contained.append((r, d))
    for r in refused:
        util.err(f"  {str(r.get('name'))!r}: refused — {contain.refusal(str(r.get('name') or ''))}. "
                 f"Fix workspaces/{name}/workspace.json.")
    # Per-entry, never per-file: a manifest is shared and committed, so rejecting the whole
    # thing would let one bad row deny the other eight repos to the whole team — an attack
    # in its own right. `restore` already skips repos it cannot reach and says so.
    repos = [r for r, _ in contained]
    if not repos:
        util.err(f"No usable repos in '{name}'s manifest.")
        return 1
    missing = [r["name"] for r, d in contained if not workspace.is_git_repo(d)]
    if missing:
        cmd_clone(SimpleNamespace(repos=missing, workspace=name))
    ok = 0
    for r, d in contained:
        if not workspace.is_git_repo(d):
            util.warn(f"  {r['name']}: not cloned (no access?) — skipped.")
            continue
        forge = gitpolicy.forge_for(d)  # THIS clone's own forge — never a hardcoded one.
        if forge is None:
            # Unrecognised host (not a default forge, not declared in charter.toml) —
            # never guess a credential helper for it; skip rather than mis-authenticate.
            util.warn(f"  {r['name']}: origin host isn't a known/declared forge — skipped.")
            continue
        cred = _cred_flag(forge)
        # #334's second half. A branch is a REF, not a path segment — `feature/x` is the
        # convention most teams use — so no name rule applies here and the treatment is
        # argv position instead. Without a guard, a manifest branch beginning with a dash
        # is read by `git checkout` as an option, and `checkout` has options that write:
        # `-b`, `-B`, `--orphan`, `--pathspec-from-file`.
        #
        # A leading dash is the whole check, and `git check-ref-format` is deliberately
        # NOT used: verified against git 2.50.1, `check-ref-format refs/heads/-b` ACCEPTS
        # `-b`, because a leading dash is legal inside a ref. Ref grammar answers a
        # different question than argv safety, and reaching for it here would have cost a
        # subprocess per repo while closing nothing.
        branch = str(r.get("branch") or "")
        if branch.startswith("-"):
            util.err(f"  {r['name']}: refused branch {branch!r} — a branch read from a "
                     f"committed manifest may not begin with '-', which git would read as "
                     f"an option rather than a ref. Fix workspaces/{name}/workspace.json.")
            continue
        # `--` after the ref, so git cannot reinterpret it as a pathspec even if the guard
        # above is ever loosened. (`checkout -- <x>` means the opposite: it forces the
        # PATHSPEC reading, which is why the separator goes after the branch, not before.)
        if _git(["checkout", branch, "--"], cwd=d).returncode == 0:
            _git([*cred, "pull", "--ff-only"], cwd=d)  # latest of the recorded branch
            util.ok(f"  {r['name']} @ {r['branch']}")
            ok += 1
        else:
            util.warn(f"  {r['name']}: couldn't checkout '{r['branch']}'.")
    util.ok(f"Restored {ok}/{len(repos)} repo(s) into '{name}'.")
    return 0


def cmd_workspace_sync(args) -> int:
    """Pull the control plane so you get every engineer's fresh workspace manifests + memory
    BEFORE working — the control plane is the shared metadata store."""
    root = config.ROOT
    https = _origin_https(root)
    if not https:
        util.warn("origin isn't on a forge charter knows (gitlab.com/github.com/…) — "
                  "pull manually.")
        return 0
    branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=root).stdout.strip()
    p = _git([*_cred_flag(gitpolicy.forge_for(root)), "pull", "--ff-only", https, branch], cwd=root)
    if p.returncode == 0:
        util.ok("Synced — fresh workspace manifests + memory pulled from the control plane.")
        return 0
    util.warn("Pull wasn't fast-forward (local control-plane changes?). Run `charter save` first, then sync.")
    for ln in (p.stderr or "").splitlines()[-3:]:
        util.warn("  " + ln)
    return 1


def cmd_workspace_save(args) -> int:
    """Commit + push THIS workspace's committed metadata (workspace.json + memory/),
    secret-scanned, via the control plane's own forge — the manual counterpart to the
    debounced auto-save."""
    name = getattr(args, "name", None) or workspace.resolve()
    if not workspace.is_live(name):
        util.err(f"workspace '{name}' is LOCAL (private) — nothing is committed. "
                 f"Make it shareable first: charter workspace live {name}")
        return 1
    rel = _ws_meta_paths(name)
    if not rel:
        util.info(f"workspace '{name}' has no manifest/memory to save yet "
                  "(snapshot it or add a memo first).")
        return 0
    msg = getattr(args, "message", None) or f"workspace({name}): manifest + memory"
    return commit_push(config.ROOT, ["add", "--", *rel], msg)


def _ws_meta_paths(name: str) -> list[str]:
    """A LIVE workspace's shareable paths, as repo-relative strings — what `save`, the
    autosave, `live --off` and `rename` hand to git.

    Must name the same set that `workspace._live_block` un-ignores, and nothing but a test
    enforces that, because the two live in different modules. `todos/` was added to the
    gitignore block and not here, so a LIVE workspace's todo list was visible to git and
    staged by nothing — un-ignoring a path only says it *may* be tracked. See
    tests/test_todos_are_committed.py, which pins the two halves together.

    Filtered by existence deliberately: these go to git as literal paths, and `git rm
    --cached` on one that was never tracked fails the whole call — taking the manifest and
    memory down with a workspace that simply had no todos.

    `changes/` is asked a sharper question than the others, because for it the existence
    filter is not a safe proxy for the one being asked. `todos/` is born with its index and
    is never empty afterwards; a `changes/` can be emptied by `charter change forget`, and
    one holding nothing but the never-committed `changes/log/` is the same case with a
    directory in the way. Both are "exists, nothing tracked", which is exactly the shape
    that fails the whole call. `change.has_records` answers what this list means to ask.
    """
    wd = workspace.workspace_dir(name)
    rel = [str(p.relative_to(config.ROOT))
           for p in (wd / "workspace.json", wd / "workspace.md", wd / "memory",
                     wd / "todos") if p.exists()]
    if change.has_records(name):
        rel.append(str((wd / change.DIRNAME).relative_to(config.ROOT)))
    return rel


def _spawn_pushbg(root) -> None:
    """Fire a detached background push of the workspace's just-committed metadata
    (best-effort) so a slow push never blocks the turn — the same mechanism, for the
    same reason, as `planegit._spawn_bg_push` (`discover`'s own background push of
    HEAD). `util.self_relaunch_argv` (#390) is what keeps the child from importing
    whatever `charter/` package happens to sit under *root* instead of the installed
    package — this always runs with cwd set to the control plane root, which for
    anyone developing charter itself IS a checkout with its own `charter/` package."""
    try:
        subprocess.Popen(util.self_relaunch_argv("workspace", "_pushbg"),
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         stdin=subprocess.DEVNULL, start_new_session=True, cwd=str(root))
    except Exception:
        pass


def cmd_workspace_autosave(args) -> int:
    """Internal (Stop hook): debounced, secret-scanned auto-save of the active workspace's
    manifest + memory — a reactive, agent-triggered commit, so it honours the control
    plane's declared ``config.MEMORY_SHARE`` posture (default ``local``: never even commits).
    Under ``commit``/``push`` it commits locally (fast, scoped); under ``push`` it also
    pushes in the BACKGROUND, so a slow push never blocks the turn. Best-effort — never
    raises, never blocks."""
    try:
        from . import instance as _instance
        # Re-clamp defensively — see `instance.clamp_share`: `config.MEMORY_SHARE` is
        # always pre-clamped at import time, but this reactive path must not itself rely
        # on that upstream guarantee.
        share = _instance.clamp_share(config.MEMORY_SHARE)
        if share == "local":
            return 0  # the safe default → this workspace memo stays on disk, never committed
        name = workspace.resolve()
        if not workspace.is_live(name):
            return 0  # LOCAL workspace → private, never auto-committed
        rel = _ws_meta_paths(name)
        if not rel:
            return 0
        if not _git(["status", "--porcelain", "--", *rel], cwd=config.ROOT).stdout.strip():
            return 0  # nothing pending
        marker = config.STATE_DIR / "ws-autosave" / name
        config.private_mkdir(marker.parent)
        if marker.exists() and time.time() - marker.stat().st_mtime < 90:
            return 0  # debounce: at most once per ~90s per workspace
        # commit locally, scoped + secret-scanned (commit_push refuses a secret → rc 1)
        if commit_push(config.ROOT, ["add", "--", *rel],
                       f"workspace({name}): auto-save memo + manifest", no_push=True) != 0:
            return 0
        config.write_for(marker, str(time.time()))
        if share == "push":
            # detached background push — the turn returns immediately
            _spawn_pushbg(config.ROOT)
    except Exception:
        pass
    return 0


def cmd_workspace_pushbg(args) -> int:
    """Internal: push HEAD to the control plane via its own forge's CLI (the background
    half of autosave, and of every reactive `persona remember`).

    **This used to be a second implementation of the push, and that was #373.** It had its
    own rebase-retry, no protected-branch recognition, and ``return 0`` on every failure —
    so on a plane whose default branch requires a pull request, `charter save` landed the
    change on `charter/<sha>` (#167) while a reactive memory commit was rejected, kept, and
    never mentioned again. Eleven commits were stranded on a local `main` in one session,
    each one destroyed by the next ordinary `git reset --hard origin/main`. Delegating is
    the fix: `planegit.push_head` is the only pusher, so the policy holds here by
    construction rather than by keeping two lists of forge signatures in step.

    ``announce=False`` because this runs DETACHED with stdout and stderr on ``/dev/null``
    (`planegit._spawn_bg_push`). Nothing printed here can reach anybody, which is exactly
    why `push_head` records the outcome as well as saying it — `charter doctor` is where
    this process gets to speak.

    Still rc 0 on every outcome: it is spawned from a Stop hook and must never break a
    turn. What changed is that rc 0 is no longer the only thing it leaves behind.
    """
    planegit.push_head(config.ROOT, announce=False)
    return 0


def _scope_note(scope: str) -> str:
    """How far the selection reaches, in the words the reader needs.

    Only a terminal pointer survives closing and reopening Claude. A session-scoped
    selection is gone the moment the session is, and saying so is the whole point: the
    reader who is not told goes looking for a bug the next time the status line says
    `default`.
    """
    if scope == "terminal":
        return " (this terminal only — kept across closing/reopening Claude)"
    if scope == "session":
        return (f" (this session only — this terminal reports no pane id, so a new "
                f"session starts at '{config.DEFAULT_WORKSPACE}')")
    return ""


def cmd_workspace_reconcile(args) -> int:
    """Internal (SessionStart hook): seed this Claude session's workspace pointer
    from its terminal pane's selection, so a reopened session resumes its workspace."""
    sid = os.environ.get("CLAUDE_CODE_SESSION_ID")
    if not sid:
        try:
            sid = (json.load(sys.stdin) or {}).get("session_id")
        except Exception:
            sid = None
    workspace.reconcile(session_id=sid)
    return 0


def cmd_workspace_remember(args) -> int:
    """Record one workspace memory as its own timestamp-prefixed file (and index it) — the
    task journal, structured like persona memory (a small explorable DB, not one big log).
    With no text, show the workspace's memories instead. Committed + shared for LIVE."""
    name = getattr(args, "workspace", None) or workspace.resolve()
    text = getattr(args, "text", None) or getattr(args, "message", None)
    if not text:
        return cmd_workspace_recall(args)
    p = workspace.remember(name, text, title=getattr(args, "title", None))
    util.ok(f"Remembered in '{name}' → workspaces/{name}/memory/{p.name}")
    if not workspace.is_live(name):
        util.info(f"  '{name}' is LOCAL (private) — memory stays on disk, not committed. "
                  f"Make it shareable: charter workspace live {name}")
    elif getattr(args, "no_sync", False):
        util.info("  (--no-sync) recorded locally; share later with: charter workspace save.")
    else:  # LIVE + reactive: the memory reaches the shared repo immediately
        rels = [str(p.relative_to(config.ROOT)),
                str(workspace.memory_index(name).relative_to(config.ROOT))]
        commit_memory_reactive(rels, f"workspace({name}): {p.stem}")
    return 0


# `note` is the long-standing verb — keep it as an alias for `remember`.
def cmd_workspace_note(args) -> int:
    return cmd_workspace_remember(args)


#: The two ways a todo stops being open — `done` (finished) and `forget` (abandoned).
#: They are safe as leading words where a `list` subcommand was not (see the docstring
#: below) because each is followed by a SLUG: recording never has a second positional, so
#: "did the user mean to record this?" is answered by the shape of the call, not by
#: guessing at the word. Two states only, and neither is `in_progress` (docs/adr/0006).
_CLOSING_VERBS = ("done", "forget")


def cmd_workspace_todo(args) -> int:
    """Record one **todo** — something this task still means to do — list them, or close one.

    Shaped like `remember`: the verb with text records, the verb bare lists. A literal
    `todo list` subcommand would be indistinguishable from recording a todo whose text is
    "list", which is a real thing somebody will eventually try to write down.

    Intent is not memory (docs/adr/0004): a todo stops being true the moment it is done,
    so it lives in its own store and never surfaces from `charter recall`.
    """
    from . import todos
    name = getattr(args, "workspace", None) or workspace.resolve()
    text = (getattr(args, "text", None) or "").strip()
    slug = (getattr(args, "slug", None) or "").strip()
    query = getattr(args, "query", None)

    if text in _CLOSING_VERBS:
        if not slug:
            # Falling through to the recording path here would record a todo whose text is
            # "done" — a silent wrong action in answer to an obvious slip, and one nobody
            # could act on later. Ask for the missing half instead.
            util.err(f"`todo {text}` needs the slug of the todo to close.")
            util.info(f"  The slug is the first column: charter ws todo --workspace {name}")
            # Issue #59: the reserved words swallow a todo you genuinely wanted to record
            # under that name. The escape already existed — matching is exact and
            # lowercase, so any other capitalisation records — but nothing said so, which
            # made a recoverable slip look like a wall.
            util.info(f"  To record a todo actually called \"{text}\", capitalise it or "
                      f"add a word: charter ws todo \"{text.capitalize()} …\"")
            return 1
        return _close_todo(name, slug, journal=(text == "done"))

    if not text:
        return _list_todos(name, query)

    # Duplicate intent is worse than duplicate memory: closing one of a near-identical
    # pair leaves its twin looking outstanding, so the list starts lying about what is
    # left. Warn and skip rather than merge, so the writer learns it is already there.
    dup = todos.duplicate_of(name, text)
    if dup:
        util.err(f"already on the list: {dup}")
        util.info(f"  See it: charter ws todo --workspace {name}")
        return 1

    p = todos.add(name, text)
    util.ok(f"Todo recorded in '{name}' → workspaces/{name}/todos/{p.name}")
    if not workspace.is_live(name):
        util.info(f"  '{name}' is LOCAL (private) — todos stay on disk, not committed.")
    return 0


def _close_todo(name: str, slug: str, *, journal: bool) -> int:
    """Close one todo: **delete** it, leaving a journal entry (`done`) or nothing (`forget`).

    Closing deletes rather than marking done because the journal is already the permanent
    record of what happened — keeping closed todos would build a second one that every read
    then has to filter past, and a store whose reads all begin with a filter is a store that
    will eventually be read without it.

    Which makes the journal entry the load-bearing half, not a courtesy. An agent may close
    todos it recorded itself, so without a trace outside the list it could create and tick
    off work unobserved and the list would always read as finished — worse than no list,
    because it would be confidently wrong. `forget` has no trace precisely because it claims
    nothing happened.

    Journalling happens BEFORE the delete: the entry is the only surviving evidence, so it
    is written while the todo is still there to name. Both halves resolve through the
    workspace's own todo directory, which is what keeps one workspace from closing another's
    work.

    **That last sentence used to claim more than the code did** (#339). `memstore.resolve`
    applies no containment, and every workspace's `todos/` lives under the same data root,
    so `../../<other>/todos/<slug>` resolved to a NEIGHBOUR's file and `unlink` took it —
    `contain.file_refusal` had nothing to object to, because the target really is plane
    data. The reachable input is argv rather than a committed file, so this was never a
    finding; the comment was load-bearing anyway, and one `segment_ok` is cheaper than
    softening it. A slug names one entry in this workspace's own directory or it names
    nothing.
    """
    from . import contain, memstore, todos
    if not contain.segment_ok(slug):
        util.err(contain.refusal(slug))
        util.info(f"  List the real ones: charter ws todo --workspace {name}")
        return 1
    d = todos.todos_dir(name)
    p = memstore.resolve(d, slug)
    if p is None:
        util.err(f"no todo '{slug}' in workspace '{name}'.")
        util.info(f"  List the real ones: charter ws todo --workspace {name}")
        return 1
    # Ask the store for the title rather than re-parsing the file here: `open_todos` already
    # decides what a todo is called, and two answers to that would eventually disagree.
    title = next((t["title"] for t in todos.open_todos(name) if t["slug"] == p.stem), p.stem)

    if journal:
        # Through the normal note path, so a LIVE workspace shares this entry exactly like
        # every other journal entry. A trace that stays on disk while the rest of the
        # journal is shared is a weaker trace than the argument above needs.
        cmd_workspace_remember(SimpleNamespace(
            workspace=name, text=f"Closed todo: {title}", title=None, no_sync=False))

    memstore.forget(d, p.name)  # drops the file AND its index line — no residue either side
    if journal:
        util.ok(f"Closed '{title}' in '{name}' — the journal has the trace.")
    else:
        util.ok(f"Dropped '{title}' from '{name}' — abandoned, so nothing was journalled.")
    return 0


def _list_todos(name: str, query: str | None) -> int:
    """Open todos, oldest first — the ranking the whole feature uses."""
    from . import todos
    if query:
        hits = todos.search(name, query)
        if not hits:
            util.info(f"No todos in '{name}' match {query!r}.")
            return 0
        for p, title, _score in hits:
            print(f"  {p.stem}  {title}")
        return 0

    open_ = todos.open_todos(name)
    if not open_:
        # Silence is indistinguishable from a broken command, so say it plainly.
        util.info(f"No open todos in '{name}'. Record one: charter ws todo \"<what>\"")
        return 0

    for t in open_:
        print(f"  {t['slug']}  {t['age_days']}d  {t['title']}")
    return 0


def cmd_workspace_recall(args) -> int:
    """Search this workspace's memories (--query) or list them all chronologically."""
    name = getattr(args, "workspace", None) or workspace.resolve()
    query = getattr(args, "query", None)
    results = workspace.recall(name, query)
    if not results:
        if query:
            util.info(f"No memories in '{name}' match '{query}'.")
        else:
            util.info(f"workspace '{name}' has no memories yet. "
                      f'Add one: charter workspace remember "<text>"')
        return 0
    for p, title, score in results:
        tag = f"  ({score})" if score else ""
        print(f"  • {title}{tag}  [{p.name}]")
    util.info(f"{len(results)} memory(ies) in workspaces/{name}/memory/ — "
              f"read one: cat workspaces/{name}/memory/<file>")
    return 0


def cmd_workspace_forget(args) -> int:
    """Delete one workspace memory by slug or filename (and drop its index line)."""
    name = getattr(args, "workspace", None) or workspace.resolve()
    if workspace.forget_memory(name, args.slug):
        util.ok(f"Forgot '{args.slug}' from workspace '{name}'.")
        # Symmetric with `remember` — see `cmd_persona_forget` for why the directory,
        # not the deleted file, is what gets staged (#82).
        commit_memory_reactive([str(workspace.memory_dir(name).relative_to(config.ROOT))],
                               f"workspace({name}): forget {args.slug}")
        return 0
    util.err(f"no memory '{args.slug}' in workspace '{name}' (list them: charter workspace recall).")
    return 1


def cmd_workspace_vision(args) -> int:
    """Show or set the workspace's Vision — the north star in its living charter
    (workspaces/<name>/workspace.md). With text, replace the Vision; without, print
    it (and the charter path). The rest of the charter — Context & decisions, Glossary
    — is edited directly in workspace.md as the work evolves."""
    name = getattr(args, "workspace", None) or workspace.resolve()
    if not workspace.workspace_dir(name).exists():
        util.err(f"no workspace '{name}'")
        return 1
    text = getattr(args, "text", None)
    if text:
        workspace.set_vision(name, text)
        util.ok(f"Vision set for '{name}' → workspaces/{name}/workspace.md")
        return 0
    vision = workspace.read_vision(name)
    if vision:
        print(vision)
    else:
        util.info(f"workspace '{name}' has no vision yet. Set it: "
                  f'charter workspace vision "<the goal>"')
    util.info(f"Full charter: workspaces/{name}/workspace.md "
              "(Vision · Context & decisions · Glossary — edit it as the work evolves).")
    return 0


def cmd_workspace_fork(args) -> int:
    """Fork a workspace: create <new> pre-loaded with <src>'s context — the living
    charter (workspace.md: vision, context, glossary), the manifest (repos+branches),
    the task memo, and the open todos — so you can branch off and continue with full
    context. The repo clones are not copied (they're reconstructible): pass --restore to
    clone them from the manifest, or `charter clone` on demand. Starts LOCAL unless --live."""
    from . import todos
    src, new = args.src, args.new
    if not workspace.valid_name(new):
        util.err(f"invalid workspace name '{new}' (use lowercase letters, digits, . _ -)")
        return 1
    if src == new:
        util.err("source and fork names are the same — nothing to fork.")
        return 1
    if not workspace.workspace_dir(src).exists():
        util.err(f"no workspace '{src}'")
        return 1
    if workspace.workspace_dir(new).exists():
        util.err(f"workspace '{new}' already exists — pick another name or remove it first.")
        return 1

    workspace.ensure(new)
    workspace.scaffold(new)  # baseline; the charter is overwritten from src below
    src_charter = workspace.charter_file(src)
    if src_charter.exists():
        shutil.copyfile(src_charter, workspace.charter_file(new))
    src_mem = workspace.memory_dir(src)
    if src_mem.exists():
        shutil.copytree(src_mem, workspace.memory_dir(new), dirs_exist_ok=True)
    # Open todos travel with the memory, and for the same reason: a fork exists so someone
    # can pick the task up with full context, and what is still to be done is the most
    # actionable part of that. Inheriting everything the task LEARNED while dropping
    # everything it still INTENDED leaves the fork re-deriving the plan — the exact problem
    # todos were built to end.
    #
    # A COPY, so the two lists diverge from here: closing one in the fork must not rewrite
    # the source's plan. Copied wholesale (files + index) rather than re-added one by one,
    # which would restamp every todo with today's date and hide a three-week-old intent
    # behind a fresh one — age is the only ranking this feature has. Closed todos never
    # arise, since finishing one deletes it.
    src_todos = todos.todos_dir(src)
    if src_todos.exists():
        shutil.copytree(src_todos, todos.todos_dir(new), dirs_exist_ok=True)
    # Membership comes from BOTH sources — see `workspace.merge_repo_rows`. Reading the
    # manifest alone made a fork of an un-snapshotted workspace inherit nothing while
    # `status` cheerfully reported its clones (#81).
    m = workspace.read_manifest(src)
    disk = [{"name": d.name, "branch": _repo_branch(d)} for d in workspace.clones(src)]
    repos, disk_only = workspace.merge_repo_rows(m.get("repos") or [], disk)
    if repos or m:
        m["name"] = new
        m["forked_from"] = src
        m["repos"] = repos
        m.setdefault("description", "")
        m["updated_at"] = datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")
        m["updated_by"] = _git_user()
        workspace.write_manifest(new, m)
    workspace.note(new, f"Forked from '{src}' — inherited its vision, context, glossary, and memo.")

    if getattr(args, "live", False):
        workspace.set_live(new, True)
    util.ok(f"Forked '{src}' → '{new}' — charter + context + memo copied "
            f"({'LIVE' if getattr(args, 'live', False) else 'LOCAL'}).")
    inherited = todos.count_open(new)
    if inherited:
        # Said out loud because the two lists are independent from here: whoever forked
        # needs to know they now own a second copy of this intent, not a view of the first.
        util.info(f"Inherited {inherited} open todo(s) from '{src}' — the fork's own list "
                  f"now: charter ws todo --workspace {new}")
    # Say which source each repo came from. A snapshot promises its branch was pushed;
    # a clone found on disk promises only that it is checked out here right now, and
    # `restore` on another machine will fail on a branch that was never pushed. Reporting
    # it at the moment of use beats a drift warning that would fire on every workspace
    # nobody has snapshotted — which is most of them, by design.
    if disk_only:
        util.info(f"  {len(disk_only)} of them come from clones on disk, not from a "
                  f"snapshot ({', '.join(disk_only)}) — their branch is whatever is "
                  f"checked out now, which may not be pushed. `charter workspace "
                  f"snapshot {src}` records them properly.")
    if getattr(args, "restore", False) and repos:
        util.info(f"Cloning {len(repos)} inherited repo(s)…")
        return cmd_workspace_restore(SimpleNamespace(name=new, on_demand=False))
    if repos:
        util.info(f"Clone its {len(repos)} inherited repo(s): charter workspace restore {new}  "
                  f"(or re-run fork with --restore).")
    else:
        util.info(f"No repos on '{src}' — neither a snapshot nor clones on disk. "
                  f"Clone into the fork: charter clone <repo> -w {new}")
    if getattr(args, "live", False):
        util.info(f"Share the fork: charter workspace save {new}")
    return 0


#: Baseline components a LIVE workspace actually shares (see the managed block in
#: .gitignore that `workspace live` writes). A workspace's refs/ and its structure
#: marker stay local, so restoring only those gives `save` nothing to commit.
_LIVE_SHARED_COMPONENTS = {"workspace.md", "memory/MEMORY.md"}


def cmd_workspace_reinit(args) -> int:
    """Bring a workspace's on-disk structure up to the current layout — create any missing
    baseline files (workspace.md charter, memory/, refs/) and stamp the structure version.
    A workspace created by an older version of charter is flagged (status line,
    `workspace list`) until this runs. Idempotent + additive: existing content is never
    touched. `--all` fixes every workspace at once (handy after a charter upgrade)."""
    if getattr(args, "all", False):
        names = workspace.list_workspaces()
    else:
        name = getattr(args, "name", None) or workspace.resolve()
        if not workspace.workspace_dir(name).exists():
            util.err(f"no workspace '{name}'")
            return 1
        names = [name]
    if not names:
        util.info("No workspaces to reinitialize.")
        return 0
    healed = 0
    for n in names:
        before = workspace.reinit(n)
        if before["ok"]:
            continue
        healed += 1
        what = (", ".join(before["missing"]) if before["missing"]
                else f"structure v{before['version']} → v{before['target']}")
        util.ok(f"Reinitialized '{n}' → added {what}.")
        # Only advise `save` when something LIVE actually shares was restored. A
        # workspace's refs/ and its structure marker are gitignored, so healing
        # only those leaves nothing to commit and the advice sends you to a
        # command that prints "Nothing to save".
        if workspace.is_live(n) and set(before["missing"]) & _LIVE_SHARED_COMPONENTS:
            util.info(f"  '{n}' is LIVE — commit the restored files: charter workspace save {n}")
    if healed == 0:
        util.ok(f"Up to date (structure v{workspace.STRUCTURE_VERSION}) — nothing to do.")
    elif len(names) > 1:
        util.info(f"Healed {healed} of {len(names)} workspace(s); the rest were current.")
    return 0


def _warn_env_override(name: str) -> None:
    env = os.environ.get("CHARTER_WORKSPACE")
    if env and env.strip() != name:
        util.warn(
            f"$CHARTER_WORKSPACE='{env}' is set and takes precedence — commands in this "
            f"session will still act on '{env}', not '{name}'."
        )
