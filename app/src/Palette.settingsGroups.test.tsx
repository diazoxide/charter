import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { DoctorRow } from "./bindings";
import { forgetThisLaunch } from "./regions";
import { GLOBAL } from "./windowprefs";
import { forgetGroups } from "./settings/links";

/**
 * **The palette's rows per Settings group** (#1201): "Project settings: Saving" and the rest
 * open Settings at that group's level with that group shown and the keyboard on it, as a link
 * from a doctor row does; You's work with no project open, where Settings is drawn in place of
 * the opener. The window and its core as `settings/SettingsFocus.test.tsx` has them.
 */

vi.mock("./SessionPane", () => ({
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

const ROW: DoctorRow = {
  name: "charter.toml",
  status: "warn",
  detail: "plane.mod in charter.toml is not read",
  hint: "Fix or remove it: until then purlis reads the next file down, or the default.",
  checked: true,
  settings: "project.saving",
  fix: null,
};

/** The layout file, holding a window text size charter cannot use when `badText`. */
function layout(badText = false) {
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: badText
      ? {
          path: "/home/op/.config/charter/layout.json",
          found: true,
          document: { version: 1, regions: [], text: { window: 2.5 } },
          trouble: null,
        }
      : { path: "/home/op/.config/charter/layout.json", found: false, document: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
}

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
          workspaces: ["alpha"].map((name) => ({
            name,
            path: `${PLANE}/workspaces/${name}`,
            vision: "",
            todos: [],
            chats: [],
          })),
        };
      if (cmd === "project_settings") return { shared: FILE("shared"), local: FILE("local") };
      if (cmd === "workspace_settings") return WORKSPACE((args as { workspace: string }).workspace);
      if (cmd === "plane_doctor") return { rows: [ROW], full: true, app_rows: [], path: null };
      if (cmd === "alerts_everywhere") return [];
      if (cmd === "start_options")
        return {
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
          refused: [["bad", "has kind nope"]],
          personas: [],
          persona: null,
          persona_profiles: {},
          ignore_fix: null,
          ignore_fix_id: null,
          declares_none: true,
        };
      return null;
    },
    { shouldMockEvents: true },
  );
}

beforeEach(() => {
  forgetThisLaunch();
  layout();
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

const nav = () => screen.getByRole("navigation", { name: "Groups" });
const level = (name: string) => screen.getByRole("radio", { name });

/** The keyboard is on the group nav's current group, named `group` when given. */
async function onTheCurrentGroup(group?: string) {
  await waitFor(() => {
    const current = within(nav())
      .getAllByRole("button")
      .find((one) => one.getAttribute("aria-current") === "true");
    expect(current).toBeDefined();
    if (group !== undefined) expect(current).toHaveAccessibleName(group);
    expect(document.activeElement).toBe(current);
  });
}

async function settled() {
  // The listeners register asynchronously; give them a turn.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

async function palette(typed: string) {
  await userEvent.keyboard("{F2}");
  await screen.findByRole("dialog", { name: "Command palette" });
  await userEvent.keyboard(typed);
  await userEvent.keyboard("{Enter}");
}

describe("the palette's row for one Settings group", () => {
  beforeEach(() => forgetGroups());

  it("opens the project's Settings at that group", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });

    await palette("Project settings: Forges");

    await waitFor(() => expect(level("Project")).toBeChecked());
    await onTheCurrentGroup("Forges");
  });

  it("opens the focused workspace's Settings at that group", async () => {
    core(PLANE);
    render(<App />);
    await userEvent.click(await screen.findByRole("tab", { name: /alpha/ }));

    await palette("Workspace settings: Repos");

    await waitFor(() => expect(level("Workspace")).toBeChecked());
    await onTheCurrentGroup("Repos");
  });

  it("opens Your settings at that group with a project in front", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });

    await palette("Your settings: Editor");

    await waitFor(() => expect(level("You")).toBeChecked());
    await onTheCurrentGroup("Editor");
  });

  it("opens Your settings at that group with no project open", async () => {
    core(null);
    render(<App />);
    await waitFor(() =>
      expect(screen.queryByRole("heading", { name: /project/, level: 1 })).toBeInTheDocument(),
    );
    await settled();

    await palette("Your settings: Chats list");

    await waitFor(() => expect(level("You")).toBeChecked());
    await onTheCurrentGroup("Chats list");
  });
});
