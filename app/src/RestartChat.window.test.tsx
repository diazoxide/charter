import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "./App";
import type { Moved, OlderSandbox } from "./bindings";
import { ChatsHere, fixedChats, nothingKnown, type State } from "./chatState";
import { forgetDismissals } from "./dismissals";
import { sandboxCommandReturned } from "./sandboxAsked";
import { SandboxChangedNotice } from "./SandboxChanged";
import { stripNamed } from "./test-strips";

/**
 * **Restart chat, and the Notice after a sandbox setting changes** (#1428), against the whole
 * window. A chat keeps the sandbox it started with, so after a setting changes the window says
 * how many chats are behind and restarts them on a press; a chat's tab menu restarts one. Both
 * ask the core for the one restart (`ask_chat_restart`, then `restart_chat` once the chat's
 * turn has ended), which a block's Allow uses too (`SandboxBlocks.window.test.tsx`).
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
  forgetDismissals();
});

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;

const CHAT = {
  session: 4,
  name: "claude 4",
  cwd: `${ALPHA}/repo`,
  harness: "claude",
  in_front: true,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona: null,
  unreported: null,
  guessed: null,
  pinned: false,
  label: null,
  from: null,
};

/** A task chat 4 dispatched, with no tab. */
const TASK = {
  ...CHAT,
  session: 6,
  name: "devops 6",
  in_front: false,
  persona: "devops",
  label: "talk",
  from: {
    chat: 4,
    name: "claude 4",
    workspace: "alpha",
    task: true,
    tab: false,
    reported: false,
    unreported: false,
  },
} as unknown as typeof CHAT;

/** Chat 4 mid-turn, then waiting for you: what the board says as its turn runs and ends. */
const RUNNING: Moved = {
  plane: PLANE,
  session: 4,
  state: "running",
  needs_you: false,
  queue: [],
  moved_at: 1,
  reports: [],
  refusals: [],
  children: [],
  sequence: 1,
};
const WAITING: Moved = { ...RUNNING, state: "waiting", moved_at: 2, sequence: 2 };

/** The project's settings changed on disk, as the core's watcher says it. */
const SETTINGS_CHANGED = { plane: PLANE, changes: null, answers: [{ answer: "settings" }] };

type Core = {
  /** What `chats_on_older_sandbox` answers, read at each ask. */
  older: OlderSandbox | null;
  /** What the restarted chat's start found to say. */
  notices: string[];
  /** Why the restart is refused, where it is. */
  refused?: string;
  /** Why the chat is not restarted yet, where it waits on a permission prompt. */
  notYet?: string;
  /** What the restart waits for before it answers, where a test holds it open. */
  held?: Promise<void>;
  /** Chat 4's tasks, which the sidebar lists beside it. */
  tasks?: (typeof CHAT)[];
};

function core(now: Core) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC(
    (cmd, args) => {
      asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return [CHAT];
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          personas: [],
          persona: null,
          unfiled: [],
          workspaces: [
            {
              name: "alpha",
              path: ALPHA,
              vision: "",
              todos: [],
              chats: [CHAT, ...(now.tasks ?? [])],
            },
          ],
        };
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "owed_restarts") return [];
      if (cmd === "chats_on_older_sandbox") return now.older;
      if (cmd === "restart_chat") {
        if (now.refused !== undefined) throw new Error(now.refused);
        if (now.notYet !== undefined) return { chat: null, notices: [], not_yet: now.notYet };
        const restarted = () => {
          // Restarted, so it is on the sandbox the project has now.
          now.older = null;
          return {
            chat: { ...CHAT, session: 9, resumed: "c1" },
            notices: now.notices,
            not_yet: null,
          };
        };
        return now.held === undefined ? restarted() : now.held.then(restarted);
      }
      return null;
    },
    { shouldMockEvents: true },
  );
  return {
    asked: (cmd: string) => asked.filter((one) => one.cmd === cmd).map(({ args }) => args),
  };
}

async function settled() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

async function aChat(now: Partial<Core> = {}) {
  const held: Core = { older: null, notices: [], ...now };
  const said = core(held);
  render(<App />);
  // The chat is on screen: its pane is drawn.
  await screen.findByTestId("pane");
  await settled();
  return { ...said, core: held };
}

/** The menu on chat 4's tab, as a right-click opens it. */
async function tabMenu() {
  const strip = stripNamed("Tabs");
  fireEvent.contextMenu(within(strip).getByRole("tab", { name: /claude 4/ }));
  return screen.findByRole("menu");
}

const ASKED = [{ plane: PLANE, session: 4 }];
const RESTARTED = [{ plane: PLANE, session: 4, columns: 80, rows: 24 }];

describe("Restart chat on a chat's tab", () => {
  it("restarts a chat whose turn has ended at once, on its conversation, in its own pane", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-moved", WAITING));

    await userEvent.click(
      within(await tabMenu()).getByRole("menuitem", { name: "Restart chat claude claude 4" }),
    );

    await waitFor(() => expect(asked("restart_chat")).toEqual(RESTARTED));
    // Asked for first: the core restarts only a chat it owes one.
    expect(asked("ask_chat_restart")).toEqual(ASKED);
    expect(await screen.findByText("session 9")).toBeInTheDocument();
    expect(screen.queryByText("session 4")).toBeNull();
  });

  it("waits for the turn to end mid-turn, and its row says so", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-moved", RUNNING));

    const menu = await tabMenu();
    expect(
      within(menu).queryByRole("menuitem", { name: "Restart chat claude claude 4" }),
    ).toBeNull();
    await userEvent.click(
      within(menu).getByRole("menuitem", {
        name: "Restart chat claude claude 4 when this turn ends",
      }),
    );
    await waitFor(() => expect(asked("ask_chat_restart")).toEqual(ASKED));
    await settled();
    expect(asked("restart_chat")).toEqual([]);
    expect(screen.getByText("session 4")).toBeInTheDocument();

    await act(() => emit("chat-moved", WAITING));

    await waitFor(() => expect(asked("restart_chat")).toEqual(RESTARTED));
    expect(await screen.findByText("session 9")).toBeInTheDocument();
  });

  it("says the wait beside the row of a chat whose turns purlis can see", async () => {
    await aChat();
    await act(() => emit("chat-moved", WAITING));

    expect(
      within(await tabMenu()).getByRole("menuitem", { name: "Restart chat claude claude 4" }),
    ).toHaveAccessibleDescription(
      "It keeps its conversation and starts on this project's settings as they are now. Mid-turn, it restarts when the turn ends.",
    );
  });

  it("says a chat that reports no state restarts at once, and restarts it: the person pressed for it", async () => {
    const { asked } = await aChat();

    const row = within(await tabMenu()).getByRole("menuitem", {
      name: "Restart chat claude claude 4",
    });
    // docs/ui-copy.md, "Uncertainty is stated, not hidden": no wait is promised for it.
    expect(row).toHaveAccessibleDescription(
      "claude claude 4 reports no state, so purlis cannot tell whether it is mid-turn, and restarts it at once. It keeps its conversation and starts on this project's settings as they are now.",
    );
    await userEvent.click(row);

    await waitFor(() => expect(asked("restart_chat")).toEqual(RESTARTED));
    expect(screen.queryByText(/restart it when you are ready/)).toBeNull();
  });

  it("says the chat is restarting while its restart runs, and never asks for one by hand", async () => {
    // S5: a chat that reports no state, restarted because the person asked. Nothing was
    // allowed, and it is restarting already: the grant flow's by-hand line has no place here.
    let finish = () => {};
    const held = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const { asked } = await aChat({ held });

    await userEvent.click(
      within(await tabMenu()).getByRole("menuitem", { name: "Restart chat claude claude 4" }),
    );

    await waitFor(() => expect(asked("restart_chat")).toEqual(RESTARTED));
    expect(await screen.findByRole("status", { name: "Restart" })).toHaveTextContent(
      "Restarting this chat…",
    );
    expect(screen.queryByText(/restart it when you are ready/)).toBeNull();
    expect(screen.queryByText(/What you allowed/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Restart now" })).toBeNull();
    expect(screen.getByText("session 4")).toBeInTheDocument();

    await act(async () => {
      finish();
      await held;
    });

    expect(await screen.findByText("session 9")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("status", { name: "Restart" })).toBeNull());
  });

  it("says why a chat at a permission prompt is not restarted yet, in the core's sentence", async () => {
    // S7: the press was once answered by nothing, and by a restart later.
    const notYet = "It is waiting on a permission prompt, so it restarts once that is answered.";
    const held = await aChat({ notYet });
    await act(() => emit("chat-moved", WAITING));

    await userEvent.click(
      within(await tabMenu()).getByRole("menuitem", { name: "Restart chat claude claude 4" }),
    );

    expect(await screen.findByRole("status", { name: "Restart" })).toHaveTextContent(notYet);
    expect(screen.getByText("session 4")).toBeInTheDocument();

    // The prompt is answered, the chat moves, and the restart it is still owed happens.
    held.core.notYet = undefined;
    await act(() => emit("chat-moved", { ...WAITING, moved_at: 3, sequence: 3 }));

    expect(await screen.findByText("session 9")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByText(notYet)).toBeNull());
  });

  it("says on the new run's tab that a chat which ran without the sandbox runs in it again", async () => {
    const line =
      "This chat ran without the sandbox until this restart. It runs in the sandbox now, because that choice lasts for one start.";
    await aChat({ notices: [line] });
    await act(() => emit("chat-moved", WAITING));

    await userEvent.click(
      within(await tabMenu()).getByRole("menuitem", { name: "Restart chat claude claude 4" }),
    );

    expect(await screen.findByText(line)).toBeInTheDocument();
    // Nothing here offers to start it without the sandbox again.
    expect(screen.queryByRole("button", { name: /without the sandbox/i })).toBeNull();
  });

  it("says why on the chat's pane when the restart is refused", async () => {
    const refused =
      "purlis did not restart chat 4: it has no conversation to resume. If it has only just started, send it a message and restart it again. Start fresh on its tab's menu starts it again without a conversation.";
    const { asked } = await aChat({ refused });
    await act(() => emit("chat-moved", WAITING));

    await userEvent.click(
      within(await tabMenu()).getByRole("menuitem", { name: "Restart chat claude claude 4" }),
    );

    // Why, and what to do about it (S8).
    await waitFor(() =>
      expect(screen.getByRole("status", { name: "Restart" })).toHaveTextContent(refused),
    );
    expect(screen.getByText("session 4")).toBeInTheDocument();

    // And Restart now asks again: a refused restart is owed no more, so it is asked for anew.
    await userEvent.click(
      within(screen.getByRole("status", { name: "Restart" })).getByRole("button", {
        name: "Restart now",
      }),
    );
    await waitFor(() => expect(asked("ask_chat_restart")).toHaveLength(2));
    await waitFor(() => expect(asked("restart_chat")).toHaveLength(2));
  });
});

describe("Restart chat on a task's row in the Chats list, for a task with no tab (#1462)", () => {
  const section = () => screen.findByRole("tree", { name: "Chats of this project" });
  const row = async (name: string) => {
    const tree = await section();
    await waitFor(() =>
      expect(
        within(tree)
          .getAllByRole("treeitem")
          .some((one) => one.querySelector(".session")?.textContent === name),
      ).toBe(true),
    );
    const found = within(tree)
      .getAllByRole("treeitem")
      .find((one) => one.querySelector(".session")?.textContent === name);
    if (found === undefined) throw new Error(`no row is named ${name}`);
    return found;
  };

  it("restarts it from its row, asked once on the row as its tab's menu asks", async () => {
    const { asked } = await aChat({ tasks: [TASK] });

    fireEvent.contextMenu(await row("talk"));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Restart chat talk" }));

    // A task's program ends to start again: asked on its row first, and nothing restarts yet.
    const question = await screen.findByRole("group", { name: /^Restart talk\?/ });
    expect(asked("ask_chat_restart")).toEqual([]);
    await userEvent.click(within(question).getByRole("button", { name: "Restart it" }));

    await waitFor(() => expect(asked("ask_chat_restart")).toEqual([{ plane: PLANE, session: 6 }]));
    // It reports no state here, so it restarts at once: the person pressed for it.
    await waitFor(() =>
      expect(asked("restart_chat")).toEqual([{ plane: PLANE, session: 6, columns: 80, rows: 24 }]),
    );
  });

  it("says a refusal on the row, without opening the task", async () => {
    const refused = "purlis did not restart chat 6: it has no conversation to resume.";
    const { asked } = await aChat({ tasks: [TASK], refused });

    fireEvent.contextMenu(await row("talk"));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Restart chat talk" }));
    await userEvent.click(
      within(await screen.findByRole("group", { name: /^Restart talk\?/ })).getByRole("button", {
        name: "Restart it",
      }),
    );

    await waitFor(() => expect(asked("restart_chat")).toHaveLength(1));
    expect(await screen.findByRole("status", { name: "Restart of talk" })).toHaveTextContent(
      refused,
    );
    // Not opened: the tab in front still shows chat 4, and no pane draws the task.
    expect(screen.getAllByTestId("pane").map((pane) => pane.textContent)).toEqual(["session 4"]);
  });

  it("has no such row for a chat that has a tab: its tab's menu has it", async () => {
    await aChat({ tasks: [TASK] });

    fireEvent.contextMenu(await row("claude claude 4"));
    await screen.findAllByRole("menuitem");
    expect(screen.queryByRole("menuitem", { name: /^Restart chat/ })).toBeNull();
  });
});

describe("the Notice after a sandbox setting changes", () => {
  const BEHIND: OlderSandbox = { chats: [{ session: 4, change: "c1" }] };

  it("shows nothing while every chat runs on the sandbox the project has now", async () => {
    const { asked } = await aChat({ older: null });

    await act(() => emit("plane-changed", SETTINGS_CHANGED));
    await settled();

    // Asked, and answered that nobody is behind: a settings file written is not a change.
    expect(asked("chats_on_older_sandbox").length).toBeGreaterThan(1);
    expect(screen.queryByRole("status", { name: "Sandbox changed" })).toBeNull();
  });

  it("says once how many chats keep the old sandbox, and restarts them on a press", async () => {
    const held = await aChat({ older: null });
    await act(() => emit("chat-moved", WAITING));
    expect(screen.queryByRole("status", { name: "Sandbox changed" })).toBeNull();

    held.core.older = BEHIND;
    await act(() => emit("plane-changed", SETTINGS_CHANGED));

    const notice = await screen.findByRole("status", { name: "Sandbox changed" });
    expect(notice).toHaveTextContent(
      "This project's sandbox changed. 1 chat keeps the old sandbox until it restarts. A restart keeps the conversation, and waits for a turn to end.",
    );
    // The same change, heard again: still the one Notice.
    await act(() => emit("plane-changed", SETTINGS_CHANGED));
    await settled();
    expect(screen.getAllByRole("status", { name: "Sandbox changed" })).toHaveLength(1);
    expect(within(notice).queryByRole("button", { name: /without the sandbox/i })).toBeNull();

    await userEvent.click(
      within(screen.getByRole("status", { name: "Sandbox changed" })).getByRole("button", {
        name: "Restart it",
      }),
    );

    await waitFor(() => expect(held.asked("restart_chat")).toEqual(RESTARTED));
    expect(held.asked("ask_chat_restart")).toEqual(ASKED);
    expect(await screen.findByText("session 9")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByRole("status", { name: "Sandbox changed" })).toBeNull(),
    );
  });

  it("restarts a chat mid-turn only when its turn ends", async () => {
    const held = await aChat({ older: BEHIND });
    await act(() => emit("chat-moved", RUNNING));

    const notice = await screen.findByRole("status", { name: "Sandbox changed" });
    await userEvent.click(within(notice).getByRole("button", { name: "Restart it" }));
    await waitFor(() => expect(held.asked("ask_chat_restart")).toEqual(ASKED));
    await settled();
    expect(held.asked("restart_chat")).toEqual([]);

    await act(() => emit("chat-moved", WAITING));
    await waitFor(() => expect(held.asked("restart_chat")).toEqual(RESTARTED));
  });

  it("stays put away once dismissed, until the sandbox changes again", async () => {
    const held = await aChat({ older: BEHIND });

    const notice = await screen.findByRole("status", { name: "Sandbox changed" });
    await userEvent.click(within(notice).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("status", { name: "Sandbox changed" })).toBeNull();
    await act(() => emit("plane-changed", SETTINGS_CHANGED));
    await settled();
    expect(screen.queryByRole("status", { name: "Sandbox changed" })).toBeNull();
    expect(held.asked("ask_chat_restart")).toEqual([]);

    held.core.older = { chats: [{ session: 4, change: "c2" }] };
    await act(() => emit("plane-changed", SETTINGS_CHANGED));

    expect(await screen.findByRole("status", { name: "Sandbox changed" })).toBeInTheDocument();
  });

  it("asks again after one of the window's own sandbox commands returns", async () => {
    // D-1428-10: a folder every chat may write, revoked in Settings' Granted list, is kept in
    // the project's state folder, and no watcher reports a write there.
    const held = await aChat({ older: null });
    await act(() => emit("chat-moved", WAITING));
    const before = held.asked("chats_on_older_sandbox").length;
    expect(screen.queryByRole("status", { name: "Sandbox changed" })).toBeNull();

    held.core.older = BEHIND;
    act(() => sandboxCommandReturned());

    expect(await screen.findByRole("status", { name: "Sandbox changed" })).toBeInTheDocument();
    expect(held.asked("chats_on_older_sandbox").length).toBeGreaterThan(before);
  });

  it("asks again when the window comes back into focus (#1462)", async () => {
    // What no watcher reports, such as a sandbox file another window or a hand wrote, or the
    // administrator's policy, is caught when the person comes back to the window.
    const held = await aChat({ older: null });
    await act(() => emit("chat-moved", WAITING));
    const before = held.asked("chats_on_older_sandbox").length;

    held.core.older = BEHIND;
    act(() => {
      window.dispatchEvent(new FocusEvent("focus"));
    });

    expect(await screen.findByRole("status", { name: "Sandbox changed" })).toBeInTheDocument();
    expect(held.asked("chats_on_older_sandbox").length).toBeGreaterThan(before);
  });

  it("asks again when a task with no tab starts or ends, as the Chats list hears it (#1462)", async () => {
    const held = await aChat({ older: null });
    await act(() => emit("chat-moved", WAITING));
    const before = held.asked("chats_on_older_sandbox").length;

    // Chat 4 dispatched a task, which has no tab, and is behind. Only the sidebar's read says
    // it is there: nothing on the strip moves.
    held.core.older = { chats: [{ session: 6, change: "c1" }] };
    held.core.tasks = [TASK];
    await act(() =>
      emit("plane-changed", { plane: PLANE, changes: null, answers: [{ answer: "sidebar" }] }),
    );

    await waitFor(() =>
      expect(held.asked("chats_on_older_sandbox").length).toBeGreaterThan(before),
    );
    expect(await screen.findByRole("status", { name: "Sandbox changed" })).toBeInTheDocument();
  });

  it("promises no wait for a chat that reports no state, and says why", async () => {
    // S6: nothing has been heard from chat 4, so purlis cannot wait for the end of its turn.
    await aChat({ older: BEHIND });

    const notice = await screen.findByRole("status", { name: "Sandbox changed" });
    expect(notice).toHaveTextContent(
      "This project's sandbox changed. 1 chat keeps the old sandbox until it restarts. A restart keeps the conversation. This chat reports no state, so purlis cannot tell whether it is mid-turn, and restarts it at once.",
    );
    expect(notice).not.toHaveTextContent("waits for a turn to end");
  });

  /** The Notice on its own, among chats in `states`, with the dismissals a window keeps. */
  function noticeFor(states: Record<number, State>) {
    const dismissed = new Set<string>();
    const settled: string[][] = [];
    const onRestart = vi.fn();
    const drawn = (older: OlderSandbox | null) => (
      <ChatsHere.Provider value={fixedChats({ ...nothingKnown, bySession: states })}>
        <SandboxChangedNotice
          older={older}
          dismissed={new Set(dismissed)}
          dismiss={(cause) => dismissed.add(cause)}
          settle={(_family, present) => {
            settled.push([...present]);
            for (const cause of [...dismissed])
              if (!present.includes(cause)) dismissed.delete(cause);
          }}
          onRestart={onRestart}
        />
      </ChatsHere.Provider>
    );
    return { dismissed, settled, onRestart, drawn };
  }
  const notice = () => screen.queryByRole("status", { name: "Sandbox changed" });

  it("counts the chats, and its words agree with the count", () => {
    const { onRestart, drawn } = noticeFor({ 4: "waiting", 5: "running" });
    render(
      drawn({
        chats: [
          { session: 4, change: "c1" },
          { session: 5, change: "c1" },
        ],
      }),
    );

    expect(notice()).toHaveTextContent(
      "This project's sandbox changed. 2 chats keep the old sandbox until they restart. A restart keeps the conversation, and waits for a turn to end.",
    );
    fireEvent.click(
      within(screen.getByRole("status", { name: "Sandbox changed" })).getByRole("button", {
        name: "Restart them",
      }),
    );
    expect(onRestart).toHaveBeenCalledWith([4, 5]);
  });

  it("says which of several chats it cannot wait for", () => {
    const behind = (sessions: number[]): OlderSandbox => ({
      chats: sessions.map((session) => ({ session, change: "c1" })),
    });
    const { drawn } = noticeFor({ 4: "waiting", 5: "running" });
    const { rerender } = render(drawn(behind([4, 5, 6])));
    expect(notice()).toHaveTextContent(
      "A restart keeps the conversation, and waits for a turn to end. 1 of them reports no state, so purlis cannot tell whether it is mid-turn, and restarts it at once.",
    );

    rerender(drawn(behind([4, 6, 7])));
    expect(notice()).toHaveTextContent(
      "A restart keeps the conversation, and waits for a turn to end. 2 of them report no state, so purlis cannot tell whether they are mid-turn, and restarts them at once.",
    );

    rerender(drawn(behind([6, 7])));
    expect(notice()).toHaveTextContent(
      "A restart keeps the conversation. These chats report no state, so purlis cannot tell whether they are mid-turn, and restarts them at once.",
    );
    expect(notice()).not.toHaveTextContent("waits for a turn to end");
  });

  it("keeps a dismissal for each chat by its own change, so one restarting brings nothing back", () => {
    // S9: two chats that compile differently (two personas, say), both behind.
    const { dismissed, settled, drawn } = noticeFor({ 4: "waiting", 5: "waiting" });
    const four = { session: 4, change: "c1" };
    const five = { session: 5, change: "c2" };
    const { rerender } = render(drawn({ chats: [four, five] }));
    expect(notice()).toHaveTextContent("2 chats keep the old sandbox");

    fireEvent.click(
      within(screen.getByRole("status", { name: "Sandbox changed" })).getByRole("button", {
        name: "Dismiss",
      }),
    );
    expect([...dismissed].sort()).toEqual(["sandbox-changed:c1", "sandbox-changed:c2"]);
    rerender(drawn({ chats: [four, five] }));
    expect(notice()).toBeNull();

    // Chat 4 restarts. Chat 5 is behind the change it was behind, which was dismissed.
    rerender(drawn({ chats: [five] }));
    expect(notice()).toBeNull();
    expect(settled.at(-1)).toEqual(["sandbox-changed:c2"]);
    expect([...dismissed]).toEqual(["sandbox-changed:c2"]);

    // The sandbox changes again: chat 5 is behind something nobody has dismissed.
    rerender(drawn({ chats: [{ session: 5, change: "c3" }] }));
    expect(notice()).toHaveTextContent("1 chat keeps the old sandbox until it restarts.");

    // And once nobody is behind, nothing is kept.
    rerender(drawn(null));
    expect(notice()).toBeNull();
    expect(dismissed.size).toBe(0);
  });
});
