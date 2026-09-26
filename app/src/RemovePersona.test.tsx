import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { RemovePersona } from "./RemovePersona";

afterEach(cleanup);

function draw(over: { trouble?: string; deleting?: boolean } = {}) {
  const del = vi.fn();
  const cancel = vi.fn();
  render(
    <RemovePersona
      persona="qa"
      trouble={over.trouble}
      deleting={over.deleting ?? false}
      onDelete={del}
      onCancel={cancel}
    />,
  );
  const dialog = screen.getByRole("alertdialog", { name: "Delete persona qa?" });
  return {
    del,
    cancel,
    dialog,
    button: within(dialog).getByRole("button", { name: "Delete persona" }),
  };
}

describe("the delete-persona dialog", () => {
  it("says what goes with it and what stays", () => {
    const { dialog } = draw();
    expect(dialog).toHaveTextContent("personas/qa/");
    expect(dialog).toHaveTextContent("its definition, its memory and its refs");
    expect(dialog).toHaveTextContent("Its vault is left alone");
  });

  it("deletes on the press and not before, with Cancel under the keyboard", async () => {
    const { del, cancel, dialog, button } = draw();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(del).not.toHaveBeenCalled();
    expect(cancel).toHaveBeenCalled();

    await userEvent.click(button);
    expect(del).toHaveBeenCalledTimes(1);
  });

  it("says the core's refusal, naming who still depends on it", () => {
    const { dialog } = draw({
      trouble: "Refusing to remove 'qa' — it is still referenced by:\n  child (extends)",
    });
    expect(within(dialog).getByRole("alert")).toHaveTextContent("child (extends)");
  });

  it("cannot be answered twice while the delete runs", () => {
    const { button } = draw({ deleting: true });
    expect(button).toBeDisabled();
  });
});
