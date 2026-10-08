import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ProjectPresetsNotice } from "./ProjectPresetsNotice";
import type { SandboxState } from "./bindings";

/**
 * The one-time Notice of a project's Internet access presets changing (#1385): each teammate is
 * told what was turned on and off, and what it widens, once; the presets apply whatever they
 * press.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const LABEL = "The project's Internet access changed";

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
  presets_changed: {
    added: ["Package registries"],
    removed: ["Code hosting"],
    widens: ["Package registries also lets chats write the project's own package caches."],
    now: ["model-providers", "toolchains"],
  },
};

function core(state: SandboxState, after: SandboxState = QUIET) {
  const asked: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "sandbox_state") return state;
    if (cmd === "acknowledge_project_presets") return after;
    return null;
  });
  return asked;
}

describe("the project's presets Notice", () => {
  it("names what was turned on and off, and what it widens", async () => {
    core(CHANGED);
    render(<ProjectPresetsNotice plane={PLANE} onReview={() => {}} />);

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent("Turned on: Package registries.");
    expect(notice).toHaveTextContent("Turned off: Code hosting.");
    expect(notice).toHaveTextContent("write the project's own package caches");
  });

  it("says nothing when the presets did not change", async () => {
    const asked = core(QUIET);
    render(<ProjectPresetsNotice plane={PLANE} onReview={() => {}} />);

    await waitFor(() => expect(asked.map((one) => one.cmd)).toContain("sandbox_state"));
    expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument();
  });

  it("records the set it showed once read, and is gone", async () => {
    const asked = core(CHANGED);
    render(<ProjectPresetsNotice plane={PLANE} onReview={() => {}} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Got it" }));

    await waitFor(() =>
      expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument(),
    );
    expect(asked.find((one) => one.cmd === "acknowledge_project_presets")?.args).toMatchObject({
      plane: PLANE,
      shown: ["model-providers", "toolchains"],
    });
  });

  it("opens the Sandbox settings to review them, and records nothing", async () => {
    const asked = core(CHANGED);
    const onReview = vi.fn();
    render(<ProjectPresetsNotice plane={PLANE} onReview={onReview} />);

    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Review Internet access" }));

    expect(onReview).toHaveBeenCalledOnce();
    expect(asked.map((one) => one.cmd)).not.toContain("acknowledge_project_presets");
  });
});
