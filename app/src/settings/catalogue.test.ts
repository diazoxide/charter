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

/** Each `export const NAME = "level.address";` the builders declare, by name. */
function constants(sources: readonly string[]): Map<string, string> {
  const named = new Map<string, string>();
  for (const source of sources)
    for (const [, name, value] of source.matchAll(
      /^export const ([A-Z_]+) = "((?:you|project|workspace)\.[a-z.]+)";$/gm,
    ))
      named.set(name, value);
  return named;
}

/** Every group the builders declare at `level`, as `id → label`. */
function built(level: "project" | "workspace"): Map<string, string> {
  const sources = BUILDERS.map((file) => readFileSync(join(HERE, file), "utf8"));
  const named = constants(sources);
  const groups = new Map<string, string>();
  for (const source of sources)
    for (const [, literal, constant, label] of source.matchAll(
      /^\s+id: (?:"([a-z]+\.[a-z]+)"|([A-Z_]+)),\n\s+label: "([^"]+)",$/gm,
    )) {
      const id = literal ?? named.get(constant);
      if (id === undefined || !id.startsWith(`${level}.`)) continue;
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

  it("gives every group one address", () => {
    const ids = Object.values(SETTINGS_GROUPS).flatMap((level) => level.map((one) => one.id));
    expect(new Set(ids).size).toBe(ids.length);
  });
});
