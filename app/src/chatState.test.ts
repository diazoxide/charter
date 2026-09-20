import { describe, expect, it } from "vitest";

import { moved, nothingKnown, quietOnes, stateOf, underneath } from "./chatState";
import type { Moved, OpenChat } from "./bindings";

function doing(session: number, state: string, queue: number[] = []): Moved {
  return { session, state, needs_you: queue.includes(session), queue };
}

describe("what the window keeps about the chats", () => {
  it("knows nothing about a chat it has not heard of", () => {
    expect(stateOf(nothingKnown, 7)).toBe("unknown");
    expect(nothingKnown.needsYou).toEqual([]);
  });

  it("takes the whole queue from every event rather than assembling it", () => {
    // A window that built the queue from edges would keep a chat in it, or out of it, for as
    // long as the app ran if it ever missed one.
    const after = moved(moved(nothingKnown, doing(7, "waiting", [7])), doing(9, "waiting", [7, 9]));

    expect(after.needsYou).toEqual([7, 9]);
  });

  it("does not let the first answer overwrite an event that beat it", () => {
    // **A defect a review reproduced.** `chatStates()` is asked once at startup, and a hook
    // can fire while the question is in flight. Folding the older answer on top dropped the
    // chat back to `running` and out of the needs-you queue — and a chat that is waiting for
    // you has no next event to correct it with.
    const arrived = moved(nothingKnown, doing(7, "waiting", [7]));

    const settled = underneath(arrived, [doing(7, "running", [])]);

    expect(stateOf(settled, 7)).toBe("waiting");
    expect(settled.needsYou).toEqual([7]);
  });

  it("still takes the first answer for a chat no event has mentioned", () => {
    const arrived = moved(nothingKnown, doing(7, "waiting", [7]));

    const settled = underneath(arrived, [doing(9, "running", [])]);

    expect(stateOf(settled, 9)).toBe("running");
    expect(stateOf(settled, 7)).toBe("waiting");
  });

  it("takes the first answer whole when nothing has arrived yet", () => {
    const settled = underneath(nothingKnown, [doing(7, "waiting", [7]), doing(9, "running", [7])]);

    expect(stateOf(settled, 7)).toBe("waiting");
    expect(settled.needsYou).toEqual([7]);
  });
});

/** A chat as the sidebar has it, with only what `quietOnes` reads worth setting. */
function open(session: number, unreported: string | null): OpenChat {
  return {
    session,
    name: `ide.${session}`,
    cwd: null,
    harness: unreported ? "codex" : "claude",
    in_front: false,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported,
  };
}

describe("the chats that can be waiting on you without saying so", () => {
  const CODEX = "it never says when it stops mid-turn for your approval";

  it("names a chat whose harness cannot report everything, and only that one", () => {
    // #27: a Codex chat asking for approval mid-turn says nothing, so an empty queue beside
    // it is not the app knowing nothing needs you.
    expect(quietOnes([open(7, CODEX), open(8, null)], nothingKnown)).toEqual(["ide.7"]);
  });

  it("leaves out one already in the queue, which is named there", () => {
    const states = moved(nothingKnown, doing(7, "waiting", [7]));

    expect(quietOnes([open(7, CODEX), open(9, CODEX)], states)).toEqual(["ide.9"]);
  });

  it("leaves out one whose program has ended, which cannot be waiting on anybody", () => {
    const states = moved(moved(nothingKnown, doing(7, "done")), doing(8, "failed"));

    expect(quietOnes([open(7, CODEX), open(8, CODEX), open(9, CODEX)], states)).toEqual(["ide.9"]);
  });

  it("keeps one that is running, because running is what a chat waiting on approval looks like", () => {
    const states = moved(nothingKnown, doing(7, "running"));

    expect(quietOnes([open(7, CODEX)], states)).toEqual(["ide.7"]);
  });
});
