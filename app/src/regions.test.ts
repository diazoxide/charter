import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import {
  CATALOGUE,
  DEFAULT_ARRANGEMENT,
  inSlots,
  REGION_IDS,
  remembered,
  shownIn,
  SIDES,
  SLOTS,
  slotSize,
  useArrangement,
} from "./regions";

const KEY = "charter.layout";

const put = (document: unknown) => globalThis.localStorage.setItem(KEY, JSON.stringify(document));
const placed = (arrangement: ReturnType<typeof remembered>, id: string) =>
  arrangement.find((one) => one.id === id);

beforeEach(() => globalThis.localStorage.clear());
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("the arrangement a window has never been told about", () => {
  it("is charter ADR 0038's four regions: explorer left, attention right, state along the bottom", () => {
    expect(remembered()).toEqual([
      { id: "explorer", side: "left", order: 0, collapsed: false },
      { id: "aside", side: "right", order: 0, collapsed: false },
      { id: "bottom", side: "bottom", order: 0, collapsed: false },
    ]);
  });

  it("names every region in the catalogue, so no region can exist with nowhere to be", () => {
    expect(DEFAULT_ARRANGEMENT.map((one) => one.id).sort()).toEqual([...REGION_IDS].sort());
  });

  it("puts every slot inside the bounds the slot itself allows", () => {
    // A default size outside its own slot's bounds is a window that lays out somewhere other
    // than where the arrangement says on its very first frame.
    for (const side of SIDES) {
      const size = slotSize(side, inSlots(DEFAULT_ARRANGEMENT)[side]);
      expect(size).toBeGreaterThanOrEqual(SLOTS[side].least);
      expect(size).toBeLessThanOrEqual(SLOTS[side].most);
    }
  });
});

describe("putting a region away", () => {
  it("puts it away and brings it back", () => {
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.toggle("explorer"));
    expect(placed(result.current.arrangement, "explorer")?.collapsed).toBe(true);

    act(() => result.current.toggle("explorer"));
    expect(placed(result.current.arrangement, "explorer")?.collapsed).toBe(false);
  });

  it("leaves every other region where it was", () => {
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.toggle("explorer"));

    expect(result.current.arrangement.filter((one) => one.collapsed).map((one) => one.id)).toEqual([
      "explorer",
    ]);
  });

  it("remembers the answer, so a project switch does not bring a region back", () => {
    // The regions are the WINDOW's layout, and a `PlaneView` that is not in front draws
    // nothing at all — so without this, looking at another project and back would undo it.
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.toggle("bottom"));

    expect(remembered()).toEqual(result.current.arrangement);
    expect(placed(remembered(), "bottom")?.collapsed).toBe(true);
  });
});

describe("moving a region", () => {
  it("puts it on the side it was moved to, and remembers", () => {
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.move("bottom", "right", 1));

    expect(placed(result.current.arrangement, "bottom")).toMatchObject({
      side: "right",
      order: 1,
    });
    expect(placed(remembered(), "bottom")).toMatchObject({ side: "right", order: 1 });
  });

  it("leaves the side it came from empty rather than holding a place for it", () => {
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.move("bottom", "right", 1));

    expect(inSlots(result.current.arrangement).bottom).toEqual([]);
  });
});

describe("how big each slot was left", () => {
  it("is remembered, which is what charter-app#141 deferred", () => {
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.resized({ left: 31 }));

    expect(placed(remembered(), "explorer")?.size).toBe(31);
    expect(slotSize("left", inSlots(remembered()).left)).toBe(31);
  });

  it("is the slot's, so every region drawn in it takes the width it was drawn at", () => {
    const { result } = renderHook(() => useArrangement());
    act(() => result.current.move("bottom", "left", 1));

    act(() => result.current.resized({ left: 33 }));

    expect(placed(result.current.arrangement, "explorer")?.size).toBe(33);
    expect(placed(result.current.arrangement, "bottom")?.size).toBe(33);
  });

  it("is not written to a region that was not drawn", () => {
    // A region that is away was not measured. Giving it the slot's width would be charter
    // deciding how wide it should come back, from a drag it had no part in.
    const { result } = renderHook(() => useArrangement());
    act(() => result.current.move("bottom", "left", 1));
    act(() => result.current.toggle("bottom"));

    act(() => result.current.resized({ left: 33 }));

    expect(placed(result.current.arrangement, "bottom")?.size).toBeUndefined();
  });

  it("ignores a zero, because a collapsed slot measures zero and that is not a width", () => {
    const { result } = renderHook(() => useArrangement());
    act(() => result.current.resized({ left: 28 }));

    act(() => result.current.resized({ left: 0 }));

    expect(placed(result.current.arrangement, "explorer")?.size).toBe(28);
  });

  it("falls back on the catalogue until something has been dragged", () => {
    expect(slotSize("right", inSlots(DEFAULT_ARRANGEMENT).right)).toBe(CATALOGUE.aside.size);
  });

  it("is the first region DRAWN in the slot, not the first one placed there", () => {
    // A slot holding a region that is put away and one that is not takes the width of the one
    // the operator can see. Asking the first placed region would size the slot from something
    // that is not on screen, and the width would jump when it came back.
    const slots = inSlots([
      { id: "explorer", side: "left", order: 0, collapsed: true, size: 40 },
      { id: "bottom", side: "left", order: 1, collapsed: false, size: 12 },
      { id: "aside", side: "right", order: 0, collapsed: false },
    ]);

    expect(slotSize("left", slots.left)).toBe(12);
  });
});

describe("a stored arrangement that is not what this build writes", () => {
  it("draws a region the document does not mention", () => {
    // A document written by an older build, or by hand. Losing a region with no way to notice
    // it went is worse than ignoring half a preference.
    put({ regions: [{ id: "explorer", side: "left", order: 0, collapsed: true }] });

    expect(placed(remembered(), "explorer")?.collapsed).toBe(true);
    expect(placed(remembered(), "aside")).toEqual({
      id: "aside",
      side: "right",
      order: 0,
      collapsed: false,
    });
  });

  it("drops a region this build does not have, and keeps the rest", () => {
    put({
      regions: [
        { id: "minimap", side: "left", order: 0, collapsed: false },
        { id: "bottom", side: "right", order: 2, collapsed: false },
      ],
    });

    expect(remembered().map((one) => one.id)).toEqual(["explorer", "aside", "bottom"]);
    expect(placed(remembered(), "bottom")).toMatchObject({ side: "right", order: 2 });
  });

  it("draws a region whose `collapsed` is not an answer", () => {
    // Only `true` puts a region away. `"yes"` is a document charter cannot read, and the
    // failure of a preference must not be the operator losing a region with no way to notice.
    put({ regions: [{ id: "aside", side: "right", order: 0, collapsed: "yes" }] });

    expect(placed(remembered(), "aside")?.collapsed).toBe(false);
  });

  it("uses the default side for a side that is not a side", () => {
    put({ regions: [{ id: "aside", side: "diagonal", order: 0, collapsed: false }] });

    expect(placed(remembered(), "aside")?.side).toBe("right");
  });

  it("uses the default order for an order that is not a number", () => {
    put({ regions: [{ id: "aside", side: "right", order: "first", collapsed: false }] });

    expect(placed(remembered(), "aside")?.order).toBe(0);
  });

  it("ignores a size that is not a percentage a panel could be given", () => {
    for (const size of [-5, 0, 140, "20%", null, Number.NaN]) {
      globalThis.localStorage.clear();
      put({ regions: [{ id: "explorer", side: "left", order: 0, collapsed: false, size }] });

      expect(placed(remembered(), "explorer")?.size).toBeUndefined();
    }
  });

  it("keeps a size that is one", () => {
    put({ regions: [{ id: "explorer", side: "left", order: 0, collapsed: false, size: 12.5 }] });

    expect(placed(remembered(), "explorer")?.size).toBe(12.5);
  });

  it("draws the default arrangement when the document is not one", () => {
    for (const held of ["{{{", "null", "[]", '"a layout"', "7", '{"regions":"none"}']) {
      globalThis.localStorage.setItem(KEY, held);

      expect(remembered()).toEqual(DEFAULT_ARRANGEMENT);
    }
  });

  it("skips an entry that is not an object at all", () => {
    put({ regions: [null, "explorer", ["aside"], { id: "bottom", side: "left", order: 3 }] });

    expect(remembered().map((one) => one.id)).toEqual(["explorer", "aside", "bottom"]);
    expect(placed(remembered(), "bottom")).toMatchObject({ side: "left", order: 3 });
  });
});

describe("the webview refusing storage", () => {
  it("draws the default arrangement when it will not be read", () => {
    vi.spyOn(globalThis.Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("storage is blocked");
    });

    expect(remembered()).toEqual(DEFAULT_ARRANGEMENT);
  });

  it("still puts a region away when it will not be stored", () => {
    // The preference is lost at the next launch. The window must not fail to lay out over it.
    vi.spyOn(globalThis.Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("storage is blocked");
    });
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.toggle("aside"));

    expect(placed(result.current.arrangement, "aside")?.collapsed).toBe(true);
  });
});

describe("the arrangement as the slots it draws", () => {
  it("gives every side a list, including one nothing is placed in", () => {
    expect(Object.keys(inSlots(DEFAULT_ARRANGEMENT)).sort()).toEqual([...SIDES].sort());
    expect(inSlots([{ id: "explorer", side: "left", order: 0, collapsed: false }]).right).toEqual(
      [],
    );
  });

  it("sorts a side by order", () => {
    const slots = inSlots([
      { id: "explorer", side: "left", order: 5, collapsed: false },
      { id: "bottom", side: "left", order: 1, collapsed: false },
      { id: "aside", side: "left", order: 3, collapsed: false },
    ]);

    expect(slots.left.map((one) => one.id)).toEqual(["bottom", "aside", "explorer"]);
  });

  it("breaks a tie the same way every launch", () => {
    // Two regions given the same order is a document a person wrote. Leaving the answer to
    // whatever order the array happened to be in would move the window between launches.
    const slots = inSlots([
      { id: "aside", side: "left", order: 0, collapsed: false },
      { id: "explorer", side: "left", order: 0, collapsed: false },
    ]);

    expect(slots.left.map((one) => one.id)).toEqual(["explorer", "aside"]);
  });

  it("counts what is drawn in a slot and not what is placed there", () => {
    const placedThere = [
      { id: "explorer" as const, side: "left" as const, order: 0, collapsed: true },
      { id: "bottom" as const, side: "left" as const, order: 1, collapsed: false },
    ];

    expect(shownIn(placedThere).map((one) => one.id)).toEqual(["bottom"]);
  });
});
