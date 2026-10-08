import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { Notice } from "./Notice";

/**
 * **A Notice in a pane's corner fits its pane, whatever it says** (#1481).
 *
 * The operator, from the dev build with two panes side by side: *"ui is broken for questions
 * modals"*. A Notice's sentence was one word wide and fourteen lines tall, its buttons each
 * wrapped to five lines beside it, and the block a button opened was drawn to the right of the
 * Notice and off the window.
 *
 * jsdom lays nothing out, so two things are held here and the measuring is `pane-notices.e2e.ts`:
 *
 * - **the shape `Notice` draws for a pane**: one box, the line, and under it what a way out
 *   opened. The stylesheet's rules are written against exactly this shape;
 * - **the rules as written**: each one is a property the fix needs, and a rule that went
 *   missing fails here before it is seen in a window.
 */

afterEach(cleanup);

const opened = { id: "what-it-opens", open: true };

const aPaneNotice = (under?: React.ReactNode) =>
  render(
    <Notice
      cause="vault-refused:7:devops"
      at="pane"
      tone="trouble"
      label="Vault"
      fixes={[
        { label: "Allow steward to use this vault", onPress: () => undefined, opens: opened },
        { label: "Keep blocked", onPress: () => undefined },
      ]}
      onDismiss={() => undefined}
      under={under}
    >
      This chat runs as steward, so purlis did not open it.
    </Notice>,
  );

describe("a Notice in a pane's corner", () => {
  it("is one box: the line, and under it what a way out opened", () => {
    const { container } = aPaneNotice(<div id="what-it-opens">Allow lets every chat…</div>);

    const box = container.firstElementChild;
    expect(container.children).toHaveLength(1);
    expect(box).toHaveClass("notice-pane-box", "notice-opened");
    expect([...(box?.children ?? [])].map((one) => one.className)).toEqual([
      "notice notice-pane notice-trouble",
      "notice-under notice-under-pane",
    ]);
    // What it opened is outside the line: the line is a live region, and a form in one would
    // be read again on every keystroke.
    const line = screen.getByRole("status", { name: "Vault" });
    expect(line).not.toContainElement(screen.getByText("Allow lets every chat…"));
  });

  it("draws the sentence first on its line, then every way out, in order", () => {
    aPaneNotice();

    const line = screen.getByRole("status", { name: "Vault" });
    expect(line).toHaveAttribute("data-cause", "vault-refused:7:devops");
    expect([...line.children].map((one) => one.className || one.tagName)).toEqual([
      "notice-says",
      "notice-fix",
      "notice-fix",
      "notice-dismiss",
    ]);
    // Each a Tab stop (`docs/ui-primitives.md`), and the one that opens something says so.
    for (const button of screen.getAllByRole("button"))
      expect(button).toHaveAttribute("tabindex", "0");
    const allow = screen.getByRole("button", { name: "Allow steward to use this vault" });
    expect(allow).toHaveAttribute("aria-expanded", "true");
    expect(allow).toHaveAttribute("aria-controls", "what-it-opens");
  });

  it("is a box with nothing under the line when nothing is opened", () => {
    const { container } = aPaneNotice();

    const box = container.firstElementChild;
    expect(box).toHaveClass("notice-pane-box");
    expect(box).not.toHaveClass("notice-opened");
    expect(box?.children).toHaveLength(1);
  });

  it("draws the persona's mark as the sentence's first word, never as an item of its own", () => {
    // An item of its own is left alone on a row when the sentence takes the next one whole.
    render(
      <Notice cause="persona-grants:7" at="pane" persona="devops" onDismiss={() => undefined}>
        devops may use two vaults.
      </Notice>,
    );

    const line = screen.getByRole("status");
    const mark = line.querySelector(".persona-mark");
    expect(mark?.parentElement).toHaveClass("notice-says");
    expect(mark?.parentElement?.firstElementChild).toBe(mark);
  });
});

describe("a Notice under the strip or in the drawer", () => {
  it.each(["band", "drawer"] as const)("is not boxed at %s: the line and what it opened", (at) => {
    const { container } = render(
      <Notice
        cause="doctor-finding:git-identity"
        at={at}
        persona="devops"
        fixes={[{ label: "Set it", onPress: () => undefined, opens: opened }]}
        under={<div id="what-it-opens">a form</div>}
      >
        git has no name for you.
      </Notice>,
    );

    expect(container.querySelector(".notice-pane-box")).toBeNull();
    expect([...container.children].map((one) => one.className)).toEqual([
      `notice notice-${at}`,
      `notice-under notice-under-${at}`,
    ]);
    // The mark stays before the sentence, where the drawer's rules place it.
    const line = screen.getByRole("status");
    expect(line.firstElementChild).toHaveClass("persona-mark");
  });
});

/**
 * **The rules the layout is made of, held over the stylesheet.** Each is named for what breaks
 * without it.
 */
describe("the pane Notice's rules", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
    /\/\*[\s\S]*?\*\//g,
    "",
  );
  const rule = (selector: string) =>
    new RegExp(`(^|[},])\\s*${selector}\\s*\\{([^}]*)\\}`).exec(css)?.[2] ?? "";

  it("keeps the corner inside the pane, on both axes", () => {
    const corner = rule("\\.pane-corner\\.at-start");
    expect(corner).toMatch(/flex-direction:\s*column/);
    expect(corner).toMatch(/max-width:\s*calc\(100% - 16px\)/);
    expect(corner).toMatch(/max-height:\s*calc\(100% - 6px\)/);
    // And nothing a pane draws is drawn outside it, by a box that cannot be scrolled.
    expect(rule("\\.pane-frame")).toMatch(/overflow:\s*clip/);
  });

  it("lets the terminal be reached through the part of the corner that draws nothing", () => {
    expect(rule("\\.pane-corner\\.at-start")).toMatch(/pointer-events:\s*none/);
    expect(rule("\\.pane-corner\\.at-start > \\*")).toMatch(/pointer-events:\s*auto/);
  });

  it("stacks a pane's Notices and scrolls them when they are taller than the pane", () => {
    const stack = rule("\\.pane-notices");
    expect(stack).toMatch(/flex-direction:\s*column/);
    expect(stack).toMatch(/min-height:\s*0/);
    expect(stack).toMatch(/overflow-y:\s*auto/);
    // Never wider than the pane, and no wider than a sentence reads well.
    expect(stack).toMatch(/max-width:\s*min\(100%, 40rem\)/);
    expect(rule("\\.notice-pane-box")).toMatch(/max-width:\s*100%/);
  });

  it("wraps the ways out under the sentence, and never breaks one that fits", () => {
    expect(rule("\\.notice-pane")).toMatch(/flex-wrap:\s*wrap/);
    // The sentence is as wide as it reads, so it shares a row only when everything fits.
    expect(rule("\\.notice-pane > \\.notice-says")).toMatch(/flex:\s*0 1 auto/);
    const way = rule("\\.notice-pane > button");
    expect(way).toMatch(/flex:\s*none/);
    expect(way).toMatch(/max-width:\s*100%/);
    expect(way).not.toMatch(/white-space/);
  });

  it("keeps what a way out opened at the Notice's width, its draft scrolling in its own box", () => {
    expect(rule("\\.notice-under-pane")).toMatch(/min-width:\s*0/);
    const draft = rule("\\.notice-under-pane pre");
    expect(draft).toMatch(/max-width:\s*100%/);
    expect(draft).toMatch(/overflow:\s*auto/);
  });

  it("is opaque, and over all of the terminal", () => {
    expect(rule("\\.notice-pane-box")).toMatch(/background:\s*var\(--surface-raised\)/);
    expect(rule("\\.pane")).toMatch(/isolation:\s*isolate/);
  });

  it("is under whatever is opened over the window: a dialog, a menu", () => {
    // A pane's corner has a z-index, and a dialog portaled to the body has none.
    expect(rule("#root")).toMatch(/isolation:\s*isolate/);
  });

  it("leaves the band's and the drawer's lines as they were", () => {
    expect(rule("\\.notice-band \\.notice-says")).toMatch(/display:\s*inline/);
    expect(rule("\\.notice-drawer")).toMatch(/flex-wrap:\s*wrap/);
    // No rule of the pane's reaches a Notice that is not in one.
    const reach = [...css.matchAll(/([^{}]+)\{[^{}]*\}/g)]
      .map((hit) => hit[1].trim())
      .filter((selector) => /notice-(pane|under-pane)|pane-notices/.test(selector));
    for (const selector of reach) expect(selector).not.toMatch(/notice-(band|drawer)/);
  });
});

describe("Ask {persona}'s rules", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
    /\/\*[\s\S]*?\*\//g,
    "",
  );
  const rule = (selector: string) =>
    new RegExp(`(^|[},])\\s*${selector}\\s*\\{([^}]*)\\}`).exec(css)?.[2] ?? "";

  it("gives the box you write in the row, and lets it grow downwards only", () => {
    const box = rule("\\.ask-persona \\.ui-setting-control > textarea\\.ui-field");
    expect(box).toMatch(/flex:\s*1 1 100%/);
    expect(box).toMatch(/min-height:/);
    expect(box).toMatch(/resize:\s*vertical/);
  });

  it("fits the window and keeps its answers at its bottom edge", () => {
    const dialog = rule("\\.warning\\.ask-persona");
    expect(dialog).toMatch(/width:\s*min\(34rem, calc\(100vw - 2rem\)\)/);
    expect(dialog).toMatch(/max-height:\s*calc\(100vh - 2rem\)/);
    const answers = rule("\\.ask-persona \\.ui-setting-actions");
    expect(answers).toMatch(/position:\s*sticky/);
    expect(answers).toMatch(/bottom:\s*0/);
    expect(answers).toMatch(/background:\s*var\(--surface-base\)/);
  });
});
