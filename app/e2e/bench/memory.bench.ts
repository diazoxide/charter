import { nextLoad, scrollingLines } from "../load.js";
import { appProcess, frame, harnesses, job, openTab, ready, record, sleep } from "./window.js";

/**
 * What an idle, hidden session costs, with its history full.
 *
 * Spec limit: an idle hidden session ≤ 50 MB, with scrollback at the shipped cap. A hidden
 * session has no pane, so what it costs is in the app's own process — its terminal in the
 * core — and not in the web view, which holds only the panes on screen.
 */
const SESSIONS = 50;
const LINES = 6000;

describe("idle hidden sessions with their history full", () => {
  const results: Record<string, unknown> = {};

  before(ready);
  after(() => record("memory", results));

  it(`${SESSIONS} sessions, each having written ${LINES} lines at 150x42`, async () => {
    await sleep(3000);
    const empty = appProcess();

    // Two light sessions first — one hidden, one in front — so what the fifty add is the
    // only thing between this reading and the last one.
    for (const sentinel of ["ONE-OPEN", "TWO-OPEN"]) {
      nextLoad(["--synthetic", "4096", "--sentinel", sentinel, "--interactive"]);
      await openTab();
    }
    await sleep(3000);
    const one = appProcess();

    const history = scrollingLines(LINES);
    for (let n = 0; n < SESSIONS; n++) {
      nextLoad(
        ["--corpus", history.path, "--wait-for-input", "--sentinel", "HISTORY-FULL"].concat([
          "--interactive",
        ]),
      );
      const { session } = await openTab();
      await frame(session);
      await sleep(200);
      await job({ kind: "painted", sentinel: "HISTORY-FULL", bytes: history.bytes, type: "\r" });
    }
    await sleep(10_000);
    const full = appProcess();

    results[`${SESSIONS} idle sessions with their history full`] = {
      sessionsRunning: harnesses().running,
      historyLinesWrittenEach: LINES,
      historyBytesEach: history.bytes,
      scrollbackCap: 5000,
      appRssMbWithNoSessions: empty.rssMb,
      appRssMbWithTwoLightSessions: one.rssMb,
      appRssMbWithAllFull: full.rssMb,
      // The app's own process only: a hidden session has no pane, so what it costs is its
      // terminal in the core. What a pane costs lives in the web view's process, which this
      // does not measure.
      perSessionWithFullHistoryMb: (full.rssMb - one.rssMb) / SESSIONS,
    };
  });
});
