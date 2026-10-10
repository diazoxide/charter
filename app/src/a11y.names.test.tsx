import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { expectEveryControlNamed, namelessControls, paidDebts } from "./a11y.testkit";

/**
 * **The check that every control has a name** (DS-6, #629), on its own cases: what counts as a
 * name, what is not looked at, and how a debt is let through and then paid. The surfaces it is
 * run on are `settings/Names.test.tsx` and `editor/Names.test.tsx`.
 */

afterEach(cleanup);

describe("a control's name", () => {
  it("is found missing on an icon button, a bare input and an unlabelled tab", () => {
    const { container } = render(
      <div>
        <button type="button" className="gear">
          <svg aria-hidden="true" />
        </button>
        <input type="text" />
        <div role="tablist">
          <div role="tab" aria-selected="true" />
        </div>
      </div>,
    );

    expect(namelessControls(container)).toEqual([
      "button <button.gear>",
      "textbox <input>",
      "tab <div>",
    ]);
  });

  it("is its text, its aria-label, its label, the element it is labelled by, or its title", () => {
    const { container } = render(
      <div>
        <button type="button">Save</button>
        <button type="button" aria-label="Close" />
        <label>
          Name <input type="text" />
        </label>
        <span id="said">Filter</span>
        <input type="search" aria-labelledby="said" />
        <button type="button" title="Settings" />
        <a href="#here">Here</a>
      </div>,
    );

    expect(namelessControls(container)).toEqual([]);
  });

  it("is not asked of what nobody is told about: inside aria-hidden or hidden", () => {
    const { container } = render(
      <div>
        <div aria-hidden="true">
          <button type="button" />
        </div>
        <button type="button" hidden />
      </div>,
    );

    expect(namelessControls(container)).toEqual([]);
  });

  it("is not a name when it is only blanks", () => {
    const { container } = render(<button type="button" aria-label="  " />);

    expect(namelessControls(container)).toEqual(["button <button>"]);
  });
});

describe("a surface held to it", () => {
  it("fails naming the nameless control", () => {
    const { container } = render(<button type="button" className="closer" />);

    expect(() => expectEveryControlNamed(container)).toThrow(/button <button\.closer>/);
  });

  it("lets a listed debt through, and says which debts are paid", () => {
    const { container } = render(<button type="button" className="closer" />);

    const found = expectEveryControlNamed(container, ["button <button.closer>"]);

    expect(paidDebts(["button <button.closer>", "tab <div.gone>"], found)).toEqual([
      "tab <div.gone>",
    ]);
  });
});
