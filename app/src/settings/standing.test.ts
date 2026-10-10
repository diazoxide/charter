import { describe, expect, it } from "vitest";
import type { FileSetting, SettingsGroup } from "./groups";
import type { SettingsRefusal } from "../bindings";
import { standingIn } from "./standing";

/**
 * **Where a standing refusal is fixed** (NO-7, #1232), by the key the core hands with it
 * (#1292): the window never reads the sentence for it. Which key each reader's sentence is about
 * is the core's to test (`settings::key_said`, `extension::project::keyed_in`).
 */

const said = (why: string, key: string[] | null): SettingsRefusal => ({ why, key });

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
      standingIn(
        [said("plane.mode in charter.toml is not a mode", ["plane", "mode"])],
        "shared",
        GROUPS,
      ),
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
      [said("plane.mode in charter.local.toml is not a mode", ["plane", "mode"])],
      "local",
      GROUPS,
    );
    expect(one.to?.setting).toBe("project.saving.plane.mode");
  });

  it("is the group of a whole table the sentence names", () => {
    const [one] = standingIn(
      [said("extensions in charter.toml is not a table", ["extensions"])],
      "shared",
      GROUPS,
    );
    expect(one.to).toEqual({ group: "project.extensions", label: "Extensions" });
  });

  it("is an extension's setting when its id holds a dot, as one step of the key", () => {
    const groups: SettingsGroup[] = [
      {
        id: "project.extensions",
        label: "Extensions",
        help: "",
        settings: [
          at("project.extensions.extensions.my.ext.enabled", "My ext: enabled", [
            "extensions",
            "my.ext",
            "enabled",
          ]),
        ],
      },
    ];
    const [one] = standingIn(
      [
        said("extensions.my.ext.enabled in charter.toml is not true or false", [
          "extensions",
          "my.ext",
          "enabled",
        ]),
      ],
      "shared",
      groups,
    );
    expect(one.to?.setting).toBe("project.extensions.extensions.my.ext.enabled");
  });

  it("is a workspace's setting by its key under settings", () => {
    const groups: SettingsGroup[] = [
      {
        id: "workspace.extensions",
        label: "Extensions",
        help: "",
        settings: [
          at(
            "workspace.extensions.extensions.stats.enabled",
            "Stats: enabled",
            ["extensions", "stats", "enabled"],
            { file: "workspace" },
          ),
        ],
      },
    ];
    const [one] = standingIn(
      [
        said("settings.extensions.stats.enabled in workspaces/a/workspace.json is 1", [
          "extensions",
          "stats",
          "enabled",
        ]),
      ],
      "workspace",
      groups,
    );
    expect(one.to?.setting).toBe("workspace.extensions.extensions.stats.enabled");
  });

  it("is nowhere for a key no setting of this file holds, or a refusal with no key", () => {
    expect(
      standingIn(
        [
          said("harness.default in charter.toml is not read", ["harness", "default"]),
          said("frame.x in charter.toml is not read", ["frame", "x"]),
          // A sentence that starts the way a key would, with no key from the core.
          said("plane.mode in charter.toml is not a mode", null),
          said("charter.toml is not valid TOML", null),
        ],
        "shared",
        GROUPS,
      ).map((one) => one.to),
    ).toEqual([undefined, undefined, undefined, undefined]);
  });
});
