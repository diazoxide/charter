import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { WorkspaceSettings } from "./ProjectSettings";
import { ViewPane } from "./Views";
import { workspaceSettingsTitle, workspaceSettingsView } from "./tabs";
import type {
  HarnessPlugin,
  HarnessPlugins,
  ProjectExtension,
  WorkspaceSettings as Settings,
  WorkspaceSettingsSaved,
} from "./bindings";

/**
 * The Workspace settings tab (charter-app#280): a workspace's `settings` in its `workspace.json`,
 * the layer between the project's Shared and Local files. What a save is refused for, and the
 * order the layers are read in, are the core's (`charter_core::settings::workspace`,
 * `extension::project::resolve`); here the core is the mock, and what is held is that the tab
 * asks for the right workspace, sends the right edits, and draws what it is told.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

const MANIFEST =
  '{\n  "name": "alpha",\n  "settings": {"extensions": {"stats": {"enabled": false}}}\n}\n';

const ALPHA: Settings = {
  workspace: "alpha",
  file: "workspaces/alpha/workspace.json",
  exists: true,
  text: MANIFEST,
  refusals: [],
  parsed: true,
  fields: [
    {
      path: [{ key: "extensions" }, { key: "stats" }, { key: "enabled" }],
      value: { kind: "bool", value: false },
    },
  ],
  live: true,
};

const EXTENSIONS: ProjectExtension[] = [
  {
    id: "stats",
    name: "Persona statistics",
    state: "off",
    source: "workspace",
    settings: [],
    ignored: [
      {
        file: "workspaces/alpha/workspace.json",
        why: "workspaces/alpha/workspace.json sets extensions.stats.settings.nope, which stats does not declare — charter hands it nothing",
      },
      {
        file: "charter.local.toml",
        why: "a Local sentence that is not this section's",
      },
    ],
  },
  { id: "solarized", name: "Solarized", state: "on", source: "local", settings: [], ignored: [] },
];

const OWN_WHY =
  "charter-app@inline is always on: it is charter's own plugin, and it carries charter's hooks and the Bash guard";

/** What the core says is in force for each harness in this workspace (charter-app#282). */
const HARNESSES: HarnessPlugins[] = [
  {
    harness: "claude",
    title: "Claude Code",
    unsupported: null,
    record: "/home/dev/.claude/plugins/installed_plugins.json",
    trouble: null,
    plugins: [
      {
        id: "charter-app@inline",
        name: "charter-app",
        origin: "",
        state: "on",
        source: "default",
        installed: false,
        pinned: OWN_WHY,
        ignored: [
          {
            file: "workspaces/alpha/workspace.json",
            why: `workspaces/alpha/workspace.json sets settings.harness_plugins.claude."charter-app@inline" to false, and ${OWN_WHY}`,
          },
          { file: "charter.toml", why: "a Shared sentence that is not this section's" },
        ],
      },
      plugin("figma@official", "off", "workspace"),
      plugin("humanizer@h", "on", "local"),
      plugin("serena@official", "on", "shared"),
      plugin("stripe@official", "not-set", "default"),
    ],
  },
  {
    harness: "opencode",
    title: "opencode",
    record: "/home/dev/.config/opencode",
    unsupported:
      "plugins for opencode are not supported yet — charter does not start opencode chats yet",
    trouble: null,
    plugins: [],
  },
  {
    harness: "codex",
    title: "Codex",
    record: "/home/dev/.codex/config.toml",
    unsupported: "plugins for Codex are not supported yet — Codex 0.147.0 ignores -c",
    trouble: null,
    plugins: [],
  },
];

function plugin(id: string, state: string, source: string): HarnessPlugin {
  return {
    id,
    name: id.split("@")[0],
    origin: "official, user",
    state,
    source,
    installed: true,
    pinned: null,
    ignored: [],
  };
}

function core(
  settings: Settings = ALPHA,
  saved?: (sent: Record<string, unknown>) => WorkspaceSettingsSaved,
) {
  const sent: Record<string, unknown>[] = [];
  const asked: [string, Record<string, unknown>][] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push([cmd, given]);
    if (cmd === "workspace_settings") return settings;
    if (cmd === "project_extensions") return EXTENSIONS;
    if (cmd === "project_harness_plugins") return HARNESSES;
    if (cmd === "extensions_on") return [];
    if (cmd === "save_workspace_settings") {
      sent.push(given);
      return saved?.(given) ?? { kind: "saved", settings };
    }
    return undefined;
  });
  return {
    sent,
    asked: (cmd: string) => asked.filter(([one]) => one === cmd).map(([, given]) => given),
  };
}

async function drawn() {
  render(<WorkspaceSettings plane={PLANE} workspace="alpha" />);
  return screen.findByTestId("settings-workspace");
}

describe("the Workspace settings tab", () => {
  it("says which file it is, who sees it, and where it sits between the project's two", async () => {
    core();
    const section = await drawn();

    expect(section).toHaveTextContent("workspaces/alpha/workspace.json");
    expect(section).toHaveTextContent("Committed with this LIVE workspace; your team sees this.");
    expect(screen.getByText(/charter\.toml, then this workspace, then charter\.local\.toml/));
    // A form, and no raw view: the manifest holds more than settings.
    expect(within(section).queryByRole("radio", { name: "Raw TOML" })).toBeNull();
  });

  it("says a workspace that is not LIVE keeps its settings on this machine", async () => {
    core({ ...ALPHA, live: false });
    const section = await drawn();

    expect(section).toHaveTextContent(
      "This workspace is not LIVE, so its workspace.json stays on this machine.",
    );
  });

  it("asks what is in force in this workspace, and says which layer decided it", async () => {
    const { asked } = core();
    const section = await drawn();

    const group = within(section).getByRole("group", { name: "Extensions" });
    await waitFor(() =>
      expect(group).toHaveTextContent("Persona statistics: off — turned off in this workspace"),
    );
    expect(group).toHaveTextContent("Solarized: on — enabled in charter.local.toml");
    expect(within(section).getByLabelText("Persona statistics: enabled")).toHaveValue("off");
    expect(within(section).getByLabelText("Solarized: enabled")).toHaveValue("");
    // Only this section's sentences.
    expect(group).toHaveTextContent("which stats does not declare");
    expect(group).not.toHaveTextContent("a Local sentence");
    expect(asked("project_extensions")).toEqual([{ plane: PLANE, workspace: "alpha" }]);
    expect(asked("workspace_settings")).toEqual([{ plane: PLANE, workspace: "alpha" }]);
  });

  it("saves a toggle as that one key, against the manifest it was read as, and asks again", async () => {
    const { sent, asked } = core();
    const section = await drawn();
    const user = userEvent.setup();

    await user.selectOptions(within(section).getByLabelText("Persona statistics: enabled"), "");
    await user.click(within(section).getByRole("button", { name: "Save workspace.json" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual({
      plane: PLANE,
      workspace: "alpha",
      base: MANIFEST,
      edits: [{ path: [{ key: "extensions" }, { key: "stats" }, { key: "enabled" }], value: null }],
    });
    await waitFor(() => expect(asked("project_extensions")).toHaveLength(2));
  });

  it("sends a manifest that is not there yet as new", async () => {
    const { sent } = core({ ...ALPHA, exists: false, text: "", fields: [] });
    const section = await drawn();
    const user = userEvent.setup();

    expect(section).toHaveTextContent("Not created yet: the first save creates it.");
    await user.selectOptions(within(section).getByLabelText("Solarized: enabled"), "off");
    await user.click(within(section).getByRole("button", { name: "Save workspace.json" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].base).toBeNull();
  });

  it("draws a refusal in the core's own words", async () => {
    core(ALPHA, () => ({
      kind: "refused",
      reasons: ["workspaces/alpha/workspace.json changed on disk since this tab read it"],
    }));
    const section = await drawn();
    const user = userEvent.setup();

    await user.selectOptions(within(section).getByLabelText("Solarized: enabled"), "on");
    await user.click(within(section).getByRole("button", { name: "Save workspace.json" }));

    expect(await within(section).findByRole("alert")).toHaveTextContent(
      "workspaces/alpha/workspace.json changed on disk since this tab read it",
    );
  });

  it("says what charter does not take from the settings as they stand", async () => {
    core({
      ...ALPHA,
      refusals: ["settings.colour in workspaces/alpha/workspace.json is not read"],
    });
    const section = await drawn();

    expect(section).toHaveTextContent(
      "settings.colour in workspaces/alpha/workspace.json is not read",
    );
  });
});

describe("the Harness plugins groups in a workspace (charter-app#282)", () => {
  it("draws one group per harness, each plugin saying which layer decided it", async () => {
    const { asked } = core();
    const section = await drawn();

    const claude = within(section).getByRole("group", { name: "Harness plugins: Claude Code" });
    await waitFor(() =>
      expect(claude).toHaveTextContent("off in this workspace — from workspace.json"),
    );
    expect(claude).toHaveTextContent("on in this workspace — from charter.local.toml");
    expect(claude).toHaveTextContent("on in this workspace — from charter.toml");
    expect(claude).toHaveTextContent("not set — Claude Code decides, from its own settings");
    expect(within(section).getByLabelText("Claude Code: figma@official")).toHaveValue("");
    // charter's own plugin is a line, never a control.
    expect(claude).toHaveTextContent(OWN_WHY);
    expect(within(section).queryByLabelText("Claude Code: charter-app@inline")).toBeNull();
    // Only this section's sentences.
    expect(claude).toHaveTextContent(
      'workspaces/alpha/workspace.json sets settings.harness_plugins.claude."charter-app@inline" to false',
    );
    expect(claude).not.toHaveTextContent("a Shared sentence");
    for (const title of ["opencode", "Codex"]) {
      expect(within(section).getByRole("group", { name: `Harness plugins: ${title}` }))
        .toHaveTextContent(`plugins for ${title} are not supported yet`);
    }
    expect(asked("project_harness_plugins")).toEqual([{ plane: PLANE, workspace: "alpha" }]);
  });

  it("saves a plugin toggle as that one key under settings, and asks again", async () => {
    const { sent, asked } = core();
    const section = await drawn();
    const user = userEvent.setup();

    await waitFor(() =>
      expect(within(section).getByLabelText("Claude Code: serena@official")).toBeInTheDocument(),
    );
    await user.selectOptions(within(section).getByLabelText("Claude Code: serena@official"), "off");
    await user.click(within(section).getByRole("button", { name: "Save workspace.json" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].edits).toEqual([
      {
        path: [{ key: "harness_plugins" }, { key: "claude" }, { key: "serena@official" }],
        value: { kind: "bool", value: false },
      },
    ]);
    await waitFor(() => expect(asked("project_harness_plugins")).toHaveLength(2));
  });
});

describe("the Workspace settings view", () => {
  it("is drawn in a view pane by its own body, for its workspace, asking open_view nothing", async () => {
    const { asked } = core();
    render(
      <ViewPane
        plane={PLANE}
        view={workspaceSettingsView("alpha")}
        title={workspaceSettingsTitle("alpha")}
        workspace="alpha"
        waits={false}
        offered={[]}
        onOpenView={() => undefined}
        onAsk={() => undefined}
        onVaultChanged={() => undefined}
      />,
    );

    expect(
      await screen.findByRole("heading", { name: "Workspace settings · alpha" }),
    ).toBeInTheDocument();
    expect(await screen.findByTestId("settings-workspace")).toBeInTheDocument();
    expect(asked("open_view")).toEqual([]);
  });
});
