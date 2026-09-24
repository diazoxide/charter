import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ProjectSettings } from "./ProjectSettings";
import { ViewPane } from "./Views";
import { SETTINGS_TITLE, SETTINGS_VIEW } from "./tabs";
import type {
  ProjectExtension,
  ProjectTheme,
  ProjectSettings as Both,
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
  source: "default",
  draws: null,
  why: null,
  ignored: [],
};

/** The core, as a mock: `project_settings` answers `both`, `project_extensions` answers
 *  `extensions`, and `save_project_settings` answers `saved` and records what it was sent. */
function core(
  both: Both,
  saved?: (sent: Record<string, unknown>) => SettingsSaved,
  extensions: ProjectExtension[] = [],
  theme: ProjectTheme = NO_PICK,
) {
  const sent: Record<string, unknown>[] = [];
  let reads = 0;
  let extensionReads = 0;
  let themeAsks = 0;
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    if (cmd === "project_settings") {
      reads += 1;
      return both;
    }
    if (cmd === "project_extensions") {
      extensionReads += 1;
      return extensions;
    }
    if (cmd === "extensions_on") return [];
    if (cmd === "project_theme") return theme;
    if (cmd === "project_theme_drawn") {
      themeAsks += 1;
      return theme.draws;
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

    expect(within(shared).getByLabelText("How far a memory travels")).toHaveValue("local");
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
    await user.selectOptions(within(shared).getByLabelText("How far a memory travels"), "commit");
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
    source: "shared",
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

  it("has the window ask the project's theme again after a save", async () => {
    const { themeAsks } = core(PICKS_SOLARIZED, undefined, [], TURNED_OFF);
    const { local } = await drawn();
    const user = userEvent.setup();

    await user.selectOptions(await within(local).findByLabelText("Theme"), "charter-light");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));

    await waitFor(() => expect(themeAsks()).toBe(1));
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
      />,
    );

    expect(await screen.findByRole("heading", { name: "Project settings" })).toBeInTheDocument();
    expect(await screen.findByTestId("settings-shared")).toBeInTheDocument();
    expect(asked).not.toContain("open_view");
  });
});
