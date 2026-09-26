import {
  listen as listenAnywhere,
  type EventCallback,
  type UnlistenFn,
} from "@tauri-apps/api/event";

/**
 * This window's own name, and the one way the page listens for the core's events (charter#126).
 *
 * **Every listener names this window as its target.** `@tauri-apps/api/event`'s `listen`
 * defaults to the target `Any`, and Tauri delivers every event to an `Any` listener, including
 * one the core sent to a different window with `emit_to`. A second window listening that way
 * would take in a project moved to the main window, answer the main window's quit, and open a
 * second launch's directory as well. A listener that names its own window still hears what the
 * core sends to every window (`emit`), and hears `emit_to` only when it is addressed here.
 */

type Internals = { metadata?: { currentWindow?: { label?: string } } };

/** The main window's label: the one `tauri.conf.json` declares. */
export const MAIN = "main";

/**
 * This window's label, as Tauri gave it — the main window's when the page is not in a Tauri
 * window at all (a unit test), which is the window every test before split windows was about.
 *
 * Read from the metadata Tauri writes into every page before it loads, which is what
 * `getCurrentWindow()` reads too; that call throws outside a Tauri window rather than answering.
 */
export function thisWindow(): string {
  const internals = (globalThis as { __TAURI_INTERNALS__?: Internals }).__TAURI_INTERNALS__;
  return internals?.metadata?.currentWindow?.label ?? MAIN;
}

/** Listens for `event` in this window: what the core sends every window, and what it sends
 *  this one. Never what it sends another window. */
export function listen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
  return listenAnywhere<T>(event, handler, { target: thisWindow() });
}
