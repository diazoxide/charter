import { StrictMode } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { BusNotice } from "./bindings";
import type { Ending } from "./QuitWarning";
import { SessionBusNotice } from "./SessionBusNotice";

/**
 * **A launch without the session bus says so in the window** (charter#746).
 *
 * Linux starts charter without the session bus when the desktop portal is silent, and when
 * there is none at all (`portal.rs`). What that run loses — the tray icon, notifications, and
 * handing a second launch over — used to be said on standard error only, which an operator who
 * clicked an icon never sees.
 */

/** What the core says; the tests below look for keywords, never the whole sentence. */
const SILENT: BusNotice = {
  says: "No tray icon, no notifications, and a second launch is refused.",
  can_restart: false,
};
const ANSWERS: BusNotice = { ...SILENT, can_restart: true };

const RESTART = "Restart with the full desktop integration";

const MID_TURN: Ending = {
  key: "p:1",
  name: "refactor",
  harness: "claude-code",
  cwd: null,
  state: "running",
};

function core(notice: BusNotice | null): { asked: string[] } {
  const asked: string[] = [];
  mockIPC(
    (cmd) => {
      asked.push(cmd);
      if (cmd === "session_bus") return notice;
      return null;
    },
    { shouldMockEvents: true },
  );
  return { asked };
}

/** The core saying the kept bus answers now. */
const busAnswers = () => act(() => emit("session-bus://answers", ANSWERS));

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
    expect(notice.textContent).toContain("tray icon");
    expect(notice.textContent).toContain("notifications");
    expect(screen.queryByRole("button", { name: RESTART })).toBeNull();
  });

  it("offers the restart once the bus answers, and restarts only when asked", async () => {
    const { asked } = core(SILENT);
    render(<SessionBusNotice />);
    await screen.findByRole("status");

    await busAnswers();

    const restart = await screen.findByRole("button", { name: RESTART });
    expect(asked).not.toContain("restart_on_the_session_bus");
    await userEvent.click(restart);
    await waitFor(() => expect(asked).toContain("restart_on_the_session_bus"));
  });

  it("asks first when a chat could be mid-turn, and waiting restarts nothing", async () => {
    const { asked } = core(SILENT);
    render(<SessionBusNotice chats={[MID_TURN]} />);
    await screen.findByRole("status");
    await busAnswers();

    await userEvent.click(await screen.findByRole("button", { name: RESTART }));

    const ask = await screen.findByRole("alertdialog");
    expect(ask.textContent).toContain("refactor");
    await userEvent.click(screen.getByRole("button", { name: "Wait" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(asked).not.toContain("restart_on_the_session_bus");

    await userEvent.click(screen.getByRole("button", { name: RESTART }));
    await userEvent.click(await screen.findByRole("button", { name: "Restart now" }));
    await waitFor(() => expect(asked).toContain("restart_on_the_session_bus"));
  });

  it("goes away when dismissed", async () => {
    core(SILENT);
    render(<SessionBusNotice />);
    await screen.findByRole("status");

    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));

    expect(screen.queryByRole("status")).toBeNull();
  });

  it("comes back with the offer when the bus answers after it was dismissed", async () => {
    core(SILENT);
    render(<SessionBusNotice />);
    await screen.findByRole("status");
    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));

    await busAnswers();

    expect(await screen.findByRole("button", { name: RESTART })).toBeTruthy();
  });

  it("puts both of its buttons in the tab order", async () => {
    core(ANSWERS);
    render(<SessionBusNotice />);

    for (const name of [RESTART, "Dismiss"])
      expect((await screen.findByRole("button", { name })).getAttribute("tabindex")).toBe("0");
  });
});
