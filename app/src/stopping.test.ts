import { describe, expect, it } from "vitest";
import {
  stepped,
  stopAllAnswer,
  stopAllSays,
  stopAllTitle,
  stopAnswer,
  stopSays,
  stopTitle,
  type StopAsked,
} from "./stopping";

const PLANE = "/home/dev/plane";

describe("the chats being stopped (#1448)", () => {
  it("holds a chat while it is stopping, and lets go of it when it has ended", () => {
    const stopping = stepped(new Set(), { plane: PLANE, session: 2, phase: "stopping" });
    expect([...stopping]).toEqual([2]);

    expect([...stepped(stopping, { plane: PLANE, session: 2, phase: "stopped" })]).toEqual([]);
  });

  it("takes a chat that ended at once, with no step before it", () => {
    expect([...stepped(new Set([3]), { plane: PLANE, session: 2, phase: "stopped" })]).toEqual([3]);
  });
});

describe("what a stop asks first (#1448)", () => {
  const asked = (over: Partial<StopAsked> = {}): StopAsked => ({
    name: "drop commons",
    below: false,
    under: 0,
    dispatched: true,
    already: false,
    waitingOn: 0,
    ...over,
  });

  it("says a chat another chat started gets a last turn, and that its asker is told", () => {
    // The question names the act as the row that asked it does.
    expect(stopTitle(asked())).toBe("Stop chat drop commons?");
    expect(stopSays(asked())).toBe(
      "drop commons gets one short turn to write what it did, then it ends. The chat that asked is told you stopped it. There is no undo.",
    );
    expect(stopAnswer(asked())).toBe("Stop chat");
  });

  it("says a chat you started yourself ends, and promises it no last turn", () => {
    expect(stopSays(asked({ dispatched: false }))).toBe("drop commons ends. There is no undo.");
  });

  it("counts what is below, in digits, with plurals that agree", () => {
    const one = asked({ below: true, under: 1 });
    expect(stopTitle(one)).toBe("Stop chat drop commons and everything below it?");
    expect(stopSays(one)).toContain("drop commons and the 1 chat below it end, deepest first.");
    // Which comes first is the turn, and it can be read no other way.
    expect(stopSays(one)).toContain(
      "A chat that another chat started gets one short turn first, to write what it did,",
    );
    expect(stopSays(one)).not.toContain("started first");
    expect(stopSays(one)).toContain("Nothing outside them is touched. There is no undo.");
    expect(stopAnswer(one)).toBe("Stop 2 chats");

    const three = asked({ below: true, under: 3 });
    expect(stopSays(three)).toContain("and the 3 chats below it end");
    expect(stopAnswer(three)).toBe("Stop 4 chats");
  });

  it("says a second press ends a chat that is still writing, without waiting", () => {
    const again = asked({ already: true });
    expect(stopTitle(again)).toBe("End chat drop commons now?");
    expect(stopSays(again)).toBe(
      "drop commons is being stopped, and has one short turn to write what it did. This ends it without waiting for that turn. There is no undo.",
    );
    expect(stopAnswer(again)).toBe("End chat");
  });

  it("says a chat held for the chats below it is waiting for them, not writing", () => {
    const held = asked({ already: true, under: 2, waitingOn: 2 });
    expect(stopTitle(held)).toBe("End chat drop commons now?");
    expect(stopSays(held)).toBe(
      "drop commons is being stopped, and is waiting for the 2 chats below it to end. This ends it now, without waiting for them. There is no undo.",
    );
    expect(stopSays(asked({ already: true, under: 3, waitingOn: 1 }))).toContain(
      "is waiting for the 1 chat below it to end.",
    );
    expect(stopSays(held)).not.toContain("writ");
  });
});

describe("everything below, asked of a chat that is already stopping (#1448)", () => {
  const asked = (under: number): StopAsked => ({
    name: "drop commons",
    below: true,
    under,
    dispatched: true,
    already: true,
    waitingOn: 0,
  });

  it("stops the chats below it in the ordinary way, and ends nothing early", () => {
    expect(stopTitle(asked(2))).toBe("Stop chat drop commons and everything below it?");
    expect(stopSays(asked(2))).toBe(
      "drop commons is being stopped already. The 2 chats below it are stopped too, deepest first: each gets one short turn to write what it did, and the chat that asked is told you stopped it. Nothing outside them is touched. There is no undo.",
    );
    // It counts the chats the answer adds, not the one already stopping.
    expect(stopAnswer(asked(2))).toBe("Stop 2 chats");
    expect(stopSays(asked(1))).toContain("The 1 chat below it is stopped too");
    expect(stopAnswer(asked(1))).toBe("Stop 1 chat");
  });
});

describe("the question Stop all tasks asks (#1498)", () => {
  it("names how many tasks end, and says the session keeps running", () => {
    const two = { name: "steward 1", tasks: [3, 2] };

    expect(stopAllTitle(two)).toBe("Stop all 2 tasks of steward 1?");
    expect(stopAllSays(two)).toBe(
      "The 2 tasks at work below steward 1 end, deepest first. Each gets one short turn to say what it did, where it can be given one, and the chat that asked is told you stopped it. steward 1 keeps running. There is no undo.",
    );
    expect(stopAllAnswer(two)).toBe("Stop 2 tasks");
  });

  it("says one task as one", () => {
    const one = { name: "steward 1", tasks: [2] };

    expect(stopAllTitle(one)).toBe("Stop the 1 task of steward 1?");
    expect(stopAllSays(one)).toContain("The 1 task at work below steward 1 ends.");
    expect(stopAllAnswer(one)).toBe("Stop 1 task");
  });
});
