import { useLayoutEffect, useRef, useState, type ReactNode, type Ref } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { RegionFrame } from "./RegionFrame";
import { DEFAULT_ARRANGEMENT, type Arrangement, type RegionId, type Side } from "./regions";

/**
 * **What the frame tells `react-resizable-panels`.**
 *
 * The library is mocked here, and that is not a shortcut around it — jsdom gives every element
 * a size of zero, so the real library defers its layout and never applies a `defaultSize` at
 * all. Reading a size off the DOM in a unit test is therefore impossible, and the question this
 * file asks is the one that *is* answerable: what was the library told, and when. `RegionFrame`
 * runs against the real library in `RegionFrame.test.tsx`, and the whole window does in
 * `FourRegions.test.tsx`.
 */

type PanelProps = {
  id: string;
  className?: string;
  collapsible?: boolean;
  collapsedSize?: string;
  defaultSize?: string;
  minSize?: string;
  maxSize?: string;
  panelRef?: Ref<unknown>;
  children?: ReactNode;
};

type Handle = {
  collapse: ReturnType<typeof vi.fn>;
  expand: ReturnType<typeof vi.fn>;
  resize: ReturnType<typeof vi.fn>;
};

/** Every `Panel` the frame rendered, by id, as it was first given — the constraints a group is
 *  registered with are the ones that matter, so the FIRST render's props are kept as well. */
const given = new Map<string, PanelProps>();
const first = new Map<string, PanelProps>();
const handles = new Map<string, Handle>();
/** Every `Group`'s settled-layout callback, in the order the frame rendered them. */
const settled: ((layout: Record<string, number>, meta: { isUserInteraction: boolean }) => void)[] =
  [];

vi.mock("react-resizable-panels", () => ({
  Group: ({
    children,
    className,
    onLayoutChanged,
  }: {
    children?: ReactNode;
    className?: string;
    onLayoutChanged?: (l: Record<string, number>, m: { isUserInteraction: boolean }) => void;
  }) => {
    if (onLayoutChanged && !settled.includes(onLayoutChanged)) settled.push(onLayoutChanged);
    return <div data-group={className}>{children}</div>;
  },
  Panel: (props: PanelProps) => {
    given.set(props.id, props);
    if (!first.has(props.id)) first.set(props.id, props);
    const handle = useRef<Handle>({ collapse: vi.fn(), expand: vi.fn(), resize: vi.fn() });
    const { panelRef } = props;
    // A layout effect, because that is when the real library attaches its imperative handle —
    // before the parent's passive effect reaches for it.
    useLayoutEffect(() => {
      if (typeof panelRef === "object" && panelRef !== null) panelRef.current = handle.current;
      handles.set(props.id, handle.current);
    });
    return (
      <div data-panel={props.id} data-testid={props.id}>
        {props.children}
      </div>
    );
  },
  Separator: ({ className }: { className?: string }) => (
    <div role="separator" className={className} />
  ),
  usePanelRef: () => useRef(null),
}));

const CONTENT: Record<RegionId, ReactNode> = {
  explorer: <div data-testid="c-explorer">explorer</div>,
  aside: <div data-testid="c-aside">aside</div>,
  bottom: <div data-testid="c-bottom">bottom</div>,
};

const onResized = vi.fn();

function Harness({ from, to }: { from: Arrangement; to?: Arrangement }) {
  const [arrangement, setArrangement] = useState(from);
  return (
    <>
      <button onClick={() => to !== undefined && setArrangement(to)}>rearrange</button>
      <RegionFrame
        arrangement={arrangement}
        content={CONTENT}
        centre={<div data-testid="centre">centre</div>}
        onResized={onResized}
      />
    </>
  );
}

const rearrange = () => userEvent.click(screen.getByRole("button", { name: "rearrange" }));

const on = (id: RegionId, side: Side, order = 0, collapsed = false, size?: number) => ({
  id,
  side,
  order,
  collapsed,
  ...(size === undefined ? {} : { size }),
});

const AWAY: Arrangement = [
  on("explorer", "left", 0, true),
  on("aside", "right"),
  on("bottom", "bottom"),
];

beforeEach(() => {
  given.clear();
  first.clear();
  handles.clear();
  settled.length = 0;
  onResized.mockClear();
});
afterEach(cleanup);

describe("how big a slot starts", () => {
  it("is the catalogue's when nothing has been dragged", () => {
    render(<Harness from={DEFAULT_ARRANGEMENT} />);

    expect(given.get("region-left")?.defaultSize).toBe("16%");
    expect(given.get("region-right")?.defaultSize).toBe("20%");
    expect(given.get("region-bottom")?.defaultSize).toBe("16%");
  });

  it("is the size the arrangement remembers, which is what charter-app#141 deferred", () => {
    render(
      <Harness
        from={[
          on("explorer", "left", 0, false, 31.5),
          on("aside", "right"),
          on("bottom", "bottom"),
        ]}
      />,
    );

    expect(given.get("region-left")?.defaultSize).toBe("31.5%");
  });

  it("is nothing at all when the slot starts with nothing in it", () => {
    // **The flash** (charter-app#141, deliberate at the time): a region that was put away used
    // to be laid out full size and collapsed by an effect, and an effect runs after the browser
    // has painted. Starting the slot at `0%` — which the library snaps to `collapsedSize` — is
    // what makes the first frame right, because `useLayoutEffect` cannot: the group registers
    // itself in its own layout effect, which React runs AFTER its children's.
    render(<Harness from={AWAY} />);

    expect(given.get("region-left")?.defaultSize).toBe("0%");
    expect(given.get("region-left")?.collapsedSize).toBe("0%");
    expect(given.get("region-left")?.collapsible).toBe(true);
  });

  it("is nothing at all when every region has moved off that side", () => {
    render(
      <Harness from={[on("explorer", "right", 1), on("aside", "right"), on("bottom", "bottom")]} />,
    );

    expect(given.get("region-left")?.defaultSize).toBe("0%");
  });
});

describe("the constraints the group is registered with", () => {
  it("never change, however the arrangement does", async () => {
    // charter-app#141 measured both halves of this: taking a panel out of a live group throws,
    // and so does changing a live panel's constraints, because the change re-registers it and a
    // separator recalculating in the gap indexes past the end of the constraint list.
    render(
      <Harness
        from={DEFAULT_ARRANGEMENT}
        to={[
          on("explorer", "bottom", 1, true),
          on("aside", "left", 0, false, 40),
          on("bottom", "bottom", 2),
        ]}
      />,
    );

    await rearrange();

    for (const [id, was] of first) {
      expect({ id, ...constraints(given.get(id)) }).toEqual({ id, ...constraints(was) });
    }
  });

  it("gives each slot the bounds of the slot and not of whatever is in it", () => {
    // A bound taken from the regions placed in a slot would move the moment one of them did,
    // which is the throw above. `SLOTS` is where they are written, once.
    render(<Harness from={DEFAULT_ARRANGEMENT} />);

    expect(constraints(given.get("region-left"))).toMatchObject({ minSize: "8%", maxSize: "45%" });
    expect(constraints(given.get("region-right"))).toMatchObject({
      minSize: "10%",
      maxSize: "45%",
    });
    expect(constraints(given.get("region-bottom"))).toMatchObject({
      minSize: "6%",
      maxSize: "50%",
    });
  });
});

const constraints = (props: PanelProps | undefined) => ({
  collapsedSize: props?.collapsedSize,
  collapsible: props?.collapsible,
  defaultSize: props?.defaultSize,
  minSize: props?.minSize,
  maxSize: props?.maxSize,
});

describe("putting a region away while the window is up", () => {
  it("touches nothing on the launch itself", () => {
    // `defaultSize` has already laid the slots out. A collapse or a resize here would be a
    // second layout for no change, landing while the group is still measuring itself.
    render(<Harness from={DEFAULT_ARRANGEMENT} />);

    for (const handle of handles.values()) {
      expect(handle.expand).not.toHaveBeenCalled();
      expect(handle.resize).not.toHaveBeenCalled();
    }
  });

  it("collapses the slot the last region left", async () => {
    render(<Harness from={DEFAULT_ARRANGEMENT} to={AWAY} />);

    await rearrange();

    expect(handles.get("region-left")?.collapse).toHaveBeenCalled();
    expect(handles.get("region-right")?.collapse).not.toHaveBeenCalled();
  });

  it("brings it back at the size it was left, and not at the library's minimum", async () => {
    // `expand()` restores the size the panel had when it was collapsed, and a slot that was
    // already away at launch never had one — it started at nothing, so the library would bring
    // it back at `minSize`. The remembered size is what it should come back to.
    render(
      <Harness
        from={[on("explorer", "left", 0, true, 31.5), on("aside", "right"), on("bottom", "bottom")]}
        to={[on("explorer", "left", 0, false, 31.5), on("aside", "right"), on("bottom", "bottom")]}
      />,
    );

    await rearrange();

    expect(handles.get("region-left")?.expand).toHaveBeenCalled();
    expect(handles.get("region-left")?.resize).toHaveBeenCalledWith("31.5%");
  });
});

describe("remembering how big a slot was left", () => {
  const layout = { "region-left": 31.5, "region-centre": 48.5, "region-right": 20 };

  it("reports each slot under the id its panel is drawn with", () => {
    render(<Harness from={DEFAULT_ARRANGEMENT} />);

    settled[0]?.(layout, { isUserInteraction: true });

    expect(onResized).toHaveBeenCalledWith({ left: 31.5, right: 20 });
  });

  it("says nothing about a slot the group did not report", () => {
    // The two groups each report their own panels. The outer one never mentions the left slot,
    // and answering for it would write the bottom's height into the explorer's width.
    render(<Harness from={DEFAULT_ARRANGEMENT} />);

    settled[0]?.({ "region-upper": 84, "region-bottom": 16 }, { isUserInteraction: true });

    expect(onResized).toHaveBeenCalledWith({ bottom: 16 });
  });

  it("ignores a layout charter itself caused", () => {
    // `onLayoutChanged` also fires on mount, on a collapse and on every imperative call —
    // including the zero a collapsed slot reports, which is not a width to come back to.
    render(<Harness from={DEFAULT_ARRANGEMENT} />);

    settled[0]?.(layout, { isUserInteraction: false });

    expect(onResized).not.toHaveBeenCalled();
  });
});
