import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SandboxOffer } from "./SandboxOffer";
import type { SandboxState } from "./bindings";

/**
 * The one-time offer of the sandbox to an existing project (ADR 0067 §1, ruling V21 1): a
 * notice, answered once, that turns nothing on by itself.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

function core(state: SandboxState, after?: SandboxState) {
  const asked: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "sandbox_state") return state;
    if (cmd === "answer_sandbox_offer") return after;
    return null;
  });
  return asked;
}

const DUE: SandboxState = {
  on: false,
  offer: true,
  said: null,
  never: ["Codex: purlis can wrap it on macOS only, so far"],
  hosts_changed: null,
  presets: [],
  persona_hosts: [],
  besides: { project_hosts: 0, your_hosts: 0, folders: 0 },
  policy: null,
};

describe("the sandbox offer", () => {
  it("is a notice, never a dialog, for a project the offer is due to", async () => {
    core(DUE);
    render(<SandboxOffer plane={PLANE} />);

    expect(await screen.findByRole("status", { name: "The sandbox offer" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("says nothing to a project that has the sandbox, or has answered", async () => {
    const asked = core({
      on: true,
      offer: false,
      said: "no chat yet",
      never: [],
      hosts_changed: null,
      presets: [],
      persona_hosts: [],
      besides: { project_hosts: 0, your_hosts: 0, folders: 0 },
      policy: null,
    });
    render(<SandboxOffer plane={PLANE} />);

    await waitFor(() => expect(asked.map((one) => one.cmd)).toContain("sandbox_state"));
    expect(screen.queryByRole("status", { name: "The sandbox offer" })).not.toBeInTheDocument();
  });

  it("turns the sandbox on only when asked to, and is gone once answered", async () => {
    const asked = core(DUE, {
      on: true,
      offer: false,
      said: "no chat yet",
      never: [],
      hosts_changed: null,
      presets: [],
      persona_hosts: [],
      besides: { project_hosts: 0, your_hosts: 0, folders: 0 },
      policy: null,
    });
    render(<SandboxOffer plane={PLANE} />);

    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Turn the sandbox on" }));

    await waitFor(() =>
      expect(screen.queryByRole("status", { name: "The sandbox offer" })).not.toBeInTheDocument(),
    );
    expect(asked.find((one) => one.cmd === "answer_sandbox_offer")?.args).toEqual({
      plane: PLANE,
      turnOn: true,
    });
  });

  it("keeps it off when asked to, and does not ask again", async () => {
    const asked = core(DUE, {
      on: false,
      offer: false,
      said: null,
      never: [],
      hosts_changed: null,
      presets: [],
      persona_hosts: [],
      besides: { project_hosts: 0, your_hosts: 0, folders: 0 },
      policy: null,
    });
    render(<SandboxOffer plane={PLANE} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Keep it off" }));

    await waitFor(() =>
      expect(screen.queryByRole("status", { name: "The sandbox offer" })).not.toBeInTheDocument(),
    );
    expect(asked.find((one) => one.cmd === "answer_sandbox_offer")?.args).toEqual({
      plane: PLANE,
      turnOn: false,
    });
  });

  it("names each harness this machine never sandboxes, so the offer never covers it", async () => {
    core(DUE);
    render(<SandboxOffer plane={PLANE} />);

    expect(await screen.findByRole("status", { name: "The sandbox offer" })).toHaveTextContent(
      "Never sandboxed on this machine, so a new chat on them starts only without it: Codex: purlis can wrap it on macOS only, so far.",
    );
  });
});
