import { StrictMode } from "react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  configure,
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
import type {
  FinishedTask,
  OpenChat,
  PastSince,
  PastTask,
  PastTaskRead,
  SessionRecordRow,
} from "./bindings";
import { CLIP_CHARS } from "./pastTasks";
import { forgetThisLaunch } from "./regions";

/**
 * **A workspace's Past tasks** (#1510) against the whole window: it opens from the workspace's
 * menu, the palette and a chat's cleared finished rows; lists the workspace's ended tasks with
 * who asked whom and how each ended; narrows by persona, end, day and name; opens a row for its
 * report and brief as text; offers Reopen only where the finished row would; and takes in a
 * task that ends while it is open without reading everything again.
 *
 * The core here is a pretend one that answers `past_tasks` as the real one does: whole when it
 * is handed no read to follow, and only what ended since when it is.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane" tabIndex={-1}>
      session {session}
    </div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);
configure({ asyncUtilTimeout: 3_000 });

/**
 * **The person is four hours east of UTC here** (#1510, B1): the records keep UTC, and every
 * time and day the view says is theirs. A zone with no summer time, so the offset is one number.
 */
const ZONE_BEFORE = process.env.TZ;
beforeAll(() => {
  process.env.TZ = "Asia/Yerevan";
});
afterAll(() => {
  if (ZONE_BEFORE === undefined) delete process.env.TZ;
  else process.env.TZ = ZONE_BEFORE;
});

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;
const QA_RECORD = "workspaces/alpha/sessions/20261007-121500-test-the-rollout.md";

/** The session that asked, still open. */
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

function past(over: Partial<PastTask> & Pick<PastTask, "id" | "name">): PastTask {
  return {
    persona: "devops",
    asker: "steward 4",
    asker_persona: "steward",
    by_person: false,
    how: "done",
    outcome: "done",
    started: "2026-10-07T12:00:00+00:00",
    ended: "2026-10-07T12:04:30+00:00",
    duration: "4m 30s",
    place: "alpha",
    asked_from: null,
    branch: null,
    says: "All good.",
    session_record: null,
    reopens: true,
    reopened: false,
    not_reopened: null,
    ...over,
  };
}

/** Newest ended first, as the core lists them. */
const ROWS: PastTask[] = [
  past({
    id: "01K6D",
    name: "test the rollout",
    persona: "qa",
    ended: "2026-10-07T12:15:00+00:00",
    duration: "15m 0s",
    session_record: QA_RECORD,
    says: "All 12 checks pass.",
  }),
  past({
    id: "01K6C",
    name: "check prod",
    how: "blocked",
    outcome: "blocked",
    ended: "2026-10-06T09:00:00+00:00",
    branch: "purlis/check-prod",
    asked_from: "beta",
    reopens: false,
  }),
  past({
    id: "01K6B",
    name: "rotate the key",
    asker: "planner 2",
    asker_persona: "planner",
    by_person: true,
    how: "unreported",
    outcome: "ended without a report",
    // Half past one in the morning of the 6th where the person is, and still the 5th in UTC.
    ended: "2026-10-05T21:30:00+00:00",
    reopens: false,
  }),
  past({
    id: "01K6A",
    name: "tidy the notes",
    persona: null,
    asker_persona: null,
    how: "stopped_by_person",
    outcome: "closed by the person",
    ended: "2026-10-01T08:00:00+00:00",
    reopened: true,
    reopens: false,
  }),
];

const RECORD: SessionRecordRow = {
  path: QA_RECORD,
  title: "Test the rollout",
  when: "2026-10-07 12:15",
  persona: "qa",
  harness: "claude",
  resumable: false,
};

function read(id: string, over: Partial<PastTaskRead> = {}): PastTaskRead {
  return {
    id,
    brief: "Run every check against staging.",
    report: "All 12 checks pass.",
    changed: null,
    files: [],
    commits: [],
    cannot_reopen: null,
    ...over,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

function core(
  on: {
    rows?: PastTask[] | Error;
    older?: number;
    unread?: number;
    undrawn?: number;
    /** What an opened row reads, by id. */
    reads?: Record<string, PastTaskRead | Error>;
    /** The core's refusal of a Reopen. */
    refuses?: string;
    /** The finished rows under the steward chat, for the sidebar. */
    finished?: FinishedTask[];
    /** The past tasks of the project's root, which is no workspace. */
    root?: PastTask[];
    /** The chats open now: the steward chat, unless it has closed. */
    open?: OpenChat[];
  } = {},
) {
  const asked: Asked[] = [];
  const chats = [...(on.open ?? [STEWARD])];
  let listed = on.rows instanceof Error ? [] : [...(on.rows ?? ROWS)];
  let finished = [...(on.finished ?? [])];
  /** What ended since the last read: what a later read answers. */
  let since: PastTask[] = [];
  let clock = 100;
  mockIPC(
    (cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      asked.push({ cmd, args: given });
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          workspaces: [{ name: "alpha", path: ALPHA, vision: "Ship it", todos: [], chats }],
          personas: ["steward", "devops", "qa"],
          persona: "steward",
          unfiled: [],
        };
      if (cmd === "opened_chats") return chats;
      if (
        [
          "reopened_views",
          "chat_states",
          "chats_that_would_not_start",
          "running_sessions",
          "stopping_chats",
          "extension_views",
          "extension_commands",
          "extension_panels",
          "extensions_on",
        ].includes(cmd)
      )
        return [];
      if (cmd === "project_theme_drawn") return null;
      if (cmd === "workspace_repos") return { workspace: "alpha", repos: [], cache_refused: null };
      if (cmd === "finished_tasks") return finished;
      if (cmd === "clear_finished_tasks") {
        const ids = given.ids as string[];
        const before = finished.length;
        finished = finished.filter((row) => !ids.includes(row.id));
        return before - finished.length;
      }
      if (cmd === "past_tasks") {
        if (on.rows instanceof Error) throw on.rows.message;
        clock += 1;
        const whole = given.since === null || given.since === undefined;
        if (given.workspace === null)
          return {
            rows: whole ? (on.root ?? []) : [],
            waiting: [],
            whole,
            older: 0,
            most: 500,
            unread: 0,
            undrawn: 0,
            read_at: String(clock),
          };
        const rows = whole ? listed : since;
        if (!whole) {
          listed = [...since, ...listed.filter((row) => !since.some((one) => one.id === row.id))];
          since = [];
        }
        return {
          rows,
          waiting: [],
          whole,
          older: whole ? (on.older ?? 0) : 0,
          most: 500,
          unread: whole ? (on.unread ?? 0) : 0,
          undrawn: whole ? (on.undrawn ?? 0) : 0,
          read_at: String(clock),
        };
      }
      if (cmd === "past_task") {
        const id = given.id as string;
        const said = on.reads?.[id] ?? read(id);
        if (said instanceof Error) throw said.message;
        return said;
      }
      if (cmd === "reopen_finished_task") {
        if (on.refuses !== undefined) throw on.refuses;
        const row = listed.find((one) => one.id === given.id);
        // Its chat is starting: no second Reopen is offered, as the core answers.
        if (row !== undefined) since = [{ ...row, reopens: false }];
        const chat: OpenChat = {
          ...STEWARD,
          session: 21,
          name: "21",
          in_front: false,
          persona: row?.persona ?? null,
          label: row?.name ?? null,
          resumed: "9f2c-the-conversation",
        };
        chats.push(chat);
        return chat;
      }
      if (cmd === "session_record")
        return {
          row: RECORD,
          place: "alpha",
          body: "# The record\n\n## Goal\n\nTest it.\n",
          persona_hosts: [],
          resume_holds: false,
          persona_hosts_locked: null,
          dispatches: [],
        };
      return null;
    },
    { shouldMockEvents: true },
  );
  return {
    asked,
    of: (cmd: string) => asked.filter((one) => one.cmd === cmd).map(({ args }) => args),
    /** A task ended while the view is open: the next later read answers it. */
    ends: (row: PastTask) => {
      since = [row, ...since];
    },
  };
}

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const selected = () => within(strip()).getByRole("tab", { selected: true }).textContent ?? "";

/** Opens workspace alpha's Past tasks from its own menu, and answers the table. */
async function opened(): Promise<HTMLElement> {
  const tab = await screen.findByRole("tab", { name: /alpha/ });
  tab.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  const menu = await screen.findByRole("menu");
  await userEvent.click(within(menu).getByRole("menuitem", { name: "Past tasks" }));
  return screen.findByRole("table", { name: "Past tasks" });
}

/** The palette's row for the catalogue's row `id`. */
function paletteRow(id: string): HTMLElement {
  const row = document.getElementById(`palette-row-${id}`);
  if (row === null) throw new Error(`the palette has no row ${id}`);
  return row;
}

/** The tasks the table lists, top to bottom. */
function names(table: HTMLElement): string[] {
  return [...table.querySelectorAll("tr[data-testid^='past-'] .past-name")].map(
    (name) => name.textContent ?? "",
  );
}

/** The line that says how many are listed, and how many of them are shown. */
const counted = () => document.querySelector(".past-count");

const cells = (id: string) =>
  within(screen.getByTestId(`past-${id}`))
    .getAllByRole("cell")
    .map((cell) => cell.textContent);

describe("a workspace's Past tasks", () => {
  it("opens from the workspace's menu and lists its ended tasks, newest first", async () => {
    const { of } = core();
    render(<App />);

    const table = await opened();

    expect(selected()).toContain("Past tasks · alpha");
    // Read whole, for this workspace and no other.
    expect(of("past_tasks")).toContainEqual({ plane: PLANE, workspace: "alpha", since: null });
    expect(
      within(table)
        .getAllByRole("columnheader")
        .map((head) => head.textContent || head.getAttribute("aria-label")),
    ).toEqual(["Ended", "Task", "Asked by", "Ran as", "How it ended", "Took", "Where", "Reopen"]);
    expect(names(table)).toEqual([
      "test the rollout",
      "check prod",
      "rotate the key",
      "tidy the notes",
    ]);
    // When, the task, who asked whom, how it ended, how long, where.
    expect(cells("01K6D").slice(0, 7)).toEqual([
      // A quarter past noon UTC, said on the person's own clock.
      "2026-10-07 16:15",
      "test the rollout",
      "steward 4",
      "qa",
      "done",
      "15m 0s",
      "alpha",
    ]);
    // The person asked for this one themselves, from a chat; and it ran as a persona.
    expect(cells("01K6B")[2]).toBe("you, from planner 2");
    // Its own branch, and where it was asked from where that is another workspace.
    expect(cells("01K6C")[6]).toBe("alpha · own branch purlis/check-prod · asked from beta");
    expect(cells("01K6A")[3]).toBe("No persona");
    expect(counted()).toHaveTextContent("4 past tasks");
  });

  it("says how each ended in the word and the shape its finished row said it in", async () => {
    core();
    render(<App />);
    await opened();

    const end = (id: string) => {
      const cell = screen.getByTestId(`past-${id}`).querySelector(".past-end");
      return {
        word: cell?.querySelector(".shown-state .word")?.textContent,
        shape: cell?.querySelector(".shown-state [data-shape]")?.getAttribute("data-shape"),
        more: [...(cell?.querySelectorAll(".outcome") ?? [])].map((one) => one.textContent),
      };
    };
    expect(end("01K6D")).toEqual({ word: "done", shape: "tick", more: [] });
    // The state's word, then the core's own where it says more.
    expect(end("01K6C")).toEqual({ word: "failed", shape: "cross", more: ["blocked"] });
    expect(end("01K6B")).toEqual({
      word: "ended without a report",
      shape: "triangle",
      more: [],
    });
    expect(end("01K6A")).toEqual({
      word: "cancelled",
      shape: "dash",
      more: ["closed by the person", "reopened"],
    });
  });

  it("is offered from the palette", async () => {
    core();
    render(<App />);
    await screen.findByRole("tab", { name: /alpha/ });

    await userEvent.keyboard("{F2}");
    const palette = await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.type(within(palette).getByRole("combobox"), "past tasks");
    // One row a workspace and one for the project's root, told apart by their notes.
    await within(palette).findAllByRole("option", { name: /Past tasks/ });
    await userEvent.click(paletteRow("workspace.past:alpha"));

    expect(await screen.findByRole("table", { name: "Past tasks" })).toBeInTheDocument();
    expect(selected()).toContain("Past tasks · alpha");
  });

  it("narrows by persona, by how it ended, by day and by the task's name", async () => {
    core();
    render(<App />);
    let table = await opened();
    const search = screen.getByRole("search", { name: "Narrow the past tasks" });

    // A persona is found as the one that ran a task and as the one that asked for it.
    const persona = within(search).getByRole("combobox", { name: "Filter by persona" });
    expect(
      within(persona)
        .getAllByRole("option")
        .map((one) => one.textContent),
    ).toEqual(["Every persona", "devops", "planner", "qa", "steward"]);
    await userEvent.selectOptions(persona, "planner");
    expect(names(table)).toEqual(["rotate the key"]);
    await userEvent.selectOptions(persona, "steward");
    expect(names(table)).toEqual(["test the rollout", "check prod"]);
    expect(counted()).toHaveTextContent("2 of 4 past tasks");
    await userEvent.selectOptions(persona, "");

    // How it ended, offered in the words the rows say it in.
    const ends = within(search).getByRole("combobox", { name: "Filter by how it ended" });
    expect(
      within(ends)
        .getAllByRole("option")
        .map((one) => one.textContent),
    ).toEqual([
      "Every end",
      "done",
      "failed (blocked)",
      "ended without a report",
      "cancelled (closed by the person)",
    ]);
    await userEvent.selectOptions(ends, "blocked");
    expect(names(table)).toEqual(["check prod"]);
    await userEvent.selectOptions(ends, "");

    // A range of days, each end of it included.
    const from = within(search).getByLabelText("Ended on or after");
    const to = within(search).getByLabelText("Ended on or before");
    // A date box takes its value whole, as a picker gives it.
    fireEvent.change(from, { target: { value: "2026-10-05" } });
    expect(names(table)).toEqual(["test the rollout", "check prod", "rotate the key"]);
    fireEvent.change(to, { target: { value: "2026-10-06" } });
    expect(names(table)).toEqual(["check prod", "rotate the key"]);
    // **A day is the person's own**: the task that ended at 01:30 on the 6th where they are
    // is found on the 6th, though its record says 21:30 on the 5th in UTC. And not on the 5th.
    fireEvent.change(from, { target: { value: "2026-10-06" } });
    expect(names(table)).toEqual(["check prod", "rotate the key"]);
    expect(cells("01K6B")[0]).toBe("2026-10-06 01:30");
    fireEvent.change(from, { target: { value: "2026-10-05" } });
    fireEvent.change(to, { target: { value: "2026-10-05" } });
    expect(screen.getByText("No past task matches.")).toBeInTheDocument();
    fireEvent.change(to, { target: { value: "2026-10-06" } });
    // The table went while nothing matched, and is drawn anew.
    table = await screen.findByRole("table", { name: "Past tasks" });
    // The zone is said once, above the table, and on no row.
    expect(screen.getByTestId("past-zone")).toHaveTextContent(
      "Times and days are your local time (Asia/Yerevan, UTC+4).",
    );
    expect(table).not.toHaveTextContent("UTC");

    // And text in the name, whatever its case, on top of the rest.
    // Focused by hand: with no layout to measure, a press anywhere in the window lands on the
    // regions' divider, which takes the focus.
    const box = within(search).getByRole("searchbox");
    box.focus();
    await userEvent.type(box, "ROTATE", { skipClick: true });
    expect(names(table)).toEqual(["rotate the key"]);
    await userEvent.type(box, " nothing", { skipClick: true });
    expect(screen.queryByRole("table", { name: "Past tasks" })).toBeNull();
    expect(screen.getByText("No past task matches.")).toBeInTheDocument();

    await userEvent.click(within(search).getByRole("button", { name: "Show every task" }));
    expect(names(await screen.findByRole("table", { name: "Past tasks" }))).toHaveLength(4);
    // Narrowing asked the core nothing: it is done over what the one read answered.
    expect(counted()).toHaveTextContent("4 past tasks");
  });

  it("opens a row to its report, what it said it changed and its brief, as text and never as markup", async () => {
    const { of } = core({
      reads: {
        "01K6D": read("01K6D", {
          report: "<b>All 12</b> checks pass.\n<img src=x onerror=alert(1)>",
          changed: "values.yaml: replicas 2 to 3",
          files: ["deploy/values.yaml"],
          commits: ["3a823aab"],
          brief: "# Test the rollout\n<script>alert(1)</script>",
        }),
      },
    });
    render(<App />);
    const table = await opened();
    // Nothing is read for a row until it is opened.
    expect(of("past_task")).toEqual([]);

    await userEvent.click(
      within(table).getByRole("button", { name: "Show the report and brief of test the rollout" }),
    );

    const shown = await screen.findByRole("region", {
      name: "Report and brief of test the rollout",
    });
    expect(of("past_task")).toEqual([{ plane: PLANE, id: "01K6D" }]);
    // Every word a chat wrote is a text node: the tags are read, never run or drawn.
    expect(shown).toHaveTextContent("<b>All 12</b> checks pass.");
    expect(shown).toHaveTextContent("<script>alert(1)</script>");
    expect(shown.querySelector("b, img, script")).toBeNull();
    expect(shown).toHaveTextContent("values.yaml: replicas 2 to 3");
    expect(shown).toHaveTextContent("deploy/values.yaml");
    expect(shown).toHaveTextContent("3a823aab");
    expect(shown).toHaveTextContent("started 2026-10-07 16:00 · ended 2026-10-07 16:15");

    // Pressed again, it shuts.
    await userEvent.click(
      within(table).getByRole("button", { name: "Hide the report and brief of test the rollout" }),
    );
    expect(screen.queryByRole("region", { name: /Report and brief/ })).toBeNull();
  });

  it("clips a long report, and shows all of it on a press", async () => {
    const long = `${"line of the report\n".repeat(40)}the last line`;
    const wide = "w".repeat(CLIP_CHARS + 50);
    core({ reads: { "01K6D": read("01K6D", { report: long, brief: wide }) } });
    render(<App />);
    const table = await opened();
    await userEvent.click(
      within(table).getByRole("button", { name: "Show the report and brief of test the rollout" }),
    );
    const shown = await screen.findByRole("region", {
      name: "Report and brief of test the rollout",
    });

    expect(shown).not.toHaveTextContent("the last line");
    const all = within(shown).getByRole("button", {
      name: "Show all of the report of test the rollout",
    });
    expect(all).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(all);
    expect(shown).toHaveTextContent("the last line");
    expect(
      within(shown).getByRole("button", { name: "Show less of the report of test the rollout" }),
    ).toHaveAttribute("aria-expanded", "true");
    // A brief of one long line is clipped at a count of characters, and says so too.
    expect(shown.textContent).not.toContain(wide);
    await userEvent.click(
      within(shown).getByRole("button", { name: "Show all of the brief of test the rollout" }),
    );
    expect(shown.textContent).toContain(wide);
  });

  it("links to the session record a task's chat wrote, and says so where it wrote none", async () => {
    const { of } = core();
    render(<App />);
    const table = await opened();

    await userEvent.click(
      within(table).getByRole("button", { name: "Show the report and brief of check prod" }),
    );
    const none = await screen.findByRole("region", { name: "Report and brief of check prod" });
    expect(none).toHaveTextContent("Its chat wrote no session record.");
    expect(within(none).queryByRole("button", { name: "Open its session record" })).toBeNull();

    await userEvent.click(
      within(table).getByRole("button", { name: "Show the report and brief of test the rollout" }),
    );
    const shown = await screen.findByRole("region", {
      name: "Report and brief of test the rollout",
    });
    await userEvent.click(within(shown).getByRole("button", { name: "Open its session record" }));

    await screen.findByTestId("session-record");
    expect(selected()).toContain("Session · test the rollout");
    expect(of("session_record")).toContainEqual({ plane: PLANE, path: QA_RECORD });
  });

  it("offers Reopen only where the finished row would, and reopens by the finished row's command", async () => {
    const { of } = core({
      reads: {
        "01K6C": read("01K6C", {
          cannot_reopen:
            "'check prod' cannot be reopened: it ran on codex, and the profile 'work' now runs claude.",
        }),
      },
    });
    render(<App />);
    const table = await opened();

    // No conversation to resume, and one reopened already: reachable, and they say why.
    const blocked = within(table).getByRole("button", { name: "Reopen check prod" });
    expect(blocked).toHaveAttribute("aria-disabled", "true");
    expect(blocked).toHaveAttribute(
      "aria-description",
      "It cannot be reopened: its harness named no conversation to resume.",
    );
    const twice = within(table).getByRole("button", { name: "Reopen tidy the notes" });
    expect(twice).toHaveAttribute("aria-disabled", "true");
    expect(twice.getAttribute("aria-description")).toMatch(/^It was reopened already/);
    await userEvent.click(blocked);
    await userEvent.click(twice);
    expect(of("reopen_finished_task")).toEqual([]);
    // The opened row says the core's own reason, in the finished row's words.
    await userEvent.click(
      within(table).getByRole("button", { name: "Show the report and brief of check prod" }),
    );
    expect(
      await screen.findByRole("region", { name: "Report and brief of check prod" }),
    ).toHaveTextContent("it ran on codex, and the profile 'work' now runs claude.");

    // One that can be: the finished row's own command, and its chat's tab comes forward.
    const reopen = within(table).getByRole("button", { name: "Reopen test the rollout" });
    expect(reopen).not.toHaveAttribute("aria-disabled");
    await userEvent.click(reopen);

    await waitFor(() => expect(of("reopen_finished_task")).toHaveLength(1));
    expect(of("reopen_finished_task")[0]).toMatchObject({ plane: PLANE, id: "01K6D" });
    await waitFor(() =>
      expect(
        within(strip())
          .getAllByRole("tab")
          .map((tab) => tab.querySelector(".tab-name")?.textContent),
      ).toContain("test the rollout"),
    );
    // Nothing else acts on a past task: no row has a Clear, a Discard or a Stop.
    expect(within(table).queryByRole("button", { name: /Clear|Discard|Stop|Delete/ })).toBeNull();
  });

  it("says the core's refusal of a Reopen under the row it was pressed on", async () => {
    core({ refuses: "'test the rollout' cannot be reopened: the folder it worked in is gone." });
    render(<App />);
    const table = await opened();

    await userEvent.click(within(table).getByRole("button", { name: "Reopen test the rollout" }));

    expect(await within(table).findByRole("alert")).toHaveTextContent(
      "'test the rollout' cannot be reopened: the folder it worked in is gone.",
    );
    expect(selected()).toContain("Past tasks · alpha");
  });

  it("takes in a task that ends while it is open, and reads only what ended since", async () => {
    const { of, ends } = core();
    render(<App />);
    const table = await opened();
    await waitFor(() => expect(names(table)).toHaveLength(4));
    const whole = () => of("past_tasks").filter((args) => args.since === null).length;
    const wholeAtFirst = whole();
    const lastRead = of("past_tasks").length;

    // A task of this project ends: its chat is closed, and the window is told.
    ends(
      past({
        id: "01K6E",
        name: "count the hosts",
        ended: "2026-10-08T10:00:00+00:00",
        says: "Forty.",
      }),
    );
    await act(() => emit("chat-stop", { plane: PLANE, session: 9, phase: "stopped" }));

    await waitFor(() => expect(names(table)[0]).toBe("count the hosts"));
    expect(names(table)).toHaveLength(5);
    // The read that found it was handed the read before it, and nothing was read whole again.
    const later = of("past_tasks").slice(lastRead);
    expect(later.length).toBeGreaterThan(0);
    for (const args of later) {
      const since = args.since as PastSince;
      expect(Number(since.at)).toBeGreaterThan(100);
      expect(since.also).toEqual([]);
    }
    expect(whole()).toBe(wholeAtFirst);
    expect(counted()).toHaveTextContent("5 past tasks");

    // A chat of another project ending, and one only beginning to stop, read nothing.
    const before = of("past_tasks").length;
    await act(() => emit("chat-stop", { plane: "/another", session: 9, phase: "stopped" }));
    await act(() => emit("chat-stop", { plane: PLANE, session: 9, phase: "stopping" }));
    expect(of("past_tasks")).toHaveLength(before);
  });

  it("says what it does not list: tasks past its bound, records that could not be read, records it will not draw", async () => {
    core({ older: 37, unread: 2, undrawn: 1 });
    render(<App />);
    await opened();

    expect(screen.getByTestId("past-older")).toHaveTextContent(
      "The newest 500 are listed. 37 older tasks are not listed, and not searched.",
    );
    expect(screen.getByTestId("past-unread")).toHaveTextContent(
      "2 records could not be read, in this project's store on this machine.",
    );
    expect(screen.getByTestId("past-undrawn")).toHaveTextContent("1 record purlis will not draw");
  });

  it("says so where the workspace has no past task, and where the list cannot be read", async () => {
    core({ rows: [], unread: 1 });
    render(<App />);
    const tab = await screen.findByRole("tab", { name: /alpha/ });
    tab.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Past tasks" }),
    );

    expect(await screen.findByTestId("past-tasks-empty")).toHaveTextContent("No past tasks yet");
    expect(screen.getByTestId("past-unread")).toHaveTextContent("1 record could not be read");
    cleanup();
    clearMocks();

    core({ rows: new Error("the store is locked") });
    render(<App />);
    const again = await screen.findByRole("tab", { name: /alpha/ });
    again.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Past tasks" }),
    );
    expect(
      await screen.findByText(
        "purlis could not read this workspace's past tasks: the store is locked",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Read again" })).toBeInTheDocument();
  });
});

describe("the project root's Past tasks", () => {
  it("is opened by itself from the palette, after the chat that asked has closed", async () => {
    // A steward chat at the root asked for a task that worked at the root, and has closed:
    // no chat is open, so no finished row and no "See past tasks" is left to press, and the
    // task is in no workspace's list.
    const { of } = core({
      open: [],
      root: [
        past({
          id: "01K6R",
          name: "audit the personas",
          place: "project root",
          says: "Three are unused.",
        }),
      ],
    });
    render(<App />);
    await screen.findByRole("tab", { name: /alpha/ });

    await userEvent.keyboard("{F2}");
    const palette = await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.type(within(palette).getByRole("combobox"), "past tasks");
    await within(palette).findAllByRole("option", { name: /Past tasks/ });
    const row = paletteRow(`project.past:${PLANE}`);
    expect(row).toHaveTextContent("Past tasks · project root");
    await userEvent.click(row);

    const table = await screen.findByRole("table", { name: "Past tasks" });
    expect(selected()).toContain("Past tasks · project root");
    // Asked for the root, which is no workspace.
    expect(of("past_tasks")).toContainEqual({ plane: PLANE, workspace: null, since: null });
    expect(names(table)).toEqual(["audit the personas"]);
    expect(cells("01K6R")[6]).toBe("project root");
  });
});

describe("a chat's cleared finished rows", () => {
  const done = (id: string, name: string): FinishedTask => ({
    id,
    asker: 4,
    name,
    persona: "devops",
    how: "done",
    outcome: "done",
    folds: true,
    report: `${name}: all good.`,
    changed: null,
    ended: "2026-10-08T12:04:30+00:00",
    place: "alpha",
    branch: null,
    reopens: true,
    not_reopened: null,
    chat: null,
  });

  /** The steward chat's finished rows. Its row folds over tasks that are all done by itself
   *  (#1499), so it is opened first. */
  const theirs = async () => {
    await userEvent.click(await screen.findByTitle("Show the chats under steward 4"));
    return screen.findByRole("group", { name: "Finished tasks of steward 4" });
  };

  it("says they stay in Past tasks, and goes there on a press", async () => {
    const { of } = core({ finished: [done("01K6X", "count"), done("01K6Y", "read")] });
    render(<App />);
    const group = await theirs();

    const clear = within(group).getByRole("button", { name: "Clear finished" });
    expect(clear).toHaveAttribute("title", "Takes these rows away. They stay in Past tasks.");
    await userEvent.click(clear);

    // The rows are gone, and the line left where they were says where they went.
    await waitFor(() =>
      expect(screen.queryByRole("group", { name: "Finished tasks of steward 4" })).toBeNull(),
    );
    const tree = screen.getByRole("tree", { name: "Chats of this project" });
    const went = within(tree).getByText(/They stay in Past tasks\./);
    expect(went).toHaveTextContent("Cleared 2 finished tasks. They stay in Past tasks.");
    expect(went).toHaveAttribute("role", "status");

    await userEvent.click(
      within(tree).getByRole("button", { name: "See past tasks, from steward 4" }),
    );

    expect(await screen.findByRole("table", { name: "Past tasks" })).toBeInTheDocument();
    expect(selected()).toContain("Past tasks · alpha");
    expect(of("past_tasks")).toContainEqual({ plane: PLANE, workspace: "alpha", since: null });
  });

  it("offers the way to Past tasks under an opened Finished (n) fold", async () => {
    core({ finished: [done("01K6X", "count")] });
    render(<App />);
    const group = await theirs();
    expect(within(group).queryByRole("button", { name: /See past tasks/ })).toBeNull();

    await userEvent.click(within(group).getByRole("button", { name: "Finished (1)" }));
    await userEvent.click(
      within(group).getByRole("button", { name: "See past tasks, from steward 4" }),
    );

    expect(await screen.findByRole("table", { name: "Past tasks" })).toBeInTheDocument();
  });
});
