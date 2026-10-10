/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Explorer, type Spot } from "./Explorer";
import { ChatsHere, fixedChats, moved, nothingKnown, type ChatStates } from "./chatState";
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
  paths: {},
  absent: [],
  refused: [],
  todos: [],
  todos_refused: null,
  personas: ["steward"],
  persona: "steward",
  sessions: [],
  contributed: [],
};

function piece(name: string, on: Partial<Piece> = {}): Piece {
  return {
    piece: name,
    path: `${CUT}/${name}`,
    branch: name,
    wired: true,
    stale: false,
    said: "",
    ...on,
  };
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
    card: null,
    guessed: null,
    pinned: false,
    label: null,
    from: null,
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
  states?: ChatStates;
}) {
  render(
    // What the chats are doing is read by each row off the project's store (SC-3), which is
    // the one `PlaneView` provides; here it is one that holds `states` and never moves.
    <ChatsHere.Provider value={fixedChats(on.states ?? nothingKnown)}>
      <Explorer
        workspace={"workspace" in on ? on.workspace : "alpha"}
        state={on.state ?? state()}
        chats={on.chats ?? []}
        spot={on.spot}
        onPick={on.onPick ?? (() => {})}
        onShowChat={on.onShowChat ?? (() => {})}
        offers={on.offers ?? new Map()}
        onPress={on.onPress ?? (() => {})}
        onReadAgain={() => {}}
      />
    </ChatsHere.Provider>,
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
    expect(screen.getByTestId("clone-tool")).toHaveTextContent("No branches cut here");
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

  it("says a worktree has no purlis layer before a chat is started in it", () => {
    // `unwired` is what the operator has to see BEFORE they click: a chat started in such a
    // tree runs with none of the plane's ask/deny rules and no persona agents.
    draw({ state: state({ pieces: { svc: [piece("one", { wired: false })], tool: [] } }) });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("unwired");
  });

  it("says what each piece declared, or how long it has been silent", () => {
    // charter#368: a finished piece and a quiet one must not look alike on the row.
    draw({
      state: state({
        pieces: {
          svc: [
            piece("one", { said: "done" }),
            piece("two", { said: "abandoned: wrong approach" }),
            piece("three", { said: "silent 3d" }),
            piece("four"),
          ],
          tool: [],
        },
      }),
    });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("done");
    expect(screen.getByTestId("piece-svc-two")).toHaveTextContent("abandoned: wrong approach");
    expect(screen.getByTestId("piece-svc-three")).toHaveTextContent("silent 3d");
    expect(screen.getByTestId("piece-svc-four").querySelector("[data-testid=piece-said]")).toBe(
      null,
    );
  });

  it("says a branch cut for a chat that never started is unclaimed, and for how long", () => {
    // #835: shown, never swept. Removing it stays the row's own action.
    draw({
      state: state({
        pieces: { svc: [piece("one", { unclaimed: "3d" }), piece("two")], tool: [] },
      }),
    });

    const mark = screen.getByTestId("piece-svc-one").querySelector("[data-testid=piece-unclaimed]");
    expect(mark).toHaveTextContent("unclaimed 3d");
    expect(mark).toHaveAttribute("title", expect.stringContaining("never started"));
    expect(screen.getByTestId("piece-svc-two").querySelector("[data-testid=piece-unclaimed]")).toBe(
      null,
    );
  });

  it("says a registration whose directory is gone is stale", () => {
    draw({ state: state({ pieces: { svc: [piece("one", { stale: true })], tool: [] } }) });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("stale");
  });

  it("names the branch each worktree is on", () => {
    draw({ state: state({ pieces: { svc: [piece("one", { branch: "fix/login" })], tool: [] } }) });

    expect(screen.getByTestId("piece-svc-one")).toHaveTextContent("fix/login");
  });

  it("reads a branch's row as its branch in its repo, with its folder kept in the tooltip (#1102)", () => {
    // ADR 0072 §4: the row is *`fix/login` in svc*. The folder's name is where git keeps the
    // branch, and is shown only where a path is wanted.
    draw({ state: state({ pieces: { svc: [piece("one", { branch: "fix/login" })], tool: [] } }) });

    const row = within(screen.getByTestId("piece-svc-one")).getByRole("treeitem", {
      name: "fix/login in svc",
    });
    expect(row).toHaveAttribute("title", `${CUT}/one`);
    // Said once: the mark beside the row no longer repeats the branch.
    expect(within(screen.getByTestId("piece-svc-one")).getAllByText("fix/login")).toHaveLength(1);
  });

  it("reads a folder with no branch checked out by the folder's name (#1102)", () => {
    draw({ state: state({ pieces: { svc: [piece("one", { branch: null })], tool: [] } }) });

    expect(
      within(screen.getByTestId("piece-svc-one")).getByRole("treeitem", { name: "one" }),
    ).toBeInTheDocument();
  });

  it("says why a clone's worktrees could not be listed rather than showing none", () => {
    draw({
      state: state({
        pieces: { tool: [] },
        piecesRefused: { svc: "purlis will not run git through a symlink" },
      }),
    });

    const svc = screen.getByTestId("clone-svc");
    const refused = within(svc).getByRole("status");
    expect(refused).toHaveTextContent("symlink");
    // Its way out (NO-4): the read asked again.
    expect(within(refused).getByRole("button", { name: "Read again" })).toBeInTheDocument();
    expect(svc).not.toHaveTextContent("No branches cut here");
  });

  it("says the listing is still coming rather than saying there is nothing", () => {
    draw({ state: state({ pieces: {} }) });

    expect(screen.getByTestId("clone-svc")).toHaveTextContent("Asking git…");
  });

  // ---------------------------------------------------------------------------------------
  // What is already running where
  // ---------------------------------------------------------------------------------------

  it("draws no state on a shell tab's chat, and what is not known on a harness's", () => {
    // A shell's terminal icon already says what it is; a broken ring beside it read as a
    // spinner. A harness that has reported nothing yet still says so (#1484).
    draw({
      chats: [
        chat(1, "shell 1", `${CUT}/one`, { harness: null, profile: null }),
        chat(2, "ide.2", `${CUT}/one`),
      ],
    });

    const mark = (name: string) =>
      screen.getByRole("treeitem", { name: new RegExp(name) }).querySelector("[data-state]");
    expect(mark("shell 1")).toBeNull();
    expect(mark("ide.2")).toHaveAttribute("data-state", "unheard");
    expect(mark("ide.2")).toHaveTextContent("running (no detail from claude)");
  });

  it("draws a shell tab's state once a harness in it reports one", () => {
    const states = moved(nothingKnown, {
      plane: "/home/dev/plane",
      session: 1,
      state: "waiting",
      needs_you: true,
      queue: [1],
      moved_at: 1,
      reports: [],
      refusals: [],
      sequence: 1,
      children: [],
    });
    draw({ chats: [chat(1, "shell 1", `${CUT}/one`, { harness: null, profile: null })], states });

    expect(
      screen.getByRole("treeitem", { name: /shell 1/ }).querySelector("[data-state]"),
    ).toHaveAttribute("data-state", "needs-you");
  });

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

  it("says a chat's helpers as a count on its row, and lists none until it is unfolded (FD-18, #1490)", () => {
    const states = moved(nothingKnown, {
      plane: "/home/dev/plane",
      session: 1,
      state: "failed",
      needs_you: false,
      queue: [],
      moved_at: 1,
      reports: [],
      refusals: [],
      sequence: 1,
      children: [
        { agent: "a1b2c3d4e5f6a7b8c9d0", state: "failed" },
        { agent: "thread-1", state: "done" },
        { agent: "thread-10", state: "running" },
      ],
    });
    draw({ chats: [chat(1, "ide.1", `${CUT}/one`), chat(2, "ide.2", `${CUT}/one`)], states });

    // A row of the tree, folded: what unfolds it and what it then lists is
    // `Explorer.tasks.test.tsx`'s.
    const count = screen.getByRole("treeitem", { name: "3 helpers of ide.1, 1 working, 1 failed" });
    expect(count).toHaveAttribute("aria-expanded", "false");
    // The chat's row is described by it, so a screen reader on it hears how many it spawned.
    expect(screen.getByRole("treeitem", { name: /^ide\.1/ })).toHaveAccessibleDescription(
      /3 helpers/,
    );
    expect(screen.getByRole("treeitem", { name: /^ide\.2/ })).not.toHaveAccessibleDescription(
      /helper/,
    );
    // No row of ids until it is asked for.
    expect(screen.queryByRole("treeitem", { name: /helper thread/ })).toBeNull();
    expect(screen.getByTestId("explorer")).not.toHaveTextContent("a1b2c3d4");
  });

  it("lists a task that has a tab of its own beside the chat that asked, by name and state (#1436, #1490)", () => {
    const states = moved(nothingKnown, {
      plane: "/home/dev/plane",
      session: 7,
      state: "running",
      needs_you: false,
      queue: [],
      moved_at: 1,
      reports: [],
      refusals: [],
      sequence: 1,
      children: [],
    });
    // Each has a tab: a task has none unless the person gave it one, and one with none is
    // counted on its session's line and not listed (`Explorer.tasks.test.tsx`).
    const asked = {
      name: "steward 1",
      workspace: "alpha",
      chat: 1,
      tab: true,
      reported: false,
      unreported: false,
    };
    draw({
      chats: [
        chat(1, "steward 1", ALPHA),
        chat(2, "steward 2", ALPHA),
        chat(7, "check the queue", ALPHA, { from: { ...asked, task: true } }),
        // A handoff is the person's to follow: it is listed beside the chat it came from.
        chat(8, "moved on", ALPHA, { from: { ...asked, task: false } }),
      ],
      states,
    });

    const asking = screen.getByRole("treeitem", { name: /steward 1/ });
    const task = screen.getByRole("treeitem", { name: /check the queue/ });
    // A row of its own at the place it works. Who asked whom is the Chats list's to say.
    expect(asking.closest("li")).not.toContainElement(task);
    expect(task.getAttribute("aria-level")).toBe(asking.getAttribute("aria-level"));
    expect(task.querySelector("[data-state]")).toHaveAttribute("data-state", "working");
    expect(task.querySelector("[data-state]")).toHaveTextContent("working");
    const handoff = screen.getByRole("treeitem", { name: /moved on/ });
    expect(handoff.getAttribute("aria-level")).toBe(asking.getAttribute("aria-level"));
    expect(asking.closest("li")).not.toContainElement(handoff);
    // It is still one of the session's tasks, and the session's line counts it.
    expect(asking.closest("li")).toContainElement(
      screen.getByRole("treeitem", { name: "1 task · 1 working" }),
    );
  });

  it("lists a task whose asking chat is not at this spot beside the other chats", () => {
    draw({
      chats: [
        chat(2, "steward 2", ALPHA),
        chat(7, "check the queue", ALPHA, {
          from: {
            name: "steward 1",
            workspace: "alpha",
            chat: 1,
            task: true,
            tab: true,
            reported: false,
            unreported: false,
          },
        }),
      ],
    });

    const task = screen.getByRole("treeitem", { name: /check the queue/ });
    expect(task.getAttribute("aria-level")).toBe(
      screen.getByRole("treeitem", { name: /steward 2/ }).getAttribute("aria-level"),
    );
  });

  it("marks a task that has reported, and one that ended without reporting, and both still open", async () => {
    const onShowChat = vi.fn();
    const from = { name: "steward 1", workspace: "alpha", chat: 1, task: true, tab: true };
    draw({
      chats: [
        chat(1, "steward 1", ALPHA),
        chat(7, "check the queue", ALPHA, { from: { ...from, reported: true, unreported: false } }),
        chat(8, "check prod", ALPHA, { from: { ...from, reported: false, unreported: false } }),
        chat(9, "check staging", ALPHA, { from: { ...from, reported: false, unreported: true } }),
      ],
      onShowChat,
    });

    const reported = screen.getByRole("treeitem", { name: /check the queue/ });
    expect(reported).toHaveTextContent("reported");
    expect(screen.getByRole("treeitem", { name: /check prod/ })).not.toHaveTextContent("report");
    // Never "reported" for a chat that did not: purlis reported in its place.
    const ended = screen.getByRole("treeitem", { name: /check staging/ });
    expect(ended).toHaveTextContent("ended without a report");
    // Still a chat: its row opens it.
    await userEvent.click(reported);
    expect(onShowChat).toHaveBeenCalledWith(7);
  });

  it("brings a task forward when its row is pressed, as any chat", async () => {
    const onShowChat = vi.fn();
    draw({
      chats: [
        chat(1, "steward 1", ALPHA),
        chat(7, "check the queue", ALPHA, {
          from: {
            name: "steward 1",
            workspace: "alpha",
            chat: 1,
            task: true,
            tab: true,
            reported: false,
            unreported: false,
          },
        }),
      ],
      onShowChat,
    });

    await userEvent.click(screen.getByRole("treeitem", { name: /check the queue/ }));

    expect(onShowChat).toHaveBeenCalledWith(7);
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

  it("shows what purlis would not read instead of a workspace with fewer repos in it", () => {
    draw({
      state: state({
        panels: {
          ...PANELS,
          repos: ["svc"],
          refused: [["alias", "'alias' is reached through a symlink"]],
        },
      }),
    });

    const refused = screen
      .getByText(/'alias' is reached through a symlink/)
      .closest("[data-cause]") as HTMLElement;
    expect(refused.getAttribute("data-cause")).toBe("repo-refused:alpha/alias");
    expect(within(refused).getByRole("button", { name: "Read again" })).toBeInTheDocument();
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
  const cut = { workspace: "alpha", repo: "svc", piece: "one", branch: "one" };
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

    const row = within(screen.getByTestId("piece-svc-one")).getByRole("treeitem", {
      name: "one in svc",
    });
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));

    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((one) => one.getAttribute("aria-label")),
    ).toEqual([
      "Focus on branch one",
      "Browse the files of one",
      // The branch's own folder (#1143).
      "Copy the absolute path of branch one",
      expect.stringMatching(/^Reveal branch one in /),
      "Open a shell tab in branch one",
      "Merge branch one into svc",
      "Remove folder one in svc",
    ]);
  });

  it("hands the catalogue's offer back when a row is pressed", async () => {
    const pressed: string[] = [];
    draw({ offers: offers(), onPress: (offer) => pressed.push(offer.id) });

    const row = within(screen.getByTestId("piece-svc-one")).getByRole("treeitem", {
      name: "one in svc",
    });
    row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    await screen.findByRole("menu");
    await userEvent.click(screen.getByRole("menuitem", { name: "Remove folder one in svc" }));

    expect(pressed).toEqual(["worktree.remove:svc/one"]);
  });
});

/**
 * Right-click on a clone's heading (charter-app#174, the second half).
 *
 * It had no menu because nothing in the catalogue was about a clone, and nothing could be until
 * the core said where one is. `Panels.paths` says so now, and the catalogue has two rows per
 * clone: a new tab in it, and picking it as where new chats start.
 */
describe("a clone row's menu", () => {
  const SVC = `${ALPHA}/svc`;
  const offers = (startsIn?: string) =>
    catalogued(
      catalogue({
        tabs: noTabs(),
        workspaces: ["alpha"],
        focused: "alpha",
        plane: "/plane",
        clones: [{ repo: "svc", path: SVC }],
        startsIn,
        needsYou: [],
        nameOf: String,
      }),
    );
  const heading = () =>
    within(screen.getByTestId("clone-svc")).getByRole("treeitem", { name: /^svc/ });

  it("opens on a right-click with the catalogue's own rows for that clone", async () => {
    draw({ offers: offers() });

    heading().dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));

    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((one) => one.getAttribute("aria-label")),
    ).toEqual([
      "Focus on repo svc",
      "New tab in svc",
      "New branch in svc…",
      "Start new chats in svc",
    ]);
  });

  it("hands the catalogue's offer back when a row is pressed", async () => {
    const pressed: string[] = [];
    draw({ offers: offers(), onPress: (offer) => pressed.push(offer.id) });

    heading().dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    await screen.findByRole("menu");
    await userEvent.click(screen.getByRole("menuitem", { name: "Start new chats in svc" }));

    expect(pressed).toEqual(["clone.pick:svc"]);
  });

  it("opens from the keyboard on the clone's row, which the arrows reach", async () => {
    const pressed: string[] = [];
    draw({ offers: offers(), onPress: (offer) => pressed.push(offer.id) });
    await userEvent.tab();
    await userEvent.keyboard("{ArrowDown}");
    expect(heading()).toHaveFocus();

    await userEvent.keyboard("{Shift>}{F10}{/Shift}");
    await screen.findByRole("menu");
    await userEvent.keyboard("{Enter}");

    // The first row, its own folder's cockpit (#1152).
    expect(pressed).toEqual(["clone.focus:svc"]);
  });

  it("marks a picked clone as where the next chat starts, and only it", () => {
    draw({ offers: offers(SVC), spot: { repo: "svc", path: SVC } });

    expect(picked()).toEqual([expect.stringMatching(/^svc/)]);
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
    expect(screen.getByRole("treeitem", { name: /^one in svc$/ })).toBeInTheDocument();
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
 * has no branches. Each repo and each branch also has its *Files* row first among its children
 * (FM-1), closed until opened.
 */
describe("the explorer is a WAI-ARIA tree", () => {
  const withAChat = () => draw({ chats: [chat(7, "seven", `${CUT}/one`)] });
  const tree = () => screen.getByRole("tree", { name: "Repos and branches" });
  const item = (name: RegExp) => within(tree()).getByRole("treeitem", { name });
  /** A row by the id the tree knows it by: the *Files* rows all share a name. */
  const row = (id: string) => {
    const found = tree().querySelector<HTMLElement>(`[data-row="${id}"]`);
    if (found === null) throw new Error(`no row ${id}`);
    return found;
  };
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
      "Files 3 1/3",
      "one 3 2/3",
      "Files 4 1/2",
      "seven 4 2/2",
      "two 3 3/3",
      "Files 4 1/1",
      "tool 2 2/2",
      "Files 3 1/1",
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
    expect(item(/^two/)).toHaveAttribute("aria-expanded", "true");
    expect(item(/^seven/)).not.toHaveAttribute("aria-expanded");
    // A branch's files fold, and are closed until opened.
    expect(row("file:svc/one:")).toHaveAttribute("aria-expanded", "false");

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

    // Its first child is the repo's own Files row.
    await userEvent.keyboard("{ArrowRight}");
    expect(row("file:svc/:")).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(item(/^one/)).toHaveFocus();
    // A branch is a parent that is always open: Right goes in, to its Files row first.
    await userEvent.keyboard("{ArrowRight}");
    expect(row("file:svc/one:")).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
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
    expect(row("file:tool/:")).toHaveFocus();
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

  it("a typed letter finds a branch's row by the branch it reads, not its folder (#1102)", async () => {
    draw({ state: state({ pieces: { svc: [piece("one", { branch: "work/login" })], tool: [] } }) });
    item(/^alpha/).focus();

    await userEvent.keyboard("w");
    expect(item(/^work\/login in svc/)).toHaveFocus();
    // The folder's name is not what the row reads, so its letter finds nothing.
    await userEvent.keyboard("o");
    expect(item(/^work\/login in svc/)).toHaveFocus();
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
