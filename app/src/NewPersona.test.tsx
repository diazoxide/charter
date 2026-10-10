import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { userEvent } from "@testing-library/user-event";
import { NewPersona } from "./NewPersona";

afterEach(() => {
  cleanup();
  clearMocks();
});

/** The project's personas, as the sidebar answers them: what Inherits from offers. */
function personas(names: string[] | Error) {
  mockIPC((cmd) => {
    if (cmd !== "plane_sidebar") return null;
    if (names instanceof Error) throw names.message;
    return { root: "/home/dev/plane", workspaces: [], personas: names, persona: null, unfiled: [] };
  });
}

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
    // While it runs the act says so (#630).
    button: within(dialog).getByRole("button", {
      name: over.making ? "Creating…" : "Create persona",
    }),
  };
}

describe("the new-persona dialog", () => {
  it("asks for what purlis persona create takes, and passes an empty box as not given", async () => {
    const { create, dialog, button } = draw();
    await userEvent.type(within(dialog).getByLabelText("Name"), "devops");
    expect(button).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText("Delegate when"), "CI/CD, k8s deploys");
    await userEvent.click(button);

    expect(create).toHaveBeenCalledWith("devops", null, "CI/CD, k8s deploys", null);
  });

  it("passes the role and the parent picked from the project's personas", async () => {
    personas(["steward", "devops"]);
    const { create, dialog, button } = draw();
    await userEvent.type(within(dialog).getByLabelText("Name"), "qa");
    await userEvent.type(within(dialog).getByLabelText("Role"), "QA Engineer");
    // A pick of the project's personas, so a typo is refused before the core sees it (#1719).
    const inherits = within(dialog).getByRole("combobox", { name: "Inherits from" });
    await within(dialog).findByRole("option", { name: "steward" });
    await userEvent.selectOptions(inherits, "steward");
    // A parent carries a routing line of its own, so none is required with one.
    expect(button).toBeEnabled();
    await userEvent.click(button);

    expect(create).toHaveBeenCalledWith("qa", "QA Engineer", null, "steward");
  });

  it("says where the persona is made, committed with the project (#1192)", () => {
    const { dialog } = draw();
    expect(within(dialog).getByText(/committed with the project/)).toHaveTextContent(
      "in /home/dev/plane/personas/, committed with the project",
    );
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

  it("says why Create waits, under the act, once a name is typed (#1719)", async () => {
    personas([]);
    const { dialog } = draw();
    expect(within(dialog).queryByText(/Create persona needs/)).toBeNull();

    await userEvent.type(within(dialog).getByLabelText("Name"), "qa");

    expect(
      within(dialog).getByText(
        "Create persona needs Delegate when, or a persona it inherits from.",
      ),
    ).toBeInTheDocument();
  });

  it("says when the project's personas could not be read, and still makes one (#1719)", async () => {
    personas(new Error("the project is gone"));
    const { dialog } = draw();

    expect(
      await within(dialog).findByText(
        "purlis could not read the project's personas: the project is gone",
      ),
    ).toBeInTheDocument();
  });
});
