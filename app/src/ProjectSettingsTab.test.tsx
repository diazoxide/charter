import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { SettingsFile } from "./bindings";

/**
 * Reaching the Project settings tab from the window (charter-app#252): the project tab's own
 * menu opens it as a view tab, and asking again brings that tab forward rather than drawing a
 * second. What the tab shows is `ProjectSettings.test.tsx`'s.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

const FILE = (which: "shared" | "local"): SettingsFile => ({
  which,
  file: which === "shared" ? "charter.toml" : "charter.local.toml",
  exists: true,
  text: "",
  refusals: [],
  parsed: true,
  fields: [],
});

function core() {
  mockIPC((cmd) => {
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "plane_sidebar")
      return { root: PLANE, personas: [], persona: null, unfiled: [], workspaces: [] };
    if (cmd === "project_settings") return { shared: FILE("shared"), local: FILE("local") };
    return null;
  });
}

async function fromTheMenu() {
  const tab = await screen.findByRole("tab", { name: /plane/ });
  fireEvent.contextMenu(tab);
  await screen.findByRole("menu");
  await userEvent.click(screen.getByRole("menuitem", { name: "Project settings…" }));
}

describe("the Project settings tab, from the window", () => {
  it("opens from the project tab's menu as a view tab of its own, once", async () => {
    core();
    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );

    await fromTheMenu();
    expect(await screen.findByTestId("settings-shared")).toBeInTheDocument();
    expect(screen.getByTestId("settings-local")).toBeInTheDocument();
    const chats = () =>
      within(screen.getByRole("tablist", { name: "Tabs" }))
        .getAllByRole("tab")
        .filter((tab) => tab.textContent?.includes("Project settings"));
    expect(chats()).toHaveLength(1);

    await fromTheMenu();
    expect(chats()).toHaveLength(1);
  });
});
