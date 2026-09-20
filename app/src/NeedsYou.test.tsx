import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { NeedsYou } from "./NeedsYou";

afterEach(cleanup);

const nameOf = (session: number) => `ide.${session}`;

describe("the needs-you queue", () => {
  it("says nothing needs you when nothing asked and every chat can say so", () => {
    render(<NeedsYou queue={[]} quiet={[]} nameOf={nameOf} show={() => {}} />);

    expect(screen.getByLabelText("Needs you")).toHaveTextContent(/^Nothing needs you$/);
  });

  it("names a chat that can be waiting on you without saying so, beside an empty queue", () => {
    // #27. A Codex chat stopped mid-turn for an approval says nothing. "Nothing needs you"
    // alone, over the top of it, would be the app claiming something it cannot see — M1.3's
    // rule that the honest half is named, never folded into the reassuring answer.
    render(<NeedsYou queue={[]} quiet={["ide.7"]} nameOf={nameOf} show={() => {}} />);

    const queue = screen.getByLabelText("Needs you");
    expect(queue).toHaveTextContent("ide.7 can be waiting on you without saying so.");
  });

  it("does not claim nothing needs you while a chat cannot say whether it does", () => {
    // #52. The headline is a claim about every chat on the plane, and the hedge under it
    // used to contradict it in the same breath. What charter knows is that nothing has
    // SAID so, and that is all the empty state is allowed to say.
    render(<NeedsYou queue={[]} quiet={["ide.7"]} nameOf={nameOf} show={() => {}} />);

    const queue = screen.getByLabelText("Needs you");
    expect(queue).toHaveTextContent("Nothing has said it needs you");
    expect(queue.textContent).not.toContain("Nothing needs you");
  });

  it("counts several such chats rather than listing them all", () => {
    render(<NeedsYou queue={[]} quiet={["ide.7", "ide.8"]} nameOf={nameOf} show={() => {}} />);

    expect(screen.getByLabelText("Needs you")).toHaveTextContent(
      "2 chats can be waiting on you without saying so.",
    );
  });

  it("still names them when the queue is not empty", () => {
    render(<NeedsYou queue={[3]} quiet={["ide.7"]} nameOf={nameOf} show={() => {}} />);

    const queue = screen.getByLabelText("Needs you");
    expect(queue).toHaveTextContent("1 need you");
    expect(screen.getByRole("button", { name: "ide.3" })).toBeInTheDocument();
    expect(queue).toHaveTextContent("ide.7 can be waiting on you without saying so.");
  });
});
