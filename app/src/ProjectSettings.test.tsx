import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ProjectSettings } from "./ProjectSettings";
import { projectThemeChanged } from "./projectTheme";
import { ViewPane } from "./Views";
import { SETTINGS_TITLE, SETTINGS_VIEW } from "./tabs";
import type {
  ProjectExtension,
  ProjectTheme,
  ProjectSettings as Both,
  SavingInForce,
  SettingsFile,
  SettingsSaved,
} from "./bindings";

/**
 * The Project settings tab (charter-app#252): what it shows of the two files, what a save sends
 * the core, and how a refusal comes back. Whether a save is refused, and in which words, is the
 * core's (`charter_core::settings`'s tests); here the core is the mock, and what is held is that
 * the window sends it the right thing and draws what it says.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

const SHARED_TEXT = `# the team's
[[forge]]
kind = "github"
owner = "acme"

[memory]
share = "local"
`;

const SHARED: SettingsFile = {
  which: "shared",
  file: "charter.toml",
  exists: true,
  text: SHARED_TEXT,
  refusals: [],
  parsed: true,
  fields: [
    {
      path: [{ key: "forge" }, { index: 0 }, { key: "kind" }],
      value: { kind: "text", value: "github" },
    },
    {
      path: [{ key: "forge" }, { index: 0 }, { key: "owner" }],
      value: { kind: "text", value: "acme" },
    },
    { path: [{ key: "memory" }, { key: "share" }], value: { kind: "text", value: "local" } },
  ],
};

const LOCAL_TEXT = `[harness.work]
kind = "claude"
command = ["claude"]
env = { CLAUDE_CONFIG_DIR = "~/.work", LANG = "C" }
`;

const LOCAL: SettingsFile = {
  which: "local",
  file: "charter.local.toml",
  exists: true,
  text: LOCAL_TEXT,
  refusals: [],
  parsed: true,
  fields: [
    {
      path: [{ key: "harness" }, { key: "work" }, { key: "kind" }],
      value: { kind: "text", value: "claude" },
    },
    {
      path: [{ key: "harness" }, { key: "work" }, { key: "command" }],
      value: { kind: "list", value: ["claude"] },
    },
    {
      path: [{ key: "harness" }, { key: "work" }, { key: "env" }, { key: "CLAUDE_CONFIG_DIR" }],
      value: { kind: "text", value: "~/.work" },
    },
    {
      path: [{ key: "harness" }, { key: "work" }, { key: "env" }, { key: "LANG" }],
      value: { kind: "text", value: "C" },
    },
  ],
};

const NO_LOCAL: SettingsFile = {
  ...LOCAL,
  exists: false,
  text: "",
  fields: [],
};

/** The theme `project_theme` answers for a project that picked none. */
const NO_PICK: ProjectTheme = {
  options: [
    { value: "charter-dark", label: "charter-dark (built in)" },
    { value: "charter-light", label: "charter-light (built in)" },
    { value: "system", label: "Follow the system" },
    { value: "solarized/Solarized Dark", label: "Solarized Dark (Solarized)" },
  ],
  picked: null,
  file: null,
  draws: null,
  why: null,
  colour: null,
  ignored: [],
  local_left_out: null,
};

/** One save setting in force: its value as the files write it, and the file that decided it. */
const said = (value: string | null, source = "default") => ({ value, source });

/** What `project_saving_in_force` answers for a plane with no `[plane]` or `[repos]` and no
 *  inventory: every default, and no repo. */
const NO_SAVING: SavingInForce = {
  plane: {
    mode: said(null),
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

/** The core, as a mock: `project_settings` answers `both`, `project_extensions` answers
 *  `extensions`, `project_saving_in_force` answers `saving`, and `save_project_settings` answers
 *  `saved` and records what it was sent. `leftOut` is why every answer says
 *  `charter.local.toml` was left out (charter-app#319). */
function core(
  both: Both,
  saved?: (sent: Record<string, unknown>) => SettingsSaved,
  extensions: ProjectExtension[] = [],
  theme: ProjectTheme | (() => ProjectTheme) = NO_PICK,
  leftOut: string | null = null,
  saving: SavingInForce | { trouble: string } = NO_SAVING,
) {
  const themeNow = () => (typeof theme === "function" ? theme() : theme);
  const sent: Record<string, unknown>[] = [];
  let reads = 0;
  let extensionReads = 0;
  let savingReads = 0;
  let themeAsks = 0;
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    if (cmd === "project_settings") {
      reads += 1;
      return both;
    }
    if (cmd === "project_saving_in_force") {
      savingReads += 1;
      if ("trouble" in saving) throw saving.trouble;
      return leftOut === null
        ? saving
        : { ...saving, plane_left_out: leftOut, repos_left_out: leftOut };
    }
    if (cmd === "project_extensions") {
      extensionReads += 1;
      return { extensions, local_left_out: leftOut };
    }
    if (cmd === "extensions_on") return [];
    if (cmd === "project_theme") return { ...themeNow(), local_left_out: leftOut };
    if (cmd === "project_theme_drawn") {
      themeAsks += 1;
      return themeNow().draws;
    }
    if (cmd === "save_project_settings") {
      sent.push(given);
      return saved?.(given) ?? { kind: "saved", file: both.shared };
    }
    return undefined;
  });
  return {
    sent,
    reads: () => reads,
    extensionReads: () => extensionReads,
    savingReads: () => savingReads,
    themeAsks: () => themeAsks,
  };
}

async function drawn() {
  render(<ProjectSettings plane={PLANE} />);
  return {
    shared: await screen.findByTestId("settings-shared"),
    local: await screen.findByTestId("settings-local"),
  };
}

describe("the Project settings tab", () => {
  it("says which file each section is and who sees it", async () => {
    core({ shared: SHARED, local: LOCAL });
    const { shared, local } = await drawn();

    expect(within(shared).getByRole("heading", { name: "Shared" })).toBeInTheDocument();
    expect(shared).toHaveTextContent("charter.toml · Committed; your team sees this.");
    expect(within(local).getByRole("heading", { name: "Local" })).toBeInTheDocument();
    expect(local).toHaveTextContent("charter.local.toml · This machine only.");
  });

  it("shows the documented keys in forms, with the values the file holds", async () => {
    core({ shared: SHARED, local: LOCAL });
    const { shared, local } = await drawn();

    expect(within(shared).getByLabelText("[memory] share (deprecated)")).toHaveValue("local");
    expect(within(shared).getByLabelText("Forge 1: kind")).toHaveValue("github");
    expect(within(shared).getByLabelText("Forge 1: owner")).toHaveValue("acme");
    expect(within(shared).getByLabelText("Default workspace")).toHaveValue("");
    expect(within(local).getByLabelText("work: kind")).toHaveValue("claude");
    expect(within(local).getByLabelText("work: command")).toHaveValue("claude");
    expect(within(local).getByLabelText("work: environment")).toHaveValue(
      "CLAUDE_CONFIG_DIR=~/.work\nLANG=C",
    );
  });

  it("saves a form's change as that one key, against the text it was read as", async () => {
    const { sent } = core({ shared: SHARED, local: LOCAL });
    const { shared } = await drawn();
    const user = userEvent.setup();

    const save = within(shared).getByRole("button", { name: "Save charter.toml" });
    expect(save).toBeDisabled();
    await user.selectOptions(
      within(shared).getByLabelText("[memory] share (deprecated)"),
      "commit",
    );
    await user.click(save);

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual({
      plane: PLANE,
      which: "shared",
      base: SHARED_TEXT,
      change: {
        kind: "edits",
        edits: [
          {
            path: [{ key: "memory" }, { key: "share" }],
            value: { kind: "text", value: "commit" },
          },
        ],
      },
    });
  });

  it("sends a removed environment line as that key removed, and leaves the others alone", async () => {
    const { sent } = core({ shared: SHARED, local: LOCAL });
    const { local } = await drawn();
    const user = userEvent.setup();

    const env = within(local).getByLabelText("work: environment");
    await user.clear(env);
    await user.type(env, "CLAUDE_CONFIG_DIR=~/.work");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect((sent[0].change as { edits: unknown }).edits).toEqual([
      { path: [{ key: "harness" }, { key: "work" }, { key: "env" }, { key: "LANG" }], value: null },
    ]);
  });

  it("draws a refusal in the core's own words and keeps what was typed", async () => {
    const why =
      "profile 'work' sets GITHUB_TOKEN, which is named like a credential — charter holds no credential in a profile.";
    core({ shared: SHARED, local: LOCAL }, () => ({ kind: "refused", reasons: [why] }));
    const { local } = await drawn();
    const user = userEvent.setup();

    const env = within(local).getByLabelText("work: environment");
    await user.type(env, "\nGITHUB_TOKEN=x");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));

    const alert = await within(local).findByRole("alert");
    expect(alert).toHaveTextContent("Nothing was saved:");
    expect(alert).toHaveTextContent(why);
    expect(env).toHaveValue("CLAUDE_CONFIG_DIR=~/.work\nLANG=C\nGITHUB_TOKEN=x");
  });

  it("saves the raw view's whole text, and will not switch views over unsaved changes", async () => {
    const { sent } = core({ shared: SHARED, local: LOCAL });
    const { shared } = await drawn();
    const user = userEvent.setup();

    await user.click(within(shared).getByRole("radio", { name: "Raw TOML" }));
    const raw = within(shared).getByRole("textbox", { name: "charter.toml, as TOML" });
    expect(raw).toHaveValue(SHARED_TEXT);
    // `[[` is user-event's way of typing one `[`.
    await user.type(raw, '[[update]\nchannel = "dev"\n');

    expect(within(shared).getByRole("radio", { name: "Form" })).toBeDisabled();
    await user.click(within(shared).getByRole("button", { name: "Save charter.toml" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].change).toEqual({
      kind: "raw",
      text: `${SHARED_TEXT}[update]\nchannel = "dev"\n`,
    });
  });

  it("says a Local file that is not there yet is created by the first save, and saves it as new", async () => {
    const { sent } = core({ shared: SHARED, local: NO_LOCAL });
    const { local } = await drawn();
    const user = userEvent.setup();

    expect(local).toHaveTextContent("Not created yet: the first save creates it.");
    await user.type(within(local).getByLabelText("Default profile"), "claude");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].base).toBeNull();
    expect(sent[0].which).toBe("local");
  });

  it("reads both files again after a save", async () => {
    const { reads } = core({ shared: SHARED, local: LOCAL });
    const { shared } = await drawn();
    const user = userEvent.setup();

    await user.type(within(shared).getByLabelText("Default workspace"), "alpha");
    await user.click(within(shared).getByRole("button", { name: "Save charter.toml" }));

    await waitFor(() => expect(reads()).toBe(2));
  });

  it("says what charter ignores in a file as it stands", async () => {
    const standing =
      "1 [[forge]] block(s) failed to resolve — [[forge]] block 0: unknown forge kind 'bitbucket'";
    core({ shared: { ...SHARED, refusals: [standing] }, local: LOCAL });
    const { shared } = await drawn();

    expect(shared).toHaveTextContent("charter does not take this from the file as it stands:");
    expect(shared).toHaveTextContent(standing);
  });

  it("opens a file that is not TOML in the raw view, the only place it can be mended", async () => {
    core({
      shared: { ...SHARED, text: "[memory\n", parsed: false, fields: [] },
      local: LOCAL,
    });
    const { shared } = await drawn();

    expect(within(shared).getByRole("textbox", { name: "charter.toml, as TOML" })).toHaveValue(
      "[memory\n",
    );
    expect(within(shared).getByRole("radio", { name: "Form" })).toBeDisabled();
  });

  it("moves between Form and Raw TOML with the arrow keys, and Tab reaches every control", async () => {
    core({ shared: SHARED, local: LOCAL });
    const { shared } = await drawn();
    const user = userEvent.setup();

    const form = within(shared).getByRole("radio", { name: "Form" });
    form.focus();
    await user.keyboard("{ArrowRight}");
    expect(within(shared).getByRole("radio", { name: "Raw TOML" })).toBeChecked();
    expect(
      within(shared).getByRole("textbox", { name: "charter.toml, as TOML" }),
    ).toBeInTheDocument();
    await user.keyboard("{ArrowLeft}");
    expect(within(shared).getByRole("radio", { name: "Form" })).toBeChecked();

    // WebKit leaves a control out of the Tab order unless it says `tabIndex` (#190).
    for (const control of within(shared).getAllByRole("combobox"))
      expect(control).toHaveAttribute("tabindex", "0");
  });

  it("goes to the raw view when a file read again no longer parses", async () => {
    let both: Both = { shared: SHARED, local: LOCAL };
    mockIPC((cmd) => {
      if (cmd === "project_settings") return both;
      if (cmd === "save_project_settings") {
        both = { ...both, shared: { ...SHARED, text: "[memory\n", parsed: false, fields: [] } };
        return { kind: "saved", file: both.shared };
      }
      return undefined;
    });
    const { shared } = await drawn();
    const user = userEvent.setup();

    await user.type(within(shared).getByLabelText("Default workspace"), "alpha");
    await user.click(within(shared).getByRole("button", { name: "Save charter.toml" }));

    expect(
      await within(shared).findByRole("textbox", { name: "charter.toml, as TOML" }),
    ).toHaveValue("[memory\n");
  });

  it("points at vaults rather than either file for a secret", async () => {
    core({ shared: SHARED, local: LOCAL });
    await drawn();

    expect(screen.getByText(/Never put a secret in either file/)).toHaveTextContent(
      "vault:<vault>/<key>",
    );
  });
});

/** What `project_extensions` answers for a project with three extensions in three states. */
const EXTENSIONS: ProjectExtension[] = [
  {
    id: "acme",
    name: "Acme",
    state: "needs-approval",
    source: "shared",
    settings: [],
    ignored: [],
  },
  {
    id: "stats",
    name: "Persona statistics",
    state: "off",
    source: "local",
    settings: [
      {
        key: "window",
        title: "Window",
        kind: "choice",
        choices: ["7d", "30d"],
        default: "30d",
        value: "7d",
        source: "shared",
      },
      {
        key: "compact",
        title: "Compact",
        kind: "bool",
        choices: [],
        default: "false",
        value: "false",
        source: "default",
      },
    ],
    ignored: [
      {
        file: "charter.local.toml",
        why: "charter.local.toml sets extensions.stats.settings.nope, which stats does not declare — charter hands it nothing",
      },
    ],
  },
  { id: "solarized", name: "Solarized", state: "on", source: "default", settings: [], ignored: [] },
];

const WITH_EXTENSIONS: Both = {
  shared: {
    ...SHARED,
    fields: [
      ...SHARED.fields,
      {
        path: [{ key: "extensions" }, { key: "acme" }, { key: "enabled" }],
        value: { kind: "bool", value: true },
      },
      {
        path: [{ key: "extensions" }, { key: "stats" }, { key: "settings" }, { key: "window" }],
        value: { kind: "text", value: "7d" },
      },
    ],
  },
  local: {
    ...LOCAL,
    fields: [
      ...LOCAL.fields,
      {
        path: [{ key: "extensions" }, { key: "stats" }, { key: "enabled" }],
        value: { kind: "bool", value: false },
      },
    ],
  },
};

describe("the Extensions group (charter-app#253)", () => {
  it("lists every extension in both sections with what it is in this project and where that comes from", async () => {
    core(WITH_EXTENSIONS, undefined, EXTENSIONS);
    const { shared, local } = await drawn();

    for (const section of [shared, local]) {
      const group = within(section).getByRole("group", { name: "Extensions" });
      expect(group).toHaveTextContent(
        "Acme: needs approval here — enabled in charter.toml, and this machine has not approved it. Approve it in Extensions.",
      );
      expect(group).toHaveTextContent("Persona statistics: off — turned off in charter.local.toml");
      expect(group).toHaveTextContent("Solarized: on — installed and approved on this machine");
    }
    expect(within(shared).getByLabelText("Acme: enabled")).toHaveValue("on");
    expect(within(shared).getByLabelText("Persona statistics: enabled")).toHaveValue("");
    expect(within(local).getByLabelText("Persona statistics: enabled")).toHaveValue("off");
    expect(within(shared).getByLabelText("Persona statistics: Window")).toHaveValue("7d");
    expect(within(local).getByLabelText("Persona statistics: Window")).toHaveValue("");
    expect(local).toHaveTextContent("which stats does not declare");
  });

  it("writes a toggle to the section it is in, as true or false, and not set removes the key", async () => {
    const { sent } = core(WITH_EXTENSIONS, undefined, EXTENSIONS);
    const { shared, local } = await drawn();
    const user = userEvent.setup();

    await user.selectOptions(within(local).getByLabelText("Persona statistics: enabled"), "on");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].which).toBe("local");
    expect((sent[0].change as { edits: unknown }).edits).toEqual([
      {
        path: [{ key: "extensions" }, { key: "stats" }, { key: "enabled" }],
        value: { kind: "bool", value: true },
      },
    ]);

    await user.selectOptions(within(shared).getByLabelText("Acme: enabled"), "");
    await user.click(within(shared).getByRole("button", { name: "Save charter.toml" }));
    await waitFor(() => expect(sent).toHaveLength(2));
    expect((sent[1].change as { edits: unknown }).edits).toEqual([
      { path: [{ key: "extensions" }, { key: "acme" }, { key: "enabled" }], value: null },
    ]);
  });

  it("writes a declared setting by its kind", async () => {
    const { sent } = core(WITH_EXTENSIONS, undefined, EXTENSIONS);
    const { local } = await drawn();
    const user = userEvent.setup();

    await user.selectOptions(within(local).getByLabelText("Persona statistics: Window"), "30d");
    await user.selectOptions(within(local).getByLabelText("Persona statistics: Compact"), "on");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect((sent[0].change as { edits: unknown }).edits).toEqual([
      {
        path: [{ key: "extensions" }, { key: "stats" }, { key: "settings" }, { key: "window" }],
        value: { kind: "text", value: "30d" },
      },
      {
        path: [{ key: "extensions" }, { key: "stats" }, { key: "settings" }, { key: "compact" }],
        value: { kind: "bool", value: true },
      },
    ]);
  });

  it("asks what is in force again after a save", async () => {
    const { extensionReads } = core(WITH_EXTENSIONS, undefined, EXTENSIONS);
    const { local } = await drawn();
    const user = userEvent.setup();
    await waitFor(() => expect(extensionReads()).toBe(1));

    await user.selectOptions(within(local).getByLabelText("Solarized: enabled"), "off");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));

    await waitFor(() => expect(extensionReads()).toBe(2));
  });

  it("says so when no extension is installed or named", async () => {
    core({ shared: SHARED, local: LOCAL });
    const { shared } = await drawn();

    expect(within(shared).getByRole("group", { name: "Extensions" })).toHaveTextContent(
      "No extension is installed on this machine or named by this project.",
    );
  });
});

describe("the Theme group (charter-app#273)", () => {
  const PICKS_SOLARIZED: Both = {
    shared: {
      ...SHARED,
      fields: [
        ...SHARED.fields,
        {
          path: [{ key: "theme" }, { key: "use" }],
          value: { kind: "text", value: "solarized/Solarized Dark" },
        },
      ],
    },
    local: LOCAL,
  };
  const TURNED_OFF: ProjectTheme = {
    ...NO_PICK,
    picked: "solarized/Solarized Dark",
    file: "charter.toml",
    draws: "charter-dark",
    why: "charter.toml picks “Solarized Dark” from solarized, but solarized is off in this project — so the built-in charter-dark is drawn",
  };

  it("offers the built-ins, following the system, and every approved extension's themes, in both sections", async () => {
    core(PICKS_SOLARIZED, undefined, [], TURNED_OFF);
    const { shared, local } = await drawn();

    const sharedPick = await within(shared).findByLabelText("Theme");
    expect(sharedPick).toHaveValue("solarized/Solarized Dark");
    expect(within(local).getByLabelText("Theme")).toHaveValue("");
    expect(
      within(sharedPick)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual([
      "not set — the window's own theme",
      "charter-dark (built in)",
      "charter-light (built in)",
      "Follow the system",
      "Solarized Dark (Solarized)",
    ]);
    expect(
      within(within(local).getByLabelText("Theme")).getAllByRole("option")[0],
    ).toHaveTextContent("not set — charter.toml's pick");
  });

  it("says why a pick whose extension is off is not drawn", async () => {
    core(PICKS_SOLARIZED, undefined, [], TURNED_OFF);
    const { shared } = await drawn();

    const group = within(shared).getByRole("group", { name: "Theme" });
    await waitFor(() =>
      expect(group).toHaveTextContent(
        "charter.toml picks “Solarized Dark” from solarized, but solarized is off in this project — so the built-in charter-dark is drawn",
      ),
    );
  });

  it("writes the pick to the section it is in, and not set removes it", async () => {
    const { sent } = core(PICKS_SOLARIZED, undefined, [], TURNED_OFF);
    const { shared, local } = await drawn();
    const user = userEvent.setup();

    await user.selectOptions(await within(local).findByLabelText("Theme"), "system");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect((sent[0].change as { edits: unknown }).edits).toEqual([
      { path: [{ key: "theme" }, { key: "use" }], value: { kind: "text", value: "system" } },
    ]);

    await user.selectOptions(within(shared).getByLabelText("Theme"), "");
    await user.click(within(shared).getByRole("button", { name: "Save charter.toml" }));
    await waitFor(() => expect(sent).toHaveLength(2));
    expect((sent[1].change as { edits: unknown }).edits).toEqual([
      { path: [{ key: "theme" }, { key: "use" }], value: null },
    ]);
  });

  it("reads the theme again when what the window draws changes — an extension approved elsewhere", async () => {
    let theme = TURNED_OFF;
    core(PICKS_SOLARIZED, undefined, [], () => theme);
    const { shared } = await drawn();
    const group = within(shared).getByRole("group", { name: "Theme" });
    await waitFor(() => expect(group).toHaveTextContent("solarized is off in this project"));

    // The Extensions dialog approved or turned something on: it tells the window, not the tab.
    theme = { ...TURNED_OFF, draws: "solarized/Solarized Dark", why: null };
    projectThemeChanged(PLANE);

    await waitFor(() => expect(group).not.toHaveTextContent("solarized is off in this project"));
    expect(within(shared).getByLabelText("Theme")).toHaveAccessibleDescription(
      "Drawn in this project: Solarized Dark (Solarized). The terminal follows the window.",
    );
  });

  it("has the window ask the project's theme again after a save", async () => {
    const { themeAsks } = core(PICKS_SOLARIZED, undefined, [], TURNED_OFF);
    const { local } = await drawn();
    const user = userEvent.setup();
    // The tab asks once as it opens, for what the window draws.
    await waitFor(() => expect(themeAsks()).toBe(1));

    await user.selectOptions(await within(local).findByLabelText("Theme"), "charter-light");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));

    await waitFor(() => expect(themeAsks()).toBe(2));
  });
});

describe("the Project settings view", () => {
  it("is drawn in a view pane by its own body, asking open_view nothing", async () => {
    const asked: string[] = [];
    mockIPC((cmd) => {
      asked.push(cmd);
      if (cmd === "project_settings") return { shared: SHARED, local: LOCAL };
      return undefined;
    });
    render(
      <ViewPane
        plane={PLANE}
        view={SETTINGS_VIEW}
        title={SETTINGS_TITLE}
        waits={false}
        offered={[]}
        onOpenView={() => undefined}
        onAsk={() => undefined}
        onVaultChanged={() => undefined}
      />,
    );

    expect(await screen.findByRole("heading", { name: "Project settings" })).toBeInTheDocument();
    expect(await screen.findByTestId("settings-shared")).toBeInTheDocument();
    expect(asked).not.toContain("open_view");
  });
});

/** The ignore check's sentence for a `charter.local.toml` git would commit, as the core says it
 *  (charter-app#308): what the Local section says, and every group that shows what is in force. */
const LEFT_OUT =
  "git would commit charter.local.toml, so charter reads nothing in it until it is ignored — charter reinit adds /charter.local.toml to .gitignore.";

describe("a charter.local.toml git would carry (charter-app#319)", () => {
  const CARRIED: Both = { shared: SHARED, local: { ...LOCAL, refusals: [LEFT_OUT] } };

  it("is said once in each Shared group that shows what is in force, in the Local section's words", async () => {
    core(CARRIED, undefined, EXTENSIONS, NO_PICK, LEFT_OUT);
    const { shared } = await drawn();

    for (const name of ["Plane", "Repos", "Extensions", "Theme"]) {
      const group = within(shared).getByRole("group", { name });
      await waitFor(() => expect(within(group).getAllByText(LEFT_OUT)).toHaveLength(1));
    }
  });

  it("is said once in the Local section, at its head, and not again in each of its groups", async () => {
    core(CARRIED, undefined, EXTENSIONS, NO_PICK, LEFT_OUT);
    const { shared, local } = await drawn();
    const said = within(shared).getByRole("group", { name: "Extensions" });
    await waitFor(() => expect(said).toHaveTextContent(LEFT_OUT));

    expect(within(local).getAllByText(LEFT_OUT)).toHaveLength(1);
    expect(within(local).getByRole("group", { name: "Extensions" })).not.toHaveTextContent(
      LEFT_OUT,
    );
  });

  it("is not said while charter reads the file", async () => {
    core({ shared: SHARED, local: LOCAL }, undefined, EXTENSIONS);
    const { shared } = await drawn();
    await within(shared).findByLabelText("Acme: enabled");

    expect(screen.queryByText(/charter reads nothing in it/)).toBeNull();
  });
});

/** A plane whose Shared file sets a mode and signing and Local overrides the mode and sets a
 *  branch, with two catalogued repos, one configured (charter-app#300). */
const SAVES: Both = {
  shared: {
    ...SHARED,
    fields: [
      ...SHARED.fields,
      { path: [{ key: "plane" }, { key: "mode" }], value: { kind: "text", value: "pr" } },
      { path: [{ key: "plane" }, { key: "sign" }], value: { kind: "bool", value: true } },
      {
        path: [{ key: "repos" }, { key: "api" }, { key: "mode" }],
        value: { kind: "text", value: "push" },
      },
    ],
  },
  local: {
    ...LOCAL,
    fields: [
      ...LOCAL.fields,
      { path: [{ key: "plane" }, { key: "mode" }], value: { kind: "text", value: "push" } },
      { path: [{ key: "plane" }, { key: "branch" }], value: { kind: "text", value: "trunk" } },
      {
        path: [{ key: "repos" }, { key: "api" }, { key: "autosave" }],
        value: { kind: "bool", value: true },
      },
    ],
  },
};

const SAVES_IN_FORCE: SavingInForce = {
  plane: {
    ...NO_SAVING.plane,
    mode: said("push", "local"),
    branch: said("trunk", "local"),
    sign: said("on", "shared"),
  },
  repos: [
    {
      name: "web",
      mode: said("pr"),
      branch: said(null),
      sign: said("off"),
      autosave: said("off"),
      autosave_after: said("1m"),
    },
    {
      name: "api",
      mode: said("push", "shared"),
      branch: said(null),
      sign: said("off"),
      autosave: said("on", "local"),
      autosave_after: said("1m"),
    },
  ],
  plane_left_out: null,
  repos_left_out: null,
};

describe("the Plane group (charter-app#300, ADR 0051)", () => {
  it("has every [plane] save key in both sections, with the value each file holds", async () => {
    core(SAVES, undefined, [], NO_PICK, null, SAVES_IN_FORCE);
    const { shared, local } = await drawn();

    const sharedPlane = within(shared).getByRole("group", { name: "Plane" });
    const localPlane = within(local).getByRole("group", { name: "Plane" });
    expect(within(sharedPlane).getByLabelText("Mode")).toHaveValue("pr");
    expect(within(localPlane).getByLabelText("Mode")).toHaveValue("push");
    expect(within(sharedPlane).getByLabelText("Target branch")).toHaveValue("");
    expect(within(localPlane).getByLabelText("Target branch")).toHaveValue("trunk");
    expect(within(sharedPlane).getByLabelText("Save branch")).toHaveValue("");
    expect(within(sharedPlane).getByLabelText("Sign commits")).toHaveValue("on");
    expect(within(localPlane).getByLabelText("Sign commits")).toHaveValue("");
    expect(within(sharedPlane).getByLabelText("Auto-save")).toHaveValue("");
    expect(within(sharedPlane).getByLabelText("Auto-save after")).toHaveValue("");
    expect(
      within(within(sharedPlane).getByLabelText("Mode"))
        .getAllByRole("option")
        .map((option) => option.getAttribute("value")),
    ).toEqual(["", "off", "commit", "push", "pr", "pr-merge"]);
  });

  it("says beside each key which file decided it, and marks a Shared value Local overrides", async () => {
    core(SAVES, undefined, [], NO_PICK, null, SAVES_IN_FORCE);
    const { shared, local } = await drawn();

    const sharedPlane = within(shared).getByRole("group", { name: "Plane" });
    const localPlane = within(local).getByRole("group", { name: "Plane" });
    await waitFor(() =>
      expect(within(sharedPlane).getByLabelText("Mode")).toHaveAccessibleDescription(
        /In this project: push, from charter\.local\.toml — overriding the value here\./,
      ),
    );
    expect(within(localPlane).getByLabelText("Mode")).toHaveAccessibleDescription(
      /In this project: push, from charter\.local\.toml\./,
    );
    expect(within(localPlane).getByLabelText("Mode")).not.toHaveAccessibleDescription(/overriding/);
    expect(within(sharedPlane).getByLabelText("Sign commits")).toHaveAccessibleDescription(
      /In this project: on, from charter\.toml\./,
    );
    expect(within(sharedPlane).getByLabelText("Auto-save after")).toHaveAccessibleDescription(
      /In this project: 1m, its default\./,
    );
    // Local decided a key Shared does not hold: nothing here is overridden.
    expect(within(sharedPlane).getByLabelText("Target branch")).toHaveAccessibleDescription(
      /In this project: trunk, from charter\.local\.toml\./,
    );
    expect(within(sharedPlane).getByLabelText("Target branch")).not.toHaveAccessibleDescription(
      /overriding/,
    );
  });

  it("says what no value means where no file sets one", async () => {
    core({ shared: SHARED, local: LOCAL });
    const { shared } = await drawn();

    const plane = within(shared).getByRole("group", { name: "Plane" });
    await waitFor(() =>
      expect(within(plane).getByLabelText("Mode")).toHaveAccessibleDescription(
        /In this project: not set — the Saving view asks once, before anything is pushed\./,
      ),
    );
    expect(within(plane).getByLabelText("Target branch")).toHaveAccessibleDescription(
      /In this project: the branch the plane has checked out, its default\./,
    );
    expect(within(plane).getByLabelText("Save branch")).toHaveAccessibleDescription(
      /In this project: charter\/save\/<this machine's name>, its default\./,
    );
  });

  it("writes each key to the section it is in, by its kind, and empty removes it", async () => {
    const { sent } = core(SAVES, undefined, [], NO_PICK, null, SAVES_IN_FORCE);
    const { shared, local } = await drawn();
    const user = userEvent.setup();

    const localPlane = within(local).getByRole("group", { name: "Plane" });
    await user.selectOptions(within(localPlane).getByLabelText("Mode"), "pr-merge");
    await user.selectOptions(within(localPlane).getByLabelText("Auto-save"), "off");
    await user.clear(within(localPlane).getByLabelText("Target branch"));
    await user.type(within(localPlane).getByLabelText("Auto-save after"), "2m");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].which).toBe("local");
    expect((sent[0].change as { edits: unknown }).edits).toEqual([
      { path: [{ key: "plane" }, { key: "mode" }], value: { kind: "text", value: "pr-merge" } },
      { path: [{ key: "plane" }, { key: "branch" }], value: null },
      { path: [{ key: "plane" }, { key: "autosave" }], value: { kind: "bool", value: false } },
      {
        path: [{ key: "plane" }, { key: "autosave_after" }],
        value: { kind: "text", value: "2m" },
      },
    ]);

    const sharedPlane = within(shared).getByRole("group", { name: "Plane" });
    await user.type(within(sharedPlane).getByLabelText("Save branch"), "charter/save/team");
    await user.click(within(shared).getByRole("button", { name: "Save charter.toml" }));
    await waitFor(() => expect(sent).toHaveLength(2));
    expect((sent[1].change as { edits: unknown }).edits).toEqual([
      {
        path: [{ key: "plane" }, { key: "save_branch" }],
        value: { kind: "text", value: "charter/save/team" },
      },
    ]);
  });

  it("draws the reader's refusal of a value in its words", async () => {
    const why =
      "plane.autosave_after in charter.toml is not a quiet period — a whole number of seconds or minutes";
    core(SAVES, () => ({ kind: "refused", reasons: [why] }), [], NO_PICK, null, SAVES_IN_FORCE);
    const { shared } = await drawn();
    const user = userEvent.setup();

    const plane = within(shared).getByRole("group", { name: "Plane" });
    await user.type(within(plane).getByLabelText("Auto-save after"), "soon");
    await user.click(within(shared).getByRole("button", { name: "Save charter.toml" }));

    const alert = await within(shared).findByRole("alert");
    expect(alert).toHaveTextContent(why);
    expect(within(plane).getByLabelText("Auto-save after")).toHaveValue("soon");
  });

  /** Both files, with `charter.toml` holding `[memory] share = word`. */
  const sharing = (word: string): Both => ({
    shared: {
      ...SHARED,
      fields: SHARED.fields.map((field) =>
        field.path[0]?.key === "memory"
          ? { ...field, value: { kind: "text" as const, value: word } }
          : field,
      ),
    },
    local: LOCAL,
  });
  const SHARE_HINT =
    "Deprecated: read as Mode — commit and push carry over, local says nothing — only while neither file sets Mode. Set Mode instead.";

  it("shows [memory] share in Shared only, as the deprecated alias of Mode, marked in force", async () => {
    core(sharing("commit"), undefined, [], NO_PICK, null, {
      ...NO_SAVING,
      plane: { ...NO_SAVING.plane, mode: said("commit", "shared"), from_share: true },
    });
    const { shared, local } = await drawn();

    const plane = within(shared).getByRole("group", { name: "Plane" });
    const share = within(plane).getByLabelText("[memory] share (deprecated)");
    expect(share).toHaveValue("commit");
    await waitFor(() =>
      expect(share).toHaveAccessibleDescription(`${SHARE_HINT} In force as Mode.`),
    );
    expect(within(local).queryByLabelText("[memory] share (deprecated)")).toBeNull();
    expect(within(plane).getByLabelText("Mode")).toHaveAccessibleDescription(
      /In this project: commit, from \[memory\] share in charter\.toml, the deprecated alias\./,
    );
  });

  it("marks a [memory] share that a Mode in either file overrides as not in force", async () => {
    core(sharing("push"), undefined, [], NO_PICK, null, {
      ...NO_SAVING,
      plane: { ...NO_SAVING.plane, mode: said("pr", "local") },
    });
    const { shared } = await drawn();

    const share = within(shared).getByLabelText("[memory] share (deprecated)");
    await waitFor(() =>
      expect(share).toHaveAccessibleDescription(
        `${SHARE_HINT} Not in force — Mode from charter.local.toml wins.`,
      ),
    );
  });

  it("gives a [memory] share of local, which says nothing, no marker", async () => {
    core(sharing("local"), undefined, [], NO_PICK, null, {
      ...NO_SAVING,
      plane: { ...NO_SAVING.plane, mode: said("pr", "shared") },
    });
    const { shared } = await drawn();

    const plane = within(shared).getByRole("group", { name: "Plane" });
    await waitFor(() =>
      expect(within(plane).getByLabelText("Mode")).toHaveAccessibleDescription(/In this project/),
    );
    expect(within(plane).getByLabelText("[memory] share (deprecated)")).toHaveAccessibleDescription(
      SHARE_HINT,
    );
  });

  it("says where [plane] worktrees is", async () => {
    core({ shared: SHARED, local: LOCAL });
    const { shared } = await drawn();

    expect(within(shared).getByRole("group", { name: "Plane" })).toHaveTextContent(
      "[plane] worktrees is under General.",
    );
  });

  it("says what is in force could not be read, rather than drawing a marker", async () => {
    core(SAVES, undefined, [], NO_PICK, null, { trouble: "the plane is not held" });
    const { shared } = await drawn();

    const plane = within(shared).getByRole("group", { name: "Plane" });
    await waitFor(() =>
      expect(plane).toHaveTextContent(
        "What this project uses could not be read: the plane is not held",
      ),
    );
    expect(within(plane).getByLabelText("Mode")).not.toHaveAccessibleDescription(/In this project/);
  });

  it("asks what is in force again after a save", async () => {
    const { savingReads } = core(SAVES, undefined, [], NO_PICK, null, SAVES_IN_FORCE);
    const { local } = await drawn();
    const user = userEvent.setup();
    await waitFor(() => expect(savingReads()).toBe(1));

    const plane = within(local).getByRole("group", { name: "Plane" });
    await user.selectOptions(within(plane).getByLabelText("Mode"), "commit");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));

    await waitFor(() => expect(savingReads()).toBe(2));
  });
});

describe("the Repos group (charter-app#300, ADR 0051)", () => {
  it("has a row per catalogued repo in both sections, each key marked with the file that decided it", async () => {
    core(SAVES, undefined, [], NO_PICK, null, SAVES_IN_FORCE);
    const { shared, local } = await drawn();

    const sharedRepos = within(shared).getByRole("group", { name: "Repos" });
    const localRepos = within(local).getByRole("group", { name: "Repos" });
    await waitFor(() =>
      expect(within(sharedRepos).getByLabelText("api: mode")).toHaveValue("push"),
    );
    for (const name of ["web", "api"])
      for (const key of ["mode", "branch", "sign", "auto-save", "auto-save after"]) {
        expect(within(sharedRepos).getByLabelText(`${name}: ${key}`)).toBeInTheDocument();
        expect(within(localRepos).getByLabelText(`${name}: ${key}`)).toBeInTheDocument();
      }
    expect(within(sharedRepos).queryByLabelText("api: save branch")).toBeNull();
    expect(within(localRepos).getByLabelText("api: mode")).toHaveValue("");
    expect(within(localRepos).getByLabelText("api: auto-save")).toHaveValue("on");
    expect(within(sharedRepos).getByLabelText("api: mode")).toHaveAccessibleDescription(
      /In this project: push, from charter\.toml\./,
    );
    expect(within(sharedRepos).getByLabelText("web: mode")).toHaveAccessibleDescription(
      /In this project: pr, its default\./,
    );
    expect(within(sharedRepos).getByLabelText("web: branch")).toHaveAccessibleDescription(
      /In this project: the repo's default branch, its default\./,
    );
    expect(within(localRepos).getByLabelText("api: auto-save")).toHaveAccessibleDescription(
      /In this project: on, from charter\.local\.toml\./,
    );
  });

  it("marks a Shared repo value that Local overrides", async () => {
    core(
      {
        ...SAVES,
        shared: {
          ...SAVES.shared,
          fields: [
            ...SAVES.shared.fields,
            {
              path: [{ key: "repos" }, { key: "api" }, { key: "autosave" }],
              value: { kind: "bool", value: false },
            },
          ],
        },
      },
      undefined,
      [],
      NO_PICK,
      null,
      SAVES_IN_FORCE,
    );
    const { shared } = await drawn();

    const repos = within(shared).getByRole("group", { name: "Repos" });
    await waitFor(() =>
      expect(within(repos).getByLabelText("api: auto-save")).toHaveAccessibleDescription(
        /In this project: on, from charter\.local\.toml — overriding the value here\./,
      ),
    );
  });

  it("writes a repo's key under [repos.<name>] in the section it is in", async () => {
    const { sent } = core(SAVES, undefined, [], NO_PICK, null, SAVES_IN_FORCE);
    const { local } = await drawn();
    const user = userEvent.setup();

    const repos = within(local).getByRole("group", { name: "Repos" });
    await user.selectOptions(await within(repos).findByLabelText("web: mode"), "pr-merge");
    await user.selectOptions(within(repos).getByLabelText("web: sign"), "on");
    await user.type(within(repos).getByLabelText("web: branch"), "develop");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));

    await waitFor(() => expect(sent).toHaveLength(1));
    expect((sent[0].change as { edits: unknown }).edits).toEqual([
      {
        path: [{ key: "repos" }, { key: "web" }, { key: "mode" }],
        value: { kind: "text", value: "pr-merge" },
      },
      {
        path: [{ key: "repos" }, { key: "web" }, { key: "branch" }],
        value: { kind: "text", value: "develop" },
      },
      {
        path: [{ key: "repos" }, { key: "web" }, { key: "sign" }],
        value: { kind: "bool", value: true },
      },
    ]);
  });

  it("says the repos could not be read when the survey failed, not that there are none", async () => {
    core(SAVES, undefined, [], NO_PICK, null, { trouble: "the plane is not held" });
    const { shared } = await drawn();

    const repos = within(shared).getByRole("group", { name: "Repos" });
    await waitFor(() =>
      expect(repos).toHaveTextContent(
        "The repos and how each is saved could not be read: the plane is not held",
      ),
    );
    expect(repos).not.toHaveTextContent("No repo is catalogued");
  });

  it("says Local was left out only in the group whose table it set", async () => {
    core(
      { shared: SHARED, local: { ...LOCAL, refusals: [LEFT_OUT] } },
      undefined,
      [],
      NO_PICK,
      null,
      { ...SAVES_IN_FORCE, repos_left_out: LEFT_OUT },
    );
    const { shared } = await drawn();

    const repos = within(shared).getByRole("group", { name: "Repos" });
    await waitFor(() => expect(within(repos).getAllByText(LEFT_OUT)).toHaveLength(1));
    expect(within(shared).getByRole("group", { name: "Plane" })).not.toHaveTextContent(LEFT_OUT);
  });

  it("says so when no repo is catalogued or named", async () => {
    core({ shared: SHARED, local: LOCAL });
    const { shared } = await drawn();

    expect(within(shared).getByRole("group", { name: "Repos" })).toHaveTextContent(
      "No repo is catalogued in inventory/repos.json or named by either file.",
    );
  });
});
