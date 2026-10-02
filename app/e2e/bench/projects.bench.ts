import { browser } from "@wdio/globals";
import { nextLoad } from "../load.js";
import {
  BUDGET_MS,
  OPEN,
  aProjectOfItsOwn,
  closeTheProjectsOfItsOwn,
  inFront,
  measureSwitches,
  openFromTheStrip,
} from "../switching.js";
import { closeEverything, job, openTab, ready, record } from "./window.js";

/**
 * Ten open projects, each with one chat, and each brought to the front through the project
 * switcher three times round (FR-27).
 *
 * Spec limit: row L9 of the budgets of record — switching among ten open projects ≤ 200 ms at the p95,
 * release absolute. Measured from the press of the project's row in the switcher until the
 * pane of the chat in front over there has painted (`bench.ts`, `project switch`).
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

    const switches = await measureSwitches(planes, 3, (plane) => sentinels.get(plane) ?? "", job);
    results["project switch"] = { budgetMs: BUDGET_MS, ...switches };

    await closeTheProjectsOfItsOwn();
    await closeEverything();
  });
});
