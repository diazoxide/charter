import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { createContext, Fragment, useContext, useEffect, useId, useRef } from "react";
import type { Offer, TaskEndWay } from "./actions";
import type { TaskEnding } from "./bindings";

/**
 * **Ending a task by hand** (#1488, V100-5, V100-18): the second step every ending takes, and
 * the two buttons a pane showing a task has for it.
 *
 * A person ends a task one of two ways. **Stop and get its report** ends its turn and gives it
 * one short turn to say what it did; **Close now** ends its program at once. The core does
 * both (`end_task`) and tells the chat that asked which, in its own words. Neither is the
 * tab's close: nothing here offers a Smart close, and the standard close dialog is never shown
 * for a task.
 *
 * **Ending a task always takes a second step, and never a modal dialog for an idle task.** A
 * press on a task's row, on Delete, or on the breadcrumb's two controls is answered where it
 * was made: "Stop it" or "Close it" beside "Keep", with the keyboard on Keep, so a stray press
 * or a stray Return ends nothing. **The one modal question** is for a task in the middle of a
 * turn and for one with tasks of its own still at work (asked once: end them too, or keep
 * them). Where purlis may not type into the task, either says why and offers Close alone.
 */

/** What the person is being asked about ending a task, in the modal question. */
export type TaskEndAsked = TaskEnding & {
  session: number;
  /** Whether the tasks at work below it are ended with it. */
  belowToo: boolean;
  busy: boolean;
  /** The core's refusal of the last answer. */
  trouble?: string;
};

/** Where a press to end a task was made: what decides where its second step is drawn. */
export type TaskEndFrom = "row" | "crumb" | "elsewhere";

/** The second step for an idle task, drawn where the press was made. */
export type TaskEndInline = {
  session: number;
  where: "row" | "crumb";
  /** The way the answer ends it: Close where a report cannot be asked for. */
  way: TaskEndWay;
  /** What is asked, said plainly. */
  says: string;
  /** The button that does it: "Stop it" or "Close it". */
  answer: string;
  busy: boolean;
  /** The core's refusal of the answer, said where it was given. */
  trouble?: string;
};

/**
 * Whether ending the task asks in the one modal question (V100-18): a task mid-turn does, and
 * so does one with tasks at work below it. Every other task is asked in place.
 */
export function asksInAModal(ending: TaskEnding): boolean {
  return ending.working || ending.below.length > 0;
}

/**
 * The second step for a task that is not mid-turn and has nothing at work below it: what is
 * asked and what the answer does. Stop where it can be given its turn to report; otherwise
 * Close, with why it cannot be asked where that is what was pressed.
 */
export function inPlace(
  ending: TaskEnding,
  way: TaskEndWay,
): Pick<TaskEndInline, "way" | "says" | "answer"> {
  const { name } = ending;
  if (way === "report" && ending.no_report === null)
    return { way: "report", says: `Stop ${name} and get its report?`, answer: "Stop it" };
  // It has reported: there is no report to ask for, and ending it tells nobody anything more.
  if (ending.reported)
    return { way: "now", says: `${name} has reported. Close it?`, answer: "Close it" };
  if (way === "report")
    return {
      way: "now",
      says: `${ending.no_report ?? ""} Close it now, with no report?`.trim(),
      answer: "Close it",
    };
  return { way: "now", says: `Close ${name} now, with no report?`, answer: "Close it" };
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
 * The one modal question (V100-18): `Stop task '<name>'? It is working.`, with Stop and get
 * its report, Close now and Cancel.
 *
 * **Stop and get its report is the default** and takes the keyboard, where it is offered: it
 * is the answer that loses least. Where purlis may not type into the task, or the task is
 * being stopped already and has its one short turn, it is not drawn, the question says why,
 * and Cancel takes the keyboard. Escape is Cancel, and a click outside answers nothing
 * (Radix's `AlertDialog`, for `ChatAsk`'s reasons). A refusal stays in the question, in the
 * core's words.
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
  const reports = asked.no_report === null && !asked.stopping;
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
              {asked.stopping ? (
                <p className="honest no-report">
                  It is being stopped already, and has one short turn to say what it did. Close now
                  ends it without waiting for that turn.
                </p>
              ) : (
                asked.no_report !== null && (
                  <p className="honest no-report">
                    {asked.no_report} Close now ends its program without a report.
                  </p>
                )
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
 * **The second step, where the press was made**: what is asked, the answer that does it, and
 * Keep. **Keep has the keyboard**, so Return and Space end nothing, and Escape is Keep. It is
 * a group named by what it asks, so a screen reader hears the question as the focus lands in
 * it. Not a dialog: nothing else in the window is taken away, and a press elsewhere leaves it
 * standing until it is answered.
 */
export function TaskEndConfirm({
  says,
  answer,
  busy,
  trouble,
  onAnswer,
  onKeep,
}: {
  says: string;
  answer: string;
  busy: boolean;
  /** The core's refusal of the answer: it stays here, and the task is as it was. */
  trouble?: string;
  onAnswer: () => void;
  onKeep: () => void;
}) {
  const keep = useRef<HTMLButtonElement>(null);
  const group = useRef<HTMLSpanElement>(null);
  const asked = useId();
  // **Keep takes the keyboard as the question is drawn, and keeps it**: a menu that was just
  // closed gives the focus back to the row it was opened on a moment later, which would
  // leave the question standing with the keyboard elsewhere. Taken again once, shortly
  // after, and only if it is not in the question by then.
  useEffect(() => {
    keep.current?.focus();
    const again = window.setTimeout(() => {
      if (group.current?.contains(document.activeElement) !== true) keep.current?.focus();
    }, 60);
    return () => window.clearTimeout(again);
  }, []);
  return (
    <span
      ref={group}
      className="task-end-confirm"
      role="group"
      aria-labelledby={asked}
      onKeyDown={(event) => {
        if (event.key !== "Escape") return;
        event.preventDefault();
        event.stopPropagation();
        onKeep();
      }}
    >
      <span className="task-end-says" id={asked}>
        {says}
      </span>
      <button type="button" className="ends-it" tabIndex={0} disabled={busy} onClick={onAnswer}>
        {answer}
      </button>
      <button type="button" ref={keep} tabIndex={0} disabled={busy} onClick={onKeep}>
        Keep
      </button>
      {trouble && (
        <span className="trouble" role="alert">
          {trouble}
        </span>
      )}
    </span>
  );
}

/** What the breadcrumb's two controls need of the window: the press, and its second step. */
export type TaskEndHand = {
  /** The second step being asked, where one is. */
  confirming?: TaskEndInline;
  /** A way was pressed on the breadcrumb of the pane showing task `session`. */
  onEnd: (session: number, way: TaskEndWay) => void;
  /** The second step was answered: carry it out, or keep the task. */
  onConfirm: (yes: boolean) => void;
};

/** The window's hand to every breadcrumb's controls, so no pane is threaded it by hand. */
export const TaskEndContext = createContext<TaskEndHand | undefined>(undefined);

/** What the breadcrumb's two buttons say: the first words of each row's title. */
const WORDS: Record<TaskEndWay, string> = { report: "Stop", now: "Close now" };

/**
 * **The two ways to end the task a pane shows**, on its breadcrumb's line (#1488): the only
 * place a pane showing a task has an ending control. Two words and no mark, so neither can be
 * taken for the tab's close, which is a cross on the strip. Each is a row of the catalogue
 * (`taskEndRows`), named by its title, which begins with the words drawn, so the palette, a
 * row's menu, a voice command and these say one thing. A row that cannot run is drawn disabled
 * and says why, as a description and as its tooltip.
 *
 * **A press ends nothing.** It is answered here, in the buttons' place, by the second step
 * (`TaskEndConfirm`); a task mid-turn is asked in the one modal question instead.
 */
export function TaskEnds({
  session,
  stop,
  close,
  focusBack,
}: {
  /** The task the pane shows. */
  session: number;
  stop: Offer | undefined;
  close: Offer | undefined;
  /** Puts the keyboard back in the pane once the second step is answered with Keep. */
  focusBack?: () => void;
}) {
  const hand = useContext(TaskEndContext);
  const why = useId();
  if (hand === undefined || (stop === undefined && close === undefined)) return null;
  const asked = hand.confirming;
  if (asked !== undefined && asked.where === "crumb" && asked.session === session)
    return (
      <span className="pane-task-ends asking">
        <TaskEndConfirm
          says={asked.says}
          answer={asked.answer}
          busy={asked.busy}
          trouble={asked.trouble}
          onAnswer={() => hand.onConfirm(true)}
          onKeep={() => {
            hand.onConfirm(false);
            focusBack?.();
          }}
        />
      </span>
    );
  const drawn: [Offer | undefined, TaskEndWay][] = [
    [stop, "report"],
    [close, "now"],
  ];
  return (
    <span className="pane-task-ends" role="group" aria-label="End this task">
      {drawn.map(([offer, way]) =>
        offer === undefined ? null : (
          <Fragment key={offer.id}>
            <button
              type="button"
              className="task-end"
              // In the tab sequence, said out loud (`docs/ui-primitives.md`).
              tabIndex={0}
              aria-label={offer.title}
              aria-disabled={!offer.available || undefined}
              aria-describedby={offer.available ? undefined : `${why}-${way}`}
              title={offer.reason || (offer.note ? `${offer.title}. ${offer.note}` : offer.title)}
              onClick={() => {
                if (offer.available) hand.onEnd(session, way);
              }}
            >
              {WORDS[way]}
            </button>
            {/* Why it cannot run, for whoever cannot see a tooltip: the button's description. */}
            {!offer.available && (
              <span hidden id={`${why}-${way}`}>
                {offer.reason}
              </span>
            )}
          </Fragment>
        ),
      )}
    </span>
  );
}
