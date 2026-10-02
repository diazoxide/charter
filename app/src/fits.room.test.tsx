import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useRoom } from "./fits";

/**
 * `useRoom` across a strip that goes and comes back, which is what a project's strips do each
 * time it leaves the front and returns (`PlaneView`, FR-27).
 *
 * jsdom lays nothing out, so the strip's width is stubbed by its name, as `ShowMore.test.tsx`
 * does. What is asserted is the width the hook hands the strip on every render, because that is
 * what `fitting` draws by: a width of zero draws every tab the strip holds.
 */

let was: PropertyDescriptor | undefined;
beforeEach(() => {
  was = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get(this: HTMLElement) {
      return this.getAttribute("aria-label") === "Strip" ? 480 : 0;
    },
  });
});
afterEach(() => {
  if (was) Object.defineProperty(HTMLElement.prototype, "clientWidth", was);
  else Reflect.deleteProperty(HTMLElement.prototype, "clientWidth");
});

/** A strip that is drawn only while `shown`, writing down the width of every render. */
function Strip({ shown, seen }: { shown: boolean; seen: number[] }) {
  const { strip, width } = useRoom(3);
  seen.push(width);
  return shown ? <div aria-label="Strip" ref={strip} /> : null;
}

describe("the room a strip has, when the strip goes and comes back", () => {
  it("keeps the width it last measured while the strip is not drawn", () => {
    const seen: number[] = [];
    const { rerender } = render(<Strip shown seen={seen} />);
    expect(seen.at(-1)).toBe(480);

    act(() => rerender(<Strip shown={false} seen={seen} />));
    expect(seen.at(-1)).toBe(480);
  });

  it("draws the strip at that width from its first render back, never at zero", () => {
    const seen: number[] = [];
    const { rerender } = render(<Strip shown seen={seen} />);
    act(() => rerender(<Strip shown={false} seen={seen} />));
    const back = seen.length;

    act(() => rerender(<Strip shown seen={seen} />));
    expect(seen.slice(back)).not.toContain(0);
    expect(seen.at(-1)).toBe(480);
  });
});
