/**
 * Whether the words can be read.
 *
 * A second theme is only proof that the contract works if it is a theme somebody could
 * actually use, and "complete" is not the same as "legible" — a light theme assembled by
 * inverting a dark one passes every other test in this directory while being unreadable.
 * So each pair of tokens that ends up as text on a background is held to a contrast ratio.
 *
 * **WCAG 2.1 AA is 4.5:1 for body text and 3:1 for large text and for a control's own
 * outline.** The window's text is 14px, so body text is held to 4.5. The chat-state marks are
 * dots and chips rather than prose and are held to 3, which is the ratio the standard gives
 * for a non-text thing that has to be distinguishable.
 *
 * This is a floor and not a target. It does not say a theme is nice; it says nobody shipped
 * one whose muted text vanished into its own background.
 */

/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { BUILT_IN, property, tinted, TOKENS, type Theme, type Token } from "./theme";
import { PALETTE } from "./tint";

/** One channel of an `#rrggbb`, as the sRGB number WCAG's formula wants. */
function channel(hex: string, at: number): number {
  const eight = hex.length <= 5 ? hex[at + 1].repeat(2) : hex.slice(1 + at * 2, 3 + at * 2);
  const value = Number.parseInt(eight, 16) / 255;
  return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
}

/** WCAG relative luminance. */
function luminance(hex: string): number {
  return 0.2126 * channel(hex, 0) + 0.7152 * channel(hex, 1) + 0.0722 * channel(hex, 2);
}

/** WCAG contrast ratio, 1 (identical) to 21 (black on white). Alpha is ignored here: a pair
 *  whose background is a wash names what the wash is laid over, and {@link onto} lays it. */
function contrast(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (light + 0.05) / (dark + 0.05);
}

/**
 * `wash` laid over the opaque `under`, as the screen draws it: a token with alpha (a wash, a
 * glow) is never seen alone, so its contrast is measured as what it makes of what is under it.
 * An opaque `wash` is itself.
 */
function onto(wash: string, under: string): string {
  const long = (hex: string) =>
    hex.length <= 5 ? [...hex.slice(1)].map((digit) => digit + digit).join("") : hex.slice(1);
  const top = long(wash);
  if (top.length === 6) return wash;
  const alpha = Number.parseInt(top.slice(6, 8), 16) / 255;
  const bottom = long(under);
  const mixed = [0, 2, 4].map((at) => {
    const value =
      Number.parseInt(top.slice(at, at + 2), 16) * alpha +
      Number.parseInt(bottom.slice(at, at + 2), 16) * (1 - alpha);
    return Math.round(value).toString(16).padStart(2, "0");
  });
  return `#${mixed.join("")}`;
}

/**
 * WCAG 2.1 AA's floor for text of a given size: 3:1 once it is "large" (18pt, which is 24px,
 * or 14pt bold, which is 18.66px), 4.5:1 under that. A pair that is words is held to the floor
 * for the size it is drawn at, not to a floor picked for the pair.
 */
function aaText(px: number, bold = false): number {
  return px >= 24 || (bold && px >= 18.66) ? 3 : 4.5;
}

/** The window's text size (`:root` in App.css, `textSize.DEFAULT_TEXT`): a button's label and
 *  a menu row are drawn at it, never larger. */
const WINDOW_TEXT_PX = 14;

/**
 * Every pair that ends up as something drawn on something, and the floor it has to clear. A
 * fourth token is what the background is laid over, for a background that is a wash.
 */
const PAIRS: [Token, Token, number, Token?][] = [
  ["text.primary", "surface.base", 4.5],
  ["text.primary", "surface.sunken", 4.5],
  ["text.primary", "surface.deep", 4.5],
  ["text.primary", "surface.raised", 4.5],
  ["text.primary", "surface.overlay", 4.5],
  ["text.primary", "surface.hover", 4.5],
  ["text.primary", "control.base", 4.5],
  ["text.primary", "control.hover", 4.5],
  ["text.primary", "control.aimed", 4.5],
  ["text.primary", "control.count", 4.5],
  ["text.primary", "tab.active", 4.5],
  ["text.primary", "accent.surface", 4.5],
  ["text.secondary", "surface.base", 4.5],
  ["text.muted", "surface.base", 4.5],
  ["text.muted", "surface.overlay", 4.5],
  // The three strips are three surfaces, and a tab that is not the selected one is muted on
  // whichever its strip sits on. A pane's own controls are muted on `surface.raised` too.
  ["text.muted", "surface.deep", 4.5],
  ["text.muted", "surface.raised", 4.5],
  // The alerts drawer is `surface.overlay`: its details are secondary text on it.
  ["text.secondary", "surface.overlay", 4.5],
  // Settings (DS-3e, SE-16's review): the level switcher's unchosen levels are secondary text on
  // `control.base`, and the group nav's unchosen groups are secondary text on the pane, which a
  // view tab draws in `surface.raised`.
  ["text.secondary", "control.base", 4.5],
  ["text.secondary", "surface.raised", 4.5],
  ["needs-you.text", "needs-you.base", 4.5],
  ["danger.text", "danger.surface", 4.5],
  ["terminal.foreground", "terminal.background", 4.5],
  // A find in a pane draws its matches as the cells' background, under the terminal's text.
  ["terminal.foreground", "terminal.find-match", 4.5],
  ["terminal.foreground", "terminal.find-match-active", 4.5],
  // Marks rather than prose: a chip, a dot, a one-word CI state.
  ["state.running", "surface.base", 3],
  ["state.waiting", "surface.base", 3],
  ["state.failed", "surface.base", 3],
  ["state.success", "surface.base", 3],
  ["state.unreadable", "surface.base", 3],
  // A chat's state on a row of a list (#1484): its mark in the state's colour and its word in
  // secondary text, on the window and on a row that is current or under the pointer.
  ["needs-you.base", "surface.base", 3],
  ["text.muted", "surface.hover", 3],
  ["state.running", "surface.hover", 3],
  ["state.waiting", "surface.hover", 3],
  ["state.failed", "surface.hover", 3],
  ["state.unreadable", "surface.hover", 3],
  ["needs-you.base", "surface.hover", 3],
  ["text.secondary", "surface.hover", 4.5],
  // An answer that ends something (`.ends-it`) says so in `danger.base` WORDS, at the window's
  // text size, so each place it is drawn is held to AA for that size (#1210):
  // - on the button's own `control.base`, in a form's `.ui-setting-actions` and a question's
  //   `AnswerBar` (`.answer`);
  // - on `danger.surface`, the same buttons under the pointer, and a menu's row when it is
  //   highlighted;
  // - on `surface.overlay`, a menu's `.menu-row.ends-it`;
  // - on the window and on a pane (`surface.base`, `surface.raised`), where a choice says it
  //   needs an approval.
  // charter-dark's `danger.base` was `#c05c5c`, 3.73:1 on `control.base`; it is lighter now, in
  // the same hue.
  ["danger.base", "control.base", aaText(WINDOW_TEXT_PX)],
  ["danger.base", "danger.surface", aaText(WINDOW_TEXT_PX)],
  ["danger.base", "surface.overlay", aaText(WINDOW_TEXT_PX)],
  ["danger.base", "surface.base", aaText(WINDOW_TEXT_PX)],
  ["danger.base", "surface.raised", aaText(WINDOW_TEXT_PX)],
  // The alerts drawer's marks, on the drawer, and the status line's bell on its button.
  ["state.waiting", "surface.overlay", 3],
  ["state.failed", "surface.overlay", 3],
  ["state.waiting", "control.base", 3],
  ["accent.base", "surface.base", 3],
  ["focus.ring", "surface.base", 3],
  // The three strips are three shades now (charter-app#193), and a tab that is not the one
  // you are on is muted text on whichever its strip is. The one you are on is primary text on
  // the lighter `layer.selected` — the pair #176 would have caught had it been sub-AA.
  ["text.muted", "layer.project", 4.5],
  ["text.muted", "layer.workspace", 4.5],
  ["text.muted", "layer.chat", 4.5],
  ["text.primary", "layer.selected", 4.5],
  // A workspace's colour (charter-app#281): its tab is drawn in its own shade with a mark in its
  // own accent, and the one in front is primary text on its tinted `layer.selected`.
  ["accent.base", "layer.workspace", 3],
  ["text.primary", "layer.workspace", 4.5],
  // A file's or a folder's icon (FM-3) is a non-text mark, on the explorer's window and the
  // sunken file tab alike. `icon.motive` is drawn over a folder, never on the window.
  ["icon.folder", "surface.base", 3],
  ["icon.folder", "surface.sunken", 3],
  ["icon.grey", "surface.base", 3],
  ["icon.grey", "surface.sunken", 3],
  ["icon.red", "surface.base", 3],
  ["icon.red", "surface.sunken", 3],
  ["icon.orange", "surface.base", 3],
  ["icon.orange", "surface.sunken", 3],
  ["icon.yellow", "surface.base", 3],
  ["icon.yellow", "surface.sunken", 3],
  ["icon.green", "surface.base", 3],
  ["icon.green", "surface.sunken", 3],
  ["icon.teal", "surface.base", 3],
  ["icon.teal", "surface.sunken", 3],
  ["icon.blue", "surface.base", 3],
  ["icon.blue", "surface.sunken", 3],
  ["icon.purple", "surface.base", 3],
  ["icon.purple", "surface.sunken", 3],
  ["icon.pink", "surface.base", 3],
  ["icon.pink", "surface.sunken", 3],
  // **The row a tree or a list has selected** (#1672): the chat in front in the Chats tree, the
  // spot in the explorer, the file a file tab shows. Everything a row draws is drawn on its
  // fill — the name, its state's word and mark, the hand of a chat that needs you — and its
  // edge is the shape that says "selected" without colour, held to a mark's floor on the fill
  // and on the window beside it.
  ["text.primary", "list.selected", 4.5],
  ["text.secondary", "list.selected", 4.5],
  ["text.muted", "list.selected", 4.5],
  ["state.running", "list.selected", 3],
  ["state.waiting", "list.selected", 3],
  ["state.failed", "list.selected", 3],
  ["state.unreadable", "list.selected", 3],
  ["needs-you.base", "list.selected", 3],
  ["accent.base", "list.selected", 3],
  ["list.selected-edge", "list.selected", 3],
  ["list.selected-edge", "surface.base", 3],
  // The keyboard's ring on the selected row it is on.
  ["focus.ring", "list.selected", 3],
  // A tree's guides: a hairline, quieter than a border and still there, on each surface a tree
  // is drawn on (the explorer on the window, a file tab's tree on a pane).
  ["tree.guide", "surface.base", 1.3],
  ["tree.guide", "surface.raised", 1.2],
  ["tree.guide", "surface.sunken", 1.2],
  ["border.subtle", "surface.base", 1.2],
  ["border.strong", "surface.base", 1.5],
  // DS-6 (#629): the pairs App.css draws in one rule that no line above held, found by the
  // stylesheet check below.
  // - A persona's mark is its initial, a word in the accent on the accent's surface.
  ["accent.base", "accent.surface", aaText(WINDOW_TEXT_PX)],
  // - A checklist's tick (`.dot`, `.box`) is a mark in the accent on a sunken box.
  ["accent.base", "surface.sunken", 3],
  // - What an extension runs as you, said inside its approval dialog on the danger wash.
  ["text.primary", "danger.wash", 4.5, "surface.base"],
  // - A task that is away draws its words on the terminal's own background.
  ["text.primary", "terminal.background", 4.5],
  // - A notice's "and N more" is secondary words on a bar of the hairline's colour.
  ["text.secondary", "border.subtle", 4.5],
  // - A count beside a workspace, a branch or a search is secondary words on a count chip.
  ["text.secondary", "control.count", 4.5],
];

/**
 * Each built-in as it ships, and **tinted by every colour a workspace can name** (charter-app
 * #281): the tint turns the accent and the tab shades, so every pair above with one of them in
 * it is held again at every hue. `tint.ts` keeps each shade's luminance to make this hold by
 * construction; this is the check that the construction did.
 */
const DRAWN: [string, Theme][] = Object.keys(BUILT_IN).flatMap((name) => [
  [name, BUILT_IN[name]] as [string, Theme],
  ...Object.keys(PALETTE).map(
    (colour) => [`${name} tinted ${colour}`, tinted(BUILT_IN[name], colour)] as [string, Theme],
  ),
]);

describe("the floor a text pair is held to", () => {
  it("is AA's: 4.5 under large text, 3 from 24px, or from 18.66px bold", () => {
    expect(aaText(WINDOW_TEXT_PX)).toBe(4.5);
    expect(aaText(18.66)).toBe(4.5);
    expect(aaText(18.66, true)).toBe(3);
    expect(aaText(24)).toBe(3);
  });

  it("measures the way WCAG does", () => {
    expect(contrast("#ffffff", "#000000")).toBeCloseTo(21, 5);
    expect(contrast("#777777", "#ffffff")).toBeCloseTo(4.48, 2);
  });
});

/**
 * **Selected is not hovered, and is not the tab you are on** (#1672). Before this, the chat in
 * front was drawn in `surface.hover`, so the row you were on and the row the pointer passed
 * over were the same row to the eye. Held as values, in each theme as it ships and tinted.
 */
describe.each(DRAWN)("%s keeps a selected row apart", (_name, theme) => {
  it.each(["surface.hover", "layer.selected", "surface.base"] as const)(
    "list.selected is not %s",
    (other) => {
      expect(theme.values["list.selected"].toLowerCase()).not.toBe(
        theme.values[other].toLowerCase(),
      );
    },
  );
});

describe.each(DRAWN)("%s can be read", (_name, theme) => {
  it.each(PAIRS)("%s on %s clears %s to 1", (front, back, floor, under) => {
    const drawn =
      under === undefined ? theme.values[back] : onto(theme.values[back], theme.values[under]);
    const ratio = contrast(theme.values[front], drawn);
    expect(Number(ratio.toFixed(2)), `${theme.values[front]} on ${drawn}`).toBeGreaterThanOrEqual(
      floor,
    );
  });

  it("keeps the sixteen ANSI colours off the terminal's own background", () => {
    // A theme whose `ansi.blue` matched its terminal background would make a whole class of
    // program's output invisible, and no other test here would notice.
    //
    // **`black` is held lower, and that is not a loophole.** ANSI black on a dark terminal is
    // dim in every theme there has ever been — it is the colour a program picks when it means
    // "recede", and a dark theme that made it clear 3:1 would not be drawing black any more.
    // It still has to be *visible*, because `ESC[30m` on charter-dark must not be an invisible
    // line, so it clears 1.5. Measured here: 2.14:1.
    const background = theme.values["terminal.background"];
    const floor = (token: Token) => (token === "terminal.ansi.black" ? 1.5 : 3);
    const lost = (Object.keys(theme.values) as Token[])
      .filter((token) => token.startsWith("terminal.ansi."))
      .filter((token) => contrast(theme.values[token], background) < floor(token));
    expect(lost).toEqual([]);
  });
});

/**
 * **Every pair the stylesheets draw is held above** (DS-6, #629). A rule that sets a token as
 * its text colour and another as its background draws the one on the other, so the pair is in
 * {@link PAIRS}, at the floor its use asks for, or this fails naming the rule. It reads one rule
 * at a time: a colour a rule takes from a parent's background, or a background a `:hover` rule
 * sets on its own, is held by the lines above by hand, not here.
 */
/** What a colour or a background may be without naming a token: nothing is drawn by it. */
const KEYWORDS = new Set(["inherit", "transparent", "none", "currentcolor", "unset", "initial"]);

/**
 * The colours the stylesheet check below cannot read, each as `rule | declaration`. None of
 * these rules sets a background, so none draws a pair of its own; the colour each one mixes
 * from is held by the lines of PAIRS above (`text.muted`, the change marks' state colours).
 */
const READ_BY_HAND: readonly string[] = [
  ".explorer [data-mark] > .file-node > .spot-name | color: var(--change-colour)",
  ".explorer .change-mark | color: var(--change-colour)",
  ".explorer .chat | color: color-mix(in srgb, currentcolor 85%, transparent)",
  ".explorer .chat .node-icon | color: color-mix(in srgb, var(--text-muted) 85%, transparent)",
  ".explorer .unreported | color: color-mix(in srgb, currentcolor 70%, transparent)",
];

describe("every pair a stylesheet draws", () => {
  const byProperty = new Map(TOKENS.map((token) => [property(token), token]));
  const held = new Set(PAIRS.map(([front, back]) => `${front} on ${back}`));
  const sheets = ["App.css", "styles.css"].map((file) =>
    readFileSync(join(process.cwd(), "src", file), "utf8").replace(/\/\*[\s\S]*?\*\//g, ""),
  );
  const drawn = new Map<string, string>();
  /** Each colour or background a rule sets that is not one token, as `rule | declaration`. */
  const unread: string[] = [];
  for (const sheet of sheets)
    for (const [, selector, body] of sheet.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      const rule = selector
        .trim()
        .split(/\s*,\s*/)[0]
        .replace(/\s+/g, " ");
      for (const [, name, value] of body.matchAll(
        /(?<![\w-])(color|background|background-color)\s*:\s*([^;]+)/g,
      )) {
        const said = value.trim().replace(/\s*!important$/, "");
        const one = /^var\((--[\w-]+)\)$/.exec(said);
        if (one !== null && byProperty.has(one[1])) continue;
        if (KEYWORDS.has(said.toLowerCase())) continue;
        unread.push(`${rule} | ${name}: ${said}`);
      }
      const read = (pattern: RegExp) =>
        [...body.matchAll(pattern)]
          .map((hit) => byProperty.get(hit[1]))
          .filter((token) => token !== undefined);
      const fronts = read(/(?<![\w-])color:\s*var\((--[\w-]+)\)/g);
      const backs = read(/(?<![\w-])background(?:-color)?:\s*var\((--[\w-]+)\)/g);
      for (const front of fronts)
        for (const back of backs)
          if (!drawn.has(`${front} on ${back}`))
            drawn.set(`${front} on ${back}`, selector.trim().split(/\s*,\s*/)[0]);
    }

  it("reads every colour and background a rule sets, or names the ones read by hand", () => {
    // A value this check cannot resolve to one token (a `color-mix`, a property of the rule's
    // own, a literal) would let its rule's pair through unmeasured, so each is listed here,
    // and a new one fails until it is held by hand in PAIRS and added below.
    expect(unread.sort()).toEqual([...READ_BY_HAND].sort());
  });

  it("is found: the stylesheets draw text on a background", () => {
    expect(drawn.get("danger.text on danger.surface")).toBeDefined();
  });

  it("is held to a floor, in every theme and tint", () => {
    const loose = [...drawn]
      .filter(([pair]) => !held.has(pair))
      .map(([pair, rule]) => `${pair} (${rule})`);
    expect(loose).toEqual([]);
  });
});

/**
 * **Every theme file is a theme the lines above are run on** (DS-6, #629): a colour theme added
 * to this directory and not to `BUILT_IN` would ship unmeasured. The icon theme is not a colour
 * theme: it has no tokens.
 */
describe("every theme file in this directory", () => {
  it("is drawn above", () => {
    const here = join(process.cwd(), "src/theme");
    const themes = readdirSync(here)
      .filter((file) => file.endsWith(".json"))
      .map((file) => JSON.parse(readFileSync(join(here, file), "utf8")) as Record<string, unknown>)
      .filter((file) => "tokens" in file)
      .map((file) => String(file.name));
    expect(themes.sort()).toEqual(Object.keys(BUILT_IN).sort());
  });
});
