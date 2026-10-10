/**
 * **The Inbox's updates** (#1693, spec #1688, I-6, I-8, I-10): what happened that the person
 * may want to know and need not answer. Listed after the asks, newest first.
 *
 * **Each comes from a source that already says it**, derived here by a plain function with no
 * state, so each rule is tested on its own:
 *
 * - a task that finished or came to nothing: its finished row ({@link taskUpdates});
 * - a finding of the doctor: its report ({@link doctorUpdates});
 * - a chat that came back, resumed or fresh: the chats a launch or a Resume put back
 *   ({@link resumeUpdates});
 * - a sandbox change that asks nothing: a block that is no host ask ({@link sandboxUpdates});
 * - a dispatch refused while nobody was there (#1507): the away list ({@link awayUpdates});
 * - a Smart close that stopped without its record (SI-8f) ({@link smartCloseUpdates}).
 *
 * **Kept a day on this machine, never in a project** (`note_inbox_updates`, I-8): the window
 * notes what it derived, by a key of its source's own, and the core keeps it in purlis's data
 * home, so the same update noted again is one update, a relaunch keeps it, and one the person
 * dismissed does not come back while its source still says it.
 *
 * **The last two are answered at their source**, so they are drawn only while it still lists
 * them ({@link LIVE}): a refused dispatch with its three answers, a stopped Smart close with
 * Go and Dismiss. Every other update stands for its day.
 *
 * **Dismiss all and Mark all read act on updates alone** (I-10): no ask is ever answered in
 * bulk ({@link useInboxUpdates}).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { awaySaid } from "./AwayRefusals";
import {
  commands,
  type AwayRefusal,
  type ChatBlocked,
  type DoctorReport,
  type FinishedTask,
  type InboxUpdate,
  type OpenChat,
  type PlaneId,
  type UpdateKind,
  type UpdateNoted,
  type UpdateSettled,
} from "./bindings";
import { qualifierOf, shownOf } from "./finished";
import { finishedBucketOf } from "./taskBuckets";

/** The time now, in ms: when the window first saw an update that has no time of its own. */
export const seenAt = () => Date.now();

/** Seconds since 1970, as an update keeps its time. */
export const secondsOf = (ms: number) => Math.floor(ms / 1000);

/** How long an update is kept: a day (I-6), as the core keeps it. */
export const KEPT_FOR_MS = 24 * 60 * 60 * 1000;

/** The kinds drawn only while their source still lists them: each is answered there. */
export const LIVE: ReadonlySet<UpdateKind> = new Set(["refused-away", "smart-close"]);

/** What a row says of what kind of thing happened. */
export const KIND_SAID: Record<UpdateKind, string> = {
  "task-done": "Task finished",
  "task-failed": "Task failed",
  doctor: "Doctor",
  resumed: "Came back",
  sandbox: "Sandbox",
  "refused-away": "Refused while you were away",
  "smart-close": "Smart close stopped",
};

/**
 * **A task that ended, done or come to nothing** (#1693): one update per finished row that
 * says when it ended, in the count its row is in (`finishedBucketOf`), worded as the row
 * words it. A row with no time it ended is no update: nothing places it. Nor is one that ended
 * a day or more before `now`: it is past what is kept.
 */
export function taskUpdates(
  finished: Iterable<FinishedTask>,
  nameOf: (session: number) => string,
  now: number,
): UpdateNoted[] {
  const out: UpdateNoted[] = [];
  for (const task of finished) {
    const ended = task.ended === null ? Number.NaN : Date.parse(task.ended);
    if (Number.isNaN(ended) || now - ended >= KEPT_FOR_MS) continue;
    const word = shownOf(task)?.word ?? task.how;
    const more = qualifierOf(task);
    out.push({
      key: `task:${task.id}`,
      kind: finishedBucketOf(task.how) === "failed" ? "task-failed" : "task-done",
      at: secondsOf(ended),
      session: task.asker,
      chain: [nameOf(task.asker)],
      says: more === undefined ? `${task.name}: ${word}` : `${task.name}: ${word} · ${more}`,
    });
  }
  return out;
}

/**
 * **What the doctor found** (#1693): each row that failed, and each warning of a check this
 * build runs (a row it does not run is no finding: `DoctorRow.checked`). An update says when
 * the window first saw it (`now`); the same finding noted again keeps that time.
 */
export function doctorUpdates(report: DoctorReport | undefined, now: number): UpdateNoted[] {
  if (report === undefined) return [];
  return [...report.rows, ...report.app_rows]
    .filter((row) => row.status === "fail" || (row.status === "warn" && row.checked))
    .map((row) => ({
      key: `doctor:${row.name}:${row.status}`,
      kind: "doctor",
      at: secondsOf(now),
      session: null,
      chain: [],
      says: row.detail === "" ? row.name : `${row.name}: ${row.detail}`,
    }));
}

/**
 * **A chat that came back** (#1693): resumed on its conversation, or started fresh in its
 * place and why. Only the chats the core said either of; a chat that simply started is none.
 */
export function resumeUpdates(chats: Iterable<OpenChat>, now: number): UpdateNoted[] {
  const out: UpdateNoted[] = [];
  for (const chat of chats) {
    if (chat.resumed === null && chat.fresh === null) continue;
    out.push({
      key: `resumed:${chat.session}:${chat.resumed ?? `fresh:${chat.fresh}`}`,
      kind: "resumed",
      at: secondsOf(now),
      session: chat.session,
      chain: [chat.name],
      says:
        chat.resumed !== null
          ? "Resumed its conversation"
          : `Came back as a new chat: ${chat.fresh}`,
    });
  }
  return out;
}

/**
 * **A sandbox change that asks nothing** (#1693): a block that is no host ask (#1690 counts
 * only a host it offers to allow). Its sentence is the block's own, the core's words.
 */
export function sandboxUpdates(
  blocks: Readonly<Record<number, readonly ChatBlocked[]>>,
  nameOf: (session: number) => string,
  now: number,
): UpdateNoted[] {
  return Object.entries(blocks).flatMap(([key, held]) => {
    const session = Number(key);
    return held
      .filter((block) => block.offer !== "host" || block.ours || block.target === null)
      .map((block) => ({
        key: `sandbox:${session}:${block.operation}:${block.kind}:${block.ours}:${block.target ?? ""}`,
        kind: "sandbox" as const,
        at: secondsOf(now),
        session,
        chain: [nameOf(session)],
        says: block.said,
      }));
  });
}

/** The key of a refused dispatch's update: its pair and workspace. */
export const awayUpdateKey = (one: Pick<AwayRefusal, "asking" | "target" | "workspace">) =>
  `away:${one.asking}#${one.target}#${one.workspace ?? ""}`;

/**
 * **A dispatch refused while nobody was at its chat** (#1507): one update a pair and
 * workspace, at its last refusal. Answered where it is listed, with its own three answers.
 */
export function awayUpdates(refused: readonly AwayRefusal[]): UpdateNoted[] {
  return refused.map((one) => ({
    key: awayUpdateKey(one),
    kind: "refused-away",
    at: one.latest,
    session: null,
    chain: [],
    says: awaySaid(one),
  }));
}

/** The key of a stopped Smart close's update: its chat and why. */
export const smartCloseKey = (session: number, why: string) => `smartclose:${session}:${why}`;

/**
 * **A Smart close that stopped without its record** (SI-8f): the chat stays open, and the
 * update says why until the person goes to it or dismisses it.
 */
export function smartCloseUpdates(
  stopped: Readonly<Record<number, string>>,
  nameOf: (session: number) => string,
  now: number,
): UpdateNoted[] {
  return Object.entries(stopped).map(([key, why]) => {
    const session = Number(key);
    return {
      key: smartCloseKey(session, why),
      kind: "smart-close",
      at: secondsOf(now),
      session,
      chain: [nameOf(session)],
      says: why,
    };
  });
}

/**
 * **The updates as the Inbox lists them**: what the core keeps, newest first, leaving out an
 * update of a {@link LIVE} kind its source no longer lists, and putting in one its source lists
 * that the core does not keep (`live`): one that happened more than a day ago is still answered
 * where its source lists it, and is put away only there.
 */
export function listed(
  kept: readonly InboxUpdate[],
  live: readonly UpdateNoted[],
): readonly InboxUpdate[] {
  const listing = new Set(live.map((one) => one.key));
  const held = new Set(kept.map((one) => one.key));
  return [
    ...kept.filter((one) => !LIVE.has(one.kind) || listing.has(one.key)),
    ...live.filter((one) => !held.has(one.key)).map((one) => ({ ...one, read: false })),
  ].sort((one, other) => other.at - one.at || one.key.localeCompare(other.key));
}

/** What a project's updates are, and what can be done with them. */
export type InboxUpdates = {
  /** Newest first; nothing before the first read. */
  updates: readonly InboxUpdate[] | undefined;
  /** Why this machine keeps none, where it does not: the list is then this window's alone. */
  trouble?: string;
  /** Mark read, or Dismiss: the ones named, or every one listed (`null`). */
  settle: (keys: readonly string[] | null, how: UpdateSettled) => void;
};

/** What is said where the core answered with no list of updates. */
const KEEPS_NONE = "purlis did not say what updates it keeps";

/** An answer that is a list of updates, or nothing: a test's catch-all is no list. */
const listOf = (data: unknown) => (Array.isArray(data) ? (data as InboxUpdate[]) : undefined);

/**
 * **A project's updates, noted and kept** (#1693): reads what the core keeps, notes each update
 * `derived` holds that this window has not noted yet, and settles what the person marks read or
 * dismisses. Where this machine keeps none (no data home), the updates are drawn from `derived`
 * alone, held in this window and gone at its end, and `trouble` says why.
 */
export function useInboxUpdates(plane: PlaneId, derived: readonly UpdateNoted[]): InboxUpdates {
  /** What the core keeps, and for which project: another project's list is none of this one's. */
  const [keptFor, setKept] = useState<{ plane: PlaneId; list: readonly InboxUpdate[] }>();
  const kept = keptFor?.plane === plane ? keptFor.list : undefined;
  const [trouble, setTrouble] = useState<string>();
  /** What this window settled, for a machine that keeps nothing. */
  const [settled, setSettled] = useState<ReadonlyMap<string, UpdateSettled>>(new Map());
  /** The keys this window noted already, so a source said again writes nothing. */
  const noted = useRef(new Set<string>());

  /** Each ask's number, and the newest whose answer landed: an answer that comes back after a
   *  newer one is older news, and is dropped. */
  const asked = useRef(0);
  const newest = useRef(0);
  const ask = useCallback(
    (
      asking: PlaneId,
      sent: () => Promise<
        { status: "ok"; data: InboxUpdate[] } | { status: "error"; error: string }
      >,
    ) => {
      const mine = ++asked.current;
      void sent()
        .then((answer) => {
          if (mine < newest.current) return;
          newest.current = mine;
          if (answer.status === "error") {
            setTrouble(answer.error);
            return;
          }
          const list = listOf(answer.data);
          // A core that answers with no list (an older build) keeps none: this window does.
          if (list === undefined) {
            setTrouble(KEEPS_NONE);
            return;
          }
          setTrouble(undefined);
          setKept({ plane: asking, list });
        })
        .catch((err: unknown) => {
          if (mine >= newest.current) setTrouble(String(err));
        });
    },
    [],
  );

  useEffect(() => {
    noted.current = new Set();
    ask(plane, () => commands.inboxUpdates(plane));
  }, [plane, ask]);

  useEffect(() => {
    const fresh = derived.filter((one) => !noted.current.has(one.key));
    if (fresh.length === 0) return;
    for (const one of fresh) noted.current.add(one.key);
    ask(plane, () => commands.noteInboxUpdates(plane, fresh));
  }, [plane, derived, ask]);

  const settle = useCallback(
    (keys: readonly string[] | null, how: UpdateSettled) => {
      setSettled((was) => {
        const now = new Map(was);
        for (const key of keys ?? derived.map((one) => one.key))
          if (now.get(key) !== "dismissed") now.set(key, how);
        return now;
      });
      ask(plane, () => commands.settleInboxUpdates(plane, keys === null ? null : [...keys], how));
    },
    [plane, derived, ask],
  );

  const live = useMemo(() => derived.filter((one) => LIVE.has(one.kind)), [derived]);
  const updates = useMemo(() => {
    if (trouble === undefined) return kept === undefined ? undefined : listed(kept, live);
    // Nothing kept on this machine: what the sources say now, as this window settled it.
    const here = derived
      .filter((one) => settled.get(one.key) !== "dismissed")
      .map((one) => ({ ...one, read: settled.get(one.key) === "read" }));
    return listed(
      here,
      live.filter((one) => settled.get(one.key) !== "dismissed"),
    );
  }, [kept, live, trouble, derived, settled]);

  return { updates, trouble, settle };
}
