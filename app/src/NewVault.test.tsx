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
    const { dialog } = draw({ trouble: "'../x' is not a vault name purlis accepts" });
    expect(within(dialog).getByRole("alert")).toHaveTextContent("not a vault name");
  });

  it("is drawn from the settings set, each answer tied to its line of help", async () => {
    // DS-3c (#1175): the dialog's form parts are the house set's rows, fields and choices, not
    // the hand-built `asks` / `choice` classes, so the help under a box is the box's own
    // description rather than a paragraph a screen reader cannot connect to it.
    const { dialog } = draw();
    expect(dialog.querySelector(".asks, .choices, .choice, .who")).toBeNull();

    const name = within(dialog).getByLabelText("Name");
    expect(name.closest(".ui-setting-row")).not.toBeNull();
    expect(name).toHaveAccessibleDescription("Letters, digits, ., _ and -.");

    const kept = within(dialog).getByRole("radiogroup", { name: "Kept in" });
    expect(kept.closest(".ui-setting-row")).not.toBeNull();
    expect(within(dialog).getByRole("radio", { name: "Plain file" })).toHaveAccessibleDescription(
      "A plaintext file under the plane's state directory, which git never sees.",
    );

    await userEvent.click(within(dialog).getByRole("radio", { name: /1Password/ }));
    expect(within(dialog).getByLabelText("1Password vault")).toHaveAccessibleDescription(
      "Where purlis creates this vault's items.",
    );
  });

  it("puts the keyboard in the name box, and Escape makes nothing", async () => {
    const { create, cancel, dialog } = draw();
    expect(within(dialog).getByLabelText("Name")).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(cancel).toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
  });
});
