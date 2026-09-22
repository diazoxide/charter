import { StrictMode, useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { RegionFrame } from "./RegionFrame";
import { DEFAULT_ARRANGEMENT, type Arrangement, type RegionId, type Side } from "./regions";

/**
 * **The frame, against the real `react-resizable-panels`** — where a region's content lands,
 * what order it is in, and what moving one costs.
 *
 * jsdom gives every element a size of zero, so the library defers its own layout and no panel
 * is ever given a width here. Everything that is about a *size* is in `RegionFrame.sizes.test`,
 * which mocks the library to read what it was told; this file is about the tree.
 */

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const CONTENT: Record<RegionId, React.ReactNode> = {
  explorer: <div data-testid="c-explorer">explorer</div>,
  aside: <div data-testid="c-aside">aside</div>,
  bottom: <div data-testid="c-bottom">bottom</div>,
};

/** The frame, with a button that rearranges it — standing in for whatever eventually does. */
function Harness({ from, to }: { from: Arrangement; to?: Arrangement }) {
  const [arrangement, setArrangement] = useState(from);
  return (
    <>
      <button onClick={() => to !== undefined && setArrangement(to)}>rearrange</button>
      <RegionFrame
        arrangement={arrangement}
        content={CONTENT}
        centre={<div data-testid="centre">centre</div>}
        onResized={vi.fn()}
      />
    </>
  );
}

/** The panel a slot is drawn in. `react-resizable-panels` puts the id on `data-testid`. */
const slot = (side: Side) => screen.getByTestId(`region-${side}`);

const rearrange = () => userEvent.click(screen.getByRole("button", { name: "rearrange" }));

const on = (id: RegionId, side: Side, order = 0, collapsed = false) => ({
  id,
  side,
  order,
  collapsed,
});

afterEach(cleanup);

describe("where a region is drawn", () => {
  it("puts each one in the slot the arrangement names", () => {
    render(<Harness from={DEFAULT_ARRANGEMENT} />);

    expect(within(slot("left")).getByTestId("c-explorer")).toBeInTheDocument();
    expect(within(slot("right")).getByTestId("c-aside")).toBeInTheDocument();
    expect(within(slot("bottom")).getByTestId("c-bottom")).toBeInTheDocument();
  });

  it("draws it on the other side when the arrangement says so, and no JSX moves", () => {
    // The whole point of the ticket: the bottom bar on the right is a change to the data.
    render(
      <Harness from={[on("explorer", "left"), on("aside", "right"), on("bottom", "right", 1)]} />,
    );

    expect(within(slot("right")).getByTestId("c-bottom")).toBeInTheDocument();
    expect(within(slot("bottom")).queryByTestId("c-bottom")).not.toBeInTheDocument();
  });

  it("draws two regions in one slot, in the order the arrangement gives them", () => {
    render(
      <Harness from={[on("explorer", "left", 1), on("bottom", "left", 0), on("aside", "right")]} />,
    );

    const drawn = within(slot("left"))
      .getAllByTestId(/^c-/)
      .map((one) => one.dataset.testid);
    expect(drawn).toEqual(["c-bottom", "c-explorer"]);
  });

  it("draws nothing in a slot every region has left, and keeps the slot", () => {
    render(
      <Harness from={[on("explorer", "right", 1), on("aside", "right"), on("bottom", "bottom")]} />,
    );

    expect(within(slot("left")).queryAllByTestId(/^c-/)).toEqual([]);
    expect(within(slot("right")).getByTestId("c-explorer")).toBeInTheDocument();
  });
});

describe("a region put away", () => {
  it("is unmounted rather than sized to nothing, so it is out of the tab order too", async () => {
    render(
      <Harness
        from={DEFAULT_ARRANGEMENT}
        to={[on("explorer", "left", 0, true), on("aside", "right"), on("bottom", "bottom")]}
      />,
    );
    expect(screen.getByTestId("c-explorer")).toBeInTheDocument();

    await rearrange();

    expect(screen.queryByTestId("c-explorer")).not.toBeInTheDocument();
  });

  it("leaves its handle in the group with nothing to drag", async () => {
    render(
      <Harness
        from={DEFAULT_ARRANGEMENT}
        to={[on("explorer", "left", 0, true), on("aside", "right"), on("bottom", "bottom")]}
      />,
    );

    await rearrange();

    // Taking the separator out from under a live group is charter-app#141's throw. It stays,
    // and `edge-gone` is what makes it invisible and unhittable.
    const handle = document.querySelector('[aria-controls="region-left"]');
    expect(handle).not.toBeNull();
    expect(handle).toHaveClass("edge-gone");
  });
});

describe("what a rearrangement may not cost", () => {
  it("never adds a panel to a live group, nor takes one out", async () => {
    // charter-app#141: a panel leaving a live group throws *"Panel constraints not found for
    // index 3"* from a document listener no `try` of ours can reach. The slots are fixed so
    // that the data is free to move; this is the assertion that says so.
    render(
      <Harness
        from={DEFAULT_ARRANGEMENT}
        to={[on("explorer", "bottom", 1, true), on("aside", "bottom"), on("bottom", "bottom", 2)]}
      />,
    );
    const before = [...document.querySelectorAll("[data-panel]")].map((one) => one.id);

    await rearrange();

    expect([...document.querySelectorAll("[data-panel]")].map((one) => one.id)).toEqual(before);
    expect(before).toEqual([
      "region-upper",
      "region-left",
      "region-centre",
      "region-right",
      "region-bottom",
    ]);
  });

  it("never takes the terminal panes with it", async () => {
    // The centre is the product. A layout change that remounted it would tear down every
    // xterm in the window and reopen it — which is what keying the group on the arrangement,
    // the other way of doing this, would have done.
    render(
      <Harness
        from={DEFAULT_ARRANGEMENT}
        to={[on("explorer", "right", 1), on("aside", "right"), on("bottom", "bottom")]}
      />,
    );
    const centre = screen.getByTestId("centre");

    await rearrange();

    expect(screen.getByTestId("centre")).toBe(centre);
  });
});
