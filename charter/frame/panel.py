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

**SIGWINCH matters because a resize does not bump the frame's version.** Only charter's
own hooks call `state.bump`; the operator resizing their terminal does not. Without a
handler, a pane sits with content painted for the OLD size until some unrelated activity
happens to bump the version next — on an idle agent, that could be a long wait, and the
pane looks broken for all of it. The handler only sets a flag; a signal handler runs
between bytecodes on the main thread, wherever the run loop happens to be, so it must not
itself call anything that could block or recurse.
"""

from __future__ import annotations

import os
import signal
import sys
import time

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


def should_redraw(seen: str, fid: str) -> bool:
    """Has the frame changed since *seen*? Never raises: a panel that dies checking a
    version file leaves a hole in the frame — the same failure `state.version` and
    `slots.render` are already built not to cause, so this must not reintroduce it on
    top of them.
    """
    try:
        return state.version(fid) != seen
    except Exception:
        return True


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


def _paint(slot: str, fid: str) -> None:
    """Clear the pane and draw *slot* whole, clamped to this pane's real row count.

    `slots.render` already clamps every line to the pane's WIDTH; clamping the line
    COUNT to the pane's HEIGHT is what this function adds on top, because `render`'s
    contract (one string) carries no notion of height at all — see the module
    docstring's "Height is this module's job" section.
    """
    lines = slots.render(slot, fid).split("\n")[:_rows()]
    sys.stdout.write("\x1b[H\x1b[2J" + "\n".join(lines))
    sys.stdout.flush()


def _install_sigwinch(resized: dict) -> object:
    """Arm the resize handler, returning whatever was installed before it so `run` can
    put it back rather than leaking a handler past this process's own lifetime — a
    real concern here specifically because `run(once=True)` is called in-process by
    tests, not only as a subprocess's whole life.
    """
    return signal.signal(signal.SIGWINCH, lambda *_a: resized.__setitem__("flag", True))


def _tick(resized: dict, seen: str, slot: str, fid: str) -> str:
    """One loop iteration's decision AND its effect — split out of `run` so the
    DECISION (paint now, or wait) can be exercised without also exercising `run`'s
    `while True`/`time.sleep`, which a test cannot call directly without either hanging
    or racing real wall-clock time.

    Reads `state.version` exactly once (`should_redraw`'s own version read is not
    reused here — it takes *seen* and *fid*, not a precomputed current value, so calling
    it as well would mean two `stat`s to decide one repaint). `should_redraw` stays the
    public, standalone answer to "has the frame changed", exercised directly by its own
    tests; this inlines the same comparison against the ALREADY-read *now* instead of
    calling it a second time.

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
    """
    now = state.version(fid)
    if resized["flag"] or now != seen:
        resized["flag"] = False
        _paint(slot, fid)
        return now
    return seen


def run(slot: str, fid: str, *, once: bool = False) -> int:
    """Run one panel: refuse an unknown slot, then paint on every version bump or
    resize until killed (`once=True` — never passed by `charter panel` itself, only by
    tests — paints exactly once and returns).

    Refuses rather than drawing an empty pane for a *slot* not in `slots.SLOTS`: an
    empty pane reads as a broken frame, and `layout.panel_argvs` can in principle be
    handed a slot name `slots.py` does not (yet) implement a renderer for (`left`/
    `right` today — see `layout.SLOT_SIZE`).
    """
    if slot not in slots.SLOTS:
        print(f"charter panel: unknown slot {slot!r} "
              f"(known: {', '.join(sorted(slots.SLOTS))})", file=sys.stderr)
        return 2

    resized = {"flag": True}  # the first pass always paints, resized or not
    old_handler = _install_sigwinch(resized)
    try:
        seen = ""
        while True:
            seen = _tick(resized, seen, slot, fid)
            if once:
                return 0
            time.sleep(TICK)
    finally:
        signal.signal(signal.SIGWINCH, old_handler)
