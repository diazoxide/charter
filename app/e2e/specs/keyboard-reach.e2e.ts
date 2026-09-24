import { browser, expect, $ } from "@wdio/globals";
import { endEveryChat, pressAndStart } from "../opening.js";

/**
 * **Where Tab goes in the window outside its dialogs**, in the shipped app (charter-app#189).
 *
 * `Window.keyboard.test.tsx` holds the order: every stop, one per strip and per list, top to
 * bottom and left to right, against the engine's rule written down. This is `picker.e2e.ts`'s
 * split applied to the whole window — what jsdom cannot see:
 *
 * - **the attributes survive the build.** An operator gets a Vite bundle inside a WKWebView or
 *   WebKitGTK, and a `tabindex` a transform dropped would leave every unit test green;
 * - **the focused pane's controls are drawn while the keyboard is still on its way to them.**
 *   That is a stylesheet rule (`:has(.pane.focused)`), and jsdom computes no stylesheet — a
 *   control that is `visibility: hidden` is not in the tab sequence at all;
 * - **Ctrl+Tab leaves a real terminal**, xterm's own textarea and not a stand-in.
 *
 * **What no scenario can do is press Tab and watch the focus move.** This driver dispatches a
 * synthetic keydown and performs no default action of any kind (`docs/ui-primitives.md`,
 * measured in #186), and it does not deliver a modifier chord at all (#176). So the Ctrl+Tab
 * below is a dispatched `KeyboardEvent`, as `workspace-lifecycle.e2e.ts` dispatches its
 * `contextmenu`, and that WebKit then walks the sequence it was handed is left to a person.
 *
 * **It asks by tag and by class, against AGENTS.md's "by role" rule, and on purpose.** The
 * rule protects a spec from how a control is drawn; this spec is ABOUT how it is drawn. WebKit's
 * gate is `HTMLFormControlElement` — the `<button>` tag, not the `button` role — and xterm's
 * `<textarea>` is the element the keyboard lands in. The regions are found by the classes their
 * own components give them because a region has no role that names it in every arrangement.
 */
describe("the window's keyboard reach", () => {
  before(async () => {
    await pressAndStart("New tab");
    await $('[data-testid="pane"] textarea').waitForExist({ timeout: 20_000 });
  });

  after(async () => {
    await endEveryChat();
  });

  it("writes a tabindex on every button and summary outside the dialogs", async () => {
    const unsaid = await browser.execute(() =>
      [...document.querySelectorAll("button, summary")]
        .filter((el) => !el.closest('[role="dialog"], [role="alertdialog"]'))
        .filter((el) => !el.hasAttribute("tabindex"))
        .map((el) => (el.getAttribute("aria-label") ?? el.textContent ?? "").trim().slice(0, 40)),
    );
    expect(unsaid).toEqual([]);
  });

  it("makes each strip ONE stop: exactly one tab says 0, the selected one when it is drawn", async () => {
    const strips = await browser.execute(() =>
      [...document.querySelectorAll('[role="tablist"]')].map((strip) => {
        const tabs = [...strip.querySelectorAll('[role="tab"]')];
        const stops = tabs.filter((tab) => tab.getAttribute("tabindex") === "0");
        const selected = tabs.find((tab) => tab.getAttribute("aria-selected") === "true");
        return {
          strip: strip.getAttribute("aria-label"),
          stops: stops.length,
          // A selected tab the strip collapsed into its menu is not drawn, and then the first
          // drawn tab is the stop (`roving.ts`).
          selectedIsTheStop: selected === undefined || stops[0] === selected,
          rest: tabs.every((tab) => stops.includes(tab) || tab.getAttribute("tabindex") === "-1"),
        };
      }),
    );
    expect(strips.map((one) => one.strip)).toEqual(["Projects", "Workspaces", "Tabs"]);
    for (const one of strips)
      expect(one).toEqual({ ...one, stops: 1, selectedIsTheStop: true, rest: true });
  });

  it("reaches the regions in the order they are laid out, top to bottom and left to right", async () => {
    const { tabbed, laidOut, controlsFirst } = await browser.execute(() => {
      const regions: [string, string][] = [
        ["title bar", ".title-bar"],
        ["projects", '[role="tablist"][aria-label="Projects"]'],
        ["workspaces", '[role="tablist"][aria-label="Workspaces"]'],
        ["chats", '[role="tablist"][aria-label="Tabs"]'],
        ["explorer", 'nav[aria-label="Explorer"]'],
        ["panes", ".panes"],
        ["attention", '[data-testid="panels"]'],
        ["status line", ".status-line"],
      ];
      const stops = [...document.querySelectorAll<HTMLElement>("[tabindex]")].filter(
        (el) => el.tabIndex >= 0 && !el.hasAttribute("disabled"),
      );
      // The regions in the order the TAB SEQUENCE first enters each one…
      const tabbed = stops
        .map((el) => regions.find(([, where]) => el.closest(where))?.[0])
        .filter((name, at, all): name is string => name !== undefined && all.indexOf(name) === at);
      // …and in the order the WINDOW draws them, measured: rows first, then left to right.
      // This is the half jsdom cannot ask — it lays nothing out — and it is what "reading
      // order" means whichever side the arrangement put a region on (ADR 0038).
      const laidOut = regions
        .map(([name, where]) => {
          const box = document.querySelector(where)?.getBoundingClientRect();
          return { name, box };
        })
        .filter((one) => one.box && one.box.width > 0 && tabbed.includes(one.name))
        .sort((a, b) => {
          const [x, y] = [a.box as DOMRect, b.box as DOMRect];
          // Regions side by side overlap vertically; only one that starts below the other's
          // bottom edge is a later row.
          if (y.top >= x.bottom - 1) return -1;
          if (x.top >= y.bottom - 1) return 1;
          return x.left - y.left;
        })
        .map((one) => one.name);
      // And inside the focused pane, its own controls before its terminal.
      const frame = document.querySelector(".pane-frame:has(.pane.focused)");
      const inFrame = frame ? stops.filter((el) => frame.contains(el)) : [];
      const controlsFirst =
        inFrame.length > 0 &&
        inFrame[0].closest(".pane-doing") !== null &&
        inFrame[inFrame.length - 1].closest('[data-testid="pane"]') !== null;
      return { tabbed, laidOut, controlsFirst };
    });
    expect(tabbed).toEqual(laidOut);
    expect(tabbed.slice(0, 4)).toEqual(["title bar", "projects", "workspaces", "chats"]);
    expect(tabbed[tabbed.length - 1]).toBe("status line");
    expect(controlsFirst).toBe(true);
  });

  it("draws the focused pane's controls while the keyboard is somewhere else", async () => {
    const drawn = await browser.execute(() => {
      (
        document.querySelector('[role="tablist"][aria-label="Tabs"] [tabindex="0"]') as HTMLElement
      ).focus();
      const doing = document.querySelector(".pane-frame:has(.pane.focused) .pane-doing");
      return doing ? getComputedStyle(doing).visibility : "(no focused pane)";
    });
    expect(drawn).toBe("visible");
  });

  it("leaves a terminal on Ctrl+Tab, and keeps a plain Tab in it", async () => {
    const moved = await browser.execute(() => {
      const terminal = document.querySelector<HTMLTextAreaElement>('[data-testid="pane"] textarea');
      if (!terminal) return { tab: "(no terminal)", ctrlTab: "(no terminal)" };
      const where = () =>
        document.activeElement?.closest('[data-testid="pane"]') ? "terminal" : "outside";
      terminal.focus();
      const tab = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
      terminal.dispatchEvent(tab);
      const afterTab = where();
      terminal.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "Tab",
          ctrlKey: true,
          bubbles: true,
          cancelable: true,
        }),
      );
      return { tab: afterTab, ctrlTab: where() };
    });
    expect(moved).toEqual({ tab: "terminal", ctrlTab: "outside" });
  });
});
