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
      "New shell in alpha",
      "Pin workspace alpha",
      "Workspace settings…",
      "Make alpha live…",
      "Rename workspace alpha…",
      "New workspace…",
    ]);
    expect(shown.below).toEqual(["Delete workspace alpha"]);
  });

  it("drops a row the catalogue does not have, without knowing which rows those are", () => {
    // A workspace with no settings row offered — here, one the catalogue was not told is on
    // the plane — has no such row in its menu. Nothing in `Menus.tsx` or in `menuOn` was told
    // which: the rows are looked up by id and the ones that do not exist are not found.
    const shown = titles({ on: "workspace", workspace: "ghost" }, { workspaces: ["alpha"] });

    expect(shown.above).toEqual(["New workspace…"]);
    expect(shown.below).toEqual([]);
  });

  it("gives the plane root a menu of its own: focus, a chat and a shell there, and no delete (SI-1)", () => {
    const shown = titles({ on: "root" }, { workspaces: [OUTSIDE, "alpha"], plane: "/plane" });

    expect(shown.above).toEqual([
      "Focus the plane root",
      "New chat at the plane root",
      "New shell at the plane root",
      "New workspace…",
    ]);
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
    const cut = { workspace: "alpha", repo: "svc", piece: "fix-it", branch: "fix-it" };
    const shown = titles(
      { on: "worktree", repo: "svc", piece: "fix-it" },
      {
        plane: "/plane",
        pieces: [cut],
      },
    );

    expect(shown.above).toEqual([
      "Focus on branch fix-it",
      "Browse the files of fix-it",
      "Merge branch fix-it into svc",
    ]);
    expect(shown.below).toEqual(["Remove folder fix-it in svc"]);
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
      "Remove folder fix-it in svc",
      "Discard that work and remove fix-it anyway",
    ]);
    expect(titles({ on: "worktree", repo: "svc", piece: "other" }, over).below).toEqual([
      "Remove folder other in svc",
    ]);
  });

  it("offers a persona's reading and editing above the line and its deletion below (SI-3)", () => {
    const shown = titles({ on: "persona", persona: "steward" }, { personas: ["steward"] });

    expect(shown.above).toEqual([
      "Show what steward is",
      "Edit steward's persona.md",
      "Set steward's profile…",
      "New persona…",
    ]);
    expect(shown.below).toEqual(["Delete persona steward…"]);
  });

  it("offers a vault's opening above the line and its deletion below (SI-3)", () => {
    const shown = titles({ on: "vault", vault: "ops" }, { vaults: ["ops"] });

    expect(shown.above).toEqual(["Open vault ops", "New vault…"]);
    expect(shown.below).toEqual(["Delete vault ops…"]);
  });

  it("offers to open and close a todo above the line and to forget it below (SI-3, #1214)", () => {
    const shown = titles(
      { on: "todo", slug: "20260302-091400-review" },
      {
        workspaces: ["alpha"],
        focused: "alpha",
        todos: [{ slug: "20260302-091400-review", title: "Review the plan" }],
      },
    );

    expect(shown.above).toEqual(["Open todo: Review the plan", "Mark done: Review the plan"]);
    expect(shown.below).toEqual(["Forget todo Review the plan"]);
  });

  it("offers the pane's own verbs in the centre of the window", () => {
    const pane = titles({ on: "pane" }, { tabs: openTab(noTabs(), 7, "one") });

    expect(pane.above).toEqual([
      "New tab",
      "New shell",
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

/** What the core answers `curation_offers` for workspace `alpha` (ADR 0061): charter's own two,
 *  `ops`'s and `qa`'s, and one file it left out. */
function curations(cannot: string | null = null): NonNullable<Now["curations"]> {
  const action = (id: string, label: string, by: string | null) => ({
    id,
    label,
    declared_by: by,
    runner: by ?? "steward",
    cwd: "/plane/workspaces/alpha",
    prompt: `${label} alpha.`,
  });
  return {
    subjects: [
      {
        subject: "workspace:alpha",
        name: "alpha",
        actions: [
          action("charter/safe-remove", "Safe remove", null),
          action("charter/compact", "Compact & improve", null),
          action("ops/tidy", "Tidy", "ops"),
          action("qa/audit", "Audit", "qa"),
        ],
        left_out: [
          {
            what: "personas/ops/curation/bad.md",
            why: "personas/ops/curation/bad.md is not offered: no label.",
          },
        ],
        trouble: null,
      },
    ],
    cannot,
  };
}

describe("the Curate ▸ submenu (ADR 0061)", () => {
  function aWorkspaceMenu(cannot: string | null = null) {
    const pressed: string[] = [];
    const offers = catalogued(
      catalogue(now({ workspaces: ["alpha"], plane: "/plane", curations: curations(cannot) })),
    );
    render(
      <Menued
        on={{ on: "workspace", workspace: "alpha" }}
        offers={offers}
        onPress={(offer) => pressed.push(offer.id)}
      >
        <span data-testid="strip-alpha">alpha</span>
      </Menued>,
    );
    return pressed;
  }

  async function openCurate() {
    fireEvent.contextMenu(screen.getByTestId("strip-alpha"));
    const menu = await screen.findByRole("menu");
    await userEvent.click(within(menu).getByRole("menuitem", { name: "Curate" }));
    const menus = await screen.findAllByRole("menu");
    return menus[menus.length - 1];
  }

  it("lists charter's own first, then a named group per persona, then what was left out", async () => {
    aWorkspaceMenu();

    const sub = await openCurate();

    expect(
      Array.from(sub.querySelectorAll("[role=menuitem], .menu-label, .menu-line")).map((el) =>
        el.getAttribute("role") === "menuitem"
          ? el.getAttribute("aria-label")
          : el.classList.contains("menu-label")
            ? `[${el.textContent}]`
            : "---",
      ),
    ).toEqual([
      "Safe remove",
      "Compact & improve",
      "---",
      "[ops]",
      "Tidy",
      "---",
      "[qa]",
      "Audit",
      "---",
      "personas/ops/curation/bad.md is left out",
    ]);
  });

  it("draws an action the core left out as a row that cannot run, saying why", async () => {
    aWorkspaceMenu();

    const sub = await openCurate();

    const left = within(sub).getByRole("menuitem", {
      name: "personas/ops/curation/bad.md is left out",
    });
    expect(left.getAttribute("aria-disabled")).toBe("true");
    expect(left.getAttribute("title")).toBe(
      "personas/ops/curation/bad.md is not offered: no label.",
    );
  });

  it("hands back the action's own row when one is chosen", async () => {
    const pressed = aWorkspaceMenu();

    const sub = await openCurate();
    await userEvent.click(within(sub).getByRole("menuitem", { name: "Tidy" }));

    expect(pressed).toEqual(["curate:workspace:alpha/ops/tidy"]);
  });

  it("draws every action ready to run when the core says a chat can be typed into (a Codex default, SI-2e)", async () => {
    // The core answers `cannot: null` for a Codex default as it does for Claude Code: the
    // prompt is typed once Codex's terminal is raw and quiet (ADR 0061, amended 2026-09-27).
    const pressed = aWorkspaceMenu(null);

    const sub = await openCurate();

    for (const name of ["Safe remove", "Compact & improve", "Tidy", "Audit"]) {
      const row = within(sub).getByRole("menuitem", { name });
      expect(row.getAttribute("aria-disabled")).not.toBe("true");
    }
    await userEvent.click(within(sub).getByRole("menuitem", { name: "Safe remove" }));
    expect(pressed).toEqual(["curate:workspace:alpha/charter/safe-remove"]);
  });

  it("draws every action disabled, with the reason, when no chat can be typed into", async () => {
    aWorkspaceMenu(
      "The default profile 'work' runs opencode, which says nothing until your first prompt.",
    );

    const sub = await openCurate();

    const safe = within(sub).getByRole("menuitem", { name: "Safe remove" });
    expect(safe.getAttribute("aria-disabled")).toBe("true");
    expect(safe.getAttribute("title")).toContain("first prompt");
  });

  it("curates the plane from the plane root's tab (SI-1)", async () => {
    const pressed: string[] = [];
    const plane = {
      subject: "plane",
      name: "plane",
      actions: [
        {
          id: "ops/audit",
          label: "Audit the plane",
          declared_by: "ops",
          runner: "ops",
          cwd: "/plane",
          prompt: "Audit.",
        },
      ],
      left_out: [],
      trouble: null,
    };
    render(
      <Menued
        on={{ on: "root" }}
        offers={catalogued(
          catalogue(
            now({
              workspaces: [OUTSIDE, "alpha"],
              plane: "/plane",
              curations: { subjects: [plane], cannot: null },
            }),
          ),
        )}
        onPress={(offer) => pressed.push(offer.id)}
      >
        <span data-testid="root">root</span>
      </Menued>,
    );

    fireEvent.contextMenu(screen.getByTestId("root"));
    const menu = await screen.findByRole("menu");
    await userEvent.click(within(menu).getByRole("menuitem", { name: "Curate" }));
    const menus = await screen.findAllByRole("menu");
    await userEvent.click(
      within(menus[menus.length - 1]).getByRole("menuitem", { name: "Audit the plane" }),
    );

    expect(pressed).toEqual(["curate:plane/ops/audit"]);
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
      ).toEqual(["New tab in svc", "New branch in svc…", "Start new chats in svc"]);
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
      "New shell in alpha",
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
