/**
 * **The dispatches every project refused while nobody was there** (#1507), for the title
 * bar's needs-you list to show and answer.
 *
 * The core says a project's whole list each time a refusal is kept (`dispatch-away-changed`),
 * and the window asks once per project for what was kept before it listened, and again each
 * time the list is opened: a pair the person said never to in the meantime is gone by the time
 * they look. Each answer comes back with the list as it then stands.
 */
import { useCallback, useEffect, useState } from "react";
import { commands, type AwayRefusal } from "./bindings";
import { listen } from "./here";

/** The event the core says a project's list on. */
export const AWAY_CHANGED = "dispatch-away-changed";

/** What the core says on that event: a project and its whole list (`AwayRefusals`). */
type AwayRefusals = { plane: string; refused: AwayRefusal[] };

/** Each project's list, by its id: the last whole list the core said for it. */
export type AwayHeld = Record<string, readonly AwayRefusal[]>;

/** What a press on an item answered: the sentence to say, and whether it is a refusal. */
export type AwayAnswer = { words: string; refused: boolean } | undefined;

export function useAwayRefusals(planes: readonly string[]): {
  held: AwayHeld;
  /** Reads every project's list again: what the list does as it opens. */
  read: () => void;
  /** Allow from now on, for the one pair the item names. */
  allow: (plane: string, item: AwayRefusal) => Promise<AwayAnswer>;
  /** Dismiss: the item goes and nothing is granted. */
  dismiss: (plane: string, item: AwayRefusal) => Promise<AwayAnswer>;
} {
  const [held, setHeld] = useState<AwayHeld>({});
  useEffect(() => {
    const listening = listen<AwayRefusals>(AWAY_CHANGED, (event) => {
      setHeld((was) => ({ ...was, [event.payload.plane]: event.payload.refused }));
    }).catch(() => undefined);
    return () => void listening.then((stop) => stop?.()).catch(() => undefined);
  }, []);
  const key = planes.join("\n");
  const readOne = useCallback((plane: string, live: () => boolean = () => true) => {
    void commands.dispatchAway(plane).then(
      (answer) => {
        // A core that answers nothing for it (an older build, a test's mock) holds none.
        const refused = answer.status === "ok" ? (answer.data as AwayRefusal[] | null) : null;
        if (live() && Array.isArray(refused)) setHeld((was) => ({ ...was, [plane]: refused }));
      },
      () => {},
    );
  }, []);
  useEffect(() => {
    let live = true;
    for (const plane of key === "" ? [] : key.split("\n")) readOne(plane, () => live);
    return () => {
      live = false;
    };
  }, [key, readOne]);
  const read = useCallback(() => {
    for (const plane of key === "" ? [] : key.split("\n")) readOne(plane);
  }, [key, readOne]);
  const allow = useCallback(
    async (plane: string, item: AwayRefusal): Promise<AwayAnswer> => {
      const answer = await commands
        .allowDispatchAway(plane, item.asking, item.target, item.workspace)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (answer.status !== "ok") {
        // Refused: the pair is no longer one to allow. What is listed now is read again.
        readOne(plane);
        return { words: answer.error, refused: true };
      }
      setHeld((was) => ({ ...was, [plane]: answer.data.refused }));
      return { words: answer.data.said, refused: false };
    },
    [readOne],
  );
  const dismiss = useCallback(
    async (plane: string, item: AwayRefusal): Promise<AwayAnswer> => {
      const answer = await commands
        .dismissDispatchAway(plane, item.asking, item.target, item.workspace)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (answer.status !== "ok") {
        readOne(plane);
        return { words: answer.error, refused: true };
      }
      setHeld((was) => ({ ...was, [plane]: answer.data }));
      return undefined;
    },
    [readOne],
  );
  return { held, read, allow, dismiss };
}
