import { describe, expect, it } from "vitest";
import type { FileSetting, SettingsGroup } from "./groups";
import { keyOfRefusal, standingIn } from "./standing";

/**
 * **Which field a standing refusal is about** (NO-7, #1232), read from the sentences as the
 * core's readers write them (`planesave`, `extension::project`, `harness_plugin`, `theme`,
 * `settings::names_nothing`): the key first, then the file.
 */

describe("the key a refusal names", () => {
  it.each([
    [
      "plane.mode in charter.toml is not a mode — one of off, commit, push, pr, pr-merge",
      ["plane", "mode"],
    ],
    ["repos.widget.sign in charter.toml is not true or false", ["repos", "widget", "sign"]],
    ['repos."my repo".mode in charter.toml is not a mode', ["repos", "my repo", "mode"]],
    ["[extensions.Bad!] in charter.toml is not an extension id", undefined],
    ["[extensions.x_y] in charter.toml is not an extension id", ["extensions", "x_y"]],
    ['[persona] default = "ghost" names no persona in this project', ["persona", "default"]],
    ['theme in charter.toml is not a table — write [theme] with use = "<theme>"', ["theme"]],
    ["charter.toml is not valid TOML", undefined],
    // The profiles loader's: `is` is a word of the sentence, never a key's name.
    [
      "[harness.default] is in charter.toml, which is committed — a profile is this machine's",
      undefined,
    ],
    [
      "[harness.work] is in charter.toml, which is committed — a profile is this machine's",
      undefined,
    ],
    ["[plane] mode holds a value purlis does not read", undefined],
    ["plane.mode in charter.local.toml is not a mode", undefined],
  ])("%s", (why, keys) => {
    expect(keyOfRefusal(why, "charter.toml")).toEqual(keys);
  });

  it("takes a workspace's settings off the front", () => {
    const file = "workspaces/alpha/workspace.json";
    expect(keyOfRefusal(`settings.theme.icons in ${file} is 7`, file, "settings")).toEqual([
      "theme",
      "icons",
    ]);
    expect(keyOfRefusal(`settings in ${file} is not an object`, file, "settings")).toBeUndefined();
    expect(
      keyOfRefusal(`${file} is not a JSON object, so purlis reads no settings`, file, "settings"),
    ).toBeUndefined();
  });
});

const at = (id: string, label: string, keys: string[], over: Partial<FileSetting> = {}) =>
  ({
    id,
    label,
    help: "",
    file: "shared",
    key: keys.map((key) => ({ key })),
    kind: "text",
    read: () => "",
    edits: () => [],
    ...over,
  }) as FileSetting;

const GROUPS: SettingsGroup[] = [
  {
    id: "project.saving",
    label: "Saving",
    help: "",
    settings: [at("project.saving.plane.mode", "Mode", ["plane", "mode"], { movable: true })],
  },
  {
    id: "project.extensions",
    label: "Extensions",
    help: "",
    settings: [
      at("project.extensions.extensions.stats.enabled", "Stats: enabled", [
        "extensions",
        "stats",
        "enabled",
      ]),
    ],
  },
  {
    id: "project.harness",
    label: "Harness & profiles",
    help: "",
    settings: [
      at("project.harness.local.harness.default", "Default profile", ["harness", "default"], {
        file: "local",
      }),
    ],
  },
];

describe("where a refusal is fixed", () => {
  it("is the setting it names, in its group", () => {
    expect(
      standingIn(["plane.mode in charter.toml is not a mode"], "charter.toml", "shared", GROUPS),
    ).toEqual([
      {
        why: "plane.mode in charter.toml is not a mode",
        to: {
          group: "project.saving",
          label: "Saving › Mode",
          setting: "project.saving.plane.mode",
        },
      },
    ]);
  });

  it("is a movable setting's from either file", () => {
    const [one] = standingIn(
      ["plane.mode in charter.local.toml is not a mode"],
      "charter.local.toml",
      "local",
      GROUPS,
    );
    expect(one.to?.setting).toBe("project.saving.plane.mode");
  });

  it("is the group of a whole table the sentence names", () => {
    const [one] = standingIn(
      ["extensions in charter.toml is not a table"],
      "charter.toml",
      "shared",
      GROUPS,
    );
    expect(one.to).toEqual({ group: "project.extensions", label: "Extensions" });
  });

  it("is nowhere for a key no setting of this file holds", () => {
    expect(
      standingIn(
        [
          "harness.default in charter.toml is not read",
          "frame.x in charter.toml is not read",
          "[harness.default] is in charter.toml, which is committed — a profile is this machine's",
          "charter.toml is not valid TOML",
        ],
        "charter.toml",
        "shared",
        GROUPS,
      ).map((one) => one.to),
    ).toEqual([undefined, undefined, undefined, undefined]);
  });
});
