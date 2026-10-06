import { afterEach, describe, expect, it } from "vitest";
import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SettingsTab } from "./SettingsTab";
import { forgetGroups } from "./links";
import type { Level } from "./groups";
import { ViewPane } from "../Views";
import { settingsView, workspaceSettingsTitle, workspaceSettingsView } from "../tabs";
import type {
  HarnessPlugin,
  HarnessPlugins,
  ProjectExtension,
  ProjectTheme,
  SettingsChange,
  SettingsEdit,
  WorkspaceSettings as Settings,
  WorkspaceSettingsSaved,
} from "../bindings";
import { BUILT_IN } from "../theme/theme";

/** Two custom colours, as `#rrggbb`: taken from the built-in themes, because no colour is written
 *  outside `src/theme/` — a test's included (`literals.test.ts`). */
const PICKED = BUILT_IN["charter-light"].values["accent.base"];
const HELD = BUILT_IN["charter-dark"].values["accent.base"];

/**
 * **The Settings tab at the Workspace level** (SE-20, #1170; the spec on #558, V89b, V89e,
 * V89h): one workspace's settings — Live · Repos · Extensions · Appearance · Plugins — each file
 * setting written as its own key through the core as it changes, with Undo and a refused write
 * said beside it, as the Project level's are. It replaces the Workspace settings page
 * (charter-app#280), and everything that page reached is held here: what is in force and which
 * layer decided it, the theme and the colour, the harness plugins, LIVE and LOCAL, and the
 * repos. What the core keeps and refuses is `purlis_core::settings::workspace`'s tests.
 */

afterEach(() => {
  cleanup();
  clearMocks();
  // Which group each level shows is the window's, for as long as it runs (SE-22).
  forgetGroups();
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
        why: "workspaces/alpha/workspace.json sets extensions.stats.settings.nope, which stats does not declare — purlis hands it nothing",
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
  local_left_out: null,
};

const OWN_WHY =
  "charter@inline is always on: it is purlis's own plugin, and it carries purlis's hooks and the Bash guard";

/** What the core says is in force for each harness in this workspace (charter-app#282). */
const HARNESSES: HarnessPlugins[] = [
  {
    harness: "claude",
    title: "Claude Code",
    unsupported: null,
    record: "/home/dev/.claude/plugins/installed_plugins.json",
    trouble: null,
    local_left_out: null,
    plugins: [
      {
        id: "charter@inline",
        name: "charter",
        origin: "",
        state: "on",
        source: "default",
        installed: false,
        pinned: OWN_WHY,
        ignored: [
          {
            file: "workspaces/alpha/workspace.json",
            why: `workspaces/alpha/workspace.json sets settings.harness_plugins.claude."charter@inline" to false, and ${OWN_WHY}`,
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
      "plugins for opencode are not supported yet — opencode has no switch that turns one plugin off",
    trouble: null,
    local_left_out: null,
    plugins: [],
  },
  {
    harness: "codex",
    title: "Codex",
    record: "/home/dev/.codex/config.toml",
    unsupported: "plugins for Codex are not supported yet — Codex 0.147.0 ignores -c",
    trouble: null,
    local_left_out: null,
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

/** A manifest as the core would answer it after `edits`: each edited key set or gone, and a
 *  text that differs, so the next write is checked against the new one. */
function applied(file: Settings, edits: readonly SettingsEdit[]): Settings {
  let fields = [...file.fields];
  for (const edit of edits) {
    const at = JSON.stringify(edit.path);
    fields = fields.filter((one) => JSON.stringify(one.path) !== at);
    if (edit.value !== null) fields.push({ path: edit.path, value: edit.value });
  }
  return { ...file, exists: true, fields, text: `${file.text} ` };
}

function core(
  settings: Settings = ALPHA,
  saved?: (sent: Record<string, unknown>) => WorkspaceSettingsSaved,
  theme: ProjectTheme = NO_THEME,
  leftOut: string | null = null,
  {
    extensions = EXTENSIONS,
    harnesses = HARNESSES,
    // `drop_repo` takes the clone, and the manifest — a hand's — keeps its row (#1228).
    dropsClean = false,
  } = {},
) {
  let file = settings;
  /** What `alpha` names and this machine has not cloned (#1228). */
  const absent = ["web"];
  const cloned = ["api"];
  const sent: Record<string, unknown>[] = [];
  const asked: [string, Record<string, unknown>][] = [];
  mockIPC(
    (cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      asked.push([cmd, given]);
      if (cmd === "workspace_settings") return file;
      // The project's two files, empty, for a tab that is at the Project level.
      if (cmd === "project_settings")
        return Object.fromEntries(
          (["shared", "local"] as const).map((which) => [
            which,
            {
              which,
              file: which === "shared" ? "charter.toml" : "charter.local.toml",
              exists: false,
              text: "",
              refusals: [],
              parsed: true,
              fields: [],
            },
          ]),
        );
      if (cmd === "project_extensions") return { extensions, local_left_out: leftOut };
      if (cmd === "project_harness_plugins")
        return harnesses.map((one) => ({ ...one, local_left_out: leftOut }));
      if (cmd === "extensions_on") return [];
      if (cmd === "project_theme") return { ...theme, local_left_out: leftOut };
      if (cmd === "project_theme_drawn") return theme.draws;
      if (cmd === "reachable_repos")
        return {
          repos: [
            { name: "api", path: "acme/api", description: "" },
            { name: "web", path: "acme/web", description: "" },
          ],
          trouble: [],
        };
      if (cmd === "workspace_repos")
        return {
          workspace: given.workspace,
          repos: cloned.map((name) => ({ name })),
          cache_refused: null,
        };
      if (cmd === "workspace_panels")
        return {
          workspace: given.workspace,
          repos: [...cloned],
          paths: {},
          absent: [...absent],
          refused: [],
          todos: [],
          todos_refused: null,
          personas: [],
          persona: null,
          sessions: [],
          contributed: [],
        };
      if (cmd === "drop_repo_membership") {
        absent.splice(absent.indexOf(String(given.repo)), 1);
        return [`Removed '${String(given.repo)}' from workspace 'alpha'.`];
      }
      if (cmd === "drop_repo" && dropsClean) {
        const repo = String(given.repo);
        cloned.splice(cloned.indexOf(repo), 1);
        absent.push(repo);
        return [`Removed '${repo}' from workspace 'alpha'.`];
      }
      if (cmd === "drop_repo")
        throw {
          said: "Refusing to remove 'api' — this would discard work: api: uncommitted changes. Push or commit first.",
          at_risk: [{ what: "api", said: "api: uncommitted changes" }],
        };
      if (cmd === "workspace_live_preview")
        return { live: file.live, files: [], remote: null, mode: "push" };
      if (cmd === "save_workspace_settings") {
        sent.push(given);
        const answer = saved?.(given);
        if (answer) return answer;
        const change = given.change as SettingsChange;
        file =
          change.kind === "raw"
            ? { ...file, exists: true, text: change.text }
            : applied(file, change.edits);
        return { kind: "saved", settings: file };
      }
      return undefined;
    },
    { shouldMockEvents: true },
  );
  return {
    sent,
    asked: (cmd: string) => asked.filter(([one]) => one === cmd).map(([, given]) => given),
  };
}

/** The edits a form's write sent (`save_workspace_settings`' change). */
const editsOf = (sent: Record<string, unknown>) =>
  (sent.change as SettingsChange & { edits?: SettingsEdit[] }).edits;

/** The tab as a view tab holds it, about `alpha`: its level is the tab's. */
function Tab({ at = "workspace" }: { at?: Level }) {
  const [level, setLevel] = useState<Level>(at);
  return <SettingsTab plane={PLANE} workspace="alpha" level={level} onLevelChange={setLevel} />;
}

const nav = () => screen.getByRole("navigation", { name: "Groups" });
const groups = () =>
  within(nav())
    .getAllByRole("button")
    .map((one) => one.textContent);
const open = (name: string) => userEvent.click(within(nav()).getByRole("button", { name }));
const shown = () => screen.getByRole("region", { name: /./ });

/** The tab at alpha's level, read, showing `group`. */
async function at(group = "Live") {
  render(<Tab />);
  await within(await screen.findByRole("navigation", { name: "Groups" })).findByRole("button", {
    name: "Live",
  });
  if (group !== "Live") await open(group);
  return shown();
}

describe("the Workspace level", () => {
  it("is offered beside You and Project for a tab about a workspace, and not otherwise", async () => {
    core();
    render(<Tab at="you" />);

    expect(screen.getByRole("radio", { name: "Workspace" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("radio", { name: "Workspace" }));
    expect(screen.getByRole("radio", { name: "Workspace" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(await within(nav()).findByRole("button", { name: "Live" })).toBeInTheDocument();

    cleanup();
    render(<SettingsTab plane={PLANE} level="you" />);
    expect(screen.queryByRole("radio", { name: "Workspace" })).toBeNull();
  });

  it("lists the five groups in order for that workspace", async () => {
    const { asked } = core();
    await at();

    await waitFor(() =>
      expect(groups()).toEqual(["Live", "Repos", "Extensions", "Appearance", "Plugins"]),
    );
    expect(asked("workspace_settings")).toContainEqual({ plane: PLANE, workspace: "alpha" });
    expect(asked("project_extensions")).toContainEqual({ plane: PLANE, workspace: "alpha" });
  });

  it("hides a group with nothing in it", async () => {
    core(ALPHA, undefined, NO_THEME, null, { extensions: [], harnesses: [] });
    await at();

    await waitFor(() => expect(groups()).toEqual(["Live", "Repos", "Appearance"]));
  });

  it("says where it sits between the project's two files, and that no secret goes in it", async () => {
    core();
    await at();

    expect(
      screen.getByText(/read between charter\.toml and charter\.local\.toml/),
    ).toHaveTextContent("vault:<vault>/<key>");
  });

  it("says a setting of a LIVE workspace is committed, and of a LOCAL one stays here", async () => {
    core();
    await at("Appearance");
    expect(await screen.findByLabelText("Theme")).toHaveAccessibleDescription(
      /Kept in workspaces\/alpha\/workspace\.json, committed with this LIVE workspace; your team sees it\./,
    );

    cleanup();
    core({ ...ALPHA, live: false });
    await at("Appearance");
    expect(await screen.findByLabelText("Theme")).toHaveAccessibleDescription(
      /stays on this machine while the workspace is not LIVE/,
    );
  });

  it("says what purlis does not take from the settings as they stand", async () => {
    core({
      ...ALPHA,
      refusals: ["settings.colour in workspaces/alpha/workspace.json is not read"],
    });
    await at();

    expect(
      screen.getByText("settings.colour in workspaces/alpha/workspace.json is not read"),
    ).toBeInTheDocument();
  });
});

describe("every workspace setting there is, at the Workspace level", () => {
  it.each([
    ["Live", ["Published with the project"]],
    ["Repos", ["Cloned here", "Not cloned here"]],
    ["Extensions", ["Persona statistics: enabled", "Solarized: enabled"]],
    ["Appearance", ["Theme", "Colour", "Icons"]],
    [
      "Plugins",
      [
        "Claude Code: figma@official",
        "Claude Code: humanizer@h",
        "Claude Code: serena@official",
        "Claude Code: stripe@official",
      ],
    ],
  ])("%s holds %j", async (name, labels) => {
    core();
    const group = await at(name);

    await waitFor(() =>
      expect(
        [...group.querySelectorAll(".ui-setting-label")].map((one) => one.textContent),
      ).toEqual(labels),
    );
  });

  it("writes the icons a workspace's file trees draw once typed, as settings.theme.icons", async () => {
    const { sent } = core();
    const group = await at("Appearance");

    await userEvent.type(within(group).getByLabelText("Icons"), "seti/seti");
    expect(sent).toHaveLength(0);
    await userEvent.tab();

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(editsOf(sent[0])).toEqual([
      { path: [{ key: "theme" }, { key: "icons" }], value: { kind: "text", value: "seti/seti" } },
    ]);
  });
});

describe("Live", () => {
  it("names the switch's group by its row", async () => {
    core();
    const group = await at("Live");

    expect(
      within(group).getByRole("group", { name: "Published with the project" }),
    ).toContainElement(within(group).getByRole("button", { name: "Make local…" }));
  });

  it("offers the same switch as the workspace's menu, through the same confirmation", async () => {
    core({ ...ALPHA, live: false });
    const group = await at("Live");

    expect(group).toHaveTextContent("LOCAL: its charter, memory and todos stay on this machine.");
    await userEvent.click(within(group).getByRole("button", { name: "Make live…" }));

    expect(await screen.findByRole("alertdialog", { name: "Make alpha live?" })).toBeTruthy();
  });

  it("offers to make a LIVE workspace local", async () => {
    core();
    const group = await at("Live");

    expect(group).toHaveTextContent(
      "LIVE: its charter, memory and todos are published with the project.",
    );
    expect(within(group).getByRole("button", { name: "Make local…" })).toBeInTheDocument();
  });
});

describe("Repos", () => {
  it("ticks what is cloned here, and says the core's refusal to remove one holding work beside it", async () => {
    const { asked } = core();
    const group = await at("Repos");

    const api = await within(group).findByRole("checkbox", { name: "api" });
    await waitFor(() => expect(api).toBeChecked());
    await userEvent.click(api);
    await userEvent.click(within(group).getByRole("button", { name: "Remove 1" }));

    expect(await within(group).findByRole("alert")).toHaveTextContent("api: uncommitted changes");
    expect(asked("drop_repo")).toEqual([{ plane: PLANE, workspace: "alpha", repo: "api" }]);
  });
});

describe("Repos not cloned here", () => {
  it("offers to remove one from the workspace, and asks first", async () => {
    const { asked } = core();
    const group = await at("Repos");

    const named = await within(group).findByRole("button", {
      name: "Remove web from workspace…",
    });
    await userEvent.click(named);
    const asking = await screen.findByRole("alertdialog", { name: "Remove web from alpha?" });
    expect(asked("drop_repo_membership")).toEqual([]);
    await userEvent.click(within(asking).getByRole("button", { name: "Remove from workspace" }));

    await waitFor(() =>
      expect(
        within(group).queryByRole("button", { name: "Remove web from workspace…" }),
      ).toBeNull(),
    );
    expect(asked("drop_repo_membership")).toEqual([
      { plane: PLANE, workspace: "alpha", repo: "web" },
    ]);
    expect(asked("drop_repo")).toEqual([]);
  });

  it("lists a clone unticked from a hand's manifest as soon as the picker applies", async () => {
    core(ALPHA, undefined, NO_THEME, null, { dropsClean: true });
    const group = await at("Repos");

    const api = await within(group).findByRole("checkbox", { name: "api" });
    await waitFor(() => expect(api).toBeChecked());
    await userEvent.click(api);
    await userEvent.click(within(group).getByRole("button", { name: "Remove 1" }));

    expect(
      await within(group).findByRole("button", { name: "Remove api from workspace…" }),
    ).toHaveClass("ends-it");
  });

  it("removes nothing when the question is cancelled", async () => {
    const { asked } = core();
    const group = await at("Repos");

    await userEvent.click(
      await within(group).findByRole("button", { name: "Remove web from workspace…" }),
    );
    const asking = await screen.findByRole("alertdialog", { name: "Remove web from alpha?" });
    await userEvent.click(within(asking).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(asked("drop_repo_membership")).toEqual([]);
    expect(
      within(group).getByRole("button", { name: "Remove web from workspace…" }),
    ).toBeInTheDocument();
  });
});

describe("Extensions", () => {
  it("says what is in force in this workspace, and which layer decided it", async () => {
    core();
    const group = await at("Extensions");

    await waitFor(() =>
      expect(group).toHaveTextContent("Persona statistics: off — turned off in this workspace"),
    );
    expect(group).toHaveTextContent("Solarized: on — enabled in charter.local.toml");
    expect(within(group).getByLabelText("Persona statistics: enabled")).toHaveValue("off");
    expect(within(group).getByLabelText("Solarized: enabled")).toHaveValue("");
    // Only this file's sentences.
    expect(group).toHaveTextContent("which stats does not declare");
    expect(group).not.toHaveTextContent("a Local sentence");
  });

  it("writes a pick as that one key, against the manifest it was read as, at once", async () => {
    const { sent, asked } = core();
    const group = await at("Extensions");

    await userEvent.selectOptions(within(group).getByLabelText("Persona statistics: enabled"), "");

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual({
      plane: PLANE,
      workspace: "alpha",
      base: MANIFEST,
      change: {
        kind: "edits",
        edits: [
          { path: [{ key: "extensions" }, { key: "stats" }, { key: "enabled" }], value: null },
        ],
      },
    });
    await waitFor(() => expect(asked("project_extensions").length).toBeGreaterThan(1));
  });

  it("sends a manifest that is not there yet as new", async () => {
    const { sent } = core({ ...ALPHA, exists: false, text: "", fields: [] });
    const group = await at("Extensions");

    await userEvent.selectOptions(within(group).getByLabelText("Solarized: enabled"), "off");

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].base).toBeNull();
  });

  it("undoes the last change by writing the value it had back", async () => {
    const { sent } = core();
    const group = await at("Extensions");
    await userEvent.selectOptions(
      within(group).getByLabelText("Persona statistics: enabled"),
      "on",
    );

    await userEvent.click(await within(group).findByRole("button", { name: "Undo" }));

    await waitFor(() => expect(sent).toHaveLength(2));
    expect(editsOf(sent[1])).toEqual([
      {
        path: [{ key: "extensions" }, { key: "stats" }, { key: "enabled" }],
        value: { kind: "bool", value: false },
      },
    ]);
    await waitFor(() =>
      expect(within(group).getByLabelText("Persona statistics: enabled")).toHaveValue("off"),
    );
  });

  it("says the value comes from the workspace's file, and its reset takes it out (SE-18)", async () => {
    const { sent } = core();
    const group = await at("Extensions");
    const box = () => within(group).getByLabelText("Persona statistics: enabled");
    expect(box()).toHaveAccessibleDescription(
      /From workspaces\/alpha\/workspace\.json, at the Workspace level\./,
    );
    const row = box().closest(".ui-setting-row") as HTMLElement;
    expect(within(row).queryByRole("radio")).toBeNull();

    await userEvent.click(within(row).getByRole("button", { name: "Reset" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(editsOf(sent[0])).toEqual([
      { path: [{ key: "extensions" }, { key: "stats" }, { key: "enabled" }], value: null },
    ]);
  });

  it("says a refused write beside its setting, in the core's own words", async () => {
    core(ALPHA, () => ({
      kind: "refused",
      reasons: ["workspaces/alpha/workspace.json changed on disk since this tab read it"],
    }));
    const group = await at("Extensions");

    await userEvent.selectOptions(within(group).getByLabelText("Solarized: enabled"), "on");

    expect(await within(group).findByRole("alert")).toHaveTextContent(
      "workspaces/alpha/workspace.json changed on disk since this tab read it",
    );
    expect(within(group).getByLabelText("Solarized: enabled")).toHaveValue("");
  });
});

describe("Appearance (charter-app#281)", () => {
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
    const group = await at("Appearance");

    const pick = await within(group).findByLabelText("Theme");
    expect(pick).toHaveValue("charter-light");
    expect(within(pick).getAllByRole("option")[0]).toHaveTextContent(
      "not set — the project's pick",
    );
    await waitFor(() =>
      expect(pick).toHaveAccessibleDescription(
        /^Drawn in this workspace: charter-light \(built in\), picked in workspaces\/alpha\/workspace\.json\. The terminal follows the window\./,
      ),
    );
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
    const group = await at("Appearance");

    await waitFor(() =>
      expect(within(group).getByLabelText("Theme")).toHaveAccessibleDescription(
        /^Drawn in this workspace: Follow the system, picked in charter\.toml\./,
      ),
    );
  });

  it("says why the project's pick falls back here", async () => {
    const why =
      "charter.toml picks “Solarized Dark” from solarized, but solarized is off in this workspace — so the built-in charter-dark is drawn";
    core(ALPHA, undefined, {
      ...NO_THEME,
      picked: "solarized/Solarized Dark",
      file: "charter.toml",
      draws: "charter-dark",
      why,
    });
    const group = await at("Appearance");

    await waitFor(() => expect(group).toHaveTextContent(why));
    expect(within(group).getByLabelText("Theme")).toHaveAccessibleDescription(
      /^Drawn in this workspace: charter-dark \(built in\)\. The terminal follows the window\./,
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
    const group = await at("Appearance");

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
    const group = await at("Appearance");

    expect(await within(group).findByLabelText("Colour")).toHaveValue(short);
    expect(within(group).queryByLabelText("Custom colour")).toBeNull();
  });

  it("offers the eight colours and a custom one, and says what a colour tints", async () => {
    core(SET, undefined, PICKS_LIGHT);
    const group = await at("Appearance");

    const colour = await within(group).findByLabelText("Colour");
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
      /^Tints this workspace's tab, its chat strip, the title bar's mark and the accent while it is in front\. Text and the terminal keep the theme's own colours\./,
    );
    expect(within(group).queryByLabelText("Custom colour")).toBeNull();
  });

  it("writes a colour under settings.theme at once, and a custom one as the #rrggbb picked", async () => {
    const { sent } = core();
    const group = await at("Appearance");

    await userEvent.selectOptions(await within(group).findByLabelText("Colour"), "purple");
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(editsOf(sent[0])).toEqual([
      { path: [{ key: "theme" }, { key: "colour" }], value: { kind: "text", value: "purple" } },
    ]);

    await userEvent.selectOptions(within(group).getByLabelText("Colour"), "custom");
    await waitFor(() => expect(sent).toHaveLength(2));
    const custom = await within(group).findByLabelText("Custom colour");
    // A drag is every step of `input`; only the pick, `change`, is written.
    fireEvent.input(custom, { target: { value: PICKED } });
    await act(() => new Promise((settle) => setTimeout(settle, 20)));
    expect(sent).toHaveLength(2);
    fireEvent.change(custom, { target: { value: PICKED } });
    await waitFor(() => expect(sent).toHaveLength(3));
    expect(editsOf(sent[2])).toEqual([
      { path: [{ key: "theme" }, { key: "colour" }], value: { kind: "text", value: PICKED } },
    ]);
  });

  it("writes a colour dragged in the well once, when the well is left", async () => {
    const { sent } = core(
      {
        ...ALPHA,
        fields: [
          { path: [{ key: "theme" }, { key: "colour" }], value: { kind: "text", value: HELD } },
        ],
      },
      undefined,
      { ...NO_THEME, colour: HELD },
    );
    const group = await at("Appearance");
    const custom = await within(group).findByLabelText("Custom colour");

    fireEvent.input(custom, { target: { value: PICKED } });
    fireEvent.blur(custom);

    await waitFor(() => expect(sent).toHaveLength(1));
    await act(() => new Promise((settle) => setTimeout(settle, 20)));
    expect(sent).toHaveLength(1);
    expect(editsOf(sent[0])).toEqual([
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
    const group = await at("Appearance");

    expect(await within(group).findByLabelText("Colour")).toHaveValue("custom");
    expect(within(group).getByLabelText("Custom colour")).toHaveValue(HELD);
  });

  it("says why a pick or a colour in this file is not used, and only this file's", async () => {
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
    const group = await at("Appearance");

    await waitFor(() => expect(group).toHaveTextContent("settings.theme.colour to 3"));
    expect(group).not.toHaveTextContent("a Shared sentence");
  });

  it("has the window ask this workspace's theme again after a write", async () => {
    const { asked } = core(SET, undefined, PICKS_LIGHT);
    const group = await at("Appearance");
    await waitFor(() => expect(asked("project_theme_drawn")).toHaveLength(1));

    await userEvent.selectOptions(within(group).getByLabelText("Colour"), "red");

    await waitFor(() => expect(asked("project_theme_drawn")).toHaveLength(2));
    expect(asked("project_theme_drawn")[1]).toEqual({ plane: PLANE, workspace: "alpha" });
  });
});

describe("Plugins (charter-app#282)", () => {
  it("lists each harness's plugins, each saying which layer decided it", async () => {
    const { asked } = core();
    const group = await at("Plugins");

    await waitFor(() =>
      expect(group).toHaveTextContent("off in this workspace — from workspace.json"),
    );
    expect(group).toHaveTextContent("on in this workspace — from charter.local.toml");
    expect(group).toHaveTextContent("on in this workspace — from charter.toml");
    expect(group).toHaveTextContent("not set — Claude Code decides, from its own settings");
    expect(within(group).getByLabelText("Claude Code: figma@official")).toHaveValue("");
    // purlis's own plugin is a line, never a control.
    expect(group).toHaveTextContent(OWN_WHY);
    expect(within(group).queryByLabelText("Claude Code: charter@inline")).toBeNull();
    expect(group).toHaveTextContent(
      'workspaces/alpha/workspace.json sets settings.harness_plugins.claude."charter@inline" to false',
    );
    expect(group).not.toHaveTextContent("a Shared sentence");
    for (const title of ["opencode", "Codex"])
      expect(group).toHaveTextContent(`plugins for ${title} are not supported yet`);
    expect(asked("project_harness_plugins")).toContainEqual({ plane: PLANE, workspace: "alpha" });
  });

  it("writes a plugin toggle as that one key under settings, and asks again", async () => {
    const { sent, asked } = core();
    const group = await at("Plugins");

    await userEvent.selectOptions(
      await within(group).findByLabelText("Claude Code: serena@official"),
      "off",
    );

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(editsOf(sent[0])).toEqual([
      {
        path: [{ key: "harness_plugins" }, { key: "claude" }, { key: "serena@official" }],
        value: { kind: "bool", value: false },
      },
    ]);
    await waitFor(() => expect(asked("project_harness_plugins").length).toBeGreaterThan(1));
  });
});

/** The ignore check's sentence for a `charter.local.toml` git would commit, as the core says it
 *  (charter-app#308). */
const LEFT_OUT =
  "git would commit charter.local.toml, so purlis reads nothing in it until it is ignored — purlis doctor --fix local-ignore adds /charter.local.toml to .gitignore.";

describe("a charter.local.toml git would carry (charter-app#319)", () => {
  it("is said in each group that shows what is in force, once per old group", async () => {
    core(ALPHA, undefined, NO_THEME, LEFT_OUT);
    for (const [name, times] of [
      ["Extensions", 1],
      ["Appearance", 1],
      ["Plugins", 3],
    ] as const) {
      const group = await at(name);
      await waitFor(() => expect(within(group).getAllByText(LEFT_OUT)).toHaveLength(times));
      cleanup();
    }
  });

  it("is not said while charter reads the file", async () => {
    core();
    await at("Plugins");

    expect(screen.queryByText(/charter reads nothing in it/)).toBeNull();
  });
});

describe("Settings at a workspace's level, as a view", () => {
  it("is drawn in a view pane for its workspace, asking open_view nothing", async () => {
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
    expect(screen.getByRole("radio", { name: "Workspace" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(await within(nav()).findByRole("button", { name: "Live" })).toBeInTheDocument();
    expect(asked("open_view")).toEqual([]);
  });

  it("moves the pane it is in to the Project level", async () => {
    core();
    const moved: [string, string][] = [];
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
        onShowInstead={(_from, to, title) => moved.push([`${to.view}/${to.key}`, title])}
      />,
    );
    await within(await screen.findByRole("navigation", { name: "Groups" })).findByRole("button", {
      name: "Live",
    });

    await userEvent.click(screen.getByRole("radio", { name: "Project" }));

    expect(moved).toEqual([["settings/project", "Settings"]]);
  });

  it("moves a Project-level pane on a workspace's strip to that workspace's level", async () => {
    core();
    const moved: [string, string][] = [];
    render(
      <ViewPane
        plane={PLANE}
        view={settingsView("project")}
        title="Settings"
        workspace="alpha"
        waits={false}
        offered={[]}
        onOpenView={() => undefined}
        onAsk={() => undefined}
        onVaultChanged={() => undefined}
        onShowInstead={(_from, to, title) => moved.push([`${to.view}/${to.key}`, title])}
      />,
    );

    await userEvent.click(await screen.findByRole("radio", { name: "Workspace" }));

    expect(moved).toEqual([["workspace-settings/alpha", "Workspace settings · alpha"]]);
  });
});

describe("Edit as JSON (NO-7, #1232)", () => {
  const editLink = () =>
    within(screen.getByRole("group", { name: "Edit as JSON" })).getByRole("button", {
      name: "workspace.json",
    });
  const box = () => screen.getByRole("textbox", { name: "workspace.json, as JSON" });

  it("sits at the foot of the nav and shows the whole manifest", async () => {
    core();
    await at();

    await userEvent.click(editLink());

    expect(box()).toHaveValue(MANIFEST);
    expect(screen.queryByRole("group", { name: "Edit as TOML" })).toBeNull();
  });

  it("saves the whole text through the core, against the text the edit began from", async () => {
    const { sent } = core();
    await at();
    await userEvent.click(editLink());
    const typed = '{"name": "alpha", "description": "mended"}';

    fireEvent.change(box(), { target: { value: typed } });
    await userEvent.click(screen.getByRole("button", { name: "Save workspace.json" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual({
      plane: PLANE,
      workspace: "alpha",
      base: MANIFEST,
      change: { kind: "raw", text: typed },
    });
    await waitFor(() => expect(box()).toHaveValue(typed));
  });

  it("says the core's refusal and keeps the edit", async () => {
    core(ALPHA, () => ({
      kind: "refused",
      reasons: ["workspaces/alpha/workspace.json would not be a JSON object, so nothing was saved"],
    }));
    await at();
    await userEvent.click(editLink());

    fireEvent.change(box(), { target: { value: "{" } });
    await userEvent.click(screen.getByRole("button", { name: "Save workspace.json" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "workspaces/alpha/workspace.json would not be a JSON object",
    );
    expect(box()).toHaveValue("{");
  });

  it("is where a manifest that is not JSON is mended", async () => {
    core({
      ...ALPHA,
      text: '{"name": ',
      parsed: false,
      fields: [],
      refusals: [
        "workspaces/alpha/workspace.json is not a JSON object, so purlis reads no settings from it — mend it by hand",
      ],
    });
    await at();

    expect(
      screen.getByText("Open workspace.json under Edit as JSON to mend it."),
    ).toBeInTheDocument();
    await userEvent.click(editLink());
    expect(box()).toHaveValue('{"name": ');
  });
});

describe("a standing refusal at the Workspace level (NO-7, #1232)", () => {
  it("links to the setting it is about, which is shown and focused", async () => {
    core({
      ...ALPHA,
      refusals: [
        "settings.theme.icons in workspaces/alpha/workspace.json is 7, not an icon theme's pick",
      ],
    });
    await at();

    await userEvent.click(screen.getByRole("button", { name: "Go to Appearance › Icons" }));

    expect(within(nav()).getByRole("button", { name: "Appearance" })).toHaveAttribute(
      "aria-current",
      "true",
    );
    await waitFor(() => expect(screen.getByLabelText("Icons")).toHaveFocus());
  });

  it("draws no link for a key no setting here holds", async () => {
    core({
      ...ALPHA,
      refusals: ["settings.colour in workspaces/alpha/workspace.json is not read"],
    });
    await at();

    const row = screen.getByText("settings.colour in workspaces/alpha/workspace.json is not read");
    expect(within(row.closest("li") as HTMLElement).queryByRole("button")).toBeNull();
  });
});
