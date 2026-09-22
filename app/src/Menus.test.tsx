import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { catalogue, menuRows, OUTSIDE, type Now } from "./actions";
import { Menued, useNoBrowserMenu } from "./Menus";
import { noTabs, openTab } from "./tabs";

/**
 * The context menus, and the browser menu they replace.
 *
 * Two claims are worth a test and both are about the SAME list: that a menu draws the
 * catalogue's rows rather than a list of its own, and that a row the catalogue does not have
 * is simply not in the menu. Everything else — what a row says, whether it can run, what it
 * costs — is `actions.test.ts`'s, and asserting it again here would be the second answer this
 * whole design exists to prevent.
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

function now(over: Partial<Now> = {}): Now {
  return {
    tabs: noTabs(),
    workspaces: [],
    needsYou: [],
    nameOf: (session) => String(session),
    ...over,
  };
}

/** The rows a menu would draw, as their titles — above the line, then below it. */
function titles(what: Parameters<typeof menuRows>[0], over: Partial<Now> = {}) {
  const rows = menuRows(what, catalogue(now(over)));
  return {
    above: rows.above.map((row) => row.title),
    below: rows.below.map((row) => row.title),
  };
}

describe("what a menu lists", () => {
  it("is the catalogue, filtered to the thing it was opened on", () => {
    const shown = titles(
      { on: "workspace", workspace: "alpha" },
      {
        workspaces: ["alpha", "beta"],
        focused: "beta",
        plane: "/plane",
      },
    );

    expect(shown.above).toEqual(["Focus workspace alpha", "Pin workspace alpha", "New workspace…"]);
    expect(shown.below).toEqual(["Delete workspace alpha"]);
  });

  it("drops a row the catalogue does not have, without knowing which rows those are", () => {
    // The strip of chats outside every workspace is not a workspace on the plane: it has no
    // pin row and no delete row, because there is nothing on disk for either to name. Nothing
    // in `Menus.tsx` or in `menuOn` was told about that case — the rows are looked up by id
    // and the two that do not exist are not found.
    const shown = titles(
      { on: "workspace", workspace: OUTSIDE },
      {
        workspaces: ["alpha", OUTSIDE],
        plane: "/plane",
      },
    );

    expect(shown.above).toEqual(["Focus the chats outside every workspace", "New workspace…"]);
    expect(shown.below).toEqual([]);
  });

  it("puts everything that destroys something below the line and nothing else", () => {
    const chat = titles({ on: "chat", tab: 1 }, { tabs: openTab(noTabs(), 7, "3 steward") });

    expect(chat.above).toEqual(["Switch to tab 3 steward", "Pin chat 3 steward"]);
    expect(chat.below).toEqual(["End chat 3 steward"]);
  });

  it("offers the pane's own verbs in the centre of the window", () => {
    const pane = titles({ on: "pane" }, { tabs: openTab(noTabs(), 7, "one") });

    expect(pane.above).toEqual([
      "New tab",
      "Split right",
      "Split down",
      "Send F2 to the chat in front",
    ]);
    expect(pane.below).toEqual(["End this pane's chat"]);
  });

  it("keeps a row that cannot run, with the catalogue's reason on it", () => {
    // The same rule the palette follows: an operator cannot ask about an option they cannot
    // see, and a greyed row saying why is an answer where a missing row is a mystery.
    const rows = menuRows(
      { on: "workspace", workspace: "alpha" },
      catalogue(now({ workspaces: ["alpha"], focused: "alpha", plane: "/plane" })),
    );

    const focus = rows.above[0];
    expect(focus.available).toBe(false);
    expect(focus.reason).toBe("It is already focused.");
  });
});

describe("a menu on screen", () => {
  /** A trigger with one chat's rows on it, and what was pressed. */
  function aMenu() {
    const pressed: string[] = [];
    const offers = catalogue(now({ tabs: openTab(noTabs(), 7, "3 steward") }));
    render(
      <Menued
        on={{ on: "chat", tab: 1 }}
        offers={offers}
        onPress={(offer) => pressed.push(offer.id)}
      >
        <span data-testid="tab">3 steward</span>
      </Menued>,
    );
    return pressed;
  }

  it("opens on a right-click and draws the rows it was given", async () => {
    aMenu();

    fireEvent.contextMenu(screen.getByTestId("tab"));

    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((row) => row.textContent),
    ).toEqual([
      "Switch to tab 3 steward",
      "Pin chat 3 stewardDraws it first on its strip. Yours, on this machine only.",
      "End chat 3 stewardEnds the program it runs. There is no undo.",
    ]);
  });

  it("hands back the catalogue's own offer when a row is pressed", async () => {
    const pressed = aMenu();
    fireEvent.contextMenu(screen.getByTestId("tab"));
    await screen.findByRole("menu");

    await userEvent.click(screen.getByRole("menuitem", { name: /^End chat 3 steward/ }));

    expect(pressed).toEqual(["tab.close:1"]);
  });

  it("adds no element of its own to the strip it is on", () => {
    // `asChild`: the trigger IS the element, because the chat strip's tabs are measured to
    // decide what is off screen (`offscreen.ts`) and a wrapper would be measured instead.
    aMenu();

    const tab = screen.getByTestId("tab");
    expect(tab.parentElement?.tagName).toBe("DIV");
    expect(tab.parentElement?.getAttribute("data-testid")).toBeNull();
  });
});

describe("the browser's own menu", () => {
  function Window() {
    useNoBrowserMenu();
    return (
      <div>
        <button type="button">nothing here has a menu</button>
        <input aria-label="a box" defaultValue="paste me" />
      </div>
    );
  }

  it("is taken away everywhere, including where charter has no menu of its own", () => {
    // A shipped app that answers a right-click with `Reload` and `Inspect Element` is showing
    // the operator the browser it is built on.
    render(<Window />);

    const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    screen.getByRole("button").dispatchEvent(event);

    expect(event.defaultPrevented).toBe(true);
  });

  it("is left alone where the operator is editing text", () => {
    // The platform's Cut/Copy/Paste is not the browser showing through — it is the only
    // pointer route to the clipboard this window has, and `Inspect Element` is off in a
    // release build anyway.
    render(<Window />);

    const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    screen.getByRole("textbox", { name: "a box" }).dispatchEvent(event);

    expect(event.defaultPrevented).toBe(false);
  });

  it("is prevented on the whole window of the real app", () => {
    mockIPC(() => null);
    render(<App />);

    const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    document.body.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(true);
  });
});
