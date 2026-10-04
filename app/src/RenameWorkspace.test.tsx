import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { RenameWorkspace } from "./RenameWorkspace";

afterEach(cleanup);

function show() {
  const onRename = vi.fn();
  render(
    <RenameWorkspace
      workspace="alpha"
      startsFresh={null}
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
});
