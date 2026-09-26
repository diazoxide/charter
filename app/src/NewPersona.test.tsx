import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { NewPersona } from "./NewPersona";

afterEach(cleanup);

function draw(over: { trouble?: string; making?: boolean } = {}) {
  const create = vi.fn();
  const cancel = vi.fn();
  render(
    <NewPersona
      plane="/home/dev/plane"
      trouble={over.trouble}
      making={over.making ?? false}
      onCreate={create}
      onCancel={cancel}
    />,
  );
  const dialog = screen.getByRole("dialog", { name: "New persona" });
  return {
    create,
    cancel,
    dialog,
    button: within(dialog).getByRole("button", { name: "Create persona" }),
  };
}

describe("the new-persona dialog", () => {
  it("asks for what charter persona create takes, and passes an empty box as not given", async () => {
    const { create, dialog, button } = draw();
    await userEvent.type(within(dialog).getByLabelText("Name"), "devops");
    expect(button).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText("Delegate when"), "CI/CD, k8s deploys");
    await userEvent.click(button);

    expect(create).toHaveBeenCalledWith("devops", null, "CI/CD, k8s deploys", null);
  });

  it("passes the role and the parent when they are typed", async () => {
    const { create, dialog, button } = draw();
    await userEvent.type(within(dialog).getByLabelText("Name"), "qa");
    await userEvent.type(within(dialog).getByLabelText("Role"), "QA Engineer");
    await userEvent.type(within(dialog).getByLabelText("Inherits from"), "steward");
    // A parent carries a routing line of its own, so none is required with one.
    expect(button).toBeEnabled();
    await userEvent.click(button);

    expect(create).toHaveBeenCalledWith("qa", "QA Engineer", null, "steward");
  });

  it("says the core's refusal in the dialog", () => {
    const { dialog } = draw({ trouble: "persona 'qa' already exists (personas/qa/persona.md)." });
    expect(within(dialog).getByRole("alert")).toHaveTextContent("already exists");
  });

  it("cannot be answered twice while charter is making it", async () => {
    const { dialog, button } = draw({ making: true });
    await userEvent.type(within(dialog).getByLabelText("Name"), "qa");
    await userEvent.type(within(dialog).getByLabelText("Delegate when"), "tests");
    expect(button).toBeDisabled();
  });

  it("puts the keyboard in the name box, and Escape makes nothing", async () => {
    const { create, cancel, dialog } = draw();
    expect(within(dialog).getByLabelText("Name")).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(cancel).toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
  });
});
