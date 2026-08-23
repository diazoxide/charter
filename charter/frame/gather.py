"""One scan of the plane's repo state, cached under the frame's own directory.

``statusline.render`` performs an expensive gather before it lays anything out:
``_repo_trees``, ``_repo_states`` (one ``git status --porcelain --branch`` per
repo), ``_branch`` per tree, and ``glstate.read_for``. The frame has up to four
panels (``top``/``left``/``right``/``bottom``); if each gathered independently
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
    }


def _entry(d: Path, branch: str, states: dict, gl: dict, cur_repo: str | None) -> dict:
    """One tree's row — repo or worktree alike, the same facts
    ``statusline._tree_cells`` draws from, minus everything that is layout
    (colour, truncation, tree glyphs, presence text). A narrow-pane renderer
    reads this directly; it never re-derives dirty/ahead/behind/ci/change from
    ``states``/``gl`` itself, which is the whole point of gathering once.
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

    try:
        repos = [_entry(d, branches.get(d, "?"), states, gl, cur_repo) for d in dirs]
        repo_for_wt = dirs[0].name if dirs else None
        worktrees = []
        for w in detail_wts:
            entry = _entry(w, branches.get(w, "?"), states, gl, cur_repo)
            entry["repo"] = repo_for_wt
            worktrees.append(entry)
    except Exception:
        repos, worktrees = [], []

    return {
        "gathered_at": time.time(),
        "workspace": active,
        "current_repo": cur_repo,
        "repos": repos,
        "worktrees": worktrees,
    }


def save(fid: str, data: dict) -> None:
    """Write *data* (from :func:`scan`) as JSON under *fid*'s frame directory.

    Atomic — a temp file plus ``os.replace``, the same shape ``state.bump`` uses
    and for the same reason: a reader must never observe a half-written cache.
    Never raises: this runs from a hook (Task 2), where a failure here must
    degrade to "the cache did not update" rather than cost the session its turn.
    """
    f = _cache_file(fid, create=True)
    if f is None:
        return
    tmp = f.with_name(f.name + ".tmp")
    try:
        tmp.write_text(json.dumps(data))
        os.replace(tmp, f)
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
    corrupt or stale cache file happened to contain.
    """
    f = _cache_file(fid, create=False)
    if f is not None:
        try:
            data = json.loads(f.read_text())
        except (OSError, ValueError):
            data = None
        if _shaped_like_a_scan(data):
            return data
    try:
        return scan(workspace=workspace, cwd=cwd)
    except Exception:
        return _empty(workspace)


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
