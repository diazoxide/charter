import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * **Every Settings group the core can link to is one the window declares** (SE-22, D-SE22f).
 *
 * A doctor row names the group its fix is made in by the group's address
 * (`charter_core::doctor::SettingsGroup`), and the window shows that group. An address the
 * window does not declare would land on the level's first group without a word, so the two
 * lists are held together here: the core's one `match` that spells each address, read from its
 * source, against the group ids `project.ts` declares.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const read = (path: string) => readFileSync(join(ROOT, path), "utf8");

/** The addresses `SettingsGroup::id` spells, in its order. */
function coreGroups(): string[] {
  const source = read("crates/charter-core/src/doctor/mod.rs");
  const body = /impl SettingsGroup \{[\s\S]*?pub const fn id\(self\)[\s\S]*?\n {4}\}/.exec(source);
  expect(body, "SettingsGroup::id is where the core spells each address").not.toBeNull();
  return [...(body?.[0] ?? "").matchAll(/=> "([a-z.]+)"/g)].map((one) => one[1]);
}

/** The group ids the Project level declares: each group's `id: "project.<group>"`. */
function projectGroups(): string[] {
  return [...read("app/src/settings/project.ts").matchAll(/^ {6}id: "(project\.[a-z]+)",$/gm)].map(
    (one) => one[1],
  );
}

describe("the Settings groups the doctor links to", () => {
  it("are each a group the Project level declares", () => {
    const named = coreGroups();
    const declared = projectGroups();

    expect(named.length).toBeGreaterThan(0);
    expect(declared).toContain("project.general");
    expect(named.filter((one) => !declared.includes(one))).toEqual([]);
  });
});
