"""One charter panel, owning one tmux pane.

Repaints whole, never diffed: a five-row pane is a few hundred cells, so diffing would be
optimising something that is already free, and `tui.py` already truncates rather than
wraps when the pane is narrow — there is no partial-line state to reconcile between one
paint and the next.

**Liveness is a poll, not a push.** A panel does not learn charter did something; it
notices `state.version(fid)` changed. `version` is a `stat`, which is why watching a
frame costs nothing at idle (see `state.py`'s own docstring). A FIFO was designed and
rejected for the same job: opening one for write blocks until a reader exists, which
would put a hang inside the hook path that calls `state.bump` — a cost this feature must
never impose on the agent turn that triggers a redraw.

**Width is `slots.py`'s job, not this module's.** `slots.render` already measures the
pane's own tty (`os.get_terminal_size(sys.stdout.fileno())`) rather than trusting
`$COLUMNS` — which a panel process, started as a tmux pane command, inherits WHOLE from
the launching shell (measured: a 22-column pane whose launcher had exported
`COLUMNS=200` saw `COLUMNS='200'` in its own environment). See `charter/frame/slots.py`'s
module docstring for the full measurement. Duplicating that logic here would just be a
second place to get it wrong; `_paint` below calls `slots.render` and trusts its output
is already clamped to the pane's width.

**Height is this module's job.** `render()`'s contract is a single string, so nothing
downstream of it knows how many ROWS the pane it is about to overwrite actually has. A
`top`/`bottom` panel is one row today (`layout.SLOT_SIZE`), but `left`/`right` are
full-height panes the layout module already supports — assuming "one line" here would
silently clip a future multi-line renderer to its first row, or (the opposite failure)
let it emit more lines than the pane holds and scroll THAT PANEL'S OWN rows — measured
against real tmux: each pane keeps its own scroll region, so an over-height paint pushes
only its own top line out of view and leaves every sibling pane untouched, not the whole
frame. `_rows` measures the pane the same way `slots._width` measures it, and `_paint`
clamps the LINE COUNT to that measurement the way `tui.truncate` already clamps each
line's WIDTH.

**Animation is scoped to work that is actually in flight, and idle stays one `stat`.**
A panel repaints on a version bump, and a dispatch STARTING does not bump anything — the
version moves from `posttooluse*` hooks, and a session that has handed work to a sub-agent
is by definition not making tool calls while it waits. So the one thing the frame animates
(`slots._inflight_field`) needs the panel to repaint on its own clock, and only while
there is something to animate. `_running` below is the whole mechanism: `inflight.stamp()`
is a single `stat` of the tracker's directory, and the records are re-read only when that
number moves or when the earliest presumed-dead deadline passes. With nothing in flight —
the directory absent, or present and empty — that is one syscall per tick, alongside the
one `state.version` already pays, and the panel paints nothing. It is self-limiting by
construction rather than by a budget: the ticking stops when the records do.

**SIGWINCH matters because a resize does not bump the frame's version.** Only charter's
own hooks call `state.bump`; the operator resizing their terminal does not. Without a
handler, a pane sits with content painted for the OLD size until some unrelated activity
happens to bump the version next — on an idle agent, that could be a long wait, and the
pane looks broken for all of it. The handler only sets a flag; a signal handler runs
between bytecodes on the main thread, wherever the run loop happens to be, so it must not
itself call anything that could block or recurse.

**A panel that fails does not exit — it paints why and holds the pane open.** This is
the half `slots.render`'s never-raise promise cannot reach from where it sits: it
guards what a RENDERER raises, and a panel that dies on the way to it (an unknown slot,
a crash in this module's own poll, a `tui` helper blowing up) bypasses that guarantee
entirely, leaving the operator a hole in the frame with nothing in it to read. Writing
the reason to stderr does not fix that, and the measurement is why (real tmux 3.7c,
`remain-on-exit on`): tmux writes its own `Pane is dead (status N, <date>)` message by
moving to the pane's LAST row and issuing a linefeed first, which scrolls the pane up by
exactly one line — in a six-row pane the first of three stderr lines is lost and the
rest survive, but `top` and `bottom` are ONE row (`layout.SLOT_SIZE`), so that one
scrolled line is the whole pane and `Pane is dead (status 2)` is provably all that is
left. It cost a real debugging session, whose only way through was running the panel's
argv by hand outside tmux. A pane whose process is still ALIVE keeps what it painted
(measured the same way), so `_hold` paints the reason and then simply does not return.

A panel that is held is also a panel tmux never sees die, so the respawn hook
`commands_frame._panel_died_hook_argv` installs never fires for it — deliberately: the
two mechanisms divide at exactly the line between a failure charter's own Python can
see (painted here, once, and left readable) and one it cannot (the interpreter itself
failing to start, a SIGKILL), which is the only kind respawning could ever help.

**Holding rather than exiting into that hook is a decision, so here is the argument.**
Exiting would let a crashed panel be retried three times, which sounds strictly better
until you ask what can actually reach the handler: `slots.render` catches everything a
renderer raises, `state.version` catches everything a read raises, and `_rows` catches
its own `OSError` — so what is left is a genuine bug in charter, or a pane whose fd has
gone away. Neither is transient, so all a retry buys is three more identical crashes,
and the cost is certain: three deaths, and then tmux's own message scrolling the reason
out of a one-row pane — the exact failure this whole section exists to end. The pane
that HAS gone away needs no special case either; `_hold`'s own write raises in turn, the
process dies for real, and the respawn hook takes it from there.
"""

from __future__ import annotations

import os
import signal
import sys
import time

from .. import tui
from . import slots, state

#: How often the version file is checked when nothing else has woken this panel. A
#: `stat` at this rate is indistinguishable from zero cost (see `state.version`'s own
#: docstring) — and unlike a FIFO, polling cannot hang the hook that writes the version
#: file waiting for a reader that may never arrive.
TICK = 0.2

#: Rows assumed when this pane's own tty cannot be measured at all — stdout piped to
#: something with no tty behind it (`charter panel top --session x > /tmp/log`, run by
#: hand for debugging, or a test). Matches `commands_frame._FALLBACK_SIZE`'s own row
#: count: the same "traditional default screen" charter already falls back to elsewhere
#: when a terminal's real size is unknowable.
_DEFAULT_ROWS = 24


def _rows() -> int:
    """This pane's own height, measured the way `slots._width` measures its own width:
    `os.get_terminal_size(sys.stdout.fileno())` asks the file descriptor this process is
    actually writing to, which for a panel launched as a tmux pane command IS the pane —
    not a pipe, not the launching terminal. Only when that raises (no tty behind the fd
    at all) does this fall back to `_DEFAULT_ROWS`.
    """
    try:
        return os.get_terminal_size(sys.stdout.fileno()).lines
    except OSError:
        return _DEFAULT_ROWS


def _write(text: str) -> None:
    """Clear the pane and put *text* in it, clamped to this pane's real row count.

    The one place anything reaches the pane's screen, shared by the ordinary paint and
    by the failure paint below — so a panel saying why it stopped goes through exactly
    the same clear-then-write discipline as a panel doing its job, rather than a second
    hand-rolled write that could leave half of tmux's own dead-pane text on screen
    beside it.
    """
    lines = text.split("\n")[:_rows()]
    sys.stdout.write("\x1b[H\x1b[2J" + "\n".join(lines))
    sys.stdout.flush()


def _paint(slot: str, fid: str) -> None:
    """Clear the pane and draw *slot* whole, clamped to this pane's real row count.

    `slots.render` already clamps every line to the pane's WIDTH; clamping the line
    COUNT to the pane's HEIGHT is what this function adds on top, because `render`'s
    contract (one string) carries no notion of height at all — see the module
    docstring's "Height is this module's job" section.
    """
    _write(slots.render(slot, fid))


def _hold(reason: str, *, once: bool, rc: int) -> int:
    """Paint *reason* into the pane and then stay alive, so it can still be read.

    The whole of this module's answer to a panel that cannot run: **returning is the
    bug**. A panel process that exits hands its pane to `remain-on-exit`, and tmux then
    scrolls the pane by exactly one line to write `Pane is dead (status N, <date>)` over
    it — which in the one-row `top`/`bottom` panes is the entire pane (measured against
    real tmux 3.7c; see the module docstring). So the reason is painted and the process
    simply does not leave, which is the only state in which a pane keeps what was
    written to it.

    Reaches no renderer on purpose — the failure being reported may BE a failure of
    that path, and a message about a broken renderer must not need the renderer to
    work. It does still measure through `slots._width`, which is deliberate and not an
    exception to that: measuring is one `os.get_terminal_size` call with its own
    `OSError` fallback, it is the module docstring's stated reason for not owning width
    here at all, and an unclamped line wraps in a 22-column `left` pane and scrolls
    itself out of the pane it was written to be readable in.

    *rc* is what `run` returns when it cannot hold — `once=True`, which only tests pass
    (`charter panel` itself never does). The distinction matters for exactly one reader:
    a test can still assert a panel REFUSED a bad slot, without the production path
    having to exit to say so.
    """
    _write(tui.truncate(f" charter: {reason}", slots._width()))
    while not once:
        # Not `signal.pause()`: SIGWINCH is armed for the ordinary loop and would wake
        # this one on every resize into a tight spin. A tick is the same idle cost the
        # live loop already pays (see `TICK`), and there is nothing here to recompute.
        time.sleep(TICK)
    return rc


def _new_inflight_cache() -> dict:
    """The state :func:`_running` carries between ticks. A dict, matching `resized`'s own
    shape in this module, so a test can build one and inspect it without a class.

    ``stamp`` starts at ``None``, which is also what `inflight.stamp()` answers for "no
    such directory" — and the collision is harmless rather than overlooked. A separate
    "never read yet" sentinel was written here first and then removed for being a claim no
    test could falsify: the two states agree on the only thing this cache reports. `None`
    from `stamp()` means the tracker directory does not exist, so nothing can be in
    flight, and ``running: 0`` is already the right answer without reading anything. Every
    state in which the answer is NOT 0 has a real ``st_mtime_ns`` behind it, which never
    compares equal to ``None``, so the first tick of a panel that starts while a dispatch
    is already running does read — which is exactly how a respawned panel comes back.
    """
    return {"stamp": None, "running": 0, "recheck": 0.0}


def _running(cache: dict) -> int:
    """How many dispatches are in flight right now — one `stat` when nothing has changed.

    The idle cost of the whole animation, and the reason it can be on by default. The
    expensive answer (`inflight.live_records()`: open the directory, read every entry,
    parse each one's JSON) is recomputed only when one of two things is true:

    * the tracker's directory mtime moved — a record was created or removed, the only two
      events that can change the set (`inflight.stamp`'s own docstring);
    * the earliest presumed-dead deadline has passed — the one way this answer changes
      with NO file changing, since `presumed_dead` is measured against the clock. Only
      consulted while something is actually running, so an idle panel never computes a
      deadline, never stores one, and never compares against one.

    Counts RUNNING records only, presumed-dead ones excluded, and that is what stops a
    killed dispatch from spinning a panel for the rest of the day: `inflight` keeps such a
    record for `PRUNE_SECONDS` (24 hours) precisely so it stays visible, and
    `slots._inflight_field` does still draw it — statically, with `⋯`. Animating it would
    claim progress that stopped hours ago, on an otherwise idle machine.

    Never raises: this sits in a panel's run loop, where an exception ends the pane
    (`run`'s own `_hold` catches it, but a panel that stops repainting has already lost).
    A tracker charter cannot read is "nothing in flight", which degrades to the stillness
    this feature's whole promise is about.
    """
    from .. import inflight
    try:
        stamp = inflight.stamp()
        stale = cache["running"] and time.time() >= cache["recheck"]
        if stamp == cache["stamp"] and not stale:
            return cache["running"]
        records = inflight.live_records()
        cache["stamp"] = stamp
        cache["running"] = sum(1 for _a, _t, dead in records if not dead)
        cache["recheck"] = min((t for _a, t, dead in records if not dead),
                               default=0.0) + inflight.PRESUMED_DEAD_SECONDS
        return cache["running"]
    except Exception:
        # The stamp is deliberately NOT reset here. `cache["stamp"]` is only ever assigned
        # after a read that completed, so a read that raised has left it describing the
        # last directory state charter actually understood — and the next change to the
        # set of records moves it, which re-reads. Resetting it instead would retry a
        # raising read on every tick, five times a second, for as long as the fault
        # lasted. Reporting 0 is the safe direction either way: it means stillness, which
        # is this feature's own promise for "charter does not know of any work".
        cache["running"] = 0
        return 0


def _install_sigwinch(resized: dict) -> object:
    """Arm the resize handler, returning whatever was installed before it so `run` can
    put it back rather than leaking a handler past this process's own lifetime — a
    real concern here specifically because `run(once=True)` is called in-process by
    tests, not only as a subprocess's whole life.
    """
    return signal.signal(signal.SIGWINCH, lambda *_a: resized.__setitem__("flag", True))


def _tick(resized: dict, seen: str, slot: str, fid: str, *,
          animating: bool = False) -> str:
    """One loop iteration's decision AND its effect — split out of `run` so the
    DECISION (paint now, or wait) can be exercised without also exercising `run`'s
    `while True`/`time.sleep`, which a test cannot call directly without either hanging
    or racing real wall-clock time.

    Reads `state.version` exactly once, and the comparison is inline. A separate
    `should_redraw(seen, fid)` predicate lived beside this for a while — the plan's own
    Task 7 contract named it — but it took *seen* and *fid* rather than a precomputed
    current value, so this function could never use it without paying a second `stat` to
    decide one repaint, and nothing in production ever called it. It has been deleted
    rather than documented: a public helper with no caller is a claim about an interface
    nobody has.

    A resize repaints even when the frame's own version has not moved: comparing versions
    alone would leave a pane showing content laid out for a size that no longer exists
    until the next unrelated version bump happened to come along — see the module
    docstring's SIGWINCH section.

    *now* is read BEFORE `_paint` runs, not after — deliberately the direction that errs
    safe. `_paint` calls `slots.render`, which reads several independent pieces of live
    state (workspace, todos, alerts) one at a time, not atomically; a second bump landing
    while that read is in flight could leave the painted content reflecting only the
    OLDER state. Recording the version from after the paint would then mark that newer
    version "seen" even though nothing on screen actually reflects it, and the next
    tick's comparison would see no difference and stay silent — a missed repaint with
    nothing left to trigger a correction. Reading first means `seen` can only lag behind
    (or exactly match) what was actually painted, so any bump during or after the paint
    is still visible to the next comparison — pinned directly by
    `Tick.test_a_bump_landing_during_the_paint_is_not_marked_seen`.

    *animating* is the third reason to repaint, and it is the same shape as *resized*: the
    frame's version has not moved and the pane's size has not changed, but what the panel
    would draw NOW differs from what is on screen, because the spinner is on a different
    frame (`slots.spinner_frame` reads the clock). Defaulted to `False` so the two callers
    that mean "repaint only on news" — every test in this module that exercises the
    decision directly — keep saying exactly that. `_watch` is what decides it, from
    :func:`_running`.
    """
    now = state.version(fid)
    if resized["flag"] or now != seen or animating:
        resized["flag"] = False
        _paint(slot, fid)
        return now
    return seen


def _watch(slot: str, fid: str, *, once: bool) -> int:
    """The live loop: paint on every version bump, resize, or spinner frame, until killed.

    The third reason is the only one that repeats on its own, and it is bounded twice over
    rather than by a timer this module owns: by the WORK, since `_running` answers 0 the
    moment the last in-flight record clears; and by the SLOT, since only a renderer in
    `slots.ANIMATED` draws anything that moves. A `top`, `left` or `right` panel therefore
    behaves exactly as it did before this feature existed — including paying no `stat`,
    because the `and` below short-circuits before `_running` is ever called.

    Split out of `run` so the SIGWINCH handler it arms is restored by its own `finally`
    before `run`'s failure path can hold the pane open — a handler left installed for
    the rest of a held process's life would keep waking a loop that has nothing left to
    repaint, and `RunOnceLoop.test_once_true_restores_the_previous_sigwinch_handler`
    already pins that this module leaks no handler past its own work.
    """
    resized = {"flag": True}  # the first pass always paints, resized or not
    inflight_cache = _new_inflight_cache()
    # Scoped to THIS slot, and the `and` short-circuits: a panel whose renderer draws
    # nothing that moves never repaints for the spinner and never even pays the `stat`
    # that would have told it to. `_watch` runs one process per slot, so an unscoped
    # check repaints all four for the length of every dispatch — three of them redrawing
    # byte-identical output, and `right` costs 4.8ms a render to do it (see
    # `slots.ANIMATED`).
    animates = slot in slots.ANIMATED
    old_handler = _install_sigwinch(resized)
    try:
        seen = ""
        while True:
            seen = _tick(resized, seen, slot, fid,
                         animating=animates and bool(_running(inflight_cache)))
            if once:
                return 0
            time.sleep(TICK)
    finally:
        signal.signal(signal.SIGWINCH, old_handler)


def run(slot: str, fid: str, *, once: bool = False) -> int:
    """Run one panel: refuse an unknown slot, then paint on every version bump or
    resize until killed (`once=True` — never passed by `charter panel` itself, only by
    tests — paints exactly once and returns).

    Refuses rather than drawing an empty pane for a *slot* not in `slots.SLOTS`: an
    empty pane reads as a broken frame, and `layout.panel_argvs` can in principle be
    handed a slot name `slots.py` does not (yet) implement a renderer for (`left`/
    `right` today — see `layout.SLOT_SIZE`).

    **Neither refusal nor a crash exits.** Both end in `_hold`, which paints the reason
    into the pane and stays there, because a panel process that returns hands its pane
    to tmux's own `Pane is dead (status N)` message — which scrolls a one-row `top`/
    `bottom` pane clean (measured; see the module docstring). The stderr line is kept
    beside the paint rather than replaced by it: it is the only trace left when this is
    run by hand for debugging with stdout redirected (`charter panel top --session x >
    /tmp/log`), the case `_DEFAULT_ROWS` already exists for, and inside a real pane the
    paint's own `\\x1b[2J` wipes it a moment later anyway.

    `except Exception`, matching `slots.render`'s own guard and for the same reason —
    `KeyboardInterrupt` and `SystemExit` are how this process is MEANT to end, and
    swallowing either would hold a pane open against the operator killing it.
    """
    if slot not in slots.SLOTS:
        reason = (f"unknown slot {slot!r} "
                  f"(known: {', '.join(sorted(slots.SLOTS))})")
        print(f"charter panel: {reason}", file=sys.stderr)
        return _hold(reason, once=once, rc=2)

    try:
        return _watch(slot, fid, once=once)
    except Exception as e:
        reason = f"{slot} panel stopped ({type(e).__name__}: {e})"
        print(f"charter panel: {reason}", file=sys.stderr)
        return _hold(reason, once=once, rc=1)
