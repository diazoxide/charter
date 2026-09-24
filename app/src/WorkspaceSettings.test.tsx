import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { WorkspaceSettings } from "./ProjectSettings";
import { ViewPane } from "./Views";
import { workspaceSettingsTitle, workspaceSettingsView } from "./tabs";
import type {
  ProjectExtension,
  ProjectTheme,
  WorkspaceSettings as Settings,
  WorkspaceSettingsSaved,
} from "./bindings";
import { BUILT_IN } from "./theme/theme";

/** Two custom colours, as `#rrggbb`: taken from the built-in themes, because no colour is written
 *  outside `src/theme/` — a test's included (`literals.test.ts`). */
const PICKED = BUILT_IN["charter-light"].values["accent.base"];
const HELD = BUILT_IN["charter-dark"].values["accent.base"];

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

/** What `project_theme` answers in a workspace that picks nothing and has no colour. */
const NO_THEME: ProjectTheme = {
  options: [
    { value: "charter-dark", label: "charter-dark (built in)" },
    { value: "charter-light", label: "charter-light (built in)" },
    { value: "system", label: "Follow the system" },
  ],
  picked: null,
  file: null,
  draws: null,
  why: null,
  colour: null,
  ignored: [],
};

function core(
  settings: Settings = ALPHA,
  saved?: (sent: Record<string, unknown>) => WorkspaceSettingsSaved,
  theme: ProjectTheme = NO_THEME,
) {
  const sent: Record<string, unknown>[] = [];
  const asked: [string, Record<string, unknown>][] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push([cmd, given]);
    if (cmd === "workspace_settings") return settings;
    if (cmd === "project_extensions") return EXTENSIONS;
    if (cmd === "extensions_on") return [];
    if (cmd === "project_theme") return theme;
    if (cmd === "project_theme_drawn") return theme.draws;
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

describe("the Theme group (charter-app#281)", () => {
  const PICKS_LIGHT: ProjectTheme = {
    ...NO_THEME,
    picked: "charter-light",
    file: "workspaces/alpha/workspace.json",
    draws: "charter-light",
    colour: "teal",
  };
  const SET: Settings = {
    ...ALPHA,
    fields: [
      ...ALPHA.fields,
      { path: [{ key: "theme" }, { key: "use" }], value: { kind: "text", value: "charter-light" } },
      { path: [{ key: "theme" }, { key: "colour" }], value: { kind: "text", value: "teal" } },
    ],
  };

  it("asks the theme in this workspace, and says what is drawn here and which file picked it", async () => {
    const { asked } = core(SET, undefined, PICKS_LIGHT);
    const section = await drawn();

    const pick = await within(section).findByLabelText("Theme");
    expect(pick).toHaveValue("charter-light");
    expect(within(pick).getAllByRole("option")[0]).toHaveTextContent(
      "not set — the project's pick",
    );
    await waitFor(() =>
      expect(pick).toHaveAccessibleDescription(
        "Drawn in this workspace: charter-light (built in), picked in workspaces/alpha/workspace.json. The terminal follows the window.",
      ),
    );
    // Asked as the tab opens, and again when the window's answer for this workspace arrives.
    for (const one of asked("project_theme"))
      expect(one).toEqual({ plane: PLANE, workspace: "alpha" });
  });

  it("says when the project's file picked the theme drawn here", async () => {
    core(ALPHA, undefined, {
      ...NO_THEME,
      picked: "system",
      file: "charter.toml",
      draws: "system",
    });
    const section = await drawn();

    await waitFor(() =>
      expect(within(section).getByLabelText("Theme")).toHaveAccessibleDescription(
        "Drawn in this workspace: Follow the system, picked in charter.toml. The terminal follows the window.",
      ),
    );
  });

  it("says why the project's pick falls back here, without naming its file as the one drawn", async () => {
    const why =
      "charter.toml picks “Solarized Dark” from solarized, but solarized is off in this workspace — so the built-in charter-dark is drawn";
    core(ALPHA, undefined, {
      ...NO_THEME,
      picked: "solarized/Solarized Dark",
      file: "charter.toml",
      draws: "charter-dark",
      why,
    });
    const section = await drawn();

    const group = within(section).getByRole("group", { name: "Theme" });
    await waitFor(() => expect(group).toHaveTextContent(why));
    expect(within(section).getByLabelText("Theme")).toHaveAccessibleDescription(
      "Drawn in this workspace: charter-dark (built in). The terminal follows the window.",
    );
  });

  it("says a grey colour tints nothing", async () => {
    const grey = BUILT_IN["charter-dark"].values["text.muted"];
    core(
      {
        ...ALPHA,
        fields: [
          { path: [{ key: "theme" }, { key: "colour" }], value: { kind: "text", value: grey } },
        ],
      },
      undefined,
      { ...NO_THEME, colour: grey },
    );
    const section = await drawn();

    const group = within(section).getByRole("group", { name: "Theme" });
    await waitFor(() =>
      expect(group).toHaveTextContent(
        `${grey} is a grey, which has no hue to tint with — so this workspace is drawn without a colour.`,
      ),
    );
  });

  it("shows a value that is neither a name nor #rrggbb as held, not as a custom colour", async () => {
    const short = BUILT_IN["charter-dark"].values["text.muted"].slice(0, 4);
    core({
      ...ALPHA,
      fields: [
        { path: [{ key: "theme" }, { key: "colour" }], value: { kind: "text", value: short } },
      ],
    });
    const section = await drawn();

    expect(await within(section).findByLabelText("Colour")).toHaveValue(short);
    expect(within(section).queryByLabelText("Custom colour")).toBeNull();
  });

  it("offers the eight colours and a custom one, and says what a colour tints", async () => {
    core(SET, undefined, PICKS_LIGHT);
    const section = await drawn();

    const colour = await within(section).findByLabelText("Colour");
    expect(colour).toHaveValue("teal");
    expect(
      within(colour)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual([
      "none — the theme as it is",
      "Red",
      "Orange",
      "Yellow",
      "Green",
      "Teal",
      "Blue",
      "Purple",
      "Pink",
      "Custom…",
    ]);
    expect(colour).toHaveAccessibleDescription(
      "Tints this workspace's tab, its chat strip, the title bar's mark and the accent while it is in front. Text and the terminal keep the theme's own colours.",
    );
    // No custom colour is picked, so there is nothing to pick it with.
    expect(within(section).queryByLabelText("Custom colour")).toBeNull();
  });

  it("writes a colour under settings.theme, and a custom one as the #rrggbb it was given", async () => {
    const { sent } = core();
    const section = await drawn();
    const user = userEvent.setup();

    await user.selectOptions(await within(section).findByLabelText("Colour"), "purple");
    await user.click(within(section).getByRole("button", { name: "Save workspace.json" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].edits).toEqual([
      { path: [{ key: "theme" }, { key: "colour" }], value: { kind: "text", value: "purple" } },
    ]);

    await user.selectOptions(within(section).getByLabelText("Colour"), "custom");
    const custom = within(section).getByLabelText("Custom colour");
    fireEvent.input(custom, { target: { value: PICKED } });
    await user.click(within(section).getByRole("button", { name: "Save workspace.json" }));
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1].edits).toEqual([
      { path: [{ key: "theme" }, { key: "colour" }], value: { kind: "text", value: PICKED } },
    ]);
  });

  it("shows a custom colour the file holds as custom, with its value", async () => {
    core(
      {
        ...ALPHA,
        fields: [
          { path: [{ key: "theme" }, { key: "colour" }], value: { kind: "text", value: HELD } },
        ],
      },
      undefined,
      { ...NO_THEME, colour: HELD },
    );
    const section = await drawn();

    expect(await within(section).findByLabelText("Colour")).toHaveValue("custom");
    expect(within(section).getByLabelText("Custom colour")).toHaveValue(HELD);
  });

  it("says why a pick or a colour in this file is not used, under this file", async () => {
    core(SET, undefined, {
      ...NO_THEME,
      ignored: [
        {
          file: "workspaces/alpha/workspace.json",
          why: "workspaces/alpha/workspace.json sets settings.theme.colour to 3, which is not a colour",
        },
        { file: "charter.toml", why: "a Shared sentence that is not this section's" },
      ],
    });
    const section = await drawn();

    const group = within(section).getByRole("group", { name: "Theme" });
    await waitFor(() => expect(group).toHaveTextContent("settings.theme.colour to 3"));
    expect(group).not.toHaveTextContent("a Shared sentence");
  });

  it("has the window ask this workspace's theme again after a save", async () => {
    const { asked } = core(SET, undefined, PICKS_LIGHT);
    const section = await drawn();
    const user = userEvent.setup();
    await waitFor(() => expect(asked("project_theme_drawn")).toHaveLength(1));

    await user.selectOptions(within(section).getByLabelText("Colour"), "red");
    await user.click(within(section).getByRole("button", { name: "Save workspace.json" }));

    await waitFor(() => expect(asked("project_theme_drawn")).toHaveLength(2));
    expect(asked("project_theme_drawn")[1]).toEqual({ plane: PLANE, workspace: "alpha" });
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
