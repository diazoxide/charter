import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "../App";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";

/**
 * **The quiet gears, and ⌘, at the focused level** (SE-23, #1173; V89g, V89i on #558).
 *
 * A gear on the project in front's tab and one on the focused workspace's tab, each the same
 * catalogue row its right-click menu and the palette run, and nowhere else: no gear on a tab
 * behind, and no settings button of its own in the title bar. The app menu's Settings… (`⌘,`,
 * `Ctrl+,` off a Mac, which the core says with an event) opens Settings at the focused
 * workspace's level, else the project's, else You. What each level shows is `ProjectLevel`'s and `WorkspaceLevel`'s.
 */

vi.mock("../SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const PLANE = "/home/dev/plane";

const FILE = (which: "shared" | "local") => ({
  which,
  file: which === "shared" ? "charter.toml" : "charter.local.toml",
  exists: which === "shared",
  text: "",
  refusals: [],
  parsed: true,
  fields: [],
});

const WORKSPACE = (workspace: string) => ({
  workspace,
  file: `workspaces/${workspace}/workspace.json`,
  exists: true,
  text: "{}\n",
  refusals: [],
  parsed: true,
  fields: [],
  live: false,
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

function core(plane: string | null) {
  mockIPC(
    (cmd, args) => {
      if (cmd === "plane_at_launch")
        return { plane, from: plane, why: plane === null ? "no plane here" : null };
      if (cmd === "opened_chats") return [];
      if (cmd === "reopened_views") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "plane_sidebar")
        return {
          root: plane,
          personas: [],
          persona: null,
          unfiled: [],
          workspaces: ["alpha", "beta"].map((name) => ({
            name,
            path: `${PLANE}/workspaces/${name}`,
            vision: "",
            todos: [],
            chats: [],
          })),
        };
      if (cmd === "project_settings") return { shared: FILE("shared"), local: FILE("local") };
      if (cmd === "workspace_settings") return WORKSPACE((args as { workspace: string }).workspace);
      return null;
    },
    { shouldMockEvents: true },
  );
}

const projects = () => screen.getByRole("tablist", { name: "Projects" });
const workspaces = () => screen.getByRole("tablist", { name: "Workspaces" });
const chatTabs = () => within(screen.getByRole("tablist", { name: "Tabs" })).queryAllByRole("tab");
const inFront = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .queryAllByRole("tab")
    .find((tab) => tab.getAttribute("aria-selected") === "true");

/** The level the Settings tab on screen is at, by its switcher. */
const level = () =>
  within(screen.getByRole("radiogroup", { name: /level/i }))
    .getAllByRole("radio")
    .find((radio) => radio.getAttribute("aria-checked") === "true")
    ?.textContent?.trim();

async function settled() {
  // The listeners register asynchronously; give them a turn.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

async function menuSettings() {
  await act(async () => {
    await emit("settings-asked");
  });
}

describe("⌘, opens Settings at the focused level", () => {
  it("opens the focused workspace's level when a workspace is focused", async () => {
    core(PLANE);
    render(<App />);
    await userEvent.click(await screen.findByRole("tab", { name: /alpha/ }));
    await settled();

    await menuSettings();

    await waitFor(() => expect(level()).toBe("Workspace"));
    expect(inFront()).toHaveTextContent("Workspace settings · alpha");
  });

  it("opens the project's level when no workspace is focused", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /alpha/ });
    // The plane root, first on the strip, is not a workspace.
    await userEvent.click(within(workspaces()).getAllByRole("tab")[0]);
    expect(within(workspaces()).getAllByRole("tab")[0]).toHaveAttribute("aria-selected", "true");
    await settled();

    await menuSettings();

    await waitFor(() => expect(level()).toBe("Project"));
  });

  it("opens You when no project is open", async () => {
    core(null);
    render(<App />);
    await waitFor(() =>
      expect(screen.queryByRole("heading", { name: /project/, level: 1 })).toBeInTheDocument(),
    );
    await settled();

    await menuSettings();

    await waitFor(() => expect(level()).toBe("You"));
  });

  it("brings the tab already open forward rather than opening a second", async () => {
    core(PLANE);
    render(<App />);
    await userEvent.click(await screen.findByRole("tab", { name: /alpha/ }));
    await settled();

    await menuSettings();
    await waitFor(() => expect(level()).toBe("Workspace"));
    await menuSettings();

    expect(
      chatTabs().filter((tab) => /Workspace settings/.test(tab.textContent ?? "")),
    ).toHaveLength(1);
  });
});

describe("Your settings…, in the palette", () => {
  it("opens You while a workspace is focused, where Settings… would open the workspace's", async () => {
    core(PLANE);
    render(<App />);
    await userEvent.click(await screen.findByRole("tab", { name: /alpha/ }));

    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("Your settings");
    await userEvent.keyboard("{Enter}");

    await waitFor(() => expect(level()).toBe("You"));
  });
});

describe("the project gear", () => {
  it("is on the project in front's tab, named, and runs the row its menu runs", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /alpha/ });

    const gears = within(projects()).getAllByRole("button", { name: "Project settings…" });
    expect(gears).toHaveLength(1);
    await userEvent.click(gears[0]);

    await waitFor(() => expect(level()).toBe("Project"));
    const opened = chatTabs().length;
    // The same row as the palette's: pressed again, it brings the same tab forward.
    await userEvent.click(gears[0]);
    expect(chatTabs()).toHaveLength(opened);
  });

  it("is one Tab away from the project in front's tab", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /alpha/ });

    within(projects()).getByRole("tab", { selected: true }).focus();
    await userEvent.tab();

    expect(within(projects()).getByRole("button", { name: "Project settings…" })).toHaveFocus();
  });
});

describe("the workspace gear", () => {
  it("is on the focused workspace's tab only, and opens Settings at its level", async () => {
    core(PLANE);
    render(<App />);
    await userEvent.click(await screen.findByRole("tab", { name: /alpha/ }));
    const gears = within(workspaces()).getAllByRole("button", { name: "Workspace settings…" });
    expect(gears).toHaveLength(1);
    // On alpha's own cell, beside its tab.
    expect(gears[0].parentElement).toContainElement(
      within(workspaces()).getByRole("tab", { name: /alpha/ }),
    );
    await userEvent.click(gears[0]);

    await waitFor(() => expect(level()).toBe("Workspace"));
    expect(inFront()).toHaveTextContent("Workspace settings · alpha");
  });

  it("has no gear on the plane root, which is not a workspace", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /alpha/ });

    await userEvent.click(within(workspaces()).getAllByRole("tab")[0]);

    expect(within(workspaces()).getAllByRole("tab")[0]).toHaveAttribute("aria-selected", "true");
    expect(
      within(workspaces()).queryByRole("button", { name: /Workspace settings/ }),
    ).not.toBeInTheDocument();
  });

  it("is one Tab away from the focused workspace's tab", async () => {
    core(PLANE);
    render(<App />);
    await userEvent.click(await screen.findByRole("tab", { name: /alpha/ }));

    within(workspaces()).getByRole("tab", { selected: true }).focus();
    await userEvent.tab();

    expect(within(workspaces()).getByRole("button", { name: "Workspace settings…" })).toHaveFocus();
  });
});

/**
 * **Quiet: drawn only under the pointer or the keyboard.** jsdom lays nothing out and computes
 * no cascade, so this holds the stylesheet's claim, as `StripMarks.test.tsx` does for the strips:
 * a gear is invisible by default, and every rule that shows it is a hover or a keyboard focus.
 */
describe("the gears' look", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8").replace(
    /\/\*[\s\S]*?\*\//g,
    "",
  );
  const rules = [...css.matchAll(/([^{}]*)\{([^{}]*)\}/g)].map((hit) => ({
    selectors: hit[1].split(",").map((one) => one.trim()),
    body: hit[2],
  }));

  it("hides a gear until its tab is under the pointer or the keyboard", () => {
    const base = rules.find((rule) => rule.selectors.includes(".project .gear"));
    expect(base?.body).toMatch(/opacity:\s*0;/);
    const shown = rules.filter(
      (rule) =>
        /opacity:\s*1;/.test(rule.body) && rule.selectors.some((one) => one.includes(".gear")),
    );
    expect(shown.length).toBeGreaterThan(0);
    for (const rule of shown)
      for (const one of rule.selectors) expect(one).toMatch(/:hover|:focus-visible/);
  });
});

/**
 * **The context-menu key still opens a tab's menu** (SE-23 review). The menu's trigger is the
 * tab itself and not the cell that holds it and its gear: `Menus.openFromTheKeyboard` opens only
 * when the trigger itself has the keyboard, so a trigger on the cell never opened from it.
 */
describe("a tab's menu, from the keyboard", () => {
  for (const key of [{ key: "F10", shiftKey: true }, { key: "ContextMenu" }]) {
    const said = key.shiftKey ? "Shift+F10" : "the menu key";

    it(`opens a workspace tab's menu on ${said}`, async () => {
      core(PLANE);
      render(<App />);
      const tab = await screen.findByRole("tab", { name: /alpha/ });
      await userEvent.click(tab);
      tab.focus();

      fireEvent.keyDown(tab, key);

      const menu = await screen.findByRole("menu");
      expect(within(menu).getByRole("menuitem", { name: "Workspace settings…" })).toBeVisible();
    });

    it(`opens the project tab's menu on ${said}`, async () => {
      core(PLANE);
      render(<App />);
      await screen.findByRole("tab", { name: /alpha/ });
      const tab = within(projects()).getByRole("tab", { selected: true });
      tab.focus();

      fireEvent.keyDown(tab, key);

      const menu = await screen.findByRole("menu");
      expect(within(menu).getByRole("menuitem", { name: "Project settings…" })).toBeVisible();
    });
  }
});
