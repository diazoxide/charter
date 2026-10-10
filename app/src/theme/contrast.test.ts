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

import { describe, expect, it } from "vitest";
import { BUILT_IN, tinted, type Theme, type Token } from "./theme";
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

/** WCAG contrast ratio, 1 (identical) to 21 (black on white). Alpha is ignored: a token with
 *  alpha is a wash or a glow, and none of the pairs below use one. */
function contrast(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (light + 0.05) / (dark + 0.05);
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

/** Every pair that ends up as something drawn on something, and the floor it has to clear. */
const PAIRS: [Token, Token, number][] = [
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
  ["border.subtle", "surface.base", 1.2],
  ["border.strong", "surface.base", 1.5],
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

describe.each(DRAWN)("%s can be read", (_name, theme) => {
  it.each(PAIRS)("%s on %s clears %s to 1", (front, back, floor) => {
    const ratio = contrast(theme.values[front], theme.values[back]);
    expect(
      Number(ratio.toFixed(2)),
      `${theme.values[front]} on ${theme.values[back]}`,
    ).toBeGreaterThanOrEqual(floor);
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
