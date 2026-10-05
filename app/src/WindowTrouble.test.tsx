import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

/**
 * **A window action's refusal is a Notice with a way out, and it does not outstay its cause**
 * (NO-4, V91b). It used to be one line that stayed until something else replaced it: a picker
 * the core could not read said so, and still said so after the next New tab had worked.
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

const REFUSED = "charter could not read the profiles: charter.local.toml is not valid TOML";

/** The core, refusing the picker's options the first `refusals` times it is asked. */
function core(refusals: number) {
  let asked = 0;
  mockIPC((cmd) => {
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "opened_chats") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "start_options") {
      if (asked++ < refusals) throw REFUSED;
      return START_OPTIONS;
    }
    return null;
  });
}

const trouble = () => screen.queryByText(REFUSED)?.closest("[data-cause]") ?? null;

describe("a window action's refusal", () => {
  it("is a Notice the operator can dismiss", async () => {
    core(1);
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));

    await waitFor(() => expect(trouble()).not.toBeNull());
    await userEvent.click(
      within(trouble() as HTMLElement).getByRole("button", { name: "Dismiss" }),
    );
    expect(trouble()).toBeNull();
  });

  it("clears itself when the same action next succeeds", async () => {
    core(1);
    render(<App />);
    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
    await waitFor(() => expect(trouble()).not.toBeNull());

    await userEvent.click(screen.getByRole("button", { name: "New tab" }));

    await screen.findByRole("dialog", { name: "Start a chat" });
    expect(trouble()).toBeNull();
  });
});
