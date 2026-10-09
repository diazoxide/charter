import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ProjectHostsNotice } from "./ProjectHostsNotice";
import type { SandboxState } from "./bindings";

/**
 * The one-time Notice of a project's hosts changing (#1341): each teammate is told what was
 * added and taken away, once, and the hosts apply whatever they press.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

const QUIET: SandboxState = {
  on: true,
  offer: false,
  said: null,
  never: [],
  hosts_changed: null,
  presets_changed: null,
  presets: [],
  persona_hosts: [],
  besides: { project_hosts: 0, your_hosts: 0, folders: 0 },
  policy: null,
};

const CHANGED: SandboxState = {
  ...QUIET,
  hosts_changed: {
    added: ["10.100.39.145:6443", "*.internal.example"],
    removed: ["old.example"],
    now: ["10.100.39.145:6443", "*.internal.example"],
  },
};

function core(state: SandboxState, after: SandboxState = QUIET) {
  const asked: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "sandbox_state") return state;
    if (cmd === "acknowledge_project_hosts") return after;
    return null;
  });
  return asked;
}

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

/**
 * A core whose state is `states[0]` until the project changes on disk, then the next, and a
 * way to say it changed: `plane-changed` for the project, as the plane watcher sends it.
 */
function changing(...states: SandboxState[]) {
  const listeners = new Map<string, number[]>();
  let at = 0;
  mockIPC((cmd, args) => {
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, [...(listeners.get(event) ?? []), handler]);
      return 1;
    }
    if (cmd === "sandbox_state") return states[Math.min(at, states.length - 1)];
    if (cmd === "acknowledge_project_hosts") return QUIET;
    return null;
  });
  return async (answers: { answer: string }[]) => {
    at += 1;
    await waitFor(() => expect(listeners.get("plane-changed") ?? []).not.toHaveLength(0));
    const handlers = listeners.get("plane-changed") ?? [];
    await act(async () => {
      for (const handler of handlers)
        window.__TAURI_INTERNALS__.runCallback(handler, {
          event: "plane-changed",
          id: 1,
          payload: { plane: PLANE, changes: null, answers },
        });
      await Promise.resolve();
    });
  };
}

describe("the project's hosts Notice", () => {
  it("names what was added and what was taken away", async () => {
    core(CHANGED);
    render(<ProjectHostsNotice plane={PLANE} onReview={() => {}} />);

    const notice = await screen.findByRole("status", { name: "The project's hosts changed" });
    expect(notice).toHaveTextContent("Added: 10.100.39.145:6443, *.internal.example.");
    expect(notice).toHaveTextContent("Taken away: old.example.");
  });

  it("says nothing when the hosts did not change", async () => {
    const asked = core(QUIET);
    render(<ProjectHostsNotice plane={PLANE} onReview={() => {}} />);

    await waitFor(() => expect(asked.map((one) => one.cmd)).toContain("sandbox_state"));
    expect(
      screen.queryByRole("status", { name: "The project's hosts changed" }),
    ).not.toBeInTheDocument();
  });

  it("records the list it showed once read, and is gone", async () => {
    const asked = core(CHANGED);
    render(<ProjectHostsNotice plane={PLANE} onReview={() => {}} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Got it" }));

    await waitFor(() =>
      expect(
        screen.queryByRole("status", { name: "The project's hosts changed" }),
      ).not.toBeInTheDocument(),
    );
    expect(asked.find((one) => one.cmd === "acknowledge_project_hosts")?.args).toMatchObject({
      plane: PLANE,
      shown: ["10.100.39.145:6443", "*.internal.example"],
    });
  });

  it("opens the Sandbox settings to review them, and records nothing", async () => {
    const asked = core(CHANGED);
    const onReview = vi.fn();
    render(<ProjectHostsNotice plane={PLANE} onReview={onReview} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Review the hosts" }));

    expect(onReview).toHaveBeenCalledOnce();
    expect(asked.map((one) => one.cmd)).not.toContain("acknowledge_project_hosts");
  });

  // #1550: a pull while the window is open is told then, not at the next time it opens.
  it("is told when the project's settings change on disk while it is open", async () => {
    const pulled = changing(QUIET, CHANGED);
    render(<ProjectHostsNotice plane={PLANE} onReview={() => {}} />);
    await waitFor(() =>
      expect(screen.queryByRole("status", { name: "The project's hosts changed" })).toBeNull(),
    );

    await pulled([{ answer: "settings" }]);

    expect(
      await screen.findByRole("status", { name: "The project's hosts changed" }),
    ).toBeInTheDocument();
  });

  it("is not read again for a change that is not the settings'", async () => {
    const pulled = changing(QUIET, CHANGED);
    render(<ProjectHostsNotice plane={PLANE} onReview={() => {}} />);
    await waitFor(() =>
      expect(screen.queryByRole("status", { name: "The project's hosts changed" })).toBeNull(),
    );

    await pulled([{ answer: "sidebar" }]);

    expect(
      screen.queryByRole("status", { name: "The project's hosts changed" }),
    ).not.toBeInTheDocument();
  });
});
