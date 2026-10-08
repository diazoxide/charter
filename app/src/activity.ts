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
 * brought the chat back, and says the chat is not open otherwise.
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
 * oldest first, and what tells a new line whether it is one of theirs.
 */
export type Timeline = {
  lines: readonly Drawn[];
  /** Every chat on the timeline, by its key, and how deep it is: the session is at 0. */
  chats: ReadonlyMap<string, number>;
  /** Every dispatch on it, by id, and its depth: 1 for the session's own task. */
  tasks: ReadonlyMap<string, number>;
};

/** A timeline with nothing on it yet, for the session whose key is `session`. */
export function emptyTimeline(session: string): Timeline {
  return { lines: [], chats: new Map([[session, 0]]), tasks: new Map() };
}

/** Where `a` sorts against `b`: by time, then by dispatch, then by place in the dispatch. */
function order(a: ActivityLine, b: ActivityLine): number {
  if (a.at !== b.at) return a.at < b.at ? -1 : 1;
  if (a.dispatch !== b.dispatch) return a.dispatch < b.dispatch ? -1 : 1;
  return a.n - b.n;
}

/**
 * `was` with `line` folded in, or `was` itself where the line is not this session's or is
 * already there.
 *
 * A dispatch joins the timeline by its first line: one from a chat already on it. Every other
 * line belongs where its dispatch does. So a line of another session's task changes nothing,
 * and neither does a line heard twice (once as an event, once in the first read).
 */
export function heard(was: Timeline, line: ActivityLine): Timeline {
  let { chats, tasks } = was;
  let depth = tasks.get(line.dispatch);
  if (depth === undefined) {
    const asker = line.kind === "dispatched" ? chats.get(line.from_key) : undefined;
    if (asker === undefined) return was;
    depth = asker + 1;
    tasks = new Map(tasks).set(line.dispatch, depth);
    // The first dispatch a chat is seen in places it: a chat is never moved deeper later.
    if (!chats.has(line.to_key)) chats = new Map(chats).set(line.to_key, depth);
  }
  const key = lineKey(line);
  if (was.lines.some((one) => lineKey(one.line) === key)) return was;
  // Lines arrive oldest first, so the place is nearly always the end.
  let at = was.lines.length;
  while (at > 0 && order(was.lines[at - 1].line, line) > 0) at -= 1;
  const lines = [...was.lines.slice(0, at), { line, depth }, ...was.lines.slice(at)];
  return { lines, chats, tasks };
}

/** The timeline the core's first answer makes. */
export function timelineOf(read: Activity): Timeline {
  return read.lines.reduce(heard, emptyTimeline(read.key));
}

/**
 * **The files two tasks of the session say they changed** (V100-68): each such path, and the
 * tasks that name it, by name. Read from the lines that end a task (`report`, `stopped`), which
 * carry the paths its report names. There are no file locks: this is the mark that says two
 * tasks touched one file.
 */
export function touchedByTwo(lines: readonly Drawn[]): ReadonlyMap<string, readonly string[]> {
  const by = new Map<string, Map<string, string>>();
  for (const { line } of lines) {
    for (const file of line.files) {
      const tasks = by.get(file) ?? new Map<string, string>();
      tasks.set(line.dispatch, line.task);
      by.set(file, tasks);
    }
  }
  const shared = new Map<string, readonly string[]>();
  for (const [file, tasks] of by) if (tasks.size > 1) shared.set(file, [...tasks.values()]);
  return shared;
}

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

/** A line's time as the tab says it: the clock, UTC. The whole stamp is its title. */
export function clockOf(stamp: string): string {
  const read = /T(\d{2}:\d{2}:\d{2})/.exec(stamp);
  return read ? read[1] : stamp;
}
