/**
 * **The words the first-hour surfaces never say** (ADR 0072 §3, settled by V6): charter's nouns
 * outside the five concepts' budget. Each is still used where it belongs — Settings, a view tab
 * opened later, the CLI, the docs. FR-3's UI-string test is meant to reuse this list.
 *
 * Matched as whole words, ignoring case, except `LIVE` and `LOCAL`, which are the save modes'
 * shouted labels: a lower-case "local" is an ordinary word, and is in `charter.local.toml`.
 */
export const OUTSIDE_THE_FIRST_HOUR = [
  "plane",
  "piece",
  "worktree",
  "run",
  "device",
  "vault",
  "mode",
  "harness",
  "profile",
  "extension",
  "capability",
  "curation",
  "change",
  "member",
  "inventory",
  "strip",
  "show-more",
  "LIVE",
  "LOCAL",
] as const;

/** The listed words `text` says, each once, in the list's order. */
export function wordsOutsideTheFirstHour(text: string): string[] {
  return OUTSIDE_THE_FIRST_HOUR.filter((word) => {
    const shouted = word === "LIVE" || word === "LOCAL";
    // Every character a regular expression gives a meaning, backslash included.
    const escaped = word.replace(/[\\^$.*+?()[\]{}|-]/g, "\\$&");
    return new RegExp(`(^|[^\\w-])${escaped}s?($|[^\\w-])`, shouted ? "" : "i").test(text);
  });
}
