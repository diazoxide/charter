import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Activity as ActivityMark, LoaderCircle, TriangleAlert } from "lucide-react";
import { EmptyState } from "./EmptyState";
import { Notice } from "./Notice";
import { commands, type ActivityHeard, type ActivityLine, type PlaneId } from "./bindings";
import {
  ACTIVITY_HEARD,
  clipped,
  clockOf,
  heard,
  lineKey,
  timelineOf,
  touchedByTwo,
  type Timeline,
} from "./activity";

/** What the tab has read: the chat's name, the timeline, and what the records did not give. */
type Read = { name: string; timeline: Timeline; unkept: number; undrawn: number };

/**
 * **One chat's Activity, in a tab of its own** (#1495, V100-44): what it and its tasks said to
 * each other, and the tasks of its tasks, as one timeline, oldest first. Each line has its time,
 * who said it to whom, what kind of line it is, and what was said.
 *
 * **Read-only.** Nothing here is typed into, and nothing here sends anything.
 *
 * **A line opens the chat it came from**: the name of the chat that said it is the control, and
 * brings that chat's tab to the front as a row of the Chats list does. A chat that has closed
 * is named in plain text.
 *
 * **What a chat said is drawn as text.** A brief, a message and a report are a chat's own words:
 * they are put on the screen as characters, with their line breaks, and never read as Markdown
 * or markup. A long one is clipped until it is asked for in full.
 *
 * **A file two tasks say they changed is marked** (V100-68) on each of the two reports. There
 * are no file locks, so this is where two tasks having touched one file is seen.
 *
 * **It follows the work while it is open.** The timeline is read once. Every line the app
 * records after that arrives as an event and is added in its place, so nothing is read again
 * for it. Listening starts before the read, and a line that arrives in between is kept and
 * folded in after it, once.
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
  const [said, setSaid] = useState<{ read?: Read; trouble?: string }>();
  const [again, setAgain] = useState(0);
  /** The lines shown in full, by {@link lineKey}. */
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());

  useEffect(() => {
    if (session === undefined) return;
    let gone = false;
    let stop: (() => void) | undefined;
    /** Lines heard before the first read answered. */
    let early: ActivityLine[] | undefined = [];
    void (async () => {
      try {
        const unlisten = await listen<ActivityHeard>(ACTIVITY_HEARD, (event) => {
          if (gone || event.payload.plane !== plane) return;
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
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in: a unit test, or a webview being torn down.
      }
      const answer = await commands
        .activity(plane, session)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (gone) return;
      if (answer.status === "error") {
        setSaid({ trouble: answer.error });
        return;
      }
      const { name, unkept, undrawn } = answer.data;
      const timeline = (early ?? []).reduce(heard, timelineOf(answer.data));
      early = undefined;
      setSaid({ read: { name, timeline, unkept, undrawn } });
    })();
    return () => {
      gone = true;
      stop?.();
    };
  }, [plane, session, again]);

  const lines = said?.read?.timeline.lines;
  const shared = useMemo(() => touchedByTwo(lines ?? []), [lines]);

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
  if (said.trouble !== undefined || said.read === undefined) {
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
  const { name, timeline, unkept, undrawn } = said.read;
  const notes = (
    <>
      {unkept > 0 && (
        <p className="none" data-testid="activity-unkept">
          {unkept === 1
            ? "1 message is not listed: purlis keeps the first 500 of a task."
            : `${unkept} messages are not listed: purlis keeps the first 500 of a task.`}
        </p>
      )}
      {/* Counted and never shown, as the Dispatches tab says it. */}
      {undrawn > 0 && (
        <p className="none" data-testid="activity-undrawn">
          {undrawn === 1
            ? "1 task is not listed: its record holds text purlis refuses to put on the screen."
            : `${undrawn} tasks are not listed: their records hold text purlis refuses to put on the screen.`}
        </p>
      )}
    </>
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
        {notes}
      </>
    );
  }
  return (
    <>
      <ol className="activity" aria-label={`Activity of ${name}`}>
        {timeline.lines.map(({ line, depth }) => {
          const key = lineKey(line);
          const full = open.has(key);
          const { shown, more } = clipped(line.text);
          const also = line.files.flatMap((file) => {
            const others = (shared.get(file) ?? []).filter((task) => task !== line.task);
            return others.length === 0 ? [] : [{ file, others }];
          });
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
                {clockOf(line.at)}
              </time>
              <span className="activity-kind">
                {line.outcome === null || line.outcome === line.kind
                  ? line.kind
                  : `${line.kind} · ${line.outcome}`}
              </span>
              <span className="activity-who">
                {line.from_session === null ? (
                  <span className="activity-gone" title="Its chat is closed">
                    {line.from}
                  </span>
                ) : (
                  <button
                    type="button"
                    className="vault-secret"
                    // #190: WebKit leaves a control out of the tab sequence without `tabIndex`.
                    tabIndex={0}
                    aria-label={`Show chat ${line.from}`}
                    title="Show its chat"
                    onClick={() => {
                      if (line.from_session !== null) onShowChat(line.from_session);
                    }}
                  >
                    {line.from}
                  </button>
                )}
                {` → ${line.to}`}
              </span>
              {/* A chat's own words: a text node, with its line breaks kept by the style. */}
              <p className="activity-text">{full ? line.text : shown}</p>
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
              {also.map(({ file, others }) => (
                <p key={file} className="activity-shared" data-testid="activity-shared">
                  <TriangleAlert className="node-icon" aria-hidden="true" />
                  <span>{`${file}: also changed by ${others.join(", ")}`}</span>
                </p>
              ))}
            </li>
          );
        })}
      </ol>
      {notes}
    </>
  );
}
