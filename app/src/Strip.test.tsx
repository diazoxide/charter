import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { LEAST, leastAt } from "./fits";
import { DEFAULT_TEXT } from "./textSize";
import type { Moved, OpenChat } from "./bindings";

/**
 * The chat strip when it holds more than it has room for (ADR 0039, as amended).
 *
 * Two rules, and they are opposites on purpose:
 *
 * - **the strip's order never changes**, because a tab that moves under the cursor breaks
 *   aiming, and
 * - **the show-more menu is sorted by last activity**, because it is a list you read rather
 *   than a surface you aim at.
 *
 * And one that replaces the scroller: **what the strip has no room for is not drawn**, so the
 * menu is the pointer route to it. The record's own constraint — charter-app#130's fifty tabs
 * with no way to reach the last of them — is what "reaches a hidden tab, and closes it" below
 * is for, and it is the test that would go red if the collapse made a tab unreachable.
 *
 * **jsdom lays nothing out** (#149), so what is measured is stubbed here rather than pretended
 * at: `clientWidth` answers what a test set, and a `ResizeObserver` of this file's own tells
 * the app when it changed. What the real browser answers is `panes.e2e.ts`'s question, and it
 * is the only place it can be asked.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

/** The wire to what the strips measure. */
type Room = {
  /** Gives the chat strip room for exactly `tabs` tabs, and tells the app its size changed. */
  roomFor: (tabs: number) => void;
  /** How many live observers are watching the chat strip itself. Scoped to it deliberately:
   *  the resizable panels and Radix's own positioning each put observers up, so a count of
   *  every observer in the window would measure somebody else's library. */
  live: () => number;
  /** Puts `clientWidth` back, so one test's stub is not every later test's. */
  stop: () => void;
};

/**
 * Stubs the one number the strips measure, and the observer that reports changes to it.
 *
 * It answers per element, which is what the real one does — the window has three strips and
 * each measures itself. Only the chat strip is given a width here; the other two measure zero,
 * which is "nobody has said", and draw everything.
 */
function measuring(): Room {
  const widths = new WeakMap<Element, number>();
  const live: { callback: ResizeObserverCallback; targets: Element[]; gone: boolean }[] = [];
  const clientWidth = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get(this: HTMLElement) {
      return widths.get(this) ?? 0;
    },
  });
  const was = globalThis.ResizeObserver;
  globalThis.ResizeObserver = class {
    private readonly mine: (typeof live)[number];
    constructor(callback: ResizeObserverCallback) {
      this.mine = { callback, targets: [], gone: false };
      live.push(this.mine);
    }
    observe(target: Element) {
      this.mine.targets.push(target);
    }
    unobserve() {}
    disconnect() {
      this.mine.gone = true;
    }
  } as unknown as typeof ResizeObserver;
  return {
    roomFor: (tabs) => {
      const strip = screen.getByRole("tablist", { name: "Tabs" });
      // The floor the strip fits by at the default window text size (charter-app#283).
      widths.set(strip, tabs * leastAt(LEAST.chat, DEFAULT_TEXT.window));
      for (const one of live) {
        if (one.gone || !one.targets.includes(strip)) continue;
        one.callback([], undefined as unknown as ResizeObserver);
      }
    },
    live: () => {
      const strip = screen.getByRole("tablist", { name: "Tabs" });
      return live.filter((one) => !one.gone && one.targets.includes(strip)).length;
    },
    stop: () => {
      if (clientWidth) Object.defineProperty(HTMLElement.prototype, "clientWidth", clientWidth);
      else Reflect.deleteProperty(HTMLElement.prototype, "clientWidth");
      globalThis.ResizeObserver = was;
    },
  };
}

/** A chat the core says it has open, with everything not under test left plain. */
function chat(session: number): OpenChat {
  return {
    session,
    name: `ide.${session}`,
    cwd: "/home/dev/plane/workspaces/ide",
    // No harness and no persona, so its tab is its own name alone and the assertions below read
    // the names they gave it (the default before the name is charter-app#254's, tested there).
    harness: null,
    in_front: session === 1,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
    pinned: false,
    label: null,
  };
}

/** The core, holding `open` chats and nothing else. There is no sidebar, so every chat is on
 *  one strip — which is what a window that has not read the plane yet draws (ADR 0036). */
function core(open: OpenChat[]): { move: (moved: Moved) => void; ended: number[] } {
  const listeners = new Map<string, number>();
  const ended: number[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, handler);
      return 1;
    }
    if (cmd === "plane_at_launch")
      return { plane: "/home/dev/plane", from: "/home/dev/plane", why: null };
    if (cmd === "opened_chats") return open;
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "close_session") {
      ended.push((args as { session: number }).session);
      return null;
    }
    return null;
  });
  return {
    ended,
    move: (moved: Moved) => {
      const handler = listeners.get("chat-moved");
      if (handler === undefined) throw new Error("the window is not listening for moves");
      window.__TAURI_INTERNALS__.runCallback(handler, {
        event: "chat-moved",
        id: 1,
        payload: moved,
      });
    },
  };
}

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

/** One move, as the core pushes it. `at` is the board's count — bigger is more recent. */
function moving(session: number, at: number): Moved {
  return {
    plane: "/home/dev/plane",
    session,
    state: "waiting",
    needs_you: false,
    queue: [],
    moved_at: at,
    // The board numbers a snapshot at least as late as the move it reports.
    sequence: at,
  };
}

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const tabNames = () =>
  within(strip())
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

/** What the show-more menu lists, top to bottom. */
const menuNames = () =>
  screen.getAllByRole("menuitem").map((row) => row.querySelector(".tab-name")?.textContent);

/** Every close button the strip is drawing, left to right. */
const closers = () => within(strip()).queryAllByRole("button", { name: /^End chat / });

/**
 * Ends the chat of the tab `closer` belongs to: press, then answer.
 *
 * **Ending a chat asks first** (`EndingChat.tsx`, the operator's *"closing session should ask
 * confirmation"*), from every surface that can ask for it. The dialog's own answer carries the
 * row's words, so this reads the button it is about to press rather than assuming which chat
 * the question is about.
 */
async function endChat(closer: HTMLElement): Promise<void> {
  const name = closer.getAttribute("aria-label") ?? "";
  await userEvent.click(closer);
  const asking = await screen.findByRole("alertdialog");
  await userEvent.click(within(asking).getByRole("button", { name }));
}

describe("the chat strip when it holds more than it has room for", () => {
  let room: Room;

  beforeEach(() => {
    room = measuring();
  });

  afterEach(() => {
    room.stop();
    cleanup();
    clearMocks();
  });

  /** Four chats open, with the first in front, and nothing measured yet. */
  async function fourChats() {
    const made = core([chat(1), chat(2), chat(3), chat(4)]);
    render(<App />);
    await vi.waitFor(() => expect(tabNames()).toEqual(["ide.1", "ide.2", "ide.3", "ide.4"]));
    return made;
  }

  it("draws every tab while nobody has said how wide the strip is", async () => {
    // A width of zero is the absence of a measurement, not a strip with no room — and every
    // environment with no layout answers zero. Drawing nothing there would be a window with
    // no tabs in it.
    await fourChats();

    expect(tabNames()).toEqual(["ide.1", "ide.2", "ide.3", "ide.4"]);
    expect(screen.queryByRole("button", { name: /the strip is not showing/ })).toBeNull();
  });

  it("says nothing while every tab fits", async () => {
    await fourChats();

    room.roomFor(4);

    // No button, not a disabled one: the whole point is that it is the first thing on the
    // strip that SAYS there are more, so it must not be there when there are not.
    await vi.waitFor(() => expect(tabNames()).toHaveLength(4));
    expect(screen.queryByRole("button", { name: /the strip is not showing/ })).toBeNull();
  });

  it("draws only what fits, and says how many it did not", async () => {
    await fourChats();

    room.roomFor(2);

    const more = await screen.findByRole("button", {
      name: "Show 2 tabs the strip is not showing",
    });
    expect(more).toHaveTextContent("2 more");
    expect(tabNames()).toEqual(["ide.1", "ide.2"]);
  });

  it("counts one hidden tab in the singular", async () => {
    await fourChats();

    room.roomFor(3);

    expect(
      await screen.findByRole("button", { name: "Show 1 tab the strip is not showing" }),
    ).toBeInTheDocument();
  });

  it("lists the hidden tabs most recently moved first", async () => {
    const { move } = await fourChats();
    // Chat 3 moved, then 4, then 2. The strip holds them in the order they opened.
    move(moving(3, 10));
    move(moving(4, 11));
    move(moving(2, 12));

    room.roomFor(1);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" }),
    );

    expect(menuNames()).toEqual(["ide.2", "ide.4", "ide.3"]);
  });

  it("leaves the strip in its own order while the menu re-sorts", async () => {
    // **The two rules at once, which is the whole record.** The same activity that puts a
    // chat at the top of the menu must not move its tab: an operator going back to the chat
    // that was third from the left goes there with their hand, not by reading.
    const { move } = await fourChats();

    move(moving(4, 9));
    move(moving(3, 10));
    room.roomFor(2);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 2 tabs the strip is not showing" }),
    );

    expect(menuNames()).toEqual(["ide.3", "ide.4"]);
    expect(tabNames()).toEqual(["ide.1", "ide.2"]);
  });

  it("keeps the strip's order in the menu for the chats nothing has been heard about", async () => {
    // At a launch nothing has moved, so every tab ties. A menu whose rows moved between two
    // openings for no reason the operator can see is the aiming defect in the one surface
    // that was allowed to sort.
    await fourChats();

    room.roomFor(1);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" }),
    );

    expect(menuNames()).toEqual(["ide.2", "ide.3", "ide.4"]);
  });

  it("draws a tab picked from the menu, with its own close button", async () => {
    // **This is what the collapse owes**, and what the scroller used to owe with
    // `scrollIntoView`: a tab reached through the menu arrives ON the strip, so ending it is
    // still two presses and never one from a menu under the cursor (charter-app#130).
    await fourChats();
    room.roomFor(1);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" }),
    );

    await userEvent.click(screen.getByRole("menuitem", { name: /ide\.3/ }));

    await vi.waitFor(() => expect(screen.getAllByTestId("pane")[0]).toHaveTextContent("session 3"));
    expect(tabNames()).toEqual(["ide.3"]);
    expect(within(strip()).getByRole("button", { name: "End chat ide.3" })).toBeInTheDocument();
  });

  it("reaches and ends every chat with room for only two of four", async () => {
    // **The fifty-tab question, at four.** `stress.e2e.ts` closes every tab by pressing the
    // last close button on the strip over and over; with the strip collapsing rather than
    // scrolling, that only terminates because closing a drawn tab gives the strip room for a
    // hidden one. If it ever does not, this goes red here rather than as a four-minute
    // scenario run that ends with sessions still alive.
    const { ended } = await fourChats();
    room.roomFor(2);
    await vi.waitFor(() => expect(closers()).toHaveLength(2));

    for (let pressed = 0; pressed < 16 && closers().length > 0; pressed++) {
      const drawn = closers();
      await endChat(drawn[drawn.length - 1]);
    }

    expect([...ended].sort()).toEqual([1, 2, 3, 4]);
    expect(screen.queryByRole("button", { name: /the strip is not showing/ })).toBeNull();
  });

  it("moves between its rows with the arrow keys", async () => {
    // **The first thing built under ADR 0037, so it is the test of it.** The picker's radio
    // group does NOT follow the arrow keys under React 19 on its own — Radix learns that an
    // arrow is down from a `keydown` listener on `document`, and React's delegated listeners
    // sit below `document`, so focus has already moved by the time Radix hears it (#137).
    // A menu's roving focus is its own `onKeyDown`, on the content element, and this is what
    // says so rather than an assumption that the two primitives are alike.
    await fourChats();
    room.roomFor(1);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" }),
    );

    await userEvent.keyboard("{ArrowDown}");

    expect(screen.getAllByRole("menuitem")[0]).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getAllByRole("menuitem")[1]).toHaveFocus();
  });

  it("brings a tab forward when Enter is pressed on its row", async () => {
    await fourChats();
    room.roomFor(1);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" }),
    );

    await userEvent.keyboard("{ArrowDown}{ArrowDown}{Enter}");

    // The second row: the strip's order, since nothing has moved.
    await vi.waitFor(() => expect(screen.getAllByTestId("pane")[0]).toHaveTextContent("session 3"));
  });

  it("offers nothing that ends a chat", async () => {
    // A menu that pops up under the cursor with `End chat` in it is charter-app#130's defect
    // with a mouse attached. Every row here brings a tab forward and nothing else.
    await fourChats();
    room.roomFor(1);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" }),
    );

    for (const row of screen.getAllByRole("menuitem")) {
      expect(row.textContent ?? "").not.toContain("End chat");
    }
  });

  it("keeps New tab out of the strip that collapses, and out of the menu", async () => {
    // Measured in charter-app#130: `panes.e2e` could not press New tab partway through
    // opening fifty chats, because the button was the strip's last child. A collapse is the
    // same hazard in different clothes — the control that is wanted exactly when the strip is
    // full must not be inside the thing that hides what does not fit.
    await fourChats();

    room.roomFor(1);

    const adding = await screen.findByRole("button", { name: "New tab" });
    expect(strip()).not.toContainElement(adding);
    expect(adding.closest(".more")).toBeNull();
  });

  it("drops a tab out of the menu when its chat ends", async () => {
    await fourChats();
    room.roomFor(1);
    await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" });

    await endChat(within(strip()).getByRole("button", { name: "End chat ide.1" }));

    expect(
      await screen.findByRole("button", { name: "Show 2 tabs the strip is not showing" }),
    ).toBeInTheDocument();
  });

  it("does not leave an observer behind for every render", async () => {
    // One observer per strip, replaced when the element it watches changes. A hook that
    // rebuilt on every render would pile them up and measure the same strip many times over.
    await fourChats();
    expect(room.live()).toBe(1);

    room.roomFor(1);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" }),
    );

    expect(room.live()).toBe(1);
  });
});
