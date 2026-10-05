import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { clearMocks, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import App from "./App";
import type { OpenChat } from "./bindings";

/**
 * A project the core let go of leaves the window (#1242).
 *
 * `close_plane` is a command anything can ask — the window's own `×`, the UI RPC an end-to-end
 * spec drives, a second window. Whichever asked, the core says `plane-closed`, and every window
 * that draws that project takes its tab out, with its chats and its views, and shows what
 * comes next. Before this the window that had not asked went on drawing a project the core no
 * longer held.
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
    card: null,
    guessed: null,
    pinned: false,
    label: null,
    from: null,
  };
}

function core(over: { restore?: string[]; handed?: string[]; chats?: Record<string, OpenChat[]> }) {
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
    if (cmd === "plane_at_launch") return { plane: null, from: null, why: null };
    if (cmd === "planes_to_restore")
      return over.restore
        ? { windows: [{ planes: over.restore, active: over.restore.length - 1 }], dropped: [] }
        : { windows: [], dropped: [] };
    if (cmd === "projects_handed") return over.handed ? { planes: over.handed, active: 0 } : null;
    if (cmd === "charter_windows") return ["main"];
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
  /** What the core tells the window, as Tauri delivers it. */
  const fire = (event: string, payload: unknown) => {
    const handlers = listeners.get(event) ?? [];
    if (handlers.length === 0) throw new Error(`the window is not listening for ${event}`);
    act(() => {
      for (const handler of handlers)
        window.__TAURI_INTERNALS__.runCallback(handler, { event, id: 1, payload });
    });
  };
  return {
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

describe("a project the core closed", () => {
  it("leaves the window, with its chats, and the project beside it comes to the front", async () => {
    const { fire, listening, sent } = core({
      restore: [ONE, TWO],
      chats: { [TWO]: [chat(1, "two.1")] },
    });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one", "two*"]));
    expect(await screen.findByRole("tab", { name: /two\.1/ })).toBeInTheDocument();
    await vi.waitFor(() => expect(listening("plane-closed")).toBe(true));

    // Asked behind the window's back: the UI RPC, or another window.
    fire("plane-closed", { plane: TWO });

    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));
    expect(screen.queryByRole("tab", { name: /two\.1/ })).not.toBeInTheDocument();
    // The core is told what this window holds now, so nothing of the closed project is
    // remembered as open in it.
    await vi.waitFor(() =>
      expect(sent("window_holds_planes").pop()).toEqual({ held: { planes: [ONE], active: 0 } }),
    );
    // The window only drew what the core had already done; it asks for no second close.
    expect(sent("close_plane")).toEqual([]);
  });

  it("leaves the opener when it was the last project", async () => {
    const { fire, listening } = core({ restore: [ONE] });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));
    await vi.waitFor(() => expect(listening("plane-closed")).toBe(true));

    fire("plane-closed", { plane: ONE });

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "You have not opened a project yet",
    );
  });

  it("changes nothing in a window that does not draw it", async () => {
    const { fire, listening } = core({ restore: [ONE] });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));
    await vi.waitFor(() => expect(listening("plane-closed")).toBe(true));

    fire("plane-closed", { plane: TWO });

    expect(projectTabs()).toEqual(["one*"]);
  });

  it("leaves a split window too, which then holds nothing", async () => {
    mockWindows("window-1");
    const { fire, listening, sent } = core({ handed: [TWO] });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["two*"]));
    await vi.waitFor(() => expect(listening("plane-closed")).toBe(true));

    fire("plane-closed", { plane: TWO });

    // Holding nothing, a split window says so, and the core closes it (`windows.rs`).
    await vi.waitFor(() =>
      expect(sent("window_holds_planes").pop()).toEqual({ held: { planes: [], active: null } }),
    );
  });
});
