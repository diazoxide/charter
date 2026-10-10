import { StrictMode } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { IN_ITS_CHAT, answerTaken, answerThrough, asksMoved, useAsks } from "./asks";
import type { Shown } from "./bindings";

/**
 * The asks registry's window side (#1690): each ask is answered through the command its own
 * source's Notice answers with, and nothing else; the list is read again whenever a source may
 * have moved, so an ask leaves it when its source stops waiting.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

/** Every command the window sent, with what it sent, answering each with `answers`. */
function sent(answers: Record<string, unknown> = {}) {
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args: args as Record<string, unknown> });
    if (cmd in answers) {
      const answer = answers[cmd];
      if (answer instanceof Error) throw answer.message;
      return answer;
    }
    return null;
  });
  return calls;
}

const PERMISSION: Shown = {
  session: 3,
  ask: "01J0000000000000000000000A",
  says: "Run cargo test",
  options: [
    { id: "allow", label: "Allow", allows: true },
    { id: "deny", label: "Deny", allows: false },
  ],
  source: "permission",
  chain: ["steward 3"],
  answer: { via: "hook" },
};

const DISPATCH: Shown = {
  session: 4,
  ask: "dispatch:7",
  says: "Wants to hand a task to devops",
  options: [
    { id: "chat", label: "Allow for this chat", allows: true },
    { id: "you", label: "Allow for me on this machine", allows: true },
    { id: "keep", label: "Keep blocked", allows: false },
    { id: "never", label: "Never for this pair", allows: false },
  ],
  source: "dispatch",
  chain: ["steward 4"],
  answer: { via: "dispatch", id: 7, shown: "digest-1" },
};

const HOST: Shown = {
  session: 5,
  ask: "block:5:connect:host:api.example.com",
  says: "The sandbox refused api.example.com",
  options: [
    { id: "chat", label: "Allow for this chat", allows: true },
    { id: "keep", label: "Keep blocked", allows: false },
  ],
  source: "sandbox-host",
  chain: ["steward 5"],
  answer: {
    via: "sandbox-block",
    shown: { operation: "connect", kind: "host", what: "host", target: "api.example.com" },
  },
};

const QUESTION: Shown = {
  session: 6,
  ask: "question:6",
  says: "Waiting on your reply",
  options: [],
  source: "question",
  chain: ["steward 6"],
  answer: { via: "in-its-pane" },
};

describe("answering through the path an ask names", () => {
  it("answers a permission prompt on its chat's own hook", async () => {
    const calls = sent();
    expect(await answerThrough(PLANE, PERMISSION, "allow")).toBeUndefined();
    expect(calls).toEqual([
      {
        cmd: "answer_ask",
        args: { plane: PLANE, session: 3, ask: PERMISSION.ask, option: "allow" },
      },
    ]);
  });

  it("allows a dispatch at the level chosen with what its question showed, and nothing ticked", async () => {
    const calls = sent({ allow_dispatch: { said: "Allowed." } });
    expect(await answerThrough(PLANE, DISPATCH, "you")).toBeUndefined();
    expect(calls).toEqual([
      {
        cmd: "allow_dispatch",
        args: { plane: PLANE, id: 7, level: "you", also: [], shown: "digest-1" },
      },
    ]);
  });

  it("keeps a dispatch blocked, or says never for its pair, by the dispatch's own commands", async () => {
    const calls = sent({ keep_dispatch_blocked: true, never_dispatch: { said: "Never." } });
    await answerThrough(PLANE, DISPATCH, "keep");
    await answerThrough(PLANE, DISPATCH, "never");
    expect(calls.map((one) => [one.cmd, one.args])).toEqual([
      ["keep_dispatch_blocked", { plane: PLANE, id: 7 }],
      ["never_dispatch", { plane: PLANE, id: 7 }],
    ]);
  });

  it("allows a refused host bound to the block shown, and keeps it blocked by the Notice's own command", async () => {
    const shown = { operation: "connect", kind: "host", what: "host", target: "api.example.com" };
    const calls = sent({ allow_sandbox_block: { said: "Allowed.", live: false } });
    await answerThrough(PLANE, HOST, "chat");
    await answerThrough(PLANE, HOST, "keep");
    // Keep blocked is `keep_sandbox_block`, as on the block's Notice (#1666): it answers the
    // block and refuses what the chat's proxy holds on it, so the command waits no longer.
    expect(calls.map((one) => [one.cmd, one.args])).toEqual([
      ["allow_sandbox_block", { plane: PLANE, session: 5, shown, level: "chat" }],
      ["keep_sandbox_block", { plane: PLANE, session: 5, shown }],
    ]);
  });

  it("says whether the chat's proxy took a host's Allow live, so no restart is owed for it", async () => {
    sent({ allow_sandbox_block: { said: "Allowed.", live: true } });
    expect(await answerTaken(PLANE, HOST, "chat")).toEqual({ refused: undefined, live: true });
    sent({ allow_sandbox_block: { said: "Allowed.", live: false } });
    expect(await answerTaken(PLANE, HOST, "chat")).toEqual({ refused: undefined, live: false });
    sent({ answer_ask: null });
    expect(await answerTaken(PLANE, PERMISSION, "allow")).toEqual({
      refused: undefined,
      live: false,
    });
  });

  it("says the source's own sentence when the source refuses", async () => {
    sent({ allow_dispatch: new Error("Nothing was allowed: the question changed.") });
    expect(await answerThrough(PLANE, DISPATCH, "chat")).toBe(
      "Nothing was allowed: the question changed.",
    );
  });

  it("sends nothing for an option the ask does not offer, or for an ask answered in its chat", async () => {
    const calls = sent();
    expect(await answerThrough(PLANE, HOST, "project")).toMatch(/not one of the answers/);
    expect(await answerThrough(PLANE, PERMISSION, "allow_always")).toMatch(/not one of/);
    expect(await answerThrough(PLANE, QUESTION, "yes")).toBe(IN_ITS_CHAT);
    expect(calls).toEqual([]);
  });
});

describe("the list", () => {
  it("is read again when a source may have moved, so an answered ask leaves it", async () => {
    let waiting: Shown[] = [PERMISSION, HOST];
    mockIPC(
      (cmd) => {
        if (cmd === "asks_waiting") return { plane: PLANE, asks: waiting };
        return null;
      },
      { shouldMockEvents: true },
    );
    const { result } = renderHook(() => useAsks([PLANE]));
    await waitFor(() => expect(result.current.held[PLANE]).toHaveLength(2));

    // Kept blocked in its Notice: the core no longer holds it, and a chat moving says so.
    waiting = [PERMISSION];
    await act(() => emit("chat-moved", { plane: PLANE, session: 5 }));
    await waitFor(() => expect(result.current.held[PLANE]).toEqual([PERMISSION]));

    // Answered elsewhere: an answer reads it again whatever it answered.
    waiting = [];
    act(() => result.current.reread(PLANE));
    await waitFor(() => expect(result.current.held[PLANE]).toEqual([]));
  });

  it("is read again when the window itself answered one where no event says so (#1692)", async () => {
    // A Notice's Keep blocked, or an answer in the Inbox, moves the core with no event of its
    // own: the window says so, and the list follows, so an answer in one place clears both.
    let waiting: Shown[] = [HOST];
    mockIPC((cmd) => (cmd === "asks_waiting" ? { plane: PLANE, asks: waiting } : null));
    const { result } = renderHook(() => useAsks([PLANE]));
    await waitFor(() => expect(result.current.held[PLANE]).toEqual([HOST]));
    waiting = [];
    act(() => asksMoved(PLANE));
    await waitFor(() => expect(result.current.held[PLANE]).toEqual([]));
  });

  it("reads its list under React's StrictMode, which runs its effects twice", async () => {
    mockIPC((cmd) => (cmd === "asks_waiting" ? { plane: PLANE, asks: [HOST] } : null));
    const { result } = renderHook(() => useAsks([PLANE]), { wrapper: StrictMode });
    await waitFor(() => expect(result.current.held[PLANE]).toEqual([HOST]));
  });

  it("holds nothing for a project the core says nothing for", async () => {
    sent();
    const { result } = renderHook(() => useAsks([PLANE]));
    await new Promise((settle) => setTimeout(settle, 20));
    expect(PLANE in result.current.held).toBe(false);
  });
});
