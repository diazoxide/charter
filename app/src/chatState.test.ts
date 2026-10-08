import { describe, expect, it } from "vitest";

import {
  backSaid,
  failedTasksOf,
  childrenOf,
  moved,
  movedAt,
  needSays,
  taskFailedSaid,
  needsOf,
  nothingKnown,
  quietOnes,
  refusalsOf,
  reportsTo,
  stateOf,
  underneath,
} from "./chatState";
import type { ChildAgent, Moved, OpenChat } from "./bindings";

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
  refusals: string[] = [],
  children: ChildAgent[] = [],
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
    refusals,
    children,
  };
}

describe("a chat's child agents (FD-18)", () => {
  const working = (agent: string): ChildAgent => ({ agent, state: "running" });

  it("knows each chat's child agents from that chat's newest snapshot", () => {
    const spawned = moved(nothingKnown, doing(3, "running", [], 0, [], [], [working("a1")]));
    expect(childrenOf(spawned, 3)).toEqual([working("a1")]);
    expect(childrenOf(spawned, 4)).toEqual([]);

    const stopped = moved(
      spawned,
      doing(3, "failed", [], 0, [], [], [{ agent: "a1", state: "failed" }]),
    );
    expect(childrenOf(stopped, 3)).toEqual([{ agent: "a1", state: "failed" }]);
  });

  it("does not let an older snapshot bring back a child that has ended", () => {
    const older = doing(3, "running", [], 0, [], [], [working("a1")]);
    const newer = doing(3, "done", [], 0, [], [], [{ agent: "a1", state: "done" }]);

    expect(childrenOf(moved(moved(nothingKnown, newer), older), 3)).toEqual([
      { agent: "a1", state: "done" },
    ]);
  });

  it("keeps a chat's children the same object when a move says nothing new about them", () => {
    const before = moved(nothingKnown, doing(3, "running", [], 1, [], [], [working("a1")]));

    const after = moved(before, doing(3, "waiting", [3], 2, [], [], [working("a1")]));

    expect(after.children).toBe(before.children);
  });

  it("takes a move from a core older than the field as a chat with no children", () => {
    const before = moved(nothingKnown, doing(3, "running", [], 0, [], [], [working("a1")]));
    const bare = { ...doing(3, "running") } as Partial<Moved>;
    delete bare.children;

    expect(childrenOf(moved(before, bare as Moved), 3)).toEqual([]);
  });
});

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
    card: null,
    guessed: null,
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

describe("a refused commit (SQ-16)", () => {
  it("knows what a chat's commits were refused for, and forgets it with its next snapshot", () => {
    const said = "commit refused in app: a.py:2  an email address  ad**";
    const refused = moved(nothingKnown, doing(3, "running", [3], 0, [], [said]));
    expect(refusalsOf(refused, 3)).toEqual([said]);
    expect(refusalsOf(refused, 4)).toEqual([]);

    const prompted = moved(refused, doing(3, "running", [], 0, [], []));
    expect(refusalsOf(prompted, 3)).toEqual([]);
  });
});

/**
 * **What a move did not change is the same object afterwards** (SC-3). The queue and every
 * chat's reports travel whole on every move, so a reader that compares what it drew from by
 * identity — a row, the needs-you list, the catalogue — would otherwise be told that all of it
 * changed every time any chat did anything.
 */
describe("a move keeps what it did not change", () => {
  it("keeps the queue when the move carries the same one", () => {
    const before = moved(nothingKnown, doing(3, "waiting", [3]));

    const after = moved(before, doing(4, "running", [3]));

    expect(after.needsYou).toBe(before.needsYou);
  });

  it("takes a queue that did change", () => {
    const before = moved(nothingKnown, doing(3, "waiting", [3]));

    const after = moved(before, doing(4, "waiting", [3, 4]));

    expect(after.needsYou).toEqual([3, 4]);
  });

  it("keeps every chat's reports and refusals when the move says nothing new about them", () => {
    const before = moved(nothingKnown, doing(3, "waiting", [3], 1, ["drop commons"], ["no key"]));

    const after = moved(before, doing(3, "running", [], 2, ["drop commons"], ["no key"]));

    expect(after.reports).toBe(before.reports);
    expect(after.refusals).toBe(before.refusals);
    expect(stateOf(after, 3)).toBe("running");
  });

  it("still takes a move that carries no queue, as an empty one", () => {
    // A core older than the field, or a test's hand-written event: the chat's own state is
    // still news, and an absent queue must not throw it away.
    const before = moved(nothingKnown, doing(3, "waiting", [3]));
    const bare = { ...doing(3, "running") } as Partial<Moved>;
    delete bare.queue;

    const after = moved(before, bare as Moved);

    expect(stateOf(after, 3)).toBe("running");
    expect(after.needsYou).toEqual([]);
  });

  it("takes reports that did change", () => {
    const before = moved(nothingKnown, doing(3, "waiting", [3], 1, ["drop commons"]));

    const after = moved(before, doing(3, "running", [], 2, []));

    expect(reportsTo(after, 3)).toEqual([]);
  });
});

describe("what the app found a chat needs you for (#1448)", () => {
  const undelivered = { kind: "report_undelivered" as const, asker: "steward 3" };

  it("says a report has nowhere to go, and whose it was", () => {
    expect(needSays(undelivered)).toBe(
      "its report has nowhere to go because steward 3 has closed or its program has ended",
    );
  });

  it("keeps it by chat, as the sentence its item says, until the chat's next snapshot", () => {
    const needed = moved(nothingKnown, { ...doing(3, "running", [3]), needs: [undelivered] });
    expect(needsOf(needed, 3)).toEqual([
      "its report has nowhere to go because steward 3 has closed or its program has ended",
    ]);
    expect(needsOf(needed, 4)).toEqual([]);

    // Its next prompt: the core sends none, as it does for nearly every chat.
    expect(needsOf(moved(needed, { ...doing(3, "running", []), needs: null }), 3)).toEqual([]);
    expect(needsOf(moved(needed, doing(3, "running", [])), 3)).toEqual([]);
  });

  it("keeps the same list when a move says the same thing", () => {
    const needed = moved(nothingKnown, { ...doing(3, "running", [3]), needs: [undelivered] });
    const again = moved(needed, { ...doing(3, "waiting", [3]), needs: [undelivered] });

    expect(again.needs).toBe(needed.needs);
  });
});

describe("a task that came to nothing, on the chat that asked for it (#1491)", () => {
  const failed = {
    kind: "task_failed" as const,
    id: "01K6PROD",
    chat: 9,
    task: "check prod",
    how: "failed" as const,
    why: "the forge refused the push",
  };
  const unreported = {
    kind: "task_failed" as const,
    id: "01K6LINT",
    task: "lint",
    how: "unreported" as const,
    why: "",
  };

  it("says which task and why, in a few words", () => {
    expect(needSays(failed)).toBe("check prod failed: the forge refused the push");
    expect(needSays(unreported)).toBe("lint ended without a report");
    expect(needSays({ ...failed, how: "did_not_start", why: "no profile runs devops" })).toBe(
      "check prod did not start: no profile runs devops",
    );
    // Nothing said why.
    expect(needSays({ ...failed, why: "" })).toBe("check prod failed");
    expect(taskFailedSaid({ task: "build", how: "did_not_start", why: "" })).toBe(
      "build did not start",
    );
  });

  it("keeps which tasks they were by the chat that asked, so its item can go to their rows", () => {
    const told = moved(nothingKnown, { ...doing(3, "running", [3]), needs: [failed, unreported] });
    // By the id its finished row carries, and its chat while that is open.
    expect(failedTasksOf(told, 3)).toEqual([
      { id: "01K6PROD", task: "check prod", chat: 9 },
      { id: "01K6LINT", task: "lint", chat: null },
    ]);
    // The same list when a move says the same thing.
    const again = moved(told, { ...doing(3, "waiting", [3]), needs: [failed, unreported] });
    expect(again.failedTasks).toBe(told.failedTasks);
    expect(needsOf(told, 3)).toEqual([
      "check prod failed: the forge refused the push",
      "lint ended without a report",
    ]);
    expect(failedTasksOf(told, 4)).toEqual([]);
  });

  it("is not a task that failed when the chat needs you for something else", () => {
    const undelivered = { kind: "report_undelivered" as const, asker: "steward 3" };
    const told = moved(nothingKnown, { ...doing(3, "running", [3]), needs: [undelivered] });
    expect(failedTasksOf(told, 3)).toEqual([]);
  });

  it("goes when the core says the person has looked", () => {
    const told = moved(nothingKnown, { ...doing(3, "running", [3]), needs: [failed] });
    const seen = moved(told, { ...doing(3, "running", []), needs: null });
    expect(failedTasksOf(seen, 3)).toEqual([]);
    expect(seen.needsYou).toEqual([]);
  });
});

describe("what the chats a chat started have done (#1448)", () => {
  it("says who reported back, who was stopped, and both, each in its own words", () => {
    expect(backSaid([], [])).toBeUndefined();
    expect(backSaid(["drop commons"])).toBe("drop commons reported back");
    expect(backSaid([], ["retry hooks"])).toBe("retry hooks was stopped");
    expect(backSaid([], ["retry hooks", "lint"])).toBe("retry hooks, lint were stopped");
    expect(backSaid(["drop commons"], ["retry hooks"])).toBe(
      "drop commons reported back; retry hooks was stopped",
    );
  });

  it("keeps a stopped chat apart from a report, by the chat that started it", () => {
    const told = moved(nothingKnown, { ...doing(3, "running"), stopped: ["retry hooks"] });

    expect(told.stoppedBelow[3]).toEqual(["retry hooks"]);
    expect(reportsTo(told, 3)).toEqual([]);
    // Its next prompt reads it: the core sends none.
    expect(moved(told, doing(3, "running")).stoppedBelow[3]).toEqual([]);
    expect(moved(told, { ...doing(3, "running"), stopped: null }).stoppedBelow[3]).toEqual([]);
  });
});
