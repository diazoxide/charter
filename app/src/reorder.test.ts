import { describe, expect, it } from "vitest";
import { afterDrop, dropped, reslotted } from "./reorder";

/** A strip of two pinned tabs and two that are not, which every drop below is made on. */
const strip = ["p1", "p2", "u1", "u2"];
const pinned = (one: string) => one.startsWith("p");

describe("a drop inside one group", () => {
  it("moves a pinned tab to where it was dropped among the pinned ones", () => {
    expect(dropped(strip, "p2", "p1", pinned)).toEqual({ order: ["p2", "p1", "u1", "u2"] });
  });

  it("moves an unpinned tab to where it was dropped among the unpinned ones", () => {
    expect(dropped(strip, "u1", "u2", pinned)).toEqual({ order: ["p1", "p2", "u2", "u1"] });
  });

  it("changes nothing when a tab is dropped back on itself", () => {
    expect(dropped(strip, "u1", "u1", pinned)).toBeUndefined();
  });

  it("changes nothing for a tab the strip does not draw", () => {
    expect(dropped(strip, "gone", "u1", pinned)).toBeUndefined();
    expect(dropped(strip, "u1", "gone", pinned)).toBeUndefined();
  });
});

describe("a drop across the pinned boundary", () => {
  it("pins an unpinned tab dropped among the pinned ones, where it was dropped", () => {
    expect(dropped(strip, "u2", "p1", pinned)).toEqual({
      order: ["u2", "p1", "p2", "u1"],
      pinned: true,
    });
  });

  it("unpins a pinned tab dropped among the unpinned ones, where it was dropped", () => {
    expect(dropped(strip, "p1", "u2", pinned)).toEqual({
      order: ["p2", "u1", "u2", "p1"],
      pinned: false,
    });
  });

  it("unpins the last pinned tab dropped on the first unpinned one, just after the boundary", () => {
    expect(dropped(strip, "p2", "u1", pinned)).toEqual({
      order: ["p1", "u1", "p2", "u2"],
      pinned: false,
    });
  });
});

describe("a fixed tab", () => {
  const fixed = (one: string) => one === "u1";

  it("is never moved", () => {
    expect(dropped(strip, "u1", "u2", pinned, fixed)).toBeUndefined();
  });

  it("is never displaced by a tab dropped on it", () => {
    expect(dropped(strip, "u2", "u1", pinned, fixed)).toBeUndefined();
    expect(dropped(strip, "p1", "u1", pinned, fixed)).toBeUndefined();
  });

  it("keeps its place while the tabs either side of it are rearranged", () => {
    const first = (one: string) => one === "f";
    expect(dropped(["f", "a", "b", "c"], "c", "a", () => false, first)).toEqual({
      order: ["f", "c", "a", "b"],
    });
  });
});

describe("putting a strip's order back into the whole it was drawn from", () => {
  it("puts the strip's tabs in the places those tabs held, in the strip's order", () => {
    // The chat strip draws one workspace's tabs out of every tab the project has open.
    expect(reslotted(["a1", "b1", "a2", "b2", "a3"], ["a3", "a1", "a2"])).toEqual([
      "a3",
      "b1",
      "a1",
      "b2",
      "a2",
    ]);
  });

  it("leaves everything alone when the strip's order is the one it already had", () => {
    expect(reslotted(["a", "b", "c"], ["a", "c"])).toEqual(["a", "b", "c"]);
  });
});

describe("after a drop on a strip that draws only some of its tabs", () => {
  it("moves the dragged tab among every tab of its group, hidden ones included", () => {
    // `h` is behind show-more; the drop is made on what the strip draws.
    const whole = ["p1", "p2", "h", "u1"];
    const drawn = ["p1", "p2", "u1"];
    expect(
      afterDrop({ whole, drawn, moved: "p2", onto: "p1", isPinned: (one) => one !== "u1" }),
    ).toEqual({ order: ["p2", "p1", "h", "u1"] });
  });

  it("keeps the whole pinned first when a drop pins a tab", () => {
    const whole = ["p1", "u1", "u2"];
    expect(afterDrop({ whole, drawn: whole, moved: "u2", onto: "p1", isPinned: pinned })).toEqual({
      order: ["u2", "p1", "u1"],
      pinned: true,
    });
  });

  it("changes nothing when the drop changed nothing", () => {
    expect(
      afterDrop({ whole: strip, drawn: strip, moved: "p1", onto: "p1", isPinned: pinned }),
    ).toBeUndefined();
  });
});
