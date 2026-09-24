import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { LiveDialog } from "./LiveDialog";
import type { LivePreview } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

function preview(over: Partial<LivePreview> = {}): LivePreview {
  return {
    live: false,
    files: ["workspaces/ide/workspace.md", "workspaces/ide/memory", "workspaces/ide/todos"],
    remote: "https://github.com/acme/plane.git",
    mode: "push",
    ...over,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

function core(read: LivePreview, switched: string[] | Error = ["✓ Workspace 'ide' is now LIVE"]) {
  const asked: Asked[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: args as Record<string, unknown> });
    if (cmd === "workspace_live_preview") return read;
    if (cmd === "workspace_live") {
      if (switched instanceof Error) throw switched.message;
      return switched;
    }
    return null;
  });
  return asked;
}

describe("LiveDialog", () => {
  it("says what making a workspace live publishes and where, before anything happens", async () => {
    const asked = core(preview());
    render(<LiveDialog plane={PLANE} workspace="ide" onClose={() => {}} onDone={() => {}} />);

    expect(await screen.findByRole("alertdialog", { name: "Make ide live?" })).toBeTruthy();
    expect(await screen.findByText("workspaces/ide/workspace.md")).toBeTruthy();
    expect(
      screen.getByText(
        "The next save pushes them to https://github.com/acme/plane.git — anyone who can read that repository will read them.",
      ),
    ).toBeTruthy();
    expect(asked.some((a) => a.cmd === "workspace_live")).toBe(false);
  });

  it("switches and saves when the operator says yes", async () => {
    const asked = core(preview());
    const onDone = vi.fn();
    render(<LiveDialog plane={PLANE} workspace="ide" onClose={() => {}} onDone={onDone} />);

    await userEvent.click(await screen.findByRole("button", { name: "Make live" }));

    await waitFor(() => expect(onDone).toHaveBeenCalledTimes(1));
    expect(asked.find((a) => a.cmd === "workspace_live")?.args).toEqual({
      plane: PLANE,
      name: "ide",
      live: true,
    });
  });

  it("making a live workspace local says pushed history stays where it is", async () => {
    core(preview({ live: true }));
    render(<LiveDialog plane={PLANE} workspace="ide" onClose={() => {}} onDone={() => {}} />);

    expect(await screen.findByRole("alertdialog", { name: "Make ide local?" })).toBeTruthy();
    expect(
      await screen.findByText(
        "It stops publishing them from now on. What was already pushed stays in the repository's history.",
      ),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Make local" })).toBeTruthy();
  });

  it("says a plane with no remote only commits, and one whose mode is off commits nothing", async () => {
    core(preview({ remote: null }));
    render(<LiveDialog plane={PLANE} workspace="ide" onClose={() => {}} onDone={() => {}} />);
    expect(
      await screen.findByText(
        "This plane has no remote charter can push to, so they are committed on this machine only.",
      ),
    ).toBeTruthy();
    cleanup();

    core(preview({ mode: "off" }));
    render(<LiveDialog plane={PLANE} workspace="ide" onClose={() => {}} onDone={() => {}} />);
    expect(
      await screen.findByText(
        "This plane's mode is off, so charter commits nothing; they are published when you commit and push them.",
      ),
    ).toBeTruthy();
  });

  it("shows a refusal in the core's words and stays open", async () => {
    core(preview(), new Error("no workspace 'ide' (create it: charter workspace create ide)"));
    const onDone = vi.fn();
    render(<LiveDialog plane={PLANE} workspace="ide" onClose={() => {}} onDone={onDone} />);

    await userEvent.click(await screen.findByRole("button", { name: "Make live" }));

    expect((await screen.findByRole("alert")).textContent).toContain("no workspace 'ide'");
    expect(onDone).not.toHaveBeenCalled();
  });
});
