import { browser } from "@wdio/globals";
import { nextLoad } from "../load.js";
import {
  BUDGET_MS,
  OPEN,
  aProjectOfItsOwn,
  closeTheProjectsOfItsOwn,
  heldToBudget,
  inFront,
  measureSwitches,
  openFromTheStrip,
} from "../switching.js";
import { closeEverything, job, openTab, ready, record } from "./window.js";

/** How many times each project is switched to. */
const ROUNDS = 3;

/**
 * Ten open projects, each with one chat, and each brought to the front through the project
 * switcher three times round (FR-27).
 *
 * Spec limit: row L9 of the budgets of record — switching among ten open projects ≤ 200 ms at the p95,
 * release absolute. Measured from the press of the project's row in the switcher until the
 * pane of the chat in front over there has painted (`bench.ts`, `project switch`).
 *
 * **This is where FR-27's acceptance is held.** The numbers are recorded first, then the spec
 * fails when the p95 is past the budget, so `tools/bench.mjs` reports the miss beside them
 * (ADR 0086: a release absolute row that misses is a bug filed before the release notes, not a
 * blocked release). CI's scenario run measures the same switches and gates nothing on them.
 */
describe(`switching among ${OPEN} open projects`, () => {
  const results: Record<string, unknown> = {};
  /** Each project's chat prints a sentinel of its own, so a paint is the RIGHT pane's. */
  const sentinels = new Map<string, string>();

  before(ready);
  after(() => record("projects", results));

  it("brings each one to the front, and says how long that took", async () => {
    await browser.setWindowSize(1600, 1000);
    const planes: string[] = [];

    // The benchmark's own plane is in front: its chat first.
    const launched = await inFront();
    if (launched === null) throw new Error("the benchmark's plane is not in front");
    nextLoad(["--synthetic", "4096", "--sentinel", "PROJECT-0", "--interactive"]);
    await openTab();
    planes.push(launched);
    sentinels.set(launched, "PROJECT-0");

    for (let n = 1; n < OPEN; n++) {
      const plane = aProjectOfItsOwn(n);
      await openFromTheStrip(plane);
      nextLoad(["--synthetic", "4096", "--sentinel", `PROJECT-${n}`, "--interactive"]);
      await openTab();
      planes.push(plane);
      sentinels.set(plane, `PROJECT-${n}`);
    }

    const switches = await measureSwitches(
      planes,
      ROUNDS,
      (plane) => sentinels.get(plane) ?? "",
      job,
    );
    const verdict = heldToBudget(switches, ROUNDS);
    results["project switch"] = { budgetMs: BUDGET_MS, met: verdict.met, ...switches };
    console.log(verdict.sentence);

    await closeTheProjectsOfItsOwn();
    await closeEverything();
    if (!verdict.met) throw new Error(verdict.sentence);
  });
});
