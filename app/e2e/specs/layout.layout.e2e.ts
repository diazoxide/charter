import { readFileSync } from "node:fs";
import { $, browser, expect } from "@wdio/globals";
import { LAYOUT_FILE, THE_LAYOUT } from "../wdio.layout.conf.js";

/**
 * **A layout written to the file before launch is the first thing the window paints** (M6.9).
 *
 * The arrangement used to be in web storage; it is `charter/layout.json` now, read by the Rust
 * side before the window exists and handed to the page in its initialization script. The risk
 * that design exists to remove is a window that paints the DEFAULT arrangement and then lays
 * itself out again once the file arrives. Where a region is at the end cannot tell those two
 * apart — the second drawing is the one left on screen — so this asks the e2e build's
 * `regionsSeen`, which is attached before React renders and writes down every slot each region
 * has ever been put in. A region drawn on its default side first has two entries.
 *
 * `wdio.layout.conf.ts` writes the file and starts the app on it; nothing here changes it.
 */

type Seen = Record<string, string[]>;

/** Waits until the window has a project in it, so every region has had its chance to draw. */
async function untilAProjectIsDrawn(): Promise<void> {
  await $('[data-testid="explorer"]').waitForExist({
    timeout: 30_000,
    timeoutMsg: "the explorer was never drawn",
  });
  await $('[data-testid="panels"]').waitForExist({ timeout: 30_000 });
}

describe("the layout file", () => {
  it("was handed to the window as it was created, not fetched", async () => {
    await untilAProjectIsDrawn();

    const handed = await browser.execute(
      () =>
        (window as unknown as Record<string, { layout: unknown }>).__CHARTER_AT_CREATION__?.layout,
    );
    expect(handed).toMatchObject({ found: true, trouble: null, document: THE_LAYOUT });
  });

  it("put every region where it says, and nowhere else first", async () => {
    await untilAProjectIsDrawn();

    const seen = await browser.execute(() => window.charterBench?.regionsSeen() ?? null);
    expect(seen).not.toBeNull();
    expect(seen as Seen).toEqual({
      explorer: ["region-right"],
      panels: ["region-left"],
      // `bottom` is put away, so its content was never mounted at all.
    });
  });

  it("sized the slot it names from its first frame", async () => {
    await untilAProjectIsDrawn();

    // The explorer's slot started at the file's 30%, against the catalogue's 16%.
    const share = await browser.execute(() => {
      const row = document
        .querySelector('[data-panel][id="region-upper"]')
        ?.getBoundingClientRect();
      const slot = document
        .querySelector('[data-panel][id="region-right"]')
        ?.getBoundingClientRect();
      return row && slot ? (slot.width / row.width) * 100 : Number.NaN;
    });
    expect(share).toBeGreaterThan(25);
    expect(share).toBeLessThan(35);
  });

  it("is left exactly as the operator wrote it by a launch that changed nothing", async () => {
    await untilAProjectIsDrawn();

    // Read and drawn, never rewritten: a window that re-laid itself out and wrote back what it
    // measured would have replaced the operator's file with its own.
    expect(JSON.parse(readFileSync(LAYOUT_FILE, "utf8"))).toEqual(THE_LAYOUT);
  });
});
