import { Fragment, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Group, Panel, Separator, usePanelRef, type Layout } from "react-resizable-panels";
import { BellRing, Compass } from "lucide-react";
import { ActivityBar, VIEW_MARKS, type PanelTab, type Tabbed } from "./ActivityBar";
import {
  CATALOGUE,
  inSlots,
  leastOf,
  openView,
  shownIn,
  SIDES,
  slotSize,
  SLOTS,
  startingSize,
  VIEWS,
  type Arrangement,
  type Placement,
  type RegionId,
  type Side,
  type ViewId,
} from "./regions";
import { useArrived } from "./lib/arrived";

/**
 * **The window, drawn from the arrangement** (ADR 0038, and `regions.ts` for why the
 * arrangement is data).
 *
 * Three panels and nothing else: a slot on the left, the centre, a slot on the right. (A slot
 * along the bottom held the State region until #1676 made it the left's Changes view, so the
 * terminals have the window's whole height.) The panel list is the same on every render of a window's life, and that is
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
  views,
  panels = [],
  badges,
  keys,
  onPick,
  centre,
  onResized,
}: {
  arrangement: Arrangement;
  /** What each region draws. Kept out of the arrangement because a React component is not
   *  something a stored document can hold. A region with views draws {@link views} instead,
   *  when they are given. */
  content: Partial<Record<RegionId, ReactNode>>;
  /**
   * **What each view draws** (#1673). A region whose catalogue entry has views, given them
   * here, is drawn as an activity bar at its side's edge and its views in its slot: the open
   * one shown, the others mounted and hidden, and all of them mounted and hidden while the
   * region is put away — collapsing hides and never unmounts, so a view keeps its scroll, its
   * folds and its filter. The right-hand side's views (#1678) arrive the same way.
   */
  views?: Partial<Record<ViewId, ReactNode>>;
  /**
   * **The approved extensions' panels, each a view** (#1678), in the order they go on the bar,
   * after purlis's own views of the region that takes them, with the name and mark each
   * declared. What each draws is in {@link views} under its id.
   */
  panels?: readonly PanelTab[];
  /** What each view's tab carries beside its icon: a count, drawn while the side is away too. */
  badges?: Partial<Record<ViewId, ReactNode>>;
  /** How each view's key is spelled, for its tab's tooltip. */
  keys?: Partial<Record<ViewId, string>>;
  /** A view's tab was pressed: the window answers with the arrangement (`regions.picked`). */
  onPick?: (view: ViewId) => void;
  centre: ReactNode;
  /** Told how big each slot was left, once a drag has settled. */
  onResized: (sizes: Partial<Record<Side, number>>) => void;
}) {
  const slots = inSlots(arrangement);
  const base = useId();
  const ids = {
    panelOf: (view: ViewId) => `${base}view-${view}`,
    tabOf: (view: ViewId) => `${base}tab-${view}`,
  };
  const withViews = (one: Placement) =>
    views !== undefined && CATALOGUE[one.id].views !== undefined;
  const present = panels.map((one) => one.view);
  /** A region's views, as its bar draws them: purlis's own, then the panels, where it takes them. */
  const tabbed = (id: RegionId): Tabbed[] => [
    ...(CATALOGUE[id].views ?? []).map((view) => ({
      view,
      name: VIEWS[view].name,
      mark: VIEW_MARKS[view],
    })),
    ...(CATALOGUE[id].panels ? panels : []),
  ];
  /** The bars at one edge: one per region with views placed in that side's slot. */
  const bars = (side: "left" | "right") =>
    slots[side]
      .filter(withViews)
      .map((one) => (
        <ActivityBar
          key={one.id}
          side={side}
          name={CATALOGUE[one.id].name}
          views={tabbed(one.id)}
          open={one.collapsed ? undefined : openView(one, present)}
          keys={keys}
          badges={badges}
          {...ids}
          onPick={(view) => onPick?.(view)}
        />
      ));
  const drawn: SlotContent = { content, views, ids, withViews, tabbed, present };

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

  // **The bars are outside the group**, at the window's edges as VS Code has them, so a side
  // put away leaves its bar — and the bar's badges — on screen. They are not panels: nothing
  // here adds to or takes from the group's panel list (charter-app#141).
  return (
    <div className="region-frame">
      {bars("left")}
      <Group className="regions" orientation="horizontal" onLayoutChanged={settled}>
        <Slot side="left" placed={slots.left} started={started.left} drawn={drawn} />
        <Edge open={shownIn(slots.left).length > 0} />
        <Panel id="region-centre" className="region-centre" minSize="20%">
          {centre}
        </Panel>
        <Edge open={shownIn(slots.right).length > 0} />
        <Slot side="right" placed={slots.right} started={started.right} drawn={drawn} />
      </Group>
      {bars("right")}
    </div>
  );
}

/** What a slot draws its regions from. */
type SlotContent = {
  content: Partial<Record<RegionId, ReactNode>>;
  views?: Partial<Record<ViewId, ReactNode>>;
  ids: { panelOf: (view: ViewId) => string; tabOf: (view: ViewId) => string };
  withViews: (one: Placement) => boolean;
  tabbed: (id: RegionId) => Tabbed[];
  /** The extensions' panels there are now, which decide whether one picked last can open. */
  present: readonly ViewId[];
};

/**
 * A region with views, in its slot: each view a tab panel, the open one shown and the rest
 * mounted and `hidden` — out of the tab order and the accessibility tree, and still holding
 * what the person did in it. Put away, every one of them is hidden.
 */
function RegionViews({ placed, drawn }: { placed: Placement; drawn: SlotContent }) {
  const open = placed.collapsed ? undefined : openView(placed, drawn.present);
  return (
    <div className="region-views" data-region={placed.id} hidden={placed.collapsed}>
      {drawn.tabbed(placed.id).map(({ view }) => (
        <div
          key={view}
          role="tabpanel"
          id={drawn.ids.panelOf(view)}
          aria-labelledby={drawn.ids.tabOf(view)}
          data-view={view}
          className="region-view"
          hidden={view !== open}
        >
          {drawn.views?.[view]}
        </div>
      ))}
    </div>
  );
}

/** The panel each slot is drawn in. Also the id `react-resizable-panels` reports a size under,
 *  so the two have to be the same string and there is one place it is written. */
export const panelOf = (side: Side): string => `region-${side}`;

/**
 * One of the two slots beside the centre: resizable, and emptied without going away.
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
  drawn,
}: {
  side: Side;
  /** What is in this slot now. */
  placed: Placement[];
  /** What was in it when the window launched, which is what sized it. */
  started: Placement[];
  drawn: SlotContent;
}) {
  const shown = shownIn(placed);
  const open = shown.length > 0;
  const panel = usePanelRef();
  // Brought back while the window is up, as opposed to open since launch: only the first is
  // drawn arriving, so a window does not fade its own regions in every time it starts.
  const arrived = useArrived(open) && open;

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
      className={`region-slot slot-${side}${arrived ? " arrived" : ""}`}
      panelRef={panel}
      // **Every one of these is constant for the life of the group, and that is the point.**
      // See this component's docstring: changing a panel's constraints re-registers it, and a
      // separator that recalculates in between indexes past the end of the constraint list.
      // `started` is the arrangement the window launched with, snapshotted for this reason.
      collapsible
      collapsedSize="0%"
      defaultSize={`${startingSize(side, started)}%`}
      minSize={leastOf(side)}
      maxSize={`${SLOTS[side].most}%`}
    >
      {/* A region without views put away is unmounted (above); one with views is kept,
          hidden, because its views are where the person's folds and scroll are (#1673). */}
      {placed.map((one) =>
        drawn.withViews(one) ? (
          <RegionViews key={one.id} placed={one} drawn={drawn} />
        ) : one.collapsed ? null : (
          <Fragment key={one.id}>{drawn.content[one.id]}</Fragment>
        ),
      )}
    </Panel>
  );
}

/** The handle between a slot and the centre. It stays in the group when its slot is empty —
 *  see `Slot` for why — and draws nothing, so there is no line to drag. */
function Edge({ open }: { open: boolean }) {
  return <Separator className={open ? undefined : "edge-gone"} />;
}

/**
 * The button that puts a region away and brings it back (ADR 0038).
 *
 * **Not a row of the catalogue**, deliberately, and this is the one place in the bar where that
 * is true. The catalogue is what a project can DO — open a chat, split a pane, remove a
 * worktree — and every one of its rows is also a palette row, because the palette is the
 * primary input. Whether the operator has the explorer on screen is not a thing to do to the
 * plane; it is how this window is laid out, it is remembered beside the window and not on the
 * plane, and a palette full of "Navigation", "Attention" would be rows of noise in
 * front of a hundred real ones at ADR 0026's limits.
 *
 * `aria-pressed` and not a label that flips between "Show" and "Hide": the name of the thing is
 * what a person looks for, and the state is what the attribute is for.
 *
 * # Its mark, and nothing else — on the status line (charter-app#193)
 *
 * The operator asked for this twice. First *"show hide buttons can be movet to bottom status
 * bar — again like ZED"*, and then, when only half of it had been done: *"you dont moved this 3
 * hide/show bottons to bottom status bar — and let make them without labels, just small icons
 * without texts, texts only with tooltips, i already asked about this — seems you missed."*
 * `StatusLine.tsx` is where they are drawn now; this is what one of them is.
 *
 * **The words do not go away — they move from the content to `aria-label`.** That is the one
 * condition `docs/design-system.md` puts on an icon with no text beside it, and it is not a
 * formality here: `pressOnly("Explorer")` is how the palette and the scenario specs reach a
 * control, and a screen reader reads exactly the same string. An icon-only button whose
 * accessible name is an icon is a button nobody can find, by either route.
 *
 * **The `title` says what pressing it does, which the name cannot.** `aria-label` has to be the
 * name of the thing — `Explorer` — because that is what a person looks for; the tooltip is
 * where *"Put the Explorer region away"* belongs, and it already said it. A tooltip that merely
 * repeated the label would be a tooltip nobody reads twice.
 *
 * **The mark is what the region IS and never where it is**, which was already true and is now
 * the whole of what is drawn — see {@link REGION_MARKS}. A panel-left glyph would be wrong the
 * first time a region moved, and with the words gone it would be the only thing left to be
 * wrong.
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
  const Mark = REGION_MARKS[id];
  return (
    <button
      type="button"
      className="region-toggle"
      // WebKit leaves a `<button>` out of the tab sequence unless its `tabindex` is written
      // down (`docs/ui-primitives.md`, charter-app#189).
      tabIndex={0}
      aria-pressed={shown}
      aria-label={name}
      title={shown ? `Put the ${name} region away` : `Bring the ${name} region back`}
      onClick={() => onToggle(id)}
    >
      <Mark />
    </button>
  );
}

/**
 * Each region's mark, by what it IS and not by where it is.
 *
 * A panel-left / panel-right icon would be the obvious choice and would be wrong the first
 * time an operator moved a region: the layout is data (`regions.ts`), so the explorer can be
 * on the right, and a toggle drawn as "left panel" would then point at the wrong edge of the
 * window. Navigation is a compass, attention is a bell.
 *
 * A `Record` over `RegionId`, so a region added to the catalogue without a mark here is a type
 * error rather than a toggle with a hole in it.
 */
const REGION_MARKS: Record<RegionId, typeof Compass> = {
  // The region the Chats and Explorer views are in: a way to somewhere, as ADR 0038 reads it.
  // The explorer's folder tree is the Explorer view's own mark now (`ActivityBar.tsx`).
  navigation: Compass,
  aside: BellRing,
};
