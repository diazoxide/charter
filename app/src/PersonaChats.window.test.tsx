import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { EndingChat } from "./EndingChat";
import type { ClosingChat, PersonaChat } from "./bindings";
import { forgetDismissals } from "./dismissals";

/**
 * **Closing a chat that asked persona chats for something**, against the whole window: the one
 * question about the ones still running, and what each answer does. Keep them running closes
 * only the chat; Stop them has the core end them first (`stop_persona_chats_of`) and takes
 * their tabs away. A chat with none running is asked nothing more than any chat is.
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

const STEWARD = {
  session: 4,
  name: "4",
  cwd: ALPHA,
  harness: "claude",
  in_front: true,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona: "steward",
  unreported: null,
  guessed: null,
  pinned: false,
  label: null,
  from: null,
};

/** The devops chat the steward chat's tab asked for, still to report. */
const TASK = {
  ...STEWARD,
  session: 9,
  name: "9",
  in_front: false,
  persona: "devops",
  label: "check prod",
  from: {
    name: "steward 4",
    workspace: "alpha",
    chat: 4,
    task: true,
    tab: true,
    reported: false,
    unreported: false,
  },
};

const RUNNING: PersonaChat = {
  session: 9,
  name: "check prod",
  persona: "devops",
  state: "running",
  said: "running",
  closes_with_its_asker: false,
};

/** What the core says of the chats below the steward chat. */
const below = (tasks: PersonaChat[], running: string[]): ClosingChat => ({ tasks, running });

function core(theirs: ClosingChat) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC(
    (cmd, args) => {
      asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return [STEWARD, TASK];
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          personas: ["devops", "steward"],
          persona: "steward",
          unfiled: [],
          workspaces: [
            { name: "alpha", path: ALPHA, vision: "", todos: [], chats: [STEWARD, TASK] },
          ],
        };
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "smart_close_offer") return { available: true, why: null, close_first: false };
      if (cmd === "smart_close") return "sent";
      if (cmd === "persona_chats_of") return theirs;
      // The core ends the ones at work, and the chat itself where asked: it says which.
      if (cmd === "close_chat_stopping")
        return (args as { then: string }).then === "close" ? [9, 4] : [9];
      return null;
    },
    { shouldMockEvents: true },
  );
  return {
    asked: (cmd: string) => asked.filter((one) => one.cmd === cmd).map(({ args }) => args),
    order: () =>
      asked
        .map((one) => one.cmd)
        .filter((cmd) => ["close_chat_stopping", "close_session", "smart_close"].includes(cmd)),
  };
}

async function settled() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

async function closingTheStewardChat(theirs: ClosingChat) {
  const said = core(theirs);
  render(<App />);
  await screen.findAllByTestId("pane");
  await settled();
  expect(tabNames()).toEqual(["steward 4", "check prod"]);
  await userEvent.click(screen.getByRole("button", { name: "End chat steward 4" }));
  return { ...said, asking: await screen.findByRole("alertdialog") };
}

function tabNames() {
  const strip = screen.getByRole("tablist", { name: "Tabs" });
  return within(strip)
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent ?? tab.textContent ?? "");
}

const ONE_RUNNING = below([RUNNING], ["check prod"]);

describe("closing a chat with chats at work below it", () => {
  it("asks once, with both choices, and starts on keeping them", async () => {
    const { asking, asked } = await closingTheStewardChat(ONE_RUNNING);

    const question = within(asking).getByRole("group", {
      name: "1 chat it started is still at work: check prod.",
    });
    expect(within(question).getAllByRole("radio")).toHaveLength(2);
    expect(within(question).getByRole("radio", { name: "Keep them running" })).toBeChecked();
    expect(within(question).getByRole("radio", { name: "Stop them" })).not.toBeChecked();
    expect(question).toHaveTextContent(
      "They go on working. A task's report goes to this chat's workspace, where the next chat to start reads it.",
    );
    // One dialog, and nothing done by asking.
    expect(screen.getAllByRole("alertdialog")).toHaveLength(1);
    expect(asked("close_chat_stopping")).toEqual([]);
    expect(asked("close_session")).toEqual([]);
  });

  it("keeps them running: the chat closes, and its persona chat goes back to the list, still open", async () => {
    const { asking, asked } = await closingTheStewardChat(ONE_RUNNING);

    await userEvent.click(within(asking).getByRole("button", { name: "Close" }));

    await waitFor(() => expect(asked("close_session")).toEqual([{ plane: PLANE, session: 4 }]));
    expect(asked("close_chat_stopping")).toEqual([]);
    // **Its tab goes with the session's** (#1489, V100-39): a task's own tab is sent back when
    // the session that asked closes, and the task is not ended by that. It was never closed,
    // and the core is told it has no tab.
    await waitFor(() =>
      expect(within(screen.getByRole("tablist", { name: "Tabs" })).queryAllByRole("tab")).toEqual(
        [],
      ),
    );
    expect(asked("open_chat_tab")).toEqual([{ plane: PLANE, session: 9, opened: false }]);
  });

  it("stops them and closes in one command to the core, and their tabs go with the chat's", async () => {
    const { asking, asked, order } = await closingTheStewardChat(ONE_RUNNING);

    await userEvent.click(within(asking).getByRole("radio", { name: "Stop them" }));
    expect(asking).toHaveTextContent(
      "They are stopped now, and so are the chats they started: each gets one short turn to write what it did, then ends. There is no undo.",
    );
    await userEvent.click(within(asking).getByRole("button", { name: "Close" }));

    await waitFor(() =>
      expect(asked("close_chat_stopping")).toEqual([{ plane: PLANE, session: 4, then: "close" }]),
    );
    // One step: the chat is never left running between the stop and its own close, and the
    // window ends nothing a second time.
    expect(order()).toEqual(["close_chat_stopping"]);
    await waitFor(() =>
      expect(within(screen.getByRole("tablist", { name: "Tabs" })).queryAllByRole("tab")).toEqual(
        [],
      ),
    );
  });

  it("carries the answer through Smart close: stopped now, and remembered for its close", async () => {
    const { asking, asked, order } = await closingTheStewardChat(ONE_RUNNING);

    await userEvent.click(within(asking).getByRole("radio", { name: "Stop them" }));
    await userEvent.click(within(asking).getByRole("button", { name: "Smart close" }));

    await waitFor(() => expect(asked("smart_close")).toEqual([{ plane: PLANE, session: 4 }]));
    expect(asked("close_chat_stopping")).toEqual([
      { plane: PLANE, session: 4, then: "smart_close" },
    ]);
    expect(order()).toEqual(["close_chat_stopping", "smart_close"]);
  });

  it("changes nothing on Cancel", async () => {
    const { asking, asked } = await closingTheStewardChat(ONE_RUNNING);

    await userEvent.click(within(asking).getByRole("radio", { name: "Stop them" }));
    await userEvent.click(within(asking).getByRole("button", { name: "Cancel" }));

    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(asked("close_chat_stopping")).toEqual([]);
    expect(asked("close_session")).toEqual([]);
    expect(tabNames()).toEqual(["steward 4", "check prod"]);
  });

  it("asks about every chat at work below it, whatever its own tasks are doing", async () => {
    // Its one task has reported, and that task had started another that is still running;
    // a chat it handed work to is mid-turn. The core names both, deepest first.
    const { asking } = await closingTheStewardChat(
      below([{ ...RUNNING, state: "reported", said: "reported" }], ["read the logs", "moved on"]),
    );

    expect(
      within(asking).getByRole("group", {
        name: "2 chats it started are still at work: read the logs, moved on.",
      }),
    ).toBeInTheDocument();
  });
});

describe("closing a chat with nothing at work below it", () => {
  it("asks nothing about them, and says which reported tasks close with it", async () => {
    const { asking, asked } = await closingTheStewardChat(
      below(
        [
          { ...RUNNING, state: "reported", said: "reported", closes_with_its_asker: true },
          {
            session: 11,
            name: "check staging",
            persona: "devops",
            state: "reported",
            said: "reported",
            // Given more to do since it reported: it is working, and stays.
            closes_with_its_asker: false,
          },
        ],
        [],
      ),
    );

    expect(within(asking).queryByRole("group")).toBeNull();
    expect(within(asking).queryByRole("radio")).toBeNull();
    expect(asking).toHaveTextContent(
      "1 reported task closes with it, each once its session record is written: check prod.",
    );
    await userEvent.click(within(asking).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(asked("close_session")).toEqual([{ plane: PLANE, session: 4 }]));
    expect(asked("close_chat_stopping")).toEqual([]);
  });

  it("says nothing more for a chat that started none", async () => {
    const { asking } = await closingTheStewardChat(below([], []));

    expect(within(asking).queryByRole("radio")).toBeNull();
    expect(asking).not.toHaveTextContent("closes with it");
  });
});

describe("closing several chats at once", () => {
  const offer = {
    id: "tab.close:1",
    title: "End tab steward 4",
    available: true,
    reason: "",
    does: { verb: "nothing" },
  } as const;

  it("asks nothing about the chats they started, and says those keep running", () => {
    render(
      <EndingChat
        offer={offer}
        keeps
        onEnd={() => undefined}
        onSmartClose={() => undefined}
        onCancel={() => undefined}
      />,
    );

    const asking = screen.getByRole("alertdialog");
    expect(within(asking).queryByRole("radio")).toBeNull();
    expect(asking).toHaveTextContent(
      "The chats these chats started keep running. Close one chat at a time to be asked about them.",
    );
  });

  it("says nothing of it where none of them started a chat that is at work", () => {
    render(
      <EndingChat
        offer={offer}
        onEnd={() => undefined}
        onSmartClose={() => undefined}
        onCancel={() => undefined}
      />,
    );

    expect(screen.getByRole("alertdialog")).not.toHaveTextContent("keep running");
  });
});
