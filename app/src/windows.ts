import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { emit, emitTo } from "@tauri-apps/api/event";

import { listen, MAIN, thisWindow } from "./here";

import { commands } from "./bindings";
import type { Offer } from "./actions";
import type { Needing, Quiet } from "./NeedsYou";
import type { Ending } from "./QuitWarning";

/**
 * charter's windows, as the page sees them (ADR 0033, amended 2026-09-26; charter#126).
 *
 * **A window holds projects, and a project is held by one window.** The main window is the one
 * a launch opens. A project tab moved into a window of its own is in a *split window*, which
 * runs this same page, so it has its own palette, its own title bar and its own `F2`, and draws
 * only the projects it holds. Which window holds which project is the core's (`Showing`), and
 * a project's events are sent to the window holding it and to no other (`windows.rs`).
 *
 * Two things are about every window at once, and this module is where they meet:
 *
 * - **Needs you.** The title bar's ✋ lists every chat, in every project, asking for the
 *   operator (ADR 0054). A split window's chats are in its own reports, so each window tells
 *   the others what it has asking, and a row for a chat in another window is carried out there.
 * - **Quit.** The main window is asked, and its warning lists what a quit would end in every
 *   window, and waits until every window has said what it has open.
 */

export { MAIN, thisWindow };

/** What one window tells the others: what is asking for the operator in it, and what a quit
 *  would end there. */
export type WindowSaid = {
  label: string;
  needing: Needing[];
  quiet: Quiet[];
  ending: Ending[];
  /** Whether every project in it has said what it has open. Until then, "nothing" is "not yet". */
  settled: boolean;
};

/** A row pressed in one window for a chat in another, carried out in the window holding it. */
export type RunElsewhere = { plane: string; offer: Offer };

/** The events the windows say these on. Emitted by the page, heard by every window. */
const SAID = "window-said";
const HELLO = "window-hello";
/** The core's: the labels of every window there is (`windows.rs`). */
const WINDOWS_CHANGED = "windows-changed";
/** Sent to one window: carry this row out. */
export const RUN_HERE = "run-offer";

/** Listens for `event`, and answers the way to stop — a window that cannot listen (a unit test,
 *  a webview being torn down) simply hears nothing. */
function hear<T>(event: string, heard: (payload: T) => void): () => void {
  let gone = false;
  let stop: (() => void) | undefined;
  void (async () => {
    try {
      const unlisten = await listen<T>(event, (e) => {
        if (!gone) heard(e.payload);
      });
      if (gone) unlisten();
      else stop = unlisten;
    } catch {
      // No window to listen in.
    }
  })();
  return () => {
    gone = true;
    try {
      stop?.();
    } catch {
      // The window may be going away under it.
    }
  };
}

/**
 * What every OTHER window has said, and this window's own share told to them.
 *
 * `mine` is told whenever it changes, and again whenever a window that has just opened says
 * hello. What a window says is dropped once the core says that window has gone.
 *
 * `settled` is whether every other window has said it is settled: a window that has not said
 * anything yet is not, so a quit asked meanwhile warns rather than ending what nobody has drawn.
 */
export function useOtherWindows(mine: Omit<WindowSaid, "label">): {
  needing: Needing[];
  quiet: Quiet[];
  ending: Ending[];
  settled: boolean;
} {
  const own = useMemo(() => thisWindow(), []);
  const [labels, setLabels] = useState<string[]>([]);
  const [heard, setHeard] = useState<Record<string, WindowSaid>>({});
  const mineNow = useRef(mine);
  useLayoutEffect(() => {
    mineNow.current = mine;
  });

  useEffect(() => {
    const say = () =>
      void emit(SAID, { ...mineNow.current, label: own } satisfies WindowSaid).catch(
        () => undefined,
      );
    const stops = [
      hear<string[]>(WINDOWS_CHANGED, (now) => setLabels(Array.isArray(now) ? now : [])),
      hear<WindowSaid>(SAID, (said) => {
        if (said.label !== own) setHeard((was) => ({ ...was, [said.label]: said }));
      }),
      hear<string>(HELLO, (from) => {
        if (from !== own) say();
      }),
    ];
    void commands
      .charterWindows()
      .then((now) => setLabels(Array.isArray(now) ? now : []))
      .catch(() => undefined);
    void emit(HELLO, own).catch(() => undefined);
    return () => stops.forEach((stop) => stop());
  }, [own]);

  // Told to the others whenever it changes. By value, so a fresh object with the same contents
  // says nothing new.
  const told = JSON.stringify(mine);
  useEffect(() => {
    void emit(SAID, { ...JSON.parse(told), label: own } as WindowSaid).catch(() => undefined);
  }, [told, own]);

  return useMemo(() => {
    const others = labels.filter((label) => label !== own);
    const from = others.flatMap((label) => (heard[label] ? [heard[label]] : []));
    return {
      needing: from.flatMap((said) => said.needing),
      quiet: from.flatMap((said) => said.quiet),
      ending: from.flatMap((said) => said.ending),
      settled: others.every((label) => heard[label]?.settled === true),
    };
  }, [heard, labels, own]);
}

/** Rows pressed in another window, for a chat this window holds: carried out here. */
export function useRunHere(run: (plane: string, offer: Offer) => void) {
  const runNow = useRef(run);
  useLayoutEffect(() => {
    runNow.current = run;
  });
  useEffect(
    () => hear<RunElsewhere>(RUN_HERE, ({ plane, offer }) => runNow.current(plane, offer)),
    [],
  );
}

/**
 * Carries a row out in the window holding `plane`, bringing that window to the front first —
 * a Go on a chat in another window is "that chat, in front", and it is in front only in its
 * own window. Answers whether any window holds it.
 */
export async function runElsewhere(plane: string, offer: Offer): Promise<boolean> {
  const label = await commands.showWindowHolding(plane).catch(() => null);
  if (label === null) return false;
  await emitTo(label, RUN_HERE, { plane, offer } satisfies RunElsewhere).catch(() => undefined);
  return true;
}
