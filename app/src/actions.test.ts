import { describe, expect, it, vi } from "vitest";
import {
  aim,
  catalogue,
  ENDS_IT,
  matches,
  narrow,
  OUTSIDE,
  perform,
  type Doing,
  type Now,
  type Offer,
} from "./actions";
import { noTabs, openTab, selectTab, splitFocusedPane, type Tabs } from "./tabs";

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
    focusWorkspace: note("focusWorkspace"),
    showChat: note("showChat"),
    removeWorktree: vi.fn(async (force: boolean) => {
      calls.push(`removeWorktree:${force}`);
      return { ok: true as const };
    }),
    mergeWorktree: vi.fn(async () => {
      calls.push("mergeWorktree");
      return { ok: true as const };
    }),
    sendKey: vi.fn(async (key: string) => {
      calls.push(`sendKey:${key}`);
      return { ok: true as const };
    }),
    openProject: note("openProject"),
    selectProject: note("selectProject"),
    closeProject: vi.fn(async (plane: string) => {
      calls.push(`closeProject:${plane}`);
      return { ok: true as const };
    }),
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

    expect(by(offers, "tab.close:1")?.title).toBe("End chat 3 steward");
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

    expect(hands.calls).toEqual(["removeWorktree:false"]);
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
      now({ tabs, plane: "/plane", worktree: PIECE, refusal: "svc/fix-it has changes" }),
    );

    await run(offers, "worktree.discard", hands);

    expect(hands.calls).toEqual(["removeWorktree:true"]);
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
        refusal: "something was in the way",
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
        "selectTab:2",
        "focusWorkspace:beta",
        "closePane",
        "closeTab:1",
        "closeTab:2",
        "removeWorktree:false",
        "removeWorktree:true",
        "mergeWorktree",
        "sendKey:F2",
        "openProject",
        "selectProject:/other",
        "closeProject:/plane",
        "closeProject:/other",
        "quit",
      ]),
    );
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
 */
describe("the palette at fifty chats", () => {
  const WORKSPACES = ["ide", "charter", "release", "statusline", "forge", "reddit"];

  /** Fifty chats named the way a plane names them: the workspace, then the chat. */
  function fiftyChats(): Tabs {
    let tabs = noTabs();
    for (let i = 0; i < 50; i++) {
      tabs = openTab(tabs, 100 + i, `${WORKSPACES[i % WORKSPACES.length]}.${i + 1}`);
    }
    return tabs;
  }

  const loaded = () =>
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

  it("puts the verb ahead of every name that merely shares its letters", () => {
    // `re` is in `release`, in `reddit` and in `worktree`. Only one of those is a word
    // charter chose; the rest are somebody's chat names.
    const rows = narrow("re", loaded());

    expect(rows.slice(0, 2).map((row) => row.title)).toEqual([
      "Merge this chat's worktree into its clone",
      "Remove this chat's worktree",
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

    expect(rows[aim(rows)].title).toBe("Merge this chat's worktree into its clone");
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
    // 118 rows: 50 chats twice over, 6 workspaces, 2 in the queue, and the ten verbs.
    expect(offers).toHaveLength(118);
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
