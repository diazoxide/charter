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
 * to be unknown (V100-71). A report whose outcome the app no longer has reads `reported`.
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
  | "ring"
  | "hand"
  | "question"
  | "tick"
  | "cross"
  | "dash"
  | "slash"
  | "dot"
  | "pause"
  | "broken-ring";

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
  /** How it reported, in the report's word, where the app still knows. */
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
  shape: "slash",
  token: "state.unreadable",
};
const IDLE: Shown = { kind: "idle", word: "idle", shape: "pause", token: "text.muted" };
const REPORTED: Shown = { kind: "reported", word: "reported", shape: "dot", token: "text.muted" };

/**
 * How a report's outcome reads, by the dispatch record's word for it. `blocked` is a task that
 * could not do the work, which the row says as `failed`; its report says why. `stopped` is a
 * task the person stopped, which reported on its way out: `cancelled`, as one its asking chat
 * cancelled is. Any other word is not one this window knows, and is not guessed at.
 */
const BY_OUTCOME: ReadonlyMap<string, Shown> = new Map([
  ["done", DONE],
  ["failed", FAILED],
  ["blocked", FAILED],
  ["cancelled", CANCELLED],
  ["stopped", CANCELLED],
]);

/**
 * The state a chat's row shows, or nothing for a chat with none to show (a shell tab nothing
 * has reported for).
 *
 * **A task's record is read before its program's state.** A task that reported is done, failed
 * or cancelled whatever its program does afterwards: its turn ending after the report is what
 * made a finished task look like a chat waiting on the person.
 */
export function shownState({ board, needsYou, task, harness }: Facts): Shown | undefined {
  if (task?.report === "sent") {
    return (task.outcome === null ? undefined : BY_OUTCOME.get(task.outcome)) ?? REPORTED;
  }
  const ended = board === "done" || board === "failed";
  // Ended owing its report: purlis tells the chat that asked, and the row says the same.
  if (task !== null && (task.report === "failed" || ended)) return UNREPORTED;
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
  if (needsYou) return NEEDS_YOU;
  // Its turn has ended and it is not asking for the person: the hand is not raised for it.
  if (board === "waiting") return IDLE;
  if (board === "running") return WORKING;
  if (board === undefined) return undefined;
  return {
    kind: "unheard",
    word: harness === null ? "running (no detail)" : `running (no detail from ${harness})`,
    shape: "broken-ring",
    token: "text.muted",
  };
}

/** What `chat`'s record says of it as a task, or nothing for a chat that is not one. */
export function taskFactsOf(chat: Pick<OpenChat, "from">): TaskFacts | null {
  const from = chat.from;
  if (!from?.task) return null;
  return {
    report: from.unreported ? "failed" : from.reported ? "sent" : "owed",
    outcome: from.outcome ?? null,
    asking: from.asking ? from.name : null,
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

/** `chat` as a row hands it on: its record as a task, flat, and its harness's name. */
export function rowFactsOf(chat: Pick<OpenChat, "from" | "harness" | "card">): RowFacts {
  const task = taskFactsOf(chat);
  return {
    report: task?.report ?? null,
    outcome: task?.outcome ?? null,
    asking: task?.asking ?? null,
    harness: harnessCalled(chat),
  };
}
