import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { OpenChat } from "./bindings";

/**
 * A chat a handoff opened lands on a strip, behind the chat the operator is reading
 * (charter-app#204).
 *
 * The core starts the chat and says so on `handoff-arrived`; the window's whole part is where
 * the tab goes. **Not in front**: a handoff is work sent away from the chat on screen, and a
 * tab that took the front would interrupt it. Visibility is the control a handoff from inside
 * the app rests on (`charter_core::hookwire::OpenChat`), so the tab has to be there, and it
 * has to be there without taking the screen.
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

function chat(session: number): OpenChat {
  return {
    session,
    name: `ide.${session}`,
    cwd: "/home/dev/plane/workspaces/ide",
    // No harness and no persona, so its tab is its own name alone and the assertions below read
    // the names they gave it (the default before the name is charter-app#254's, tested there).
    harness: null,
    in_front: session === 1,
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

/** The core, holding `open` chats, and a way to say a handoff opened one. */
function core(open: OpenChat[]): { arrive: (payload: unknown) => void } {
  const listeners = new Map<string, number>();
  mockIPC((cmd, args) => {
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, handler);
      return 1;
    }
    if (cmd === "plane_at_launch")
      return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
    if (cmd === "opened_chats") return open;
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    return null;
  });
  return {
    arrive: (payload) => {
      const handler = listeners.get("handoff-arrived");
      if (handler === undefined) throw new Error("the window is not listening for handoffs");
      window.__TAURI_INTERNALS__.runCallback(handler, {
        event: "handoff-arrived",
        id: 1,
        payload,
      });
    },
  };
}

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const tabNames = () =>
  within(strip())
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);
const selected = () =>
  within(strip())
    .getAllByRole("tab")
    .filter((tab) => tab.getAttribute("aria-selected") === "true")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

/** The tab whose name is `name`. */
const tabNamed = (name: string) =>
  within(strip())
    .getAllByRole("tab")
    .find((tab) => tab.querySelector(".tab-name")?.textContent === name);

describe("a chat a handoff opened, named for its task (charter-app#258)", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("is called the task name the handoff gave it, and says where it came from by name", async () => {
    const { arrive } = core([chat(1)]);
    render(<App />);
    await waitFor(() => expect(tabNames()).toEqual(["ide.1"]));

    arrive({
      plane: "/home/dev/plane",
      session: 2,
      name: "2",
      label: "drop commons",
      from: { name: "steward 3", workspace: "platform-next" },
      workspace: "ide",
      persona: null,
      harness: "claude",
    });

    await waitFor(() => expect(tabNames()).toEqual(["ide.1", "drop commons"]));
    expect(tabNamed("drop commons")).toHaveAttribute("title", "↳ from steward 3 · platform-next");
  });

  it("makes four handoffs from one chat four tabs that can be told apart", async () => {
    // The operator's report: four handoffs, four tabs, every one "handoff from 16".
    const { arrive } = core([chat(1)]);
    render(<App />);
    await waitFor(() => expect(tabNames()).toEqual(["ide.1"]));

    for (const session of [2, 3, 4, 5])
      arrive({
        plane: "/home/dev/plane",
        session,
        name: String(session),
        from: { name: "ide.1", workspace: "ide" },
        workspace: "ide",
        persona: "steward",
      });

    await waitFor(() =>
      expect(tabNames()).toEqual(["ide.1", "steward 2", "steward 3", "steward 4", "steward 5"]),
    );
  });

  it("comes back from the record still saying where it came from, in its tab and its pane", async () => {
    core([
      {
        ...chat(1),
        label: "drop commons",
        from: { name: "steward 3", workspace: "platform-next" },
      },
    ]);
    render(<App />);

    await waitFor(() => expect(tabNames()).toEqual(["drop commons"]));
    expect(tabNamed("drop commons")).toHaveAttribute("title", "↳ from steward 3 · platform-next");
    expect(document.querySelector(".pane-from")).toHaveTextContent(
      "↳ from steward 3 · platform-next",
    );
  });

  it("says nothing of where a chat the operator opened came from", async () => {
    core([chat(1)]);
    render(<App />);

    await waitFor(() => expect(tabNames()).toEqual(["ide.1"]));
    expect(tabNamed("ide.1")).not.toHaveAttribute("title");
    expect(document.querySelector(".pane-from")).toBeNull();
  });
});

describe("a chat a handoff opened", () => {
  afterEach(() => {
    cleanup();
    clearMocks();
  });

  it("is drawn on the strip without taking the front from the chat being read", async () => {
    const { arrive } = core([chat(1)]);
    render(<App />);
    await waitFor(() => expect(tabNames()).toEqual(["ide.1"]));

    arrive({
      plane: "/home/dev/plane",
      session: 2,
      name: "handoff from 1",
      workspace: "ide",
      persona: null,
    });

    await waitFor(() => expect(tabNames()).toEqual(["ide.1", "handoff from 1"]));
    expect(selected()).toEqual(["ide.1"]);
  });

  it("is named by its persona, or its harness, and then its name, as any chat is (charter-app#254)", async () => {
    const { arrive } = core([chat(1)]);
    render(<App />);
    await waitFor(() => expect(tabNames()).toEqual(["ide.1"]));

    arrive({
      plane: "/home/dev/plane",
      session: 2,
      name: "handoff from 1",
      workspace: "ide",
      persona: null,
      harness: "claude",
    });

    await waitFor(() => expect(tabNames()).toEqual(["ide.1", "claude handoff from 1"]));
  });

  it("is not drawn in a window showing another plane", async () => {
    // Every plane numbers its chats from one, so the plane is half the chat's identity.
    const { arrive } = core([chat(1)]);
    render(<App />);
    await waitFor(() => expect(tabNames()).toEqual(["ide.1"]));

    arrive({
      plane: "/home/dev/other-plane",
      session: 2,
      name: "handoff from 1",
      workspace: "ide",
      persona: null,
    });

    await new Promise((settle) => setTimeout(settle, 50));
    expect(tabNames()).toEqual(["ide.1"]);
  });
});
