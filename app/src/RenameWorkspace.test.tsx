import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { RenameWorkspace } from "./RenameWorkspace";

afterEach(cleanup);

// `undefined` is the core still being asked, so it is passed as it is and never defaulted.
function show({ startsFresh }: { startsFresh: string | null | undefined } = { startsFresh: null }) {
  const onRename = vi.fn();
  render(
    <RenameWorkspace
      workspace="alpha"
      startsFresh={startsFresh}
      renaming={false}
      onRename={onRename}
      onCancel={vi.fn()}
    />,
  );
  return { onRename };
}

describe("renaming a workspace", () => {
  it("draws its one box as a setting is drawn (DS-3d)", () => {
    show();

    const dialog = screen.getByRole("dialog", { name: "Rename workspace alpha" });
    expect(dialog.querySelector(".ui-setting-row")).not.toBeNull();
    expect(dialog.querySelector(".asks")).toBeNull();
    expect(screen.getByLabelText("New name")).toHaveAccessibleDescription(
      /Its folder under workspaces\/ moves/,
    );
  });

  it("opens with the name selected, so typing replaces it", async () => {
    const { onRename } = show();

    expect(screen.getByLabelText("New name")).toHaveFocus();
    await userEvent.keyboard("beta{Enter}");

    expect(onRename).toHaveBeenCalledWith("beta");
  });

  it("says it is asking which chats start fresh while Rename waits for that answer", () => {
    show({ startsFresh: undefined });

    expect(screen.getByText("Asking which chats would start fresh…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Rename workspace" })).toBeDisabled();
  });

  it("drops the asking line once the core has answered", () => {
    show({ startsFresh: null });

    expect(screen.queryByText("Asking which chats would start fresh…")).toBeNull();
  });
});
