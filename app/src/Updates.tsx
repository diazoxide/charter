import { useCallback, useEffect, useRef, useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as Dialog from "@radix-ui/react-dialog";
import * as RadioGroup from "@radix-ui/react-radio-group";
import { listen } from "./here";
import { ArrowUpCircle, LoaderCircle, Pin } from "lucide-react";
import { commands, type Offer, type PinReport, type PlaneId } from "./bindings";
import { EndingList, MidTurnSaid, mightBeMidTurn, type Ending } from "./QuitWarning";
import { ReleaseNotes } from "./ReleaseNotes";

/**
 * **"An update is available", and the pin that drifts** — the two version facts charter ADR
 * 0038 left with no surface, drawn on the status line.
 *
 * # The update offer
 *
 * The updater (#158, ADR 0042) checks on its own and installs only on a click, and its
 * author named this file's job: listen for `update://checked`, show the offer, call
 * `installUpdate()` from it, and show `updateChannel()` / `setUpdateChannel()` beside it.
 *
 * **What installing costs is said before the click, never after it.** Every session is a child
 * of this process (ADR 0025). On Windows the installer ends charter at once; on macOS and Linux
 * the new version is put in place and runs from the next start — and starting it again means
 * ending this one. Either way the running chats end, and the offer says so.
 *
 * **Once it is installed the line says Restart to update** (charter-app#251), and that is the
 * way to finish. The core writes every project's record, saying a restart to update wrote it,
 * ends the chats and restarts; the launch after it asks #250's question — reopen every session
 * or start fresh — with "charter restarted to install an update." in it and Reopen all in
 * front. **A chat that is mid-turn is named first**, in the words the quit warning uses, with
 * Wait as the answer a stray Return finds. Nothing here restarts charter behind the
 * operator's back.
 *
 * **A check that failed on its own is not drawn.** The timer runs every few hours whether or
 * not the laptop is on a train, and a line that turned amber every time Wi-Fi dropped would be
 * furniture by Friday. A failure is drawn only after the operator asked for something — a
 * check, an install, a channel change.
 *
 * # The pin
 *
 * Only when `charter version` says the plane's `[charter] version` is one this charter does not
 * meet — and that verdict is `adopt::version_report`'s exit status (`app/src-tauri/src/pin.rs`),
 * never a comparison made here (ADR 0030, as amended by ADR 0045). Its dialog carries `charter version`'s own
 * sentences.
 */

/** Where the updater is, as the window knows it. */
export type UpdateState =
  | { kind: "quiet" }
  | { kind: "offered"; offer: Offer }
  | { kind: "installing"; offer: Offer }
  /** `refused` is the core's sentence when a restart to update did not go ahead. */
  | { kind: "installed"; version: string; refused?: string }
  | { kind: "failed"; why: string };

/** What the window knows about updates, and what it can ask for. */
export type Updates = {
  state: UpdateState;
  /** The channel this machine takes charter from, once asked. */
  channel?: string;
  check: () => void;
  install: () => void;
  choose: (channel: string) => void;
  /** Restart into the installed update — once the operator has heard what is mid-turn. */
  restart: () => void;
};

/**
 * Stops one listener, and never lets that failure escape as an unhandled rejection.
 *
 * **`listen`'s unlisten function is `async`** (`@tauri-apps/api/event`'s `_unlisten`), so
 * calling it without awaiting hands back a promise nobody is holding — and it rejects whenever
 * `__TAURI_EVENT_PLUGIN_INTERNALS__` is not there: a webview being torn down, and every jsdom
 * test whose mocked IPC has no event plugin behind it. The `.catch` further down covers the
 * async block around the awaits; a bare call is outside it.
 *
 * It surfaced the day the updater moved to the window (`TitleBar.tsx`): mounted inside a
 * project it outlived the registration, mounted on the window it is torn down by any test that
 * renders and unmounts quickly — so `gone` was true before `listen` resolved, and the cleanup
 * fired into nothing. There is nothing to do about a listener that cannot be removed from a
 * page that is going away, and an unhandled rejection out of a cleanup function is a real
 * failure in a real window as well as a red suite.
 */
function stopListening(stop: () => void) {
  try {
    void Promise.resolve(stop()).catch(() => {});
  } catch {
    // A synchronous throw from the same cause, and the same answer.
  }
}

/**
 * The updater's events, turned into one state.
 *
 * `asked` is what separates a failure worth drawing from one that is not (see the module doc):
 * it is set by every gesture that asks the updater for something and cleared by the answer.
 *
 * **Called once, on the WINDOW** (`App.tsx`), and drawn once, on the title bar. It used to be
 * called in each `PlaneView`, which made a window holding eight projects hold eight clients
 * for one app-wide fact.
 */
export function useUpdates(): Updates {
  const [state, setState] = useState<UpdateState>({ kind: "quiet" });
  const [channel, setChannel] = useState<string>();
  const asked = useRef(false);

  useEffect(() => {
    let gone = false;
    const stops: (() => void)[] = [];
    void (async () => {
      const on = async <T,>(event: string, then: (payload: T) => void) => {
        const stop = await listen<T>(event, (e) => {
          if (!gone) then(e.payload);
        });
        if (gone) stopListening(stop);
        else stops.push(stop);
      };
      await on<Offer | null>("update://checked", (offer) => {
        asked.current = false;
        setState((was) =>
          // An install under way is not undone by a timer's check landing in the middle of it.
          was.kind === "installing" || was.kind === "installed"
            ? was
            : offer
              ? { kind: "offered", offer }
              : { kind: "quiet" },
        );
      });
      await on<string>("update://installed", (version) => {
        asked.current = false;
        setState({ kind: "installed", version });
      });
      await on<string>("update://failed", (why) => {
        const wasAsked = asked.current;
        asked.current = false;
        if (wasAsked) setState({ kind: "failed", why });
      });
    })().catch(() => {
      // A window that cannot listen draws the quiet updater — the icon, the channel, and
      // `Check now` — rather than taking the status line down. Seen on CI: a test whose mock
      // core throws for every command it does not name made `listen` reject, unhandled.
    });
    void commands
      .updateChannel()
      .then((now) => {
        if (!gone && typeof now === "string") setChannel(now);
      })
      .catch(() => {});
    return () => {
      gone = true;
      for (const stop of stops) stopListening(stop);
    };
  }, []);

  const check = useCallback(() => {
    asked.current = true;
    void commands.checkForUpdate();
  }, []);

  const install = useCallback(() => {
    setState((was) => (was.kind === "offered" ? { kind: "installing", offer: was.offer } : was));
    asked.current = true;
    void commands.installUpdate();
  }, []);

  const choose = useCallback((next: string) => {
    void commands.setUpdateChannel(next).then((done) => {
      if (done.status === "error") {
        setState({ kind: "failed", why: done.error });
        return;
      }
      setChannel(next);
      // An offer from the other channel's manifest is not an offer on this one.
      setState({ kind: "quiet" });
      asked.current = true;
      void commands.checkForUpdate();
    });
  }, []);

  const restart = useCallback(() => {
    void commands
      .restartToUpdate()
      .then((done) => (done.status === "error" ? done.error : undefined))
      // A command that never reached the core is a refusal too, and says so in the same place.
      .catch((why: unknown) => `charter could not ask to restart: ${String(why)}`)
      .then((refused) => {
        // On success nothing comes back: the process is on its way out.
        if (refused !== undefined)
          setState((was) => (was.kind === "installed" ? { ...was, refused } : was));
      });
  }, []);

  return { state, channel, check, install, choose, restart };
}

/** The words on the line for each state, or none — a quiet updater is an icon and no words. */
function said(state: UpdateState): string | undefined {
  switch (state.kind) {
    case "offered":
      return `${state.offer.version} available`;
    case "installing":
      return `installing ${state.offer.version}`;
    case "installed":
      return "Restart to update";
    case "failed":
      return "update failed";
    case "quiet":
      return undefined;
  }
}

/** The sentence that has to be read before Install is pressed. */
export const INSTALL_ENDS_SESSIONS =
  "Installing ends every running chat. On Windows charter closes at once to install; on macOS " +
  "and Linux the new version is put in place, and Restart to update ends every chat and offers " +
  "to reopen them all when charter starts again.";

/**
 * The status line's update button, and the dialog it opens.
 *
 * `chats` is every chat the window holds, each with its state, as the quit warning is given
 * them — the restart ends them all, so it asks about the ones that are mid-turn across every
 * project, not only the one in front.
 */
export function UpdateItem({
  updates,
  chats = [],
}: {
  updates: Updates;
  chats?: readonly Ending[];
}) {
  const [open, setOpen] = useState(false);
  /** Whether the dialog is asking about the chats a restart would interrupt. */
  const [asking, setAsking] = useState(false);
  const { state, channel, check, install, choose, restart } = updates;
  // Live, so a chat that finishes its turn while the ask is up leaves it.
  const midTurn = chats.filter(mightBeMidTurn);
  const words = said(state);
  const toRestart = () => {
    if (midTurn.length === 0) return restart();
    setOpen(false);
    setAsking(true);
  };
  const label = words
    ? `Updates: ${words}`
    : `Updates — ${channel ?? "…"} channel, nothing new known`;
  return (
    <>
      <Dialog.Root open={open} onOpenChange={setOpen}>
        <Dialog.Trigger asChild>
          <button
            type="button"
            className={`status-update update-${state.kind}`}
            // WebKit leaves a `<button>` out of the tab sequence unless its `tabindex` is written
            // down (`docs/ui-primitives.md`, charter-app#189).
            tabIndex={0}
            data-testid="status-update"
            aria-label={label}
            title={label}
          >
            {state.kind === "installing" ? (
              <LoaderCircle aria-hidden="true" className="spinning" />
            ) : (
              <ArrowUpCircle aria-hidden="true" />
            )}
            {words && <span> {words}</span>}
          </button>
        </Dialog.Trigger>
        <Dialog.Portal>
          <Dialog.Overlay className="asking" />
          <Dialog.Content className="warning update" aria-describedby="update-what">
            <Dialog.Title>Updates</Dialog.Title>
            <div id="update-what">
              {state.kind === "offered" || state.kind === "installing" ? (
                <>
                  <p>
                    charter <strong>{state.offer.version}</strong> is available on the{" "}
                    {state.offer.channel} channel. This is {state.offer.current}.
                  </p>
                  {/* A stable release's notes are its CHANGELOG.md section, in Markdown. */}
                  {state.offer.notes && (
                    <div className="update-notes">
                      <ReleaseNotes markdown={state.offer.notes} />
                    </div>
                  )}
                  <p className="honest mid-turn" role="alert" data-testid="update-ends-sessions">
                    {INSTALL_ENDS_SESSIONS}
                  </p>
                </>
              ) : state.kind === "installed" ? (
                <>
                  <p className="honest">
                    charter {state.version} is installed. Restart to update ends every chat, starts
                    charter {state.version}, and asks whether to reopen them — Reopen all puts each
                    one back, resuming its conversation where its harness can.
                  </p>
                  {state.refused && (
                    <p className="honest doctor-trouble" role="alert">
                      {state.refused}
                    </p>
                  )}
                </>
              ) : state.kind === "failed" ? (
                <p className="honest doctor-trouble" role="alert">
                  {state.why}
                </p>
              ) : (
                <p className="honest">
                  No newer charter is known. charter checks on its own every few hours.
                </p>
              )}
            </div>
            <h3 id="update-channel">Channel</h3>
            <RadioGroup.Root
              className="choices"
              aria-labelledby="update-channel"
              value={channel ?? ""}
              onValueChange={choose}
            >
              {["stable", "dev"].map((name) => (
                <div className="choice" key={name}>
                  <RadioGroup.Item className="dot" value={name} id={`update-channel-${name}`}>
                    <RadioGroup.Indicator className="dot-mark" />
                  </RadioGroup.Item>
                  <label className="who" htmlFor={`update-channel-${name}`}>
                    {name}
                  </label>
                </div>
              ))}
            </RadioGroup.Root>
            {/* `tabIndex={0}` on every one, per `docs/ui-primitives.md` (charter-app#186). The
                channel radios above are the scope's first edge and `Close` is its last, which
                left `Install`, `Restart to update` and `Check now` in the middle — where Radix's
                focus scope does nothing and WebKit will not tab to a `<button>` whose
                `tabindex` is not written down. Installing an update was a mouse-only act. */}
            <div className="answer">
              {state.kind === "offered" && (
                <button type="button" tabIndex={0} onClick={install}>
                  Install {state.offer.version}
                </button>
              )}
              {state.kind === "installed" && (
                <button type="button" tabIndex={0} onClick={toRestart}>
                  Restart to update
                </button>
              )}
              {state.kind !== "installing" && state.kind !== "installed" && (
                <button type="button" tabIndex={0} onClick={check}>
                  Check now
                </button>
              )}
              <Dialog.Close asChild>
                <button type="button" tabIndex={0}>
                  Close
                </button>
              </Dialog.Close>
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
      {asking && (
        <MidTurn
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

/**
 * What Restart to update asks when a chat could be mid-turn: restart now, or wait.
 *
 * The quit warning's rows and sentences (`QuitWarning.tsx`), because it is the same act for
 * those chats — they are ended — with one difference the words carry: the restart offers them
 * back. A chat that reports no state is asked about too, for the quit warning's reason: it could
 * be mid-turn and charter would never know.
 *
 * **Radix's `AlertDialog`**, per `docs/ui-primitives.md`: it arrives because of what the
 * operator pressed, a click outside answers nothing, and its `Cancel` — **Wait** — is first and
 * has the keyboard, so the answer a stray Return or Escape finds interrupts nothing. **Restart
 * now is a plain button, not the primitive's `Action`**, for `RelaunchAsk`'s reason: an `Action`
 * also closes the dialog, and closing is this dialog's Wait. The list is live: a chat that
 * finishes its turn while this is up leaves it.
 */
function MidTurn({
  chats,
  onWait,
  onRestart,
}: {
  chats: readonly Ending[];
  onWait: () => void;
  onRestart: () => void;
}) {
  return (
    <AlertDialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onWait();
      }}
    >
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="asking" />
        <AlertDialog.Content className="warning" aria-describedby="restart-mid-turn-said">
          <AlertDialog.Title>Restart to update</AlertDialog.Title>
          <EndingList chats={chats} />
          <AlertDialog.Description asChild>
            <div id="restart-mid-turn-said">
              <MidTurnSaid chats={chats} />
              <p className="honest">
                Every chat is offered back when charter starts again. Wait to let a turn finish, or
                restart now.
              </p>
            </div>
          </AlertDialog.Description>
          {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). */}
          <div className="answer">
            <AlertDialog.Cancel asChild>
              <button type="button" tabIndex={0}>
                Wait
              </button>
            </AlertDialog.Cancel>
            <button type="button" className="ends-it" tabIndex={0} onClick={onRestart}>
              Restart now
            </button>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}

/** The plane's pin report: once when the project opens, and again whenever it is opened. */
export function usePin(plane: PlaneId): { pin?: PinReport; again: () => void } {
  const [pin, setPin] = useState<PinReport>();
  const ask = useCallback(() => {
    void commands
      .planePin(plane)
      .then((answer) => {
        if (answer.status === "ok" && answer.data && typeof answer.data.drift === "boolean")
          setPin(answer.data);
      })
      .catch(() => {});
  }, [plane]);
  useEffect(() => ask(), [ask]);
  return { pin, again: ask };
}

/** The pin item: nothing unless `charter version` says the pin drifts. */
export function PinItem({ pin, again }: { pin?: PinReport; again: () => void }) {
  if (!pin?.drift) return null;
  const label = `The plane pins charter ${pin.pinned ?? "(unreadable)"}; this charter is ${pin.brought}`;
  return (
    <Dialog.Root onOpenChange={(now) => now && again()}>
      <Dialog.Trigger asChild>
        <button
          type="button"
          className="status-pin"
          // WebKit leaves a `<button>` out of the tab sequence unless its `tabindex` is written
          // down (`docs/ui-primitives.md`, charter-app#189).
          tabIndex={0}
          data-testid="status-pin"
          aria-label={label}
          title={label}
        >
          <Pin aria-hidden="true" /> pin {pin.pinned}
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content className="warning update" aria-describedby="pin-said">
          <Dialog.Title>The plane&apos;s pin</Dialog.Title>
          <div id="pin-said">
            {pin.said.map((line) => (
              <p key={line} className="honest">
                {line}
              </p>
            ))}
          </div>
          {/* `tabIndex={0}`, per `docs/ui-primitives.md` (charter-app#186): the one control
              this dialog has, and WebKit leaves a `<button>` out of the tab sequence unless
              its `tabindex` is written down. */}
          <div className="answer">
            <Dialog.Close asChild>
              <button type="button" tabIndex={0}>
                Close
              </button>
            </Dialog.Close>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
