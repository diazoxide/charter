import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { catalogue, catalogued, type Offer } from "./actions";
import { noTabs } from "./tabs";
import { NeedsYou } from "./NeedsYou";

afterEach(cleanup);

const nameOf = (session: number) => `ide.${session}`;
/** A catalogue with no rows, for a test about what the queue says rather than what it does. */
const NONE = new Map<string, Offer>();

describe("the needs-you queue", () => {
  it("says nothing needs you when nothing asked and every chat can say so", () => {
    render(
      <NeedsYou
        queue={[]}
        quiet={[]}
        nameOf={nameOf}
        show={() => {}}
        offers={NONE}
        onPress={() => {}}
      />,
    );

    expect(screen.getByLabelText("Needs you")).toHaveTextContent(/^Nothing needs you$/);
  });

  it("names a chat that can be waiting on you without saying so, beside an empty queue", () => {
    // #27. A Codex chat stopped mid-turn for an approval says nothing. "Nothing needs you"
    // alone, over the top of it, would be the app claiming something it cannot see — M1.3's
    // rule that the honest half is named, never folded into the reassuring answer.
    render(
      <NeedsYou
        queue={[]}
        quiet={["ide.7"]}
        nameOf={nameOf}
        show={() => {}}
        offers={NONE}
        onPress={() => {}}
      />,
    );

    const queue = screen.getByLabelText("Needs you");
    expect(queue).toHaveTextContent("ide.7 can be waiting on you without saying so.");
  });

  it("does not claim nothing needs you while a chat cannot say whether it does", () => {
    // #52. The headline is a claim about every chat on the plane, and the hedge under it
    // used to contradict it in the same breath. What charter knows is that nothing has
    // SAID so, and that is all the empty state is allowed to say.
    render(
      <NeedsYou
        queue={[]}
        quiet={["ide.7"]}
        nameOf={nameOf}
        show={() => {}}
        offers={NONE}
        onPress={() => {}}
      />,
    );

    const queue = screen.getByLabelText("Needs you");
    expect(queue).toHaveTextContent("Nothing has said it needs you");
    expect(queue.textContent).not.toContain("Nothing needs you");
  });

  it("counts several such chats rather than listing them all", () => {
    render(
      <NeedsYou
        queue={[]}
        quiet={["ide.7", "ide.8"]}
        nameOf={nameOf}
        show={() => {}}
        offers={NONE}
        onPress={() => {}}
      />,
    );

    expect(screen.getByLabelText("Needs you")).toHaveTextContent(
      "2 chats can be waiting on you without saying so.",
    );
  });

  it("still names them when the queue is not empty", () => {
    render(
      <NeedsYou
        queue={[3]}
        quiet={["ide.7"]}
        nameOf={nameOf}
        show={() => {}}
        offers={NONE}
        onPress={() => {}}
      />,
    );

    const queue = screen.getByLabelText("Needs you");
    expect(queue).toHaveTextContent("1 need you");
    expect(screen.getByRole("button", { name: "ide.3" })).toBeInTheDocument();
    expect(queue).toHaveTextContent("ide.7 can be waiting on you without saying so.");
  });
});

describe("ignoring a chat in the queue (charter-app#248)", () => {
  /** The queue as the window draws it: its rows are the catalogue's, as every surface's are. */
  function queued(queue: number[]) {
    const pressed: string[] = [];
    const offers = catalogued(
      catalogue({ tabs: noTabs(), workspaces: [], needsYou: queue, nameOf }),
    );
    render(
      <NeedsYou
        queue={queue}
        quiet={[]}
        nameOf={nameOf}
        show={() => {}}
        offers={offers}
        onPress={(offer: Offer) => pressed.push(offer.id)}
      />,
    );
    return pressed;
  }

  it("puts an Ignore on every chat asking, named for what it does", async () => {
    const pressed = queued([3, 5]);

    await userEvent.click(screen.getByRole("button", { name: "Ignore ide.5 until it asks again" }));

    expect(pressed).toEqual(["needs.ignore:5"]);
    expect(
      screen.getByRole("button", { name: "Ignore ide.3 until it asks again" }),
    ).toBeInTheDocument();
  });

  it("keeps the Ignore out of the Tab order, so the queue is still one stop", () => {
    // charter-app#189: fifty chats asking must not be a hundred stops. The keyboard's way to
    // it is Delete on the chat, below, and the palette's row for it.
    queued([3, 5]);

    for (const ignore of screen.getAllByRole("button", { name: /^Ignore / }))
      expect(ignore).toHaveAttribute("tabindex", "-1");
  });

  it("ignores the focused chat on Delete, and leaves the keyboard on the next one", async () => {
    const pressed = queued([3, 5]);
    screen.getByRole("button", { name: "ide.3" }).focus();

    await userEvent.keyboard("{Delete}");

    expect(pressed).toEqual(["needs.ignore:3"]);
    await waitFor(() => expect(screen.getByRole("button", { name: "ide.5" })).toHaveFocus());
  });

  it("leaves the keyboard on the one before when the last chat is ignored", async () => {
    queued([3, 5]);
    screen.getByRole("button", { name: "ide.5" }).focus();

    await userEvent.keyboard("{Delete}");

    await waitFor(() => expect(screen.getByRole("button", { name: "ide.3" })).toHaveFocus());
  });

  it("leaves a chord alone", async () => {
    const pressed = queued([3]);
    screen.getByRole("button", { name: "ide.3" }).focus();

    await userEvent.keyboard("{Shift>}{Delete}{/Shift}");

    expect(pressed).toEqual([]);
  });
});
