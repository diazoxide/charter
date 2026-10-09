import { useEffect, useState } from "react";
import { commands, type HarnessGlance, type PlaneId } from "./bindings";

/**
 * **Every harness card a project has, without an open chat** (HP-19 follow-up, #1134): the
 * palette lists a row per harness, *What Codex can do here*, that opens its card tab.
 *
 * The cards are the core's (`harness_cards`, `purlis_core::harness_card::read`), the same ones
 * the picker and a chat's header draw. Only the declarations are read, never a program, so this
 * is not the picker's `start_options`, which may ask a program its version.
 *
 * Read while the project is in front: again when it comes back in front, and when `changes`
 * moves (the project's settings, where its harnesses are declared). A project whose harnesses
 * cannot be read has none to list; the picker says why when it is asked.
 */
export function useHarnessCards(
  plane: PlaneId,
  inFront: boolean,
  changes: number,
): readonly HarnessGlance[] {
  const [cards, setCards] = useState<readonly HarnessGlance[]>([]);
  useEffect(() => {
    if (!inFront) return;
    let gone = false;
    void commands
      .harnessCards(plane)
      .then((read) => {
        if (!gone && read.status === "ok" && Array.isArray(read.data)) setCards(read.data);
      })
      .catch(() => {
        // No core to ask (a test, a window going away): nothing to list.
      });
    return () => {
      gone = true;
    };
  }, [plane, inFront, changes]);
  return cards;
}
