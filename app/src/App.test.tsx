import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

// The pane's own terminal is driven by the scenario tests, against the real app. Here it
// stands in for one, so these tests are about the tabs, the splits and what they ask the core.
vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

/** The app as `main.tsx` renders it: in StrictMode, which runs effects and state updates
 *  twice, so anything that opens or ends a session twice shows up here. */
const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
});

/** Answers every command the app sends, and records what it was asked. */
function core(): { asked: { cmd: string; args: unknown }[] } {
  const asked: { cmd: string; args: unknown }[] = [];
  let opened = 0;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "plane_root") return "/home/dev/plane";
    if (cmd === "running_sessions") return [];
    if (cmd === "open_session") return ++opened;
    return null;
  });
  return { asked };
}

const panes = () => screen.getAllByTestId("pane").map((pane) => pane.textContent);
const tabs = () => screen.getAllByRole("tab").map((tab) => tab.textContent);

describe("App", () => {
  it("shows the plane the core found", async () => {
    mockIPC((cmd) => (cmd === "plane_root" ? "/home/dev/plane" : []));

    render(<App />);

    expect(await screen.findByText("/home/dev/plane")).toBeInTheDocument();
  });

  it("says why there is no plane when the core finds none", async () => {
    mockIPC(() => {
      throw new Error("no charter.toml in /tmp or any directory above it");
    });

    render(<App />);

    expect(await screen.findByRole("alert")).toHaveTextContent("no charter.toml in /tmp");
  });

  it("tells the core when its first frame is on screen", async () => {
    // Where cold start ends. The core prints it only when the app was started to be measured.
    const { asked } = core();

    render(<App />);

    await vi.waitFor(() => expect(asked.map((one) => one.cmd)).toContain("first_frame"));
  });

  it("does not send that marker once the window has gone", async () => {
    // It is sent a frame after the window paints, which can be after the window is gone —
    // and then it reaches whatever the next test, or the next window, has put there.
    const { asked } = core();

    const { unmount } = render(<App />);
    unmount();
    await new Promise((done) => setTimeout(done, 100));

    expect(asked.map((one) => one.cmd)).not.toContain("first_frame");
  });

  it("stays usable when the core cannot take that marker", async () => {
    // It is instrumentation: a window whose marker is refused is still a window, and an
    // unhandled rejection is not how it says so.
    mockIPC((cmd) => {
      if (cmd === "first_frame") throw new Error("no");
      if (cmd === "plane_root") return "/home/dev/plane";
      return null;
    });

    render(<App />);

    expect(await screen.findByText("/home/dev/plane")).toBeInTheDocument();
    expect(await screen.findByText(/No sessions/)).toBeInTheDocument();
  });

  it("ends sessions the core is left holding when the window reloads", async () => {
    // A reload leaves the window with no tabs and the core with every session it had, which
    // no pane can ever reach again.
    const asked: { cmd: string; args: unknown }[] = [];
    mockIPC((cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "plane_root") return "/home/dev/plane";
      if (cmd === "running_sessions") return [7, 8];
      return null;
    });

    render(<App />);

    await vi.waitFor(() =>
      expect(asked.filter(({ cmd }) => cmd === "close_session").map(({ args }) => args)).toEqual([
        { session: 7 },
        { session: 8 },
      ]),
    );
  });

  it("opens a session in a new tab", async () => {
    const { asked } = core();
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));

    expect(await screen.findByTestId("pane")).toHaveTextContent("session 1");
    expect(tabs()).toEqual(["1"]);
    expect(asked.some(({ cmd }) => cmd === "open_session")).toBe(true);
  });

  it("splits the pane in front into two, each with its own session", async () => {
    core();
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));

    await userEvent.click(screen.getByRole("button", { name: "Split right" }));

    expect(panes()).toEqual(["session 1", "session 2"]);
  });

  it("shows only the panes of the tab in front", async () => {
    core();
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));

    await userEvent.click(screen.getByRole("button", { name: "New tab" }));

    expect(tabs()).toEqual(["1", "2"]);
    expect(panes()).toEqual(["session 2"]);
  });

  it("brings a tab back to the front when it is chosen", async () => {
    core();
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
    await userEvent.click(screen.getByRole("button", { name: "New tab" }));

    await userEvent.click(screen.getAllByRole("tab")[0]);

    expect(panes()).toEqual(["session 1"]);
  });

  it("ends the sessions of a tab that closes", async () => {
    const { asked } = core();
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
    await userEvent.click(screen.getByRole("button", { name: "Split right" }));

    await userEvent.click(screen.getByRole("button", { name: "Close tab 1" }));

    expect(screen.queryAllByTestId("pane")).toEqual([]);
    expect(asked.filter(({ cmd }) => cmd === "close_session").map(({ args }) => args)).toEqual([
      { session: 1 },
      { session: 2 },
    ]);
  });

  it("ends only the session of a pane that closes", async () => {
    const { asked } = core();
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
    await userEvent.click(screen.getByRole("button", { name: "Split down" }));

    await userEvent.click(screen.getByRole("button", { name: "Close pane" }));

    expect(panes()).toEqual(["session 1"]);
    expect(asked.filter(({ cmd }) => cmd === "close_session").map(({ args }) => args)).toEqual([
      { session: 2 },
    ]);
  });

  it("ends a session it opened for a split whose tab closed while it was starting", async () => {
    // Starting a session is a real round trip: the tab can be gone by the time it answers,
    // and then nothing would ever show that session.
    const asked: { cmd: string; args: unknown }[] = [];
    let letTheSecondSessionStart = () => {};
    let opened = 0;
    mockIPC(async (cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "plane_root") return "/home/dev/plane";
      if (cmd === "running_sessions") return [];
      if (cmd !== "open_session") return null;
      if (++opened === 1) return 1;
      await new Promise<void>((starts) => (letTheSecondSessionStart = starts));
      return 2;
    });
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));

    await userEvent.click(screen.getByRole("button", { name: "Split right" }));
    await userEvent.click(screen.getByRole("button", { name: "Close tab 1" }));
    letTheSecondSessionStart();

    await vi.waitFor(() =>
      expect(asked.filter(({ cmd }) => cmd === "close_session").map(({ args }) => args)).toEqual([
        { session: 1 },
        { session: 2 },
      ]),
    );
    expect(screen.queryAllByTestId("pane")).toEqual([]);
  });

  it("says so when the core cannot start a session, and opens no tab", async () => {
    mockIPC((cmd) => {
      if (cmd === "plane_root") return "/home/dev/plane";
      if (cmd === "running_sessions") return [];
      throw new Error('could not start "zsh": no such file or directory');
    });
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("could not start");
    expect(screen.queryAllByTestId("pane")).toEqual([]);
  });
});
