/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Closer, Doer, MARKS, Pin } from "./PlaneView";
import { RegionToggle } from "./RegionFrame";
import type { Offer } from "./actions";

/**
 * The strips and the bar, drawn as tabs with icons (M6.6).
 *
 * **What an icon must never do is rename a control.** Every scenario spec reaches the bar by
 * words — `pressOnly("New tab")`, `button[aria-pressed="true"]=Explorer` — and a screen reader
 * announces it by the same words. So these tests hold, for each control that gained a mark,
 * that the mark is there AND that the name is exactly what it was.
 */

afterEach(cleanup);

function offer(id: string, title: string, on: Partial<Offer> = {}): Offer {
  return { id, title, available: true, reason: "", does: { verb: "nothing" }, ...on };
}

describe("the bar's buttons carry a mark beside their words", () => {
  it.each([
    ["chat.new", "New tab", "lucide-plus"],
    ["pane.split.right", "Split right", "lucide-square-split-horizontal"],
    ["pane.split.down", "Split down", "lucide-square-split-vertical"],
    ["pane.close", "End this pane's chat", "lucide-x"],
    // The project strip's two, and they are deliberately not the same glyph: `FolderPlus` is
    // what every file manager puts on *New folder*, `FolderOpen` on *Open* (charter-app#178).
    // Two icon-only buttons an inch apart carrying one icon is a strip aimed at by memory.
    ["project.create", "New project…", "lucide-folder-plus"],
    ["project.open", "Open a project…", "lucide-folder-open"],
  ])("%s is named %s and draws its mark", (id, title, mark) => {
    render(<Doer offer={offer(id, title)} onPress={() => {}} />);

    const button = screen.getByRole("button", { name: title });
    expect(button.textContent).toBe(title);
    expect(button.querySelector(`svg.${mark}`)).not.toBeNull();
  });

  it("draws words alone for a row it has no mark for, rather than dropping the button", () => {
    render(<Doer offer={offer("charter.quit", "Quit charter")} onPress={() => {}} />);

    expect(screen.getByRole("button", { name: "Quit charter" }).querySelector("svg")).toBeNull();
  });

  it("marks the button that ends a pane's chat as one that ends something", () => {
    render(<Doer offer={offer("pane.close", "End this pane's chat")} onPress={() => {}} />);

    expect(screen.getByRole("button")).toHaveClass("ends-a-chat");
  });

  it("has a mark for exactly the rows the bar and the project strip draw", () => {
    // A row added to the bar without a mark is not an error — it draws its words — but a
    // mark for a row nothing draws is dead weight that looks like coverage.
    expect(Object.keys(MARKS).sort()).toEqual(
      [
        "chat.new",
        "pane.close",
        "pane.split.down",
        "pane.split.right",
        "project.create",
        "project.open",
      ].sort(),
    );
  });

  /**
   * **Icon-only is only legible while the icons differ** (charter-app#178). The project
   * strip's controls carry no words at all — `aria-label` is their whole name — so two of
   * them drawing one glyph would leave a pointer with nothing to tell them apart, and the
   * operator pressing *New project…* when they meant *Open a project…* is a folder-picker
   * they did not ask for at best. Held here because it is a property of the set, which
   * neither row's own test can see.
   */
  it("never gives two rows the same mark", () => {
    const marks = Object.values(MARKS);
    expect(new Set(marks).size).toBe(marks.length);
  });
});

describe("a tab's close", () => {
  it("is named for what it does, with the X drawn and unread", async () => {
    const onPress = vi.fn();
    const ends = offer("tab.close:3", "End chat 3 steward", { note: "Ends the program." });
    render(<Closer offer={ends} onPress={onPress} />);

    const button = screen.getByRole("button", { name: "End chat 3 steward" });
    expect(button.querySelector("svg.lucide-x")?.getAttribute("aria-hidden")).toBe("true");
    await userEvent.click(button);
    expect(onPress).toHaveBeenCalledWith(ends);
  });

  /**
   * **It may not look lighter than it is** (charter-app#130). The `×` ends a chat with no undo,
   * and the one thing the styling must not do is agree with a glyph that reads as "hide".
   * jsdom computes no hover, so this reads the rule.
   */
  it("goes to the danger colours when a pointer is on it, in the stylesheet", () => {
    const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8");
    const hover = /\.tab \.closer:hover[^{]*\{([^}]*)\}/.exec(css)?.[1] ?? "";
    expect(hover).toMatch(/background:\s*var\(--danger-surface\)/);
    expect(hover).toMatch(/color:\s*var\(--danger-text\)/);
  });
});

describe("a pinned mark", () => {
  it("is still one image named for what is pinned, with the pin drawn inside it", () => {
    render(<Pin held what="chat" />);

    const mark = screen.getByRole("img", { name: "pinned chat" });
    // The name is the span's, and the drawing inside it is hidden. `role="img"` already makes
    // its children presentational in the spec, but that is a rule every engine has to get
    // right; `aria-hidden` on the drawing is the one nothing can misread.
    expect(mark.querySelector("svg.lucide-pin")?.getAttribute("aria-hidden")).toBe("true");
  });
});

describe("a region toggle", () => {
  it.each([
    ["explorer", "Explorer", "lucide-folder-tree"],
    ["aside", "Attention", "lucide-bell-ring"],
    ["bottom", "State", "lucide-activity"],
  ] as const)("%s is named %s exactly, and marked by what it is", (id, name, mark) => {
    render(<RegionToggle id={id} shown onToggle={() => {}} />);

    // `regions.e2e.ts` presses `button[aria-pressed="true"]=Explorer`, which is a match on
    // the whole text: one extra character in the button and that spec cannot find it.
    const button = screen.getByRole("button", { name });
    expect(button.textContent).toBe(name);
    expect(button.querySelector(`svg.${mark}`)).not.toBeNull();
  });
});

/**
 * **A strip's tabs are cells, and the selected one is lit — by a rule, not by a border colour
 * on a pill.** jsdom lays nothing out, so what can be held here is the stylesheet's claim: all
 * three strips share the lit edge, and hovering never repaints the tab that is selected.
 */
describe("the three strips share one tab shape", () => {
  // Comments out first: a selector read with the prose above it splits on the prose's commas.
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
    /\/\*[\s\S]*?\*\//g,
    "",
  );

  it("lights the selected tab's edge on every strip", () => {
    const lit = /([^{}]*)\{\s*background:\s*var\(--accent-base\);\s*\}/g;
    const selectors = [...css.matchAll(lit)].map((hit) => hit[1]).join(",");
    expect(selectors).toContain('.projects .project:has([aria-selected="true"])::after');
    expect(selectors).toContain('.workspaces-strip [role="tab"][aria-selected="true"]::after');
    expect(selectors).toContain('.tabs .tab:has([aria-selected="true"])::after');
  });

  it("never lets hover repaint the selected tab", () => {
    const hovers = [...css.matchAll(/([^{}]*\[role="tab"\]:hover[^{}]*)\{/g)].map((hit) => hit[1]);
    expect(hovers.length).toBeGreaterThan(0);
    for (const selector of hovers) {
      for (const one of selector.split(",")) {
        expect(one).toContain(':not([aria-selected="true"])');
      }
    }
  });
});
