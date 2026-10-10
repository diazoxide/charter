/**
 * **The window's half of the asks' system notifications** (#1694, spec #1688, I-7).
 *
 * The core decides what is sent (`asknotify.rs`): only an ask raises a notification, never an
 * update; one chat's asks a few seconds apart share one; and nothing is sent while the person
 * is looking, at the chat itself or at the Inbox in the window in front. Two things only the
 * window knows, and it says them here:
 *
 * - **whether a project's Inbox is open** ({@link useInboxOpenTold}), which the core reads with
 *   whether that window is focused and has the project in front;
 * - **what a click on a notification lands on** ({@link useNotificationLanding}): the Inbox, at
 *   the group of the chat it was about.
 *
 * **A click is read as the window coming to the front.** On the desktop the system brings the
 * app forward when its notification is clicked, and tells it nothing more, so the window holds
 * the last notification sent while it was behind and lands on it the next time it gets the
 * focus, once. A window already in front when one was sent holds nothing: no click can bring it
 * forward, and a later return to it is not that click.
 */
import { useEffect, useRef } from "react";
import { commands } from "./bindings";
import { listen } from "./here";

/** The event the core sends the window holding a project when it notified about a chat's asks. */
export const ASKS_NOTIFIED = "asks-notified";

/** Where a notification lands: a project's Inbox, at one chat's group. */
export type Landing = { plane: string; session: number };

const isLanding = (said: unknown): said is Landing =>
  typeof said === "object" &&
  said !== null &&
  typeof (said as Landing).plane === "string" &&
  Number.isInteger((said as Landing).session);

/**
 * Tells the core whether `plane`'s Inbox is open in this window, each time that changes, and
 * that it is closed once the project's view goes. Best effort: a core that does not hear it
 * reads the Inbox as closed, so a notification is sent rather than held back.
 */
export function useInboxOpenTold(plane: string, open: boolean): void {
  useEffect(() => {
    void commands.inboxShown(plane, open).catch(() => undefined);
    return () => {
      if (open) void commands.inboxShown(plane, false).catch(() => undefined);
    };
  }, [plane, open]);
}

/**
 * Lands on the Inbox at a notification's group (`land`) when the window comes to the front
 * after the core sent one while it was behind: the latest one, once.
 */
export function useNotificationLanding(land: (to: Landing) => void): void {
  const landing = useRef(land);
  useEffect(() => {
    landing.current = land;
  }, [land]);
  useEffect(() => {
    let waiting: Landing | undefined;
    let gone = false;
    const stop = listen<unknown>(ASKS_NOTIFIED, (said) => {
      if (!isLanding(said.payload)) return;
      waiting = document.hasFocus() ? undefined : said.payload;
    }).catch(() => undefined);
    const focused = () => {
      if (gone || waiting === undefined) return;
      const to = waiting;
      waiting = undefined;
      landing.current(to);
    };
    window.addEventListener("focus", focused);
    return () => {
      gone = true;
      window.removeEventListener("focus", focused);
      void stop.then((off) => off?.()).catch(() => undefined);
    };
  }, []);
}
