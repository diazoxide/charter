import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * Where a scenario spec's timeout has to be written down.
 *
 * WebdriverIO does not leave the deadline to mocha. `executeAsync` in `@wdio/utils` reads
 * `this._runnable._timeout` ONCE, before the test body starts, and races the body against a
 * timer of its own that rejects with a bare `Error: Timeout`:
 *
 * ```js
 * const runnableTimeout = this?._runnable?._timeout;          // read before fn runs
 * const _timeout = (frameworkTimeout ?? timeout) - TIME_BUFFER;
 * await Promise.race([fn.apply(this, args), <setTimeout(_timeout) -> reject(Error("Timeout"))>]);
 * ```
 *
 * So `this.timeout(ms)` in the first line of a test body moves mocha's runnable and arrives
 * too late for the race that is already set up: the spec keeps running under
 * `mochaOpts.timeout`. On a `describe` the value is on the suite before the runnable is
 * built, the runnable inherits it, and WebdriverIO reads the number that was asked for.
 *
 * This is not theory. `fifty tabs (macos-latest)` was red on five of its last six runs on
 * `main` because `stress.e2e.ts` asked for 900 s and ran under 180, and the macOS runner
 * needs 170–218 s. Both spellings typecheck, lint and read the same, so nothing but this
 * check stands between the next spec that needs room and the same red X.
 */
const specs = join(dirname(fileURLToPath(import.meta.url)), "specs");

function spec(name: string): string {
  return readFileSync(join(specs, name), "utf8");
}

const everySpec = readdirSync(specs).filter((name) => name.endsWith(".e2e.ts"));

describe("a scenario spec's timeout", () => {
  it.each(everySpec)("is declared where WebdriverIO reads it, in %s", (name) => {
    const source = spec(name);
    const firstTest = source.indexOf("\n  it(");
    for (
      let at = source.indexOf("this.timeout(");
      at !== -1;
      at = source.indexOf("this.timeout(", at + 1)
    ) {
      expect(
        firstTest === -1 || at < firstTest,
        `${name} calls this.timeout() inside a test body, where WebdriverIO has already read ` +
          `the old value — put it on the describe() instead`,
      ).toBe(true);
    }
  });

  it("gives the fifty-tab spec more than the three minutes that made it red", () => {
    // 180 s is `mochaOpts.timeout`, which is what the spec was silently running under while
    // the macOS runner needed 170–218 s. The check above is what makes this number reach
    // WebdriverIO at all; this one is that the number is still worth reaching.
    const source = spec("stress.e2e.ts");
    expect(source).toContain("this.timeout(BUDGET)");
    const budget = /const BUDGET = (\d+) \* 60_000;/.exec(source);
    expect(budget, "stress.e2e.ts no longer declares a BUDGET in minutes").not.toBeNull();
    expect(Number(budget?.[1]) * 60_000).toBeGreaterThan(180_000);
  });
});
