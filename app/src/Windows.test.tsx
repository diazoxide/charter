import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render as renderBare,
  screen,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import App from "./App";
import type { OpenChat } from "./bindings";
import type { WindowSaid } from "./windows";

/**
 * A project tab split into an OS window of its own, and moved back (charter#126; ADR 0033,
 * amended 2026-09-26).
 *
 * What is pinned here is the window's half: which rows it offers, what it tells the core, what
 * it draws when it is a split window, and how the needs-you list and the quit warning reach
 * across windows. Which window holds which project, and what a closed window hands back, is the
 * core's (`planes.rs`, `windows.rs`) and tested there.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
});

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

const ONE = "/home/dev/one";
const TWO = "/home/dev/two";
const THREE = "/home/dev/three";

function chat(session: number, name: string): OpenChat {
  return {
    session,
    name,
    cwd: null,
    harness: null,
    in_front: true,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
    pinned: false,
    label: null,
    from: null,
  };
}

function core(
  over: {
    launch?: string | null;
    restore?: { windows: { planes: string[]; active: number | null }[]; dropped: string[] };
    handed?: { planes: string[]; active: number | null } | null;
    chats?: Record<string, OpenChat[]>;
    windows?: string[];
    holder?: Record<string, string>;
    refuseMove?: string;
  } = {},
) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  const listeners = new Map<string, number[]>();
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plugin:event|listen") {
      const { event, handler } = given as unknown as { event: string; handler: number };
      listeners.set(event, [...(listeners.get(event) ?? []), handler]);
      return 1;
    }
    const plane = (given.plane as string | undefined) ?? "";
    if (cmd === "plane_at_launch")
      return over.launch
        ? { plane: over.launch, from: over.launch, why: null }
        : { plane: null, from: null, why: null };
    if (cmd === "planes_to_restore") return over.restore ?? { windows: [], dropped: [] };
    if (cmd === "projects_handed") return over.handed ?? null;
    if (cmd === "charter_windows") return over.windows ?? ["main"];
    if (cmd === "show_window_holding") return over.holder?.[plane] ?? null;
    if (cmd === "move_projects") {
      if (over.refuseMove !== undefined) throw over.refuseMove;
      return (given.to as string | null) ?? "window-1";
    }
    if (cmd === "open_plane") return { plane: given.path, ask: null };
    if (cmd === "plane_sidebar")
      return { root: plane, workspaces: [], personas: [], persona: null, unfiled: [] };
    if (cmd === "opened_chats") return over.chats?.[plane] ?? [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "recent_planes") return { planes: [], dropped: [], forgetful: null };
    if (cmd === "extensions_on") return [];
    return null;
  });
  const fire = (event: string, payload: unknown) => {
    const handlers = listeners.get(event) ?? [];
    if (handlers.length === 0) throw new Error(`the window is not listening for ${event}`);
    act(() => {
      for (const handler of handlers)
        window.__TAURI_INTERNALS__.runCallback(handler, { event, id: 1, payload });
    });
  };
  return {
    asked,
    sent: (cmd: string) => asked.filter((one) => one.cmd === cmd).map((one) => one.args),
    fire,
    listening: (event: string) => (listeners.get(event) ?? []).length > 0,
  };
}

const projectTabs = () =>
  within(screen.getByRole("tablist", { name: "Projects" }))
    .getAllByRole("tab")
    .map(
      (tab) =>
        `${tab.querySelector(".project-name")?.textContent}${
          tab.getAttribute("aria-selected") === "true" ? "*" : ""
        }`,
    );

function projectTab(name: string): HTMLElement {
  const tab = within(screen.getByRole("tablist", { name: "Projects" }))
    .getAllByRole("tab")
    .find((one) => one.querySelector(".project-name")?.textContent === name);
  if (!tab) throw new Error(`no project tab called ${name}`);
  return tab;
}

/** Right-clicks a project tab and presses a row on its menu. */
async function fromTheMenu(project: string, row: string) {
  fireEvent.contextMenu(projectTab(project));
  const menu = await screen.findByRole("menu");
  await userEvent.click(within(menu).getByRole("menuitem", { name: row }));
}

/** What another window tells this one. */
function said(label: string, over: Partial<WindowSaid> = {}): WindowSaid {
  return { label, needing: [], quiet: [], ending: [], settled: true, ...over };
}

describe("a project moved into a window of its own", () => {
  it("leaves the window it was in without ending anything", async () => {
    const { sent } = core({
      restore: { windows: [{ planes: [ONE, TWO], active: 0 }], dropped: [] },
    });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*", "two"]));

    await fromTheMenu("two", "Move project two to a new window");

    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));
    expect(sent("move_projects")).toEqual([{ projects: [TWO], front: TWO, to: null }]);
    // **Nothing was closed**: its chats go on running, and the new window draws them.
    expect(sent("close_plane")).toEqual([]);
    expect(sent("close_session")).toEqual([]);
    // And the core is told what this window holds now, so its events stop coming here.
    await vi.waitFor(() =>
      expect(sent("window_holds_planes").pop()).toEqual({ held: { planes: [ONE], active: 0 } }),
    );
  });

  it("keeps its tab when the core refuses, and says why", async () => {
    core({
      restore: { windows: [{ planes: [ONE, TWO], active: 0 }], dropped: [] },
      refuseMove: "charter could not make a window: no display",
    });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*", "two"]));

    await fromTheMenu("two", "Move project two to a new window");

    expect(await screen.findByText("charter could not make a window: no display")).toBeVisible();
    expect(projectTabs()).toEqual(["one*", "two"]);
  });

  it("is drawn by a split window, which restores nothing and asks the launch nothing", async () => {
    mockWindows("window-1");
    const { sent } = core({
      handed: { planes: [TWO], active: 0 },
      chats: { [TWO]: [chat(1, "two.1")] },
    });
    render(<App />);

    await vi.waitFor(() => expect(projectTabs()).toEqual(["two*"]));
    expect(await screen.findByRole("tab", { name: /two\.1/ })).toBeInTheDocument();
    // A split window is not a launch: the working directory, the relaunch question and the
    // remembered window set are all the main window's.
    expect(sent("plane_at_launch")).toEqual([]);
    expect(sent("relaunch_ask")).toEqual([]);
    expect(sent("relaunch")).toEqual([]);
    expect(sent("planes_to_restore")).toEqual([]);
  });

  it("listens only for what is sent to every window or to it", async () => {
    // Tauri delivers every event to a listener whose target is `Any`, including one the core
    // sent to another window. A split window listening that way would answer the main window's
    // quit and take in projects moved to the main window.
    mockWindows("window-1");
    const { asked } = core({
      handed: { planes: [TWO], active: 0 },
      chats: { [TWO]: [chat(1, "two.1")] },
    });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["two*"]));
    await screen.findByRole("tab", { name: /two\.1/ });

    const listening = asked.filter((one) => one.cmd === "plugin:event|listen");
    const events = new Set(listening.map((one) => one.args.event));
    for (const event of ["quit-asked", "projects-arrived", "open-plane", "chat-moved", "run-offer"])
      expect(events).toContain(event);
    for (const one of listening)
      expect({ event: one.args.event, target: one.args.target }).toEqual({
        event: one.args.event,
        target: { kind: "AnyLabel", label: "window-1" },
      });
  });

  it("moves back to the main window from the split window, which then holds nothing", async () => {
    mockWindows("window-1");
    const { sent } = core({ handed: { planes: [TWO], active: 0 } });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["two*"]));

    await fromTheMenu("two", "Move project two to the main window");

    expect(sent("move_projects")).toEqual([{ projects: [TWO], front: TWO, to: "main" }]);
    // Holding nothing, a split window says so, and the core closes it (`windows.rs`).
    await vi.waitFor(() =>
      expect(sent("window_holds_planes").pop()).toEqual({ held: { planes: [], active: null } }),
    );
    expect(sent("close_plane")).toEqual([]);
  });

  it("does not offer the main window's projects a way back to where they already are", async () => {
    core({ restore: { windows: [{ planes: [ONE, TWO], active: 0 }], dropped: [] } });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*", "two"]));

    fireEvent.contextMenu(projectTab("two"));
    const menu = await screen.findByRole("menu");

    expect(within(menu).queryByRole("menuitem", { name: /to the main window/ })).toBeNull();
  });

  it("arrives in the window it was moved to, in front", async () => {
    const { fire } = core({ launch: ONE });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));

    fire("projects-arrived", { planes: [TWO], front: TWO });

    await vi.waitFor(() => expect(projectTabs()).toEqual(["one", "two*"]));
  });

  it("comes back behind what is showing when a split window is closed", async () => {
    const { fire } = core({ launch: ONE });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));

    fire("projects-arrived", { planes: [TWO, THREE], front: null });

    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*", "two", "three"]));
  });

  it("is not taken into a second window when it is opened again there", async () => {
    // One project is one tab in one window: opening it from another window's opener brings the
    // window holding it to the front instead.
    const { sent } = core({ launch: ONE, holder: { [TWO]: "window-1" } });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));

    await userEvent.click(screen.getByRole("button", { name: "Open a project…" }));
    const person = userEvent.setup();
    await person.type(await screen.findByLabelText("Or type a path"), TWO);
    await person.click(screen.getByRole("button", { name: "Open" }));

    await vi.waitFor(() => expect(sent("show_window_holding")).toContainEqual({ plane: TWO }));
    expect(projectTabs()).toEqual(["one"]);
  });
});

describe("a cold launch that remembers two windows", () => {
  it("puts the second window's projects back in a window of their own", async () => {
    const { sent } = core({
      restore: {
        windows: [
          { planes: [ONE], active: 0 },
          { planes: [TWO, THREE], active: 1 },
        ],
        dropped: [],
      },
    });
    render(<App />);

    await vi.waitFor(() =>
      expect(sent("move_projects")).toEqual([{ projects: [TWO, THREE], front: THREE, to: null }]),
    );
    // Each one still went through the gate, here, before it was moved.
    expect(sent("open_plane").map((one) => one.path)).toEqual([ONE, TWO, THREE]);
    expect(projectTabs()).toEqual(["one*"]);
  });
});

describe("needs you, across windows", () => {
  const go = (session: number) => ({
    id: `needs.show:${session}`,
    title: "Show the chat",
    available: true,
    reason: "",
    does: { verb: "showChat" as const, session },
  });

  it("lists a chat asking in another window, and goes there to show it", async () => {
    const { fire, asked } = core({
      launch: ONE,
      windows: ["main", "window-1"],
      holder: { [TWO]: "window-1" },
    });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));

    fire(
      "window-said",
      said("window-1", {
        needing: [
          { session: 3, name: "two.3", workspace: "alpha", go: go(3), plane: TWO, project: "two" },
        ],
      }),
    );

    const hand = await screen.findByRole("button", { name: "1 chat needs you" });
    await userEvent.click(hand);
    await userEvent.click(await screen.findByRole("menuitem", { name: /Go to two\.3/ }));

    await vi.waitFor(() =>
      expect(asked.find((one) => one.cmd === "plugin:event|emit_to")?.args).toMatchObject({
        target: { kind: "AnyLabel", label: "window-1" },
        event: "run-offer",
        payload: { plane: TWO, offer: go(3) },
      }),
    );
    expect(asked.some((one) => one.cmd === "show_window_holding" && one.args.plane === TWO)).toBe(
      true,
    );
  });

  it("drops what a window said once that window has gone", async () => {
    const { fire } = core({ launch: ONE, windows: ["main", "window-1"] });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));
    fire(
      "window-said",
      said("window-1", {
        needing: [
          { session: 3, name: "two.3", workspace: "alpha", go: go(3), plane: TWO, project: "two" },
        ],
      }),
    );
    await screen.findByRole("button", { name: "1 chat needs you" });

    fire("windows-changed", ["main"]);

    await vi.waitFor(() =>
      expect(screen.queryByRole("button", { name: "1 chat needs you" })).toBeNull(),
    );
  });
});

describe("quitting with two windows", () => {
  const ending = {
    key: `${TWO}#1`,
    project: "two",
    name: "two.1",
    harness: null,
    cwd: null,
    state: "running" as const,
  };

  it("warns about the chats in the other window too", async () => {
    const { fire, sent } = core({ launch: ONE, windows: ["main", "window-1"] });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));
    fire("window-said", said("window-1", { ending: [ending] }));

    fire("quit-asked", null);

    const dialog = within(await screen.findByRole("dialog"));
    expect(within(dialog.getByRole("list")).getByText("two.1")).toBeInTheDocument();
    expect(sent("quit")).toEqual([]);
  });

  it("warns rather than quitting while another window has not said what it has open", async () => {
    const { fire, sent } = core({ launch: ONE, windows: ["main", "window-1"] });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));

    fire("quit-asked", null);

    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(sent("quit")).toEqual([]);
  });

  it("quits at once when no window has anything open", async () => {
    const { fire, sent } = core({ launch: ONE, windows: ["main", "window-1"] });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));
    fire("window-said", said("window-1"));

    await vi.waitFor(() => {
      fire("quit-asked", null);
      expect(sent("quit").length).toBeGreaterThan(0);
    });
  });
});
