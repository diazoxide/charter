import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { noLongerAsking } from "./AnswerPanel";
import type { OpenChat } from "./bindings";
import { forgetThisLaunch } from "./regions";

/**
 * **The person answers a task's question from its row in the Chats list** (#1496, #1551),
 * against the whole window. A row is one button, so its menu's Answer opens a dialog of its own
 * holding the form the Activity tab draws under the question's line. The tab chip's menu opens
 * the same dialog from the task's line (`TabChip.window.test.tsx`).
 *
 * The core here is a pretend one that answers `task_question` as the real one does while the
 * task is paused on a question, and records what it was sent.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session, focused }: { session: number; focused: boolean }) => (
    <div data-testid="pane" className={focused ? "pane focused" : "pane"}>
      session {session}
      <textarea aria-label={`Terminal of chat ${session}`} />
    </div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;

const STEWARD: OpenChat = {
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
  card: null,
  guessed: null,
  pinned: false,
  label: null,
  from: null,
};

/** A task of the steward chat's, paused on a question to it (`asking`), or not. */
function task(asking: boolean): OpenChat {
  return {
    ...STEWARD,
    session: 9,
    name: "9",
    in_front: false,
    persona: "devops",
    label: "read the logs",
    from: {
      name: "steward 4",
      workspace: "alpha",
      chat: 4,
      task: true,
      tab: false,
      reported: false,
      unreported: false,
      asking,
    },
  };
}

const QUESTION = "Which pods?\nOnly the ones in staging?";

type Asked = { cmd: string; args: Record<string, unknown> };

/** The core: the steward's chat and its task in workspace alpha. */
function core(asking = true) {
  const asked: Asked[] = [];
  const chats = [STEWARD, task(asking)];
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      asked.push({ cmd, args: a });
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return chats;
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          personas: ["devops", "steward"],
          persona: "steward",
          unfiled: [],
          workspaces: [{ name: "alpha", path: ALPHA, vision: "", todos: [], chats }],
        };
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "stopping_chats") return [];
      if (cmd === "finished_tasks") return [];
      if (cmd === "task_question")
        return chats[1].from?.asking === true && a.session === 9
          ? { task: "read the logs", asked: "steward 4", number: 3, question: QUESTION }
          : null;
      return null;
    },
    { shouldMockEvents: true },
  );
  return {
    sent: () => asked.filter((one) => one.cmd === "answer_task_question").map(({ args }) => args),
    /** The task stops asking: the asking chat answered it first. */
    answeredElsewhere: async () => {
      chats[1] = task(false);
      await act(() =>
        emit("plane-changed", {
          plane: PLANE,
          changes: [{ kind: "chats", workspace: null, persona: null, path: "" }],
          answers: [{ answer: "sidebar" }],
        }),
      );
    },
  };
}

const section = () => screen.findByRole("tree", { name: "Chats of this project" });
const row = (tree: HTMLElement, name: string) => {
  const found = within(tree)
    .getAllByRole("treeitem")
    .find((one) => one.querySelector(".session")?.textContent === name);
  if (found === undefined) throw new Error(`no row is named ${name}`);
  return found;
};
const ANSWER = "Answer read the logs's question";

/** Opens the Answer dialog from the task's row menu. */
async function opened() {
  const tree = await section();
  fireEvent.contextMenu(row(tree, "read the logs"));
  await userEvent.click(await screen.findByRole("menuitem", { name: ANSWER }));
  return { tree, dialog: await screen.findByRole("dialog", { name: ANSWER }) };
}

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
});

describe("Answer, from a task's row menu in the Chats list (#1551)", () => {
  it("opens a dialog with the question the task is paused on, sends the answer for it, and closes", async () => {
    const said = core();
    render(<App />);

    const { dialog } = await opened();
    const form = await within(dialog).findByRole("form", { name: ANSWER });
    // The chat's words, as text, with their line break.
    expect(form.querySelector(".answer-asked")?.textContent).toBe(QUESTION);
    const box = within(dialog).getByRole("textbox", { name: "Your answer" });
    await waitFor(() => expect(box).toHaveFocus());
    await userEvent.type(box, "Staging only.{Enter}");

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // Once, by the number the core gave the question: what the answer is for.
    expect(said.sent()).toEqual([
      { plane: PLANE, session: 9, number: 3, question: QUESTION, text: "Staging only." },
    ]);
  });

  it("is offered only while the task asks", async () => {
    core(false);
    render(<App />);
    const tree = await section();

    fireEvent.contextMenu(row(tree, "read the logs"));

    await screen.findByRole("menuitem", { name: "Brief of read the logs" });
    expect(screen.queryByRole("menuitem", { name: /^Answer / })).toBeNull();
  });

  it("is not closed by a press outside it, and Cancel puts it away having sent nothing", async () => {
    const said = core();
    render(<App />);

    const { dialog } = await opened();
    await within(dialog).findByRole("form", { name: ANSWER });
    fireEvent.pointerDown(document.body);
    expect(screen.getByRole("dialog", { name: ANSWER })).toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(said.sent()).toEqual([]);
  });

  it("says where the question closed while it was open, and keeps what was typed", async () => {
    const said = core();
    render(<App />);
    const { dialog } = await opened();
    const box = await within(dialog).findByRole("textbox", { name: "Your answer" });
    await userEvent.type(box, "Half an answer");

    await said.answeredElsewhere();

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      noLongerAsking("read the logs"),
    );
    expect(box).toHaveValue("Half an answer");
    expect(within(dialog).getByRole("button", { name: "Send" })).toBeDisabled();
    expect(said.sent()).toEqual([]);
  });
});
