import { useEffect, useState } from "react";
import { commands, type AskOffer, type PlaneId } from "./bindings";

/**
 * What **Ask {persona}…** offers on a project's chats, as the core answers
 * (`ask_persona_offer`): the personas that can be asked, or why none can.
 *
 * **Which personas are finished, and whether policy locks dispatch, are the core's to say.**
 * The window draws a row per persona it is handed and holds no rule of its own about either.
 *
 * Asked again when the project's personas change, by name or on disk (a draft that is
 * finished), and when its settings change on disk, which is when the answer can have moved:
 * `changes` counts both. Until the core has answered there is none,
 * and no row: a core that answers nothing (an older one, a test's stand-in) offers nothing.
 */
export function useAskOffer(
  plane: PlaneId,
  personas: readonly string[] | undefined,
  changes: number,
  /** The chats open here, as one word: which asks policy takes off a tab is per chat. */
  chats: string,
): AskOffer | undefined {
  // By name and not by the list itself: the panels are read again for a todo or a memory,
  // which hands this the same personas in a new list, and that is not a reason to ask.
  const named = (personas ?? []).join("\n");
  const [known, setKnown] = useState<{ plane: PlaneId; offer: AskOffer }>();
  useEffect(() => {
    let gone = false;
    void commands
      .askPersonaOffer(plane)
      .then((answer) => {
        if (gone || answer?.status !== "ok") return;
        const offer = answer.data as AskOffer | null | undefined;
        if (offer && Array.isArray(offer.personas))
          setKnown({
            plane,
            offer: {
              personas: offer.personas,
              locked: offer.locked ?? null,
              locked_for: Array.isArray(offer.locked_for) ? offer.locked_for : [],
            },
          });
      })
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [plane, named, changes, chats]);
  // Keyed by the project it is about: another project's answer offers nothing here.
  return known?.plane === plane ? known.offer : undefined;
}
