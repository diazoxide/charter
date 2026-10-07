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
import type { DispatchRow, OpenChat, SessionRecordRow } from "./bindings";

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
  from: { name: "steward 3", workspace: "alpha" },
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
    open_session: null,
    session_record: null,
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

function core(on: { rows?: DispatchRow[] | Error; undrawn?: number } = {}) {
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
