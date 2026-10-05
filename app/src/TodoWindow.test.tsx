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
import type { PanelRow, TodoView } from "./bindings";

/**
 * **A todo's tab, against the whole window** (#1214): a todo row opens the todo as a view tab on
 * its workspace's strip — its title, its whole text, when it was opened, its state and its
 * actions — and a second open brings the same tab forward. The palette has a row per todo.
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

const CHAT = {
  session: 1,
  name: "1",
  cwd: ALPHA,
  harness: "claude",
  in_front: true,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona: "steward",
};

const SIDEBAR = {
  root: PLANE,
  workspaces: [{ name: "alpha", path: ALPHA, vision: "Ship it", todos: [], chats: [CHAT] }],
  personas: ["steward"],
  persona: "steward",
  unfiled: [],
};

const TODOS: TodoView[] = [
  {
    workspace: "alpha",
    slug: "20260302-091400-review",
    title: "Review the rollout plan",
    stamp: "2026-03-02",
    body: "The rollout plan, in full: **every** region before Friday.",
  },
  {
    workspace: "alpha",
    slug: "20260303-101500-drop",
    title: "Drop the old importer",
    stamp: "2026-03-03",
    body: "",
  },
];

/** A todo's row, as `panels.rs` sends it: it runs `todo.open:<slug>`. */
function todoRow(todo: TodoView): PanelRow {
  return {
    key: todo.slug,
    text: todo.title,
    note: todo.stamp,
    mark: "todo",
    tone: "plain",
    detail: { kind: "text", text: todo.body || todo.title },
    runs: `todo.open:${todo.slug}`,
    actions: [],
  };
}

/** The core. The open todos are the ones not closed or forgotten, and `extra` is one more. */
function core(extra?: Pick<TodoView, "title" | "body">) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  const gone = new Set<string>();
  const all: TodoView[] =
    extra === undefined
      ? TODOS
      : [
          ...TODOS,
          { workspace: "alpha", slug: "20260304-080000-hostile", stamp: "2026-03-04", ...extra },
        ];
  const open = () => all.filter((todo) => !gone.has(todo.slug));
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "opened_chats")
      return [{ ...CHAT, unreported: null, guessed: null, pinned: false }];
    if (cmd === "reopened_views") return [];
    if (["chat_states", "chats_that_would_not_start", "running_sessions"].includes(cmd)) return [];
    if (["extension_views", "extension_commands", "extension_panels"].includes(cmd)) return [];
    if (cmd === "extensions_on") return [];
    if (cmd === "project_theme_drawn") return null;
    if (cmd === "workspace_panels")
      return {
        workspace: "alpha",
        repos: [],
        paths: {},
        absent: [],
        refused: [],
        todos: open().map(({ slug, title, stamp }) => ({ slug, title, stamp })),
        todos_refused: null,
        personas: ["steward"],
        persona: "steward",
        contributed: [
          {
            key: "charter/todos",
            title: "Todos",
            order: 10,
            mark: "todo",
            from: null,
            about: null,
            blocks: [
              {
                kind: "list",
                rows: open().map(todoRow),
                empty: { headline: "Nothing to do", body: null, offer: null },
              },
            ],
          },
        ],
      };
    if (cmd === "workspace_repos") return { workspace: "alpha", repos: [], cache_refused: null };
    if (cmd === "vault_list") return [];
    if (cmd === "todo_read") {
      return open().find((todo) => todo.slug === given.slug) ?? null;
    }
    if (cmd === "todo_done") {
      gone.add(String(given.slug));
      return "Closed 'Review the rollout plan' in 'alpha' — the journal has the trace.";
    }
    if (cmd === "todo_forget") {
      gone.add(String(given.slug));
      return "Dropped it.";
    }
    return null;
  });
  return { asked };
}

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const tabNames = () =>
  within(strip())
    .getAllByRole("tab")
    .map((tab) => tab.textContent);

async function todoRowFor(title: string) {
  const panel = await screen.findByTestId("panel-todos");
  return within(panel).findByRole("button", { name: new RegExp(title) });
}

describe("a todo row", () => {
  it("opens the todo as a view tab on its workspace's strip, with all of it", async () => {
    const { asked } = core();
    render(<App />);

    await userEvent.click(await todoRowFor("Review the rollout plan"));

    const body = await screen.findByTestId("todo-body");
    expect(body).toHaveTextContent("The rollout plan, in full: every region before Friday.");
    // Rendered as the Markdown it is.
    expect(within(body).getByText("every").tagName).toBe("STRONG");
    const tab = within(strip()).getByRole("tab", { selected: true });
    expect(tab).toHaveTextContent("Review the rollout plan");
    const pane = screen.getByRole("region", { name: "Review the rollout plan" });
    expect(within(pane).getByText("Opened")).toBeInTheDocument();
    expect(within(pane).getByText("2026-03-02")).toBeInTheDocument();
    expect(within(pane).getByText("State")).toBeInTheDocument();
    expect(within(pane).getByText("Open")).toBeInTheDocument();
    // Its actions are the catalogue's rows for it.
    expect(within(pane).getByRole("button", { name: /Mark done/ })).toBeInTheDocument();
    expect(within(pane).getByRole("button", { name: /Forget todo/ })).toBeInTheDocument();
    // No card beside the row: the tab holds the whole of it.
    expect(screen.queryByTestId("row-detail-20260302-091400-review")).toBeNull();
    expect(asked.find((one) => one.cmd === "todo_read")?.args).toMatchObject({
      workspace: "alpha",
      slug: "20260302-091400-review",
    });
  });

  it("says the title as its text when the todo has no body", async () => {
    core();
    render(<App />);

    await userEvent.click(await todoRowFor("Drop the old importer"));

    expect(await screen.findByTestId("todo-body")).toHaveTextContent("Drop the old importer");
  });

  it("draws no HTML from the todo's file, and no javascript: link", async () => {
    // A todo is a file a chat or a terminal wrote: `skipHtml` drops raw HTML, and the tab's
    // links are `SessionRecordTab.COMPONENTS`', which make only http and https links.
    core({
      title: "Hostile",
      body: 'Before <script>window.ran = true</script> <img src="x" onerror="window.ran = true"> [press me](javascript:alert(1)) after.',
    });
    render(<App />);

    await userEvent.click(await todoRowFor("Hostile"));

    const body = await screen.findByTestId("todo-body");
    expect(body).toHaveTextContent("press me");
    expect(body.querySelector("script")).toBeNull();
    expect(body.querySelector("img")).toBeNull();
    // Dropped, not escaped into the text: what `skipHtml` does that the default does not.
    expect(body.textContent).not.toMatch(/<script|<img|onerror/);
    // No anchor at all: a `javascript:` link is words, not a link with an emptied href.
    expect(body.querySelector("a")).toBeNull();
    expect((window as { ran?: boolean }).ran).toBeUndefined();
  });

  it("brings the tab already open forward rather than opening a second", async () => {
    core();
    render(<App />);
    await userEvent.click(await todoRowFor("Review the rollout plan"));
    await screen.findByTestId("todo-body");
    await userEvent.click(within(strip()).getByRole("tab", { name: /steward 1/ }));

    await userEvent.click(await todoRowFor("Review the rollout plan"));

    await waitFor(() =>
      expect(within(strip()).getByRole("tab", { selected: true })).toHaveTextContent(
        "Review the rollout plan",
      ),
    );
    expect(tabNames()).toEqual(["steward 1", "Review the rollout plan"]);
  });

  it("opens from the keyboard: Enter on the row", async () => {
    core();
    render(<App />);
    const row = await todoRowFor("Drop the old importer");

    row.focus();
    await userEvent.keyboard("{Enter}");

    expect(await screen.findByTestId("todo-body")).toHaveTextContent("Drop the old importer");
    expect(within(strip()).getByRole("tab", { selected: true })).toHaveTextContent(
      "Drop the old importer",
    );
  });

  it("offers Open first on its menu, then Mark done, and under the line Forget", async () => {
    core();
    render(<App />);

    fireEvent.contextMenu(await todoRowFor("Review the rollout plan"));

    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((item) => item.getAttribute("aria-label")),
    ).toEqual([
      "Open todo: Review the rollout plan",
      "Mark done: Review the rollout plan",
      "Forget todo Review the rollout plan",
    ]);
  });
});

describe("the palette", () => {
  it("has an Open todo row per open todo, which opens its tab", async () => {
    core();
    render(<App />);
    await todoRowFor("Review the rollout plan");

    await userEvent.keyboard("{F2}");
    const palette = await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("Open todo");
    const rows = await within(palette).findAllByRole("option", { name: /^Open todo: / });
    expect(rows.map((row) => row.getAttribute("aria-label") ?? row.textContent)).toEqual(
      expect.arrayContaining([
        expect.stringContaining("Open todo: Review the rollout plan"),
        expect.stringContaining("Open todo: Drop the old importer"),
      ]),
    );
    await userEvent.clear(within(palette).getByRole("combobox"));
    await userEvent.keyboard("Open todo: Drop the old{Enter}");

    expect(await screen.findByTestId("todo-body")).toHaveTextContent("Drop the old importer");
  });
});

describe("the tab's actions", () => {
  it("Mark done closes the todo through the core, and its tab closes", async () => {
    const { asked } = core();
    render(<App />);
    await userEvent.click(await todoRowFor("Review the rollout plan"));
    await screen.findByTestId("todo-body");

    await userEvent.click(screen.getByRole("button", { name: /Mark done/ }));

    await waitFor(() => expect(tabNames()).toEqual(["steward 1"]));
    expect(asked.find((one) => one.cmd === "todo_done")?.args).toMatchObject({
      workspace: "alpha",
      slug: "20260302-091400-review",
    });
  });

  it("Forget from the row's menu closes the todo's open tab too", async () => {
    core();
    render(<App />);
    await userEvent.click(await todoRowFor("Drop the old importer"));
    await screen.findByTestId("todo-body");
    await userEvent.click(within(strip()).getByRole("tab", { name: /steward 1/ }));
    fireEvent.contextMenu(await todoRowFor("Drop the old importer"));
    await userEvent.click(await screen.findByRole("menuitem", { name: /Forget todo/ }));
    await waitFor(() => expect(tabNames()).toEqual(["steward 1"]));
  });
});
