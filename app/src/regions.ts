import { useCallback, useState } from "react";
import { commands } from "./bindings";
import { forgetTextSizes, onTextSizes, textSizes, type TextSizes } from "./textSize";
import { atCreation, sayAboutThisMachine, type Reading } from "./windowprefs";

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
 * **Slots are fixed; regions are not.** `RegionFrame` always renders the same four panels —
 * left, centre, right, bottom — and a region's `side` says which of them its content goes in.
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
 *  the catalogue exhaustive at compile time. */
export type RegionId = "explorer" | "aside" | "bottom";

/** Where a region can be put. These are the slots `RegionFrame` draws, and the centre is not
 *  one of them: the terminal panes are the product, and a window with no centre is not a state
 *  the operator can get into by pressing something. */
export type Side = "left" | "right" | "bottom";

/** Every slot, in the order the toggle buttons list their regions. */
export const SIDES: readonly Side[] = ["left", "right", "bottom"];

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
export const SLOTS: Record<Side, { least: number; most: number }> = {
  left: { least: 8, most: 45 },
  right: { least: 10, most: 45 },
  bottom: { least: 6, most: 50 },
};

/** What a region *is* — the part that is code and not data, because it cannot be JSON. */
export type Definition = {
  /** What the button that puts it away calls it. */
  name: string;
  /** How big its slot is when nothing has been dragged, as a percentage of the group. */
  size: number;
};

/**
 * Every region this build has.
 *
 * A region's *look* is not in here — a slot's border is the slot's (`App.css`), because
 * `surface.deep` is defined as the topmost strip and the bottom of the window rather than as
 * one region's colour. A region that moves to another side takes on that side's look, which is
 * what "the window is four regions" means.
 */
export const CATALOGUE: Record<RegionId, Definition> = {
  explorer: { name: "Explorer", size: 16 },
  aside: { name: "Attention", size: 20 },
  bottom: { name: "State", size: 16 },
};

/** Every region there is, in a fixed order, so anything iterating them is deterministic. */
export const REGION_IDS = Object.keys(CATALOGUE) as RegionId[];

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
};

export type Arrangement = Placement[];

/** Today's four-region window (ADR 0038), as the default *value* of the arrangement
 *  rather than as a shape in `PlaneView`. */
export const DEFAULT_ARRANGEMENT: Arrangement = [
  { id: "explorer", side: "left", order: 0, collapsed: false },
  { id: "aside", side: "right", order: 0, collapsed: false },
  { id: "bottom", side: "bottom", order: 0, collapsed: false },
];

/** Where web storage held the arrangement before it was a file. Read once, to move it. */
export const LEGACY_KEY = "charter.layout";

/** The one version of the file's format this build writes. `charter_core::windowprefs` refuses
 *  any other before the window sees it. */
export const VERSION = 1;

/** The document, as it is written to the file. `text` is the two text sizes (`textSize.ts`,
 *  charter-app#283), kept here because they are the same kind of preference — how one operator
 *  likes their window — and this is the one writer of the file. */
type Document = { version: typeof VERSION; regions: Arrangement; text: TextSizes };

/** A document read field by field, and what had to be put right to read it. */
export type Loaded = { regions: Arrangement; said: string[] };

/**
 * The arrangement, and the two things that change it while the window is up.
 *
 * `side` and `order` change through {@link move}. Nothing in this build calls it — there is no
 * reorder control yet, and adding one is M6.4's — but it is the operation the whole module is
 * shaped around, it is safe against charter-app#141's throw because the slots are fixed, and it
 * is tested. A feature that has to rewrite this module to arrive was not made cheap by it.
 */
export function useArrangement(): {
  arrangement: Arrangement;
  /** Put a region away, or bring it back. */
  toggle: (id: RegionId) => void;
  /** Move a region to a side, at a position within it. */
  move: (id: RegionId, side: Side, order: number) => void;
  /** Remember how big each slot was left. Called with the group's settled layout. */
  resized: (sizes: Partial<Record<Side, number>>) => void;
} {
  const [arrangement, setArrangement] = useState<Arrangement>(remembered);

  const change = useCallback((how: (was: Arrangement) => Arrangement) => {
    setArrangement((was) => {
      const next = how(was);
      remember(next);
      return next;
    });
  }, []);

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

  return { arrangement, toggle, move, resized };
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
    if (layout.trouble !== null)
      return { regions: DEFAULT_ARRANGEMENT, said: [], trouble: layout.trouble };
    return load(layout.document);
  }
  let held: string | null = null;
  try {
    held = globalThis.localStorage?.getItem(LEGACY_KEY) ?? null;
  } catch {
    // A webview that refuses storage has nothing to move.
  }
  if (held === null) return { regions: DEFAULT_ARRANGEMENT, said: [] };
  try {
    return { ...load(JSON.parse(held)), legacy: true };
  } catch {
    // Not JSON: there is nothing in it worth moving, and the next change writes the file.
    return { regions: DEFAULT_ARRANGEMENT, said: [] };
  }
}

/**
 * The arrangement as the window last left it: what it changed this launch, or what the launch
 * started from. A project opened after the operator moved something gets what they moved.
 */
export function remembered(): Arrangement {
  return changed ?? startingLayout().regions;
}

/** What the window has changed the arrangement to this launch, if anything. */
let changed: Arrangement | undefined;

/** Forgets what this launch changed, as a new launch would. For tests, which are many launches
 *  in one module. */
export function forgetThisLaunch(): void {
  changed = undefined;
  writing = Promise.resolve();
  forgetTextSizes();
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
      detail: `charter could not move the arrangement it kept in the window into ${where(layout.path)}: ${moved.error}`,
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
      remedy: `fix ${where(path)} or delete it; the next change you make to the layout replaces it`,
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

const asDocument = (regions: Arrangement): Document => ({
  version: VERSION,
  regions,
  text: textSizes(),
});

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
  // A `Map`, and the result is built by walking the DEFAULT arrangement and asking it — never
  // by walking the document. That is what makes an id this build does not have cost nothing:
  // it is simply never asked for, so there is no unknown region to draw and no unknown key to
  // reach a prototype through.
  const said: string[] = [];
  const held = new Map<string, Record<string, unknown>>();
  if (raw !== null && typeof raw === "object" && !Array.isArray(raw)) {
    const regions = (raw as { regions?: unknown }).regions;
    if (Array.isArray(regions)) {
      for (const one of regions) {
        if (one === null || typeof one !== "object" || Array.isArray(one)) {
          said.push(`${JSON.stringify(one)} is not a region's placement, so it was skipped`);
          continue;
        }
        const placement = one as Record<string, unknown>;
        if (typeof placement.id !== "string") {
          said.push("a placement with no id was skipped");
        } else if (!(REGION_IDS as string[]).includes(placement.id)) {
          said.push(
            `${JSON.stringify(placement.id)} is not a region this charter has (${REGION_IDS.join(", ")}), so it was left out`,
          );
        } else {
          held.set(placement.id, placement);
        }
      }
    } else {
      said.push('there is no "regions" list, so every region is where it starts');
    }
  } else {
    said.push("it is not a layout, so every region is where it starts");
  }

  return {
    said,
    regions: DEFAULT_ARRANGEMENT.map((fallback) => {
      const one = held.get(fallback.id);
      if (one === undefined) return fallback;
      if (one.side !== undefined && !isSide(one.side)) {
        said.push(
          `${fallback.id}'s side ${JSON.stringify(one.side)} is not left, right or bottom, so it is on the ${fallback.side}`,
        );
      }
      if (one.order !== undefined && !Number.isFinite(one.order)) {
        said.push(`${fallback.id}'s order ${JSON.stringify(one.order)} is not a number`);
      }
      if (one.size !== undefined && !usable(one.size)) {
        said.push(`${fallback.id}'s size ${JSON.stringify(one.size)} is not a percentage above 0`);
      }
      return {
        id: fallback.id,
        side: isSide(one.side) ? one.side : fallback.side,
        order: Number.isFinite(one.order) ? (one.order as number) : fallback.order,
        // Only `true` puts a region away. Anything else — missing, a string, a number — is a
        // region charter cannot read the answer for, and it is SHOWN.
        collapsed: one.collapsed === true,
        ...(usable(one.size) ? { size: one.size } : {}),
      };
    }),
  };
}

/**
 * Keeps the arrangement: for the rest of this launch at once, and in the file behind it.
 *
 * A write that fails is said in the alerts drawer and costs nothing else: the window keeps
 * drawing what the operator did, and only the next launch would not know. A write that lands
 * takes back whatever the drawer was saying about the file, because the file is now one this
 * window wrote.
 */
function remember(arrangement: Arrangement): void {
  changed = arrangement;
  const text = JSON.stringify(asDocument(arrangement));
  writing = writing.then(async () => {
    const kept = await commands
      .writeLayout(text)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (kept.status === "error") {
      sayAboutThisMachine("layout", {
        severity: "warn",
        detail: `charter could not keep the layout: ${kept.error}`,
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
  return side === "left" || side === "right" || side === "bottom";
}

/**
 * The arrangement as the slots it draws: every side, in `order`, ties broken on
 * {@link REGION_IDS}.
 *
 * Every side is present even when nothing is in it, because `RegionFrame` renders a panel per
 * side whatever the data says — see this module's docstring for why the panel list is fixed.
 */
export function inSlots(arrangement: Arrangement): Record<Side, Placement[]> {
  const slots: Record<Side, Placement[]> = { left: [], right: [], bottom: [] };
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
