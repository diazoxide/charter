import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Inbox } from "lucide-react";
import { EmptyState } from "./EmptyState";

/**
 * **The one empty state** (DS-3 #626): what is missing, what goes there, and the way out where a
 * surface has one. Every view tab draws its empty state through it (`emptyStates.guard.test.ts`).
 */

afterEach(cleanup);

describe("EmptyState", () => {
  it("says what is missing, what goes there, and offers the way out", async () => {
    const add = vi.fn();
    render(
      <EmptyState
        mark={Inbox}
        headline="No saves recorded yet"
        body="Each save is listed here."
        action={
          <button type="button" onClick={add}>
            Save
          </button>
        }
        testid="empty"
      />,
    );

    const empty = screen.getByTestId("empty");
    expect(empty).toHaveClass("empty-state", "empty-state-page");
    expect(screen.getByText("No saves recorded yet")).toHaveClass("empty-state-headline");
    expect(screen.getByText("Each save is listed here.")).toHaveClass("empty-state-body");
    expect(empty.querySelector("svg.empty-state-mark")).not.toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(add).toHaveBeenCalledTimes(1);
    // Not a live region unless the caller says it is one.
    expect(empty).not.toHaveAttribute("role");
  });

  it("draws only the headline where there is nothing more to say, at a panel's size", () => {
    render(<EmptyState headline="Nothing archived" body="" size="panel" testid="empty" />);

    const empty = screen.getByTestId("empty");
    expect(empty).toHaveClass("empty-state-panel");
    expect(empty.querySelector(".empty-state-body")).toBeNull();
    expect(empty.querySelector(".empty-state-action")).toBeNull();
    expect(empty.querySelector(".empty-state-mark")).toBeNull();
  });

  it("is said to a screen reader as it changes where the caller asks", () => {
    render(<EmptyState headline="Reading" role="status" />);

    expect(screen.getByRole("status")).toHaveTextContent("Reading");
  });
});
