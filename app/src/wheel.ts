import type { IDisposable, Terminal } from "@xterm/xterm";

/**
 * **A wheel or trackpad scrolls a terminal by the distance it moved** (SI-4): a row of history,
 * or a wheel report to the program, for every cell's height of pixels, with what is left over
 * carried to the next event. That is what Zed's terminal does (`determine_scroll_lines`, one
 * report per line in `scroll_report`), and what the operator measures charter against in iTerm2
 * and Terminal.app.
 *
 * xterm.js 6.0.0 does neither of its two jobs that way, which is why a chat felt slow to start
 * and fast when flicked:
 *
 * - **To a program tracking the mouse** (Claude Code's full-screen mode, opencode, vim with
 *   `mouse=a`), `CoreMouseService.consumeWheelEvent` multiplies every pixel delta under 50 by
 *   0.3 — "likely trackpad" — and `bindMouse` then sends ONE report for the event whatever the
 *   count came to. A slow start of 75px sent one report where there are four rows; a flick of
 *   80px an event sent one report an event where there are four.
 * - **In the history**, the viewport is VS Code's scrollable element, which reads a pixel as
 *   1/40 of a line of 50px and rounds every event up to a whole pixel: 1.25 times the distance,
 *   more on a slow start. It does not ask `attachCustomWheelEventHandler` either, so that API
 *   cannot stand in front of it.
 *
 * No option reaches either (`scrollSensitivity` scales both the 0.3 and the one-report cap
 * alike), so the pane takes the wheel before xterm sees it, on the capture phase of `holder`.
 * What it decides is only how many rows; the rows themselves go through xterm's own paths:
 *
 * - **history**: `Terminal.scrollLines`.
 * - **a program that asked for wheel reports, or a full-screen program with no history**: one
 *   `DOM_DELTA_LINE` wheel event of one line per row, handed to xterm. xterm counts such an
 *   event as exactly one, so it sends exactly one report — in the protocol and encoding the
 *   program asked for (SGR, X10, URXVT, SGR-pixels), with the modifiers and cell under the
 *   pointer — or, with no mouse tracking, one cursor key in the mode the program is in. The
 *   encoding is not public API, so writing reports here would be a second, partial copy of it.
 *
 * Nothing changes when the cell size is not known yet: the event is left to xterm.
 */
export function scrollByDistance(pane: Terminal, holder: HTMLElement): IDisposable {
  /** Pixels moved and not yet a whole row, signed like `deltaY`. */
  let carried = 0;
  /** The events handed to xterm, which this listener lets through. */
  const ours = new WeakSet<Event>();

  const take = (event: WheelEvent) => {
    if (ours.has(event)) return;
    const element = pane.element;
    const screen = element?.querySelector<HTMLElement>(".xterm-screen");
    // Both renderers size the screen to the rows they draw: it is the cell height xterm's own
    // viewport scrolls by.
    const cell = screen ? parseFloat(screen.style.height) / pane.rows : NaN;
    if (!element || !(cell > 0) || event.deltaY === 0) return;

    let rows: number;
    if (event.deltaMode === WheelEvent.DOM_DELTA_PIXEL) {
      // A change of direction starts afresh, so the fingers turning round are answered at once.
      if (Math.sign(carried) === -Math.sign(event.deltaY)) carried = 0;
      carried += event.deltaY;
      rows = Math.trunc(carried / cell);
      carried -= rows * cell;
    } else {
      rows = event.deltaY * (event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? pane.rows : 1);
      rows = Math.trunc(rows);
    }
    // xterm is not to see this event: its own answer is the one being replaced.
    event.preventDefault();
    event.stopPropagation();
    if (rows === 0) return;

    const tracked = pane.modes.mouseTrackingMode;
    const reports = tracked !== "none" && tracked !== "x10"; // X10 reports presses only
    if (!reports && pane.buffer.active.type === "normal") {
      pane.scrollLines(rows);
      return;
    }
    for (let i = 0; i < Math.abs(rows); i++) {
      const one = new WheelEvent("wheel", {
        deltaY: Math.sign(rows),
        deltaMode: WheelEvent.DOM_DELTA_LINE,
        clientX: event.clientX,
        clientY: event.clientY,
        screenX: event.screenX,
        screenY: event.screenY,
        ctrlKey: event.ctrlKey,
        altKey: event.altKey,
        shiftKey: event.shiftKey,
        metaKey: event.metaKey,
        cancelable: true,
      });
      ours.add(one);
      element.dispatchEvent(one);
    }
  };

  holder.addEventListener("wheel", take, { capture: true, passive: false });
  return { dispose: () => holder.removeEventListener("wheel", take, { capture: true }) };
}
