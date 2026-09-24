import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { forgetThisLaunch } from "./regions";

/**
 * **The panels follow the plane on disk** (charter-app#264).
 *
 * `charter ws todo done` closed four todos — the files were gone and the CLI no longer listed
 * them — and the window's Todos panel went on drawing all four for hours. The window read a
 * workspace once, when it was focused, and nothing ever told it the plane had changed under it.
 *
 * The core watches the plane now and says so on `plane-changed` (`planewatch.rs`, whose own
 * tests remove a real todo file from a real plane). This half is the window's: told, it reads
 * the workspace again, so the row goes and the count on the status line follows it.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;

/** The `todos/` store on disk, by slug. A test deletes from it the way `charter ws todo done`
 *  deletes a file. */
let disk: Map<string, string>;

function todosPanel() {
  return {
    key: "charter/todos",
    title: "Todos",
    order: 10,
    mark: "todo",
    from: null,
    about: null,
    blocks: [
      {
        kind: "list",
        rows: [...disk].map(([slug, title]) => ({
          key: slug,
          text: title,
          note: null,
          mark: "todo",
          tone: "plain",
          detail: null,
          runs: null,
        })),
        empty: { headline: "Nothing to do", body: null, offer: null },
      },
    ],
  };
}

/** The core, reading the plane afresh on every ask, and a way to say the plane changed. */
function core(): {
  asked: string[];
  changed: (plane: string) => void;
} {
  const asked: string[] = [];
  const listeners = new Map<string, number[]>();
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push(cmd);
    if (cmd === "plugin:event|listen") {
      const { event, handler } = a as { event: string; handler: number };
      listeners.set(event, [...(listeners.get(event) ?? []), handler]);
      return handler;
    }
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: [],
        persona: null,
        unfiled: [],
        workspaces: [
          { name: "alpha", path: ALPHA, vision: "", todos: [...disk.values()], chats: [] },
        ],
      };
    if (cmd === "workspace_panels")
      return {
        workspace: a.workspace,
        repos: [],
        absent: [],
        refused: [],
        todos: [...disk].map(([slug, title]) => ({ slug, title, stamp: "" })),
        todos_refused: null,
        personas: [],
        persona: null,
        contributed: [todosPanel()],
      };
    if (cmd === "workspace_repos")
      return { workspace: a.workspace, repos: [], cache_refused: null };
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "alerts_everywhere") return [{ plane: PLANE, alerts: [], stopped: null }];
    return null;
  });
  return {
    asked,
    changed: (plane) => {
      const handlers = listeners.get("plane-changed") ?? [];
      if (handlers.length === 0) throw new Error("the window is not listening for plane changes");
      for (const handler of handlers)
        window.__TAURI_INTERNALS__.runCallback(handler, {
          event: "plane-changed",
          id: 1,
          payload: { plane },
        });
    },
  };
}

const todoRows = () =>
  within(screen.getByTestId("panel-todos"))
    .queryAllByRole("listitem")
    .map((row) => row.textContent);

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
  disk = new Map([
    ["m8-1", "M8.1 Ship the watcher"],
    ["m8-2", "M8.2 Test the watcher"],
  ]);
});
afterEach(() => {
  cleanup();
  clearMocks();
});

describe("the panels, when the plane changes on disk", () => {
  it("drop a todo whose file was removed, and the status line's count follows", async () => {
    const { changed } = core();
    render(<App />);
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    expect(screen.getByTestId("status-todos")).toHaveTextContent("todo 2");

    // `charter ws todo done m8-1`, from a terminal: the file goes, and the core says so.
    disk.delete("m8-1");
    changed(PLANE);

    await waitFor(() => expect(todoRows()).toHaveLength(1));
    expect(todoRows()[0]).toContain("M8.2 Test the watcher");
    expect(screen.getByTestId("status-todos")).toHaveTextContent("todo 1");
  });

  it("take the alerts with them, which are about the plane too", async () => {
    const { asked, changed } = core();
    render(<App />);
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    const before = asked.filter((cmd) => cmd === "alerts_everywhere").length;

    changed(PLANE);

    await waitFor(() =>
      expect(asked.filter((cmd) => cmd === "alerts_everywhere").length).toBeGreaterThan(before),
    );
  });

  it("ignore a change to another plane, which is not the one on screen", async () => {
    const { asked, changed } = core();
    render(<App />);
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    const before = asked.filter((cmd) => cmd === "workspace_panels").length;

    disk.delete("m8-1");
    changed("/home/dev/other-plane");

    await new Promise((settle) => setTimeout(settle, 50));
    expect(asked.filter((cmd) => cmd === "workspace_panels")).toHaveLength(before);
    expect(todoRows()).toHaveLength(2);
  });
});
