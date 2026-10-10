import { describe, expect, it } from "vitest";
import { catalogue, withQueue, type Now } from "./actions";
import type { ListedChat } from "./chatsTree";
import { noTabs, openTab } from "./tabs";

/** A window with three chats in tabs, one of them stopped by its Smart close. */
function now(over: Partial<Now> = {}): Now {
  const tabs = openTab(openTab(openTab(noTabs(), 1, "one"), 2, "two"), 3, "three");
  return {
    tabs,
    workspaces: ["alpha"],
    needsYou: [],
    nameOf: (session) => `chat ${session}`,
    quiet: ["codex 9"],
    stopped: { 2: "the record was not written" },
    reportsTo: (session) => (session === 1 ? ["helper"] : []),
    neededFor: (session) => (session === 3 ? ["a vault"] : []),
    ...over,
  };
}

/** A task with no tab, which the Chats list holds. */
const task: ListedChat = {
  session: 7,
  name: "chat 7",
  persona: null,
  workspace: "alpha",
  shell: false,
  parent: 1,
  mode: "task",
  from: "chat 1",
  tab: false,
  branch: null,
  report: null,
  outcome: null,
  asking: null,
  harness: null,
};

/** What the view builds once: the catalogue with no queue in it. */
const queueless = (window: Now) => catalogue({ ...window, needsYou: [], stopped: undefined });

describe("the catalogue with the queue put in", () => {
  it.each([
    ["no chat asking", []],
    ["one chat asking", [3]],
    ["the stopped chat asking", [2, 1]],
    ["a task with no tab asking", [7, 3]],
  ])("is the catalogue built with the queue, with %s", (_, queue) => {
    const window = now({ listed: [task] });
    const built = catalogue({ ...window, needsYou: queue });
    // Built without the queue, the catalogue has none of its rows, nor the stopped chat's.
    const ids = queueless(window).map((offer) => offer.id);
    expect(ids.filter((id) => /^needs\.(show|ignore|dismiss):/.test(id))).toEqual([]);

    expect(withQueue(queueless(window), queue, window)).toEqual(built);
  });

  it("hands a catalogue that was not built back as it is", () => {
    const none: never[] = [];
    expect(withQueue(none, [1], now())).toBe(none);
  });
});
