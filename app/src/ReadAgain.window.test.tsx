import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { forgetThisLaunch } from "./regions";

/**
 * **A refused read has its way out with the explorer hidden** (#1244). The bottom region says
 * what purlis could not read and, by ADR 0038, holds nothing to press; its Read again was the
 * explorer's Notice. With the explorer put away, the refusal line's menu and the palette both
 * have the same Read again, and it asks the workspace's reads again.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;
const REFUSED = "purlis could not read the forge cache: it is not JSON";

/** One workspace whose forge cache is refused until `fixed` is set. */
function core() {
  const asked: string[] = [];
  const state = { fixed: false };
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push(cmd);
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: ["steward"],
        persona: "steward",
        unfiled: [],
        workspaces: [{ name: "alpha", path: ALPHA, vision: "", todos: [], chats: [] }],
      };
    if (cmd === "workspace_panels")
      return {
        workspace: a.workspace,
        repos: [],
        paths: {},
        absent: [],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["steward"],
        persona: "steward",
        sessions: [],
        contributed: [],
      };
    if (cmd === "workspace_repos")
      return { workspace: a.workspace, repos: [], cache_refused: state.fixed ? null : REFUSED };
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "alerts_everywhere") return [{ plane: PLANE, alerts: [], stopped: null }];
    return null;
  });
  return { asked, state };
}

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
});

/** Puts the explorer away, as the navigation region's toggle does (#1673: the explorer is one
 *  of that region's views). */
async function hideTheExplorer() {
  await screen.findByRole("tabpanel", { name: "Chats" });
  await userEvent.click(screen.getByRole("button", { name: "Navigation", pressed: true }));
  await waitFor(() => expect(screen.queryByRole("tabpanel")).toBeNull());
}

/** The bottom region's line saying the refusal. */
const refusal = () =>
  within(screen.getByTestId("bottom-bar")).findByText(REFUSED, { exact: false });

describe("Read again with the explorer hidden (#1244)", () => {
  it("is on the refusal line's menu, and reads the workspace again", async () => {
    const { asked, state } = core();
    render(<App />);
    await hideTheExplorer();

    const before = asked.filter((cmd) => cmd === "workspace_repos").length;
    state.fixed = true;
    fireEvent.contextMenu(await refusal());
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Read the workspace again" }),
    );

    await waitFor(() =>
      expect(asked.filter((cmd) => cmd === "workspace_repos").length).toBeGreaterThan(before),
    );
    await waitFor(() =>
      expect(within(screen.getByTestId("bottom-bar")).queryByText(REFUSED)).toBeNull(),
    );
  });

  it("is a palette row while the refusal stands", async () => {
    const { asked } = core();
    render(<App />);
    await hideTheExplorer();
    await refusal();

    const before = asked.filter((cmd) => cmd === "workspace_repos").length;
    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("Read the workspace again");
    await userEvent.keyboard("{Enter}");

    await waitFor(() =>
      expect(asked.filter((cmd) => cmd === "workspace_repos").length).toBeGreaterThan(before),
    );
  });
});
