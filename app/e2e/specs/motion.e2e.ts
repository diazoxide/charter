import { browser, expect, $$ } from "@wdio/globals";

/**
 * **The motion layer, against the real app** (M7.2, `app/src/theme/motion.ts`).
 *
 * `motion.test.ts` and `literals.test.ts` hold everything jsdom can: which tokens there are,
 * what a theme may say, that reduced motion collapses every duration, and that no timing is
 * written outside a theme file. What jsdom cannot hold is that any of it **survives the Vite
 * build into a WebView**: jsdom evaluates no stylesheet, so a `var(--motion-duration-quick)`
 * that Tailwind's compiler or Lightning CSS dropped or rewrote would leave every unit test green
 * and the window still.
 *
 * So this reads computed styles, which is the half a scenario can do. It presses nothing and
 * waits on no animation: this driver performs no default action (`docs/ui-primitives.md`), and
 * a test that timed a fade would be a test of the runner's clock.
 *
 * **Whatever the runner's reduced-motion setting is, the assertions hold**, because they ask
 * the page what it decided rather than assuming: with the setting on, every duration the layer
 * wrote is zero and a tab's transition is zero with it; with it off, both are the theme's.
 */

/** Waits until the plane has been read, so the strips have tabs in them to read. */
async function untilTheStripIsRead(): Promise<void> {
  await browser.waitUntil(
    async () =>
      (await $$('[role="tablist"][aria-label="Workspaces"] [role="tab"]').getElements()).length > 0,
    { timeout: 30_000, interval: 250, timeoutMsg: "the strip never listed the fixture plane" },
  );
}

describe("the motion layer", () => {
  it("puts every motion token on the document, collapsed only if the operator asked", async () => {
    await untilTheStripIsRead();

    const read = await browser.execute(() => {
      const root = getComputedStyle(document.documentElement);
      const names = [
        "--motion-duration-quick",
        "--motion-duration-enter",
        "--motion-duration-settle",
        "--motion-duration-spin",
        "--motion-duration-breathe",
        "--motion-easing-standard",
        "--motion-easing-enter",
        "--motion-easing-steady",
        "--motion-easing-breathe",
      ];
      return {
        reduced: matchMedia("(prefers-reduced-motion: reduce)").matches,
        values: Object.fromEntries(names.map((name) => [name, root.getPropertyValue(name).trim()])),
      };
    });

    for (const [name, value] of Object.entries(read.values)) {
      if (name.includes("-duration-")) {
        expect({ name, value }).toEqual({ name, value: expect.stringMatching(/^\d+ms$/) });
        const ms = Number.parseFloat(value);
        if (read.reduced) expect(ms).toBe(0);
        else expect(ms).toBeGreaterThan(0);
      } else {
        expect({ name, value }).toEqual({ name, value: expect.stringMatching(/^cubic-bezier\(/) });
      }
    }
  });

  it("gives a tab the transition its token says, through the built stylesheet", async () => {
    // The consumer half: a rule in `App.css` that reads the token, compiled by the real build
    // and resolved by the real engine. A rule that emitted nothing reads `0s` and `ease` here —
    // the engine's defaults — and a token the rule could not resolve reads the same.
    await untilTheStripIsRead();

    const read = await browser.execute(() => {
      const tab = document.querySelector('[role="tablist"][aria-label="Workspaces"] [role="tab"]');
      if (tab === null) return null;
      const css = getComputedStyle(tab);
      const root = getComputedStyle(document.documentElement);
      return {
        property: css.transitionProperty,
        duration: css.transitionDuration,
        easing: css.transitionTimingFunction,
        token: root.getPropertyValue("--motion-duration-quick").trim(),
        curve: root.getPropertyValue("--motion-easing-standard").trim(),
      };
    });

    expect(read).not.toBeNull();
    if (read === null) return;
    expect(read.property).toContain("background-color");
    // The engine reports seconds and the token is milliseconds; the same number either way.
    expect(Number.parseFloat(read.duration) * 1000).toBeCloseTo(Number.parseFloat(read.token), 3);
    // Numbers compared rather than strings, because an engine may print `0.2` as `.2`.
    const numbers = (text: string) => (text.match(/-?\d*\.?\d+/g) ?? []).map(Number);
    expect(numbers(read.easing)).toEqual(numbers(read.curve));
  });
});
