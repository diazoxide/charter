import { describe, expect, it } from "vitest";
import {
  backId,
  BESIDE_ID,
  besideId,
  catalogue,
  catalogued,
  menuRows,
  ownTabId,
  paneCloseOf,
  perform,
  TASK_NOT_FRESH,
  taskRows,
  taskShowId,
  type Doing,
} from "./actions";
import type { ListedChat } from "./chatsTree";
import { askedByOf } from "./tabChats";
import {
  moveToOwnTab,
  noTabs,
  openBeside,
  openTab,
  panesOf,
  selectTab,
  switchTabTo,
  type Tabs,
} from "./tabs";

const listed = (session: number, more: Partial<ListedChat> = {}): ListedChat => ({
  session,
  name: `devops ${session}`,
  persona: "devops",
  workspace: "beta",
  shell: false,
  parent: 1,
  mode: "task",
  from: "steward 1",
  tab: false,
  branch: null,
  report: null,
  outcome: null,
  asking: null,
  harness: "claude",
  ...more,
});

describe("the palette's rows for tasks (#1499, V100-49)", () => {
  it("has a row for each task, which shows it, and says where it works and who asked", () => {
    const rows = taskRows([listed(2), listed(3, { name: "live check talk" })]);

    expect(rows.map((row) => [row.id, row.title, row.available, row.does])).toEqual([
      [
        taskShowId(2),
        "Show task devops 2 in beta, asked by steward 1",
        true,
        { verb: "showChat", session: 2 },
      ],
      [
        taskShowId(3),
        "Show task live check talk in beta, asked by steward 1",
        true,
        { verb: "showChat", session: 3 },
      ],
    ]);
    // The name is the row's own, so the palette puts a name that was typed first.
    expect(rows[1].name).toBe("live check talk");
  });

  it("has one for a task that is on screen too, and none for a chat that is not a task", () => {
    // A task shown in its session's tab has no tab row of its own to be found by (#1486).
    expect(taskRows([listed(2, { tab: true })])).toHaveLength(1);
    expect(taskRows([listed(4, { mode: "handoff" }), listed(5, { mode: null })])).toEqual([]);
  });
});

/** Session 1 and session 9 in tabs of their own; the tasks of `listed` are session 1's. */
const sessions = () =>
  selectTab(openTab(openTab(noTabs(), 1, "1", "steward"), 9, "9", "steward"), 1);
const names: Record<number, string> = {
  1: "steward 1",
  9: "steward 9",
  2: "talk",
  3: "sweep",
  12: "left behind",
};
const tasks = [listed(2, { name: "talk" }), listed(3, { name: "sweep" })];
const session = (number: number): ListedChat =>
  listed(number, { name: names[number], parent: null, mode: null, from: null, tab: true });
const all = [session(1), session(9), ...tasks];
const rowsOf = (tabs: Tabs, chats: readonly ListedChat[] = all) =>
  catalogued(
    catalogue({
      tabs,
      workspaces: [],
      needsYou: [],
      nameOf: (number) => names[number] ?? String(number),
      listed: chats,
    }),
  );
const tabOf = (tabs: Tabs, number: number) =>
  tabs.order.find((id) => panesOf(tabs, id).some((one) => one.session === number)) ?? -1;

describe("where a task is drawn, as rows of the catalogue (#1489, V100-38)", () => {
  it("offers each task a tab of its own and a pane beside its session, and no way back from nowhere", () => {
    const rows = rowsOf(sessions());

    expect(rows.get(ownTabId(2))).toMatchObject({
      title: "Move talk to its own tab",
      available: true,
      does: { verb: "ownTab", session: 2 },
      name: "talk",
    });
    expect(rows.get(besideId(2))).toMatchObject({
      title: "Open talk beside steward 1",
      available: true,
      does: { verb: "beside", session: 2 },
    });
    expect(rows.get(backId(2))).toBeUndefined();
    // Nothing of the kind for a chat nobody asked for.
    expect(rows.get(ownTabId(1))).toBeUndefined();
    expect(rows.get(besideId(1))).toBeUndefined();
  });

  it("makes the tab's close row of a task's own tab its minimise, which ends nothing", () => {
    const tabs = moveToOwnTab(sessions(), 2, "talk", "devops", null);
    const rows = rowsOf(tabs);

    const close = rows.get(`tab.close:${tabOf(tabs, 2)}`);
    expect(close).toMatchObject({
      title: "Send talk back into steward 1's tab",
      available: true,
      does: { verb: "sendBack", session: 2 },
    });
    // No row about that tab ends a chat, by any id.
    expect(
      [...rows.values()].filter(
        (row) => row.does.verb === "closeTab" && row.does.tab === tabOf(tabs, 2),
      ),
    ).toEqual([]);
    // Its pane's close is the same minimise.
    expect(
      paneCloseOf(tabs, tabs.byId[tabOf(tabs, 2)].focused, (n) => names[n], askedByOf(all)),
    ).toMatchObject({ id: "pane.close", does: { verb: "sendBack", session: 2 } });
    // The session's own tab still has the one close on the strip.
    expect(rows.get(`tab.close:${tabOf(tabs, 1)}`)).toMatchObject({
      title: "End chat steward 1",
      does: { verb: "closeTab", ends: true },
    });
    // And the rows about where it is say it: back, and not to its own tab again.
    expect(rows.get(backId(2))?.title).toBe("Send talk back into steward 1's tab");
    expect(rows.get(ownTabId(2))).toMatchObject({
      available: false,
      reason: "talk is in a tab of its own.",
    });
  });

  it("gives a task beside its session a minimise on its pane, and leaves the session's pane its close", () => {
    const tabs = openBeside(sessions(), 2, askedByOf(all), "talk");
    const id = tabOf(tabs, 1);
    const [own, beside] = panesOf(tabs, id);
    const nameOf = (n: number) => names[n];

    expect(paneCloseOf(tabs, beside.pane, nameOf, askedByOf(all))).toMatchObject({
      title: "Send talk back among steward 1's tasks",
      available: true,
      does: { verb: "sendBack", session: 2 },
    });
    expect(paneCloseOf(tabs, own.pane, nameOf, askedByOf(all))).toMatchObject({
      title: "End this pane's chat",
      does: { verb: "closePane", ends: true },
    });
    const rows = rowsOf(tabs);
    expect(rows.get(besideId(2))).toMatchObject({
      available: false,
      reason: "talk is open beside it.",
    });
    // The tab is the session's, and its close is the session's.
    expect(rows.get(`tab.close:${id}`)?.title).toBe("End chat steward 1");
  });

  it("says a task whose session has no tab here goes back to the Chats list, and opens beside nothing", () => {
    const orphan = listed(12, { name: "left behind", parent: 40, from: "steward 40" });
    const tabs = moveToOwnTab(sessions(), 12, "left behind", "devops", null);
    const rows = rowsOf(tabs, [...all, orphan]);

    expect(rows.get(`tab.close:${tabOf(tabs, 12)}`)).toMatchObject({
      title: "Send left behind back to the Chats list",
      does: { verb: "sendBack", session: 12 },
    });
    expect(rows.get(besideId(12))).toMatchObject({
      available: false,
      reason:
        "steward 40 has no tab in this window, so there is nothing to open left behind beside.",
    });
  });

  it("lists the three in a task's menu in the Chats list, above the line", () => {
    const tabs = moveToOwnTab(sessions(), 2, "talk", "devops", null);
    const menu = menuRows({ on: "listed", session: 2 }, rowsOf(tabs));
    expect(menu.above.map((row) => row.id)).toEqual([ownTabId(2), besideId(2), backId(2)]);
    // A session's row has none of them.
    expect(menuRows({ on: "listed", session: 1 }, rowsOf(tabs)).above).toEqual([]);
  });

  it("carries each out through the window's one table of verbs", async () => {
    const calls: string[] = [];
    const doing = new Proxy({} as Doing, {
      get:
        (_, verb: string) =>
        (...args: unknown[]) => {
          calls.push(`${verb}:${args.join(",")}`);
        },
    });
    const rows = rowsOf(moveToOwnTab(sessions(), 2, "talk", "devops", null));
    for (const id of [ownTabId(3), besideId(3), backId(2)]) {
      const row = rows.get(id);
      if (row === undefined) throw new Error(`no row ${id}`);
      await perform(row, doing);
    }
    expect(calls).toEqual(["ownTab:3", "beside:3", "sendBack:2"]);
  });
});

describe("the next and the previous chat in a tab (fix round 1)", () => {
  it("move inside the tab whatever the person set for a pressed task", async () => {
    const rows = rowsOf(sessions());
    const next = rows.get("tasks.next");
    expect(next?.does).toEqual({ verb: "showChat", session: 2, inside: true });
    const calls: unknown[][] = [];
    const doing = new Proxy({} as Doing, {
      get:
        (_, verb: string) =>
        (...args: unknown[]) => {
          calls.push([verb, ...args]);
        },
    });
    if (next === undefined) throw new Error("no row");
    await perform(next, doing);
    expect(calls).toEqual([["showChat", 2, true]]);
  });
});

describe("starting a task fresh (fix round 1, M2)", () => {
  it("is a row that cannot run on a task's own tab, with the core's own sentence", () => {
    const tabs = moveToOwnTab(sessions(), 2, "talk", "devops", null);
    const rows = catalogued(
      catalogue({
        tabs,
        workspaces: [],
        needsYou: [],
        nameOf: (number) => names[number] ?? String(number),
        listed: all,
        planeUpdated: { 1: ["CLAUDE.md"], 2: ["CLAUDE.md"] },
      }),
    );
    expect(rows.get(`tab.fresh:${tabOf(tabs, 2)}`)).toMatchObject({
      available: false,
      reason: TASK_NOT_FRESH,
    });
    expect(rows.get(`tab.fresh:${tabOf(tabs, 1)}`)?.available).toBe(true);
  });
});

describe("opening the task in front beside its session (#1499, #1489)", () => {
  it("is one row, which runs for the task the tab in front shows", () => {
    const shown = switchTabTo(sessions(), 2, askedByOf(all));
    const rows = catalogue({
      tabs: shown,
      workspaces: [],
      needsYou: [],
      nameOf: (number) => names[number] ?? String(number),
      listed: all,
    }).filter((row) => row.id === BESIDE_ID);

    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ available: true, does: { verb: "beside", session: 2 } });
  });

  it("cannot run where the chat in front is not a task, and says what does it", () => {
    const row = rowsOf(sessions()).get(BESIDE_ID);
    expect(row?.available).toBe(false);
    expect(row?.reason).toBe(
      "The chat in front is not a task. Space on a task's row in the Chats list opens it beside the chat that asked for it.",
    );
  });
});
