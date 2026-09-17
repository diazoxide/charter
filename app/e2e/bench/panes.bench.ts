import { browser } from "@wdio/globals";
import type { Bench } from "../../src/bench.ts";
import { nextLoad } from "../load.js";
import { arm, job, openTab, panes, ready, record, sleep } from "./window.js";

/**
 * Many panes on screen at once, which is where the WebGL arm can run out: WebKit gives a page
 * a limited number of WebGL contexts (research §5.1). Every pane still has to draw.
 *
 * It splits the biggest pane each time, so the panes stay big enough to draw a terminal, and
 * records how many it reached: a pane that never paints ends the count instead of the run.
 */
const VISIBLE = 20;

describe(`${VISIBLE} panes on screen at once`, () => {
  const results: Record<string, unknown> = {};

  before(ready);
  after(() => record("panes", results));

  it("splits until there are that many, and says what each is drawing with", async () => {
    await browser.setWindowSize(1600, 1000);
    nextLoad(["--synthetic", "4096", "--sentinel", "PANE-0", "--interactive"]);
    await openTab();

    const splits: number[] = [];
    let trouble: string | undefined;
    for (let n = 1; n < VISIBLE && !trouble; n++) {
      const biggest = (await panes()).reduce((one, other) =>
        other.columns * other.rows > one.columns * one.rows ? other : one,
      );
      await browser.execute(
        (session) => (window.charterBench as Bench).focus(session),
        biggest.session,
      );
      nextLoad(["--synthetic", "4096", "--sentinel", `PANE-${n}`, "--interactive"]);
      try {
        const split = await job<{ ms: number }>({
          kind: "split",
          // A cell is about twice as tall as it is wide.
          direction: biggest.columns > biggest.rows * 2 ? "Split right" : "Split down",
          sentinel: `PANE-${n}`,
        });
        splits.push(split.ms);
      } catch (err) {
        trouble = String(err);
      }
    }
    await sleep(2000);

    const all = await panes();
    results["panes on screen at once"] = {
      arm,
      asked: VISIBLE,
      panesThatDrew: splits.length + 1,
      stoppedBecause: trouble,
      drawingWithTheArm: all.filter((one) => one.active).length,
      fellBack: all.filter((one) => !one.active).map((one) => one.trouble),
      everyPaneWasSentOutput: all.every((one) => one.characters > 0),
      smallest: all.reduce((one, other) =>
        other.columns * other.rows < one.columns * one.rows ? other : one,
      ),
      // Each split re-lays out every pane, so a later one costs more than the first.
      splitMs: {
        p50: [...splits].sort((a, b) => a - b)[Math.floor(splits.length / 2)],
        worst: Math.max(...splits),
        samples_ms: splits,
      },
    };
  });
});
