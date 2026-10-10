import { afterAll, afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type {
  HarnessPlugins,
  PlaneId,
  ProjectExtension,
  ProjectTheme,
  SettingsField,
  SettingsFile,
  WorkspaceSettings,
} from "../bindings";
import { expectEveryControlNamed, paidDebts } from "../a11y.testkit";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import { forgetGroups } from "./links";
import { SettingsTab } from "./SettingsTab";

/**
 * **Every control the Settings tab draws has a name** (DS-6, #629): each level it offers — You,
 * Project, Workspace — and in each, every group its nav lists, picked the way a person picks it
 * and held to `expectEveryControlNamed` with the whole tab on screen (the switcher, the nav, the
 * filter and the group). The core is a mock that answers each group with something in it, so
 * the group draws its rows rather than its empty state. The rule's own cases are
 * `a11y.names.test.tsx`.
 */

/**
 * Nameless controls drawn by files this lane does not hold (DS-6's follow-ups), each as the
 * check writes it. It only shrinks: a debt no level draws any more fails the last test.
 */
const DEBT: readonly string[] = [];

const found: string[] = [];

const PLANE = "/home/dev/plane";

const field = (keys: (string | number)[], value: SettingsField["value"]): SettingsField => ({
  path: keys.map((one) => (typeof one === "number" ? { index: one } : { key: one })),
  value,
});

const SHARED: SettingsFile = {
  which: "shared",
  file: "charter.toml",
  exists: true,
  text: '[plane]\nmode = "push"\n\n[[forge]]\nkind = "github"\nowner = "acme"\n',
  refusals: [],
  parsed: true,
  fields: [
    field(["plane", "mode"], { kind: "text", value: "push" }),
    field(["forge", 0, "kind"], { kind: "text", value: "github" }),
    field(["forge", 0, "owner"], { kind: "text", value: "acme" }),
  ],
};

const LOCAL: SettingsFile = {
  which: "local",
  file: "charter.local.toml",
  exists: true,
  text: '[harness.work]\nkind = "claude"\ncommand = ["claude"]\n',
  refusals: [],
  parsed: true,
  fields: [
    field(["harness", "work", "kind"], { kind: "text", value: "claude" }),
    field(["harness", "work", "command"], { kind: "list", value: ["claude"] }),
  ],
  entries: [
    {
      collection: "profiles",
      id: "profile:work",
      label: "work",
      keys: [{ key: "harness" }, { key: "work" }],
      values: [
        { field: "name", value: "work" },
        { field: "kind", value: "claude" },
        { field: "command", value: "claude" },
      ],
    },
  ],
};

const WORKSPACE: WorkspaceSettings = {
  workspace: "alpha",
  file: "workspaces/alpha/workspace.json",
  exists: true,
  text: '{\n  "name": "alpha"\n}\n',
  refusals: [],
  parsed: true,
  fields: [],
  live: true,
};

const EXTENSIONS: ProjectExtension[] = [
  { id: "linter", name: "Linter", state: "on", source: "default", settings: [], ignored: [] },
];

const HARNESSES: HarnessPlugins[] = [
  {
    harness: "claude",
    title: "Claude Code",
    unsupported: null,
    record: null,
    trouble: null,
    local_left_out: null,
    plugins: [
      {
        id: "review@acme",
        name: "review",
        origin: "acme",
        state: "not-set",
        source: "default",
        installed: true,
        pinned: null,
        ignored: [],
      },
    ],
  },
];

const THEME: ProjectTheme = {
  options: [
    { value: "charter-dark", label: "charter-dark (built in)" },
    { value: "charter-light", label: "charter-light (built in)" },
  ],
  picked: null,
  file: null,
  draws: null,
  why: null,
  colour: null,
  ignored: [],
  local_left_out: null,
};

const said = (value: string | null, source = "default") => ({ value, source });

/** The core, answering every read a level's groups make with something to draw. */
function core() {
  mockIPC(
    (cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "project_settings":
          return { shared: SHARED, local: LOCAL };
        case "workspace_settings":
          return WORKSPACE;
        case "project_extensions":
          return { extensions: EXTENSIONS, local_left_out: null };
        case "project_harness_plugins":
          return HARNESSES;
        case "project_theme":
          return THEME;
        case "project_saving_in_force":
          return {
            plane: {
              mode: said("push", "shared"),
              from_share: false,
              branch: said(null),
              save_branch: said(null),
              sign: said("off"),
              autosave: said("on"),
              autosave_after: said("1m"),
            },
            repos: [],
            plane_left_out: null,
            repos_left_out: null,
          };
        case "extensions_on":
        case "extension_icon_themes":
          return [];
        case "sandbox_state":
          return {
            on: false,
            offer: false,
            said: null,
            never: [],
            hosts_changed: null,
            presets_changed: null,
            presets: [],
            persona_hosts: [],
            besides: { project_hosts: 0, your_hosts: 0, folders: 0 },
            policy: null,
          };
        case "reachable_repos":
          return { repos: [{ name: "api", path: "acme/api", description: "" }], trouble: [] };
        case "workspace_repos":
          return { workspace: given.workspace, repos: [{ name: "api" }], cache_refused: null };
        case "workspace_panels":
          return {
            workspace: given.workspace,
            repos: ["api"],
            paths: {},
            absent: [],
            refused: [],
            todos: [],
            todos_refused: null,
            personas: [],
            persona: null,
            sessions: [],
            contributed: [],
          };
        case "sandbox_network":
          return {
            on: true,
            open: [{ title: "AI providers", hosts: ["api.anthropic.com"] }],
            blocked: [
              {
                target: "db.example.com:5432",
                looked_up: null,
                said: "a connection to an internet host this project does not allow",
                chat: "fix the build",
                at: 1_790_000_000,
                times: 3,
                reached: false,
                levels: ["you", "project"],
              },
            ],
          };
        case "sandbox_grants":
          return [
            {
              id: "you\u001fhost\u001fapi.example.com:443",
              what: "host",
              target: "api.example.com:443",
              persona: null,
              level: "you",
              by: null,
              at: 1_790_000_000,
              chat: null,
              locked: null,
              waiting: null,
              for_no_persona: false,
            },
          ];
        case "workspace_live_preview":
          return { live: true, files: [], remote: null, mode: "push" };
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}

beforeEach(() => {
  forgetThisLaunch();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: "/home/op/.config/charter/layout.json", found: false, document: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
  core();
});
afterEach(() => {
  cleanup();
  clearMocks();
  forgetGroups();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

/** Picks each group the level's nav lists, in turn, and holds the whole tab each time. */
async function everyGroup(level: string) {
  const nav = await screen.findByRole("navigation", { name: "Groups" });
  // A level reads its files before it lists its groups: wait until the list stops growing.
  await waitFor(() => expect(within(nav).getAllByRole("button").length).toBeGreaterThan(1));
  const names = within(nav)
    .getAllByRole("button")
    .map((one) => one.textContent ?? "");
  for (const name of names) {
    await userEvent.click(within(nav).getByRole("button", { name }));
    await screen.findByRole("region", { name });
    // Let what the group reads on its own arrive before it is held.
    await waitFor(() => expect(document.querySelector("[aria-busy='true']")).toBeNull());
    found.push(...expectEveryControlNamed(document.body, DEBT, `${level} › ${name}`));
  }
  return names;
}

/** Opens each of the level's files under Edit as TOML, in turn, and holds the tab each time. */
async function everyFile(level: string) {
  const links = () => screen.getByRole("group", { name: "Edit as TOML" });
  const files = within(links())
    .getAllByRole("button")
    .map((one) => one.textContent ?? "");
  for (const file of files) {
    await userEvent.click(within(links()).getByRole("button", { name: file }));
    await screen.findByRole("textbox", { name: (said) => said.includes(file) });
    found.push(
      ...expectEveryControlNamed(document.body, DEBT, `${level} › Edit as TOML › ${file}`),
    );
  }
  return files;
}

describe("every control the Settings tab draws has a name", () => {
  it("at You, in every group", async () => {
    render(<SettingsTab />);

    expect(await everyGroup("You")).toContain("Text");
  });

  it("at Project, in every group and under Edit as TOML", async () => {
    render(<SettingsTab plane={PLANE as PlaneId} level="project" />);

    expect(await everyGroup("Project")).toContain("Harness & profiles");
    expect(await everyFile("Project")).toHaveLength(2);
  });

  it("at Workspace, in every group", async () => {
    render(<SettingsTab plane={PLANE as PlaneId} workspace="alpha" level="workspace" />);

    expect(await everyGroup("Workspace")).toContain("Repos");
  });
});

afterAll(() => {
  // Run last, over what every level above drew: a paid debt comes off the list.
  expect(paidDebts(DEBT, found), "debts no level draws any more").toEqual([]);
});
