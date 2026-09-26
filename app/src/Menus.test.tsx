import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { catalogue, catalogued, menuRows, OUTSIDE, type Now } from "./actions";
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
  const rows = menuRows(what, catalogued(catalogue(now(over))));
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

    expect(shown.above).toEqual([
      "Focus workspace alpha",
      "Pin workspace alpha",
      "Workspace settings…",
      "Make alpha live…",
      "Rename workspace alpha…",
      "New workspace…",
    ]);
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

    expect(chat.above).toEqual([
      "Switch to tab 3 steward",
      "Rename chat 3 steward…",
      "Pin chat 3 steward",
    ]);
    expect(chat.below).toEqual(["End chat 3 steward"]);
  });

  it("offers a worktree's own verbs on the explorer's rows (charter-app#174)", () => {
    // The surface #172 could not reach, and the reason it could not: the catalogue had two
    // worktree rows and both were about the chat in front. These are about this piece.
    const cut = { workspace: "alpha", repo: "svc", piece: "fix-it" };
    const shown = titles(
      { on: "worktree", repo: "svc", piece: "fix-it" },
      {
        plane: "/plane",
        pieces: [cut],
      },
    );

    expect(shown.above).toEqual(["Merge worktree fix-it into svc"]);
    expect(shown.below).toEqual(["Remove worktree fix-it in svc"]);
  });

  it("draws the discard row on the piece whose removal was refused, and on no other", () => {
    // Listed in every worktree menu by `menuOn` and found in one, because the catalogue only
    // builds it beside the removal the core has just refused. Nothing in `Menus.tsx` or in
    // `menuOn` knows that — it is the same drop that leaves `Outside every workspace` with
    // no pin row.
    const pieces = [
      { workspace: "alpha", repo: "svc", piece: "fix-it" },
      { workspace: "alpha", repo: "svc", piece: "other" },
    ];
    const over = { plane: "/plane", pieces, refused: "worktree.remove:svc/fix-it" };

    expect(titles({ on: "worktree", repo: "svc", piece: "fix-it" }, over).below).toEqual([
      "Remove worktree fix-it in svc",
      "Discard that work and remove fix-it anyway",
    ]);
    expect(titles({ on: "worktree", repo: "svc", piece: "other" }, over).below).toEqual([
      "Remove worktree other in svc",
    ]);
  });

  it("offers a persona's reading and editing above the line and its deletion below (SI-3)", () => {
    const shown = titles({ on: "persona", persona: "steward" }, { personas: ["steward"] });

    expect(shown.above).toEqual([
      "Show what steward is",
      "Edit steward's persona.md",
      "New persona…",
    ]);
    expect(shown.below).toEqual(["Delete persona steward…"]);
  });

  it("offers a vault's opening above the line and its deletion below (SI-3)", () => {
    const shown = titles({ on: "vault", vault: "ops" }, { vaults: ["ops"] });

    expect(shown.above).toEqual(["Open vault ops", "New vault…"]);
    expect(shown.below).toEqual(["Delete vault ops…"]);
  });

  it("offers to close a todo above the line and to forget it below (SI-3)", () => {
    const shown = titles(
      { on: "todo", slug: "20260302-091400-review" },
      {
        workspaces: ["alpha"],
        focused: "alpha",
        todos: [{ slug: "20260302-091400-review", title: "Review the plan" }],
      },
    );

    expect(shown.above).toEqual(["Mark done: Review the plan"]);
    expect(shown.below).toEqual(["Forget todo Review the plan"]);
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
      catalogued(catalogue(now({ workspaces: ["alpha"], focused: "alpha", plane: "/plane" }))),
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
    const offers = catalogued(catalogue(now({ tabs: openTab(noTabs(), 7, "3 steward") })));
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
      "Rename chat 3 steward…",
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

describe("a menu from the keyboard (charter-app#174)", () => {
  /** A focusable row with a menu, and a text box inside a second trigger — a pane's terminal
   *  takes its keys the same way. */
  function rows() {
    const pressed: string[] = [];
    const offers = catalogue(
      now({ tabs: openTab(noTabs(), 7, "3 steward"), clones: [{ repo: "svc", path: "/svc" }] }),
    );
    render(
      <>
        <Menued
          on={{ on: "clone", repo: "svc" }}
          offers={catalogued(offers)}
          onPress={(offer) => pressed.push(offer.id)}
        >
          <button type="button">svc</button>
        </Menued>
        <Menued on={{ on: "chat", tab: 1 }} offers={catalogued(offers)} onPress={() => {}}>
          <div data-testid="panes">
            <textarea aria-label="terminal" />
          </div>
        </Menued>
      </>,
    );
    return pressed;
  }

  for (const key of [{ key: "F10", shiftKey: true }, { key: "ContextMenu" }]) {
    it(`opens on ${key.shiftKey ? "Shift+F10" : "the menu key"} on the row that has the keyboard`, async () => {
      // macOS has no keyboard convention for a context menu, so its WebView raises no
      // `contextmenu` for either key; without this a menu there is a pointer's alone.
      const pressed = rows();
      const row = screen.getByRole("button", { name: "svc" });
      row.focus();

      fireEvent.keyDown(row, key);

      const menu = await screen.findByRole("menu");
      expect(
        within(menu)
          .getAllByRole("menuitem")
          .map((item) => item.textContent),
      ).toEqual(["New tab in svc", "Start new chats in svc"]);
      await userEvent.keyboard("{Enter}");
      expect(pressed).toEqual(["clone.chat:svc"]);
    });
  }

  it("leaves Shift+F10 to whatever inside the trigger has the keyboard", () => {
    // The panes' menu is on the box the terminals are in, and a program in a terminal may want
    // the key. Only the trigger itself having the keyboard opens its menu.
    rows();
    const terminal = screen.getByRole("textbox", { name: "terminal" });
    terminal.focus();

    fireEvent.keyDown(terminal, { key: "F10", shiftKey: true });

    expect(screen.queryByRole("menu")).toBeNull();
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

/**
 * **The discriminator**, and the reason it is written out at length rather than folded into
 * the tests above.
 *
 * The scenario run cannot open a context menu: a WebDriver right-click opens nothing on
 * webkit 605.1.15 (macOS) or on WebKitGTK 605.1.15 (Linux) — `e2e/specs/workspace-lifecycle`
 * records the runs. Two engines behaving identically under one driver points at the driver,
 * but "points at" is not a measurement, and the alternative — that charter's own wiring
 * swallows the event — would be a bug the operator meets a minute after installing.
 *
 * So the question is asked where there is **no WebDriver anywhere in the picture**: one
 * `MouseEvent`, constructed and dispatched on a real workspace tab of the real `App`, with
 * `useNoBrowserMenu` mounted — which is the only thing charter has that could prevent it.
 *
 * What this stands guard over is the ordering `Menus.tsx` documents. React 19 attaches its
 * delegated listeners to the root container, BELOW `window`, so Radix's composed
 * `onContextMenu` runs before the window-level suppressor and opens the menu. A capturing
 * suppressor would reverse that: `composeEventHandlers` skips Radix's own handler once the
 * event is `defaultPrevented`, so every context menu in the app would stop opening —
 * silently, because nothing throws and the browser menu would still be gone.
 */
describe("a real contextmenu event, with the suppressor live", () => {
  const PLANE = "/home/dev/plane";

  function aPlane() {
    mockIPC((cmd) => {
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          personas: [],
          persona: null,
          unfiled: [],
          workspaces: [
            { name: "alpha", path: `${PLANE}/workspaces/alpha`, vision: "", todos: [], chats: [] },
          ],
        };
      if (
        cmd === "opened_chats" ||
        cmd === "chats_that_would_not_start" ||
        cmd === "running_sessions" ||
        cmd === "chat_states"
      )
        return [];
      return null;
    });
  }

  it("opens charter's own menu, and still takes the browser's away", async () => {
    aPlane();
    render(<App />);
    const tab = await screen.findByRole("tab", { name: /alpha/ });

    // Not `fireEvent`, deliberately: what a WebView sends is a `MouseEvent`, so that is what
    // is sent, and the event object is kept so the second claim can be made about it.
    const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    tab.dispatchEvent(event);

    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((row) => row.getAttribute("aria-label")),
    ).toEqual([
      "Focus workspace alpha",
      "Pin workspace alpha",
      "Workspace settings…",
      "Make alpha live…",
      "Rename workspace alpha…",
      "New workspace…",
      "Delete workspace alpha",
    ]);
    // Both halves of one event: charter answered it, and the WebView's own menu is still gone.
    expect(event.defaultPrevented).toBe(true);
  });
});
