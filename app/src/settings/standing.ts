import type { SettingsRefusal } from "../bindings";
import { inAFile, type FileSetting, type SettingsFileId, type SettingsGroup } from "./groups";

/**
 * **A standing refusal and the field it is about** (NO-7, #1232; the spec on #1221, user story
 * 24): each sentence charter says it does not take from a level's files as they stand, with a
 * link to the setting it names, where the level has one.
 *
 * **The key comes from the core with the sentence** (#1292): `SettingsRefusal.key`, set by the
 * reader that refused, one step per table or key — so an extension id with dots in it
 * (`extensions.my.ext.enabled`) is one step, which no reading of the sentence could tell. A
 * workspace's is already the key under its `settings`. A refusal with no key — a file that does
 * not parse, a profile the loader refused — draws no link. The window never reads the sentence
 * for it.
 */

/** Where a refusal's link goes: the group, its words, and the setting when one is named. */
export type StandingLink = { group: string; label: string; setting?: string };

/** One sentence of what charter does not take from the files, and where it is fixed. */
export type Standing = { why: string; to?: StandingLink };

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
 * **Each refusal about the file `which`, with where it is fixed** among the level's `groups`:
 * the setting whose key is the refusal's key, or the nearest one above it
 * (`extensions.stats.settings.x` is Persona statistics' settings); else, for a whole table
 * (`theme in … is not a table`), the group of the first setting under it. A key no setting here
 * holds draws no link.
 */
export function standingIn(
  refusals: readonly SettingsRefusal[],
  which: SettingsFileId,
  groups: readonly SettingsGroup[],
): Standing[] {
  const settings = groups.flatMap((group) =>
    group.settings
      .filter(inAFile)
      .filter((one) => keptIn(one, which))
      .map((setting) => ({ group, setting })),
  );
  return refusals.map(({ why, key: keys }) => {
    if (keys === null || keys.length === 0) return { why };
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
