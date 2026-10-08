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

  it("is owed no longer once the person puts the keyboard somewhere themselves", () => {
    const took: number[] = [];
    const field = document.createElement("input");
    document.body.append(field);
    giveKeyboardTo(PLANE, 4);

    field.focus();
    paneDrawn(PLANE, 4, () => took.push(4));

    expect(took).toEqual([]);
    field.remove();
  });

  it("is still owed when the focus only falls to the page, as it does when a terminal goes", () => {
    const took: number[] = [];
    giveKeyboardTo(PLANE, 4);

    document.body.dispatchEvent(new FocusEvent("focusin", { bubbles: true }));
    paneDrawn(PLANE, 4, () => took.push(4));

    expect(took).toEqual([4]);
  });

  it("is not taken from a dialog the person is answering", () => {
    const took: number[] = [];
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "alertdialog");
    const answer = document.createElement("button");
    dialog.append(answer);
    document.body.append(dialog);
    paneDrawn(PLANE, 4, () => took.push(4));
    answer.focus();

    giveKeyboardTo(PLANE, 4);

    expect(took).toEqual([]);
    expect(document.activeElement).toBe(answer);
    dialog.remove();
  });

  it("forgets a terminal that is gone", () => {
    const took: number[] = [];
    paneDrawn(PLANE, 4, () => took.push(4))();

    giveKeyboardTo(PLANE, 4);

    expect(took).toEqual([]);
  });
});
