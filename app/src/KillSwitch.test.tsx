import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { TitleBar } from "./TitleBar";

/**
 * **The kill switch on the title bar** (OV-1, ADR 0071): one control that stops every chat and
 * shell purlis started, in every project and window, and, once thrown, the one that re-arms.
 * What stopping does is the core's and is tested there (`planes.rs`, `killswitch.rs`); this is
 * that the bar sends it, draws what the CORE says the switch is — never what it hoped — says a
 * stop that was not kept, and hears `charter stop --all` throw it from a terminal.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

/**
 * A core whose switch starts as `stopped`, and what it was sent. `stop` is what its
 * `stop_every_agent` does: throw the switch and answer 20, or throw it and answer that it
 * could not be kept, or refuse and leave it where it was.
 */
function core(stopped: boolean, stop: "kept" | "not-kept" | "refused" = "kept") {
  const sent: string[] = [];
  let now = stopped;
  mockIPC(
    (cmd) => {
      sent.push(cmd);
      if (cmd === "agents_stopped") return now;
      if (cmd === "stop_every_agent") {
        if (stop === "refused") throw "the stop did not finish";
        now = true;
        if (stop === "not-kept")
          throw "every agent is stopped in this app, but the stop marker could not be written";
        return 20;
      }
      if (cmd === "rearm_agents") {
        now = false;
        return null;
      }
      return null;
    },
    { shouldMockEvents: true },
  );
  return sent;
}

const stopControl = () => screen.findByRole("button", { name: /stop every chat and shell/i });
const rearmControl = () => screen.findByRole("button", { name: /re-arm/i });

describe("the kill switch", () => {
  it("stops every agent in one press, with no question in between", async () => {
    const sent = core(false);
    render(<TitleBar />);

    await userEvent.click(await stopControl());

    await vi.waitFor(() => expect(sent).toContain("stop_every_agent"));
    await rearmControl();
  });

  it("says what it stops: the chats and shells charter started, and nothing it cannot reach", async () => {
    core(false);
    render(<TitleBar />);

    expect(await stopControl()).toHaveAttribute(
      "title",
      expect.stringMatching(/every chat and shell purlis started/),
    );
  });

  it("re-arms from the bar, and only then offers the stop again", async () => {
    const sent = core(true);
    render(<TitleBar />);

    await userEvent.click(await rearmControl());

    await vi.waitFor(() => expect(sent).toContain("rearm_agents"));
    await stopControl();
  });

  it("draws the switch the core answered, not the one it pressed for", async () => {
    // A stop the core refused leaves agents running, and the bar must not say otherwise.
    core(false, "refused");
    render(<TitleBar />);

    await userEvent.click(await stopControl());

    expect(await screen.findByRole("alert")).toHaveTextContent("the stop did not finish");
    expect(screen.queryByRole("button", { name: /re-arm/i })).toBeNull();
    await stopControl();
  });

  it("says loudly when the stop holds in the app but was not kept on disk", async () => {
    core(false, "not-kept");
    render(<TitleBar />);

    await userEvent.click(await stopControl());

    expect(await screen.findByRole("alert")).toHaveTextContent("could not be written");
    await rearmControl();
  });

  it("opens on a switch already thrown, as a launch after a stop does", async () => {
    core(true);
    render(<TitleBar />);

    await rearmControl();
  });

  it("hears charter stop --all throw it from a terminal, and the re-arm after", async () => {
    core(false);
    render(<TitleBar />);
    await stopControl();

    await act(() => emit("kill-switch", true));
    await waitFor(() => expect(screen.getByRole("button", { name: /re-arm/i })).toBeTruthy());

    await act(() => emit("kill-switch", false));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /stop every chat and shell/i })).toBeTruthy(),
    );
  });

  it("has the tabindex WebKit's tab sequence needs", async () => {
    core(false);
    render(<TitleBar />);

    expect(await stopControl()).toHaveAttribute("tabindex", "0");
  });
});
