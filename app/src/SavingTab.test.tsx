import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { PlaneSaving } from "./bindings";

/**
 * The save indicator and the Saving tab, from the window (charter-app#294): the title bar says
 * where the project in front's unsaved work sits, opens that project's Saving tab, and saves.
 * What the tab shows is `SavingView.test.tsx`'s.
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

function standing(over: Partial<PlaneSaving> = {}): PlaneSaving {
  return {
    stage: "changed",
    changed: ["a.md"],
    ahead: 0,
    pr: null,
    blocked: null,
    branch: "main",
    pushes: true,
    behind: 0,
    pushFailed: null,
    live: [],
    conflicts: [],
    notice: null,
    mode: "push",
    modeFrom: "charter.toml",
    journal: [],
    ...over,
  };
}

/** The core, whose plane has one file unsaved until `save_plane` is asked. */
function core(): { saves: unknown[] } {
  const saves: unknown[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "plane_sidebar")
      return { root: PLANE, personas: [], persona: null, unfiled: [], workspaces: [] };
    if (cmd === "plane_saving")
      return saves.length === 0 ? standing() : standing({ stage: "saved", changed: [] });
    if (cmd === "save_plane") {
      saves.push(args);
      return ["✓ Committed"];
    }
    return null;
  });
  return { saves };
}

describe("the save indicator, in the window", () => {
  it("says what is unsaved in the project in front, and opens its Saving tab once", async () => {
    core();
    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );

    const bar = screen.getByTestId("title-bar");
    await userEvent.click(await within(bar).findByRole("button", { name: "Saving: 1 changed" }));
    expect(await screen.findByTestId("saving-view")).toBeInTheDocument();
    const tabs = () =>
      within(screen.getByRole("tablist", { name: "Tabs" }))
        .getAllByRole("tab")
        .filter((tab) => tab.textContent?.includes("Saving"));
    expect(tabs()).toHaveLength(1);

    await userEvent.click(within(bar).getByRole("button", { name: "Saving: 1 changed" }));
    expect(tabs()).toHaveLength(1);
  });

  it("saves the project in front from the bar, and says so once it is saved", async () => {
    const { saves } = core();
    render(<App />);

    const bar = screen.getByTestId("title-bar");
    await userEvent.click(await within(bar).findByRole("button", { name: "Save the project" }));

    await waitFor(() => expect(saves).toEqual([{ plane: PLANE, message: null }]));
    expect(await within(bar).findByRole("button", { name: "Saving: Saved" })).toBeTruthy();
  });
});

describe("the project strip (charter-app#302)", () => {
  it("marks a project with unsaved work, and not one with nothing left to save", async () => {
    core();
    render(<App />);
    const strip = await screen.findByRole("tablist", { name: "Projects" });

    expect(await within(strip).findByRole("img", { name: /^unsaved work in / })).toBeTruthy();

    // Saved from the bar: the dot goes with the work.
    await userEvent.click(
      await within(screen.getByTestId("title-bar")).findByRole("button", {
        name: "Save the project",
      }),
    );
    await waitFor(() =>
      expect(within(strip).queryByRole("img", { name: /^unsaved work in / })).toBeNull(),
    );
  });
});

describe("a blocked save's ways out, in the window (charter-app#295)", () => {
  it("opens a plain terminal in the plane from the Saving tab", async () => {
    const opened: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "plane_sidebar")
        return { root: PLANE, personas: [], persona: null, unfiled: [], workspaces: [] };
      if (cmd === "plane_saving")
        return standing({ stage: "blocked", blocked: "conflict", conflicts: ["notes.md"] });
      if (cmd === "open_session") {
        opened.push(args);
        return 7;
      }
      return null;
    });
    render(<App />);
    const bar = screen.getByTestId("title-bar");
    await userEvent.click(
      await within(bar).findByRole("button", { name: "Saving: Blocked: conflict" }),
    );

    await userEvent.click(await screen.findByRole("button", { name: "Open terminal here" }));

    await waitFor(() =>
      expect(opened).toEqual([
        expect.objectContaining({
          plane: PLANE,
          program: null,
          args: [],
          cwd: PLANE,
          name: "terminal",
        }),
      ]),
    );
  });
});
