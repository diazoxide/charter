import { StrictMode } from "react";
import { afterEach, describe, expect, it } from "vitest";
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
import { vi } from "vitest";
import App from "./App";
import { forgetRepoClones } from "./repoClones";
import { SETTINGS_LINK, type SettingsLinkAsk } from "./settings/links";
import { stripNamed } from "./test-strips";

/**
 * **A refusal that names a setting links to it** (#1201, SE-22): the window's line under the
 * strip says the core's words and, beside them, a link into the Settings group that puts it
 * right. Here, a repo that did not clone into a new workspace: "Retry from the workspace's
 * settings." links to that workspace's Repos.
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
  forgetRepoClones();
});

const PLANE = "/home/dev/plane";

/** One workspace, alpha, and a core whose clone of `api` fails. */
function core() {
  const made: string[] = [];
  mockIPC((cmd, args) => {
    const got = (args ?? {}) as Record<string, unknown>;
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "plane_pins") return { project: false, workspaces: [], missing: [] };
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: ["steward"],
        persona: "steward",
        unfiled: [],
        workspaces: ["alpha", ...made].map((name) => ({
          name,
          path: `${PLANE}/workspaces/${name}`,
          vision: "",
          todos: [],
          chats: [],
        })),
      };
    if (cmd === "reachable_repos")
      return { repos: [{ name: "api", path: "acme/api", description: "" }], trouble: [] };
    if (cmd === "take_repos") return null;
    if (cmd === "clone_repo") throw "api: clone failed — no access.";
    // The link opens Settings at the workspace's Repos, which reads the workspace: refused here,
    // so the level says so rather than drawing a read this core never made.
    if (cmd === "workspace_settings") throw "not read in this test";
    if (cmd === "workspace_create") {
      made.push(String(got.name));
      return [`✓ Workspace '${String(got.name)}' ready (LOCAL) → workspaces/${String(got.name)}/`];
    }
    return null;
  });
}

describe("a refusal's way to its setting (#1201)", () => {
  it("links a clone that failed after a new workspace to that workspace's Repos", async () => {
    core();
    const links: SettingsLinkAsk[] = [];
    const heard = (event: Event) => links.push((event as CustomEvent<SettingsLinkAsk>).detail);
    window.addEventListener(SETTINGS_LINK, heard);
    try {
      render(<App />);
      const tab = await waitFor(() => {
        const found = within(stripNamed("Workspaces"))
          .getAllByRole("tab")
          .find((one) => one.querySelector(".workspace-name")?.textContent === "alpha");
        if (found === undefined) throw new Error("alpha is not on the strip yet");
        return found;
      });
      fireEvent.contextMenu(tab);
      await userEvent.click(await screen.findByRole("menuitem", { name: "New workspace…" }));
      const dialog = await screen.findByRole("dialog");
      await userEvent.type(within(dialog).getByLabelText("Name"), "gamma");
      await userEvent.click(await within(dialog).findByRole("checkbox", { name: "api" }));
      await userEvent.click(within(dialog).getByRole("button", { name: "Create workspace" }));

      const words = await screen.findByText(/Could not clone api into gamma/);
      expect(words).toHaveTextContent("Retry from the workspace's settings.");
      const said = words.closest<HTMLElement>('[role="alert"]');
      if (said === null) throw new Error("the clone's failure is not said as an alert");
      await userEvent.click(
        within(said).getByRole("button", { name: "Open gamma's repo settings" }),
      );

      expect(links).toEqual([
        { plane: PLANE, link: { group: "workspace.repos", workspace: "gamma" } },
      ]);
    } finally {
      window.removeEventListener(SETTINGS_LINK, heard);
    }
  });
});
