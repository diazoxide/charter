import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { DeleteVault, type VaultHolds } from "./DeleteVault";

afterEach(cleanup);

function draw(
  over: {
    holds?: VaultHolds;
    unreadable?: string;
    trouble?: string;
    deleting?: boolean;
  } = {},
) {
  const del = vi.fn();
  const cancel = vi.fn();
  render(
    <DeleteVault
      vault="ops"
      holds={
        "holds" in over ? over.holds : { provider: "keyring", secrets: ["API_TOKEN", "DB_URL"] }
      }
      unreadable={over.unreadable}
      trouble={over.trouble}
      deleting={over.deleting ?? false}
      onDelete={del}
      onCancel={cancel}
    />,
  );
  const dialog = screen.getByRole("alertdialog", { name: "Delete vault ops?" });
  return {
    del,
    cancel,
    dialog,
    button: within(dialog).getByRole("button", { name: "Delete vault" }),
    box: within(dialog).getByLabelText("Type ops to confirm"),
  };
}

describe("the delete-vault dialog", () => {
  it("deletes nothing until the vault's own name is typed back", async () => {
    const { del, button, box } = draw();
    expect(button).toBeDisabled();

    await userEvent.type(box, "op{Enter}");
    expect(button).toBeDisabled();
    await userEvent.type(box, "s {Enter}");
    expect(button).toBeDisabled();
    expect(del).not.toHaveBeenCalled();

    await userEvent.keyboard("{Backspace}");
    expect(button).toBeEnabled();
    await userEvent.click(button);
    expect(del).toHaveBeenCalledTimes(1);
  });

  it("names every secret it holds and says plainly they are destroyed for good", () => {
    const { dialog } = draw();
    const held = within(dialog).getByRole("list", { name: "Secrets in ops" });
    expect(
      within(held)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["API_TOKEN", "DB_URL"]);
    expect(dialog).toHaveTextContent(
      "Its 2 secrets are destroyed in your system keychain and cannot be recovered.",
    );
  });

  it("does not claim to destroy what a file vault keeps on disk", () => {
    const { dialog } = draw({ holds: { provider: "plain-file", secrets: ["K"] } });
    expect(dialog).not.toHaveTextContent("destroyed");
    expect(dialog).toHaveTextContent("Its file is left on disk");
  });

  it("says it is still reading rather than that the vault is empty", () => {
    const { dialog } = draw({ holds: undefined });
    expect(dialog).toHaveTextContent("Reading what it holds…");
    expect(within(dialog).queryByRole("list")).not.toBeInTheDocument();
  });

  it("says the core's refusal and keeps the answer the operator gave", async () => {
    const { dialog, box, button } = draw({ trouble: "charter could not delete 'x': locked" });
    expect(within(dialog).getByRole("alert")).toHaveTextContent("locked");
    await userEvent.type(box, "ops");
    expect(button).toBeEnabled();
  });

  it("cannot be answered twice while the delete runs", async () => {
    const { box, button } = draw({ deleting: true });
    await userEvent.type(box, "ops");
    expect(button).toBeDisabled();
  });

  it("puts the keyboard in the name box, and Escape deletes nothing", async () => {
    const { del, cancel, box } = draw();
    expect(box).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(cancel).toHaveBeenCalled();
    expect(del).not.toHaveBeenCalled();
  });
});
