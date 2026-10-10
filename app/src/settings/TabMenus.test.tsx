import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "../App";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import { stripNamed } from "../test-strips";

/**
 * **A tab's menu opens Settings at that tab's level** (#1213): right-click a project, then
 * Project settings…, and Settings opens at the Project level in THAT project, brought to the
 * front; right-click a workspace, then Workspace settings…, and it opens at that workspace's
 * level, on its strip. The palette's rows and the gears are the same catalogue rows
 * (D-SE23a), so each lands on the tab its menu opened rather than a second one.
 *
 * `SettingsWindow.test.tsx` has the project menu with one project open and nothing focused;
 * `SettingsGears.test.tsx` has the gears and the menu key. `app/e2e/specs/settings-menus.e2e.ts`
 * drives both menus from the keyboard in the built app.
 */

vi.mock("../SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const ONE = "/home/dev/one";
const TWO = "/home/dev/two";

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

/** A core holding `planes`, the first in front, each with the workspaces alpha and beta, and
 *  `pinned` pinned in each. */
function core(planes: string[], pinned: string[] = []) {
  mockIPC(
    (cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      const plane = (given.plane as string | undefined) ?? planes[0];
      if (cmd === "plane_at_launch") return { plane: null, from: null, why: null };
      if (cmd === "planes_to_restore") return { windows: [{ planes, active: 0 }], dropped: [] };
      if (cmd === "open_plane") return { plane: given.path, ask: null };
      if (cmd === "recent_planes") return { planes: [], dropped: [], forgetful: null };
      if (cmd === "opened_chats") return [];
      if (cmd === "reopened_views") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "extensions_on") return [];
      if (cmd === "plane_pins") return { workspaces: pinned, missing: [] };
      if (cmd === "plane_sidebar")
        return {
          root: plane,
          personas: [],
          persona: null,
          unfiled: [],
          workspaces: ["alpha", "beta"].map((name) => ({
            name,
            path: `${plane}/workspaces/${name}`,
            vision: "",
            todos: [],
            chats: [],
          })),
        };
      if (cmd === "project_settings") return { shared: FILE("shared"), local: FILE("local") };
      if (cmd === "workspace_settings") return WORKSPACE(given.workspace as string);
      return null;
    },
    { shouldMockEvents: true },
  );
}

const projects = () => stripNamed("Projects");
const workspaces = () => stripNamed("Workspaces");
const tabs = () => within(stripNamed("Tabs")).queryAllByRole("tab");
const inFront = () => tabs().find((tab) => tab.getAttribute("aria-selected") === "true");

/** One project's tab on the strip, by the name it carries. */
function projectTab(name: string): HTMLElement {
  const tab = within(projects())
    .getAllByRole("tab")
    .find((one) => one.querySelector(".project-name")?.textContent === name);
  if (!tab) throw new Error(`no project tab called ${name}`);
  return tab;
}

/** The level the Settings tab on screen is at, by its switcher. */
const level = () =>
  within(screen.getByRole("radiogroup", { name: /level/i }))
    .getAllByRole("radio")
    .find((radio) => radio.getAttribute("aria-checked") === "true")
    ?.textContent?.trim();

/** Right-clicks `on` and presses the menu's row called `row`. */
async function fromItsMenu(on: HTMLElement, row: string) {
  fireEvent.contextMenu(on);
  const menu = await screen.findByRole("menu");
  await userEvent.click(within(menu).getByRole("menuitem", { name: row }));
}

async function palette(typed: string) {
  await userEvent.keyboard("{F2}");
  await screen.findByRole("dialog", { name: "Command palette" });
  await userEvent.keyboard(typed);
  await userEvent.keyboard("{Enter}");
}

/** Puts a tab other than the one in front there — Settings at You, from the palette — and
 *  answers it. */
async function anotherInFront(): Promise<HTMLElement> {
  const was = inFront();
  await palette("Your settings…");
  await waitFor(() => expect(level()).toBe("You"));
  const now = inFront();
  if (now === undefined || now === was) throw new Error("no other tab came to the front");
  return now;
}

describe("a project tab's menu", () => {
  it("opens Settings at the Project level in the project right-clicked, bringing it to the front", async () => {
    core([ONE, TWO]);
    render(<App />);
    await screen.findByRole("tab", { name: /alpha/ });
    expect(projectTab("one")).toHaveAttribute("aria-selected", "true");

    await fromItsMenu(projectTab("two"), "Project settings…");

    await waitFor(() => expect(projectTab("two")).toHaveAttribute("aria-selected", "true"));
    await waitFor(() => expect(level()).toBe("Project"));
    expect(inFront()).toHaveTextContent("Settings");
  });

  it("opens the Project level while a workspace is focused, not the workspace's", async () => {
    core([ONE]);
    render(<App />);
    await screen.findByRole("tab", { name: /alpha/ });
    await userEvent.click(within(workspaces()).getByRole("tab", { name: /alpha/ }));

    await fromItsMenu(projectTab("one"), "Project settings…");

    await waitFor(() => expect(level()).toBe("Project"));
  });

  it("lands on the tab the palette's Project settings… and the gear open, not a second", async () => {
    core([ONE]);
    render(<App />);
    await screen.findByRole("tab", { name: /alpha/ });

    await fromItsMenu(projectTab("one"), "Project settings…");
    await waitFor(() => expect(level()).toBe("Project"));
    const project = inFront();
    // Another tab in front, so a row that did nothing would leave it there.
    const other = await anotherInFront();
    const opened = tabs().length;

    await palette("Project settings…");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(inFront()).toBe(project));
    expect(level()).toBe("Project");

    await userEvent.click(other);
    expect(inFront()).toBe(other);
    await userEvent.click(within(projects()).getByRole("button", { name: "Project settings…" }));

    await waitFor(() => expect(inFront()).toBe(project));
    expect(level()).toBe("Project");
    expect(tabs()).toHaveLength(opened);
  });
});

describe("a workspace tab's menu", () => {
  it("opens Settings at the right-clicked workspace's level, on its strip, from another's", async () => {
    // Pinned, so beta has a tab on the strip while alpha is the one focused (ADR 0054).
    core([ONE], ["beta"]);
    render(<App />);
    await screen.findByRole("tab", { name: /alpha/ });
    const tab = (name: RegExp) => within(workspaces()).getByRole("tab", { name });
    await userEvent.click(tab(/alpha/));
    expect(tab(/alpha/)).toHaveAttribute("aria-selected", "true");

    await fromItsMenu(tab(/beta/), "Workspace settings…");

    await waitFor(() => expect(level()).toBe("Workspace"));
    expect(inFront()).toHaveTextContent("Settings · beta");
    // Beta's strip is the one in front now, and the only one.
    expect(within(workspaces()).getByRole("tab", { selected: true })).toBe(tab(/beta/));
  });

  it("lands on the tab the palette's Workspace settings… and the gear open, not a second", async () => {
    core([ONE]);
    render(<App />);
    await screen.findByRole("tab", { name: /alpha/ });
    const alpha = within(workspaces()).getByRole("tab", { name: /alpha/ });
    await userEvent.click(alpha);

    await fromItsMenu(alpha, "Workspace settings…");
    await waitFor(() => expect(level()).toBe("Workspace"));
    const workspace = inFront();
    // Another tab in front, so a row that did nothing would leave it there.
    const other = await anotherInFront();
    const opened = tabs().length;

    // One row per workspace, told apart by the note that names it.
    await userEvent.keyboard("{F2}");
    const dialog = await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("Workspace settings");
    const row = within(dialog)
      .getAllByRole("option")
      .find((one) => one.textContent?.includes("alpha:"));
    if (row === undefined) throw new Error("no Workspace settings… row for alpha");
    await userEvent.click(row);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(inFront()).toBe(workspace));

    await userEvent.click(other);
    expect(inFront()).toBe(other);
    await userEvent.click(
      within(workspaces()).getByRole("button", { name: "Workspace settings…" }),
    );

    await waitFor(() => expect(inFront()).toBe(workspace));
    expect(inFront()).toHaveTextContent("Settings · alpha");
    expect(tabs()).toHaveLength(opened);
  });
});
