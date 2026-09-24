import { useCallback, useEffect, useState } from "react";
import { commands, type PlaneAlerts, type PlaneId } from "./bindings";
import { usePlaneChanged } from "./planeChanged";

/**
 * **charter's alerts, for every project this window holds** — the reading the status bar's
 * button counts and the drawer lays out.
 *
 * Alerts are about a PLANE: its pin, its front door, its workspaces' layout, its root. They
 * are not about the workspace or the chat on screen, which is why they left the right-hand
 * region (a per-project surface) for a drawer the window owns. The core decides them — the
 * same `charter_core::alerts` that draws `charter statusline`'s rows — and this only asks.
 */
export type AlertsReading =
  /** Nothing has come back yet. */
  | { at: "reading" }
  /** The core's answer, one entry per project it holds. */
  | { at: "read"; planes: PlaneAlerts[] }
  /** The ask itself failed, with the core's words. */
  | { at: "failed"; why: string };

/**
 * The number the status bar may draw — or `undefined` when charter cannot stand behind one.
 *
 * **Dropped, never zero** (`footer.rs`, zone 1): a count taken before every project answered,
 * or from a project where charter stopped looking halfway, is a smaller number than the truth
 * with nothing on it saying so. Zero IS a number here, once every project has been read to the
 * end — "nothing needs you" is then a claim charter can make.
 */
export function countOf(
  reading: AlertsReading,
  planes: readonly PlaneId[],
  /** What the window itself is saying about this machine (`windowprefs.ts`), which is counted
   *  with the projects' alerts because it is drawn in the same drawer. */
  aboutThisMachine = 0,
): number | undefined {
  if (reading.at !== "read") return undefined;
  const answered = new Set(reading.planes.map((one) => one.plane));
  if (planes.some((plane) => !answered.has(plane))) return undefined;
  if (reading.planes.some((one) => one.stopped !== null)) return undefined;
  return reading.planes.reduce((total, one) => total + one.alerts.length, aboutThisMachine);
}

/** How often the reading is refreshed while nothing else asks for it. Alerts move when a
 *  plane's files move — a pin edited, a workspace reinitialised — which is minutes, not
 *  frames; and each reading asks git for one status per project. */
export const REREAD_EVERY_MS = 60_000;

/**
 * The reading, kept fresh: asked when the projects change, when one of them changes on disk
 * (`planeChanged.ts`, charter-app#264), when the window comes back into focus, when
 * {@link reread} is called (the drawer calls it as it opens), and once a minute.
 *
 * **A reading is never replaced by "reading…"** once there has been one: the drawer keeps
 * drawing the last answer while the next is on its way, rather than blanking every minute.
 */
export function useAlerts(planes: readonly PlaneId[]): {
  reading: AlertsReading;
  reread: () => void;
} {
  const [reading, setReading] = useState<AlertsReading>({ at: "reading" });
  const [asked, setAsked] = useState(0);
  const holding = planes.join("\n");
  const onDisk = usePlaneChanged(planes);

  // Written as `Extensions`'s first read is: the command's own promise, a `gone` flag, and the
  // state set inside the callback — so an answer that lands after a newer ask began, or after
  // the window let go, sets nothing.
  useEffect(() => {
    let gone = false;
    void commands
      .alertsEverywhere()
      .then((answer) => {
        if (gone) return;
        if (answer.status === "error") setReading({ at: "failed", why: answer.error });
        else if (!Array.isArray(answer.data))
          setReading({ at: "failed", why: "charter did not answer with a reading" });
        else setReading({ at: "read", planes: answer.data });
      })
      .catch((err: unknown) => {
        if (!gone) setReading({ at: "failed", why: String(err) });
      });
    return () => {
      gone = true;
    };
  }, [holding, asked, onDisk]);

  useEffect(() => {
    const again = () => setAsked((n) => n + 1);
    window.addEventListener("focus", again);
    const timer = setInterval(again, REREAD_EVERY_MS);
    return () => {
      window.removeEventListener("focus", again);
      clearInterval(timer);
    };
  }, []);

  const reread = useCallback(() => setAsked((n) => n + 1), []);
  return { reading, reread };
}
