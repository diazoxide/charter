import { StrictMode, useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { RegionFrame } from "./RegionFrame";
import { ActivityCount } from "./ActivityBar";
import { drawnWith } from "./cascade.testkit";
import {
  DEFAULT_ARRANGEMENT,
  type Arrangement,
  type RegionId,
  type PanelViewId,
  type Side,
  type ViewId,
} from "./regions";
import { Flame, type LucideIcon } from "lucide-react";

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
  navigation: <div data-testid="c-navigation">navigation</div>,
  aside: <div data-testid="c-aside">aside</div>,
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

    expect(within(slot("left")).getByTestId("c-navigation")).toBeInTheDocument();
    expect(within(slot("right")).getByTestId("c-aside")).toBeInTheDocument();
  });

  it("has a slot on each side and none along the bottom (#1676)", () => {
    render(<Harness from={DEFAULT_ARRANGEMENT} />);

    expect([...document.querySelectorAll("[data-panel]")].map((one) => one.id)).toEqual([
      "region-left",
      "region-centre",
      "region-right",
    ]);
  });

  it("draws it on the other side when the arrangement says so, and no JSX moves", () => {
    // The whole point of the ticket: the attention region on the left is a change to the data.
    render(<Harness from={[on("navigation", "right"), on("aside", "left")]} />);

    expect(within(slot("left")).getByTestId("c-aside")).toBeInTheDocument();
    expect(within(slot("right")).queryByTestId("c-aside")).not.toBeInTheDocument();
  });

  it("draws two regions in one slot, in the order the arrangement gives them", () => {
    render(<Harness from={[on("navigation", "left", 1), on("aside", "left", 0)]} />);

    const drawn = within(slot("left"))
      .getAllByTestId(/^c-/)
      .map((one) => one.dataset.testid);
    expect(drawn).toEqual(["c-aside", "c-navigation"]);
  });

  it("draws nothing in a slot every region has left, and keeps the slot", () => {
    render(<Harness from={[on("navigation", "right", 1), on("aside", "right")]} />);

    expect(within(slot("left")).queryAllByTestId(/^c-/)).toEqual([]);
    expect(within(slot("right")).getByTestId("c-navigation")).toBeInTheDocument();
  });
});

describe("a region put away", () => {
  it("is unmounted rather than sized to nothing, so it is out of the tab order too", async () => {
    render(
      <Harness
        from={DEFAULT_ARRANGEMENT}
        to={[on("navigation", "left", 0, true), on("aside", "right")]}
      />,
    );
    expect(screen.getByTestId("c-navigation")).toBeInTheDocument();

    await rearrange();

    expect(screen.queryByTestId("c-navigation")).not.toBeInTheDocument();
  });

  it("leaves its handle in the group with nothing to drag", async () => {
    render(
      <Harness
        from={DEFAULT_ARRANGEMENT}
        to={[on("navigation", "left", 0, true), on("aside", "right")]}
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
        to={[on("navigation", "right", 1, true), on("aside", "left")]}
      />,
    );
    const before = [...document.querySelectorAll("[data-panel]")].map((one) => one.id);

    await rearrange();

    expect([...document.querySelectorAll("[data-panel]")].map((one) => one.id)).toEqual(before);
    expect(before).toEqual(["region-left", "region-centre", "region-right"]);
  });

  it("never takes the terminal panes with it", async () => {
    // The centre is the product. A layout change that remounted it would tear down every
    // xterm in the window and reopen it — which is what keying the group on the arrangement,
    // the other way of doing this, would have done.
    render(
      <Harness
        from={DEFAULT_ARRANGEMENT}
        to={[on("navigation", "right", 1), on("aside", "right")]}
      />,
    );
    const centre = screen.getByTestId("centre");

    await rearrange();

    expect(screen.getByTestId("centre")).toBe(centre);
  });
});

/** The views of both sides, as a window hands them over. */
const VIEWS: Partial<Record<ViewId, React.ReactNode>> = {
  chats: <div data-testid="v-chats">chats</div>,
  explorer: <div data-testid="v-explorer">explorer</div>,
  search: <div data-testid="v-search">search</div>,
  changes: <div data-testid="v-changes">changes</div>,
  todos: <div data-testid="v-todos">todos</div>,
  memory: <div data-testid="v-memory">memory</div>,
  personas: <div data-testid="v-personas">personas</div>,
  sessions: <div data-testid="v-sessions">sessions</div>,
  vaults: <div data-testid="v-vaults">vaults</div>,
  "panel:ext/stats/burn": <div data-testid="v-burn">burn</div>,
};

/** The frame with views, an activity bar and a badge, rearranged by `to` on the button. */
function WithViews({
  from,
  to,
  onPick = vi.fn(),
  badge,
  panels = [],
}: {
  from: Arrangement;
  to?: Arrangement;
  onPick?: (view: ViewId) => void;
  badge?: React.ReactNode;
  panels?: { view: PanelViewId; name: string; mark: LucideIcon }[];
}) {
  const [arrangement, setArrangement] = useState(from);
  return (
    <>
      <button onClick={() => to !== undefined && setArrangement(to)}>rearrange</button>
      <RegionFrame
        arrangement={arrangement}
        content={CONTENT}
        views={VIEWS}
        panels={panels}
        badges={{ chats: badge }}
        onPick={onPick}
        centre={<div data-testid="centre">centre</div>}
        onResized={vi.fn()}
      />
    </>
  );
}

const bar = () => screen.getByRole("tablist", { name: "Navigation" });

describe("the activity bar of a region with views (#1673)", () => {
  it("is a vertical tab list beside the side, one tab per view, each named and with a tooltip", () => {
    render(<WithViews from={DEFAULT_ARRANGEMENT} />);

    expect(bar()).toHaveAttribute("aria-orientation", "vertical");
    const tabs = within(bar()).getAllByRole("tab");
    expect(tabs.map((tab) => tab.getAttribute("aria-label"))).toEqual([
      "Chats",
      "Explorer",
      "Search",
      "Changes",
    ]);
    for (const tab of tabs) expect(tab.getAttribute("title")).toBeTruthy();
    // Outside the slot, so it stays when the slot is put away.
    expect(slot("left").contains(bar())).toBe(false);
  });

  it("selects the open view's tab and shows that view alone, the other mounted and hidden", () => {
    render(<WithViews from={DEFAULT_ARRANGEMENT} />);

    expect(screen.getByRole("tab", { name: "Chats" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Explorer" })).toHaveAttribute("aria-selected", "false");
    expect(screen.getByRole("tabpanel", { name: "Chats" })).toContainElement(
      screen.getByTestId("v-chats"),
    );
    expect(screen.getByTestId("v-explorer")).toBeInTheDocument();
    expect(screen.queryByRole("tabpanel", { name: "Explorer" })).toBeNull();
    // Each tab says which panel it controls.
    expect(screen.getByRole("tab", { name: "Chats" })).toHaveAttribute(
      "aria-controls",
      screen.getByRole("tabpanel", { name: "Chats" }).id,
    );
  });

  it("draws a hidden view as nothing, though the stylesheet lays the views out as boxes", () => {
    render(<WithViews from={DEFAULT_ARRANGEMENT} />);

    // `hidden` is the browser's `display: none`, which any `display` a class sets outranks: the
    // real window drew the Explorer under the Chats view until App.css said it again (train 34).
    const open = screen.getByRole("tabpanel", { name: "Chats" });
    const shut = screen.getByTestId("v-explorer").closest('[role="tabpanel"]') as HTMLElement;
    expect(shut).toHaveAttribute("hidden");
    expect(drawnWith(open, "display")).toBe("flex");
    expect(drawnWith(shut, "display")).toBe("none");
  });

  it("says a press of a tab, and the content does not change until the arrangement does", async () => {
    const onPick = vi.fn();
    render(<WithViews from={DEFAULT_ARRANGEMENT} onPick={onPick} />);

    await userEvent.click(screen.getByRole("tab", { name: "Explorer" }));
    await userEvent.click(screen.getByRole("tab", { name: "Chats" }));

    expect(onPick.mock.calls).toEqual([["explorer"], ["chats"]]);
  });

  it("is reached with the arrows and pressed with Enter, as a tab list is", async () => {
    const onPick = vi.fn();
    render(<WithViews from={DEFAULT_ARRANGEMENT} onPick={onPick} />);

    screen.getByRole("tab", { name: "Chats" }).focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("tab", { name: "Explorer" })).toHaveFocus();
    await userEvent.keyboard("{Enter}");

    expect(onPick).toHaveBeenCalledWith("explorer");
  });

  it("keeps the views mounted when the side is put away, and selects no tab", async () => {
    render(
      <WithViews
        from={DEFAULT_ARRANGEMENT}
        to={[on("navigation", "left", 0, true), on("aside", "right")]}
      />,
    );
    const chats = screen.getByTestId("v-chats");

    await rearrange();

    expect(screen.getByTestId("v-chats")).toBe(chats);
    expect(within(slot("left")).queryByRole("tabpanel")).toBeNull();
    expect(within(bar()).queryAllByRole("tab", { selected: true })).toEqual([]);
  });

  it("goes with the region to the other side", async () => {
    render(
      <WithViews
        from={DEFAULT_ARRANGEMENT}
        to={[on("navigation", "right", 1), on("aside", "right")]}
      />,
    );

    await rearrange();

    expect(bar().closest("[data-side]")).toHaveAttribute("data-side", "right");
  });

  it("draws a view's badge on its tab, and the tab is described by it", () => {
    render(
      <WithViews
        from={[on("navigation", "left", 0, true), on("aside", "right")]}
        badge={<ActivityCount count={2} said="2 chats need you" tone="needs-you" />}
      />,
    );

    const chats = screen.getByRole("tab", { name: "Chats" });
    expect(chats).toHaveTextContent("2");
    expect(chats).toHaveAccessibleDescription("2 chats need you");
  });
});

describe("the right side's activity bar (#1678)", () => {
  const right = () => screen.getByRole("tablist", { name: "Attention" });

  it("holds purlis's views in order, opens on Memory, and sits at the right edge", () => {
    render(<WithViews from={DEFAULT_ARRANGEMENT} />);

    expect(
      within(right())
        .getAllByRole("tab")
        .map((tab) => tab.getAttribute("aria-label")),
    ).toEqual(["Todos", "Memory", "Personas", "Sessions", "Vaults"]);
    expect(right().closest("[data-side]")).toHaveAttribute("data-side", "right");
    expect(screen.getByRole("tabpanel", { name: "Memory" })).toContainElement(
      screen.getByTestId("v-memory"),
    );
    // The others are mounted and hidden.
    expect(screen.getByTestId("v-todos")).toBeInTheDocument();
    expect(screen.queryByRole("tabpanel", { name: "Todos" })).toBeNull();
  });

  it("gives each extension's panel a tab after purlis's own, named and marked as it says", () => {
    render(
      <WithViews
        from={DEFAULT_ARRANGEMENT}
        panels={[{ view: "panel:ext/stats/burn", name: "Burn rate", mark: Flame }]}
      />,
    );

    const tabs = within(right()).getAllByRole("tab");
    expect(tabs.map((tab) => tab.getAttribute("aria-label")).at(-1)).toBe("Burn rate");
    expect(tabs.at(-1)).toHaveAttribute("title", "Show the Burn rate view");
  });

  it("shows an extension's panel when it is the open view", () => {
    render(
      <WithViews
        from={[
          on("navigation", "left"),
          { ...on("aside", "right"), view: "panel:ext/stats/burn" },
          on("bottom", "bottom"),
        ]}
        panels={[{ view: "panel:ext/stats/burn", name: "Burn rate", mark: Flame }]}
      />,
    );

    expect(screen.getByRole("tabpanel", { name: "Burn rate" })).toContainElement(
      screen.getByTestId("v-burn"),
    );
  });

  it("opens on Memory when the panel picked last is not contributed any more", () => {
    render(
      <WithViews
        from={[
          on("navigation", "left"),
          { ...on("aside", "right"), view: "panel:ext/stats/burn" },
          on("bottom", "bottom"),
        ]}
      />,
    );

    expect(screen.getByRole("tabpanel", { name: "Memory" })).toBeInTheDocument();
    expect(screen.queryByTestId("v-burn")).toBeNull();
  });
});
