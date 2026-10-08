import type { PastTask } from "./bindings";
import { qualifierOf, shownOf } from "./finished";

/**
 * **Past tasks in the window** (#1510, V100-52): what was dispatched in a workspace, read after
 * each task has ended and whether or not the session that asked is still open.
 *
 * A view tab per workspace, `tabs.pastTasksView(workspace)`, opened from the workspace's menu,
 * the palette, and "Past tasks" beside a chat's Finished (n) line. The rows are the core's
 * (`past_tasks`), read from the dispatch records this machine keeps for this project: a task
 * whose row was cleared from the Chats list, or went with the chat that asked, is still here.
 * Tasks still running are not: they are in the Chats list.
 *
 * **The Dispatches tab is the project's ledger**: every dispatch of every workspace, running or
 * ended, handoffs too, with what each cost and its branch folder to discard. This is one
 * workspace's ended tasks, to find one again: narrowed by persona, by how it ended, by date and
 * by name, opened for its report and brief, and reopened where its finished row would allow.
 *
 * The narrowing is done here, over the rows the core answered. The list is bounded
 * (`PastTasks.most`); what is older than the bound is counted and said, and is not searched.
 */

/** What the list is narrowed to. Every part is `""` where it narrows nothing. */
export type PastFilter = {
  /** A persona, as the asker or as the one that ran the task. */
  persona: string;
  /** How it ended (`PastTask.how`). */
  how: string;
  /** The first and the last day it may have ended on, `YYYY-MM-DD`: **the person's own days**,
   *  where they are, as the rows say a time. */
  from: string;
  to: string;
  /** Text in the task's name, whatever its case. */
  text: string;
};

/** Everything, which is how the view opens. */
export const EVERY_PAST: PastFilter = { persona: "", how: "", from: "", to: "", text: "" };

/** Whether `filter` narrows anything. */
export function narrows(filter: PastFilter): boolean {
  return Object.values(filter).some((part) => part !== "");
}

/** A record's time (UTC, RFC 3339) as a moment, or nothing where it does not read as one. */
function momentOf(stamp: string | null): Date | undefined {
  if (stamp === null || !/^\d{4}-\d{2}-\d{2}T/.test(stamp)) return undefined;
  const at = new Date(stamp);
  return Number.isNaN(at.getTime()) ? undefined : at;
}

const two = (n: number) => String(n).padStart(2, "0");

/**
 * **The day a task ended, where the person is**: `2026-10-07`. The record keeps its time in
 * UTC, and a day is the person's own: a task that ended at 01:00 their time ended today, though
 * it was still yesterday in UTC. Empty where the time does not read as one.
 */
export function dayOf(row: Pick<PastTask, "ended">): string {
  const at = momentOf(row.ended);
  return at === undefined
    ? ""
    : `${at.getFullYear()}-${two(at.getMonth() + 1)}-${two(at.getDate())}`;
}

/**
 * A record's time as this view says it, **in the person's local time**: `2026-10-07 16:15`. The
 * zone is said once, in the view's heading ({@link zoneSaid}), and not on every row. The text
 * as it is where it is not a time.
 */
export function localAt(stamp: string): string {
  const at = momentOf(stamp);
  if (at === undefined) return stamp;
  const day = `${at.getFullYear()}-${two(at.getMonth() + 1)}-${two(at.getDate())}`;
  return `${day} ${two(at.getHours())}:${two(at.getMinutes())}`;
}

/**
 * The zone the view's times and days are in, as its heading says it: the zone's name where the
 * machine names one, and how far it is from UTC now (`Asia/Yerevan, UTC+4`, `UTC`). The stored
 * times stay UTC; this is only how they are read.
 */
export function zoneSaid(now: Date = new Date()): string {
  // Minutes to ADD to local time to get UTC: negative east of Greenwich.
  const minutes = -now.getTimezoneOffset();
  const [hours, rest] = [Math.trunc(Math.abs(minutes) / 60), Math.abs(minutes) % 60];
  const offset =
    minutes === 0
      ? "UTC"
      : `UTC${minutes > 0 ? "+" : "-"}${hours}${rest === 0 ? "" : `:${two(rest)}`}`;
  let name: string | undefined;
  try {
    name = Intl.DateTimeFormat().resolvedOptions().timeZone;
  } catch {
    // No zone database: the offset alone is still true.
  }
  return name === undefined || name === "" || name === offset ? offset : `${name}, ${offset}`;
}

/** The rows `filter` keeps, in the order they came: newest ended first. */
export function shownPast(rows: readonly PastTask[], filter: PastFilter): PastTask[] {
  const text = filter.text.trim().toLocaleLowerCase();
  return rows.filter((row) => {
    if (filter.persona !== "" && row.persona !== filter.persona) {
      if (row.asker_persona !== filter.persona) return false;
    }
    if (filter.how !== "" && row.how !== filter.how) return false;
    if (filter.from !== "" || filter.to !== "") {
      const day = dayOf(row);
      // A row whose day does not read is in no range of days.
      if (day === "") return false;
      if (filter.from !== "" && day < filter.from) return false;
      if (filter.to !== "" && day > filter.to) return false;
    }
    return text === "" || row.name.toLocaleLowerCase().includes(text);
  });
}

/** Every persona the rows name, as the one that asked or the one that ran the task, by name. */
export function personasOfPast(rows: readonly PastTask[]): string[] {
  const named = new Set<string>();
  for (const row of rows) {
    if (row.persona !== null) named.add(row.persona);
    if (row.asker_persona !== null) named.add(row.asker_persona);
  }
  return [...named].sort((a, b) => a.localeCompare(b));
}

/**
 * How a past task ended, **in the words every row says a state in** (#1484): the state's word,
 * then the core's own where it says more (`failed (blocked)`), as a finished row draws the two
 * side by side.
 */
export function endSaid(row: Pick<PastTask, "how" | "outcome">): string {
  const word = shownOf(row)?.word ?? row.outcome;
  const more = qualifierOf(row);
  return more === undefined ? word : `${word} (${more})`;
}

/** The ends the rows have, each once, in the order they first appear: what the list is
 *  narrowed by. */
export function endsOf(rows: readonly PastTask[]): { how: string; said: string }[] {
  const seen = new Map<string, string>();
  for (const row of rows) if (!seen.has(row.how)) seen.set(row.how, endSaid(row));
  return [...seen].map(([how, said]) => ({ how, said }));
}

/** Who asked for a task: the person, from the chat they dispatched it in, or that chat. */
export function askedBy(row: Pick<PastTask, "asker" | "by_person">): string {
  return row.by_person ? `you, from ${row.asker}` : row.asker;
}

/** Newest ended first; two that ended in one second by their ids, which sort by start. */
function newestFirst(a: PastTask, b: PastTask): number {
  const [ended, other] = [a.ended ?? "", b.ended ?? ""];
  if (ended !== other) return ended < other ? 1 : -1;
  return a.id < b.id ? 1 : a.id > b.id ? -1 : 0;
}

/**
 * The list after a later read: `changed` are the rows that are new or were written again, and
 * each takes the place of the row with its id. Nothing leaves the list by a later read: a
 * record collected since is found gone when its row is opened.
 */
export function mergedPast(held: readonly PastTask[], changed: readonly PastTask[]): PastTask[] {
  if (changed.length === 0) return [...held];
  const by = new Map(held.map((row) => [row.id, row]));
  for (const row of changed) by.set(row.id, row);
  return [...by.values()].sort(newestFirst);
}

/** The most of a brief or a report drawn before it is asked for whole. */
export const CLIP_CHARS = 1200;
export const CLIP_LINES = 12;

/**
 * `text` cut to what an opened row shows at first, or `undefined` where it already fits: at
 * most {@link CLIP_LINES} lines and {@link CLIP_CHARS} characters, cut at a character and
 * never inside one.
 */
export function clipped(text: string): string | undefined {
  const lines = text.split("\n");
  const some = lines.slice(0, CLIP_LINES).join("\n");
  const chars = [...some];
  if (lines.length <= CLIP_LINES && chars.length <= CLIP_CHARS) return undefined;
  return chars.slice(0, CLIP_CHARS).join("");
}

/** `2 records`, `1 record`. */
export function counted(count: number, one: string, many: string): string {
  return count === 1 ? `1 ${one}` : `${count} ${many}`;
}
