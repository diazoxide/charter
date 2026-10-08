import { beforeEach, describe, expect, it } from "vitest";
import { forgetKeyboard, giveKeyboardTo, paneDrawn } from "./paneKeyboard";

const PLANE = "/home/dev/plane";

beforeEach(forgetKeyboard);

describe("the keyboard after a tab is switched to another chat", () => {
  it("goes into that chat's terminal at once when it is on screen", () => {
    const took: number[] = [];
    paneDrawn(PLANE, 4, () => took.push(4));
    paneDrawn(PLANE, 5, () => took.push(5));

    giveKeyboardTo(PLANE, 5);

    expect(took).toEqual([5]);
  });

  it("goes into it when it is drawn, for a chat whose terminal is not there yet", () => {
    const took: number[] = [];
    giveKeyboardTo(PLANE, 4);
    expect(took).toEqual([]);

    paneDrawn(PLANE, 5, () => took.push(5));
    paneDrawn(PLANE, 4, () => took.push(4));

    expect(took).toEqual([4]);
  });

  it("is owed to the last chat asked for, and to no chat before it", () => {
    const took: number[] = [];
    giveKeyboardTo(PLANE, 4);
    giveKeyboardTo(PLANE, 5);

    paneDrawn(PLANE, 4, () => took.push(4));
    paneDrawn(PLANE, 5, () => took.push(5));

    expect(took).toEqual([5]);
  });

  it("is taken once: the same chat drawn again later takes nothing", async () => {
    const took: string[] = [];
    giveKeyboardTo(PLANE, 4);
    const gone = paneDrawn(PLANE, 4, () => took.push("first"));
    gone();
    // Drawn twice in one turn, as React draws a pane in development: the one that stays has it.
    paneDrawn(PLANE, 4, () => took.push("second"))();
    expect(took).toEqual(["first", "second"]);

    await Promise.resolve();
    // Its tab went behind and came forward again: nobody asked for the keyboard this time.
    paneDrawn(PLANE, 4, () => took.push("later"));
    expect(took).toEqual(["first", "second"]);
  });

  it("tells one project's chat from another project's chat of the same number", () => {
    const took: string[] = [];
    giveKeyboardTo("/other/plane", 4);
    paneDrawn(PLANE, 4, () => took.push("here"));
    expect(took).toEqual([]);
  });

  it("forgets a terminal that is gone", () => {
    const took: number[] = [];
    paneDrawn(PLANE, 4, () => took.push(4))();

    giveKeyboardTo(PLANE, 4);

    expect(took).toEqual([]);
  });
});
