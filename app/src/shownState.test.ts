import { describe, expect, it } from "vitest";
import type { OpenChat } from "./bindings";
import { rowFactsOf, shownState, taskFactsOf, type Facts, type TaskFacts } from "./shownState";
import { TOKENS } from "./theme/theme";

/**
 * **What a chat's row says it is doing** (#1484): one word and one shape, derived in one place
 * from the board's state, the needs-you queue and a task's own record.
 *
 * Every input here is one the core sends: the queue is its word for "the person has the next
 * move", a task's `report` is `owed`, `sent` or `failed` (written in its place), and `outcome`
 * is its dispatch record's word.
 */

const OWED: TaskFacts = { report: "owed", outcome: null, asking: null };
const sent = (outcome: string | null): TaskFacts => ({ report: "sent", outcome, asking: null });
/** Purlis wrote the report in the task's place: it died (`failed`), or was stopped. */
const inItsPlace = (outcome: string | null): TaskFacts => ({
  report: "failed",
  outcome,
  asking: null,
});

/** A chat on Claude Code the person started, unless `more` says otherwise. */
function of(more: Partial<Facts>): Facts {
  return { board: "running", needsYou: false, task: null, harness: "Claude Code", ...more };
}

const said = (more: Partial<Facts>) => {
  const shown = shownState(of(more));
  return shown === undefined ? undefined : `${shown.shape} ${shown.word}`;
};

describe("a chat the person started", () => {
  it("is working while its turn runs", () => {
    expect(said({ board: "running" })).toBe("ring working");
  });

  it("needs you once its turn has ended", () => {
    expect(said({ board: "waiting", needsYou: true })).toBe("hand needs you");
  });

  it("is idle, with no hand, once its turn has ended and it is not asking for the person", () => {
    // Ignored until it asks again, say: the queue is what raises the hand, here as everywhere.
    expect(said({ board: "waiting", needsYou: false })).toBe("pause idle");
  });

  it("needs you when the app raised a need its own state does not say", () => {
    expect(said({ board: "running", needsYou: true })).toBe("hand needs you");
  });

  it("is done or failed as its program ended", () => {
    expect(said({ board: "done" })).toBe("tick done");
    expect(said({ board: "failed" })).toBe("cross failed");
  });
});

describe("a chat in the needs-you queue", () => {
  it("needs you whatever else is true of it, as the title bar's list says", () => {
    // A reported task the person typed in, whose new turn has ended or stopped on a prompt.
    expect(said({ needsYou: true, board: "waiting", task: sent("done") })).toBe("hand needs you");
    expect(said({ needsYou: true, board: "running", task: sent("failed") })).toBe("hand needs you");
    // A task with a question open that has stopped on a permission prompt: the person is the
    // one it waits on.
    expect(said({ needsYou: true, task: { ...OWED, asking: "steward 4" } })).toBe("hand needs you");
    // One that owes its report, and one purlis reported for.
    expect(said({ needsYou: true, board: "waiting", task: OWED })).toBe("hand needs you");
    expect(said({ needsYou: true, task: inItsPlace("failed") })).toBe("hand needs you");
  });
});

describe("a task", () => {
  it("is working while its turn runs", () => {
    expect(said({ task: OWED })).toBe("ring working");
  });

  it("is asking the chat that dispatched it, by that chat's name, while its question is open", () => {
    expect(said({ task: { ...OWED, asking: "steward 4" } })).toBe("question asking steward 4");
    // Its turn has ended on the question, and the person ignored it: it waits on that chat.
    expect(said({ task: { ...OWED, asking: "steward 4" }, board: "waiting" })).toBe(
      "question asking steward 4",
    );
  });

  it("says how it reported once its turn has ended, however that turn ended", () => {
    for (const board of ["waiting", "done", "failed", "unknown"] as const) {
      expect(said({ board, task: sent("done") })).toBe("tick done");
      expect(said({ board, task: sent("failed") })).toBe("cross failed");
      expect(said({ board, task: sent("cancelled") })).toBe("dash cancelled");
    }
  });

  it("is working again when the person has it run another turn after its report", () => {
    expect(said({ board: "running", task: sent("done") })).toBe("ring working");
    expect(said({ board: "running", task: sent(null) })).toBe("ring working");
  });

  it("reads failed when it reported that it could not go on", () => {
    expect(said({ board: "waiting", task: sent("blocked") })).toBe("cross failed");
  });

  it("says only that it reported when how is not known, and guesses no outcome", () => {
    expect(said({ board: "waiting", task: sent(null) })).toBe("dot reported");
    expect(said({ board: "waiting", task: sent("a word of later") })).toBe("dot reported");
    expect(said({ board: "waiting", task: sent("constructor") })).toBe("dot reported");
  });

  it("ended without a report: by its record, or by its program ending while it owed one", () => {
    expect(said({ task: inItsPlace("failed") })).toBe("triangle ended without a report");
    // No record says how: still not a guess that it was stopped.
    expect(said({ task: inItsPlace(null) })).toBe("triangle ended without a report");
    expect(said({ task: OWED, board: "done" })).toBe("triangle ended without a report");
    expect(said({ task: OWED, board: "failed" })).toBe("triangle ended without a report");
  });

  it("says in its own word and shape which way the person ended it, and never cancelled", () => {
    // #1488. Who ended it and which way are the core's fact, sent as the record's outcome.
    for (const board of ["done", "failed", "waiting", "unknown", undefined] as const) {
      // Closed: the report was written in its place. Stopped: it sent its one short report.
      expect(said({ board, task: inItsPlace("closed_by_person") })).toBe("octagon closed by you");
      expect(said({ board, task: sent("stopped_by_person") })).toBe("square stopped by you");
      // A record from before the way was kept: the person ended it, read as closed.
      expect(said({ board, task: inItsPlace("stopped") })).toBe("octagon closed by you");
    }
    // A stopped task still in the turn that sent its report is working, as any reported one.
    expect(said({ board: "running", task: sent("stopped_by_person") })).toBe("ring working");
    // What a task reports of itself is never one of these: its outcome is one of four words.
    expect(said({ board: "waiting", task: sent("cancelled") })).toBe("dash cancelled");
    // And both are over: a folded session counts them, and neither is at work.
    const kind = (task: TaskFacts) =>
      shownState({ board: "done", needsYou: false, task, harness: null })?.kind;
    expect(kind(inItsPlace("closed_by_person"))).toBe("closed-by-you");
    expect(kind(sent("stopped_by_person"))).toBe("stopped-by-you");
  });

  it("is told from a chat waiting on the person by word and by shape, with colour removed", () => {
    const waiting = shownState(of({ board: "waiting", needsYou: true }));
    // Its turn ended after its report: the board says `waiting` of it too, and the core keeps
    // it out of the queue.
    for (const outcome of ["done", "failed", "cancelled"]) {
      const finished = shownState(of({ board: "waiting", task: sent(outcome) }));
      expect(finished?.word).not.toBe(waiting?.word);
      expect(finished?.shape).not.toBe(waiting?.shape);
    }
  });
});

describe("a harness purlis has heard nothing from", () => {
  it("says the program is running and what is not known, by the harness's name", () => {
    expect(said({ board: "unknown", harness: "opencode" })).toBe(
      "dots running (no detail from opencode)",
    );
    expect(said({ board: "unknown", harness: "opencode", task: OWED })).toBe(
      "dots running (no detail from opencode)",
    );
  });

  it("names no harness where none is known", () => {
    expect(said({ board: "unknown", harness: null })).toBe("dots running (no detail)");
  });

  it("still says what the app itself knows", () => {
    expect(said({ board: "unknown", harness: "opencode", needsYou: true })).toBe("hand needs you");
    expect(said({ board: "unknown", harness: "opencode", task: inItsPlace("failed") })).toBe(
      "triangle ended without a report",
    );
  });
});

describe("a shell tab", () => {
  it("draws nothing until something reports a state for it", () => {
    expect(shownState(of({ board: undefined, harness: null }))).toBeUndefined();
  });
});

describe("every state", () => {
  const each: Partial<Facts>[] = [
    { board: "running" },
    { board: "waiting", needsYou: true },
    { task: { ...OWED, asking: "steward 4" } },
    { board: "waiting", task: sent("done") },
    { board: "waiting", task: sent("failed") },
    { board: "waiting", task: sent("cancelled") },
    { task: inItsPlace("failed") },
    { board: "unknown" },
    { board: "waiting" },
    { board: "waiting", task: sent(null) },
  ];

  it("has a word and a shape of its own", () => {
    const shown = each.map((facts) => shownState(of(facts)));
    expect(new Set(shown.map((one) => one?.shape)).size).toBe(each.length);
    expect(new Set(shown.map((one) => one?.word)).size).toBe(each.length);
    expect(new Set(shown.map((one) => one?.kind)).size).toBe(each.length);
  });

  it("is coloured by a token of the theme, and finished ones are the muted grey", () => {
    for (const facts of each) expect(TOKENS).toContain(shownState(of(facts))?.token);
    expect(shownState(of(each[3]))?.token).toBe("text.muted");
    expect(shownState(of(each[5]))?.token).toBe("text.muted");
  });
});

describe("what a task's record says of it", () => {
  const chat = (from: OpenChat["from"]): Pick<OpenChat, "from"> => ({ from });
  const from = {
    chat: 4,
    name: "steward 4",
    workspace: "alpha",
    task: true,
    tab: false,
    reported: false,
    unreported: false,
  };

  it("is nothing for a chat the person started, and for a handoff", () => {
    expect(taskFactsOf(chat(null))).toBeNull();
    expect(taskFactsOf(chat({ ...from, task: false, tab: true }))).toBeNull();
  });

  it("is a report owed, sent with its outcome, or written in its place with the record's word", () => {
    expect(taskFactsOf(chat(from))).toEqual(OWED);
    expect(taskFactsOf(chat({ ...from, reported: true, outcome: "cancelled" }))).toEqual(
      sent("cancelled"),
    );
    expect(taskFactsOf(chat({ ...from, unreported: true, outcome: "stopped" }))).toEqual(
      inItsPlace("stopped"),
    );
    expect(taskFactsOf(chat({ ...from, unreported: true }))).toEqual(inItsPlace(null));
  });

  it("names the chat it is asking as that chat is called now, so a rename is followed", () => {
    const asking = chat({ ...from, asking: true });
    const renamed = (session: number) => (session === 4 ? "the release" : undefined);
    expect(taskFactsOf(asking, renamed)?.asking).toBe("the release");
    expect(rowFactsOf({ ...asking, harness: "claude", card: null }, renamed).asking).toBe(
      "the release",
    );
  });

  it("falls back to the name that chat had at the dispatch once it has closed", () => {
    const asking = chat({ ...from, asking: true });
    expect(taskFactsOf(asking, () => undefined)?.asking).toBe("steward 4");
    expect(taskFactsOf(asking)?.asking).toBe("steward 4");
  });
});
