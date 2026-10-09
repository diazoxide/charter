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
import type { Phase, SavedRecord, SmartCloseOffer } from "./bindings";
import { LEAST, LEAST_CHIP, leastAt } from "./fits";
import { dragWithTheKeyboard, laidOutInARow, stripNamed } from "./test-strips";
import { DEFAULT_TEXT } from "./textSize";

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
  vi.restoreAllMocks();
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
  persona_profiles: {},
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
async function step(
  session: number,
  phase: Phase,
  plane = PLANE,
  record: SavedRecord | null = null,
) {
  await act(() => emit("smart-close", { plane, session, phase, record }));
}

const tabOf = (name: string) =>
  within(stripNamed("Tabs"))
    .queryAllByRole("tab")
    .find((tab) => tab.querySelector(".tab-name")?.textContent === name);

/** The chip a tab called `name` shrinks to while it wraps up: no name on it, only its tooltip. */
const chipOf = (name: string) =>
  within(stripNamed("Tabs"))
    .queryAllByRole("tab")
    .find((tab) => tab.getAttribute("aria-label") === `${name} — wrapping up`);

/** The chip, which the test needs to be there. */
function theChip(name: string): HTMLElement {
  const chip = chipOf(name);
  if (chip === undefined) throw new Error("no such chip");
  return chip;
}

/** The chat strip as the operator reads it, left to right: a chip by its tooltip. */
const strip = () =>
  within(stripNamed("Tabs"))
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent ?? tab.getAttribute("aria-label"));

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
    expect(chipOf("steward 1")).toBeDefined();
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
  it("shrinks to a chip: the chat's icon and the breathing mark, its name only in its tooltip", async () => {
    core();
    render(<App />);
    await openAChat();

    await step(1, "sent");

    await waitFor(() => expect(chipOf("steward 1")).toBeDefined());
    const chip = theChip("steward 1");
    expect(chip.closest(".tab")).toHaveAttribute("data-wrapping-up");
    expect(chip.closest(".tab")).toHaveAttribute("data-chip");
    expect(chip).toHaveAttribute("title", "steward 1 — wrapping up");
    expect(chip.querySelector(".tab-name")).toBeNull();
    expect(chip.querySelector('[data-mark="chat"]')).not.toBeNull();
    expect(within(chip).getByRole("img", { name: "wrapping up" })).toHaveClass("breathing");
    expect(tabOf("steward 1")).toBeUndefined();
  });

  it("offers Cancel smart close on its tab's menu, and cancelling asks the core", async () => {
    const { asked: log } = core();
    render(<App />);
    await openAChat();
    await step(1, "queued");
    await waitFor(() => expect(chipOf("steward 1")).toBeDefined());

    fireEvent.contextMenu(theChip("steward 1"));
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
    await waitFor(() => expect(chipOf("steward 1")).toBeDefined());

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

  /** The Notice a Smart close with no pass leaves on chat `session`'s tab (#1361). */
  const keptOpenNotice = (session: number) =>
    document.querySelector<HTMLElement>(`[data-cause="smart-close-kept-open:${session}"]`);

  it("stays open after a Smart close it held no pass for, says so, and Close tab closes it plainly", async () => {
    const { asked: log } = core();
    render(<App />);
    await openAChat();

    await step(1, "kept_open", PLANE, { path: "sessions/a.md", title: "A" });

    await waitFor(() => expect(keptOpenNotice(1)).not.toBeNull());
    const notice = keptOpenNotice(1) as HTMLElement;
    expect(notice).toHaveTextContent(
      "steward 1 saved its session record. purlis did not see " +
        "/smart-close typed in it, so its tab stayed open.",
    );
    expect(tabOf("steward 1")).toBeDefined();
    expect(asked(log, "close_session")).toEqual([]);

    await userEvent.click(within(notice).getByRole("button", { name: "Close tab" }));

    await waitFor(() => expect(tabOf("steward 1")).toBeUndefined());
    expect(asked(log, "close_session")).toEqual([{ plane: PLANE, session: 1 }]);
    expect(keptOpenNotice(1)).toBeNull();
  });

  it("puts that Notice away on Dismiss and closes nothing", async () => {
    const { asked: log } = core();
    render(<App />);
    await openAChat();
    await step(1, "kept_open", PLANE, { path: "sessions/a.md", title: "A" });
    await waitFor(() => expect(keptOpenNotice(1)).not.toBeNull());

    await userEvent.click(
      within(keptOpenNotice(1) as HTMLElement).getByRole("button", { name: /Dismiss/ }),
    );

    await waitFor(() => expect(keptOpenNotice(1)).toBeNull());
    expect(tabOf("steward 1")).toBeDefined();
    expect(asked(log, "close_session")).toEqual([]);
  });

  it("hears nothing about another project's chat of the same number", async () => {
    core();
    render(<App />);
    await openAChat();

    await step(1, "closed", "/somewhere/else");

    expect(tabOf("steward 1")).toBeDefined();
  });
});

/** One chat as the core reports it at a launch, named exactly `name` on its tab. */
function restored(session: number, name: string, { inFront = false, pinned = false } = {}) {
  return {
    session,
    name,
    cwd: `${PLANE}/workspaces/alpha`,
    harness: null,
    in_front: inFront,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
    guessed: null,
    pinned,
  };
}

/** The core with `open` put back at the launch, `one` pinned; Smart close answers `began`. */
function coreWith(
  open = [
    restored(1, "one", { pinned: true }),
    restored(2, "two", { inFront: true }),
    restored(3, "three"),
  ],
  began: () => unknown = () => "sent",
): { asked: { cmd: string; args: unknown }[] } {
  const asked: { cmd: string; args: unknown }[] = [];
  mockIPC(
    (cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "plane_sidebar")
        return { ...SIDEBAR, workspaces: [{ ...SIDEBAR.workspaces[0], chats: open }] };
      if (cmd === "opened_chats") return open;
      if (cmd === "running_sessions") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "smart_close_offer") return OFFERED;
      if (cmd === "smart_close") return began();
      if (cmd === "smart_closing") return [];
      return null;
    },
    { shouldMockEvents: true },
  );
  return { asked };
}

/** Smart close on the tab called `name`, answered through the close dialog. */
async function smartClose(name: string) {
  await userEvent.click(await screen.findByRole("button", { name: `End chat ${name}` }));
  const asking = await screen.findByRole("alertdialog");
  await userEvent.click(within(asking).getByRole("button", { name: "Smart close" }));
}

const selected = () =>
  within(stripNamed("Tabs"))
    .getAllByRole("tab")
    .find((tab) => tab.getAttribute("aria-selected") === "true");

describe("a chat put into the background (SI-8f)", () => {
  it("goes to the strip's left edge, before the pinned tabs", async () => {
    coreWith();
    render(<App />);
    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));

    await step(3, "sent");

    await waitFor(() => expect(strip()).toEqual(["three — wrapping up", "one", "two"]));
  });

  it("takes only a chip's room, so the strip still shows every tab it has room for", async () => {
    // jsdom lays nothing out (#149): the chat strip answers `clientWidth` with room for exactly
    // one chip and two tabs, and every other element none, which draws everything.
    const room =
      leastAt(LEAST_CHIP, DEFAULT_TEXT.window) + 2 * leastAt(LEAST.chat, DEFAULT_TEXT.window);
    const was = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
    Object.defineProperty(HTMLElement.prototype, "clientWidth", {
      configurable: true,
      get(this: HTMLElement) {
        return this.getAttribute("data-strip") === "Tabs" ? room : 0;
      },
    });
    try {
      coreWith();
      render(<App />);
      await waitFor(() => expect(strip()).toEqual(["one", "two"]));

      await step(2, "sent");

      await waitFor(() => expect(strip()).toEqual(["two — wrapping up", "one", "three"]));
    } finally {
      if (was) Object.defineProperty(HTMLElement.prototype, "clientWidth", was);
      else Reflect.deleteProperty(HTMLElement.prototype, "clientWidth");
    }
  });

  it("cannot be picked up and carried along the strip", async () => {
    laidOutInARow();
    const { asked: log } = coreWith();
    render(<App />);
    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));
    await step(3, "sent");
    await waitFor(() => expect(chipOf("three")).toBeDefined());

    const toldBefore = asked(log, "chat_order").length;

    theChip("three").focus();
    await dragWithTheKeyboard("{ArrowRight}");

    expect(strip()).toEqual(["three — wrapping up", "one", "two"]);
    expect(theChip("three")).not.toHaveAttribute("aria-describedby");
    expect(asked(log, "chat_order")).toHaveLength(toldBefore);
  });

  it("sends the front exactly where Close would have: the tab before it", async () => {
    coreWith();
    render(<App />);
    await waitFor(() => expect(selected()?.textContent).toContain("two"));

    await smartClose("two");

    await waitFor(() => expect(selected()?.querySelector(".tab-name")?.textContent).toBe("one"));
    await waitFor(() => expect(chipOf("two")).toBeDefined());
  });

  it("leaves the workspace's empty state in front when it was the last chat", async () => {
    coreWith([restored(1, "only", { inFront: true })]);
    render(<App />);
    await waitFor(() => expect(strip()).toEqual(["only"]));

    await smartClose("only");

    expect(await screen.findByTestId("empty-window")).toBeInTheDocument();
    expect(strip()).toEqual(["only — wrapping up"]);
  });

  it("shows the chat working when the chip is clicked, and stays a chip", async () => {
    coreWith();
    render(<App />);
    await smartClose("two");
    await step(2, "sent");
    await waitFor(() => expect(chipOf("two")).toBeDefined());

    await userEvent.click(theChip("two"));

    await waitFor(() => expect(theChip("two")).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByTestId("pane")).toHaveTextContent("session 2");
    expect(strip()).toEqual(["two — wrapping up", "one", "three"]);
  });

  it("goes when its record lands, and a quiet notice offers to open the record", async () => {
    coreWith();
    render(<App />);
    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));
    await step(2, "sent");
    const record = {
      path: "workspaces/alpha/sessions/20260928-160000-ship-it.md",
      title: "Ship it",
    };

    await step(2, "closed", PLANE, record);

    await waitFor(() => expect(chipOf("two")).toBeUndefined());
    const notice = (await screen.findByText(/^Session saved — Ship it/)).closest<HTMLElement>(
      '[role="status"]',
    );
    if (notice === null) throw new Error("the notice is not a status");
    expect(screen.queryByRole("alertdialog")).toBeNull();
    await userEvent.click(within(notice).getByRole("button", { name: "Open record" }));
    await waitFor(() => expect(tabOf("Session · Ship it")).toBeDefined());
    expect(screen.queryByText(/^Session saved — Ship it/)).toBeNull();
  });

  it("comes back in its old place when no record arrives, and the needs-you menu says why", async () => {
    coreWith();
    render(<App />);
    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));
    await step(2, "sent");
    await waitFor(() => expect(strip()).toEqual(["two — wrapping up", "one", "three"]));

    await step(2, "no_record");

    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));
    expect(
      screen.getByText(
        "No session record arrived from two within five minutes, so it was left open.",
      ),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "1 chat needs you" }));
    expect(
      await screen.findByRole("menuitem", {
        name: /^Go to two: Smart close stopped — no session record arrived/,
      }),
    ).toBeInTheDocument();
  });

  it("comes back in its old place when the chat ends before its record, listed too", async () => {
    coreWith();
    render(<App />);
    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));
    await step(1, "sent");
    await waitFor(() => expect(strip()[0]).toBe("one — wrapping up"));

    await step(1, "ended");

    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));
    await userEvent.click(screen.getByRole("button", { name: "1 chat needs you" }));
    expect(
      await screen.findByRole("menuitem", { name: /^Go to one: Smart close stopped — it ended/ }),
    ).toBeInTheDocument();
  });

  it("comes back in its old place when its queued prompt could not be sent, and says so", async () => {
    coreWith();
    render(<App />);
    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));
    await step(2, "queued");
    await waitFor(() => expect(strip()).toEqual(["two — wrapping up", "one", "three"]));

    await step(2, "not_sent");

    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));
    expect(
      screen.getByText("Smart close could not send two its prompt, so it was left open."),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "1 chat needs you" }));
    expect(
      await screen.findByRole("menuitem", {
        name: /^Go to two: Smart close stopped — its prompt could not be sent/,
      }),
    ).toBeInTheDocument();
  });

  it("comes back in its old place when Smart close could not start, listed too", async () => {
    coreWith(undefined, () => {
      throw "That chat is not open any more.";
    });
    render(<App />);
    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));

    await smartClose("two");

    expect(await screen.findByText("That chat is not open any more.")).toBeInTheDocument();
    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));
    await userEvent.click(screen.getByRole("button", { name: "1 chat needs you" }));
    expect(
      await screen.findByRole("menuitem", { name: /^Go to two: Smart close did not start/ }),
    ).toBeInTheDocument();
  });

  it("comes back in its old place when cancelled, and nothing is listed", async () => {
    coreWith();
    render(<App />);
    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));
    await step(3, "sent");
    await waitFor(() => expect(strip()[0]).toBe("three — wrapping up"));

    await step(3, "cancelled");

    await waitFor(() => expect(strip()).toEqual(["one", "two", "three"]));
    expect(screen.queryByRole("button", { name: /needs you/ })).toBeNull();
  });
});
