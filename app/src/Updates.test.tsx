import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import {
  INSTALL_ENDS_SESSIONS,
  PinItem,
  UpdateItem,
  useUpdates,
  type Updates,
  type UpdateState,
} from "./Updates";
import type { Offer, PinReport } from "./bindings";
import type { Ending } from "./QuitWarning";

/**
 * **The update offer and the pin, on the status line.**
 *
 * The updater's events are the core's (`app/src-tauri/src/updates.rs`); the pin's verdict is
 * `adopt::version_report`'s (`app/src-tauri/src/pin.rs`). What is here is the window's half:
 * what each state draws, what installing says before it is pressed, and which failures are
 * worth drawing at all.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const OFFER: Offer = { version: "0.2.0", current: "0.1.0", channel: "stable", notes: "Fixes." };

function updates(state: UpdateState, over: Partial<Updates> = {}): Updates {
  return {
    state,
    channel: "stable",
    check: () => {},
    install: () => {},
    choose: () => {},
    restart: () => {},
    ...over,
  };
}

const button = () => screen.getByTestId("status-update");

/** One chat the window holds, as the quit warning is given it. */
function chat(name: string, state: Ending["state"], project?: string): Ending {
  return { key: `${project ?? ""}/${name}`, project, name, harness: "claude", cwd: null, state };
}

describe("the update button", () => {
  it("is an icon with no words while nothing new is known", () => {
    render(<UpdateItem updates={updates({ kind: "quiet" })} />);

    expect(button().textContent?.trim()).toBe("");
    expect(button()).toHaveAccessibleName(/stable channel, nothing new known/);
  });

  it("says the version on offer", () => {
    render(<UpdateItem updates={updates({ kind: "offered", offer: OFFER })} />);

    expect(button().textContent).toContain("0.2.0 available");
  });

  it("says, before Install is pressed, that installing ends every running chat", async () => {
    let installed = 0;
    render(
      <UpdateItem
        updates={updates({ kind: "offered", offer: OFFER }, { install: () => (installed += 1) })}
      />,
    );

    await userEvent.click(button());

    const dialog = await screen.findByRole("dialog");
    // The warning is on screen, in the dialog, BEFORE the button that acts on it is pressed.
    expect(within(dialog).getByTestId("update-ends-sessions").textContent).toBe(
      INSTALL_ENDS_SESSIONS,
    );
    expect(INSTALL_ENDS_SESSIONS).toMatch(/ends every running chat/);
    expect(installed).toBe(0);

    await userEvent.click(within(dialog).getByRole("button", { name: "Install 0.2.0" }));
    expect(installed).toBe(1);
  });

  it("offers no Install when there is nothing on offer", async () => {
    render(<UpdateItem updates={updates({ kind: "quiet" })} />);

    await userEvent.click(button());

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByRole("button", { name: /Install/ })).toBeNull();
    expect(within(dialog).getByRole("button", { name: "Check now" })).toBeInTheDocument();
  });

  it("says Restart to update once the update is installed", () => {
    render(<UpdateItem updates={updates({ kind: "installed", version: "0.2.0" })} />);

    expect(button().textContent?.trim()).toBe("Restart to update");
    expect(button()).toHaveAccessibleName("Updates: Restart to update");
  });

  it("restarts at once when no chat is mid-turn", async () => {
    let restarted = 0;
    render(
      <UpdateItem
        updates={updates(
          { kind: "installed", version: "0.2.0" },
          { restart: () => (restarted += 1) },
        )}
        chats={[chat("ide.1", "done"), chat("ide.2", "waiting")]}
      />,
    );

    await userEvent.click(button());
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Restart to update" }));

    expect(restarted).toBe(1);
  });

  it("names each chat that is mid-turn and waits when asked to", async () => {
    let restarted = 0;
    render(
      <UpdateItem
        updates={updates(
          { kind: "installed", version: "0.2.0" },
          { restart: () => (restarted += 1) },
        )}
        chats={[chat("ide.1", "running", "alpha"), chat("ide.2", "done", "alpha")]}
      />,
    );

    await userEvent.click(button());
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Restart to update" }),
    );

    const asking = await screen.findByRole("dialog");
    expect(within(asking).getByRole("alert").textContent).toBe(
      "ide.1 is mid-turn and will be interrupted.",
    );
    expect(within(asking).queryByText("ide.2")).toBeNull();
    // The answer that ends nothing is the one a stray Return finds.
    await waitFor(() => expect(within(asking).getByRole("button", { name: "Wait" })).toHaveFocus());
    await userEvent.click(within(asking).getByRole("button", { name: "Wait" }));

    expect(restarted).toBe(0);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("restarts over a chat that is mid-turn when the operator says to continue", async () => {
    let restarted = 0;
    render(
      <UpdateItem
        updates={updates(
          { kind: "installed", version: "0.2.0" },
          { restart: () => (restarted += 1) },
        )}
        chats={[chat("ide.1", "running", "alpha"), chat("ide.1", "running", "beta")]}
      />,
    );

    await userEvent.click(button());
    await userEvent.click(
      within(await screen.findByRole("dialog")).getByRole("button", { name: "Restart to update" }),
    );
    const asking = await screen.findByRole("dialog");
    expect(within(asking).getByRole("alert").textContent).toBe(
      "2 chats are mid-turn and will be interrupted.",
    );
    // Two projects, and a chat's name is only unique inside its own.
    expect(within(asking).getByText("alpha")).toBeInTheDocument();
    expect(within(asking).getByText("beta")).toBeInTheDocument();
    await userEvent.click(within(asking).getByRole("button", { name: "Restart now" }));

    expect(restarted).toBe(1);
  });

  it("says why charter would not restart, and still offers it", async () => {
    render(
      <UpdateItem
        updates={updates({
          kind: "installed",
          version: "0.2.0",
          refused: "no update has been installed",
        })}
      />,
    );

    await userEvent.click(button());
    const dialog = await screen.findByRole("dialog");

    expect(within(dialog).getByRole("alert").textContent).toBe("no update has been installed");
    expect(within(dialog).getByRole("button", { name: "Restart to update" })).toBeInTheDocument();
  });

  it("spins only while installing", () => {
    const { rerender } = render(
      <UpdateItem updates={updates({ kind: "installing", offer: OFFER })} />,
    );
    expect(button().querySelector(".spinning")).not.toBeNull();

    rerender(<UpdateItem updates={updates({ kind: "offered", offer: OFFER })} />);
    expect(button().querySelector(".spinning")).toBeNull();
  });

  it("puts the machine on the channel the operator picks", async () => {
    const chose: string[] = [];
    render(<UpdateItem updates={updates({ kind: "quiet" }, { choose: (c) => chose.push(c) })} />);

    await userEvent.click(button());
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("radio", { name: "dev" }));

    expect(chose).toEqual(["dev"]);
  });
});

describe("useUpdates", () => {
  let asked: string[];

  beforeEach(() => {
    asked = [];
    mockIPC(
      (cmd) => {
        asked.push(cmd);
        if (cmd === "update_channel") return "stable";
        return null;
      },
      { shouldMockEvents: true },
    );
  });

  /** Lets the listeners register. */
  async function mounted() {
    const hook = renderHook(() => useUpdates());
    await waitFor(() => expect(hook.result.current.channel).toBe("stable"));
    // The listeners register asynchronously; give them a turn.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    return hook;
  }

  it("draws the quiet updater, and rejects nothing, when the core refuses to listen", async () => {
    clearMocks();
    mockIPC(() => {
      throw new Error("refused");
    });
    const unhandled: unknown[] = [];
    const note = (e: PromiseRejectionEvent | unknown) => unhandled.push(e);
    process.on("unhandledRejection", note);
    try {
      const { result } = renderHook(() => useUpdates());
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 20));
      });
      expect(result.current.state).toEqual({ kind: "quiet" });
      expect(unhandled).toEqual([]);
    } finally {
      process.off("unhandledRejection", note);
    }
  });

  it("shows the offer a check found, and goes quiet when a check finds none", async () => {
    const { result } = await mounted();

    await act(() => emit("update://checked", OFFER));
    expect(result.current.state).toEqual({ kind: "offered", offer: OFFER });

    await act(() => emit("update://checked", null));
    expect(result.current.state).toEqual({ kind: "quiet" });
  });

  it("draws no failure from a check nobody asked for", async () => {
    // The timer checks every few hours, train or no train. Amber on every dropped Wi-Fi would
    // be furniture by Friday.
    const { result } = await mounted();

    await act(() => emit("update://failed", "could not reach the stable channel"));

    expect(result.current.state).toEqual({ kind: "quiet" });
  });

  it("draws the failure of a check the operator asked for", async () => {
    const { result } = await mounted();

    act(() => result.current.check());
    await act(() => emit("update://failed", "could not reach the stable channel"));

    expect(result.current.state).toEqual({
      kind: "failed",
      why: "could not reach the stable channel",
    });
    expect(asked).toContain("check_for_update");
  });

  it("does not let a timer's check undo an install under way", async () => {
    const { result } = await mounted();
    await act(() => emit("update://checked", OFFER));

    act(() => result.current.install());
    expect(result.current.state.kind).toBe("installing");
    await act(() => emit("update://checked", null));

    expect(result.current.state.kind).toBe("installing");
    await act(() => emit("update://installed", "0.2.0"));
    expect(result.current.state).toEqual({ kind: "installed", version: "0.2.0" });
  });

  it("asks the core to restart to update, and says why when it will not", async () => {
    clearMocks();
    mockIPC(
      (cmd) => {
        asked.push(cmd);
        if (cmd === "update_channel") return "stable";
        if (cmd === "restart_to_update") throw "no update has been installed";
        return null;
      },
      { shouldMockEvents: true },
    );
    const { result } = await mounted();
    await act(() => emit("update://installed", "0.2.0"));

    await act(async () => {
      result.current.restart();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(asked).toContain("restart_to_update");
    expect(result.current.state).toEqual({
      kind: "installed",
      version: "0.2.0",
      refused: "no update has been installed",
    });
  });

  it("drops an offer from the old channel when the channel changes, and looks again", async () => {
    const { result } = await mounted();
    await act(() => emit("update://checked", OFFER));

    await act(async () => {
      result.current.choose("dev");
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(result.current.state).toEqual({ kind: "quiet" });
    expect(result.current.channel).toBe("dev");
    expect(asked).toContain("set_update_channel");
    expect(asked).toContain("check_for_update");
  });
});

describe("the pin item", () => {
  const DRIFT: PinReport = {
    drift: true,
    brought: "0.1.0",
    pinned: "9.0.0",
    said: ["drift: this control plane pins 9.0.0, and this charter is 0.1.0."],
    news: [{ version: "0.2.0", headline: "Something came" }],
    more_news: 0,
  };

  it("draws nothing when charter version says the pin is met", () => {
    const { container } = render(
      <PinItem pin={{ ...DRIFT, drift: false, news: [] }} again={() => {}} />,
    );

    expect(container.textContent).toBe("");
  });

  it("draws nothing before charter version has answered", () => {
    const { container } = render(<PinItem pin={undefined} again={() => {}} />);

    expect(container.textContent).toBe("");
  });

  it("names the pin, and its dialog says charter version's own words and what came since", async () => {
    let asked = 0;
    render(<PinItem pin={DRIFT} again={() => (asked += 1)} />);

    const item = screen.getByTestId("status-pin");
    expect(item.textContent).toContain("pin 9.0.0");

    await userEvent.click(item);
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(DRIFT.said[0])).toBeInTheDocument();
    expect(within(dialog).getByText("Something came")).toBeInTheDocument();
    // Opening it asks again, so a pin moved since the project opened is not shown stale.
    expect(asked).toBe(1);
  });
});
