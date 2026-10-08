import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { useRef } from "react";
import type { Offer, TaskEndWay } from "./actions";
import type { TaskEnding } from "./bindings";

/**
 * **Ending a task by hand** (#1488, V100-5, V100-18): the one question it asks, and the two
 * buttons a pane showing a task has for it.
 *
 * A person ends a task one of two ways. **Stop and get its report** ends its turn and gives it
 * one short turn to say what it did; **Close now** ends its program at once. The core does
 * both (`end_task`) and tells the chat that asked which, in its own words. Neither is the
 * tab's close: nothing here offers a Smart close, and the standard close dialog is never shown
 * for a task.
 *
 * **Nothing is asked of an idle or finished task.** The question is for a task in the middle
 * of a turn, for one with tasks of its own still at work (asked once: stop them too, or keep
 * them), and for one purlis may not type into, where it says why and offers Close now alone.
 */

/** What the person is being asked about ending a task. */
export type TaskEndAsked = TaskEnding & {
  session: number;
  /** Whether the tasks at work below it are ended with it. */
  belowToo: boolean;
  busy: boolean;
  /** The core's refusal of the last answer. */
  trouble?: string;
};

/**
 * Whether ending the task asks the person anything first, or is done as pressed (V100-18): a
 * task mid-turn asks, and so does one with tasks at work below it. One that cannot be given
 * its turn to report says so where that was what was pressed, unless it has reported already,
 * in which case there is no report to ask for and nothing to say.
 */
export function asksFirst(ending: TaskEnding, way: TaskEndWay): boolean {
  if (ending.working || ending.below.length > 0) return true;
  return way === "report" && ending.no_report !== null && !ending.reported;
}

/** The question's title: V100-18's own words. */
export function taskEndTitle(name: string): string {
  return `Stop task '${name}'?`;
}

/** What the tasks at work below it are said as. */
export function belowSaid(below: readonly string[]): string {
  const names = below.join(", ");
  return below.length === 1
    ? `1 task it asked for is still working: ${names}.`
    : `${below.length} tasks it asked for are still working: ${names}.`;
}

/**
 * The one question (V100-18): `Stop task '<name>'? It is working.`, with Stop and get its
 * report, Close now and Cancel.
 *
 * **Stop and get its report is the default** and takes the keyboard, where it is offered: it
 * is the answer that loses least. Where purlis may not type into the task it is not drawn, the
 * question says why, and Cancel takes the keyboard. Escape is Cancel, and a click outside
 * answers nothing (Radix's `AlertDialog`, for `ChatAsk`'s reasons). A refusal stays in the
 * question, in the core's words.
 */
export function TaskEndAsk({
  asked,
  onBelow,
  onAnswer,
  onCancel,
}: {
  asked: TaskEndAsked;
  /** The person chose what becomes of the tasks at work below it. */
  onBelow: (too: boolean) => void;
  onAnswer: (way: TaskEndWay) => void;
  onCancel: () => void;
}) {
  const stop = useRef<HTMLButtonElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const reports = asked.no_report === null;
  return (
    <AlertDialog.Root
      open
      onOpenChange={(open) => {
        if (!open && !asked.busy) onCancel();
      }}
    >
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="asking" />
        <AlertDialog.Content
          className="warning task-end"
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            (reports ? stop : cancel).current?.focus();
          }}
        >
          <AlertDialog.Title>{taskEndTitle(asked.name)}</AlertDialog.Title>
          <AlertDialog.Description asChild>
            <div>
              {asked.working && <p className="honest">It is working.</p>}
              {!reports && (
                <p className="honest no-report">
                  {asked.no_report} Close now ends its program without a report.
                </p>
              )}
              {asked.below.length > 0 && (
                <fieldset className="task-end-below">
                  <legend>{belowSaid(asked.below)}</legend>
                  <label>
                    <input
                      type="radio"
                      name="task-end-below"
                      // #190: WebKit leaves a control out of the tab sequence without it.
                      tabIndex={0}
                      checked={asked.belowToo}
                      disabled={asked.busy}
                      onChange={() => onBelow(true)}
                    />
                    End them too, the same way. Each is told of in its own report.
                  </label>
                  <label>
                    <input
                      type="radio"
                      name="task-end-below"
                      tabIndex={0}
                      checked={!asked.belowToo}
                      disabled={asked.busy}
                      onChange={() => onBelow(false)}
                    />
                    Keep them working. They finish with nobody to report to, and stay in the Chats
                    list marked as from {asked.name}.
                  </label>
                </fieldset>
              )}
            </div>
          </AlertDialog.Description>
          {asked.trouble && (
            <p className="trouble" role="alert">
              {asked.trouble}
            </p>
          )}
          <div className="answer">
            <AlertDialog.Cancel asChild>
              <button type="button" ref={cancel} tabIndex={0} disabled={asked.busy}>
                Cancel
              </button>
            </AlertDialog.Cancel>
            <button
              type="button"
              className="ends-it"
              tabIndex={0}
              disabled={asked.busy}
              onClick={() => onAnswer("now")}
            >
              Close now
            </button>
            {reports && (
              <button
                type="button"
                ref={stop}
                className="default-answer"
                tabIndex={0}
                disabled={asked.busy}
                onClick={() => onAnswer("report")}
              >
                Stop and get its report
              </button>
            )}
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}

/**
 * **The two ways to end the task a pane shows**, on its breadcrumb's line (#1488): the only
 * place a pane showing a task has an ending control. Two words and no mark, so neither can be
 * taken for the tab's close, which is a cross on the strip and closes the session. Each is a
 * row of the catalogue (`taskEndRows`), named by its title, so the palette, a row's menu and
 * these say one thing; a row that cannot run is drawn disabled with its reason.
 */
export function TaskEnds({
  stop,
  close,
  onPress,
}: {
  stop: Offer | undefined;
  close: Offer | undefined;
  onPress: (offer: Offer) => void;
}) {
  if (stop === undefined && close === undefined) return null;
  const drawn: [Offer | undefined, string][] = [
    [stop, "Stop"],
    [close, "Close now"],
  ];
  return (
    <span className="pane-task-ends" role="group" aria-label="End this task">
      {drawn.map(([offer, words]) =>
        offer === undefined ? null : (
          <button
            key={offer.id}
            type="button"
            className="task-end"
            // In the tab sequence, said out loud (`docs/ui-primitives.md`).
            tabIndex={0}
            aria-label={offer.title}
            aria-disabled={!offer.available || undefined}
            title={offer.reason || (offer.note ? `${offer.title}. ${offer.note}` : offer.title)}
            onClick={() => {
              if (offer.available) onPress(offer);
            }}
          >
            {words}
          </button>
        ),
      )}
    </span>
  );
}
