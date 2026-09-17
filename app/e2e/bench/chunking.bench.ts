import { nextLoad, synchronizedFrame } from "../load.js";
import { openTab, paneOf, ready, record, sleep } from "./window.js";

/**
 * How a repaint written in one write reaches the pane.
 *
 * xterm.js skips any render that falls while a `?2026` update is open, so a repaint that
 * arrives in pieces is the case that stalls (the burst spec measures what that costs). What
 * decides it is whether one write survives as one message: the pseudo-terminal, the core's
 * read and the IPC each get to split it. This needs no paint, so it also answers on a machine
 * whose screen is locked.
 */
describe("a repaint written in one write", () => {
  const results: Record<string, unknown> = {};

  before(ready);
  after(() => record("chunking", results));

  const frames: [string, Parameters<typeof synchronizedFrame>[1]][] = [
    ["a small repaint", { rows: 40, columns: 60, runs: 1 }],
    ["a full-screen repaint", { rows: 42, columns: 150, runs: 6 }],
    ["a repaint of a very wide screen", { rows: 42, columns: 600, runs: 12 }],
  ];
  for (const [name, shape] of frames) {
    it(`${name}: how many messages the pane is sent`, async () => {
      const frame = synchronizedFrame(name.split(" ").join("-"), shape);
      // One write per repaint, ten repaints, far enough apart to arrive on their own.
      nextLoad([
        "--corpus",
        frame.path,
        "--chunk",
        `${frame.bytes}`,
        "--loops",
        "10",
        "--interval-ms",
        "100",
      ]);
      const { session } = await openTab();
      await sleep(3000);

      const pane = await paneOf(session);
      results[name] = {
        bytesPerRepaint: frame.bytes,
        repaints: 10,
        messagesToThePane: pane.chunks,
        charactersPerMessage: Math.round(pane.characters / Math.max(1, pane.chunks)),
        // One message per repaint means nothing split it, so no render can land mid-update.
        repaintsArrivedWhole: pane.chunks <= 11,
      };
    });
  }
});
