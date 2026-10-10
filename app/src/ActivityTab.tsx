import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Activity as ActivityMark, LoaderCircle, TriangleAlert } from "lucide-react";
import { AnswerQuestion } from "./AnswerQuestion";
import { EmptyState } from "./EmptyState";
import { Notice } from "./Notice";
import {
  commands,
  type Activity,
  type ActivityHeard,
  type ActivityLine,
  type PlaneId,
} from "./bindings";
import {
  ACTIVITY_HEARD,
  alsoSaid,
  answerable,
  askAgain,
  chatClosedSaid,
  chatsOn,
  clipped,
  closedWhy,
  EXPIRED_SAID,
  heard,
  LEFT_OUT_SAID,
  leftOutAsked,
  lineKey,
  namedByOthers,
  timelineOf,
  unkeptSaid,
  type Timeline,
} from "./activity";
import { saidAt } from "./dispatches";
import { usePretendActivity } from "./e2eActivity";
import { useReferenceChats } from "./references";
import { useTasksBelow } from "./TasksBelow";

/** A count as the tab says it: `2,000`. */
const count = (n: number) => n.toLocaleString("en-US");

/** `was` with what the core said of `key`'s chat: the same map where it says that already. */
function toldOf(
  was: ReadonlyMap<string, number | null>,
  key: string,
  now: number | null,
): ReadonlyMap<string, number | null> {
  return was.has(key) && was.get(key) === now ? was : new Map(was).set(key, now);
}

/** What the tab has read: the chat's name, the timeline, and the tasks it could not list. */
type Read = {
  name: string;
  timeline: Timeline;
  undrawn: number;
  /** This chat's oldest tasks past what a timeline lists, and the project's oldest records
   *  past what it reads (#1520). */
  unlisted: number;
  unread: number;
  /** The bounds the core read within: what the tab says of them. */
  mostListed: number;
  mostRead: number;
};

/** What the tab has read, from the core's answer, with the lines heard while it was asked. */
function readOf(answer: Activity, early: readonly ActivityLine[] = []): Read {
  return {
    name: answer.name,
    timeline: early.reduce(heard, timelineOf(answer)),
    undrawn: answer.undrawn,
    unlisted: answer.unlisted,
    unread: answer.unread,
    mostListed: answer.most_listed,
    mostRead: answer.most_read,
  };
}

/** What the tab knows: nothing yet, the timeline, that its chat is not open, or why not. */
type Said = { read?: Read; closed?: true; trouble?: string };

/**
 * **One chat's Activity, in a tab of its own** (#1495, V100-44): what it and its tasks said to
 * each other, and the tasks of its tasks, as one timeline, oldest first. Each line has its day
 * and time (UTC, as the Dispatches tab says them), what kind of line it is, who said it to whom,
 * and what was said.
 *
 * **Read-only, but for one thing: a question a task is waiting on can be answered here**
 * (#1496, V100-46). A question its task is still paused on has an Answer control, which opens
 * a small form under the line ({@link AnswerQuestion}). The task is handed the answer as the
 * person's, and the asking chat is told. A question that has been answered, by that chat or by
 * the person, and one whose task has reported or ended, offers none. **A form that is open
 * stays open**: where its question closes while the person types, it says why and keeps what
 * they typed.
 *
 * **Who said a line is who said it.** A task the person dispatched from a chat's tab is "you,
 * from" that chat (V100-70), and an answer the person gave is "you" (#1496). An ending purlis
 * recorded in a task's place (stopped, ended without a report) is purlis's, for that task.
 * Those words are the app's to say: the core hands a chat's name over with "(a chat)" after it
 * where the name reads like one of them, so a name cannot pass for the mark.
 *
 * **A line opens the chat it came from**: the chat's name is the control. The press asks the core
 * which session that chat has now, so it reaches a chat that was restarted since the timeline
 * was read; a chat that has closed by then is named in plain text from there on, with "(chat
 * closed)" after it.
 *
 * **The control follows the chats while the tab is open** (#1457). The window's own chats (the
 * ones its tabs hold, and the session's open tasks) are watched; where a chat on the timeline
 * leaves them, the core is asked which session it has now, so a chat that closed loses its
 * control and its Answer, and one that was restarted keeps them. Nothing is asked while no chat
 * comes or goes.
 *
 * **What a chat said is drawn as text.** A brief, a message and a report are a chat's own words:
 * they are put on the screen as characters, with their line breaks, and never read as Markdown
 * or markup. A long one is clipped until it is asked for in full.
 *
 * **What is not there is said where it would have stood.** A task's record keeps its first
 * messages; one line, in the task's place, says how many it did not keep and why. A message
 * whose words were kept for 30 days after its task ended keeps its line and says so, and so does
 * one whose text read like a credential, which purlis never kept (#1520).
 *
 * **A file another task's report also names is marked** (V100-68) on each of the two reports,
 * for tasks that worked in the same folder at the same time. It is what the reports say, and no
 * more.
 *
 * **It follows the work while it is open.** The timeline is read once. Every line the app
 * records after that arrives as an event and is added, so nothing is read again for it.
 * Listening starts before the read, and a line that arrives in between is folded in after it.
 */
export function ActivityTab({
  plane,
  session,
  onShowChat,
}: {
  plane: PlaneId;
  /** The chat it is about, by the app's number for it. */
  session: number | undefined;
  /** Bring the chat in `session` to the front. */
  onShowChat: (session: number) => void;
}) {
  const [said, setSaid] = useState<Said>();
  const [again, setAgain] = useState(0);
  /** The lines shown in full, by {@link lineKey}. */
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  /** What the core said, since the timeline was read, of the session a chat has now, by its
   *  key: asked by a press, or as the window's chats changed. `null`: it is closed. A line told
   *  after that says it again, and takes its chat out of here. */
  const [told, setTold] = useState<ReadonlyMap<string, number | null>>(new Map());
  /** The question being answered: its line, and the session its task has now. */
  const [answering, setAnswering] = useState<{ key: string; session: number }>();
  /** The questions answered from this tab, by {@link lineKey}: each offers Answer no more,
   *  whether or not the record kept the answer's words for a line to say so. */
  const [answered, setAnswered] = useState<ReadonlySet<string>>(new Set());
  /** Each line's controls, by {@link lineKey}, while they are drawn: Answer, and its chat. */
  const answerControls = useRef(new Map<string, HTMLButtonElement>());
  const chatControls = useRef(new Map<string, HTMLButtonElement>());
  /** The timeline itself: where the keyboard goes when a line has no control left. */
  const list = useRef<HTMLOListElement>(null);
  /** The question whose form was just put away: where the keyboard goes back to. Its Answer
   *  control where that is still drawn (the form was cancelled), else its line's chat, else,
   *  where that chat has closed, the timeline. */
  const backTo = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (answering !== undefined || backTo.current === undefined) return;
    const key = backTo.current;
    backTo.current = undefined;
    (answerControls.current.get(key) ?? chatControls.current.get(key) ?? list.current)?.focus();
  }, [answering]);
  /** The questions whose chat a press could not find, by {@link lineKey}, and why. */
  const [unfound, setUnfound] = useState<ReadonlyMap<string, string>>(new Map());

  useEffect(() => {
    if (session === undefined) return;
    let left = false;
    let stop: (() => void) | undefined;
    /** Lines heard before the first read answered. */
    let early: ActivityLine[] | undefined = [];
    void (async () => {
      try {
        const unlisten = await listen<ActivityHeard>(ACTIVITY_HEARD, (event) => {
          if (left || event.payload.plane !== plane) return;
          const line = event.payload.line;
          if (early !== undefined) {
            early.push(line);
            return;
          }
          setSaid((was) =>
            was?.read === undefined
              ? was
              : { read: { ...was.read, timeline: heard(was.read.timeline, line) } },
          );
          setTold((was) => {
            if (!was.has(line.from_key)) return was;
            const less = new Map(was);
            less.delete(line.from_key);
            return less;
          });
        });
        if (left) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in: a unit test, or a webview being torn down.
      }
      const answer = await commands
        .activity(plane, session)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (left) return;
      if (answer.status === "error") {
        setSaid({ trouble: answer.error });
        return;
      }
      if (answer.data === null) {
        setSaid({ closed: true });
        return;
      }
      const read = readOf(answer.data, early);
      early = undefined;
      setSaid({ read });
    })();
    return () => {
      left = true;
      stop?.();
    };
  }, [plane, session, again]);

  /* A timeline a scenario spec hands the tab (`e2eActivity.ts`): only in the e2e build. */
  usePretendActivity((read) => setSaid({ read: readOf(read) }));

  const lines = said?.read?.timeline.lines;
  const also = useMemo(() => namedByOthers(lines ?? []), [lines]);
  const toAnswer = useMemo(() => answerable(lines ?? []), [lines]);
  /** The session each chat on the timeline has now, by key: its lines, under what was told. */
  const chats = useMemo(() => {
    const now = new Map(chatsOn(lines ?? []));
    for (const [key, session] of told) if (now.has(key)) now.set(key, session);
    return now;
  }, [lines, told]);
  const closed = (key: string) => chats.get(key) === null;

  /* The window's open chats, as its own stores hold them: the chats its tabs show, and this
     session's open tasks at any depth (#1457). Only what changes them is followed. */
  const lent = useReferenceChats();
  const below = useTasksBelow(session);
  const openNow = useMemo(
    () =>
      lent === undefined
        ? undefined
        : [
            ...new Set([
              ...lent.chats.map((one) => one.session),
              ...below.open.map((one) => one.session),
            ]),
          ]
            .sort((a, b) => a - b)
            .join(","),
    [lent, below],
  );
  const openBefore = useRef<string | undefined>(undefined);
  const chatsNow = useRef(chats);
  useEffect(() => {
    chatsNow.current = chats;
  }, [chats]);
  useEffect(() => {
    const before = openBefore.current;
    openBefore.current = openNow;
    if (before === undefined || openNow === undefined || before === openNow) return;
    const set = (said: string) => new Set(said === "" ? [] : said.split(",").map(Number));
    let left = false;
    for (const key of askAgain(chatsNow.current, set(before), set(openNow))) {
      void commands
        .activityChat(plane, key)
        .then((now) => {
          if (!left && now.status === "ok") setTold((was) => toldOf(was, key, now.data));
        })
        .catch(() => undefined);
    }
    return () => {
      left = true;
    };
  }, [openNow, plane]);

  /** Answer was pressed on a question: the session its task has now is asked for. */
  const answer = async (line: ActivityLine) => {
    const key = lineKey(line);
    const now = await commands
      .activityChat(plane, line.from_key)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (now.status === "error") {
      // Said on the line, and Answer stays to be pressed again.
      setUnfound((was) => new Map(was).set(key, now.error));
      return;
    }
    setUnfound((was) => {
      if (!was.has(key)) return was;
      const less = new Map(was);
      less.delete(key);
      return less;
    });
    setTold((was) => toldOf(was, line.from_key, now.data));
    if (now.data !== null) setAnswering({ key, session: now.data });
  };

  /** A line's chat was pressed: the session it has now is asked for, and shown. */
  const show = async (line: ActivityLine) => {
    const now = await commands
      .activityChat(plane, line.from_key)
      .catch(() => ({ status: "error" as const, error: "" }));
    if (now.status !== "ok") return;
    setTold((was) => toldOf(was, line.from_key, now.data));
    if (now.data !== null) onShowChat(now.data);
  };

  if (session === undefined) {
    return (
      <EmptyState
        mark={ActivityMark}
        headline="This tab names no chat"
        body="Open a chat's Activity from its tab's menu."
        testid="activity-no-chat"
      />
    );
  }
  if (said === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading the activity…
      </p>
    );
  }
  if (said.closed) {
    // Not a trouble with a way to try again: reading again would say the same.
    return (
      <EmptyState
        mark={ActivityMark}
        headline="This chat is not open"
        body="A chat's activity is listed while the chat is open."
        testid="activity-closed"
      />
    );
  }
  if (said.read === undefined) {
    return (
      <Notice
        cause="activity-unread"
        tone="trouble"
        fixes={[{ label: "Read again", onPress: () => setAgain((was) => was + 1) }]}
      >
        {`purlis could not read this chat's activity: ${said.trouble ?? "nothing was answered"}`}
      </Notice>
    );
  }
  const { name, timeline, undrawn, unlisted, unread, mostListed, mostRead } = said.read;
  /* What a long-lived chat's timeline leaves out, said above the oldest line shown: the oldest
     tasks past what one lists, and the project's oldest records past what one reads (#1520). */
  const older = (unlisted > 0 || unread > 0) && (
    <p className="none" data-testid="activity-older">
      {[
        unlisted > 0 &&
          (unlisted === 1
            ? `The oldest task ${name} dispatched is not listed, nor the tasks under it: a chat's activity lists the newest ${count(mostListed)} it dispatched.`
            : `The ${unlisted} oldest tasks ${name} dispatched are not listed, nor the tasks under them: a chat's activity lists the newest ${count(mostListed)} it dispatched.`),
        unread > 0 &&
          `purlis read this project's newest ${count(mostRead)} dispatch records, so a task older than those is not listed.`,
      ]
        .filter(Boolean)
        .join(" ")}
    </p>
  );
  /* Counted and never shown, as the Dispatches tab says it: this chat's tasks, and the tasks
     under them. */
  const refused = undrawn > 0 && (
    <p className="none" data-testid="activity-undrawn">
      {undrawn === 1
        ? "1 task is not listed: its record holds text purlis refuses to put on the screen."
        : `${undrawn} tasks are not listed: their records hold text purlis refuses to put on the screen.`}
    </p>
  );
  if (timeline.lines.length === 0) {
    return (
      <>
        <EmptyState
          mark={ActivityMark}
          headline="No activity yet"
          body={`When ${name} dispatches a task, what the two say to each other is listed here.`}
          testid="activity-empty"
        />
        {older}
        {refused}
      </>
    );
  }
  return (
    <>
      {older}
      <ol ref={list} className="activity" aria-label={`Activity of ${name}`} tabIndex={-1}>
        {timeline.lines.map(({ line, depth }) => {
          const key = lineKey(line);
          const full = open.has(key);
          const { shown, more } = clipped(line.text);
          const chat = closed(line.from_key) ? (
            <>
              <span className="activity-gone" title="Its chat is closed">
                {line.from}
              </span>
              <span className="activity-absent"> (chat closed)</span>
            </>
          ) : (
            <button
              type="button"
              className="vault-secret"
              // #190: WebKit leaves a control out of the tab sequence without `tabIndex`.
              tabIndex={0}
              ref={(node) => {
                if (node === null) chatControls.current.delete(key);
                else chatControls.current.set(key, node);
              }}
              aria-label={`Show chat ${line.from}`}
              title="Show its chat"
              onClick={() => void show(line)}
            >
              {line.from}
            </button>
          );
          return (
            <li
              key={key}
              className="activity-line"
              data-testid={`activity-${key}`}
              data-kind={line.kind}
              data-depth={depth}
              // Each level under the session is set in by one step, up to the deepest a chain goes.
              style={{ paddingInlineStart: `${0.5 + Math.min(depth - 1, 7) * 1.25}rem` }}
            >
              <time className="activity-at" dateTime={line.at} title={line.at}>
                {saidAt(line.at)}
              </time>
              <span className="activity-kind">
                {line.outcome === null || line.outcome === line.kind
                  ? line.kind
                  : `${line.kind} · ${line.outcome}`}
              </span>
              <span className="activity-who">
                {/* purlis's own line, for a task; the person's own brief, from a chat's tab;
                    the person's own answer, in that chat's place; else the chat's. */}
                {line.by_person && line.kind === "answer" ? (
                  <strong className="activity-you" title="You answered this in the purlis window">
                    you
                  </strong>
                ) : (
                  <>
                    {line.by_purlis ? "purlis, for " : line.by_person ? "you, from " : ""}
                    {chat}
                  </>
                )}
                {` → ${line.to}`}
              </span>
              {line.unkept !== null ? (
                <p className="activity-text activity-absent" data-testid="activity-unkept">
                  {unkeptSaid(line)}
                </p>
              ) : line.left_out ? (
                <p className="activity-text activity-absent" data-testid="activity-left-out">
                  {LEFT_OUT_SAID}
                  {line.kind === "question" && ` ${leftOutAsked(line)}`}
                </p>
              ) : line.expired ? (
                <p className="activity-text activity-absent" data-testid="activity-expired">
                  {EXPIRED_SAID}
                </p>
              ) : (
                /* A chat's own words: a text node, with its line breaks kept by the style. */
                <p className="activity-text">{full ? line.text : shown}</p>
              )}
              {more && (
                <button
                  type="button"
                  className="activity-more"
                  tabIndex={0}
                  aria-expanded={full}
                  onClick={() =>
                    setOpen((was) => {
                      const now = new Set(was);
                      if (!now.delete(key)) now.add(key);
                      return now;
                    })
                  }
                >
                  {full ? "Show less" : "Show all"}
                </button>
              )}
              {/* An answer the person gave that the task never had (#1496): it ended first. */}
              {line.unread && (
                <p className="activity-text activity-absent" data-testid="activity-unread">
                  {`${line.to} ended before it was handed this answer.`}
                </p>
              )}
              {/* The form (#1496), **for as long as it is open**: where its question closes
                  while the person types, it says why and keeps their text. */}
              {answering?.key === key ? (
                <AnswerQuestion
                  plane={plane}
                  session={answering.session}
                  task={line.from}
                  number={line.asks ?? 0}
                  question={line.text}
                  closed={
                    closed(line.from_key)
                      ? chatClosedSaid(line)
                      : toAnswer.has(key)
                        ? undefined
                        : closedWhy(line, lines ?? [])
                  }
                  onDone={() => {
                    // Answer is gone from the line, so the keyboard goes to its chat.
                    backTo.current = key;
                    setAnswered((was) => new Set(was).add(key));
                    setAnswering(undefined);
                  }}
                  onCancel={() => {
                    // Answer is drawn again in the form's place where the question is still
                    // open, and the keyboard goes back to it; else to the line's chat.
                    backTo.current = key;
                    setAnswering(undefined);
                  }}
                />
              ) : (
                /* A question its task is still paused on: the person may answer it, while
                   its chat is open. */
                toAnswer.has(key) &&
                !answered.has(key) &&
                !closed(line.from_key) && (
                  <button
                    type="button"
                    className="panel-view activity-answer"
                    ref={(node) => {
                      if (node === null) answerControls.current.delete(key);
                      else answerControls.current.set(key, node);
                    }}
                    tabIndex={0}
                    aria-label={`Answer ${line.from}'s question`}
                    title={`${line.from} is paused until this is answered. Your answer reaches it marked as yours.`}
                    onClick={() => void answer(line)}
                  >
                    Answer
                  </button>
                )
              )}
              {unfound.has(key) && (
                <p className="activity-text answer-refused" role="alert">
                  {`purlis could not find the chat of ${line.from}, so nothing was opened to answer in: ${unfound.get(key) ?? ""}`}
                </p>
              )}
              {/* Said once the answer is taken, for whoever cannot see the control go. */}
              {answered.has(key) && (
                <p className="activity-text activity-absent" role="status">
                  {`Your answer was sent to ${line.from}.`}
                </p>
              )}
              {line.files.length > 0 &&
                (also.get(line.dispatch) ?? []).map((one) => (
                  <p key={one.file} className="activity-shared" data-testid="activity-shared">
                    <TriangleAlert className="node-icon" aria-hidden="true" />
                    <span>{alsoSaid(one)}</span>
                  </p>
                ))}
            </li>
          );
        })}
      </ol>
      {refused}
    </>
  );
}
