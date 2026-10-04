import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "../App";
import type { ViewTab } from "../bindings";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";

/**
 * **Reaching Settings from the window** (SE-16, #1166): the palette, the app menu's Settings…
 * (`⌘,`, which the core says with an event), and with no project open. What the tab shows is
 * `SettingsTab.test.tsx`'s. "Preferences" is gone from all of them.
 */

vi.mock("../SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const PLANE = "/home/dev/plane";

/** An empty settings file, as `project_settings` answers it. */
const FILE = (which: "shared" | "local") => ({
  which,
  file: which === "shared" ? "charter.toml" : "charter.local.toml",
  exists: which === "shared",
  text: "",
  refusals: [],
  parsed: true,
  fields: [],
});

beforeEach(() => {
  forgetThisLaunch();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: "/home/op/.config/charter/layout.json", found: false, document: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

function core(plane: string | null, views: ViewTab[] = []) {
  mockIPC(
    (cmd) => {
      if (cmd === "plane_at_launch")
        return { plane, from: plane, why: plane === null ? "no plane here" : null };
      if (cmd === "opened_chats") return [];
      if (cmd === "reopened_views") return views;
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "plane_sidebar")
        return { root: plane, personas: [], persona: null, unfiled: [], workspaces: [] };
      if (cmd === "project_settings") return { shared: FILE("shared"), local: FILE("local") };
      return null;
    },
    { shouldMockEvents: true },
  );
}

async function palette(typed: string) {
  await userEvent.keyboard("{F2}");
  const dialog = await screen.findByRole("dialog", { name: "Command palette" });
  await userEvent.keyboard(typed);
  return dialog;
}

const settingsTabs = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .queryAllByRole("tab")
    .filter((tab) => /^Settings/.test(tab.textContent ?? ""));

/** The Settings tab's own nav, which says it is on screen. */
const groups = () => screen.queryByRole("navigation", { name: "Groups" });

describe("Settings, from the window", () => {
  it("opens from the palette as a view tab of its own, once", async () => {
    core(PLANE);
    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );
    await screen.findByRole("tab", { name: /plane/ });

    await palette("Settings…");
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(groups()).toBeInTheDocument());
    expect(settingsTabs()).toHaveLength(1);

    await palette("Settings…");
    await userEvent.keyboard("{Enter}");
    expect(settingsTabs()).toHaveLength(1);
  });

  it("moves its own tab to Project, and to a level whose tab is open brings that tab forward", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    await palette("Settings…");
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(groups()).toBeInTheDocument());

    await userEvent.click(screen.getByRole("radio", { name: "Project" }));

    expect(
      await within(screen.getByRole("navigation", { name: "Groups" })).findByRole("button", {
        name: "General",
      }),
    ).toBeInTheDocument();
    expect(settingsTabs()).toHaveLength(1);

    // Settings… opens at You, which no tab shows now; its switcher then finds Project's tab.
    await palette("Settings…");
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(screen.getByRole("radio", { name: "You" })).toBeChecked());
    expect(settingsTabs()).toHaveLength(2);
    await userEvent.click(screen.getByRole("radio", { name: "Project" }));

    expect(
      await within(screen.getByRole("navigation", { name: "Groups" })).findByRole("button", {
        name: "General",
      }),
    ).toBeInTheDocument();
    expect(settingsTabs()).toHaveLength(2);
  });

  it("opens from the app menu's Settings… (⌘,), which the core says with an event", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    // The listeners register asynchronously; give them a turn.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    await act(async () => {
      await emit("settings-asked");
    });

    await waitFor(() => expect(groups()).toBeInTheDocument());
    expect(settingsTabs()).toHaveLength(1);
  });

  it("is drawn where the opener is when no project is open, and Done goes back to it", async () => {
    core(null);
    render(<App />);
    const opener = () => screen.queryByRole("heading", { name: /project/, level: 1 });
    await waitFor(() => expect(opener()).toBeInTheDocument());

    await palette("Settings…");
    await userEvent.keyboard("{Enter}");

    await waitFor(() => expect(groups()).toBeInTheDocument());
    expect(opener()).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(groups()).not.toBeInTheDocument();
    expect(opener()).toBeInTheDocument();
  });

  it("offers no Preferences in the palette any more", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });

    const dialog = await palette("Preferences");

    expect(within(dialog).queryByRole("option", { name: /Preferences/ })).not.toBeInTheDocument();
  });

  it("puts a Preferences tab an older launch left open back as Settings", async () => {
    core(PLANE, [
      {
        from: null,
        view: "preferences",
        key: "",
        title: "Preferences",
        workspace: null,
        at: 0,
        active: true,
        pinned: false,
      },
    ]);
    render(<App />);

    await waitFor(() => expect(groups()).toBeInTheDocument());
    expect(settingsTabs()).toHaveLength(1);
  });
});
