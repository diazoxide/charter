import { useEffect, useId, useRef, useState } from "react";
import { commands, type PlaneId } from "./bindings";

/** What the form says when it is sent with nothing in it. The core says the same of blanks. */
export const EMPTY_ANSWER = "The answer is empty, so nothing was sent.";

/** What the form shows where the task has no question to answer any more. */
export function noQuestionSaid(task: string): string {
  return `${task} has no question waiting for an answer now.`;
}

/** What follows the reason a question closed while the person was typing an answer to it. */
export const TEXT_IS_KEPT = "What you typed is still in the box, to copy.";

/** The question a form answers: the number the core gave it, and its words. */
type Asked = { number: number; question: string };

/**
 * **The person answers a question a task put to its asking chat** (#1496, V100-46): a small
 * inline form with the question, a text area, Send and Cancel.
 *
 * The task is handed the answer marked as the person's and carries on; the asking chat is told
 * the person answered, and its own answer to that question is refused from then on.
 *
 * **Mounted wherever a task's question is shown**: on the question's line in a chat's Activity
 * tab, and by any other surface that names a task (its row in the Chats list, its tab's menu).
 * A surface that has the question passes its `number` and its words as `question`; one that
 * does not leaves both out, and the form asks the core which question the task is paused on.
 *
 * - **It answers one question, by its number.** The core numbers each question as it is asked.
 *   The number of the question shown goes to the core with the answer, and an answer for a
 *   question the task is no longer paused on is refused there, though the task has asked
 *   another in the very same words. It is never delivered as the answer to another.
 * - **The question is drawn as text.** It is a chat's own words: a text node, with its line
 *   breaks, never read as Markdown or markup.
 * - **Enter sends, Shift+Enter starts a new line**, as in a chat's own pane. An Enter that only
 *   picks a candidate while composing a character is not a send. **One send is final**: a
 *   question is answered once, and more is said in the task's own tab.
 * - **Empty is refused**, in a sentence, and nothing is asked of the core.
 * - **What the person typed is never lost or cut.** A refusal from the core (the asking chat
 *   answered first, the task has moved on, the text is too long or holds a character purlis
 *   hands to no chat) is shown as the core says it, and the text stays in the box.
 * - **It stays while they type, whatever happens to the question.** Where the question closes
 *   under them (`closed`: the asking chat answered first, or the task reported or ended), the
 *   form says why, keeps their text in the box where it can be selected and copied, and has
 *   nothing left to send.
 */
export function AnswerQuestion({
  plane,
  session,
  task,
  number,
  question,
  closed,
  onDone,
  onCancel,
}: {
  plane: PlaneId;
  /** The task's chat, by the app's number for it now. */
  session: number;
  /** The task, by the name the person sees it under. */
  task: string;
  /** The question's number, as the core gave it; left out with `question`, the core is asked. */
  number?: number;
  /** The question's words as the surface shows them. */
  question?: string;
  /** Why the question can no longer be answered, in one sentence, once that is so. */
  closed?: string;
  /** The answer was taken: the surface puts the form away. */
  onDone: () => void;
  /** The person put the form away without sending. */
  onCancel: () => void;
}) {
  const given: Asked | undefined =
    number !== undefined && question !== undefined ? { number, question } : undefined;
  /** The question the core says the task is paused on, where the surface gave none. */
  const [read, setRead] = useState<Asked | null>();
  /** The question to answer; `null` where the task asks none, nothing while it is read. */
  const asked = given ?? read;
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [refused, setRefused] = useState<string>();
  const box = useRef<HTMLTextAreaElement>(null);
  const ids = useId();

  const has = given !== undefined;
  useEffect(() => {
    if (has) return;
    let left = false;
    void commands
      .taskQuestion(plane, session)
      .then((answer) => {
        if (left) return;
        if (answer.status === "error") {
          setRead(null);
          setRefused(answer.error);
        } else if (answer.data === null) setRead(null);
        else setRead({ number: answer.data.number, question: answer.data.question });
      })
      .catch((err: unknown) => {
        if (left) return;
        setRead(null);
        setRefused(`purlis could not read the question: ${String(err)}`);
      });
    return () => {
      left = true;
    };
  }, [plane, session, has]);

  // The box is where the person types next, once there is a question to answer.
  const ready = asked !== undefined && asked !== null;
  useEffect(() => {
    if (ready) box.current?.focus();
  }, [ready]);

  /* Closed under the person: not while their own send is on its way, whose answer is what
     closes it. */
  const shut = closed !== undefined && !sending ? `${closed} ${TEXT_IS_KEPT}` : undefined;

  const send = async () => {
    if (sending || shut !== undefined || asked === undefined || asked === null) return;
    if (text.trim() === "") {
      setRefused(EMPTY_ANSWER);
      box.current?.focus();
      return;
    }
    setSending(true);
    setRefused(undefined);
    const answer = await commands
      .answerTaskQuestion(plane, session, asked.number, asked.question, text)
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
  /** What is wrong, where something is: that the question closed, else the last refusal. */
  const said = shut ?? refused;
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
        {asked.question}
      </p>
      <textarea
        ref={box}
        className="ui-field answer-text"
        aria-label="Your answer"
        aria-describedby={
          said === undefined ? `${ids}-asked ${ids}-how` : `${ids}-asked ${ids}-refused`
        }
        aria-invalid={refused !== undefined && shut === undefined}
        rows={Math.min(Math.max(2, text.split("\n").length), 10)}
        value={text}
        disabled={sending}
        // Closed: nothing more is typed, and what was typed can still be selected and copied.
        readOnly={shut !== undefined}
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
      {said !== undefined && (
        <p className="answer-refused" id={`${ids}-refused`} role="alert">
          {said}
        </p>
      )}
      <div className="answer-actions">
        <button
          type="submit"
          className="panel-view"
          tabIndex={0}
          disabled={sending || shut !== undefined}
        >
          Send
        </button>
        <button type="button" className="panel-view" tabIndex={0} onClick={onCancel}>
          {shut === undefined ? "Cancel" : "Close"}
        </button>
        {shut === undefined && (
          <span className="answer-how" id={`${ids}-how`}>
            {`${task} is told this is your answer. Enter sends, Shift+Enter starts a new line.`}
          </span>
        )}
      </div>
    </form>
  );
}
