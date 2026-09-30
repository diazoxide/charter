import { useEffect, useState } from "react";
import { OctagonX, Play } from "lucide-react";
import { commands } from "./bindings";
import { listen } from "./here";

/**
 * **The kill switch** (OV-1): stop every agent — every chat in every project, in every window
 * — and start none until the operator re-arms it.
 *
 * On the title bar because it is about the whole app and not one project, and because the
 * title bar is on screen in every window. **One press, with no question in between**: a switch
 * that asks first is not one an operator can reach for. Nothing is lost by a press made by
 * mistake — the chats stay as tabs, each reading as one whose program ended, and re-arming lets
 * the operator reopen them.
 *
 * `charter stop --all` in a terminal throws the same switch; the core hears it and every
 * window's bar is told (`kill-switch`), as it is after a press here. Re-arming is here and only
 * here: a command any agent could run cannot be the thing that lets agents start again.
 *
 * Asked once at the mount, so a window opened after a stop — or a launch that found the switch
 * thrown — says so from its first frame that can.
 */
export function KillSwitch() {
  const [stopped, setStopped] = useState(false);
  const [busy, setBusy] = useState(false);
  const [trouble, setTrouble] = useState<string>();

  useEffect(() => {
    let gone = false;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unlisten = await listen<boolean>("kill-switch", (event) => {
          if (!gone) setStopped(event.payload === true);
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in: a unit test, or a webview being torn down.
      }
    })();
    void commands
      .agentsStopped()
      .then((said) => {
        if (!gone && typeof said === "boolean") setStopped(said);
      })
      .catch(() => {});
    return () => {
      gone = true;
      stop?.();
    };
  }, []);

  const press = () => {
    setBusy(true);
    setTrouble(undefined);
    const asked = stopped ? commands.rearmAgents() : commands.stopEveryAgent();
    // The switch is thrown before the core answers — the answer comes once every program has
    // ended — so the bar says so at once rather than after that half second.
    if (!stopped) setStopped(true);
    void asked
      .then((said) => {
        if (said.status === "error") setTrouble(said.error);
        else if (stopped) setStopped(false);
      })
      .catch((why: unknown) => setTrouble(String(why)))
      .finally(() => setBusy(false));
  };

  const label = stopped
    ? "Every agent is stopped — re-arm to let chats start again"
    : "Stop every agent: every chat in every project and window";
  return (
    // `tabIndex={0}`, per `docs/ui-primitives.md` (charter-app#186); being a `<button>` is what
    // keeps it out of the title bar's drag region (`TitleBar.tsx`).
    <button
      type="button"
      tabIndex={0}
      className="title-kill-switch"
      data-testid="kill-switch"
      data-stopped={stopped ? "yes" : "no"}
      aria-label={trouble ? `${label}. ${trouble}` : label}
      title={trouble ?? label}
      disabled={busy}
      onClick={press}
    >
      {stopped ? (
        <>
          <Play aria-hidden="true" /> <span>Stopped · Re-arm</span>
        </>
      ) : (
        // An icon alone while nothing is stopped: the bar's right-hand end never gives way
        // (ADR 0054), and what it spends the project tabs lose in a 1024 px window. Its name is
        // its `aria-label` and its `title`. Once thrown it says so in words, because a stopped
        // app must never read as an idle one.
        <OctagonX aria-hidden="true" />
      )}
    </button>
  );
}
