import { vi } from "vitest";
import { screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";

/**
 * **A layout for jsdom, which lays nothing out**, for the tests that drag a tab (SI-6): a drag
 * is decided by where things are, and every box jsdom reports is empty.
 *
 * Every element is given a box by its place among its siblings — one tab a hundred pixels to
 * the right of the last — and a tablist one wide enough to hold them all, because `dnd-kit`
 * keeps a dragged tab inside its strip. Nothing else is faked: the sensor, the drop and what
 * the window then tells the core are the real ones.
 *
 * Undone by `vi.restoreAllMocks()`.
 */
export function laidOutInARow() {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    const strip = this.getAttribute("role") === "tablist";
    const at = this.parentElement ? [...this.parentElement.children].indexOf(this) : 0;
    const left = strip ? 0 : at * 100;
    const width = strip ? 1000 : 90;
    return {
      x: left,
      y: 0,
      left,
      top: 0,
      right: left + width,
      bottom: 30,
      width,
      height: 30,
      toJSON: () => ({}),
    } as DOMRect;
  });
}

/** Picks up the focused tab with the keyboard, presses `arrow` once, and puts it down. */
export async function dragWithTheKeyboard(arrow: "{ArrowLeft}" | "{ArrowRight}") {
  await userEvent.keyboard("{Shift>}[Space]{/Shift}");
  await userEvent.keyboard(arrow);
  await userEvent.keyboard("[Space]");
}

/**
 * The window's status lines that are saying something.
 *
 * **Not every `role="status"`**: each strip's `DndContext` keeps a live region of that role for
 * what it announces during a drag, and it is empty until a tab is picked up. A test about what
 * charter says asks this rather than for the one status on the page, which there no longer is.
 */
export const sayingSomething = () =>
  screen.queryAllByRole("status").filter((one) => (one.textContent ?? "") !== "");
