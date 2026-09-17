import { browser } from "@wdio/globals";
import { nextLoad } from "../load.js";
import {
  appProcess,
  corpus,
  frame,
  frames,
  harnesses,
  job,
  openTab,
  ready,
  record,
  sleep,
  watchFrames,
} from "./window.js";

/**
 * Fifty live sessions: forty-nine streaming in tabs behind, one in front being typed into.
 *
 * Spec limits: 50 live sessions · keystroke to screen ≤ 50 ms while 49 others stream ·
 * tab switch ≤ 100 ms.
 */
describe("fifty sessions, forty-nine of them streaming", () => {
  const results: Record<string, unknown> = {};

  before(ready);
  after(() => record("load", results));

  // Throttled is already more than any real session writes for long: about 1 MB/s each, 49
  // MB/s between them. Flat out is every harness writing as fast as the app reads.
  const loads: [string, string[]][] = [
    ["49 streaming at ~1 MB/s each", ["--interval-ms", "4"]],
    ["49 streaming flat out", []],
  ];
  for (const [name, pace] of loads) {
    it(`${name}: keystrokes in the fiftieth, and switching back to it`, async () => {
      nextLoad(["--corpus", corpus, "--loops", "1000000"].concat(pace));
      const streaming: string[] = [];
      for (let n = 0; n < 49; n++) streaming.push((await openTab()).tab);

      nextLoad(["--synthetic", "4096", "--sentinel", "TYPED-HERE", "--interactive"]);
      const front = await openTab();
      await frame(front.session);
      await sleep(2000);
      const running = harnesses();

      await watchFrames();
      const keys = await job({ kind: "keystrokes", count: 30, session: front.session });
      const drawn = await frames();

      const switches: number[] = [];
      for (let n = 0; n < 10; n++) {
        await job({ kind: "switch", tab: streaming[n], sentinel: "" });
        const back = await job<{ ms: number }>({
          kind: "switch",
          tab: front.tab,
          sentinel: "you said:",
        });
        switches.push(back.ms);
      }
      const sorted = [...switches].sort((a, b) => a - b);

      results[name] = {
        sessionsRunning: running.running,
        harnessCpuPercent: running.cpuPercent,
        appRssMb: appProcess().rssMb,
        keystrokeToScreen: keys,
        longestFrameGapMs: drawn.longestGapMs,
        framesPerSecond: drawn.framesPerSecond,
        switchBackToTheTypedTab: {
          samples: sorted.length,
          p50: sorted[Math.floor(sorted.length / 2)],
          worst: sorted[sorted.length - 1],
          samples_ms: switches,
        },
      };

      // Every tab closed, so the next load starts from none.
      await browser.execute(() => {
        for (const close of [...document.querySelectorAll('button[aria-label^="Close tab"]')]) {
          (close as HTMLElement).click();
        }
      });
      await browser.waitUntil(async () => harnesses().running === 0, {
        timeout: 60_000,
        timeoutMsg: "closing every tab left harnesses running",
      });
    });
  }
});
