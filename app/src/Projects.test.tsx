import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { Moved, OpenChat } from "./bindings";

/**
 * A window holding more than one project (ADR 0033, spec decision 23).
 *
 * **The thing being pinned is that switching is navigation and not a teardown.** The operator
 * asked for Zed's project tabs by name and gave the reason: eight projects is eight things to
 * arrange. The reason he wanted one window PER project before that was the same one — fifty
 * chats in project A must not be torn down because he glanced at project B — so a window that
 * merged them by closing one would be the feature with its point removed.
 *
 * Everything here drives the real `App`, because the extraction it is about is `App`'s: until
 * `PlaneView` existed, a second project had nowhere to put its tabs, its sidebar or its chat
 * states, and the window could draw exactly one.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
});

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

const ONE = "/home/dev/one";
const TWO = "/home/dev/two";

/** A chat the core says a project has open. */
function chat(one: Partial<OpenChat> & { session: number; name: string }): OpenChat {
  return {
    cwd: null,
    // No harness and no persona, so its tab is its own name alone and the assertions below read
    // the names they gave it (the default before the name is charter-app#254's, tested there).
    harness: null,
    in_front: true,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
    pinned: false,
    label: null,
    from: null,
    ...one,
  };
}

/**
 * What the core says a project's plane holds.
 *
 * **Both projects have an `alpha`**, and that is the point rather than laziness: a workspace
 * name is half an answer, and the panels used to resolve their project out of the process's
 * working directory. A fixture where only one project had the name could not tell a panel
 * asking about the right project from one asking about the only project that had it.
 */
function sidebarOf(root: string) {
  return {
    root,
    workspaces: [
      { name: "alpha", path: `${root}/workspaces/alpha`, vision: "Ship it", todos: [], chats: [] },
    ],
    personas: [],
    persona: null,
    unfiled: [],
  };
}

/**
 * A core holding whatever projects a test names, with every call kept.
 *
 * `chats` says what each project has open, keyed by its root — which is the whole point of
 * these tests: every project numbers its chats from one, so two projects each holding a chat
 * `1` is the shape a window can be wrong about.
 */
function core(
  over: {
    launch?: string | null;
    restore?: { planes: string[]; active: number | null; dropped: string[] } | null;
    chats?: Record<string, OpenChat[]>;
  } = {},
) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  // Every handler, not the last one: `chat-moved` is emitted on the APP and each project
  // listens for it, so a test that fired one handler would fire whichever project happened to
  // mount last — and the whole question here is what the OTHER project does with it.
  const listeners = new Map<string, number[]>();
  const chats = over.chats ?? {};
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plugin:event|listen") {
      const { event, handler } = given as unknown as { event: string; handler: number };
      listeners.set(event, [...(listeners.get(event) ?? []), handler]);
      return 1;
    }
    const plane = given.plane as string | undefined;
    if (cmd === "plane_at_launch")
      return over.launch === undefined || over.launch === null
        ? { plane: null, from: null, why: null }
        : { plane: over.launch, from: over.launch, why: null };
    if (cmd === "planes_to_restore")
      return over.restore ?? { planes: [], active: null, dropped: [] };
    if (cmd === "open_plane") return { plane: given.path, ask: null };
    if (cmd === "plane_sidebar") return sidebarOf(plane ?? "");
    if (cmd === "opened_chats") return chats[plane ?? ""] ?? [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "recent_planes") return { planes: [], dropped: [], forgetful: null };
    return null;
  });
  return {
    asked,
    /** Fires one `chat-moved`, the way the core pushes one. */
    move(moved: Moved) {
      const handlers = listeners.get("chat-moved") ?? [];
      if (handlers.length === 0) throw new Error("the window is not listening for moves");
      for (const handler of handlers) {
        window.__TAURI_INTERNALS__.runCallback(handler, {
          event: "chat-moved",
          id: 1,
          payload: moved,
        });
      }
    },
    /** Hands the window a second launch's directory, the way the single-instance plugin does. */
    secondLaunch(cwd: string) {
      const handler = last(listeners, "open-plane");
      window.__TAURI_INTERNALS__.runCallback(handler, {
        event: "open-plane",
        id: 1,
        payload: cwd,
      });
    },
    askToQuit() {
      const handler = last(listeners, "quit-asked");
      window.__TAURI_INTERNALS__.runCallback(handler, {
        event: "quit-asked",
        id: 1,
        payload: null,
      });
    },
  };
}

/** The handler still listening for an event only the window itself listens for. React runs
 *  an effect twice under StrictMode, so the live one is the last registered. */
function last(listeners: Map<string, number[]>, event: string): number {
  const handlers = listeners.get(event) ?? [];
  if (handlers.length === 0) throw new Error(`the window is not listening for ${event}`);
  return handlers[handlers.length - 1];
}

/** The projects on the strip, and which one is in front. */
const projectTabs = () =>
  within(screen.getByRole("tablist", { name: "Projects" }))
    .getAllByRole("tab")
    .map(
      (tab) =>
        `${tab.querySelector(".project-name")?.textContent}${
          tab.getAttribute("aria-selected") === "true" ? "*" : ""
        }`,
    );

/** One project's tab on the strip, by the name it carries. Scoped, because a chat tab in the
 *  project in front can be called after the project it is in. */
function projectTab(name: string): HTMLElement {
  const tab = within(screen.getByRole("tablist", { name: "Projects" }))
    .getAllByRole("tab")
    .find((one) => one.querySelector(".project-name")?.textContent === name);
  if (!tab) throw new Error(`no project tab called ${name}`);
  return tab;
}

/** The chat tabs of the project in front. */
const chatTabs = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

/** What the chat tab called `name` says its chat is doing. */
const stateOnTab = (name: string) =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .getAllByRole("tab")
    .find((tab) => tab.querySelector(".tab-name")?.textContent === name)
    ?.querySelector("[data-state]")
    ?.getAttribute("data-state");

/** Opens a project by typing its path into the opener. */
async function openByPath(path: string) {
  const person = userEvent.setup();
  await person.type(await screen.findByLabelText("Or type a path"), path);
  await person.click(screen.getByRole("button", { name: "Open" }));
  return person;
}

describe("a window holding more than one project", () => {
  it("opens a second project as another tab, without letting go of the first", async () => {
    const { asked } = core({
      launch: ONE,
      chats: { [ONE]: [chat({ session: 1, name: "one.1" })] },
    });
    render(<App />);
    await vi.waitFor(() => expect(chatTabs()).toEqual(["one.1"]));

    await userEvent.click(screen.getByRole("button", { name: "Open a project…" }));
    await openByPath(TWO);

    await vi.waitFor(() => expect(projectTabs()).toEqual(["one", "two*"]));
    // **Nothing was closed.** A window that merged two projects by letting go of one would be
    // this feature with its reason removed.
    expect(asked.some((one) => one.cmd === "close_plane")).toBe(false);
    expect(asked.some((one) => one.cmd === "close_session")).toBe(false);
  });

  it("keeps the first project's chats running while the second is in front", async () => {
    // The operator's own reason for wanting one window per project, which project tabs have
    // to preserve rather than trade away: fifty chats in project A are not torn down because
    // he glanced at project B.
    const { asked } = core({
      launch: ONE,
      chats: {
        [ONE]: [chat({ session: 1, name: "one.1" }), chat({ session: 2, name: "one.2" })],
        [TWO]: [chat({ session: 1, name: "two.1" })],
      },
    });
    render(<App />);
    await vi.waitFor(() => expect(chatTabs()).toEqual(["one.1", "one.2"]));

    await userEvent.click(screen.getByRole("button", { name: "Open a project…" }));
    await openByPath(TWO);
    await vi.waitFor(() => expect(chatTabs()).toEqual(["two.1"]));

    expect(asked.some((one) => one.cmd === "close_session")).toBe(false);
    // And going back shows the first project's tabs again, without asking the core for them
    // a second time: they were never thrown away.
    const asksSoFar = asked.filter((one) => one.cmd === "opened_chats").length;
    await userEvent.click(projectTab("one"));
    await vi.waitFor(() => expect(chatTabs()).toEqual(["one.1", "one.2"]));
    expect(asked.filter((one) => one.cmd === "opened_chats").length).toBe(asksSoFar);
  });

  it("ignores a chat from the queue, and its project's count goes down with it (charter-app#248)", async () => {
    const { move, asked } = core({
      launch: ONE,
      chats: { [ONE]: [chat({ session: 1, name: "one.1" }), chat({ session: 2, name: "one.2" })] },
    });
    render(<App />);
    await waitFor(() => expect(chatTabs()).toEqual(["one.1", "one.2"]));
    const asking = { plane: ONE, state: "waiting", moved_at: 1, reports: [] };
    move({ ...asking, session: 1, needs_you: true, queue: [1, 2], sequence: 1 });
    move({ ...asking, session: 2, needs_you: true, queue: [1, 2], sequence: 2 });
    const count = () => projectTab("one").querySelector(".project-needs")?.textContent;
    await waitFor(() => expect(count()).toBe("2"));

    await userEvent.click(screen.getByRole("button", { name: "2 chats need you" }));
    await userEvent.click(
      within(await screen.findByRole("menu", { name: "2 chats need you" })).getByRole("button", {
        name: "Ignore one.1 until it asks again",
      }),
    );

    // The core holds the ignore, and answers it the way it answers every move.
    await vi.waitFor(() =>
      expect(asked.filter((one) => one.cmd === "ignore_needs_you").map((one) => one.args)).toEqual([
        { plane: ONE, session: 1 },
      ]),
    );
    move({ ...asking, session: 1, needs_you: false, queue: [2], sequence: 10 });
    await waitFor(() => expect(count()).toBe("1"));

    // A report the board took before the ignore, landing after it, does not bring it back.
    // The one after it is older than the ignore too, but it is the newest word about chat 2 —
    // so chat 2's tab changing is proof that both had been taken in when the count is read.
    move({ ...asking, session: 1, needs_you: true, queue: [1, 2], sequence: 3 });
    move({ ...asking, session: 2, state: "running", needs_you: false, queue: [1], sequence: 4 });
    await waitFor(() => expect(stateOnTab("one.2")).toBe("running"));
    expect(count()).toBe("1");
    expect(screen.getByRole("button", { name: "1 chat needs you" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /^Go to one\.1 / })).toBeNull();
  });

  it("goes from the title bar to a chat in a project behind the one in front (charter-app#249)", async () => {
    const { move } = core({
      launch: ONE,
      chats: {
        [ONE]: [
          chat({ session: 1, name: "one.1", in_front: false }),
          chat({ session: 2, name: "one.2", in_front: true }),
        ],
        [TWO]: [chat({ session: 1, name: "two.1" })],
      },
    });
    render(<App />);
    await waitFor(() => expect(chatTabs()).toEqual(["one.1", "one.2"]));
    await userEvent.click(screen.getByRole("button", { name: "Open a project…" }));
    await openByPath(TWO);
    await waitFor(() => expect(chatTabs()).toEqual(["two.1"]));
    // Project ONE's chat 1 asks while TWO is in front — and ONE's chat 2 was the tab in front
    // over there, so Go has to change the tab as well as the project.
    move({
      plane: ONE,
      session: 1,
      state: "waiting",
      needs_you: true,
      queue: [1],
      moved_at: 1,
      sequence: 1,
      reports: [],
    });
    const hand = await screen.findByRole("button", { name: "1 chat needs you" });
    expect(
      within(screen.getByTestId("title-bar")).getByRole("button", { name: "1 chat needs you" }),
    ).toBe(hand);

    await userEvent.click(hand);
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Go to one.1 · Outside every workspace · one" }),
    );

    await waitFor(() => expect(projectTabs()).toEqual(["one*", "two"]));
    expect(
      within(screen.getByRole("tablist", { name: "Tabs" }))
        .getByRole("tab", { selected: true })
        .querySelector(".tab-name")?.textContent,
    ).toBe("one.1");
  });

  it("draws no queue in the Attention panel any more (charter-app#249)", async () => {
    const { move } = core({ launch: ONE, chats: { [ONE]: [chat({ session: 1, name: "one.1" })] } });
    render(<App />);
    await waitFor(() => expect(chatTabs()).toEqual(["one.1"]));
    move({
      plane: ONE,
      session: 1,
      state: "waiting",
      needs_you: true,
      queue: [1],
      moved_at: 1,
      sequence: 1,
      reports: [],
    });
    await screen.findByRole("button", { name: "1 chat needs you" });

    expect(within(screen.getByTestId("panels")).queryByLabelText("Needs you")).toBeNull();
    expect(within(screen.getByTestId("panels")).queryByText(/need you/)).toBeNull();
  });

  it("says on a project's tab when a chat over there needs you", async () => {
    // The reason a project behind the one on screen goes on listening rather than being torn
    // down: an operator looking at project B has no other way to learn that A is waiting.
    const { move } = core({
      launch: ONE,
      chats: {
        [ONE]: [chat({ session: 1, name: "one.1" })],
        [TWO]: [chat({ session: 1, name: "two.1" })],
      },
    });
    render(<App />);
    await vi.waitFor(() => expect(chatTabs()).toEqual(["one.1"]));
    await userEvent.click(screen.getByRole("button", { name: "Open a project…" }));
    await openByPath(TWO);
    await vi.waitFor(() => expect(chatTabs()).toEqual(["two.1"]));

    // Project ONE's chat 1, while project TWO is on screen. Both projects have a chat 1, so
    // a window that ignored the plane on the event would mark the wrong tab.
    move({
      plane: ONE,
      session: 1,
      state: "waiting",
      needs_you: true,
      queue: [1],
      moved_at: 1,
      sequence: 1,
      reports: [],
    });

    await vi.waitFor(() =>
      expect(projectTab("one").querySelector(".project-needs")?.textContent).toBe("1"),
    );
    expect(projectTab("two").querySelector(".project-needs")).toBeNull();
  });

  it("asks about the workspace of the project in front, and not of the one it launched in", async () => {
    // The last two commands that resolved a plane out of `current_dir()`. A window showing a
    // project the launch had not opened drew the launch's `alpha` under this project's
    // heading — already wrong when #121 let a window open a second project, and plainly
    // wrong now that a window holds both at once.
    const { asked } = core({ launch: ONE });
    render(<App />);
    await vi.waitFor(() => expect(asked.some((one) => one.cmd === "workspace_panels")).toBe(true));

    await userEvent.click(screen.getByRole("button", { name: "Open a project…" }));
    await openByPath(TWO);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one", "two*"]));

    for (const cmd of ["workspace_panels", "workspace_repos"]) {
      await vi.waitFor(() =>
        expect(asked.filter((one) => one.cmd === cmd).pop()?.args).toEqual({
          plane: TWO,
          workspace: "alpha",
        }),
      );
    }
  });

  it("closes one project without disturbing the other", async () => {
    const { asked } = core({
      launch: ONE,
      chats: {
        [ONE]: [chat({ session: 1, name: "one.1" })],
        [TWO]: [chat({ session: 1, name: "two.1" })],
      },
    });
    render(<App />);
    await vi.waitFor(() => expect(chatTabs()).toEqual(["one.1"]));
    await userEvent.click(screen.getByRole("button", { name: "Open a project…" }));
    await openByPath(TWO);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one", "two*"]));

    await userEvent.click(screen.getByRole("button", { name: "Close project two" }));

    // It has a chat open, so it asks first (charter-app#239's ruling), and this answers it.

    await userEvent.click(
      within(await screen.findByRole("alertdialog")).getByRole("button", {
        name: /^Close and end/,
      }),
    );

    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));
    // The core was told to let go of THAT project and no other, and the tab beside it came
    // to the front — `closeTab`'s rule, one scope up.
    expect(asked.filter((one) => one.cmd === "close_plane").map((one) => one.args)).toEqual([
      { plane: TWO },
    ]);
    expect(chatTabs()).toEqual(["one.1"]);
  });

  it("warns about every project's chats when it is asked to quit, and tells two chat 1s apart", async () => {
    // Quit ends the process, and the process holds them all. A warning that counted only what
    // was on screen would understate what it is about to end by however many projects the
    // operator had merged into the window — and every project numbers its chats from one, so
    // the two `1`s have to be two rows.
    const { askToQuit } = core({
      launch: ONE,
      chats: {
        [ONE]: [chat({ session: 1, name: "one.1" })],
        [TWO]: [chat({ session: 1, name: "two.1" })],
      },
    });
    render(<App />);
    await vi.waitFor(() => expect(chatTabs()).toEqual(["one.1"]));
    await userEvent.click(screen.getByRole("button", { name: "Open a project…" }));
    await openByPath(TWO);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one", "two*"]));

    askToQuit();

    const dialog = within(await screen.findByRole("dialog"));
    expect(dialog.getByText(/2 sessions will be ended/)).toBeInTheDocument();
    expect(within(dialog.getByRole("list")).getByText("one.1")).toBeInTheDocument();
    expect(within(dialog.getByRole("list")).getByText("two.1")).toBeInTheDocument();
  });

  it("tells the core what it holds and which of them is in front", async () => {
    // One call, because it is one fact: the notification gate reads the front project and the
    // cold-launch restore reads the strip, and two calls could disagree about the same window.
    const { asked } = core({ launch: ONE });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));

    await userEvent.click(screen.getByRole("button", { name: "Open a project…" }));
    await openByPath(TWO);

    await vi.waitFor(() =>
      expect(asked.filter((one) => one.cmd === "window_holds_planes").pop()?.args).toEqual({
        held: { planes: [ONE, TWO], active: 1 },
      }),
    );
  });

  it("says it is holding no project in front while the opener is up", async () => {
    // `+` pressed on a window full of projects. Everything it holds is still running, and
    // none of it is on screen — so a notification about any of them is sent rather than
    // suppressed.
    const { asked } = core({ launch: ONE });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));

    await userEvent.click(screen.getByRole("button", { name: "Open a project…" }));

    await vi.waitFor(() =>
      expect(asked.filter((one) => one.cmd === "window_holds_planes").pop()?.args).toEqual({
        held: { planes: [ONE], active: null },
      }),
    );
    // And the opener says what it is, rather than telling an operator looking at one project
    // that he has not opened one yet.
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "Open another project",
    );
  });

  it("opens a second launch's directory as another tab rather than saying it cannot", async () => {
    // ADR 0033's last unspent half: a second launch hands its plane to the process already
    // running. It used to be told on screen that the window was busy with another project.
    const { secondLaunch } = core({ launch: ONE });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));

    secondLaunch(TWO);

    await vi.waitFor(() => expect(projectTabs()).toEqual(["one", "two*"]));
  });

  it("raises the project a second launch names when it is already a tab", async () => {
    // "Open it" for a project this window holds means "show me that project". The core
    // answers with the id it already has and binds nothing twice; the window brings its tab
    // to the front rather than opening a second one.
    const { secondLaunch } = core({ launch: ONE });
    render(<App />);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*"]));
    await userEvent.click(screen.getByRole("button", { name: "Open a project…" }));
    await openByPath(TWO);
    await vi.waitFor(() => expect(projectTabs()).toEqual(["one", "two*"]));

    secondLaunch(ONE);

    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*", "two"]));
  });
});

describe("the cold launch putting the last quit's projects back", () => {
  it("opens every project it remembered, on the tab that was in front", async () => {
    const { asked } = core({
      launch: null,
      restore: { planes: [ONE, TWO], active: 1, dropped: [] },
    });

    render(<App />);

    await vi.waitFor(() => expect(projectTabs()).toEqual(["one", "two*"]));
    // Through `open_plane`, which is the trust gate — never a path of its own. A restore that
    // opened these itself would be ADR 0035 turned off for every project the operator had
    // ever had open at once.
    expect(asked.filter((one) => one.cmd === "open_plane").map((one) => one.args.path)).toEqual([
      ONE,
      TWO,
    ]);
  });

  it("says which project it could not take back, and opens the rest", async () => {
    // ADR 0033: a project that has moved or gone is dropped with a line saying so, never an
    // error dialog. A restore is a convenience, and one that blocks the launch is worse than
    // the thing it was restoring.
    core({
      launch: null,
      restore: {
        planes: [TWO],
        active: 0,
        dropped: ["~/dev/gone is no longer there"],
      },
    });

    render(<App />);

    expect(await screen.findByText(/~\/dev\/gone is no longer there/)).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await vi.waitFor(() => expect(projectTabs()).toEqual(["two*"]));
  });

  it("leaves the launch's own project in front of everything it put back", async () => {
    // Running `charter` inside a project is the operator saying which project he means, and
    // it outranks an arrangement from yesterday. The rest still come back as tabs.
    core({ launch: ONE, restore: { planes: [TWO], active: 0, dropped: [] } });

    render(<App />);

    await vi.waitFor(() => expect(projectTabs()).toEqual(["one*", "two"]));
  });

  it("does not write the arrangement back before it has finished reading it", async () => {
    // The store the restore reads from is the store the window writes to. A window that
    // reported "I hold nothing" on its first render would wipe the very record it is about to
    // read — so nothing is said about the arrangement until the restore is done.
    const { asked } = core({
      launch: null,
      restore: { planes: [ONE, TWO], active: 0, dropped: [] },
    });

    render(<App />);

    await vi.waitFor(() =>
      expect(asked.some((one) => one.cmd === "window_holds_planes")).toBe(true),
    );
    const first = asked.filter((one) => one.cmd === "window_holds_planes")[0];
    expect(first.args).toEqual({ held: { planes: [ONE, TWO], active: 0 } });
  });

  it("comes up on the opener when there is nothing to put back", async () => {
    core({ launch: null, restore: { planes: [], active: null, dropped: [] } });

    render(<App />);

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "You have not opened a project yet",
    );
    expect(screen.queryByRole("tablist", { name: "Projects" })).not.toBeInTheDocument();
  });
});
