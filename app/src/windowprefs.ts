import { useSyncExternalStore } from "react";
import type { AlertRow } from "./bindings";
import { load, type Theme } from "./theme/theme";

/**
 * **What the window was handed as it was created**: the operator's layout file and theme file,
 * read by the Rust side before the window existed (`src-tauri/src/windowprefs.rs`).
 *
 * Both are files beside `machine.json` (`docs/design-system.md` has their formats), and both
 * are needed by the very first frame — a layout drawn after it is a re-layout the operator
 * watches happen. A Tauri command is asynchronous, so they are not fetched: the window's
 * initialization script defines {@link GLOBAL} before any of this code runs, and reading it is
 * a property access.
 */

/** The global the initialization script defines. `windowprefs.rs` names it too. */
export const GLOBAL = "__CHARTER_AT_CREATION__";

/** One file's reading — `charter_core::windowprefs::Reading`, as serialised. */
export type Reading = {
  /** Where the file is, or would be. */
  path: string;
  /** Whether there is a file at all. */
  found: boolean;
  /** What it holds, when charter can use it. Untrusted: a file anybody may have edited. */
  document: unknown;
  /** Why a file that is there is not in force. */
  trouble: string | null;
};

/** Both readings. */
export type AtCreation = { layout: Reading; theme: Reading };

const NOTHING: Reading = { path: "", found: false, document: null, trouble: null };

/**
 * What the window was handed, or two empty readings when it was handed nothing — a page opened
 * outside the app, or a test that has not set one. Checked field by field rather than trusted,
 * because a reading the window cannot make sense of must cost the preference and never the
 * window.
 */
export function atCreation(): AtCreation {
  const given = (globalThis as Record<string, unknown>)[GLOBAL];
  const held =
    given !== null && typeof given === "object" ? (given as Record<string, unknown>) : {};
  return { layout: reading(held.layout), theme: reading(held.theme) };
}

function reading(raw: unknown): Reading {
  if (raw === null || typeof raw !== "object") return NOTHING;
  const from = raw as Record<string, unknown>;
  return {
    path: typeof from.path === "string" ? from.path : "",
    found: from.found === true,
    document: from.document ?? null,
    trouble: typeof from.trouble === "string" ? from.trouble : null,
  };
}

/**
 * **What charter says about this machine rather than about a project** — a layout file it could
 * not use, a theme file with a token it had to put right, a layout it could not keep.
 *
 * The alerts drawer is where charter says what is wrong, and every alert it had was about a
 * plane. These are not: they are about files in the operator's config directory, and they are
 * known to the window rather than to the core, because it is the window's vocabulary — the
 * regions and the tokens — that decides whether a file said something usable. So the window
 * keeps them here, one per subject, and the drawer draws them above the projects.
 */
const said = new Map<string, AlertRow>();
let listed: AlertRow[] = [];
const listeners = new Set<() => void>();

/** Says something about this machine under `subject`, replacing what was said under it before,
 *  or takes it back with `undefined`. */
export function sayAboutThisMachine(subject: string, row: Omit<AlertRow, "subject"> | undefined) {
  if (row === undefined) {
    if (!said.delete(subject)) return;
  } else {
    said.set(subject, { ...row, subject });
  }
  listed = [...said.values()];
  for (const listener of listeners) listener();
}

/** What is being said about this machine right now. */
export function aboutThisMachine(): AlertRow[] {
  return listed;
}

/** {@link aboutThisMachine}, for a component that redraws when it changes. */
export function useAboutThisMachine(): AlertRow[] {
  return useSyncExternalStore((listener) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  }, aboutThisMachine);
}

/** {@link theirTheme} of what the window was handed, read once: so drawing it again after a
 *  project's own theme is the same object, and repaints nothing that is already drawn. */
let theirs: { theme: Theme | undefined } | undefined;

export function theirThemeOnce(): Theme | undefined {
  theirs ??= { theme: theirTheme() };
  return theirs.theme;
}

/** For tests: read the handed theme again. */
export function forgetTheirTheme() {
  theirs = undefined;
}

/**
 * **The operator's own theme, or none** (M6.7) — `charter/theme.json`, the address
 * `docs/design-system.md` gave it, and until now read by nothing.
 *
 * It arrives with the window, so `main.tsx` draws it **before the first frame**: an operator
 * with a theme of their own never sees the built-in painted first. `theme.load` is what judges
 * it — every token against the vocabulary and the hex grammar — and whatever it had to put
 * right is said in the alerts drawer, with the file's path, because a colour that silently
 * came out as the built-in's is a theme that looks like it was ignored.
 *
 * A file that is not a theme at all is said the same way, and the window keeps the built-in.
 */
export function theirTheme(reading: Reading = atCreation().theme): Theme | undefined {
  const where = reading.path || "the theme file";
  if (reading.trouble !== null) {
    sayAboutThisMachine("theme", {
      severity: "warn",
      detail: `${reading.trouble} — the window is drawn in the built-in theme`,
      remedy: `fix ${where}, or delete it to keep the built-in`,
    });
    return undefined;
  }
  if (reading.document === null) return undefined;
  const { theme, complaints } = load(reading.document);
  if (complaints.length > 0) {
    sayAboutThisMachine("theme", {
      severity: "warn",
      detail: `${where}: ${complaints.map((one) => one.said).join("; ")}`,
      remedy: `fix ${where}; everything else in it is drawn`,
    });
  }
  return theme;
}
