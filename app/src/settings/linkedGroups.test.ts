import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * **Every Settings group the core can link to is one the window declares** (SE-22, D-SE22f).
 *
 * A doctor row names the group its fix is made in by the group's address
 * (`purlis_core::doctor::SettingsGroup`), and the window shows that group. An address the
 * window does not declare would land on the level's first group without a word, so the two
 * lists are held together here: the core's one `match` that spells each address, read from its
 * source, against the group ids `project.ts` declares.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const read = (path: string) => readFileSync(join(ROOT, path), "utf8");

/**
 * The addresses `SettingsGroup::id` spells, in its order. Every arm is read whatever its
 * address holds, and there must be one per variant: an arm this pattern cannot read (wrapped
 * by rustfmt, or returning a const) fails here instead of dropping out of the check.
 */
function coreGroups(): string[] {
  const source = read("crates/purlis-core/src/doctor/mod.rs");
  const variants = /pub enum SettingsGroup \{([^}]*)\}/.exec(source);
  expect(variants, "SettingsGroup is the enum the core links by").not.toBeNull();
  const body = /impl SettingsGroup \{[\s\S]*?pub const fn id\(self\)[\s\S]*?\n {4}\}/.exec(source);
  expect(body, "SettingsGroup::id is where the core spells each address").not.toBeNull();
  const ids = [...(body?.[0] ?? "").matchAll(/Self::\w+ => "([^"]+)",/g)].map((one) => one[1]);
  const count = [...(variants?.[1] ?? "").matchAll(/^\s*[A-Z]\w*,/gm)].length;
  expect(ids.length, "one arm per variant: SettingsGroup::id spells every group").toBe(count);
  return ids;
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
