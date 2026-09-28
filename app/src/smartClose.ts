/**
 * Which of a project's chats are **wrapping up** — being smart-closed (ADR 0064) — kept current
 * by being told, as `chatState.ts` is.
 *
 * The core holds the truth (`smartclose.rs`): it sends the prompt, hears the record, closes the
 * chat, gives up after five minutes, hears the chat end, or finds the prompt could not be sent.
 * The window is told each step on `smart-close` and draws the tab wrapping up while the step
 * says so. A step that ends it is handed to `onEnd`, which is where the window closes the tab or
 * says why it did not.
 */
import { useEffect, useRef, useState } from "react";
import { listen } from "./here";
import { commands, type Phase, type PlaneId, type SmartClosing } from "./bindings";

/** Whether a step leaves the chat still wrapping up. */
export function wrappingUp(phase: Phase): boolean {
  return phase === "queued" || phase === "sent";
}

/** The sentence the window says when a smart close ends without closing — or none, where the
 *  tab going back to normal says it all (a cancel is the operator's own act). */
export function saidWhenItEnds(phase: Phase, name: string): string | undefined {
  if (phase === "no_record")
    return `No session record arrived from ${name} within five minutes, so it was left open.`;
  if (phase === "ended") return `${name} ended before it wrote its session record.`;
  if (phase === "not_sent")
    return `Smart close could not send ${name} its prompt, so it was left open.`;
  return undefined;
}

/**
 * Why a smart close stopped without its record, as the title bar's needs-you list says it
 * beside the chat's name (SI-8f) — or none, for an end that needs nothing from the operator: the
 * record landed, or they cancelled it themselves.
 */
export function stoppedWhy(phase: Phase): string | undefined {
  if (phase === "no_record")
    return "smart close stopped — no session record arrived in five minutes";
  if (phase === "ended") return "smart close stopped — it ended before it wrote its record";
  if (phase === "not_sent") return "smart close stopped — its prompt could not be sent";
  return undefined;
}

/** What the needs-you list says of a Smart close the core refused to start. */
export const DID_NOT_START = "smart close did not start";

/** Folds one step into the chats wrapping up. */
export function stepped(was: ReadonlySet<number>, step: SmartClosing): ReadonlySet<number> {
  const now = new Set(was);
  if (wrappingUp(step.phase)) now.add(step.session);
  else now.delete(step.session);
  return now;
}

/**
 * The chats of `plane` wrapping up, and every step that ends one handed to `onEnd`.
 *
 * Listening starts at the mount and the snapshot (`smart_closing`) is asked after it, so a step
 * that lands in between is not lost — `chatState.ts`'s order, for its reason.
 */
export function useSmartClosing(
  plane: PlaneId,
  onEnd: (step: SmartClosing) => void,
): ReadonlySet<number> {
  // Keyed by the project it is about, as `chatState.ts` keys its states: every project numbers
  // its chats from one, so one project's chat 1 wrapping up is nothing to another's.
  const [known, setKnown] = useState<{ plane?: PlaneId; closing: ReadonlySet<number> }>(() => ({
    closing: new Set(),
  }));

  const told = useRef(onEnd);
  useEffect(() => {
    told.current = onEnd;
  }, [onEnd]);

  useEffect(() => {
    let gone = false;
    let stop: (() => void) | undefined;
    const setClosing = (fold: (was: ReadonlySet<number>) => ReadonlySet<number>) =>
      setKnown((was) => ({ plane, closing: fold(was.plane === plane ? was.closing : new Set()) }));
    void (async () => {
      try {
        const unlisten = await listen<SmartClosing>("smart-close", (event) => {
          const step = event.payload;
          if (gone || step.plane !== plane) return;
          setClosing((was) => stepped(was, step));
          if (!wrappingUp(step.phase)) told.current(step);
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in — a unit test, or a webview being torn down.
      }
      try {
        const now = await commands.smartClosing(plane);
        if (gone || now.status !== "ok" || !Array.isArray(now.data)) return;
        setClosing((was) => now.data.reduce(stepped, was));
      } catch {
        // Nothing to start from; the steps still arrive.
      }
    })();
    return () => {
      gone = true;
      stop?.();
    };
  }, [plane]);

  return known.plane === plane ? known.closing : NOTHING_CLOSING;
}

const NOTHING_CLOSING: ReadonlySet<number> = new Set();
