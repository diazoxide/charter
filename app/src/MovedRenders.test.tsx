import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { Moved, OpenChat } from "./bindings";
import type { State } from "./chatState";

/**
 * **A chat moving re-renders that chat's row and nothing else** (SC-3, research 02 §5.6).
 *
 * `chat-moved` is the one event that arrives all the time — every turn starts and stops one —
 * and it used to be held in a hook at the top of `PlaneView`, so each one re-rendered the whole
 * project view: every tab on the strip, every pane, and the catalogue under them. At fifty
 * chats that is fifty rows redrawn to change one dot.
 *
 * What is counted is what the window DRAWS per chat: the state mark every tab carries
 * (`ChatMark`) and the terminal in the pane in front (`SessionPane`, stubbed as in every
 * window test). A mark drawn for a chat that did not move, or a pane drawn at all, is the
 * project view rendering again.
 */

const drawn = vi.hoisted(() => ({ marks: [] as (string | undefined)[], panes: [] as number[] }));

vi.mock("./NeedsYou", async (original) => {
  const real = await original<typeof import("./NeedsYou")>();
  return {
    ...real,
    ChatMark: (props: { state: State | undefined }) => {
      drawn.marks.push(props.state);
      return <real.ChatMark {...props} />;
    },
  };
});

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => {
    drawn.panes.push(session);
    return <div data-testid="pane">session {session}</div>;
  },
}));

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

const PLANE = "/home/dev/plane";

/** A harness chat, so its tab draws a state mark from the start (`markOf`). */
function chat(session: number): OpenChat {
  return {
    session,
    name: `ide.${session}`,
    cwd: `${PLANE}/workspaces/ide`,
    harness: "claude",
    in_front: session === 1,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
    guessed: null,
    pinned: false,
    label: null,
    from: null,
  };
}

/** The core, holding `open` chats, and the wire its `chat-moved` events come down. */
function core(open: OpenChat[]): (moved: Moved) => void {
  const listeners = new Map<string, number>();
  mockIPC((cmd, args) => {
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, handler);
      return 1;
    }
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return open;
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    return null;
  });
  return (moved) => {
    const handler = listeners.get("chat-moved");
    if (handler === undefined) throw new Error("the window is not listening for moves");
    act(() => {
      window.__TAURI_INTERNALS__.runCallback(handler, {
        event: "chat-moved",
        id: 1,
        payload: moved,
      });
    });
  };
}

/** One chat's move, as the core pushes it: `queue` is the whole needs-you queue after it. */
function moving(session: number, state: State, at: number, queue: number[] = []): Moved {
  return {
    plane: PLANE,
    session,
    state,
    needs_you: queue.includes(session),
    queue,
    moved_at: at,
    sequence: at,
    reports: [],
    refusals: [],
  };
}

const marksOnTheStrip = () =>
  within(screen.getByRole("tablist", { name: "Tabs" })).getAllByRole("img", {
    name: /./,
  });

afterEach(() => {
  cleanup();
  clearMocks();
  drawn.marks.length = 0;
  drawn.panes.length = 0;
});

describe("a chat moving", () => {
  async function fiveChats() {
    const move = core([chat(1), chat(2), chat(3), chat(4), chat(5)]);
    render(<App />);
    await vi.waitFor(() => expect(marksOnTheStrip()).toHaveLength(5));
    await vi.waitFor(() => expect(screen.getByTestId("pane")).toBeTruthy());
    // And let every answer the first draw asked for land, so what is counted after this is the
    // move and not the window still settling.
    for (let turn = 0; turn < 10; turn += 1) await act(async () => {});
    drawn.marks.length = 0;
    drawn.panes.length = 0;
    return move;
  }

  it("redraws that chat's tab and no other", async () => {
    const move = await fiveChats();

    move(moving(3, "running", 10));

    expect(drawn.marks).toEqual(["running"]);
    expect(drawn.panes).toEqual([]);
  });

  it("redraws only that chat's tab when it starts asking for you, too", async () => {
    const move = await fiveChats();
    move(moving(3, "running", 10));
    drawn.marks.length = 0;
    drawn.panes.length = 0;

    // The queue changes with it, and the queue is the project's: the counts on the strips and
    // the title bar's ✋ redraw. The other chats' rows still do not.
    move(moving(3, "waiting", 11, [3]));

    expect(drawn.marks).toEqual(["waiting"]);
    // The queue is the project's, and the project view still redraws for it: the pane in front
    // with it, once at most (#1034 is narrowing that to the counts alone).
    expect(drawn.panes.length).toBeLessThanOrEqual(1);
    expect(screen.getAllByRole("img", { name: /waiting/i }).length).toBeGreaterThan(0);
  });

  it("still counts the chats running on the status line, which reads them itself", async () => {
    const move = await fiveChats();

    move(moving(2, "running", 10));
    move(moving(4, "running", 11));
    expect(screen.getByTestId("status-running").textContent).toBe("2 chats running");

    move(moving(2, "waiting", 12, [2]));
    expect(screen.getByTestId("status-running").textContent).toBe("1 chat running");
  });

  it("still draws what it moved to", async () => {
    const move = await fiveChats();

    move(moving(2, "done", 10));

    const tabs = within(screen.getByRole("tablist", { name: "Tabs" })).getAllByRole("tab");
    expect(within(tabs[1]).getByRole("img").getAttribute("aria-label")).toMatch(/done|finished/i);
    expect(within(tabs[0]).getByRole("img").getAttribute("aria-label")).not.toMatch(
      /done|finished/i,
    );
  });
});
