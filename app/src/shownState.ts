/**
 * **What a chat's row says it is doing: one word and one shape** (#1484, V100-3, V100-72).
 *
 * Derived here and nowhere else, from what already exists: the board's state for the chat,
 * whether it is in the needs-you queue, and, for a task, what its own record says (the report
 * it owes, how it reported, a question it has open with the chat that dispatched it). The Chats
 * list and the explorer both draw this value, so the same chat reads the same in both, and a
 * later surface (a tab's chip, its menu) calls the same function.
 *
 * **Nothing is guessed.** A harness purlis has heard nothing from reads `running (no detail
 * from <harness>)`: the program is running, which the app knows itself, and the rest is said
 * to be unknown (V100-71). A report whose outcome no record gives reads `reported`.
 */
import type { OpenChat } from "./bindings";
import type { State } from "./chatState";
import type { Token } from "./theme/theme";

/** Which state it is: what a surface folds, counts or sorts by. */
export type ShownKind =
  | "working"
  | "needs-you"
  | "asking"
  | "done"
  | "failed"
  | "cancelled"
  | "unreported"
  | "reported"
  | "idle"
  | "unheard";

/** The mark's shape, one per kind, so a state is told without its colour. */
export type ShownShape =
  "ring" | "hand" | "question" | "tick" | "cross" | "dash" | "triangle" | "dot" | "pause" | "dots";

/** A chat's state as a row shows it. */
export type Shown = {
  kind: ShownKind;
  /** The word beside the mark. Always drawn. */
  word: string;
  shape: ShownShape;
  /** The theme token the mark is coloured by. Never the word's colour: the word stays
   *  legible whatever a theme does with these. Finished states share the muted grey. */
  token: Token;
};

/** What a task's own record says of it, apart from what its program is doing. */
export type TaskFacts = {
  /** The one report it owes the chat that dispatched it: still owed, sent, or failed, which
   *  is purlis's word for a task that ended without one. */
  report: "owed" | "sent" | "failed";
  /** How it ended, in its dispatch record's word, where a record says: its own report's, or
   *  the one purlis wrote in its place (`stopped` for a task the person stopped). */
  outcome: string | null;
  /** The chat it has a question open with, by name, while it has one. */
  asking: string | null;
};

/** Everything a chat's shown state is derived from. */
export type Facts = {
  /** The board's state for it, or nothing for a shell tab nothing has reported for
   *  (`markOf`). `unknown` is a harness chat the board has not heard from. */
  board: State | undefined;
  /** Whether it is in the needs-you queue. A chat whose turn has ended and which is not in it
   *  is one the person told purlis to ignore until it asks again, or a task whose report
   *  already reached the chat that asked. */
  needsYou: boolean;
  /** Its record as a task, or nothing for a chat that is not one. */
  task: TaskFacts | null;
  /** Its harness, as the person calls it: named in what is not known. */
  harness: string | null;
};

const WORKING: Shown = { kind: "working", word: "working", shape: "ring", token: "state.running" };
const NEEDS_YOU: Shown = {
  kind: "needs-you",
  word: "needs you",
  shape: "hand",
  token: "needs-you.base",
};
const DONE: Shown = { kind: "done", word: "done", shape: "tick", token: "text.muted" };
const FAILED: Shown = { kind: "failed", word: "failed", shape: "cross", token: "state.failed" };
const CANCELLED: Shown = {
  kind: "cancelled",
  word: "cancelled",
  shape: "dash",
  token: "text.muted",
};
const UNREPORTED: Shown = {
  kind: "unreported",
  word: "ended without a report",
  shape: "triangle",
  token: "state.unreadable",
};
const IDLE: Shown = { kind: "idle", word: "idle", shape: "pause", token: "text.muted" };
const REPORTED: Shown = { kind: "reported", word: "reported", shape: "dot", token: "text.muted" };

/**
 * How a report a task sent reads, by the dispatch record's word for its outcome. `blocked` is
 * a task that could not do the work, which the row says as `failed`; its report says why. Any
 * other word is not one this window knows, and is not guessed at.
 */
const BY_OUTCOME: ReadonlyMap<string, Shown> = new Map([
  ["done", DONE],
  ["failed", FAILED],
  ["blocked", FAILED],
  ["cancelled", CANCELLED],
]);

/** The record's word for a task the person stopped before it reported: purlis wrote the
 *  report in its place, so the task itself sent none, and it did not die either. */
const STOPPED = "stopped";

/**
 * The state a chat's row shows, or nothing for a chat with none to show (a shell tab nothing
 * has reported for).
 *
 * **The needs-you queue is read first.** It is the core's word for "the person has the next
 * move", and the title bar's list and the hands on the rows above are drawn from it: a row
 * that said anything else would disagree with them. The core keeps a reported task's ordinary
 * end of turn out of the queue, so a finished task is not in it for having finished.
 *
 * **Then a task's record, before its program's state.** A task that reported is done, failed
 * or cancelled however its turn then ends: that end is what made a finished task look like a
 * chat waiting on the person. One that is running again, on the person's word, is working.
 */
export function shownState({ board, needsYou, task, harness }: Facts): Shown | undefined {
  if (needsYou) return NEEDS_YOU;
  if (task?.report === "sent") {
    if (board === "running") return WORKING;
    return (task.outcome === null ? undefined : BY_OUTCOME.get(task.outcome)) ?? REPORTED;
  }
  // The person stopped it: not a task that died, though neither sent a report.
  if (task?.report === "failed") return task.outcome === STOPPED ? CANCELLED : UNREPORTED;
  // Its program ended owing its report: purlis tells the chat that asked, and the row says
  // the same without waiting to be told.
  if (task !== null && (board === "done" || board === "failed")) return UNREPORTED;
  if (board === "done") return DONE;
  if (board === "failed") return FAILED;
  if (task !== null && task.asking !== null) {
    return {
      kind: "asking",
      word: `asking ${task.asking}`,
      shape: "question",
      token: "state.waiting",
    };
  }
  // Its turn has ended and it is not asking for the person: the hand is not raised for it.
  if (board === "waiting") return IDLE;
  if (board === "running") return WORKING;
  if (board === undefined) return undefined;
  return {
    kind: "unheard",
    word: harness === null ? "running (no detail)" : `running (no detail from ${harness})`,
    shape: "dots",
    token: "text.muted",
  };
}

/**
 * What `chat`'s record says of it as a task, or nothing for a chat that is not one.
 *
 * `nameOf` is what a chat is called now, by its number: the chat it is asking is named as that
 * chat's own row names it, so a rename is followed. Its name when the task was dispatched is
 * what is left to say once that chat has closed.
 */
export function taskFactsOf(
  chat: Pick<OpenChat, "from">,
  nameOf: (session: number) => string | undefined = () => undefined,
): TaskFacts | null {
  const from = chat.from;
  if (!from?.task) return null;
  return {
    report: from.unreported ? "failed" : from.reported ? "sent" : "owed",
    outcome: from.outcome ?? null,
    asking: from.asking ? (nameOf(from.chat) ?? from.name) : null,
  };
}

/** A chat's harness as the person calls it (`Codex`), or the project's word for it. */
export function harnessCalled(chat: Pick<OpenChat, "harness" | "card">): string | null {
  return chat.card?.title ?? chat.harness;
}

/** What a row hands the one function about its chat beside the board's word: plain values, so
 *  a row held on them is drawn again only when one changes. */
export type RowFacts = {
  report: TaskFacts["report"] | null;
  outcome: string | null;
  asking: string | null;
  harness: string | null;
};

/** `chat` as a row hands it on: its record as a task, flat, and its harness's name. `nameOf`
 *  is `taskFactsOf`'s. */
export function rowFactsOf(
  chat: Pick<OpenChat, "from" | "harness" | "card">,
  nameOf?: (session: number) => string | undefined,
): RowFacts {
  const task = taskFactsOf(chat, nameOf);
  return {
    report: task?.report ?? null,
    outcome: task?.outcome ?? null,
    asking: task?.asking ?? null,
    harness: harnessCalled(chat),
  };
}
