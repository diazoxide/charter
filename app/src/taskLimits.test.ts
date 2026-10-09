import { describe, expect, it } from "vitest";
import type { AtLimit } from "./bindings";
import type { ChatRow } from "./chatsTree";
import { runningByTab, runningSaid } from "./taskLimits";

/** The facts the footer reads of a tab's own session, at the top of its list. */
function own(
  tasksLimit: number | null,
  tasksRunning: number | null,
  level = 1,
  atLimit: AtLimit | null = null,
): ChatRow {
  return { session: 1, level, tasksLimit, tasksRunning, atLimit } as unknown as ChatRow;
}

function binds(row: string, own: boolean): AtLimit {
  return { limit: 1, row, said: "the sentence", own };
}

describe("the tab menu's footer (#1498)", () => {
  it("says how many tasks run against the limit in force for the session", () => {
    expect(runningSaid(own(6, 4))).toBe("4 of 6 running");
    expect(runningSaid(own(1, 1))).toBe("1 of 1 running");
  });

  it("says nothing where no task counts against the limit or the core gave no number", () => {
    expect(runningSaid(own(6, 0))).toBeUndefined();
    expect(runningSaid(own(null, 2))).toBeUndefined();
    expect(runningSaid(own(6, null))).toBeUndefined();
    expect(runningSaid(undefined)).toBeUndefined();
  });

  it("says another limit that binds after the count, and never the count's own twice (#1540)", () => {
    const chain = binds("at its chain's limit (16 live)", false);
    expect(runningSaid(own(6, 2, 1, chain))).toBe(
      "2 of 6 running · at its chain's limit (16 live)",
    );
    // With nothing of its own running, the limit that binds is still said.
    expect(runningSaid(own(6, 0, 1, chain))).toBe("at its chain's limit (16 live)");
    expect(runningSaid(own(null, null, 1, chain))).toBe("at its chain's limit (16 live)");
    // Its own running limit: `6 of 6 running` says it already.
    expect(runningSaid(own(6, 6, 1, binds("at its task limit (6)", true)))).toBe("6 of 6 running");
  });

  it("reads each tab's own session, never a task listed under it", () => {
    const tabs = new Map([
      [10, [own(6, 3), own(2, 2, 2)]],
      [11, [own(6, 0)]],
    ]);

    expect([...runningByTab(tabs)]).toEqual([[10, "3 of 6 running"]]);
  });
});
