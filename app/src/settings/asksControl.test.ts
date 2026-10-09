import { describe, expect, it } from "vitest";
import type { SettingsFile } from "../bindings";
import { ASKS, asksControl, type ProjectRead } from "./project";

/**
 * **Profiles that ask before they act** (#1522): the box per profile on Harness & profiles that
 * marks it in `[harness] asks` of the Local file, which is the one click back after a task was
 * refused for a profile nobody marked.
 */

const LOCAL = (asks: string[] | null, tables: string[] = []): SettingsFile => ({
  which: "local",
  file: "purlis.local.toml",
  exists: true,
  text: "",
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

const read = (local: SettingsFile, profile?: string[]): ProjectRead =>
  ({ local, entries: profile === undefined ? {} : { profile } }) as unknown as ProjectRead;

describe("profiles that ask before they act", () => {
  it("offers a box per profile, with the built-ins that ask by default ticked and fixed", () => {
    const local = LOCAL(null, ["work"]);
    const control = asksControl(read(local, ["claude", "opencode", "codex", "work"]));

    expect(control.kind).toBe("checks");
    expect(control.options?.map((one) => [one.value, one.disabled === true])).toEqual([
      ["claude", true],
      ["opencode", false],
      ["codex", true],
      ["work", false],
    ]);
    expect(control.read(local).split("\n").sort()).toEqual(["claude", "codex"]);
  });

  it("gives a table of the Local file with a built-in's name a box of its own", () => {
    const local = LOCAL(null, ["codex"]);
    const control = asksControl(read(local, ["claude", "opencode", "codex"]));

    expect(control.options?.find((one) => one.value === "codex")?.disabled).toBeUndefined();
    expect(control.read(local)).toBe("claude");
  });

  it("writes the ticked profiles to [harness] asks, never the built-ins it shows ticked", () => {
    const local = LOCAL(["work"], ["work"]);
    const control = asksControl(read(local, ["claude", "opencode", "codex", "work"]));

    expect(control.read(local).split("\n")).toContain("work");
    expect(control.edits("claude\ncodex\nwork\nopencode", local)).toEqual([
      { path: ASKS, value: { kind: "list", value: ["work", "opencode"] } },
    ]);
    // None ticked: the key goes.
    expect(control.edits("claude\ncodex", local)).toEqual([{ path: ASKS, value: null }]);
  });
});
