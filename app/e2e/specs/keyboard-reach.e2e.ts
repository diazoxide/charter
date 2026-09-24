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

  it("makes each strip ONE stop: exactly one tab in it says 0, the selected one", async () => {
    const strips = await browser.execute(() =>
      [...document.querySelectorAll('[role="tablist"]')].map((strip) => {
        const tabs = [...strip.querySelectorAll('[role="tab"]')];
        const stops = tabs.filter((tab) => tab.getAttribute("tabindex") === "0");
        return {
          strip: strip.getAttribute("aria-label"),
          stops: stops.length,
          selected: stops.every((tab) => tab.getAttribute("aria-selected") === "true"),
          rest: tabs.every((tab) => stops.includes(tab) || tab.getAttribute("tabindex") === "-1"),
        };
      }),
    );
    expect(strips.map((one) => one.strip)).toEqual(["Projects", "Workspaces", "Tabs"]);
    for (const one of strips) expect(one).toEqual({ ...one, stops: 1, selected: true, rest: true });
  });

  it("puts the regions in the order they are drawn, and the pane's controls before its terminal", async () => {
    const order = await browser.execute(() => {
      const regions: [string, string][] = [
        ["title bar", ".title-bar"],
        ["projects", '[role="tablist"][aria-label="Projects"]'],
        ["workspaces", '[role="tablist"][aria-label="Workspaces"]'],
        ["chats", '[role="tablist"][aria-label="Tabs"]'],
        ["pane controls", ".pane-frame .pane-doing"],
        ["terminal", '[data-testid="pane"]'],
        ["status line", ".status-line"],
      ];
      // The first stop inside each, in document order: what WebKit's sequence reaches first.
      const stops = [...document.querySelectorAll<HTMLElement>("[tabindex]")].filter(
        (el) => el.tabIndex >= 0 && !el.hasAttribute("disabled"),
      );
      return stops
        .map((el) => regions.find(([, where]) => el.closest(where))?.[0])
        .filter((name, at, all): name is string => name !== undefined && all[at - 1] !== name);
    });
    expect(order).toEqual([
      "title bar",
      "projects",
      "workspaces",
      "chats",
      "pane controls",
      "terminal",
      "status line",
    ]);
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
