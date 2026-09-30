import { StrictMode } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { WithoutTheBus } from "./bindings";
import { SessionBusNotice } from "./WithoutTheBus";

/**
 * **A launch without the session bus says so in the window** (charter#746).
 *
 * Linux starts charter without the session bus when the desktop portal is silent, and when
 * there is none at all (`portal.rs`). What that run loses — the tray icon, notifications, and
 * handing a second launch over — used to be said on standard error only, which an operator who
 * clicked an icon never sees.
 */

const SILENT: WithoutTheBus = {
  says:
    "charter started without the session bus: the desktop portal did not answer on it within " +
    "300 ms. For this run there is no tray icon and no desktop notifications, and a second " +
    "launch is refused instead of being handed to this one — charter-app#24.",
  can_restart: false,
};

const RESTART = "Restart with the full desktop integration";

function core(notice: WithoutTheBus | null): { asked: string[] } {
  const asked: string[] = [];
  mockIPC(
    (cmd) => {
      asked.push(cmd);
      if (cmd === "session_bus") return notice;
      if (cmd === "restart_on_the_session_bus") return null;
      return null;
    },
    { shouldMockEvents: true },
  );
  return { asked };
}

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("the notice about a launch without the session bus", () => {
  it("is not drawn on a launch that has the bus", async () => {
    const { asked } = core(null);

    render(
      <StrictMode>
        <SessionBusNotice />
      </StrictMode>,
    );

    await waitFor(() => expect(asked).toContain("session_bus"));
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("names what is off for the run, and offers no restart while the bus is still silent", async () => {
    core(SILENT);

    render(<SessionBusNotice />);

    const notice = await screen.findByRole("status");
    expect(notice.textContent).toContain("no tray icon");
    expect(notice.textContent).toContain("notifications");
    expect(notice.textContent).toContain("second launch");
    expect(screen.queryByRole("button", { name: RESTART })).toBeNull();
  });

  it("offers the restart once the bus answers, and restarts only when asked", async () => {
    const { asked } = core(SILENT);
    render(<SessionBusNotice />);
    await screen.findByRole("status");

    await act(() => emit("session-bus://answers", { ...SILENT, can_restart: true }));

    const restart = await screen.findByRole("button", { name: RESTART });
    expect(asked).not.toContain("restart_on_the_session_bus");
    await userEvent.click(restart);
    await waitFor(() => expect(asked).toContain("restart_on_the_session_bus"));
  });

  it("goes away when dismissed", async () => {
    core(SILENT);
    render(<SessionBusNotice />);
    await screen.findByRole("status");

    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));

    expect(screen.queryByRole("status")).toBeNull();
  });
});
