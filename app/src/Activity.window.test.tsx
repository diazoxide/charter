import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "./App";
import type { Activity, ActivityLine, OpenChat, ViewTab } from "./bindings";

/**
 * **A session's Activity tab** (#1495) against the whole window: it opens from the chat tab's
 * menu and from the palette, lists what the chat and its tasks said to each other in order, a
 * line goes to the chat it came from, what a chat said is drawn as text, a long line is clipped
 * until asked for, a file two tasks say they changed is marked, and a line the app records
 * while the tab is open arrives without the timeline being read again.
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

function chat(over: Partial<OpenChat> & Pick<OpenChat, "session">): OpenChat {
  return {
    name: String(over.session),
    cwd: ALPHA,
    harness: "claude",
    in_front: false,
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
    ...over,
  };
}

/** The session, and the task it dispatched: both open, each in a tab. */
const STEWARD = chat({ session: 3, in_front: true });
const TALK = chat({
  session: 7,
  persona: "devops",
  label: "talk",
  from: {
    name: "steward 3",
    workspace: "alpha",
    chat: 3,
    task: true,
    tab: true,
    reported: false,
    unreported: false,
  },
});

const STEWARD_KEY = "01K6STEWARD";
const TALK_KEY = "01K6TALK";

function line(
  over: Partial<ActivityLine> & Pick<ActivityLine, "n" | "kind" | "text">,
): ActivityLine {
  const up = over.kind === "note" || over.kind === "question" || over.kind === "report";
  return {
    dispatch: "01K6D1",
    at: `2026-10-08T09:0${over.n}:00+00:00`,
    from: up ? "talk" : "steward 3",
    from_key: up ? TALK_KEY : STEWARD_KEY,
    from_session: up ? 7 : 3,
    to: up ? "steward 3" : "talk",
    to_key: up ? STEWARD_KEY : TALK_KEY,
    outcome: null,
    files: [],
    task: "talk",
    ...over,
  };
}

const LINES: ActivityLine[] = [
  line({ n: 0, kind: "dispatched", text: "Is the rollout healthy?" }),
  line({ n: 1, kind: "note", text: "Reading the config." }),
  line({ n: 2, kind: "question", text: "Which host?" }),
  line({ n: 3, kind: "answer", text: "prod-2." }),
  line({ n: 4, kind: "follow-up", text: "Check the cache too." }),
  line({ n: 5, kind: "report", text: "Both are healthy.", outcome: "done" }),
];

function core(
  on: { lines?: ActivityLine[]; unkept?: number; undrawn?: number; reopened?: ViewTab[] } = {},
) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC(
    (cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      asked.push({ cmd, args: given });
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          workspaces: [
            { name: "alpha", path: ALPHA, vision: "Ship it", todos: [], chats: [STEWARD, TALK] },
          ],
          personas: ["steward", "devops"],
          persona: "steward",
          unfiled: [],
        };
      if (cmd === "opened_chats") return [STEWARD, TALK];
      if (cmd === "reopened_views") return on.reopened ?? [];
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "extension_views") return [];
      if (cmd === "extension_commands") return [];
      if (cmd === "extension_panels") return [];
      if (cmd === "extensions_on") return [];
      if (cmd === "project_theme_drawn") return null;
      if (cmd === "activity") {
        if (given.session !== 3) throw `chat ${String(given.session)} is not one this app has open`;
        const answer: Activity = {
          name: "steward 3",
          key: STEWARD_KEY,
          lines: on.lines ?? LINES,
          unkept: on.unkept ?? 0,
          undrawn: on.undrawn ?? 0,
        };
        return answer;
      }
      return null;
    },
    { shouldMockEvents: true },
  );
  return {
    asked: (cmd: string) => asked.filter((one) => one.cmd === cmd).map(({ args }) => args),
  };
}

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const selected = () => within(strip()).getByRole("tab", { selected: true }).textContent ?? "";

/** Opens the steward chat's Activity from its tab's menu, and answers its list. */
async function opened(): Promise<HTMLElement> {
  await screen.findAllByTestId("pane");
  const tabs = await waitFor(() => {
    const all = within(strip()).getAllByRole("tab");
    expect(all).toHaveLength(2);
    return all;
  });
  fireEvent.contextMenu(tabs[0]);
  await userEvent.click(
    within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Activity" }),
  );
  return screen.findByRole("list", { name: "Activity of steward 3" });
}

/** Each line as `time kind who: text`, top to bottom. */
function read(list: HTMLElement): string[] {
  return within(list)
    .getAllByRole("listitem")
    .map((item) => {
      const part = (name: string) =>
        item.querySelector(`.activity-${name}`)?.textContent?.trim() ?? "";
      return `${part("at")} ${part("kind")} ${part("who")}: ${part("text")}`;
    });
}

describe("a session's Activity tab", () => {
  it("opens from the chat tab's menu and lists every line once, in order", async () => {
    const { asked } = core();
    render(<App />);

    const list = await opened();

    expect(selected()).toContain("Activity");
    expect(asked("activity")).toContainEqual({ plane: PLANE, session: 3 });
    expect(read(list)).toEqual([
      "09:00:00 dispatched steward 3 → talk: Is the rollout healthy?",
      "09:01:00 note talk → steward 3: Reading the config.",
      "09:02:00 question talk → steward 3: Which host?",
      "09:03:00 answer steward 3 → talk: prod-2.",
      "09:04:00 follow-up steward 3 → talk: Check the cache too.",
      "09:05:00 report · done talk → steward 3: Both are healthy.",
    ]);
    // Read-only: nothing in it is typed into.
    expect(within(list).queryByRole("textbox")).toBeNull();
  });

  it("is offered from the palette", async () => {
    core();
    render(<App />);
    await screen.findAllByTestId("pane");

    await userEvent.keyboard("{F2}");
    const palette = await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.type(within(palette).getByRole("combobox"), "activity");
    const offered = await within(palette).findAllByRole("option", { name: /Activity/ });
    await userEvent.click(offered[0]);

    expect(await screen.findByRole("list", { name: "Activity of steward 3" })).toBeInTheDocument();
  });

  it("goes from a line to the chat it came from", async () => {
    core();
    render(<App />);
    const list = await opened();
    expect(selected()).toContain("Activity");

    const note = within(list).getAllByRole("listitem")[1];
    await userEvent.click(within(note).getByRole("button", { name: "Show chat talk" }));

    await waitFor(() => expect(selected()).toContain("talk"));
    expect(selected()).not.toContain("Activity");
  });

  it("names a chat that is closed in plain text", async () => {
    core({ lines: [LINES[0], { ...LINES[1], from_session: null }] });
    render(<App />);
    const list = await opened();

    const note = within(list).getAllByRole("listitem")[1];

    expect(within(note).queryByRole("button", { name: /Show chat/ })).toBeNull();
    expect(within(note).getByText("talk")).toHaveAttribute("title", "Its chat is closed");
  });

  it("draws what a chat said as text, never as markup", async () => {
    const hostile =
      '<img src=x onerror="alert(1)"> **bold** [a link](https://example.com) <b>b</b>';
    core({ lines: [LINES[0], line({ n: 1, kind: "note", text: hostile })] });
    render(<App />);
    const list = await opened();

    const said = within(list).getByText(hostile);

    expect(said).toBeInTheDocument();
    expect(list.querySelector("img, b, strong, a")).toBeNull();
  });

  it("clips a long line until it is asked for in full", async () => {
    const long = `${"word ".repeat(200)}the end`;
    core({ lines: [LINES[0], line({ n: 1, kind: "report", text: long, outcome: "done" })] });
    render(<App />);
    const list = await opened();
    const report = within(list).getAllByRole("listitem")[1];
    expect(report).not.toHaveTextContent("the end");

    await userEvent.click(within(report).getByRole("button", { name: "Show all" }));

    expect(report).toHaveTextContent("the end");
    await userEvent.click(within(report).getByRole("button", { name: "Show less" }));
    expect(report).not.toHaveTextContent("the end");
  });

  it("marks a file two tasks say they changed", async () => {
    const lint = {
      dispatch: "01K6D2",
      task: "lint",
      from: "lint",
      from_key: "01K6LINT",
      from_session: null,
    };
    core({
      lines: [
        LINES[0],
        line({
          ...lint,
          n: 0,
          kind: "dispatched",
          text: "Lint it.",
          from: "steward 3",
          from_key: STEWARD_KEY,
          from_session: 3,
          to: "lint",
          to_key: "01K6LINT",
          at: "2026-10-08T09:00:30+00:00",
        }),
        line({
          n: 5,
          kind: "report",
          text: "Fixed the probe.",
          outcome: "done",
          files: ["src/app.rs", "docs/guide.md"],
        }),
        line({
          ...lint,
          n: 1,
          kind: "report",
          text: "Formatted.",
          outcome: "done",
          files: ["src/app.rs", "src/lib.rs"],
          at: "2026-10-08T09:06:00+00:00",
        }),
      ],
    });
    render(<App />);
    const list = await opened();

    const marks = within(list).getAllByTestId("activity-shared");

    expect(marks.map((mark) => mark.textContent)).toEqual([
      "src/app.rs: also changed by lint",
      "src/app.rs: also changed by talk",
    ]);
    // A file one task changed is not marked.
    expect(list).not.toHaveTextContent("docs/guide.md");
    expect(list).not.toHaveTextContent("src/lib.rs");
  });

  it("adds a line as the app records it, without reading the timeline again", async () => {
    const { asked } = core({ lines: LINES.slice(0, 2) });
    render(<App />);
    const list = await opened();
    expect(read(list)).toHaveLength(2);
    // What opening the tab asked (twice over under StrictMode): nothing below adds to it.
    const reads = asked("activity").length;

    await act(() => emit("activity-line", { plane: PLANE, line: LINES[2] }));
    // Another session's task, and a line of another project: neither is this tab's.
    await act(() =>
      emit("activity-line", {
        plane: PLANE,
        line: line({
          dispatch: "01K6D9",
          n: 0,
          kind: "dispatched",
          text: "Not yours.",
          from: "planner 4",
          from_key: "01K6PLANNER",
        }),
      }),
    );
    await act(() => emit("activity-line", { plane: "/home/dev/other", line: LINES[3] }));
    // The same line heard twice is drawn once.
    await act(() => emit("activity-line", { plane: PLANE, line: LINES[2] }));

    await waitFor(() =>
      expect(read(list)).toEqual([
        "09:00:00 dispatched steward 3 → talk: Is the rollout healthy?",
        "09:01:00 note talk → steward 3: Reading the config.",
        "09:02:00 question talk → steward 3: Which host?",
      ]),
    );
    expect(asked("activity")).toHaveLength(reads);
  });

  it("follows a task the session dispatches while the tab is open, and the tasks under it", async () => {
    core({ lines: [] });
    render(<App />);
    await screen.findAllByTestId("pane");
    fireEvent.contextMenu(within(strip()).getAllByRole("tab")[0]);
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Activity" }),
    );
    expect(await screen.findByTestId("activity-empty")).toBeInTheDocument();

    await act(() => emit("activity-line", { plane: PLANE, line: LINES[0] }));
    await act(() =>
      emit("activity-line", {
        plane: PLANE,
        line: line({
          dispatch: "01K6D3",
          n: 0,
          kind: "dispatched",
          text: "Dig.",
          task: "dig",
          from: "talk",
          from_key: TALK_KEY,
          from_session: 7,
          to: "dig",
          to_key: "01K6DIG",
          at: "2026-10-08T09:01:00+00:00",
        }),
      }),
    );

    const list = await screen.findByRole("list", { name: "Activity of steward 3" });
    await waitFor(() =>
      expect(read(list)).toEqual([
        "09:00:00 dispatched steward 3 → talk: Is the rollout healthy?",
        "09:01:00 dispatched talk → dig: Dig.",
      ]),
    );
    const depths = within(list)
      .getAllByRole("listitem")
      .map((item) => item.getAttribute("data-depth"));
    expect(depths).toEqual(["1", "2"]);
  });

  it("says how many messages were not kept, and how many records it will not draw", async () => {
    core({ unkept: 3, undrawn: 1 });
    render(<App />);
    await opened();

    expect(screen.getByTestId("activity-unkept")).toHaveTextContent(
      "3 messages are not listed: purlis keeps the first 500 of a task.",
    );
    expect(screen.getByTestId("activity-undrawn")).toHaveTextContent(
      "1 task is not listed: its record holds text purlis refuses to put on the screen.",
    );
  });

  it("is brought back with the window, and reads its chat's timeline again", async () => {
    core({
      reopened: [
        {
          from: null,
          view: "activity",
          key: "3",
          title: "Activity · steward 3",
          workspace: "alpha",
          at: 2,
          active: true,
          pinned: false,
          split: null,
        },
      ],
    });
    render(<App />);

    await screen.findAllByTestId("pane");

    // On the strip again, behind the chat in front; pressed, it reads its chat's timeline.
    await userEvent.click(
      await within(strip()).findByRole("tab", { name: /Activity · steward 3/ }),
    );

    const list = await screen.findByRole("list", { name: "Activity of steward 3" });
    expect(selected()).toContain("Activity · steward 3");
    expect(read(list)).toHaveLength(LINES.length);
  });

  it("says so where the chat is not open", async () => {
    core();
    render(<App />);
    await screen.findAllByTestId("pane");
    const tabs = await waitFor(() => {
      const all = within(strip()).getAllByRole("tab");
      expect(all).toHaveLength(2);
      return all;
    });
    // The task's own Activity: the pretend core knows only the steward's.
    fireEvent.contextMenu(tabs[1]);
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Activity" }),
    );

    expect(await screen.findByText(/chat 7 is not one this app has open/)).toBeInTheDocument();
  });
});
