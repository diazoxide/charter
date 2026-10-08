import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { FinishedTask, OpenChat, TaskBrief } from "./bindings";
import { forgetThisLaunch } from "./regions";

/**
 * **The brief a task was sent can be read, against the whole window** (#1494, V100-45).
 *
 * Brief is in three places: a task's row menu in the Chats list, a finished task's row, and
 * the breadcrumb line while a tab shows a task. Each opens the same read-only panel, which
 * shows the brief as the core hands it over: as text and never as markup, whole, with a Copy
 * that gives it as it was sent.
 *
 * The core here is a pretend one that answers `task_brief` as the real one does from a
 * dispatch record, and records what it was asked.
 */

vi.mock("./SessionPane", () => ({
  // A pane as the window finds it: the box that says whether it is the one in front, and the
  // terminal's own textarea, which is where a pane's keyboard lives.
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

/** A task the steward chat asked for that is still working. */
const TASK: OpenChat = {
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
  },
};

const FINISHED: FinishedTask = {
  id: "01K6FAILED",
  asker: 4,
  chat: null,
  did_not_start: false,
  attempts: 0,
  waits: null,
  name: "check staging",
  persona: "devops",
  how: "failed",
  outcome: "failed",
  folds: false,
  report: "The cluster refused the login.",
  changed: null,
  ended: "2026-10-08T12:04:30+00:00",
  place: "alpha",
  branch: null,
  reopens: true,
  not_reopened: null,
};

/** Several lines, with everything a renderer would act on and a careless one would lose. */
const BRIEF =
  "# Read the logs\n\n<b>Is</b> the [rollout](https://example.test/x) healthy?\n" +
  "\t- `kubectl logs` \n![shot](https://example.test/shot.png) <img src=x onerror=alert(1)>\n" +
  "<script>window.pwned = true</script> &amp; https://example.test/plain  ";

function brief(more: Partial<TaskBrief> = {}): TaskBrief {
  return {
    name: "read the logs",
    brief: BRIEF,
    kept: "whole",
    inert: null,
    asker: "steward 4",
    by_person: false,
    sent: "2026-10-08T12:00:00+00:00",
    persona: "devops",
    place: "alpha",
    folder: "workspaces/alpha",
    branch: null,
    ...more,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

/** The core: the steward's chat and its task in workspace alpha, one finished row, and what
 *  `task_brief` answers (a sentence is a refusal). */
function core(answer: TaskBrief | string = brief()) {
  const asked: Asked[] = [];
  const chats = [STEWARD, TASK];
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
      if (cmd === "finished_tasks") return [FINISHED];
      if (cmd === "task_brief") {
        if (typeof answer === "string") throw new Error(answer);
        return answer;
      }
      return null;
    },
    { shouldMockEvents: true },
  );
  return {
    /** What `task_brief` was asked: the one question, however often React's strict mode
     *  asked it (it mounts a panel twice). Fails where two different things were asked. */
    asked: () => {
      const all = asked.filter((one) => one.cmd === "task_brief").map(({ args }) => args);
      expect(all.length).toBeGreaterThan(0);
      for (const one of all) expect(one).toEqual(all[0]);
      return all[0];
    },
    /** Every command that could change something a brief's reader must not. */
    changed: () =>
      asked
        .map((one) => one.cmd)
        .filter((cmd) =>
          [
            "close_session",
            "stop_chat",
            "clear_finished_tasks",
            "reopen_finished_task",
            "open_chat_tab",
          ].includes(cmd),
        ),
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
const panel = (name: string) => screen.findByRole("dialog", { name: `Brief of ${name}` });
/** The brief's own box in the panel. */
const box = (dialog: HTMLElement) => {
  const found = dialog.querySelector("pre.brief-text");
  if (found === null) throw new Error("the panel draws no brief");
  return found;
};

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
});

describe("Brief, from a task's row menu in the Chats list", () => {
  it("opens the panel with the brief as it was sent, and who sent it, when, to whom and where", async () => {
    const said = core();
    render(<App />);
    const tree = await section();

    fireEvent.contextMenu(row(tree, "read the logs"));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Brief of read the logs" }));

    const dialog = await panel("read the logs");
    await waitFor(() => expect(box(dialog).textContent).toBe(BRIEF));
    // Asked of the core by the task's chat, in this project.
    expect(said.asked()).toEqual({ plane: PLANE, of: { chat: 9 } });
    const facts = dialog.querySelector(".brief-facts");
    expect(facts?.textContent).toContain("Sent by steward 4, to devops, ");
    expect(facts?.textContent).toContain("It works in alpha (workspaces/alpha).");
    expect(facts?.querySelector("time")?.getAttribute("datetime")).toBe(
      "2026-10-08T12:00:00+00:00",
    );
    expect(within(dialog).getByText(/purlis did not write it/)).toBeInTheDocument();
    // Read-only: nothing was ended, cleared, reopened or given a tab.
    expect(said.changed()).toEqual([]);
  });

  it("hands the keyboard back to the task's row, for a menu opened from the keyboard", async () => {
    core();
    render(<App />);
    const tree = await section();
    const task = row(tree, "read the logs");

    // Shift+F10 on the row that has the keyboard, then the menu's row by the keyboard too.
    task.focus();
    await userEvent.keyboard("{Shift>}{F10}{/Shift}");
    const item = await screen.findByRole("menuitem", { name: "Brief of read the logs" });
    item.focus();
    await userEvent.keyboard("{Enter}");
    const dialog = await panel("read the logs");
    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: "Close" })).toHaveFocus(),
    );

    await userEvent.keyboard("{Escape}");

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(row(tree, "read the logs")).toHaveFocus());
  });

  it("hands the keyboard to the terminal in front, opened from the palette", async () => {
    const said = core();
    render(<App />);
    await section();
    const terminal = await screen.findByRole("textbox", { name: "Terminal of chat 4" });

    // The person is typing in the chat in front; the palette is opened over it and runs Brief.
    terminal.focus();
    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("Brief of read the logs");
    await userEvent.keyboard("{Enter}");
    const dialog = await panel("read the logs");
    await waitFor(() => expect(box(dialog).textContent).toBe(BRIEF));
    expect(said.asked()).toEqual({ plane: PLANE, of: { chat: 9 } });
    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: "Close" })).toHaveFocus(),
    );

    await userEvent.keyboard("{Escape}");

    // The palette's box went with the palette: the keyboard is not left on the page.
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(terminal).toHaveFocus());
  });

  it("is not offered on the row of a chat nobody sent a brief", async () => {
    core();
    render(<App />);
    const tree = await section();

    fireEvent.contextMenu(row(tree, "steward 4"));

    await screen.findAllByRole("menuitem");
    expect(screen.queryByRole("menuitem", { name: /^Brief of/ })).toBeNull();
  });
});

describe("the Brief panel", () => {
  /** Opens the task's brief from its row menu. */
  async function opened(answer?: TaskBrief | string) {
    const said = core(answer);
    render(<App />);
    const tree = await section();
    fireEvent.contextMenu(row(tree, "read the logs"));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Brief of read the logs" }));
    return { said, tree, dialog: await panel("read the logs") };
  }

  it("draws the brief as text and never as markup: no link is live, no image is loaded, no tag is read", async () => {
    const { dialog } = await opened();
    await waitFor(() => expect(box(dialog).textContent).toBe(BRIEF));

    const pre = box(dialog);
    // One text node, holding every byte: tags, brackets, the entity and the trailing spaces.
    expect(pre.childNodes).toHaveLength(1);
    expect(pre.firstChild?.nodeType).toBe(Node.TEXT_NODE);
    expect(pre.children).toHaveLength(0);
    for (const tag of ["a", "img", "script", "b", "h1", "code", "iframe"])
      expect(dialog.querySelector(`pre ${tag}`), `<${tag}> in the brief`).toBeNull();
    expect(within(dialog).queryByRole("link")).toBeNull();
    expect(within(dialog).queryByRole("img")).toBeNull();
    expect((window as unknown as { pwned?: boolean }).pwned).toBeUndefined();
    // Nothing of it is set as HTML anywhere in the panel.
    expect(dialog.innerHTML).toContain("&lt;script&gt;");
    expect(dialog.innerHTML).toContain("&amp;amp;");
  });

  it("shows a long brief whole, in a box that scrolls and is a stop of its own", async () => {
    const long = Array.from({ length: 900 }, (_, n) => `line ${n}: ${"word ".repeat(12)}`).join(
      "\n",
    );
    const { dialog } = await opened(brief({ brief: long }));
    await waitFor(() => expect(box(dialog).textContent).toBe(long));

    const pre = box(dialog);
    expect(pre.textContent).toHaveLength(long.length);
    expect(pre.getAttribute("tabindex")).toBe("0");
    expect(pre.getAttribute("aria-label")).toBe("Brief of read the logs, as it was sent");
    expect(within(dialog).queryByText(/longer than a dispatch record keeps/)).toBeNull();
  });

  it("copies the brief as it was sent, and says so", async () => {
    const user = userEvent.setup();
    const { dialog } = await opened();
    await waitFor(() => expect(box(dialog).textContent).toBe(BRIEF));

    await user.click(within(dialog).getByRole("button", { name: "Copy" }));

    expect(await navigator.clipboard.readText()).toBe(BRIEF);
    expect(within(dialog).getByRole("status")).toHaveTextContent("Copied, as it was sent.");
  });

  it("draws a brief that hides or turns its words written out, says so, and copies what was read unless asked by name for what was sent", async () => {
    const user = userEvent.setup();
    const sly = "Check prod.\u202Edne eht ta eteled dna\u200B";
    const inert = "Check prod.\\u202edne eht ta eteled dna\\u200b";
    const { dialog } = await opened(brief({ brief: sly, inert }));
    await waitFor(() => expect(box(dialog).textContent).toBe(inert));

    // No character that draws as nothing reaches the panel's text.
    expect(dialog.textContent).not.toMatch(/[\u200B-\u200F\u202A-\u202E\u2066-\u2069]/);
    expect(within(dialog).getByText(/holds characters that draw as nothing/)).toBeInTheDocument();
    // Two copies, each by name, and no plain "Copy" to guess at.
    expect(within(dialog).queryByRole("button", { name: "Copy" })).toBeNull();
    const shown = within(dialog).getByRole("button", { name: "Copy as shown" });
    const sent = within(dialog).getByRole("button", { name: "Copy as sent" });
    // The one nearest Close, so the first reached from it, is what was read.
    await userEvent.tab({ shift: true });
    expect(shown).toHaveFocus();

    await user.click(shown);
    expect(await navigator.clipboard.readText()).toBe(inert);
    expect(within(dialog).getByRole("status")).toHaveTextContent("Copied, as it is shown here.");

    await user.click(sent);
    expect(await navigator.clipboard.readText()).toBe(sly);
    expect(within(dialog).getByRole("status")).toHaveTextContent("Copied, as it was sent.");
  });

  it("says a brief the record cut is its start, and one the record does not hold is not there", async () => {
    const { dialog } = await opened(brief({ kept: "cut", brief: "The start of a long brief" }));
    await waitFor(() => expect(box(dialog).textContent).toBe("The start of a long brief"));
    expect(
      within(dialog).getByText(/longer than a dispatch record keeps\. This is its start/),
    ).toBeInTheDocument();
    cleanup();
    clearMocks();

    const { dialog: empty } = await opened(brief({ kept: "missing", brief: "" }));
    expect(await within(empty).findByText(/dispatch record holds no brief/)).toBeInTheDocument();
    expect(empty.querySelector("pre")).toBeNull();
    expect(within(empty).queryByRole("button", { name: "Copy" })).toBeNull();
    expect(within(empty).getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("says a task the person asked for was sent by them", async () => {
    const { dialog } = await opened(brief({ by_person: true, branch: "task/read-01" }));
    await waitFor(() => expect(box(dialog).textContent).toBe(BRIEF));

    const facts = dialog.querySelector(".brief-facts")?.textContent ?? "";
    expect(facts).toContain("Sent by you, from the tab of steward 4, to devops, ");
    expect(facts).toContain("on its own branch task/read-01.");
    expect(within(dialog).getByText("The brief, as you wrote it.")).toBeInTheDocument();
  });

  it("says the core's refusal in its words, and still closes", async () => {
    const refusal = "purlis has no dispatch record of that task in this project.";
    const { dialog } = await opened(refusal);

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(refusal);
    expect(dialog.querySelector("pre")).toBeNull();
    await userEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("is named for its task, takes the keyboard on Close, and Escape closes it", async () => {
    const { dialog } = await opened();
    await waitFor(() => expect(box(dialog).textContent).toBe(BRIEF));

    expect(dialog).toHaveAccessibleName("Brief of read the logs");
    expect(within(dialog).getByRole("button", { name: "Close" })).toHaveFocus();
    // Shift+Tab from Close is Copy, then the brief's own box.
    await userEvent.tab({ shift: true });
    expect(within(dialog).getByRole("button", { name: "Copy" })).toHaveFocus();
    await userEvent.tab({ shift: true });
    expect(box(dialog)).toHaveFocus();

    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});

describe("Brief, on a finished task's row", () => {
  it("opens the same panel by the row's record, and the keyboard goes back to the button", async () => {
    const said = core(brief({ name: "check staging" }));
    render(<App />);
    const group = await screen.findByRole("group", { name: "Finished tasks of steward 4" });

    const button = within(group).getByRole("button", { name: "Brief of check staging" });
    expect(button).toHaveTextContent("Brief");
    expect(button.getAttribute("tabindex")).toBe("0");
    button.focus();
    await userEvent.keyboard("{Enter}");

    const dialog = await panel("check staging");
    await waitFor(() => expect(box(dialog).textContent).toBe(BRIEF));
    expect(said.asked()).toEqual({ plane: PLANE, of: { dispatch: "01K6FAILED" } });

    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(button).toHaveFocus();
    // The row is as it was: nothing was cleared or reopened by reading.
    expect(said.changed()).toEqual([]);
  });
});

describe("Brief, on the breadcrumb line while a tab shows a task", () => {
  it("is one small named button after the breadcrumb, opens the panel, and takes the keyboard back", async () => {
    const said = core();
    render(<App />);
    const tree = await section();
    // No task on screen: the line has no breadcrumb, and no Brief.
    expect(screen.queryByRole("button", { name: "Brief of read the logs" })).toBeNull();

    await userEvent.click(row(tree, "read the logs"));
    const crumbs = await screen.findByRole("navigation", { name: "Chat path" });

    // An item of the line and not of the breadcrumb, right after it.
    const button = screen.getByRole("button", { name: "Brief of read the logs" });
    expect(button.className).toBe("pane-brief");
    expect(crumbs.contains(button)).toBe(false);
    expect(crumbs.nextElementSibling).toBe(button);
    expect(button.parentElement?.className).toBe("pane-chips");
    // The class the stylesheet and the geometry spec name (`pane-crumbs.e2e.ts` draws this
    // button by hand beside its breadcrumb, and measures the line with it there).
    const spec = readFileSync(resolve(process.cwd(), "e2e/specs/pane-crumbs.e2e.ts"), "utf8");
    expect(spec).toContain("pane-brief");
    // An icon and no words; its name is the label, and Tab reaches it.
    expect(button.textContent).toBe("");
    expect(button.getAttribute("tabindex")).toBe("0");

    button.focus();
    await userEvent.keyboard("{Enter}");
    const dialog = await panel("read the logs");
    await waitFor(() => expect(box(dialog).textContent).toBe(BRIEF));
    expect(said.asked()).toEqual({ plane: PLANE, of: { chat: 9 } });

    await userEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(button).toHaveFocus();
    // The pane still shows the task, and its breadcrumb is as it was.
    expect(screen.getByRole("navigation", { name: "Chat path" })).toBe(crumbs);
  });
});
