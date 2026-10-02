import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  configure,
  fireEvent,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { SessionRecordRow } from "./bindings";
import { heardFrom, lostOnResume, type Resuming } from "./sessions";

/**
 * **The Sessions panel and Resume** (SI-8d, ADR 0064) against the whole window: the operator's
 * ruling that *the user can always get old sessions back*. A place's session records are a
 * panel of their own, newest first; a row opens the record as a view tab; Resume starts a new
 * chat from it; the palette has both rows; and the plane root's tab shows the plane's own.
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

const NEWER = "workspaces/alpha/sessions/20260928-140000-ship-the-widget.md";
const OLDER = "workspaces/alpha/sessions/20260927-090000-plan-it.md";
const ROOTS = "sessions/20260928-080000-tidy-personas.md";

const SIDEBAR = {
  root: PLANE,
  workspaces: [{ name: "alpha", path: ALPHA, vision: "Ship it", todos: [], chats: [] }],
  personas: ["steward"],
  persona: "steward",
  unfiled: [],
};

function record(path: string, title: string, when: string, resumable: boolean): SessionRecordRow {
  return { path, title, when, persona: "steward", harness: "claude", resumable };
}

/** A Sessions panel as `panels::sessions_panel` draws one, for `records` in the order given. */
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
          note: `${one.when} · steward · claude${one.resumable ? " · ↻ resumable" : ""}`,
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

const ALPHAS = [
  record(NEWER, "Ship the widget", "2026-09-28 14:00", true),
  record(OLDER, "Plan it", "2026-09-27 09:00", false),
];
const ROOT_RECORDS = [record(ROOTS, "Tidy personas", "2026-09-28 08:00", false)];

/** The chat `resume_session` answers with, as `OpenChat`. */
function resumedChat(session: number, over: Record<string, unknown> = {}) {
  return {
    session,
    name: String(session),
    cwd: ALPHA,
    harness: "claude",
    in_front: false,
    resumed: "0f6c2a1e-aaaa-4bbb-8ccc-123456789abc",
    fresh: null,
    profile: "claude",
    persona: "steward",
    unreported: null,
    guessed: null,
    pinned: false,
    label: null,
    from: null,
    ...over,
  };
}

function core(
  on: {
    resumes?: ((args: Record<string, unknown>) => unknown)[];
    /** The view tabs the last launch left open (`reopened_views`). */
    reopened?: unknown[];
    /** The focused workspace's records, when not {@link ALPHAS}. */
    alphas?: SessionRecordRow[];
  } = {},
) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  const moves: number[] = [];
  let resumed = 0;
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plugin:event|listen") {
      if (given.event === "chat-moved") moves.push(given.handler as number);
      return 1;
    }
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "opened_chats") return [];
    if (cmd === "reopened_views") return on.reopened ?? [];
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
        personas: ["steward"],
        persona: "steward",
        sessions: on.alphas ?? ALPHAS,
        contributed: [sessionsPanel(on.alphas ?? ALPHAS)],
      };
    if (cmd === "plane_root_panels")
      return { sessions: ROOT_RECORDS, contributed: [sessionsPanel(ROOT_RECORDS)] };
    if (cmd === "workspace_repos") return { workspace: "alpha", repos: [], cache_refused: null };
    if (cmd === "session_record")
      return {
        row: ALPHAS.find((one) => one.path === given.path) ?? ROOT_RECORDS[0],
        place: "alpha",
        body: "# Ship the widget\n\n## Goal\n\nShip it.\n\n## Open\n\n- the **docs**\n",
      };
    if (cmd === "resume_session") {
      const answer = on.resumes?.[resumed];
      resumed += 1;
      return answer ? answer(given) : resumedChat(7);
    }
    return null;
  });
  return {
    asked,
    /** Fires one `chat-moved`, the way the core pushes one. */
    move(payload: Record<string, unknown>) {
      for (const handler of moves)
        window.__TAURI_INTERNALS__.runCallback(handler, { event: "chat-moved", id: 1, payload });
    },
  };
}

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const tabNames = () =>
  within(strip())
    .getAllByRole("tab")
    .map((tab) => tab.textContent);

describe("the Sessions panel", () => {
  it("lists the workspace's session records newest first, marking the resumable", async () => {
    core();
    render(<App />);
    const panel = await screen.findByTestId("panel-sessions");
    await waitFor(() => expect(within(panel).getByText("Ship the widget")).toBeTruthy());

    const rows = within(panel)
      .getAllByRole("button")
      .map((row) => row.textContent ?? "")
      .filter((text) => text.includes("·"));
    expect(rows.map((text) => text.replace(/ · .*/, "").replace(/\d{4}-.*$/, ""))).toEqual([
      "Ship the widget",
      "Plan it",
    ]);
    expect(rows[0]).toContain("↻ resumable");
    expect(rows[1]).not.toContain("resumable");
  });

  it("opens a record as a read-only view tab of rendered Markdown", async () => {
    const { asked } = core();
    render(<App />);
    const panel = await screen.findByTestId("panel-sessions");
    await userEvent.click(await within(panel).findByRole("button", { name: /Ship the widget/ }));

    const view = await screen.findByTestId("session-record");
    expect(within(view).getByRole("heading", { name: "Goal" })).toBeTruthy();
    expect(within(view).getByText("docs").tagName).toBe("STRONG");
    expect(view.closest(".view-body")).not.toBeNull();
    expect(within(strip()).getByRole("tab", { selected: true })).toHaveTextContent(
      "Session · Ship the widget",
    );
    expect(
      asked.filter((one) => one.cmd === "session_record").map((one) => one.args),
    ).toContainEqual({ plane: PLANE, path: NEWER });
  });

  it("resumes a record from its row's menu as a new chat that says it was resumed", async () => {
    const { asked } = core();
    render(<App />);
    const panel = await screen.findByTestId("panel-sessions");
    const row = await within(panel).findByRole("button", { name: /Ship the widget/ });

    fireEvent.contextMenu(row);
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Resume session: Ship the widget" }),
    );

    await waitFor(() => expect(tabNames()).toContain("steward 1"));
    const call = asked.find((one) => one.cmd === "resume_session");
    expect(call?.args).toMatchObject({ plane: PLANE, path: NEWER, insteadOf: null });
    expect(await screen.findByText(/was resumed — conversation/)).toBeTruthy();
  });

  it("resumes from the Resume button on the record's own tab", async () => {
    const { asked } = core();
    render(<App />);
    const panel = await screen.findByTestId("panel-sessions");
    await userEvent.click(await within(panel).findByRole("button", { name: /Ship the widget/ }));
    await screen.findByTestId("session-record");

    await userEvent.click(screen.getByRole("button", { name: "Resume session: Ship the widget" }));

    await waitFor(() =>
      expect(asked.find((one) => one.cmd === "resume_session")?.args).toMatchObject({
        path: NEWER,
      }),
    );
  });

  it("resumes ITS record from a record's tab whatever records the place in front lists", async () => {
    // The tab's button was the catalogue's row for the place in front, so a record the
    // focused place's list did not hold drew no Resume at all (SI-8e).
    const { asked } = core({
      alphas: [ALPHAS[0]],
      reopened: [
        {
          from: null,
          view: "session",
          key: OLDER,
          title: "Session · Plan it",
          workspace: "alpha",
          at: 0,
          active: true,
          pinned: false,
        },
      ],
    });
    render(<App />);
    await screen.findByTestId("session-record");

    await userEvent.click(await screen.findByRole("button", { name: /^Resume session: / }));

    await waitFor(() =>
      expect(asked.find((one) => one.cmd === "resume_session")?.args).toMatchObject({
        path: OLDER,
      }),
    );
  });

  it("says what a Resume had to guess, beside what it resumed", async () => {
    core({
      resumes: [
        () =>
          resumedChat(7, {
            guessed:
              "its session record does not say which directory it ran in, so it starts in workspace alpha's own directory",
          }),
      ],
    });
    render(<App />);
    const panel = await screen.findByTestId("panel-sessions");
    fireEvent.contextMenu(await within(panel).findByRole("button", { name: /Ship the widget/ }));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Resume session: Ship the widget" }),
    );

    expect(await screen.findByText(/was resumed — conversation/)).toBeTruthy();
    expect(
      await screen.findByText(/does not say which directory it ran in, so it starts in/),
    ).toBeTruthy();
  });

  it("says which happened when the record came back as a new chat", async () => {
    core({
      resumes: [
        () =>
          resumedChat(7, {
            resumed: null,
            fresh:
              "its session record holds no conversation id, so it starts with the record in its briefing",
          }),
      ],
    });
    render(<App />);
    const panel = await screen.findByTestId("panel-sessions");
    fireEvent.contextMenu(await within(panel).findByRole("button", { name: /Plan it/ }));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Resume session: Plan it" }));

    expect(
      await screen.findByText(
        /came back as a new chat: its session record holds no conversation id/,
      ),
    ).toBeTruthy();
  });

  it("starts the record again fresh when the harness ends without bringing the conversation back", async () => {
    const window = core({
      resumes: [
        () => resumedChat(7),
        () =>
          resumedChat(8, {
            resumed: null,
            fresh:
              "claude could not bring back conversation 0f6c, so it starts with the record in its briefing",
          }),
      ],
    });
    render(<App />);
    const panel = await screen.findByTestId("panel-sessions");
    fireEvent.contextMenu(await within(panel).findByRole("button", { name: /Ship the widget/ }));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Resume session: Ship the widget" }),
    );
    await waitFor(() => expect(tabNames()).toContain("steward 1"));

    window.move({
      plane: PLANE,
      session: 7,
      state: "failed",
      needs_you: false,
      queue: [],
      moved_at: 1,
      sequence: 1,
      reports: [],
    });

    await waitFor(() =>
      expect(
        window.asked.filter((one) => one.cmd === "resume_session").map((one) => one.args.insteadOf),
        // The same chat started again, so it keeps its id (ADR 0066's `fresh`).
      ).toEqual([null, 7]),
    );
    expect(
      await screen.findByText(/came back as a new chat: claude could not bring back/),
    ).toBeTruthy();
    expect(window.asked.some((one) => one.cmd === "close_session" && one.args.session === 7)).toBe(
      true,
    );
  });
});

describe("the palette's session rows", () => {
  it("offers to open and to resume each of the focused workspace's records", async () => {
    core();
    render(<App />);
    await screen.findByTestId("panel-sessions");
    await waitFor(() =>
      expect(screen.getByTestId("panel-sessions").textContent).toContain("Plan it"),
    );

    await userEvent.keyboard("{F2}");
    const palette = await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.type(within(palette).getByRole("combobox"), "session");

    const titles = within(palette)
      .getAllByRole("option")
      .map((row) => row.querySelector(".palette-title")?.textContent);
    expect(titles).toEqual(
      expect.arrayContaining([
        "Open session record: Ship the widget",
        "Resume session: Ship the widget",
        "Open session record: Plan it",
        "Resume session: Plan it",
      ]),
    );
    expect(titles).not.toContain("Open session record: Tidy personas");
  });
});

describe("the plane root's tab", () => {
  it("shows the plane's own session records, and none of a workspace's", async () => {
    core();
    render(<App />);
    const rootTab = await waitFor(() =>
      within(screen.getByRole("tablist", { name: "Workspaces" })).getByRole("tab", {
        name: "Plane root",
      }),
    );
    await userEvent.click(rootTab);

    const panels = await screen.findByTestId("panels");
    const sessions = await within(panels).findByTestId("panel-sessions");
    await waitFor(() => expect(within(sessions).getByText("Tidy personas")).toBeTruthy());
    expect(within(sessions).queryByText("Ship the widget")).toBeNull();
  });
});

describe("whether a resumed chat lost its conversation", () => {
  const watched: Resuming = { path: NEWER, afterFailure: false, heard: false };

  it("is so when its program failed before its harness said anything", () => {
    expect(lostOnResume(watched, "failed")).toBe(true);
  });

  it("is not so once the harness has reported, or when it ended cleanly", () => {
    expect(lostOnResume(heardFrom(watched, "waiting"), "failed")).toBe(false);
    expect(lostOnResume(watched, "done")).toBe(false);
  });

  it("is never so for the fresh chat a failed resume already fell back to", () => {
    expect(lostOnResume({ ...watched, afterFailure: true }, "failed")).toBe(false);
  });
});
