import { useSyncExternalStore } from "react";
import { CHAT_KEYBOARD } from "./actions";
import { onAMac } from "./tabKeys";
import { atCreation, sayAboutThisMachine, type Reading } from "./windowprefs";

/**
 * **How big the text is: the window's and the terminal's, two sizes, per machine**
 * (charter-app#283).
 *
 * The operator, from the running app: *"now font size is very small, and will be good to make
 * default font size 1-2 px bigger"* — and, grilled, two sizes rather than one, because a
 * terminal's grid and the window's chrome are read at different distances and a person who
 * wants one bigger does not always want the other.
 *
 * **Where they are kept: the layout file, beside the regions** (`charter/layout.json`,
 * `regions.ts`, #216). Both are "how one operator likes their window", which is the whole of
 * what that file is for, and neither is anything a plane should carry to somebody else's
 * machine: a size committed to `charter.toml` would change the text on every clone. The file
 * is read before the window exists and handed to it, so the first frame is drawn at the size
 * the operator left — no fetch, no reflow a moment after launch. `regions.ts` writes the whole
 * document, so it writes these too ({@link textSizes}), and a size change is one more reason for
 * it to write ({@link onTextSizes}).
 *
 * **The window's size is the root's `font-size`**, and every size in `App.css` is in `rem` or
 * `em`, so the whole UI scales from that one number. The terminal's is xterm's `fontSize`,
 * which `SessionPane` hands every live pane and then refits, so the rows and columns the
 * program is told are the ones that now fit.
 */

/** Which of the two sizes. */
export type Which = "window" | "terminal";

export type TextSizes = Record<Which, number>;

/** The smallest a size may be, in px. Below this the terminal's cells stop being legible. */
export const LEAST_TEXT = 10;
/** The largest, in px. Above this a laptop's terminal is a few dozen columns. */
export const MOST_TEXT = 24;

/**
 * The sizes a machine starts at: **one step bigger than both were** (#283). The window was
 * `13px` in `App.css` and the terminal `12` in `SessionPane`.
 */
export const DEFAULT_TEXT: TextSizes = { window: 14, terminal: 13 };

/** What the two sizes are called, in a sentence and on the Preferences tab. */
export const TEXT_NAMES: Record<Which, string> = {
  window: "window text size",
  terminal: "terminal text size",
};

/**
 * The sizes a layout document holds, read field by field, and what had to be put right.
 *
 * A document with no `text` at all is an older charter's, or one written by hand before this
 * existed, and is the defaults with nothing to say. A size that is not a whole number within the
 * bounds is the default, and said — a size that silently came out as 14 is a preference that
 * looks ignored.
 */
export function loadText(raw: unknown): { sizes: TextSizes; said: string[] } {
  const said: string[] = [];
  const held =
    raw !== null && typeof raw === "object" && !Array.isArray(raw)
      ? (raw as { text?: unknown }).text
      : undefined;
  if (held === undefined) return { sizes: DEFAULT_TEXT, said };
  if (held === null || typeof held !== "object" || Array.isArray(held)) {
    said.push(`"text" ${JSON.stringify(held)} is not the two text sizes, so both are the defaults`);
    return { sizes: DEFAULT_TEXT, said };
  }
  const from = held as Record<string, unknown>;
  const one = (which: Which): number => {
    const given = from[which];
    if (given === undefined) return DEFAULT_TEXT[which];
    if (isTextSize(given)) return given;
    said.push(
      `${TEXT_NAMES[which]} ${JSON.stringify(given)} is not a whole number from ${LEAST_TEXT} to ${MOST_TEXT}, so it is ${DEFAULT_TEXT[which]}`,
    );
    return DEFAULT_TEXT[which];
  };
  return { sizes: { window: one("window"), terminal: one("terminal") }, said };
}

function isTextSize(size: unknown): size is number {
  return Number.isInteger(size) && (size as number) >= LEAST_TEXT && (size as number) <= MOST_TEXT;
}

/** `px` as a size this window can be: whole, and within the bounds. Not a number is the least. */
export function clampText(px: number): number {
  if (!Number.isFinite(px)) return LEAST_TEXT;
  return Math.min(MOST_TEXT, Math.max(LEAST_TEXT, Math.round(px)));
}

/** What this launch changed the sizes to, if anything. */
let changed: TextSizes | undefined;
/** The sizes the launch started from, read once — and said once, when they cost anything. */
let started: TextSizes | undefined;
const listeners = new Set<(sizes: TextSizes) => void>();

/** The sizes the launch started from: the layout file's, when it had usable ones. */
function startingText(layout: Reading = atCreation().layout): TextSizes {
  if (started !== undefined) return started;
  // A layout file charter refused is said once, by `regions.ts`, and is the defaults here too.
  const { sizes, said } =
    layout.found && layout.trouble === null
      ? loadText(layout.document)
      : { sizes: DEFAULT_TEXT, said: [] };
  if (said.length > 0) {
    const where = layout.path || "the layout file";
    sayAboutThisMachine("text", {
      severity: "warn",
      detail: `${where}: ${said.join("; ")}`,
      remedy: `fix ${where}, or set the size on the Preferences tab, which rewrites it`,
    });
  }
  started = sizes;
  return sizes;
}

/** The two sizes in force. */
export function textSizes(): TextSizes {
  return changed ?? startingText();
}

/** Sets one size, clamped to the bounds; tells every listener when it changed. */
export function setTextSize(which: Which, px: number): void {
  const was = textSizes();
  const next = clampText(px);
  if (was[which] === next) return;
  changed = { ...was, [which]: next };
  sayAboutThisMachine("text", undefined);
  for (const listener of listeners) listener(changed);
}

/** One step, of 1px, bigger or smaller. */
export function stepText(which: Which, by: 1 | -1): void {
  setTextSize(which, textSizes()[which] + by);
}

/** Back to the default. */
export function resetText(which: Which): void {
  setTextSize(which, DEFAULT_TEXT[which]);
}

/** Calls `listener` whenever a size changes. Answers the way to stop. */
export function onTextSizes(listener: (sizes: TextSizes) => void): () => void {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

/** {@link textSizes}, for a component that redraws when they change. */
export function useTextSizes(): TextSizes {
  return useSyncExternalStore(onTextSizes, textSizes);
}

/** Forgets what this launch changed and read, as a new launch would. For tests. */
export function forgetTextSizes(): void {
  changed = undefined;
  started = undefined;
}

/** Draws the window's text at `px`: the root's font size, which every `rem` is measured by. */
export function drawWindowText(px: number, root: HTMLElement = document.documentElement): void {
  root.style.fontSize = `${px}px`;
}

/** What a size key asks for. */
export type SizeKey = "bigger" | "smaller" | "reset";

/**
 * Whether this keystroke is a text-size key, and which.
 *
 * **`⌘` on a Mac and `Ctrl` elsewhere, with `=` or `+`, `-`, or `0`** — the zoom keys of every
 * browser, of VS Code, and of GNOME Terminal, Konsole and Windows Terminal. One modifier per
 * platform, so a Mac's `Ctrl` chords stay the terminal's whatever a later xterm sends for them.
 *
 * **Taken from the chat, and that takes nothing** (`docs/ui-primitives.md`, #106, #187). A
 * `⌘` chord is nothing to xterm.js 6.0.0 but `⌘A`. With `Ctrl`, `evaluateKeyboardEvent` encodes
 * a letter, space, `3`–`8`, `[`, `\` and `]` — not `=`, `-` or `0`, which send no bytes. The one
 * neighbour a shell does own is **`Ctrl+Shift+-`**: its `key` is `_`, and xterm sends that as
 * `^_`, readline's `undo`. So `-` is matched only without `Shift`, and `_` never. (A real
 * xterm sends `^_` for a plain `Ctrl+-` as well; xterm.js does not, and xterm.js is the
 * terminal in every pane here.)
 */
export function sizeKey(e: KeyboardEvent, mac: boolean = onAMac()): SizeKey | undefined {
  const held = mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  if (!held || e.altKey) return undefined;
  if (e.key === "=" || e.key === "+") return "bigger";
  if (e.shiftKey) return undefined;
  if (e.key === "-") return "smaller";
  if (e.key === "0") return "reset";
  return undefined;
}

/**
 * Whose size a key changes: **the terminal's in a terminal pane, the window's anywhere else.**
 *
 * The question the palette asks (`theChatKeepsIt`): where the keystroke was delivered. Inside a
 * pane that is xterm's textarea, a descendant of the holder marked {@link CHAT_KEYBOARD}.
 */
export function whoseSize(target: EventTarget | null): Which {
  return target instanceof Element && target.closest(`[${CHAT_KEYBOARD}]`) !== null
    ? "terminal"
    : "window";
}

/**
 * Listens for the size keys on `on`, **captured**, so a pane's terminal never sees one: xterm
 * reads its own textarea, and a capturing listener on the window runs first. Answers the way to
 * stop.
 */
export function listenForSizeKeys(on: Window): () => void {
  const key = (e: KeyboardEvent) => {
    const asked = sizeKey(e);
    if (asked === undefined) return;
    e.preventDefault();
    e.stopPropagation();
    const which = whoseSize(e.target);
    if (asked === "reset") resetText(which);
    else stepText(which, asked === "bigger" ? 1 : -1);
  };
  on.addEventListener("keydown", key, true);
  return () => on.removeEventListener("keydown", key, true);
}
