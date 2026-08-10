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

from . import config, gitpolicy, util, workspace
from .commands import (_cred_flag, _git, _origin_https, cmd_clone, commit_memory_reactive,
                       commit_push)


def cmd_workspace_list(args) -> int:
    active = workspace.resolve()
    names = workspace.list_workspaces()
    if not names:
        util.info(f"No workspaces yet. Active resolves to '{active}'. "
                  "Create one: charter workspace create <name>")
        return 0
    print(f"Active workspace: {active}  (via {workspace.source()})\n")
    live = workspace.live_workspaces()
    fmt = "{}{:<22}{:<7}{:<7}{}"
    print(fmt.format("  ", "WORKSPACE", "MODE", "CLONES", "REPOS"))
    for n in names:
        cl = workspace.clones(n)
        repos = ", ".join(d.name for d in cl) if cl else "—"
        mark = "* " if n == active else "  "
        stale = " ⚠" if workspace.needs_reinit(n) else ""
        print(fmt.format(mark, n + stale, "live" if n in live else "local", str(len(cl)), repos))
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

    if args.repos:
        return cmd_clone(SimpleNamespace(repos=args.repos, workspace=args.name))
    if not args.use:
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
    existing = workspace.list_workspaces()
    if args.name not in existing and not getattr(args, "create", False):
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

    shutil.rmtree(wd)
    util.ok(f"Removed workspace '{name}' and its clones.")
    if workspace.resolve() == name and workspace.source() in ("session", "active-file"):
        # Removing the locked workspace: force past the lock, then re-lock to default.
        workspace.set_active(config.DEFAULT_WORKSPACE, force=True)
        util.info(f"Active workspace reset to '{config.DEFAULT_WORKSPACE}'.")
    return 0


def cmd_workspace_rename(args) -> int:
    """Rename a workspace: move workspaces/<old>/ → workspaces/<new>/ (clones, memory,
    refs, and manifest come along), fix the manifest name + liveness block, and repoint
    the active session/terminal pointer + lock so a renamed active workspace stays
    active. For a LIVE workspace, commit the tracked move (manifest + memory) so the
    rename propagates to the team."""
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

    workspace.rename(old, new)
    util.ok(f"Renamed workspace '{old}' → '{new}' (clones, memory, and manifest moved).")
    if workspace.resolve() == new and workspace.source() in ("session", "active-file"):
        util.info(f"This session's active workspace followed the rename → '{new}' (still 🔒 locked).")

    if not was_live:
        util.info(f"'{new}' is LOCAL (private) — nothing committed.")
        return 0
    new_rel = _ws_meta_paths(new)
    msg = getattr(args, "message", None) or f"workspace: rename {old} → {new}"
    return commit_push(config.ROOT, ["add", "-A", "--", *tracked_old, *new_rel, ".gitignore"], msg)


def _work_at_risk(name: str) -> list[str]:
    """Clones in the workspace with uncommitted or unpushed work."""
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
    missing = [r["name"] for r in repos
               if not workspace.is_git_repo(workspace.workspace_dir(name) / r["name"])]
    if missing:
        cmd_clone(SimpleNamespace(repos=missing, workspace=name))
    ok = 0
    for r in repos:
        d = workspace.workspace_dir(name) / r["name"]
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
        if _git(["checkout", r["branch"]], cwd=d).returncode == 0:
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
    wd = workspace.workspace_dir(name)
    return [str(p.relative_to(config.ROOT))
            for p in (wd / "workspace.json", wd / "workspace.md", wd / "memory") if p.exists()]


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
        marker.parent.mkdir(parents=True, exist_ok=True)
        if marker.exists() and time.time() - marker.stat().st_mtime < 90:
            return 0  # debounce: at most once per ~90s per workspace
        # commit locally, scoped + secret-scanned (commit_push refuses a secret → rc 1)
        if commit_push(config.ROOT, ["add", "--", *rel],
                       f"workspace({name}): auto-save memo + manifest", no_push=True) != 0:
            return 0
        marker.write_text(str(time.time()))
        if share == "push":
            # detached background push — the turn returns immediately
            subprocess.Popen([sys.executable, "-m", "charter", "workspace", "_pushbg"],
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                             stdin=subprocess.DEVNULL, start_new_session=True, cwd=str(config.ROOT))
    except Exception:
        pass
    return 0


def cmd_workspace_pushbg(args) -> int:
    """Internal: push HEAD to the control plane via its own forge's CLI (the background
    half of autosave)."""
    root = config.ROOT
    https = _origin_https(root)
    if not https:
        return 0
    cred = _cred_flag(gitpolicy.forge_for(root))
    branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=root).stdout.strip()
    p = _git([*cred, "push", https, f"HEAD:{branch}"], cwd=root)
    if p.returncode != 0 and any(s in (p.stderr or "") for s in ("fetch first", "non-fast-forward", "rejected")):
        _git([*cred, "fetch", https, branch], cwd=root)
        if _git(["rebase", "FETCH_HEAD"], cwd=root).returncode == 0:
            p = _git([*cred, "push", https, f"HEAD:{branch}"], cwd=root)
        else:
            _git(["rebase", "--abort"], cwd=root)
            return 0
    if p.returncode == 0:
        _git(["update-ref", f"refs/remotes/origin/{branch}", "HEAD"], cwd=root)
    return 0


def _scope_note(scope: str) -> str:
    if scope in ("session", "terminal"):
        return " (this terminal only — kept across closing/reopening Claude)"
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
    and the task memo — so you can branch off and continue with full context. The
    repo clones are not copied (they're reconstructible): pass --restore to clone them
    from the manifest, or `charter clone` on demand. Starts LOCAL unless --live."""
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
    m = workspace.read_manifest(src)
    repos = list(m.get("repos") or [])
    if m:
        m["name"] = new
        m["forked_from"] = src
        m["updated_at"] = datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")
        m["updated_by"] = _git_user()
        workspace.write_manifest(new, m)
    workspace.note(new, f"Forked from '{src}' — inherited its vision, context, glossary, and memo.")

    if getattr(args, "live", False):
        workspace.set_live(new, True)
    util.ok(f"Forked '{src}' → '{new}' — charter + context + memo copied "
            f"({'LIVE' if getattr(args, 'live', False) else 'LOCAL'}).")
    if getattr(args, "restore", False) and repos:
        util.info(f"Cloning {len(repos)} repo(s) from the inherited manifest…")
        return cmd_workspace_restore(SimpleNamespace(name=new, on_demand=False))
    if repos:
        util.info(f"Clone its {len(repos)} recorded repo(s): charter workspace restore {new}  "
                  f"(or re-run fork with --restore).")
    else:
        util.info(f"No repos recorded on '{src}' yet — clone into the fork: charter clone <repo> -w {new}")
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
