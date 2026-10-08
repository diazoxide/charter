import { StrictMode } from "react";
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
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { OpenChat } from "./bindings";
import { forgetKeyboard } from "./paneKeyboard";
import { forgetThisLaunch } from "./regions";
import { GLOBAL } from "./windowprefs";

/**
 * **The first dispatch on a machine explains the chip, once** (#1501, V100-55), against the
 * whole window and a core that is a fixture, as `TabChip.window.test.tsx` has it: a task is a
 * chat whose `from` says `task: true`.
 *
 * Where the Notice stands over the real pane, and that the chip's menu is drawn over it, are
 * the scenario spec's: jsdom lays nothing out. The menu is portaled over the whole window
 * (`App.css`, `#root`), so nothing drawn in a pane can cover it.
 */

vi.mock("./SessionPane", async () => {
  const { paneDrawn } = await import("./paneKeyboard");
  const { useEffect, useRef } = await import("react");
  return {
    /** A pane that takes the keyboard when the window gives it, as a terminal does. */
    SessionPane: ({ plane, session }: { plane: string; session: number }) => {
      const pane = useRef<HTMLDivElement>(null);
      useEffect(() => paneDrawn(plane, session, () => pane.current?.focus()), [plane, session]);
      return (
        <div
          data-testid="pane"
          data-session={session}
          data-chat-keyboard=""
          tabIndex={-1}
          ref={pane}
        >
          session {session}
        </div>
      );
    },
  };
});

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

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

/** What this machine's layout file holds of what was seen once, as a launch reads it. */
const disk: { seen: string[] } = { seen: [] };

/** A launch: the window is handed the layout file as it is on disk now. */
function launch() {
  forgetThisLaunch();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout:
      disk.seen.length === 0
        ? { path: "", found: false, document: null, trouble: null }
        : {
            path: "/home/dev/.config/purlis/layout.json",
            found: true,
            document: { version: 1, regions: [], dismissed: { "on this machine": disk.seen } },
            trouble: null,
          },
    theme: { path: "", found: false, document: null, trouble: null },
  };
}

/** A core that lists `open`, and answers which chats a person is at by `attended`. */
function core(open: Listed[], attended: (session: number) => boolean = () => true) {
  const asked: Asked[] = [];
  const listeners = new Map<string, number[]>();
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: a });
    if (cmd === "plugin:event|listen") {
      const { event, handler } = args as { event: string; handler: number };
      listeners.set(event, [...(listeners.get(event) ?? []), handler]);
      return 1;
    }
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return open.map(asListed);
    if (cmd === "chat_attended") return attended(a.session as number);
    if (cmd === "see_on_this_machine") {
      const cause = a.cause as string;
      if (!disk.seen.includes(cause)) disk.seen = [...disk.seen, cause];
      return null;
    }
    if (cmd === "seen_on_this_machine") return disk.seen.includes(a.cause as string);
    if (cmd === "tab_shows") {
      const one = open.find((chat) => chat.session === a.session);
      if (one) one.shows = (a.shown as number | null) ?? null;
      return null;
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
        workspaces: ["alpha"].map((name) => ({
          name,
          path: `${PLANE}/workspaces/${name}`,
          vision: "",
          todos: [],
          colour: null,
          live: false,
          chats: open.filter((one) => one.workspace === name).map(asListed),
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
  });
  const said = async (event: string, payload: unknown) => {
    const handlers = listeners.get(event) ?? [];
    if (handlers.length === 0) throw new Error(`the window is not listening for ${event}`);
    await act(async () => {
      for (const handler of handlers)
        window.__TAURI_INTERNALS__.runCallback(handler, { event, id: 1, payload });
      await Promise.resolve();
    });
  };
  return {
    asked,
    /** Chat `asker` dispatched `task`, and the core says its rows changed. */
    dispatched: async (task: Listed) => {
      open.push(task);
      await said("plane-changed", {
        plane: PLANE,
        changes: [{ kind: "chats", workspace: null, persona: null, path: "" }],
        answers: [{ answer: "sidebar" }],
      });
    },
  };
}

const section = () => screen.findByRole("tree", { name: "Chats of this project" });

/** The window, drawn, with every chat of `open` listed. */
async function drawn(open: Listed[], attended?: (session: number) => boolean) {
  const held = core(open, attended);
  render(<App />);
  const tree = await section();
  await waitFor(() => expect(within(tree).getAllByRole("treeitem")).toHaveLength(open.length));
  return held;
}

/** The Notice that explains the chip, where one stands. */
const explained = () => document.querySelector<HTMLElement>('[data-cause="chip-explained"]');

/** It, once the window has drawn it. */
const theNotice = () =>
  waitFor(() => {
    const found = explained();
    if (found === null) throw new Error("no Notice explains the chip");
    return found;
  });

const said = (notice: HTMLElement) =>
  notice.querySelector(".notice-says")?.textContent?.replace(/\s+/g, " ").trim();

/** The sessions the panes on screen show, left to right. */
const onScreen = () =>
  screen.queryAllByTestId("pane").map((pane) => Number(pane.getAttribute("data-session")));

/** The tab called `name` on the strip. */
const tabOf = (name: string) => {
  const found = within(screen.getByRole("tablist", { name: "Tabs" }))
    .getAllByRole("tab")
    .find((one) => one.querySelector(".tab-name")?.textContent === name);
  if (found === undefined) throw new Error(`no tab is called ${name}`);
  return found;
};

/** Lets every answer the window asked for land. */
const settle = () => act(() => new Promise((done) => setTimeout(done, 50)));

const talk = () => chat(4, "alpha", { persona: "devops", label: "talk", from: taskOf(1) });
const sweep = () => chat(5, "alpha", { persona: "devops", label: "sweep", from: taskOf(1) });

beforeEach(() => {
  globalThis.localStorage.clear();
  disk.seen = [];
  launch();
  forgetKeyboard();
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

describe("the first dispatch on a machine explains the chip (#1501)", () => {
  it("says on that tab that its task works inside it, and how to see it", async () => {
    const held = await drawn([chat(1, "alpha")]);
    await settle();
    expect(explained()).toBeNull();

    await held.dispatched(talk());
    const notice = await theNotice();
    expect(said(notice)).toBe(
      "This chat started a task, and it works inside this tab. The chip beside the tab's " +
        "name says whether it is working, waiting, failed or done; press the chip to switch to it.",
    );
    // On the pane of the session that asked: the Notices of a pane are its corner's.
    expect(notice.closest(".pane-notices")).not.toBeNull();
    // A Notice is drawn, never focused: the person typing goes on typing.
    expect(notice.contains(document.activeElement)).toBe(false);
    expect(within(notice).getByRole("button", { name: "Show the task" })).toBeInTheDocument();
  });

  it("reads right for several tasks", async () => {
    await drawn([chat(1, "alpha"), talk(), sweep()]);
    const notice = await theNotice();
    expect(said(notice)).toBe(
      "This chat started 2 tasks, and they work inside this tab. The chip beside the tab's " +
        "name counts them as working, waiting, failed and done; press the chip to switch " +
        "between them.",
    );
    expect(within(notice).getByRole("button", { name: "Show the tasks" })).toBeInTheDocument();
  });

  it("is shown once and never again after Dismiss, on this machine", async () => {
    const held = await drawn([chat(1, "alpha"), talk()]);
    fireEvent.click(within(await theNotice()).getByRole("button", { name: "Dismiss" }));
    expect(explained()).toBeNull();
    await settle();
    // Kept on this machine, in the person's own layout file, under no project.
    // Added by the core under the file's lock: never a whole list written back.
    expect(held.asked.filter((one) => one.cmd === "see_on_this_machine")).toEqual([
      { cmd: "see_on_this_machine", args: { cause: "chip-explained" } },
    ]);
    expect(disk.seen).toEqual(["chip-explained"]);
  });

  it("is never shown again at a later launch once it was seen on this machine", async () => {
    // What the Dismiss above kept, as the next launch reads it from the layout file. (A test
    // never opens one project twice in one process, so the relaunch is a launch of its own.)
    disk.seen = ["chip-explained"];
    launch();
    const held = await drawn([chat(1, "alpha"), talk()]);
    await held.dispatched(sweep());
    await settle();
    expect(explained()).toBeNull();
    expect(held.asked.some((one) => one.cmd === "chat_attended")).toBe(false);
  });

  it("goes on its own action, which opens the chip's menu, and is not shown again", async () => {
    const held = await drawn([chat(1, "alpha"), talk()]);
    fireEvent.click(within(await theNotice()).getByRole("button", { name: "Show the task" }));
    expect(await screen.findByRole("menu", { name: /^Tasks of / })).toBeInTheDocument();
    expect(explained()).toBeNull();
    await settle();
    expect(held.asked.some((one) => one.cmd === "see_on_this_machine")).toBe(true);
    expect(disk.seen).toEqual(["chip-explained"]);
  });

  it("is not shown to a chat nobody is at, and waits for one a person is at", async () => {
    // Chat 1 runs with its harness's prompts off: nobody is at it. Chat 2 has a person.
    const held = await drawn(
      [chat(1, "alpha"), chat(2, "alpha"), talk(), chat(5, "alpha", { from: taskOf(2) })],
      (session) => session !== 1,
    );
    await settle();
    expect(held.asked.some((one) => one.cmd === "chat_attended")).toBe(true);
    expect(explained()).toBeNull();
    // Nothing was kept for it: the next session a person is at still has it explained.
    expect(held.asked.some((one) => one.cmd === "see_on_this_machine")).toBe(false);

    fireEvent.click(tabOf("steward 2"));
    const notice = await theNotice();
    expect(said(notice)).toMatch(/^This chat started a task/);
  });

  it("asks again whether a person is at the chat when its tasks change", async () => {
    // Nobody is at chat 1 when its first task starts; a person is by its second.
    let present = false;
    const held = await drawn([chat(1, "alpha"), talk()], () => present);
    await settle();
    expect(explained()).toBeNull();
    const asks = () => held.asked.filter((one) => one.cmd === "chat_attended").length;
    const before = asks();

    present = true;
    await held.dispatched(sweep());
    await theNotice();
    expect(asks()).toBeGreaterThan(before);
  });

  it("is not shown when another window saw it after this one launched", async () => {
    // This window read nothing at its launch; the file says it was seen since.
    const held = await drawn([chat(1, "alpha")]);
    disk.seen = ["chip-explained"];
    await held.dispatched(talk());
    await settle();
    expect(held.asked.some((one) => one.cmd === "seen_on_this_machine")).toBe(true);
    expect(explained()).toBeNull();
  });

  it("is not on a pane that shows a task", async () => {
    // The session's tab shows its task talk: talk's pane says where it came from in its
    // breadcrumb, and has nothing to explain.
    await drawn([chat(1, "alpha", { shows: 4 }), talk()]);
    await waitFor(() => expect(onScreen()).toEqual([4]));
    await settle();
    expect(explained()).toBeNull();
  });

  it("is not on a task's own tab, even one with helpers below it", async () => {
    await drawn([
      chat(1, "alpha", { in_front: false }),
      chat(4, "alpha", {
        persona: "devops",
        label: "talk",
        in_front: true,
        from: taskOf(1, { tab: true }),
      }),
      chat(7, "alpha", { persona: "devops", label: "deep", from: taskOf(4, { name: "talk" }) }),
    ]);
    await waitFor(() => expect(tabOf("talk").getAttribute("aria-selected")).toBe("true"));
    await settle();
    expect(explained()).toBeNull();
  });

  it("is not on the session's pane while its task is open beside it", async () => {
    // Split: each side says which chat it is, so the session's side has a path too.
    await drawn([
      chat(1, "alpha"),
      chat(4, "alpha", { persona: "devops", label: "talk", from: taskOf(1), beside: 1 }),
    ]);
    await waitFor(() => expect(onScreen()).toEqual([1, 4]));
    await settle();
    expect(explained()).toBeNull();
  });

  it("is not shown while the core cannot say a person is at the chat", async () => {
    // A core that answers nothing, as one that predates the question: fail closed, say nothing.
    await drawn([chat(1, "alpha"), talk()], () => null as unknown as boolean);
    await settle();
    expect(explained()).toBeNull();
  });
});
