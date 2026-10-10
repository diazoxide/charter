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

  it("gives a GitHub key and a GitLab key as its examples", async () => {
    render(<Opener />);
    await userEvent.click(screen.getByRole("button", { name: "the tab" }));
    const key = await screen.findByLabelText("Tracker key");
    expect(key).toHaveAccessibleDescription(
      expect.stringContaining("github:github.com/owner/repo#12"),
    );
    expect(key).toHaveAccessibleDescription(
      expect.stringContaining("gitlab:gitlab.com/group/repo#12"),
    );
  });
});

describe("Link to work item's words (#630)", () => {
  const dialog = (linking: boolean) =>
    render(
      <LinkWorkItem
        chat="steward 1"
        linking={linking}
        onLink={() => undefined}
        onCancel={() => undefined}
      />,
    );

  it("names its act with what it acts on, and says when it is at work", () => {
    dialog(false);
    expect(screen.getByRole("button", { name: "Link work item" })).toBeEnabled();
    cleanup();
    dialog(true);
    expect(screen.getByRole("button", { name: "Linking…" })).toBeDisabled();
  });

  it("says where the link goes without naming the store underneath", () => {
    dialog(false);
    expect(screen.queryByText(/work link log/)).not.toBeInTheDocument();
    expect(screen.getByText(/your other devices see it/)).toBeInTheDocument();
  });

  it("is described by the chat the link is for (#1719)", () => {
    render(
      <LinkWorkItem
        chat="steward 1"
        linking={false}
        onLink={() => undefined}
        onCancel={() => undefined}
      />,
    );

    expect(screen.getByRole("dialog", { name: "Link to work item" })).toHaveAccessibleDescription(
      "For steward 1",
    );
  });
});
