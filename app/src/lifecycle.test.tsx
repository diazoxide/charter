import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { OpenChat } from "./bindings";

/** What Tauri's own mocks put on the window: the registry `listen` hands its callback to,
 *  which is how a test fires an event the app is listening for. */
declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

/**
 * The window over a whole day: closed to the tray with everything still running, quit with a
 * warning about what that ends, and opened again on the chats it was left on.
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

type Asked = { cmd: string; args: unknown };

/** A chat the core says it has open, with everything not under test left plain. */
function chat(one: Partial<OpenChat> & { session: number }): OpenChat {
  return {
    name: `ide.${one.session}`,
    cwd: "/home/dev/plane/workspaces/ide",
    harness: "claude",
    in_front: false,
    resumed: null,
    fresh: null,
    ...one,
  };
}

/** The core, answering with `open` as the chats it already has. */
function core(
  open: OpenChat[] = [],
  wouldNot: [string, string][] = [],
): {
  asked: Asked[];
  /** Fires the event the app is sent when something asks it to quit. */
  askToQuit: () => Promise<void>;
} {
  const asked: Asked[] = [];
  const listeners = new Map<string, number>();
  let opened = 0;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, handler);
      return 1;
    }
    if (cmd === "plane_root") return "/home/dev/plane";
    if (cmd === "opened_chats") return open;
    if (cmd === "chats_that_would_not_start") return wouldNot;
    if (cmd === "open_session") return open.length + ++opened;
    return null;
  });
  return {
    asked,
    askToQuit: async () => {
      const handler = listeners.get("quit-asked");
      if (handler === undefined) throw new Error("the window is not listening for a quit");
      window.__TAURI_INTERNALS__.runCallback(handler, {
        event: "quit-asked",
        id: 1,
        payload: null,
      });
      await vi.waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    },
  };
}

const tabs = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .getAllByRole("tab")
    .map((tab) => tab.textContent);
const panes = () => screen.getAllByTestId("pane").map((pane) => pane.textContent);
const of = (cmd: string, asked: Asked[]) => asked.filter((one) => one.cmd === cmd);

describe("what the window does with the chats the core already has", () => {
  it("shows a tab for each one instead of ending it", async () => {
    // This is the relaunch: the core put the record back before there was a window, so the
    // window's job is to draw what is already running — never to sweep it away.
    const { asked } = core([chat({ session: 7 }), chat({ session: 8 })]);

    render(<App />);

    await vi.waitFor(() => expect(tabs()).toEqual(["ide.7", "ide.8"]));
    expect(of("close_session", asked)).toEqual([]);
  });

  it("shows the one that was in front when the app was quit", async () => {
    core([chat({ session: 7 }), chat({ session: 8, in_front: true })]);

    render(<App />);

    await vi.waitFor(() => expect(panes()).toEqual(["session 8"]));
  });

  it("says a chat was resumed, and by which conversation", async () => {
    core([chat({ session: 7, resumed: "11111111-2222-4333-8444-555555555555", in_front: true })]);

    render(<App />);

    expect(await screen.findByText(/resumed/i)).toHaveTextContent("11111111");
  });

  it("says a chat came back as a new one, and why", async () => {
    // The honest half: a Codex chat has no conversation the app could have recorded, and the
    // window says that rather than letting it look like the chat it was.
    core([
      chat({
        session: 7,
        harness: "codex",
        fresh: "no conversation was recorded for it",
        in_front: true,
      }),
    ]);

    render(<App />);

    expect(await screen.findByText(/new chat/i)).toHaveTextContent(
      "no conversation was recorded for it",
    );
  });

  it("says nothing about a shell that was not resumed", async () => {
    // Every chat is a shell until the harness picker lands, and a shell has no conversation
    // to bring back. Explaining that on every relaunch, forever, is noise about the normal
    // case — the note is for a harness that could have been resumed and was not.
    core([
      chat({
        session: 7,
        harness: null,
        fresh: "charter has not measured how this program resumes",
        in_front: true,
      }),
    ]);

    render(<App />);

    await vi.waitFor(() => expect(tabs()).toHaveLength(1));
    expect(screen.queryByText(/new chat/i)).not.toBeInTheDocument();
  });

  it("names the chats this launch could not start, and says they are still recorded", async () => {
    // A chat whose directory has moved would otherwise just be a tab that is quietly not
    // there — and the operator has no way to know it is still coming back.
    core([chat({ session: 7, in_front: true })], [["ide.9", "no such file or directory"]]);

    render(<App />);

    const said = await screen.findByText(/did not start/i);
    expect(said).toHaveTextContent("ide.9");
    expect(said).toHaveTextContent("no such file or directory");
    expect(said).toHaveTextContent(/still recorded/i);
  });

  it("says nothing about either when a chat is one the operator just opened", async () => {
    const { asked } = core();
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));

    expect(screen.queryByText(/resumed/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/new chat/i)).not.toBeInTheDocument();
    expect(of("open_session", asked)).toHaveLength(1);
  });

  it("tells the core which chat is in front, so the next quit records it", async () => {
    const { asked } = core([chat({ session: 7, in_front: true }), chat({ session: 8 })]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toEqual(["ide.7", "ide.8"]));

    await userEvent.click(
      within(screen.getByRole("tablist", { name: "Tabs" })).getAllByRole("tab")[1],
    );

    await vi.waitFor(() =>
      expect(of("chat_in_front", asked).slice(-1)[0]?.args).toEqual({ session: 8 }),
    );
  });
});

describe("being asked to quit", () => {
  it("names every session it is about to end", async () => {
    const { askToQuit } = core([chat({ session: 7 }), chat({ session: 8, harness: null })]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toHaveLength(2));

    await askToQuit();

    const dialog = within(screen.getByRole("dialog"));
    expect(dialog.getByText(/2 sessions/)).toBeInTheDocument();
    expect(dialog.getByText(/ide\.7/)).toBeInTheDocument();
    expect(dialog.getByText(/ide\.8/)).toBeInTheDocument();
  });

  it("does not claim to know whether a session is mid-turn", async () => {
    // Session state comes from hooks only (spec decision 3), and hooks are M1.3. The dialog
    // says what charter can tell and names what it cannot, rather than implying it knows a
    // harness is thinking.
    const { askToQuit } = core([chat({ session: 7 })]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toHaveLength(1));

    await askToQuit();

    expect(within(screen.getByRole("dialog")).getByText(/cannot yet tell/i)).toHaveTextContent(
      "mid-turn",
    );
  });

  it("quits when that is the answer", async () => {
    const { asked, askToQuit } = core([chat({ session: 7 })]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toHaveLength(1));
    await askToQuit();

    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: /quit/i }),
    );

    expect(of("quit", asked)).toHaveLength(1);
  });

  it("stays open when the answer is no, and ends nothing", async () => {
    const { asked, askToQuit } = core([chat({ session: 7 })]);
    render(<App />);
    await vi.waitFor(() => expect(tabs()).toHaveLength(1));
    await askToQuit();

    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: /cancel/i }),
    );

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(of("quit", asked)).toEqual([]);
    expect(of("close_session", asked)).toEqual([]);
    // And the core is told, so the next Cmd-Q warns again instead of quitting outright.
    expect(of("quit_cancelled", asked)).toHaveLength(1);
  });

  it("warns rather than quitting while it is still finding out what is open", async () => {
    // A launch answers `opened_chats` after the window is already interactive. Quitting on
    // "no tabs yet" would end fifty chats the window had not drawn.
    const asked: Asked[] = [];
    const listeners = new Map<string, number>();
    let letTheChatsArrive = () => {};
    mockIPC(async (cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "plugin:event|listen") {
        const { event, handler } = args as { event: string; handler: number };
        listeners.set(event, handler);
        return 1;
      }
      if (cmd === "plane_root") return "/home/dev/plane";
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd !== "opened_chats") return null;
      await new Promise<void>((arrive) => (letTheChatsArrive = arrive));
      return [chat({ session: 7 })];
    });
    render(<App />);
    await vi.waitFor(() => expect(listeners.has("quit-asked")).toBe(true));

    window.__TAURI_INTERNALS__.runCallback(listeners.get("quit-asked") as number, {
      event: "quit-asked",
      id: 1,
      payload: null,
    });

    await vi.waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    expect(of("quit", asked)).toEqual([]);
    letTheChatsArrive();
  });

  it("quits straight away when there is nothing to end", async () => {
    // A warning listing nothing is a dialog in the way.
    const { asked } = core();
    render(<App />);
    await screen.findByText(/No sessions/);

    const listen = of("plugin:event|listen", asked).find(
      (one) => (one.args as { event: string }).event === "quit-asked",
    );
    window.__TAURI_INTERNALS__.runCallback((listen?.args as { handler: number }).handler, {
      event: "quit-asked",
      id: 1,
      payload: null,
    });

    await vi.waitFor(() => expect(of("quit", asked)).toHaveLength(1));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
