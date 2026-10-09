/**
 * **While you were away: what one summary says** (#1514, V100-65, V100-73).
 *
 * The person comes back to a window they have been away from for a while, and in place of
 * reading what each task did on its own row, one Notice per project sums it up:
 * "While you were away: 7 tasks done, 1 failed, 2 waiting on you", each part a link to the
 * chats it counts. This file is what the summary counts and what it says. Plain inputs and no
 * state: nothing here reads a chat, a store or the core. `AwaySummary.tsx` watches the window
 * and draws it.
 *
 * **Away is the window not in use, hidden, or left with no input** (`AwaySummary.tsx`'s
 * `useTimeAway` says which signals). A time away shorter than {@link AWAY_AFTER_MS} is no
 * time away: switching to an editor and back is not leaving.
 *
 * **Only what happened while away is counted** (the ticket's first line):
 *
 * - a finished task counts when it ended inside a time away, by the time its record keeps
 *   (`FinishedTask.ended`), in the count `taskBuckets.finishedBucketOf` gives it, the same
 *   count every surface puts it in;
 * - a chat counts as waiting on you when it came to need the person while they were away and
 *   still does: it came into the needs-you queue, **or it was in it already and has something
 *   new for them** (it asked again, or another reason was added to its item:
 *   {@link cameToNeedOf}). What was already waiting when they left, and nothing more, is not
 *   news, and one they have since answered is not waiting. A chat that is in the queue only for tasks that came
 *   to nothing is counted once, as those failures, and not again as waiting. A task that
 *   reported blocked and whose chat stays open waiting on the person is both: a failure by its
 *   finished row, and a chat waiting on you with a question to answer.
 *
 * **It hides nothing.** Every task keeps its row, its Activity lines and its needs-you item,
 * and every question keeps its Notice where it was asked. The summary only says where to look;
 * Dismiss puts the summary away and nothing else.
 *
 * - a dispatch refused while nobody was at its chat (#1507) counts when it was last refused
 *   inside a time away, by the time its item keeps (`AwayRefusal.latest`), for as long as its
 *   item stands in the title bar's needs-you list, which is where it is answered (#1551).
 *
 * **The seam for what else happened while away.** A part is a kind, a count's words and the
 * places it goes to. Another source joins as another {@link AwayPartKind} with its own items
 * and words, counted here by its own time, and links to where it is answered; nothing here
 * changes for the parts already said.
 */
import { clipped } from "./AwayRefusals";
import type { AwayRefusal, FinishedTask } from "./bindings";
import { taskFailedSaid, type ChatStates } from "./chatState";
import { finishedBucketOf } from "./taskBuckets";

/** How long the window must not be the one in use for the person to have been away: five
 *  minutes. */
export const AWAY_AFTER_MS = 5 * 60 * 1000;

/** One time away, as the window's own clock read it (milliseconds since the epoch). */
export type Away = { from: number; to: number };

/** Whether `at` (milliseconds) falls inside any of `away`. */
export function whileAway(away: readonly Away[], at: number): boolean {
  return away.some((one) => one.from <= at && at <= one.to);
}

/** The parts a summary says, in the order it says them: the spec's own, then the dispatches
 *  refused while nobody was at their chat (#1507, #1551). */
export type AwayPartKind = "done" | "failed" | "waiting" | "refused";

export const AWAY_PARTS: readonly AwayPartKind[] = ["done", "failed", "waiting", "refused"];

/** Where one counted thing goes on a press. */
export type AwayGo =
  /** A chat that is open: shown where it lives. */
  | { to: "chat"; session: number }
  /** A task that has finished: its finished row, under the session that asked for it. */
  | { to: "finished"; task: FinishedTask }
  /** A dispatch refused while nobody was there: the title bar's needs-you list, where it is
   *  answered. */
  | { to: "needs-you" };

/** One thing a part counts, and what its line in the part's list says. */
export type AwayItem = { key: string; says: string; go: AwayGo };

/** What the summary counts, part by part. A part with nothing in it is not said. */
export type AwaySummaryOf = Readonly<Record<AwayPartKind, readonly AwayItem[]>>;

export const NOTHING_AWAY: AwaySummaryOf = { done: [], failed: [], waiting: [], refused: [] };

/** How many things a summary counts. */
export function countedAway(summary: AwaySummaryOf): number {
  return AWAY_PARTS.reduce((sum, part) => sum + summary[part].length, 0);
}

/** The shares of what the chats are doing that a summary counts from. */
export type AwayRead = Pick<
  ChatStates,
  "bySession" | "needsYou" | "needs" | "failedTasks" | "reports" | "refusals" | "stoppedBelow"
>;

/** Those shares of `states`. */
export function readOf(states: ChatStates): AwayRead {
  const { bySession, needsYou, needs, failedTasks, reports, refusals, stoppedBelow } = states;
  return { bySession, needsYou, needs, failedTasks, reports, refusals, stoppedBelow };
}

/** Nothing read: what is selected while there is no time away to count. */
export const NOTHING_READ: AwayRead = readOf({
  bySession: {},
  needsYou: [],
  movedAt: {},
  queueFrom: 0,
  heardAt: {},
  reports: {},
  refusals: {},
  stoppedBelow: {},
  needs: {},
  failedTasks: {},
  children: {},
});

/** Whether two reads hold the same shares: the store keeps a share that did not change. */
export function sameRead(one: AwayRead, other: AwayRead): boolean {
  return (Object.keys(one) as (keyof AwayRead)[]).every((key) => one[key] === other[key]);
}

/**
 * Whether `need`, a sentence a chat's needs-you item says, is the one a failure of task `task`
 * says (`taskFailedSaid`, in each of its three ways).
 */
function saysFailureOf(need: string, task: string): boolean {
  const said = [
    taskFailedSaid({ task, how: "failed", why: "" }),
    taskFailedSaid({ task, how: "unreported", why: "" }),
    taskFailedSaid({ task, how: "did_not_start", why: "" }),
  ];
  return said.some((one) => need === one || need.startsWith(`${one}: `));
}

/**
 * Whether chat `session` is in the needs-you queue only because tasks it asked for came to
 * nothing: **every reason it has is the sentence of one of those failures**
 * (`ChatStates.failedTasks`, each matched to a sentence of its own among `needs`), it reported
 * nothing, nothing of it was refused or stopped, and it is not itself waiting on the person.
 * By what each sentence is, never by how many there are: a reason of another kind beside the
 * failures (its report has nowhere to go) makes it waiting on you.
 */
function onlyForFailures(states: AwayRead, session: number): boolean {
  const failed = states.failedTasks[session] ?? [];
  if (failed.length === 0) return false;
  const left = [...failed];
  const allFailures = (states.needs[session] ?? []).every((need) => {
    const at = left.findIndex((one) => saysFailureOf(need, one.task));
    if (at < 0) return false;
    left.splice(at, 1);
    return true;
  });
  return (
    allFailures &&
    (states.reports[session] ?? []).length === 0 &&
    (states.refusals[session] ?? []).length === 0 &&
    (states.stoppedBelow[session] ?? []).length === 0 &&
    states.bySession[session] !== "waiting"
  );
}

/** The shares of what the chats are doing that tell what each chat in the queue waits on. */
export type Queued = Pick<
  ChatStates,
  "needsYou" | "needs" | "reports" | "refusals" | "stoppedBelow" | "failedTasks"
>;

/**
 * The reasons chat `session`'s needs-you item gives, one entry each, **leaving out the
 * failures of its tasks**: those are counted as failed, by their finished rows, and never again
 * as waiting. Two reasons in the same words are two entries.
 */
function reasonsOf(states: Queued, session: number): string[] {
  const failed = states.failedTasks[session] ?? [];
  return [
    ...(states.needs[session] ?? [])
      .filter((one) => !failed.some((task) => saysFailureOf(one, task.task)))
      .map((one) => `need:${one}`),
    ...(states.reports[session] ?? []).map((one) => `report:${one}`),
    ...(states.refusals[session] ?? []).map((one) => `refused:${one}`),
    ...(states.stoppedBelow[session] ?? []).map((one) => `stopped:${one}`),
  ];
}

/**
 * **The chats that came to need the person between `was` and `now`**, oldest in the queue
 * first: each in the queue now that was not in it then, and each that was and **has something
 * new for them** (#1551), by item and not by chat:
 *
 * - a wait that began since: the chat came into `waiting` while the person was away
 *   (`cameIntoWaiting`, as the window saw its state change), whatever it waited on before;
 * - a reason its item gives that it did not give then, other than a task's failure.
 *
 * What only touches the chat (a task of it failed, a sub-agent of it ended, a report came back)
 * is not a new wait, and one that only kept what it had, or lost some of it, is not new.
 */
export function cameToNeedOf(
  was: Queued,
  now: Queued,
  cameIntoWaiting: ReadonlySet<number>,
): number[] {
  return now.needsYou.filter((session) => {
    if (!was.needsYou.includes(session) || cameIntoWaiting.has(session)) return true;
    const before = reasonsOf(was, session);
    return reasonsOf(now, session).some((one) => {
      const at = before.indexOf(one);
      if (at < 0) return true;
      before.splice(at, 1);
      return false;
    });
  });
}

/**
 * **What a summary counts**: the finished tasks that ended while the person was away, by how
 * each ended, and the chats that came to need them while away and still do, oldest in the
 * queue first. A record with no time it ended, or one that does not read as a time, is not
 * counted: nothing is said to have happened while away that the record cannot place there.
 */
export function awaySummaryOf({
  away,
  finished,
  cameToNeed,
  states,
  nameOf,
  refusedAway = [],
}: {
  away: readonly Away[];
  /** Every finished task of the project's open chats. */
  finished: Iterable<FinishedTask>;
  /** The chats that came into the needs-you queue while the person was away. */
  cameToNeed: ReadonlySet<number>;
  /** What the chats are doing now. */
  states: AwayRead;
  /** What a chat is called here. */
  nameOf: (session: number) => string;
  /** The project's dispatches refused while nobody was at their chat, as the title bar's
   *  needs-you list holds them now (#1507). */
  refusedAway?: readonly AwayRefusal[];
}): AwaySummaryOf {
  if (away.length === 0) return NOTHING_AWAY;
  const done: AwayItem[] = [];
  const failed: AwayItem[] = [];
  for (const task of finished) {
    const ended = task.ended === null ? Number.NaN : Date.parse(task.ended);
    if (Number.isNaN(ended) || !whileAway(away, ended)) continue;
    const item: AwayItem = {
      key: `task:${task.id}`,
      says: `${task.name}, a task of ${nameOf(task.asker)}`,
      go: { to: "finished", task },
    };
    (finishedBucketOf(task.how) === "failed" ? failed : done).push(item);
  }
  const waiting = states.needsYou
    .filter((session) => cameToNeed.has(session) && !onlyForFailures(states, session))
    .map((session): AwayItem => ({
      key: `chat:${session}`,
      says: nameOf(session),
      go: { to: "chat", session },
    }));
  // Said as the needs-you list says it, from what the item holds: two personas, and never a
  // word a chat wrote.
  const refused = refusedAway
    .filter((item) => whileAway(away, item.latest * 1000))
    .map((item): AwayItem => ({
      key: `refused:${item.asking}:${item.target}:${item.workspace ?? ""}`,
      says: `${clipped(item.asking)} wanted ${clipped(item.target)}`,
      go: { to: "needs-you" },
    }));
  return { done, failed, waiting, refused };
}

/**
 * **What each part says**, in the spec's words and order: `7 tasks done`, `1 failed`,
 * `2 waiting on you`. The first part about tasks names them; a part after it does not say
 * "tasks" again. A summary of chats waiting alone says they are chats. Refused dispatches
 * always say what they are: `1 dispatch refused`.
 */
export function awayPartsSaid(summary: AwaySummaryOf): { part: AwayPartKind; says: string }[] {
  const said: { part: AwayPartKind; says: string }[] = [];
  let named = false;
  for (const part of AWAY_PARTS) {
    const n = summary[part].length;
    if (n === 0) continue;
    if (part === "refused") {
      said.push({ part, says: `${n} ${n === 1 ? "dispatch" : "dispatches"} refused` });
      continue;
    }
    const noun = part === "waiting" ? (n === 1 ? "chat" : "chats") : n === 1 ? "task" : "tasks";
    const words = part === "waiting" ? "waiting on you" : part;
    said.push({ part, says: named ? `${n} ${words}` : `${n} ${noun} ${words}` });
    named = true;
  }
  return said;
}

/** The whole sentence, as a screen reader and a test read it. */
export function awaySaid(summary: AwaySummaryOf): string {
  const parts = awayPartsSaid(summary);
  return parts.length === 0
    ? ""
    : `While you were away: ${parts.map((one) => one.says).join(", ")}`;
}
