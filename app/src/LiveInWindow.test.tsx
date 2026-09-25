import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { WorkspaceSettings } from "./ProjectSettings";

/**
 * LIVE and LOCAL, from the window (charter-app#301): a LIVE workspace is marked where it is
 * drawn, its menu asks before switching, and the workspace settings page offers the same.
 * What the confirmation says is `LiveDialog.test.tsx`'s.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

function workspace(name: string, live: boolean) {
  return {
    name,
    path: `${PLANE}/workspaces/${name}`,
    vision: "",
    todos: [],
    chats: [],
    colour: null,
    live,
  };
}

function core() {
  const asked: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: [],
        persona: null,
        unfiled: [],
        workspaces: [workspace("alpha", true), workspace("beta", false)],
      };
    if (cmd === "workspace_live_preview")
      return { live: false, files: ["workspaces/beta/workspace.md"], remote: null, mode: "commit" };
    return null;
  });
  return asked;
}

describe("LIVE and LOCAL, in the window", () => {
  it("marks a LIVE workspace's tab and leaves a LOCAL one unmarked", async () => {
    core();
    render(<App />);
    const strip = await screen.findByRole("tablist", { name: "Workspaces" });
    const alpha = await within(strip).findByRole("tab", { name: /alpha/ });
    const beta = within(strip).getByRole("tab", { name: /beta/ });
    expect(within(alpha).getByRole("img", { name: "live" })).toBeTruthy();
    expect(within(beta).queryByRole("img", { name: "live" })).toBeNull();
  });

  it("asks before making a workspace live, from the workspace's own menu", async () => {
    const asked = core();
    render(<App />);
    const strip = await screen.findByRole("tablist", { name: "Workspaces" });
    fireEvent.contextMenu(await within(strip).findByRole("tab", { name: /beta/ }));
    const menu = await screen.findByRole("menu");
    await userEvent.click(within(menu).getByRole("menuitem", { name: /Make beta live/ }));

    expect(await screen.findByRole("alertdialog", { name: "Make beta live?" })).toBeTruthy();
    expect(asked.some((a) => a.cmd === "workspace_live")).toBe(false);
  });
});

describe("the workspace settings page", () => {
  it("offers the same switch, through the same confirmation", async () => {
    mockIPC((cmd) => {
      if (cmd === "workspace_settings")
        return {
          workspace: "beta",
          file: "workspaces/beta/workspace.json",
          exists: true,
          text: "{}",
          refusals: [],
          fields: [],
          live: false,
          parsed: true,
        };
      if (cmd === "workspace_live_preview")
        return { live: false, files: [], remote: null, mode: "push" };
      return null;
    });
    render(<WorkspaceSettings plane={PLANE} workspace="beta" />);

    await userEvent.click(await screen.findByRole("button", { name: "Make live…" }));

    await waitFor(() =>
      expect(screen.getByRole("alertdialog", { name: "Make beta live?" })).toBeTruthy(),
    );
  });
});
