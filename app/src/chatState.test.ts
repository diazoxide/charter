import { describe, expect, it } from "vitest";

import {
  moved,
  movedAt,
  nothingKnown,
  quietOnes,
  reportsTo,
  stateOf,
  underneath,
} from "./chatState";
import type { Moved, OpenChat } from "./bindings";

/** One plane, because these are about the reducer and not about telling planes apart. */
const PLANE = "/home/dev/plane";

/** How many snapshots the "board" below has taken. Each `doing` is the next one. */
let taken = 0;

/**
 * One snapshot, numbered after every snapshot built before it — the order the core's board
 * numbers them in (`Moved.sequence`). So the order a test BUILDS them in is the order the
 * board read them in, and the order it folds them in is the order they reached the window.
 */
function doing(
  session: number,
  state: string,
  queue: number[] = [],
  movedAt = 0,
  reports: string[] = [],
): Moved {
  taken += 1;
  return {
    plane: PLANE,
    session,
    state,
    needs_you: queue.includes(session),
    queue,
    moved_at: movedAt,
    sequence: taken,
    reports,
  };
}

describe("a report back (charter-app#259)", () => {
  it("knows which chats reported back to a chat, and forgets them with its next snapshot", () => {
    const reported = moved(nothingKnown, doing(3, "running", [3], 0, ["drop commons"]));
    expect(reportsTo(reported, 3)).toEqual(["drop commons"]);
    expect(reportsTo(reported, 4)).toEqual([]);

    const prompted = moved(reported, doing(3, "running", [], 0, []));
    expect(reportsTo(prompted, 3)).toEqual([]);
  });

  it("does not let an older snapshot bring a read report back", () => {
    const older = doing(3, "running", [3], 0, ["drop commons"]);
    const newer = doing(3, "running", [], 0, []);

    expect(reportsTo(moved(moved(nothingKnown, newer), older), 3)).toEqual([]);
  });
});

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
    const answer = [doing(7, "running", [])];
    const arrived = moved(nothingKnown, doing(7, "waiting", [7]));

    const settled = underneath(arrived, answer);

    expect(stateOf(settled, 7)).toBe("waiting");
    expect(settled.needsYou).toEqual([7]);
  });

  it("still takes the first answer for a chat no event has mentioned", () => {
    const answer = [doing(9, "running", [])];
    const arrived = moved(nothingKnown, doing(7, "waiting", [7]));

    const settled = underneath(arrived, answer);

    expect(stateOf(settled, 9)).toBe("running");
    expect(stateOf(settled, 7)).toBe("waiting");
  });

  // **Snapshots reach the window in any order (charter-app#248).** Each is numbered under the
  // board's lock but sent after it is let go, on the thread that built it, so an older one can
  // land after a newer one. The window keeps the newer.

  it("keeps the newer queue when two snapshots land out of order", () => {
    const older = doing(7, "waiting", [7]);
    const newer = doing(7, "running", []);

    const after = moved(moved(nothingKnown, newer), older);

    expect(after.needsYou).toEqual([]);
    expect(stateOf(after, 7)).toBe("running");
  });

  it("keeps a closed chat out of the queue when the report before the close lands after it", () => {
    // #256 narrowed this and could not close it: a hook's report taken just before a close is
    // sent on the socket's thread, unordered against the close's. The close is the newer fact.
    const report = doing(7, "waiting", [7, 9]);
    const close = doing(7, "unknown", [9]);

    const after = moved(moved(nothingKnown, close), report);

    expect(after.needsYou).toEqual([9]);
    expect(stateOf(after, 7)).toBe("unknown");
  });

  it("still takes an older snapshot's news about a chat nothing newer has mentioned", () => {
    // The queue is the whole board's and a newer one replaces it; a chat's state is only
    // that chat's, and the snapshot that lost the race for the queue is still the newest
    // word about the chat it was about.
    const about7 = doing(7, "running", [], 4);
    const about9 = doing(9, "waiting", [9], 5);

    const after = moved(moved(nothingKnown, about9), about7);

    expect(stateOf(after, 7)).toBe("running");
    expect(movedAt(after, 7)).toBe(4);
    expect(after.needsYou).toEqual([9]);
  });

  it("lowers the queue when a chat that asked is ignored", () => {
    // Ignore is the core's (`ignore_needs_you`), and what the window gets is the next
    // snapshot: the chat still waiting, and a queue without it.
    const asked = moved(nothingKnown, doing(7, "waiting", [7, 9]));

    const after = moved(asked, doing(7, "waiting", [9]));

    expect(after.needsYou).toEqual([9]);
    expect(stateOf(after, 7)).toBe("waiting");
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
    pinned: false,
    label: null,
    from: null,
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

describe("when each chat last moved", () => {
  it("knows nothing about a chat it has not heard of", () => {
    // `0` sorts last in the overflow menu, which is what "nothing has told me" should do.
    expect(movedAt(nothingKnown, 7)).toBe(0);
  });

  it("takes the count only for the chat the event is about", () => {
    // The count is per chat, unlike the queue, which travels whole. Folding it over the map
    // would stamp this chat's number onto every other and make every tab read as having
    // just moved — which is the overflow menu in an arbitrary order.
    const after = moved(
      moved(nothingKnown, doing(7, "running", [], 4)),
      doing(9, "running", [], 5),
    );

    expect(movedAt(after, 7)).toBe(4);
    expect(movedAt(after, 9)).toBe(5);
  });

  it("does not let the first answer overwrite a count an event already brought", () => {
    // The same race `bySession` has: `chatStates()` is asked once at startup and a hook can
    // fire while it is in flight. The older answer must not put a chat back down the menu.
    const answer = [doing(7, "running", [], 2), doing(8, "running", [], 3)];
    const heard = moved(nothingKnown, doing(7, "waiting", [7], 9));

    const after = underneath(heard, answer);

    expect(movedAt(after, 7)).toBe(9);
    expect(movedAt(after, 8)).toBe(3);
  });
});
