import { describe, expect, it, vi } from "vitest";
import {
  aim,
  catalogue,
  ENDS_IT,
  KEEPS_THE_BRANCH,
  matches,
  menuOn,
  menuRows,
  narrow,
  OUTSIDE,
  perform,
  type Cut,
  type Doing,
  type Now,
  type Offer,
} from "./actions";
import {
  noTabs,
  openTab,
  openView,
  selectTab,
  splitFocusedPane,
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
    split: note("split"),
    closePane: note("closePane"),
    closeTab: note("closeTab"),
    selectTab: note("selectTab"),
    renameTab: note("renameTab"),
    focusWorkspace: note("focusWorkspace"),
    createWorkspace: note("createWorkspace"),
    removeWorkspace: note("removeWorkspace"),
    showChat: note("showChat"),
    pickVault: note("pickVault"),
    createVault: note("createVault"),
    ignoreNeedsYou: vi.fn(async (session: number) => {
      calls.push(`ignoreNeedsYou:${session}`);
      return { ok: true as const };
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
    sendKey: vi.fn(async (key: string) => {
      calls.push(`sendKey:${key}`);
      return { ok: true as const };
    }),
    openProject: note("openProject"),
    createProject: note("createProject"),
    showExtensions: note("showExtensions"),
    installCli: vi.fn(async () => {
      calls.push("installCli");
      return { ok: true as const, said: "On PATH" };
    }),
    selectProject: note("selectProject"),
    closeProject: vi.fn(async (plane: string) => {
      calls.push(`closeProject:${plane}`);
      return { ok: true as const };
    }),
    openSettings: vi.fn((plane: string) => {
      calls.push(`openSettings:${plane}`);
    }),
    openPreferences: note("openPreferences"),
    quit: note("quit"),
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

  it("gives the strip for chats outside every workspace words of its own", () => {
    // Its name is a sentinel that cannot be a directory, so the row cannot be built the way
    // the others are — and `Focus workspace outside/every/workspace` is not a sentence.
    const offers = catalogue(now({ workspaces: ["alpha", OUTSIDE] }));

    expect(by(offers, `workspace.focus:${OUTSIDE}`)?.title).toBe(
      "Focus the chats outside every workspace",
    );
    expect(by(offers, `workspace.focus:${OUTSIDE}`)?.name).toBeUndefined();
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

  it("offers Preferences everywhere, with no project open too, because it is the machine's", async () => {
    // charter-app#283: the text sizes are this machine's, so the row does not wait for a plane.
    for (const offers of [catalogue(now()), catalogue(now({ plane: "/p/one" }))]) {
      const hands = doing();
      const row = by(offers, "preferences.show");
      expect(row?.title).toBe("Preferences…");
      expect(row?.available).toBe(true);
      await run(offers, "preferences.show", hands);
      expect(hands.calls).toEqual(["openPreferences"]);
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
      "The chat in front is not working in a worktree charter cut.",
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

  it("names one merge and one remove per piece of the focused workspace", () => {
    // charter-app#174: the explorer's rows had no menu because the only worktree rows there
    // were were about THE CHAT IN FRONT, and a piece nobody is running in is not in front of
    // anything. These name their piece, which is what makes a row about one possible at all.
    const cut = { workspace: "alpha", repo: "svc", piece: "fix-it" };
    const offers = catalogue(now({ plane: "/plane", pieces: [cut] }));

    expect(by(offers, "worktree.merge:svc/fix-it")?.title).toBe("Merge worktree fix-it into svc");
    expect(by(offers, "worktree.remove:svc/fix-it")?.title).toBe("Remove worktree fix-it in svc");
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
      "charter found no plane, so it cannot reach a worktree.",
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

  it("says why a vault cannot be opened on a plane that has none, and still offers to make one", () => {
    const offers = catalogue(now({ plane: "/plane", vaults: [] }));
    expect(by(offers, "vault.pick")?.available).toBe(false);
    expect(by(offers, "vault.pick")?.reason).toMatch(/no vaults/);
    expect(by(offers, "vault.create")?.available).toBe(true);

    const nowhere = catalogue(now());
    expect(by(nowhere, "vault.create")?.available).toBe(false);
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
        needsYou: [8],
        nameOf: (s) => String(s),
      }),
    );

    for (const offer of offers) await perform(offer, hands);

    expect(new Set(hands.calls)).toEqual(
      new Set([
        "newChat",
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
        "openView:charter/persona/steward,steward",
        "openView:charter/vault/ops,ops",
        "pickVault",
        "createVault",
        "sendKey:F2",
        "openProject",
        "createProject",
        "showExtensions",
        "openPreferences",
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
        "openSettings:/plane",
        "openSettings:/other",
        "quit",
      ]),
    );
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

  const loaded = () =>
    catalogue(
      now({
        tabs: fiftyChats(),
        workspaces: WORKSPACES,
        focused: "ide",
        plane: "/plane",
        worktree: PIECE,
        pieces: fiftyPieces(),
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

    const verbs = rows.slice(0, 10).map((row) => row.title);
    expect(verbs).toEqual([
      "New workspace…",
      "New project…",
      // `vault.create` is charter's `create` too (charter-app#235), and `preferences` is
      // charter's word (charter-app#283); both join the rows that make things.
      "Preferences…",
      "New vault…",
      // **Both of the chat in front's rows, then the pieces'.** `aboutWhatIsInFront` is the
      // second rule inside this group (charter-app#174): a row with no name in its id acts on
      // what the operator is looking at, and fifty rows about other worktrees do not get to
      // stand in front of it. Inside each half the catalogue's own order stands.
      "Merge this chat's worktree into its clone",
      "Remove this chat's worktree",
      // Then the rows about things that are not in front, in the catalogue's order.
      // `ignore` has `re` in it, and it is charter's word, so the two queued chats' Ignore
      // rows (charter-app#248) are verbs here too, and still behind every row about what is
      // in front. Then a rename per chat (charter-app#254), before the pieces, as the tab
      // rows always have.
      "Ignore chat 103 until it asks again",
      "Ignore chat 107 until it asks again",
      "Rename chat ide.1…",
      "Rename chat charter.2…",
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
      narrow(typed, offers).findIndex((row) => row.title === "Remove this chat's worktree") + 1;

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
      // and `Preferences…` (charter-app#283) are both rows that make/land near the creates.
      expect(at("re", loaded())).toBe(6);
      expect(at("r", loaded())).toBe(9);
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
      const rows = narrow("Remove worktree piece-3 in repo-7", loaded());

      expect(rows[0].id).toBe("worktree.remove:repo-7/piece-3");
    });
  });

  it("does not reorder anything when every row matched charter's own word", () => {
    // `switch` is charter's word on fifty rows and nobody's name. The rule must be a
    // partition and not a score: rows that all match the same way keep the catalogue's order.
    const offers = loaded();
    const selects = offers.filter((row) => row.id.startsWith("tab.select:"));

    expect(narrow("switch", offers)).toEqual(selects);
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
    expect(offers.filter((row) => row.id.startsWith("persona.show:"))).toHaveLength(8);
    // 347 rows: 50 chats four times over, 6 workspaces THREE times, 50 pieces TWICE, 8
    // personas, 2 in the queue TWICE (show it, and ignore it — charter-app#248), and the
    // verbs — with Preferences (charter-app#283) and the vault picker and New vault…
    // (charter-app#235; this plane has no vaults, so no `vault.open:` rows). It was 118 before the pins, 174 before
    // the extension list (ADR 0041), 175 before a workspace could be made and deleted
    // from the window, 183 before the explorer's rows had anything to offer, 291 before
    // the row that puts `charter` on a terminal's PATH, 292 before a queued chat could be
    // ignored, and 294 before a chat could be renamed (charter-app#254). What the
    // hundred buys is the surface the operator asked for and the menu system could not reach;
    // what it costs is measured on `narrow` two tests up and on `menuRows` below.
    expect(offers).toHaveLength(347);
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
   * below is the work itself, which is the standard #133 set: 200 lookups (150 before a chat
   * could be renamed, charter-app#254), and the same 200 whichever catalogue it is.
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

    it("asks for four rows per tab and never walks the list", () => {
      const offers = new Counting(loaded().map((offer) => [offer.id, offer]));

      // 50 tabs × the four ids a chat menu lists. **Not fifty scans of 291 rows**, which is
      // what this cost before the lookup was built once for the window — and the number that
      // does not move when the catalogue grows again.
      expect(strip(offers)).toBe(200);
    });

    it("is the same 200 whether the catalogue carries the pieces or not", () => {
      // The property, not the timing: the cost of a menu is flat in the length of the list it
      // reads. A scan is not, which is why #174's hundred rows needed this first.
      const small = new Counting(
        catalogue(now({ tabs: fiftyChats(), workspaces: WORKSPACES, focused: "ide" })).map(
          (offer) => [offer.id, offer],
        ),
      );

      expect(strip(small)).toBe(200);
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
