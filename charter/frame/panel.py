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

**Clamping the height is this module's job; MEASURING it is not, any more.**
`render()`'s contract is a single string, so nothing downstream of it knows how many ROWS
the pane it is about to overwrite actually has. Assuming "one line" here would silently
clip a multi-line renderer to its first row, or (the opposite failure) let it emit more
lines than the pane holds and scroll THAT PANEL'S OWN rows — measured against real tmux:
each pane keeps its own scroll region, so an over-height paint pushes only its own top
line out of view and leaves every sibling pane untouched, not the whole frame. `_paint`
clamps the LINE COUNT the way `tui.truncate` already clamps each line's WIDTH.

The measurement itself moved to `slots._height` with #488, beside `slots._width` and for
the same reason width already lived there: a renderer needs it now. `repos` is sized to
its content and draws as much of the repo table as its pane holds, choosing which rows to
spend on through `statusline._pick_rows` — a clamp applied after the fact would cut the
table at whatever came last, which is the unranked slice that ranking exists to prevent.
So the renderer measures first and this clamp goes back to being a safety net.

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
rest survive, but `top` and `bottom` are ONE row each (`layout.SLOT_SIZE`) and `repos` is
workspace holds no clones (its own floor, since #488), so that one scrolled line is the
whole pane and `Pane is dead (status 2)` is provably all that is left. It cost a real debugging session, whose only way through was running the panel's
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
renderer raises, `state.version` catches everything a read raises, and `_rows` (through
`slots._height`) catches its own `OSError` — so what is left is a genuine bug in charter,
or a pane whose fd has gone away. Neither is transient, so all a retry buys is three more identical crashes,
and the cost is certain: three deaths, and then tmux's own message scrolling the reason
out of a one-row pane — the exact failure this whole section exists to end. The pane
that HAS gone away needs no special case either; `_hold`'s own write raises in turn, the
process dies for real, and the respawn hook takes it from there.
"""

from __future__ import annotations

import signal
import sys
import time

from .. import contain, tui
from . import chrome, slots, state

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
#:
#: Re-exported from `slots` rather than declared here, because #488 gave a RENDERER a
#: reason to ask the same question (`repos` chooses which repo rows to spend its pane
#: on, so it has to know how many it has) and two copies of a fallback are two answers
#: to "how tall is a pane nobody can measure" — one of which would eventually move.
_DEFAULT_ROWS = slots._DEFAULT_ROWS


def _rows() -> int:
    """This pane's own height — `slots._height()`, the same one the renderer measures
    with, for exactly the reason this module already asks `slots` for the WIDTH.

    It was implemented here first, and correctly, back when height was purely this
    module's clamp. #488 made it a renderer's question too (`slots._repos` decides how
    many repo rows to draw), and a second `os.get_terminal_size` with its own `OSError`
    fallback beside the first is how the two come to disagree about a pane neither can
    measure. Kept as a named function rather than inlined: `_write` reads better for it,
    and this is where the module docstring's "Height is this module's job" section points.
    """
    return slots._height()


def _write(text: str) -> None:
    """Clear the pane and put *text* in it, clamped to this pane's real row count.

    The one place anything reaches the pane's screen, shared by the ordinary paint and
    by the failure paint below — so a panel saying why it stopped goes through exactly
    the same clear-then-write discipline as a panel doing its job, rather than a second
    hand-rolled write that could leave half of tmux's own dead-pane text on screen
    beside it.

    **The clear is prefixed with a reset, because `\\x1b[2J` erases with whatever
    attributes are currently set.** A renderer that leaves a background on — a
    provider's, or charter's own after a future edit — makes the NEXT repaint's
    clear-screen fill the whole pane with it. Measured in a real 20-column pane: after a
    paint that omitted its reset, the following `\\x1b[H\\x1b[2J` + content came out with
    row 0 carrying the leaked background and every other row filled with it, and nothing
    in the session ever cleared it. Constraint 4 still holds — it costs that pane and no
    other — but it cost it for the rest of the session rather than for one paint. One
    escape, on a path that already writes two.

    It goes BEFORE the cursor-home rather than after the clear, so
    ``split("\\x1b[2J", 1)[1]`` still answers the content: four test call sites read the
    pane that way (`tests/test_frame_panel.py:129,172,193`,
    `tests/test_component_id_is_the_currency.py:110`).

    **And this is where `NO_COLOR` is honoured** (`chrome.colour_ok`), because it is the
    one place anything reaches a pane's screen — so a component charter did not write is
    covered by the same answer as a renderer charter did, and neither has to ask. The
    strip is `tui.strip_ansi` per LINE and never over the whole write: `sanitize` drops
    every CSI that is not SGR, so stripping the assembled string would delete the
    clear-screen that makes a repaint a repaint.
    """
    lines = text.split("\n")[:_rows()]
    if not chrome.colour_ok():
        # No SGR at all, the reset included: there is nothing to reset when nothing is
        # painted, and "no colour on the operator's screen caused by charter" is the
        # promise rather than "no colour except charter's own housekeeping".
        return _out("\x1b[H\x1b[2J" + "\n".join(chrome.plain(ln) for ln in lines))
    _out("\x1b[m\x1b[H\x1b[2J" + "\n".join(lines))


def _out(payload: str) -> None:
    """The write itself — one statement, so the two branches above cannot come to
    disagree about flushing."""
    sys.stdout.write(payload)
    sys.stdout.flush()


def _paint(slot: str, fid: str) -> None:
    """Clear the pane and draw *slot* whole, clamped to this pane's real row count.

    `slots.render` already clamps every line to the pane's WIDTH; clamping the line
    COUNT to the pane's HEIGHT is what this function adds on top, because `render`'s
    contract (one string) carries no notion of height at all — see the module
    docstring's "Height is this module's job" section.
    """
    _write(slots.render(slot, fid))


def _component_painter(reg, cid: str):
    """A paint function for *cid*, drawn out of *reg* — `_paint`'s shape for a component.

    **The registry is built once and closed over, not rebuilt per tick.** A panel drawing
    a provider's component repaints on the same clock as any other, and rebuilding would
    re-scan the installed distributions five times a second for an answer that cannot
    change inside one process — the kind of cost §4's whole budget argument exists to keep
    off the repaint path.

    The *name* the loop hands back is ignored: `_watch` carries the name it was started
    with, and what this draws was decided once, when `run` resolved that name. Taking the
    argument and not reading it keeps one signature for both painters rather than a second
    shape for `_tick` to know about.
    """
    def paint(_name: str, fid: str) -> None:
        _write(_component_text(reg, cid, fid))
    return paint


def _component_text(reg, cid: str, fid: str) -> str:
    """*cid*'s rows for this pane, as the one string `_write` takes.

    ``ctx`` is built from what the component DECLARED (§4e), so a component that declared
    nothing costs this tick no `gather.read` at all — the idle-cost property survives a
    renderer charter did not write only if the declaration is what decides, rather than
    the snapshot being handed over and the declaration describing it afterwards.

    **Never raises**, which is `slots.render`'s promise said again for the other kind of
    renderer. Everything a stranger's code does wrong is already contained one layer down
    — `Registry.draw` catches, names the component, escapes what it returned and clips it
    to the rectangle (§4b properties 1 and 3) — so what is caught here is the ways THIS
    function can fail: a snapshot that cannot be read, a pane whose size cannot be
    measured. A panel that stops repainting has already lost the pane, whatever the
    traceback would have said.

    **Never raises means never raises an `Exception`**, and `except Exception` is the
    whole of what keeps the operator's own interrupt out of that promise —
    `KeyboardInterrupt` and `SystemExit` are `BaseException`s, which this clause does not
    reach. It used to carry an `except KeyboardInterrupt: raise` above it as well, and
    that was dead code rather than defence in depth: the clause below could never have
    caught one either way. `tools/sweep.py` proved it equivalent on `main` and #568
    removed it, because a line that cannot change an outcome is not documentation of an
    intent — it is a second, weaker answer to a question `except Exception` had already
    answered. Do not read `Registry.draw`'s identical-looking guard as the same thing:
    the clause below THAT one is `except BaseException`, so there the two lines are the
    only reason an interrupt survives a stranger's renderer at all.
    `TheOperatorsInterruptIsNotAComponentFailure` pins both halves.
    """
    from . import ctx as _ctx, gather
    try:
        c = reg.get(cid)
        snapshot = gather.read(fid) if c.needs else {}
        drew = reg.draw(cid, _ctx.build(c.needs, width=slots._width(),
                                        height=_rows(), fid=fid, snapshot=snapshot))
        return "\n".join(drew)
    except Exception as e:
        # `contain.one_line` BEFORE the width arithmetic, the order the rest of charter
        # keeps: *cid* arrived on this process's own command line, an escape sequence in
        # it is an instruction to the terminal rather than a character, and measuring
        # first would measure a string that is not what the terminal is about to do.
        return tui.truncate(
            f" charter: {contain.one_line(cid)} unavailable ({type(e).__name__})",
            slots._width())


def _hold(reason: str, *, once: bool, rc: int) -> int:
    """Paint *reason* into the pane and then stay alive, so it can still be read.

    The whole of this module's answer to a panel that cannot run: **returning is the
    bug**. A panel process that exits hands its pane to `remain-on-exit`, and tmux then
    scrolls the pane by exactly one line to write `Pane is dead (status N, <date>)` over
    it — which in a one-row pane, which `top` and `bottom` always are and `repos` is on a
    no clones, is the entire pane (measured against real tmux 3.7c; see the module
    docstring). So the reason is painted and the process
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
    """How much work is in flight right now — one `stat` when nothing has changed.

    The idle cost of the whole animation, and the reason it can be on by default. The
    expensive answer (`inflight.live_records()`: open the directory, read every entry,
    parse each one's JSON) is recomputed only when one of two things is true:

    * the tracker's directory mtime moved — a record was created or removed, the only two
      events that can change the set (`inflight.stamp`'s own docstring);
    * the earliest presumed-dead deadline has passed — the one way this answer changes
      with NO file changing, since `presumed_dead` is measured against the clock. Only
      consulted while something is actually running, so an idle panel never computes a
      deadline, never stores one, and never compares against one.

    Counts RUNNING records of EVERY kind — a clone and a `gl-refresh` move the spinner
    exactly as a dispatch does (#420) — presumed-dead ones excluded, and that is what
    stops a killed dispatch from spinning a panel for the rest of the day: `inflight` keeps such a
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
        # `kind=None`: the gate has to agree with what `slots._inflight_field` will
        # DRAW, or a clone would leave the row showing a spinner frame frozen at whatever
        # instant the last version bump happened to be (#420). The nudge's dispatch-only
        # view is a different question, asked elsewhere — see `inflight`'s own docstring.
        records = inflight.live_records(kind=None)
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
          animating: bool = False, paint=None) -> str:
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

    *paint* is WHAT to draw, defaulting to :func:`_paint` — charter's own renderer for
    this slot. A panel hosting a provider's component passes :func:`_component_painter`'s
    instead. A parameter rather than a branch inside the loop, because the choice is made
    once, where `run` resolves the name: asking "is this a component?" every tick would
    put an installed-distribution scan on the repaint path.
    """
    now = state.version(fid)
    if resized["flag"] or now != seen or animating:
        resized["flag"] = False
        (paint or _paint)(slot, fid)
        return now
    return seen


def _watch(slot: str, fid: str, *, once: bool, paint=None) -> int:
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
            seen = _tick(resized, seen, slot, fid, paint=paint,
                         animating=animates and bool(_running(inflight_cache)))
            if once:
                return 0
            time.sleep(TICK)
    finally:
        signal.signal(signal.SIGWINCH, old_handler)


def run(slot: str, fid: str, *, once: bool = False) -> int:
    """Run one panel: refuse a name nothing can draw, then paint on every version bump or
    resize until killed (`once=True` — never passed by `charter panel` itself, only by
    tests — paints exactly once and returns).

    **`slot` is a component NAME, and this is the link Phase 1 could not build.** A panel
    process builds no registry and knows no providers, so `charter panel acme.metrics` was
    `unknown slot` however correctly the config boundary had placed it — which is why
    `Registry.place` shipped with zero production callers. Three names reach the same
    component here: a committed slot name (`top`), the id behind it (`identity`), and a
    provider's own id, resolved through `builtins.component_id` and `slots.drawable`.

    **Charter's own components never fall through to the providers.** A built-in whose
    renderer is missing from `slots.SLOTS` is refused as it always was, rather than sent
    off to look for an installed distribution called `sidebar` — a provider must not be
    able to answer a question about charter's own component by claiming its name.

    Refuses rather than drawing an empty pane for a name nothing can draw: an empty pane
    reads as a broken frame, and `layout.panel_argvs` can in principle be handed a slot
    name `slots.py` does not (yet) implement a renderer for (`left`/`right` were, for a
    release — see `layout.SLOT_SIZE`). An id an installed provider DOES supply is never
    refused here, even when loading it fails: that is a pane saying which distribution
    failed and why (§4b property 4), and a refusal would be the silent drop instead.

    **Neither refusal nor a crash exits.** Both end in `_hold`, which paints the reason
    into the pane and stays there, because a panel process that returns hands its pane
    to tmux's own `Pane is dead (status N)` message — which scrolls a one-row `top`/
    one-row pane clean (measured; see the module docstring). The stderr line is kept
    beside the paint rather than replaced by it: it is the only trace left when this is
    run by hand for debugging with stdout redirected (`charter panel top --session x >
    /tmp/log`), the case `_DEFAULT_ROWS` already exists for, and inside a real pane the
    paint's own `\\x1b[2J` wipes it a moment later anyway.

    `except Exception`, matching `slots.render`'s own guard and for the same reason —
    `KeyboardInterrupt` and `SystemExit` are how this process is MEANT to end, and
    swallowing either would hold a pane open against the operator killing it.
    """
    if not slots.drawable(slot):
        reason = (f"unknown slot {contain.one_line(repr(slot))} "
                  f"(known: {', '.join(sorted(slots.SLOTS))})")
        print(f"charter panel: {reason}", file=sys.stderr)
        return _hold(reason, once=once, rc=2)

    from . import builtins as _builtins
    cid = _builtins.component_id(slot)
    try:
        painter = None
        if cid in _builtins.SLOT_OF:
            # One of charter's own, under either spelling: `slots.SLOTS` draws it,
            # exactly as it has since there were four names and nothing else.
            slot = _builtins.SLOT_OF[cid]
        else:
            reg = _builtins.build()
            # The one production caller `Registry.place` was written for. It does not
            # propagate a provider's failure: one that cannot be loaded becomes a standin
            # component holding this pane and drawing the reason, which is what makes a
            # missing or broken provider a message rather than a hole (§4b). Inside the
            # `try` all the same — this is a stranger's distribution metadata being read,
            # and the promise this function makes is that a panel PAINTS why it stopped.
            reg.place(cid)
            painter = _component_painter(reg, cid)
        return _watch(slot, fid, once=once, paint=painter)
    except Exception as e:
        reason = f"{contain.one_line(slot)} panel stopped ({type(e).__name__}: {e})"
        print(f"charter panel: {reason}", file=sys.stderr)
        return _hold(reason, once=once, rc=1)
