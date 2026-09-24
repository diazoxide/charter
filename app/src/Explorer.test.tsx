/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Explorer, type Spot } from "./Explorer";
import { nothingKnown } from "./chatState";
import { catalogue, catalogued, type Catalogued, type Offer } from "./actions";
import { noTabs } from "./tabs";
import type { OpenChat, Panels as PanelsModel, Piece } from "./bindings";
import type { WorkspaceState } from "./workspaceState";

afterEach(cleanup);

const ALPHA = "/home/dev/plane/workspaces/alpha";
const CUT = `${ALPHA}/.worktrees/svc`;

const PANELS: PanelsModel = {
  workspace: "alpha",
  repos: ["svc", "tool"],
  absent: [],
  refused: [],
  todos: [],
  todos_refused: null,
  personas: ["steward"],
  persona: "steward",
  contributed: [],
};

function piece(name: string, on: Partial<Piece> = {}): Piece {
  return { piece: name, path: `${CUT}/${name}`, branch: name, wired: true, stale: false, ...on };
}

function chat(session: number, name: string, cwd: string, on: Partial<OpenChat> = {}): OpenChat {
  return {
    session,
    name,
    cwd,
    harness: "claude",
    in_front: false,
    resumed: null,
    fresh: null,
    profile: "claude",
    persona: null,
    unreported: null,
    pinned: false,
    ...on,
  };
}

function state(on: Partial<WorkspaceState> = {}): WorkspaceState {
  return {
    panels: PANELS,
    repos: { workspace: "alpha", repos: [], cache_refused: null },
    pieces: { svc: [piece("one"), piece("two")], tool: [] },
    piecesRefused: {},
    reading: false,
    ...on,
  };
}

function draw(on: {
  state?: WorkspaceState;
  chats?: OpenChat[];
  spot?: Spot;
  onPick?: (spot: Spot | undefined) => void;
  onShowChat?: (session: number) => void;
  workspace?: string;
  /** The catalogue a piece row's menu is drawn out of. Empty here for every test but the one
   *  about the menu: `Menued` draws nothing for an item the catalogue has no rows for, so the
   *  tree these tests are about is the tree they were always about. */
  offers?: Catalogued;
  onPress?: (offer: Offer) => void;
}) {
  render(
    <Explorer
      workspace={"workspace" in on ? on.workspace : "alpha"}
      state={on.state ?? state()}
      chats={on.chats ?? []}
      states={nothingKnown}
      spot={on.spot}
      onPick={on.onPick ?? (() => {})}
      onShowChat={on.onShowChat ?? (() => {})}
      offers={on.offers ?? new Map()}
      onPress={on.onPress ?? (() => {})}
    />,
  );
}

/** The spot marked as the one the next chat would start in. */
const picked = () =>
  screen
    .getAllByRole("treeitem")
    .filter((one) => one.getAttribute("aria-current") === "true")
    .map((one) => one.textContent);

describe("the explorer", () => {
  it("lists the focused workspace's clones and the worktrees cut off each", () => {
    draw({});

    const svc = screen.getByTestId("clone-svc");
    expect(within(svc).getByRole("treeitem", { name: /one/ })).toBeInTheDocument();
    expect(within(svc).getByRole("treeitem", { name: /two/ })).toBeInTheDocument();
    expect(screen.getByTestId("clone-tool")).toHaveTextContent("No worktrees cut here");
  });

  it("does not list every workspace, because the strip above already answers that", () => {
    // ADR 0038: the sidebar used to draw every workspace with its vision text, under
    // the strip that had just been made the axis. That duplication is what this region
    // replaced, and a test is the only thing that keeps it replaced.
    draw({});

    expect(screen.queryByText("beta")).not.toBeInTheDocument();
    expect(screen.getByTestId("explorer")).not.toHaveTextContent("vision");
  });

  it("starts on the workspace's own directory, so a plain New tab opens where it always did", () => {
    draw({});

    expect(picked()).toEqual([expect.stringContaining("alpha")]);
  });

  it("hands back the path the core spelled when a worktree is picked", async () => {
    // The window never joins a path together. `worktree_list` answers with one, and that is
    // what the next chat is started in.
    const onPick = vi.fn();
    draw({ onPick });

    await userEvent.click(screen.getByRole("treeitem", { name: /^one/ }));

    expect(onPick).toHaveBeenCalledWith({ repo: "svc", piece: "one", path: `${CUT}/one` });
  });

  it("marks the picked worktree, and only it", () => {
    draw({ spot: { repo: "svc", piece: "two", path: `${CUT}/two` } });

    expect(picked()).toEqual([expect.stringContaining("two")]);
  });

  it("goes back to the workspace when its own row is picked", async () => {
    const onPick = vi.fn();
    draw({ spot: { repo: "svc", piece: "two", path: `${CUT}/two` }, onPick });

    await userEvent.click(screen.getByRole("treeitem", { name: /the workspace itself/ }));

    expect(onPick).toHaveBeenCalledWith(undefined);
  });

  it("says a worktree has no charter layer before a chat is started in it", () => {
    // `unwired` is what the operator has to see BEFORE they click: a chat started in such a
    // tree runs with none of the plane's ask/deny rules and no persona agents.
    draw({ state: state({ pieces: { svc: [piece("one", { wired: false })], tool: [] } }) });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("unwired");
  });

  it("says a registration whose directory is gone is stale", () => {
    draw({ state: state({ pieces: { svc: [piece("one", { stale: true })], tool: [] } }) });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("stale");
  });

  it("names the branch each worktree is on", () => {
    draw({ state: state({ pieces: { svc: [piece("one", { branch: "fix/login" })], tool: [] } }) });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("fix/login");
  });

  it("says why a clone's worktrees could not be listed rather than showing none", () => {
    draw({
      state: state({
        pieces: { tool: [] },
        piecesRefused: { svc: "charter will not run git through a symlink" },
      }),
    });

    const svc = screen.getByTestId("clone-svc");
    expect(within(svc).getByRole("alert")).toHaveTextContent("symlink");
    expect(svc).not.toHaveTextContent("No worktrees cut here");
  });

  it("says the listing is still coming rather than saying there is nothing", () => {
    draw({ state: state({ pieces: {} }) });

    expect(screen.getByTestId("clone-svc")).toHaveTextContent("Asking git…");
  });

  // ---------------------------------------------------------------------------------------
  // What is already running where
  // ---------------------------------------------------------------------------------------

  it("shows the chats working in a worktree under that worktree", () => {
    draw({ chats: [chat(1, "ide.1", `${CUT}/one/deep`), chat(2, "ide.2", ALPHA)] });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("ide.1");
    expect(screen.getByTestId("piece-svc-one")).not.toHaveTextContent("ide.2");
  });

  it("shows a chat that is in no worktree under the workspace itself", () => {
    draw({ chats: [chat(2, "ide.2", `${ALPHA}/svc`)] });

    // Not inside any `piece-…` row: it works in the clone, not in a piece of it.
    expect(screen.getByTestId("explorer")).toHaveTextContent("ide.2");
    expect(screen.getByTestId("piece-svc-one")).not.toHaveTextContent("ide.2");
  });

  it("does not put a chat in a worktree whose name merely starts the same way", () => {
    // By path components and never by string prefix: `…/one-more` starts with `…/one`.
    draw({ chats: [chat(3, "ide.3", `${CUT}/one-more`)] });

    expect(screen.getByTestId("piece-svc-one")).not.toHaveTextContent("ide.3");
  });

  it("brings a chat forward when its row is pressed", async () => {
    const onShowChat = vi.fn();
    draw({ chats: [chat(1, "ide.1", `${CUT}/one`)], onShowChat });

    await userEvent.click(screen.getByRole("treeitem", { name: /ide\.1/ }));

    expect(onShowChat).toHaveBeenCalledWith(1);
  });

  it("says on a chat what its harness cannot report, rather than leaving it unexplained", () => {
    draw({
      chats: [
        chat(1, "ide.1", `${CUT}/one`, {
          unreported: "codex does not report an approval prompt",
        }),
      ],
    });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("does not report");
  });

  // ---------------------------------------------------------------------------------------
  // Nothing to explore
  // ---------------------------------------------------------------------------------------

  it("says so when the strip is on the chats that are in no workspace", () => {
    draw({ workspace: undefined });

    expect(screen.getByTestId("explorer")).toHaveTextContent("nothing to explore");
  });

  it("says a workspace holds no repos rather than drawing an empty region", () => {
    draw({ state: state({ panels: { ...PANELS, repos: [] }, pieces: {} }) });

    expect(screen.getByTestId("explorer")).toHaveTextContent("No repos in this workspace");
  });

  it("names a repo the manifest claims that nobody has cloned", () => {
    draw({ state: state({ panels: { ...PANELS, repos: ["svc"], absent: ["later"] } }) });

    expect(screen.getByTestId("absent")).toHaveTextContent("later");
    // And it is not a heading with worktrees under it: there is nothing to cut one from.
    expect(screen.queryByTestId("clone-later")).not.toBeInTheDocument();
  });

  it("shows what charter would not read instead of a workspace with fewer repos in it", () => {
    draw({
      state: state({
        panels: {
          ...PANELS,
          repos: ["svc"],
          refused: [["alias", "'alias' is reached through a symlink"]],
        },
      }),
    });

    expect(screen.getByRole("alert")).toHaveTextContent("alias");
  });
});

/**
 * Right-click on a piece row (charter-app#174).
 *
 * **Asked in jsdom, because a WebDriver right-click sends no `contextmenu` event at all** —
 * measured on webkit 605.1.15 and on WebKitGTK, and recorded in `docs/ui-primitives.md`. A
 * scenario spec can prove a menu is on screen; whether a right-click opens one is a question
 * only this environment can answer.
 *
 * What is asserted here is the wiring and nothing else: the row has a menu, and the menu's
 * rows are the catalogue's. What those rows SAY is `actions.test.ts`'s, and saying it again
 * here would be the second answer the whole design exists to prevent.
 */
describe("a piece row's menu", () => {
  const cut = { workspace: "alpha", repo: "svc", piece: "one" };
  const offers = () =>
    catalogued(
      catalogue({
        tabs: noTabs(),
        workspaces: ["alpha"],
        focused: "alpha",
        plane: "/plane",
        pieces: [cut],
        needsYou: [],
        nameOf: String,
      }),
    );

  it("opens on a right-click with the catalogue's own rows for that piece", async () => {
    draw({ offers: offers() });

    const row = within(screen.getByTestId("piece-svc-one")).getByRole("treeitem", { name: "one" });
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));

    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((one) => one.getAttribute("aria-label")),
    ).toEqual(["Merge worktree one into svc", "Remove worktree one in svc"]);
  });

  it("hands the catalogue's offer back when a row is pressed", async () => {
    const pressed: string[] = [];
    draw({ offers: offers(), onPress: (offer) => pressed.push(offer.id) });

    const row = within(screen.getByTestId("piece-svc-one")).getByRole("treeitem", { name: "one" });
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    await screen.findByRole("menu");
    await userEvent.click(screen.getByRole("menuitem", { name: "Remove worktree one in svc" }));

    expect(pressed).toEqual(["worktree.remove:svc/one"]);
  });

  it("leaves a clone row without one, because nothing in the catalogue is about a clone", () => {
    // The gap this component's docstring records: `workspace_panels` answers with clone NAMES
    // and nothing charter can do takes one. A menu there would have to invent a verb.
    draw({ offers: offers() });

    const clone = within(screen.getByTestId("clone-svc")).getByText("svc");
    clone.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));

    expect(screen.queryByRole("menu")).toBeNull();
  });
});

// ---------------------------------------------------------------------------------------
// Drawn as a tree (M6.6)
// ---------------------------------------------------------------------------------------

describe("the explorer is drawn as a tree", () => {
  it("marks each kind of node with its own icon, so a chat leaf is told from a worktree leaf", () => {
    draw({ chats: [chat(1, "ide.1", `${CUT}/one`)] });

    const svc = screen.getByTestId("clone-svc");
    // Lucide names each `<svg>` after its icon, which is the only handle a test has on which
    // mark was drawn — the mark itself is hidden from the accessibility tree on purpose.
    expect(svc.querySelector("summary .lucide-folder-git-2")).not.toBeNull();
    expect(svc.querySelector("summary .twisty")).not.toBeNull();
    const one = screen.getByTestId("piece-svc-one");
    expect(one.querySelector(".spot .lucide-git-branch")).not.toBeNull();
    expect(one.querySelector(".chat .lucide-square-terminal")).not.toBeNull();
  });

  it("keeps every row named by its words, with the icons beside them unread", () => {
    draw({ chats: [chat(1, "ide.1", `${CUT}/one`)] });

    // The names the rest of this file and the scenario specs press by. An icon that joined
    // an accessible name would rename every row it was put on.
    expect(screen.getByRole("treeitem", { name: /^one$/ })).toBeInTheDocument();
    for (const svg of screen.getByTestId("explorer").querySelectorAll("svg")) {
      expect(svg.getAttribute("aria-hidden")).toBe("true");
    }
  });

  it("spins only while something is still being read", () => {
    draw({ state: state({ pieces: {} }) });
    expect(screen.getByTestId("clone-svc").querySelector(".pending .spinning")).not.toBeNull();

    cleanup();
    draw({});
    expect(screen.getByTestId("explorer").querySelector(".spinning")).toBeNull();
  });
});

/**
 * **The tree's guides may not paint a background**, because a region moves (ADR 0038).
 *
 * The usual way to stop a tree's vertical line at its last row is a rectangle in the background
 * colour laid over the line's tail. That works exactly as long as the tree is on that
 * background — and the explorer can be put in the bottom slot, which is `surface.deep`, where
 * the mask would be a visible block. So each row draws only its own segment, and this holds
 * the rule a well-meaning simplification would break.
 */
describe("the explorer's tree guides", () => {
  const css = readFileSync(join(process.cwd(), "src/App.css"), "utf8");
  const guides = [...css.matchAll(/([^{}]*\.explorer[^{}]*::(?:before|after)[^{}]*)\{([^}]*)\}/g)];

  it("are drawn by rules the stylesheet has", () => {
    expect(guides.length).toBeGreaterThan(0);
  });

  it("draw with borders only, so they need to know nothing about what is behind them", () => {
    const painted = guides
      .filter(([, , body]) => /(?:^|[;\s])background(?:-color)?\s*:/.test(body))
      .map(([, selector]) => selector.trim());
    expect(painted).toEqual([]);
  });
});

// ---------------------------------------------------------------------------------------
// A WAI-ARIA tree (charter-app#238)
// ---------------------------------------------------------------------------------------

/**
 * **The explorer is a tree for the keyboard and for a screen reader**, the WAI-ARIA "Tree
 * View" pattern: `role="tree"` and `treeitem`, each row's level and place among its siblings,
 * `aria-expanded` on the rows that fold, and the keys that open, close and climb.
 *
 * The tree drawn here is `alpha` → `svc` (→ `one` → the chat `seven`; `two`) and `tool`, which
 * has no worktrees.
 */
describe("the explorer is a WAI-ARIA tree", () => {
  const withAChat = () => draw({ chats: [chat(7, "seven", `${CUT}/one`)] });
  const tree = () => screen.getByRole("tree", { name: "Repos and worktrees" });
  const item = (name: RegExp) => within(tree()).getByRole("treeitem", { name });
  /** A treeitem as its first word, its level and its place among its siblings. */
  const shape = (row: HTMLElement) =>
    [
      row.querySelector(".spot-name, .repo, .session")?.textContent,
      row.getAttribute("aria-level"),
      `${row.getAttribute("aria-posinset")}/${row.getAttribute("aria-setsize")}`,
    ].join(" ");

  it("draws every row as a treeitem with its level and its place among its siblings", () => {
    withAChat();

    expect(within(tree()).getAllByRole("treeitem").map(shape)).toEqual([
      "alpha 1 1/1",
      "svc 2 1/2",
      "one 3 1/2",
      "seven 4 1/1",
      "two 3 2/2",
      "tool 2 2/2",
    ]);
  });

  it("says on every parent whether it is open, and nothing on a leaf", async () => {
    withAChat();

    // A clone folds, even one with no worktrees: what it opens onto is the note saying so.
    expect(item(/^svc/)).toHaveAttribute("aria-expanded", "true");
    expect(item(/^tool/)).toHaveAttribute("aria-expanded", "true");
    // Parents that cannot fold are always open, and say so.
    expect(item(/^alpha/)).toHaveAttribute("aria-expanded", "true");
    expect(item(/^one/)).toHaveAttribute("aria-expanded", "true");
    for (const leaf of [/^seven/, /^two/]) {
      expect(item(leaf)).not.toHaveAttribute("aria-expanded");
    }

    await userEvent.click(item(/^svc/));
    expect(item(/^svc/)).toHaveAttribute("aria-expanded", "false");
  });

  it("Right opens a closed clone, then moves to its first child, and stops at a leaf", async () => {
    withAChat();
    await userEvent.click(item(/^svc/));
    expect(item(/^svc/)).toHaveAttribute("aria-expanded", "false");

    await userEvent.keyboard("{ArrowRight}");
    expect(item(/^svc/)).toHaveAttribute("aria-expanded", "true");
    expect(item(/^svc/)).toHaveFocus();

    await userEvent.keyboard("{ArrowRight}");
    expect(item(/^one/)).toHaveFocus();
    // A worktree with a chat in it is a parent that is always open: Right goes in.
    await userEvent.keyboard("{ArrowRight}");
    expect(item(/^seven/)).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    expect(item(/^seven/)).toHaveFocus();
  });

  it("Right on the workspace row moves to its first child", async () => {
    withAChat();
    item(/^alpha/).focus();

    await userEvent.keyboard("{ArrowRight}");
    expect(item(/^svc/)).toHaveFocus();
  });

  it("Left moves to the parent, closes an open clone, and stops at the workspace row", async () => {
    withAChat();
    item(/^seven/).focus();

    await userEvent.keyboard("{ArrowLeft}");
    expect(item(/^one/)).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(item(/^svc/)).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(item(/^svc/)).toHaveAttribute("aria-expanded", "false");
    expect(item(/^svc/)).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(item(/^alpha/)).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(item(/^alpha/)).toHaveFocus();
  });

  it("Up and Down skip what a folded clone holds, and Home and End reach the ends", async () => {
    withAChat();
    item(/^svc/).focus();
    await userEvent.keyboard("{ArrowLeft}");

    await userEvent.keyboard("{ArrowDown}");
    expect(item(/^tool/)).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(item(/^alpha/)).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(item(/^tool/)).toHaveFocus();
  });

  it("a typed letter moves to the next row whose name starts with it, and wraps", async () => {
    withAChat();
    item(/^alpha/).focus();

    await userEvent.keyboard("t");
    expect(item(/^two/)).toHaveFocus();
    await userEvent.keyboard("T");
    expect(item(/^tool/)).toHaveFocus();
    await userEvent.keyboard("t");
    expect(item(/^two/)).toHaveFocus();
    // A letter nothing starts with moves nothing.
    await userEvent.keyboard("z");
    expect(item(/^two/)).toHaveFocus();
  });

  it("Enter does what a click does: a worktree is picked and a chat brought forward", async () => {
    const onPick = vi.fn();
    const onShowChat = vi.fn();
    draw({ chats: [chat(7, "seven", `${CUT}/one`)], onPick, onShowChat });

    item(/^one/).focus();
    await userEvent.keyboard("{Enter}");
    expect(onPick).toHaveBeenCalledWith({ repo: "svc", piece: "one", path: `${CUT}/one` });

    item(/^seven/).focus();
    await userEvent.keyboard("{Enter}");
    expect(onShowChat).toHaveBeenCalledWith(7);
  });

  it("leaves a key with a modifier alone", async () => {
    withAChat();
    item(/^svc/).focus();

    await userEvent.keyboard("{Meta>}{ArrowLeft}{/Meta}");
    expect(item(/^svc/)).toHaveAttribute("aria-expanded", "true");
    await userEvent.keyboard("{Control>}t{/Control}");
    expect(item(/^svc/)).toHaveFocus();
  });
});
