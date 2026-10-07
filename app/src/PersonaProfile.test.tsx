import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import type { PersonaProfile as Read } from "./bindings";
import { PersonaProfile } from "./PersonaProfile";

afterEach(cleanup);

const READ: Read = {
  named: "codex",
  inherited_from: null,
  offered: ["claude", "opencode", "codex", "work"],
};

function draw(over: { read?: Read | null; unreadable?: string; trouble?: string } = {}) {
  const save = vi.fn();
  const cancel = vi.fn();
  render(
    <PersonaProfile
      persona="ops"
      read={over.read === null ? undefined : (over.read ?? READ)}
      unreadable={over.unreadable}
      trouble={over.trouble}
      saving={false}
      onSave={save}
      onCancel={cancel}
    />,
  );
  const dialog = screen.getByRole("dialog", { name: "Profile of ops" });
  return { save, cancel, dialog, user: userEvent.setup() };
}

describe("a persona's profile, set from its view (#1445)", () => {
  it("shows the profile the definition names among the project's profiles", () => {
    const { dialog } = draw();

    expect(within(dialog).getByRole("radio", { name: "codex" })).toBeChecked();
    expect(
      within(dialog)
        .getAllByRole("radio")
        .map((radio) => radio.getAttribute("value") ?? radio.textContent),
    ).toHaveLength(5);
    expect(dialog).toHaveTextContent("personas/ops/persona.md");
  });

  it("saves the profile the person picks", async () => {
    const { save, dialog, user } = draw();

    await user.click(within(dialog).getByRole("radio", { name: "work" }));
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(save).toHaveBeenCalledWith("work");
  });

  it("saves none as no profile at all", async () => {
    const { save, dialog, user } = draw();

    await user.click(within(dialog).getByRole("radio", { name: "none" }));
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(save).toHaveBeenCalledWith(null);
  });

  it("says whose profile an inherited one is, and none still saves as none", async () => {
    // M2: the core writes `profile: none` for a child, so "none" is honest here too.
    const { save, dialog, user } = draw({ read: { ...READ, inherited_from: "base" } });

    expect(dialog).toHaveTextContent("Inherited from base, which this persona extends.");
    expect(within(dialog).getByRole("radio", { name: "codex" })).toBeChecked();
    await user.click(within(dialog).getByRole("radio", { name: "none" }));
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(save).toHaveBeenCalledWith(null);
  });

  it("says nothing of inheriting for a profile that is the persona's own", () => {
    const { dialog } = draw();

    expect(dialog).not.toHaveTextContent("Inherited from");
  });

  it("starts on none for a persona that names no profile", () => {
    const { dialog } = draw({ read: { ...READ, named: null } });

    expect(within(dialog).getByRole("radio", { name: "none" })).toBeChecked();
  });

  it("says so for a name the project does not offer, and offers no way to keep it", async () => {
    const { save, dialog, user } = draw({ read: { ...READ, named: "sh -c evil" } });

    expect(dialog).toHaveTextContent("which this project does not offer");
    expect(within(dialog).queryByRole("radio", { name: "sh -c evil" })).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Save" })).toBeDisabled();

    await user.click(within(dialog).getByRole("radio", { name: "claude" }));
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(save).toHaveBeenCalledWith("claude");
  });

  it("draws the core's refusal and keeps Cancel under the keyboard", async () => {
    const { cancel, dialog } = draw({ trouble: "this project does not offer it" });

    expect(within(dialog).getByRole("alert")).toHaveTextContent("this project does not offer it");
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(cancel).toHaveBeenCalled();
  });

  it("says why it could not be read, with nothing to save", () => {
    const { dialog } = draw({ read: null, unreadable: "no persona 'ops' on this plane" });

    expect(within(dialog).getByRole("alert")).toHaveTextContent("no persona 'ops'");
    expect(within(dialog).queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
  });
});
