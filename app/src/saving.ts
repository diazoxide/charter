import { useCallback, useEffect, useState } from "react";
import { commands, type PlaneId, type PlaneSaving } from "./bindings";
import { usePlaneChanged } from "./planeChanged";

/** The window event a finished save sends, so every reader of the save standing reads again —
 *  the title bar after the Saving tab's button, and the tab after the title bar's. */
export const PLANE_SAVED = "charter-plane-saved";

/** The window event the Saving tab sends to leave a blocked save (charter-app#295): a chat, or a
 *  terminal, opened in the plane by the project that holds it. */
export const WAY_OUT = "charter-saving-way-out";

/** What a way out asks for, and of which project. */
export type WayOut = { plane: string; way: "chat" | "terminal" };

/** Ask the project `plane` to open a chat or a terminal in its plane. */
export function askWayOut(plane: string, way: WayOut["way"]): void {
  window.dispatchEvent(new CustomEvent<WayOut>(WAY_OUT, { detail: { plane, way } }));
}

/** Say that a save of some project finished. */
export function tellSaved(): void {
  window.dispatchEvent(new Event(PLANE_SAVED));
}

/** How often the standing is read while nothing else asks. A file an agent writes is not an
 *  event this window hears (`planewatch.rs` watches no memory directory and no `.git`), so the
 *  title bar asks git — one `status` — this often. */
export const SAVING_REREAD_MS = 10_000;

/**
 * **The project's save standing, kept fresh** (charter-app#294): asked when the project
 * changes, when it changes on disk, when the window comes back into focus, when any save
 * finishes, and every {@link SAVING_REREAD_MS}. Written as `useAlerts` is: the command's own
 * promise, a `gone` flag, and state set only in its callback.
 *
 * The last answer is kept while the next is on its way, so the bar never blanks between reads.
 */
export function usePlaneSaving(plane: PlaneId | undefined): {
  saving: PlaneSaving | undefined;
  reread: () => void;
} {
  const [saving, setSaving] = useState<{ plane: PlaneId; standing: PlaneSaving }>();
  const [asked, setAsked] = useState(0);
  const changed = usePlaneChanged(plane === undefined ? [] : [plane]);

  useEffect(() => {
    if (plane === undefined) return;
    let gone = false;
    void commands
      .planeSaving(plane)
      .then((answer) => {
        // Only a standing is drawn: an answer that is not one says nothing about the plane.
        if (!gone && answer.status === "ok" && typeof answer.data?.stage === "string")
          setSaving({ plane, standing: answer.data });
      })
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [plane, asked, changed]);

  // Coming back to the window is when the operator wants to know what came in: fetch, which the
  // core does at most once a minute, and whose answer reaches here as a plane change.
  useEffect(() => {
    if (plane === undefined) return;
    const fetch = () => void commands.planeFetch(plane).catch(() => undefined);
    window.addEventListener("focus", fetch);
    return () => window.removeEventListener("focus", fetch);
  }, [plane]);

  useEffect(() => {
    const again = () => setAsked((n) => n + 1);
    window.addEventListener("focus", again);
    window.addEventListener(PLANE_SAVED, again);
    const timer = setInterval(again, SAVING_REREAD_MS);
    return () => {
      window.removeEventListener("focus", again);
      window.removeEventListener(PLANE_SAVED, again);
      clearInterval(timer);
    };
  }, []);

  const reread = useCallback(() => setAsked((n) => n + 1), []);
  // Another project's answer is not this one's: drawn only for the project it was read for.
  return { saving: saving?.plane === plane ? saving?.standing : undefined, reread };
}
