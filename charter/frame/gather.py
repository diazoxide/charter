"""One scan of the plane's repo state, cached under the frame's own directory.

``statusline.render`` performs an expensive gather before it lays anything out:
``_repo_trees``, ``_repo_states`` (one ``git status --porcelain --branch`` per
repo), ``_branch`` per tree, and ``glstate.read_for``. The frame has up to four
panels (``top``/``repos``/``right``/``bottom``); if each gathered independently
that would be four identical git sweeps per repaint, which destroys the property
the frame was built for — see ``charter/frame/panel.py``'s own docstring: a panel
repaints only on a ``state.version`` bump, so watching a frame must cost a
``stat`` at idle, never a sweep.

So the gather happens ONCE here and is cached as plain JSON under the frame's own
directory (``charter/frame/state.py``'s ``frame_dir``) — never as ``tui.Node``\\ s
or ANSI. Composition (ranking, capping, columns) is a later task's job: "share the
gather, not the composition" (docs/superpowers/plans/2026-08-23-frame-parity.md).
:func:`scan` is the expensive part; :func:`save`/:func:`read` are the cache's
write/read pair; :func:`refresh` is the two composed, for the hook that will call
this on every plane-state bump (Task 2).

Every helper that actually gathers something is reused from ``statusline.py`` —
the SAME ``_repo_trees``, ``_repo_states``, ``_branch``, ``_detail_worktrees``,
``_current`` the status line calls, including the SAME ``_STATE_TTL``-cached
``repostate.json`` underneath ``_repo_states``. A fix to a repo row's git-status
logic lands here automatically; nothing about a repo's dirty/ahead/behind state is
re-derived. ``charter/statusline.py``'s render path is not modified — this module
only calls it.

**Every repo entry also carries its own ``worktree_count``** (fix round 1,
finding 2, #385) — ``worktree.dirs_for(active, d.name)``, the SAME filesystem-only
(no subprocess) call ``statusline._repo_rows`` already makes PER REPO on every
render for its ``⑂N`` badge, gathered here once instead. Task 3 shipped without
this and reported the gap rather than adding a live call to its own renderer: a
multi-repo workspace's cache carried repo rows with no piece information at all,
because ``_detail_worktrees`` (below) only ever builds full piece ROWS for the
single-repo case, and nothing folded the plain per-repo COUNT in for every other
shape. Now every ``repos[i]["worktree_count"]`` is real regardless of repo count;
only the full per-piece detail rows (``worktrees``, below) stay single-repo-only.

**Never raises.** Every field gathered below is wrapped individually, so one bad
repo (a ``.git`` gone missing between ``_repo_trees`` and ``_branch``, say)
degrades that repo's row rather than the whole scan, and the whole function falls
back to an empty structure rather than propagate. Task 2 will call this from
inside charter's hooks, where "a hook may cost a session its briefing, never its
turn" (``charter/frame/notify.py``) applies with the same force here.

**No per-session payload.** The status line receives Claude Code's JSON payload
(session id, the session's cwd) on stdin every render; a frame panel has no such
thing. :func:`scan` asks ``workspace.resolve()``/``_current({"cwd": ...})`` with
the *process's own* cwd instead — the same precedence ``frame/slots.py``'s
``_top`` already reads. A frame panel and the ambient ``charter statusline
--watch`` are the same kind of caller as far as this is concerned.
"""

from __future__ import annotations

import json
import os
import time
from pathlib import Path

from .. import config
from . import state

#: Filename under a frame's own directory (``state.frame_dir``) that holds the
#: cached scan. Private to this module — nothing outside ``gather.py`` needs the
#: name; a caller asks :func:`save`/:func:`read` instead of the path.
_CACHE_NAME = "gather.json"

#: How many of the workspace's open todos :func:`scan` writes into the cache.
#:
#: **A bound on the cache file, not a rendering choice.** ``frame/slots.py``'s ``_right``
#: draws at most a handful of rows and says how many it hid, so nothing on screen wants
#: more than this; what this number actually stops is a workspace with four hundred open
#: todos turning a file every panel re-reads into a four-hundred-entry JSON document. It
#: is deliberately far above what any pane can draw, so the renderer's own budget — not
#: this — is what decides how many rows appear.
#:
#: The COUNT is stored separately (``todo_count``) and is never clipped, so the
#: ``…(+N more)`` line stays honest past this bound: clipping the list and then deriving
#: the total from its length would tell an operator with four hundred todos they have
#: twenty.
_MAX_TODOS = 20


def _cache_file(fid: str, *, create: bool) -> Path | None:
    """The cache file's path for *fid*, or ``None`` when ``state.frame_dir``
    refuses it (a hostile id) or — with ``create=False`` — its directory has never
    been made.

    ``create`` is forwarded to :func:`state.frame_dir` unchanged, for the same
    reason that module keeps reads and writes on separate paths: a READ
    (:func:`read`) must never create the directory it is only trying to look at
    (the same rule ``state.version`` documents — a panel's poll must not
    resurrect a directory ``reap()`` just removed), while a WRITE (:func:`save`)
    is exactly the case allowed to mint one.
    """
    d = state.frame_dir(fid, create=create)
    return None if d is None else d / _CACHE_NAME


def _empty(workspace: str | None) -> dict:
    """The structure :func:`scan` (and :func:`read`, on total failure) falls back
    to — still a plain, JSON-serialisable dict, so a renderer never needs a
    ``None``-check before indexing into it."""
    return {
        "gathered_at": time.time(),
        "workspace": workspace or "",
        "current_repo": None,
        "repos": [],
        "worktrees": [],
        "todos": [],
        "todo_count": 0,
        "changes": [],
    }


def _change_rows(active: str) -> list[dict]:
    """This workspace's changes, as rows a pane can draw — **file reads only**.

    Two files per change and nothing else: the record (what the operator declared) and
    the landing log (what charter declared it merged). No forge call, no git subprocess,
    no `glstate`. That is the cost half of §4g's idle-tick property said at the source
    rather than at the renderer: a five-member change is five forge reads, and a scan that
    made them would put them on every plane-state bump.

    **`glstate` is explicitly not reused**, and it is tempting because it already caches
    forge state. It is wrong twice: it is keyed on each clone's *currently checked-out*
    branch, which is frequently not the member's, and what it caches is `ci_status`, which
    the change surface is forbidden to read — a single string that answers ``None`` for a
    CLI failure, a timeout, an auth error and *"no check ever ran"* alike (#561).

    **So every member charter has no landing declaration for is ``unknown``, and the
    change is never greener than its worst member.** That is not a placeholder: `unknown`
    is the only value meaning *charter did not look*, and what stands between a member and
    its own landing — its request's state, its checks at its head sha — is a forge read
    this scan does not make. A member charter observed is `landed` or `blocked`; every
    other member says so.

    Never raises, like every other field here: one unreadable record degrades to the rows
    that could be read, and a workspace with no `changes/` at all yields ``[]`` — which is
    the lazy-creation case, not an error.
    """
    from .. import change as change_mod

    records, _refused = change_mod.all_for(active)
    # The whole log is read ONCE and grouped, rather than once per change. `landings`
    # parses every line of every host's file to answer about one slug, so asking it per
    # record is the log read N times on a path that already runs on every plane-state bump.
    # The rows below carry no line count and no per-change file, so nothing downstream can
    # tell the difference — which is exactly why it would have gone unnoticed.
    by_change: dict[str, set[str]] = {}
    try:
        for line in change_mod.landings(active):
            by_change.setdefault(line["change"], set()).add(line["repo"])
    except Exception:
        by_change = {}
    rows: list[dict] = []
    for rec in records:
        slug = rec["change"]
        states = change_mod.member_states(rec, by_change.get(slug, set()))
        done, total = change_mod.landed_count(states.values())
        rows.append({
            "change": slug,
            "why": rec["why"],
            "state": change_mod.worst(states.values()),
            "landed": done,
            "total": total,
            "excluded": len(rec["excluded"]),
            "members": [{"repo": m["repo"], "branch": m["branch"],
                         "needs": list(m["needs"]),
                         "state": states.get(m["repo"], "unknown")}
                        for m in rec["members"]],
        })
    return rows


def _entry(d: Path, branch: str, states: dict, gl: dict, cur_repo: str | None,
          worktree_count: int = 0) -> dict:
    """One tree's row — repo or worktree alike, the same facts
    ``statusline._tree_cells`` draws from, minus everything that is layout
    (colour, truncation, tree glyphs, presence text). A narrow-pane renderer
    reads this directly; it never re-derives dirty/ahead/behind/ci/change from
    ``states``/``gl`` itself, which is the whole point of gathering once.

    *worktree_count* defaults to 0 (right for a worktree/piece entry — a piece
    does not itself have pieces); :func:`scan` passes the real count only for
    a REPO entry. See ``worktree_count``'s own note on :func:`scan`.
    """
    st = states.get(d) or {}
    info = gl.get(d) or {}
    return {
        "name": d.name,
        "branch": branch,
        "dirty": bool(st.get("dirty")),
        "tracked_dirty": bool(st.get("tracked_dirty")),
        "ahead": int(st.get("ahead") or 0),
        "behind": int(st.get("behind") or 0),
        "ci": info.get("ci"),
        "change": info.get("change"),
        "sigil": info.get("sigil") or "",
        "current": d.name == cur_repo,
        "worktree_count": int(worktree_count),
    }


def scan(workspace: str | None = None, cwd: str | None = None) -> dict:
    """Perform the gather ``statusline.render`` performs before layout, and
    return it as a plain dict — no ``tui.Node``, no ANSI, no truncation, safe to
    ``json.dumps`` as-is.

    *workspace* defaults to ``workspace.resolve()`` (no session payload — see the
    module docstring). *cwd* defaults to ``os.getcwd()`` and is used only to
    decide which repo (or worktree) is ``"current"``, matching ``_repo_rows``'
    own rule verbatim.

    Best-effort at every step, individually wrapped, so one failing piece
    degrades gracefully rather than losing everything gathered so far; the
    return is still a well-formed structure even if every single step failed.
    """
    from .. import glstate
    from .. import statusline as sl
    from .. import workspace as ws_mod

    try:
        resolved_cwd = cwd if cwd is not None else os.getcwd()
    except OSError:
        resolved_cwd = None

    try:
        active = workspace or ws_mod.resolve(cwd=resolved_cwd)
    except Exception:
        active = workspace or config.DEFAULT_WORKSPACE_FALLBACK

    try:
        cur = sl._current({"cwd": resolved_cwd}) if resolved_cwd else None
    except Exception:
        cur = None

    try:
        dirs = sl._repo_trees(active)
    except Exception:
        dirs = []

    try:
        detail_wts = sl._detail_worktrees(active, dirs)
    except Exception:
        detail_wts = []

    scan_dirs = [*dirs, *detail_wts]

    try:
        states = sl._repo_states(scan_dirs)
    except Exception:
        states = {}

    branches: dict = {}
    for d in scan_dirs:
        try:
            branches[d] = sl._branch(d)
        except Exception:
            branches[d] = "?"

    try:
        gl = glstate.read_for(scan_dirs, branches)
    except Exception:
        gl = {}

    # Same background refresh the status line kicks off on every render (see
    # `render`'s own call to it). Deliberately kept: a frame that never runs
    # alongside a rendered status line — the ordinary case once a session lives
    # inside `charter claude` — would otherwise never keep CI/MR state warm at
    # all, and `glstate.read_for` only ever serves what was last fetched. Cheap
    # when there is nothing to do: gated by `glstate`'s own `SPAWN_COOLDOWN` and
    # an on-disk lock, so four panels (or a burst of hook-driven refreshes)
    # calling this pay for one stat each, not one spawn each.
    try:
        glstate.maybe_spawn(scan_dirs, active)
    except Exception:
        pass

    # Verbatim copy of `_repo_rows`' own rule: the workspace half of `cur` may
    # name the active workspace, or (see `_current`'s docstring) the shared root
    # tree that counts as current in every workspace alike.
    cur_repo = cur[1] if (cur and (cur[0] is None or cur[0] == active)) else None

    # Fix round 1, finding 2 (#385): `_repo_rows` gets its `⑂N` badge from a
    # fresh, always-live `worktree.dirs_for(active, d.name)` call PER REPO, on
    # every render, independent of `detail_wts` above (which only ever holds
    # rows for the single-repo case — `_detail_worktrees`' own rule). That
    # per-repo count was never folded into this scan, so a multi-repo
    # workspace's cache carried no piece information at all — the exact gap
    # Task 3 found and reported rather than working around with a live call of
    # its own. `worktree.dirs_for` is filesystem-only (no subprocess, no
    # network — see its own docstring), so gathering it here costs the same
    # `iterdir`+`stat` sweep `_repo_rows` already pays, once, rather than once
    # per panel that wants it.
    wt_counts: dict = {}
    try:
        from .. import worktree as wt_mod
        for d in dirs:
            try:
                wt_counts[d] = len(wt_mod.dirs_for(active, d.name))
            except Exception:
                wt_counts[d] = 0
    except Exception:
        pass

    try:
        repos = [_entry(d, branches.get(d, "?"), states, gl, cur_repo,
                        worktree_count=wt_counts.get(d, 0)) for d in dirs]
        repo_for_wt = dirs[0].name if dirs else None
        worktrees = []
        for w in detail_wts:
            entry = _entry(w, branches.get(w, "?"), states, gl, cur_repo)
            entry["repo"] = repo_for_wt
            worktrees.append(entry)
    except Exception:
        repos, worktrees = [], []

    # This workspace's own open todos, gathered here for exactly the reason every other
    # field is: ``frame/slots.py``'s ``_right`` lists them in the sidebar, and a panel
    # repaints without asking anybody's permission. ``todos.open_todos`` opens and parses
    # one file per todo — affordable once per plane-state bump, and a per-repaint cost the
    # frame is not allowed to have (``panel.py``: an idle tick is one ``stat``). The
    # renderer therefore reads this and never ``todos`` directly.
    #
    # Only what a row needs — the title. Neither the body nor the age is carried, and the
    # rule is the same for both: a cache every panel re-reads is the wrong place to keep
    # text nothing draws. A sidebar row is one line in a 22-column pane, ``charter ws
    # todo`` is what shows the rest, and a field added here "in case a renderer wants it"
    # is a field nothing is measuring the cost of.
    #
    # Open todos only, which is the whole of what ``open_todos`` returns and is stated
    # here because it is a choice: a done todo is not something the frame is asking you to
    # look at, and a sidebar listing them would be a list you read past rather than act on.
    todo_items: list = []
    todo_count = 0
    try:
        from .. import todos as todos_mod
        open_todos = todos_mod.open_todos(active)
        todo_count = len(open_todos)
        todo_items = [{"title": t.get("title") or ""} for t in open_todos[:_MAX_TODOS]]
    except Exception:
        todo_items, todo_count = [], 0

    # This workspace's changes. Carried in the ONE snapshot, under the one timestamp, for
    # §4f's clock rule and not only its store rule: everything on screen has to be from
    # the same moment by construction rather than by two caches happening to agree, and a
    # design that answered only the store rule would be §4f half-read. The component draws
    # this timestamp's AGE, which is what makes "read 4m ago" a fact rather than a hope.
    try:
        changes = _change_rows(active)
    except Exception:
        changes = []

    return {
        "gathered_at": time.time(),
        "workspace": active,
        "current_repo": cur_repo,
        "repos": repos,
        "worktrees": worktrees,
        "todos": todo_items,
        "todo_count": todo_count,
        "changes": changes,
    }


def save(fid: str, data: dict) -> None:
    """Write *data* (from :func:`scan`) as JSON under *fid*'s frame directory.

    Atomic — `config.replace_for`, the same call ``state.bump`` makes and for the same
    reason: a reader must never observe a half-written cache. **Nor another writer's
    half**, which is the part ``os.replace`` alone never gave and #893 is. While the temp
    file was named after its target and nothing else, two savers for one *fid* wrote into
    one inode, and this cache is where that surfaced: a frame's ``gather.json`` came out
    of CI existing and zero bytes, and :func:`cached` hands back whatever parses, so an
    empty scan is read as a true one rather than as a file to distrust.

    Never raises: this runs from a hook (Task 2), where a failure here must
    degrade to "the cache did not update" rather than cost the session its turn.
    """
    f = _cache_file(fid, create=True)
    if f is None:
        return
    try:
        config.replace_for(f, json.dumps(data))
    except OSError:
        return


def _shaped_like_a_scan(data) -> bool:
    """True when *data* is at least the shape a renderer can index into without
    its own ``None``/type check — a ``dict`` carrying ``repos``/``worktrees`` as
    lists.

    Mirrors ``glstate.read_for``'s own defensive-read pattern (``glstate.py``'s
    docstring: "an entry written by an OLDER charter... is read with a fallback
    so a stale on-disk cache still renders... rather than raising a KeyError").
    A cache file surviving a redeploy is the ordinary case here too, and
    ``json.loads`` succeeding proves only that the bytes were valid JSON — ``42``,
    ``"a string"``, ``[1, 2, 3]``, and ``{"foo": "bar"}`` all parse cleanly and
    are all useless to :func:`read`'s caller. This is deliberately loose beyond
    that: it does not require every field :func:`_entry` writes, so a *future*
    ``scan()`` adding a new field does not make today's otherwise-good cache
    file look corrupt.
    """
    return (isinstance(data, dict)
            and isinstance(data.get("repos"), list)
            and isinstance(data.get("worktrees"), list))


def read(fid: str, *, workspace: str | None = None, cwd: str | None = None) -> dict:
    """The cached scan for *fid*, or a fresh one when there is nothing valid to
    read.

    Degrades rather than raises at every stage a cache can go wrong: a hostile or
    never-bumped *fid* (no directory at all — the ordinary cold-start case, not
    an error), a directory with no cache file yet, a cache file that is not valid
    JSON (``json.JSONDecodeError``, a ``ValueError`` subclass), and — because
    ``json.loads`` succeeding says nothing about what it produced — a cache file
    that parses to something not :func:`_shaped_like_a_scan` (a bare int/str/
    list, a dict missing ``repos``/``worktrees``, or the wrong types for them)
    all fall through to a fresh :func:`scan`, the same one a caller would
    otherwise have no data to draw from. This never returns anything but a dict
    carrying ``repos``/``worktrees`` as lists — never the raw, untrusted value a
    corrupt or stale cache file happened to contain. :func:`cached` is where each of
    those degradations actually lives; this is that plus the fallback.
    """
    data = cached(fid)
    if data is not None:
        return data
    try:
        return scan(workspace=workspace, cwd=cwd)
    except Exception:
        return _empty(workspace)


def _load(fid: str) -> tuple[dict | None, bool]:
    """*fid*'s cache, and whether the file that holds it is UNREADABLE.

    One reader under :func:`cached` and :func:`unreadable`, so the two cannot answer from
    two different readings of the same bytes — the defect #735 is at one remove. A panel
    asks both in the same repaint, and a version that read the file twice through two
    functions could tell an operator "this cache is broken" about a file the other half
    had just accepted.

    The bool is **not** "there is no cache". Those are the two states #735 exists to keep
    apart, and this is the narrower of them: a file that IS there and cannot be read as a
    scan. No directory and no file are both ``(None, False)`` — a gather that has not
    landed yet, which is the ordinary cold start.

    An ``OSError`` that is not :class:`FileNotFoundError` — a permission, an ``EISDIR``, a
    truncated read — counts as unreadable rather than absent, because it is: the path is
    occupied by something this frame cannot read a scan out of, and a re-gather is still
    the remedy.
    """
    f = _cache_file(fid, create=False)
    if f is None:
        return None, False
    try:
        data = json.loads(f.read_text())
    except FileNotFoundError:
        return None, False
    except (OSError, ValueError):
        return None, True
    return (data, False) if _shaped_like_a_scan(data) else (None, True)


def cached(fid: str) -> dict | None:
    """Whatever *fid*'s cache file holds, if it holds something a renderer can index
    into — ``None`` for every way that can fail.

    The half of :func:`read` that does NOT fall back to a live :func:`scan`, split out
    because :func:`row_count` needs exactly this and must not pay for the other half.
    Degrades rather than raising at every stage: a hostile or never-bumped *fid* (no
    directory at all — the ordinary cold-start case, not an error), a directory with no
    cache file yet, a file that is not valid JSON (``json.JSONDecodeError``, a
    ``ValueError`` subclass), and — because ``json.loads`` succeeding says nothing about
    what it produced — one that parses to something not :func:`_shaped_like_a_scan`.

    **Still ``None`` for all four, and #735 did not change that.** Five call sites read this
    — :func:`read`, :func:`row_count`, `slots._repos`, `slots._selected_detail` and
    `commands_frame`'s palette snapshot — and every one wants a value it can draw or a
    value it cannot; raising at the two that are corruption would push a ``try`` onto
    :func:`row_count`'s launch path and into a hook. What #735 added is a SECOND
    question — :func:`unreadable` — asked only by the one caller that says something
    different about the answer.
    """
    return _load(fid)[0]


def unreadable(fid: str) -> bool:
    """Whether *fid* HAS a cache file that cannot be read as a scan.

    The fact `frame/slots.py`'s `_repos` needs and :func:`cached`'s ``None`` throws away.
    Four different readings collapse into that ``None`` — no frame directory, no cache
    file, a file that is not JSON, a file that parses to something that is not a scan —
    and the first two are a gather that has not landed while the last two are a gather
    that never will. Drawn identically they were the same sentence, so a corrupt
    ``gather.json`` read as `⋯ gathering this workspace's repos…` forever (#735): a panel
    "never gathers on its own — it reads the cache or says it has none" (docs/frame.md),
    so nothing was coming to correct it.

    **A fact at the moment it is asked, never a duration.** The alternative was to
    time-box the gathering message — after N seconds of no cache, say something else —
    and that is a guess wearing a fact's clothes: a plane with forty clones on a cold
    mount crosses any N that a corrupt file crosses, and the pane would call a slow
    gather broken. This asks the filesystem instead, of the same file the caller has just
    failed to read, and :func:`save`'s ``os.replace`` is what makes the answer stable —
    "a reader must never observe a half-written cache", so there is no window in which a
    gather that IS running looks like this.

    Asked only when :func:`cached` has already answered ``None``, which is where the two
    compose into three states. It is not the negation of that call and must not be used
    as one: a cache that reads perfectly well is ``False`` here for the same reason a
    frame that has never gathered is.
    """
    return _load(fid)[1]


def row_count(fid: str) -> int:
    """How many table rows this frame's repos and pieces would fill — **never a git
    sweep**, on either path.

    #488 made the table pane's HEIGHT a function of its content, which means somebody
    outside a panel has to know how much content there is: the launcher, before any panel
    exists, and `commands_frame.cmd_resize`, every time the window changes size. Both are
    on paths where cost is felt directly — a launch the operator is waiting on, and a
    hook that fires on every step of a terminal drag — so neither may call :func:`scan`.

    Two sources, and the order is the point:

    * **The cache, when there is one.** `notify.plane_changed` keeps it current, so a
      running frame's count is exactly the count its panels are drawing from.
    * **A directory listing, when there is not.** `cmd_launch` calls :func:`discard`
      before it draws anything (a recycled pid must not adopt another frame's repos —
      see that function), so the launch path reaches here with no cache BY DESIGN.
      `statusline._repo_trees` and `_detail_worktrees` are the same two calls `scan`
      itself starts with and both are filesystem-only — no subprocess, no network, no
      `git status` — so this costs an `iterdir`, not a sweep. It is the SAME pair `scan`
      uses, rather than a second way of counting repos that could disagree with the rows
      the panel then draws.

    **The listing counts the FRAME's workspace, not the asking process's** (#512).
    `state.workspace_for` is the one rule every frame surface asks — what was chosen inside
    the frame, else what the launcher recorded, else a local resolve — so the pane is sized
    from the same workspace `slots._repos` then draws. The two callers make the difference
    real: the launcher IS the process that resolved it and would agree either way, but
    `cmd_resize` runs as a tmux `run-shell` child, whose environment is the SERVER's and
    whose cwd and pane id are not the operator's — so it would size `repos` for whatever
    workspace it resolved for itself, which on the plane that reported #512 was a
    `default` holding no clones at all.

    Zero for anything that fails, which is the floor `layout.repos_rows` already
    handles: a frame whose repo count could not be established gets a one-row pane saying
    so, never a taller one full of nothing.
    """
    data = cached(fid)
    if data is not None:
        return len(data.get("repos") or []) + len(data.get("worktrees") or [])
    try:
        from .. import statusline as sl
        active = state.workspace_for(fid)
        dirs = sl._repo_trees(active)
        return len(dirs) + len(sl._detail_worktrees(active, dirs))
    except Exception:
        return 0


def discard(fid: str) -> None:
    """Forget the cached scan under *fid*, because a NEW frame is claiming the id.

    The gather half of ``state.clear_exit``'s bill, and the same recycled pid
    underneath it (#383). A frame id WAS ``<workspace>-<launcher pid>``; ``state.reap``
    keeps such a directory for as long as the pid in its name is live, and on a launch
    that pid is live because it is the launcher's own — so a launcher landing on a pid an
    earlier launcher for the same workspace already used adopted that earlier frame's
    whole directory, ``gather.json`` included. A chat's id is allocated
    (``state.new_chat_id``) and cannot collide that way, so on the launch path this is
    now belt and braces; the case it is for is Stage 5c's reopen, which relaunches into a
    cold chat's own existing directory.

    :func:`read` has no freshness check and needs none — it is the hot path a panel
    polls, and the cache is kept current by ``notify.plane_changed`` — so it hands
    back whatever parses. Nothing corrects it either: a panel repaints only on a
    ``state.version`` bump, so the dead frame's repos, branches and CI would sit on
    screen until the session's first hook fires, which may be minutes of the operator
    reading a scan from another day. Before #383 this could not happen, because
    ``reap`` had deleted the directory and :func:`read` fell through to a live
    :func:`scan`; deleting the file here puts a launch back on exactly that path
    rather than inventing a TTL, which would be the age heuristic ``reap``'s own
    docstring refuses.

    Never raises, and never creates — ``create=False``, the same rule :func:`read`
    follows, because the ordinary first launch for a workspace has no directory here
    at all and a launch must not mint one just to delete a file inside it.
    """
    f = _cache_file(fid, create=False)
    if f is None:
        return
    try:
        f.unlink(missing_ok=True)
    except OSError:
        # Same must-not-raise promise the rest of this module makes: a launch is not
        # worth failing over a file that could not be deleted. The cost of losing here
        # is a stale panel until the first bump, not a lost launch.
        return


def refresh(fid: str, *, workspace: str | None = None, cwd: str | None = None) -> dict:
    """:func:`scan` then :func:`save` — what the bumping hook (Task 2) calls.

    Returns the freshly-gathered data, so a caller that wants it immediately does
    not have to pay for a second :func:`read` right after writing it. Never
    raises: `scan` and `save` each already promise that individually, so this
    only has to not undo it by calling them any other way.
    """
    try:
        data = scan(workspace=workspace, cwd=cwd)
    except Exception:
        data = _empty(workspace)
    save(fid, data)
    return data
