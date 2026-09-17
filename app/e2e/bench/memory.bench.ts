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

    nextLoad(["--synthetic", "4096", "--sentinel", "ONE-OPEN", "--interactive"]);
    await openTab();
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
    // One more tab in front, so every session with a full history is hidden.
    nextLoad(["--synthetic", "4096", "--sentinel", "IN-FRONT", "--interactive"]);
    await openTab();
    await sleep(10_000);
    const full = appProcess();

    results[`${SESSIONS} idle hidden sessions`] = {
      sessionsRunning: harnesses().running,
      historyLinesWrittenEach: LINES,
      historyBytesEach: history.bytes,
      scrollbackCap: 5000,
      appRssMbWithNoSessions: empty.rssMb,
      appRssMbWithOneSession: one.rssMb,
      appRssMbWithAllFull: full.rssMb,
      perHiddenSessionMb: (full.rssMb - one.rssMb) / SESSIONS,
    };
  });
});
