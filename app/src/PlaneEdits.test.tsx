import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { usePlaneEdits } from "./PlaneEdits";
import type { VaultContents } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

const OPS: VaultContents = {
  name: "ops",
  provider: "keyring",
  count: 1,
  health: { ok: true, detail: "" },
  secrets: [{ key: "API_TOKEN", size: null, updated: null }],
  identity: [],
  identity_in_app_env: [],
};

type Asked = { cmd: string; args: Record<string, unknown> };

/** The core: each command answers what `answers` says, or `null`; an `Error` is a refusal. */
function core(answers: Record<string, unknown>): Asked[] {
  const asked: Asked[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: args as Record<string, unknown> });
    const answer = answers[cmd];
    if (answer instanceof Error) throw answer.message;
    return answer ?? null;
  });
  return asked;
}

/** The hook in a window of its own, with the four things it asks of the window recorded. */
function mount() {
  const window = {
    showView: vi.fn(),
    closeView: vi.fn(),
    reread: vi.fn(),
    reloadVaults: vi.fn(),
  };
  const held: { edits?: ReturnType<typeof usePlaneEdits> } = {};
  function Window() {
    const edits = usePlaneEdits({ plane: PLANE, ...window });
    held.edits = edits;
    return <>{edits.dialogs}</>;
  }
  render(<Window />);
  const edits = () => {
    if (held.edits === undefined) throw new Error("the window is not drawn");
    return held.edits;
  };
  return { window, edits };
}

describe("deleting a vault from the window", () => {
  it("lists what it holds, deletes only once its name is typed, then closes its tab", async () => {
    const asked = core({
      vault_open: OPS,
      vault_remove: { name: "ops", provider: "keyring", destroyed: ["API_TOKEN"] },
    });
    const { window, edits } = mount();

    act(() => edits().doing.removeVault("ops"));
    const dialog = await screen.findByRole("alertdialog", { name: "Delete vault ops?" });
    await waitFor(() => expect(within(dialog).getByText("API_TOKEN")).toBeInTheDocument());
    expect(asked.map((one) => one.cmd)).not.toContain("vault_remove");

    await userEvent.type(within(dialog).getByLabelText("Type ops to confirm"), "ops{Enter}");

    await waitFor(() =>
      expect(screen.queryByRole("alertdialog", { name: "Delete vault ops?" })).toBeNull(),
    );
    expect(asked.find((one) => one.cmd === "vault_remove")?.args).toEqual({
      plane: PLANE,
      vault: "ops",
    });
    expect(window.closeView).toHaveBeenCalledWith({ from: null, view: "vault", key: "ops" });
    expect(window.reloadVaults).toHaveBeenCalled();
  });

  it("keeps the dialog up with the core's refusal when the keyring will not delete", async () => {
    core({ vault_open: OPS, vault_remove: new Error("the keyring is locked") });
    const { window, edits } = mount();

    act(() => edits().doing.removeVault("ops"));
    const dialog = await screen.findByRole("alertdialog", { name: "Delete vault ops?" });
    await userEvent.type(within(dialog).getByLabelText("Type ops to confirm"), "ops{Enter}");

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("the keyring is locked");
    expect(window.closeView).not.toHaveBeenCalled();
  });
});

describe("a todo written from the window", () => {
  it("goes to the workspace it names, and the panels are read again", async () => {
    const asked = core({ todo_add: "Todo recorded in 'beta'." });
    const { window, edits } = mount();

    const refused = await edits().addTodo("beta", "Cut the release");

    expect(refused).toBeUndefined();
    expect(asked.find((one) => one.cmd === "todo_add")?.args).toEqual({
      plane: PLANE,
      workspace: "beta",
      text: "Cut the release",
    });
    expect(window.reread).toHaveBeenCalled();
  });

  it("hands back the core's refusal and reads nothing again", async () => {
    core({ todo_add: new Error("already on the list: Cut the release") });
    const { window, edits } = mount();

    expect(await edits().addTodo("beta", "cut the release")).toBe(
      "already on the list: Cut the release",
    );
    expect(window.reread).not.toHaveBeenCalled();
  });
});

describe("a persona made from the window", () => {
  it("opens its tab once the core has made it", async () => {
    const asked = core({ persona_create: ["✓ Created persona 'qa'"] });
    const { window, edits } = mount();

    act(() => edits().doing.createPersona());
    const dialog = await screen.findByRole("dialog", { name: "New persona" });
    await userEvent.type(within(dialog).getByLabelText("Name"), "qa");
    await userEvent.type(within(dialog).getByLabelText("Delegate when"), "tests{Enter}");

    await waitFor(() =>
      expect(window.showView).toHaveBeenCalledWith(
        { from: null, view: "persona", key: "qa" },
        "qa",
      ),
    );
    expect(asked.find((one) => one.cmd === "persona_create")?.args).toEqual({
      plane: PLANE,
      name: "qa",
      role: null,
      delegateWhen: "tests",
      parent: null,
    });
  });
});
