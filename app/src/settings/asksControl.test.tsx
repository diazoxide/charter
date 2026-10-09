import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { SettingsFile } from "../bindings";
import { asksBoxes, asksSetting, ASKS } from "./asks";
import type { ProjectRead } from "./project";
import type { LiveSetting } from "./groups";

/**
 * **Profiles that ask before they act** (#1522): the box per profile on Harness & profiles that
 * marks it in `[harness] asks` of the Local file, through its own window-only command, which is
 * the one click back after a task was refused for a profile nobody marked.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const LOCAL = (asks: string[] | null, tables: string[] = []): SettingsFile => ({
  which: "local",
  file: "purlis.local.toml",
  exists: true,
  text: "the text read",
  refusals: [],
  parsed: true,
  fields: asks === null ? [] : [{ path: ASKS, value: { kind: "list", value: asks } }],
  entries: tables.map((name) => ({
    collection: "profiles",
    id: `profile:${name}`,
    label: name,
    keys: [{ key: "harness" }, { key: name }],
    values: [{ field: "name", value: name }],
  })),
});

const read = (local: SettingsFile, profile?: string[], more: Partial<ProjectRead> = {}) =>
  ({ local, entries: profile === undefined ? {} : { profile }, ...more }) as unknown as ProjectRead;

function Row({ setting }: { setting: LiveSetting }) {
  const { control } = setting.useControl();
  return <>{control({ id: "asks", labelledBy: "asks-label" })}</>;
}

describe("profiles that ask before they act", () => {
  it("offers a box per profile, with the built-ins that ask by default ticked and fixed", () => {
    const boxes = asksBoxes(read(LOCAL(null, ["work"]), ["claude", "opencode", "codex", "work"]));

    expect(boxes.options.map((one) => [one.value, one.disabled === true])).toEqual([
      ["claude", true],
      ["opencode", false],
      ["codex", true],
      ["work", false],
    ]);
    expect([...boxes.ticked].sort()).toEqual(["claude", "codex"]);
  });

  it("gives a table of the Local file with a built-in's name a box of its own", () => {
    const boxes = asksBoxes(read(LOCAL(["work"], ["codex"]), ["claude", "opencode", "codex"]));

    expect(boxes.options.find((one) => one.value === "codex")?.disabled).toBeUndefined();
    expect([...boxes.ticked].sort()).toEqual(["claude", "work"]);
  });

  it("marks a profile with one click, through its own command, and reads the files again", async () => {
    const calls: unknown[] = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === "mark_profile_asks") return { kind: "saved", file: LOCAL(["opencode"]) };
      return null;
    });
    let reread = 0;
    const setting = asksSetting(
      read(LOCAL(null), ["claude", "opencode", "codex"], {
        plane: "p1" as ProjectRead["plane"],
        reread: () => (reread += 1),
      }),
    );
    render(<Row setting={setting} />);

    await userEvent.click(screen.getByRole("checkbox", { name: "opencode" }));

    await waitFor(() => expect(reread).toBe(1));
    expect(calls).toContainEqual([
      "mark_profile_asks",
      { plane: "p1", base: "the text read", name: "opencode", asks: true },
    ]);
  });
});
