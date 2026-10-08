import { describe, expect, it } from "vitest";
import type { ChatBlocked } from "./bindings";
import type { ListedChat } from "./chatsTree";
import { listed, taskBlockGroups, whoseOf, withoutGrouped } from "./taskAsks";

/** A chat as the core lists it: `parent` asked for it as a task, where there is one. */
function listedChat(session: number, name: string, parent: number | null = null): ListedChat {
  return {
    session,
    name,
    persona: "steward",
    workspace: "alpha",
    shell: false,
    parent,
    mode: parent === null ? null : "task",
    from: null,
    tab: parent === null,
    branch: null,
  } as ListedChat;
}

/** steward 4 asked for talk and sweep; talk asked for deep. */
const steward = listedChat(4, "steward 4");
const talk = listedChat(7, "talk", 4);
const sweep = listedChat(8, "sweep", 4);
const deep = listedChat(9, "deep", 7);
const chats = [steward, talk, sweep, deep];

function hostBlock(session: number, host: string | null = "registry.npmjs.org"): ChatBlocked {
  return {
    plane: "/plane",
    session,
    operation: "connect",
    kind: "host",
    ours: false,
    harness: "claude",
    said: "a connection to a host the project does not list",
    offer: "host",
    target: host,
    route: null,
    levels: ["chat", "you", "project"],
  };
}

describe("whose a task's question is", () => {
  it("names the session for its own, and the whole path for a task at any depth", () => {
    expect(whoseOf(steward, steward, chats)).toBe("steward 4");
    expect(whoseOf(talk, steward, chats)).toBe("talk (a task of steward 4)");
    expect(whoseOf(deep, steward, chats)).toBe("deep (a task of steward 4 › talk)");
  });

  it("guesses no link the core's list does not hold", () => {
    const orphan = listedChat(9, "deep", 70);
    expect(whoseOf(orphan, steward, [steward, orphan])).toBe("deep (a task of steward 4)");
    // A handoff is no task: the path stops there.
    const handed = { ...listedChat(9, "deep", 7), mode: "handoff" as const };
    expect(whoseOf(handed, steward, [steward, talk, handed])).toBe("deep (a task of steward 4)");
  });
});

describe("one question for tasks hitting the same block", () => {
  const tasks = [talk, sweep, deep].map((one) => ({
    session: one.session,
    whose: whoseOf(one, steward, chats),
  }));

  it("asks once for three tasks blocked on one host, and asks each other host on its own", () => {
    const blocks = {
      7: [hostBlock(7)],
      8: [hostBlock(8), hostBlock(8, "api.example.com")],
      9: [hostBlock(9)],
    };
    const groups = taskBlockGroups(blocks, 4, tasks);
    expect(groups).toHaveLength(1);
    expect(groups[0].target).toBe("registry.npmjs.org");
    expect(groups[0].members.map((one) => one.session)).toEqual([7, 8, 9]);
    expect(groups[0].members.map((one) => one.whose)).toEqual([
      "talk (a task of steward 4)",
      "sweep (a task of steward 4)",
      "deep (a task of steward 4 › talk)",
    ]);
    // Each is asked once: the grouped ones are taken from the tasks' own Notices.
    expect(withoutGrouped(blocks, groups)).toEqual({ 8: [hostBlock(8, "api.example.com")] });
  });

  it("never puts the session's own block in a group, and a group of one is no group", () => {
    const blocks = { 4: [hostBlock(4)], 7: [hostBlock(7)] };
    expect(taskBlockGroups(blocks, 4, [{ session: 4, whose: "steward 4" }, ...tasks])).toEqual([]);
  });

  it("groups no block that names nothing to allow", () => {
    const unnamed = { 7: [hostBlock(7, null)], 8: [hostBlock(8, null)] };
    expect(taskBlockGroups(unnamed, 4, tasks)).toEqual([]);
    const ours = { 7: [{ ...hostBlock(7), ours: true }], 8: [{ ...hostBlock(8), ours: true }] };
    expect(taskBlockGroups(ours, 4, tasks)).toEqual([]);
  });

  it("offers only the levels every task's block may be allowed at", () => {
    const blocks = {
      7: [hostBlock(7)],
      8: [{ ...hostBlock(8), levels: ["chat" as const, "you" as const] }],
    };
    expect(taskBlockGroups(blocks, 4, tasks)[0].levels).toEqual(["chat", "you"]);
  });
});

describe("a list of names", () => {
  it("reads as a sentence does", () => {
    expect(listed(["talk"])).toBe("talk");
    expect(listed(["talk", "sweep"])).toBe("talk and sweep");
    expect(listed(["talk", "sweep", "probe"])).toBe("talk, sweep and probe");
  });
});
