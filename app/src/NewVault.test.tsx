import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { NewVault } from "./NewVault";

afterEach(cleanup);

function draw(over: { trouble?: string; making?: boolean } = {}) {
  const create = vi.fn();
  const cancel = vi.fn();
  render(
    <NewVault
      plane="/home/dev/plane"
      trouble={over.trouble}
      making={over.making ?? false}
      onCreate={create}
      onCancel={cancel}
    />,
  );
  return { create, cancel, dialog: screen.getByRole("dialog", { name: "New vault" }) };
}

describe("the new-vault dialog", () => {
  it("makes a keyring vault unless another provider is chosen", async () => {
    const { create, dialog } = draw();
    expect(within(dialog).getByRole("radio", { name: /System keychain/ })).toBeChecked();

    await userEvent.type(within(dialog).getByLabelText("Name"), "ops");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create vault" }));

    expect(create).toHaveBeenCalledWith("ops", "keyring", null);
  });

  it("asks which 1Password vault a 1Password vault keeps its items in", async () => {
    const { create, dialog } = draw();
    await userEvent.type(within(dialog).getByLabelText("Name"), "team");
    expect(within(dialog).queryByLabelText("1Password vault")).not.toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole("radio", { name: /1Password/ }));
    const create_ = within(dialog).getByRole("button", { name: "Create vault" });
    expect(create_).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText("1Password vault"), "Engineering");
    await userEvent.click(create_);

    expect(create).toHaveBeenCalledWith("team", "1password", "Engineering");
  });

  it("says the core's refusal where the name is typed", () => {
    const { dialog } = draw({ trouble: "'../x' is not a vault name charter accepts" });
    expect(within(dialog).getByRole("alert")).toHaveTextContent("not a vault name");
  });

  it("puts the keyboard in the name box, and Escape makes nothing", async () => {
    const { create, cancel, dialog } = draw();
    expect(within(dialog).getByLabelText("Name")).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(cancel).toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
  });
});
