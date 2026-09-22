import { useCallback, useState } from "react";

/**
 * **The window's layout is data** (charter ADR 0038, charter-app#141 for the four regions
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
 * **Where this is kept, and why it is not a file.** A pin is machine state (charter ADR 0040,
 * amending 0034) and a theme is a file beside `machine.json` (`docs/design-system.md`); a
 * layout is an arrangement like both, and it is in neither. Two reasons, and the first is the
 * one that decides it:
 *
 * - **The first frame.** Both of those live behind a Tauri command, which is asynchronous. A
 *   layout read after the window has painted means the window paints the *default* arrangement
 *   and then re-lays-out — which is the flash this module exists to remove, one level up.
 *   `main.tsx` makes the same argument for the theme in as many words: nothing is read from
 *   disk on the way to the first frame, because ADR 0026 holds cold start at 2 s. Web storage
 *   is the only store in a webview that answers synchronously.
 * - **It is not ADR 0034's kind of fact.** ADR 0040 amended 0034 for *"how the operator
 *   arranged what this file already names"* — the planes the store holds and the workspaces
 *   inside them. A region arrangement names nothing that file holds. Amending 0034 again for
 *   it would be appending to a limit whose whole value is that the fourth thing had to be
 *   argued for.
 *
 * The seam is {@link load}, which takes whatever `JSON.parse` gave and is tested against
 * garbage — the same shape `theme.ts` left for its own file. If a layout ever has to be shared,
 * hand-edited or contributed by a plugin (charter ADR 0041), the source changes there and
 * nothing that renders changes at all; a file would then have to be injected into the window at
 * creation rather than fetched from it, for the reason above.
 *
 * **The old key is not read.** charter-app#141's `charter.regions.shown` held which regions were
 * drawn and nothing else. It is not migrated: carrying a second format forward is permanent, and
 * the whole cost of dropping it is that a region an operator had put away comes back — visible,
 * and one click to undo. Losing a *size* would be silent; losing a hidden region is not.
 */

/** A region. Data, but a closed set in this build: nothing outside the app contributes one
 *  until charter ADR 0041's plugin runtime exists, and a `Record` keyed on it is what makes
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

/** Today's four-region window (charter ADR 0038), as the default *value* of the arrangement
 *  rather than as a shape in `PlaneView`. */
export const DEFAULT_ARRANGEMENT: Arrangement = [
  { id: "explorer", side: "left", order: 0, collapsed: false },
  { id: "aside", side: "right", order: 0, collapsed: false },
  { id: "bottom", side: "bottom", order: 0, collapsed: false },
];

/** Where the arrangement is kept between launches. */
const KEY = "charter.layout";

/** The document's shape, once it has been read. */
type Document = { regions: Arrangement };

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

/** The arrangement as it was left, or the default when nothing readable was stored. */
export function remembered(): Arrangement {
  try {
    return load(JSON.parse(globalThis.localStorage?.getItem(KEY) ?? "null")).regions;
  } catch {
    // A webview that refuses storage, or a stored value that is not JSON. Both are "nothing
    // was remembered", and a window must not fail to lay out over a layout preference.
    return DEFAULT_ARRANGEMENT;
  }
}

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
export function load(raw: unknown): Document {
  // A `Map`, and the result is built by walking the DEFAULT arrangement and asking it — never
  // by walking the document. That is what makes an id this build does not have cost nothing:
  // it is simply never asked for, so there is no unknown region to draw and no unknown key to
  // reach a prototype through.
  const said = new Map<string, Record<string, unknown>>();
  if (raw !== null && typeof raw === "object" && !Array.isArray(raw)) {
    const regions = (raw as { regions?: unknown }).regions;
    if (Array.isArray(regions)) {
      for (const one of regions) {
        if (one === null || typeof one !== "object" || Array.isArray(one)) continue;
        const held = one as Record<string, unknown>;
        if (typeof held.id === "string") said.set(held.id, held);
      }
    }
  }

  return {
    regions: DEFAULT_ARRANGEMENT.map((fallback) => {
      const held = said.get(fallback.id);
      if (held === undefined) return fallback;
      return {
        id: fallback.id,
        side: isSide(held.side) ? held.side : fallback.side,
        order: Number.isFinite(held.order) ? (held.order as number) : fallback.order,
        // Only `true` puts a region away. Anything else — missing, a string, a number — is a
        // region charter cannot read the answer for, and it is SHOWN.
        collapsed: held.collapsed === true,
        ...(usable(held.size) ? { size: held.size } : {}),
      };
    }),
  };
}

function remember(arrangement: Arrangement): void {
  try {
    globalThis.localStorage?.setItem(
      KEY,
      JSON.stringify({ regions: arrangement } satisfies Document),
    );
  } catch {
    // A webview that will not store it still draws it. The operator loses the arrangement at
    // the next launch and nothing else.
  }
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
