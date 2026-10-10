import { describe, expect, it } from "vitest";
import { moved, nothingKnown } from "./chatState";
import { helpersCountOf, helpersCountSaid, helpersSaid } from "./explorerTasks";

describe("a chat's helpers", () => {
  it("are counted by how they stand: an ended one stays in the total and out of the working", () => {
    const states = moved(nothingKnown, {
      plane: "/p",
      session: 1,
      state: "running",
      needs_you: false,
      queue: [],
      moved_at: 1,
      reports: [],
      refusals: [],
      sequence: 1,
      children: [
        { agent: "a", state: "running" },
        { agent: "b", state: "done" },
        { agent: "c", state: "failed" },
        { agent: "d", state: "done" },
      ],
    });

    const count = helpersCountOf(states, 1);

    expect(count).toEqual({ total: 4, working: 1, failed: 1 });
    expect(helpersCountSaid(count)).toBe("4 helpers · 1 working · 1 failed");
    expect(helpersCountSaid({ total: 40, working: 0, failed: 0 })).toBe("40 helpers");
    expect(helpersCountOf(states, 2)).toEqual({ total: 0, working: 0, failed: 0 });
  });

  it("are counted in the singular and the plural", () => {
    expect(helpersSaid(1)).toBe("1 helper");
    expect(helpersSaid(3)).toBe("3 helpers");
  });

  it("count one the harness says no known word of as working, and an idle one as neither", () => {
    const states = moved(nothingKnown, {
      plane: "/p",
      session: 1,
      state: "running",
      needs_you: false,
      queue: [],
      moved_at: 1,
      reports: [],
      refusals: [],
      sequence: 1,
      children: [
        { agent: "a", state: "paused" },
        { agent: "b", state: "" },
        { agent: "c", state: "waiting" },
      ],
    });

    expect(helpersCountOf(states, 1)).toEqual({ total: 3, working: 2, failed: 0 });
  });
});
