import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import type { Offer } from "./actions";
import { EndingChat, KEEP_SAYS, RUNNING_SAYS, STOP_SAYS } from "./EndingChat";

afterEach(cleanup);

/**
 * **The answer about the chats a closing chat started** (#630, DS-8): a radio group of the
 * settings set, so it is a Radix radio group in WebKit's tab sequence (`docs/ui-primitives.md`:
 * a native radio `<input>` is in WebKit's restricted one), and each answer says what it does.
 */
describe("the close question's answer about the chats still at work", () => {
  const offer = { id: "pane.close", title: "Close chat" } as unknown as Offer;

  it("is one group, named by its question, whose answers each say what they do", () => {
    render(
      <EndingChat
        offer={offer}
        running={["helper"]}
        onEnd={() => undefined}
        onSmartClose={() => undefined}
        onCancel={() => undefined}
      />,
    );
    const group = screen.getByRole("radiogroup", { name: RUNNING_SAYS(["helper"]) });
    expect(group).toBeTruthy();
    const keep = screen.getByRole("radio", { name: "Keep them running" });
    const stop = screen.getByRole("radio", { name: "Stop them" });
    expect(keep.getAttribute("aria-checked")).toBe("true");
    // A Radix item is a button the roving focus gives a written-down tabindex.
    expect(keep.tagName).toBe("BUTTON");
    expect(keep.getAttribute("tabindex")).not.toBeNull();
    expect(screen.getByText(KEEP_SAYS).id).toBe(keep.getAttribute("aria-describedby"));
    expect(screen.getByText(STOP_SAYS).id).toBe(stop.getAttribute("aria-describedby"));
  });

  it("sends the answer picked with Close", async () => {
    const onEnd = vi.fn();
    render(
      <EndingChat
        offer={offer}
        running={["helper"]}
        onEnd={onEnd}
        onSmartClose={() => undefined}
        onCancel={() => undefined}
      />,
    );
    await userEvent.click(screen.getByRole("radio", { name: "Stop them" }));
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onEnd).toHaveBeenCalledWith(true);
  });
});
