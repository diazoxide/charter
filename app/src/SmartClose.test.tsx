import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "./App";
import type { Phase, SmartCloseOffer } from "./bindings";

/**
 * **Smart close, as the window draws it** (ADR 0064, SI-8c): the close dialog's three answers,
 * the tab wrapping up while the chat writes its record, the menu's Cancel, and the tab going
 * when — and only when — the core says the record landed.
 *
 * The core's side (the prompt, the queue, the record, the timeout, typing cancelling it) is
 * `planes.rs`'s smart-close tests, on a real terminal and a real socket. This is the window
 * reading what the core tells it.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

const SIDEBAR = {
  root: PLANE,
  workspaces: [
    {
      name: "alpha",
      path: `${PLANE}/workspaces/alpha`,
      vision: "Ship it",
      todos: [],
      chats: [],
    },
  ],
  personas: ["steward"],
  persona: "steward",
  unfiled: [],
};

const START_OPTIONS = {
  profiles: [
    {
      name: "claude",
      kind: "claude",
      shown: "claude",
      source: "built-in",
      is_default: true,
      approval: null,
    },
  ],
  refused: [],
  personas: ["steward"],
  persona: "steward",
  ignore_fix: null,
  declares_none: true,
};

/** A chat with turns behind it, waiting: Smart close is offered and is the default. */
const OFFERED: SmartCloseOffer = { available: true, why: null, close_first: false };

/** Answers every command, with `offer` as the core's answer about Smart close. */
function core(offer: SmartCloseOffer = OFFERED): { asked: { cmd: string; args: unknown }[] } {
  const asked: { cmd: string; args: unknown }[] = [];
  let opened = 0;
  mockIPC(
    (cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "plane_sidebar") return SIDEBAR;
      if (cmd === "running_sessions") return [];
      if (cmd === "opened_chats") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "start_options") return START_OPTIONS;
      if (cmd === "start_chat") return { session: ++opened };
      if (cmd === "smart_close_offer") return offer;
      if (cmd === "smart_close") return "sent";
      if (cmd === "smart_closing") return [];
      return null;
    },
    { shouldMockEvents: true },
  );
  return { asked };
}

async function openAChat() {
  await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
  await userEvent.click(await screen.findByRole("button", { name: "Start" }));
}

/** The core telling the window a step of chat `session`'s smart close. */
async function step(session: number, phase: Phase, plane = PLANE) {
  await act(() => emit("smart-close", { plane, session, phase }));
}

const tabOf = (name: string) =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .queryAllByRole("tab")
    .find((tab) => tab.querySelector(".tab-name")?.textContent === name);

/** The tab called `name`, which the test needs to be there. */
function theTab(name: string): HTMLElement {
  const tab = tabOf(name);
  if (tab === undefined) throw new Error("no such tab");
  return tab;
}

const asked = (log: { cmd: string; args: unknown }[], cmd: string) =>
  log.filter((one) => one.cmd === cmd).map((one) => one.args);

describe("the close dialog's three answers", () => {
  it("asks Cancel, Close and Smart close, with Smart close first for a chat with turns behind it", async () => {
    const { asked: log } = core();
    render(<App />);
    await openAChat();

    await userEvent.click(screen.getByRole("button", { name: "End chat steward 1" }));
    const asking = await screen.findByRole("alertdialog");

    const answers = within(asking).getAllByRole("button");
    expect(answers.map((answer) => answer.textContent)).toEqual(["Cancel", "Close", "Smart close"]);
    await waitFor(() => expect(answers[2]).toHaveFocus());
    expect(asked(log, "smart_close_offer")).toEqual([{ plane: PLANE, session: 1 }]);
  });

  it("starts on Close for a chat that has had at most one turn", async () => {
    core({ available: true, why: null, close_first: true });
    render(<App />);
    await openAChat();

    await userEvent.click(screen.getByRole("button", { name: "End chat steward 1" }));
    const asking = await screen.findByRole("alertdialog");

    await waitFor(() =>
      expect(within(asking).getByRole("button", { name: "Close" })).toHaveFocus(),
    );
    expect(within(asking).getByRole("button", { name: "Smart close" })).toBeEnabled();
  });

  it("offers Close only, with the core's reason, where the chat cannot write a record", async () => {
    const why = "This chat is asking you something. Answer it first, then smart close it.";
    core({ available: false, why, close_first: false });
    render(<App />);
    await openAChat();

    await userEvent.click(screen.getByRole("button", { name: "End chat steward 1" }));
    const asking = await screen.findByRole("alertdialog");

    const smart = within(asking).getByRole("button", { name: "Smart close" });
    expect(smart).toBeDisabled();
    expect(within(asking).getByText(why)).toBeInTheDocument();
    // Nothing charter knows says which answer is right, so a reflex Return cancels.
    await waitFor(() =>
      expect(within(asking).getByRole("button", { name: "Cancel" })).toHaveFocus(),
    );
  });

  it("starts a smart close on the press and ends nothing", async () => {
    const { asked: log } = core();
    render(<App />);
    await openAChat();

    await userEvent.click(screen.getByRole("button", { name: "End chat steward 1" }));
    const asking = await screen.findByRole("alertdialog");
    await userEvent.click(within(asking).getByRole("button", { name: "Smart close" }));

    expect(screen.queryByRole("alertdialog")).toBeNull();
    await vi.waitFor(() =>
      expect(asked(log, "smart_close")).toEqual([{ plane: PLANE, session: 1 }]),
    );
    expect(asked(log, "close_session")).toEqual([]);
    expect(tabOf("steward 1")).toBeDefined();
  });

  it("closes as it always did when Close is pressed", async () => {
    const { asked: log } = core();
    render(<App />);
    await openAChat();

    await userEvent.click(screen.getByRole("button", { name: "End chat steward 1" }));
    const asking = await screen.findByRole("alertdialog");
    await userEvent.click(within(asking).getByRole("button", { name: "Close" }));

    expect(asked(log, "close_session")).toEqual([{ plane: PLANE, session: 1 }]);
    expect(asked(log, "smart_close")).toEqual([]);
  });
});

describe("a chat wrapping up", () => {
  it("wears the closing look on its tab, and its tooltip says why", async () => {
    core();
    render(<App />);
    await openAChat();

    await step(1, "sent");

    const tab = theTab("steward 1");
    await waitFor(() => expect(tab.closest(".tab")).toHaveAttribute("data-wrapping-up"));
    expect(tab).toHaveAttribute("title", expect.stringContaining("Wrapping up"));
    expect(within(tab).getByRole("img", { name: "wrapping up" })).toBeInTheDocument();
  });

  it("offers Cancel smart close on its tab's menu, and cancelling asks the core", async () => {
    const { asked: log } = core();
    render(<App />);
    await openAChat();
    await step(1, "queued");
    await waitFor(() =>
      expect(theTab("steward 1").closest(".tab")).toHaveAttribute("data-wrapping-up"),
    );

    fireEvent.contextMenu(theTab("steward 1"));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: /Cancel smart close of steward 1/ }),
    );

    await vi.waitFor(() =>
      expect(asked(log, "cancel_smart_close")).toEqual([{ plane: PLANE, session: 1 }]),
    );
    expect(asked(log, "close_session")).toEqual([]);
  });

  it("goes back to normal when the core says it was cancelled — the operator typed into it", async () => {
    core();
    render(<App />);
    await openAChat();
    await step(1, "sent");
    await waitFor(() =>
      expect(theTab("steward 1").closest(".tab")).toHaveAttribute("data-wrapping-up"),
    );

    await step(1, "cancelled");

    await waitFor(() =>
      expect(theTab("steward 1").closest(".tab")).not.toHaveAttribute("data-wrapping-up"),
    );
    expect(theTab("steward 1").getAttribute("title") ?? "").not.toContain("Wrapping up");
    expect(screen.queryByRole("menuitem", { name: /Cancel smart close/ })).toBeNull();
  });

  it("closes its tab when the core says the record landed, and never ends the chat a second time", async () => {
    const { asked: log } = core();
    render(<App />);
    await openAChat();
    await step(1, "sent");

    await step(1, "closed");

    await waitFor(() => expect(tabOf("steward 1")).toBeUndefined());
    expect(asked(log, "close_session")).toEqual([]);
  });

  it("stays open, and says so, when no record arrived in time", async () => {
    core();
    render(<App />);
    await openAChat();
    await step(1, "sent");

    await step(1, "no_record");

    expect(
      await screen.findByText(
        "No session record arrived from steward 1 within five minutes, so it was left open.",
      ),
    ).toBeInTheDocument();
    expect(theTab("steward 1").closest(".tab")).not.toHaveAttribute("data-wrapping-up");
  });

  it("says the record was not written when the chat ended on its own", async () => {
    core();
    render(<App />);
    await openAChat();
    await step(1, "sent");

    await step(1, "ended");

    expect(
      await screen.findByText("steward 1 ended before it wrote its session record."),
    ).toBeInTheDocument();
  });

  it("hears nothing about another project's chat of the same number", async () => {
    core();
    render(<App />);
    await openAChat();

    await step(1, "closed", "/somewhere/else");

    expect(tabOf("steward 1")).toBeDefined();
  });
});
