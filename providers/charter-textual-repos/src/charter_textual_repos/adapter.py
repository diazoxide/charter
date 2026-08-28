"""The shape charter's contract actually describes: Textual as a renderer.

`Component.render` is ``Callable[[Any], list[str]]`` and `Registry.draw` takes what it
answers, escapes it, clips it to the pane and hands it to `panel._write`. So the only way
a widget framework fits *as written* is to run it where nobody can see it and copy its
screen out — which is what this module does: a `ReposApp` on Textual's **headless**
driver, on a background thread, whose composited screen is read back through
``Screen._compositor.render_strips()`` once per repaint.

**This works, and three things are lost on the way out.** Each is measured in the
experiment's report; each is a property of charter's contract rather than of Textual.

1. **Colour.** `Registry.draw` escapes every line a provider returned
   (``escape=cid in self._foreign``), and `contain.one_line` replaces ESC with the four
   characters ``\\x1b``. A Textual strip rendered with ``Strip.render(console)`` is
   almost entirely SGR, so returning it paints the escape sequences as literal text.
   ``Strip.text`` is therefore what this module returns, and every colour, every
   background, the cursor bar and the zebra stripes are dropped at the boundary. A
   provider component is a **monochrome** component, and nothing in the contract says so.

2. **Input.** `Component.events` is validated by `component.names` at construction and
   read by no production code in charter (grep: the only reader is the validator itself).
   There is no dispatch path, so the headless app receives no key, no click and no
   scroll — the `DataTable`'s cursor cannot be moved and its scrollbar cannot be reached.
   A scrolling table that cannot scroll is the one thing this widget was worth having.

3. **The frame's clock.** That half survives intact and is the good news: charter calls
   `render` again on every version bump and hands over a fresh `ctx`, so the table here
   is as live as charter's own.
"""

from __future__ import annotations

import os
import sys
import threading
import time

from .ui import ReposApp

#: How long a repaint will wait for the app to come up before drawing a placeholder. A
#: panel repaints on a 0.2 s tick (`panel.TICK`) and must never block it: the app starts
#: once per panel PROCESS, so this is paid at most once, and if it is not ready the pane
#: says so for one tick rather than the loop stalling.
START_TIMEOUT = float(os.environ.get("CHARTER_TEXTUAL_START_TIMEOUT", "3.0"))

#: Experiment apparatus: set to ``1`` and :func:`_give_stdout_back` does nothing, which is
#: what a provider author who has not read Textual's source ships. `measure/m8_stdout.sh`
#: runs a pane both ways so the defect can be looked at rather than described — a pane
#: that paints once, blanks on the next repaint, and reports nothing anywhere.
KEEP_CAPTURE_VAR = "CHARTER_TEXTUAL_KEEP_CAPTURE"


class _Headless:
    """One background Textual app, kept for the life of the panel process.

    **Started once, not per repaint**, for `panel._component_painter`'s own reason one
    layer up: "The registry is built once and closed over, not rebuilt per tick." A
    Textual app costs ~80 ms warm to import and a further event-loop start to run; paying
    that five times a second is the cost §4's whole budget argument exists to keep off the
    repaint path.

    **Restarted on a resize, and only on a resize.** The headless driver takes its size
    at ``run(size=…)``. Posting a synthetic ``Resize`` into a running app is the tidier
    thing to do and is not what this does, because the tidier thing has a failure mode
    the operator sees — a widget tree half-laid-out at two different widths — and a
    restart costs one tick of a placeholder on an event that happens when a human drags a
    window edge.
    """

    def __init__(self) -> None:
        self.app: ReposApp | None = None
        self.size: tuple[int, int] | None = None
        self._thread: threading.Thread | None = None
        #: Whatever the app thread died of, so a repaint can put it in the pane rather
        #: than leaving a blank one — charter's own rule (§4b property 1) applied by the
        #: provider to itself.
        self.died: BaseException | None = None

    def _run(self, app: ReposApp, size: tuple[int, int]) -> None:
        try:
            app.run(headless=True, size=size)
        except BaseException as exc:            # noqa: BLE001 - reported, never swallowed
            self.died = exc

    def ensure(self, width: int, height: int, gathered) -> ReposApp | None:
        """The running app at *width* x *height*, started or restarted if need be."""
        want = (max(width, 1), max(height, 1))
        if self.app is not None and self.size == want and self.app.is_running:
            return self.app
        self.stop()
        self.died = None
        app = ReposApp(gathered=gathered, note="headless")
        self._thread = threading.Thread(
            target=self._run, args=(app, want), daemon=True,
            name="charter-textual-repos")
        self._thread.start()
        deadline = time.monotonic() + START_TIMEOUT
        while time.monotonic() < deadline:
            if self.died is not None:
                return None
            if app.ready.is_set():
                self.app, self.size = app, want
                return app
            time.sleep(0.005)
        return None

    def stop(self) -> None:
        app, self.app, self.size = self.app, None, None
        if app is not None and app.is_running:
            try:
                app.call_from_thread(app.exit)
            except Exception:
                pass
        if self._thread is not None:
            self._thread.join(timeout=1.0)
            self._thread = None


#: One per panel process. Module state, which is what a provider gets: charter builds the
#: registry once and closes over it, and a component is a frozen dataclass whose `render`
#: is an ordinary function — there is nowhere else for a long-lived handle to live.
#: `frame/ctx.py` is explicit that this is not a sandbox and that a provider's module is
#: ordinary Python; this is what "ordinary Python" looks like when a framework needs a
#: process-lifetime object.
_HOST = _Headless()


def render(ctx) -> list[str]:
    """*ctx*'s repos, drawn by Textual, as the plain lines charter's contract takes.

    Never raises, though it does not have to: `Registry.draw` already catches everything a
    provider's renderer does wrong and turns it into a pane naming the component. The
    guard is here anyway because the *message* matters — "textual.repos failed to draw —
    RuntimeError: …" tells an operator less than a line saying the app would not start —
    and because a provider that leans on the host's containment for its own bugs is a
    provider that never finds them.
    """
    width, height = int(ctx.width), int(ctx.height)
    if width <= 0 or height <= 0:
        return []
    try:
        app = _HOST.ensure(width, height, ctx.gather)
        if app is None:
            why = type(_HOST.died).__name__ if _HOST.died else "timeout"
            return [f"textual.repos: app did not start ({why})"]
        return app.call_from_thread(_screen, app, ctx.gather)
    except Exception as exc:                    # noqa: BLE001
        return [f"textual.repos: {type(exc).__name__}: {exc}"]
    finally:
        # In a `finally`, and it has to be: charter measures the pane BEFORE it calls
        # render — `panel._component_text` evaluates `slots._width()` and `_rows()` as
        # arguments to `ctx.build` — so restoring stdout on the way IN is one tick too
        # late. See :func:`_give_stdout_back` for what the tick in between looked like.
        _give_stdout_back()


def _give_stdout_back() -> None:
    """Undo Textual's process-wide stdout capture, because charter's pane IS `sys.stdout`.

    **The sharpest finding of the experiment, and it is four lines of provider code
    standing in for a hole in the contract.**

    `textual/app.py:3491` wraps the app's message pump in
    ``contextlib.redirect_stdout(self._capture_stdout)`` so that a ``print()`` inside a
    widget lands in the app's log instead of on the screen. ``redirect_stdout`` assigns
    ``sys.stdout`` — a process-wide module global — and it does it on the app's own
    thread, headless or not. Measured: with a headless Textual app running in this
    process, ``sys.stdout`` is a ``textual.app._PrintCapture``.

    charter reaches `sys.stdout` in three places, and that object subverts all three,
    because `_PrintCapture` (`textual/app.py:260`) answers ``isatty() -> True`` and
    ``fileno() -> -1``:

    * `panel._out` — ``sys.stdout.write(payload); sys.stdout.flush()``. The pane's paint
      goes into Textual's print log. The pane keeps whatever was on it.
    * `slots._width` / `slots._height` — ``os.get_terminal_size(sys.stdout.fileno())``,
      i.e. of fd ``-1``, which raises ``OSError`` and is caught: charter silently falls
      back to :data:`slots._DEFAULT_COLS` x :data:`slots._DEFAULT_ROWS`, **80x24**.
    * `chrome.colour_ok` — ``sys.stdout.isatty()``, which answers True however the panel
      was actually launched, so `NO_COLOR`'s companion check is answered by the framework
      rather than by the terminal.

    **Measured symptom, before this function existed**, in a real 150x10 pane: the first
    paint is correct, and the second paint — the first one after the app is up — draws a
    pane that is blank except for its last line. The chain is the three bullets together:
    charter measures 80x24 instead of 150x10, the component lays itself out 24 rows tall,
    `panel._write` clamps to 24 rows and writes all of them into a 10-row pane, and the
    pane scrolls its own content away. Nothing raises. Nothing is logged. `Registry.draw`
    has nothing to catch, because nothing failed.

    Called from `render`'s ``finally`` and not on the way in, because charter measures the
    pane BEFORE it calls render: `panel._component_text` evaluates ``slots._width()`` and
    ``_rows()`` as arguments to `ctx.build`. Restoring on entry fixes the paint and leaves
    the measurement one tick stale forever.
    """
    if os.environ.get(KEEP_CAPTURE_VAR) == "1":
        return                                  # apparatus — see the constant
    if sys.stdout is not sys.__stdout__ and sys.__stdout__ is not None:
        sys.stdout = sys.__stdout__
    if sys.stderr is not sys.__stderr__ and sys.__stderr__ is not None:
        sys.stderr = sys.__stderr__


def _screen(app: ReposApp, gathered) -> list[str]:
    """Push *gathered* into *app* and copy its composited screen out, as plain text.

    Runs on the app's own event loop thread (`App.call_from_thread`), which is the only
    place a widget tree may be touched. Both halves are done in one hop deliberately: two
    hops would let a repaint read a screen laid out for the previous snapshot, which is
    the "everything on screen is from the same moment" property `frame/ctx.py` builds the
    whole one-snapshot design around.

    ``Strip.text`` and never ``Strip.render(console)`` — see this module's docstring.
    """
    app.apply(gathered)
    # **Three private attributes of Textual's, and there is no public alternative.**
    # `Screen._refresh_layout` is what arranges the widget tree into the compositor, and
    # `Compositor.render_strips` is what a driver would write to a terminal. Textual's
    # public surface for "give me my screen" is `save_screenshot`/`export_screenshot`,
    # which answers SVG — an image, for a bug report. There is no supported way to ask a
    # Textual app for its screen as terminal lines, because no supported use of Textual
    # wants one: the framework's contract is that it owns the terminal. Charter's contract
    # is that it owns the terminal. That is the collision, and reaching into
    # `_refresh_layout` and `_compositor` is what it looks like from the provider's side —
    # a component that breaks on a Textual point release, in a way charter would surface
    # as "textual.repos failed to draw".
    app.screen._refresh_layout(app.size)
    return [strip.text for strip in app.screen._compositor.render_strips()]
