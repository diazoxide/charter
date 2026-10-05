import { inAFile, type FileSetting, type SettingsFileId, type SettingsGroup } from "./groups";

/**
 * **A standing refusal and the field it is about** (NO-7, #1232; the spec on #1221, user story
 * 24): each sentence charter says it does not take from a level's files as they stand, with a
 * link to the setting it names, where the level has one.
 *
 * **The key is read from the sentence the way the core writes it.** Every reader of the
 * settings files starts its sentence with the key it is about, then the file:
 * `plane.mode in charter.toml is not a mode — …`, `settings.theme.icons in
 * workspaces/alpha/workspace.json is …`; a table's or a default's says it as TOML does,
 * `[extensions.x] in charter.toml …`, `[persona] default = "ghost" names no persona …` (the form
 * `picking` in `project.ts` already reads). A sentence of any other shape — a file that does not
 * parse, a profile the loader refused — names no field, and draws no link.
 */

/** Where a refusal's link goes: the group, its words, and the setting when one is named. */
export type StandingLink = { group: string; label: string; setting?: string };

/** One sentence of what charter does not take from the files, and where it is fixed. */
export type Standing = { why: string; to?: StandingLink };

/** One key of a dotted key at the start of `text` — bare or quoted — and what follows it. */
function oneKey(text: string): [string, string] | undefined {
  const bare = /^[A-Za-z0-9_-]+/.exec(text);
  if (bare) return [bare[0], text.slice(bare[0].length)];
  const quote = text[0];
  if (quote === "'") {
    const end = text.indexOf("'", 1);
    return end < 0 ? undefined : [text.slice(1, end), text.slice(end + 1)];
  }
  if (quote !== '"') return undefined;
  let key = "";
  for (let at = 1; at < text.length; at++) {
    const char = text[at];
    if (char === '"') return [key, text.slice(at + 1)];
    if (char === "\\" && at + 1 < text.length) {
      at++;
      key += text[at];
    } else key += char;
  }
  return undefined;
}

/** The dotted key at the start of `text`, and what follows it; `undefined` when there is none. */
function dotted(text: string): [string[], string] | undefined {
  const keys: string[] = [];
  let rest = text;
  for (;;) {
    const one = oneKey(rest);
    if (one === undefined) return undefined;
    keys.push(one[0]);
    rest = one[1];
    if (!rest.startsWith(".")) return [keys, rest];
    rest = rest.slice(1);
  }
}

/**
 * **The key a refusal about `file` is about**, as the core starts its sentence, or `undefined`
 * when it starts with none. `under` is a prefix every key in the file is kept under — a
 * workspace's `settings` — taken off, so the key is the one a setting at the level holds.
 */
export function keyOfRefusal(why: string, file: string, under?: string): string[] | undefined {
  let keys: string[];
  if (why.startsWith("[")) {
    // `[table] key = …` or `[table.id] in <file> …`. A name is a key only when ` =` follows it:
    // in `[harness.x] is in charter.toml …` the word after the table is the sentence's.
    const header = dotted(why.slice(1));
    if (header === undefined || !header[1].startsWith("]")) return undefined;
    keys = header[0];
    const after = header[1].slice(1);
    if (!after.startsWith(` in ${file}`)) {
      const name = after.startsWith(" ") ? oneKey(after.slice(1)) : undefined;
      if (name === undefined || !name[1].startsWith(" =")) return undefined;
      keys = [...keys, name[0]];
    }
  } else {
    const found = dotted(why);
    if (found === undefined || !found[1].startsWith(` in ${file}`)) return undefined;
    keys = found[0];
  }
  if (under !== undefined) {
    if (keys[0] !== under) return undefined;
    keys = keys.slice(1);
  }
  return keys.length > 0 ? keys : undefined;
}

/** Whether `prefix` is where `keys` start: every one of its steps a key, in order. */
function startsWith(keys: readonly string[], prefix: FileSetting["key"]): boolean {
  return (
    prefix.length <= keys.length &&
    prefix.every((step, at) => "key" in step && step.key === keys[at])
  );
}

/** Whether `setting` is kept in the file `which`: its own, or either of a movable one's two. */
function keptIn(setting: FileSetting, which: SettingsFileId): boolean {
  return setting.file === which || (setting.movable === true && which !== "workspace");
}

/**
 * **Each refusal about the file `which` (named `file`), with where it is fixed** among the
 * level's `groups`: the setting whose key is the key the sentence names, or the nearest one
 * above it (`extensions.stats.settings.x` is Persona statistics' settings); else, for a whole
 * table (`theme in … is not a table`), the group of the first setting under it. A key no
 * setting here holds draws no link.
 */
export function standingIn(
  whys: readonly string[],
  file: string,
  which: SettingsFileId,
  groups: readonly SettingsGroup[],
  under?: string,
): Standing[] {
  const settings = groups.flatMap((group) =>
    group.settings
      .filter(inAFile)
      .filter((one) => keptIn(one, which))
      .map((setting) => ({ group, setting })),
  );
  return whys.map((why) => {
    const keys = keyOfRefusal(why, file, under);
    if (keys === undefined) return { why };
    const nearest = settings
      .filter(({ setting }) => startsWith(keys, setting.key))
      .sort((a, b) => b.setting.key.length - a.setting.key.length)[0];
    if (nearest !== undefined)
      return {
        why,
        to: {
          group: nearest.group.id,
          label: `${nearest.group.label} › ${nearest.setting.label}`,
          setting: nearest.setting.id,
        },
      };
    const table = settings.find(({ setting }) =>
      keys.every((key, at) => {
        const step = setting.key[at];
        return step !== undefined && "key" in step && step.key === key;
      }),
    );
    return table === undefined
      ? { why }
      : { why, to: { group: table.group.id, label: table.group.label } };
  });
}
