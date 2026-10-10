import type { MemoryScope, RowAction } from "./bindings";
import { DRAFT, memoryKey, memoryView, type MemoryRef } from "./memories";
import { describe, expect, it, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import {
  aim,
  catalogue,
  paneCloseOf,
  catalogued,
  curateRows,
  curateSubjectOf,
  ENDS_IT,
  FILE_VERBS,
  fileRows,
  KEEPS_THE_BRANCH,
  matches,
  menuOn,
  memoryOffers,
  menuRows,
  narrow,
  needsYouRows,
  stopBelowId,
  stopAllId,
  STOPS_ALL,
  stopId,
  handedOffId,
  handedOffRows,
  stopRows,
  taskCloseId,
  taskEndIds,
  taskEndRows,
  taskStopId,
  CLOSES_THE_TASK,
  STOPS_THE_TASK,
  STOPS_IT,
  toKeep,
  OUTSIDE,
  READ_AGAIN,
  perform,
  revealSaid,
  SHELL_KEY_SAID,
  SEARCH_ID,
  SEARCH_KEY_SAID,
  SWITCHER_KEY_SAID,
  harnessCardId,
  type BranchPath,
  type Cut,
  type Doing,
  type Now,
  type Offer,
  type Ran,
  RESTART_NOTE,
  noteOf,
  restartNoteNoState,
  titleOf,
  ownTabId,
  besideId,
} from "./actions";
import { ASK_LOCKED_ID, askId, askRows, inPalette } from "./actions";
import { NO_LINK_FOLLOWED, listedMemoryOffers, memoryMoveId, moveRows } from "./actions";
import { PUBLISHED_WITH_THE_PROJECT } from "./memoryMoves";
import { ANSWER_SAYS, BRIEF_SAYS, answerId, answerRows, briefId, briefRows } from "./actions";
import type { ListedChat } from "./chatsTree";
import {
  SETTINGS_LINK,
  forgetGroups,
  settingsPlace,
  useShownGroup,
  type SettingsLinkAsk,
} from "./settings/links";
import {
  noTabs,
  openTab,
  openPreview,
  openView,
  selectTab,
  panesOf,
  splitFocusedPane,
  switchTabTo,
  viewKey,
  type Tabs,
  type ViewRef,
} from "./tabs";

/** Nothing happens unless a test says it does, and each call is counted. */
function doing(): Doing & { calls: string[] } {
  const calls: string[] = [];
  const note =
    (what: string) =>
    (...args: unknown[]) => {
      calls.push(args.length === 0 ? what : `${what}:${args.join(",")}`);
    };
  return {
    calls,
    newChat: note("newChat"),
    newShell: note("newShell"),
    resumeSession: async (...args: unknown[]) => {
      note("resumeSession")(...args);
      return { ok: true };
    },
    split: note("split"),
    closePane: note("closePane"),
    closeTab: note("closeTab"),
    selectTab: note("selectTab"),
    renameTab: note("renameTab"),
    linkWorkItem: note("linkWorkItem"),
    startFresh: note("startFresh"),
    restartChat: note("restartChat"),
    restartListed: note("restartListed"),
    askPersona: note("askPersona"),
    unlinkWorkItem: async (...args: unknown[]) => {
      note("unlinkWorkItem")(...args);
      return { ok: true };
    },
    focusWorkspace: note("focusWorkspace"),
    createWorkspace: note("createWorkspace"),
    removeWorkspace: note("removeWorkspace"),
    showChat: note("showChat"),
    showTabTasks: note("showTabTasks"),
    ownTab: note("ownTab"),
    beside: note("beside"),
    sendBack: note("sendBack"),
    pickVault: note("pickVault"),
    createVault: note("createVault"),
    removeVault: note("removeVault"),
    createPersona: note("createPersona"),
    removePersona: note("removePersona"),
    setPersonaProfile: note("setPersonaProfile"),
    editPersona: vi.fn(async (persona: string) => {
      calls.push(`editPersona:${persona}`);
      return { ok: true as const };
    }),
    closeTodo: vi.fn(async (workspace: string, slug: string) => {
      calls.push(`closeTodo:${workspace},${slug}`);
      return { ok: true as const };
    }),
    forgetTodo: vi.fn(async (workspace: string, slug: string) => {
      calls.push(`forgetTodo:${workspace},${slug}`);
      return { ok: true as const };
    }),
    ignoreNeedsYou: vi.fn(async (session: number) => {
      calls.push(`ignoreNeedsYou:${session}`);
      return { ok: true as const };
    }),
    cancelSmartClose: vi.fn(async (session: number) => {
      calls.push(`cancelSmartClose:${session}`);
      return { ok: true as const };
    }),
    dismissStopped: vi.fn((session: number) => {
      calls.push(`dismissStopped:${session}`);
    }),
    stopChat: vi.fn((session: number, below: boolean) => {
      calls.push(`stopChat:${session}:${below}`);
    }),
    stopAllTasks: vi.fn((session: number) => {
      calls.push(`stopAllTasks:${session}`);
    }),
    endTask: vi.fn((session: number, way: string) => {
      calls.push(`endTask:${session}:${way}`);
    }),
    answerQuestion: vi.fn((session: number) => {
      calls.push(`answerQuestion:${session}`);
    }),
    showBrief: vi.fn((session: number) => {
      calls.push(`showBrief:${session}`);
    }),
    pinTab: vi.fn(async (tab: number, pinned: boolean) => {
      calls.push(`pinTab:${tab},${pinned}`);
      return { ok: true as const };
    }),
    pinWorkspace: vi.fn(async (workspace: string, pinned: boolean) => {
      calls.push(`pinWorkspace:${workspace},${pinned}`);
      return { ok: true as const };
    }),
    pinProject: vi.fn(async (plane: string, pinned: boolean) => {
      calls.push(`pinProject:${plane},${pinned}`);
      return { ok: true as const };
    }),
    curate: vi.fn(async (subject: string, action: string) => {
      calls.push(`curate:${subject},${action}`);
      return { ok: true as const };
    }),
    runAction: vi.fn(async (extension: string, action: RowAction, name: string) => {
      calls.push(`runAction:${extension},${action.id},${name}`);
      return { ok: true as const };
    }),
    openView: vi.fn((view: ViewRef, title: string) => {
      calls.push(`openView:${viewKey(view)},${title}`);
    }),
    // The piece is part of what was called, because "which worktree did that row mean" is the
    // whole of what charter-app#174 changed here.
    removeWorktree: vi.fn(async (cut: Cut, force: boolean) => {
      calls.push(`removeWorktree:${cut.repo}/${cut.piece},${force}`);
      return { ok: true as const };
    }),
    mergeWorktree: vi.fn(async (cut: Cut) => {
      calls.push(`mergeWorktree:${cut.repo}/${cut.piece}`);
      return { ok: true as const };
    }),
    declareWorktreeDone: vi.fn(async (cut: Cut) => {
      calls.push(`declareWorktreeDone:${cut.repo}/${cut.piece}`);
      return { ok: true as const };
    }),
    pickClone: note("pickClone"),
    focusRepo: note("focusRepo"),
    focusBranch: vi.fn((cut: Cut) => {
      calls.push(`focusBranch:${cut.repo}/${cut.piece}`);
    }),
    newBranch: note("newBranch"),
    cloneMissing: vi.fn(async (workspace: string, repos: string[]) => {
      calls.push(`cloneMissing:${workspace}:${repos.join(",")}`);
      return { ok: true as const };
    }),
    askDropMembership: vi.fn((workspace: string, repo: string) => {
      calls.push(`askDropMembership:${workspace}:${repo}`);
    }),
    newChatIn: note("newChatIn"),
    sendKey: vi.fn(async (key: string) => {
      calls.push(`sendKey:${key}`);
      return { ok: true as const };
    }),
    openProject: note("openProject"),
    createProject: note("createProject"),
    showExtensions: note("showExtensions"),
    showSideView: note("showSideView"),
    toggleRegion: note("toggleRegion"),
    installCli: vi.fn(async () => {
      calls.push("installCli");
      return { ok: true as const, said: "On PATH" };
    }),
    selectProject: note("selectProject"),
    switchProject: note("switchProject"),
    closeProject: vi.fn(async (plane: string) => {
      calls.push(`closeProject:${plane}`);
      return { ok: true as const };
    }),
    moveProject: vi.fn(async (plane: string, to: string | null) => {
      calls.push(`moveProject:${plane},${to ?? "new"}`);
      return { ok: true as const };
    }),
    openSettings: vi.fn((plane: string) => {
      calls.push(`openSettings:${plane}`);
    }),
    openSaving: note("openSaving"),
    openWorkspaceSettings: note("openWorkspaceSettings"),
    switchLive: note("switchLive"),
    renameWorkspace: note("renameWorkspace"),
    openSettingsTab: note("openSettingsTab"),
    openYourSettings: note("openYourSettings"),
    readAgain: note("readAgain"),
    taskBranch: note("taskBranch"),
    openMemory: vi.fn((ref: MemoryRef, title: string, keep: boolean) => {
      calls.push(`openMemory:${memoryKey(ref)},${title},${keep}`);
    }),
    editMemory: vi.fn((ref: MemoryRef, title: string) => {
      calls.push(`editMemory:${memoryKey(ref)},${title}`);
    }),
    archiveMemory: vi.fn(async (ref: MemoryRef, title: string) => {
      calls.push(`archiveMemory:${memoryKey(ref)},${title}`);
      return { ok: true as const };
    }),
    moveMemory: vi.fn(async (ref: MemoryRef, title: string, to: MemoryScope) => {
      calls.push(`moveMemory:${memoryKey(ref)},${title},${JSON.stringify(to)}`);
      return { ok: true as const };
    }),
    newMemory: vi.fn((scope: MemoryScope) => {
      calls.push(`newMemory:${JSON.stringify(scope)}`);
    }),
    keepTab: note("keepTab"),
    quit: note("quit"),
    copyPath: vi.fn(async (at: BranchPath, absolute: boolean) => {
      calls.push(`copyPath:${at.repo}/${at.piece ?? ""}:${at.path},${absolute}`);
      return { ok: true as const };
    }),
    revealPath: vi.fn(async (at: BranchPath) => {
      calls.push(`revealPath:${at.repo}/${at.piece ?? ""}:${at.path}`);
      return { ok: true as const };
    }),
    openInEditor: vi.fn(async (at: BranchPath, line: number) => {
      calls.push(`openInEditor:${at.repo}/${at.piece ?? ""}:${at.path},${line}`);
      return { ok: true as const };
    }),
    shellInFolder: vi.fn((at: BranchPath) => {
      calls.push(`shellInFolder:${at.repo}/${at.piece ?? ""}:${at.path}`);
    }),
    startChatHere: vi.fn(async (at: BranchPath) => {
      calls.push(`startChatHere:${at.repo}/${at.piece ?? ""}:${at.path}`);
      return { ok: true as const };
    }),
    addToChat: vi.fn((at: BranchPath, folder: boolean) => {
      calls.push(`addToChat:${at.repo}/${at.piece ?? ""}:${at.path},${folder}`);
    }),
  };
}

const PIECE = {
  workspace: "alpha",
  repo: "svc",
  piece: "fix-it",
  branch: "charter/fix-it",
  wired: false,
  stale: false,
};

function now(over: Partial<Now> = {}): Now {
  return {
    tabs: noTabs(),
    workspaces: [],
    needsYou: [],
    nameOf: (session) => String(session),
    ...over,
  };
}

const ids = (offers: readonly Offer[]) => offers.map((offer) => offer.id);
const by = (offers: readonly Offer[], id: string) => offers.find((offer) => offer.id === id);

/** Carries out one row of a catalogue, the way the window does when a surface asks. */
const run = (offers: readonly Offer[], id: string, hands: Doing) => {
  const offer = by(offers, id);
  if (!offer) throw new Error(`no row called ${id}`);
  return perform(offer, hands);
};

/** The group shown at a Settings place, read the way a Settings tab reads it. */
function shownAt(place: string): string | undefined {
  const { result, unmount } = renderHook(() => useShownGroup(place));
  const group = result.current?.group;
  unmount();
  return group;
}

describe("the one list of actions", () => {
  it("offers every verb the window has, on an empty window", () => {
    const offers = catalogue(now());

    // The verbs that do not depend on anything being open are all here, even with no chat.
    expect(ids(offers)).toEqual(
      expect.arrayContaining([
        "chat.new",
        "pane.split.right",
        "pane.split.down",
        "needs.next",
        "worktree.merge",
        "pane.close",
        "worktree.remove",
        "charter.quit",
      ]),
    );
  });

  it("lists an action that cannot run WITH its reason, rather than dropping it", () => {
    // An operator cannot ask about an option they cannot see — #512, one surface along.
    const offers = catalogue(now());

    const split = by(offers, "pane.split.right");
    expect(split?.available).toBe(false);
    expect(split?.reason).toBe("No chat is in front, so there is no pane to split.");
  });

  it("gives every unavailable row a reason and every available row none", () => {
    // The pair cannot contradict itself on screen if it cannot contradict itself here.
    const offers = catalogue(
      now({
        tabs: openTab(noTabs(), 7, "one"),
        workspaces: ["alpha", "beta"],
        focused: "alpha",
        plane: "/plane",
        worktree: PIECE,
        needsYou: [7],
      }),
    );

    for (const offer of offers) {
      expect(offer.reason === "").toBe(offer.available);
    }
  });

  it("puts the destructive rows last, so Enter on a fresh palette never closes a chat", () => {
    const tabs = openTab(noTabs(), 7, "one");
    const offers = ids(catalogue(now({ tabs, plane: "/plane", worktree: PIECE })));

    for (const gentle of ["chat.new", "pane.split.right", "worktree.merge"]) {
      for (const sharp of ["pane.close", "tab.close:1", "worktree.remove", "charter.quit"]) {
        expect(offers.indexOf(gentle)).toBeLessThan(offers.indexOf(sharp));
      }
    }
  });

  it("runs the verb the window handed it, and nothing else", async () => {
    const tabs = openTab(noTabs(), 7, "one");
    const hands = doing();
    const offers = catalogue(now({ tabs }));

    await run(offers, "pane.split.right", hands);
    await run(offers, "pane.split.down", hands);
    await run(offers, "chat.new", hands);

    expect(hands.calls).toEqual(["split:row", "split:column", "newChat"]);
  });

  it("names one row per tab, and says of the one in front that it already is", () => {
    const tabs = selectTab(openTab(openTab(noTabs(), 7, "one"), 8, "two"), 1);
    const offers = catalogue(now({ tabs }));

    expect(by(offers, "tab.select:1")?.available).toBe(false);
    expect(by(offers, "tab.select:1")?.reason).toBe("It is already in front.");
    expect(by(offers, "tab.select:2")?.title).toBe("Switch to tab two");
    expect(by(offers, "tab.select:2")?.available).toBe(true);
  });

  it("names one row per workspace, and says of the focused one that it already is", () => {
    const offers = catalogue(now({ workspaces: ["alpha", "beta"], focused: "beta" }));

    expect(by(offers, "workspace.focus:beta")?.reason).toBe("It is already focused.");
    expect(by(offers, "workspace.focus:alpha")?.title).toBe("Focus workspace alpha");
  });

  it("gives the plane root words of its own", () => {
    // Its name is a sentinel that cannot be a directory, so the row cannot be built the way
    // the others are — and `Focus workspace outside/every/workspace` is not a sentence.
    const offers = catalogue(now({ workspaces: [OUTSIDE, "alpha"] }));

    expect(by(offers, `workspace.focus:${OUTSIDE}`)?.title).toBe("Focus the plane root");
    expect(by(offers, `workspace.focus:${OUTSIDE}`)?.name).toBeUndefined();
  });

  it("starts a chat and a shell at the plane root from its own rows (SI-1)", async () => {
    const done = doing();
    const offers = catalogue(now({ workspaces: [OUTSIDE, "alpha"], plane: "/plane" }));

    const chat = by(offers, "root.chat");
    const shell = by(offers, `shell.new:${OUTSIDE}`);
    expect(chat?.title).toBe("New chat at the project root");
    expect(shell?.title).toBe("New shell at the project root");
    if (chat === undefined || shell === undefined) throw new Error("no root rows");
    await perform(chat, done);
    await perform(shell, done);
    expect(done.calls).toEqual(["newChatIn:/plane", `newShell:${OUTSIDE}`]);
    // With no plane read there is no root to start in.
    expect(by(catalogue(now({ workspaces: [OUTSIDE] })), "root.chat")?.available).toBe(false);
  });

  it("says on the row that ends a chat what ending it costs", () => {
    // charter-app#130. The row is `End chat`, not `Close tab`: it calls `close_session`,
    // which ends the program. Nothing said so, and the `×` read as "hide this tab".
    const offers = catalogue(now({ tabs: openTab(noTabs(), 7, "3", "steward") }));

    expect(by(offers, "tab.close:1")?.title).toBe("End chat steward 3");
    expect(by(offers, "tab.close:1")?.note).toBe(ENDS_IT);
    // And not on rows that only navigate: a note on everything is a note on nothing.
    expect(by(offers, "tab.select:1")?.note).toBeUndefined();
    expect(by(offers, "chat.new")?.note).toBeUndefined();
  });

  it("says the same of the pane close, which ends a chat too", () => {
    const offers = catalogue(now({ tabs: openTab(noTabs(), 7, "3") }));

    expect(by(offers, "pane.close")?.title).toBe("End this pane's chat");
    expect(by(offers, "pane.close")?.note).toBe(ENDS_IT);
  });

  it("offers no pane close while the pane shows a task, and still ends the session from its tab", () => {
    // Chat 9 is a task of chat 7, shown inside chat 7's tab (#1486).
    const tabs = switchTabTo(openTab(noTabs(), 7, "3", "steward"), 9, (session) =>
      session === 9 ? 7 : undefined,
    );
    const offers = catalogue(now({ tabs, nameOf: () => "steward 3" }));

    expect(by(offers, "pane.close")?.available).toBe(false);
    expect(by(offers, "pane.close")?.reason).toBe(
      "This pane shows a task of steward 3. Go back to steward 3 to end its chat, or close the tab.",
    );
    // The tab is the session's, and its close is the session's.
    expect(by(offers, "tab.close:1")?.title).toBe("End chat steward 3");
    expect(by(offers, "tab.close:1")?.available).toBe(true);
    expect(by(offers, "tab.close:1")?.does).toEqual({ verb: "closeTab", tab: 1, ends: true });
  });

  it("decides a pane's close for that pane: the one beside a pane showing a task still closes", () => {
    // Chat 7's tab is split with chat 8 beside it, and chat 8's pane shows its task 9.
    const split = splitFocusedPane(openTab(noTabs(), 7, "3", "steward"), "row", 8);
    const tabs = switchTabTo(split, 9, (session) => (session === 9 ? 8 : undefined));
    const [first, second] = panesOf(tabs, 1);
    const names = (session: number) => (session === 8 ? "devops 8" : "steward 3");

    // The pane in focus shows the task: the catalogue's row is that pane's, and names ITS chat.
    expect(by(catalogue(now({ tabs, nameOf: names })), "pane.close")?.reason).toBe(
      "This pane shows a task of devops 8. Go back to devops 8 to end its chat, or close the tab.",
    );
    expect(paneCloseOf(tabs, second.pane, names).available).toBe(false);
    // The other pane shows its own chat, and its close is its own.
    expect(paneCloseOf(tabs, first.pane, names)).toMatchObject({
      available: true,
      title: "End this pane's chat",
      does: { verb: "closePane", ends: true },
    });
  });

  it("says on letting go of a project what goes with it and what does not", () => {
    const offers = catalogue(now({ projects: [{ plane: "/p/one", name: "one" }] }));

    expect(by(offers, "project.close:/p/one")?.note).toBe(
      "Ends every chat in it. Nothing of the project on disk goes.",
    );
  });

  it("offers every project's settings under the words its tab's menu uses, told apart by name", async () => {
    const hands = doing();
    const offers = catalogue(
      now({
        projects: [
          { plane: "/p/one", name: "one" },
          { plane: "/p/two", name: "two" },
        ],
      }),
    );

    expect(by(offers, "project.settings:/p/two")?.title).toBe("Project settings…");
    expect(by(offers, "project.settings:/p/two")?.note).toBe(
      "two: charter.toml, for the team, and charter.local.toml, for this machine.",
    );
    await run(offers, "project.settings:/p/two", hands);
    expect(hands.calls).toEqual(["openSettings:/p/two"]);
    // On the project tab's own menu, above the line: it opens a tab and ends nothing.
    expect(menuOn({ on: "project", plane: "/p/two" }).above).toContain("project.settings:/p/two");
  });

  it("offers each project's Saving tab in the palette and on its tab's menu", async () => {
    // charter-app#294: where a project's unsaved work sits, reached the way its settings are.
    const hands = doing();
    const offers = catalogue(
      now({
        projects: [
          { plane: "/p/one", name: "one" },
          { plane: "/p/two", name: "two" },
        ],
      }),
    );

    expect(by(offers, "project.saving:/p/two")?.title).toBe("Saving…");
    expect(by(offers, "project.saving:/p/two")?.note).toBe(
      "two: what is not saved yet, and the save button.",
    );
    await run(offers, "project.saving:/p/two", hands);
    expect(hands.calls).toEqual(["openSaving:/p/two"]);
    expect(menuOn({ on: "project", plane: "/p/two" }).above).toContain("project.saving:/p/two");
  });

  it("offers each workspace's settings on its menu and in the palette, and none outside every workspace", async () => {
    // charter-app#280: a workspace's settings are its workspace.json, so the strip of chats
    // outside every workspace has none to open.
    const hands = doing();
    const offers = catalogue(now({ workspaces: ["alpha", OUTSIDE], plane: "/p" }));

    const row = by(offers, "workspace.settings:alpha");
    expect(row?.title).toBe("Workspace settings…");
    expect(row?.note).toBe(
      "alpha: Settings at its level — live, repos, extensions, appearance and plugins.",
    );
    await run(offers, "workspace.settings:alpha", hands);
    expect(hands.calls).toEqual(["openWorkspaceSettings:alpha"]);
    expect(by(offers, `workspace.settings:${OUTSIDE}`)).toBeUndefined();
    // On the workspace tab's own menu, above the line: it opens a tab and ends nothing.
    expect(menuOn({ on: "workspace", workspace: "alpha" }).above).toContain(
      "workspace.settings:alpha",
    );
  });

  it("offers to rename each workspace but the chats outside every one, in the palette and its menu", async () => {
    // charter#367: a row in the palette and on the workspace tab's menu, above the line —
    // it discards nothing. It opens the dialog; the core refuses what it refuses.
    const hands = doing();
    const offers = catalogue(now({ workspaces: ["alpha", OUTSIDE], plane: "/plane" }));
    const row = by(offers, "workspace.rename:alpha");
    expect(row?.title).toBe("Rename workspace alpha…");
    expect(row?.available).toBe(true);
    await run(offers, "workspace.rename:alpha", hands);
    expect(hands.calls).toEqual(["renameWorkspace:alpha"]);
    expect(by(offers, `workspace.rename:${OUTSIDE}`)).toBeUndefined();
    expect(menuOn({ on: "workspace", workspace: "alpha" }).above).toContain(
      "workspace.rename:alpha",
    );
  });

  it("says a live switch publishes with the project, either way (#1192)", () => {
    const offers = catalogue(now({ workspaces: ["alpha", "beta"], live: ["beta"] }));

    expect(by(offers, "workspace.live:alpha")?.note).toBe(
      "Publish its charter, memory and todos with the project.",
    );
    expect(by(offers, "workspace.live:beta")?.note).toBe(
      "Stop publishing its charter, memory and todos with the project.",
    );
  });

  it("offers Settings everywhere, with no project open too, because You is the machine's", async () => {
    // SE-16: with no project open, Settings opens at the You level, this machine's (SE-23: the
    // focused level otherwise), so the row does not wait for a plane — and no row says
    // Preferences any more.
    for (const offers of [catalogue(now()), catalogue(now({ plane: "/p/one" }))]) {
      const hands = doing();
      const row = by(offers, "settings.show");
      expect(row?.title).toBe("Settings…");
      expect(row?.available).toBe(true);
      await run(offers, "settings.show", hands);
      expect(hands.calls).toEqual(["openSettingsTab"]);
      expect(offers.filter((one) => /preferences/i.test(one.title))).toEqual([]);
    }
  });

  it("offers Your settings… everywhere, so You is reachable with a project open (SE-23)", async () => {
    // Settings… opens at the focused level, so the machine's own level needs a row of its own
    // while a project is in front (V89c: Settings is a palette action per level).
    for (const offers of [catalogue(now()), catalogue(now({ plane: "/p/one" }))]) {
      const hands = doing();
      const row = by(offers, "settings.you");
      expect(row?.title).toBe("Your settings…");
      expect(row?.available).toBe(true);
      await run(offers, "settings.you", hands);
      expect(hands.calls).toEqual(["openYourSettings"]);
    }
  });

  it("offers one row per Settings group, You's with no project open too (#1201)", async () => {
    // You's groups are the machine's, so they are there with or without a project, as Your
    // settings… is; with none, the group is shown at You's place and Your settings… opens it.
    forgetGroups();
    const alone = catalogue(now());
    expect(
      alone.filter((one) => one.id.startsWith("settings.group:")).map((one) => one.title),
    ).toEqual([
      "Your settings: Text",
      "Your settings: Editor",
      "Your settings: Chats list",
      "Your settings: This machine",
    ]);
    const hands = doing();
    await run(alone, "settings.group:you.editor", hands);
    expect(hands.calls).toEqual(["openYourSettings"]);
    expect(shownAt(settingsPlace("you"))).toBe("you.editor");
  });

  it("offers the project's groups and the focused workspace's, each a link into Settings (#1201)", async () => {
    const offers = catalogue(now({ plane: "/p/one", focused: "web", workspaces: ["web", "api"] }));
    const rows = offers.filter((one) => one.id.startsWith("settings.group:"));
    expect(rows.map((one) => one.title)).toEqual(
      expect.arrayContaining([
        "Your settings: Text",
        "Project settings: Saving",
        "Project settings: Network",
        "Workspace settings: Repos",
      ]),
    );
    // Only the focused workspace's: the other one is a focus away.
    expect(rows.filter((one) => one.title.startsWith("Workspace settings:"))).toHaveLength(6);
    expect(by(offers, "settings.group:workspace.repos:web")?.note).toBe(
      "web: Settings at its level, at Repos.",
    );
    expect(rows.every((one) => one.available)).toBe(true);

    const asked: SettingsLinkAsk[] = [];
    const hear = (event: Event) => asked.push((event as CustomEvent<SettingsLinkAsk>).detail);
    window.addEventListener(SETTINGS_LINK, hear);
    try {
      const hands = doing();
      await run(offers, "settings.group:project.saving", hands);
      await run(offers, "settings.group:workspace.repos:web", hands);
      await run(offers, "settings.group:you.text", hands);
      // Every one through the window's link, which brings the tab forward and the keyboard in.
      expect(hands.calls).toEqual([]);
      expect(asked).toEqual([
        { plane: "/p/one", link: { group: "project.saving" } },
        { plane: "/p/one", link: { group: "workspace.repos", workspace: "web" } },
        { plane: "/p/one", link: { group: "you.text" } },
      ]);
    } finally {
      window.removeEventListener(SETTINGS_LINK, hear);
    }
  });

  it("says nothing needs you rather than leaving the queue's row out", () => {
    const empty = by(catalogue(now()), "needs.next");
    expect(empty?.available).toBe(false);
    expect(empty?.reason).toBe("Nothing needs you.");
  });

  it("shows the oldest chat in the queue, and one row for each of them", async () => {
    const tabs = openTab(openTab(noTabs(), 7, "one"), 8, "two");
    const hands = doing();
    const offers = catalogue(
      now({ tabs, needsYou: [8, 7], nameOf: (s) => (s === 7 ? "one" : "two") }),
    );

    expect(by(offers, "needs.show:7")?.title).toBe("Show one, which needs you");
    await run(offers, "needs.next", hands);

    expect(hands.calls).toEqual(["showChat:8"]);
  });

  it("says who reported back to a chat in the queue (charter-app#259)", () => {
    const tabs = openTab(noTabs(), 7, "one");
    const offers = catalogue(
      now({
        tabs,
        needsYou: [7],
        nameOf: () => "steward 3",
        reportsTo: () => ["drop commons"],
      }),
    );

    expect(by(offers, "needs.show:7")?.title).toBe("Show steward 3: drop commons reported back");
  });

  it("says what a chat in the queue had its commit refused for (SQ-16)", () => {
    const tabs = openTab(noTabs(), 7, "one");
    const offers = catalogue(
      now({
        tabs,
        needsYou: [7],
        nameOf: () => "steward 3",
        refusedIn: () => ["commit refused in app: a.py:2  an email address  ad**"],
      }),
    );

    expect(by(offers, "needs.show:7")?.title).toBe(
      "Show steward 3: commit refused in app: a.py:2  an email address  ad**",
    );
  });

  it("ignores a chat in the queue until it asks again, one row for each (charter-app#248)", async () => {
    const hands = doing();
    const offers = catalogue(now({ needsYou: [8, 7], nameOf: (s) => (s === 7 ? "one" : "two") }));

    const row = by(offers, "needs.ignore:7");
    expect(row?.title).toBe("Ignore one until it asks again");
    expect(row?.name).toBe("one");
    // No tab is needed: ignoring a chat is about its request, not about showing it.
    expect(row?.available).toBe(true);
    expect(by(offers, "needs.ignore:9")).toBeUndefined();
    await run(offers, "needs.ignore:7", hands);

    expect(hands.calls).toEqual(["ignoreNeedsYou:7"]);
  });

  it("refuses the worktree rows with charter's reason when no chat is in front", () => {
    const offers = catalogue(now({ plane: "/plane" }));

    expect(by(offers, "worktree.remove")?.reason).toBe("No chat is in front.");
    expect(by(offers, "worktree.merge")?.reason).toBe("No chat is in front.");
  });

  it("refuses them when the chat in front works nowhere charter cut", () => {
    const tabs = openTab(noTabs(), 7, "one");
    const offers = catalogue(now({ tabs, plane: "/plane" }));

    expect(by(offers, "worktree.remove")?.reason).toBe(
      "The chat in front is not working in a branch purlis cut.",
    );
  });

  it("never sends force on the operator's behalf", async () => {
    const tabs = openTab(noTabs(), 7, "one");
    const hands = doing();
    const offers = catalogue(now({ tabs, plane: "/plane", worktree: PIECE }));

    await run(offers, "worktree.remove", hands);

    // And it names the piece the chat in front is in, rather than leaving the window to work
    // out what "this chat's worktree" meant after the row was pressed (charter-app#174).
    expect(hands.calls).toEqual(["removeWorktree:svc/fix-it,false"]);
  });

  it("offers no discard row until a refusal has been read", () => {
    const tabs = openTab(noTabs(), 7, "one");
    const offers = catalogue(now({ tabs, plane: "/plane", worktree: PIECE }));

    // Absent rather than refused: a row permanently offering to throw work away is a
    // destructive action nobody was warned about, and there is nothing to warn about yet.
    expect(by(offers, "worktree.discard")).toBeUndefined();
  });

  it("offers it once the core has refused, and it is the row that forces", async () => {
    const tabs = openTab(noTabs(), 7, "one");
    const hands = doing();
    const offers = catalogue(
      now({ tabs, plane: "/plane", worktree: PIECE, refused: "worktree.remove" }),
    );

    await run(offers, "worktree.discard", hands);

    expect(hands.calls).toEqual(["removeWorktree:svc/fix-it,true"]);
  });

  describe("a piece's Merge, Remove and Done while one is on its way (#1610)", () => {
    const cut = { workspace: "alpha", repo: "svc", piece: "fix-it", branch: "fix/login" };
    const offers = catalogue(now({ plane: "/plane", pieces: [cut] }));

    /** Hands whose three piece verbs land only when `land` is called. */
    function held() {
      const hands = doing();
      let land: (answer: Ran) => void = () => {};
      const landing = new Promise<Ran>((resolve) => (land = resolve));
      const slow = (what: string) =>
        vi.fn(async (one: Cut) => {
          hands.calls.push(`${what}:${one.repo}/${one.piece}`);
          return landing;
        });
      hands.mergeWorktree = slow("mergeWorktree");
      hands.removeWorktree = slow("removeWorktree");
      hands.declareWorktreeDone = slow("declareWorktreeDone");
      return { hands, land: (answer: Ran) => land(answer) };
    }

    it.each([
      ["worktree.merge:svc/fix-it", "mergeWorktree:svc/fix-it"],
      ["worktree.remove:svc/fix-it", "removeWorktree:svc/fix-it"],
      ["worktree.done:svc/fix-it", "declareWorktreeDone:svc/fix-it"],
    ])("sends one command for two presses of %s", async (id, sent) => {
      const { hands, land } = held();

      const first = run(offers, id, hands);
      const second = await run(offers, id, hands);
      // Said nothing: the first press's answer is the one to read.
      expect(second).toEqual({ ok: true });
      land({ ok: true, said: "landed" });

      expect(await first).toEqual({ ok: true, said: "landed" });
      expect(hands.calls).toEqual([sent]);
    });

    it("refuses Remove while a Merge of the same piece is on its way, and says why", async () => {
      const { hands, land } = held();

      const merging = run(offers, "worktree.merge:svc/fix-it", hands);
      const removing = await run(offers, "worktree.remove:svc/fix-it", hands);
      land({ ok: true });
      await merging;

      expect(removing).toEqual({
        ok: false,
        refused: "fix-it is still merging. Try again once that has landed.",
      });
      expect(hands.calls).toEqual(["mergeWorktree:svc/fix-it"]);
    });

    it("lets a refused press be pressed again", async () => {
      const { hands, land } = held();

      const first = run(offers, "worktree.merge:svc/fix-it", hands);
      land({ ok: false, refused: "Not a fast-forward." });
      await first;
      const again = held();
      const second = run(offers, "worktree.merge:svc/fix-it", again.hands);
      again.land({ ok: true });
      await second;

      expect(again.hands.calls).toEqual(["mergeWorktree:svc/fix-it"]);
    });

    it("lets go of the piece when the command throws", async () => {
      const hands = doing();
      hands.mergeWorktree = vi.fn(async () => {
        throw new Error("the bridge went away");
      });

      await expect(run(offers, "worktree.merge:svc/fix-it", hands)).rejects.toThrow("bridge");
      await run(offers, "worktree.merge:svc/fix-it", doing());

      expect(hands.mergeWorktree).toHaveBeenCalledTimes(1);
    });

    it("holds each piece on its own", async () => {
      const other = { ...cut, piece: "other", branch: "other" };
      const both = catalogue(now({ plane: "/plane", pieces: [cut, other] }));
      const { hands, land } = held();

      const one = run(both, "worktree.merge:svc/fix-it", hands);
      const two = run(both, "worktree.merge:svc/other", hands);
      land({ ok: true });
      await Promise.all([one, two]);

      expect(hands.calls).toEqual(["mergeWorktree:svc/fix-it", "mergeWorktree:svc/other"]);
    });
  });

  it("offers each piece of the focused workspace's files, in the light editor (RC-5)", () => {
    const cut = { workspace: "alpha", repo: "svc", piece: "fix-it" };
    const offers = catalogue(now({ plane: "/plane", pieces: [cut] }));

    const files = by(offers, "worktree.files:svc/fix-it");
    expect(files?.title).toBe("Browse the files of fix-it");
    expect(files?.does).toEqual({
      verb: "openView",
      view: { from: null, view: "piece-files", key: "alpha/svc/fix-it" },
      title: "Files · fix-it",
    });

    const planeless = catalogue(now({ plane: undefined, pieces: [cut] }));
    expect(by(planeless, "worktree.files:svc/fix-it")?.available).toBe(false);
  });

  it("offers to mark each piece of the focused workspace done, above the line", () => {
    // charter#368: a declaration is one line in the piece log. It names its piece, as the
    // merge beside it does, and it is not destructive, so nothing asks first.
    const cut = { workspace: "alpha", repo: "svc", piece: "fix-it", branch: "fix/login" };
    const offers = catalogue(now({ plane: "/plane", pieces: [cut] }));

    const done = by(offers, "worktree.done:svc/fix-it");
    // #989: of the branch, by the branch's own name, which need not be the folder's.
    expect(done?.title).toBe("Mark branch fix/login done");
    expect(done?.does).toEqual({ verb: "declareWorktreeDone", cut });
    expect(done?.note).toBeUndefined();

    const planeless = catalogue(now({ plane: undefined, pieces: [cut] }));
    expect(by(planeless, "worktree.done:svc/fix-it")?.available).toBe(false);
  });

  it("offers to focus the explorer on each branch, first in its menu (FM-5)", async () => {
    const cut = { workspace: "alpha", repo: "svc", piece: "fix-it", branch: "fix/login" };
    const offers = catalogue(now({ pieces: [cut] }));
    const hands = doing();

    const focus = by(offers, "worktree.focus:svc/fix-it");
    expect(focus?.title).toBe("Focus on branch fix/login");
    // It changes nothing on disk, so it needs no project to reach a branch.
    expect(focus?.available).toBe(true);
    expect(menuOn({ on: "worktree", repo: "svc", piece: "fix-it" }).above[0]).toBe(
      "worktree.focus:svc/fix-it",
    );
    await run(offers, "worktree.focus:svc/fix-it", hands);
    expect(hands.calls).toEqual(["focusBranch:svc/fix-it"]);
    expect(
      by(catalogue(now({ pieces: [{ ...cut, branch: null }] })), "worktree.focus:svc/fix-it")
        ?.title,
    ).toBe("Focus on folder fix-it");
  });

  it("offers each branch's own folder its absolute path, a reveal and a shell tab (#1143)", async () => {
    const cut = { workspace: "alpha", repo: "svc", piece: "fix-it", branch: "fix/login" };
    const offers = catalogue(now({ plane: "/plane", pieces: [cut] }));
    const hands = doing();
    const own = { workspace: "alpha", repo: "svc", piece: "fix-it", path: "" };

    expect(menuOn({ on: "worktree", repo: "svc", piece: "fix-it" }).above).toEqual([
      "worktree.focus:svc/fix-it",
      "worktree.files:svc/fix-it",
      "worktree.copypath:svc/fix-it",
      "worktree.reveal:svc/fix-it",
      "worktree.shell:svc/fix-it",
      "worktree.merge:svc/fix-it",
    ]);
    expect(by(offers, "worktree.copypath:svc/fix-it")?.title).toBe(
      "Copy the absolute path of branch fix/login",
    );
    expect(by(offers, "worktree.reveal:svc/fix-it")?.title).toMatch(
      /^Reveal branch fix\/login in (Finder|File Explorer|Files)$/,
    );
    expect(by(offers, "worktree.shell:svc/fix-it")?.title).toBe(
      "Open a shell tab in branch fix/login",
    );
    expect(by(offers, "worktree.shell:svc/fix-it")?.name).toBe("fix/login");
    expect(by(offers, "worktree.copypath:svc/fix-it")?.does).toEqual({
      verb: "copyPath",
      at: own,
      absolute: true,
    });
    // No Copy relative path: the folder's path relative to itself says nothing (D-1143-1).
    expect(ids(offers).filter((id) => id.endsWith(":svc/fix-it") && /path/.test(id))).toEqual([
      "worktree.copypath:svc/fix-it",
    ]);

    // The sentence names the folder, not an empty path.
    expect(await run(offers, "worktree.copypath:svc/fix-it", hands)).toMatchObject({
      ok: true,
      said: "Copied the absolute path of fix-it.",
    });
    await run(offers, "worktree.reveal:svc/fix-it", hands);
    await run(offers, "worktree.shell:svc/fix-it", hands);
    expect(hands.calls).toEqual([
      "copyPath:svc/fix-it:,true",
      "revealPath:svc/fix-it:",
      "shellInFolder:svc/fix-it:",
    ]);

    // A folder git has on no branch is named as a folder; with no plane, nothing is placed.
    const folder = catalogue(now({ plane: "/plane", pieces: [{ ...cut, branch: null }] }));
    expect(by(folder, "worktree.shell:svc/fix-it")?.title).toBe(
      "Open a shell tab in folder fix-it",
    );
    const planeless = catalogue(now({ plane: undefined, pieces: [cut] }));
    for (const id of ["copypath", "reveal", "shell"])
      expect(by(planeless, `worktree.${id}:svc/fix-it`)?.available).toBe(false);
  });

  it("offers the branch's own Files row the same three, at the empty path (#1143)", () => {
    const at = { workspace: "alpha", repo: "svc", piece: "fix-it", path: "" };
    const rows = fileRows({ on: "file", at, kind: "folder" });

    expect(rows.map((row) => [row.title, row.does])).toEqual([
      ["Copy absolute path", { verb: "copyPath", at, absolute: true }],
      [revealSaid(navigator.platform), { verb: "revealPath", at }],
      ["Open a shell tab here", { verb: "shellInFolder", at }],
    ]);
  });

  it("names a folder git has on no branch as a folder, never as a branch (#989)", () => {
    const cut = { workspace: "alpha", repo: "svc", piece: "fix-it" };
    const offers = catalogue(now({ plane: "/plane", pieces: [cut] }));

    expect(by(offers, "worktree.merge:svc/fix-it")?.title).toBe("Merge folder fix-it into svc");
    expect(by(offers, "worktree.done:svc/fix-it")?.title).toBe("Mark folder fix-it done");
    expect(by(offers, "worktree.remove:svc/fix-it")?.title).toBe("Remove folder fix-it in svc");
    for (const row of offers.filter((offer) => offer.id.startsWith("worktree."))) {
      expect(row.title).not.toMatch(/worktree/i);
    }
  });

  it("names one merge and one remove per piece of the focused workspace", () => {
    // charter-app#174: the explorer's rows had no menu because the only worktree rows there
    // were were about THE CHAT IN FRONT, and a piece nobody is running in is not in front of
    // anything. These name their piece, which is what makes a row about one possible at all.
    const cut = { workspace: "alpha", repo: "svc", piece: "fix-it", branch: "fix/login" };
    const offers = catalogue(now({ plane: "/plane", pieces: [cut] }));

    // #989: the merge names the branch by its own name; the removal takes the folder and
    // leaves the branch, so it names the folder.
    expect(by(offers, "worktree.merge:svc/fix-it")?.title).toBe("Merge branch fix/login into svc");
    expect(by(offers, "worktree.remove:svc/fix-it")?.title).toBe("Remove folder fix-it in svc");
    // The clone is in the id and in the title, because two clones of one workspace can each
    // hold a piece called `fix-it` and a destructive row may not be ambiguous about which.
    expect(by(offers, "worktree.remove:svc/fix-it")?.name).toBe("fix-it");
    expect(by(offers, "worktree.remove:svc/fix-it")?.does).toEqual({
      verb: "removeWorktree",
      cut,
      force: false,
    });
  });

  it("says on the piece's remove row that the branch is not what goes", () => {
    // `git worktree remove` takes the directory and leaves the ref, so "remove" here is not
    // the word it is on a workspace — and this row pops up under the pointer.
    const offers = catalogue(
      now({ plane: "/plane", pieces: [{ workspace: "alpha", repo: "svc", piece: "fix-it" }] }),
    );

    expect(by(offers, "worktree.remove:svc/fix-it")?.note).toBe(KEEPS_THE_BRANCH);
  });

  it("refuses a piece's rows with charter's reason when there is no plane to reach it in", () => {
    const offers = catalogue(
      now({ pieces: [{ workspace: "alpha", repo: "svc", piece: "fix-it" }] }),
    );

    expect(by(offers, "worktree.remove:svc/fix-it")?.reason).toBe(
      "purlis found no project, so it cannot reach a branch.",
    );
  });

  it("puts the discard row beside the removal that was refused, and beside no other", () => {
    // With one removal per piece, a discard row that appeared for all of them would be fifty
    // offers to throw work away raised by one refusal about one piece.
    const pieces = [
      { workspace: "alpha", repo: "svc", piece: "fix-it" },
      { workspace: "alpha", repo: "svc", piece: "other" },
    ];
    const offers = catalogue(
      now({ plane: "/plane", pieces, refused: "worktree.remove:svc/fix-it" }),
    );

    expect(by(offers, "worktree.discard:svc/fix-it")?.title).toBe(
      "Discard that work and remove fix-it anyway",
    );
    expect(by(offers, "worktree.discard:svc/other")).toBeUndefined();
    // And not the front chat's either: that row answers a refusal `worktree.remove` gave.
    expect(by(offers, "worktree.discard")).toBeUndefined();
  });

  it("offers one row per persona, which is the only thing charter can do to one", () => {
    // charter-app#174. A persona is a file `charter persona create` writes and an operator
    // edits; reading what it says is the whole of what this window can do to one, so it is
    // the whole of what a menu on a persona row lists.
    const offers = catalogue(now({ personas: ["steward", "release"] }));

    expect(by(offers, "persona.show:steward")?.title).toBe("Show what steward is");
    expect(by(offers, "persona.show:steward")?.name).toBe("steward");
    expect(by(offers, "persona.show:release")?.does).toEqual({
      verb: "openView",
      view: { from: null, view: "persona", key: "release" },
      title: "release",
    });
  });

  it("offers the focused workspace's changes as a view tab, and none outside every workspace", () => {
    // charter#470: a view tab keyed by the workspace, opened from the palette.
    const offers = catalogue(now({ plane: "/plane", workspaces: ["alpha"], focused: "alpha" }));

    expect(by(offers, "workspace.changes:alpha")).toMatchObject({
      title: "Open changes",
      available: true,
      does: {
        verb: "openView",
        view: { from: null, view: "changes", key: "alpha" },
        title: "Changes · alpha",
      },
    });
    // FG-3: a member may be on GitLab, whose request is a merge request, so the note names
    // neither forge's word.
    expect(by(offers, "workspace.changes:alpha")?.note).toBe(
      "alpha: each cross-repo change, each member's request and its checks.",
    );
    const outside = catalogue(now({ plane: "/plane", workspaces: [OUTSIDE], focused: OUTSIDE }));
    expect(outside.some((offer) => offer.id.startsWith("workspace.changes:"))).toBe(false);
  });

  it("offers a row per vault that opens its tab, a picker, and a way to make one", () => {
    // charter-app#235. The panel's rows run `vault.open:<name>`, and so does the picker.
    const offers = catalogue(now({ plane: "/plane", vaults: ["ops", "team"] }));

    expect(by(offers, "vault.open:ops")?.title).toBe("Open vault ops");
    expect(by(offers, "vault.open:ops")?.name).toBe("ops");
    expect(by(offers, "vault.open:team")?.does).toEqual({
      verb: "openView",
      view: { from: null, view: "vault", key: "team" },
      title: "team",
    });
    expect(by(offers, "vault.pick")).toMatchObject({
      title: "Open vault…",
      available: true,
      does: { verb: "pickVault" },
    });
    expect(by(offers, "vault.create")).toMatchObject({
      title: "New vault…",
      available: true,
      does: { verb: "createVault" },
    });
  });

  it("offers Resume for every open session record's tab, whichever place is in front", async () => {
    // SI-8e: the tab's Resume button is this row, and it used to exist only for the records of
    // the place in front — so a record's tab drew no Resume once another place was focused.
    const path = "workspaces/beta/sessions/20260928-140000-ship-it.md";
    const tabs = openView(
      noTabs(),
      { from: null, view: "session", key: path },
      "Session · Ship it",
      "beta",
    );
    const offers = catalogue(
      now({
        plane: "/plane",
        tabs,
        focused: "alpha",
        sessions: [
          {
            path: "workspaces/alpha/sessions/20260927-090000-plan.md",
            title: "Plan",
            resumable: true,
          },
        ],
      }),
    );

    const row = by(offers, `session.resume:${path}`);
    expect(row).toMatchObject({ title: "Resume session: Ship it", available: true });
    const hands = doing();
    await run(offers, `session.resume:${path}`, hands);
    expect(hands.calls).toEqual([`resumeSession:${path}`]);
    // One row per record, even when its tab is open AND it is the place in front's.
    const both = catalogue(
      now({ plane: "/plane", tabs, sessions: [{ path, title: "Ship it", resumable: true }] }),
    );
    expect(ids(both).filter((id) => id === `session.resume:${path}`)).toHaveLength(1);
  });

  it("says why a vault cannot be opened on a plane that has none, and still offers to make one", () => {
    const offers = catalogue(now({ plane: "/plane", vaults: [] }));
    expect(by(offers, "vault.pick")?.available).toBe(false);
    expect(by(offers, "vault.pick")?.reason).toMatch(/no vaults/);
    expect(by(offers, "vault.create")?.available).toBe(true);

    const nowhere = catalogue(now());
    expect(by(nowhere, "vault.create")?.available).toBe(false);
  });

  describe("a clone of the focused workspace (charter-app#174)", () => {
    const SVC = { repo: "svc", path: "/plane/workspaces/alpha/svc" };

    it("offers to start the next chats in it, carrying the path the core spelled", () => {
      // The verb the issue named: a clone picked as the spot, one level up from a piece.
      const offers = catalogue(now({ clones: [SVC] }));

      expect(by(offers, "clone.pick:svc")?.title).toBe("Start new chats in svc");
      expect(by(offers, "clone.pick:svc")?.name).toBe("svc");
      expect(by(offers, "clone.pick:svc")?.does).toEqual({
        verb: "pickClone",
        repo: "svc",
        path: "/plane/workspaces/alpha/svc",
      });
    });

    it("offers a new tab in it, for that one tab, leaving the pick alone", async () => {
      const hands = doing();
      const offers = catalogue(now({ clones: [SVC] }));

      expect(by(offers, "clone.chat:svc")?.title).toBe("New tab in svc");
      await run(offers, "clone.chat:svc", hands);

      // Not `pickClone`: that would make every later New tab start in the clone too, and then
      // the two rows would be one row with two names (the #174 review).
      expect(hands.calls).toEqual(["newChatIn:/plane/workspaces/alpha/svc"]);
    });

    it("greys the pick with a reason once new chats already start there", () => {
      const offers = catalogue(now({ clones: [SVC], startsIn: SVC.path }));

      expect(by(offers, "clone.pick:svc")?.available).toBe(false);
      expect(by(offers, "clone.pick:svc")?.reason).toBe("New chats already start in svc.");
      // Starting another one there is still something to do.
      expect(by(offers, "clone.chat:svc")?.available).toBe(true);
    });

    it("has no row for a clone it was not told the path of", () => {
      expect(by(catalogue(now()), "clone.pick:svc")).toBeUndefined();
    });

    it("offers its own folder's cockpit, which narrows the explorer and changes nothing (#1152)", async () => {
      const hands = doing();
      const offers = catalogue(now({ clones: [SVC] }));

      expect(by(offers, "clone.focus:svc")?.title).toBe("Focus on repo svc");
      await run(offers, "clone.focus:svc", hands);

      expect(hands.calls).toEqual(["focusRepo:svc"]);
    });

    it("offers a new branch in it, which cuts one and starts nothing (GL-1)", async () => {
      // ADR 0072 §4: the action that cuts a piece is "New branch", and it names the repo it
      // will be cut in. The dialog it opens names the branch.
      const hands = doing();
      const offers = catalogue(now({ clones: [SVC] }));

      expect(by(offers, "clone.branch:svc")?.title).toBe("New branch in svc…");
      expect(by(offers, "clone.branch:svc")?.available).toBe(true);
      await run(offers, "clone.branch:svc", hands);

      expect(hands.calls).toEqual(["newBranch:svc"]);
    });

    it("lists its focus, the new tab, the new branch, the pick, then Read again, and nothing below the line", () => {
      // Its own folder's cockpit first, as a branch's is (#1152). Read again is found only while
      // a read stands refused (#1244).
      expect(menuOn({ on: "clone", repo: "svc" })).toEqual({
        above: [
          "clone.focus:svc",
          "clone.chat:svc",
          "clone.branch:svc",
          "clone.pick:svc",
          READ_AGAIN,
        ],
        below: [],
      });
    });
  });

  describe("a finished task's row (#1534)", () => {
    it("lists Merge… above the line and Discard branch… below it", () => {
      expect(menuOn({ on: "finished", id: "01K6" })).toEqual({
        above: ["task.merge:01K6"],
        below: ["task.discard:01K6"],
      });
    });
  });

  describe("a refused read of the focused workspace (#1244)", () => {
    it("offers Read again only while one stands, and it reads the workspace again", async () => {
      expect(by(catalogue(now({ focused: "alpha" })), READ_AGAIN)).toBeUndefined();
      expect(
        by(catalogue(now({ focused: OUTSIDE, readRefused: true })), READ_AGAIN),
      ).toBeUndefined();

      const offers = catalogue(now({ focused: "alpha", readRefused: true }));
      expect(by(offers, READ_AGAIN)?.title).toBe("Read the workspace again");
      const hands = doing();
      await run(offers, READ_AGAIN, hands);
      expect(hands.calls).toEqual(["readAgain"]);
    });
  });

  describe("a repo the focused workspace names and has not cloned here (#1215)", () => {
    it("offers to clone each one, and all of them, into that workspace", async () => {
      const hands = doing();
      const offers = catalogue(now({ focused: "alpha", absent: ["charter", "web"] }));

      expect(by(offers, "absent.clone:charter")?.title).toBe("Clone charter");
      expect(by(offers, "absent.cloneAll")?.title).toBe("Clone all missing repos");
      await run(offers, "absent.clone:charter", hands);
      await run(offers, "absent.cloneAll", hands);

      expect(hands.calls).toEqual(["cloneMissing:alpha:charter", "cloneMissing:alpha:charter,web"]);
    });

    it("greys a repo already being cloned, and leaves it out of Clone all", () => {
      const offers = catalogue(
        now({ focused: "alpha", absent: ["charter", "web"], cloning: ["charter"] }),
      );

      expect(by(offers, "absent.clone:charter")?.available).toBe(false);
      expect(by(offers, "absent.clone:charter")?.reason).toBe("charter is being cloned.");
      expect(by(offers, "absent.cloneAll")?.does).toEqual({
        verb: "cloneMissing",
        workspace: "alpha",
        repos: ["web"],
      });
    });

    it("greys Clone all with its reason while every missing repo is already being cloned", () => {
      const offers = catalogue(now({ focused: "alpha", absent: ["web"], cloning: ["web"] }));

      expect(by(offers, "absent.cloneAll")?.reason).toBe("Every missing repo is being cloned.");
    });

    it("has no Clone all when nothing is missing, nor at the project root", () => {
      expect(by(catalogue(now({ focused: "alpha" })), "absent.cloneAll")).toBeUndefined();
      expect(
        by(catalogue(now({ focused: OUTSIDE, absent: ["web"] })), "absent.cloneAll"),
      ).toBeUndefined();
    });

    it("draws Clone on the row's menu, and taking it out of the workspace below the line", () => {
      expect(menuOn({ on: "absent", repo: "web" })).toEqual({
        above: ["absent.clone:web"],
        below: ["absent.drop:web"],
      });
    });

    it("asks to take each one out of the workspace, and never while it is being cloned (#1228)", async () => {
      const hands = doing();
      const offers = catalogue(
        now({ focused: "alpha", absent: ["charter", "web"], cloning: ["charter"] }),
      );

      expect(by(offers, "absent.drop:web")?.title).toBe("Remove web from workspace…");
      expect(by(offers, "absent.drop:charter")?.available).toBe(false);
      expect(by(offers, "absent.drop:charter")?.reason).toBe("charter is being cloned.");
      await run(offers, "absent.drop:web", hands);

      expect(hands.calls).toEqual(["askDropMembership:alpha:web"]);
    });
  });

  it("offers every view an approved extension offers, by the same verb a persona's tab is opened by", () => {
    const offers = catalogue(
      now({
        views: [
          {
            extension: "persona-statistics",
            id: "statistics",
            title: "Statistics",
            about: "personas",
          },
        ],
      }),
    );

    const row = by(offers, "view.open:persona-statistics/statistics");
    // Whose it is is in the words: what is in force is shown after approval (ADR 0041 item 5).
    expect(row?.title).toBe("Open Statistics from persona-statistics");
    expect(row?.does).toEqual({
      verb: "openView",
      view: { from: "persona-statistics", view: "statistics", key: "" },
      title: "Statistics",
    });
  });

  describe("an extension's palette commands (charter-app#341)", () => {
    const CLOSE: RowAction = { id: "close", title: "Close all", asks_first: true, deletes: true };
    const commanded = () =>
      catalogue(
        now({
          commands: [
            {
              extension: "todo",
              name: "Todos",
              id: "open",
              title: "Show todos",
              does: { kind: "open", view: "list", title: "Todo list" },
            },
            {
              extension: "todo",
              name: "Todos",
              id: "close",
              title: "Close every todo",
              does: { kind: "run", action: CLOSE },
            },
          ],
        }),
      );

    it("names each with the extension's name, so where it came from is on the row", () => {
      const offers = commanded();
      expect(by(offers, "ext.command:todo/open")?.title).toBe("Todos: Show todos");
      expect(by(offers, "ext.command:todo/close")?.title).toBe("Todos: Close every todo");
    });

    it("opens its view by the verb every view is opened by", () => {
      expect(by(commanded(), "ext.command:todo/open")?.does).toEqual({
        verb: "openView",
        view: { from: "todo", view: "list", key: "" },
        title: "Todo list",
      });
    });

    it("runs its action through the window, which asks first when the action does", async () => {
      const hands = doing();
      await run(commanded(), "ext.command:todo/close", hands);
      expect(hands.calls).toEqual(["runAction:todo,close,Todos"]);
    });
  });

  describe("on a tab that shows a view", () => {
    const STEWARD: ViewRef = { from: null, view: "persona", key: "steward" };
    const withView = () => openView(openTab(noTabs(), 7, "one"), STEWARD, "steward", "alpha");

    it("closes the tab and ends nothing, so it is neither worded nor asked about as an ending", () => {
      const tabs = withView();
      const row = by(catalogue(now({ tabs })), `tab.close:${tabs.inFront}`);

      expect(row?.title).toBe("Close steward");
      expect(row?.does).toEqual({ verb: "closeTab", tab: tabs.inFront, ends: false });
      expect(row?.note).toBeUndefined();
    });

    it("closes the view's pane with words that say so, and ends nothing", () => {
      const row = by(catalogue(now({ tabs: withView() })), "pane.close");

      expect(row?.title).toBe("Close this view");
      expect(row?.does).toEqual({ verb: "closePane", ends: false });
    });

    it("sends the palette's key nowhere, because a view pane has no chat to send it to", () => {
      const row = by(catalogue(now({ tabs: withView() })), "pane.sendkey");

      expect(row?.available).toBe(false);
      expect(row?.reason).toMatch(/shows a view, not a chat/);
    });

    it("still splits, so a chat can be started beside the view", () => {
      expect(by(catalogue(now({ tabs: withView() })), "pane.split.right")?.available).toBe(true);
    });

    it("pins the tab as the view it shows, since it has no chat to pin", () => {
      const tabs = withView();
      const id = tabs.inFront as number;

      expect(by(catalogue(now({ tabs })), `tab.pin:${id}`)?.title).toBe("Pin tab steward");
      expect(
        by(
          catalogue(
            now({
              tabs,
              pinned: { chats: [], views: [viewKey(STEWARD)], workspaces: [], projects: [] },
            }),
          ),
          `tab.pin:${id}`,
        )?.title,
      ).toBe("Unpin tab steward");
    });

    it("offers no rename, because a view's tab is named after what it shows", () => {
      const tabs = withView();

      expect(by(catalogue(now({ tabs })), `tab.rename:${tabs.inFront}`)).toBeUndefined();
    });

    it("still ends a chat's tab beside it, and asks about that", () => {
      const tabs = withView();
      const row = by(catalogue(now({ tabs })), `tab.close:${tabs.order[0]}`);

      expect(row?.does).toEqual({ verb: "closeTab", tab: tabs.order[0], ends: true });
    });
  });

  it("closes a pane that exists and refuses one that does not, by the same rule", () => {
    const tabs = splitFocusedPane(openTab(noTabs(), 7, "one"), "row", 8);

    expect(by(catalogue(now({ tabs })), "pane.close")?.available).toBe(true);
    expect(by(catalogue(now()), "pane.close")?.available).toBe(false);
  });
});

describe("narrowing by what is typed", () => {
  const rows: Offer[] = [
    { id: "chat.new", title: "New tab", available: true, reason: "", does: { verb: "nothing" } },
    {
      id: "pane.split.right",
      title: "Split right",
      available: true,
      reason: "",
      does: { verb: "nothing" },
    },
    {
      id: "tab.select:17",
      title: "Switch to tab docs",
      available: true,
      reason: "",
      does: { verb: "nothing" },
    },
    {
      id: "pane.close",
      title: "Close pane",
      available: false,
      reason: "No chat is in front, so there is no pane to close.",
      does: { verb: "nothing" },
    },
  ];

  it("matches without caring about case, in either direction", () => {
    // An operator typing `split` must find `Split right`, and one typing `SPLIT` too.
    expect(narrow("split", rows).map((row) => row.id)).toEqual(["pane.split.right"]);
    expect(narrow("SPLIT", rows).map((row) => row.id)).toEqual(["pane.split.right"]);
  });

  it("matches charter's own name for the action as well as its words", () => {
    expect(narrow("chat.new", rows).map((row) => row.id)).toEqual(["chat.new"]);
  });

  it("does not match the part of an id that is only a name", () => {
    // `tab.select:17` is one row about the tab called `docs`. Typing `17` must not find it:
    // the number is charter's own counter, never drawn and never typed.
    expect(narrow("17", rows)).toEqual([]);
    expect(narrow("docs", rows).map((row) => row.id)).toEqual(["tab.select:17"]);
  });

  it("never matches the reason a row cannot run", () => {
    // `front` is in `Close pane`'s reason and nowhere else. Matching it would make typing a
    // word out of charter's own explanation list rows that merely mention one.
    expect(narrow("front", rows)).toEqual([]);
  });

  it("keeps an unavailable row, with its reason, among the matches", () => {
    const kept = narrow("close", rows);

    expect(kept.map((row) => row.id)).toEqual(["pane.close"]);
    expect(kept[0].reason).toBe("No chat is in front, so there is no pane to close.");
  });

  it("puts the title typed in FULL first, and leaves everything else where it was", () => {
    // The longer row comes FIRST in the catalogue, so "pulled to the top" is something the
    // order can show. With it second, the rule would look kept whether or not it was.
    const plus: Offer[] = [
      {
        id: "x.y",
        title: "New tabs, plural",
        available: true,
        reason: "",
        does: { verb: "nothing" },
      },
      ...rows,
    ];

    expect(narrow("new tab", plus).map((row) => row.id)).toEqual(["chat.new", "x.y"]);
  });

  it("caps nothing and reorders nothing when nothing is typed", () => {
    expect(narrow("", rows)).toEqual(rows);
    expect(narrow("   ", rows)).toEqual(rows);
  });

  it("matches on the title or the verb, and says so one row at a time", () => {
    expect(matches("tab", rows[0])).toBe(true);
    expect(matches("pane", rows[1])).toBe(true);
    expect(matches("nothing", rows[0])).toBe(false);
  });
});

describe("what Enter is aimed at", () => {
  const refused = (id: string): Offer => ({
    id,
    title: id,
    available: false,
    reason: "not now",
    does: { verb: "nothing" },
  });
  const ready = (id: string): Offer => ({
    id,
    title: id,
    available: true,
    reason: "",
    does: { verb: "nothing" },
  });

  it("is the first row that CAN run, not simply the first row", () => {
    expect(aim([refused("a"), refused("b"), ready("c")])).toBe(2);
  });

  it("is nothing at all when no row can run", () => {
    expect(aim([refused("a")])).toBe(-1);
    expect(aim([])).toBe(-1);
  });
});

describe("carrying out a row", () => {
  it("does nothing at all for a row that cannot run, and answers its reason", async () => {
    // Checked twice on purpose: a surface that forgot to look at `available` must not be
    // the place a refused action runs after all.
    const hands = doing();
    const offers = catalogue(now());

    const answer = await run(offers, "pane.split.right", hands);

    expect(answer).toEqual({
      ok: false,
      refused: "No chat is in front, so there is no pane to split.",
    });
    expect(hands.calls).toEqual([]);
  });

  it("hands the core's refusal back whole, without wording it again", async () => {
    const hands = doing();
    hands.removeWorktree = async () => ({
      ok: false,
      refused: "svc/fix-it has uncommitted changes; commit or stash them first",
    });
    const offers = catalogue(
      now({ tabs: openTab(noTabs(), 7, "one"), plane: "/plane", worktree: PIECE }),
    );

    expect(await run(offers, "worktree.remove", hands)).toEqual({
      ok: false,
      refused: "svc/fix-it has uncommitted changes; commit or stash them first",
    });
  });

  it("reaches every verb it can be given", async () => {
    // Nothing in the catalogue is a verb `perform` has no arm for, and nothing in `perform`
    // is an arm no row reaches: a row that fell through would silently do nothing.
    const tabs = selectTab(openTab(openTab(noTabs(), 7, "one"), 8, "two"), 1);
    const hands = doing();
    const offers = catalogue(
      now({
        tabs,
        workspaces: ["alpha", "beta"],
        focused: "alpha",
        plane: "/plane",
        projects: [
          { plane: "/plane", name: "plane" },
          { plane: "/other", name: "other" },
        ],
        worktree: PIECE,
        refused: "worktree.remove",
        // The focused workspace's own pieces and the plane's personas, so the rows
        // charter-app#174 added are reached here too.
        pieces: [PIECE],
        personas: ["steward"],
        vaults: ["ops"],
        todos: [{ slug: "20260302-091400-review", title: "Review the plan" }],
        needsYou: [8],
        nameOf: (s) => String(s),
        // A split window, so the row that moves a project back is reached too (charter#126).
        split: true,
      }),
    );

    for (const offer of offers) await perform(offer, hands);

    expect(new Set(hands.calls)).toEqual(
      new Set([
        "newChat",
        "newShell",
        // Search in files (#1137): the Search view since #1676, as the views' rows are.
        "showSideView:search",
        "newShell:alpha",
        "newShell:beta",
        "split:row",
        "split:column",
        "showChat:8",
        "ignoreNeedsYou:8",
        "selectTab:2",
        "focusWorkspace:beta",
        "closePane",
        "closeTab:1",
        "closeTab:2",
        "renameTab:1",
        "renameTab:2",
        // The same three calls whether the row was the chat in front's or the explorer's:
        // both name the piece, so both arrive here identically (charter-app#174).
        "removeWorktree:svc/fix-it,false",
        "removeWorktree:svc/fix-it,true",
        "mergeWorktree:svc/fix-it",
        "declareWorktreeDone:svc/fix-it",
        "focusBranch:svc/fix-it",
        // The branch's own folder, placed by the core at the empty path (#1143).
        "copyPath:svc/fix-it:,true",
        "revealPath:svc/fix-it:",
        "shellInFolder:svc/fix-it:",
        "openView:charter/persona/steward,steward",
        "openView:charter/piece-files/alpha/svc/fix-it,Files · fix-it",
        "openView:charter/vault/ops,ops",
        "openView:charter/changes/alpha,Changes · alpha",
        "openView:charter/memory-archive/persona/steward,Archived memory · steward",
        "openView:charter/memory-archive/shared,Archived shared memory",
        "openView:charter/memory-archive/workspace/alpha,Archived memory · alpha",
        // SI-9c: a new memory in each store the window lists, and the shared list.
        `newMemory:${JSON.stringify({ kind: "workspace", name: "alpha" })}`,
        `newMemory:${JSON.stringify({ kind: "persona", name: "steward" })}`,
        `newMemory:${JSON.stringify({ kind: "shared" })}`,
        "openView:charter/shared-memory/,Shared memory",
        // #1452: the project's dispatches, in a tab of their own.
        "openView:charter/dispatches/,Dispatches",
        // A chat's Activity, one row per chat tab (#1495).
        "openView:charter/activity/7,Activity · one",
        "openView:charter/activity/8,Activity · two",
        "openView:charter/chat-network/7,Network · one",
        "openView:charter/chat-network/8,Network · two",
        "openView:charter/todo/alpha/20260302-091400-review,Review the plan",
        "pickVault",
        "createVault",
        // SI-3: a vault, a persona and a todo are made and deleted from the window too.
        "removeVault:ops",
        "createPersona",
        "editPersona:steward",
        "setPersonaProfile:steward",
        "removePersona:steward",
        "closeTodo:alpha,20260302-091400-review",
        "forgetTodo:alpha,20260302-091400-review",
        "sendKey:F2",
        "openProject",
        "createProject",
        "showExtensions",
        "showSideView:chats",
        "showSideView:explorer",
        "showSideView:changes",
        "toggleRegion:navigation",
        "openSettingsTab",
        "openYourSettings",
        "installCli",
        "createWorkspace",
        "removeWorkspace:alpha",
        "removeWorkspace:beta",
        // Three pin verbs and not one, because they are three stores (ADR 0040).
        "pinTab:1,true",
        "pinTab:2,true",
        "pinWorkspace:alpha,true",
        "pinWorkspace:beta,true",
        "pinProject:/plane,true",
        "pinProject:/other,true",
        "selectProject:/other",
        "closeProject:/plane",
        "closeProject:/other",
        "moveProject:/plane,new",
        "moveProject:/other,new",
        "moveProject:/plane,main",
        "moveProject:/other,main",
        "openSettings:/plane",
        "openSettings:/other",
        "openSaving:/plane",
        "openSaving:/other",
        "openWorkspaceSettings:alpha",
        "openWorkspaceSettings:beta",
        "switchLive:alpha",
        "switchLive:beta",
        "switchProject",
        "renameWorkspace:alpha",
        "renameWorkspace:beta",
        "quit",
      ]),
    );
  });
});

describe("a plain shell tab (SI-5)", () => {
  it("offers a new shell right after a new tab, on an empty window too", () => {
    const offers = ids(catalogue(now()));

    expect(offers.indexOf("shell.new")).toBe(offers.indexOf("chat.new") + 1);
  });

  it("runs a new shell where a new chat would start, with nothing named", async () => {
    const hands = doing();

    await run(catalogue(now({ workspaces: ["alpha"], focused: "alpha" })), "shell.new", hands);

    expect(hands.calls).toEqual(["newShell"]);
  });

  it("offers a new shell in each workspace, and runs it in that one", async () => {
    const hands = doing();
    const offers = catalogue(now({ workspaces: ["alpha", "beta"], focused: "alpha" }));

    await run(offers, "shell.new:beta", hands);

    expect(by(offers, "shell.new:beta")?.title).toBe("New shell in beta");
    expect(hands.calls).toEqual(["newShell:beta"]);
  });

  it("offers the plane root's shell only once there is a plane root to start it in (SI-1)", () => {
    // The strip of chats outside every workspace became the plane root, whose directory is
    // the plane's own — so it has a shell row, which cannot run before the plane is read.
    const unread = catalogue(now({ workspaces: [OUTSIDE, "alpha"] }));
    expect(by(unread, `shell.new:${OUTSIDE}`)?.available).toBe(false);
    const read = catalogue(now({ workspaces: [OUTSIDE, "alpha"], plane: "/plane" }));
    expect(by(read, `shell.new:${OUTSIDE}`)?.available).toBe(true);
  });

  it("puts a new shell on a workspace's menu and on the panes' menu, beside a new tab", () => {
    expect(menuOn({ on: "workspace", workspace: "alpha" }).above).toContain("shell.new:alpha");
    const pane = menuOn({ on: "pane" }).above;
    expect(pane.indexOf("shell.new")).toBe(pane.indexOf("chat.new") + 1);
  });

  it("says the key that opens one on the row, so the palette teaches it", () => {
    const row = by(catalogue(now()), "shell.new");

    expect(row?.note).toContain(SHELL_KEY_SAID);
  });
});

describe("the project switcher (FR-27)", () => {
  const two = [
    { plane: "/one", name: "one" },
    { plane: "/two", name: "two" },
  ];
  const find = (offers: Offer[], id: string) => offers.find((offer) => offer.id === id);

  it("offers one row that opens the switcher, and says its key so the palette teaches it", () => {
    const row = find(catalogue(now({ plane: "/one", projects: two })), "project.switch");

    expect(row?.title).toBe("Switch project…");
    expect(row?.available).toBe(true);
    expect(row?.does).toEqual({ verb: "switchProject" });
    expect(row?.note).toContain(SWITCHER_KEY_SAID);
  });

  it("says why there is nothing to switch to in a window holding one project", () => {
    const row = find(catalogue(now({ plane: "/one", projects: [two[0]] })), "project.switch");

    expect(row?.available).toBe(false);
    expect(row?.reason).toBe("It is the only project in this window.");
  });

  it("asks the window for the switcher, and switches nothing by itself", async () => {
    const done = doing();
    const row = find(catalogue(now({ plane: "/one", projects: two })), "project.switch");
    if (!row) throw new Error("no switcher row");

    await perform(row, done);

    expect(done.calls).toEqual(["switchProject"]);
  });
});

describe("moving a project between windows (charter#126)", () => {
  const two = [
    { plane: "/one", name: "one" },
    { plane: "/two", name: "two" },
  ];
  const find = (offers: Offer[], id: string) => offers.find((offer) => offer.id === id);

  it("offers each project a window of its own, from the palette and the tab's menu", () => {
    const offers = catalogue(now({ plane: "/one", projects: two }));

    const move = find(offers, "project.window:/two");
    expect(move?.title).toBe("Move project two to a new window");
    expect(move?.available).toBe(true);
    expect(move?.does).toEqual({ verb: "moveProject", plane: "/two", to: null });
    expect(menuOn({ on: "project", plane: "/two" }).above).toContain("project.window:/two");
  });

  it("says why a window's only project cannot be split from it", () => {
    const offers = catalogue(now({ plane: "/one", projects: [two[0]] }));

    const move = find(offers, "project.window:/one");
    expect(move?.available).toBe(false);
    expect(move?.reason).toBe("It is the only project in this window.");
  });

  it("offers the way back to the main window only in a split window", () => {
    expect(find(catalogue(now({ plane: "/one", projects: two })), "project.main:/one")).toBe(
      undefined,
    );

    const back = find(
      catalogue(now({ plane: "/one", projects: [two[0]], split: true })),
      "project.main:/one",
    );
    expect(back?.title).toBe("Move project one to the main window");
    expect(back?.available).toBe(true);
    expect(back?.does).toEqual({ verb: "moveProject", plane: "/one", to: "main" });
    expect(menuOn({ on: "project", plane: "/one" }).above).toContain("project.main:/one");
  });
});

describe("renaming a chat (charter-app#254)", () => {
  it("is a row per chat tab, named by the tab's name", () => {
    const tabs = openTab(noTabs(), 7, "3", "steward");
    const row = by(catalogue(now({ tabs })), `tab.rename:${tabs.order[0]}`);

    expect(row?.title).toBe("Rename chat steward 3…");
    expect(row?.available).toBe(true);
    expect(row?.does).toEqual({ verb: "renameTab", tab: tabs.order[0] });
  });
});

describe("a chat's work link (V60, ADR 0088)", () => {
  const tabs = openTab(openTab(noTabs(), 7, "3", "steward"), 8, "4", "steward");
  const [first, second] = tabs.order;
  const inAWorkspace = (session: number) => session === 7;

  it("offers Link to work item… on a chat in a workspace, naming the chat in its note", () => {
    const offers = catalogue(now({ tabs, linkable: inAWorkspace }));
    const row = by(offers, `tab.worklink:${first}`);

    expect(row?.title).toBe("Link to work item…");
    expect(row?.available).toBe(true);
    expect(row?.note).toBe("Chat steward 3");
    expect(row?.does).toEqual({ verb: "linkWorkItem", tab: first });
    expect(by(offers, `tab.workunlink:${first}`)).toBeUndefined();
  });

  it("offers Unlink work item only on a linked chat, and names the item", () => {
    const offers = catalogue(
      now({ tabs, linkable: inAWorkspace, workItems: { 7: "github:github.com/acme/api#12" } }),
    );
    const row = by(offers, `tab.workunlink:${first}`);

    expect(row?.title).toBe("Unlink work item");
    expect(row?.note).toBe("Chat steward 3 · Work item: github:github.com/acme/api#12");
    expect(row?.does).toEqual({ verb: "unlinkWorkItem", tab: first });
    // Linking again is still offered: it replaces the link.
    expect(by(offers, `tab.worklink:${first}`)?.available).toBe(true);
  });

  it("offers neither on a chat at the project root", () => {
    const offers = catalogue(
      now({ tabs, linkable: inAWorkspace, workItems: { 8: "github:github.com/acme/api#12" } }),
    );

    expect(by(offers, `tab.worklink:${second}`)).toBeUndefined();
    expect(by(offers, `tab.workunlink:${second}`)).toBeUndefined();
  });

  it("is on the chat tab's menu, above the line", () => {
    expect(menuOn({ on: "chat", tab: 3 }).above).toEqual(
      expect.arrayContaining(["tab.worklink:3", "tab.workunlink:3"]),
    );
  });

  it("runs through the window's hands", async () => {
    const hands = {
      linkWorkItem: vi.fn(),
      unlinkWorkItem: vi.fn(async () => ({ ok: true }) as const),
    } as unknown as Doing;
    const offers = catalogue(
      now({ tabs, linkable: inAWorkspace, workItems: { 7: "todo:alpha/20261002-080000-x" } }),
    );

    run(offers, `tab.worklink:${first}`, hands);
    await run(offers, `tab.workunlink:${first}`, hands);

    expect(hands.linkWorkItem).toHaveBeenCalledWith(first);
    expect(hands.unlinkWorkItem).toHaveBeenCalledWith(first);
  });
});

describe("the catalogue as the tabs change", () => {
  it("grows a row per tab as tabs open", () => {
    let tabs: Tabs = noTabs();
    expect(ids(catalogue(now({ tabs }))).filter((id) => id.startsWith("tab."))).toEqual([]);

    tabs = openTab(tabs, 7, "one");
    tabs = openTab(tabs, 8, "two");

    expect(ids(catalogue(now({ tabs }))).filter((id) => id.startsWith("tab."))).toEqual([
      "tab.select:1",
      "tab.select:2",
      // The pin rows sit with the switching rows, above the line: pinning is an arrangement
      // and ends nothing, and `frame/leave.py`'s rule is that only the destructive go last.
      "tab.pin:1",
      "tab.pin:2",
      // Renaming is an arrangement too, and ends nothing (charter-app#254).
      "tab.rename:1",
      "tab.rename:2",
      // A chat's Activity only reads (#1495), so it is above the line too.
      "tab.activity:1",
      "tab.activity:2",
      "tab.network:1",
      "tab.network:2",
      "tab.close:1",
      "tab.close:2",
    ]);
  });
});

/**
 * The palette at the scale the limits are written for.
 *
 * charter-app#48 asked whether fifty chats' worth of browsable rows bury the verb the
 * operator typed for, and asked for it to be MEASURED before anything was changed. It does,
 * and these are the measurements, kept as assertions so the answer cannot quietly rot.
 *
 * What was measured, on the catalogue built below — fifty tabs, six workspaces, two chats in
 * the queue, a worktree in front:
 *
 * | typed | `Remove this chat's worktree` was | is now |
 * |-------|-----------------------------------|--------|
 * | `re`  | 41st of 41                        | 2nd    |
 * | `r`   | 86th of 87                        | 12th   |
 *
 * **And the frame's own remedy would not have moved either number.** The tmux frame kept
 * workspaces out of its browsable list; the rows ahead of the verb under `re` are tabs and
 * close-tab rows almost to the last one, and no version of this list has ever left the tabs
 * out. That is why this is ranking and not filtering.
 *
 * ## What charter-app#174 cost this, measured — and what was done about it
 *
 * The explorer's rows needed a row per piece to have a menu at all, so the shape below grew
 * ten clones with five pieces each and the plane's personas — the limits ADR 0026 writes for.
 * That is 183 rows to **291**, and the first cut of it moved `Remove this chat's worktree`
 * down the list:
 *
 * | typed | before #174 | #174, first cut | #174 as merged |
 * |-------|-------------|-----------------|----------------|
 * | `re`  | 4th of 62   | 54th of 164     | **4th of 164** |
 * | `r`   | 19th of 125 | 77th of 233     | **7th of 233** |
 * | `rem` | 1st of 7    | 1st of 57       | 1st of 57      |
 *
 * **The fifty rows that got in the way were all worktree rows, and that made it a different
 * defect from #48 with the same shape on screen.** #48 was forty CHAT NAMES containing `re`;
 * these are fifty merges of named worktrees, every one of them charter's own vocabulary, so
 * #48's rule could not see them — they pass `byItsWords` exactly as the target does. From the
 * operator's seat that distinction buys nothing: the row he wanted was 54th either way.
 *
 * So `narrow` gained a second rule inside that group — `aboutWhatIsInFront`, the colon in the
 * id — and the row is back where it was. The `r` column improves on the BEFORE number too
 * (19th to 7th), because the same rule lifts it above `Delete workspace <name>` and `Switch
 * to project <name>`, which are rows about things the operator is not looking at either.
 * Asserted below as a property — the same rank with the pieces and without — so the next row
 * added about something else has to look at it.
 */
describe("the palette at fifty chats", () => {
  const WORKSPACES = ["ide", "charter", "release", "statusline", "forge", "reddit"];
  /** What a plane's personas look like, at the count a real one carries. */
  const PERSONAS = ["steward", "release", "forge", "reddit", "statusline", "docs", "ops", "qa"];

  /** Fifty chats named the way a plane names them: the workspace, then the chat. */
  function fiftyChats(): Tabs {
    let tabs = noTabs();
    for (let i = 0; i < 50; i++) {
      tabs = openTab(tabs, 100 + i, `${WORKSPACES[i % WORKSPACES.length]}.${i + 1}`);
    }
    return tabs;
  }

  /** The focused workspace at ADR 0026's shape: ten clones, five pieces cut in each. */
  function fiftyPieces() {
    const cut = [];
    for (let repo = 0; repo < 10; repo++)
      for (let piece = 0; piece < 5; piece++)
        cut.push({ workspace: "ide", repo: `repo-${repo}`, piece: `piece-${piece}` });
    return cut;
  }

  /** The same ten clones, each with the path the core spelled. */
  function tenClones() {
    return Array.from({ length: 10 }, (_, repo) => ({
      repo: `repo-${repo}`,
      path: `/plane/workspaces/ide/repo-${repo}`,
    }));
  }

  const loaded = () =>
    catalogue(
      now({
        tabs: fiftyChats(),
        workspaces: WORKSPACES,
        focused: "ide",
        plane: "/plane",
        worktree: PIECE,
        pieces: fiftyPieces(),
        clones: tenClones(),
        personas: PERSONAS,
        needsYou: [103, 107],
        nameOf: (session) => `chat ${session}`,
      }),
    );

  it("puts the verb ahead of every name that merely shares its letters", () => {
    // `re` is in `release`, in `reddit` and in `worktree`. Only the last is a word charter
    // chose; the rest are somebody's chat names. **And `create` is one of charter's words
    // too**, which is why the two rows that make things come first: `workspace.create` and
    // `project.create` are matched on charter's own half of the id, exactly as `worktree` is,
    // and within that group the catalogue's own order stands. Every row here is a verb.
    const rows = narrow("re", loaded());

    const verbs = rows.slice(0, 12).map((row) => row.title);
    expect(verbs).toEqual([
      "New workspace…",
      "New project…",
      // `vault.create` is charter's `create` too (charter-app#235), and joins the rows that make
      // things. (`Settings…`, SE-16, has no `re` in it, as `Preferences…` had.)
      "New persona…",
      // `shared` has `re` in it, and `memory.shared` is charter's word (SI-9c).
      "Open shared memory",
      "New vault…",
      // `Previous` has `re` in it, and the row is about the tab in front (#1487): it stands
      // with the rows about what is in front, in the catalogue's order.
      "Previous chat in this tab",
      // **Both of the chat in front's rows, then the pieces'.** `aboutWhatIsInFront` is the
      // second rule inside this group (charter-app#174): a row with no name in its id acts on
      // what the operator is looking at, and fifty rows about other worktrees do not get to
      // stand in front of it. Inside each half the catalogue's own order stands.
      "Merge this chat's branch into its clone",
      "Remove the folder of this chat's branch",
      // The left side's rows (#1673): about this window, and `Explorer` has `re` in it.
      "Show the Explorer view",
      "Put the Navigation region away",
      // Then the rows about things that are not in front, in the catalogue's order.
      // `ignore` has `re` in it, and it is charter's word, so the two queued chats' Ignore
      // rows (charter-app#248) are verbs here too, and still behind every row about what is
      // in front. Then a rename per chat (charter-app#254), before the pieces, as the tab
      // rows always have.
      "Ignore chat 103 until it asks again",
      "Ignore chat 107 until it asks again",
    ]);
    // Not a cap and not a filter: every name that matched is still listed, below.
    expect(rows.some((row) => row.title === "Switch to tab release.3")).toBe(true);
  });

  it("leaves a name findable by its own name, which is what a name row is for", () => {
    const rows = narrow("release.3", loaded());

    expect(rows[0].title).toBe("Switch to tab release.3");
  });

  it("aims Enter at a verb rather than at a chat that happens to sort first", () => {
    const rows = narrow("re", loaded());

    expect(rows[aim(rows)].title).toBe("New workspace…");
  });

  /**
   * A hundred rows about other worktrees do not move the row about this one.
   *
   * **This is the guard charter-app#174 needed and #48's rule could not give.** #48 split
   * charter's own words from somebody's name; every row in this fight passes that test, so
   * the fifty per-piece merges sat in front of `Remove this chat's worktree` on charter's own
   * vocabulary — 4th of 62 to 54th of 164, measured before it was fixed. `aboutWhatIsInFront`
   * is the second rule inside that group, and what it buys is asserted as a property rather
   * than as a rank: **the same place, with the pieces and without them.** A row added about
   * something the operator is not looking at fails here rather than being found in the
   * palette at fifty chats.
   */
  describe("where the chat in front's own rows land", () => {
    /** The same window with the pieces and the personas taken out — the catalogue as it was
     *  before #174, so the two can be compared rather than described. */
    const before = () =>
      catalogue(
        now({
          tabs: fiftyChats(),
          workspaces: WORKSPACES,
          focused: "ide",
          plane: "/plane",
          worktree: PIECE,
          needsYou: [103, 107],
          nameOf: (session) => `chat ${session}`,
        }),
      );

    const at = (typed: string, offers: Offer[]) =>
      narrow(typed, offers).findIndex(
        (row) => row.title === "Remove the folder of this chat's branch",
      ) + 1;

    it("is exactly where it was before a hundred rows were added around it", () => {
      for (const typed of ["re", "r", "rem", "worktree", "remove"]) {
        expect({ typed, rank: at(typed, loaded()) }).toEqual({
          typed,
          rank: at(typed, before()),
        });
      }
    });

    it("is near the top of what was typed, and not fifty rows down it", () => {
      // The numbers themselves, so "unchanged" cannot be satisfied by both being bad.
      // Two further down than #174 left it under `re` and `r`: `New vault…` (charter-app#235)
      // and `New persona…` (SI-3) are rows that make/land near the creates. One more since
      // SI-9c: `Open shared memory` (`shared` has `re`). And one more under `r` since FR-27:
      // `Switch project…` (`project` has an `r`). One up under both since SE-16: `Settings…`
      // took `Preferences…`'s place, and has neither an `re` nor an `r`. One more under `r`
      // since SE-23: `Your settings…` (`your` has an `r`). One more under `r` since #1499:
      // `Open a chat beside the one in front` (`front` has an `r`).
      // One more under both since #1487: `Previous chat in this tab`, a row about the tab in
      // front, before the branch's two in the catalogue. One more under `r` since #1137:
      // `Search in files` (`search` has an `r`).
      expect(at("re", loaded())).toBe(8);
      expect(at("r", loaded())).toBe(15);
      expect(at("rem", loaded())).toBe(1);
    });

    it("leaves every row that was added still findable, because this is ranking", () => {
      const rows = narrow("re", loaded());

      expect(rows.filter((row) => row.id.startsWith("worktree.merge:"))).toHaveLength(50);
      expect(rows.filter((row) => row.id.startsWith("worktree.remove:"))).toHaveLength(50);
    });

    it("puts a piece the operator named in full first, ahead of the row about this chat", () => {
      // The rule above is about CHARTER'S words. A name typed in full is the operator saying
      // which piece they mean, and #48's first group has always won over everything.
      const rows = narrow("Remove folder piece-3 in repo-7", loaded());

      expect(rows[0].id).toBe("worktree.remove:repo-7/piece-3");
    });
  });

  it("does not reorder anything when every row matched charter's own word", () => {
    // `switch` is charter's word on fifty rows and nobody's name. The rule must be a
    // partition and not a score: rows that all match the same way keep the catalogue's order.
    const offers = loaded();
    const selects = offers.filter((row) => row.id.startsWith("tab.select:"));
    // The switcher's own row (FR-27) is the one `switch` row about what is in front — it has no
    // name after a colon — so it leads, and the fifty after it keep their order.
    const switcher = offers.filter((row) => row.id === "project.switch");

    expect(switcher).toHaveLength(1);
    expect(narrow("switch", offers)).toEqual([...switcher, ...selects]);
  });

  it("still offers one row per chat and per workspace, browsable with nothing typed", () => {
    // The count itself is the measurement #48 asked for, asserted rather than described: a
    // row added without thinking about this is a failing test, not a surprise at fifty chats.
    const offers = loaded();

    expect(offers.filter((row) => row.id.startsWith("tab.select:"))).toHaveLength(50);
    expect(offers.filter((row) => row.id.startsWith("tab.close:"))).toHaveLength(50);
    expect(offers.filter((row) => row.id.startsWith("workspace.focus:"))).toHaveLength(6);
    // And one pin row per chat and per workspace (ADR 0039). **This is the cost of
    // pinning through the palette rather than through a control on every tab**, and it is
    // the number that decides whether that was the right trade: the catalogue is half as
    // long again. It buys back fifty controls on the one strip that broke at fifty
    // (charter-app#130), and `narrow` ranks a verb the operator typed above any row that
    // merely carries a name, so the rows these crowd are other names and not the verbs.
    expect(offers.filter((row) => row.id.startsWith("tab.pin:"))).toHaveLength(50);
    expect(offers.filter((row) => row.id.startsWith("workspace.pin:"))).toHaveLength(6);
    // And one rename row per chat (charter-app#254), for the pin's reason: it is how the
    // tab's menu and the palette are one surface, and a verb typed still ranks first.
    expect(offers.filter((row) => row.id.startsWith("tab.rename:"))).toHaveLength(50);
    // One row per workspace that can be deleted, and never one for the strip of chats
    // outside every workspace: that strip is not a workspace on the plane, and there is
    // nothing on disk for a delete to name.
    expect(offers.filter((row) => row.id.startsWith("workspace.remove:"))).toHaveLength(6);
    // **And two rows per piece of the focused workspace** (charter-app#174), which is what the
    // explorer's rows needed to have a menu at all. Ten clones with five pieces each is a
    // hundred rows on a list of 183 — the biggest single thing ever added to it, and the
    // reason the issue asked for a number before it was allowed to grow. One row per persona
    // beside them, which is cheap: a plane has a handful.
    expect(offers.filter((row) => row.id.startsWith("worktree.merge:"))).toHaveLength(50);
    expect(offers.filter((row) => row.id.startsWith("worktree.remove:"))).toHaveLength(50);
    // And a third (charter#368): mark it done, so a finished piece stops reading as silent.
    expect(offers.filter((row) => row.id.startsWith("worktree.done:"))).toHaveLength(50);
    expect(offers.filter((row) => row.id.startsWith("persona.show:"))).toHaveLength(8);
    // And two more per persona (SI-3): edit its persona.md, and delete it.
    expect(offers.filter((row) => row.id.startsWith("persona.edit:"))).toHaveLength(8);
    expect(offers.filter((row) => row.id.startsWith("persona.remove:"))).toHaveLength(8);
    // And its profile (#1445): the harness profile its chats start on.
    expect(offers.filter((row) => row.id.startsWith("persona.profile:"))).toHaveLength(8);
    // Two rows per clone (charter-app#174, the second half): a new tab in it and the pick.
    // Ten clones is twenty rows, on the shape above; `narrow` is held to the same rank with
    // them and without them two tests up.
    expect(offers.filter((row) => row.id.startsWith("clone.chat:"))).toHaveLength(10);
    expect(offers.filter((row) => row.id.startsWith("clone.pick:"))).toHaveLength(10);
    // And a third per clone (GL-1): New branch…, ten more rows.
    expect(offers.filter((row) => row.id.startsWith("clone.branch:"))).toHaveLength(10);
    // One settings row per workspace (charter-app#280), and none for the strip outside.
    expect(offers.filter((row) => row.id.startsWith("workspace.settings:"))).toHaveLength(6);
    // One new-shell row per workspace (SI-5), and none for the strip outside.
    expect(offers.filter((row) => row.id.startsWith("shell.new:"))).toHaveLength(6);
    // One changes row, for the focused workspace only (charter#470).
    expect(offers.filter((row) => row.id.startsWith("workspace.changes:"))).toHaveLength(1);
    // 436 rows: 50 chats four times over, 6 workspaces SIX times, 50 pieces THRICE, 10
    // clones TWICE, 8 personas, 2 in the queue TWICE (show it, and ignore it — charter-app#248),
    // and the sixteen verbs — the sixteenth is Settings (SE-16) — plus the vault picker and New vault…
    // (charter-app#235; this plane has no vaults, so no `vault.open:` rows). It was 118 before the pins, 174 before
    // the extension list (ADR 0041), 175 before a workspace could be made and deleted
    // from the window, 183 before the explorer's rows had anything to offer, 291 before
    // the row that puts `charter` on a terminal's PATH, 292 before a queued chat could be
    // ignored, 294 before a chat could be renamed (charter-app#254), 345 before a clone
    // could be picked from its own menu, 367 before a workspace had settings, 373 before
    // a workspace could be made LIVE or LOCAL (charter-app#301), 379 before one could be
    // renamed (charter#367), 385 before a piece could be marked done (charter#368), and 435 before
    // the focused workspace's changes had a row (charter#470). What the
    // hundred buys is the surface the operator asked for and the menu system could not reach;
    // what it costs is measured on `narrow` two tests up and on `menuRows` below.
    //
    // 453 since SI-3 (436 before it): New persona…, and an edit and a delete row for each of the 8 personas.
    // 460 since SI-5: a shell tab's row, and one per workspace.
    // 471 since SI-9c: a new memory in the focused workspace, in each of the 8 personas and in
    // the shared store, and the shared list's own row.
    // 481 since GL-1: New branch… in each of the ten clones.
    // 482 since FR-27: Switch project…, one row however many projects the window holds.
    // 532 since RC-5: Browse the files of each of the 50 pieces.
    // 582 since FM-5: Focus on each of the 50 branches.
    // 592 since KN-4: the archive of the focused workspace's journal, of each of the 8
    // personas' stores and of the shared store.
    // 593 since SE-23: Your settings…, one row.
    // 601 since #1445: Set <persona>'s profile…, one row per persona.
    // 602 since #1452: Open dispatches, one row.
    // 603 since #1499: Open a chat beside the one in front, one row and not one per chat.
    // 607 since #1487: the tab in front's task menu, its next and previous chat, and back to
    // its own chat. Four rows, however many tabs and tasks there are.
    // 657 since #1495: Activity, one row per chat.
    // 658 since #1137: Search in files, one row.
    // 679 since #1201: a row per Settings group — You's 4, the project's 11 and the focused
    // workspace's 6. A fixed number, however many workspaces there are.
    // 829 since #1143: each of the 50 branches' own folder copied, revealed and given a shell
    // tab, three rows a branch as its browse and focus rows are.
    // 839 since #1152: Focus on repo, one row in each of the ten clones.
    // 842 since #1673: Show the Chats view, Show the Explorer view, and the Navigation region.
    // 893 since #1676: Show the Changes view (Search's row is Search in files).
    // This window has no todos loaded, so no `todo.` rows.
    expect(offers).toHaveLength(893);
  });

  /**
   * What a context menu costs the strip it is on, at the same limits.
   *
   * charter-app#174 named this before it allowed the rows above to exist: `menuRows` looked a
   * row up by scanning the catalogue, and a context menu on a strip is drawn per tab per
   * render — so one render of a fifty-tab strip was fifty scans of a list the same change was
   * making 291 long. **Measured on this machine, one render of that strip, min of ten batches
   * with each arm in its own process:**
   *
   * | the catalogue          | offers touched | scanning | through `catalogued` |
   * |------------------------|----------------|----------|----------------------|
   * | 183 rows (before #174) | 13,375         | 0.047 ms | 0.017 ms             |
   * | 291 rows (after)       | 16,275         | 0.049 ms | 0.017 ms             |
   *
   * **31 µs is not a speed anybody feels**, and #133 refused a change for less. What is
   * different is the shape: the scan's cost is the catalogue's length, and #174 is the change
   * that grew it — a third more comparisons for the same fifty menus, before anything is
   * added next. A millisecond assertion would be flaky on a shared runner, so what is pinned
   * below is the work itself, which is the standard #133 set: 250 lookups (150 before a chat
   * could be renamed, charter-app#254; 200 before a wrapping-up tab could cancel its smart
   * close, ADR 0064), and the same 250 whichever catalogue it is.
   */
  describe("what a menu on the chat strip costs (charter-app#174)", () => {
    /** A catalogue that counts what is asked of it. `Map` and not a stand-in, so what is
     *  counted is what `menuRows` actually does. */
    class Counting extends Map<string, Offer> {
      lookups = 0;
      override get(id: string): Offer | undefined {
        this.lookups += 1;
        return super.get(id);
      }
    }

    /** One render of the chat strip: every tab draws a menu, and every menu asks. */
    function strip(offers: Counting) {
      for (let tab = 1; tab <= 50; tab++) menuRows({ on: "chat", tab }, offers);
      return offers.lookups;
    }

    it("asks for eleven rows per tab and never walks the list", () => {
      const offers = new Counting(loaded().map((offer) => [offer.id, offer]));

      // 50 tabs × the eleven ids a chat menu lists (the two work link rows are V60's, Start
      // fresh is NO-3's, Restart chat is #1428's, Activity is #1495's, Network is #1662's).
      // **Not fifty scans of 291 rows**, which is
      // what this cost before the lookup was built once for the window — and the number that
      // does not move when the catalogue grows again.
      expect(strip(offers)).toBe(550);
    });

    it("is the same 550 whether the catalogue carries the pieces or not", () => {
      // The property, not the timing: the cost of a menu is flat in the length of the list it
      // reads. A scan is not, which is why #174's hundred rows needed this first.
      const small = new Counting(
        catalogue(now({ tabs: fiftyChats(), workspaces: WORKSPACES, focused: "ide" })).map(
          (offer) => [offer.id, offer],
        ),
      );

      expect(strip(small)).toBe(550);
    });
  });
});

describe("the key the palette claimed", () => {
  it("offers a row that hands it to the chat in front", () => {
    // charter-app#47: the palette takes F2 capture-phase, so a harness that binds F2 never
    // sees it. The way out is a row like any other — browsable, typeable, and the same
    // thing the second F2 runs.
    const offers = catalogue(now({ tabs: openTab(noTabs(), 7, "one") }));

    const row = by(offers, "pane.sendkey");
    expect(row?.available).toBe(true);
    expect(row?.title).toBe("Send F2 to the chat in front");
    expect(row?.does).toEqual({ verb: "sendKey", key: "F2" });
  });

  it("is findable by the key's own name", () => {
    const offers = catalogue(now({ tabs: openTab(noTabs(), 7, "one") }));

    expect(narrow("F2", offers).map((row) => row.id)).toEqual(["pane.sendkey"]);
  });

  it("says why it cannot run rather than going missing when no chat is in front", () => {
    const row = by(catalogue(now()), "pane.sendkey");

    expect(row?.available).toBe(false);
    expect(row?.reason).toBe("No chat is in front, so there is nowhere to send it.");
  });
});

describe("what the queue's row claims", () => {
  it("says nothing needs you when every open chat can say whether it does", () => {
    expect(by(catalogue(now()), "needs.next")?.reason).toBe("Nothing needs you.");
  });

  it("does not claim it while a chat cannot say (charter-app#52)", () => {
    // A Codex chat stopped mid-turn for an approval reports nothing, and there is no signal
    // for it that is not a hook deciding a permission. So the row says what is known.
    const reason = by(catalogue(now({ quiet: ["ide.7"] })), "needs.next")?.reason;

    expect(reason).toBe(
      "Nothing has said it needs you — and ide.7 can be waiting on you without saying so.",
    );
  });

  it("counts them rather than listing them all", () => {
    const reason = by(catalogue(now({ quiet: ["ide.7", "ide.8"] })), "needs.next")?.reason;

    expect(reason).toBe(
      "Nothing has said it needs you — and 2 chats can be waiting on you without saying so.",
    );
  });
});

describe("curation actions (ADR 0061)", () => {
  /** What the core answers for the plane and workspace `alpha`. */
  function curations(cannot: string | null = null): NonNullable<Now["curations"]> {
    const action = (id: string, label: string, by: string | null, runner: string | null) => ({
      id,
      label,
      declared_by: by,
      runner,
      cwd: "/plane/workspaces/alpha",
      prompt: `${label}.`,
    });
    return {
      subjects: [
        {
          subject: "plane",
          name: "plane",
          actions: [],
          left_out: [],
          trouble: null,
        },
        {
          subject: "workspace:alpha",
          name: "alpha",
          actions: [
            action("charter/safe-remove", "Safe remove", null, null),
            action("ops/tidy", "Tidy", "ops", "ops"),
          ],
          left_out: [
            {
              what: "personas/qa/curation/x.md",
              why: "personas/qa/curation/x.md is not offered: no label.",
            },
          ],
          trouble: null,
        },
        {
          subject: "workspace:gone",
          name: "workspace:gone",
          actions: [],
          left_out: [],
          trouble: "no workspace 'gone'",
        },
      ],
      cannot,
    };
  }

  it("lists each action as a palette row named for its subject, in the core's order", () => {
    const offers = catalogue(now({ plane: "/plane", curations: curations() }));

    const rows = offers.filter((offer) => offer.id.startsWith("curate:"));

    expect(rows.map((row) => [row.id, row.title, row.available])).toEqual([
      ["curate:workspace:alpha/charter/safe-remove", "Curate alpha: Safe remove", true],
      ["curate:workspace:alpha/ops/tidy", "Curate alpha: Tidy", true],
      ["curate:workspace:alpha/!0", "Curate alpha: personas/qa/curation/x.md is left out", false],
      ["curate:workspace:gone/!", "Curate workspace:gone", false],
    ]);
    expect(by(offers, "curate:workspace:alpha/!0")?.reason).toContain("no label");
    expect(by(offers, "curate:workspace:gone/!")?.reason).toBe("no workspace 'gone'");
  });

  it("says who runs it and where, and that nothing is sent", () => {
    const offers = catalogue(now({ plane: "/plane", curations: curations() }));

    expect(by(offers, "curate:workspace:alpha/ops/tidy")?.note).toBe(
      "Opens a chat as ops in /plane/workspaces/alpha, with its prompt typed and not sent.",
    );
    expect(by(offers, "curate:workspace:alpha/charter/safe-remove")?.note).toContain(
      "with no persona",
    );
  });

  it("is found by the palette under the word curate", () => {
    const offers = catalogue(now({ plane: "/plane", curations: curations() }));

    expect(ids(narrow("curate", offers))).toContain("curate:workspace:alpha/ops/tidy");
  });

  it("opens the chat through the window's curate, by subject and action id", async () => {
    const hands = doing();
    const offers = catalogue(now({ plane: "/plane", curations: curations() }));

    await run(offers, "curate:workspace:alpha/ops/tidy", hands);

    expect(hands.calls).toEqual(["curate:workspace:alpha,ops/tidy"]);
  });

  it("offers nothing to run when the project's default harness cannot be typed into", async () => {
    const hands = doing();
    const offers = catalogue(
      now({
        plane: "/plane",
        curations: curations("opencode says nothing until your first prompt"),
      }),
    );

    const said = await run(offers, "curate:workspace:alpha/ops/tidy", hands);

    expect(said).toEqual({ ok: false, refused: "opencode says nothing until your first prompt" });
    expect(hands.calls).toEqual([]);
  });

  it("groups a subject's rows for its submenu: charter's, each persona's, what was left out", () => {
    const offers = catalogued(catalogue(now({ plane: "/plane", curations: curations() })));

    const rows = curateRows("workspace:alpha", offers);

    expect(ids(rows.charter)).toEqual(["curate:workspace:alpha/charter/safe-remove"]);
    expect(rows.personas.map((group) => [group.persona, ids(group.rows)])).toEqual([
      ["ops", ["curate:workspace:alpha/ops/tidy"]],
    ]);
    expect(ids(rows.leftOut)).toEqual(["curate:workspace:alpha/!0"]);
    // A subject whose name starts another's is not mixed into it.
    expect(curateRows("workspace:alph", offers).charter).toEqual([]);
  });

  it("is offered on a workspace, a persona and the plane root's tab", () => {
    expect(curateSubjectOf({ on: "workspace", workspace: "alpha" })).toBe("workspace:alpha");
    expect(curateSubjectOf({ on: "persona", persona: "ops" })).toBe("persona:ops");
    expect(curateSubjectOf({ on: "root" })).toBe("plane");
    expect(curateSubjectOf({ on: "workspace", workspace: OUTSIDE })).toBeUndefined();
    expect(curateSubjectOf({ on: "chat", tab: 1 })).toBeUndefined();
  });
});

describe("a memory's rows (SI-9b, ADR 0065)", () => {
  const ref: MemoryRef = {
    scope: { kind: "persona", name: "steward" },
    slug: "defects-go-upstream",
  };
  const key = "persona/steward/defects-go-upstream";

  it("are Open and Edit above the line and Delete below it, as the row's menu lists them", () => {
    expect(menuOn({ on: "memory", key })).toEqual({
      above: [`memory.open:${key}`, `memory.edit:${key}`],
      below: [`memory.delete:${key}`],
    });
  });

  it("open, edit and archive that memory, by its store and slug", async () => {
    const hands = doing();
    const offers = memoryOffers(ref, "Defects go upstream");

    await run(offers, `memory.open:${key}`, hands);
    await run(offers, `memory.edit:${key}`, hands);
    await run(offers, `memory.delete:${key}`, hands);

    expect(hands.calls).toEqual([
      `openMemory:${key},Defects go upstream,false`,
      `editMemory:${key},Defects go upstream`,
      `archiveMemory:${key},Defects go upstream`,
    ]);
  });

  it("open as a preview on a single click, and kept on a double-click", async () => {
    const hands = doing();
    const [open, edit] = memoryOffers(ref, "Defects go upstream");

    await perform(toKeep(open), hands);

    expect(hands.calls).toEqual([`openMemory:${key},Defects go upstream,true`]);
    // Every other row is its own double-click.
    expect(toKeep(edit)).toBe(edit);
  });

  it("say Delete archives it, and that Undo brings it back", () => {
    const del = memoryOffers(ref, "Defects go upstream")[2];
    expect(del.id).toBe(`memory.delete:${key}`);
    expect(del.title).toBe("Delete memory: Defects go upstream");
    expect(del.note).toMatch(/archive/i);
    expect(del.note).toMatch(/undo/i);
  });

  it("are in the catalogue for every open memory tab, so its heading's Edit and Delete run", () => {
    const tabs = openView(noTabs(), memoryView(ref), "Defects go upstream", "alpha");

    const offers = catalogue(now({ tabs }));

    expect(ids(offers)).toEqual(
      expect.arrayContaining([`memory.open:${key}`, `memory.edit:${key}`, `memory.delete:${key}`]),
    );
    expect(by(offers, `memory.edit:${key}`)?.title).toBe("Edit memory: Defects go upstream");
  });

  describe("Move (#1190)", () => {
    const stores: MemoryScope[] = [
      { kind: "workspace", name: "alpha" },
      { kind: "persona", name: "steward" },
      { kind: "persona", name: "ops" },
      { kind: "shared" },
    ];
    const moves = (offers: readonly Offer[]) =>
      offers.filter((offer) => offer.does.verb === "moveMemory");

    it("offers a row per store but its own, by `memory.move:<key>:<store>`", () => {
      const rows = moves(memoryOffers(ref, "Defects go upstream", stores));

      expect(ids(rows)).toEqual([
        `memory.move:${key}:workspace/alpha`,
        `memory.move:${key}:persona/ops`,
        `memory.move:${key}:shared`,
      ]);
      expect(rows.map((row) => row.title)).toEqual([
        "Move memory to alpha's memory: Defects go upstream",
        "Move memory to ops's memory: Defects go upstream",
        "Move memory to shared memory: Defects go upstream",
      ]);
      expect(memoryMoveId(key, { kind: "shared" })).toBe(`memory.move:${key}:shared`);
    });

    it("says the tab's audience sentence on a move into a store the project publishes", () => {
      const rows = moves(memoryOffers(ref, "Defects go upstream", stores));

      expect(rows.map((row) => row.note)).toEqual([
        undefined,
        PUBLISHED_WITH_THE_PROJECT,
        PUBLISHED_WITH_THE_PROJECT,
      ]);
    });

    it("says a move into a LIVE workspace's journal is published, and one into a LOCAL one nothing", () => {
      const more: MemoryScope[] = [...stores, { kind: "workspace", name: "beta" }];
      const rows = moves(memoryOffers(ref, "Defects go upstream", more, ["beta"]));

      expect(rows.map((row) => [row.id, row.note])).toEqual([
        [`memory.move:${key}:workspace/alpha`, undefined],
        [`memory.move:${key}:persona/ops`, PUBLISHED_WITH_THE_PROJECT],
        [`memory.move:${key}:shared`, PUBLISHED_WITH_THE_PROJECT],
        [
          `memory.move:${key}:workspace/beta`,
          "beta is LIVE, so its journal is published with the project.",
        ],
      ]);
    });

    it("says it on a memory list's rows and an open memory tab's, from the LIVE names lent", () => {
      const listed = listedMemoryOffers(
        [
          {
            kind: "list",
            empty: { headline: "", body: null, offer: null },
            rows: [
              {
                key: "a",
                text: "Defects go upstream",
                note: null,
                mark: "",
                tone: "",
                detail: null,
                runs: `memory.open:${key}`,
                actions: [],
              },
            ],
          },
        ],
        stores,
        ["alpha"],
      );
      expect(listed.get(`memory.move:${key}:workspace/alpha`)?.note).toBe(
        "alpha is LIVE, so its journal is published with the project.",
      );

      const tabs = openView(noTabs(), memoryView(ref), "Defects go upstream", "alpha");
      const offers = catalogue(now({ tabs, memoryStores: stores, live: ["alpha"] }));
      expect(by(offers, `memory.move:${key}:workspace/alpha`)?.note).toBe(
        "alpha is LIVE, so its journal is published with the project.",
      );
    });

    it("offers no Move while the stores are unread, and none where its own is the only one", () => {
      expect(moves(memoryOffers(ref, "Defects go upstream"))).toEqual([]);
      expect(moves(memoryOffers(ref, "Defects go upstream", [ref.scope]))).toEqual([]);
    });

    it("moves that memory into that store, asking nothing first", async () => {
      const hands = doing();
      const offers = memoryOffers(ref, "Defects go upstream", stores);

      await run(offers, `memory.move:${key}:shared`, hands);

      expect(hands.calls).toEqual([`moveMemory:${key},Defects go upstream,{"kind":"shared"}`]);
    });

    it("says the core's refusal in its words", async () => {
      const hands = doing();
      hands.moveMemory = vi.fn(async () => ({
        ok: false as const,
        refused: "shared memory already holds defects-go-upstream",
      }));
      const [row] = moves(memoryOffers(ref, "Defects go upstream", stores));

      expect(await perform(row, hands)).toEqual({
        ok: false,
        refused: "shared memory already holds defects-go-upstream",
      });
    });

    it("are what a row's Move to submenu draws, and only that memory's", () => {
      const other = { scope: ref.scope, slug: "defects" };
      const offers = catalogued([
        ...memoryOffers(ref, "Defects go upstream", stores),
        ...memoryOffers(other, "Defects", stores),
      ]);

      expect(ids(moveRows(key, offers))).toEqual([
        `memory.move:${key}:workspace/alpha`,
        `memory.move:${key}:persona/ops`,
        `memory.move:${key}:shared`,
      ]);
      // A memory whose slug starts another's is not mixed into it.
      expect(moveRows("persona/steward/defects", offers).map((row) => row.name)).toEqual([
        "Defects",
        "Defects",
        "Defects",
      ]);
    });

    it("are listed for a memory list's rows, and for an open memory tab in the palette", () => {
      const listed = listedMemoryOffers(
        [
          {
            kind: "list",
            empty: { headline: "", body: null, offer: null },
            rows: [
              {
                key: "a",
                text: "Defects go upstream",
                note: null,
                mark: "",
                tone: "",
                detail: null,
                runs: `memory.open:${key}`,
                actions: [],
              },
            ],
          },
        ],
        stores,
      );
      expect(listed.has(`memory.move:${key}:shared`)).toBe(true);

      const tabs = openView(noTabs(), memoryView(ref), "Defects go upstream", "alpha");
      expect(ids(catalogue(now({ tabs, memoryStores: stores })))).toEqual(
        expect.arrayContaining([`memory.move:${key}:shared`, `memory.move:${key}:persona/ops`]),
      );
    });
  });

  it("are not offered for a new memory's tab, which has nothing yet to edit or delete", () => {
    const tabs = openView(
      noTabs(),
      memoryView({ scope: { kind: "shared" }, slug: DRAFT }),
      "New memory",
      "alpha",
    );

    expect(ids(catalogue(now({ tabs }))).filter((id) => id.startsWith("memory."))).toEqual([]);
  });

  it("offer to keep a preview tab, and only a preview tab", async () => {
    // In the palette and on a double-click of the tab, and not on the tab's menu: that menu is
    // five rows a tab at fifty tabs (charter-app#174), and a keep is not worth a sixth.
    const previewing = openPreview(noTabs(), memoryView(ref), "Defects go upstream", "alpha");
    const kept = openView(noTabs(), memoryView(ref), "Defects go upstream", "alpha");
    const hands = doing();

    const offers = catalogue(now({ tabs: previewing }));
    await run(offers, `tab.keep:${previewing.order[0]}`, hands);

    expect(hands.calls).toEqual([`keepTab:${previewing.order[0]}`]);
    expect(ids(catalogue(now({ tabs: kept })))).not.toContain(`tab.keep:${kept.order[0]}`);
  });
});

describe("making a memory, and the shared list (SI-9c, ADR 0065 Q6, Q9)", () => {
  it("offers a new memory in the focused workspace, in each persona and in the shared store", async () => {
    const hands = doing();
    const offers = catalogue(
      now({ plane: "/p", workspaces: ["alpha"], focused: "alpha", personas: ["steward"] }),
    );

    await run(offers, "memory.new:workspace/alpha", hands);
    await run(offers, "memory.new:persona/steward", hands);
    await run(offers, "memory.new:shared", hands);

    expect(hands.calls).toEqual([
      `newMemory:${JSON.stringify({ kind: "workspace", name: "alpha" })}`,
      `newMemory:${JSON.stringify({ kind: "persona", name: "steward" })}`,
      `newMemory:${JSON.stringify({ kind: "shared" })}`,
    ]);
    // The palette finds them by what they make and where.
    expect(by(offers, "memory.new:workspace/alpha")?.title).toBe("New memory in alpha…");
    expect(by(offers, "memory.new:persona/steward")?.title).toBe("New memory for steward…");
    expect(by(offers, "memory.new:shared")?.title).toBe("New shared memory…");
  });

  it("offers no workspace's new memory with the plane root or nothing focused", () => {
    // The plane root has no journal (SI-1), so there is nowhere for one to go.
    for (const focused of [undefined, OUTSIDE]) {
      const offers = catalogue(now({ plane: "/p", focused }));
      expect(ids(offers).filter((id) => id.startsWith("memory.new:workspace/"))).toEqual([]);
    }
  });

  it("opens the shared list in a tab of its own, from the Personas panel's shared row", async () => {
    const hands = doing();
    const offers = catalogue(now({ plane: "/p" }));

    await run(offers, "memory.shared", hands);

    expect(by(offers, "memory.shared")?.does).toEqual({
      verb: "openView",
      view: { from: null, view: "shared-memory", key: "" },
      title: "Shared memory",
    });
  });

  it("opens each store's archive in a tab of its own (KN-4)", async () => {
    const hands = doing();
    const offers = catalogue(
      now({ plane: "/p", workspaces: ["alpha"], focused: "alpha", personas: ["steward"] }),
    );

    expect(by(offers, "memory.archived:workspace/alpha")?.does).toEqual({
      verb: "openView",
      view: { from: null, view: "memory-archive", key: "workspace/alpha" },
      title: "Archived memory · alpha",
    });
    expect(by(offers, "memory.archived:persona/steward")?.does).toEqual({
      verb: "openView",
      view: { from: null, view: "memory-archive", key: "persona/steward" },
      title: "Archived memory · steward",
    });
    expect(by(offers, "memory.archived:shared")?.does).toEqual({
      verb: "openView",
      view: { from: null, view: "memory-archive", key: "shared" },
      title: "Archived shared memory",
    });
    // The row is a verb; the tab it opens is named for what it holds.
    expect(by(offers, "memory.archived:workspace/alpha")?.title).toBe("Open alpha's archive");
    expect(by(offers, "memory.archived:persona/steward")?.title).toBe("Open steward's archive");
    expect(by(offers, "memory.archived:shared")?.title).toBe("Open the shared archive");
    await run(offers, "memory.archived:shared", hands);
    expect(hands.calls).toHaveLength(1);
    // The plane root has no journal, and so no journal's archive.
    expect(
      ids(catalogue(now({ plane: "/p", focused: OUTSIDE }))).filter((id) =>
        id.startsWith("memory.archived:workspace/"),
      ),
    ).toEqual([]);
  });

  it("offers neither with no plane to keep a memory in", () => {
    const offers = catalogue(now({ personas: ["steward"] }));

    expect(ids(offers).filter((id) => id.startsWith("memory."))).toEqual([]);
  });
});

describe("a branch's file and folder rows (FM-10)", () => {
  const at = (path: string): BranchPath => ({
    workspace: "alpha",
    repo: "svc",
    piece: "fix-it",
    path,
  });
  const file = fileRows({ on: "file", at: at("src/lib.rs"), kind: "file" });
  const folder = fileRows({ on: "file", at: at("src"), kind: "folder" });
  const link = fileRows({ on: "file", at: at("CLAUDE.md"), kind: "link" });
  const titles = (rows: readonly Offer[]) => rows.map((row) => row.title);
  const verbs = (rows: readonly Offer[]) => rows.map((row) => row.does.verb);

  it("offers a file its paths, a reveal and your editor", () => {
    expect(titles(file)).toEqual([
      "Copy relative path",
      "Copy absolute path",
      revealSaid(navigator.platform),
      "Open in your editor",
      "Start a chat here",
      "Add to a chat's context",
    ]);
  });

  it("offers a folder its paths, a reveal and a shell tab there", () => {
    expect(titles(folder)).toEqual([
      "Copy relative path",
      "Copy absolute path",
      revealSaid(navigator.platform),
      "Open a shell tab here",
      "Start a chat here",
      "Add to a chat's context",
    ]);
  });

  it("names the file manager the platform has", () => {
    expect(revealSaid("MacIntel")).toBe("Reveal in Finder");
    expect(revealSaid("Win32")).toBe("Reveal in File Explorer");
    expect(revealSaid("Linux x86_64")).toBe("Reveal in Files");
  });

  it("offers no row that creates, renames, moves or deletes, on any row (V86 F8, ADR 0081)", () => {
    const every = [
      ...file,
      ...folder,
      ...link,
      ...fileRows({ on: "file", at: at("x"), kind: "file", refused: "ignored" }),
    ];
    const writes = /creat|new|renam|mov|delet|remov|trash|archiv|discard|writ|save|edit\b/i;

    // A row that cannot run does nothing at all; every other does one of the four.
    expect(new Set(verbs(every).filter((verb) => verb !== "nothing"))).toEqual(new Set(FILE_VERBS));
    expect([...FILE_VERBS]).toEqual([
      "copyPath",
      "revealPath",
      "openInEditor",
      "shellInFolder",
      "startChatHere",
      "addToChat",
    ]);
    for (const row of every) {
      expect(row.title).not.toMatch(writes);
      expect(row.id.split(":")[0]).not.toMatch(writes);
      expect(row.does.verb).not.toMatch(writes);
    }
    for (const kind of ["file", "folder", "link"] as const)
      expect(menuOn({ on: "file", at: at("a"), kind }).below).toEqual([]);
  });

  it("draws a file row's menu from its own rows, not from the catalogue", () => {
    const rows = menuRows({ on: "file", at: at("src"), kind: "folder" }, catalogued([]));

    expect(titles(rows.above)).toEqual(titles(folder));
    expect(rows.below).toEqual([]);
  });

  it("copies a link's relative path only, saying it follows no link", () => {
    expect(link.map((row) => [row.title, row.available])).toEqual([
      ["Copy relative path", true],
      ["Copy absolute path", false],
      [revealSaid(navigator.platform), false],
      ["Open in your editor", true],
      ["Start a chat here", false],
      ["Add to a chat's context", false],
    ]);
    expect(link[1].reason).toMatch(/follows no link/);
    expect(link[5].reason).toBe(NO_LINK_FOLLOWED);
  });

  it("keeps your editor on a file that does not open, with the tree's reason", () => {
    const ignored = fileRows({
      on: "file",
      at: at("target/out.log"),
      kind: "file",
      refused: "git ignores it",
    });

    expect(ignored.find((row) => row.title === "Open in your editor")).toMatchObject({
      available: false,
      reason: "git ignores it",
    });
    expect(ignored[0].available).toBe(true);
  });

  it("carries out each row with the branch and the path it was opened on", async () => {
    const hands = doing();

    for (const row of [...file, ...folder]) await perform(row, hands);

    expect(hands.calls).toEqual([
      "copyPath:svc/fix-it:src/lib.rs,false",
      "copyPath:svc/fix-it:src/lib.rs,true",
      "revealPath:svc/fix-it:src/lib.rs",
      "openInEditor:svc/fix-it:src/lib.rs,1",
      "startChatHere:svc/fix-it:src/lib.rs",
      "addToChat:svc/fix-it:src/lib.rs,false",
      "copyPath:svc/fix-it:src,false",
      "copyPath:svc/fix-it:src,true",
      "revealPath:svc/fix-it:src",
      "shellInFolder:svc/fix-it:src",
      "startChatHere:svc/fix-it:src",
      "addToChat:svc/fix-it:src,true",
    ]);
  });

  it("opens your editor at the line last read in the file's preview (#1143)", async () => {
    const hands = doing();
    const read = fileRows({ on: "file", at: at("src/lib.rs"), kind: "file", line: 42 });
    const editor = read.find((row) => row.does.verb === "openInEditor");

    expect(editor?.note).toBe("At line 42, where its preview was last read.");
    if (editor) await perform(editor, hands);

    expect(hands.calls).toEqual(["openInEditor:svc/fix-it:src/lib.rs,42"]);
  });

  it("opens your editor at line 1, saying nothing more, where the preview was not read", () => {
    for (const line of [undefined, 1]) {
      const editor = fileRows({ on: "file", at: at("src/lib.rs"), kind: "file", line }).find(
        (row) => row.does.verb === "openInEditor",
      );
      expect(editor?.does).toMatchObject({ line: 1 });
      expect(editor?.note).toBeUndefined();
    }
  });

  it("offers the branch's own folder no Add to a chat's context, as it offers no chat", () => {
    const own = fileRows({ on: "file", at: at(""), kind: "folder" });

    expect(verbs(own)).not.toContain("addToChat");
  });
});

describe("Restart chat on a chat's tab (#1428)", () => {
  const tabs = openTab(noTabs(), 7, "one");
  const tab = tabs.order[0];
  const row = (over: Partial<Now>) =>
    catalogued(catalogue(now({ tabs, ...over }))).get(`tab.restart:${tab}`);

  it("restarts the tab's chat, and says what it keeps and what it changes", async () => {
    const offers = catalogue(now({ tabs, restartable: () => true }));
    const offer = catalogued(offers).get(`tab.restart:${tab}`);

    expect(offer).toMatchObject({
      title: "Restart chat one",
      available: true,
      note: RESTART_NOTE,
    });
    const hands = doing();
    await run(offers, `tab.restart:${tab}`, hands);
    expect(hands.calls).toEqual([`restartChat:${tab}`]);
  });

  it("says a chat mid-turn restarts when the turn ends, read as the row is drawn", () => {
    const offer = row({ restartable: () => true });

    expect(offer?.midTurn).toEqual({
      session: 7,
      title: "Restart chat one when this turn ends",
    });
    expect(offer && titleOf(offer, true)).toBe("Restart chat one when this turn ends");
    expect(offer && titleOf(offer, false)).toBe("Restart chat one");
  });

  it("says the wait in its note too, so the palette and the menu agree", () => {
    // The palette shows a row's plain title (D-1428-7) and its note.
    expect(RESTART_NOTE).toBe(
      "It keeps its conversation and starts on this project's settings as they are now. Mid-turn, it restarts when the turn ends.",
    );
  });

  it("says a chat that reports no state restarts at once, and promises no wait for it", () => {
    const offer = row({ restartable: () => true });

    expect(offer?.noState).toEqual({ session: 7, note: restartNoteNoState("one") });
    expect(offer && noteOf(offer, false)).toBe(RESTART_NOTE);
    const unknown = offer && noteOf(offer, true);
    expect(unknown).toBe(
      "one reports no state, so purlis cannot tell whether it is mid-turn, and restarts it at once. It keeps its conversation and starts on this project's settings as they are now.",
    );
    expect(unknown).not.toMatch(/when the turn ends/);
  });

  it("has no row for a chat that cannot be restarted, a shell among them", () => {
    expect(row({ restartable: () => false })).toBeUndefined();
    expect(row({})).toBeUndefined();
  });

  it("is below the line in the tab's menu, above Start fresh and End chat", () => {
    expect(menuOn({ on: "chat", tab }).below).toEqual([
      `tab.restart:${tab}`,
      `tab.fresh:${tab}`,
      `tab.close:${tab}`,
    ]);
  });
});

describe("Ask a persona from a chat's tab", () => {
  const tabs = openTab(openTab(noTabs(), 7, "3", "steward"), 8, "sh");
  const [chat, shell] = tabs.order;
  const notAShell = (session: number) => session === 7;
  const open = { personas: ["devops", "steward"], locked: null };

  it("offers one row per persona on a chat's tab, naming the chat in its note", () => {
    const offers = catalogue(now({ tabs, ask: open, askable: notAShell }));
    const row = by(offers, askId(chat, "devops"));

    expect(row).toMatchObject({
      title: "Ask devops…",
      available: true,
      note: "From chat steward 3. A chat starts as devops, and its report comes back to this one.",
      does: { verb: "askPersona", tab: chat, persona: "devops" },
    });
    expect(askRows(chat, catalogued(offers)).map((offer) => offer.title)).toEqual([
      "Ask devops…",
      "Ask steward…",
    ]);
  });

  it("lists one row per persona in the palette, for the chat in front, and every tab's menu keeps its own (#1468)", () => {
    // Two chat tabs; the second is in front.
    const two = openTab(openTab(noTabs(), 7, "3", "steward"), 9, "5", "steward");
    const [first, second] = two.order;
    const offers = catalogue(now({ tabs: two, ask: open, askable: (s) => s === 7 || s === 9 }));

    const asks = inPalette(offers).filter((offer) => offer.title.startsWith("Ask "));
    expect(asks.map((offer) => [offer.title, offer.does])).toEqual([
      ["Ask devops…", { verb: "askPersona", tab: second, persona: "devops" }],
      ["Ask steward…", { verb: "askPersona", tab: second, persona: "steward" }],
    ]);
    // The tab behind still has its rows on its own menu.
    expect(askRows(first, catalogued(offers)).map((offer) => offer.title)).toEqual([
      "Ask devops…",
      "Ask steward…",
    ]);
    // A shell in front: no chat to ask from, so the palette has none.
    const shellInFront = catalogue(now({ tabs, ask: open, askable: notAShell }));
    expect(inPalette(shellInFront).filter((offer) => offer.title.startsWith("Ask "))).toEqual([]);
  });

  it("has no row on a shell, and none before the core has said who can be asked", () => {
    const offers = catalogued(catalogue(now({ tabs, ask: open, askable: notAShell })));

    expect(askRows(shell, offers)).toEqual([]);
    expect(askRows(chat, catalogued(catalogue(now({ tabs, askable: notAShell }))))).toEqual([]);
  });

  it("opens the dialog through the window's hands, and starts nothing by itself", async () => {
    const hands = doing();
    const offers = catalogue(now({ tabs, ask: open, askable: notAShell }));

    await run(offers, askId(chat, "devops"), hands);

    expect(hands.calls).toEqual([`askPersona:${chat},devops`]);
  });

  it("is absent from every tab where policy locks dispatch, and the palette says why", () => {
    const locked =
      "No chat is dispatched to a persona in this project. Locked by policy, set by root in /etc/purlis/policy.toml.";
    const offers = catalogue(now({ tabs, ask: { personas: [], locked }, askable: notAShell }));

    expect(askRows(chat, catalogued(offers))).toEqual([]);
    expect(ids(offers).filter((id) => id.startsWith("tab.ask:"))).toEqual([]);
    expect(by(offers, ASK_LOCKED_ID)).toMatchObject({
      title: "Ask a persona…",
      available: false,
      reason: locked,
    });
    // And no such row where nothing is locked: it exists to say why the others have gone.
    expect(
      by(catalogue(now({ tabs, ask: open, askable: notAShell })), ASK_LOCKED_ID),
    ).toBeUndefined();
  });

  it("takes a pair policy locks off that chat's tab, and its palette row says who locked it", () => {
    const why =
      "Policy forbids steward chats dispatching to devops. Locked by policy, set by the platform team in /etc/purlis/policy.json.";
    const offers = catalogue(
      now({
        tabs,
        ask: { ...open, locked_for: [{ session: 7, persona: "devops", why }] },
        askable: notAShell,
      }),
    );

    expect(askRows(chat, catalogued(offers)).map((offer) => offer.title)).toEqual(["Ask steward…"]);
    expect(by(offers, askId(chat, "devops"))).toMatchObject({
      title: "Ask devops…",
      available: false,
      reason: why,
    });
  });
});

/** A running chat as the Chats section lists it, started by `parent` where one started it. */
function listed(session: number, parent: number | null = null, tab = true): ListedChat {
  return {
    session,
    name: `chat ${session}`,
    persona: null,
    workspace: "alpha",
    shell: false,
    parent,
    mode: parent === null ? null : "task",
    from: parent === null ? null : `chat ${parent}`,
    tab,
    branch: null,
    report: null,
    outcome: null,
    asking: null,
    harness: null,
  };
}

/** `listed`, for a chat a handoff opened: it is stopped as any chat is, and is no task. */
function handed(session: number, parent: number, tab = true): ListedChat {
  return { ...listed(session, parent, tab), mode: "handoff" };
}

describe("stopping a chat (#1448)", () => {
  /** 1 handed work to 2, a session of its own, which asked for task 3. 3 has no tab. What is
   *  below a chat is the tasks it asked for (#1492), and a task is ended its own two ways and
   *  not stopped as a chat is (#1488). */
  const three = [listed(1), handed(2, 1), listed(3, 2, false)];

  it("offers Stop, and Stop with everything below it, on every running chat", () => {
    const offers = catalogue(now({ listed: three }));

    expect(by(offers, stopId(2))).toMatchObject({
      title: "Stop chat chat 2",
      available: true,
      note: STOPS_IT,
      does: { verb: "stopChat", session: 2, below: false },
    });
    expect(by(offers, stopBelowId(2))).toMatchObject({
      title: "Stop chat chat 2 and everything below it",
      available: true,
      does: { verb: "stopChat", session: 2, below: true },
    });
    // The task below it is not stopped by these rows: it has the two that end a task.
    expect(by(offers, stopId(3))).toBeUndefined();
    expect(taskEndIds(3).map((id) => by(offers, id)?.available)).toEqual([true, true]);
  });

  it("says why a chat with nothing below it cannot be stopped with what is below it", () => {
    // 1 handed its work off, which put nothing below it.
    const below = by(catalogue(now({ listed: three })), stopBelowId(1));

    expect(below).toMatchObject({
      available: false,
      reason: "chat 1 started no chat that is still running.",
      does: { verb: "nothing" },
    });
  });

  it("offers to end a chat that is already stopping, without waiting", () => {
    const rows = stopRows(three, [2]);

    expect(rows.find((row) => row.id === stopId(2))?.title).toBe("End chat chat 2 now");
    // The chat under it is still running: it can be stopped too, in the ordinary way.
    expect(rows.find((row) => row.id === stopBelowId(2))).toMatchObject({
      title: "Stop chat chat 2 and everything below it",
      available: true,
      does: { verb: "stopChat", session: 2, below: true },
    });
    // A chat stopping with nothing under it has only the one row.
    expect(stopRows(three, [3]).some((row) => row.id === stopBelowId(3))).toBe(false);
    expect(rows.find((row) => row.id === stopId(1))?.title).toBe("Stop chat chat 1");
  });

  it("counts no handoff as below the chat it came from", () => {
    // #1492, V100-69: 1 handed its work off to 2, which is a session of its own.
    const moved = [listed(1), { ...listed(2, 1), mode: "handoff" as const }];

    expect(stopRows(moved, [1]).some((row) => row.id === stopBelowId(1))).toBe(false);
    expect(by(catalogue(now({ listed: moved })), stopBelowId(1))).toMatchObject({
      available: false,
      reason: "chat 1 started no chat that is still running.",
    });
  });

  it("has a row to go to each chat a chat's work was handed off to, in that chat's menu, the newest first", () => {
    // #1492: the keyboard's way to "handed off to", and to the "and 2 more".
    const moved = [
      listed(1),
      { ...listed(2, 1), mode: "handoff" as const, name: "drop commons" },
      { ...listed(3, 1), mode: "handoff" as const, name: "release notes" },
      listed(4, 1),
      // From a chat that has closed: no row holds it.
      { ...listed(6, 9), mode: "handoff" as const },
    ];
    const offers = catalogue(now({ listed: moved }));

    expect(
      offers
        .filter((offer) => offer.id.startsWith("chat.handed:"))
        .map((offer) => [offer.id, offer.title, offer.available, offer.does]),
    ).toEqual([
      [
        handedOffId(1, 3),
        "Go to release notes (handed off to by chat 1)",
        true,
        { verb: "showChat", session: 3 },
      ],
      [
        handedOffId(1, 2),
        "Go to drop commons (handed off to by chat 1)",
        true,
        { verb: "showChat", session: 2 },
      ],
    ]);
    // The palette puts the name of the chat it goes to first.
    expect(by(offers, handedOffId(1, 3))?.name).toBe("release notes");
    // In the menu of the row it was handed off from, above the line, and in no other row's.
    const menu = menuRows({ on: "listed", session: 1, handed: [3, 2] }, catalogued(offers));
    expect(ids(menu.above)).toEqual([handedOffId(1, 3), handedOffId(1, 2)]);
    expect(ids(menuRows({ on: "listed", session: 2 }, catalogued(offers)).above)).toEqual([]);
    expect(handedOffRows([listed(1), listed(4, 1)])).toEqual([]);
  });

  it("asks first: the row opens the question and stops nothing by itself", () => {
    const hands = doing();
    const offers = catalogue(now({ listed: three }));

    void run(offers, stopBelowId(2), hands);

    expect(hands.calls).toEqual(["stopChat:2:true"]);
  });

  it("puts both rows under the line of a row's menu and of the chat's tab menu", () => {
    const offers = catalogued(catalogue(now({ listed: three })));

    const row = menuRows({ on: "listed", session: 2 }, offers);
    // And Stop all tasks beside them (#1498): 2 asked for task 3.
    expect(ids(row.below)).toEqual([stopId(2), stopBelowId(2), stopAllId(2)]);
    // Above the line, where a task is drawn (#1489): nothing there ends anything. 2 is a
    // session of its own, with none; 3 is a task.
    expect(ids(row.above)).toEqual([]);
    const task = menuRows({ on: "listed", session: 3 }, offers);
    expect(ids(task.above)).toEqual(expect.arrayContaining([ownTabId(3), besideId(3)]));
    expect(ids(task.below)).toEqual(taskEndIds(3));

    expect(menuOn({ on: "chat", tab: 7, session: 2 }).below.slice(-5, -2)).toEqual([
      stopId(2),
      stopBelowId(2),
      stopAllId(2),
    ]);
    // A tab that holds no chat has no chat to stop.
    expect(menuOn({ on: "chat", tab: 7 }).below).toEqual([
      "tab.restart:7",
      "tab.fresh:7",
      "tab.close:7",
    ]);
  });

  it("offers no stop for a chat that is not running", () => {
    expect(ids(catalogue(now())).filter((id) => id.startsWith("chat.stop"))).toEqual([]);
  });
});

describe("stopping all of a session's tasks (#1498)", () => {
  /** 1 handed work to 2, a session of its own, which asked for task 3. 4 asked for nothing. */
  const four = [listed(1), handed(2, 1), listed(3, 2, false), listed(4)];

  it("offers Stop all tasks to a session with a task open, and asks first", () => {
    const offers = catalogue(now({ listed: four }));

    expect(by(offers, stopAllId(2))).toMatchObject({
      title: "Stop all tasks of chat 2",
      available: true,
      note: STOPS_ALL,
      does: { verb: "stopAllTasks", session: 2 },
    });
    const hands = doing();
    void run(offers, stopAllId(2), hands);
    expect(hands.calls).toEqual(["stopAllTasks:2"]);
  });

  it("says why a chat with no task open has none to stop, and a handoff is no task of it", () => {
    const offers = catalogue(now({ listed: four }));

    expect(by(offers, stopAllId(4))).toMatchObject({
      available: false,
      reason: "chat 4 has no task open.",
    });
    // 1 handed its work to 2: that is no task of 1's.
    expect(by(offers, stopAllId(1))?.available).toBe(false);
    // A task is ended its own two ways, and has no Stop all tasks row.
    expect(by(offers, stopAllId(3))).toBeUndefined();
  });

  it("is still offered while the session itself is being stopped alone", () => {
    const rows = stopRows(four, [2]);

    expect(rows.find((row) => row.id === stopAllId(2))).toMatchObject({
      available: true,
      does: { verb: "stopAllTasks", session: 2 },
    });
  });
});

describe("ending a task by hand (#1488)", () => {
  /** 1 dispatched task 2, which dispatched task 3. 3 has no tab. */
  const tasks = [listed(1), listed(2, 1), listed(3, 2, false)];

  it("offers a task its own two rows, and none of a chat's Stop rows", () => {
    const offers = catalogue(now({ listed: tasks }));

    expect(by(offers, taskStopId(2))).toMatchObject({
      title: "Stop and get its report: task chat 2",
      available: true,
      note: STOPS_THE_TASK,
      does: { verb: "endTask", session: 2, way: "report" },
    });
    expect(by(offers, taskCloseId(2))).toMatchObject({
      title: "Close now: task chat 2",
      available: true,
      note: CLOSES_THE_TASK,
      does: { verb: "endTask", session: 2, way: "now" },
    });
    // A task with no tab has them too, so the palette finds them.
    expect(by(offers, taskStopId(3))?.available).toBe(true);
    // A task is not stopped as a chat is: one vocabulary for ending it.
    expect(by(offers, stopId(2))).toBeUndefined();
    expect(by(offers, stopBelowId(2))).toBeUndefined();
    // And the session that asked is not a task: it keeps its Stop rows and has neither of these.
    expect(by(offers, stopId(1))?.title).toBe("Stop chat chat 1");
    expect(by(offers, taskStopId(1))).toBeUndefined();
    expect(by(offers, taskCloseId(1))).toBeUndefined();
  });

  it("says no word of a Smart close or of closing a tab on either row", () => {
    for (const row of taskEndRows(tasks, [])) {
      const said = `${row.title} ${row.note ?? ""}`;
      expect(said).not.toMatch(/smart close|close tab|end chat/i);
    }
  });

  it("does not stop a task twice: one being stopped is only closed now", () => {
    const rows = taskEndRows(tasks, [2]);

    expect(rows.find((row) => row.id === taskStopId(2))).toMatchObject({
      available: false,
      reason:
        "chat 2 is being stopped already, and has one short turn to say what it did. Close now ends it without waiting.",
    });
    expect(rows.find((row) => row.id === taskCloseId(2))?.available).toBe(true);
    expect(rows.find((row) => row.id === taskStopId(3))?.available).toBe(true);
  });

  it("does not offer Stop on a harness purlis types nothing into, and offers Close now", () => {
    // V100-71: there is no turn to give it. Said on the row, which cannot run.
    const unheard: ListedChat = { ...listed(3, 2, false), typed: false, harness: "opencode" };
    const rows = taskEndRows([listed(1), listed(2, 1), unheard], []);

    expect(rows.find((row) => row.id === taskStopId(3))).toMatchObject({
      available: false,
      reason:
        "purlis does not type into opencode, so it cannot ask chat 3 for a report. Close now ends it.",
    });
    expect(rows.find((row) => row.id === taskCloseId(3))?.available).toBe(true);
    // A harness it does type into, and a chat nothing was read of: offered.
    expect(rows.find((row) => row.id === taskStopId(2))?.available).toBe(true);
  });

  it("closes a task's own tab and pane by sending it back to the list, ending nothing", () => {
    // A tab holding task 2 and nothing else, and a tab holding the session.
    const tabs = openTab(openTab(noTabs(), 1, "chat 1", null, null), 2, "chat 2", null, null);
    const offers = catalogue(now({ tabs, listed: tasks }));
    const [session, task] = tabs.order;

    // The one row a task's tab has for it (#1489): it sends the task back, by the one command
    // (`close_chat_tab`), and is no close at all.
    expect(by(offers, `tab.close:${task}`)).toMatchObject({
      available: true,
      // It ends nothing, so nothing is asked and it is not drawn as a row that ends a chat.
      does: { verb: "sendBack", session: 2 },
    });
    expect(by(offers, `tab.close:${task}`)?.title).toMatch(/^Send .*2 back/);
    expect(by(offers, `tab.close:${session}`)).toMatchObject({
      title: "End chat chat 1",
      does: { verb: "closeTab", tab: session, ends: true },
    });
    // The pane's close of the tab in front (the task's) says the same.
    expect(by(offers, "pane.close")).toMatchObject({
      does: { verb: "sendBack", session: 2 },
    });
  });

  it("asks first: a row hands the window the task and the way, and ends nothing by itself", () => {
    const hands = doing();
    const offers = catalogue(now({ listed: tasks }));

    void run(offers, taskStopId(3), hands);
    void run(offers, taskCloseId(2), hands);

    expect(hands.calls).toEqual(["endTask:3:report", "endTask:2:now"]);
  });

  it("puts both under the line of the task's row menu, and is what a tab's menu mounts", () => {
    const offers = catalogued(catalogue(now({ listed: tasks })));

    const row = menuRows({ on: "listed", session: 2 }, offers);
    expect(ids(row.below)).toEqual([taskStopId(2), taskCloseId(2)]);
    expect(taskEndIds(2)).toEqual([taskStopId(2), taskCloseId(2)]);
    // A task that has a tab of its own: the same two, after the tab's own rows.
    expect(menuOn({ on: "chat", tab: 7, session: 2 }).below.slice(-2)).toEqual(taskEndIds(2));
    const tab = menuRows({ on: "chat", tab: 7, session: 2 }, offers);
    expect(ids(tab.below).slice(-2)).toEqual(taskEndIds(2));
  });
});

describe("a chat in the queue the app found a reason for (#1448)", () => {
  const nameOf = (session: number) => `chat ${session}`;

  it("says why on its row", () => {
    const [show] = needsYouRows(
      [3],
      nameOf,
      noTabs(),
      () => [],
      () => [],
      () => ["its report has nowhere to go because chat 1 has closed or its program has ended"],
      () => true,
    );

    expect(show.title).toBe(
      "Show chat 3: its report has nowhere to go because chat 1 has closed or its program has ended",
    );
  });

  it("can show a listed chat that has no tab yet, and not one the project does not list", () => {
    const show = (isListed: boolean) =>
      needsYouRows(
        [3],
        nameOf,
        noTabs(),
        () => [],
        () => [],
        () => [],
        () => isListed,
      )[0];

    expect(show(true)).toMatchObject({ available: true, does: { verb: "showChat", session: 3 } });
    expect(show(false)).toMatchObject({
      available: false,
      reason: "That chat has no tab in this window.",
    });
  });
});

describe("what a queued chat's row says first (#1448)", () => {
  const nameOf = (session: number) => `chat ${session}`;
  const title = (
    reported: string[],
    needed: string[],
    stoppedBelow: string[] = [],
    refused: string[] = [],
  ) =>
    needsYouRows(
      [3],
      nameOf,
      noTabs(),
      () => reported,
      () => refused,
      () => needed,
      () => true,
      () => stoppedBelow,
    )[0].title;

  it("says why the chat itself needs you before what the chats it started did", () => {
    expect(
      title(
        ["chat 5"],
        ["its report has nowhere to go because chat 1 has closed or its program has ended"],
      ),
    ).toBe(
      "Show chat 3: its report has nowhere to go because chat 1 has closed or its program has ended",
    );
  });

  it("says a chat it started was stopped, in purlis's words and not as a report", () => {
    expect(title([], [], ["chat 5"])).toBe("Show chat 3: chat 5 was stopped");
    expect(title(["chat 4"], [], ["chat 5"])).toBe(
      "Show chat 3: chat 4 reported back; chat 5 was stopped",
    );
  });

  it("says a refused commit only when nothing above applies", () => {
    expect(title([], [], [], ["a key in config.env"])).toBe("Show chat 3: a key in config.env");
    expect(title(["chat 4"], [], [], ["a key in config.env"])).toBe(
      "Show chat 3: chat 4 reported back",
    );
  });
});

describe("the chats inside the tab in front (#1487)", () => {
  /** Chat 1 has a tab and asked for 2 and 3; 2 asked for 4. Chat 9 has a tab and no tasks. */
  const chats = [
    listed(1),
    listed(2, 1, false),
    listed(3, 1, false),
    listed(4, 2, false),
    listed(9),
  ];
  const askedBy = (session: number) =>
    chats.find((chat) => chat.session === session)?.parent ?? undefined;
  /** Tabs for 1 and 9, with 1's in front. */
  const two = () => selectTab(openTab(openTab(noTabs(), 1), 9), 1);
  const rows = (tabs: Tabs) =>
    catalogue(now({ tabs, listed: chats, nameOf: (session) => `chat ${session}` }));
  const frontTab = (tabs: Tabs) => tabs.inFront as number;

  it("offers the tab's task menu, and says its key", () => {
    const tabs = two();

    expect(by(rows(tabs), "tasks.menu")).toMatchObject({
      title: "Show this tab's tasks",
      available: true,
      does: { verb: "showTabTasks", tab: frontTab(tabs) },
    });
    expect(by(rows(tabs), "tasks.menu")?.note).toContain("Ctrl+Shift+J");

    const hands = doing();
    void run(rows(tabs), "tasks.menu", hands);
    expect(hands.calls).toEqual([`showTabTasks:${frontTab(tabs)}`]);
  });

  it("goes to the next and the previous chat in the menu's order, round its ends", () => {
    // The menu's order: 1, then 2, then 4 under 2, then 3.
    const own = two();
    expect(by(rows(own), "tasks.next")).toMatchObject({
      available: true,
      does: { verb: "showChat", session: 2 },
      // The key, and that a keyboard whose ] needs AltGr has it where a US keyboard does.
      note: "Ctrl+Shift+]. Where ] needs AltGr, it is the key in its place on a US keyboard.",
    });
    expect(by(rows(own), "tasks.previous")).toMatchObject({
      available: true,
      does: { verb: "showChat", session: 3 },
      note: "Ctrl+Shift+[. Where [ needs AltGr, it is the key in its place on a US keyboard.",
    });

    const onFour = switchTabTo(own, 4, askedBy);
    expect(by(rows(onFour), "tasks.next")?.does).toEqual({
      verb: "showChat",
      session: 3,
      inside: true,
    });
    expect(by(rows(onFour), "tasks.previous")?.does).toEqual({
      verb: "showChat",
      session: 2,
      inside: true,
    });

    const onThree = switchTabTo(own, 3, askedBy);
    expect(by(rows(onThree), "tasks.next")?.does).toEqual({
      verb: "showChat",
      session: 1,
      inside: true,
    });
  });

  it("goes back to the session's own chat only while the tab shows a task", () => {
    const own = two();
    expect(by(rows(own), "tasks.own")).toMatchObject({
      available: false,
      reason: "This tab is showing its own chat.",
    });

    const onTask = switchTabTo(own, 4, askedBy);
    const back = by(rows(onTask), "tasks.own");
    expect(back).toMatchObject({
      title: "Back to this tab's own chat",
      available: true,
      does: { verb: "showChat", session: 1 },
    });
    expect(back?.note).toContain("Shows chat 1 again.");
    expect(back?.note).toContain("Ctrl+Shift+H");
  });

  it("lists all four on a tab with no tasks, unable to run and saying why", () => {
    const tabs = selectTab(two(), 2);
    const offers = rows(tabs);
    // Named as its tab is named.
    const why = `${tabs.byId[2].name} has no tasks.`;

    for (const id of ["tasks.menu", "tasks.next", "tasks.previous"])
      expect(by(offers, id)).toMatchObject({ available: false, reason: why });
    expect(by(offers, "tasks.own")?.available).toBe(false);
  });

  it("offers the menu of a tab whose tasks have all finished, and no next chat to go to", () => {
    const tabs = selectTab(two(), 2);
    const offers = catalogue(
      now({
        tabs,
        listed: chats,
        nameOf: String,
        finished: new Map([
          [
            9,
            [
              {
                chat: null,
                did_not_start: false,
                attempts: 0,
                waits: null,
                id: "01K6",
                asker: 9,
                name: "old",
                persona: null,
                how: "done",
                outcome: "done",
                folds: true,
                report: "",
                changed: null,
                ended: null,
                place: "alpha",
                branch: null,
                reopens: false,
                not_reopened: null,
              },
            ],
          ],
        ]),
      }),
    );

    expect(by(offers, "tasks.menu")).toMatchObject({
      available: true,
      does: { verb: "showTabTasks", tab: 2 },
    });
    expect(by(offers, "tasks.next")).toMatchObject({
      available: false,
      reason: `${tabs.byId[2].name} is the only chat in this tab.`,
    });
  });

  it("lists all four with no chat in front, and says so", () => {
    const offers = catalogue(now());

    for (const id of ["tasks.menu", "tasks.next", "tasks.previous", "tasks.own"])
      expect(by(offers, id)).toMatchObject({ available: false, reason: "No chat is in front." });
  });

  it("goes from a task that is listed no more to the ends of what is", () => {
    // The tab still shows 4, which has ended and is gone from the list.
    const onFour = switchTabTo(two(), 4, askedBy);
    const without = chats.filter((chat) => chat.session !== 4);
    const offers = catalogue(now({ tabs: onFour, listed: without, nameOf: String }));

    expect(by(offers, "tasks.menu")?.available).toBe(true);
    expect(by(offers, "tasks.next")?.does).toEqual({ verb: "showChat", session: 1, inside: true });
    expect(by(offers, "tasks.previous")?.does).toEqual({
      verb: "showChat",
      session: 3,
      inside: true,
    });
    expect(by(offers, "tasks.own")?.does).toEqual({ verb: "showChat", session: 1 });
  });
});

describe("Answer, for a task that asks its asking chat a question (#1551)", () => {
  /** 1 is a chat a person opened; it asked for 2, which asks it something, and 3, which asks
   *  nothing. */
  const three = [listed(1), { ...listed(2, 1), asking: "chat 1" }, listed(3, 1, false)];

  it("has a row only for a task paused on a question, named for it", () => {
    const rows = answerRows(three);

    expect(rows.map((row) => [row.id, row.title, row.available])).toEqual([
      [answerId(2), "Answer chat 2's question", true],
    ]);
    expect(rows[0]?.note).toBe(ANSWER_SAYS);
    expect(answerId(2)).toBe("chat.answer:2");
    // In the catalogue, so the palette finds it too; gone once nothing is asked.
    expect(
      ids(catalogue(now({ listed: three }))).filter((id) => id.startsWith("chat.answer:")),
    ).toEqual([answerId(2)]);
    const answered = three.map((chat) => ({ ...chat, asking: null }));
    expect(
      ids(catalogue(now({ listed: answered }))).filter((id) => id.startsWith("chat.answer:")),
    ).toEqual([]);
  });

  it("is an ordinary row of the task's row menu and of its own tab's menu, never under the line", () => {
    const offers = catalogued(catalogue(now({ listed: three })));

    const task = menuRows({ on: "listed", session: 2 }, offers);
    expect(ids(task.above)).toContain(answerId(2));
    expect(ids(task.below)).not.toContain(answerId(2));
    expect(ids(menuRows({ on: "listed", session: 3 }, offers).above)).not.toContain(answerId(3));
    expect(menuOn({ on: "chat", tab: 7, session: 2 }).above).toContain(answerId(2));
  });

  it("opens the form through the window's hands, and sends nothing itself", () => {
    const hands = doing();

    void run(catalogue(now({ listed: three })), answerId(2), hands);

    expect(hands.calls).toEqual(["answerQuestion:2"]);
  });
});

describe("Brief, for a task (#1494)", () => {
  /** 1 is a chat a person opened; it asked for 2, which asked for 3. */
  const three = [listed(1), listed(2, 1), listed(3, 2, false)];

  it("has a row for each task, named for it, and none for a chat nobody sent a brief", () => {
    const rows = briefRows(three);

    expect(rows.map((row) => [row.id, row.title, row.available])).toEqual([
      [briefId(2), "Brief of chat 2", true],
      [briefId(3), "Brief of chat 3", true],
    ]);
    expect(rows.every((row) => row.note === BRIEF_SAYS)).toBe(true);
    expect(briefId(2)).toBe("chat.brief:2");
    // In the catalogue, so the palette finds it too.
    const offers = catalogue(now({ listed: three }));
    expect(ids(offers).filter((id) => id.startsWith("chat.brief:"))).toEqual([
      briefId(2),
      briefId(3),
    ]);
    expect(ids(catalogue(now())).filter((id) => id.startsWith("chat.brief:"))).toEqual([]);
  });

  it("is an ordinary row of the task's row menu and of its own tab's menu, never under the line", () => {
    const offers = catalogued(catalogue(now({ listed: three })));

    const task = menuRows({ on: "listed", session: 3 }, offers);
    expect(ids(task.above)).toContain(briefId(3));
    expect(ids(task.below)).not.toContain(briefId(3));
    // A chat a person opened has no Brief on its row or its tab.
    expect(ids(menuRows({ on: "listed", session: 1 }, offers).above)).toEqual([]);
    expect(menuOn({ on: "chat", tab: 7, session: 2 }).above).toContain(briefId(2));
    expect(menuOn({ on: "chat", tab: 7 }).above.some((id) => id.startsWith("chat.brief:"))).toBe(
      false,
    );
  });

  it("opens the panel through the window's hands, and does nothing else", () => {
    const hands = doing();
    const offers = catalogue(now({ listed: three }));

    void run(offers, briefId(3), hands);

    expect(hands.calls).toEqual(["showBrief:3"]);
  });
});

describe("Search in files in the palette (#1137, the Search view's row since #1676)", () => {
  it("shows the Search view, searching as narrow as the focus, with the key said on the row", async () => {
    const branch = { workspace: "alpha", repo: "svc", piece: "fix-it" };
    const offers = catalogue(now({ workspaces: ["alpha"], focused: "alpha", plane: "/p", branch }));
    const row = by(offers, SEARCH_ID);
    const hands = doing();

    await run(offers, SEARCH_ID, hands);

    expect(row?.title).toBe("Search in files");
    expect(row?.note).toBe(`Every file of branch fix-it, in the Search view. ${SEARCH_KEY_SAID}.`);
    // The window searches where it is focused when it shows the view (#1676).
    expect(hands.calls).toEqual(["showSideView:search"]);
  });

  it("searches the workspace in front with no branch picked, and the project at its root", () => {
    const inAlpha = by(catalogue(now({ focused: "alpha", plane: "/p" })), SEARCH_ID);
    const atRoot = by(catalogue(now({ focused: OUTSIDE, plane: "/p" })), SEARCH_ID);

    expect(inAlpha?.note).toContain("Every file of workspace alpha");
    expect(atRoot?.note).toContain("Every file of this project");
  });

  it("cannot search with no project open, and says so", () => {
    const row = by(catalogue(now()), SEARCH_ID);

    expect(row?.available).toBe(false);
    expect(row?.reason).toBe("No project is open, so there are no files to search.");
  });
});

describe("a harness's card with no chat open (#1134)", () => {
  const codex = {
    name: "codex",
    title: "Codex",
    label: "What Codex can do here",
    lines: [],
    cannot_type: null,
  };

  it("offers a row per harness that opens its card tab", async () => {
    const offers = catalogue(now({ harnesses: [codex] }));
    const row = by(offers, harnessCardId("codex"));
    const hands = doing();

    await run(offers, harnessCardId("codex"), hands);

    expect(row?.title).toBe("What Codex can do here");
    expect(row?.name).toBe("Codex");
    expect(hands.calls).toEqual(["openView:charter/harness/codex,What Codex can do here"]);
  });

  it("offers none where the project's harnesses are not known", () => {
    expect(ids(catalogue(now())).some((id) => id.startsWith("harness.card:"))).toBe(false);
  });
});
