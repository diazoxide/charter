import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { Group, Panel, Separator, usePanelRef, type Layout } from "react-resizable-panels";
import {
  CATALOGUE,
  inSlots,
  shownIn,
  SIDES,
  slotSize,
  SLOTS,
  startingSize,
  type Arrangement,
  type Placement,
  type RegionId,
  type Side,
} from "./regions";

/**
 * **The window, drawn from the arrangement** (charter ADR 0038, and `regions.ts` for why the
 * arrangement is data).
 *
 * Four panels and nothing else: a slot on the left, the centre, a slot on the right, a slot
 * along the bottom. The panel list is the same on every render of a window's life, and that is
 * the load-bearing part — see `regions.ts` for charter-app#141's throw. What the *data* decides
 * is which slot a region's content goes in, what order it is in, whether it is drawn at all,
 * and how big its slot starts.
 *
 * So moving a region is moving its content between two panels that both already exist. Nothing
 * is added to a live group, nothing is taken out of one, no constraint changes, and the centre
 * — the terminal panes, which are the product — is never remounted by a layout change.
 */
export function RegionFrame({
  arrangement,
  content,
  centre,
  onResized,
}: {
  arrangement: Arrangement;
  /** What each region draws. Kept out of the arrangement because a React component is not
   *  something a stored document can hold. */
  content: Record<RegionId, ReactNode>;
  centre: ReactNode;
  /** Told how big each slot was left, once a drag has settled. */
  onResized: (sizes: Partial<Record<Side, number>>) => void;
}) {
  const slots = inSlots(arrangement);

  // **How big each slot starts, read once.** `defaultSize` is a constraint, and a constraint
  // that changes re-registers the panel — charter-app#141 again. The arrangement the window
  // launched with is what sizes the slots; a size remembered mid-session is remembered for the
  // next launch and does not move the slot under the operator's hands.
  const [started] = useState(() => inSlots(arrangement));

  const settled = (layout: Layout, meta: { isUserInteraction: boolean }) => {
    // `onLayoutChanged` also fires on mount, on a collapse and on any other imperative call.
    // Only a drag or a resize key is the operator saying how big they want it; everything else
    // would write back a measurement charter itself caused — including the zero a collapsed
    // slot reports, which is not a width to come back to.
    if (!meta.isUserInteraction) return;
    const sizes: Partial<Record<Side, number>> = {};
    for (const side of SIDES) {
      const measured = layout[panelOf(side)];
      if (typeof measured === "number") sizes[side] = measured;
    }
    onResized(sizes);
  };

  return (
    <Group className="regions" orientation="vertical" onLayoutChanged={settled}>
      <Panel id="region-upper" className="region-upper" minSize="30%">
        <Group className="region-row" orientation="horizontal" onLayoutChanged={settled}>
          <Slot side="left" placed={slots.left} started={started.left} content={content} />
          <Edge open={shownIn(slots.left).length > 0} />
          <Panel id="region-centre" className="region-centre" minSize="20%">
            {centre}
          </Panel>
          <Edge open={shownIn(slots.right).length > 0} />
          <Slot side="right" placed={slots.right} started={started.right} content={content} />
        </Group>
      </Panel>
      <Edge open={shownIn(slots.bottom).length > 0} />
      <Slot side="bottom" placed={slots.bottom} started={started.bottom} content={content} />
    </Group>
  );
}

/** The panel each slot is drawn in. Also the id `react-resizable-panels` reports a size under,
 *  so the two have to be the same string and there is one place it is written. */
export const panelOf = (side: Side): string => `region-${side}`;

/**
 * One of the three slots around the centre: resizable, and emptied without going away.
 *
 * **The panel stays mounted with constraints that never change; the library collapses it.**
 * That is the whole reason this component exists, and neither obvious alternative works.
 * Rendering the `Panel` and its `Separator` conditionally throws — `react-resizable-panels`
 * recalculates a separator's aria values against the group's constraint list, and taking a
 * panel out from under a live separator leaves it indexing past the end
 * (*"Panel constraints not found for index 3"*), from a document listener where no `try` of
 * ours can reach it. Clamping the panel's `minSize`/`maxSize` to zero instead throws the same
 * way, because changing a constraint re-registers the panel and a recalculation lands in the
 * gap. So the constraints are written once and `collapse()`/`expand()` — the library's own
 * way of doing this — is what moves it.
 *
 * **Its content is unmounted all the same**, which is the part that has to be true for more
 * than tidiness: a slot squeezed to zero pixels with its markup still in the document is still
 * in the tab order and still read out in full by a screen reader, so "put away" would mean
 * "invisible and in the way". Nothing in a region is state worth keeping across that — what
 * they draw is read fresh from the plane every time a workspace is focused, because the plane
 * is a directory the operator also edits by hand.
 *
 * **The flash is fixed by the first layout and not by an earlier effect, and that is measured.**
 * charter-app#141 left a window that launched with a region put away drawing it full size for
 * one frame: the panel was sized normally and a `useEffect` collapsed it, and a passive effect
 * runs after the browser has painted. The obvious repair is `useLayoutEffect`, and it **throws**
 * — *"Group &lt;id&gt; not found"*, from `getPanelConstraints`, because the group registers
 * itself in its *own* layout effect and React runs a child's layout effects before its parent's.
 * There is nowhere inside the group that runs later.
 *
 * So the first layout is made right instead: `startingSize` gives a slot that starts with
 * nothing in it `0%`, which `react-resizable-panels` snaps to `collapsedSize`, and the window
 * paints it collapsed. The effect below is then only about *changes* — a region put away or
 * brought back while the window is up — where a frame's delay is no more than the click's own.
 */
function Slot({
  side,
  placed,
  started,
  content,
}: {
  side: Side;
  /** What is in this slot now. */
  placed: Placement[];
  /** What was in it when the window launched, which is what sized it. */
  started: Placement[];
  content: Record<RegionId, ReactNode>;
}) {
  const shown = shownIn(placed);
  const open = shown.length > 0;
  const panel = usePanelRef();

  /** How big it should be when it is brought back. */
  const wanted = slotSize(side, placed);

  const before = useRef(open);
  useEffect(() => {
    const it = panel.current;
    // Only a slot that has just been emptied or filled is acted on. `wanted` is in the
    // dependencies because it is read below and React insists, but a run that is only about a
    // size stops here — that size is one the operator's own drag has already applied, and
    // pushing it back at the panel would be charter resizing what somebody is holding.
    const changed = before.current !== open;
    before.current = open;
    if (it === null) return;
    if (!open) {
      // Idempotent, and a no-op on the launch of a window whose arrangement already started
      // this slot at nothing — the library will not collapse what is already collapsed.
      it.collapse();
      return;
    }
    // Nothing to do at the launch: `defaultSize` has already put the slot where the
    // arrangement says, and a resize here would be a second layout for no change — in the real
    // window, one that lands while the group is still measuring itself.
    if (!changed) return;
    it.expand();
    // `expand()` goes back to the size the panel had when it was collapsed, and a slot that was
    // already away when the window launched never had one — it started at nothing, so the
    // library would bring it back at its minimum. The remembered size is what it should come
    // back to, and saying so is cheaper than painting it once to find out.
    it.resize(`${wanted}%`);
  }, [panel, open, wanted]);

  return (
    <Panel
      id={panelOf(side)}
      className={`region-slot slot-${side}`}
      panelRef={panel}
      // **Every one of these is constant for the life of the group, and that is the point.**
      // See this component's docstring: changing a panel's constraints re-registers it, and a
      // separator that recalculates in between indexes past the end of the constraint list.
      // `started` is the arrangement the window launched with, snapshotted for this reason.
      collapsible
      collapsedSize="0%"
      defaultSize={`${startingSize(side, started)}%`}
      minSize={`${SLOTS[side].least}%`}
      maxSize={`${SLOTS[side].most}%`}
    >
      {shown.map((one) => (
        <Fragment key={one.id}>{content[one.id]}</Fragment>
      ))}
    </Panel>
  );
}

/** The handle between a slot and the centre. It stays in the group when its slot is empty —
 *  see `Slot` for why — and draws nothing, so there is no line to drag. */
function Edge({ open }: { open: boolean }) {
  return <Separator className={open ? undefined : "edge-gone"} />;
}

/**
 * The button that puts a region away and brings it back (charter ADR 0038).
 *
 * **Not a row of the catalogue**, deliberately, and this is the one place in the bar where that
 * is true. The catalogue is what a project can DO — open a chat, split a pane, remove a
 * worktree — and every one of its rows is also a palette row, because the palette is the
 * primary input. Whether the operator has the explorer on screen is not a thing to do to the
 * plane; it is how this window is laid out, it is remembered beside the window and not on the
 * plane, and a palette full of "Explorer", "Attention", "State" would be three rows of noise in
 * front of a hundred real ones at ADR 0026's limits.
 *
 * `aria-pressed` and not a label that flips between "Show" and "Hide": the name of the thing is
 * what a person looks for, and the state is what the attribute is for.
 */
export function RegionToggle({
  id,
  shown,
  onToggle,
}: {
  id: RegionId;
  shown: boolean;
  onToggle: (id: RegionId) => void;
}) {
  const name = CATALOGUE[id].name;
  return (
    <button
      type="button"
      className="region-toggle"
      aria-pressed={shown}
      title={shown ? `Put the ${name} region away` : `Bring the ${name} region back`}
      onClick={() => onToggle(id)}
    >
      {name}
    </button>
  );
}
