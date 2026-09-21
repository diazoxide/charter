import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { Moved, OpenChat } from "./bindings";

/**
 * The chat strip when it holds more than it can show (charter ADR 0039).
 *
 * Two rules, and they are opposites on purpose:
 *
 * - **the strip's order never changes**, because a tab that moves under the cursor breaks
 *   aiming, and
 * - **the show-more menu is sorted by last activity**, because it is a list you read rather
 *   than a surface you aim at.
 *
 * What does not fit is a measurement — of the strip's width, the tabs in it and where it is
 * scrolled to — so these drive a real `IntersectionObserver` of their own rather than
 * pretending jsdom lays anything out. jsdom gives every element a zero-sized box, so the
 * browser's own observer would answer nothing here, and the app's answer is the one under
 * test rather than the measurement.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

/** Every observer this test made, so it can say what is on screen and what is not. */
type Watching = {
  /** Tells the app that exactly these tabs are wholly visible and the rest are not. */
  onlyShowing: (ids: readonly number[]) => void;
  /** How many observers are live: a strip that rebuilt one per render would show here. */
  live: () => number;
};

/**
 * Installs an `IntersectionObserver` jsdom does not have, and hands back the wire to it.
 *
 * It answers per target, which is what the real one does: a callback carries only the
 * targets whose visibility CHANGED, and code that assumed it carried all of them would pass
 * a test that fired one entry per tab every time.
 */
function watchingTabs(): Watching {
  type Live = {
    callback: IntersectionObserverCallback;
    targets: Element[];
    disconnected: boolean;
  };
  const all: Live[] = [];
  globalThis.IntersectionObserver = class {
    private readonly live: Live;
    constructor(callback: IntersectionObserverCallback) {
      this.live = { callback, targets: [], disconnected: false };
      all.push(this.live);
    }
    observe(target: Element) {
      this.live.targets.push(target);
    }
    unobserve() {}
    disconnect() {
      this.live.disconnected = true;
    }
    takeRecords(): IntersectionObserverEntry[] {
      return [];
    }
    readonly root = null;
    readonly rootMargin = "";
    readonly thresholds: readonly number[] = [];
  } as unknown as typeof IntersectionObserver;
  return {
    onlyShowing: (ids) => {
      for (const live of all) {
        if (live.disconnected) continue;
        // **Right to left, deliberately.** A real observer delivers entries in whatever
        // order the engine noticed them, and delivering them in the strip's order would let
        // code that simply kept the order it was handed pass every test below.
        const entries = [...live.targets].reverse().map((target) => ({
          target,
          isIntersecting: ids.includes(Number(target.getAttribute("data-tab"))),
        }));
        live.callback(
          entries as unknown as IntersectionObserverEntry[],
          undefined as unknown as IntersectionObserver,
        );
      }
    },
    live: () => all.filter((one) => !one.disconnected).length,
  };
}

/** A chat the core says it has open, with everything not under test left plain. */
function chat(session: number): OpenChat {
  return {
    session,
    name: `ide.${session}`,
    cwd: "/home/dev/plane/workspaces/ide",
    harness: "claude",
    in_front: session === 1,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
  };
}

/** The core, holding `open` chats and nothing else. There is no sidebar, so every chat is on
 *  one strip — which is what a window that has not read the plane yet draws (ADR 0036). */
function core(open: OpenChat[]): { move: (moved: Moved) => void } {
  const listeners = new Map<string, number>();
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
    return null;
  });
  return {
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

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("the chat strip when it holds more than it shows", () => {
  let watching: Watching;

  beforeEach(() => {
    watching = watchingTabs();
  });

  /** Four chats open, with the first in front, and nothing measured yet. */
  async function fourChats() {
    const { move } = core([chat(1), chat(2), chat(3), chat(4)]);
    render(<App />);
    await vi.waitFor(() => expect(tabNames()).toEqual(["ide.1", "ide.2", "ide.3", "ide.4"]));
    return { move };
  }

  it("says nothing while every tab is on screen", async () => {
    await fourChats();

    watching.onlyShowing([1, 2, 3, 4]);

    // No button, not a disabled one: the whole point is that it is the first thing on the
    // strip that SAYS there are more, so it must not be there when there are not.
    expect(screen.queryByRole("button", { name: /the strip is not showing/ })).toBeNull();
  });

  it("says how many tabs are past the edge, and names them for a screen reader", async () => {
    await fourChats();

    watching.onlyShowing([1, 2]);

    const more = await screen.findByRole("button", {
      name: "Show 2 tabs the strip is not showing",
    });
    expect(more).toHaveTextContent("2 more");
  });

  it("counts one hidden tab in the singular", async () => {
    await fourChats();

    watching.onlyShowing([1, 2, 3]);

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

    watching.onlyShowing([1]);
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
    watching.onlyShowing([1]);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" }),
    );

    expect(menuNames()).toEqual(["ide.3", "ide.4", "ide.2"]);
    expect(tabNames()).toEqual(["ide.1", "ide.2", "ide.3", "ide.4"]);
  });

  it("keeps the strip's order in the menu for the chats nothing has been heard about", async () => {
    // At a launch nothing has moved, so every tab ties. A menu that shuffled them would be
    // a list whose rows move between two openings for no reason the operator can see.
    await fourChats();

    watching.onlyShowing([]);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 4 tabs the strip is not showing" }),
    );

    expect(menuNames()).toEqual(["ide.1", "ide.2", "ide.3", "ide.4"]);
  });

  it("brings a tab to the front when its row is picked", async () => {
    await fourChats();
    watching.onlyShowing([1]);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" }),
    );

    await userEvent.click(screen.getByRole("menuitem", { name: /ide\.3/ }));

    await vi.waitFor(() => expect(screen.getAllByTestId("pane")[0]).toHaveTextContent("session 3"));
    expect(
      within(strip())
        .getAllByRole("tab")
        .find((tab) => tab.getAttribute("aria-selected") === "true")
        ?.querySelector(".tab-name")?.textContent,
    ).toBe("ide.3");
  });

  it("moves between its rows with the arrow keys", async () => {
    // **The first thing built under ADR 0037, so it is the test of it.** The picker's radio
    // group does NOT follow the arrow keys under React 19 on its own — Radix learns that an
    // arrow is down from a `keydown` listener on `document`, and React's delegated listeners
    // sit below `document`, so focus has already moved by the time Radix hears it (#137).
    // A menu's roving focus is its own `onKeyDown`, on the content element, and this is what
    // says so rather than an assumption that the two primitives are alike.
    await fourChats();
    watching.onlyShowing([1]);
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
    watching.onlyShowing([1]);
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
    watching.onlyShowing([1]);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" }),
    );

    for (const row of screen.getAllByRole("menuitem")) {
      expect(row.textContent ?? "").not.toContain("End chat");
    }
  });

  it("leaves every hidden tab on the strip, with its close button", async () => {
    // **Nothing is collapsed out of the DOM.** The scroller is what keeps a tab reachable —
    // by keyboard, by the palette, by the needs-you queue — and the menu is an affordance
    // saying there is more, not the only way to what is there (charter-app#130/#131).
    await fourChats();

    watching.onlyShowing([1]);

    expect(tabNames()).toEqual(["ide.1", "ide.2", "ide.3", "ide.4"]);
    expect(within(strip()).getAllByRole("button", { name: /^End chat / })).toHaveLength(4);
  });

  it("keeps New tab out of the strip that scrolls, and out of the menu", async () => {
    // Measured in charter-app#130: `panes.e2e` could not press New tab partway through
    // opening fifty chats, because the button was the strip's last child. A menu that
    // collapsed the strip's tail would be the same hazard in different clothes.
    await fourChats();

    watching.onlyShowing([1]);

    const adding = await screen.findByRole("button", { name: "New tab" });
    expect(strip()).not.toContainElement(adding);
    expect(adding.closest(".more")).toBeNull();
  });

  it("drops a tab out of the menu when its chat ends", async () => {
    // A measurement is one frame behind the strip. Without the app intersecting what was
    // measured with what is drawn, the menu would look the ended tab up and find nothing.
    await fourChats();
    watching.onlyShowing([1]);
    await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" });

    await userEvent.click(within(strip()).getByRole("button", { name: "End chat ide.4" }));

    expect(
      await screen.findByRole("button", { name: "Show 2 tabs the strip is not showing" }),
    ).toBeInTheDocument();
  });

  it("does not leave an observer behind for every render", async () => {
    // One observer per strip, replaced when what it watches changes. A hook that rebuilt on
    // every render would pile them up and measure the same tabs many times over.
    await fourChats();

    watching.onlyShowing([1]);
    await userEvent.click(
      await screen.findByRole("button", { name: "Show 3 tabs the strip is not showing" }),
    );

    expect(watching.live()).toBe(1);
  });
});
