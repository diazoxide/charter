import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

/**
 * What a chat's start hands the window to say (ADR 0085, V35): one line each, on the chat's own
 * pane, non-modal, until the operator dismisses it. Most starts have nothing to say, and then
 * nothing is drawn.
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

const PLANE = "/home/dev/plane";

const SIDEBAR = {
  root: PLANE,
  workspaces: [
    { name: "alpha", path: `${PLANE}/workspaces/alpha`, vision: "", todos: [], chats: [] },
  ],
  personas: ["steward"],
  persona: "steward",
  unfiled: [],
};

const START_OPTIONS = {
  profiles: [
    {
      name: "claude",
      kind: "claude",
      shown: "claude",
      source: "built-in",
      is_default: true,
      approval: null,
    },
  ],
  refused: [],
  personas: ["steward"],
  persona: "steward",
  ignore_fix: null,
  declares_none: true,
};

const HIDDEN =
  "/home/dev/plane/workspaces/alpha/svc/AGENTS.md: not written by charter, and hidden from git " +
  "status by the /AGENTS.md line charter keeps in that repository's info/exclude.";

function core(notices: string[]) {
  let started = 0;
  mockIPC((cmd) => {
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "opened_chats") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "start_chat") return { session: ++started, label: null, notices };
    return null;
  });
}

async function openAChat() {
  await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
  const picker = await screen.findByRole("dialog", { name: "Start a chat" });
  await userEvent.click(within(picker).getByRole("button", { name: "Start" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
}

describe("a chat's start notice", () => {
  it("names a hidden AGENTS.md on the chat's pane until it is dismissed", async () => {
    core([HIDDEN]);
    render(<App />);

    await openAChat();

    const note = await screen.findByRole("status", { name: "What this chat's start found" });
    expect(note).toHaveTextContent("workspaces/alpha/svc/AGENTS.md");
    await userEvent.click(within(note).getByRole("button", { name: "Dismiss" }));
    await waitFor(() =>
      expect(
        screen.queryByRole("status", { name: "What this chat's start found" }),
      ).not.toBeInTheDocument(),
    );
  });

  it("is not drawn when the start had nothing to say", async () => {
    core([]);
    render(<App />);

    await openAChat();

    await screen.findByTestId("pane");
    expect(
      screen.queryByRole("status", { name: "What this chat's start found" }),
    ).not.toBeInTheDocument();
  });
});
