import { describe, expect, it, vi } from "vitest";
import {
  aim,
  catalogue,
  matches,
  narrow,
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
