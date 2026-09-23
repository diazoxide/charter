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
    // And the workspace strip's own `+` (charter-app#193), which shares `chat.new`'s glyph
    // rather than taking a third folder icon: each is the one control at the end of its own
    // strip, a whole row apart from the other, and both mean *make one more of what this
    // strip lists*. #178's rule above is about two controls drawn side by side.
    ["workspace.create", "New workspace…", "lucide-plus"],
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
        "workspace.create",
      ].sort(),
    );
  });

  /**
   * **Icon-only is only legible while the icons drawn TOGETHER differ** (charter-app#178,
   * narrowed in charter-app#193 to what its own argument supports).
   *
   * The argument was about the project strip: its controls carry no words at all —
   * `aria-label` is their whole name — so two of them an inch apart drawing one glyph leaves
   * a pointer with nothing to tell them apart, and the operator pressing *New project…* when
   * they meant *Open a project…* gets a folder-picker they did not ask for at best. That is a
   * claim about **adjacency**, and it was written down as a claim about the whole set because
   * at the time the whole set was one group.
   *
   * It is not any more. `workspace.create` is the one control at the end of the workspace
   * strip and `chat.new` is the one control on the bar, a whole row apart, and they share
   * `Plus` on purpose: the `+` at the end of a strip makes one more of what the strip lists,
   * which is the thing an operator learns once and then reads on every strip in the window.
   * Holding the whole set to be distinct would forbid exactly that, so the groups are written
   * down instead and each is held to the original rule.
   */
  const SIDE_BY_SIDE: Record<string, string[]> = {
    // `App.tsx`'s `.strip-doing`, the only place two icon-only rows are drawn together.
    "the project strip": ["project.open", "project.create"],
    // One each, so these cannot collide with anything — listed so that a second control
    // arriving on either strip has somewhere to be added and something to fail against.
    "the workspace strip": ["workspace.create"],
    "the chat strip": ["chat.new"],
  };

  it.each(Object.keys(SIDE_BY_SIDE))("never gives %s two rows with the same mark", (strip) => {
    const marks = SIDE_BY_SIDE[strip].map((id) => MARKS[id]);
    expect(marks.filter(Boolean)).toHaveLength(SIDE_BY_SIDE[strip].length);
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

  it("centres a tab's label in its cell, on all three", () => {
    // The operator's *"lets make tabs labels center aligned"* (charter-app#193). One rule over
    // the three strips, because it is a property of a tab and not of a strip — three rules
    // would be three places for the next person to centre two of them.
    const centred = [...css.matchAll(/([^{}]*)\{([^{}]*justify-content:\s*center;[^{}]*)\}/g)]
      .map((hit) => hit[1])
      .join(",");
    for (const strip of ['.projects [role="tab"]', '.workspaces-strip [role="tab"]']) {
      expect(centred).toContain(strip);
    }
    expect(centred).toContain('.tabs [role="tab"]');
  });
});

/**
 * **`N more` is one button, not one per strip** (charter-app#193).
 *
 * The operator: *"lets make 'N more' button looks like that buttons, to have all buttons in
 * same style."* It was drawn three ways — a bordered box on the chat strip from `.bar button`,
 * a borderless one on the project strip from `.projects button`, and a rule of its own on the
 * workspace strip with a third padding — and none of those differences was about the control.
 * Each was whichever ancestor's rule happened to reach it, which is a look that changes when
 * somebody moves the markup.
 *
 * jsdom computes no cascade, so the property is held over the stylesheet's text: nothing
 * dresses this per strip, and what does dress it is the family the `+` beside it is in.
 */
describe("the show-more button", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
    /\/\*[\s\S]*?\*\//g,
    "",
  );

  /** Every selector in the stylesheet that opens a rule and mentions `.show-more`. */
  const selectors = [...css.matchAll(/([^{}]*)\{/g)]
    .flatMap((hit) => hit[1].split(","))
    .map((one) => one.trim())
    .filter((one) => one.includes(".show-more"));

  it("is dressed by nothing that names a strip", () => {
    expect(selectors.length).toBeGreaterThan(0);
    const perStrip = selectors.filter((one) =>
      [".projects", ".workspaces", ".bar", ".tabs"].some((strip) => one.includes(strip)),
    );
    expect(perStrip, "one control, one rule — a strip may not redress it").toEqual([]);
  });

  it("wears the same family as the `+` beside it: no border, no fill, muted until hovered", () => {
    const rule = (selector: string) =>
      new RegExp(`(^|[},])\\s*${selector}\\s*\\{([^}]*)\\}`).exec(css)?.[2] ?? "";
    const drawn = rule("button\\.show-more");
    // Read against `button.bare`, which is what the strip's `+` wears, so the claim is that
    // the two match rather than that this one happens to say some words.
    const bare = rule("button\\.bare");
    expect(bare).not.toBe("");
    for (const declaration of ["border: 0", "background: none", "color: var(--text-muted)"]) {
      expect(bare, `button.bare no longer says ${declaration}`).toContain(declaration);
      expect(drawn, `button.show-more does not say ${declaration}`).toContain(declaration);
    }
    expect(rule("button\\.show-more:hover:not\\(:disabled\\)")).toContain(
      "background: var(--surface-hover)",
    );
  });
});

/**
 * **What says which of the three strips you are looking at** (charter-app#193).
 *
 * charter-app#171 drew the nesting with three signals — height, inset and surface — and the
 * operator read the inset back off the running app as stray padding: *"workspaces tabs and
 * sessions tabs have some padding from left, they should be like project tabs without
 * padding."* So the inset is gone and a colour carries the depth instead, one token per strip.
 *
 * jsdom lays nothing out and computes no stylesheet, so what can be held here is the rule as it
 * is written — which is enough for both halves of the claim: that each strip names its own
 * layer token, and that nothing indents a strip any more.
 */
describe("each strip says which layer it is, in colour rather than in indent", () => {
  const raw = readFileSync(join(process.cwd(), "src/App.css"), "utf8");
  const css = raw.replace(/\/\*[\s\S]*?\*\//g, "");

  /** One rule's declarations, by the selector that opens it. */
  const block = (selector: string): string => {
    const found = new RegExp(`(^|[},])\\s*${selector.replace(/\./g, "\\.")}\\s*\\{([^}]*)\\}`).exec(
      css,
    );
    expect(found, `no ${selector} rule in App.css`).not.toBeNull();
    return found?.[2] ?? "";
  };

  /** What a rule sets as its inline-start padding, in whichever of the three spellings. */
  const beginsAt = (declarations: string): string => {
    const start = /padding-inline-start:\s*([^;]+)/.exec(declarations)?.[1];
    if (start) return start.trim();
    const inline = /padding-inline:\s*([^;]+)/.exec(declarations)?.[1];
    if (inline) return inline.trim().split(/\s+/)[0];
    const all = /(?:^|[;{\s])padding:\s*([^;]+)/.exec(declarations)?.[1];
    if (all === undefined) return "0";
    // top | top right | top right bottom | top right bottom left
    const parts = all.trim().split(/\s+/);
    return parts.length === 4 ? parts[3] : parts.length === 1 ? parts[0] : parts[1];
  };

  it.each([
    [".projects", "--layer-project"],
    [".workspaces", "--layer-workspace"],
    [".bar", "--layer-chat"],
  ])("%s draws its own layer colour along its bottom", (selector, token) => {
    expect(block(selector)).toContain(`border-bottom: 2px solid var(${token})`);
  });

  it("gives the three of them three different tokens", () => {
    // One token used twice would pass every case above and draw two rows the same colour,
    // which is the whole of what the operator asked to be able to tell apart.
    const drawn = [".projects", ".workspaces", ".bar"].map(
      (selector) => /border-bottom:\s*2px solid var\((--layer-\w+)\)/.exec(block(selector))?.[1],
    );
    expect(new Set(drawn).size).toBe(3);
  });

  it("indents none of the three, so all three begin at the same x", () => {
    // `--nested` was #171's one value used twice. A strip that starts further in than the one
    // above it is the thing the operator asked to have taken away, and the property is that
    // there is no such number left to reach for.
    expect(raw).not.toContain("--nested");
    for (const selector of [".projects", ".workspaces", ".bar"]) {
      expect(beginsAt(block(selector)), `${selector} begins further in than the strip above`).toBe(
        "0",
      );
    }
  });
});
