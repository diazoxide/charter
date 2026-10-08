import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SettingsTab } from "./SettingsTab";
import { projectThemeChanged } from "../projectTheme";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import type {
  ProjectExtension,
  ProjectTheme,
  SandboxState,
  SavingInForce,
  SettingsEdit,
  SettingsField,
  SettingsFile,
  SettingsWhich,
} from "../bindings";

/**
 * **What the old Project settings page held, at the Project level** (SE-19, #1169): the page
 * (charter-app#252) is retired, and these are its tests carried over to the Settings tab — what
 * each group says of the files and of what is in force, and the keys a change writes. The
 * Extensions (charter-app#253), Theme (#273), Plane and Repos (#300), left-out Local (#319) and
 * sandbox (ADR 0067 §7) behaviour each page held is held here, on the groups V89h put them in.
 *
 * Its Shared and Local sections are one form now: a setting is kept in the file it belongs in,
 * and a key in the other file is reached with Edit as TOML (`RawToml.test.tsx`) until SE-18
 * gives each value its file choice. The core is the mock; what it refuses, and in which words,
 * is `purlis_core::settings`'s tests.
 */

const PLANE = "/home/dev/plane";

const field = (keys: (string | number)[], value: SettingsField["value"]): SettingsField => ({
  path: keys.map((one) => (typeof one === "number" ? { index: one } : { key: one })),
  value,
});

const SHARED: SettingsFile = {
  which: "shared",
  file: "charter.toml",
  exists: true,
  text: `# the team's\n[[forge]]\nkind = "github"\nowner = "acme"\n\n[memory]\nshare = "local"\n`,
  refusals: [],
  parsed: true,
  fields: [
    field(["forge", 0, "kind"], { kind: "text", value: "github" }),
    field(["forge", 0, "owner"], { kind: "text", value: "acme" }),
    field(["memory", "share"], { kind: "text", value: "local" }),
  ],
};

const LOCAL: SettingsFile = {
  which: "local",
  file: "charter.local.toml",
  exists: true,
  text: `[harness.work]\nkind = "claude"\ncommand = ["claude"]\nenv = { CLAUDE_CONFIG_DIR = "~/.work", LANG = "C" }\n`,
  refusals: [],
  parsed: true,
  fields: [
    field(["harness", "work", "kind"], { kind: "text", value: "claude" }),
    field(["harness", "work", "command"], { kind: "list", value: ["claude"] }),
    field(["harness", "work", "env", "CLAUDE_CONFIG_DIR"], { kind: "text", value: "~/.work" }),
    field(["harness", "work", "env", "LANG"], { kind: "text", value: "C" }),
  ],
  // The core lists the profile as an entry of the profiles collection, with a page (ST-4).
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

/** No `[plane]` or `[repos]`, and no inventory: every default, and no repo. */
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

const SANDBOX_OFF: SandboxState = {
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

/** A file as the core would answer it after `edits`: each edited key set or gone. */
function applied(file: SettingsFile, edits: readonly SettingsEdit[]): SettingsFile {
  let fields = [...file.fields];
  for (const edit of edits) {
    const at = JSON.stringify(edit.path);
    fields = fields.filter((one) => JSON.stringify(one.path) !== at);
    if (edit.value !== null) fields.push({ path: edit.path, value: edit.value });
  }
  return { ...file, exists: true, fields, text: `${file.text}# written\n` };
}

type Sent = { which: SettingsWhich; base: string | null; edits: SettingsEdit[] };

/** The core, as a mock. `leftOut` is why every answer says `charter.local.toml` was left out. */
function core({
  shared = SHARED,
  local = LOCAL,
  extensions = [] as ProjectExtension[],
  theme = NO_PICK as ProjectTheme | (() => ProjectTheme),
  leftOut = null as string | null,
  saving = NO_SAVING as SavingInForce | { trouble: string },
  sandbox = SANDBOX_OFF,
  refuse = undefined as string[] | undefined,
} = {}) {
  const files: Record<SettingsWhich, SettingsFile> = { shared, local };
  const themeNow = () => (typeof theme === "function" ? theme() : theme);
  const sent: Sent[] = [];
  const count = { extensions: 0, saving: 0, themeDrawn: 0 };
  mockIPC(
    (cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "project_settings":
          return { shared: files.shared, local: files.local };
        case "project_extensions":
          count.extensions += 1;
          return { extensions, local_left_out: leftOut };
        case "project_harness_plugins":
          return [];
        case "project_saving_in_force":
          count.saving += 1;
          if ("trouble" in saving) throw saving.trouble;
          return leftOut === null
            ? saving
            : { ...saving, plane_left_out: leftOut, repos_left_out: leftOut };
        case "project_theme":
          return { ...themeNow(), local_left_out: leftOut };
        case "project_theme_drawn":
          count.themeDrawn += 1;
          return themeNow().draws;
        case "extensions_on":
          return [];
        case "sandbox_state":
          return sandbox;
        case "save_project_settings": {
          const which = given.which as SettingsWhich;
          const change = given.change as { kind: "edits"; edits: SettingsEdit[] };
          sent.push({ which, base: given.base as string | null, edits: change.edits });
          if (refuse) return { kind: "refused", reasons: refuse };
          files[which] = applied(files[which], change.edits);
          return { kind: "saved", file: files[which] };
        }
      }
      return undefined;
    },
    { shouldMockEvents: true },
  );
  return { sent, count };
}

beforeEach(() => {
  forgetThisLaunch();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: "/home/dev/.config/charter/layout.json", found: false, document: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

const nav = () => screen.getByRole("navigation", { name: "Groups" });

/** The tab at the Project level, read, with `name`'s group on the right. */
async function at(name = "General") {
  render(<SettingsTab plane={PLANE} level="project" />);
  const button = await within(await screen.findByRole("navigation", { name: "Groups" })).findByRole(
    "button",
    { name },
  );
  await userEvent.click(button);
  return screen.getByRole("region", { name });
}

/** Leaves the field with the focus, which writes what was typed into it. */
const leave = () => userEvent.tab();

describe("the files, at the Project level", () => {
  it("shows the documented keys with the values each file holds", async () => {
    core();
    const saving = await at("Saving");
    expect(within(saving).getByLabelText("[memory] share (deprecated)")).toHaveValue("local");

    await userEvent.click(within(nav()).getByRole("button", { name: "Forges" }));
    expect(screen.getByLabelText("Forge 1: kind")).toHaveValue("github");
    expect(screen.getByLabelText("Forge 1: owner")).toHaveValue("acme");
    await userEvent.click(within(nav()).getByRole("button", { name: "General" }));
    expect(screen.getByLabelText("Default workspace")).toHaveValue("");
    // A profile's rows are on its own page (ST-4).
    await userEvent.click(within(nav()).getByRole("button", { name: "work" }));
    expect(screen.getByLabelText("work: kind")).toHaveValue("claude");
    expect(screen.getByLabelText("work: command")).toHaveValue("claude");
    expect(screen.getByLabelText("work: environment")).toHaveValue(
      "CLAUDE_CONFIG_DIR=~/.work\nLANG=C",
    );
  });

  it("sends a removed environment line as that key removed, and leaves the others alone", async () => {
    const { sent } = core();
    await at("work");

    const env = screen.getByLabelText("work: environment");
    await userEvent.clear(env);
    await userEvent.type(env, "CLAUDE_CONFIG_DIR=~/.work");
    await leave();

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({ which: "local", base: LOCAL.text });
    expect(sent[0].edits).toEqual([
      { path: [{ key: "harness" }, { key: "work" }, { key: "env" }, { key: "LANG" }], value: null },
    ]);
  });

  it("says what charter ignores in a file as it stands", async () => {
    const standing =
      "1 [[forge]] block(s) failed to resolve — [[forge]] block 0: unknown forge kind 'bitbucket'";
    core({ shared: { ...SHARED, refusals: [standing] } });
    await at();

    expect(
      screen.getByText("purlis does not take this from the files as they stand:"),
    ).toBeVisible();
    expect(screen.getByText(`charter.toml: ${standing}`)).toBeVisible();
  });

  it("points at vaults rather than either file for a secret", async () => {
    core();
    await at();

    expect(screen.getByText(/Never put a secret in its files/)).toHaveTextContent(
      "vault:<vault>/<key>",
    );
  });

  it("puts every choice in the Tab order (#190)", async () => {
    core();
    await at("Saving");

    for (const control of screen.getAllByRole("combobox"))
      expect(control).toHaveAttribute("tabindex", "0");
  });
});

/** Three extensions in three states (charter-app#253). */
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
        file: "charter.toml",
        why: "charter.toml sets extensions.stats.settings.nope, which stats does not declare — purlis hands it nothing",
      },
    ],
  },
  { id: "solarized", name: "Solarized", state: "on", source: "default", settings: [], ignored: [] },
];

const WITH_EXTENSIONS: SettingsFile = {
  ...SHARED,
  fields: [
    ...SHARED.fields,
    field(["extensions", "acme", "enabled"], { kind: "bool", value: true }),
    field(["extensions", "stats", "settings", "window"], { kind: "text", value: "7d" }),
  ],
};

describe("the Extensions group (charter-app#253)", () => {
  it("lists every extension with what it is in this project and where that comes from", async () => {
    core({ shared: WITH_EXTENSIONS, extensions: EXTENSIONS });
    const group = await at("Extensions");

    expect(group).toHaveTextContent(
      "Acme: needs approval here — enabled in charter.toml, and this machine has not approved it. Approve it in Extensions.",
    );
    expect(group).toHaveTextContent("Persona statistics: off — turned off in charter.local.toml");
    expect(group).toHaveTextContent("Solarized: on — installed and approved on this machine");
    expect(group).toHaveTextContent("which stats does not declare");
    expect(within(group).getByLabelText("Acme: enabled")).toHaveValue("on");
    expect(within(group).getByLabelText("Persona statistics: enabled")).toHaveValue("");
    expect(within(group).getByLabelText("Persona statistics: Window")).toHaveValue("7d");
  });

  it("writes a toggle as true or false, and not set removes the key", async () => {
    const { sent } = core({ shared: WITH_EXTENSIONS, extensions: EXTENSIONS });
    await at("Extensions");

    await userEvent.selectOptions(screen.getByLabelText("Persona statistics: enabled"), "on");
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({
      which: "shared",
      edits: [
        {
          path: [{ key: "extensions" }, { key: "stats" }, { key: "enabled" }],
          value: { kind: "bool", value: true },
        },
      ],
    });

    await userEvent.selectOptions(screen.getByLabelText("Acme: enabled"), "");
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1].edits).toEqual([
      { path: [{ key: "extensions" }, { key: "acme" }, { key: "enabled" }], value: null },
    ]);
  });

  it("writes a declared setting by its kind", async () => {
    const { sent } = core({ shared: WITH_EXTENSIONS, extensions: EXTENSIONS });
    await at("Extensions");

    await userEvent.selectOptions(screen.getByLabelText("Persona statistics: Window"), "30d");
    await userEvent.selectOptions(screen.getByLabelText("Persona statistics: Compact"), "on");

    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent.map((one) => one.edits)).toEqual([
      [
        {
          path: [{ key: "extensions" }, { key: "stats" }, { key: "settings" }, { key: "window" }],
          value: { kind: "text", value: "30d" },
        },
      ],
      [
        {
          path: [{ key: "extensions" }, { key: "stats" }, { key: "settings" }, { key: "compact" }],
          value: { kind: "bool", value: true },
        },
      ],
    ]);
  });

  it("asks what is in force again after a change", async () => {
    const { count } = core({ shared: WITH_EXTENSIONS, extensions: EXTENSIONS });
    await at("Extensions");
    await waitFor(() => expect(count.extensions).toBe(1));

    await userEvent.selectOptions(screen.getByLabelText("Solarized: enabled"), "off");

    await waitFor(() => expect(count.extensions).toBe(2));
  });
});

describe("the theme, in Appearance (charter-app#273)", () => {
  const PICKS_SOLARIZED: SettingsFile = {
    ...SHARED,
    fields: [
      ...SHARED.fields,
      field(["theme", "use"], { kind: "text", value: "solarized/Solarized Dark" }),
    ],
  };
  const TURNED_OFF: ProjectTheme = {
    ...NO_PICK,
    picked: "solarized/Solarized Dark",
    file: "charter.toml",
    draws: "charter-dark",
    why: "charter.toml picks “Solarized Dark” from solarized, but solarized is off in this project — so the built-in charter-dark is drawn",
  };

  it("offers the built-ins, following the system, and every approved extension's themes", async () => {
    core({ shared: PICKS_SOLARIZED, theme: TURNED_OFF });
    await at("Appearance");

    const pick = await screen.findByLabelText("Theme");
    expect(pick).toHaveValue("solarized/Solarized Dark");
    expect(
      within(pick)
        .getAllByRole("option")
        .map((option) => option.textContent),
    ).toEqual([
      "not set — the window's own theme",
      "charter-dark (built in)",
      "charter-light (built in)",
      "Follow the system",
      "Solarized Dark (Solarized)",
    ]);
  });

  it("says why a pick whose extension is off is not drawn", async () => {
    core({ shared: PICKS_SOLARIZED, theme: TURNED_OFF });
    const group = await at("Appearance");

    await waitFor(() =>
      expect(group).toHaveTextContent(
        "charter.toml picks “Solarized Dark” from solarized, but solarized is off in this project — so the built-in charter-dark is drawn",
      ),
    );
  });

  it("writes the pick, and not set removes it", async () => {
    const { sent } = core({ shared: PICKS_SOLARIZED, theme: TURNED_OFF });
    await at("Appearance");

    await userEvent.selectOptions(await screen.findByLabelText("Theme"), "system");
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].edits).toEqual([
      { path: [{ key: "theme" }, { key: "use" }], value: { kind: "text", value: "system" } },
    ]);

    await userEvent.selectOptions(screen.getByLabelText("Theme"), "");
    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1].edits).toEqual([{ path: [{ key: "theme" }, { key: "use" }], value: null }]);
  });

  it("reads the theme again when what the window draws changes — an extension approved elsewhere", async () => {
    let theme = TURNED_OFF;
    core({ shared: PICKS_SOLARIZED, theme: () => theme });
    const group = await at("Appearance");
    await waitFor(() => expect(group).toHaveTextContent("solarized is off in this project"));

    // The Extensions dialog approved or turned something on: it tells the window, not the tab.
    theme = { ...TURNED_OFF, draws: "solarized/Solarized Dark", why: null };
    projectThemeChanged(PLANE);

    await waitFor(() => expect(group).not.toHaveTextContent("solarized is off in this project"));
    expect(screen.getByLabelText("Theme")).toHaveAccessibleDescription(
      /Drawn in this project: Solarized Dark \(Solarized\)\. The terminal follows the window\./,
    );
  });

  it("has the window ask the project's theme again after a change", async () => {
    const { count } = core({ shared: PICKS_SOLARIZED, theme: TURNED_OFF });
    await at("Appearance");
    const before = count.themeDrawn;

    await userEvent.selectOptions(await screen.findByLabelText("Theme"), "charter-light");

    await waitFor(() => expect(count.themeDrawn).toBeGreaterThan(before));
  });
});

/** The ignore check's sentence for a `charter.local.toml` git would commit, as the core says it
 *  (charter-app#308). */
const LEFT_OUT =
  "git would commit charter.local.toml, so purlis reads nothing in it until it is ignored — purlis doctor --fix local-ignore adds /charter.local.toml to .gitignore.";

describe("a charter.local.toml git would carry (charter-app#319)", () => {
  it("is said once in each group that shows what is in force", async () => {
    core({
      local: { ...LOCAL, refusals: [LEFT_OUT] },
      shared: WITH_EXTENSIONS,
      extensions: EXTENSIONS,
      leftOut: LEFT_OUT,
    });
    await at();

    for (const name of ["Saving", "Extensions", "Appearance"]) {
      await userEvent.click(within(nav()).getByRole("button", { name }));
      const group = screen.getByRole("region", { name });
      await waitFor(() => expect(within(group).getAllByText(LEFT_OUT)).toHaveLength(1));
    }
  });

  it("is not said while charter reads the file", async () => {
    core({ shared: WITH_EXTENSIONS, extensions: EXTENSIONS });
    await at("Extensions");
    await screen.findByLabelText("Acme: enabled");

    expect(screen.queryByText(/charter reads nothing in it/)).toBeNull();
  });
});

/** Shared sets a mode and signing, and Local overrides the mode and sets a branch; two
 *  catalogued repos, one configured (charter-app#300). */
const SAVES_SHARED: SettingsFile = {
  ...SHARED,
  fields: [
    ...SHARED.fields,
    field(["plane", "mode"], { kind: "text", value: "pr" }),
    field(["plane", "sign"], { kind: "bool", value: true }),
    field(["repos", "api", "mode"], { kind: "text", value: "push" }),
  ],
};

/** The row a setting is drawn in, by its control's label. */
function rowOf(label: string): HTMLElement {
  const row = screen.getByLabelText(label).closest(".ui-setting-row");
  if (!(row instanceof HTMLElement)) throw new Error(`no row for ${label}`);
  return row;
}

/** A Local file holding what {@link SAVES_IN_FORCE} says it decided. */
const LOCAL_SAVES: SettingsFile = {
  ...LOCAL,
  fields: [
    ...LOCAL.fields,
    field(["plane", "mode"], { kind: "text", value: "push" }),
    field(["plane", "branch"], { kind: "text", value: "trunk" }),
    field(["repos", "api", "autosave"], { kind: "bool", value: true }),
  ],
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
      mode: said("off"),
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

describe("the project's own save keys, in Saving (charter-app#300, ADR 0051)", () => {
  it("has every [plane] save key, with the value charter.toml holds", async () => {
    core({ shared: SAVES_SHARED, saving: SAVES_IN_FORCE });
    const saving = await at("Saving");

    expect(within(saving).getByLabelText("Mode")).toHaveValue("pr");
    expect(within(saving).getByLabelText("Target branch")).toHaveValue("");
    expect(within(saving).getByLabelText("Save branch")).toHaveValue("");
    expect(within(saving).getByLabelText("Sign commits")).toHaveValue("on");
    expect(within(saving).getByLabelText("Auto-save")).toHaveValue("");
    expect(within(saving).getByLabelText("Auto-save after")).toHaveValue("");
    expect(within(saving).getByLabelText("Mode")).toHaveAccessibleDescription(
      /pr \(push to the save branch and keep one request open\), or pr-merge \(and set that request to auto-merge\)/,
    );
    expect(
      within(within(saving).getByLabelText("Mode"))
        .getAllByRole("option")
        .map((option) => option.getAttribute("value")),
    ).toEqual(["", "off", "commit", "push", "pr", "pr-merge"]);
  });

  it("says beside each key which file decided it, and marks a value charter.local.toml overrides", async () => {
    // One form (SE-18): the row shows Local's value where Local holds one, badged as overriding
    // what charter.toml has.
    core({ shared: SAVES_SHARED, local: LOCAL_SAVES, saving: SAVES_IN_FORCE });
    const saving = await at("Saving");

    await waitFor(() =>
      expect(within(saving).getByLabelText("Mode")).toHaveAccessibleDescription(
        /In this project: push, from charter\.local\.toml\./,
      ),
    );
    expect(within(saving).getByLabelText("Mode")).toHaveValue("push");
    expect(within(saving).getByLabelText("Mode")).toHaveAccessibleDescription(
      /From charter\.local\.toml, at the Project level: this machine only\. charter\.toml has pr, which this overrides\./,
    );
    expect(within(rowOf("Mode")).getByText("Overrides charter.toml")).toBeInTheDocument();
    expect(within(saving).getByLabelText("Sign commits")).toHaveAccessibleDescription(
      /In this project: on, from charter\.toml\./,
    );
    expect(within(saving).getByLabelText("Auto-save after")).toHaveAccessibleDescription(
      /In this project: 1m, its default\./,
    );
    // Local decided a key Shared does not hold: nothing here is overridden.
    expect(within(saving).getByLabelText("Target branch")).toHaveAccessibleDescription(
      /In this project: trunk, from charter\.local\.toml\./,
    );
    expect(within(saving).getByLabelText("Target branch")).not.toHaveAccessibleDescription(
      /overrides/,
    );
    expect(within(rowOf("Target branch")).queryByText("Overrides charter.toml")).toBeNull();
  });

  it("says what no value means where no file sets one", async () => {
    core();
    const saving = await at("Saving");

    await waitFor(() =>
      expect(within(saving).getByLabelText("Mode")).toHaveAccessibleDescription(
        /In this project: not set — the Saving view asks once, before anything is pushed\./,
      ),
    );
    expect(within(saving).getByLabelText("Target branch")).toHaveAccessibleDescription(
      /In this project: the branch the project has checked out, its default\./,
    );
    expect(within(saving).getByLabelText("Save branch")).toHaveAccessibleDescription(
      /In this project: purlis\/save\/<this machine's name>, its default\./,
    );
  });

  it("writes each key by its kind, and empty removes it", async () => {
    const { sent } = core({ shared: SAVES_SHARED, saving: SAVES_IN_FORCE });
    await at("Saving");

    await userEvent.selectOptions(screen.getByLabelText("Mode"), "pr-merge");
    await userEvent.selectOptions(screen.getByLabelText("Auto-save"), "off");
    await userEvent.type(screen.getByLabelText("Save branch"), "charter/save/team");
    await leave();
    await userEvent.selectOptions(screen.getByLabelText("Sign commits"), "");

    await waitFor(() => expect(sent).toHaveLength(4));
    expect(sent.every((one) => one.which === "shared")).toBe(true);
    expect(sent.map((one) => one.edits)).toEqual([
      [{ path: [{ key: "plane" }, { key: "mode" }], value: { kind: "text", value: "pr-merge" } }],
      [{ path: [{ key: "plane" }, { key: "autosave" }], value: { kind: "bool", value: false } }],
      [
        {
          path: [{ key: "plane" }, { key: "save_branch" }],
          value: { kind: "text", value: "charter/save/team" },
        },
      ],
      [{ path: [{ key: "plane" }, { key: "sign" }], value: null }],
    ]);
  });

  it("says the reader's refusal of a value beside it, in its words", async () => {
    const why =
      "plane.autosave_after in charter.toml is not a quiet period — a whole number of seconds or minutes";
    core({ shared: SAVES_SHARED, saving: SAVES_IN_FORCE, refuse: [why] });
    await at("Saving");

    await userEvent.type(screen.getByLabelText("Auto-save after"), "soon");
    await leave();

    const row = (await screen.findByText(why)).closest(".ui-setting-row");
    expect(within(row as HTMLElement).getByLabelText("Auto-save after")).toHaveValue("");
  });

  /** charter.toml holding `[memory] share = word`. */
  const sharing = (word: string): SettingsFile => ({
    ...SHARED,
    fields: SHARED.fields.map((one) =>
      one.path[0]?.key === "memory"
        ? { ...one, value: { kind: "text" as const, value: word } }
        : one,
    ),
  });
  const SHARE_HINT =
    "Deprecated: read as Mode — commit and push carry over, local says nothing — only while neither file sets Mode. Set Mode instead.";

  it("shows [memory] share as the deprecated alias of Mode, marked in force", async () => {
    core({
      shared: sharing("commit"),
      saving: {
        ...NO_SAVING,
        plane: { ...NO_SAVING.plane, mode: said("commit", "shared"), from_share: true },
      },
    });
    const saving = await at("Saving");

    const share = within(saving).getByLabelText("[memory] share (deprecated)");
    expect(share).toHaveValue("commit");
    await waitFor(() =>
      expect(share).toHaveAccessibleDescription(new RegExp(`^${SHARE_HINT} In force as Mode\\.`)),
    );
    expect(within(saving).getByLabelText("Mode")).toHaveAccessibleDescription(
      /In this project: commit, from \[memory\] share in charter\.toml, the deprecated alias\./,
    );
  });

  it("marks a [memory] share that a Mode in either file overrides as not in force", async () => {
    core({
      shared: sharing("push"),
      saving: { ...NO_SAVING, plane: { ...NO_SAVING.plane, mode: said("pr", "local") } },
    });
    await at("Saving");

    await waitFor(() =>
      expect(screen.getByLabelText("[memory] share (deprecated)")).toHaveAccessibleDescription(
        new RegExp(`^${SHARE_HINT} Not in force — Mode from charter\\.local\\.toml wins\\.`),
      ),
    );
  });

  it("gives a [memory] share of local, which says nothing, no marker", async () => {
    core({
      shared: sharing("local"),
      saving: { ...NO_SAVING, plane: { ...NO_SAVING.plane, mode: said("pr", "shared") } },
    });
    await at("Saving");

    await waitFor(() =>
      expect(screen.getByLabelText("Mode")).toHaveAccessibleDescription(/In this project/),
    );
    expect(screen.getByLabelText("[memory] share (deprecated)")).not.toHaveAccessibleDescription(
      /in force/i,
    );
  });

  it("says where [plane] worktrees is", async () => {
    core();
    const saving = await at("Saving");

    expect(saving).toHaveTextContent("[plane] worktrees is under General.");
  });

  it("says what is in force could not be read, rather than drawing a marker or no repo", async () => {
    core({ shared: SAVES_SHARED, saving: { trouble: "the plane is not held" } });
    const saving = await at("Saving");

    await waitFor(() =>
      expect(saving).toHaveTextContent(
        "What this project uses could not be read: the plane is not held",
      ),
    );
    expect(saving).not.toHaveTextContent("No repo is catalogued");
    expect(within(saving).getByLabelText("Mode")).not.toHaveAccessibleDescription(
      /In this project/,
    );
  });

  it("asks what is in force again after a change", async () => {
    const { count } = core({ shared: SAVES_SHARED, saving: SAVES_IN_FORCE });
    await at("Saving");
    await waitFor(() => expect(count.saving).toBe(1));

    await userEvent.selectOptions(screen.getByLabelText("Mode"), "commit");

    await waitFor(() => expect(count.saving).toBe(2));
  });
});

describe("each repo's save keys, in Saving (charter-app#300, ADR 0051)", () => {
  it("has a row per catalogued repo, each key marked with the file that decided it", async () => {
    core({ shared: SAVES_SHARED, saving: SAVES_IN_FORCE });
    const saving = await at("Saving");

    await waitFor(() => expect(within(saving).getByLabelText("api: mode")).toHaveValue("push"));
    for (const name of ["web", "api"])
      for (const key of ["mode", "branch", "sign", "auto-save", "auto-save after"])
        expect(within(saving).getByLabelText(`${name}: ${key}`)).toBeInTheDocument();
    expect(within(saving).queryByLabelText("api: save branch")).toBeNull();
    expect(within(saving).getByLabelText("api: mode")).toHaveAccessibleDescription(
      /In this project: push, from charter\.toml\./,
    );
    expect(within(saving).getByLabelText("web: mode")).toHaveAccessibleDescription(
      /In this project: off, its default\./,
    );
    // The glossary's word, which names a pull request and a merge request both — never "PR".
    expect(within(saving).getByLabelText("web: branch")).toHaveAccessibleDescription(
      /The branch a request goes into\./,
    );
    expect(within(saving).getByLabelText("web: branch")).toHaveAccessibleDescription(
      /In this project: the repo's default branch, its default\./,
    );
  });

  it("marks a repo value charter.local.toml overrides", async () => {
    core({
      shared: {
        ...SAVES_SHARED,
        fields: [
          ...SAVES_SHARED.fields,
          field(["repos", "api", "autosave"], { kind: "bool", value: false }),
        ],
      },
      local: LOCAL_SAVES,
      saving: SAVES_IN_FORCE,
    });
    await at("Saving");

    await waitFor(() =>
      expect(screen.getByLabelText("api: auto-save")).toHaveAccessibleDescription(
        /In this project: on, from charter\.local\.toml\./,
      ),
    );
    expect(screen.getByLabelText("api: auto-save")).toHaveAccessibleDescription(
      /charter\.toml has off, which this overrides\./,
    );
    expect(within(rowOf("api: auto-save")).getByText("Overrides charter.toml")).toBeInTheDocument();
  });

  it("writes a repo's key under [repos.<name>]", async () => {
    const { sent } = core({ shared: SAVES_SHARED, saving: SAVES_IN_FORCE });
    await at("Saving");

    await userEvent.selectOptions(await screen.findByLabelText("web: mode"), "pr-merge");
    await userEvent.type(screen.getByLabelText("web: branch"), "develop");
    await leave();

    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent.map((one) => one.edits)).toEqual([
      [
        {
          path: [{ key: "repos" }, { key: "web" }, { key: "mode" }],
          value: { kind: "text", value: "pr-merge" },
        },
      ],
      [
        {
          path: [{ key: "repos" }, { key: "web" }, { key: "branch" }],
          value: { kind: "text", value: "develop" },
        },
      ],
    ]);
  });

  it("says once that Local was left out of the repos, and only where it set them", async () => {
    core({
      local: { ...LOCAL, refusals: [LEFT_OUT] },
      shared: WITH_EXTENSIONS,
      extensions: EXTENSIONS,
      saving: { ...SAVES_IN_FORCE, repos_left_out: LEFT_OUT },
    });
    const saving = await at("Saving");

    await waitFor(() => expect(within(saving).getAllByText(LEFT_OUT)).toHaveLength(1));
    await userEvent.click(within(nav()).getByRole("button", { name: "Extensions" }));
    expect(screen.getByRole("region", { name: "Extensions" })).not.toHaveTextContent(LEFT_OUT);
  });
});

describe("the sandbox, in Sandbox (ADR 0067 §7, ruling V78 d)", () => {
  it("shows this machine's opt-out count where the sandbox is on, and that it is never sent", async () => {
    core({
      shared: {
        ...SHARED,
        fields: [...SHARED.fields, field(["sandbox", "mode"], { kind: "text", value: "on" })],
      },
      sandbox: {
        on: true,
        offer: false,
        never: [],
        hosts_changed: null,
        presets_changed: null,
        presets: [],
        persona_hosts: [],
        besides: { project_hosts: 0, your_hosts: 0, folders: 0 },
        policy: null,
        said: "1 of 4 chats started without the sandbox on this machine (25%); the bar is under 10%",
      },
    });
    await at("Sandbox");

    await waitFor(() =>
      expect(screen.getByRole("group", { name: "Sandbox" })).toHaveTextContent(
        "1 of 4 chats started without the sandbox on this machine (25%); the bar is under 10%. Counted on this machine only, and never sent.",
      ),
    );
  });

  it("says how a project that has it off turns it on", async () => {
    core();
    await at("Sandbox");

    await waitFor(() =>
      expect(screen.getByRole("group", { name: "Sandbox" })).toHaveTextContent(
        "Off in this project.",
      ),
    );
    expect(screen.getByRole("button", { name: "Turn the sandbox on" })).toBeInTheDocument();
  });
});
