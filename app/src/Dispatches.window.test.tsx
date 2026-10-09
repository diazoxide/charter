import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  configure,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { DispatchRow, OpenChat, SessionRecordRow, WorktreeLoss } from "./bindings";

/**
 * **The Dispatches tab** (#1452) against the whole window: it is offered from the Sessions
 * panel's heading and from the palette, lists the project's dispatches with what each cost,
 * narrows by persona and by asking chat, and a row opens the persona chat while it is open, else
 * the session record that chat wrote. And a session record's own tab lists the dispatches its
 * chat made.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane" tabIndex={-1}>
      session {session}
    </div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);
configure({ asyncUtilTimeout: 5_000 });

beforeEach(() => globalThis.localStorage.clear());
afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;
const QA_RECORD = "workspaces/alpha/sessions/20261007-121500-test-the-rollout.md";
const ASKERS_RECORD = "workspaces/alpha/sessions/20261007-130000-ship-the-rollout.md";

/** The persona chat a running dispatch started, still open in this window. */
const DEVOPS_CHAT: OpenChat = {
  session: 7,
  name: "7",
  cwd: ALPHA,
  harness: "claude",
  in_front: true,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona: "devops",
  unreported: null,
  card: null,
  guessed: null,
  pinned: false,
  label: "check prod",
  from: {
    name: "steward 3",
    workspace: "alpha",
    chat: 3,
    task: false,
    tab: true,
    reported: false,
    unreported: false,
  },
};

const SIDEBAR = {
  root: PLANE,
  workspaces: [{ name: "alpha", path: ALPHA, vision: "Ship it", todos: [], chats: [DEVOPS_CHAT] }],
  personas: ["steward", "devops", "qa"],
  persona: "steward",
  unfiled: [],
};

function dispatch(over: Partial<DispatchRow> & Pick<DispatchRow, "id" | "task">): DispatchRow {
  return {
    mode: "handoff",
    persona: "devops",
    asker: "steward 3",
    asker_key: "01K6STEWARD",
    asker_persona: "steward",
    by_person: false,
    place: "alpha",
    folder: "workspaces/alpha",
    outcome: "done",
    started: "2026-10-07T12:00:00+00:00",
    ended: "2026-10-07T12:04:30+00:00",
    duration: "4m 30s",
    needed_you: 0,
    messages: 0,
    cost: null,
    tokens: null,
    brief: "the brief",
    report: null,
    changed: null,
    open_session: null,
    session_record: null,
    worktree: null,
    ...over,
  };
}

/** Newest first, as the core lists them: one running, one done with a record, one gone. */
const ROWS: DispatchRow[] = [
  dispatch({
    id: "01K6C",
    task: "check prod",
    outcome: "running",
    ended: null,
    duration: "2m 5s",
    needed_you: 2,
    open_session: 7,
    brief: "Is the rollout healthy?",
  }),
  dispatch({
    id: "01K6B",
    task: "test the rollout",
    persona: "qa",
    outcome: "done",
    cost: "$0.42",
    tokens: "15k in, 4k out",
    report: "All 12 checks pass.",
    session_record: QA_RECORD,
  }),
  dispatch({
    id: "01K6A",
    task: "rotate the key",
    asker: "planner 2",
    asker_key: "#2",
    asker_persona: null,
    outcome: "failed",
    report: "ended without a report",
  }),
];

const ASKERS: SessionRecordRow = {
  path: ASKERS_RECORD,
  title: "Ship the rollout",
  when: "2026-10-07 13:00",
  persona: "steward",
  harness: "claude",
  resumable: false,
};

function sessionsPanel(records: SessionRecordRow[]) {
  return {
    key: "charter/sessions",
    title: "Sessions",
    order: 30,
    mark: "note",
    from: null,
    about: null,
    blocks: [
      {
        kind: "list",
        rows: records.map((one) => ({
          key: one.path,
          text: one.title,
          note: one.when,
          mark: "note",
          tone: "plain",
          detail: null,
          runs: `session.open:${one.path}`,
          actions: [],
        })),
        empty: { headline: "No session records yet", body: null, offer: null },
      },
    ],
  };
}

function core(
  on: {
    rows?: DispatchRow[] | Error;
    undrawn?: number;
    /** What the core says a discard would lose, or its refusal to ask. */
    loss?: WorktreeLoss | Error;
    /** The core's refusal of the discard itself. */
    discardRefused?: string;
  } = {},
) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plugin:event|listen") return 1;
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "opened_chats") return [DEVOPS_CHAT];
    if (cmd === "reopened_views") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "extension_views") return [];
    if (cmd === "extension_commands") return [];
    if (cmd === "extension_panels") return [];
    if (cmd === "extensions_on") return [];
    if (cmd === "project_theme_drawn") return null;
    if (cmd === "workspace_panels")
      return {
        workspace: "alpha",
        repos: [],
        paths: {},
        absent: [],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["steward", "devops", "qa"],
        persona: "steward",
        sessions: [ASKERS],
        contributed: [sessionsPanel([ASKERS])],
      };
    if (cmd === "workspace_repos") return { workspace: "alpha", repos: [], cache_refused: null };
    if (cmd === "dispatches") {
      const rows = on.rows ?? ROWS;
      if (rows instanceof Error) throw rows.message;
      return { rows, undrawn: on.undrawn ?? 0 };
    }
    if (cmd === "dispatch_worktree_loss") {
      if (on.loss === undefined) throw "no worktree was asked about";
      if (on.loss instanceof Error) throw on.loss.message;
      return on.loss;
    }
    if (cmd === "task_changes")
      return {
        id: given.id,
        task: "fix the queue",
        running: false,
        own: { repo: "api", branch: "fix-the-queue-b5rc0def", standing: "kept", acts: true },
        places: [],
        elsewhere: [],
        more: false,
        said: null,
        unknown: null,
      };
    if (cmd === "dispatch_worktree_discard") {
      if (on.discardRefused !== undefined) throw on.discardRefused;
      return null;
    }
    if (cmd === "session_record")
      return {
        row:
          given.path === ASKERS_RECORD
            ? ASKERS
            : { ...ASKERS, path: QA_RECORD, title: "Test the rollout", persona: "qa" },
        place: "alpha",
        body: "# The record\n\n## Goal\n\nShip it.\n",
        persona_hosts: [],
        resume_holds: false,
        persona_hosts_locked: null,
        persona_hosts_wait: false,
        dispatches:
          given.path === ASKERS_RECORD
            ? [
                { persona: "qa", task: "test the rollout", outcome: "done" },
                { persona: "devops", task: "check prod", outcome: "running" },
              ]
            : [],
      };
    return null;
  });
  return { asked };
}

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const selected = () => within(strip()).getByRole("tab", { selected: true }).textContent ?? "";

/** Opens the Dispatches tab from the Sessions panel's heading, and answers its table. */
async function opened(): Promise<HTMLElement> {
  const panel = await screen.findByTestId("panel-sessions");
  await userEvent.click(await within(panel).findByRole("button", { name: "Open dispatches" }));
  return screen.findByRole("table", { name: "Dispatches" });
}

/** The tasks the table lists, top to bottom. */
function tasks(table: HTMLElement): string[] {
  return within(table)
    .getAllByRole("row")
    .slice(1)
    .map((row) => within(row).getAllByRole("cell")[0].textContent ?? "");
}

describe("the Dispatches tab", () => {
  it("opens from the Sessions panel and lists every dispatch with its columns", async () => {
    const { asked } = core();
    render(<App />);

    const table = await opened();

    expect(selected()).toContain("Dispatches");
    expect(asked.filter((one) => one.cmd === "dispatches").map((one) => one.args)).toContainEqual({
      plane: PLANE,
    });
    expect(
      within(table)
        .getAllByRole("columnheader")
        .map((head) => head.textContent),
    ).toEqual([
      "Task",
      "Persona",
      "Asked by",
      "Where",
      "Outcome",
      "Duration",
      "Needed you",
      "Cost (reported)",
    ]);
    expect(tasks(table)).toEqual(["check prod", "test the rollout", "rotate the key"]);
    const cells = (id: string) =>
      within(screen.getByTestId(`dispatch-${id}`))
        .getAllByRole("cell")
        .map((cell) => cell.textContent);
    expect(cells("01K6C")).toEqual([
      "check prod",
      "devops",
      "steward 3",
      "alpha",
      "running",
      "2m 5s",
      "2",
      "not reported",
    ]);
    // A cost where the harness reported one, and words, never a zero, where it did not.
    expect(cells("01K6B")[7]).toBe("$0.42");
    expect(cells("01K6A")[7]).toBe("not reported");
    expect(screen.queryByText("$0.00")).toBeNull();
  });

  it("is offered from the palette", async () => {
    core();
    render(<App />);
    await screen.findByTestId("panel-sessions");

    await userEvent.keyboard("{F2}");
    const palette = await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.type(within(palette).getByRole("combobox"), "dispatches");
    await userEvent.click(await within(palette).findByRole("option", { name: /Open dispatches/ }));

    expect(await screen.findByRole("table", { name: "Dispatches" })).toBeInTheDocument();
    expect(selected()).toContain("Dispatches");
  });

  it("filters by persona and by asking chat", async () => {
    core();
    render(<App />);
    const table = await opened();

    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Filter by persona" }),
      "devops",
    );
    expect(tasks(table)).toEqual(["check prod", "rotate the key"]);

    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Filter by asking chat" }),
      screen.getByRole("option", { name: "planner 2" }),
    );
    expect(tasks(table)).toEqual(["rotate the key"]);

    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Filter by persona" }),
      "qa",
    );
    expect(screen.queryByRole("table", { name: "Dispatches" })).toBeNull();
    expect(screen.getByText("No dispatch matches these filters.")).toBeInTheDocument();

    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "Filter by asking chat" }),
      "Every asking chat",
    );
    expect(tasks(screen.getByRole("table", { name: "Dispatches" }))).toEqual(["test the rollout"]);
  });

  it("opens the persona chat from the row of a dispatch whose chat is still open", async () => {
    core();
    render(<App />);
    const table = await opened();
    expect(selected()).toContain("Dispatches");

    await userEvent.click(within(table).getByRole("button", { name: "check prod" }));

    await waitFor(() => expect(selected()).toContain("check prod"));
    expect(selected()).not.toContain("Dispatches");
  });

  it("opens the session record from the row of a dispatch whose chat is closed", async () => {
    const { asked } = core();
    render(<App />);
    const table = await opened();

    await userEvent.click(within(table).getByRole("button", { name: "test the rollout" }));

    await screen.findByTestId("session-record");
    expect(selected()).toContain("Session · test the rollout");
    expect(
      asked.filter((one) => one.cmd === "session_record").map((one) => one.args),
    ).toContainEqual({ plane: PLANE, path: QA_RECORD });
  });

  it("opens nothing from a dispatch whose chat is closed and wrote no record", async () => {
    core();
    render(<App />);
    const table = await opened();

    const gone = within(table).getByText("rotate the key");

    expect(gone.closest("button")).toBeNull();
    expect(gone).toHaveAttribute("title", "Its chat is closed and wrote no session record");
  });

  it("shows a dispatch's brief and report under its row on a press", async () => {
    core();
    render(<App />);
    const table = await opened();
    expect(screen.queryByText("All 12 checks pass.")).toBeNull();

    await userEvent.click(
      within(table).getByRole("button", {
        name: "Show the brief and report of test the rollout",
      }),
    );

    expect(screen.getByText("All 12 checks pass.")).toBeInTheDocument();
    expect(screen.getByText("the brief")).toBeInTheDocument();
    // A running one has no report yet, and says so.
    await userEvent.click(
      within(table).getByRole("button", { name: "Show the brief and report of check prod" }),
    );
    expect(screen.getByText("Is the rollout healthy?")).toBeInTheDocument();
    expect(screen.getByText("Not reported yet.")).toBeInTheDocument();
  });

  it("draws a dispatch's mode, its times and who asked as whom with its brief", async () => {
    core();
    render(<App />);
    const table = await opened();

    await userEvent.click(
      within(table).getByRole("button", {
        name: "Show the brief and report of test the rollout",
      }),
    );
    expect(
      screen.getByText(
        "Handoff · started 2026-10-07 12:00 UTC · ended 2026-10-07 12:04 UTC · asked by steward 3 · as steward",
      ),
    ).toBeInTheDocument();

    await userEvent.click(
      within(table).getByRole("button", {
        name: "Show the brief and report of rotate the key",
      }),
    );
    expect(screen.getByText(/asked by planner 2 · No persona$/)).toBeInTheDocument();
  });

  it("says what a task's report says changed, and how many messages passed", async () => {
    core({
      rows: [
        dispatch({
          id: "01K6T",
          task: "drain the queue",
          mode: "task",
          outcome: "blocked",
          messages: 4,
          report: "Forty are stuck.",
          changed: "svc: 2 files\nbranch fix/queue, 1 commit",
        }),
        dispatch({
          id: "01K6S",
          task: "count the retries",
          mode: "task",
          messages: 1,
          report: "3.",
        }),
      ],
    });
    render(<App />);
    const table = await opened();

    await userEvent.click(
      within(table).getByRole("button", { name: "Show the brief and report of drain the queue" }),
    );

    expect(screen.getByRole("heading", { name: "What it says changed" })).toBeInTheDocument();
    expect(screen.getByText(/svc: 2 files\s+branch fix\/queue, 1 commit/)).toBeInTheDocument();
    expect(screen.getByText(/^Task · .* · as steward · 4 messages$/)).toBeInTheDocument();
    // A report that said nothing of it has no such heading, and one message is one.
    await userEvent.click(
      within(table).getByRole("button", {
        name: "Show the brief and report of count the retries",
      }),
    );
    expect(screen.queryByRole("heading", { name: "What it says changed" })).toBeNull();
    expect(screen.getByText(/ · as steward · 1 message$/)).toBeInTheDocument();
  });

  it("does not say running of a dispatch that has not ended and whose chat is not open", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const { asked } = core({
        rows: [
          dispatch({
            id: "01K6N",
            task: "check staging",
            outcome: "not open",
            ended: null,
            duration: "",
          }),
        ],
      });
      render(<App />);
      const table = await opened();

      const cells = within(screen.getByTestId("dispatch-01K6N")).getAllByRole("cell");
      expect(cells[4]).toHaveTextContent("not open");
      expect(cells[5]).toHaveTextContent("");
      await userEvent.click(
        within(table).getByRole("button", {
          name: "Show the brief and report of check staging",
        }),
      );
      expect(screen.getByText(/ · not ended, and its chat is not open · /)).toBeInTheDocument();
      expect(screen.getByText("Not reported yet.")).toBeInTheDocument();
      // Nothing is running, so the list is not read again on a timer.
      const reads = () => asked.filter((one) => one.cmd === "dispatches").length;
      const before = reads();
      await vi.advanceTimersByTimeAsync(12_000);
      expect(reads()).toBe(before);
    } finally {
      vi.useRealTimers();
    }
  });

  it("says No persona, in those words, of a dispatch that went to none", async () => {
    core({ rows: [dispatch({ id: "01K6Z", task: "tidy the notes", persona: null })] });
    render(<App />);
    const table = await opened();

    expect(within(screen.getByTestId("dispatch-01K6Z")).getAllByRole("cell")[1]).toHaveTextContent(
      "No persona",
    );
    expect(
      within(screen.getByRole("combobox", { name: "Filter by persona" })).getByRole("option", {
        name: "No persona",
      }),
    ).toBeInTheDocument();
    expect(within(table).queryByText("none")).toBeNull();
  });

  it("counts the records purlis will not draw, and shows none of them", async () => {
    core({ undrawn: 2 });
    render(<App />);
    const table = await opened();

    expect(tasks(table)).toHaveLength(3);
    expect(screen.getByTestId("dispatches-undrawn")).toHaveTextContent(
      "2 records purlis will not draw: they hold text purlis refuses to put on the screen.",
    );
  });

  it("says so when every record in the store is one purlis will not draw", async () => {
    core({ rows: [], undrawn: 1 });
    render(<App />);
    const panel = await screen.findByTestId("panel-sessions");
    await userEvent.click(await within(panel).findByRole("button", { name: "Open dispatches" }));

    expect(await screen.findByTestId("dispatches-undrawn")).toHaveTextContent(
      "1 record purlis will not draw: it holds text purlis refuses to put on the screen.",
    );
    expect(screen.queryByTestId("dispatches-empty")).toBeNull();
  });

  it("says there are none yet in a project whose chats have dispatched nothing", async () => {
    core({ rows: [] });
    render(<App />);
    const panel = await screen.findByTestId("panel-sessions");
    await userEvent.click(await within(panel).findByRole("button", { name: "Open dispatches" }));

    expect(await screen.findByTestId("dispatches-empty")).toHaveTextContent("No dispatches yet");
  });

  it("says what purlis could not read and why, and reads again on a press", async () => {
    const { asked } = core({ rows: new Error("the store is a link out of the project") });
    render(<App />);
    const panel = await screen.findByTestId("panel-sessions");
    await userEvent.click(await within(panel).findByRole("button", { name: "Open dispatches" }));

    const notice = await waitFor(() => {
      const found = document.querySelector<HTMLElement>('[data-cause="dispatches-unread"]');
      expect(found).not.toBeNull();
      return found as HTMLElement;
    });
    expect(notice).toHaveTextContent(
      "purlis could not read this project's dispatches: the store is a link out of the project",
    );
    const reads = () => asked.filter((one) => one.cmd === "dispatches").length;
    const before = reads();

    await userEvent.click(within(notice).getByRole("button", { name: "Read again" }));

    await vi.waitFor(() => expect(reads()).toBeGreaterThan(before));
  });
});

describe("a dispatch's own branch", () => {
  const BRANCH = "fix-the-queue-b5rc0def";
  /** A task that worked on a branch of its own: one kept, and one purlis took away. */
  const WITH_BRANCHES: DispatchRow[] = [
    dispatch({
      id: "01K6W",
      task: "fix the queue",
      mode: "task",
      place: `alpha · ${BRANCH}`,
      worktree: { repo: "api", branch: BRANCH, standing: "kept", discard: true },
    }),
    dispatch({
      id: "01K6M",
      task: "tidy the docs",
      mode: "task",
      place: "alpha · tidy-the-docs-00000000",
      worktree: {
        repo: "api",
        branch: "tidy-the-docs-00000000",
        standing: "merged",
        discard: false,
      },
    }),
    dispatch({ id: "01K6P", task: "plain task" }),
  ];
  const LOSS: WorktreeLoss = {
    task: "fix the queue",
    repo: "api",
    branch: BRANCH,
    on: BRANCH,
    changes: ["?? scratch.txt", " M README.md"],
    ignored: ["target/"],
    unmerged: 1,
    lost: [],
  };
  const DISCARD = "Discard the branch folder of fix the queue";
  const ASKS = "Discard this branch's folder?";
  const row = (id: string) => screen.getByTestId(`dispatch-${id}`);

  it("is listed on its row with how it stands, and only a kept one offers Discard", async () => {
    core({ rows: WITH_BRANCHES });
    render(<App />);
    await opened();

    const where = (id: string) => within(row(id)).getAllByRole("cell")[3];
    expect(where("01K6W")).toHaveTextContent(`alpha · ${BRANCH}own branch, folder kept`);
    expect(within(row("01K6W")).getByRole("button", { name: DISCARD })).toBeInTheDocument();
    expect(where("01K6M")).toHaveTextContent("own branch, merged and removed");
    expect(within(row("01K6M")).queryByRole("button", { name: /Discard/ })).toBeNull();
    // A dispatch that had no branch of its own says nothing of one.
    expect(where("01K6P")).toHaveTextContent(/^alpha$/);
    expect(within(row("01K6P")).queryByRole("button", { name: /Discard/ })).toBeNull();
  });

  it("opens a kept branch's Changes tab, where it is merged, from its row (#1534)", async () => {
    const { asked } = core({ rows: WITH_BRANCHES });
    render(<App />);
    await opened();
    const REVIEW = "Review changes of fix the queue";
    expect(within(row("01K6M")).queryByRole("button", { name: /Review changes/ })).toBeNull();
    expect(within(row("01K6P")).queryByRole("button", { name: /Review changes/ })).toBeNull();

    await userEvent.click(within(row("01K6W")).getByRole("button", { name: REVIEW }));

    await waitFor(() => expect(selected()).toContain("Changes · fix the queue"));
    await waitFor(() =>
      expect(asked.find((one) => one.cmd === "task_changes")?.args).toEqual({
        plane: PLANE,
        id: "01K6W",
      }),
    );
  });

  it("asks before it discards, naming every file that would go and what the branch keeps", async () => {
    const { asked } = core({ rows: WITH_BRANCHES, loss: LOSS });
    render(<App />);
    await opened();

    await userEvent.click(within(row("01K6W")).getByRole("button", { name: DISCARD }));

    const question = await screen.findByRole("alertdialog", { name: ASKS });
    expect(asked.find((one) => one.cmd === "dispatch_worktree_loss")?.args).toEqual({
      plane: PLANE,
      id: "01K6W",
    });
    expect(question).toHaveTextContent(
      `This removes the folder of the branch ${BRANCH} in api, which purlis cut for fix the queue, for good. Nothing is merged.`,
    );
    const lost = within(question).getByTestId("discard-loses");
    expect(lost).toHaveTextContent("2 uncommitted files would be lost:");
    expect(lost).toHaveTextContent("?? scratch.txt");
    expect(lost).toHaveTextContent("M README.md");
    expect(lost).toHaveTextContent("1 ignored path goes with it:");
    expect(lost).toHaveTextContent("target/");
    // The commit on the branch is not lost: the branch stays.
    expect(within(question).getByTestId("discard-branch")).toHaveTextContent(
      `No commit is lost: the branch ${BRANCH} holds 1 commit that exists nowhere else, so it stays.`,
    );
    expect(within(question).queryByTestId("discard-loses-nothing")).toBeNull();
    // Nothing has been discarded by asking.
    expect(asked.some((one) => one.cmd === "dispatch_worktree_discard")).toBe(false);

    // Cancel discards nothing.
    await userEvent.click(within(question).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(asked.some((one) => one.cmd === "dispatch_worktree_discard")).toBe(false);
  });

  it("discards what the person was shown, and reads the list again", async () => {
    const { asked } = core({ rows: WITH_BRANCHES, loss: LOSS });
    render(<App />);
    await opened();
    await userEvent.click(within(row("01K6W")).getByRole("button", { name: DISCARD }));
    const question = await screen.findByRole("alertdialog", { name: ASKS });
    const reads = () => asked.filter((one) => one.cmd === "dispatches").length;
    const before = reads();

    await userEvent.click(within(question).getByRole("button", { name: "Discard" }));

    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    // The core is handed back exactly what the question showed, so it removes nothing else.
    expect(asked.find((one) => one.cmd === "dispatch_worktree_discard")?.args).toEqual({
      plane: PLANE,
      id: "01K6W",
      seen: LOSS,
    });
    await vi.waitFor(() => expect(reads()).toBeGreaterThan(before));
  });

  it("says so where nothing in the folder would be lost", async () => {
    core({
      rows: WITH_BRANCHES,
      loss: { ...LOSS, changes: [], ignored: [], unmerged: 0 },
    });
    render(<App />);
    await opened();

    await userEvent.click(within(row("01K6W")).getByRole("button", { name: DISCARD }));

    const question = await screen.findByRole("alertdialog", { name: ASKS });
    expect(within(question).getByTestId("discard-loses-nothing")).toHaveTextContent(
      "The folder holds no uncommitted file and no ignored one, so nothing in it would be lost.",
    );
    expect(within(question).getByTestId("discard-branch")).toHaveTextContent(
      `No commit is lost: the branch ${BRANCH} is removed only if it is already merged.`,
    );
  });

  it("is not asked about while a chat is open in it: the refusal stands as a Notice", async () => {
    const open =
      "A chat is still open in the folder of the branch cut for 'fix the queue'. Close it first: purlis will not remove a folder a chat is working in.";
    const { asked } = core({ rows: WITH_BRANCHES, loss: new Error(open) });
    render(<App />);
    await opened();

    await userEvent.click(within(row("01K6W")).getByRole("button", { name: DISCARD }));

    const kept = "The branch folder of fix the queue was not discarded";
    const notice = await screen.findByRole("status", { name: kept });
    expect(notice).toHaveTextContent(open);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(asked.some((one) => one.cmd === "dispatch_worktree_discard")).toBe(false);
    // It has a way out.
    await userEvent.click(within(notice).getByRole("button", { name: "Dismiss" }));
    await waitFor(() => expect(screen.queryByRole("status", { name: kept })).toBeNull());
  });

  it("names the commits a folder on no branch would lose, and never says none is lost", async () => {
    // #1453 review, M2: its chat detached the folder and committed there.
    core({
      rows: WITH_BRANCHES,
      loss: {
        ...LOSS,
        on: null,
        changes: [],
        ignored: [],
        unmerged: 12,
        lost: ["3a823aab fix the retry", "9f00d1c2 and its test"],
      },
    });
    render(<App />);
    await opened();

    await userEvent.click(within(row("01K6W")).getByRole("button", { name: DISCARD }));

    const question = await screen.findByRole("alertdialog", { name: ASKS });
    expect(within(question).getByTestId("discard-loses-commits")).toHaveTextContent(
      "12 commits made on no branch would be lost:",
    );
    const lost = within(question).getByTestId("discard-loses");
    expect(lost).toHaveTextContent("3a823aab fix the retry");
    expect(lost).toHaveTextContent("9f00d1c2 and its test");
    expect(lost).toHaveTextContent("… and 10 more");
    expect(question).not.toHaveTextContent(/No commit is lost/);
    expect(within(question).queryByTestId("discard-loses-nothing")).toBeNull();
    // The branch line names the branch purlis cut, which is the one the core may delete.
    expect(within(question).getByTestId("discard-branch")).toHaveTextContent(
      `The folder is on no branch. The branch ${BRANCH}, which purlis cut, is removed only if it is already merged.`,
    );
  });

  it("keeps the question open with the core's refusal where the folder changed", async () => {
    const changed =
      "What that branch's folder holds has changed since you were asked, so nothing was removed. Press Discard again to see what would be lost now.";
    core({ rows: WITH_BRANCHES, loss: LOSS, discardRefused: changed });
    render(<App />);
    await opened();
    await userEvent.click(within(row("01K6W")).getByRole("button", { name: DISCARD }));
    const question = await screen.findByRole("alertdialog", { name: ASKS });

    await userEvent.click(within(question).getByRole("button", { name: "Discard" }));

    expect(await within(question).findByRole("alert")).toHaveTextContent(changed);
    expect(screen.getByRole("alertdialog", { name: ASKS })).toBeInTheDocument();
  });

  it("is said of a branch and its folder, never of a worktree", async () => {
    // ADR 0072 §4: a piece is shown as its branch, and its directory is the branch's folder.
    core({ rows: WITH_BRANCHES, loss: LOSS });
    render(<App />);
    const table = await opened();
    expect(table).not.toHaveTextContent(/worktree/i);

    await userEvent.click(within(row("01K6W")).getByRole("button", { name: DISCARD }));

    const question = await screen.findByRole("alertdialog", { name: ASKS });
    expect(question).not.toHaveTextContent(/worktree/i);
  });
});

describe("a session record's tab", () => {
  it("lists the dispatches its chat made: persona, task and outcome", async () => {
    core();
    render(<App />);
    const panel = await screen.findByTestId("panel-sessions");
    await userEvent.click(await within(panel).findByRole("button", { name: /Ship the rollout/ }));
    await screen.findByTestId("session-record");

    const made = screen.getByRole("region", { name: "Dispatches this chat made" });

    expect(
      within(made)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["qa · test the rollout · done", "devops · check prod · running"]);
  });

  it("draws no list on a record whose chat dispatched nothing", async () => {
    core();
    render(<App />);
    const table = await opened();
    await userEvent.click(within(table).getByRole("button", { name: "test the rollout" }));
    await screen.findByTestId("session-record");

    expect(screen.queryByRole("region", { name: "Dispatches this chat made" })).toBeNull();
  });
});
