import { Profiler, StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { forgetThisLaunch } from "./regions";
import type { PlaneAnswer } from "./bindings";
import { stripNamed } from "./test-strips";

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

/** The chats the core put back, and which of them it says are running on instructions the
 *  plane has changed since they started (charter#369). */
let chatsOpen: unknown[];
let updated: { session: number; files: string[] }[];

/** The workspaces on disk, and the sidebar reads held back to answer late, oldest first: the
 *  core reads the sidebar off the window's thread (SC-2), so an older read can answer after a
 *  newer one. */
let workspacesOnDisk: string[];
let holdNextSidebar: boolean;
let heldSidebars: (() => void)[];

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
  changed: (plane: string, answers?: PlaneAnswer[] | null) => void;
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
    if (cmd === "opened_chats") return chatsOpen;
    if (cmd === "chats_plane_updated") return updated;
    if (cmd === "plane_sidebar") {
      const answer = {
        root: PLANE,
        personas: [],
        persona: null,
        unfiled: [],
        workspaces: workspacesOnDisk.map((name) => ({
          name,
          path: `${PLANE}/workspaces/${name}`,
          vision: "",
          todos: name === "alpha" ? [...disk.values()] : [],
          chats: [],
        })),
      };
      if (!holdNextSidebar) return answer;
      holdNextSidebar = false;
      return new Promise((answered) => heldSidebars.push(() => answered(answer)));
    }
    if (cmd === "workspace_panels")
      return {
        workspace: a.workspace,
        repos: [],
        paths: {},
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
    changed: (plane, answers) => {
      const handlers = listeners.get("plane-changed") ?? [];
      if (handlers.length === 0) throw new Error("the window is not listening for plane changes");
      for (const handler of handlers)
        window.__TAURI_INTERNALS__.runCallback(handler, {
          event: "plane-changed",
          id: 1,
          payload: answers === undefined ? { plane } : { plane, changes: [], answers },
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
  chatsOpen = [];
  updated = [];
  workspacesOnDisk = ["alpha"];
  holdNextSidebar = false;
  heldSidebars = [];
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

/**
 * **A reader asks again only for a change its answer is made of** (FD-10). The core says what
 * moved — each path, and whether it is a todo of `alpha`, a memory, a session record — and a
 * memory an agent saves no longer makes the sidebar list every workspace's todos once more.
 */
describe("the readers of the plane, told what changed", () => {
  const count = (asked: string[], cmd: string) => asked.filter((one) => one === cmd).length;
  const settle = () => new Promise((done) => setTimeout(done, 50));

  it("read the focused workspace's panels again for its memory, and leave the sidebar be", async () => {
    const { asked, changed } = core();
    render(<App />);
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    const panels = count(asked, "workspace_panels");
    const sidebar = count(asked, "plane_sidebar");

    // What the core says a memory of alpha concerns.
    changed(PLANE, [{ answer: "panels", workspace: "alpha" }, { answer: "views" }]);

    await waitFor(() => expect(count(asked, "workspace_panels")).toBeGreaterThan(panels));
    await settle();
    expect(count(asked, "plane_sidebar")).toBe(sidebar);
  });

  it("read the sidebar again for another workspace's todo, and not the focused one's panels", async () => {
    const { asked, changed } = core();
    render(<App />);
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    const panels = count(asked, "workspace_panels");
    const sidebar = count(asked, "plane_sidebar");

    // What the core says a todo of beta concerns.
    changed(PLANE, [
      { answer: "sidebar" },
      { answer: "panels", workspace: "beta" },
      { answer: "git" },
      { answer: "views" },
    ]);

    await waitFor(() => expect(count(asked, "plane_sidebar")).toBeGreaterThan(sidebar));
    await settle();
    expect(count(asked, "workspace_panels")).toBe(panels);
  });

  it("draw nothing again and ask git nothing for a memory or record of a workspace not in front", async () => {
    // A memory an agent saves is the commonest write there is. The window in front is on
    // `alpha`; one saved in `beta` concerns no panel on screen, and redrawing the whole window
    // for it — or asking git for the standings — is what made switching slow (FR-27).
    const { asked, changed } = core();
    let commits = 0;
    render(
      <Profiler id="window" onRender={() => (commits += 1)}>
        <App />
      </Profiler>,
    );
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    await settle();
    const before = asked.length;
    const drawn = commits;

    // What the core says a memory and a session record of beta concern.
    changed(PLANE, [{ answer: "panels", workspace: "beta" }, { answer: "views" }]);

    await settle();
    expect(asked.slice(before)).toEqual([]);
    expect(commits).toBe(drawn);
  });

  it("ask nothing but the focused workspace's panels again for a memory saved in it", async () => {
    // FD-10d: a memory an agent saves in the workspace in front is part of its panels and of
    // the memory views, and of nothing else the window reads — not the sidebar, not the
    // instructions a chat started on, not the curations, and no git reader.
    const { asked, changed } = core();
    render(<App />);
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    await settle();
    const before = asked.length;

    // What the core says a memory of alpha concerns.
    changed(PLANE, [{ answer: "panels", workspace: "alpha" }, { answer: "views" }]);

    await waitFor(() => expect(asked.slice(before)).toContain("workspace_panels"));
    await settle();
    expect(asked.slice(before)).toEqual(["workspace_panels"]);
  });

  it("ask neither the instructions nor the curations again for a todo", async () => {
    // FD-10d: neither is made of a todo, and a todo is closed many times a day.
    const { asked, changed } = core();
    render(<App />);
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    await waitFor(() => expect(asked).toContain("chats_plane_updated"));
    await waitFor(() => expect(asked).toContain("curation_offers"));
    await settle();
    const instructions = count(asked, "chats_plane_updated");
    const curations = count(asked, "curation_offers");
    const sidebar = count(asked, "plane_sidebar");

    // What the core says a todo of beta concerns.
    changed(PLANE, [
      { answer: "sidebar" },
      { answer: "panels", workspace: "beta" },
      { answer: "git" },
      { answer: "views" },
    ]);

    await waitFor(() => expect(count(asked, "plane_sidebar")).toBeGreaterThan(sidebar));
    await settle();
    expect(count(asked, "chats_plane_updated")).toBe(instructions);
    expect(count(asked, "curation_offers")).toBe(curations);
  });

  it("ask the instructions and the curations again for a persona's charter", async () => {
    const { asked, changed } = core();
    render(<App />);
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    await waitFor(() => expect(asked).toContain("chats_plane_updated"));
    await waitFor(() => expect(asked).toContain("curation_offers"));
    await settle();
    const instructions = count(asked, "chats_plane_updated");
    const curations = count(asked, "curation_offers");

    // What the core says `personas/steward/persona.md` concerns.
    changed(PLANE, [
      { answer: "sidebar" },
      { answer: "panels", workspace: null },
      { answer: "instructions" },
      { answer: "curations" },
      { answer: "git" },
      { answer: "views" },
    ]);

    await waitFor(() => expect(count(asked, "chats_plane_updated")).toBeGreaterThan(instructions));
    await waitFor(() => expect(count(asked, "curation_offers")).toBeGreaterThan(curations));
  });

  it("keep the newest sidebar when an older read of it answers last", async () => {
    const { asked, changed } = core();
    render(<App />);
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    const workspaceTabs = () =>
      within(stripNamed("Workspaces"))
        .queryAllByRole("tab")
        .map((tab) => tab.textContent ?? "");
    const drawn = (name: string) => workspaceTabs().some((tab) => tab.includes(name));
    // What the core says a todo of alpha concerns.
    const todos: PlaneAnswer[] = [
      { answer: "sidebar" },
      { answer: "panels", workspace: "alpha" },
      { answer: "git" },
      { answer: "views" },
    ];

    // A read that is slow to answer: it was asked while `alpha` was still on disk.
    const sidebar = count(asked, "plane_sidebar");
    holdNextSidebar = true;
    changed(PLANE, todos);
    await waitFor(() => expect(heldSidebars).toHaveLength(1));
    expect(count(asked, "plane_sidebar")).toBeGreaterThan(sidebar);

    // `charter workspace rename alpha gamma`, read by the next one, which answers at once.
    workspacesOnDisk = ["gamma"];
    changed(PLANE, todos);
    await waitFor(() => expect(drawn("gamma")).toBe(true));

    heldSidebars[0]?.();
    await settle();
    expect(drawn("gamma")).toBe(true);
    expect(drawn("alpha")).toBe(false);
  });

  it("ask git again when auto-save says what it did, and read neither the sidebar nor the panels", async () => {
    // #933: a save, a push or a fetch used to be told as "anything may have moved", and every
    // reader read the plane again about 30 s after each memory written.
    const { asked, changed } = core();
    render(<App />);
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    await waitFor(() => expect(asked).toContain("alerts_everywhere"));
    await settle();
    const alerts = count(asked, "alerts_everywhere");
    const panels = count(asked, "workspace_panels");
    const sidebar = count(asked, "plane_sidebar");

    changed(PLANE, [{ answer: "git" }]);

    await waitFor(() => expect(count(asked, "alerts_everywhere")).toBeGreaterThan(alerts));
    await settle();
    expect(count(asked, "workspace_panels")).toBe(panels);
    expect(count(asked, "plane_sidebar")).toBe(sidebar);
  });

  it("read everything again when the core cannot say what changed", async () => {
    const { asked, changed } = core();
    render(<App />);
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    const panels = count(asked, "workspace_panels");
    const sidebar = count(asked, "plane_sidebar");

    changed(PLANE, null);

    await waitFor(() => expect(count(asked, "workspace_panels")).toBeGreaterThan(panels));
    await waitFor(() => expect(count(asked, "plane_sidebar")).toBeGreaterThan(sidebar));
  });
});

describe("what a project has on, when its settings change on disk (charter-app#253)", () => {
  it("is asked again, so an edit to charter.toml or charter.local.toml takes effect", async () => {
    const { asked, changed } = core();
    render(<App />);
    await waitFor(() => expect(todoRows()).toHaveLength(2));
    await waitFor(() => expect(asked).toContain("extensions_on"));
    const before = asked.filter((cmd) => cmd === "extensions_on").length;

    // `[extensions.x] enabled = false` written in an editor, or arriving with a `git pull`.
    changed(PLANE);

    await waitFor(() =>
      expect(asked.filter((cmd) => cmd === "extensions_on").length).toBeGreaterThan(before),
    );
  });
});

describe("a chat running on instructions the plane has changed since (charter#369)", () => {
  const steward = {
    session: 1,
    name: "steward",
    cwd: ALPHA,
    harness: "claude-code",
    in_front: true,
    resumed: null,
    fresh: null,
    profile: null,
    persona: "steward",
    unreported: null,
    guessed: null,
    pinned: false,
    label: null,
    from: null,
  };
  // On the strip the mark is the button that starts the chat fresh (NO-3), named for both.
  const mark = () => screen.queryByRole("button", { name: /project updated/i });

  it("has its tab marked once the plane says so, naming what changed", async () => {
    chatsOpen = [steward];
    const { changed } = core();
    render(<App />);
    await waitFor(() => expect(screen.getAllByTestId("pane").length).toBeGreaterThan(0));
    expect(mark()).toBeNull();

    // `CLAUDE.md` edited in another chat, or arriving with a `git pull`.
    updated = [{ session: 1, files: ["CLAUDE.md"] }];
    changed(PLANE);

    await waitFor(() => expect(mark()).not.toBeNull());
    expect(mark()).toHaveAttribute("title", expect.stringContaining("CLAUDE.md"));
  });

  it("is not a needs-you item: the queue and its counts stay as they were", async () => {
    chatsOpen = [steward];
    updated = [{ session: 1, files: ["CLAUDE.md"] }];
    core();
    render(<App />);

    await waitFor(() => expect(mark()).not.toBeNull());
    expect(screen.queryByRole("button", { name: /need you/i })).toBeNull();
  });
});
