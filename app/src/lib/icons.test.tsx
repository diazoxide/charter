/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render, screen } from "@testing-library/react";
import { CircleAlert, Plus } from "lucide-react";
import { describe, expect, it } from "vitest";

/**
 * Lucide is charter's icon set, and this is the one thing about it the theme layer has to be
 * sure of before anybody puts an icon on a button: **an icon takes its colour from the text
 * around it.** Lucide draws with `stroke="currentColor"`, so an icon inside a rule that sets
 * `color: var(--state-failed)` is that colour, and a theme reaches it without an icon ever
 * naming a colour. An icon set that baked its own fills in would be a second palette.
 */
describe("an icon is drawn in the colour of the text it sits in", () => {
  it("strokes with currentColor and fills with nothing", () => {
    const { container } = render(<CircleAlert aria-label="trouble" />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("stroke")).toBe("currentColor");
    expect(svg?.getAttribute("fill")).toBe("none");
  });

  it("takes its size from the font, so a token-sized row sizes its own icons", () => {
    const { container } = render(<CircleAlert size="1em" aria-label="trouble" />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("width")).toBe("1em");
  });
});

/**
 * **The icon layer is one CSS rule, and these are the two facts it stands on.**
 *
 * No call site in the window passes a `size` — `App.css` sizes every icon at `1em` through the
 * `lucide` class Lucide puts on each `<svg>`. That is what keeps fifty call sites from each
 * choosing a number. It also means two things can break every icon at once without a single
 * test about a component noticing: Lucide dropping the class, or somebody deleting the rule.
 * Either one draws every icon in the window at Lucide's default of 24px — a bar of buttons
 * twice as tall as their words.
 */
describe("every icon is sized by the one rule the stylesheet has for it", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8");

  it("carries the class the stylesheet sizes it by", () => {
    const { container } = render(<Plus />);
    expect(container.querySelector("svg")?.classList.contains("lucide")).toBe(true);
  });

  it("is sized at 1em by that class, so no call site has to say a size", () => {
    const rule = /(?:^|\})\s*\.lucide\s*\{([^}]*)\}/m.exec(css)?.[1] ?? "";
    expect(rule).toMatch(/(?:^|;|\s)width:\s*1em\s*;/);
    expect(rule).toMatch(/(?:^|;|\s)height:\s*1em\s*;/);
  });
});

/**
 * **A button's name is its words, and an icon beside them adds nothing to it.**
 *
 * The scenario tests press `New tab` by its text and a screen reader announces it by its name;
 * an icon that joined the name would rename every button it was put on. Lucide hides an icon
 * given no accessible name of its own (`aria-hidden="true"`) — this pins that, because the
 * whole window relies on it and passes no `aria-hidden` itself.
 */
describe("an icon beside a button's words is not part of its name", () => {
  it("is hidden from assistive technology when it is given no name", () => {
    const { container } = render(<Plus />);
    expect(container.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  });

  it("leaves the button named by its words alone", () => {
    render(
      <button type="button">
        <Plus />
        New tab
      </button>,
    );
    expect(screen.getByRole("button", { name: "New tab" })).toBeInTheDocument();
  });

  it("is not hidden when it IS given a name, so a lone icon can still say what it means", () => {
    const { container } = render(<CircleAlert aria-label="trouble" />);
    expect(container.querySelector("svg")?.hasAttribute("aria-hidden")).toBe(false);
  });
});

/**
 * **The spin is the window's one animation, and it stops for anyone who asked for less motion.**
 *
 * `.spinning` means "this is still happening" (`App.css` has the argument). Motion is the
 * decoration on that meaning, never the meaning itself, and `prefers-reduced-motion` is the
 * operating system's way of saying so on the operator's behalf. jsdom evaluates no media
 * query and no animation, so this reads the rule rather than a computed style.
 */
describe("the one animation respects reduced motion", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8");
  const reduced = [
    ...css.matchAll(/@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{([\s\S]*?)\n\}/g),
  ].map((hit) => hit[1]);

  it("spins what is still happening", () => {
    expect(css).toMatch(/\.spinning\s*\{[^}]*animation:\s*charter-spin\b/);
  });

  it("stops the spin when the operating system asks for reduced motion", () => {
    expect(reduced.some((block) => /\.spinning\s*\{[^}]*animation:\s*none/.test(block))).toBe(true);
  });
});
