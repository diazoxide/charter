import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { LinkWorkItem } from "./LinkWorkItem";

afterEach(cleanup);

/** A control that had the focus, and the dialog it opens, as a tab and its menu do. */
function Opener() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)}>the tab</button>
      {open && (
        <LinkWorkItem
          chat="steward 1"
          linking={false}
          onLink={() => undefined}
          onCancel={() => setOpen(false)}
        />
      )}
    </>
  );
}

describe("Link to work item", () => {
  it("gives the focus back to what had it when it is cancelled", async () => {
    render(<Opener />);
    const tab = screen.getByRole("button", { name: "the tab" });
    tab.focus();
    await userEvent.keyboard("{Enter}");
    await screen.findByRole("dialog", { name: "Link to work item" });
    expect(screen.getByLabelText("Tracker key")).toHaveFocus();

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(tab).toHaveFocus();
  });
});
