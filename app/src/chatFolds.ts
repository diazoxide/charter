/**
 * **The folds the person set on the Chats list, kept across a reload of the window** (#1459,
 * V100-48: "a hand-set fold is kept").
 *
 * Kept in the window's session storage, by project, and nowhere else: a fold names a chat by
 * its number, which holds for as long as the app runs and no longer, so a reload of the window
 * keeps it and a new launch starts from the folds the list makes by itself. Nothing here is
 * needed: storage a webview refuses, or a value that is not one, is no fold.
 */

const KEY = "purlis.chats.folds:";

/** The folds kept for `plane`, by chat: true is folded. */
export function keptFolds(plane: string | undefined): ReadonlyMap<number, boolean> {
  const folds = new Map<number, boolean>();
  if (plane === undefined) return folds;
  try {
    const raw: unknown = JSON.parse(globalThis.sessionStorage.getItem(KEY + plane) ?? "null");
    if (raw === null || typeof raw !== "object" || Array.isArray(raw)) return folds;
    for (const [session, shut] of Object.entries(raw as Record<string, unknown>)) {
      const number = Number(session);
      if (Number.isInteger(number) && number > 0 && typeof shut === "boolean")
        folds.set(number, shut);
    }
  } catch {
    // No storage, or nothing readable in it: no fold.
  }
  return folds;
}

/** Keeps `folds` for `plane`, in place of what was kept. */
export function keepFolds(plane: string | undefined, folds: ReadonlyMap<number, boolean>): void {
  if (plane === undefined) return;
  try {
    if (folds.size === 0) globalThis.sessionStorage.removeItem(KEY + plane);
    else globalThis.sessionStorage.setItem(KEY + plane, JSON.stringify(Object.fromEntries(folds)));
  } catch {
    // Storage refused: the fold holds for this window only, as it did before.
  }
}
