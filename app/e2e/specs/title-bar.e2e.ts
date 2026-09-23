import { basename } from "node:path";
import { browser, expect, $, $$ } from "@wdio/globals";

/**
 * **The window's title bar**, against the real app (`app/src/TitleBar.tsx`).
 *
 * `TitleBar.test.tsx` holds the readings — what each degraded breadcrumb says, what `running`
 * counts, what About draws when the core refuses. Those are props-in/markup-out and jsdom
 * settles them. Three things it cannot settle are here:
 *
 * - **The bar is really the first thing in the window**, above the project strip and touching
 *   the top edge. jsdom lays nothing out, so document order is the most it could assert, and
 *   document order is exactly what would still be true of a bar drawn 200 px down.
 * - **The breadcrumb names the project the app really opened**, out of the real core, rather
 *   than a string a test handed it.
 * - **The attributes the shipped bundle carries.** A jsdom test renders components; an
 *   operator gets a Vite build inside a WKWebView, and an attribute a transform dropped would
 *   leave every unit test green and the bar undraggable. This is `picker.e2e.ts`'s split for
 *   `tabindex`, applied to `data-tauri-drag-region`.
 *
 * **What no test anywhere proves is that the window then MOVES when the bar is dragged.** This
 * driver dispatches a synthetic event and performs no default action of any kind
 * (`docs/ui-primitives.md`), and a synthetic `mousedown` that did reach Tauri's listener would
 * end in an IPC call rather than an observable drag. The three facts that decision rests on are
 * each checked where they live: the attribute is here, the controls' `tabindex` is here, and
 * `core:window:allow-start-dragging` — which `core:default` does **not** include — is named in
 * `capabilities/default.json`, where a missing permission fails the call at runtime rather than
 * at build time. That last one is the sharpest thing to attack in this change.
 *
 * It changes nothing it does not put back: one app process serves the whole scenario run.
 */

/** Waits until the plane has been read, so a workspace can be focused. */
async function untilTheStripIsRead(): Promise<void> {
  await browser.waitUntil(
    async () => {
      const names = await $$(
        '[role="tablist"][aria-label="Workspaces"] [role="tab"] .workspace-name',
      ).getElements();
      return (
        (await Promise.all([...names].map((name) => name.getText()))).join(",") === "alpha,beta"
      );
    },
    { timeout: 30_000, interval: 250, timeoutMsg: "the strip never listed the fixture plane" },
  );
}

/** Focuses a workspace from the strip, by the tab's own name rather than by its whole text —
 *  a strip tab carries counts beside its name. */
async function focus(workspace: string): Promise<void> {
  const tabs = await $$('[role="tablist"][aria-label="Workspaces"] [role="tab"]').getElements();
  for (const tab of tabs) {
    if ((await tab.$(".workspace-name").getText()) === workspace) {
      await tab.click();
      return;
    }
  }
  throw new Error(`no ${workspace} on the workspace strip`);
}

/** The plane the app says it is acting on — the copy this run was given, never the repo's. */
async function planeRoot(): Promise<string> {
  const said = await $("span.plane code");
  await said.waitForDisplayed({ timeout: 30_000 });
  return (await said.getText()).trim();
}

/** Waits for the breadcrumb to say something, and says what it did say when it never does. */
async function untilTheBarSays(want: string | RegExp): Promise<void> {
  const crumbs = await $('[data-testid="title-crumbs"]');
  await crumbs.waitForExist({ timeout: 20_000 });
  let last = "";
  try {
    await browser.waitUntil(
      async () => {
        last = await crumbs.getText();
        return typeof want === "string" ? last.includes(want) : want.test(last);
      },
      { timeout: 30_000, interval: 250 },
    );
  } catch {
    throw new Error(`the title bar never said ${want.toString()}; it said ${JSON.stringify(last)}`);
  }
}

describe("the title bar", () => {
  it("is the first thing in the window, above the project strip and against the top edge", async () => {
    await untilTheStripIsRead();

    const bar = await $('[data-testid="title-bar"]');
    await bar.waitForDisplayed({ timeout: 20_000 });
    const top = (await bar.getLocation("y")) as number;
    const height = (await bar.getSize("height")) as number;
    // Nothing above it. A pixel of slack, because a fractional layout rounds.
    expect(top).toBeLessThanOrEqual(1);
    // And the project strip begins where it ends, rather than under it or over it.
    const strip = await $('[role="tablist"][aria-label="Projects"]');
    expect((await strip.getLocation("y")) as number).toBeGreaterThanOrEqual(top + height - 1);
  });

  it("is tall enough for the window controls macOS floats over it", async () => {
    // Under `titleBarStyle: "Overlay"` the traffic lights are drawn at the system's own title
    // bar height whatever this element does, so a bar shorter than that has them overlapping
    // the project strip below. The bound is a floor with room in it, not a pin on a font
    // metric — and it holds on Linux too, where there is nothing floating over anything and a
    // one-line row is still the shape this is.
    await untilTheStripIsRead();

    const height = (await (await $('[data-testid="title-bar"]')).getSize("height")) as number;
    expect(height).toBeGreaterThanOrEqual(26);
    expect(height).toBeLessThanOrEqual(48);
  });

  it("names the project the app really opened, and follows the workspace strip", async () => {
    await untilTheStripIsRead();
    const name = basename(await planeRoot());

    await untilTheBarSays(name);
    await focus("alpha");
    await untilTheBarSays("alpha");

    await focus("beta");
    await untilTheBarSays("beta");

    // Put back, because one app process serves every spec after this one.
    await focus("alpha");
    await untilTheBarSays("alpha");
  });

  it("counts chats that are RUNNING rather than chats that are open", async () => {
    // **This suite's harness carries no state hook at all** — the ones that report through
    // hooks run under `wdio.state.conf.ts`, because the app reads `SHELL` once and one run is
    // one harness. So however many chats the specs before this one left open, every one of
    // them reads `unknown`, and that makes this the one place in the scenario suite where the
    // two numbers are visibly different: a tab strip with chats on it, and a bar that honestly
    // says none of them is running.
    //
    // It is also the case for zero being a sentence here rather than a dropped cell
    // (`TitleBar.tsx` argues why that differs from the status line's rule): the clause is
    // still there to read.
    await untilTheStripIsRead();

    const clause = await $('[data-testid="title-crumbs"] .crumb-chats');
    await clause.waitForExist({ timeout: 20_000 });
    // A settled reading first, so this is never asserted against "counting chats…".
    await browser.waitUntil(async () => (await clause.getAttribute("data-running")) !== "unknown", {
      timeout: 30_000,
      timeoutMsg: "the title bar never finished counting what was running",
    });

    // The tab count travels into the assertion so a failure says how many chats were on the
    // strip at the moment the bar disagreed with it.
    const open = (await $$('.bar .tabs [role="tab"]').getElements()).length;
    expect(`${await clause.getText()} · ${open} chat tabs open`).toBe(
      `no chats running · ${open} chat tabs open`,
    );
  });

  it("carries the drag region the shipped bundle has to keep, and keeps it off its controls", async () => {
    // `deep` and not the bare attribute: Tauri's handler
    // (`tauri/src/window/scripts/drag.js`) walks the composed path up from what was pressed,
    // and a bare attribute drags only on a DIRECT press of the element carrying it — which on
    // a bar made of text spans is everywhere except on its own words. The same walk returns
    // false at the first CLICKABLE element, and `BUTTON` is in its `CLICKABLE_TAGS` — so the
    // tag alone is what keeps a control pressable and **no attribute of ours is load-bearing
    // here**.
    //
    // **This test was written asserting `tabindex="0"` on every control and the real app
    // refuted it**, which is the reason it is worded the way it is now: the update item's
    // trigger carries none. That is charter-app#189's — every button outside a dialog is in
    // the same position, and #189 argues at length that the answer there is a tab order and
    // not `tabIndex={0}` sprinkled on one more button. About carries one because it is a
    // button this change ADDED and the standing rule applies to it; the two disagreeing is
    // that open issue showing through, not a defect in this bar.
    await untilTheStripIsRead();

    const shape = await browser.execute(() => {
      const bar = document.querySelector('[data-testid="title-bar"]');
      if (!bar) return { bar: "(the title bar was not on screen)", controls: [] as string[] };
      return {
        bar: bar.getAttribute("data-tauri-drag-region") ?? "(none)",
        controls: [...bar.querySelectorAll('button, a, [role="button"]')].map(
          (el) =>
            `${(el.getAttribute("data-testid") || el.textContent || "?").trim()}` +
            `=<${el.tagName.toLowerCase()}>` +
            `/${el.hasAttribute("data-tauri-drag-region") ? "drags" : "presses"}`,
        ),
      };
    });

    expect(shape.bar).toBe("deep");
    // About and the update item. A bar that drew neither would pass a filter over an empty
    // list, which is the failure this guards against first.
    expect(shape.controls.length).toBeGreaterThanOrEqual(2);
    // Every one of them, and the failure names which one lost it rather than saying "false".
    expect(shape.controls.filter((said) => !said.endsWith("=<button>/presses"))).toEqual([]);
  });

  it("carries the updater, which is on this bar now and no longer on the status line", async () => {
    // Moved rather than copied (`StatusLine.tsx` argues it): an offer is a fact about the app
    // and the status line is drawn once per open project, so eight projects meant eight
    // updater clients reporting one thing. A scenario build never checks on its own
    // (`charter_core::updates::checks_on_its_own`), so nothing is on offer: the button is the
    // way in to the channel, with no words.
    await untilTheStripIsRead();

    const update = await $('[data-testid="title-bar"] [data-testid="status-update"]');
    await update.waitForExist({ timeout: 20_000 });
    await browser.waitUntil(
      async () => ((await update.getAttribute("aria-label")) ?? "").includes("channel"),
      { timeout: 20_000, timeoutMsg: "the update button never said which channel it is on" },
    );
    expect(await update.getAttribute("aria-label")).toContain("nothing new known");

    // And it is not drawn twice. The status line still carries the plane's PIN, which is a
    // different question about a different thing (charter ADR 0030).
    expect(await $('[data-testid="status-line"] [data-testid="status-update"]').isExisting()).toBe(
      false,
    );
  });

  it("opens About on the version this build really shipped, with that version's own news", async () => {
    // The whole point of the dialog, and the half jsdom cannot reach: the corpus is compiled
    // into the binary by `build.rs`, so this is the real `news::for_version(shipped_version())`
    // rather than a list a mock handed it. `about.rs` holds the invariant that makes an empty
    // list impossible — the version is DERIVED from the entries.
    await untilTheStripIsRead();

    await (await $('[data-testid="title-about"]')).click();
    const dialog = await $('[role="dialog"][aria-describedby="about-what"]');
    await dialog.waitForDisplayed({ timeout: 20_000 });

    const version = await (await $('[data-testid="about-version"]')).getText();
    // A release charter really named, rather than the crate's own `0.1.0` — `news.rs` argues
    // at length why those are different numbers.
    expect(version).toMatch(/^\d+\.\d+\.\d+/);
    await expect(dialog).toHaveText(expect.stringContaining(`What ${version} brought`));

    const headlines = await $$('[role="dialog"] .about-news li strong').getElements();
    expect(headlines.length).toBeGreaterThan(0);
    expect((await headlines[0].getText()).trim().length).toBeGreaterThan(0);

    // Never left over the window: one app process serves the whole run, and a scrim left up
    // blocks every spec after this one.
    await browser.keys(["Escape"]);
    await browser.waitUntil(async () => !(await dialog.isExisting()), {
      timeout: 20_000,
      timeoutMsg: "the About dialog did not close",
    });
  });
});
