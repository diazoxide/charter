import { browser, expect, $, $$ } from "@wdio/globals";
import { READY } from "../harness.js";
import { harnessRowsDrawn, pickAndStart, pressOnly } from "../opening.js";

/**
 * Picking a harness profile and a persona, against the real app in a copy of the `daily`
 * fixture plane, with a real `charter.local.toml` beside it.
 *
 * Nothing here is stubbed. The app reads the plane, asks the operator to approve a command it
 * has never run, and only then starts anything, armed with the plugin the app ships. The
 * profile's program is a wrapper that writes down what it was started with and then runs the
 * fake harness — which is also the shape ADR 0022 names, a program that is not called
 * `claude`.
 *
 * Every test here is independent of the order the specs run in. They share one app process,
 * and an approval is recorded per profile, so the approval test picks a profile of its own.
 */

const dialog = () => $('[role="dialog"]');

describe("starting a chat", () => {
  it("asks which profile and which persona, and starts nothing until a row is picked", async () => {
    const panes = (await $$('[data-testid="pane"]').getElements()).length;

    await pressOnly("New tab");

    await expect(dialog()).toBeDisplayed();
    // Both come off the plane on disk: the profiles it declares, and the personas it has.
    await expect(dialog()).toHaveText(expect.stringContaining("scenario"));
    await expect(dialog()).toHaveText(expect.stringContaining("steward"));
    // Nothing has started while the question is still on screen.
    expect(await $$('[data-testid="pane"]').getElements()).toHaveLength(panes);
  });

  it("escapes having started nothing", async () => {
    const panes = (await $$('[data-testid="pane"]').getElements()).length;

    await browser.keys(["Escape"]);

    await expect(dialog()).not.toBeDisplayed();
    expect(await $$('[data-testid="pane"]').getElements()).toHaveLength(panes);
  });

  it("ships every answer with the tabindex the engine's tab sequence needs", async () => {
    // **Half of charter-app#186's fix, checked where jsdom cannot see it: in the built app.**
    //
    // The fix is one attribute. WebKit leaves a `<button>` out of the tab sequence unless full
    // keyboard access is on **or** its `tabindex` is written down —
    // `HTMLFormControlElement::isKeyboardFocusable` hands over to
    // `Element::isKeyboardFocusable` the moment `tabIndexSetExplicitly()`, and never consults
    // the gate. `Modals.keyboard.test.tsx` walks every modal against a model of that rule.
    //
    // **What a model cannot tell you is whether the attribute survives the build.** A jsdom
    // test renders the components; an operator gets a Vite bundle inside a WKWebView, and an
    // attribute that a transform dropped would leave every one of those tests green and the
    // window exactly as broken as before. This asserts the other half: the real app, the real
    // bundle, the real DOM.
    //
    // **What it deliberately does NOT assert is that Tab then moves.** That was tried and it
    // cannot be: this driver delivers a keydown and performs no default action at all, so Tab
    // does not move the focus here even between two plain text `<input>`s — measured, and
    // written up in `docs/ui-primitives.md` as the third face of the finding that already
    // explains why a synthesised `Enter` does not press a button. Whether WebKit honours a
    // written-down `tabindex` is WebKit's rule, cited in that file; whether charter writes one
    // down is this test.
    await pressOnly("New tab");
    await harnessRowsDrawn();

    const answers = await browser.execute(() => {
      const box = document.querySelector('[role="dialog"]');
      if (!box) return ["(the picker was not on screen)"];
      // The footer checkbox and the two answers — every control in this dialog that is a
      // `<button>` without a roving `tabindex` of its own, which is to say every one the
      // engine would otherwise skip.
      return [...box.querySelectorAll(".box, .answer button")].map(
        (el) =>
          `${(el.textContent || el.getAttribute("role") || "").trim() || "checkbox"}=${el.getAttribute("tabindex")}`,
      );
    });

    // Every one of them, and the failure names which lost it rather than saying "false".
    expect(answers.filter((said) => !said.endsWith("=0"))).toEqual([]);

    await browser.keys(["Escape"]);
    await expect(dialog()).not.toBeDisplayed();
  });

  it("shows the command of a profile charter has never run, and asks before running it", async () => {
    // The file is gitignored, so an edit to it leaves no diff for a reviewer to catch —
    // which is why the ask is about the words that are about to run, not the profile's name.
    await pressOnly("New tab");
    // The label is a real `<label for>` tied to the row's control, so clicking the words picks
    // the row — which is the association that was missing.
    await harnessRowsDrawn();
    await (await $("label*=needs-approval")).click();

    await expect(dialog()).toHaveText(expect.stringContaining("claude-stand-in"));
    await expect($("button=Approve and start")).toBeDisplayed();

    await pickAndStart();
    const pane = await $('[data-testid="pane"]');
    await pane.waitForDisplayed({ timeout: 30_000 });
    await browser.waitUntil(async () => (await pane.getText()).includes(READY), {
      timeout: 30_000,
      interval: 250,
      timeoutMsg: "the harness the profile names never reached the pane",
    });
  });

  it("names the profile and the persona on the chat in the explorer", async () => {
    // The profile AND its kind. The kind is the one the profile declares, not one read off
    // the program's name — the program here is `claude-stand-in`, a wrapper, and
    // `Harness::of_command` answers `None` for one exactly as it does for a shell.
    //
    // In the explorer since ADR 0038: the chats are listed under the spot each one
    // works in, and the left region is `nav[aria-label="Explorer"]`.
    const explorer = await $('nav[aria-label="Explorer"]');
    await explorer.waitForDisplayed({ timeout: 20_000 });

    let said = "";
    await browser
      .waitUntil(
        async () => {
          said = await explorer.getText();
          return said.includes("needs-approval") && said.includes("(claude)");
        },
        {
          timeout: 20_000,
          interval: 250,
          // What it actually said, so a failure here is one somebody can act on rather than
          // one they have to reproduce.
          timeoutMsg: "the explorer never named the profile the chat started on",
        },
      )
      .catch((why: unknown) => {
        // What it actually said, so a failure here is one somebody can act on rather than one
        // they have to reproduce.
        throw new Error(`${String(why)} — the explorer said: ${said}`);
      });
  });

  it("does not ask again for a profile it has already run, exactly as it stands", async () => {
    await pressOnly("New tab");
    await harnessRowsDrawn();
    await (await $("label*=needs-approval")).click();

    await expect($("button=Start")).toBeDisplayed();
    await browser.keys(["Escape"]);
  });
});
