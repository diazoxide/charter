import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { Curating, Curations } from "./bindings";

/**
 * A curation action, chosen in the window, opens a chat (ADR 0061).
 *
 * The core opens it — the default profile, the action's runner, where the action runs — and
 * types its prompt when the harness reports its start, which is `curation.rs`'s to test. The
 * window's part is to ask for exactly that action on exactly that subject, and to put the tab
 * the core answers with on the strip it is filed under, in front, named for the action and its
 * subject.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const SIDEBAR = {
  root: "/home/dev/plane",
  workspaces: [
    {
      name: "alpha",
      path: "/home/dev/plane/workspaces/alpha",
      vision: "Ship it",
      todos: [],
      chats: [],
    },
  ],
  personas: ["steward", "ops"],
  persona: "steward",
  unfiled: [],
};

const CURATIONS: Curations = {
  subjects: [
    {
      subject: "workspace:alpha",
      name: "alpha",
      actions: [
        {
          id: "charter/safe-remove",
          label: "Safe remove",
          declared_by: null,
          runner: "steward",
          cwd: "/home/dev/plane",
          prompt: "Retire alpha.",
        },
        {
          id: "ops/tidy",
          label: "Tidy",
          declared_by: "ops",
          runner: "ops",
          cwd: "/home/dev/plane/workspaces/alpha",
          prompt: "Tidy alpha.",
        },
      ],
      left_out: [],
      trouble: null,
    },
  ],
  cannot: null,
};

/** The core, answering `curate` with `opened`, and what it was asked. */
function core(opened: Curating | string) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    if (cmd === "plane_at_launch")
      return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "opened_chats") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "curation_offers") return CURATIONS;
    if (cmd === "curate") {
      if (typeof opened === "string") throw opened;
      return opened;
    }
    return null;
  });
  return asked;
}

afterEach(() => {
  cleanup();
  clearMocks();
});

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const tabNames = () =>
  within(strip())
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

/** Runs the palette row titled `title`, the way an operator does. */
async function runFromThePalette(title: string) {
  await userEvent.keyboard("{F2}");
  const palette = await screen.findByRole("dialog", { name: "Command palette" });
  await userEvent.type(within(palette).getByRole("combobox"), "curate alpha");
  const row = await within(palette).findByRole("option", { name: new RegExp(`^${title}`) });
  await userEvent.click(row);
}

describe("choosing a curation action", () => {
  it("asks the core for that action on that subject and nothing of its text", async () => {
    const asked = core({
      session: 4,
      name: "4",
      label: "Tidy · alpha",
      persona: "ops",
      harness: "claude",
      workspace: "alpha",
    });
    render(<App />);
    await waitFor(() => expect(asked.some((one) => one.cmd === "curation_offers")).toBe(true));

    await runFromThePalette("Curate alpha: Tidy");

    await waitFor(() => expect(asked.some((one) => one.cmd === "curate")).toBe(true));
    const curate = asked.find((one) => one.cmd === "curate");
    expect(curate?.args).toMatchObject({
      plane: "/home/dev/plane",
      subject: "workspace:alpha",
      action: "ops/tidy",
    });
    // The prompt is never the window's to send: the core resolves it again.
    expect(JSON.stringify(curate?.args)).not.toContain("Tidy alpha.");
  });

  it("puts the chat's tab in front, named for the action and its subject", async () => {
    core({
      session: 4,
      name: "4",
      label: "Tidy · alpha",
      persona: "ops",
      harness: "claude",
      workspace: "alpha",
    });
    render(<App />);

    await runFromThePalette("Curate alpha: Tidy");

    await waitFor(() => expect(tabNames()).toContain("Tidy · alpha"));
    const tab = within(strip())
      .getAllByRole("tab")
      .find((one) => one.querySelector(".tab-name")?.textContent === "Tidy · alpha");
    expect(tab).toHaveAttribute("aria-selected", "true");
  });

  it("files a chat that runs at the plane root on the plane root's tab", async () => {
    core({
      session: 5,
      name: "5",
      label: "Safe remove · alpha",
      persona: "steward",
      harness: "claude",
      workspace: null,
    });
    render(<App />);

    await runFromThePalette("Curate alpha: Safe remove");

    await waitFor(() => expect(tabNames()).toContain("Safe remove · alpha"));
    expect(
      within(screen.getByRole("tablist", { name: "Workspaces" })).getByRole("tab", {
        name: "Plane root",
      }),
    ).toHaveAttribute("aria-selected", "true");
  });

  it("says the core's refusal and opens no tab", async () => {
    core("The default profile 'work' runs opencode, which says nothing until your first prompt.");
    render(<App />);

    await runFromThePalette("Curate alpha: Tidy");

    await screen.findByText(/says nothing until your first prompt/);
    expect(screen.queryByRole("tab", { name: /Tidy · alpha/ })).toBeNull();
  });
});
