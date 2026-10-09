import { useSyncExternalStore } from "react";
import {
  SETTINGS_TAB_TITLE,
  settingsView,
  workspaceSettingsTitle,
  workspaceSettingsView,
  type ViewRef,
} from "../tabs";
import { LEVELS, type GroupLink, type Level } from "./groups";

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

/**
 * A link into Settings: the group's address, and — at the Workspace level — which workspace.
 * It may also name one setting of the group (NO-7, #1232), by its id (`project.saving.plane.mode`):
 * the group is shown and that setting's control is focused.
 */
export type SettingsLink = { group: string; workspace?: string; setting?: string };

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
 * so the group is on screen) from "the person picked a group". `setting` is the setting the last
 * link named, until the tab has focused it ({@link focusedSetting}).
 */
export type Shown = { group: string; linked: number; setting?: string };

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

/** A link landed on `group` at `place` — on its setting `setting`, when it names one. */
export function linkToGroup(place: string, group: string, setting?: string): void {
  put(place, { group, linked: (shown.get(place)?.linked ?? 0) + 1, setting });
}

/** The tab at `place` focused the setting the last link named: a redraw does not do it again. */
export function focusedSetting(place: string): void {
  const was = shown.get(place);
  if (was?.setting !== undefined) put(place, { group: was.group, linked: was.linked });
}

/** The window event a view sends to follow a link into Settings in its project's window: the
 *  Saving view's Notice does (NO-7, #1232), as its ways out send `WAY_OUT`. */
export const SETTINGS_LINK = "charter-settings-link";

/** What a link asked of a project's window: which project, and the link. */
export type SettingsLinkAsk = { plane: string; link: SettingsLink };

/** Asks the window of the project `plane` to follow `link` (`PlaneView` hears it). */
export function askSettingsLink(plane: string, link: SettingsLink): void {
  window.dispatchEvent(
    new CustomEvent<SettingsLinkAsk>(SETTINGS_LINK, { detail: { plane, link } }),
  );
}

/**
 * **A link out of Settings** (#1387, #1388; the spec on #1221, stories 37 and 38): what Settings
 * does not hold itself has one home elsewhere — extensions in the Extensions dialog, a vault or a
 * persona in its own tab — and Settings links to it. The link names a row of the window's
 * catalogue (`actions.ts`) by its id, and the project's window runs that row as the palette
 * would (`PlaneView` hears it). A row the catalogue does not hold now does nothing.
 */
export const SETTINGS_ACTION = "charter-settings-action";

/** What a link out of Settings asked of a project's window: which project, and the row's id. */
export type SettingsActionAsk = { plane: string; action: string };

/** Asks the window of the project `plane` to run the catalogue's row `action`. */
export function askSettingsAction(plane: string, action: string): void {
  window.dispatchEvent(
    new CustomEvent<SettingsActionAsk>(SETTINGS_ACTION, { detail: { plane, action } }),
  );
}

/** The catalogue's rows a link out of Settings runs, by the ids `actions.ts` gives them. */
export const OPEN_EXTENSIONS = "extensions.show";
/** The Extensions group's link, at every level: where an extension is installed and approved. */
export const EXTENSIONS_LINK: GroupLink = { label: "Go to Extensions…", action: OPEN_EXTENSIONS };
/** Opens the vault picker: Settings names "a vault", not one. */
export const PICK_VAULT = "vault.pick";
/** Opens the vault `vault`'s tab. */
export const openVault = (vault: string) => `vault.open:${vault}`;
/** Opens the persona `persona`'s tab. */
export const showPersona = (persona: string) => `persona.show:${persona}`;

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
