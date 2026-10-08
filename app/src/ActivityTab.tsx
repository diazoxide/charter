import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Activity as ActivityMark, LoaderCircle, TriangleAlert } from "lucide-react";
import { EmptyState } from "./EmptyState";
import { Notice } from "./Notice";
import { commands, type ActivityHeard, type ActivityLine, type PlaneId } from "./bindings";
import {
  ACTIVITY_HEARD,
  alsoSaid,
  clipped,
  EXPIRED_SAID,
  heard,
  lineKey,
  namedByOthers,
  timelineOf,
  unkeptSaid,
  type Timeline,
} from "./activity";
import { saidAt } from "./dispatches";

/** What the tab has read: the chat's name, the timeline, and the tasks it could not list. */
type Read = { name: string; timeline: Timeline; undrawn: number };

/** What the tab knows: nothing yet, the timeline, that its chat is not open, or why not. */
type Said = { read?: Read; closed?: true; trouble?: string };

/**
 * **One chat's Activity, in a tab of its own** (#1495, V100-44): what it and its tasks said to
 * each other, and the tasks of its tasks, as one timeline, oldest first. Each line has its day
 * and time (UTC, as the Dispatches tab says them), what kind of line it is, who said it to whom,
 * and what was said.
 *
 * **Read-only.** Nothing here is typed into, and nothing here sends anything.
 *
 * **Who said a line is who said it.** A task the person dispatched from a chat's tab is "you,
 * from" that chat (V100-70). An ending purlis recorded in a task's place (stopped, ended without
 * a report) is purlis's, for that task.
 *
 * **A line opens the chat it came from**: the chat's name is the control. The press asks the core
 * which session that chat has now, so it reaches a chat that was restarted since the timeline
 * was read; a chat that has closed by then is named in plain text from there on.
 *
 * **What a chat said is drawn as text.** A brief, a message and a report are a chat's own words:
 * they are put on the screen as characters, with their line breaks, and never read as Markdown
 * or markup. A long one is clipped until it is asked for in full.
 *
 * **What is not there is said where it would have stood.** A task's record keeps its first
 * messages; one line, in the task's place, says how many it did not keep and why. A message
 * whose words were kept for 30 days after its task ended keeps its line and says so.
 *
 * **A file another task's report also names is marked** (V100-68) on each of the two reports,
 * for tasks that worked in the same folder. It is what the reports say, and no more.
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
  /** The chats a press found closed, by key: named in plain text from then on. */
  const [gone, setGone] = useState<ReadonlySet<string>>(new Set());

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
      const { name, undrawn } = answer.data;
      const timeline = (early ?? []).reduce(heard, timelineOf(answer.data));
      early = undefined;
      setSaid({ read: { name, timeline, undrawn } });
    })();
    return () => {
      left = true;
      stop?.();
    };
  }, [plane, session, again]);

  const lines = said?.read?.timeline.lines;
  const also = useMemo(() => namedByOthers(lines ?? []), [lines]);

  /** A line's chat was pressed: the session it has now is asked for, and shown. */
  const show = async (line: ActivityLine) => {
    const now = await commands
      .activityChat(plane, line.from_key)
      .catch(() => ({ status: "error" as const, error: "" }));
    if (now.status === "ok" && now.data !== null) onShowChat(now.data);
    else if (now.status === "ok") setGone((was) => new Set(was).add(line.from_key));
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
  const { name, timeline, undrawn } = said.read;
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
        {refused}
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
          const chat =
            line.from_session === null || gone.has(line.from_key) ? (
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
                    else the chat's. */}
                {line.by_purlis ? "purlis, for " : line.by_person ? "you, from " : ""}
                {chat}
                {` → ${line.to}`}
              </span>
              {line.unkept !== null ? (
                <p className="activity-text activity-absent" data-testid="activity-unkept">
                  {unkeptSaid(line)}
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
