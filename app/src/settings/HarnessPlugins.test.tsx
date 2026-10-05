import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SettingsTab } from "./SettingsTab";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import type { HarnessPlugins, ProjectSettings as Both, SettingsFile } from "../bindings";

/**
 * The **Plugins** group of the Settings tab's Project level (charter-app#274, ADR 0050; carried
 * over from the old Project settings page by SE-19): every harness charter knows, each plugin
 * as a setting kept in `charter.toml`. What each plugin resolves to, and which harnesses can
 * apply it, is the core's (`charter_core::harness_plugin`'s tests); here the core is the mock,
 * and what is held is that every harness is said, a fixed plugin has no control, a harness that
 * cannot apply says so, and a toggle writes its key.
 */

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
 *  (charter-app#308): what every group that shows what is in force says. */
const LEFT_OUT =
  "git would commit charter.local.toml, so charter reads nothing in it until it is ignored — charter doctor --fix local-ignore adds /charter.local.toml to .gitignore.";

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
  mockIPC(
    (cmd, args) => {
      if (cmd === "project_settings") return BOTH;
      if (cmd === "project_extensions") return { extensions: [], local_left_out: null };
      if (cmd === "extensions_on") return [];
      if (cmd === "project_harness_plugins") {
        reads += 1;
        return harnesses;
      }
      if (cmd === "save_project_settings") {
        sent.push((args ?? {}) as Record<string, unknown>);
        return { kind: "saved", file: { ...BOTH.shared, text: "# written\n" } };
      }
      return undefined;
    },
    { shouldMockEvents: true },
  );
  return { sent, reads: () => reads };
}

/** The tab at the Project level, with Plugins on the right. */
async function drawn() {
  render(<SettingsTab plane={PLANE} level="project" />);
  const nav = await screen.findByRole("navigation", { name: "Groups" });
  await userEvent.click(await within(nav).findByRole("button", { name: "Plugins" }));
  return screen.getByRole("region", { name: "Plugins" });
}

describe("the Plugins group (charter-app#274)", () => {
  it("says every harness charter knows, and never leaves one out", async () => {
    core();
    const group = await drawn();

    expect(group).toHaveTextContent(
      "Listed from /home/dev/.claude/plugins/installed_plugins.json.",
    );
    expect(group).toHaveTextContent("plugins for opencode are not supported yet");
    expect(group).toHaveTextContent("plugins for Codex are not supported yet");
  });

  it("gives each plugin a project may choose on, off and not set, as the file holds it", async () => {
    core();
    const group = await drawn();

    expect(within(group).getByLabelText("Claude Code: figma@official")).toHaveValue("off");
    expect(within(group).getByLabelText("Claude Code: serena@official")).toHaveValue("");
    expect(group).toHaveTextContent("off in this project — from charter.toml");
    expect(group).toHaveTextContent("not set — Claude Code decides, from its own settings");
    expect(group).toHaveTextContent(
      "not installed on this machine — named in charter.local.toml, so no chat is handed it",
    );
  });

  it("names the file it listed a harness's plugins from, since a profile may list another", async () => {
    core();
    const group = await drawn();

    expect(group).toHaveTextContent(
      "Listed from /home/dev/.claude/plugins/installed_plugins.json. A profile that points Claude Code at another directory is listed against that one when its chat starts.",
    );
  });

  it("draws charter's own plugin and the old one as fixed, with no control to change either", async () => {
    core();
    const group = await drawn();

    expect(within(group).queryByLabelText("Claude Code: charter@inline")).toBeNull();
    expect(within(group).queryByLabelText("Claude Code: charter@charter")).toBeNull();
    expect(group).toHaveTextContent(OWN_WHY);
    expect(group).toHaveTextContent(OLD_WHY);
    // The value a file tried to set, said with the file that set it.
    expect(group).toHaveTextContent('sets harness_plugins.claude."charter@inline" to false');
  });

  it("says a harness that cannot apply is not supported yet, and offers no control for it", async () => {
    core();
    const group = await drawn();

    expect(group).toHaveTextContent("plugins for Codex are not supported yet");
    expect(within(group).queryByLabelText("Codex: charter@charter")).toBeNull();
    // What it has installed is still listed, so nothing about it is hidden.
    expect(group).toHaveTextContent("charter@charter (charter, on in config.toml)");
  });

  it("writes a toggle to charter.toml, and not set removes the key", async () => {
    const { sent } = core();
    const group = await drawn();

    await userEvent.selectOptions(
      within(group).getByLabelText("Claude Code: serena@official"),
      "on",
    );
    await waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].which).toBe("shared");
    expect((sent[0].change as { edits: unknown }).edits).toEqual([
      {
        path: [{ key: "harness_plugins" }, { key: "claude" }, { key: "serena@official" }],
        value: { kind: "bool", value: true },
      },
    ]);

    await userEvent.selectOptions(within(group).getByLabelText("Claude Code: figma@official"), "");
    await waitFor(() => expect(sent).toHaveLength(2));
    expect((sent[1].change as { edits: unknown }).edits).toEqual([
      {
        path: [{ key: "harness_plugins" }, { key: "claude" }, { key: "figma@official" }],
        value: null,
      },
    ]);
  });

  it("asks what is in force again after a change", async () => {
    const { reads } = core();
    const group = await drawn();
    await waitFor(() => expect(reads()).toBe(1));

    await userEvent.selectOptions(
      within(group).getByLabelText("Claude Code: serena@official"),
      "off",
    );

    await waitFor(() => expect(reads()).toBe(2));
  });

  it("says once for each harness why charter.local.toml was left out (charter-app#319)", async () => {
    core(HARNESSES.map((one) => ({ ...one, local_left_out: LEFT_OUT })));
    const group = await drawn();

    expect(within(group).getAllByText(LEFT_OUT)).toHaveLength(HARNESSES.length);
  });
});
