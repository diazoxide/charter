import { useCallback, useState } from "react";
import { ATTENTION_VIEWS, isPanelView, type OwnViewId, type ViewId } from "./sideViews";
import { commands } from "./bindings";
import { forgetTextSizes, onTextSizes, textSizes, type TextSizes } from "./textSize";
import { CHATS_LIST, type ChatsListPrefs } from "./chatsListPrefs";
import { YOUR_EDITOR } from "./yourEditor";
import { EXPLORER_SECTIONS, type SectionId } from "./explorerSections";
import { forgetGroups } from "./settings/links";
import {
  facetsOf,
  forgetProjectViews,
  onProjectViews,
  projectsTouched,
  touch,
  type Facet,
} from "./projectViews";
import { forgetEntering } from "./settings/entering";
import { forgetDismissals } from "./dismissals";
import type { YourEditor } from "./bindings";
import { atCreation, onLayoutMovedAside, sayAboutThisMachine, type Reading } from "./windowprefs";

/**
 * **The window's layout is data** (ADR 0038, charter-app#141 for the four regions
 * themselves).
 *
 * The four regions used to be four pieces of JSX in a fixed arrangement, with `localStorage`
 * remembering only *which* of them were drawn. Moving one, reordering them, or remembering how
 * big each was meant editing `PlaneView`. This module is the other half of that: an
 * **arrangement** — an array of placements, one per region — and the window is drawn from it.
 * Adding a region is a line in {@link CATALOGUE}, a line in {@link DEFAULT_ARRANGEMENT} and a
 * piece of content for `RegionFrame` to put in a slot. No JSX moves.
 *
 * **Slots are fixed; regions are not.** `RegionFrame` always renders the same three panels —
 * left, centre, right — and a region's `side` says which of them its content goes in. (There
 * was a fourth, along the bottom, until #1676 folded its content into the Changes view.)
 * That is not timidity: `react-resizable-panels` throws *"Panel constraints not found for
 * index 3"* from a document listener when a panel leaves a live group, measured in
 * charter-app#141, and clamping a panel's constraints to zero throws the same way. Fixed slots
 * mean the panel list never changes, so `side` and `order` are free to change while the window
 * is up — a region moves by moving its *content*, and a slot with nothing shown in it collapses
 * by exactly the mechanism a hidden region already used.
 *
 * **Where this is kept: a file, injected at creation** (M6.9). The arrangement is
 * `charter/layout.json` in charter's config directory, beside `machine.json` and the operator's
 * `theme.json` — `docs/design-system.md` documents the format, because a file is the one form an
 * operator can hand-edit and `side` and `order` have no control in the window yet.
 *
 * **It is read before the window exists, not fetched from it.** A Tauri command is
 * asynchronous; a layout that arrived after the first paint would mean painting the *default*
 * arrangement and then re-laying-out, which is the flash this module exists to remove. So the
 * Rust side reads the file and hands it to the page in the window's initialization script
 * (`windowprefs.ts`), and {@link remembered} is a property access. What the window changes goes
 * back to the file through a command, in the order it was changed.
 *
 * **It is not a field of the machine store** — ADR 0040 amended 0034 for *"how the operator
 * arranged what this file already names"*, and a region arrangement names nothing that file
 * holds.
 *
 * **Web storage held it before this, under `charter.layout`, and is read exactly once more**:
 * on the first launch that finds no file, the old value is drawn and moved into the file
 * ({@link settleLayout}), and the key is removed once the file has it. After that nothing reads
 * it; a second store answering the same question is how the two come to disagree.
 *
 * **A file that is wrong never costs the window.** The Rust side refuses a file that is not a
 * layout at all; {@link load} drops what this build does not know field by field. Either way
 * the window draws what it can, and says what it put right in the alerts drawer rather than a
 * console nobody reads.
 *
 * **The older key is not read either.** charter-app#141's `charter.regions.shown` held which regions were
 * drawn and nothing else. It is not migrated: carrying a second format forward is permanent, and
 * the whole cost of dropping it is that a region an operator had put away comes back — visible,
 * and one click to undo. Losing a *size* would be silent; losing a hidden region is not.
 */

/** A region. Data, but a closed set in this build: nothing outside the app contributes one
 *  until ADR 0041's plugin runtime exists, and a `Record` keyed on it is what makes
 *  the catalogue exhaustive at compile time.
 *
 *  **`navigation` was `explorer` in version 1 of the file** (#1673): the left region holds
 *  the Chats and Explorer views now, so the explorer is one of its views and no longer its
 *  name. {@link load} reads the old id as the new one.
 *
 *  **There is no `bottom` region any more** (#1676, B-7): what it drew is the navigation
 *  region's Changes view, so the terminals have the window's whole height. A file that still
 *  places it is read without it, and without a word ({@link RETIRED}). */
export type RegionId = "navigation" | "aside";

// A view (`sideViews.ts`): what a region with an activity bar shows, one at a time.
export { ATTENTION_VIEWS, VIEWS, type OwnViewId, type PanelViewId, type ViewId } from "./sideViews";

/** Where a region can be put. These are the slots `RegionFrame` draws, and the centre is not
 *  one of them: the terminal panes are the product, and a window with no centre is not a state
 *  the operator can get into by pressing something. Nothing is drawn along the bottom (#1676):
 *  a side is the whole height of the window, as the terminals are. */
export type Side = "left" | "right";

/** Every slot, in the order the toggle buttons list their regions. */
export const SIDES: readonly Side[] = ["left", "right"];

/**
 * How far a slot may be dragged.
 *
 * **A bound belongs to the slot and not to the region in it**, and that is charter-app#141's
 * finding rather than a preference: changing a panel's `minSize` or `maxSize` re-registers it
 * and a separator recalculating in the gap indexes past the end of the constraint list, from a
 * document listener no `try` of ours can reach. A bound derived from whichever regions happen
 * to be in a slot would change the moment one moved. These are written once and never move, so
 * a region is free to.
 */
export const SLOTS: Record<Side, { least: number; most: number; floor?: string }> = {
  // **The left slot has a floor in the text's own unit too** (#1499): 8% of the narrowest
  // window is about 80px, where a nested row of the Chats list has no room left for its state.
  // 11rem is a row's twist, mark and state word on one line, and it follows the text size, so a
  // person who makes the text bigger does not get a sidebar its rows no longer fit.
  left: { least: 8, most: 45, floor: "11rem" },
  right: { least: 10, most: 45 },
};

/** The least a slot may be dragged to, as its panel is told: its floor where it has one. */
export function leastOf(side: Side): string {
  return SLOTS[side].floor ?? `${SLOTS[side].least}%`;
}

/** What a region *is* — the part that is code and not data, because it cannot be JSON. */
export type Definition = {
  /** What the button that puts it away calls it. */
  name: string;
  /** How big its slot is when nothing has been dragged, as a percentage of the group. */
  size: number;
  /** The views it switches between with an activity bar, in the bar's order. A region without
   *  them draws its content alone. */
  views?: readonly OwnViewId[];
  /** The view open until the person picks another, when it is not the bar's first (#1678: the
   *  right side opens on Memory, and Todos is first on its bar). */
  opens?: ViewId;
  /** It also holds the approved extensions' panels, each a view after purlis's own (#1678). */
  panels?: true;
};

/**
 * Every region this build has.
 *
 * A region's *look* is not in here — a slot's border is the slot's (`App.css`), because
 * `surface.deep` is defined as the topmost strip and the bottom of the window rather than as
 * one region's colour. A region that moves to another side takes on that side's look, which is
 * what "the window is regions" means.
 */
export const CATALOGUE: Record<RegionId, Definition> = {
  // ADR 0038's own reading: the left is navigation, the right is attention. The bottom was
  // state, and is the left's Changes view since #1676.
  navigation: {
    name: "Navigation",
    size: 16,
    // Search and Changes since #1676: Changes holds what the bottom region drew (B-7).
    views: ["chats", "explorer", "search", "changes"],
  },
  // The right is "for you" (ADR 0038 as amended 2026-10-10, B-13): what is asking for you
  // and what you keep, each a view, opening on Memory; the extensions' panels after them.
  aside: {
    name: "Attention",
    size: 20,
    views: ATTENTION_VIEWS,
    opens: "memory",
    panels: true,
  },
};

/** Every region there is, in a fixed order, so anything iterating them is deterministic. */
export const REGION_IDS = Object.keys(CATALOGUE) as RegionId[];

/** Whether `view` is one region `id` can show: one of its own, or an extension's panel where
 *  the region takes them. */
function holds(id: RegionId, view: string): view is ViewId {
  const region = CATALOGUE[id];
  return (
    (region.views as readonly string[] | undefined)?.includes(view) === true ||
    (region.panels === true && isPanelView(view))
  );
}

/** The region whose activity bar holds `view`. */
export function regionOf(view: ViewId): RegionId {
  const found = REGION_IDS.find((id) => holds(id, view));
  if (found === undefined) throw new Error(`no region holds the ${view} view`);
  return found;
}

/** The view a region opens on until the person picks one. */
const opening = (id: RegionId): ViewId | undefined =>
  CATALOGUE[id].opens ?? CATALOGUE[id].views?.[0];

/**
 * The view a region with views shows: the one picked last, else the one it opens on. Nothing
 * for a region without views.
 *
 * `panels` are the extensions' panels the window has now (#1678). One picked last whose
 * extension is not contributing it any more — removed, or not approved on this machine — is not
 * there to show, so the region opens on its default; the file keeps the pick, so it comes back
 * with the panel.
 */
export function openView(placement: Placement, panels: readonly ViewId[] = []): ViewId | undefined {
  const picked = placement.view;
  if (picked !== undefined && (!isPanelView(picked) || panels.includes(picked))) return picked;
  return opening(placement.id);
}

/**
 * Where one region is, and how big. **The whole of what is stored**, which is why the
 * component that draws it is not in here: a React component cannot be JSON, and the moment the
 * stored document held one it would stop being a document.
 */
export type Placement = {
  id: RegionId;
  side: Side;
  /** Position within the side. Ties break on {@link REGION_IDS}, so two regions that were
   *  given the same order still draw in the same sequence every launch. */
  order: number;
  /** Put away. The content is unmounted and the slot is collapsed — `RegionFrame` says why. */
  collapsed: boolean;
  /** How big its slot was left, as a percentage of the group (0..100). Absent until something
   *  has been dragged, in which case the catalogue's default is used. */
  size?: number;
  /** The view open in a region with views, once one was picked (#1673). Absent, the one it
   *  opens on. */
  view?: ViewId;
};

export type Arrangement = Placement[];

/** Today's window (ADR 0038, as #1676 left it), as the default *value* of the arrangement
 *  rather than as a shape in `PlaneView`. */
export const DEFAULT_ARRANGEMENT: Arrangement = [
  { id: "navigation", side: "left", order: 0, collapsed: false },
  { id: "aside", side: "right", order: 0, collapsed: false },
];

/** Where web storage held the arrangement before it was a file. Read once, to move it. */
export const LEGACY_KEY = "charter.layout";

/** The version of the file's format this build writes (#1673): each project's arrangement of
 *  its own, and the open view. `purlis_core::windowprefs` hands the window version 1 too, which
 *  {@link load} moves forward, and refuses any other before the window sees it. */
export const VERSION = 2;

/** The most projects whose arrangements are kept, read or sent: the core's own bound
 *  (`purlis_core::windowprefs::MOST_PROJECTS`), so the file stays inside its size. */
export const MOST_PROJECTS = 32;

/** The most bytes the projects' entries take of the file, as written (#1686): the core's own
 *  bound (`purlis_core::windowprefs::PROJECTS_MOST_BYTES`). With the dismissals' half, the rest
 *  of the file always has room. A write holds what it sends to it, the projects changed longest
 *  ago let go first. */
export const PROJECTS_MOST_BYTES = 24 * 1024;

/** The longest view name kept from the file: an extension's panel key is two short ids. */
const MOST_VIEW = 200;

/** A region id version 1 wrote, by the id it has now. */
const RENAMED: Record<string, RegionId> = { explorer: "navigation" };

/** Region ids an older build wrote for a region this one has folded into another, and so
 *  skips without a word: what it drew is still on screen, somewhere else. The `bottom` region
 *  is the navigation region's Changes view since #1676. */
const RETIRED: readonly string[] = ["bottom"];

/** The side every build before #1676 had along the bottom. A region a file put there goes
 *  back to its own side without a word, and its size, a height there, is not read as a width. */
const RETIRED_SIDE = "bottom";

/**
 * **The preferences the file keeps beside the arrangement and the text sizes** (`layoutPref.ts`,
 * #1686), in the order the file has them: each is written under its key where it is not its
 * default, listened to and forgotten from this one list. A new one is one more row here, and
 * its field in {@link Document}: the build checks that each row's key is a field there, and
 * that no field is left without its row ({@link Unlisted}).
 */
const LAYOUT_PREFS = [YOUR_EDITOR, CHATS_LIST, EXPLORER_SECTIONS] as const satisfies readonly {
  readonly key: PrefField;
}[];

/** The fields of {@link Document} that are preferences of {@link LAYOUT_PREFS}. */
type PrefField = Exclude<keyof Document, "version" | "regions" | "projects" | "text">;

/** A preference field of the document no row of {@link LAYOUT_PREFS} writes: none, or
 *  {@link asDocument} does not type-check, so a preference is never silently left unwritten. */
type Unlisted = Exclude<PrefField, (typeof LAYOUT_PREFS)[number]["key"]>;

/** The document, as it is written to the file. `text` is the two text sizes (`textSize.ts`,
 *  charter-app#283), kept here because they are the same kind of preference — how one operator
 *  likes their window — and this is the one writer of the file. */
type Document = {
  version: typeof VERSION;
  /** The machine's arrangement: what a project with none of its own starts from, and the one
   *  arrangement version 1 had. */
  regions: Arrangement;
  /** Each project's own (#1673), and what its views keep (#1686), by its path: only the
   *  projects this window changed ({@link projectsSent}), since the core keeps the file's
   *  others. */
  projects?: Record<string, ProjectEntry>;
  text: TextSizes;
  /** Your editor (`yourEditor.ts`, RC-20), when one is chosen. */
  editor?: YourEditor;
  /** How the Chats list is drawn (`chatsListPrefs.ts`, #1499), when it is not the default. */
  chats?: ChatsListPrefs;
  /** Explorer's folded sections (`explorerSections.ts`, #1677), when any is. */
  explorer?: { closed: SectionId[] };
};

/** One project's entry: its own arrangement, once it has one, and what its views keep
 *  (`projectViews.ts`, B-11). */
type ProjectEntry = { regions?: Arrangement } & Partial<Record<Facet, unknown>>;

/** A document read field by field, and what had to be put right to read it. */
export type Loaded = {
  regions: Arrangement;
  /** Each project's own arrangement the file holds, by path. A `Map`, so no path can be a key
   *  that reaches a prototype. */
  projects: ReadonlyMap<string, Arrangement>;
  said: string[];
};

/**
 * The arrangement, and the two things that change it while the window is up.
 *
 * `side` and `order` change through {@link move}. Nothing in this build calls it — there is no
 * reorder control yet, and adding one is M6.4's — but it is the operation the whole module is
 * shaped around, it is safe against charter-app#141's throw because the slots are fixed, and it
 * is tested. A feature that has to rewrite this module to arrive was not made cheap by it.
 */
export function useArrangement(project?: string): {
  arrangement: Arrangement;
  /** Put a region away, or bring it back. */
  toggle: (id: RegionId) => void;
  /** Move a region to a side, at a position within it. */
  move: (id: RegionId, side: Side, order: number) => void;
  /** Remember how big each slot was left. Called with the group's settled layout. */
  resized: (sizes: Partial<Record<Side, number>>) => void;
  /** A press of a view's icon on its activity bar: {@link picked}. `panels` are the
   *  extensions' panels the bar drew, which decide what was open. */
  pick: (view: ViewId, panels?: readonly ViewId[]) => void;
  /** A view asked for by a key or the palette: {@link showing}. */
  show: (view: ViewId) => void;
} {
  const [arrangement, setArrangement] = useState<Arrangement>(() => remembered(project));

  const change = useCallback(
    (how: (was: Arrangement) => Arrangement) => {
      setArrangement((was) => {
        const next = how(was);
        remember(next, project);
        return next;
      });
    },
    [project],
  );

  const pick = useCallback(
    (view: ViewId, panels?: readonly ViewId[]) => change((was) => picked(was, view, panels)),
    [change],
  );
  const show = useCallback((view: ViewId) => change((was) => showing(was, view)), [change]);

  const toggle = useCallback(
    (id: RegionId) =>
      change((was) =>
        was.map((one) => (one.id === id ? { ...one, collapsed: !one.collapsed } : one)),
      ),
    [change],
  );

  const move = useCallback(
    (id: RegionId, side: Side, order: number) =>
      change((was) => was.map((one) => (one.id === id ? { ...one, side, order } : one))),
    [change],
  );

  const resized = useCallback(
    (sizes: Partial<Record<Side, number>>) =>
      change((was) =>
        was.map((one) => {
          const size = sizes[one.side];
          // Every shown region in the slot, not just the first: the slot is what was dragged,
          // and a region that is later moved out of a shared slot should take the width it was
          // actually drawn at rather than a width it never had. A region that is put away was
          // not drawn, so it keeps the size it had when it was.
          return size !== undefined && !one.collapsed && usable(size) ? { ...one, size } : one;
        }),
      ),
    [change],
  );

  return { arrangement, toggle, move, resized, pick, show };
}

/**
 * **A press of a view's icon, as VS Code's activity bar answers it** (B-1, #1673): the open view
 * of a side that is out puts the side away; any other view opens, and a side that was away comes
 * back on it. The view stays mounted either way (`RegionFrame`).
 */
export function picked(
  arrangement: Arrangement,
  view: ViewId,
  panels: readonly ViewId[] = [],
): Arrangement {
  const region = regionOf(view);
  const was = arrangement.find((one) => one.id === region);
  if (was !== undefined && !was.collapsed && openView(was, panels) === view) {
    return arrangement.map((one) => (one.id === region ? { ...one, collapsed: true } : one));
  }
  return showing(arrangement, view);
}

/** `view` open and its side out, whatever was there: what a key or a palette row asks for,
 *  which never puts a side away. */
export function showing(arrangement: Arrangement, view: ViewId): Arrangement {
  const region = regionOf(view);
  return arrangement.map((one) => (one.id === region ? { ...one, view, collapsed: false } : one));
}

/**
 * The arrangement this launch started from, and what it cost to get it.
 *
 * - **A file charter could use**: that file, loaded field by field.
 * - **A file charter could not**: the default, and the reason.
 * - **No file**: what web storage held before the file existed, if it held anything — the
 *   window is drawn from it, and {@link settleLayout} moves it into the file.
 *
 * Pure but for the one read of web storage, and that read happens only while there is no file.
 */
export function startingLayout(
  layout: Reading = atCreation().layout,
): Loaded & { trouble?: string; legacy?: boolean } {
  if (layout.found) {
    if (layout.trouble !== null) return { ...nothingHeld(), trouble: layout.trouble };
    return load(layout.document);
  }
  let held: string | null = null;
  try {
    held = globalThis.localStorage?.getItem(LEGACY_KEY) ?? null;
  } catch {
    // A webview that refuses storage has nothing to move.
  }
  if (held === null) return nothingHeld();
  try {
    return { ...load(JSON.parse(held)), legacy: true };
  } catch {
    // Not JSON: there is nothing in it worth moving, and the next change writes the file.
    return nothingHeld();
  }
}

const nothingHeld = (): Loaded => ({ regions: DEFAULT_ARRANGEMENT, projects: new Map(), said: [] });

/**
 * The arrangement as the window last left it, for `project`: what this launch changed in it,
 * else what the file kept for it, else the machine's — the last one changed in any project this
 * launch, or the file's. So a project opened after the person moved something elsewhere gets
 * what they moved, and a project they arranged gets its own back (#1673).
 */
export function remembered(project?: string): Arrangement {
  if (project !== undefined) {
    const own = changedIn.get(project);
    if (own !== undefined) return own;
  }
  const started = movedAside ? nothingHeld() : startingLayout();
  return (
    (project !== undefined ? started.projects.get(project) : undefined) ??
    changed ??
    started.regions
  );
}

/** What the window has changed the machine's arrangement to this launch, if anything. */
let changed: Arrangement | undefined;
/** What it changed each project's to, by path, oldest first. */
const changedIn = new Map<string, Arrangement>();
/** Whether the file was moved aside this launch (Use the default layout, #1289): what it held
 *  is no project's starting point any more. */
let movedAside = false;
onLayoutMovedAside(() => {
  movedAside = true;
  changed = undefined;
  changedIn.clear();
});

/** Forgets what this launch changed, as a new launch would. For tests, which are many launches
 *  in one module. */
export function forgetThisLaunch(): void {
  changed = undefined;
  changedIn.clear();
  movedAside = false;
  writing = Promise.resolve();
  forgetTextSizes();
  for (const pref of LAYOUT_PREFS) pref.forget();
  forgetDismissals();
  forgetProjectViews();
  forgetGroups();
  forgetEntering();
  clearTimeout(textWrite);
}

/**
 * **After the first frame**: says what the layout file cost, and moves web storage's old value
 * into the file.
 *
 * Called once by `main.tsx`. Nothing here is on the way to the first paint — that was drawn from
 * {@link startingLayout} already — so the move is an ordinary asynchronous command, and the old
 * key is removed only once the core says the file has it. A move that fails leaves the key where
 * it is, so the next launch tries again rather than losing the arrangement.
 */
export async function settleLayout(layout: Reading = atCreation().layout): Promise<void> {
  const started = startingLayout(layout);
  sayWhatTheLayoutCost(layout.path, started);
  if (!started.legacy) return;
  const moved = await commands
    .adoptLayout(JSON.stringify(asDocument(started.regions)))
    .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
  if (moved.status === "error") {
    sayAboutThisMachine("layout", {
      severity: "warn",
      detail: `purlis could not move the arrangement it kept in the window into ${where(layout.path)}: ${moved.error}`,
      remedy: "nothing to do: it is still drawn, and the next launch tries again",
    });
    return;
  }
  // The key is about to go, and a project opened later this launch must still get what it held
  // — {@link remembered} would otherwise find neither a file in the reading nor a key.
  changed ??= started.regions;
  try {
    globalThis.localStorage?.removeItem(LEGACY_KEY);
  } catch {
    // Nothing to remove from a webview that refuses storage.
  }
}

function sayWhatTheLayoutCost(path: string, started: ReturnType<typeof startingLayout>): void {
  if (started.trouble !== undefined) {
    sayAboutThisMachine("layout", {
      severity: "warn",
      detail: `${started.trouble} — the window is drawn in the default arrangement`,
      remedy: `fix ${where(path)}, or use the default layout, which moves it aside; the next change you make to the layout replaces it`,
      defaultLayout: true,
    });
  } else if (started.said.length > 0 && !started.legacy) {
    sayAboutThisMachine("layout", {
      severity: "warn",
      detail: `${where(path)}: ${started.said.join("; ")}`,
      remedy: `fix ${where(path)}; the next change you make to the layout rewrites it without these`,
    });
  }
}

const where = (path: string) => path || "the layout file";

const asDocument = (regions: Arrangement): Document => {
  const listed: [Unlisted] extends [never] ? typeof LAYOUT_PREFS : never = LAYOUT_PREFS;
  const projects = projectsSent();
  return {
    version: VERSION,
    regions,
    ...(projects !== undefined ? { projects } : {}),
    text: textSizes(),
    ...Object.fromEntries(
      listed.flatMap((pref) => {
        const written = pref.written();
        return written === undefined ? [] : [[pref.key, written]];
      }),
    ),
  };
};

/**
 * **The projects this launch changed, each with its whole entry** (#1673, #1686): its
 * arrangement — this launch's, else the file's, else none, so it starts from the machine's — and
 * every facet its views keep. The core keeps every other project's entry as the file has it.
 *
 * **Held to {@link MOST_PROJECTS} and {@link PROJECTS_MOST_BYTES}**, as written: the project
 * changed last is taken first, and the projects changed longest ago are let go once the next
 * would not fit. Oldest first, as `changedIn` was always sent.
 */
function projectsSent(): Record<string, ProjectEntry> | undefined {
  const started = movedAside ? nothingHeld() : startingLayout();
  const taken: [string, ProjectEntry][] = [];
  for (const project of projectsTouched().reverse()) {
    if (taken.length === MOST_PROJECTS) break;
    const own = changedIn.get(project) ?? started.projects.get(project);
    const entry: ProjectEntry = {
      ...(own !== undefined ? { regions: own } : {}),
      ...facetsOf(project),
    };
    if (Object.keys(entry).length === 0) continue;
    // Measured whole, as the core measures the map: at most 32 small entries.
    if (bytesOf(Object.fromEntries([...taken, [project, entry]])) > PROJECTS_MOST_BYTES) break;
    taken.push([project, entry]);
  }
  return taken.length === 0 ? undefined : Object.fromEntries(taken.reverse());
}

/** `value`'s bytes as the core writes it: pretty, two spaces, UTF-8. */
const bytesOf = (value: unknown): number =>
  new TextEncoder().encode(JSON.stringify(value, null, 2)).length;

/**
 * A text size changed: the file is rewritten with it, and with the arrangement as it stands —
 * **once the sizes settle**, not per step. A slider dragged from 10 to 24 is fourteen changes
 * in a second, each drawn at once; the file only needs the last.
 */
export const TEXT_WRITE_SETTLES_MS = 300;
let textWrite: ReturnType<typeof setTimeout> | undefined;
onTextSizes(() => {
  clearTimeout(textWrite);
  textWrite = setTimeout(() => remember(remembered()), TEXT_WRITE_SETTLES_MS);
});

/** One of the preferences the file keeps beside the arrangement changed: your editor (RC-20),
 *  how the Chats list is drawn (#1499), Explorer's folded sections (#1677). One change, written
 *  at once. */
for (const pref of LAYOUT_PREFS) pref.on(() => remember(remembered()));

/** What a view keeps for a project changed (B-11, #1686): one change, written at once. */
onProjectViews(() => remember(remembered()));

/** Every write, in the order the window made it. Tauri runs commands on a thread pool, and two
 *  writes that raced there could land the older one last. */
let writing: Promise<void> = Promise.resolve();

/**
 * A stored document, read field by field.
 *
 * **Every region is drawn unless the document says otherwise, and every field falls back on
 * its own.** A document written by an older build, by a newer one, or by hand names some of
 * what this build knows and none of what it does not; losing a region with no way to notice it
 * went is worse than ignoring half a preference, which is the rule charter-app#141 set for the
 * key this one replaces. So an unknown region is dropped, a missing one is placed from the
 * default, and a field that is not what it should be is the default's.
 */
export function load(raw: unknown): Loaded {
  const said: string[] = [];
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) {
    said.push("it is not a layout, so every region is where it starts");
    return { regions: DEFAULT_ARRANGEMENT, projects: new Map(), said };
  }
  const regions = placements((raw as { regions?: unknown }).regions, said);
  const projects = new Map<string, Arrangement>();
  const theirs = (raw as { projects?: unknown }).projects;
  if (
    theirs !== undefined &&
    theirs !== null &&
    typeof theirs === "object" &&
    !Array.isArray(theirs)
  ) {
    for (const [project, own] of Object.entries(theirs).slice(0, MOST_PROJECTS)) {
      const list =
        own !== null && typeof own === "object"
          ? (own as { regions?: unknown }).regions
          : undefined;
      // An entry that keeps only what its views keep (B-11) has no arrangement of its own: the
      // project starts from the machine's, and there is nothing to say.
      if (list === undefined && own !== null && typeof own === "object" && !Array.isArray(own))
        continue;
      if (!Array.isArray(list)) {
        said.push(
          `${JSON.stringify(project)} has no "regions" list, so it starts from the machine's`,
        );
        continue;
      }
      const ownSaid: string[] = [];
      projects.set(project, placements(list, ownSaid));
      said.push(...ownSaid.map((one) => `in ${JSON.stringify(project)}, ${one}`));
    }
  } else if (theirs !== undefined) {
    said.push(
      '"projects" is not a list of projects by path, so every project starts from the machine\'s',
    );
  }
  return { regions, projects, said };
}

/** One list of placements, read field by field. */
function placements(regions: unknown, said: string[]): Arrangement {
  // A `Map`, and the result is built by walking the DEFAULT arrangement and asking it — never
  // by walking the document. That is what makes an id this build does not have cost nothing:
  // it is simply never asked for, so there is no unknown region to draw and no unknown key to
  // reach a prototype through.
  const held = new Map<string, Record<string, unknown>>();
  if (Array.isArray(regions)) {
    for (const one of regions) {
      if (one === null || typeof one !== "object" || Array.isArray(one)) {
        said.push(`${JSON.stringify(one)} is not a region's placement, so it was skipped`);
        continue;
      }
      const placement = one as Record<string, unknown>;
      // Version 1's id, read as the one it has now; a file naming both keeps the new one.
      const id =
        typeof placement.id === "string" &&
        Object.prototype.hasOwnProperty.call(RENAMED, placement.id)
          ? RENAMED[placement.id]
          : placement.id;
      if (typeof id !== "string") {
        said.push("a placement with no id was skipped");
      } else if (RETIRED.includes(id)) {
        // Folded into another region (#1676): nothing of the person's is lost by skipping it.
      } else if (!(REGION_IDS as string[]).includes(id)) {
        said.push(
          `${JSON.stringify(id)} is not a region this purlis has (${REGION_IDS.join(", ")}), so it was left out`,
        );
      } else if (!(held.has(id) && id !== placement.id)) {
        held.set(id, placement);
      }
    }
  } else {
    said.push('there is no "regions" list, so every region is where it starts');
  }

  return DEFAULT_ARRANGEMENT.map((fallback) => {
    const one = held.get(fallback.id);
    if (one === undefined) return fallback;
    const sideRetired = one.side === RETIRED_SIDE;
    if (one.side !== undefined && !isSide(one.side) && !sideRetired) {
      said.push(
        `${fallback.id}'s side ${JSON.stringify(one.side)} is not left or right, so it is on the ${fallback.side}`,
      );
    }
    if (one.order !== undefined && !Number.isFinite(one.order)) {
      said.push(`${fallback.id}'s order ${JSON.stringify(one.order)} is not a number`);
    }
    if (one.size !== undefined && !usable(one.size) && !sideRetired) {
      said.push(`${fallback.id}'s size ${JSON.stringify(one.size)} is not a percentage above 0`);
    }
    const views = CATALOGUE[fallback.id].views;
    // An extension's panel is kept by name, bounded, whether or not it is contributed now: its
    // extension may be approved again, and `openView` draws the default while it is not.
    const view =
      typeof one.view === "string" && one.view.length <= MOST_VIEW && holds(fallback.id, one.view)
        ? one.view
        : undefined;
    if (one.view !== undefined && view === undefined) {
      said.push(
        views === undefined
          ? `${fallback.id} has no views, so its view ${JSON.stringify(one.view)} was left out`
          : `${JSON.stringify(one.view)} is not a view of ${fallback.id} (${views.join(", ")}${CATALOGUE[fallback.id].panels ? ", or an extension's panel" : ""}), so it opens on ${opening(fallback.id)}`,
      );
    }
    return {
      id: fallback.id,
      side: isSide(one.side) ? one.side : fallback.side,
      order: Number.isFinite(one.order) ? (one.order as number) : fallback.order,
      // Only `true` puts a region away. Anything else — missing, a string, a number — is a
      // region charter cannot read the answer for, and it is SHOWN.
      collapsed: one.collapsed === true,
      ...(usable(one.size) && !sideRetired ? { size: one.size } : {}),
      ...(view !== undefined ? { view } : {}),
    };
  });
}

/**
 * Keeps the arrangement: for the rest of this launch at once, and in the file behind it.
 *
 * A write that fails is said in the alerts drawer and costs nothing else: the window keeps
 * drawing what the operator did, and only the next launch would not know. A write that lands
 * takes back whatever the drawer was saying about the file, because the file is now one this
 * window wrote.
 */
function remember(arrangement: Arrangement, project?: string): void {
  changed = arrangement;
  if (project !== undefined) {
    // Last changed last, so the bound lets go of the project arranged longest ago.
    changedIn.delete(project);
    changedIn.set(project, arrangement);
    touch(project);
    if (changedIn.size > MOST_PROJECTS) changedIn.delete(changedIn.keys().next().value as string);
  }
  const text = JSON.stringify(asDocument(arrangement));
  writing = writing.then(async () => {
    const kept = await commands
      .writeLayout(text)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (kept.status === "error") {
      sayAboutThisMachine("layout", {
        severity: "warn",
        detail: `purlis could not keep the layout: ${kept.error}`,
        remedy: "the window keeps it until you quit; the next launch starts from the last one kept",
      });
    } else {
      sayAboutThisMachine("layout", undefined);
    }
  });
}

/** A percentage a panel can actually be given. `0` is excluded on purpose: a slot is collapsed
 *  by `collapse()` and never by being sized to nothing, so a stored zero is a measurement
 *  taken while a region was away and is not a width to come back to. */
function usable(size: unknown): size is number {
  return typeof size === "number" && Number.isFinite(size) && size > 0 && size <= 100;
}

function isSide(side: unknown): side is Side {
  return side === "left" || side === "right";
}

/**
 * The arrangement as the slots it draws: every side, in `order`, ties broken on
 * {@link REGION_IDS}.
 *
 * Every side is present even when nothing is in it, because `RegionFrame` renders a panel per
 * side whatever the data says — see this module's docstring for why the panel list is fixed.
 */
export function inSlots(arrangement: Arrangement): Record<Side, Placement[]> {
  const slots: Record<Side, Placement[]> = { left: [], right: [] };
  for (const one of arrangement) slots[one.side].push(one);
  for (const side of SIDES) {
    slots[side].sort(
      (a, b) => a.order - b.order || REGION_IDS.indexOf(a.id) - REGION_IDS.indexOf(b.id),
    );
  }
  return slots;
}

/** The regions drawn in a slot. */
export const shownIn = (placed: Placement[]): Placement[] => placed.filter((one) => !one.collapsed);

/**
 * How big a slot starts.
 *
 * The first region drawn in it owns the slot's size, and the catalogue answers when it has no
 * remembered one. A slot with nothing shown in it is about to be collapsed, so its size is
 * whatever the first region placed there would have taken — which is the width `expand()` gives
 * back when the region comes out of hiding.
 */
export function slotSize(side: Side, placed: Placement[]): number {
  const first = shownIn(placed)[0] ?? placed[0];
  if (first === undefined) return SLOTS[side].least;
  return first.size ?? CATALOGUE[first.id].size;
}

/**
 * How big a slot is on the window's very first frame.
 *
 * **Nothing, when nothing is drawn in it, and that is the fix for the flash** (charter-app#141
 * left it: a region that was put away drew full size for one frame at every launch and was then
 * taken away). The old shape was a panel sized normally and an effect that collapsed it, and an
 * effect runs after the browser has painted. `RegionFrame`'s `Slot` says what happens to a
 * layout effect that tries to do it sooner.
 *
 * `0` is a size a collapsible panel accepts: `react-resizable-panels` snaps a size below half
 * the minimum to `collapsedSize`, which is `0%` here, so the slot is laid out collapsed before
 * anything has been painted and no effect has to undo anything.
 */
export function startingSize(side: Side, placed: Placement[]): number {
  return shownIn(placed).length === 0 ? 0 : slotSize(side, placed);
}
