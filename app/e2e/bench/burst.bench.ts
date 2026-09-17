import { nextLoad, scrollingLines, synchronizedFrame } from "../load.js";
import {
  CORPUS_BYTES,
  corpus,
  frame,
  frames,
  job,
  openTab,
  paneOf,
  ready,
  record,
  sleep,
  type,
  watchFrames,
} from "./window.js";

/**
 * One visible pane under a burst, and the moves a person makes between panes.
 *
 * Spec limits: 2 MB and 13 MB bursts never freeze the UI, and input and other panes stay
 * responsive · tab or pane switch ≤ 100 ms · a `?2026` animation stays ≥ 30 fps.
 */
describe("bursts and switches", () => {
  const results: Record<string, unknown> = {};

  before(ready);
  after(() => record("burst", results));

  const bursts: [string, number][] = [
    ["corpus once (132 KB)", 1],
    ["2 MB (corpus x16)", 16],
    ["13 MB (corpus x99)", 99],
  ];
  for (const [name, loops] of bursts) {
    it(`${name}: first byte to the sentinel painted, and the longest frame meanwhile`, async () => {
      const sentinel = `BURST-DONE-${loops}`;
      nextLoad(
        ["--corpus", corpus, "--loops", `${loops}`, "--wait-for-input"].concat([
          "--sentinel",
          sentinel,
          "--interactive",
        ]),
      );
      const { session } = await openTab();
      await frame(session);
      await sleep(500);

      await watchFrames();
      const painted = await job({
        kind: "painted",
        sentinel,
        bytes: CORPUS_BYTES * loops,
        type: "\r",
      });
      const drawn = await frames();

      const pane = await paneOf(session);
      results[name] = {
        ...painted,
        longestFrameGapMs: drawn.longestGapMs,
        framesPerSecond: drawn.framesPerSecond,
        columns: pane.columns,
        rows: pane.rows,
      };
    });
  }

  it("13 MB in one pane: typing in the pane beside it, and the longest frame", async () => {
    nextLoad(
      ["--corpus", corpus, "--loops", "99", "--wait-for-input"].concat([
        "--sentinel",
        "BESIDE-DONE",
        "--interactive",
      ]),
    );
    const { session: bursting } = await openTab();
    nextLoad(["--synthetic", "4096", "--sentinel", "BESIDE-READY", "--interactive"]);
    const { session: typing } = await job<{ session: number }>({
      kind: "split",
      direction: "Split right",
      sentinel: "BESIDE-READY",
    });
    await frame(bursting);
    await sleep(500);

    const before = await paneOf(bursting);
    await watchFrames();
    await type(bursting, "\r");
    const keys = await job({ kind: "keystrokes", count: 20, session: typing });
    const drawn = await frames();
    const after = await paneOf(bursting);

    results["13 MB beside: keystrokes in the other pane"] = {
      ...keys,
      longestFrameGapMs: drawn.longestGapMs,
      // How much of the burst arrived while the typing went on: if the burst had already
      // finished, these keystrokes were not measured under it.
      burstCharactersArrivedDuringTyping: after.characters - before.characters,
    };
  });

  // A `?2026` animation, offered at 60 frames a second. xterm.js skips a render that falls
  // while a synchronized update is open, so how the frames arrive decides what is drawn:
  const animations: [string, () => string[]][] = [
    [
      // each repaint in one write, small: 40 lines with one colour run each
      "?2026 animation, a 3 KB repaint written whole",
      () => {
        const frame = synchronizedFrame("small");
        return ["--corpus", frame.path, "--chunk", `${frame.bytes}`, "--loops", "100000"];
      },
    ],
    [
      // a full screen with colour through it, still one write: what a harness that repaints
      // everything hands to the pseudo-terminal at once
      "?2026 animation, a 30 KB repaint written whole",
      () => {
        const frame = synchronizedFrame("full", { rows: 42, columns: 150, runs: 6 });
        return ["--corpus", frame.path, "--chunk", `${frame.bytes}`, "--loops", "100000"];
      },
    ],
    [
      // repaints cut at 4 KB wherever they fall, so most renders land mid-update
      "?2026 animation, repaints split across writes",
      () => ["--synthetic", "400000", "--loops", "1000"],
    ],
  ];
  for (const [name, output] of animations) {
    it(`${name}: how often the pane draws`, async () => {
      nextLoad([...output(), "--interval-ms", "16", "--wait-for-input"]);
      const { session } = await openTab();
      await frame(session);
      await sleep(500);
      await type(session, "\r");
      await sleep(1000);

      await watchFrames();
      const rate = await job({ kind: "draw rate", ms: 5000, session });
      const drawn = await frames();
      const pane = await paneOf(session);
      results[name] = {
        ...rate,
        longestFrameGapMs: drawn.longestGapMs,
        // How much arrived per message: a repaint handed over in pieces is the case that
        // xterm.js skips renders for.
        charactersPerChunk: Math.round(pane.characters / Math.max(1, pane.chunks)),
      };
    });
  }

  it("tab switch: back to a light tab, and to one with its whole history", async () => {
    nextLoad(["--synthetic", "4096", "--sentinel", "LIGHT-TAB", "--interactive"]);
    const light = await openTab();

    const history = scrollingLines(6000);
    nextLoad(
      ["--corpus", history.path, "--wait-for-input", "--sentinel", "HEAVY-TAB"].concat([
        "--interactive",
      ]),
    );
    const heavy = await openTab();
    await frame(heavy.session);
    await sleep(500);
    await job({ kind: "painted", sentinel: "HEAVY-TAB", bytes: history.bytes, type: "\r" });

    nextLoad(["--synthetic", "4096", "--sentinel", "AWAY-TAB", "--interactive"]);
    const away = await openTab();

    const toLight: number[] = [];
    const toHeavy: number[] = [];
    for (let n = 0; n < 10; n++) {
      const l = await job<{ ms: number }>({
        kind: "switch",
        tab: light.tab,
        sentinel: "LIGHT-TAB",
      });
      toLight.push(l.ms);
      await job({ kind: "switch", tab: away.tab, sentinel: "AWAY-TAB" });
      const h = await job<{ ms: number }>({
        kind: "switch",
        tab: heavy.tab,
        sentinel: "HEAVY-TAB",
      });
      toHeavy.push(h.ms);
      await job({ kind: "switch", tab: away.tab, sentinel: "AWAY-TAB" });
    }
    results["tab switch to a light tab"] = summary(toLight);
    results["tab switch to a tab with 5000 lines of history"] = {
      ...summary(toHeavy),
      historyLinesWritten: 6000,
    };
  });

  it("split: pressed until the new pane has painted", async () => {
    const samples: number[] = [];
    for (let n = 0; n < 5; n++) {
      const sentinel = `SPLIT-${n}`;
      nextLoad(["--synthetic", "4096", "--sentinel", sentinel, "--interactive"]);
      const split = await job<{ ms: number }>({
        kind: "split",
        direction: n % 2 ? "Split down" : "Split right",
        sentinel,
      });
      samples.push(split.ms);
    }
    results["split, until the new pane has painted"] = summary(samples);
  });
});

function summary(samples: number[]) {
  const sorted = [...samples].sort((a, b) => a - b);
  return {
    samples: sorted.length,
    p50: sorted[Math.floor(sorted.length / 2)],
    worst: sorted[sorted.length - 1],
    samples_ms: samples,
  };
}
