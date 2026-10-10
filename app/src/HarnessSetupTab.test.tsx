import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { HarnessSetupTab } from "./HarnessSetupTab";
import type { HarnessSetupFound } from "./bindings";

/**
 * **The harness setup tab's empty parts** (DS-3 #626, FR-19 #614): no model served on this
 * machine is an `EmptyState` that says what would be named there, and a look at the machine
 * that failed says so instead of claiming no harness is installed. The setup's way through, end
 * to end, is `FirstRun.test.tsx`'s.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

const NOTHING: HarnessSetupFound = {
  harnesses: [
    {
      name: "opencode",
      title: "opencode",
      installed: false,
      signed_in: false,
      installer: "curl -fsSL https://opencode.ai/install | bash",
      installer_page: "https://opencode.ai/docs/",
    },
  ],
  local_models: [],
};

function open() {
  render(<HarnessSetupTab plane={PLANE} cwd="/home/dev/web" workspace="web" />);
}

describe("the harness setup tab", () => {
  it("says what would be named where no model is served on this machine", async () => {
    mockIPC((cmd) => (cmd === "harness_setup_found" ? NOTHING : null));
    open();

    const empty = await screen.findByTestId("harness-setup-no-local-model");
    expect(within(empty).getByText("No model is served on this machine")).toBeInTheDocument();
    expect(empty).toHaveTextContent(/Without an account, opencode can run on a model served/);
  });

  it("says the machine could not be looked at, never that no harness is installed", async () => {
    mockIPC((cmd) => {
      if (cmd === "harness_setup_found") throw new Error("the look timed out");
      return null;
    });
    open();

    expect(
      await screen.findByRole("heading", { name: "purlis could not look at this machine" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("the look timed out");
    expect(screen.queryByRole("heading", { name: "No harness found" })).toBeNull();
    expect(screen.queryByText(/none is installed on this machine/)).toBeNull();
    expect(screen.queryByTestId("harness-setup-no-local-model")).toBeNull();
    expect(screen.getByRole("button", { name: "Check again" })).toBeEnabled();
  });
});
