/**
 * Whether the words can be read.
 *
 * A second theme is only proof that the contract works if it is a theme somebody could
 * actually use, and "complete" is not the same as "legible" — a light theme assembled by
 * inverting a dark one passes every other test in this directory while being unreadable.
 * So each pair of tokens that ends up as text on a background is held to a contrast ratio.
 *
 * **WCAG 2.1 AA is 4.5:1 for body text and 3:1 for large text and for a control's own
 * outline.** charter's window is 13px, so body text is held to 4.5. The chat-state marks are
 * dots and chips rather than prose and are held to 3, which is the ratio the standard gives
 * for a non-text thing that has to be distinguishable.
 *
 * This is a floor and not a target. It does not say a theme is nice; it says nobody shipped
 * one whose muted text vanished into its own background.
 */

import { describe, expect, it } from "vitest";
import { BUILT_IN, type Theme, type Token } from "./theme";

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
  ["needs-you.text", "needs-you.base", 4.5],
  ["danger.text", "danger.surface", 4.5],
  ["terminal.foreground", "terminal.background", 4.5],
  // Marks rather than prose: a chip, a dot, a one-word CI state.
  ["state.running", "surface.base", 3],
  ["state.waiting", "surface.base", 3],
  ["state.failed", "surface.base", 3],
  ["state.success", "surface.base", 3],
  ["state.unreadable", "surface.base", 3],
  ["danger.base", "surface.base", 3],
  // The alerts drawer's marks, on the drawer, and the status line's bell on its button.
  ["state.waiting", "surface.overlay", 3],
  ["state.failed", "surface.overlay", 3],
  ["state.waiting", "control.base", 3],
  ["accent.base", "surface.base", 3],
  ["focus.ring", "surface.base", 3],
  // The rule along the bottom of each strip, on the surface that strip sits on. A 2px band is
  // a non-text graphic and is held to 3, and it has to clear it in both themes or the one
  // signal that says which row is which disappears into the row.
  ["layer.project", "surface.deep", 3],
  ["layer.workspace", "surface.raised", 3],
  ["layer.chat", "surface.base", 3],
  ["border.subtle", "surface.base", 1.2],
  ["border.strong", "surface.base", 1.5],
];

describe.each(Object.keys(BUILT_IN))("%s can be read", (name) => {
  const theme: Theme = BUILT_IN[name];
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
