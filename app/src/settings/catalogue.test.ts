import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { SETTINGS_GROUPS } from "./catalogue";
import { youGroups } from "./you";

/**
 * **The catalogue of Settings groups says what the builders build** (#1201).
 *
 * The You level is built with nothing read, so its groups are compared whole, in order. The
 * Project and Workspace levels need their files read, so their builders' sources are read
 * instead, as `linkedGroups.test.ts` reads the core's: every group object — an `id` that is a
 * group's address, written as a string or as a constant, with its `label` on the next line — is
 * held against the catalogue both ways.
 *
 * **A group is read by its shape, never by how many parts its id has** (#1201): an object whose
 * `id` and `label` are followed, at their own depth, by its `settings`. A row has an `id` and a
 * `label` too (`workspace.repos.cloned`), and no `settings` of its own, so a sub page's
 * three-part id (`project.sandbox.mine`) is a group and a row's is not, whether either is
 * written as a string or as a constant.
 */

const HERE = dirname(fileURLToPath(import.meta.url));
const BUILDERS = [
  "project.ts",
  "workspace.tsx",
  "dispatch.tsx",
  "GrantedList.tsx",
  "you.tsx",
  "thisMachine.tsx",
];

/** A group's address at any depth: a level, then one or more parts. */
const ADDRESS = String.raw`(?:you|project|workspace)(?:\.[a-z][a-z0-9-]*)+`;

/** Each `export const NAME = "level.address";` the builders declare, by name. */
function constants(sources: readonly string[]): Map<string, string> {
  const named = new Map<string, string>();
  for (const source of sources)
    for (const [, name, value] of source.matchAll(
      new RegExp(String.raw`^export const ([A-Z_]+) = "(${ADDRESS})";$`, "gm"),
    ))
      named.set(name, value);
  return named;
}

/** Whether the object whose `label` line ends at `from` in `source`, its keys indented by
 *  `indent`, has `settings` of its own: a group does, a row does not. */
function hasSettings(source: string, from: number, indent: string): boolean {
  for (const line of source.slice(from).split("\n").slice(1)) {
    if (line.trim() === "") continue;
    const depth = line.length - line.trimStart().length;
    if (depth < indent.length) return false;
    if (depth === indent.length && line.trimStart().startsWith("settings:")) return true;
  }
  return false;
}

/** Every group `sources` declare, as `[id, label]` in the order written: an `id` written as a
 *  string or as one of `named`, its `label` on the next line, and its own `settings` after. */
function groupsIn(
  sources: readonly string[],
  named: ReadonlyMap<string, string>,
): [string, string][] {
  const groups: [string, string][] = [];
  const pair = new RegExp(
    String.raw`^( +)id: (?:"(${ADDRESS})"|([A-Z_]+)),\n\1label: "([^"]+)",$`,
    "gm",
  );
  for (const source of sources)
    for (const found of source.matchAll(pair)) {
      const [whole, indent, literal, constant, label] = found;
      const id = literal ?? named.get(constant);
      if (id === undefined || !hasSettings(source, found.index + whole.length, indent)) continue;
      groups.push([id, label]);
    }
  return groups;
}

/** Every group the builders declare at `level`, as `id → label`. */
function built(level: "project" | "workspace"): Map<string, string> {
  const sources = BUILDERS.map((file) => readFileSync(join(HERE, file), "utf8"));
  const groups = new Map<string, string>();
  for (const [id, label] of groupsIn(sources, constants(sources))) {
    if (!id.startsWith(`${level}.`)) continue;
    expect(groups.get(id) ?? label, `${id} is labelled one way`).toBe(label);
    groups.set(id, label);
  }
  return groups;
}

describe("the catalogue of Settings groups", () => {
  it("holds the You level's groups, in its order", () => {
    expect(SETTINGS_GROUPS.you).toEqual(youGroups().map(({ id, label }) => ({ id, label })));
  });

  it.each(["project", "workspace"] as const)(
    "holds every group the %s level builds, by its own label, and none it does not",
    (level) => {
      const groups = built(level);
      // The read itself is checked, so a pattern that stopped matching cannot pass as empty.
      expect(groups.get(`${level}.extensions`)).toBe("Extensions");
      expect(groups.get(`${level}.dispatch`)).toBe("Dispatch");

      const listed = new Map(SETTINGS_GROUPS[level].map((one) => [one.id, one.label]));
      expect([...listed].sort()).toEqual([...groups].sort());
    },
  );

  it("reads a group by its shape, at any depth of id, and never a row", () => {
    // The mutation the train-23 review made by hand, kept: a sub page whose literal id has
    // three parts is a group, and a row beside it with three parts of its own is not.
    const source = [
      "  return [",
      "    {",
      '      id: "project.sandbox.x",',
      '      label: "A sub page",',
      '      help: "One written as a string.",',
      "      settings: [",
      "        {",
      '          id: "project.sandbox.x.row",',
      '          label: "A row",',
      '          help: "Not a group.",',
      "        },",
      "      ],",
      "      sub: true,",
      "    },",
      "    {",
      "      id: SUB,",
      '      label: "By a constant",',
      '      help: "One written as a constant.",',
      "      settings: [],",
      "    },",
      "    {",
      '      id: "workspace.repos.cloned",',
      '      label: "A row on its own",',
      '      help: "No settings of its own.",',
      "    },",
      "  ];",
    ].join("\n");
    expect(groupsIn([source], new Map([["SUB", "project.sandbox.y.z"]]))).toEqual([
      ["project.sandbox.x", "A sub page"],
      ["project.sandbox.y.z", "By a constant"],
    ]);
  });

  it("gives every group one address", () => {
    const ids = Object.values(SETTINGS_GROUPS).flatMap((level) => level.map((one) => one.id));
    expect(new Set(ids).size).toBe(ids.length);
  });
});
