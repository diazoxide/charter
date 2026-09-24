import { describe, expect, it } from "vitest";
import { capacity, fitting, LEAST, LEAST_TUNED_AT, leastAt } from "./fits";

/**
 * The arithmetic that decides what a strip draws.
 *
 * **This is the part `panes.e2e.ts` cannot ask and jsdom cannot answer.** A width is a fact
 * about a laid-out window, so a unit test can only assert what the app does with a width it is
 * given — and that is exactly what this file is for. The other half, that a real WebView gives
 * the strip a width at all and that the tabs in it really do not overflow, is a scenario and
 * lives there.
 */

/** Tabs by name, which is all this cares about. */
const four = ["a", "b", "c", "d"];

describe("how many tabs fit", () => {
  it("is the width over the floor", () => {
    expect(capacity(400, 100)).toBe(4);
    expect(capacity(399, 100)).toBe(3);
  });

  it("is everything when nobody has said how wide the strip is", () => {
    // Zero is the absence of a measurement, not a strip with no room — and every environment
    // with no layout answers zero, including jsdom and the first frame of a real one.
    expect(capacity(0, 100)).toBe(Infinity);
  });

  it("is never zero, however narrow the window is", () => {
    // One squeezed tab is a strip. No tabs at all is a window that looks like it has lost
    // the operator's chats.
    expect(capacity(40, 100)).toBe(1);
  });
});

describe("what a strip draws", () => {
  it("draws everything when everything fits", () => {
    expect(fitting(four, "a", 4 * 100, 100)).toEqual({ shown: four, hidden: [] });
  });

  it("draws a prefix of the order and hides the rest", () => {
    expect(fitting(four, "a", 2 * 100, 100)).toEqual({ shown: ["a", "b"], hidden: ["c", "d"] });
  });

  it("never re-orders what it draws", () => {
    // ADR 0039's first rule. A tab that moves under the cursor breaks aiming, and a collapse
    // is the one change most likely to introduce a sort nobody asked for.
    const { shown, hidden } = fitting(four, "d", 2 * 100, 100);
    expect([...shown, ...hidden].sort()).toEqual([...four].sort());
    expect(shown).toEqual([...shown].sort((one, other) => four.indexOf(one) - four.indexOf(other)));
  });

  it("always draws the selected tab, in the place the order gives it", () => {
    // The promise the scroller kept with `scrollIntoView`. It takes the place of the last tab
    // that fits and stays where the order puts it — a tab that jumped to the right-hand edge
    // when it was picked would be the moving target the record refuses.
    expect(fitting(four, "d", 2 * 100, 100)).toEqual({ shown: ["a", "d"], hidden: ["b", "c"] });
  });

  it("draws the selected tab even with room for one", () => {
    expect(fitting(four, "c", 100, 100)).toEqual({ shown: ["c"], hidden: ["a", "b", "d"] });
  });

  it("hides nothing when nothing is selected and everything fits", () => {
    expect(fitting(four, undefined, 0, 100)).toEqual({ shown: four, hidden: [] });
  });

  it("collapses without a selected tab too", () => {
    // A workspace strip on a plane charter has not read yet has no focused workspace, and a
    // project strip can be drawn while nothing is in front.
    expect(fitting(four, undefined, 2 * 100, 100)).toEqual({
      shown: ["a", "b"],
      hidden: ["c", "d"],
    });
  });

  it("ignores a selected tab that is not on this strip", () => {
    // The chat strip shows one workspace's chats (ADR 0036), so the tab in front can belong
    // to another strip entirely for the frame between a close and the focus following it.
    expect(fitting(four, "z", 2 * 100, 100)).toEqual({ shown: ["a", "b"], hidden: ["c", "d"] });
  });

  it("gives every hidden tab back exactly once", () => {
    // What makes the menu reachable-by-construction: nothing may be dropped between the two
    // lists, because a tab in neither is a chat with no way to it at all.
    const many = Array.from({ length: 50 }, (_, at) => at + 1);
    const { shown, hidden } = fitting(many, 50, 8 * LEAST.chat, LEAST.chat);
    expect(shown).toHaveLength(8);
    expect([...shown, ...hidden].sort((one: number, other: number) => one - other)).toEqual(many);
  });
});

describe("the floors the three strips fit by", () => {
  it("gets wider as the axis gets shallower", () => {
    // The operator's "PROJECT is holder of workspaces, workspaces are holder of sessions",
    // as a number: a project's segment is the widest thing on the window and a chat's is the
    // narrowest. A change that made them equal would put the nesting back to three rows of
    // the same thing, which is what he was complaining about.
    expect(LEAST.project).toBeGreaterThan(LEAST.workspace);
    expect(LEAST.workspace).toBeGreaterThan(LEAST.chat);
  });
});

describe("the floors at the window text size in force (charter-app#283)", () => {
  it("are the floors themselves at the size they were tuned at, and scale with the text", () => {
    expect(leastAt(LEAST.chat, LEAST_TUNED_AT)).toBe(LEAST.chat);
    expect(leastAt(LEAST.chat, 26)).toBe(LEAST.chat * 2);
    // So at the new 14px default a tab floor is a step wider than it was at 13px.
    expect(leastAt(LEAST.chat, 14)).toBeGreaterThan(LEAST.chat);
    expect(leastAt(LEAST.project, 10)).toBeLessThan(LEAST.project);
  });

  it("so a strip draws fewer, wider tabs when the text is bigger", () => {
    const many = Array.from({ length: 20 }, (_, at) => at);
    const width = 8 * LEAST.chat;
    const atDefault = fitting(many, undefined, width, leastAt(LEAST.chat, LEAST_TUNED_AT)).shown;
    const atBigger = fitting(many, undefined, width, leastAt(LEAST.chat, 21)).shown;
    expect(atBigger.length).toBeLessThan(atDefault.length);
  });
});
