import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

// The pane's own terminal is driven by the scenario tests, against the real app. Here it
// stands in for one, so these tests are about the tabs, the splits and what they ask the core.
vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

/** The app as `main.tsx` renders it: in StrictMode, which runs effects and state updates
 *  twice, so anything that opens or ends a session twice shows up here. */
const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
});

/** What the core answers when the sidebar reads the plane. These tests are about the tabs,
 *  the splits and what they ask the core, so it is the smallest plane that draws: one
 *  workspace, nothing in it. `Sidebar.test.tsx` is where the sidebar itself is tested. */
const SIDEBAR = {
  root: "/home/dev/plane",
  workspaces: [
    {
      name: "alpha",
      path: "/home/dev/plane/workspaces/alpha",
      vision: "Ship it",
      todos: [],
      chats: [],
    },
  ],
  personas: ["steward"],
  persona: "steward",
  unfiled: [],
};

/** What the picker draws. One profile, so picking is one click — which is also the shape
 *  ADR 0022 insists on showing rather than skipping: a one-harness machine still picks. */
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

/** Opens a chat the way the operator does now: New tab, then a row, then Start.
 *
 *  A harness starts only once a row is picked (ADR 0022) — the dialog shows even when one
 *  profile is available, because skipping it would bring back the harness nobody picked on
 *  a one-harness machine. Every test that wants a session goes through here, which is also
 *  what keeps that rule from being quietly removed: take the picker out of the path and
 *  every one of these fails.
 */
async function openAChat() {
  await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
  await userEvent.click(await screen.findByRole("button", { name: "Start" }));
}

/** Splits the pane in front. A split starts a harness too, so it picks as well — there is
 *  no path in this app by which a chat starts on a profile nobody chose. */
async function splitInto(which: "Split right" | "Split down") {
  await userEvent.click(screen.getByRole("button", { name: which }));
  await userEvent.click(await screen.findByRole("button", { name: "Start" }));
}

/**
 * Ends a chat the way the operator does now: press, then answer.
 *
 * **Every route to ending one asks first** (`EndingChat.tsx`, the operator's *"closing
 * session should ask confirmation"*) — a tab's `×`, a pane's `×` and the palette's rows all
 * go through one place. So every test that ends a chat comes through here, which is also
 * what keeps the rule from being quietly removed: take the question out and every one of
 * these fails with a chat that ended without being asked about.
 *
 * `name` is the row's own words, which are the dialog's too, so this presses the answer that
 * belongs to the chat under test rather than whichever button happens to be second.
 */
async function endChat(name: string) {
  await userEvent.click(screen.getByRole("button", { name }));
  const asking = await screen.findByRole("alertdialog");
  await userEvent.click(within(asking).getByRole("button", { name }));
}

/**
 * The controls in one pane's corner, found by the session that pane is showing.
 *
 * **There is one set per pane now**, which is the operator's whole point: `Split right` and
 * `End this pane's chat` used to be single buttons on the bar acting on whichever pane was
 * focused, and a window split four ways gave no sign of which that was. So a test that says
 * "press Split right" no longer names a target, and this is what names one.
 */
function paneDoing(session: number) {
  const holder = screen.getByText(`session ${session}`).closest(".pane-holder");
  if (!holder) throw new Error(`no pane is showing session ${session}`);
  return within(holder as HTMLElement);
}

/** Answers every command the app sends, and records what it was asked. */
function core(): { asked: { cmd: string; args: unknown }[] } {
  const asked: { cmd: string; args: unknown }[] = [];
  let opened = 0;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "plane_at_launch")
      return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "running_sessions") return [];
    if (cmd === "opened_chats") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "open_session") return ++opened;
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "start_chat") return { session: ++opened, wired: null };
    return null;
  });
  return { asked };
}

const panes = () => screen.getAllByTestId("pane").map((pane) => pane.textContent);
// Scoped to the tab strip: the sidebar lists workspaces as a tablist too, so a query for
// `role="tab"` across the whole window mixes a workspace in among the tabs.
const tabs = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .getAllByRole("tab")
    // The NAME a tab carries, not everything drawn in it: a tab also says what its chat is
    // doing, and these tests are about which tabs exist.
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

describe("App", () => {
  it("shows the plane the core found", async () => {
    mockIPC((cmd) => {
      if (cmd === "plane_at_launch")
        return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
      if (cmd === "plane_sidebar") return SIDEBAR;
      return [];
    });

    render(<App />);

    expect(await screen.findByText("/home/dev/plane")).toBeInTheDocument();
  });

  it("says why there is no project here, in the resolver's own words", async () => {
    // A launch that HAD a directory and found no project in it. The operator asked a
    // question by running charter there, so they get the answer — and they get the opener
    // under it, because "no plane" used to be the whole of what this window could say.
    mockIPC(() => {
      throw new Error("no charter.toml in /tmp or any directory above it");
    });

    render(<App />);

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "charter found no project here",
    );
    expect(screen.getByText(/no charter.toml in \/tmp/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open Project…" })).toBeInTheDocument();
  });

  it("does not open with an error when the launch had nothing to go on at all", async () => {
    // A `.app` double-clicked from the dock has `/` for a working directory, so the core
    // opens holding nothing. That is a newcomer's first screen and it must not be an error
    // about a concept they do not have yet — which is exactly what the window used to draw.
    mockIPC((cmd) => {
      if (cmd === "plane_at_launch") return { plane: null, from: null, why: null };
      if (cmd === "recent_planes") return { planes: [], dropped: [], forgetful: null };
      return null;
    });

    render(<App />);

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "You have not opened a project yet",
    );
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("tells the core when its first frame is on screen", async () => {
    // Where cold start ends. The core prints it only when the app was started to be measured.
    const { asked } = core();

    render(<App />);

    await vi.waitFor(() => expect(asked.map((one) => one.cmd)).toContain("first_frame"));
  });

  it("says why a launch took longer than the limit, because nothing else could have", async () => {
    // charter-app#24: on a Linux session whose desktop portal cannot start, the app is not
    // on screen for half a minute with no window and no icon to say why. The core writes a
    // line to standard error while it waits; an operator who clicked an icon never sees it,
    // so the first frame is where they are told.
    mockIPC((cmd) => {
      if (cmd === "first_frame") return "charter took 31 s to start, against a 2 s limit.";
      if (cmd === "plane_at_launch")
        return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
      if (cmd === "plane_sidebar") return SIDEBAR;
      if (cmd === "chats_that_would_not_start") return [];
      return null;
    });

    render(<App />);

    const said = await screen.findByRole("status");
    expect(said).toHaveTextContent("charter took 31 s to start, against a 2 s limit.");
  });

  it("says nothing about an ordinary launch", async () => {
    // The core answers with nothing when the launch was inside the limit, and a window that
    // drew an empty notice on every start would be noise on every start.
    core();

    render(<App />);

    expect(await screen.findByText("/home/dev/plane")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("puts that notice away when it is dismissed", async () => {
    // The launch is over by the time it is read, and the news does not improve.
    mockIPC((cmd) => {
      if (cmd === "first_frame") return "charter took 31 s to start, against a 2 s limit.";
      if (cmd === "plane_at_launch")
        return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
      if (cmd === "plane_sidebar") return SIDEBAR;
      if (cmd === "chats_that_would_not_start") return [];
      return null;
    });
    render(<App />);
    await screen.findByRole("status");

    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("does not send that marker once the window has gone", async () => {
    // It is sent a frame after the window paints, which can be after the window is gone —
    // and then it reaches whatever the next test, or the next window, has put there.
    const { asked } = core();

    const { unmount } = render(<App />);
    unmount();
    await new Promise((done) => setTimeout(done, 100));

    expect(asked.map((one) => one.cmd)).not.toContain("first_frame");
  });

  it("stays usable when the core cannot take that marker", async () => {
    // It is instrumentation: a window whose marker is refused is still a window, and an
    // unhandled rejection is not how it says so.
    mockIPC((cmd) => {
      if (cmd === "first_frame") throw new Error("no");
      if (cmd === "plane_at_launch")
        return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
      if (cmd === "plane_sidebar") return SIDEBAR;
      return null;
    });

    render(<App />);

    expect(await screen.findByText("/home/dev/plane")).toBeInTheDocument();
    expect(await screen.findByText(/No sessions/)).toBeInTheDocument();
  });

  // What the window does with sessions the core is already holding is in
  // `lifecycle.test.tsx`: it draws them as tabs. It used to end them, which a relaunch and a
  // reload both now depend on it not doing.

  it("opens a session in a new tab", async () => {
    const { asked } = core();
    render(<App />);

    await openAChat();

    expect(await screen.findByTestId("pane")).toHaveTextContent("session 1");
    expect(tabs()).toEqual(["1 steward"]);
    // `start_chat` and not `open_session`: a chat now starts on the profile that was
    // picked, and the command that opens a bare shell is not in this path at all.
    const started = asked.find(({ cmd }) => cmd === "start_chat");
    expect(started?.args).toMatchObject({ profile: "claude", persona: "steward" });
  });

  it("splits the pane in front into two, each with its own session", async () => {
    core();
    render(<App />);
    await openAChat();

    await splitInto("Split right");

    expect(panes()).toEqual(["session 1", "session 2"]);
  });

  it("shows only the panes of the tab in front", async () => {
    core();
    render(<App />);
    await openAChat();

    await openAChat();

    expect(tabs()).toEqual(["1 steward", "2 steward"]);
    expect(panes()).toEqual(["session 2"]);
  });

  it("brings a tab back to the front when it is chosen", async () => {
    core();
    render(<App />);
    await openAChat();
    await openAChat();

    await userEvent.click(
      within(screen.getByRole("tablist", { name: "Tabs" })).getAllByRole("tab")[0],
    );

    expect(panes()).toEqual(["session 1"]);
  });

  it("ends the sessions of a tab that closes", async () => {
    const { asked } = core();
    render(<App />);
    await openAChat();
    await splitInto("Split right");

    await endChat("End chat 1 steward");

    expect(screen.queryAllByTestId("pane")).toEqual([]);
    // The plane travels with the session, because a session number alone names a chat in
    // every plane the process holds.
    expect(asked.filter(({ cmd }) => cmd === "close_session").map(({ args }) => args)).toEqual([
      { plane: "/home/dev/plane", session: 1 },
      { plane: "/home/dev/plane", session: 2 },
    ]);
  });

  it("ends only the session of a pane that closes", async () => {
    const { asked } = core();
    render(<App />);
    await openAChat();
    await splitInto("Split down");

    await userEvent.click(paneDoing(2).getByRole("button", { name: "End this pane's chat" }));
    const asking = await screen.findByRole("alertdialog");
    await userEvent.click(within(asking).getByRole("button", { name: "End this pane's chat" }));

    expect(panes()).toEqual(["session 1"]);
    expect(asked.filter(({ cmd }) => cmd === "close_session").map(({ args }) => args)).toEqual([
      { plane: "/home/dev/plane", session: 2 },
    ]);
  });

  it("ends the chat of the pane the button is ON, not the focused one", async () => {
    // **The operator's reason for moving these onto the panes**, as a test: *"this will be
    // clear for spliting — user will know what pane is spliting."* A split leaves the NEW
    // pane focused, so pressing the older pane's own `×` is precisely the case a bar button
    // gets wrong — it would end session 2 and leave the pane the operator aimed at.
    const { asked } = core();
    render(<App />);
    await openAChat();
    await splitInto("Split right");

    await userEvent.click(paneDoing(1).getByRole("button", { name: "End this pane's chat" }));
    const asking = await screen.findByRole("alertdialog");
    await userEvent.click(within(asking).getByRole("button", { name: "End this pane's chat" }));

    expect(panes()).toEqual(["session 2"]);
    expect(asked.filter(({ cmd }) => cmd === "close_session").map(({ args }) => args)).toEqual([
      { plane: "/home/dev/plane", session: 1 },
    ]);
  });

  it("splits the pane the button is ON, not the focused one", async () => {
    // The same rule for the other two controls. After one split the second pane is focused;
    // splitting from the FIRST pane's button has to divide the first pane, which is what an
    // operator aiming at it means and what the bar's button could not express.
    core();
    render(<App />);
    await openAChat();
    await splitInto("Split right");

    await userEvent.click(paneDoing(1).getByRole("button", { name: "Split down" }));
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));

    // Session 3 is beside session 1 and not beside session 2: the pane that was pressed is
    // the one that divided. Read off the DOM order, which is the layout's own order.
    expect(panes()).toEqual(["session 1", "session 3", "session 2"]);
  });

  it("asks before it ends a chat, and ends nothing if the answer is no", async () => {
    // The operator's *"closing session should ask confirmation"*. The half worth testing is
    // the cancel: a dialog that ends the chat whichever button is pressed is worse than no
    // dialog, because it teaches the operator that the question is a formality.
    const { asked } = core();
    render(<App />);
    await openAChat();

    await userEvent.click(screen.getByRole("button", { name: "End chat 1 steward" }));
    const asking = await screen.findByRole("alertdialog");
    await userEvent.click(within(asking).getByRole("button", { name: "Cancel" }));

    expect(panes()).toEqual(["session 1"]);
    expect(asked.filter(({ cmd }) => cmd === "close_session")).toEqual([]);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("asks with two answers, Cancel focused and the confirm at the other edge", async () => {
    // **The shape the keyboard route depends on, pinned because the route is not obvious.**
    //
    // Radix's `FocusScope` intercepts Tab only at the EDGES of the scope
    // (`@radix-ui/react-focus-scope`, `handleKeyDown`): on the FIRST tabbable it acts on
    // Shift+Tab and moves the focus to the last itself; on the LAST it acts on Tab and moves
    // to the first. In between it does nothing and the platform decides — and a WKWebView on
    // macOS does not put a `<button>` in the tab sequence unless Full Keyboard Access is on,
    // which is what sent `palette.e2e.ts` back red pressing plain Tab (charter-app#176).
    //
    // So the confirm is reachable by keyboard on every platform exactly while these two are
    // the only tabbables and the confirm is the second: then Cancel IS the first edge and the
    // confirm IS the last, and Shift+Tab is Radix's own `focus()` call rather than the
    // browser's tab sequence. **A third focusable added between them would take that away
    // silently on one platform**, and this is what fails instead.
    core();
    render(<App />);
    await openAChat();

    await userEvent.click(screen.getByRole("button", { name: "End chat 1 steward" }));
    const asking = await screen.findByRole("alertdialog");

    // Read off the DOM rather than asked for by name: "the only two, in this order" is the
    // claim, and `getByRole` for each would pass with a third between them.
    const answers = within(asking).getAllByRole("button");
    expect(answers.map((answer) => answer.textContent)).toEqual(["Cancel", "End chat 1 steward"]);
    // Cancel first, so a Return pressed by reflex cancels. The dialog itself is not in the
    // sequence — Radix gives the content `tabIndex={-1}`.
    expect(answers[0]).toHaveFocus();

    await userEvent.tab({ shift: true });

    expect(answers[1]).toHaveFocus();
  });

  it("answers the question with Escape, and Escape means no", async () => {
    // `docs/ui-primitives.md`'s rule for every modal in this window: Escape answers, with the
    // NON-destructive answer. It matters most on this one, because this is the only modal
    // that appears without being asked for — a keyboard user's reflex must not end a chat.
    const { asked } = core();
    render(<App />);
    await openAChat();

    await userEvent.click(screen.getByRole("button", { name: "End chat 1 steward" }));
    await screen.findByRole("alertdialog");
    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(panes()).toEqual(["session 1"]);
    expect(asked.filter(({ cmd }) => cmd === "close_session")).toEqual([]);
  });

  it("ends a session it opened for a split whose tab closed while it was starting", async () => {
    // Starting a session is a real round trip: the tab can be gone by the time it answers,
    // and then nothing would ever show that session.
    const asked: { cmd: string; args: unknown }[] = [];
    let letTheSecondSessionStart = () => {};
    let opened = 0;
    mockIPC(async (cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "plane_at_launch")
        return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
      if (cmd === "plane_sidebar") return SIDEBAR;
      if (cmd === "running_sessions") return [];
      if (cmd === "opened_chats") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "start_options") return START_OPTIONS;
      if (cmd !== "start_chat") return null;
      if (++opened === 1) return { session: 1, wired: null };
      await new Promise<void>((starts) => (letTheSecondSessionStart = starts));
      return { session: 2, wired: null };
    });
    render(<App />);
    await openAChat();

    await splitInto("Split right");
    // The picker is still up while the split's harness starts, and it is modal: the tab strip
    // behind it is inert and out of the accessibility tree, so an operator reaches the tab's
    // `×` the only way there is — by leaving the picker first. Escape does not call the start
    // back; it is already in flight, which is exactly the race this test is about.
    await userEvent.keyboard("{Escape}");
    await endChat("End chat 1 steward");
    letTheSecondSessionStart();

    await vi.waitFor(() =>
      expect(asked.filter(({ cmd }) => cmd === "close_session").map(({ args }) => args)).toEqual([
        { plane: "/home/dev/plane", session: 1 },
        { plane: "/home/dev/plane", session: 2 },
      ]),
    );
    expect(screen.queryAllByTestId("pane")).toEqual([]);
  });

  it("says why a chat would not start, in the picker, and opens no tab", async () => {
    // Beside the rows and not behind them: the operator is still choosing, and a refusal
    // they cannot see next to what they picked is one they cannot act on. Every refusal a
    // launch has arrives this way — an unwired profile, a kind v1 does not start, a file
    // git would carry.
    mockIPC((cmd) => {
      if (cmd === "plane_at_launch")
        return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
      if (cmd === "plane_sidebar") return SIDEBAR;
      if (cmd === "running_sessions") return [];
      if (cmd === "opened_chats") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "start_options") return START_OPTIONS;
      // The panels ask the core too, and this test is about the picker: a refusal from them
      // is a second alert, which is not the one being asserted on.
      if (cmd === "workspace_panels" || cmd === "workspace_repos") return null;
      throw new Error("profile 'claude' is not wired — charter@charter is not installed");
    });
    render(<App />);

    await openAChat();

    expect(await screen.findByRole("alert")).toHaveTextContent("is not wired");
    expect(screen.queryAllByTestId("pane")).toEqual([]);
    // Still open, so the operator can pick another row without starting over.
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});
