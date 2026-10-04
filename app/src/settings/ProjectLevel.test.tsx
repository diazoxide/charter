import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useState } from "react";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SettingsTab } from "./SettingsTab";
import type { Level } from "./groups";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import type {
  HarnessPlugins,
  ProjectExtension,
  SettingsEdit,
  SettingsField,
  SettingsFile,
  SettingsWhich,
} from "../bindings";

/**
 * **The Settings tab at the Project level** (SE-17, #1167; the spec on #558, V89b, V89e, V89h):
 * the project's groups in V89h's order, each control writing its one key through the core as it
 * changes, Undo of the last change, and a refused write said beside its setting. The core is the
 * mock here: what it keeps of a file and what it refuses is `charter_core::settings`'s tests.
 * What is held here is what the person sees and what the window sends.
 */

const PLANE = "/home/dev/plane";

const SHARED_TEXT = `# the team's
[plane]
mode = "push"   # how far a save goes

[[forge]]
kind = "github"
owner = "acme"
`;

const field = (keys: (string | number)[], value: SettingsField["value"]): SettingsField => ({
  path: keys.map((one) => (typeof one === "number" ? { index: one } : { key: one })),
  value,
});

const SHARED: SettingsFile = {
  which: "shared",
  file: "charter.toml",
  exists: true,
  text: SHARED_TEXT,
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
  text: `[harness.work]\nkind = "claude"\ncommand = ["claude"]\n`,
  refusals: [],
  parsed: true,
  fields: [
    field(["harness", "work", "kind"], { kind: "text", value: "claude" }),
    field(["harness", "work", "command"], { kind: "list", value: ["claude"] }),
  ],
};

const said = (value: string | null, source = "default") => ({ value, source });

const SAVING = {
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

const THEME = {
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

const LINTER: ProjectExtension = {
  id: "linter",
  name: "Linter",
  state: "on",
  source: "default",
  settings: [],
  ignored: [],
};

const CLAUDE: HarnessPlugins = {
  harness: "claude",
  title: "Claude Code",
  unsupported: null,
  record: null,
  trouble: null,
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
  local_left_out: null,
};

/** A file as the core would answer it after `edits`: each edited key set or gone, and a text
 *  that differs, so the next write is checked against the new one. */
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

/** The core, as a mock. `refuse` answers a write with these reasons instead of writing it. */
function core({
  extensions = [] as ProjectExtension[],
  harnesses = [] as HarnessPlugins[],
  refuse = undefined as string[] | undefined,
  sandboxOn = false,
} = {}) {
  const files: Record<SettingsWhich, SettingsFile> = { shared: SHARED, local: LOCAL };
  const sent: Sent[] = [];
  let reads = 0;
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    switch (cmd) {
      case "project_settings":
        reads += 1;
        return { shared: files.shared, local: files.local };
      case "project_extensions":
        return { extensions, local_left_out: null };
      case "project_harness_plugins":
        return harnesses;
      case "project_saving_in_force":
        return SAVING;
      case "project_theme":
        return THEME;
      case "project_theme_drawn":
        return null;
      case "extensions_on":
        return [];
      case "sandbox_state":
        return sandboxOn
          ? { on: true, offer: false, said: "No chat started without it", never: [] }
          : { on: false, offer: false, said: null, never: [] };
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
  });
  return { sent, reads: () => reads };
}

beforeEach(() => {
  forgetThisLaunch();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: {
      path: "/home/dev/.config/charter/layout.json",
      found: false,
      document: null,
      trouble: null,
    },
    theme: { path: "", found: false, document: null, trouble: null },
  };
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

/** The tab as a view tab holds it: its level is the tab's, and the switcher changes it. */
function Tab({ at = "project" }: { at?: Level }) {
  const [level, setLevel] = useState<Level>(at);
  return <SettingsTab plane={PLANE} level={level} onLevelChange={setLevel} />;
}

const nav = () => screen.getByRole("navigation", { name: "Groups" });
const groups = () =>
  within(nav())
    .getAllByRole("button")
    .map((one) => one.textContent);
const open = (name: string) => userEvent.click(within(nav()).getByRole("button", { name }));
const shown = () => screen.getByRole("region", { name: /./ });

async function atProject() {
  render(<Tab />);
  await screen.findByRole("button", { name: "General" });
}

describe("the Project level", () => {
  it("is offered beside You, and the switcher moves the tab to it", async () => {
    core();
    render(<Tab at="you" />);

    await userEvent.click(screen.getByRole("radio", { name: "Project" }));

    expect(screen.getByRole("radio", { name: "Project" })).toHaveAttribute("aria-checked", "true");
    expect(await within(nav()).findByRole("button", { name: "General" })).toBeInTheDocument();
  });

  it("lists the eight groups in order when each has something to set", async () => {
    core({ extensions: [LINTER], harnesses: [CLAUDE] });
    await atProject();

    await waitFor(() =>
      expect(groups()).toEqual([
        "General",
        "Saving",
        "Harness & profiles",
        "Sandbox",
        "Forges",
        "Extensions",
        "Appearance",
        "Plugins",
      ]),
    );
  });

  it("hides a group with nothing in it", async () => {
    core();
    await atProject();

    expect(groups()).not.toContain("Extensions");
    expect(groups()).not.toContain("Plugins");
    expect(groups()).toContain("Forges");
  });

  it("says which file a setting is kept in", async () => {
    core();
    await atProject();
    await open("Saving");

    expect(screen.getByLabelText("Mode")).toHaveAccessibleDescription(/charter\.toml/);
    await open("Harness & profiles");
    expect(screen.getByLabelText("work: kind")).toHaveAccessibleDescription(/charter\.local\.toml/);
  });
});

describe("every project setting there is, at the Project level", () => {
  it.each([
    [
      "General",
      [
        "Default workspace",
        "Default persona",
        "Update channel",
        "Version lock",
        "Worktrees folder",
      ],
    ],
    [
      "Saving",
      [
        "Mode",
        "Target branch",
        "Save branch",
        "Sign commits",
        "Auto-save",
        "Auto-save after",
        "Assisted-by trailer",
        "[memory] share (deprecated)",
      ],
    ],
    [
      "Harness & profiles",
      [
        "Default harness",
        "Default profile",
        "work: kind",
        "work: command",
        "work: environment",
        "Environment passed to chats",
      ],
    ],
    ["Sandbox", ["Sandbox mode", "Hosts it may reach"]],
    ["Forges", ["Forge 1: kind", "Forge 1: owner", "Forge 1: host", "Forge 1: repos never listed"]],
    ["Extensions", ["Linter: enabled"]],
    ["Appearance", ["Theme", "Icons"]],
    ["Plugins", ["Claude Code: review@acme"]],
  ])("%s has its settings", async (name, labels) => {
    core({ extensions: [LINTER], harnesses: [CLAUDE] });
    await atProject();
    await waitFor(() => expect(groups()).toContain(name));

    await open(name);

    expect(shown()).toHaveAccessibleName(name);
    for (const label of labels) expect(within(shown()).getByLabelText(label)).toBeInTheDocument();
  });
});

describe("a change at the Project level", () => {
  it("writes its one key to its file as it is picked, against the text it was read as", async () => {
    const { sent } = core();
    await atProject();
    await open("Saving");

    await userEvent.selectOptions(screen.getByLabelText("Mode"), "commit");

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toEqual({
      which: "shared",
      base: SHARED_TEXT,
      edits: [
        { path: [{ key: "plane" }, { key: "mode" }], value: { kind: "text", value: "commit" } },
      ],
    });
    expect(screen.getByLabelText("Mode")).toHaveValue("commit");
  });

  it("writes a typed value once, when the field is left, and not at every key", async () => {
    const { sent } = core();
    await atProject();

    await userEvent.type(screen.getByLabelText("Default workspace"), "main");
    expect(sent).toHaveLength(0);
    await userEvent.tab();

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].edits).toEqual([
      { path: [{ key: "workspace" }, { key: "default" }], value: { kind: "text", value: "main" } },
    ]);
  });

  it("writes a local setting to charter.local.toml", async () => {
    const { sent } = core();
    await atProject();
    await open("Harness & profiles");

    await userEvent.selectOptions(screen.getByLabelText("work: kind"), "codex");

    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].which).toBe("local");
  });

  it("undoes the last change by writing the value it had back", async () => {
    const { sent } = core();
    await atProject();
    await open("Saving");
    await userEvent.selectOptions(screen.getByLabelText("Mode"), "commit");

    await userEvent.click(await screen.findByRole("button", { name: "Undo" }));

    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1].which).toBe("shared");
    expect(sent[1].edits).toEqual([
      { path: [{ key: "plane" }, { key: "mode" }], value: { kind: "text", value: "push" } },
    ]);
    expect(screen.getByLabelText("Mode")).toHaveValue("push");
    expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
  });

  it("undoes a key that was not set by taking it out again", async () => {
    const { sent } = core();
    await atProject();
    await open("Saving");
    await userEvent.selectOptions(screen.getByLabelText("Sign commits"), "on");

    await userEvent.click(await screen.findByRole("button", { name: "Undo" }));

    await waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1].edits).toEqual([{ path: [{ key: "plane" }, { key: "sign" }], value: null }]);
  });

  it("says a refused write beside its setting, and shows what is on disk", async () => {
    core({ refuse: ["plane.mode in charter.toml is not a mode"] });
    await atProject();
    await open("Saving");

    await userEvent.selectOptions(screen.getByLabelText("Mode"), "pr");

    const row = (await screen.findByText("plane.mode in charter.toml is not a mode")).closest(
      ".ui-setting-row",
    );
    expect(row).not.toBeNull();
    expect(within(row as HTMLElement).getByLabelText("Mode")).toHaveValue("push");
    expect(screen.getByRole("alert")).toHaveTextContent("plane.mode in charter.toml is not a mode");
    expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
  });

  it("never offers to take the sandbox off once the project has it on", async () => {
    core({ sandboxOn: true });
    SHARED.fields.push(field(["sandbox", "mode"], { kind: "text", value: "on" }));
    try {
      await atProject();
      await open("Sandbox");

      const options = within(screen.getByLabelText("Sandbox mode"))
        .getAllByRole("option")
        .map((one) => one.textContent);
      expect(options).toEqual(["on"]);
    } finally {
      SHARED.fields.pop();
    }
  });
});
