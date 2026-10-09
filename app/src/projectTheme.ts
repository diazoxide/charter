import { useEffect, useState, useSyncExternalStore } from "react";
import { commands, type ExtensionTheme, type PlaneId } from "./bindings";
import {
  BUILT_IN_ICONS,
  DEFAULT_ICONS,
  loadIcons,
  type IconTheme,
  type LoadedIcons,
} from "./theme/icons";

/**
 * **The theme each project this window holds draws** (charter-app#273, ADR 0048) — **in each of
 * its workspaces** (charter-app#281) — kept as `extensionsOn.ts` keeps what each has on, and for
 * the same reason: the answer is the core's (`extension::project::theme::resolve`, asked with
 * `project_theme_drawn` — the record, the project's two files and the workspace's
 * `workspace.json`, no extension's directory), asked once per project and workspace and again
 * after a save. A workspace's `settings.theme.use` is a layer between the project's Shared and
 * Local files, so the question is the pair; `undefined` asks for the project outside every
 * workspace.
 *
 * `null` is an answer: nothing picked a theme, and the window keeps its own. A question that
 * failed is read as that too — a project whose theme cannot be asked must not hold the window's
 * theme back.
 *
 * **Only what something mounted asks about is kept.** The window asks for the workspace in
 * front, each project the window holds for the workspace it is on, in front or not (so a
 * project switch finds its answer already here, FR-27), and a settings tab for its own. The
 * pairs change with every workspace switch, and a workspace can be deleted. Each hook says it
 * is interested while it is mounted; a pair nothing is interested in any more is forgotten, so a
 * change on disk re-asks at most one pair per open project and settings tab, and not every
 * workspace ever focused.
 */

type Where = { plane: PlaneId; workspace: string | undefined };

/** What the core says a project picks: the pick as a file holds it, or `null`. */
type Asked = (
  plane: PlaneId,
  workspace: string | null,
) => Promise<{ status: "ok"; data: string | null } | { status: "error"; error: string }>;

/**
 * One kept answer per project and workspace pair, asked of the core with `askCore`. The theme
 * and the icon theme (FM-3, #1106) are two of these, asked on the same occasions: both are read
 * from the same `[theme]` table, so whatever re-asks one re-asks the other.
 */
function pickStore(askCore: Asked) {
  const whereOf = new Map<string, Where>();
  const known = new Map<string, string | null>();
  /** The newest question out per project and workspace: an answer to an older one is dropped. */
  const latest = new Map<string, number>();
  /** How many answers each pair has had: what a settings tab re-reads its own sentence on, since
   *  an approval in the Extensions dialog tells this store and not the tab. */
  const answers = new Map<string, number>();
  /** How many mounted hooks are asking about each pair. */
  const interest = new Map<string, number>();
  const listeners = new Set<() => void>();

  function ask(plane: PlaneId, workspace: string | undefined) {
    const key = keyOf(plane, workspace);
    whereOf.set(key, { plane, workspace });
    const mine = (latest.get(key) ?? 0) + 1;
    latest.set(key, mine);
    void askCore(plane, workspace ?? null)
      .then((said) => (said.status === "ok" ? (said.data ?? null) : null))
      .catch(() => null)
      .then((drawn) => {
        if (latest.get(key) !== mine) return;
        known.set(key, drawn);
        answers.set(key, (answers.get(key) ?? 0) + 1);
        for (const listener of listeners) listener();
      });
  }

  function changed(plane?: PlaneId) {
    for (const where of [...whereOf.values()])
      if (plane === undefined || where.plane === plane) ask(where.plane, where.workspace);
  }

  function useInterest(plane: PlaneId | undefined, workspace: string | undefined) {
    useEffect(() => {
      if (plane === undefined) return;
      const key = keyOf(plane, workspace);
      interest.set(key, (interest.get(key) ?? 0) + 1);
      if (!latest.has(key)) ask(plane, workspace);
      return () => {
        const left = (interest.get(key) ?? 1) - 1;
        if (left > 0) {
          interest.set(key, left);
          return;
        }
        interest.delete(key);
        whereOf.delete(key);
        known.delete(key);
        latest.delete(key);
        answers.delete(key);
      };
    }, [plane, workspace]);
  }

  function subscribe(listener: () => void) {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }

  function usePick(plane: PlaneId | undefined, workspace?: string): string | null | undefined {
    useInterest(plane, workspace);
    return useSyncExternalStore(subscribe, () =>
      plane === undefined ? undefined : known.get(keyOf(plane, workspace)),
    );
  }

  function useAnswers(plane: PlaneId, workspace?: string): number {
    useInterest(plane, workspace);
    return useSyncExternalStore(subscribe, () => answers.get(keyOf(plane, workspace)) ?? 0);
  }

  function forget() {
    whereOf.clear();
    known.clear();
    latest.clear();
    answers.clear();
    interest.clear();
  }

  const answersOf = (plane: PlaneId, workspace: string | undefined) =>
    answers.get(keyOf(plane, workspace)) ?? 0;

  return { changed, subscribe, answersOf, useInterest, usePick, useAnswers, forget };
}

function keyOf(plane: PlaneId, workspace: string | undefined): string {
  return `${plane}\u0000${workspace ?? ""}`;
}

const themes = pickStore((plane, workspace) => commands.projectThemeDrawn(plane, workspace));
const icons = pickStore((plane, workspace) => commands.projectIconsDrawn(plane, workspace));

/** Ask `plane` again, in every workspace something on screen asks about — after its settings
 *  or a workspace's were saved, the plane changed on disk, or an extension was approved or
 *  removed. With no plane, every one. Its icon theme is asked again too. */
export function projectThemeChanged(plane?: PlaneId) {
  iconThemes = undefined;
  themes.changed(plane);
  icons.changed(plane);
}

/** What `plane` draws in `workspace`, as a file holds it; `null` when nothing picked one;
 *  `undefined` until it has said. Redraws when it changes, asking once. */
export function useProjectTheme(
  plane: PlaneId | undefined,
  workspace?: string,
): string | null | undefined {
  return themes.usePick(plane, workspace);
}

/** Keeps what `plane` draws in `workspace` known while this is mounted, asking once, and draws
 *  nothing: for a project behind the one in front, so that a switch back finds the answer
 *  here rather than asking the core and drawing the window again when it replies (FR-27). An
 *  answer redraws only the hooks that read it. */
export function useProjectThemeKept(plane: PlaneId, workspace?: string): void {
  themes.useInterest(plane, workspace);
}

/** How many times `plane` has answered in `workspace`, asking once: a number that changes
 *  whenever what the window draws there was asked again — after a save, an approval or a
 *  removal, or a change on disk. */
export function useProjectThemeAnswers(plane: PlaneId, workspace?: string): number {
  return themes.useAnswers(plane, workspace);
}

/** Every icon theme an approved extension contributes, asked once and again after a change. */
let iconThemes: Promise<ExtensionTheme[]> | undefined;
/** Each contributed icon theme's text, loaded once, with what it got wrong. */
const loadedIcons = new Map<string, LoadedIcons>();

/** A contributed icon theme's text as icons, and what was wrong with it: charter's own for text
 *  that is not JSON. */
function loaded(text: string): LoadedIcons {
  let icons = loadedIcons.get(text);
  if (icons === undefined) {
    try {
      icons = loadIcons(JSON.parse(text) as unknown);
    } catch {
      icons = { icons: DEFAULT_ICONS, complaints: ["an icon theme is JSON, and this is not"] };
    }
    loadedIcons.set(text, icons);
  }
  return icons;
}

const iconsOf = (text: string): IconTheme => loaded(text).icons;

/** Every icon theme an approved extension contributes, asked once and again after a change
 *  (`projectThemeChanged`). Nothing, when the core could not say. */
export function offeredIconThemes(): Promise<ExtensionTheme[]> {
  iconThemes ??= commands
    .extensionIconThemes()
    .then((said) => (said.status === "ok" ? (said.data ?? []) : []))
    .catch((): ExtensionTheme[] => []);
  return iconThemes;
}

/**
 * **What a contributed icon theme got wrong** (#1145): `loadIcons`' complaints about its text —
 * a path that is not path data, a tone that is not an `icon.*` token, a name mapped to no
 * symbol. What it could not read is drawn as charter's own, so the trees never stop; this is
 * what says so, on the extension's row in the Extensions dialog.
 */
export function iconThemeComplaints(text: string): readonly string[] {
  return loaded(text).complaints;
}

/**
 * **The icons the file trees of `plane` draw in `workspace`** (FM-3, #1106): charter's own until
 * the project says otherwise, a built-in it picks, or the icon theme an extension contributes
 * when that is the pick — whose text is the survey's (`extension_icon_themes`), asked only then.
 * A pick no approved extension contributes draws charter's own.
 */
export function useFileIcons(plane: PlaneId | undefined, workspace?: string): IconTheme {
  const pick = icons.usePick(plane, workspace);
  const asked = useSyncExternalStore(icons.subscribe, () =>
    plane === undefined ? 0 : icons.answersOf(plane, workspace),
  );
  const builtIn =
    pick == null
      ? DEFAULT_ICONS
      : Object.prototype.hasOwnProperty.call(BUILT_IN_ICONS, pick)
        ? BUILT_IN_ICONS[pick]
        : undefined;
  const [theirs, setTheirs] = useState<{ pick: string; icons: IconTheme }>();
  useEffect(() => {
    if (pick == null || builtIn !== undefined) return;
    let live = true;
    void offeredIconThemes().then((offered) => {
      const found = offered.find((one) => `${one.extension}/${one.name}` === pick);
      if (live)
        setTheirs({ pick, icons: found === undefined ? DEFAULT_ICONS : iconsOf(found.text) });
    });
    return () => {
      live = false;
    };
  }, [pick, builtIn, asked]);
  if (builtIn !== undefined) return builtIn;
  return theirs !== undefined && theirs.pick === pick ? theirs.icons : DEFAULT_ICONS;
}

/** For tests: forget every answer. */
export function forgetProjectThemes() {
  themes.forget();
  icons.forget();
  iconThemes = undefined;
  loadedIcons.clear();
}
