import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { LEAST, leastAt } from "./fits";
import { DEFAULT_TEXT } from "./textSize";
import type { Moved, OpenChat } from "./bindings";

/**
 * A show-more button carries the needs-you count of what it hides (ADR 0054, charter-app#392).
 *
 * **The operator's constraint, stated before anything else was settled: hiding a workspace
 * must never hide a chat that needs you.** A strip that draws only some of its workspaces or
 * projects puts the rest behind its show-more button, so that button is now an attention
 * surface as well as an overflow — and a defect that hides its count hides a chat that is
 * waiting. The record asks for the count to be tested in its own right, not as a side effect
 * of the tab counts, and these are those tests.
 *
 * Against the whole `App`, observed only through what the operator reads: the button's words
 * and the rows of its menu. What the strips measure is stubbed, because jsdom lays nothing
 * out (#149): each strip answers `clientWidth` by its own name, and a strip nobody sized
 * answers zero, which draws everything.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

declare global {
  interface Window {
    __TAURI_INTERNALS__: { runCallback: (id: number, payload: unknown) => void };
  }
}

/**
 * Gives each named strip room for exactly `room[name]` tabs, and every other element none.
 *
 * Read on attach and whenever a strip holds a different number of things (`fits.useRoom`),
 * so a width set before the render is the width the strip fits by. Answers the stub it
 * replaced to `restore`, so one file's widths are not every later file's.
 */
function roomFor(room: { Workspaces?: number; Projects?: number }): { restore: () => void } {
  const least = {
    Workspaces: leastAt(LEAST.workspace, DEFAULT_TEXT.window),
    Projects: leastAt(LEAST.project, DEFAULT_TEXT.window),
  };
  const was = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get(this: HTMLElement) {
      const name = this.getAttribute("aria-label");
      if (name !== "Workspaces" && name !== "Projects") return 0;
      return (room[name] ?? 0) * least[name];
    },
  });
  return {
    restore: () => {
      if (was) Object.defineProperty(HTMLElement.prototype, "clientWidth", was);
      else Reflect.deleteProperty(HTMLElement.prototype, "clientWidth");
    },
  };
}

/** A chat the core says a project has open, with everything not under test left plain. */
function chat(session: number, name: string, cwd: string | null): OpenChat {
  return {
    session,
    name,
    cwd,
    harness: null,
    in_front: session === 1,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
    pinned: false,
    label: null,
    from: null,
  };
}

/**
 * A core holding the projects a test names, each with the workspaces and chats it names.
 *
 * Chats are filed by the directory they work in, as the real core files them: nothing on the
 * plane records a chat, so the mock does not get told where one belongs either.
 */
function core(
  projects: Record<string, { workspaces: string[]; chats: OpenChat[] }>,
  restore: { planes: string[]; active: number },
) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  // Every handler: each project listens for `chat-moved` on the app, and the question is
  // what the right one does with a move about it.
  const listeners = new Map<string, number[]>();
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plugin:event|listen") {
      const { event, handler } = given as unknown as { event: string; handler: number };
      listeners.set(event, [...(listeners.get(event) ?? []), handler]);
      return 1;
    }
    const plane = (given.plane as string | undefined) ?? "";
    if (cmd === "plane_at_launch") return { plane: null, from: null, why: null };
    if (cmd === "planes_to_restore") return { ...restore, dropped: [] };
    if (cmd === "open_plane") return { plane: given.path, ask: null };
    if (cmd === "plane_sidebar") {
      const held = projects[plane];
      return {
        root: plane,
        workspaces: (held?.workspaces ?? []).map((name) => ({
          name,
          path: `${plane}/workspaces/${name}`,
          vision: "",
          todos: [],
          chats: (held?.chats ?? []).filter((one) => one.cwd === `${plane}/workspaces/${name}`),
        })),
        personas: [],
        persona: null,
        unfiled: [],
      };
    }
    if (cmd === "opened_chats") return projects[plane]?.chats ?? [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "recent_planes") return { planes: [], dropped: [], forgetful: null };
    if (cmd === "extensions_on") return [];
    return null;
  });
  return {
    asked,
    /** Fires one `chat-moved`, the way the core pushes one. */
    move(moved: Moved) {
      for (const handler of listeners.get("chat-moved") ?? [])
        window.__TAURI_INTERNALS__.runCallback(handler, {
          event: "chat-moved",
          id: 1,
          payload: moved,
        });
    },
  };
}

/** A move saying `session` in `plane` is waiting, and the queue that plane now has. */
function asking(plane: string, session: number, queue: number[], sequence: number): Moved {
  return {
    plane,
    session,
    state: "waiting",
    needs_you: queue.includes(session),
    queue,
    moved_at: sequence,
    sequence,
    reports: [],
  };
}

/** The strip's show-more button, by its words. */
const showMore = (noun: string) =>
  screen.getByRole("button", { name: new RegExp(`^Show \\d+ ${noun}s? the strip is not showing`) });

/** What the open show-more menu lists, top to bottom, by the name each row carries. */
const menuRows = (name: string) =>
  within(screen.getByRole("menu"))
    .getAllByRole("menuitem")
    .map((row) => row.querySelector(name)?.textContent);

const PLANE = "/home/dev/plane";
const at = (workspace: string) => `${PLANE}/workspaces/${workspace}`;

let room: { restore: () => void } | undefined;

afterEach(() => {
  room?.restore();
  room = undefined;
  cleanup();
  clearMocks();
});

describe("the workspace strip's show-more button", () => {
  beforeEach(() => {
    room = roomFor({ Workspaces: 1 });
  });

  /** One project with three workspaces and room to draw one: alpha, in front. */
  function threeWorkspaces() {
    return core(
      {
        [PLANE]: {
          workspaces: ["alpha", "beta", "gamma"],
          chats: [
            chat(1, "one", at("alpha")),
            chat(5, "five", at("beta")),
            chat(6, "six", at("gamma")),
          ],
        },
      },
      { planes: [PLANE], active: 0 },
    );
  }

  it("counts a chat that needs you in a workspace it is hiding", async () => {
    const { move } = threeWorkspaces();
    render(<App />);
    await waitFor(() => expect(showMore("workspace")).toBeInTheDocument());

    move(asking(PLANE, 6, [6], 1));

    expect(
      await screen.findByRole("button", {
        name: "Show 2 workspaces the strip is not showing, where 1 chat needs you",
      }),
    ).toBeInTheDocument();
  });

  it("drops the count when that chat is ignored in the needs-you menu", async () => {
    // The count is read from the same queue as the tabs', so it goes down when they do — and
    // with nothing hidden needing you, the button says only how many it hides.
    const { asked, move } = threeWorkspaces();
    render(<App />);
    await waitFor(() => expect(showMore("workspace")).toBeInTheDocument());
    move(asking(PLANE, 6, [6], 1));
    await screen.findByRole("button", {
      name: "Show 2 workspaces the strip is not showing, where 1 chat needs you",
    });

    await userEvent.click(screen.getByRole("button", { name: "1 chat needs you" }));
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("button", {
        name: "Ignore six until it asks again",
      }),
    );
    await vi.waitFor(() => expect(asked.some((one) => one.cmd === "ignore_needs_you")).toBe(true));
    // What the core answers an ignore with: the chat still waiting, and a queue without it.
    move(asking(PLANE, 6, [], 2));

    expect(
      await screen.findByRole("button", { name: "Show 2 workspaces the strip is not showing" }),
    ).toBeInTheDocument();
  });

  it("lists the workspaces that need you first", async () => {
    // beta comes before gamma on the strip, and gamma is the one asking.
    const { move } = threeWorkspaces();
    render(<App />);
    await waitFor(() => expect(showMore("workspace")).toBeInTheDocument());
    move(asking(PLANE, 6, [6], 1));

    await userEvent.click(
      await screen.findByRole("button", {
        name: "Show 2 workspaces the strip is not showing, where 1 chat needs you",
      }),
    );

    expect(menuRows(".workspace-name")).toEqual(["gamma", "beta"]);
  });

  it("lists the rest most recently active first", async () => {
    // Nothing needs you, and gamma's chat moved last: the menu's own rule (ADR 0039).
    const { move } = threeWorkspaces();
    render(<App />);
    await waitFor(() => expect(showMore("workspace")).toBeInTheDocument());
    move(asking(PLANE, 6, [], 1));

    await userEvent.click(showMore("workspace"));

    expect(menuRows(".workspace-name")).toEqual(["gamma", "beta"]);
  });

  it("adds up every hidden workspace's count", async () => {
    const { move } = threeWorkspaces();
    render(<App />);
    await waitFor(() => expect(showMore("workspace")).toBeInTheDocument());

    move(asking(PLANE, 5, [5], 1));
    move(asking(PLANE, 6, [5, 6], 2));

    expect(
      await screen.findByRole("button", {
        name: "Show 2 workspaces the strip is not showing, where 2 chats need you",
      }),
    ).toBeInTheDocument();
  });
});

describe("the project strip's show-more button", () => {
  beforeEach(() => {
    room = roomFor({ Projects: 1 });
  });

  const ONE = "/home/dev/one";
  const TWO = "/home/dev/two";
  const THREE = "/home/dev/three";

  /** Three projects and room to draw one: `one`, in front. Every project numbers its chats
   *  from one, so a count that ignored which plane a move was about would be wrong here. */
  function threeProjects() {
    const project = (root: string) => ({
      workspaces: ["alpha"],
      chats: [chat(1, `${root.split("/").pop()}.1`, `${root}/workspaces/alpha`)],
    });
    return core(
      { [ONE]: project(ONE), [TWO]: project(TWO), [THREE]: project(THREE) },
      { planes: [ONE, TWO, THREE], active: 0 },
    );
  }

  it("counts a chat that needs you in a project it is hiding", async () => {
    const { move } = threeProjects();
    render(<App />);
    await waitFor(() => expect(showMore("project")).toBeInTheDocument());

    move(asking(THREE, 1, [1], 1));

    expect(
      await screen.findByRole("button", {
        name: "Show 2 projects the strip is not showing, where 1 chat needs you",
      }),
    ).toBeInTheDocument();
  });

  it("drops the count when that chat is ignored in the needs-you menu", async () => {
    const { asked, move } = threeProjects();
    render(<App />);
    await waitFor(() => expect(showMore("project")).toBeInTheDocument());
    move(asking(THREE, 1, [1], 1));
    await screen.findByRole("button", {
      name: "Show 2 projects the strip is not showing, where 1 chat needs you",
    });

    await userEvent.click(screen.getByRole("button", { name: "1 chat needs you" }));
    await userEvent.click(
      within(await screen.findByRole("menu")).getByRole("button", {
        name: "Ignore three.1 until it asks again",
      }),
    );
    await vi.waitFor(() =>
      expect(asked.filter((one) => one.cmd === "ignore_needs_you").map((one) => one.args)).toEqual([
        { plane: THREE, session: 1 },
      ]),
    );
    move(asking(THREE, 1, [], 2));

    expect(
      await screen.findByRole("button", { name: "Show 2 projects the strip is not showing" }),
    ).toBeInTheDocument();
  });

  it("lists the projects that need you first", async () => {
    // two comes before three on the strip, and three is the one asking.
    const { move } = threeProjects();
    render(<App />);
    await waitFor(() => expect(showMore("project")).toBeInTheDocument());
    move(asking(THREE, 1, [1], 1));

    await userEvent.click(
      await screen.findByRole("button", {
        name: "Show 2 projects the strip is not showing, where 1 chat needs you",
      }),
    );

    expect(menuRows(".project-name")).toEqual(["three", "two"]);
  });

  it("with no project needing you, lists hidden projects most recently active first", async () => {
    // two comes before three on the strip, and three's chat moved last (charter#401).
    const { move } = threeProjects();
    render(<App />);
    await waitFor(() => expect(showMore("project")).toBeInTheDocument());
    move({ ...asking(TWO, 1, [], 1), state: "running" });
    move({ ...asking(THREE, 1, [], 2), state: "running" });

    await userEvent.click(showMore("project"));

    expect(menuRows(".project-name")).toEqual(["three", "two"]);
  });
});
