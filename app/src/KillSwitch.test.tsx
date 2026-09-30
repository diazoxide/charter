import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { TitleBar } from "./TitleBar";

/**
 * **The kill switch on the title bar** (OV-1): one control that stops every agent in every
 * project and window, and, once thrown, the one that re-arms. What stopping does is the core's
 * and is tested there (`planes.rs`); this is that the bar sends it, says it is thrown, and hears
 * `charter stop --all` throw it from a terminal.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

/** The core, answering whether agents are stopped with `stopped`, and noting what it is sent. */
function core(stopped: boolean) {
  const sent: string[] = [];
  mockIPC(
    (cmd) => {
      sent.push(cmd);
      if (cmd === "agents_stopped") return stopped;
      if (cmd === "stop_every_agent") return 20;
      if (cmd === "rearm_agents") return null;
      return null;
    },
    { shouldMockEvents: true },
  );
  return sent;
}

describe("the kill switch", () => {
  it("stops every agent in one press, with no question in between", async () => {
    const sent = core(false);
    render(<TitleBar />);

    await userEvent.click(await screen.findByRole("button", { name: /stop every agent/i }));

    await vi.waitFor(() => expect(sent).toContain("stop_every_agent"));
  });

  it("says every agent is stopped and offers the re-arm once it is thrown", async () => {
    const sent = core(false);
    render(<TitleBar />);
    await userEvent.click(await screen.findByRole("button", { name: /stop every agent/i }));

    const rearm = await screen.findByRole("button", { name: /re-arm/i });
    expect(screen.queryByRole("button", { name: /stop every agent/i })).toBeNull();

    await userEvent.click(rearm);
    await vi.waitFor(() => expect(sent).toContain("rearm_agents"));
    await screen.findByRole("button", { name: /stop every agent/i });
  });

  it("opens on a switch already thrown, as a launch after a stop does", async () => {
    core(true);
    render(<TitleBar />);

    await screen.findByRole("button", { name: /re-arm/i });
  });

  it("hears charter stop --all throw it from a terminal, and the re-arm after", async () => {
    core(false);
    render(<TitleBar />);
    await screen.findByRole("button", { name: /stop every agent/i });

    await act(() => emit("kill-switch", true));
    await waitFor(() => expect(screen.getByRole("button", { name: /re-arm/i })).toBeTruthy());

    await act(() => emit("kill-switch", false));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /stop every agent/i })).toBeTruthy(),
    );
  });

  it("is a button, with the tabindex WebKit's tab sequence needs, on a bar that drags", async () => {
    core(false);
    render(<TitleBar />);

    const stop = await screen.findByRole("button", { name: /stop every agent/i });
    expect(stop.tagName).toBe("BUTTON");
    expect(stop).toHaveAttribute("tabindex", "0");
  });
});
