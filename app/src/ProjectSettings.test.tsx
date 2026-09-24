import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ProjectSettings } from "./ProjectSettings";
import { ViewPane } from "./Views";
import { SETTINGS_TITLE, SETTINGS_VIEW } from "./tabs";
import type { ProjectSettings as Both, SettingsFile, SettingsSaved } from "./bindings";

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

/** The core, as a mock: `project_settings` answers `both`, and `save_project_settings` answers
 *  `saved` and records what it was sent. */
function core(both: Both, saved?: (sent: Record<string, unknown>) => SettingsSaved) {
  const sent: Record<string, unknown>[] = [];
  let reads = 0;
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    if (cmd === "project_settings") {
      reads += 1;
      return both;
    }
    if (cmd === "save_project_settings") {
      sent.push(given);
      return saved?.(given) ?? { kind: "saved", file: both.shared };
    }
    return undefined;
  });
  return { sent, reads: () => reads };
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
