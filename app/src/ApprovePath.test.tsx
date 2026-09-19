import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
});

const SIDEBAR = {
  root: "/home/dev/plane",
  workspaces: [],
  personas: ["steward", "release"],
  persona: "steward",
  unfiled: [],
};

/** A profile that has NOT been approved, plus two personas and a plane default. */
const START_OPTIONS = {
  profiles: [
    {
      name: "work",
      kind: "claude",
      shown: "CLAUDE_CONFIG_DIR=~/.claude-work claude",
      source: "charter.local.toml",
      is_default: true,
      approval: "new",
    },
  ],
  refused: [],
  personas: ["steward", "release"],
  persona: "steward",
  ignore_fix: null,
  declares_none: false,
};

function core(options: typeof START_OPTIONS = START_OPTIONS) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  let opened = 0;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    if (cmd === "plane_root") return "/home/dev/plane";
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "running_sessions") return [];
    if (cmd === "opened_chats") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "start_options") return options;
    if (cmd === "approve_profile") return null;
    if (cmd === "start_chat") return { session: ++opened, wired: null };
    return null;
  });
  return { asked };
}

describe("approving a profile and starting on it", () => {
  // Both of these were RED.  passed the plane's default persona out of
  // the options instead of the one the operator had picked in the dialog, so the choice was
  // thrown away silently — on the one path where a profile is being used for the first
  // time, which is every profile's first use.
  // Both of these were RED. `approveAndStart` passed the plane's default persona out of the
  // options instead of the one the operator had picked in the dialog, so the choice was
  // thrown away silently — on the one path where a profile is being used for the first
  // time, which is every profile's first use.
  it("carries the persona the operator picked, not the plane default", async () => {
    const { asked } = core();
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
    // Pick a persona that is NOT the plane default.
    await userEvent.click(await screen.findByRole("radio", { name: /release/ }));
    await userEvent.click(screen.getByRole("button", { name: "Approve and start" }));

    const started = await vi.waitFor(() => {
      const one = asked.find(({ cmd }) => cmd === "start_chat");
      if (one === undefined) throw new Error("no chat was started");
      return one;
    });
    expect(started.args).toMatchObject({ profile: "work", persona: "release" });
  });

  it("carries 'no persona at all' when the operator picked none", async () => {
    const { asked } = core();
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
    await userEvent.click(await screen.findByRole("radio", { name: /none/ }));
    await userEvent.click(screen.getByRole("button", { name: "Approve and start" }));

    const started = await vi.waitFor(() => {
      const one = asked.find(({ cmd }) => cmd === "start_chat");
      if (one === undefined) throw new Error("no chat was started");
      return one;
    });
    expect(started.args).toMatchObject({ profile: "work", persona: null });
  });
  it("approves the line the operator read, not whatever the file says when they click", async () => {
    // `charter.local.toml` is gitignored, so an edit to it leaves no diff for a reviewer to
    // catch and nothing stops a chat writing plane config. The approval therefore carries
    // the exact line the dialog DREW, and the core checks it against the file again — so an
    // operator cannot approve, and charter cannot run, a command they never read.
    const { asked } = core();
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
    await userEvent.click(screen.getByRole("button", { name: "Approve and start" }));

    const approved = await vi.waitFor(() => {
      const one = asked.find(({ cmd }) => cmd === "approve_profile");
      if (one === undefined) throw new Error("nothing was approved");
      return one;
    });
    expect(approved.args).toMatchObject({
      name: "work",
      shown: "CLAUDE_CONFIG_DIR=~/.claude-work claude",
    });
  });
});
