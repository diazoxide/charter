import { StrictMode, useEffect, useRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { OpenChat } from "./bindings";
import { setChatsListPrefs } from "./chatsListPrefs";
import { forgetKeyboard } from "./paneKeyboard";
import { forgetThisLaunch } from "./regions";

/**
 * **A task gets a tab of its own only when asked, and a pane beside its session** (#1489,
 * V100-38, V100-39, V100-74), against the whole window. The tab is drawn so it is not a
 * session's: the task mark, whose task it is, and a minimise where a session's tab has its
 * close. Nothing on it ends the task, and the minimise sends it back. Open beside splits the
 * session's own tab. The session's close is the only close on the strip.
 *
 * The core here is a fixture, as `TaskInTab.window.test.tsx` has it: a task is a chat whose
 * `from` says `task: true`. It keeps what the window tells it of each pane (`tab_shows`) and
 * of a task's tab (`open_chat_tab`), as the record does, and **every command it is sent is
 * kept**, so a test can say that nothing that ends a chat was sent.
 */

vi.mock("./SessionPane", async () => {
  const { paneDrawn } = await import("./paneKeyboard");
  return {
    SessionPane: ({ plane, session }: { plane: string; session: number }) => {
      const pane = useRef<HTMLDivElement>(null);
      useEffect(() => paneDrawn(plane, session, () => pane.current?.focus()), [plane, session]);
      return (
        <div data-testid="pane" data-session={session} tabIndex={-1} ref={pane}>
          session {session}
        </div>
      );
    },
  };
});

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";

type Lineage = NonNullable<OpenChat["from"]>;
type Listed = OpenChat & { workspace: string };

function chat(session: number, workspace: string, more: Partial<OpenChat> = {}): Listed {
  return {
    session,
    name: String(session),
    cwd: `${PLANE}/workspaces/${workspace}`,
    harness: "claude",
    in_front: session === 1,
    resumed: null,
    fresh: null,
    profile: null,
    persona: "steward",
    unreported: null,
    card: null,
    guessed: null,
    pinned: false,
    label: null,
    from: null,
    ...more,
    workspace,
  };
}

function taskOf(asker: number, more: Partial<Lineage> = {}): Lineage {
  return {
    chat: asker,
    name: `steward ${asker}`,
    workspace: "alpha",
    task: true,
    tab: false,
    reported: false,
    unreported: false,
    ...more,
  };
}

function asListed(one: Listed): OpenChat {
  const sent: OpenChat & { workspace?: string } = { ...one };
  delete sent.workspace;
  return sent;
}

type Asked = { cmd: string; args: Record<string, unknown> };

/** Every command that ends a chat's program, by any road. */
const ENDS = [
  "close_session",
  "close_chat_stopping",
  "stop_chat",
  "stop_persona_chats_of",
  "smart_close",
];

/** What a test's core does beside the ordinary. */
type Core = {
  /** The chats at work below a closing chat, by name: what its close asks about. */
  running?: string[];
  /** The chats a stop of everything below does not end. */
  spared?: number[];
  /** The chats the project's instructions changed under, with the files. */
  updated?: { session: number; files: string[] }[];
};

function core(open: Listed[], { running = [], spared = [], updated = [] }: Core = {}) {
  const asked: Asked[] = [];
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      asked.push({ cmd, args: a });
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return open.map(asListed);
      if (cmd === "open_chat_tab") {
        const one = open.find((chat) => chat.session === a.session);
        if (one?.from) one.from = { ...one.from, tab: true };
        return null;
      }
      if (cmd === "close_chat_tab") {
        // The one way a task's tab is sent back to the list (#1488): it has none from here on.
        const one = open.find((chat) => chat.session === a.session);
        if (one?.from) one.from = { ...one.from, tab: false };
        return null;
      }
      if (cmd === "chat_in_front") {
        for (const one of open) one.in_front = one.session === a.session;
        return null;
      }
      if (cmd === "tab_shows") {
        const one = open.find((chat) => chat.session === a.session);
        if (one === undefined) throw new Error(`no chat ${String(a.session)} is open`);
        one.shows = (a.shown as number | null) ?? null;
        one.beside = (a.beside as number | null) ?? null;
        return null;
      }
      if (cmd === "close_session") {
        const at = open.findIndex((chat) => chat.session === a.session);
        if (at >= 0) open.splice(at, 1);
        return null;
      }
      if (cmd === "persona_chats_of") return { tasks: [], running };
      if (cmd === "close_chat_stopping") {
        // The core ends everything below the chat, then the chat: it says which.
        const below = (session: number): number[] =>
          open
            .filter((chat) => chat.from?.task && chat.from.chat === session)
            .flatMap((chat) => [...below(chat.session), chat.session]);
        const gone = [...below(a.session as number), a.session as number].filter(
          (session) => !spared.includes(session),
        );
        for (const session of gone)
          open.splice(
            open.findIndex((chat) => chat.session === session),
            1,
          );
        return gone;
      }
      if (cmd === "smart_close_offer") return { available: false, why: null, close_first: false };
      if (cmd === "chats_plane_updated") return updated;
      if (cmd === "ask_chat_restart") return null;
      if (cmd === "task_ending") {
        // An idle task with nothing below it: the one that is asked about in place.
        const one = open.find((chat) => chat.session === a.session);
        return {
          name: one?.label ?? "",
          working: false,
          no_report: null,
          below: [],
          stopping: false,
          reported: false,
        };
      }
      if (cmd === "restart_chat") {
        // The same chat on its conversation, under a new number.
        const one = open.find((chat) => chat.session === a.session);
        if (!one) throw new Error(`no chat ${String(a.session)} is open`);
        one.session = (a.session as number) + 20;
        return { chat: asListed(one), notices: [], not_yet: null };
      }
      if (cmd === "dispatch_grants_needed") return [];
      if (cmd === "vault_refusals") return [];
      if (cmd === "owed_restarts") return [];
      if (cmd === "finished_tasks") return [];
      if (cmd === "stopping_chats") return [];
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          personas: ["steward", "devops"],
          persona: "steward",
          unfiled: [],
          workspaces: ["alpha", "beta"].map((name) => ({
            name,
            path: `${PLANE}/workspaces/${name}`,
            vision: "",
            todos: [],
            colour: null,
            live: false,
            chats: open.filter((chat) => chat.workspace === name).map(asListed),
          })),
        };
      if (cmd === "workspace_panels")
        return {
          workspace: a.workspace,
          repos: [],
          paths: {},
          absent: [],
          refused: [],
          todos: [],
          todos_refused: null,
          personas: ["steward", "devops"],
          persona: "steward",
        };
      if (cmd === "workspace_repos")
        return { workspace: a.workspace, repos: [], cache_refused: null };
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "alerts_everywhere") return [{ plane: PLANE, alerts: [], stopped: null }];
      return null;
    },
    { shouldMockEvents: true },
  );
  return { asked, open };
}

const section = () => screen.findByRole("tree", { name: "Chats of this project" });

const rows = (tree: HTMLElement) => within(tree).getAllByRole("treeitem");
const row = (tree: HTMLElement, name: string) => {
  const found = rows(tree).find((one) => one.querySelector(".session")?.textContent === name);
  if (found === undefined) throw new Error(`no row is named ${name}`);
  return found;
};

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const tabNames = () =>
  within(strip())
    .queryAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);
const tab = (name: string) => {
  const found = within(strip())
    .getAllByRole("tab")
    .find((one) => one.querySelector(".tab-name")?.textContent === name);
  if (found === undefined) throw new Error(`no tab is ${name}'s`);
  return found;
};
/** The cell of the strip a tab is drawn in: the tab, its chip and its one button. */
const cell = (name: string) => tab(name).closest<HTMLElement>(".tab") as HTMLElement;
/** The one button at the end of a tab's cell: a session's close, or a task's minimise. */
const ender = (name: string) => cell(name).querySelector<HTMLElement>("button.closer");

const onScreen = () =>
  screen.queryAllByTestId("pane").map((pane) => Number(pane.getAttribute("data-session")));

const crumbsSaid = () =>
  screen
    .queryAllByRole("navigation", { name: "Chat path" })
    .map((nav) => nav.textContent?.replace(/\s+/g, " ").trim());

const commandsOf = (asked: Asked[], cmd: string) =>
  asked.filter((one) => one.cmd === cmd).map((one) => one.args);

/** What was sent that ends a chat. */
const ended = (asked: Asked[]) => asked.filter((one) => ENDS.includes(one.cmd));

/** A press as a click and nothing before it (`TabChip.window.test.tsx` says why). */
const press = (on: Element) => fireEvent.click(on);

/**
 * steward 1 in alpha dispatched talk (4) and sweep (5) in alpha, and probe (6) in beta. talk
 * dispatched deep (7). steward 2 is another session in alpha.
 */
const withTasks = () => [
  chat(1, "alpha"),
  chat(2, "alpha"),
  chat(4, "alpha", { persona: "devops", label: "talk", from: taskOf(1) }),
  chat(5, "alpha", { persona: "devops", label: "sweep", from: taskOf(1) }),
  chat(6, "beta", { persona: "devops", label: "probe", from: taskOf(1) }),
  chat(7, "alpha", { persona: "devops", label: "deep", from: taskOf(4, { name: "talk" }) }),
];

async function drawn(open: Listed[], how: Core = {}) {
  const held = core(open, how);
  render(<App />);
  const tree = await section();
  await waitFor(() => expect(rows(tree)).toHaveLength(open.length));
  return { ...held, tree };
}

/** Picks a row of the context menu that is open. */
async function pick(name: string | RegExp) {
  await userEvent.click(await screen.findByRole("menuitem", { name }));
}

/** Moves a task to a tab of its own from its row's menu in the Chats list. */
async function toOwnTab(tree: HTMLElement, name: string) {
  fireEvent.contextMenu(row(tree, name));
  await pick(`Move ${name} to its own tab`);
  await waitFor(() => expect(tabNames()).toContain(name));
}

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
  forgetKeyboard();
});
afterEach(() => {
  cleanup();
  clearMocks();
});

describe("moving a task to a tab of its own", () => {
  it("gives it a tab from its row's menu, drawn as a task's and not a session's", async () => {
    const { tree, asked } = await drawn(withTasks());

    await toOwnTab(tree, "talk");

    expect(tabNames()).toEqual(["steward 1", "steward 2", "talk"]);
    expect(tab("talk").getAttribute("aria-selected")).toBe("true");
    expect(onScreen()).toEqual([4]);
    // The task mark, its name, and whose task it is.
    expect(tab("talk").querySelector('[data-mark="task"]')).not.toBeNull();
    expect(tab("talk").textContent?.replace(/\s+/g, " ")).toContain("talk, a task of · steward 1");
    expect(tab("talk").querySelector(".tab-asker")?.textContent).toBe("steward 1");
    // A session's tab has none of that.
    expect(tab("steward 1").querySelector('[data-mark="task"]')).toBeNull();
    expect(cell("steward 1").hasAttribute("data-task-tab")).toBe(false);
    // The core is told it has a tab, which is what brings it back at a launch, and that it is
    // the chat in front, which is what it is looked at by.
    expect(commandsOf(asked, "open_chat_tab")).toEqual([{ plane: PLANE, session: 4 }]);
    expect(commandsOf(asked, "chat_in_front").at(-1)).toEqual({ plane: PLANE, session: 4 });
    expect(ended(asked)).toEqual([]);
  });

  it("has a minimise in place of the close, and no control that ends the task", async () => {
    const { tree } = await drawn(withTasks());
    await toOwnTab(tree, "talk");

    const minimise = ender("talk");
    expect(minimise?.getAttribute("aria-label")).toBe("Send talk back into steward 1's tab");
    expect(minimise?.hasAttribute("data-minimise")).toBe(true);
    // Not the danger look a close wears, and no close anywhere in the tab's cell.
    expect(minimise?.className).toContain("keeps");
    const said = [...cell("talk").querySelectorAll("button")].map(
      (button) => button.getAttribute("aria-label") ?? button.textContent ?? "",
    );
    expect(said.filter((name) => /\b(end|close|stop)\b/i.test(name))).toEqual([]);
    // Its pane has the same minimise where a session's pane has its close.
    expect(screen.queryByRole("button", { name: "End this pane's chat" })).toBeNull();
    expect(
      screen.getAllByRole("button", { name: "Send talk back into steward 1's tab" }),
    ).toHaveLength(2);
    // The session's tab keeps the one close on the strip.
    expect(ender("steward 1")?.getAttribute("aria-label")).toBe("End chat steward 1");
    expect(ender("steward 1")?.hasAttribute("data-minimise")).toBe(false);
  });

  it("is sent back by the minimise, which ends nothing: the task is in its session's tab again", async () => {
    const { tree, asked } = await drawn(withTasks());
    await toOwnTab(tree, "talk");

    await userEvent.click(ender("talk") as HTMLElement);

    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "steward 2"]));
    // No dialog, and nothing that ends a chat was sent.
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(ended(asked)).toEqual([]);
    expect(commandsOf(asked, "close_chat_tab").at(-1)).toEqual({
      plane: PLANE,
      session: 4,
    });
    // It is still listed, and its session's tab shows what it showed: its own chat.
    expect(row(tree, "talk")).toBeTruthy();
    await userEvent.click(tab("steward 1"));
    await waitFor(() => expect(onScreen()).toEqual([1]));
    // Pressed again, it is shown inside its session's tab, as any task is.
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
  });

  it("is minimised by the key that closes a tab, and by its pane's control", async () => {
    const { tree, asked } = await drawn(withTasks());
    await toOwnTab(tree, "talk");

    tab("talk").focus();
    await userEvent.keyboard("{Delete}");
    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "steward 2"]));
    expect(screen.queryByRole("alertdialog")).toBeNull();

    await toOwnTab(tree, "talk");
    const controls = screen.getAllByRole("button", { name: "Send talk back into steward 1's tab" });
    await userEvent.click(controls[controls.length - 1]);
    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "steward 2"]));
    expect(ended(asked)).toEqual([]);
  });

  it("is still listed by its session's chip and menu, marked, and picking it brings its tab forward", async () => {
    const { tree } = await drawn(withTasks());
    await toOwnTab(tree, "talk");
    await userEvent.click(tab("steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));

    const chip = within(cell("steward 1")).getByRole("button", { name: /^Tasks of steward 1/ });
    // Its three tasks are counted, the one in its own tab among them. What is below that
    // one (deep) is its own tab's.
    expect(chip.getAttribute("aria-label")).toBe("Tasks of steward 1: 3 working");
    press(chip);
    const menu = await screen.findByRole("menu", { name: /^Tasks of steward 1/ });
    const line = within(menu)
      .getAllByRole("menuitem")
      .find((item) => item.querySelector(".name")?.textContent === "talk");
    expect(line?.textContent?.replace(/\s+/g, " ")).toContain("talk, in its own tab");
    // What is below it is its own tab's, and is not listed here.
    expect(within(menu).queryByText("deep")).toBeNull();

    await userEvent.click(line as HTMLElement);

    await waitFor(() => expect(onScreen()).toEqual([4]));
    expect(tab("talk").getAttribute("aria-selected")).toBe("true");
    expect(tabNames()).toEqual(["steward 1", "steward 2", "talk"]);
  });

  it("is offered on the task's line in its tab's menu: Enter with a modifier, and a button at the line's end", async () => {
    const { asked } = await drawn(withTasks());
    const open = async () => {
      press(within(cell("steward 1")).getByRole("button", { name: /^Tasks of steward 1/ }));
      return screen.findByRole("menu", { name: /^Tasks of steward 1/ });
    };
    const lineOf = (menu: HTMLElement, name: string) => {
      const found = within(menu)
        .getAllByRole("menuitem")
        .find((item) => item.querySelector(".name")?.textContent === name);
      if (found === undefined) throw new Error(`the menu has no line for ${name}`);
      return found;
    };

    // The keyboard: Ctrl+Enter on its line is a tab of its own, as a link opens in a new tab.
    let menu = await open();
    lineOf(menu, "sweep").focus();
    await userEvent.keyboard("{Control>}{Enter}{/Control}");
    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "steward 2", "sweep"]));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(commandsOf(asked, "open_chat_tab")).toEqual([{ plane: PLANE, session: 5 }]);

    // The pointer: the buttons at the end of a task's line. A press of one is not a press of
    // the line, which would have shown the task in this tab.
    menu = await open();
    const places = [...lineOf(menu, "talk").querySelectorAll<HTMLElement>(".tasks-menu-place")];
    expect(places.map((one) => one.dataset.says)).toEqual([
      "Move talk to its own tab",
      "Open talk beside its session",
    ]);
    // sweep is in its own tab: its line offers the way back, and not a second tab.
    expect(
      [...lineOf(menu, "sweep").querySelectorAll<HTMLElement>(".tasks-menu-place")].map(
        (one) => one.dataset.says,
      ),
    ).toEqual(["Open sweep beside its session", "Send sweep back into this tab"]);
    // The session's own chat has none: it is its tab.
    expect(lineOf(menu, "steward 1").querySelector(".tasks-menu-place")).toBeNull();
    press(places[1]);
    await waitFor(() => expect(onScreen()).toEqual([1, 4]));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(tabNames()).toEqual(["steward 1", "steward 2", "sweep"]);

    // Alt+Enter is beside its session.
    menu = await open();
    lineOf(menu, "sweep").focus();
    await userEvent.keyboard("{Alt>}{Enter}{/Alt}");
    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "steward 2"]));
    expect(onScreen()).toContain(5);
    expect(ended(asked)).toEqual([]);
  });

  it("is offered on the breadcrumb's line while its session's tab shows it", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    // The pane's controls, on the line the breadcrumb is in.
    await userEvent.click(screen.getByRole("button", { name: "Move talk to its own tab" }));

    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "steward 2", "talk"]));
    // The session's tab does not go on showing it: one chat, one place.
    await userEvent.click(tab("steward 1"));
    await waitFor(() => expect(onScreen()).toEqual([1]));
    expect(screen.queryByTestId("task-away")).toBeNull();
  });

  it("has the task's menu on a right-click of its breadcrumb", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));

    fireEvent.contextMenu(screen.getByRole("navigation", { name: "Chat path" }));
    await pick("Open talk beside steward 1");

    await waitFor(() => expect(onScreen()).toEqual([1, 4]));
  });

  it("says its path in its own pane's top line, so the pane is not read as a session's", async () => {
    const { tree } = await drawn(withTasks());
    await toOwnTab(tree, "deep");
    expect(crumbsSaid()).toEqual([expect.stringContaining("steward 1 › talk › deep")]);
  });

  it("takes its place among the tabs for the arrows, as any tab does", async () => {
    const { tree } = await drawn(withTasks());
    await toOwnTab(tree, "talk");

    tab("talk").focus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(tab("steward 2"));
    await userEvent.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(tab("talk"));
  });

  it("is on the strip of the workspace it works in, where that is not its session's", async () => {
    // probe works in beta, and steward 1 asked for it from alpha (V100-40).
    const { tree } = await drawn(withTasks());

    await toOwnTab(tree, "probe");

    // The strip in front is beta's now, and it holds the task's tab alone.
    expect(tabNames()).toEqual(["probe"]);
    expect(tab("probe").querySelector(".tab-asker")?.textContent).toBe("steward 1");
    expect(ender("probe")?.getAttribute("aria-label")).toBe("Send probe back into steward 1's tab");
  });

  it("goes back to the Chats list where the chat that asked has no tab here", async () => {
    // Chat 9 asked for it and has closed since.
    const { tree, asked } = await drawn([
      chat(1, "alpha"),
      chat(12, "alpha", { persona: "devops", label: "left behind", from: taskOf(9) }),
    ]);
    await userEvent.click(row(tree, "left behind"));
    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "left behind"]));

    // Its tab is a task's too: a minimise, never a close.
    expect(ender("left behind")?.getAttribute("aria-label")).toBe(
      "Send left behind back to the Chats list",
    );
    await userEvent.click(ender("left behind") as HTMLElement);

    await waitFor(() => expect(tabNames()).toEqual(["steward 1"]));
    expect(row(tree, "left behind")).toBeTruthy();
    expect(ended(asked)).toEqual([]);
  });
});

describe("opening a task beside its session", () => {
  it("splits the session's own tab on Space, the session on one side and the task on the other", async () => {
    const { tree, asked } = await drawn(withTasks());

    row(tree, "talk").focus();
    await userEvent.keyboard(" ");

    await waitFor(() => expect(onScreen()).toEqual([1, 4]));
    // No tab was added, and the tab is the session's.
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
    expect(tab("steward 1").getAttribute("aria-selected")).toBe("true");
    // Each pane says which chat it is.
    expect(crumbsSaid()).toEqual([
      expect.stringMatching(/^steward 1 ·/),
      expect.stringMatching(/^steward 1 › talk ·/),
    ]);
    // The core is told the task is on screen beside the session: every pane of the tab in
    // front is looked at (`Chats::looked_at`).
    expect(commandsOf(asked, "tab_shows").at(-1)).toEqual({
      plane: PLANE,
      session: 4,
      shown: null,
      beside: 1,
    });
    expect(commandsOf(asked, "chat_in_front").at(-1) ?? { session: 1 }).toMatchObject({
      session: 1,
    });
    expect(ended(asked)).toEqual([]);
  });

  it("gives the task's pane a minimise and the session's pane its close", async () => {
    const { tree, asked } = await drawn(withTasks());
    fireEvent.contextMenu(row(tree, "talk"));
    await pick("Open talk beside steward 1");
    await waitFor(() => expect(onScreen()).toEqual([1, 4]));

    // One close, the session's pane's; and one minimise, the task's.
    expect(screen.getAllByRole("button", { name: "End this pane's chat" })).toHaveLength(1);
    const minimise = screen.getByRole("button", { name: "Send talk back among steward 1's tasks" });
    expect(minimise.hasAttribute("data-minimise")).toBe(true);
    expect(minimise.className).not.toContain("ends-a-chat");

    await userEvent.click(minimise);

    // The session's pane takes the room, and the task is in the list as it was.
    await waitFor(() => expect(onScreen()).toEqual([1]));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(ended(asked)).toEqual([]);
    expect(row(tree, "talk")).toBeTruthy();
    expect(crumbsSaid()).toEqual([]);
    // The core is told its pane is gone: it is not on screen any more.
    expect(commandsOf(asked, "tab_shows").at(-1)).toEqual({
      plane: PLANE,
      session: 4,
      shown: null,
      beside: null,
    });
  });

  it("lists it under its session in the tab's menu, marked, with what is below it", async () => {
    const { tree } = await drawn(withTasks());
    row(tree, "talk").focus();
    await userEvent.keyboard(" ");
    await waitFor(() => expect(onScreen()).toEqual([1, 4]));

    press(within(cell("steward 1")).getByRole("button", { name: /^Tasks of steward 1/ }));
    const menu = await screen.findByRole("menu", { name: /^Tasks of steward 1/ });
    const names = within(menu)
      .getAllByRole("menuitem")
      // Less the one line that opens the ways to end a task (#1488), which is no chat's.
      .filter((item) => !item.classList.contains("tasks-menu-end"))
      .map((item) => item.querySelector(".name")?.textContent);
    expect(names).toEqual(["steward 1", "talk", "deep", "sweep", "probe"]);
    expect(within(menu).getByText(", beside it")).toBeTruthy();
  });

  it("keeps the task's own name in the list: it is not called what the session's tab is", async () => {
    const { tree } = await drawn(withTasks());
    row(tree, "talk").focus();
    await userEvent.keyboard(" ");
    await waitFor(() => expect(onScreen()).toEqual([1, 4]));
    expect(rows(tree).map((one) => one.querySelector(".session")?.textContent)).toContain("talk");
    expect(
      rows(tree).filter((one) => one.querySelector(".session")?.textContent === "steward 1"),
    ).toHaveLength(1);
  });

  it("says why Space does nothing on a chat that is not a task", async () => {
    const { tree, asked } = await drawn(withTasks());

    row(tree, "steward 2").focus();
    await userEvent.keyboard(" ");

    expect(
      await screen.findByText(
        "Only a task opens beside the chat that asked for it. Press Enter to show this chat.",
      ),
    ).toBeTruthy();
    expect(onScreen()).toEqual([1]);
    expect(commandsOf(asked, "tab_shows")).toEqual([]);
  });

  it("closes the session's tab without ending the task beside it, which goes back to the list", async () => {
    const { tree, asked } = await drawn(withTasks(), {
      running: ["talk", "sweep", "probe", "deep"],
    });
    row(tree, "talk").focus();
    await userEvent.keyboard(" ");
    await waitFor(() => expect(onScreen()).toEqual([1, 4]));

    await userEvent.click(ender("steward 1") as HTMLElement);
    // One chat ends, the session: the question is the one a session's close always asked.
    const question = await screen.findByRole("alertdialog", { name: "End chat steward 1?" });
    await userEvent.click(within(question).getByRole("button", { name: "Close" }));

    await waitFor(() => expect(tabNames()).toEqual(["steward 2"]));
    expect(commandsOf(asked, "close_session")).toEqual([{ plane: PLANE, session: 1 }]);
  });
});

describe("the setting that opens tasks in their own tabs", () => {
  it("is off until the person turns it on: a pressed task switches its session's tab", async () => {
    const { tree } = await drawn(withTasks());
    await userEvent.click(row(tree, "talk"));
    await waitFor(() => expect(onScreen()).toEqual([4]));
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
  });

  it("opens a pressed task as its own tab, with the minimise, and the chip and menu stay", async () => {
    const { tree, asked } = await drawn(withTasks());
    act(() => setChatsListPrefs({ tabbed: true }));

    await userEvent.click(row(tree, "talk"));

    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "steward 2", "talk"]));
    expect(onScreen()).toEqual([4]);
    expect(ender("talk")?.hasAttribute("data-minimise")).toBe(true);
    expect(commandsOf(asked, "open_chat_tab")).toEqual([{ plane: PLANE, session: 4 }]);
    // The session's tab still wears its chip, and its menu still lists the task.
    const chip = within(cell("steward 1")).getByRole("button", { name: /^Tasks of steward 1/ });
    press(chip);
    const menu = await screen.findByRole("menu", { name: /^Tasks of steward 1/ });
    expect(within(menu).getByText(", in its own tab")).toBeTruthy();
    // A session's own chat is still its tab.
    await userEvent.click(row(tree, "steward 2"));
    await waitFor(() => expect(onScreen()).toEqual([2]));
    expect(tabNames()).toEqual(["steward 1", "steward 2", "talk"]);
  });
});

describe("the session's close, the only close on the strip", () => {
  it("says how many of its tasks have a tab of their own, and sends them back when they are kept", async () => {
    const { tree, asked } = await drawn(withTasks(), {
      running: ["talk", "sweep", "probe", "deep"],
    });
    await toOwnTab(tree, "talk");
    await toOwnTab(tree, "sweep");

    // Every other button at a tab's end is a minimise.
    expect(ender("talk")?.hasAttribute("data-minimise")).toBe(true);
    expect(ender("sweep")?.hasAttribute("data-minimise")).toBe(true);
    await userEvent.click(ender("steward 1") as HTMLElement);

    const question = await screen.findByRole("alertdialog", { name: "End chat steward 1?" });
    expect(question.textContent).toContain(
      "2 of its tasks have tabs of their own. Those tabs close with this one: tasks that go on working stay in the Chats list.",
    );
    // Asked once, as it always was.
    expect(within(question).getAllByRole("radio")).toHaveLength(2);
    expect(ended(asked)).toEqual([]);
    await userEvent.click(within(question).getByRole("button", { name: "Close" }));

    await waitFor(() => expect(tabNames()).toEqual(["steward 2"]));
    // Only the session ended. Its tasks go on, with no tab.
    expect(ended(asked)).toEqual([{ cmd: "close_session", args: { plane: PLANE, session: 1 } }]);
    expect(
      commandsOf(asked, "close_chat_tab")
        .map((one) => one.session)
        .sort(),
    ).toEqual([4, 5]);
    expect(row(tree, "talk")).toBeTruthy();
  });

  it("takes their tabs with it when the answer is to stop them", async () => {
    const { tree, asked } = await drawn(withTasks(), {
      running: ["talk", "sweep", "probe", "deep"],
    });
    await toOwnTab(tree, "talk");

    await userEvent.click(ender("steward 1") as HTMLElement);
    const question = await screen.findByRole("alertdialog", { name: "End chat steward 1?" });
    await userEvent.click(within(question).getByRole("radio", { name: "Stop them" }));
    await userEvent.click(within(question).getByRole("button", { name: "Close" }));

    await waitFor(() => expect(tabNames()).toEqual(["steward 2"]));
    expect(commandsOf(asked, "close_chat_stopping")).toEqual([
      { plane: PLANE, session: 1, then: "close" },
    ]);
  });

  it("says nothing of tabs for a session none of whose tasks has one", async () => {
    await drawn(withTasks(), { running: ["talk"] });
    await userEvent.click(ender("steward 1") as HTMLElement);
    const question = await screen.findByRole("alertdialog", { name: "End chat steward 1?" });
    expect(question.textContent).not.toContain("of its own");
    expect(question.textContent).not.toContain("of their own");
  });
});

describe("after a relaunch", () => {
  it("puts a task's own tab back as a task's tab, and a task beside its session back beside it", async () => {
    const open = withTasks();
    open[2].from = taskOf(1, { tab: true });
    open[3].beside = 1;

    await drawn(open);

    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "steward 2", "talk"]));
    expect(ender("talk")?.hasAttribute("data-minimise")).toBe(true);
    expect(tab("talk").querySelector('[data-mark="task"]')).not.toBeNull();
    // The session's tab is in front, split, as it was left.
    expect(onScreen()).toEqual([1, 5]);
    expect(
      screen.getByRole("button", { name: "Send sweep back among steward 1's tasks" }),
    ).toBeTruthy();
  });

  it("leaves a task in the list when the session it was beside did not come back, and tells the core", async () => {
    const { tree, asked } = await drawn([
      chat(1, "alpha"),
      chat(12, "alpha", {
        persona: "devops",
        label: "left behind",
        from: taskOf(9),
        beside: 9,
      }),
    ]);

    expect(tabNames()).toEqual(["steward 1"]);
    expect(onScreen()).toEqual([1]);
    expect(row(tree, "left behind")).toBeTruthy();
    // The record's word is taken back: nothing is beside a chat that is gone.
    await waitFor(() =>
      expect(commandsOf(asked, "tab_shows")).toEqual([
        { plane: PLANE, session: 12, shown: null, beside: null },
      ]),
    );
  });
});

describe("nothing drawn on a task's tab ends it (fix round 1, M2)", () => {
  const changed = { updated: [1, 4].map((session) => ({ session, files: ["CLAUDE.md"] })) };

  it("draws no Start fresh mark on a task's own tab, where a session's tab has one", async () => {
    const { tree } = await drawn(withTasks(), changed);
    await toOwnTab(tree, "talk");

    // The session's tab wears the mark: the project's instructions changed under it.
    await userEvent.click(tab("steward 1"));
    await waitFor(() =>
      expect(within(cell("steward 1")).queryByRole("button", { name: /fresh/i })).not.toBeNull(),
    );
    // The task's tab does not, though they changed under it too.
    const said = [...cell("talk").querySelectorAll("button")].map(
      (button) => button.getAttribute("aria-label") ?? button.textContent ?? "",
    );
    expect(said.filter((name) => /\b(fresh|end|close|stop|restart)\b/i.test(name))).toEqual([]);
  });

  it("keeps Start fresh in the tab's menu as a row that cannot run, and says why", async () => {
    const { tree, asked } = await drawn(withTasks(), changed);
    await toOwnTab(tree, "talk");

    fireEvent.contextMenu(tab("talk"));
    const row = await screen.findByRole("menuitem", { name: /^Start chat .*talk fresh/ });
    expect(row.getAttribute("aria-disabled")).toBe("true");
    expect(row.title).toContain("This chat is a task");
    expect(commandsOf(asked, "start_chat_fresh")).toEqual([]);
  });

  it("asks before a task's Restart chat, and restarts nothing until the person says so", async () => {
    const { tree, asked } = await drawn(withTasks());
    await toOwnTab(tree, "talk");

    fireEvent.contextMenu(tab("talk"));
    await pick(/^Restart chat .*talk/);

    // Asked where an end of the task is asked (#1488): on its own breadcrumb's line, with
    // Keep, and no dialog.
    const question = await screen.findByRole("group", { name: /^Restart talk\?/ });
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(question.textContent).toContain("It stays a task, and still owes its report.");
    expect(commandsOf(asked, "ask_chat_restart")).toEqual([]);
    await userEvent.click(within(question).getByRole("button", { name: "Restart it" }));
    await waitFor(() =>
      expect(commandsOf(asked, "ask_chat_restart")).toEqual([{ plane: PLANE, session: 4 }]),
    );
  });

  it("asks in a dialog on a task's tab that draws no breadcrumb for it, for an end and for a restart", async () => {
    // Chat 9 asked for it and has closed since: its own tab has no path to say, so there is
    // no breadcrumb line to ask on. A question set there would stand where nobody sees it.
    const { tree, asked } = await drawn([
      chat(1, "alpha"),
      chat(12, "alpha", { persona: "devops", label: "left behind", from: taskOf(9) }),
    ]);
    await userEvent.click(row(tree, "left behind"));
    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "left behind"]));
    expect(crumbsSaid()).toEqual([]);

    fireEvent.contextMenu(tab("left behind"));
    await pick(/^Stop and get its report/);
    const end = await screen.findByRole("alertdialog");
    expect(screen.queryByRole("group", { name: /left behind/ })).toBeNull();
    expect(commandsOf(asked, "end_task")).toEqual([]);
    await userEvent.click(within(end).getByRole("button", { name: /^(Keep|Cancel)/ }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());

    fireEvent.contextMenu(tab("left behind"));
    await pick(/^Restart chat .*left behind/);
    const restart = await screen.findByRole("alertdialog", { name: "Restart left behind?" });
    expect(commandsOf(asked, "ask_chat_restart")).toEqual([]);
    await userEvent.click(within(restart).getByRole("button", { name: "Restart it" }));
    await waitFor(() =>
      expect(commandsOf(asked, "ask_chat_restart")).toEqual([{ plane: PLANE, session: 12 }]),
    );
    expect(ended(asked)).toEqual([]);
  });

  it("lists a task's Send back above the line of its tab's menu: it ends nothing", async () => {
    const { tree } = await drawn(withTasks());
    await toOwnTab(tree, "talk");

    fireEvent.contextMenu(tab("talk"));
    const menu = await screen.findByRole("menu");
    const items = [...menu.querySelectorAll('[role="menuitem"], [role="separator"]')];
    const back = items.findIndex((one) => one.textContent?.includes("Send talk back"));
    const line = items.findIndex((one) => one.getAttribute("role") === "separator");
    expect(back).toBeGreaterThanOrEqual(0);
    expect(line < 0 || back < line).toBe(true);
  });
});

describe("a session's pane goes: its tasks' own tabs go back to the list, by every path (M3)", () => {
  /** talk in a tab of its own, and the session's tab in front. */
  async function withTalkInItsTab(how: Core = {}) {
    const held = await drawn(withTasks(), how);
    await toOwnTab(held.tree, "talk");
    await userEvent.click(tab("steward 1"));
    await waitFor(() => expect(onScreen()).toEqual([1]));
    return held;
  }
  /** talk's tab is gone, talk is not ended, and the core is told it has no tab. */
  async function talkWentBack({ asked, tree }: { asked: Asked[]; tree: HTMLElement }) {
    await waitFor(() => expect(tabNames()).toEqual(["steward 2"]));
    expect(row(tree, "talk")).toBeTruthy();
    expect(commandsOf(asked, "close_chat_tab").at(-1)).toEqual({
      plane: PLANE,
      session: 4,
    });
    expect(ended(asked).filter((one) => one.args.session === 4 || one.cmd === "stop_chat")).toEqual(
      [],
    );
  }

  it("a close of the session's pane", async () => {
    const held = await withTalkInItsTab({ running: ["talk"] });
    await userEvent.click(screen.getByRole("button", { name: "End this pane's chat" }));
    const question = await screen.findByRole("alertdialog");
    // The sentence is said for this close too, and is true of it.
    expect(question.textContent).toContain("1 of its tasks has a tab of its own");
    await userEvent.click(within(question).getByRole("button", { name: "Close" }));
    await talkWentBack(held);
    expect(commandsOf(held.asked, "close_session")).toEqual([{ plane: PLANE, session: 1 }]);
  });

  it("a Smart close whose record has landed", async () => {
    const held = await withTalkInItsTab();
    held.open.splice(0, 1);
    await act(() =>
      emit("smart-close", { plane: PLANE, session: 1, phase: "closed", record: null }),
    );
    await talkWentBack(held);
  });

  it("the session ending, as a stop ends it", async () => {
    const held = await withTalkInItsTab();
    held.open.splice(0, 1);
    await act(() => emit("chat-stop", { plane: PLANE, session: 1, phase: "stopped" }));
    await talkWentBack(held);
  });

  it("a close that stops what is below, for a task the core did not end", async () => {
    const held = await withTalkInItsTab({ running: ["sweep"], spared: [4, 7] });
    await userEvent.click(ender("steward 1") as HTMLElement);
    const question = await screen.findByRole("alertdialog", { name: "End chat steward 1?" });
    await userEvent.click(within(question).getByRole("radio", { name: "Stop them" }));
    await userEvent.click(within(question).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(tabNames()).toEqual(["steward 2"]));
    expect(commandsOf(held.asked, "close_chat_tab").at(-1)).toEqual({
      plane: PLANE,
      session: 4,
    });
  });

  it("the session's pane closed in a split: the task beside it goes back too, and is never the tab's own chat", async () => {
    const { tree, asked } = await drawn(withTasks(), { running: ["talk"] });
    row(tree, "talk").focus();
    await userEvent.keyboard(" ");
    await waitFor(() => expect(onScreen()).toEqual([1, 4]));

    await userEvent.click(screen.getByRole("button", { name: "End this pane's chat" }));
    await userEvent.click(
      within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Close" }),
    );

    await waitFor(() => expect(tabNames()).toEqual(["steward 2"]));
    expect(commandsOf(asked, "close_session")).toEqual([{ plane: PLANE, session: 1 }]);
    expect(row(tree, "talk")).toBeTruthy();
    // The core is not left believing the closed session is in front, or the task beside it.
    expect(commandsOf(asked, "chat_in_front").at(-1)).toEqual({ plane: PLANE, session: 2 });
    expect(commandsOf(asked, "tab_shows").at(-1)).toEqual({
      plane: PLANE,
      session: 4,
      shown: null,
      beside: null,
    });
  });
});

describe("what the core is told is on screen (M1, M4)", () => {
  it("says the chat in front when the tab in front comes to be another chat's without changing", async () => {
    const { open, asked } = await drawn(withTasks());
    expect(tab("steward 1").getAttribute("aria-selected")).toBe("true");

    // steward 1 is started again on its conversation: the same tab, in front, is chat 21's.
    fireEvent.contextMenu(tab("steward 1"));
    await pick(/^Restart chat .*steward 1/);
    await waitFor(() => expect(open[0].session).toBe(21));

    await waitFor(() =>
      expect(commandsOf(asked, "chat_in_front").at(-1)).toEqual({ plane: PLANE, session: 21 }),
    );
    expect(tabNames()).toEqual(["steward 1", "steward 2"]);
  });

  it("stops the session's pane showing a task of the task that is moved, and tells the core", async () => {
    const { tree, asked } = await drawn(withTasks());
    // steward 1's tab shows deep, a task of talk.
    await userEvent.click(row(tree, "deep"));
    await waitFor(() => expect(onScreen()).toEqual([7]));
    await waitFor(() =>
      expect(commandsOf(asked, "tab_shows").at(-1)).toEqual({
        plane: PLANE,
        session: 1,
        shown: 7,
        beside: null,
      }),
    );

    await toOwnTab(tree, "talk");

    // deep went with talk: the session's pane shows its own chat, and the core is told so.
    await waitFor(() =>
      expect(commandsOf(asked, "tab_shows").at(-1)).toEqual({
        plane: PLANE,
        session: 1,
        shown: null,
        beside: null,
      }),
    );
    await userEvent.click(tab("steward 1"));
    await waitFor(() => expect(onScreen()).toEqual([1]));
    expect(screen.queryByTestId("task-away")).toBeNull();
  });

  it("keeps the minimise on a task's own pane while that pane shows a task of its own", async () => {
    const { tree } = await drawn(withTasks());
    row(tree, "talk").focus();
    await userEvent.keyboard(" ");
    await waitFor(() => expect(onScreen()).toEqual([1, 4]));

    await userEvent.click(row(tree, "deep"));
    await waitFor(() => expect(onScreen()).toEqual([1, 7]));

    expect(
      screen.getByRole("button", { name: "Send talk back among steward 1's tasks" }),
    ).toBeTruthy();
  });

  it("draws a task's tab as a task's from the launch's own word, before the list is read", async () => {
    const open = withTasks();
    open[2].from = taskOf(1, { tab: true });
    core(open);
    render(<App />);

    // The first frame the tab is in: a minimise, never a session's close.
    await waitFor(() => expect(tabNames()).toContain("talk"));
    expect(ender("talk")?.hasAttribute("data-minimise")).toBe(true);
  });

  it("says the key in the tooltip of a line's own buttons", async () => {
    await drawn(withTasks());
    press(within(cell("steward 1")).getByRole("button", { name: /^Tasks of steward 1/ }));
    const menu = await screen.findByRole("menu", { name: /^Tasks of steward 1/ });
    const titles = [...menu.querySelectorAll<HTMLElement>(".tasks-menu-place")]
      .slice(0, 2)
      .map((one) => one.title);
    expect(titles).toEqual([
      "Move talk to its own tab (Ctrl+Enter on its line)",
      "Open talk beside its session (Alt+Enter on its line)",
    ]);
  });
});
