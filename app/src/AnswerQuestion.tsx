import { useEffect, useId, useRef, useState } from "react";
import { commands, type PlaneId } from "./bindings";

/** What the form says when it is sent with nothing in it. The core says the same of blanks. */
export const EMPTY_ANSWER = "The answer is empty, so nothing was sent.";

/** What the form shows where the task has no question to answer any more. */
export function noQuestionSaid(task: string): string {
  return `${task} has no question waiting for an answer now.`;
}

/**
 * **The person answers a question a task put to its asking chat** (#1496, V100-46): a small
 * inline form with the question, a text area, Send and Cancel.
 *
 * The task is handed the answer marked as the person's and carries on; the asking chat is told
 * the person answered, and its own answer to that question is refused from then on.
 *
 * **Mounted wherever a task's question is shown**: on the question's line in a chat's Activity
 * tab, and by any other surface that names a task (its row in the Chats list, its tab's menu).
 * A surface that has the question's words passes them as `question`; one that does not leaves
 * it out, and the form asks the core which question the task is paused on.
 *
 * - **The question is drawn as text.** It is a chat's own words: a text node, with its line
 *   breaks, never read as Markdown or markup.
 * - **Enter sends, Shift+Enter starts a new line**, as in a chat's own pane. An Enter that only
 *   picks a candidate while composing a character is not a send.
 * - **Empty is refused**, in a sentence, and nothing is asked of the core.
 * - **What the person typed is never lost or cut.** A refusal from the core (the asking chat
 *   answered first, the task has moved on, the text is too long or holds a character purlis
 *   hands to no chat) is shown as the core says it, and the text stays in the box.
 * - **It answers the question it shows.** The words shown go to the core with the answer, and
 *   an answer to a question the task is no longer paused on is refused there, not delivered
 *   as the answer to another.
 */
export function AnswerQuestion({
  plane,
  session,
  task,
  question,
  onDone,
  onCancel,
}: {
  plane: PlaneId;
  /** The task's chat, by the app's number for it now. */
  session: number;
  /** The task, by the name the person sees it under. */
  task: string;
  /** The question as the surface shows it; left out, the core is asked for it. */
  question?: string;
  /** The answer was taken: the surface puts the form away. */
  onDone: () => void;
  /** The person put the form away without sending. */
  onCancel: () => void;
}) {
  /** The question the core says the task is paused on, where the surface gave none. */
  const [read, setRead] = useState<string | null>();
  /** The question to answer; `null` where the task asks none, nothing while it is read. */
  const asked = question ?? read;
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [refused, setRefused] = useState<string>();
  const box = useRef<HTMLTextAreaElement>(null);
  const ids = useId();

  useEffect(() => {
    if (question !== undefined) return;
    let left = false;
    void commands
      .taskQuestion(plane, session)
      .then((answer) => {
        if (left) return;
        if (answer.status === "error") {
          setRead(null);
          setRefused(answer.error);
        } else setRead(answer.data?.question ?? null);
      })
      .catch((err: unknown) => {
        if (left) return;
        setRead(null);
        setRefused(`purlis could not read the question: ${String(err)}`);
      });
    return () => {
      left = true;
    };
  }, [plane, session, question]);

  // The box is where the person types next, once there is a question to answer.
  const ready = typeof asked === "string";
  useEffect(() => {
    if (ready) box.current?.focus();
  }, [ready]);

  const send = async () => {
    if (sending || typeof asked !== "string") return;
    if (text.trim() === "") {
      setRefused(EMPTY_ANSWER);
      box.current?.focus();
      return;
    }
    setSending(true);
    setRefused(undefined);
    const answer = await commands
      .answerTaskQuestion(plane, session, asked, text)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    setSending(false);
    if (answer.status === "error") {
      // Said as the core says it, with what was typed still in the box.
      setRefused(answer.error);
      box.current?.focus();
      return;
    }
    onDone();
  };

  if (asked === undefined) {
    return (
      <p className="answer-question pending" aria-busy="true">
        Reading the question…
      </p>
    );
  }
  if (asked === null) {
    return (
      <div className="answer-question" data-testid="answer-question">
        <p className="answer-refused" role="alert">
          {refused ?? noQuestionSaid(task)}
        </p>
        <div className="answer-actions">
          <button type="button" className="panel-view" tabIndex={0} onClick={onCancel}>
            Close
          </button>
        </div>
      </div>
    );
  }
  return (
    <form
      className="answer-question"
      data-testid="answer-question"
      aria-label={`Answer ${task}'s question`}
      onSubmit={(event) => {
        event.preventDefault();
        void send();
      }}
    >
      <p className="answer-asked" id={`${ids}-asked`}>
        {asked}
      </p>
      <textarea
        ref={box}
        className="ui-field answer-text"
        aria-label="Your answer"
        aria-describedby={
          refused === undefined ? `${ids}-asked ${ids}-how` : `${ids}-asked ${ids}-refused`
        }
        aria-invalid={refused !== undefined}
        rows={Math.min(Math.max(2, text.split("\n").length), 10)}
        value={text}
        disabled={sending}
        onChange={(event) => {
          setText(event.currentTarget.value);
          setRefused(undefined);
        }}
        onKeyDown={(event) => {
          // An Enter that picks a candidate while composing a character is the input
          // method's, not a send.
          if (event.key !== "Enter" || event.nativeEvent.isComposing) return;
          if (event.shiftKey || event.altKey) return;
          event.preventDefault();
          void send();
        }}
      />
      {refused !== undefined && (
        <p className="answer-refused" id={`${ids}-refused`} role="alert">
          {refused}
        </p>
      )}
      <div className="answer-actions">
        <button type="submit" className="panel-view" tabIndex={0} disabled={sending}>
          Send
        </button>
        <button type="button" className="panel-view" tabIndex={0} onClick={onCancel}>
          Cancel
        </button>
        <span className="answer-how" id={`${ids}-how`}>
          {`${task} is told this is your answer. Enter sends, Shift+Enter starts a new line.`}
        </span>
      </div>
    </form>
  );
}
