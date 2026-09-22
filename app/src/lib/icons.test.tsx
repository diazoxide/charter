import { render } from "@testing-library/react";
import { CircleAlert } from "lucide-react";
import { describe, expect, it } from "vitest";

/**
 * Lucide is charter's icon set, and this is the one thing about it the theme layer has to be
 * sure of before anybody puts an icon on a button: **an icon takes its colour from the text
 * around it.** Lucide draws with `stroke="currentColor"`, so an icon inside a rule that sets
 * `color: var(--state-failed)` is that colour, and a theme reaches it without an icon ever
 * naming a colour. An icon set that baked its own fills in would be a second palette.
 */
describe("an icon is drawn in the colour of the text it sits in", () => {
  it("strokes with currentColor and fills with nothing", () => {
    const { container } = render(<CircleAlert aria-label="trouble" />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("stroke")).toBe("currentColor");
    expect(svg?.getAttribute("fill")).toBe("none");
  });

  it("takes its size from the font, so a token-sized row sizes its own icons", () => {
    const { container } = render(<CircleAlert size="1em" aria-label="trouble" />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("width")).toBe("1em");
  });
});
