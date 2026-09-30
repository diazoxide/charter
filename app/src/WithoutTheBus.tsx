import { useEffect, useState } from "react";
import { listen } from "./here";

import { commands, type WithoutTheBus } from "./bindings";

/**
 * The line a launch without the session bus puts at the top of the window (charter#746).
 *
 * On Linux charter starts without the session bus when the desktop portal is silent, which
 * would otherwise cost half a minute, and when there is no bus to be had at all (`portal.rs`).
 * For that run there is no tray icon, no desktop notifications, and a second launch is refused
 * instead of being handed over. Standard error says so too, but an operator who clicked an icon
 * has none, so it is said here, where they can see it.
 *
 * A little after the launch the core asks the bus once more. When the portal answers by then
 * — it was only slow — the core says so on `session-bus://answers` and the line offers to
 * restart on the bus. **The operator's choice, never an automatic one**: a restart ends every
 * chat, and the next launch offers them back the way it does after a quit.
 *
 * The same line as a slow launch's (`came-back`), and dismissible like it: the run is what it
 * is, and the news does not improve by staying up.
 */
export function SessionBusNotice() {
  const [notice, setNotice] = useState<WithoutTheBus | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    let gone = false;
    void commands
      .sessionBus()
      .then((now) => {
        if (!gone && now) setNotice((was) => was ?? now);
      })
      .catch(() => undefined);
    const listening = listen<WithoutTheBus>("session-bus://answers", (event) => {
      if (!gone) setNotice(event.payload);
    }).catch(() => undefined);
    return () => {
      gone = true;
      void listening.then((stop) => stop?.()).catch(() => undefined);
    };
  }, []);

  if (!notice || dismissed) return null;
  return (
    <p className="came-back trouble" role="status">
      {notice.says}
      {notice.can_restart && (
        <>
          {" "}
          The session bus answers now. A restart ends every chat, and the next launch offers them
          back.
          <button
            type="button"
            className="offer"
            onClick={() => void commands.restartOnTheSessionBus().catch(() => undefined)}
          >
            Restart with the full desktop integration
          </button>
        </>
      )}
      <button type="button" className="dismiss" onClick={() => setDismissed(true)}>
        Dismiss
      </button>
    </p>
  );
}
