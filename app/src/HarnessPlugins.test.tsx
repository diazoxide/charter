import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ProjectSettings } from "./ProjectSettings";
import type { HarnessPlugins, ProjectSettings as Both, SettingsFile } from "./bindings";

/**
 * The **Harness plugins** groups of the Project settings tab (charter-app#274, ADR 0050): one per
 * harness charter knows, in both sections. What each plugin resolves to, and which harnesses can
 * apply it, is the core's (`charter_core::harness_plugin`'s tests); here the core is the mock,
 * and what is held is that every harness is drawn, a fixed plugin has no control, a harness that
 * cannot apply says so, and a toggle writes the key of the section it is in.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

const file = (which: "shared" | "local", fields: SettingsFile["fields"] = []): SettingsFile => ({
  which,
  file: which === "shared" ? "charter.toml" : "charter.local.toml",
  exists: true,
  text: "",
  refusals: [],
  parsed: true,
  fields,
});

const BOTH: Both = {
  shared: file("shared", [
    {
      path: [{ key: "harness_plugins" }, { key: "claude" }, { key: "figma@official" }],
      value: { kind: "bool", value: false },
    },
  ]),
  local: file("local"),
};

const OWN_WHY =
  "charter@inline is always on: it is charter's own plugin, and it carries charter's hooks and the Bash guard";
const OLD_WHY =
  "charter@charter is always off: it is the Python charter's plugin, and a chat the app starts carrying it too would have two sets of hooks and two handoff skills";

/** The ignore check's sentence for a `charter.local.toml` git would commit, as the core says it
 *  (charter-app#308): what the Local section says, and every group that shows what is in force. */
const LEFT_OUT =
  "git would commit charter.local.toml, so charter reads nothing in it until it is ignored — charter reinit adds /charter.local.toml to .gitignore.";

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
        id: "acme@corp",
        name: "acme",
        origin: "",
        state: "on",
        source: "local",
        installed: false,
        pinned: null,
        ignored: [],
      },
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
            file: "charter.toml",
            why: `charter.toml sets harness_plugins.claude."charter@inline" to false, and ${OWN_WHY}`,
          },
        ],
      },
      {
        id: "charter@charter",
        name: "charter",
        origin: "charter, user",
        state: "off",
        source: "default",
        installed: true,
        pinned: OLD_WHY,
        ignored: [],
      },
      {
        id: "figma@official",
        name: "figma",
        origin: "official, user",
        state: "off",
        source: "shared",
        installed: true,
        pinned: null,
        ignored: [],
      },
      {
        id: "serena@official",
        name: "serena",
        origin: "official, project",
        state: "not-set",
        source: "default",
        installed: true,
        pinned: null,
        ignored: [],
      },
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
    unsupported:
      "plugins for Codex are not supported yet — Codex 0.147.0 turns a plugin on or off only in its own config.toml",
    trouble: null,
    local_left_out: null,
    plugins: [
      {
        id: "charter@charter",
        name: "charter",
        origin: "charter, on in config.toml",
        state: "not-set",
        source: "default",
        installed: true,
        pinned: null,
        ignored: [],
      },
    ],
  },
];

function core(harnesses: HarnessPlugins[] = HARNESSES) {
  const sent: Record<string, unknown>[] = [];
  let reads = 0;
  mockIPC((cmd, args) => {
    if (cmd === "project_settings") return BOTH;
    if (cmd === "project_extensions") return { extensions: [], local_left_out: null };
    if (cmd === "extensions_on") return [];
    if (cmd === "project_harness_plugins") {
      reads += 1;
      return harnesses;
    }
    if (cmd === "save_project_settings") {
      sent.push((args ?? {}) as Record<string, unknown>);
      return { kind: "saved", file: BOTH.shared };
    }
    return undefined;
  });
  return { sent, reads: () => reads };
}

async function drawn() {
  render(<ProjectSettings plane={PLANE} />);
  const shared = await screen.findByTestId("settings-shared");
  const local = await screen.findByTestId("settings-local");
  await within(shared).findByRole("group", { name: "Harness plugins: Claude Code" });
  return { shared, local };
}

describe("the Harness plugins groups (charter-app#274)", () => {
  it("draws one group per harness in both sections, and never leaves one out", async () => {
    core();
    const { shared, local } = await drawn();

    for (const section of [shared, local]) {
      for (const title of ["Claude Code", "opencode", "Codex"]) {
        expect(
          within(section).getByRole("group", { name: `Harness plugins: ${title}` }),
        ).toBeInTheDocument();
      }
    }
  });

  it("gives each plugin a project may choose on, off and not set, as the file holds it", async () => {
    core();
    const { shared, local } = await drawn();

    expect(within(shared).getByLabelText("Claude Code: figma@official")).toHaveValue("off");
    expect(within(local).getByLabelText("Claude Code: figma@official")).toHaveValue("");
    const group = within(shared).getByRole("group", { name: "Harness plugins: Claude Code" });
    expect(group).toHaveTextContent("off in this project — from charter.toml");
    expect(group).toHaveTextContent("not set — Claude Code decides, from its own settings");
    expect(group).toHaveTextContent(
      "not installed on this machine — named in charter.local.toml, so no chat is handed it",
    );
  });

  it("names the file it listed a harness's plugins from, since a profile may list another", async () => {
    core();
    const { shared } = await drawn();

    expect(
      within(shared).getByRole("group", { name: "Harness plugins: Claude Code" }),
    ).toHaveTextContent(
      "Listed from /home/dev/.claude/plugins/installed_plugins.json. A profile that points Claude Code at another directory is listed against that one when its chat starts.",
    );
  });

  it("draws charter's own plugin and the old one as fixed, with no control to change either", async () => {
    core();
    const { shared } = await drawn();
    const group = within(shared).getByRole("group", { name: "Harness plugins: Claude Code" });

    expect(within(group).queryByLabelText("Claude Code: charter@inline")).toBeNull();
    expect(within(group).queryByLabelText("Claude Code: charter@charter")).toBeNull();
    expect(group).toHaveTextContent(OWN_WHY);
    expect(group).toHaveTextContent(OLD_WHY);
    // The value a file tried to set, said in the section of the file that set it.
    expect(group).toHaveTextContent('sets harness_plugins.claude."charter@inline" to false');
  });

  it("says a harness that cannot apply is not supported yet, and offers no control for it", async () => {
    core();
    const { shared } = await drawn();
    const codex = within(shared).getByRole("group", { name: "Harness plugins: Codex" });

    expect(codex).toHaveTextContent("plugins for Codex are not supported yet");
    expect(within(codex).queryByRole("combobox")).toBeNull();
    // What it has installed is still listed, so nothing about it is hidden.
    expect(codex).toHaveTextContent("charter@charter (charter, on in config.toml)");
    expect(
      within(shared).getByRole("group", { name: "Harness plugins: opencode" }),
    ).toHaveTextContent("plugins for opencode are not supported yet");
  });

  it("writes a toggle to the section it is in, and not set removes the key", async () => {
    const { sent } = core();
    const { shared, local } = await drawn();
    const user = userEvent.setup();

    await user.selectOptions(within(local).getByLabelText("Claude Code: serena@official"), "on");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].which).toBe("local");
    expect((sent[0].change as { edits: unknown }).edits).toEqual([
      {
        path: [{ key: "harness_plugins" }, { key: "claude" }, { key: "serena@official" }],
        value: { kind: "bool", value: true },
      },
    ]);

    await user.selectOptions(within(shared).getByLabelText("Claude Code: figma@official"), "");
    await user.click(within(shared).getByRole("button", { name: "Save charter.toml" }));
    await waitFor(() => expect(sent).toHaveLength(2));
    expect((sent[1].change as { edits: unknown }).edits).toEqual([
      {
        path: [{ key: "harness_plugins" }, { key: "claude" }, { key: "figma@official" }],
        value: null,
      },
    ]);
  });

  it("asks what is in force again after a save", async () => {
    const { reads } = core();
    const { local } = await drawn();
    const user = userEvent.setup();
    await waitFor(() => expect(reads()).toBe(1));

    await user.selectOptions(within(local).getByLabelText("Claude Code: serena@official"), "off");
    await user.click(within(local).getByRole("button", { name: "Save charter.local.toml" }));

    await waitFor(() => expect(reads()).toBe(2));
  });

  it("says once in each harness's group why charter.local.toml was left out (charter-app#319)", async () => {
    core(HARNESSES.map((one) => ({ ...one, local_left_out: LEFT_OUT })));
    const { shared, local } = await drawn();

    for (const title of ["Claude Code", "opencode", "Codex"]) {
      const name = `Harness plugins: ${title}`;
      expect(
        within(within(shared).getByRole("group", { name })).getAllByText(LEFT_OUT),
      ).toHaveLength(1);
      // The Local section's groups leave it to the section's head, which says it from the file's
      // own refusals (held in ProjectSettings.test.tsx).
      expect(within(local).getByRole("group", { name })).not.toHaveTextContent(LEFT_OUT);
    }
  });
});
