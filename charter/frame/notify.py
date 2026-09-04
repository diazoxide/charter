"""Telling a running frame that the plane changed.

Called from `charter/hooks.py`, where the posture is absolute (see that module's own
docstring, and `contain.py`'s): a hook may cost a session its briefing, never its turn.

**Bumping the version and refreshing the gather cache share the one debounce below,
deliberately, not two.** `gather.refresh` is the expensive half (a cold `scan()` costs
~35ms and three git invocations; `state.bump` costs one `os.replace` of a few bytes) but
they exist for the same reason: "the plane changed enough that a panel should redraw
with new facts." A panel only ever looks at the cache *because* the version it polls
moved (`panel.py`'s own contract) — a separate, longer-lived debounce for the cache would
mean the version bumps on every debounce-eligible call while the cache trails behind on
its own slower clock, and a panel would repaint on a version bump into a cache that is
`stat`-fresh but factually stale, exactly the gap this task exists to close. One shared
gate means a panel that just saw the version move always finds a cache gathered at the
same moment, not an older one.

Refresh happens BEFORE the bump, not after, for the same reason: a panel's poll loop
(`panel.TICK`) reads `state.version` first and the cache second, so if the version were
bumped first there would be a window — however small — where a poller sees the new
version and still reads the stale cache. Refreshing first closes that window instead of
leaving it to be usually-too-small-to-matter.

The scan's own expense is bounded twice over, not once: this debounce caps it at once
per 250ms, and inside that, `statusline._repo_states`' 5-second TTL caps the actual `git
status` subprocesses far below even that — so in a burst of tool calls closer together
than 250ms apart, only one in the burst pays anything beyond the cheap
`now - _last["at"] < DEBOUNCE` check, and even that one is usually paying for cache-hit
work (~0.3ms), not a cold sweep.

**Every `posttooluse*` handler calls this, not just the bare-named `posttooluse`.**
`hooks/hooks.json` scopes `posttooluse` itself to `Write|Edit|MultiEdit` — Bash, Skill,
Task/Agent and SendMessage each route to their own handler (`posttooluse-bash`,
`-skill`, `-dispatch`, `-message`). Relying on `posttooluse` alone would leave the frame
blind to Bash specifically, which is where most plane-state changes that matter to a
panel actually happen — commits, branch moves, worktree edits — none of them a
Write/Edit/MultiEdit call. Every handler bumping, rather than picking the ones that seem
to matter today, is the one rule simple enough that a future sixth `posttooluse-*`
handler has an unambiguous answer for whether it should call this too (yes). Each call
site sits behind the same 250ms debounce below, so calling from five handlers instead of
one costs nothing extra on the common path — it only changes which single call in a
quiet stretch is the one that survives the debounce and actually writes.

Being called from five hot paths instead of one changes nothing about what this module
owes them: never raise, and never cost any of them anything worth measuring.

**:func:`plane_changed` reaches exactly ONE frame, and :func:`plane_changed_everywhere` is
the other half** (#886). A hook fires inside the frame it belongs to and `$CHARTER_SESSION_ID`
names that frame, which is right for a hook and wrong for everything else: an agent cloning
inside chat `alpha.1` refreshed `alpha.1` and left sibling `alpha.2` drawing the same
workspace and the same repo list, stale, with nothing coming to correct it — and a `charter`
command typed in a plain terminal, which fires no hook at all, reached none of them. The
precedent is `state.rename_workspace`, an out-of-frame CLI command that already scans the
frame root and bumps every frame it touched, and whose docstring is the argument for this
whole subject: *"a record written in silence is a panel that goes on contradicting it for
as long as the frame is idle."* That precedent is bump-only; the function below adds the
refresh half, because a panel does not sweep (#512).

A FIFO was considered and rejected: opening one for write blocks until a reader exists,
which would put a hang directly in the hook path the first time no panel was listening.
`state.bump`'s plain version-file-plus-`stat` shape exists because it cannot hang, and
this module rides on top of it rather than inventing a second channel.

The debounce is a plain module-level dict rather than a `threading.Lock`-guarded
counter: each hook invocation is a fresh short-lived `python3 -m charter hook ...`
process (see `hooks.dispatch`), so there is never a second thread in here to race —
only ever a second *call*, later in the same process, when a handler is exercised
directly (as the tests below do).
"""

from __future__ import annotations

import os
import time

from . import state

#: At most one bump per this many seconds. A panel ticks at 0.2s (`panel.TICK`), so a
#: tighter debounce buys nothing a reader could ever see — it would only add cost to a
#: hot path (every `posttooluse*` handler, so once per Bash/Write/Edit/MultiEdit/Skill/
#: Task/Agent/SendMessage call) for no visible benefit.
DEBOUNCE = 0.25

#: Mutable through a dict, not a bare module global, so a test can reset it
#: (`notify._last["at"] = 0.0`) without `global` statements on both sides.
_last = {"at": 0.0}


def plane_changed() -> None:
    """Bump the frame this process is running inside, if any, and refresh its
    gather cache so panels stay pure readers. Never raises.

    A no-op outside a frame (`$CHARTER_SESSION_ID` unset — the common case, most
    sessions run with no frame at all) and a no-op inside the debounce window. Both
    checks happen before `gather` is even imported, so the common "no frame" path never
    touches the gather module at all, let alone gathers anything.

    The `gather.refresh` call is wrapped in its OWN `try/except`, separate from the
    outer one: `state.bump` must still run — and the version must still move — even if
    the refresh fails (a corrupt cache directory, a `TypeError` in some future field),
    because bumping the version is this function's original, load-bearing promise and a
    test (`test_a_change_bumps_the_running_frame`) already pins it. Losing a cache
    refresh degrades a panel to what it already shows; losing the bump would make a
    panel go stale forever. The outer `except Exception` is not defensive boilerplate
    even so: `state.bump` and `gather.refresh` already swallow their own errors, but
    this also guards `os.environ.get`/`time.monotonic`/the `gather` import itself and
    any future change to any of them — the one rule this module exists to keep is that
    NOTHING reaching it from a hook ever turns into a raised exception.
    """
    try:
        fid = os.environ.get("CHARTER_SESSION_ID")
        if not fid:
            return
        now = time.monotonic()
        if now - _last["at"] < DEBOUNCE:
            return
        _last["at"] = now
        try:
            from . import gather
            # The FRAME's workspace, not this hook process's own answer (#512). The cache
            # belongs to the frame and a panel draws it whole, so a refresh keyed to a
            # different workspace does not degrade the table, it REPLACES it — the launch
            # gathers the workspace you launched for, and the first tool call swaps in
            # another one's repos. The two answers can genuinely differ: this runs inside
            # the harness, whose cwd and pane id are its own. `state.workspace_for` is the
            # one rule every frame surface asks — an explicit `charter workspace use`
            # inside the frame first, then the launcher's recorded answer, then a local
            # resolve for a frame that predates the record.
            gather.refresh(fid, workspace=state.workspace_for(fid))
        except Exception:
            pass
        state.bump(fid)
    except Exception:
        return


def _kept_current_repo(fid: str):
    """The repo *fid*'s own cache says it is standing in — never this process's answer.

    The one field of a scan that is a fact about a READER rather than about a workspace.
    ``scan`` derives ``repos`` from `workspace.clones(ws)`, which is why one scan can be
    written into every frame of a workspace; it derives ``current_repo`` from a **cwd**
    (`statusline._current`), and the cwd a fan-out has is whichever directory the operator
    happened to type `charter clone` in. Writing that into every frame is #512's rule one
    noun down — *"a refresh keyed to a different workspace than the frame it is refreshing
    is the defect, not a stale value"* — and it costs something visible: a frame whose
    harness sits in `api` loses the marked row on the repo table because somebody ran a
    command from their home directory, and gets it back only on that frame's next hook.

    So the fan-out never DECIDES which repo a frame is in. It keeps what that frame last
    recorded, and ``None`` for a frame that has recorded nothing — the same value a frame
    with no cache at all already draws, so this never invents a location either. Only a
    reader that genuinely knows a frame's cwd — the frame's own hook, through
    :func:`plane_changed` — ever moves this field.

    A plain read of the cache the fan-out is about to replace, so it must be called before
    :func:`gather.save`. `gather.cached` already answers ``None`` for every way a cache
    can be missing or unreadable and never raises, which is the whole of the fallback.
    """
    from . import gather
    data = gather.cached(fid)
    return data.get("current_repo") if data else None


def plane_changed_everywhere() -> None:
    """Refresh and bump EVERY frame on this plane — what a `charter` command that wrote
    plane state calls, ONCE, when it is done. Never raises.

    The out-of-frame half of :func:`plane_changed` (#886, and see the note in the module
    docstring). Nothing here reads `$CHARTER_SESSION_ID`: a command typed in a plain
    terminal has no frame of its own, and a command run by an agent inside `alpha.1` is
    the case where naming one frame is precisely the bug.

    **One scan per distinct workspace, not one per frame.** A cold `gather.scan()` costs
    ~35ms and three git invocations, and every frame in a workspace would get the same
    answer out of it — `scan` derives the repo list from `workspace.clones(ws)`, which is
    a fact about the workspace. So the frames are grouped, each workspace is scanned once,
    and each frame's cache is written from the shared result. The cheaper-looking
    alternative — bump every frame and let each panel gather for itself — is the one thing
    #512 forbids outright: *"a panel does not sweep."*

    **`state.own_workspace`, never `state.workspace_for`.** That second function's top
    rung is `$CHARTER_WORKSPACE` **out of this process's own environment**, which is the
    frame's environment when a hook calls it and is a stranger's when a terminal does — so
    a fan-out asking it would file every frame on the plane under whatever the operator
    happened to have exported and then overwrite each of their caches with that one
    workspace's repos. `own_workspace` is the frame's own answer (the pin the LAUNCH
    recorded, then the launch record), which is the reading `chats.of_workspace` uses for
    membership. A frame that answers ``None`` says nothing about its workspace, and there
    is no rung under it a terminal could honestly stand on, so it is bumped and not
    refreshed — `rename_workspace`'s bump-only outcome exactly, and no worse than today.

    **Refresh, then bump — every cache written before any version moves.** A panel's poll
    loop reads `state.version` first and the cache second, so a bump that lands ahead of
    its own cache opens a window where a poller sees the new version and re-reads the old
    facts. The two loops are separate rather than interleaved per workspace so that the
    property holds across the WHOLE fan-out and not just within one group: with two
    workspaces on the plane, an interleaved version would bump the first group and then
    spend 35ms scanning the second, which is a real window and not a theoretical one.

    **No liveness check, deliberately.** Every frame directory is bumped, dead ones
    included, exactly as `rename_workspace` does: a bump into a dead frame is a few bytes
    `state.reap` will remove, while `state.is_live` costs a tmux subprocess and — for a
    chat — cannot even answer without a pane a terminal command does not have.

    **Called once per command, not once per unit of work.** Cloning ten repos is one
    fan-out. :data:`DEBOUNCE` cannot help here and is deliberately not consulted: it is
    per hook *process*, and a CLI command is a single process that would have to debounce
    against itself. The discipline is at the call site — notify at completion — which is
    also what the operator sees, a list arriving complete rather than growing a row at a
    time.

    Best-effort at every step, like everything else in this module. A frame root that
    cannot be listed is nothing to fan out to; a workspace whose scan fails leaves its
    frames' caches exactly as they were rather than blanking them, and they are still
    bumped.

    **Not debounced, and no cheap `$CHARTER_SESSION_ID` guard in front of it either.**
    Both of :func:`plane_changed`'s early exits are properties of a hook that fires
    thousands of times a session, and neither is true of a command an operator typed once.
    """
    try:
        from . import gather
        # No `is_dir()` filter, for `rename_workspace`'s reason: nothing found here is
        # handed to tmux, and a loose FILE called `api.2` in the frame root answers `None`
        # from `own_workspace` and is refused by `frame_dir`'s mkdir on the way into both
        # writers. A guard that cannot change an outcome is one that gets deleted later for
        # the wrong reason. And no `except OSError` of its own around the scan, unlike
        # `rename_workspace`: that function has no outer `try`, this one does, and a second
        # handler whose only reachable behaviour is the outer one's is a line the deletion
        # sweep is right to call equivalent. A frame root that cannot be listed — or that
        # goes unreadable part-way through, `os.scandir` being lazy — is nothing to fan out
        # to either way.
        fids = [e.name for e in os.scandir(state._root())]
        by_workspace: dict[str, list[str]] = {}
        for fid in fids:
            ws = state.own_workspace(fid)
            if ws:
                by_workspace.setdefault(ws, []).append(fid)
        for ws, group in by_workspace.items():
            try:
                data = gather.scan(workspace=ws)
            except Exception:
                # `scan` promises not to raise and each of its steps is wrapped
                # individually, so this is the future-change guard rather than a path with
                # a known trigger. What it decides is which way the failure falls: skip the
                # group and every frame in it keeps the facts it already had, where
                # `gather.refresh`'s `_empty` fallback would blank a whole workspace's
                # tables at once. One frame degrading itself is not N frames degraded by a
                # command that ran somewhere else.
                continue
            for fid in group:
                gather.save(fid, {**data, "current_repo": _kept_current_repo(fid)})
        for fid in fids:
            state.bump(fid)
    except Exception:
        return
