import { describe, expect, it } from "vitest";
import type { OpenChat } from "./bindings";
import { shownState, taskFactsOf, type Facts, type TaskFacts } from "./shownState";
import { TOKENS } from "./theme/theme";

/**
 * **What a chat's row says it is doing** (#1484): one word and one shape, derived in one place
 * from the board's state, the needs-you queue and a task's own record.
 */

const OWED: TaskFacts = { report: "owed", outcome: null, asking: null };

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

describe("a task", () => {
  it("is working, and needs you, as any chat", () => {
    expect(said({ task: OWED })).toBe("ring working");
    expect(said({ task: OWED, board: "waiting", needsYou: true })).toBe("hand needs you");
  });

  it("is asking the chat that dispatched it, by that chat's name, while its question is open", () => {
    expect(said({ task: { ...OWED, asking: "steward 4" } })).toBe("question asking steward 4");
    // Its turn has ended on the question: it waits on that chat, and not on the person.
    expect(said({ task: { ...OWED, asking: "steward 4" }, board: "waiting", needsYou: true })).toBe(
      "question asking steward 4",
    );
  });

  it("says how it reported, whatever its program is doing since", () => {
    for (const board of ["running", "waiting", "done", "failed", "unknown"] as const) {
      const sent = (outcome: string): Partial<Facts> => ({
        board,
        needsYou: board === "waiting",
        task: { report: "sent", outcome, asking: null },
      });
      expect(said(sent("done"))).toBe("tick done");
      expect(said(sent("failed"))).toBe("cross failed");
      expect(said(sent("cancelled"))).toBe("dash cancelled");
    }
  });

  it("reads failed when it reported that it could not go on", () => {
    expect(said({ task: { report: "sent", outcome: "blocked", asking: null } })).toBe(
      "cross failed",
    );
  });

  it("reads cancelled when the person stopped it and it reported on its way out", () => {
    expect(said({ task: { report: "sent", outcome: "stopped", asking: null } })).toBe(
      "dash cancelled",
    );
  });

  it("says only that it reported when how is not known, and guesses no outcome", () => {
    expect(said({ task: { report: "sent", outcome: null, asking: null } })).toBe("dot reported");
    expect(said({ task: { report: "sent", outcome: "a word of later", asking: null } })).toBe(
      "dot reported",
    );
    expect(said({ task: { report: "sent", outcome: "constructor", asking: null } })).toBe(
      "dot reported",
    );
  });

  it("ended without a report: by its record, or by its program ending while it owed one", () => {
    expect(said({ task: { report: "failed", outcome: null, asking: null } })).toBe(
      "slash ended without a report",
    );
    expect(said({ task: OWED, board: "done" })).toBe("slash ended without a report");
    expect(said({ task: OWED, board: "failed" })).toBe("slash ended without a report");
  });

  it("is told from a chat waiting on the person by word and by shape, with colour removed", () => {
    const waiting = shownState(of({ board: "waiting", needsYou: true }));
    // Its turn ended after its report, so the board says of it what it says of a waiting chat.
    for (const outcome of ["done", "failed", "cancelled"]) {
      const finished = shownState(
        of({ board: "waiting", needsYou: true, task: { report: "sent", outcome, asking: null } }),
      );
      expect(finished?.word).not.toBe(waiting?.word);
      expect(finished?.shape).not.toBe(waiting?.shape);
    }
  });
});

describe("a harness purlis has heard nothing from", () => {
  it("says the program is running and what is not known, by the harness's name", () => {
    expect(said({ board: "unknown", harness: "opencode" })).toBe(
      "broken-ring running (no detail from opencode)",
    );
    expect(said({ board: "unknown", harness: "opencode", task: OWED })).toBe(
      "broken-ring running (no detail from opencode)",
    );
  });

  it("names no harness where none is known", () => {
    expect(said({ board: "unknown", harness: null })).toBe("broken-ring running (no detail)");
  });

  it("still says what the app itself knows", () => {
    expect(said({ board: "unknown", harness: "opencode", needsYou: true })).toBe("hand needs you");
    expect(
      said({
        board: "unknown",
        harness: "opencode",
        task: { report: "failed", outcome: null, asking: null },
      }),
    ).toBe("slash ended without a report");
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
    { task: { report: "sent", outcome: "done", asking: null } },
    { task: { report: "sent", outcome: "failed", asking: null } },
    { task: { report: "sent", outcome: "cancelled", asking: null } },
    { task: { report: "failed", outcome: null, asking: null } },
    { board: "unknown" },
    { board: "waiting" },
  ];

  it("has a shape of its own", () => {
    const shapes = each.map((facts) => shownState(of(facts))?.shape);
    expect(new Set(shapes).size).toBe(each.length);
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

  it("is a report owed, sent with its outcome, or failed", () => {
    expect(taskFactsOf(chat(from))).toEqual(OWED);
    expect(taskFactsOf(chat({ ...from, reported: true, outcome: "cancelled" }))).toEqual({
      report: "sent",
      outcome: "cancelled",
      asking: null,
    });
    expect(taskFactsOf(chat({ ...from, unreported: true }))).toEqual({
      report: "failed",
      outcome: null,
      asking: null,
    });
  });

  it("names the chat it is asking while its question is open", () => {
    expect(taskFactsOf(chat({ ...from, asking: true }))?.asking).toBe("steward 4");
  });
});
