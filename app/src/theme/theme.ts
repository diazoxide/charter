/**
 * The theme: one data file, two consumers, and nothing to drift.
 *
 * **Why this module exists.** The window drew itself from five custom properties in `App.css`
 * and the terminal drew itself from hex written out again in `SessionPane.tsx` —
 * `background: "#181818"` is `--paper` spelled a second time, in a second language, with
 * nothing on earth making the two agree. That is two colour systems, and the second one is
 * the one an operator stares at all day. xterm cannot be handed a CSS custom property: its
 * theme is a JavaScript object of sixteen ANSI colours plus a background, a foreground, a
 * cursor and a selection. So the shared thing cannot be CSS, and it cannot be JavaScript
 * either without the CSS becoming a copy of it.
 *
 * It is **data**. A theme is a file of semantic tokens; charter writes CSS custom properties
 * from it and builds xterm's object from it, and neither consumer is a source. That is what
 * VS Code and Zed do, and it is the only arrangement in which the terminal is themed at all.
 *
 * **Semantic, not palette.** A theme sets `surface.raised`, never `gray-800`. The token names
 * are the contract a theme author writes against, so they are named for what they mean in
 * this window. `needs-you.base` and `danger.base` hold the same value in both built-in
 * themes and are still two tokens, because they are two meanings: one marks a chat that wants
 * the operator, the other marks an answer that cannot be undone. A theme that wanted the
 * first to shout and the second to whisper can say so; a palette token could not.
 *
 * **Values are hex and only hex.** Not a stylistic preference — a theme value is written
 * straight into a CSS declaration *and* into xterm's object, and a theme file is a file, which
 * means it is attacker-influenced in exactly the way `machine.rs` describes its own store as
 * being. `red; } body { display: none` is a CSS injection if the value is passed through, and
 * no amount of care elsewhere takes that back. Restricting the grammar to `#rgb`, `#rgba`,
 * `#rrggbb` and `#rrggbbaa` removes the question instead of answering it, and costs an author
 * nothing that a converter cannot give back. It also means every value is one xterm accepts.
 *
 * **A bad theme never stops the window.** ADR 0026 holds cold start at 2 s and the window has
 * to come up. So a token that is missing, misspelled or malformed falls back to the built-in
 * theme of the same appearance and the substitution is *reported*, rather than the theme being
 * refused whole or — worse — half-applied into something nobody can read. `load` always
 * answers with a complete theme.
 *
 * **A theme also says how the window moves** (M7.2). Durations and easings are a second, smaller
 * vocabulary under a theme file's `motion`, held to the same rules — semantic names, a closed
 * grammar, a fallback per token, nothing written outside this directory — and `motion.ts` is
 * where it and its reasons live, including why reduced motion is decided there and only there.
 */

import dark from "./charter-dark.json";
import light from "./charter-light.json";
import {
  builtInMotion,
  loadMotion,
  motionReduced,
  motionVariables,
  REDUCE,
  type Motion,
} from "./motion";

/**
 * Every token, in the order a theme file is easiest to read in.
 *
 * This array **is** the vocabulary: it is what a theme is validated against, what is written
 * to the document, and what the guard in `literals.test.ts` says the rest of the window may
 * not go around. Adding a colour to charter means adding a name here and a value to both
 * built-in themes; there is no third way to get one.
 */
export const TOKENS = [
  // The layers of the window, from the deepest to the one nearest the operator. `deep` is
  // the topmost strip and the bottom region, which read as furthest back; `base` is the
  // window itself; `raised` is a strip above it; `overlay` is a menu or a dialog.
  "surface.base",
  "surface.sunken",
  "surface.deep",
  "surface.raised",
  "surface.overlay",
  "surface.hover",

  // Things that are pressed. `aimed` is the row the keyboard is on, which is not the row the
  // pointer is over and must not look like it.
  "control.base",
  "control.hover",
  "control.aimed",
  "control.count",

  "text.primary",
  "text.secondary",
  "text.muted",

  "border.subtle",
  "border.strong",

  // What charter is drawing attention to, and where the keyboard is.
  "accent.base",
  "accent.surface",
  "focus.ring",
  "tab.active",

  // **The three strips of the axis, one quiet shade each, and the tab you are on** (ADR
  // 0036, charter-app#193). A project holds workspaces and a workspace holds chats; #171
  // drew that by indenting each row under the one above, the operator read the indent as stray
  // padding, and then turned down coloured rules in its place — *"this is not looks
  // professional, it should be minimalistic, and i prefer to change little bit backgrounds of
  // tabs and little lighter for selected tab"*. So the depth is a background, one small step
  // per row, outermost deepest, and `selected` is a step lighter than any of them.
  //
  // Neutral greys, no hue: the distinction is meant to be felt rather than noticed. Their own
  // group rather than `surface.*`, because what they mean is "which row of the axis" and a
  // theme author should be able to move them without moving every other surface in the window.
  "layer.project",
  "layer.workspace",
  "layer.chat",
  "layer.selected",
  // The hairline between two tabs of one strip — the operator: *"for tabs lets add very very
  // light visible border - just for little bit highlight separation"*. Its own token rather
  // than `border.subtle`, which sat so close to the layer shades that the dividers it drew were
  // invisible, and a theme should be able to lift the tabs apart without lifting every other
  // subtle rule in the window.
  "layer.divider",

  // The one signal this whole app exists for: a chat that has stopped and is waiting.
  "needs-you.base",
  "needs-you.text",

  // An answer that cannot be taken back, and the words charter says when something went wrong.
  "danger.base",
  "danger.surface",
  "danger.text",
  "danger.wash",

  // What a chat, or a check on a branch, is doing. `waiting-glow` is the ring around a chat
  // that wants the operator — a colour and not a shadow recipe, so a theme can turn it off by
  // making it transparent.
  "state.running",
  "state.waiting",
  "state.waiting-glow",
  "state.failed",
  "state.success",
  "state.unreadable",

  // What goes over the window when something modal is up.
  "overlay.scrim",
  "overlay.shadow",

  // The terminal. These are the reason a theme is data: none of them can be a CSS variable,
  // because xterm is handed an object.
  "terminal.background",
  "terminal.foreground",
  "terminal.cursor",
  "terminal.cursor-accent",
  "terminal.selection",
  "terminal.ansi.black",
  "terminal.ansi.red",
  "terminal.ansi.green",
  "terminal.ansi.yellow",
  "terminal.ansi.blue",
  "terminal.ansi.magenta",
  "terminal.ansi.cyan",
  "terminal.ansi.white",
  "terminal.ansi.bright-black",
  "terminal.ansi.bright-red",
  "terminal.ansi.bright-green",
  "terminal.ansi.bright-yellow",
  "terminal.ansi.bright-blue",
  "terminal.ansi.bright-magenta",
  "terminal.ansi.bright-cyan",
  "terminal.ansi.bright-white",
] as const;

/** One token's name. */
export type Token = (typeof TOKENS)[number];

/** Whether a theme is drawn on a dark window or a light one. */
export type Appearance = "dark" | "light";

/** A theme, complete: every token in {@link TOKENS} has a value. */
export type Theme = {
  /** What to call it, as a person would. */
  name: string;
  /** Which built-in a partial theme falls back to, and what `color-scheme` the window gets so
   *  native scrollbars and form controls are drawn the right way round. */
  appearance: Appearance;
  values: Record<Token, string>;
  /** How the window moves: the durations and easings in `motion.ts`, which a theme sets under
   *  `motion` exactly as it sets colours under `tokens`. */
  motion: Motion;
};

/**
 * The grammar of a value: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, and nothing else.
 *
 * See this module's header for why the set is this small. Anchored at both ends, so a value
 * that merely *starts* with a colour is not one.
 */
const HEX = /^#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;

/** Whether a string is a value a theme may hold. */
export function isColour(value: unknown): value is string {
  return typeof value === "string" && HEX.test(value);
}

/** The custom property a token is written to: `surface.base` becomes `--surface-base`. */
export function property(token: Token): string {
  return `--${token.split(".").join("-")}`;
}

/** The two themes charter ships, by the name a theme file or a setting names them by. */
export const BUILT_IN: Record<string, Theme> = {
  "charter-dark": asTheme(dark),
  "charter-light": asTheme(light),
};

/** The theme the window comes up in when nothing has said otherwise. */
export const DEFAULT_THEME: Theme = BUILT_IN["charter-dark"];

/** The pick that follows the operating system's appearance, as a project's `[theme] use` holds it
 *  (charter-app#273; `charter_core::extension::project::theme::SYSTEM`). */
export const SYSTEM = "system";

/** The media query the operating system answers with its appearance: true when it is light. */
export const PREFERS_LIGHT = "(prefers-color-scheme: light)";

/**
 * The built-in that matches the operating system's appearance right now — what a project that
 * picked "follow the system" draws (charter-app#273). Dark where the platform cannot say, which
 * is what the window comes up in.
 */
export function systemTheme(): Theme {
  const light = typeof matchMedia === "function" && matchMedia(PREFERS_LIGHT).matches;
  return light ? BUILT_IN["charter-light"] : BUILT_IN["charter-dark"];
}

/** The built-in a theme of this appearance falls back to, token by token. */
function fallbackFor(appearance: Appearance): Theme {
  return appearance === "light" ? BUILT_IN["charter-light"] : BUILT_IN["charter-dark"];
}

/**
 * A built-in, which is checked into this repo beside this file and is therefore trusted to be
 * complete — but is still checked, because a built-in with a missing token is a defect that
 * would otherwise only show up as an unreadable window.
 */
function asTheme(raw: unknown): Theme {
  const from = raw as {
    name: string;
    appearance: Appearance;
    tokens: Record<string, string>;
    motion: unknown;
  };
  const values = {} as Record<Token, string>;
  for (const token of TOKENS) {
    const value = from.tokens[token];
    if (!isColour(value)) throw new Error(`the built-in theme ${from.name} has no ${token}`);
    values[token] = value;
  }
  return {
    name: from.name,
    appearance: from.appearance,
    values,
    motion: builtInMotion(from.name, from.motion),
  };
}

/** What `load` had to put right. Every entry is a value the file did not supply usably. */
export type Complaint = {
  /** What was wrong, in the words to put in front of whoever wrote the file. */
  said: string;
  /** The token it was about, when it was about one. */
  token?: string;
};

/** A theme, and what it cost to get one. */
export type Loaded = { theme: Theme; complaints: Complaint[] };

/**
 * Reads whatever a theme file parsed to, and always answers with a complete theme.
 *
 * `raw` is untrusted: it is `JSON.parse` of a file charter did not write. Nothing here throws
 * and nothing here trusts a type — a window that will not start because a colour was spelled
 * wrong is worse than every possible wrong colour.
 */
export function load(raw: unknown): Loaded {
  const complaints: Complaint[] = [];
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) {
    return { theme: DEFAULT_THEME, complaints: [{ said: "a theme is a JSON object" }] };
  }
  const from = raw as Record<string, unknown>;

  const appearance: Appearance = from.appearance === "light" ? "light" : "dark";
  if (from.appearance !== "light" && from.appearance !== "dark") {
    complaints.push({ said: `appearance is "dark" or "light"; read this one as dark` });
  }
  const base = fallbackFor(appearance);

  let name = base.name;
  if (typeof from.name === "string" && from.name.trim() !== "") name = from.name.trim();
  else complaints.push({ said: `a theme has a name; called this one ${base.name}` });

  const holdsTokens =
    from.tokens !== null && typeof from.tokens === "object" && !Array.isArray(from.tokens);
  const tokens = holdsTokens ? (from.tokens as Record<string, unknown>) : {};
  if (!holdsTokens) complaints.push({ said: "tokens is an object of token names to colours" });

  const known = new Set<string>(TOKENS);
  for (const given of Object.keys(tokens)) {
    if (!known.has(given)) {
      complaints.push({ said: `${given} is not a charter token`, token: given });
    }
  }

  const values = {} as Record<Token, string>;
  for (const token of TOKENS) {
    const given = tokens[token];
    if (isColour(given)) {
      values[token] = given;
      continue;
    }
    values[token] = base.values[token];
    if (given !== undefined) {
      complaints.push({
        said: `${token} is ${JSON.stringify(given)}, which is not #rgb, #rgba, #rrggbb or #rrggbbaa; used ${base.values[token]}`,
        token,
      });
    }
  }

  // A theme that says nothing about motion moves the way the built-in of its appearance does,
  // exactly as a token it does not name is coloured by that built-in.
  const moved = loadMotion(from.motion, base.motion);
  for (const said of moved.said) complaints.push({ said });

  return { theme: { name, appearance, values, motion: moved.motion }, complaints };
}

/**
 * The theme as the custom properties the stylesheet reads.
 *
 * Every token, always — a theme is complete by the time it gets here, so there is no such
 * thing as a property the stylesheet asks for and does not find, and therefore no `var(--x,
 * #3a3a3a)` fallback anywhere in `App.css`. Those fallbacks were a second palette hiding in
 * the first one.
 */
export function cssVariables(theme: Theme): Record<string, string> {
  const written: Record<string, string> = {};
  for (const token of TOKENS) written[property(token)] = theme.values[token];
  return written;
}

/** Each terminal token, and the key xterm knows it by. Written out rather than derived: the
 *  names differ in shape (`cursor-accent` against `cursorAccent`, `selection` against
 *  `selectionBackground`) and a clever conversion would be one rename away from silently
 *  handing xterm a key it ignores. */
const XTERM: [Token, string][] = [
  ["terminal.background", "background"],
  ["terminal.foreground", "foreground"],
  ["terminal.cursor", "cursor"],
  ["terminal.cursor-accent", "cursorAccent"],
  ["terminal.selection", "selectionBackground"],
  ["terminal.ansi.black", "black"],
  ["terminal.ansi.red", "red"],
  ["terminal.ansi.green", "green"],
  ["terminal.ansi.yellow", "yellow"],
  ["terminal.ansi.blue", "blue"],
  ["terminal.ansi.magenta", "magenta"],
  ["terminal.ansi.cyan", "cyan"],
  ["terminal.ansi.white", "white"],
  ["terminal.ansi.bright-black", "brightBlack"],
  ["terminal.ansi.bright-red", "brightRed"],
  ["terminal.ansi.bright-green", "brightGreen"],
  ["terminal.ansi.bright-yellow", "brightYellow"],
  ["terminal.ansi.bright-blue", "brightBlue"],
  ["terminal.ansi.bright-magenta", "brightMagenta"],
  ["terminal.ansi.bright-cyan", "brightCyan"],
  ["terminal.ansi.bright-white", "brightWhite"],
];

/**
 * The theme as xterm's own object — the second consumer, built from the same values as the
 * first, which is the whole point of the file.
 */
export function xtermTheme(theme: Theme): Record<string, string> {
  const object: Record<string, string> = {};
  for (const [token, key] of XTERM) object[key] = theme.values[token];
  return object;
}

/**
 * Puts a theme on the document, and says what the window is now drawn on.
 *
 * `reduced` is whether the operator has asked for less motion, and it is read here rather than
 * by any component: the motion layer collapses every duration in one place (`motion.ts`), so a
 * component can neither forget to honour the setting nor honour it differently.
 *
 * Called from `main.tsx` **before React renders**, so no frame is ever painted in one theme
 * and repainted in another. It is `TOKENS.length` calls to `setProperty` on one element —
 * see `theme.bench.ts` for what that costs against ADR 0026's 2 s.
 *
 * `color-scheme` goes on too: it is what makes the platform's own scrollbars, text selection
 * and form controls match, and no custom property can do it.
 */
export function apply(theme: Theme, to: HTMLElement, reduced = motionReduced()): void {
  for (const [name, value] of Object.entries(cssVariables(theme)))
    to.style.setProperty(name, value);
  for (const [name, value] of Object.entries(motionVariables(theme.motion, reduced)))
    to.style.setProperty(name, value);
  to.style.colorScheme = theme.appearance;
  to.dataset.theme = theme.name;
}

/** What the running window is drawn in. Read by a pane that is opening a terminal, which then
 *  follows it through {@link onDrawn}. */
let current: Theme = DEFAULT_THEME;

/** The theme in force. */
export function inForce(): Theme {
  return current;
}

/** Puts a theme in force and on the document.
 *
 *  Not called `use`: `use` is a React hook's name in React 19, and `react-hooks/rules-of-hooks`
 *  reads any call to one as a hook call — which it then refuses inside a `try`. */
export function drawIn(theme: Theme, to: HTMLElement = document.documentElement): void {
  const changed = theme !== current;
  current = theme;
  drawnOn = to;
  apply(theme, to);
  followReducedMotion();
  if (changed) for (const follow of followers) follow(theme);
}

/**
 * **The second consumer's half of a live switch** (M6.7). The stylesheet follows a theme drawn
 * while the window is up by itself — {@link apply} rewrites the custom properties it reads —
 * but xterm was handed an *object* when a pane was built, and nothing about a custom property
 * changing reaches it. So a pane subscribes here and hands its terminal the new object; without
 * this, "one source, two consumers" held at boot and only one consumer was live.
 *
 * Told only when the theme in force actually changes. Answers the way to stop being told, which
 * a pane calls when it goes.
 */
export function onDrawn(follow: (theme: Theme) => void): () => void {
  followers.add(follow);
  return () => {
    followers.delete(follow);
  };
}

/** Everything following the theme in force: one per terminal on screen. */
const followers = new Set<(theme: Theme) => void>();

/** Where the theme in force was last drawn, so a change of the reduced-motion setting can be
 *  drawn onto the same element. */
let drawnOn: HTMLElement | null = null;

/** Whether the setting is already being followed. One listener for the life of the window. */
let following = false;

/**
 * Redraws the theme in force when the operator changes the reduced-motion setting while the
 * window is up. The platform answers the query live, and a window that only read it at launch
 * would go on animating for somebody who has just asked it to stop.
 */
function followReducedMotion(): void {
  if (following || typeof matchMedia !== "function") return;
  following = true;
  matchMedia(REDUCE).addEventListener("change", (event) => {
    if (drawnOn !== null) apply(current, drawnOn, event.matches);
  });
}
