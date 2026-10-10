import type { Activity, ActivityLine } from "./bindings";
import type { ViewRef } from "./tabs";

/**
 * **A session's Activity in the window** (#1495, V100-44): one read-only timeline of what a chat
 * and its tasks said to each other — each dispatch, follow-up, progress note, question, answer
 * and report, and the tasks of its tasks — oldest first.
 *
 * It is a view tab, `{ from: null, view: "activity", key: "<session>" }`: one per chat, opened
 * from the chat tab's menu and from the palette (`tab.activity:<tab>`). The lines are read once
 * from the app's own dispatch records (`purlis_core::activity`), and each line the app records
 * after that arrives as an `activity-line` event and is folded in here ({@link heard}), so an
 * open tab follows the work without reading the records again.
 *
 * **Keyed by the chat's number**, which is all the window knows a chat by. A tab brought back
 * at the next launch asks for that number again: it shows the same chat where the launch
 * brought the chat back, and says the chat is not open otherwise. A chat that is restarted
 * gets a new number, and its Activity tab is moved to it with its panes (`tabs.replaceSession`).
 */
export const ACTIVITY_VIEW = "activity";

/** The Activity tab of the chat in `session`. */
export function activityView(session: number): ViewRef {
  return { from: null, view: ACTIVITY_VIEW, key: String(session) };
}

/** What the Activity tab of the chat called `name` is called. */
export function activityTitle(name: string): string {
  return `Activity · ${name}`;
}

/** Whether `view` is a chat's Activity tab. */
export function isActivity(view: ViewRef): boolean {
  return view.from === null && view.view === ACTIVITY_VIEW;
}

/** The chat an Activity tab is about; `undefined` for a key that is not a chat's number. */
export function activitySession(view: ViewRef): number | undefined {
  return /^[1-9]\d*$/.test(view.key) ? Number(view.key) : undefined;
}

/** The event the app sends for each line it records. */
export const ACTIVITY_HEARD = "activity-line";

/** What names a line: its dispatch and its place in it. A line is drawn once. */
export function lineKey(line: ActivityLine): string {
  return `${line.dispatch}:${line.n}`;
}

/** One line as the tab draws it: the line, and how deep its task is under the session. */
export type Drawn = { line: ActivityLine; depth: number };

/**
 * What an open Activity tab holds: the lines of one session's tasks and of the tasks under them,
 * oldest first, and what tells a line heard later whether it is one of theirs.
 */
export type Timeline = {
  lines: readonly Drawn[];
  /** Every chat on the timeline, by its key, and how deep it is: the session is at 0. */
  chats: ReadonlyMap<string, number>;
  /** Every dispatch on it, by id, and its depth: 1 for the session's own task. */
  tasks: ReadonlyMap<string, number>;
};

/**
 * The timeline the core's answer makes: **every line it listed, in its order and at its depth.**
 *
 * Which records are a session's, how deep each is and what order their lines stand in are the
 * core's to say (`purlis_core::activity`), and nothing here decides them again: a line the core
 * listed is drawn. What is built beside the lines is only what a line heard later is matched
 * against ({@link heard}).
 */
export function timelineOf(read: Activity): Timeline {
  const chats = new Map<string, number>([[read.key, 0]]);
  const tasks = new Map<string, number>();
  for (const line of read.lines) {
    if (!tasks.has(line.dispatch)) tasks.set(line.dispatch, line.depth);
    // The first dispatch a chat is seen in places it.
    if (line.kind === "dispatched" && !chats.has(line.to_key)) chats.set(line.to_key, line.depth);
  }
  return { lines: read.lines.map((line) => ({ line, depth: line.depth })), chats, tasks };
}

/**
 * `was` with `line`, **a line the app told as it recorded it**, folded in; or `was` itself where
 * the line is not this session's.
 *
 * A dispatch joins the timeline by its first line: one from a chat already on it. Every other
 * line belongs where its dispatch does, so a line of another session's task changes nothing.
 * A line told after the read is newer than what was read, so it goes last. One that is already
 * there (heard and read both, or the one line that counts a task's unkept messages, told again
 * as the count grows) takes its own place and is drawn once.
 */
export function heard(was: Timeline, line: ActivityLine): Timeline {
  let { chats, tasks } = was;
  let depth = tasks.get(line.dispatch);
  if (depth === undefined) {
    const asker = line.kind === "dispatched" ? chats.get(line.from_key) : undefined;
    if (asker === undefined) return was;
    depth = asker + 1;
    tasks = new Map(tasks).set(line.dispatch, depth);
    if (!chats.has(line.to_key)) chats = new Map(chats).set(line.to_key, depth);
  }
  const key = lineKey(line);
  const at = was.lines.findIndex((one) => lineKey(one.line) === key);
  const lines =
    at < 0
      ? [...was.lines, { line, depth }]
      : was.lines.map((one, place) => (place === at ? { line, depth } : one));
  return { lines, chats, tasks };
}

/**
 * **The session each chat on the timeline has, as its lines say it**, by the chat's key
 * (`ActivityLine.from_key`): `null` for a chat that was closed when its line was read or told.
 * The newest line says it, so a line told after the read stands over what the read said.
 */
export function chatsOn(lines: readonly Drawn[]): ReadonlyMap<string, number | null> {
  const chats = new Map<string, number | null>();
  for (const { line } of lines) chats.set(line.from_key, line.from_session);
  return chats;
}

/**
 * **The chats to ask the app about again, now that the window's open chats are `after`** and
 * were `before` (#1457): by key, from `chats`, the session each chat on the timeline is known to
 * have (`null`: closed).
 *
 * A chat whose session left the window's chats may have closed, or been restarted under another
 * number: only the app can say which, so it is asked, and only for those. Where a chat came in,
 * a chat known to be closed may be the one that came back, so those are asked too. A chat whose
 * session is still open, or that the window never held, is not asked: a press still asks.
 */
export function askAgain(
  chats: ReadonlyMap<string, number | null>,
  before: ReadonlySet<number>,
  after: ReadonlySet<number>,
): string[] {
  const came = [...after].some((one) => !before.has(one));
  return [...chats]
    .filter(([, session]) => (session === null ? came : before.has(session) && !after.has(session)))
    .map(([key]) => key);
}

/** What the Answer form says where the task's chat closed while the person was typing. */
export function chatClosedSaid(line: ActivityLine): string {
  return `${line.from}'s chat has closed, so an answer would reach no turn of its work.`;
}

/** The kinds of line that end a task: after one, none of its questions takes an answer. */
const ENDS_A_TASK: readonly string[] = ["report", "stopped"];

/**
 * **The questions the person may answer now** (#1496), by {@link lineKey}: a question the app
 * said its task is paused on, with that question's number (`ActivityLine.asks`), which nothing
 * told since has closed.
 *
 * **A question is closed by its number, never by where a line stands.** The app tells an answer
 * with the number of the question it answered (`ActivityLine.answers`), whoever gave it; and
 * tells the same on the one "not listed" line where the record kept none of the answer's words.
 * So an answer that is recorded after the task's next question does not close that next
 * question. A task's report or its ending closes every question it had.
 *
 * The app says which question is open as it reads the timeline and as it tells a question. What
 * is told afterwards is folded in here, so a question answered while the tab is open stops
 * offering Answer without the timeline being read again.
 */
export function answerable(lines: readonly Drawn[]): ReadonlySet<string> {
  const open = new Set<string>();
  for (const { line } of lines) {
    // A question whose text was left out (#1520) is answered in its asking chat, never here.
    if (
      line.kind === "question" &&
      line.asks !== null &&
      !line.left_out &&
      closedBy(line, lines) === undefined
    )
      open.add(lineKey(line));
  }
  return open;
}

/** The line that closed the question on `asked`, where one has been told: its answer (or the
 *  line that stands for an answer whose words were not kept), else its task's ending. */
function closedBy(asked: ActivityLine, lines: readonly Drawn[]): ActivityLine | undefined {
  const mine = lines.map((one) => one.line).filter((one) => one.dispatch === asked.dispatch);
  return (
    mine.find((one) => one.answers !== null && one.answers === asked.asks) ??
    mine.find((one) => ENDS_A_TASK.includes(one.kind))
  );
}

/**
 * **Why the question on `asked` can no longer be answered**, in one sentence for the person who
 * was answering it (M2 of #1496's review); `undefined` while it still can be.
 */
export function closedWhy(asked: ActivityLine, lines: readonly Drawn[]): string | undefined {
  if (asked.asks === null) return `${asked.from} is not waiting on this question any more.`;
  const by = closedBy(asked, lines);
  if (by === undefined) return undefined;
  if (ENDS_A_TASK.includes(by.kind))
    return `${asked.from} has ended, so an answer would reach no turn of its work.`;
  if (by.kind === "answer" && by.by_person)
    return "You have already answered this question, and one answer is final.";
  if (by.kind === "answer")
    return `The chat ${by.from} answered this question first, so there is nothing left to send.`;
  return "This question was answered first, so there is nothing left to send.";
}

/** A file another task's report names too, and those tasks, by name. */
export type Also = { file: string; others: readonly string[] };

/**
 * **The files more than one task's report names** (V100-68), for each task that named one: the
 * file, and the other tasks that named it. Keyed by dispatch.
 *
 * - **A task is its dispatch**, never its name: a task run again under the same name is another
 *   task, and the two mark each other.
 * - **Only tasks that worked in the same place** (`ActivityLine.place`: the same workspace and
 *   folder). A task given a branch of its own works in a folder of its own, so it marks nothing
 *   and is marked by nothing; neither is a task in another workspace.
 * - **Only tasks that ran at the same time** (#1520): from its dispatch to its report, each
 *   taken from its lines. A task that ended before the other started could not have clashed with
 *   it, so a long-lived session's common files are not marked on every task it ever ran.
 *
 * It is read from what each report **says**: the paths its words name. There are no file locks,
 * and nothing here knows what a task really changed.
 */
export function namedByOthers(lines: readonly Drawn[]): ReadonlyMap<string, readonly Also[]> {
  /** By place and file: the dispatches whose report names it, and each one's task name. */
  const named = new Map<string, Map<string, string>>();
  const spot = (line: ActivityLine, file: string) => `${line.place}\u0000${file}`;
  for (const { line } of lines) {
    for (const file of line.files) {
      const tasks = named.get(spot(line, file)) ?? new Map<string, string>();
      tasks.set(line.dispatch, line.task);
      named.set(spot(line, file), tasks);
    }
  }
  const ran = spans(lines);
  const together = (a: string, b: string) => {
    const one = ran.get(a);
    const other = ran.get(b);
    return one !== undefined && other !== undefined && one[0] <= other[1] && other[0] <= one[1];
  };
  const also = new Map<string, Also[]>();
  for (const { line } of lines) {
    for (const file of line.files) {
      const others = [...(named.get(spot(line, file)) ?? [])]
        .filter(([dispatch]) => dispatch !== line.dispatch && together(dispatch, line.dispatch))
        .map(([, task]) => task);
      if (others.length === 0) continue;
      const mine = also.get(line.dispatch) ?? [];
      if (!mine.some((one) => one.file === file)) mine.push({ file, others });
      also.set(line.dispatch, mine);
    }
  }
  return also;
}

/**
 * When each task on `lines` ran, by dispatch, in milliseconds: from its first line (its dispatch)
 * to the line that ends it, or on without end while it has none. A time that does not read as
 * one stretches the task's span, so such a task is never left out of a mark.
 */
function spans(lines: readonly Drawn[]): Map<string, [number, number]> {
  const ran = new Map<string, [number, number]>();
  for (const { line } of lines) {
    const read = Date.parse(line.at);
    const unread = Number.isNaN(read);
    const [from, to] = ran.get(line.dispatch) ?? [Infinity, Infinity];
    ran.set(line.dispatch, [
      Math.min(from, unread ? -Infinity : read),
      ENDS_A_TASK.includes(line.kind) ? (unread ? Infinity : read) : to,
    ]);
  }
  return ran;
}

/** What the mark on a report says: a claim about another report's words, and no more. */
export function alsoSaid({ file, others }: Also): string {
  return others.length === 1
    ? `${others[0]}'s report also names ${file}`
    : `The reports of ${others.join(", ")} also name ${file}`;
}

/** How many messages a record keeps the text of, and how much text, as the tab says them
 *  (`purlis_core::dispatchrecord::MOST_SAID`, `MOST_SAID_BYTES`). */
const MOST_SAID = 500;
const MOST_SAID_SAID = "256 KiB";

/**
 * What the one line that stands for a task's unkept messages says: how many, whose, and the
 * reason that is true of that record.
 */
export function unkeptSaid(line: ActivityLine): string {
  const count = line.unkept ?? 0;
  const many = count === 1 ? "1 message" : `${count} messages`;
  const are = count === 1 ? "is" : "are";
  const why =
    line.unkept_why === "before"
      ? count === 1
        ? "it was sent before purlis kept what tasks say."
        : "they were sent before purlis kept what tasks say."
      : line.unkept_why === "count"
        ? `purlis keeps the first ${MOST_SAID} messages of a task.`
        : `purlis keeps the first ${MOST_SAID_SAID} of what a task and its asking chat say.`;
  return `${many} of ${line.task} ${are} not listed: ${why}`;
}

/** What a message whose text read like a credential says in its place (#1520). */
export const LEFT_OUT_SAID =
  "Its text was left out: it reads like a credential, and purlis keeps none.";

/** What a question whose text was left out says of why it has no Answer here (#1520). */
export function leftOutAsked(line: ActivityLine): string {
  return `${line.to} has this question: purlis did not keep its text, so it is answered there and not here.`;
}

/** What a message whose words are no longer kept says in their place: they expired, or were
 *  forgotten with the task's row (#1520). */
export const EXPIRED_SAID =
  "Its words are no longer kept: purlis forgets what a task said 30 days after the task ended, and when its row is cleared or the chat that asked closes.";

/** The most of a line's text drawn before it is asked for in full: characters, and lines. */
export const CLIP_CHARS = 400;
export const CLIP_LINES = 6;

/** `text` as a line shows it before it is opened, and whether anything was left out. */
export function clipped(text: string): { shown: string; more: boolean } {
  const rows = text.split("\n");
  let shown = rows.length > CLIP_LINES ? rows.slice(0, CLIP_LINES).join("\n") : text;
  // By code point, so a character is never cut in half.
  const chars = Array.from(shown);
  if (chars.length > CLIP_CHARS) shown = chars.slice(0, CLIP_CHARS).join("");
  return shown === text ? { shown, more: false } : { shown: `${shown}…`, more: true };
}
