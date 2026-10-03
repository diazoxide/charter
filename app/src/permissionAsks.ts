/**
 * **The permission prompts every project holds open for the operator** (HP-6), for the title
 * bar's needs-you list to show and answer.
 *
 * The core says a project's whole list each time it changes (`asks-changed`), and the window
 * asks once per project for what it already held before it listened. Answering sends the
 * option back on the chat's own permission hook (`answer_ask`); the list then changes, and is
 * said again, whether the answer applied or was refused.
 */
import { useEffect, useState } from "react";
import { commands, type Asking, type Shown } from "./bindings";
import { listen } from "./here";

/** The event the core says a project's asks on. */
export const ASKS_CHANGED = "asks-changed";

/** Each project's open asks, by its id: the last whole list the core said for it. */
export function usePermissionAsks(planes: readonly string[]): Record<string, readonly Shown[]> {
  const [held, setHeld] = useState<Record<string, readonly Shown[]>>({});
  useEffect(() => {
    const listening = listen<Asking>(ASKS_CHANGED, (event) => {
      setHeld((was) => ({ ...was, [event.payload.plane]: event.payload.asks }));
    }).catch(() => undefined);
    return () => void listening.then((stop) => stop?.()).catch(() => undefined);
  }, []);
  const key = planes.join("\n");
  useEffect(() => {
    let live = true;
    for (const plane of key === "" ? [] : key.split("\n")) {
      void commands.pendingAsks(plane).then(
        (answer) => {
          // A core that answers nothing for it (an older build, a test's mock) holds none.
          const asks = answer.status === "ok" ? (answer.data as Asking | null)?.asks : undefined;
          if (live && asks) setHeld((was) => (plane in was ? was : { ...was, [plane]: asks }));
        },
        () => {},
      );
    }
    return () => {
      live = false;
    };
  }, [key]);
  return held;
}

/** Answers chat `session`'s ask in `plane` with `option`: nothing when it applied, and the
 *  core's sentence when it was refused — answered elsewhere, timed out, withdrawn. */
export async function answerAsk(
  plane: string,
  session: number,
  ask: string,
  option: string,
): Promise<string | undefined> {
  const answer = await commands
    .answerAsk(plane, session, ask, option)
    .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
  return answer.status === "ok" ? undefined : answer.error;
}
