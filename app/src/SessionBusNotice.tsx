import { useEffect, useState } from "react";
import { listen } from "./here";

import { commands, SESSION_BUS_ANSWERS, type BusNotice } from "./bindings";
import { MidTurn, mightBeMidTurn, type Ending } from "./QuitWarning";

/**
 * The line a launch without the session bus puts at the top of the window (charter#746).
 *
 * On Linux charter starts without the session bus when the desktop portal is silent, which
 * would otherwise cost half a minute, and when there is no bus to be had at all (`portal.rs`).
 * For that run there is no tray icon, no desktop notifications, no keyring vault from the
 * window, and a second launch is refused instead of being handed over. Standard error says so
 * too, but an operator who clicked an icon has none, so it is said here, where they can see it.
 *
 * After the launch the core asks the bus again, backing off, until it answers. When the portal
 * answers — it was only slow — the core says so on `SESSION_BUS_ANSWERS` and the line offers
 * to restart on the bus, **coming back if it was dismissed**, since that is news. The restart is
 * the operator's choice, never an automatic one: it ends every chat, so a chat that could be
 * mid-turn is named and asked about first, with Restart to update's question (`MidTurn`), and the
 * next launch offers them back the way it does after a quit.
 *
 * The same line as a slow launch's (`came-back`), and dismissible like it.
 *
 * `chats` is every chat the window holds, as the quit warning is given them.
 */
export function SessionBusNotice({ chats = [] }: { chats?: readonly Ending[] }) {
  const [notice, setNotice] = useState<BusNotice | null>(null);
  const [dismissed, setDismissed] = useState(false);
  /** Whether the ask about mid-turn chats is up. */
  const [asking, setAsking] = useState(false);

  useEffect(() => {
    let gone = false;
    void commands
      .sessionBus()
      .then((now) => {
        if (!gone && now) setNotice((was) => was ?? now);
      })
      .catch(() => undefined);
    const listening = listen<BusNotice>(SESSION_BUS_ANSWERS, (event) => {
      if (gone) return;
      setNotice(event.payload);
      setDismissed(false);
    }).catch(() => undefined);
    return () => {
      gone = true;
      void listening.then((stop) => stop?.()).catch(() => undefined);
    };
  }, []);

  // Live, so a chat that finishes its turn while the ask is up leaves it.
  const midTurn = chats.filter(mightBeMidTurn);
  const restart = () => void commands.restartOnTheSessionBus().catch(() => undefined);
  const toRestart = () => (midTurn.length === 0 ? restart() : setAsking(true));

  if (!notice || dismissed) return null;
  return (
    <>
      <p className="came-back trouble" role="status">
        {notice.says}
        {notice.can_restart && (
          <>
            {" "}
            The session bus answers now. A restart ends every chat, and the next launch offers them
            back.
            {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). */}
            <button type="button" className="offer" tabIndex={0} onClick={toRestart}>
              Restart with the full desktop integration
            </button>
          </>
        )}
        <button type="button" className="dismiss" tabIndex={0} onClick={() => setDismissed(true)}>
          Dismiss
        </button>
      </p>
      {asking && (
        <MidTurn
          title="Restart with the full desktop integration"
          chats={midTurn}
          onWait={() => setAsking(false)}
          onRestart={() => {
            setAsking(false);
            restart();
          }}
        />
      )}
    </>
  );
}
