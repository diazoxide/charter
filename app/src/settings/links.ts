import { useSyncExternalStore } from "react";
import {
  SETTINGS_TAB_TITLE,
  settingsView,
  workspaceSettingsTitle,
  workspaceSettingsView,
  type ViewRef,
} from "../tabs";
import { LEVELS, type Level } from "./groups";

/**
 * **Deep links into Settings** (SE-22, #1172; the spec on #558, V89c). A group's stable id
 * (`groups.ts`: `project.saving`, `you.editor`) is its address: anything that tells a person to
 * change a setting — a doctor row, a notice — names the group, and following it opens Settings
 * at that group's level with that group shown.
 *
 * **The group is not part of the view** (D-SE22a). The Settings tab is keyed by its level and
 * target (`tabs.settingsView`, `tabs.workspaceSettingsView`), so a link to a group of a level
 * whose tab is open brings that tab forward rather than opening another. Which group a tab
 * shows is held here instead, per level and target — a **place** — for as long as the window
 * runs: the group last looked at there, or the one a link last landed on. That is also how
 * each level remembers its last group (V89b, user story 31), and why it is not written to the
 * launch's record: it is this session's, not the tab's.
 */

/** A link into Settings: the group's address, and — at the Workspace level — which workspace. */
export type SettingsLink = { group: string; workspace?: string };

/** The level a group's address is at — the word before its first dot — or `undefined`. */
export function levelOf(group: string): Level | undefined {
  const at = group.split(".")[0];
  return LEVELS.find((one) => one.id === at)?.id;
}

/**
 * **Where a group is remembered**: a level and its target. You is the machine's, the same in
 * every project; a project's level is that project's; a workspace's is that workspace's.
 */
export function settingsPlace(level: Level, plane?: string, workspace?: string): string {
  if (level === "you") return "you";
  return [level, plane ?? "", level === "workspace" ? (workspace ?? "") : ""].join("\u0000");
}

/**
 * What a place shows: its group, and how many links have landed on it — a count that moves on
 * a link and never on a click, so a tab can tell "a link brought me here" (and clear its filter
 * so the group is on screen) from "the person picked a group".
 */
export type Shown = { group: string; linked: number };

let shown = new Map<string, Shown>();
const listeners = new Set<() => void>();

function put(place: string, next: Shown): void {
  shown = new Map(shown).set(place, next);
  for (const listener of listeners) listener();
}

/** The person picked `group` at `place`: it is the one shown there from now on. */
export function chooseGroup(place: string, group: string): void {
  const was = shown.get(place);
  if (was?.group === group) return;
  put(place, { group, linked: was?.linked ?? 0 });
}

/** A link landed on `group` at `place`. */
export function linkToGroup(place: string, group: string): void {
  put(place, { group, linked: (shown.get(place)?.linked ?? 0) + 1 });
}

/** What `place` shows, redrawn as it changes; `undefined` until a group is picked or linked. */
export function useShownGroup(place: string): Shown | undefined {
  return useSyncExternalStore(subscribe, () => shown.get(place));
}

/** One function for every tab, so React never resubscribes a tab because it redrew. */
function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Forgets every place's group: a fresh window's state, for tests. */
export function forgetGroups(): void {
  shown = new Map();
}

/**
 * **Where a link lands in a project's window**: the place it is remembered at, and the Settings
 * tab to open or bring forward — its view, its title and, for a workspace's, the workspace whose
 * strip it is filed on. `undefined` for a link this window cannot follow: a group of no level
 * it offers, or a workspace's group with no workspace named.
 */
export function landing(
  link: SettingsLink,
  plane: string,
): { place: string; view: ViewRef; title: string; workspace?: string } | undefined {
  const level = levelOf(link.group);
  if (level === "you" || level === "project")
    return {
      place: settingsPlace(level, plane),
      view: settingsView(level),
      title: SETTINGS_TAB_TITLE,
    };
  if (level === "workspace" && link.workspace !== undefined)
    return {
      place: settingsPlace(level, plane, link.workspace),
      view: workspaceSettingsView(link.workspace),
      title: workspaceSettingsTitle(link.workspace),
      workspace: link.workspace,
    };
  return undefined;
}
