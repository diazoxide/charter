import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useEffect, useState } from "react";
import { ChatsSection } from "./ChatsSection";
import { ChatsHere, fixedChats, moved, nothingKnown, type ChatStates } from "./chatState";
import { chatsTree, type ChatRow, type ListedChat } from "./chatsTree";
import type { Reveal } from "./revealTask";
import { forgetThisLaunch } from "./regions";

/**
 * **This tab's rarer cases** (#1696): the Chats view drawn on its own, with the tab in front
 * handed to it as the window hands it, so a tab can be brought forward a draw after what asked
 * for it, as the window's own steps can.
 */

function listed(session: number, workspace: string, more: Partial<ListedChat> = {}): ListedChat {
  return {
    session,
    name: `steward ${session}`,
    persona: "steward",
    workspace,
    shell: false,
    parent: null,
    mode: null,
    from: null,
    tab: true,
    branch: null,
    report: null,
    outcome: null,
    asking: null,
    harness: "claude",
    ...more,
  };
}

const task = (session: number, parent: number, workspace: string, tab = false) =>
  listed(session, workspace, {
    name: `devops ${session}`,
    persona: "devops",
    parent,
    mode: "task",
    from: `steward ${parent}`,
    tab,
  });

/** Chat 1's tab, with its task 2; chat 3's tab; both in alpha. Chat 5 in beta. */
const CHATS = [listed(1, "alpha"), task(2, 1, "alpha"), listed(3, "alpha"), listed(5, "beta")];
const EVERY = chatsTree(CHATS);
const ofTab = (...sessions: number[]): readonly ChatRow[] =>
  chatsTree(CHATS.filter((chat) => sessions.includes(chat.session)));

/** The board, with `queue` needing the person. */
function board(queue: number[] = []): ChatStates {
  return CHATS.reduce(
    (known, chat, at) =>
      moved(known, {
        plane: "/plane",
        session: chat.session,
        state: queue.includes(chat.session) ? "waiting" : "running",
        needs_you: queue.includes(chat.session),
        queue,
        moved_at: at + 1,
        sequence: at + 1,
        reports: [],
        refusals: [],
        children: [],
      }),
    nothingKnown,
  );
}

const theList = () => screen.getByRole("tree");
const shape = () =>
  within(theList())
    .queryAllByRole("treeitem")
    .map(
      (one) => `${one.getAttribute("aria-level")} ${one.querySelector(".session")?.textContent}`,
    );
const scopeOn = () =>
  within(screen.getByRole("radiogroup", { name: "Show the chats of" }))
    .getAllByRole("radio")
    .filter((one) => one.getAttribute("aria-checked") === "true")
    .map((one) => one.textContent);
const thisTab = () => userEvent.click(screen.getByRole("radio", { name: "This tab" }));

beforeEach(forgetThisLaunch);
afterEach(cleanup);

describe("This tab, in its rarer cases (#1696)", () => {
  it("reads a widening once the tab in front has settled: a tab that comes a draw later lists the row", async () => {
    let ask: (reveal: Reveal) => void = () => {};
    const told: [number, boolean][] = [];
    /** The window: a row asked for, and the tab of the chat it is in brought forward a draw
     *  after, as a step that does both can. */
    function Window() {
      const [reveal, setReveal] = useState<Reveal>();
      const [tab, setTab] = useState({ id: 1, chats: ofTab(1, 2) });
      ask = setReveal;
      useEffect(() => {
        if (reveal !== undefined) setTab({ id: 3, chats: ofTab(3) });
      }, [reveal]);
      return (
        <ChatsHere.Provider value={fixedChats(board())}>
          <ChatsSection
            rows={EVERY}
            here="alpha"
            tab={tab}
            onOpen={() => {}}
            reveal={reveal}
            onRevealed={(asked, shown) => told.push([asked.asker, shown])}
          />
        </ChatsHere.Provider>
      );
    }
    render(<Window />);
    await thisTab();
    expect(shape()).toEqual(["1 steward 1", "2 devops 2"]);

    act(() => ask({ asker: 3, at: 1 }));

    await waitFor(() => expect(told).toEqual([[3, true]]));
    expect(shape()).toEqual(["1 steward 3"]);
    expect(scopeOn()).toEqual(["This tab"]);
  });

  it("still widens to the workspace for a row no tab brought forward", async () => {
    let ask: (reveal: Reveal) => void = () => {};
    function Window() {
      const [reveal, setReveal] = useState<Reveal>();
      ask = setReveal;
      return (
        <ChatsHere.Provider value={fixedChats(board())}>
          <ChatsSection
            rows={EVERY}
            here="alpha"
            tab={{ id: 1, chats: ofTab(1, 2) }}
            onOpen={() => {}}
            reveal={reveal}
          />
        </ChatsHere.Provider>
      );
    }
    render(<Window />);
    await thisTab();

    act(() => ask({ asker: 3, at: 1 }));

    await waitFor(() => expect(scopeOn()).toEqual(["Workspace"]));
    expect(shape()).toEqual(["1 steward 1", "2 devops 2", "1 steward 3"]);
  });

  it("lists nothing with a view tab in front, and names every chat that needs you as outside it", async () => {
    render(
      <ChatsHere.Provider value={fixedChats(board([3]))}>
        <ChatsSection rows={EVERY} here="alpha" tab={{ id: 9, chats: [] }} onOpen={() => {}} />
      </ChatsHere.Provider>,
    );
    await thisTab();

    expect(screen.queryAllByRole("treeitem")).toEqual([]);
    expect(await screen.findByText("steward 3 needs you, outside this tab.")).toBeTruthy();
  });

  it("heads its tab's list with a task in a tab of its own, its asker not listed", async () => {
    const chats = [listed(1, "alpha"), task(2, 1, "alpha", true), task(4, 2, "alpha")];
    render(
      <ChatsHere.Provider value={fixedChats(board())}>
        <ChatsSection
          rows={chatsTree(chats)}
          here="alpha"
          tab={{ id: 2, chats: chatsTree(chats.filter((chat) => chat.session !== 1)) }}
          onOpen={() => {}}
        />
      </ChatsHere.Provider>,
    );
    await thisTab();

    expect(shape()).toEqual(["1 devops 2", "2 devops 4"]);
  });

  it("lists the tab's chats with the project's root focused, as with a workspace", async () => {
    const chats = [listed(1, "plane root"), task(2, 1, "alpha"), listed(3, "plane root")];
    render(
      <ChatsHere.Provider value={fixedChats(board())}>
        <ChatsSection
          rows={chatsTree(chats)}
          here="plane root"
          tab={{ id: 1, chats: chatsTree(chats.slice(0, 2)) }}
          onOpen={() => {}}
        />
      </ChatsHere.Provider>,
    );
    expect(shape()).toEqual(["1 steward 1", "2 devops 2", "1 steward 3"]);

    await thisTab();

    expect(shape()).toEqual(["1 steward 1", "2 devops 2"]);
  });
});
