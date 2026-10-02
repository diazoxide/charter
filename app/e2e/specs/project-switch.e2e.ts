import { existsSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { browser, expect } from "@wdio/globals";
import { READY, built, declareAProfile, writeShell } from "../harness.js";
import { endChat, pressAndStart } from "../opening.js";
import { logLine } from "../processes.js";
import {
  BUDGET_MS,
  OPEN,
  aProjectOfItsOwn,
  ask,
  jobInTheWindow,
  closeTheProjectsOfItsOwn,
  inFront,
  measureSwitches,
  openFromTheStrip,
} from "../switching.js";

/**
 * **Ten projects open, and each brought to the front through the switcher** (FR-27), in the
 * built app, timed — the evidence row L9 of the spec's budgets of record reads on every pull
 * request.
 *
 * Each project has one chat open, as the operator holds them, and each switch is timed inside
 * the window (`bench.ts`, `project switch`): from the press of the project's row until the pane
 * of its chat has painted, which is when a person sees the project they asked for. Timed from
 * the test process, every sample would carry the driver's own round trip, which is the size of
 * the budget.
 *
 * **What it asserts is that every switch arrives, not how fast.** A timing held absolutely on a
 * shared runner is ADR 0086's short list of exceptions, and this row is not on it: L9 is release
 * absolute (`bench/projects.bench.ts`, on the operator's machine) and CI relative once SC-16's
 * `bench` job exists. Until then the numbers are written down — `logs/project-switch.jsonl` and
 * the job's own output — beside the budget they are for.
 *
 * **It leaves the window as it found it.** Specs share one app process and run in name order:
 * the nine projects it opened are closed, the chat it opened in the launch's project is ended,
 * and the project the launch opened is in front again.
 */

const TABS = '[role="tablist"][aria-label="Tabs"]';

/**
 * What a switched-to pane is waited for: the last word of `READY`, which every scenario chat
 * prints. A pane that comes back on screen is sent the screen as it already is (`watch_session`)
 * rather than the bytes the program wrote, and a screen redrawn need not spell the space between
 * two words as a space — one word is the same text either way.
 */
const PAINTED = READY.split(" ").at(-1) ?? READY;

/** How many times each project is switched to. */
const ROUNDS = 3;

/**
 * Opens one chat in `plane`, the project in front, and answers with the name its `×` carries.
 *
 * **Known by its session, not by the strip.** The strip collapses rather than scrolling, so in a
 * project an earlier spec left full of chats a new tab can push another into the show-more and
 * the strip draws as many tabs as before; and the tab in front is the OLD chat until the new one
 * is up. So the new chat is the session the core's list did not have before, and its `×` is read
 * only once that session's own pane is on screen — the pane in front is the tab in front's, so
 * the `×` beside the selected tab is then this chat's and never another spec's.
 */
async function aChatIn(plane: string): Promise<string> {
  const before = new Set(await ask<number[]>("running_sessions", { plane }));
  await pressAndStart("New tab");
  let session: number | undefined;
  await browser.waitUntil(
    async () => {
      session = (await ask<number[]>("running_sessions", { plane })).find(
        (one) => !before.has(one),
      );
      return session !== undefined;
    },
    { timeout: 30_000, timeoutMsg: "the chat this spec opened never started" },
  );
  let closer = "";
  await browser.waitUntil(
    async () => {
      const found = await browser.execute(
        (tabs: string, mine: number) =>
          document.querySelector(`[data-testid="pane"][data-session="${mine}"]`)
            ? (document
                .querySelector(`${tabs} [role="tab"][aria-selected="true"]`)
                ?.closest(".tab")
                ?.querySelector('button[aria-label^="End chat "]')
                ?.getAttribute("aria-label") ?? null)
            : null,
        TABS,
        session ?? -1,
      );
      closer = found ?? "";
      return closer !== "";
    },
    { timeout: 30_000, timeoutMsg: "the chat this spec opened never came to the front" },
  );
  return closer;
}

describe(`switching among ${OPEN} open projects`, function () {
  // On the describe, where WebdriverIO reads it (`e2e/budget.test.ts`). Nine opens through the
  // trust gate and nine chats are most of it; the thirty switches are seconds.
  this.timeout(600_000);

  /** The project the launch opened, which this spec never closes. */
  let first = "";
  /** The `×` of the chat this spec opened there, so only that chat is ended again. */
  let mine = "";
  /** The window's size before this spec made room for ten project tabs. */
  let size = { width: 0, height: 0 };
  const planes: string[] = [];

  before(async () => {
    size = await browser.getWindowSize();
    await browser.setWindowSize(1600, 1000);
    first = (await ask<string[]>("open_planes"))[0];
    planes.push(first);
    // A chat of this spec's own in front, always: whatever an earlier spec left in front there
    // may be a view, which has no pane to paint.
    mine = await aChatIn(first);
    for (let n = 1; n < OPEN; n++) {
      // The profile every scenario chat runs, so each project's chat is the fake harness
      // printing `READY`.
      const plane = aProjectOfItsOwn(n, (at) =>
        declareAProfile(at, writeShell(built("fake-harness"))),
      );
      await openFromTheStrip(plane);
      await aChatIn(plane);
      planes.push(plane);
    }
    expect(await ask<string[]>("open_planes")).toHaveLength(OPEN);
  });

  after(async () => {
    await closeTheProjectsOfItsOwn();
    expect(await ask<string[]>("open_planes")).toEqual([first]);
    await browser.waitUntil(async () => (await inFront()) === first, {
      timeout: 30_000,
      timeoutMsg: "the launch's project did not come back to the front",
    });
    // It was the tab in front there when the project was left, so it is the one in front now.
    if (mine !== "") await endChat(mine);
    await browser.setWindowSize(size.width, size.height);
  });

  it("brings each one to the front through the switcher, and its chat's pane paints", async () => {
    // In the order they were opened: the one in front is the last one opened, so no switch is
    // ever to the project already there.
    type Got = { ms: number; t0: number; marks: Record<string, number>; cmds: unknown[]; rows: string[]; lags: string[] };
    const got: Got[] = [];
    const job = async <T,>(plan: Parameters<typeof jobInTheWindow>[0]): Promise<T> => {
      const r = await jobInTheWindow<T>(plan);
      got.push(r as unknown as Got);
      return r;
    };
    console.log(
      `PROF launch project sessions ${(await ask<number[]>("running_sessions", { plane: first })).length}, tabs ${await browser.execute(() => document.querySelectorAll('[role="tablist"][aria-label="Tabs"] [role="tab"]').length)}`,
    );
    const switches = await measureSwitches(planes, ROUNDS, () => PAINTED, job);
    const file = [join(tmpdir(), "charter-ipc-prof.jsonl"), "/tmp/charter-ipc-prof.jsonl"].find(
      (one) => existsSync(one),
    );
    const rust = file
      ? readFileSync(file, "utf8")
          .split("\n")
          .filter(Boolean)
          .flatMap((l) => {
            try {
              return [JSON.parse(l) as { k: string; at: number; ms: number; c?: string; main?: boolean }];
            } catch {
              return [];
            }
          })
      : [];
    console.log(`PROF file ${file} lines ${rust.length}`);
    got.forEach((one, i) => {
      const end = one.t0 + one.ms + 30;
      const near = rust
        .filter((r) => r.at >= one.t0 - 20 && r.at <= end)
        .map((r) =>
          r.k === "cmd"
            ? `${r.c}${r.main ? "@main" : ""}@${Math.round(r.at - one.t0)}+${r.ms.toFixed(1)}`
            : `STALL@${Math.round(r.at - one.t0)}+${r.ms}`,
        );
      const marks = Object.fromEntries(
        Object.entries(one.marks).map(([k, v]) => [k, Math.round(v - one.t0)]),
      );
      console.log(`PROF ${JSON.stringify({ i, ms: Math.round(one.ms), marks, js: one.cmds, rust: near, rows: one.rows, lags: one.lags })}`);
    });

    const arms: [string, string][] = [
      ["explorer hidden", "nav.explorer { display: none !important; }"],
      ["explorer rows content-visibility", ".explorer li { content-visibility: auto; contain-intrinsic-size: auto 22px; }"],
      ["explorer contain strict", "nav.explorer { contain: strict; }"],
      ["again normal", ""],
    ];
    for (const [name, css] of arms) {
      await browser.execute((text: string) => {
        document.getElementById("prof-arm")?.remove();
        const el = document.createElement("style");
        el.id = "prof-arm";
        el.textContent = text;
        document.head.append(el);
      }, css);
      const arm = await measureSwitches(planes, ROUNDS, () => PAINTED, jobInTheWindow);
      const launch = [0, 10, 20].map((i) => Math.round(arm.samples_ms[i]));
      console.log(`PROF arm ${name}: p50 ${Math.round(arm.p50)} p95 ${Math.round(arm.p95)} launch ${launch.join(",")}`);
    }
    await browser.execute(() => document.getElementById("prof-arm")?.remove());
    for (const plane of [planes[1], first]) {
      await job({ kind: "project switch", plane, sentinel: PAINTED });
      const census = await browser.execute(() => {
        const out: Record<string, number> = { all: document.querySelectorAll("*").length };
        const walk = (el: Element, depth: number, path: string) => {
          const n = el.querySelectorAll("*").length;
          if (n < 60) return;
          const name = `${path}>${el.tagName.toLowerCase()}${el.className && typeof el.className === "string" ? "." + el.className.split(" ").slice(0, 2).join(".") : ""}`;
          out[name] = n;
          if (depth < 7) for (const c of Array.from(el.children)) walk(c, depth + 1, depth < 2 ? "" : "…");
        };
        const main = document.querySelector("main");
        if (main) walk(main, 0, "");
        return out;
      });
      console.log(`PROF census ${plane === first ? "launch" : "fresh"} ${JSON.stringify(census)}`);
    }
    logLine("project-switch.jsonl", { budgetMs: BUDGET_MS, ...switches });
    // In the job's own output too, where a reviewer reads it without downloading anything.
    console.log(`L9 project switch, ${BUDGET_MS} ms budget: ${JSON.stringify(switches)}`);

    expect(switches.samples).toBe(OPEN * ROUNDS);
  });
});
