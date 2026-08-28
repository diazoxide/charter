"""The shape charter's contract does not describe: Textual owning the pane.

`render(ctx)` starts a real Textual app on the panel process's own tty — alternate
screen, its own asyncio loop, its own SIGWINCH handler, its own mouse request — and does
not return until the app exits. For as long as it runs, the pane is Textual's: charter's
`panel._watch` loop is parked inside `Registry.draw`, `panel._write` never fires, and
nothing clears the screen underneath it.

**This is the only shape in which the interesting half of the experiment exists.** Keys,
the `DataTable` cursor, wheel scrolling and click-to-select are all real here and all
absent from the adapter, because `Component.events` reaches no dispatcher in charter and
a headless app has no terminal to read from.

**And it costs exactly one thing, which the footer clock puts on screen.** `render` is
called once, with one `ctx`, and a `ctx` is one repaint's snapshot with no way to ask for
another (`frame/ctx.py`: "refreshing is the frame's decision, on the frame's clock"). By
never returning, this component makes that decision unreachable: charter would hand it a
fresh snapshot on the next version bump, and there is no next call. The table is frozen
at the moment the pane started.

``CHARTER_TEXTUAL_LIVE_REFRESH=1`` is the measurement of what un-freezing it would take,
and it is deliberately NOT the default: it makes the app import `charter.frame.state` and
`charter.frame.gather` and poll them itself. That works, and it is a provider reaching
around the seam it was given — it re-reads the plane on its own clock, pays its own
`gather.read`, and charter's "one snapshot, one timestamp" property no longer holds
across the frame. Shipping it on by default would be answering finding #1 by pretending
it is not there.
"""

from __future__ import annotations

import os
import time

from .ui import ReposApp

#: The env var that turns on the out-of-contract refresh. Read at render time rather than
#: at import: a panel is a process per pane, and an operator flipping this wants the next
#: pane to see it, not the next release.
REFRESH_VAR = "CHARTER_TEXTUAL_LIVE_REFRESH"

#: **Experiment apparatus, and it is here rather than in a second package on purpose.**
#: The question "what does a Textual app crashing cost — its own pane, or more?" cannot be
#: answered by reasoning about `Registry.draw`; it has to be answered by crashing one
#: inside a real tmux pane and looking at the other panes. A fault injector reachable only
#: from an env var nobody sets is the smallest thing that makes that repeatable.
#:
#: ``render`` raises before the app starts; ``loop`` raises from inside Textual's own
#: message pump, which is the case that matters — a framework with its own event loop can
#: fail somewhere charter has no frame on the stack.
FAULT_VAR = "CHARTER_TEXTUAL_FAULT"

#: How often the out-of-contract refresh checks the frame's version. `panel.TICK` is
#: 0.2 s and this deliberately matches it: the point of the measurement is what a
#: provider would have to reimplement, and half of that is charter's own poll interval.
REFRESH_TICK = 0.2


class LiveApp(ReposApp):
    """:class:`ReposApp` with the out-of-contract poll bolted on, when asked.

    A subclass rather than a flag inside the shared app, so that the code doing the thing
    the report says a provider should not have to do is in one readable place and cannot
    be reached from the adapter by accident.
    """

    def __init__(self, *, gathered, fid: str, refresh: bool) -> None:
        super().__init__(gathered=gathered,
                         note="live · reaching around ctx" if refresh else "live · frozen")
        self._fid = fid
        self._refresh = refresh
        self._seen = ""

    def on_mount(self) -> None:
        super().on_mount()
        if os.environ.get(FAULT_VAR) == "loop":
            self.set_timer(1.0, self._fault)
        if self._refresh:
            self.set_interval(REFRESH_TICK, self._poll)

    def _fault(self) -> None:
        raise RuntimeError("injected fault inside Textual's message pump")

    def _poll(self) -> None:
        """`panel._tick`'s decision, reimplemented inside a provider — the finding.

        Every line of this is charter's own and none of it is reachable from `ctx`: which
        file carries the frame's version, that comparing it is how a panel learns
        anything happened, which workspace the FRAME resolved (#526), and that
        `gather.read` is the cache rather than a scan. A provider that gets this wrong
        gets it wrong quietly — a pane that stops updating, or one that runs a full
        `gather.scan` five times a second in every pane on the machine.
        """
        try:
            from charter.frame import gather, state
        except Exception:
            return
        try:
            now = state.version(self._fid)
            if now == self._seen:
                return
            self._seen = now
            self.apply(gather.read(self._fid))
        except Exception:
            return


def render(ctx) -> list[str]:
    """Hand the pane to Textual, and answer only once it hands the pane back.

    The return value is what charter paints *after* the app has exited — which is the one
    moment the contract and the framework agree on what a pane is. Charter's
    `panel._write` then clears the pane and writes this, so an operator who quit the app
    is left with a line saying so rather than with whatever the alternate screen restored.

    **`mouse=True` is the request the experiment is about.** tmux enables mouse reporting
    on the outer terminal from the ACTIVE pane's mode alone (§4i, re-measured here), so
    this pane asks for SGR reporting and gets it exactly while it is the active pane —
    which is the modal behaviour the spec listed as an open question and could not test
    without a program that wanted the mouse.

    **No try/except around `app.run`, and that is the measured decision rather than an
    omission.** Anything raised on the way in — including :data:`FAULT_VAR`'s ``render``
    injection — travels to `Registry.draw`, which names the component and paints the
    reason into this pane and no other. That is §4b property 1 holding for a framework
    charter never anticipated, and it is the result that came out better than expected.
    A ``try: … except: raise`` here would change no outcome, which #568 is the standing
    argument against: a line that cannot change an outcome is not documentation of an
    intent.

    What a guard here could NOT do is catch the case that actually matters. Measured: a
    crash inside Textual's own message pump never reaches this frame at all. Textual
    catches it, prints a Rich traceback to ``sys.__stderr__`` — 19,358 bytes of it —
    and `run()` returns **normally**, so `Registry.draw` sees a successful render and
    `panel._write` clears the pane over the traceback within one tick. The two lines
    below are all the operator is left with, and they cannot say why.
    """
    if os.environ.get(FAULT_VAR) == "render":
        raise RuntimeError("injected fault before the app started")
    fid = str(ctx.fid)
    refresh = os.environ.get(REFRESH_VAR) == "1"
    started = time.time()
    app = LiveApp(gathered=ctx.gather, fid=fid, refresh=refresh)
    app.run(mouse=True)
    ran = time.time() - started
    return [f"textual.live: app exited after {ran:.0f}s "
            f"(clicks {app.clicks}, scroll {app.scrolls}, keys {app.keys})",
            "charter repaints this pane again on the next version bump"]
