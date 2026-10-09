import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { forgetReadsOutsideChatsHere, readsOutsideChatsHere } from "./chatState";
import { forgetDismissals } from "./dismissals";
import { forgetThisLaunch } from "./regions";

/**
 * **Every chat state the window draws is read below `ChatsHere`** (#1037). A component that
 * reads the chats with no project above it draws every chat as "unknown" and says nothing
 * (`chatsHere.test.tsx`); this holds the whole window to none such read, with a chat in a tab, a
 * task it dispatched, and the explorer and the Chats list drawn.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;

const CHAT = {
  session: 4,
  name: "claude 4",
  cwd: `${ALPHA}/svc`,
  harness: "claude",
  in_front: true,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona: null,
  unreported: null,
  guessed: null,
  pinned: false,
  label: null,
  from: null,
};

/** A task chat 4 dispatched, with no tab. */
const TASK = {
  ...CHAT,
  session: 6,
  name: "devops 6",
  in_front: false,
  persona: "devops",
  label: "talk",
  from: {
    chat: 4,
    name: "claude 4",
    workspace: "alpha",
    task: true,
    tab: false,
    reported: false,
    unreported: false,
  },
};

function core() {
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return [CHAT];
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          personas: ["steward", "devops"],
          persona: "steward",
          unfiled: [],
          workspaces: [{ name: "alpha", path: ALPHA, vision: "", todos: [], chats: [CHAT, TASK] }],
        };
      if (cmd === "workspace_panels")
        return {
          workspace: a.workspace,
          repos: ["svc"],
          paths: { svc: `${ALPHA}/svc` },
          absent: [],
          refused: [],
          todos: [],
          todos_refused: null,
          personas: ["steward", "devops"],
          persona: "steward",
        };
      if (cmd === "workspace_repos")
        return { workspace: a.workspace, repos: [], cache_refused: null };
      if (cmd === "worktree_list") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "owed_restarts") return [];
      if (cmd === "alerts_everywhere") return [{ plane: PLANE, alerts: [], stopped: null }];
      return null;
    },
    { shouldMockEvents: true },
  );
}

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
  forgetReadsOutsideChatsHere();
});
afterEach(() => {
  cleanup();
  clearMocks();
  forgetDismissals();
});

describe("the chats the window draws (#1037)", () => {
  it("are all read below the project's ChatsHere", async () => {
    core();
    render(<App />);

    // The chat's tab, its row and its task's row are drawn: each reads its chat's state.
    await screen.findByTestId("pane");
    const chats = await screen.findByRole("tree", { name: /chats/i });
    await within(chats).findAllByText(/claude 4/);

    expect(readsOutsideChatsHere()).toBe(0);
  });
});
