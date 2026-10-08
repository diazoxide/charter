import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { FinishedTask, OpenChat } from "./bindings";
import { forgetThisLaunch } from "./regions";

/**
 * **A chat's finished tasks, against the whole window** (#1485): a task ends at its report, and
 * its row stays under the chat that asked. Done and cancelled fold into one "Finished (n)" line
 * with Clear finished; a failure stays a row of its own. A row shows its report as text, and
 * Reopen makes it an ordinary chat with a tab.
 *
 * The core here is a pretend one that holds the finished rows as the real one reads them from
 * its dispatch records: `finished_tasks` lists them, `clear_finished_tasks` takes rows away,
 * and `reopen_finished_task` starts a chat and takes the row.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
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
const WORKING: OpenChat = {
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

function finished(id: string, name: string, more: Partial<FinishedTask> = {}): FinishedTask {
  return {
    id,
    asker: 4,
    name,
    persona: "devops",
    outcome: "done",
    folds: true,
    report: `${name}: all good.`,
    changed: null,
    ended: "2026-10-08T12:04:30+00:00",
    place: "alpha",
    branch: null,
    reopens: true,
    ...more,
  };
}

/** Five that came out done, as the five "live check" tasks did, and one that failed. */
const FIVE_DONE = ["talk", "listen", "read", "write", "count"].map((name, at) =>
  finished(`01K6DONE${at}`, `live check ${name}`),
);
const FAILED = finished("01K6FAILED", "check staging", {
  outcome: "failed",
  folds: false,
  report: "The cluster refused the login.\nNothing was changed.",
});

type Asked = { cmd: string; args: Record<string, unknown> };

/** The core, with `open` chats in workspace alpha and `rows` finished under them. */
function core(rows: FinishedTask[], open: OpenChat[] = [STEWARD], refuses?: string) {
  const asked: Asked[] = [];
  let listed = [...rows];
  const chats = [...open];
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
      if (cmd === "finished_tasks") return listed;
      if (cmd === "clear_finished_tasks") {
        const ids = a.ids as string[];
        const before = listed.length;
        listed = listed.filter((row) => !ids.includes(row.id));
        return before - listed.length;
      }
      if (cmd === "reopen_finished_task") {
        if (refuses !== undefined) throw new Error(refuses);
        const row = listed.find((one) => one.id === a.id);
        listed = listed.filter((one) => one.id !== a.id);
        // An ordinary chat: nobody asked for it.
        const chat: OpenChat = {
          ...STEWARD,
          session: 21,
          name: "21",
          in_front: false,
          persona: row?.persona ?? null,
          label: row?.name ?? null,
          resumed: "9f2c-the-conversation",
          from: null,
        };
        chats.push(chat);
        return chat;
      }
      return null;
    },
    { shouldMockEvents: true },
  );
  return {
    asked: (cmd: string) => asked.filter((one) => one.cmd === cmd).map(({ args }) => args),
    /** Every command that ends, closes or stops a chat, in the order it was asked. */
    ended: () =>
      asked
        .map((one) => one.cmd)
        .filter((cmd) =>
          ["close_session", "close_chat_stopping", "smart_close", "stop_chat"].includes(cmd),
        ),
  };
}

const section = () => screen.findByRole("tree", { name: "Chats of this project" });
const theirs = () => screen.findByRole("group", { name: "Finished tasks of steward 4" });
/** The finished row named `name`: the button its report opens on, or none. */
const finishedRow = (group: HTMLElement, name: string) =>
  within(group)
    .queryAllByRole("button")
    .find((one) => one.querySelector(".session")?.textContent === name);

/** The same, where it must be there. */
const theRow = (group: HTMLElement, name: string) => {
  const found = finishedRow(group, name);
  if (found === undefined) throw new Error(`no finished row is named ${name}`);
  return found;
};

const tabNames = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .queryAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
});

describe("a chat's finished tasks", () => {
  it("shows five done tasks as one Finished (5) line and a failed one as a row of its own", async () => {
    core([...FIVE_DONE, FAILED]);
    render(<App />);
    const group = await theirs();

    // One line for the five, and none of them drawn until it is opened.
    const fold = within(group).getByRole("button", { name: "Finished (5)" });
    expect(fold).toHaveAttribute("aria-expanded", "false");
    expect(within(group).queryByText("live check talk")).toBeNull();
    // The failure is never behind the count: its own row, with its word.
    const failed = theRow(group, "check staging");
    expect(failed).toHaveTextContent("failed");
    expect(within(group).getByRole("button", { name: "Clear finished" })).toBeInTheDocument();

    // Opened, the five are there, each with how it ended.
    await userEvent.click(fold);
    expect(fold).toHaveAttribute("aria-expanded", "true");
    for (const task of FIVE_DONE) expect(theRow(group, task.name)).toHaveTextContent("done");
    // None of them is a chat: the tree lists the one that is.
    const tree = await section();
    expect(within(tree).getAllByRole("treeitem")).toHaveLength(1);
  });

  it("folds a cancelled task with the done ones, and keeps every other end out of the fold", async () => {
    core([
      finished("01K6A", "counted", { outcome: "done" }),
      finished("01K6B", "called off", { outcome: "cancelled" }),
      finished("01K6C", "stuck", { outcome: "blocked", folds: false }),
      finished("01K6D", "died", { outcome: "ended without a report", folds: false }),
      finished("01K6E", "shut", { outcome: "closed by the person", folds: false }),
    ]);
    render(<App />);
    const group = await theirs();

    expect(within(group).getByRole("button", { name: "Finished (2)" })).toBeInTheDocument();
    expect(theRow(group, "stuck")).toHaveTextContent("blocked");
    expect(theRow(group, "died")).toHaveTextContent("ended without a report");
    expect(theRow(group, "shut")).toHaveTextContent("closed by the person");
    expect(within(group).queryByText("counted")).toBeNull();
    expect(within(group).queryByText("called off")).toBeNull();
  });

  it("clears the finished rows and nothing else: the failure stays, and no chat is touched", async () => {
    const said = core([...FIVE_DONE, FAILED]);
    render(<App />);
    const group = await theirs();

    await userEvent.click(within(group).getByRole("button", { name: "Clear finished" }));

    // The five rows, by their records' ids, in one ask.
    await waitFor(() =>
      expect(said.asked("clear_finished_tasks")).toEqual([
        { plane: PLANE, ids: FIVE_DONE.map((task) => task.id) },
      ]),
    );
    await waitFor(() =>
      expect(within(group).queryByRole("button", { name: /^Finished/ })).toBeNull(),
    );
    expect(within(group).queryByRole("button", { name: "Clear finished" })).toBeNull();
    // The failed row is still its own row, until it is cleared itself.
    expect(theRow(group, "check staging")).toBeInTheDocument();
    // Rows only: nothing was closed, stopped or reopened, and the steward chat is as it was.
    expect(said.ended()).toEqual([]);
    expect(said.asked("reopen_finished_task")).toEqual([]);
    expect(tabNames()).toEqual(["steward 4"]);

    await userEvent.click(within(group).getByRole("button", { name: "Clear check staging" }));

    await waitFor(() =>
      expect(said.asked("clear_finished_tasks")[1]).toEqual({ plane: PLANE, ids: [FAILED.id] }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("group", { name: "Finished tasks of steward 4" })).toBeNull(),
    );
  });

  it("shows a finished row's report on a press, as text and never as markup", async () => {
    core([
      finished("01K6X", "check prod", {
        outcome: "failed",
        folds: false,
        report:
          '<b>Healthy</b> <img src=x onerror="alert(1)">\n# Not a heading\n[a link](https://example.com)',
        changed: "values.yaml: **replicas** 2 to 3",
        branch: "check-prod-b5rc0def",
      }),
    ]);
    render(<App />);
    const group = await theirs();
    const name = theRow(group, "check prod");
    // Its first line on hover; nothing of it drawn until it is opened.
    expect(name).toHaveAttribute("title", '<b>Healthy</b> <img src=x onerror="alert(1)">');
    expect(screen.queryByRole("region", { name: "Report of check prod" })).toBeNull();

    await userEvent.click(name);

    const report = screen.getByRole("region", { name: "Report of check prod" });
    // Every character as written, line breaks and all.
    expect(report.querySelector(".report-text")?.textContent).toBe(
      '<b>Healthy</b> <img src=x onerror="alert(1)">\n# Not a heading\n[a link](https://example.com)',
    );
    expect(report).toHaveTextContent("Changed: values.yaml: **replicas** 2 to 3");
    expect(report).toHaveTextContent("alpha · own branch check-prod-b5rc0def");
    // And nothing in it became an element.
    expect(report.querySelector("b, img, h1, a, strong")).toBeNull();

    await userEvent.click(name);
    expect(screen.queryByRole("region", { name: "Report of check prod" })).toBeNull();
  });

  it("reopens a finished task as an ordinary chat with a tab, and its row goes", async () => {
    const said = core([FAILED]);
    render(<App />);
    const group = await theirs();
    expect(tabNames()).toEqual(["steward 4"]);

    await userEvent.click(within(group).getByRole("button", { name: "Reopen check staging" }));

    await waitFor(() =>
      expect(said.asked("reopen_finished_task")).toEqual([
        { plane: PLANE, id: FAILED.id, columns: expect.any(Number), rows: expect.any(Number) },
      ]),
    );
    // A tab of its own, named as the task was, beside the chat that asked.
    await waitFor(() => expect(tabNames()).toEqual(["steward 4", "check staging"]));
    // Its row has gone: it is a chat now, and no finished task.
    await waitFor(() =>
      expect(screen.queryByRole("group", { name: "Finished tasks of steward 4" })).toBeNull(),
    );
    // And nothing was ended to make it.
    expect(said.ended()).toEqual([]);
    expect(said.asked("clear_finished_tasks")).toEqual([]);
  });

  it("says why on the row when a task cannot be reopened, and offers no Reopen where there is no conversation", async () => {
    core(
      [FAILED, finished("01K6N", "no conversation", { folds: false, reopens: false })],
      [STEWARD],
      "'check staging' cannot be reopened: the folder it worked in is gone (workspaces/alpha). Its report is still here to read.",
    );
    render(<App />);
    const group = await theirs();

    expect(within(group).getByRole("button", { name: "Reopen no conversation" })).toBeDisabled();
    await userEvent.click(within(group).getByRole("button", { name: "Reopen check staging" }));

    expect(await within(group).findByRole("alert")).toHaveTextContent(
      "'check staging' cannot be reopened: the folder it worked in is gone (workspaces/alpha). Its report is still here to read.",
    );
    // The row is still there, and no tab was opened.
    expect(theRow(group, "check staging")).toBeInTheDocument();
    expect(tabNames()).toEqual(["steward 4"]);
  });

  it("draws them under the chat that asked, after its running tasks, and folds them away with it", async () => {
    core([...FIVE_DONE, FAILED], [STEWARD, WORKING]);
    render(<App />);
    const tree = await section();
    const group = await theirs();

    // Inside the tree, after the task still working under the steward chat.
    const working = within(tree).getByRole("treeitem", { name: /read the logs/ });
    expect(tree).toContainElement(group);
    expect(working.compareDocumentPosition(group) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    // Folding the steward chat's row hides everything under it.
    const steward = within(tree).getByRole("treeitem", { name: /steward 4/ });
    steward.focus();
    await userEvent.keyboard("{ArrowLeft}");
    await waitFor(() =>
      expect(screen.queryByRole("group", { name: "Finished tasks of steward 4" })).toBeNull(),
    );
    await userEvent.keyboard("{ArrowRight}");
    expect(await theirs()).toBeInTheDocument();
  });

  it("draws nothing for a chat with no finished tasks", async () => {
    core([]);
    render(<App />);
    await section();

    await waitFor(() => expect(screen.getAllByTestId("pane").length).toBeGreaterThan(0));
    expect(screen.queryByRole("group", { name: /Finished tasks/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Finished \(/ })).toBeNull();
  });
});
