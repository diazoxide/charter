import { basename } from "node:path";
import { browser, expect, $, $$ } from "@wdio/globals";

/**
 * **The window's title bar**, against the real app (`app/src/TitleBar.tsx`, ADR 0054).
 *
 * `TitleBar.test.tsx` holds what jsdom settles — that the project strip is in the bar with its
 * counts, its show-more and its `+`, that the breadcrumb is gone, that the right-hand end is
 * there, and what About draws when the core refuses. Four things it cannot settle are here:
 *
 * - **The bar is really the first thing in the window**, touching the top edge, with the
 *   project strip inside it and the workspace strip under it. jsdom lays nothing out, so
 *   document order is the most it could assert, and document order is exactly what would
 *   still be true of a bar drawn 200 px down.
 * - **The strip names the project the app really opened**, out of the real core, rather than
 *   a string a test handed it.
 * - **A stretch of the bar is left to grab** between the strip and the right-hand end, and a
 *   press there lands on the bar itself rather than on a control.
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

/** The plane the app says it is acting on — the copy this run was given, never the repo's. */
async function planeRoot(): Promise<string> {
  const said = await $("span.plane code");
  await said.waitForDisplayed({ timeout: 30_000 });
  return (await said.getText()).trim();
}

/** The strip in the bar, by its role and name. */
const PROJECTS = '[data-testid="title-bar"] [role="tablist"][aria-label="Projects"]';

describe("the title bar", () => {
  it("is the first thing in the window, against the top edge, with the project strip in it", async () => {
    await untilTheStripIsRead();

    const bar = await $('[data-testid="title-bar"]');
    await bar.waitForDisplayed({ timeout: 20_000 });
    const top = (await bar.getLocation("y")) as number;
    const height = (await bar.getSize("height")) as number;
    // Nothing above it. A pixel of slack, because a fractional layout rounds.
    expect(top).toBeLessThanOrEqual(1);
    // The project strip is inside it rather than a row of its own (ADR 0054)…
    const strip = await $(PROJECTS);
    await strip.waitForDisplayed({ timeout: 20_000 });
    const stripTop = (await strip.getLocation("y")) as number;
    expect(stripTop).toBeGreaterThanOrEqual(top - 1);
    expect(stripTop + ((await strip.getSize("height")) as number)).toBeLessThanOrEqual(
      top + height + 1,
    );
    // …and the workspace strip begins where the bar ends, rather than under it or over it.
    const workspaces = await $('[role="tablist"][aria-label="Workspaces"]');
    expect((await workspaces.getLocation("y")) as number).toBeGreaterThanOrEqual(top + height - 1);
  });

  it("is tall enough for the window controls macOS floats over it", async () => {
    // Under `titleBarStyle: "Overlay"` the traffic lights are drawn at the system's own title
    // bar height whatever this element does, so a bar shorter than that has them overlapping
    // the workspace strip below. The bound is a floor with room in it, not a pin on a font
    // metric — and it holds on Linux too, where there is nothing floating over anything and a
    // one-line row is still the shape this is.
    await untilTheStripIsRead();

    const height = (await (await $('[data-testid="title-bar"]')).getSize("height")) as number;
    expect(height).toBeGreaterThanOrEqual(26);
    expect(height).toBeLessThanOrEqual(48);
  });

  it("names the project the app really opened, on the selected tab of the strip in it", async () => {
    await untilTheStripIsRead();
    const root = await planeRoot();
    const name = basename(root);

    // The selected tab carries the project's whole path as its title, and its name is the
    // directory's.
    const selected = await $(`${PROJECTS} [aria-selected="true"]`);
    await selected.waitForExist({ timeout: 20_000 });
    expect(await selected.getAttribute("title")).toBe(root);
    expect(await selected.getText()).toContain(name);
  });

  it("leaves a stretch to grab between the project strip and its right-hand end", async () => {
    // The tabs give way before the right-hand end does, and never take the whole of what is
    // left: `--title-bar-drag` is kept free so a window full of projects can still be moved.
    // A press in the middle of that stretch has to land on the bar itself, which is the
    // element carrying `data-tauri-drag-region="deep"`, and not on a control.
    await untilTheStripIsRead();

    const seen = await browser.execute((strip: string) => {
      const bar = document.querySelector<HTMLElement>('[data-testid="title-bar"]');
      const tabs = document.querySelector(strip);
      const end = bar?.querySelector(".title-bar-doing");
      if (!bar || !tabs || !end) return { gap: -1, least: 0, pressed: "(nothing to measure)" };
      const rem = Number.parseFloat(getComputedStyle(document.documentElement).fontSize);
      const least =
        Number.parseFloat(getComputedStyle(bar).getPropertyValue("--title-bar-drag")) * rem;
      const from = tabs.getBoundingClientRect();
      const to = end.getBoundingClientRect();
      const hit = document.elementFromPoint((from.right + to.left) / 2, from.top + from.height / 2);
      return {
        gap: to.left - from.right,
        least,
        pressed: hit === bar ? "the bar" : (hit?.outerHTML.slice(0, 80) ?? "(nothing)"),
      };
    }, PROJECTS);

    expect(seen.least).toBeGreaterThan(0);
    expect(seen.gap).toBeGreaterThanOrEqual(seen.least - 1);
    expect(seen.pressed).toBe("the bar");
  });

  it("leaves the project strip room for two tabs in a narrow window, whatever the save indicator says", async () => {
    // The right-hand end never gives way (ADR 0054), so whatever it spends the tabs lose. A
    // save indicator saying a whole sentence once left a 1024 px window room for ONE project
    // tab, and `projects.e2e.ts` failed on the macOS runner; `.save-indicator-words` in
    // `App.css` has the numbers. Every fixture plane is not a git repository, so the bar here
    // says that sentence.
    //
    // **Room, measured the way the strip measures it** (`fits.useRoom`): the strip's width
    // less its own controls, against the floor each tab is drawn at (`--least`). Two tabs fit
    // exactly when that room is at least twice the floor (`fits.capacity`).
    await untilTheStripIsRead();
    await $('[data-testid="title-bar"] [aria-label^="Saving:"]').waitForExist({
      timeout: 20_000,
    });

    const was = await browser.getWindowSize();
    await browser.setWindowSize(1024, 768);
    try {
      let seen = { window: 0, room: 0, least: 0 };
      await browser.waitUntil(
        async () => {
          seen = await browser.execute((strip: string) => {
            const tabs = document.querySelector<HTMLElement>(strip);
            const controls = tabs?.querySelector<HTMLElement>(".strip-doing");
            return {
              window: window.innerWidth,
              room: (tabs?.clientWidth ?? 0) - (controls?.offsetWidth ?? 0),
              least: Number.parseFloat(tabs?.style.getPropertyValue("--least") ?? "") || 0,
            };
          }, PROJECTS);
          return seen.window <= 1024;
        },
        { timeout: 20_000, timeoutMsg: "the window never became 1024 px wide" },
      );

      expect(seen.least).toBeGreaterThan(0);
      expect(seen.room).toBeGreaterThanOrEqual(2 * seen.least);
    } finally {
      // One app process serves the whole run, and every spec after this one expects the
      // window it was built for.
      await browser.setWindowSize(was.width, was.height);
    }
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
    // About, the update item, and the project strip's tab, `×`, `+` buttons and show-more
    // (ADR 0054) — each a `<button>`, so the walk stops at it and the tabs need no drag rule of
    // their own. A bar that drew none would pass a filter over an empty list, which is the
    // failure this guards against first.
    expect(shape.controls.length).toBeGreaterThanOrEqual(4);
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
    // different question about a different thing (ADR 0030).
    expect(await $('[data-testid="status-line"] [data-testid="status-update"]').isExisting()).toBe(
      false,
    );
  });

  it("opens About on the version this build really is, with its changelog section", async () => {
    // The half jsdom cannot reach: the version is the bundle's own (`app.package_info()`) and
    // the notes are CHANGELOG.md as compiled into the binary, rather than an answer a mock
    // handed it. Before this, About said charter's news corpus version (0.62.1) on a build
    // whose release page said 0.1.0.
    await untilTheStripIsRead();

    await (await $('[data-testid="title-about"]')).click();
    const dialog = await $('[role="dialog"][aria-describedby="about-what"]');
    await dialog.waitForDisplayed({ timeout: 20_000 });

    const version = await (await $('[data-testid="about-version"]')).getText();
    // The app's own version line, which starts at 0.1.0 — not Python charter's 0.62.
    expect(version).toMatch(/^0\.\d+\.\d+/);
    expect(version).not.toMatch(/^0\.6\d\./);

    // A scenario build carries the crate's version with no dev suffix: its own section once
    // that version is released, `[Unreleased]` before — and `about.rs` has the test that keeps
    // one of the two in the file. Right after a release `[Unreleased]` is empty, and the dialog
    // says so rather than drawing nothing.
    const said = await dialog.getText();
    if (!said.includes("Nothing is recorded for it yet.")) {
      const items = await $$('[role="dialog"] .release-notes li').getElements();
      expect(items.length).toBeGreaterThan(0);
      expect((await items[0].getText()).trim().length).toBeGreaterThan(0);
    }

    // Never left over the window: one app process serves the whole run, and a scrim left up
    // blocks every spec after this one.
    await browser.keys(["Escape"]);
    await browser.waitUntil(async () => !(await dialog.isExisting()), {
      timeout: 20_000,
      timeoutMsg: "the About dialog did not close",
    });
  });
});
