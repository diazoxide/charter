import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { forgetThisLaunch } from "./regions";
import { dragWithTheKeyboard, laidOutInARow } from "./test-strips";

/**
 * **Dragging a tab with the keyboard** (SI-6): Shift+Space picks the focused tab up, the arrows
 * carry it along its strip, Space drops it and Escape puts it back — `dnd-kit`'s keyboard
 * sensor, on the strips the operator already walks with the arrows.
 *
 * Laid out by `laidOutInARow`, which says what is faked and why.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div className="pane" data-testid="pane">
      <textarea aria-label={`Terminal ${session}`} tabIndex={0} />
    </div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;
const BETA = `${PLANE}/workspaces/beta`;

function chat(session: number, name: string, { inFront = false, pinned = false } = {}) {
  return {
    session,
    name,
    cwd: ALPHA,
    harness: "claude",
    in_front: inFront,
    resumed: null,
    fresh: null,
    profile: "claude",
    persona: "steward",
    unreported: null,
    guessed: null,
    pinned,
  };
}

/** A plane with two pinned workspaces and three chats in the first; answers are recorded. */
function core({
  chats = [chat(1, "one"), chat(2, "two", { inFront: true }), chat(3, "three")],
  pinChat,
  pinned = ["alpha", "beta"],
  pinWorkspace,
}: {
  chats?: ReturnType<typeof chat>[];
  /** How the core answers `pin_chat`; it pins at once when this is not given. */
  pinChat?: () => Promise<unknown>;
  /** The workspaces the machine store has pinned. */
  pinned?: string[];
  /** How the core answers `pin_workspace`; nothing is written when this is not given. */
  pinWorkspace?: () => Promise<unknown>;
} = {}) {
  /** What the window sent, by command, with its arguments. */
  const said: { cmd: string; args: Record<string, unknown> }[] = [];
  /** The workspace pins, as the machine store would hold them. */
  let pins = [...pinned];
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    said.push({ cmd, args: a });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return chats;
    if (cmd === "plane_pins") return { project: false, workspaces: pins, missing: [] };
    if (cmd === "arrange_workspace_pins")
      pins = (a.workspaces as string[]).filter((name) => pins.includes(name));
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: ["steward"],
        persona: "steward",
        unfiled: [],
        workspaces: [
          { name: "alpha", path: ALPHA, vision: "", todos: [], chats },
          { name: "beta", path: BETA, vision: "", todos: [], chats: [] },
        ],
      };
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "alerts_everywhere") return [{ plane: PLANE, alerts: [], stopped: null }];
    if (cmd === "pin_chat" && pinChat !== undefined) return pinChat();
    if (cmd === "pin_workspace" && pinWorkspace !== undefined) return pinWorkspace();
    return null;
  });
  return {
    /** The last time the window said `cmd`, with what it said. */
    last: (cmd: string) => said.filter((one) => one.cmd === cmd).at(-1)?.args,
  };
}

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
  laidOutInARow();
});
afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
});

/** The names on one strip's tabs, in the order it draws them. */
/** The names on a strip — leaving out the plane root's tab, which is first on every
 *  workspace strip and can never be dragged (SI-1; asserted below). */
const namesOn = (strip: string) =>
  within(screen.getByRole("tablist", { name: strip }))
    .getAllByRole("tab")
    .filter((tab) => !tab.classList.contains("plane-root"))
    .map((tab) => tab.textContent);

/** One strip's tab whose text includes `name`. */
const tabOn = (strip: string, name: string) =>
  within(screen.getByRole("tablist", { name: strip }))
    .getAllByRole("tab")
    .find((tab) => tab.textContent?.includes(name)) as HTMLElement;

describe("dragging a chat tab with the keyboard", () => {
  it("moves it along the strip and tells the core the order to record", async () => {
    const window = core();
    render(<App />);
    await waitFor(() => expect(namesOn("Tabs")).toHaveLength(3));
    expect(namesOn("Tabs").map((name) => name?.match(/one|two|three/)?.[0])).toEqual([
      "one",
      "two",
      "three",
    ]);

    tabOn("Tabs", "three").focus();
    await dragWithTheKeyboard("{ArrowLeft}");

    await waitFor(() =>
      expect(namesOn("Tabs").map((name) => name?.match(/one|two|three/)?.[0])).toEqual([
        "one",
        "three",
        "two",
      ]),
    );
    await waitFor(() =>
      expect(window.last("chat_order")).toEqual({ plane: PLANE, sessions: [1, 3, 2] }),
    );
    // Moved, not selected: the tab in front is still the one that was.
    expect(tabOn("Tabs", "two")).toHaveAttribute("aria-selected", "true");
    // And the keyboard is still on the tab it carried.
    expect(tabOn("Tabs", "three")).toHaveFocus();
  });

  it("pins an unpinned tab carried among the pinned ones", async () => {
    const window = core({
      chats: [
        chat(1, "one", { pinned: true }),
        chat(2, "two", { inFront: true }),
        chat(3, "three"),
      ],
    });
    render(<App />);
    await waitFor(() => expect(namesOn("Tabs")).toHaveLength(3));

    tabOn("Tabs", "two").focus();
    await dragWithTheKeyboard("{ArrowLeft}");

    await waitFor(() =>
      expect(window.last("pin_chat")).toEqual({ plane: PLANE, session: 2, pinned: true }),
    );
    await waitFor(() =>
      expect(window.last("chat_order")).toEqual({ plane: PLANE, sessions: [2, 1, 3] }),
    );
  });

  it("is drawn in its new group at once, while the core has not answered the pin", async () => {
    // **The drop is the answer the operator sees** (SI-6b): a tab carried across the boundary
    // that stayed in its old group until the core wrote the pin jumped back and then forward
    // again under the pointer. The core is held here, so nothing but the drop can place it.
    let answer: (said: unknown) => void = () => undefined;
    const window = core({
      chats: [
        chat(1, "one", { pinned: true }),
        chat(2, "two", { inFront: true }),
        chat(3, "three"),
      ],
      pinChat: () => new Promise((resolve) => (answer = resolve)),
    });
    render(<App />);
    await waitFor(() => expect(namesOn("Tabs")).toHaveLength(3));

    tabOn("Tabs", "two").focus();
    await dragWithTheKeyboard("{ArrowLeft}");

    expect(window.last("pin_chat")).toEqual({ plane: PLANE, session: 2, pinned: true });
    expect(namesOn("Tabs").map((name) => name?.match(/one|two|three/)?.[0])).toEqual([
      "two",
      "one",
      "three",
    ]);
    expect(within(tabOn("Tabs", "two")).getByRole("img", { name: /^pinned / })).toBeTruthy();
    answer(null);
  });

  it("goes back to its own group, and says why, when the core refuses the pin", async () => {
    core({
      chats: [
        chat(1, "one", { pinned: true }),
        chat(2, "two", { inFront: true }),
        chat(3, "three"),
      ],
      pinChat: () => Promise.reject("The plane's record could not be written."),
    });
    render(<App />);
    await waitFor(() => expect(namesOn("Tabs")).toHaveLength(3));

    tabOn("Tabs", "two").focus();
    await dragWithTheKeyboard("{ArrowLeft}");

    expect(await screen.findByText("The plane's record could not be written.")).toBeInTheDocument();
    expect(namesOn("Tabs").map((name) => name?.match(/one|two|three/)?.[0])).toEqual([
      "one",
      "two",
      "three",
    ]);
    expect(within(tabOn("Tabs", "two")).queryByRole("img", { name: /^pinned / })).toBeNull();
  });

  it("puts it back where it was when Escape is pressed", async () => {
    const window = core();
    render(<App />);
    await waitFor(() => expect(namesOn("Tabs")).toHaveLength(3));

    tabOn("Tabs", "three").focus();
    await userEvent.keyboard("{Shift>}[Space]{/Shift}");
    await userEvent.keyboard("{ArrowLeft}");
    await userEvent.keyboard("{Escape}");

    expect(namesOn("Tabs").map((name) => name?.match(/one|two|three/)?.[0])).toEqual([
      "one",
      "two",
      "three",
    ]);
    expect(window.last("chat_order")?.sessions ?? [1, 2, 3]).toEqual([1, 2, 3]);
  });

  it("leaves Space alone on a tab that was not picked up: it still selects", async () => {
    core();
    render(<App />);
    await waitFor(() => expect(namesOn("Tabs")).toHaveLength(3));

    tabOn("Tabs", "three").focus();
    await userEvent.keyboard("[Space]");

    await waitFor(() => expect(tabOn("Tabs", "three")).toHaveAttribute("aria-selected", "true"));
  });
});

describe("dragging a workspace tab with the keyboard", () => {
  it("rearranges the pins in the machine store", async () => {
    const window = core();
    render(<App />);
    await waitFor(() => expect(namesOn("Workspaces")).toHaveLength(2));

    tabOn("Workspaces", "beta").focus();
    await dragWithTheKeyboard("{ArrowLeft}");

    await waitFor(() =>
      expect(window.last("arrange_workspace_pins")).toEqual({
        plane: PLANE,
        workspaces: ["beta", "alpha"],
      }),
    );
    await waitFor(() =>
      expect(namesOn("Workspaces").map((name) => name?.match(/alpha|beta/)?.[0])).toEqual([
        "beta",
        "alpha",
      ]),
    );
  });

  it("is drawn among the pinned at once, while the core has not answered the pin", async () => {
    // The workspace strip draws from `pinnedWorkspaces`, which the drop sets before it asks the
    // core anything: held here so it stays that way.
    let answer: (said: unknown) => void = () => undefined;
    core({ pinned: ["beta"], pinWorkspace: () => new Promise((resolve) => (answer = resolve)) });
    render(<App />);
    await waitFor(() =>
      expect(namesOn("Workspaces").map((name) => name?.match(/alpha|beta/)?.[0])).toEqual([
        "beta",
        "alpha",
      ]),
    );

    tabOn("Workspaces", "alpha").focus();
    await dragWithTheKeyboard("{ArrowLeft}");

    expect(namesOn("Workspaces").map((name) => name?.match(/alpha|beta/)?.[0])).toEqual([
      "alpha",
      "beta",
    ]);
    expect(
      within(tabOn("Workspaces", "alpha")).getByRole("img", { name: /^pinned / }),
    ).toBeTruthy();
    answer(null);
  });

  it("never moves anything in front of the plane root, nor the root itself (SI-1)", async () => {
    const window = core();
    render(<App />);
    await waitFor(() => expect(namesOn("Workspaces")).toHaveLength(2));
    const first = () =>
      within(screen.getByRole("tablist", { name: "Workspaces" })).getAllByRole("tab")[0];
    expect(first()).toHaveAttribute("aria-label", "Plane root");

    // alpha, the first named tab, carried one place left: onto the root's.
    tabOn("Workspaces", "alpha").focus();
    await dragWithTheKeyboard("{ArrowLeft}");
    expect(window.last("arrange_workspace_pins")).toBeUndefined();
    expect(first()).toHaveAttribute("aria-label", "Plane root");

    // And the root does not pick up.
    first().focus();
    await dragWithTheKeyboard("{ArrowRight}");
    expect(window.last("arrange_workspace_pins")).toBeUndefined();
    expect(first()).toHaveAttribute("aria-label", "Plane root");
  });
});

describe("dragging a project tab with the keyboard", () => {
  const OTHER = "/home/dev/other";

  /** A window holding two projects, `other` pinned; `pin_project` answers as `pinProject`. */
  function twoProjects(pinProject: () => Promise<unknown>) {
    const said: { cmd: string; args: Record<string, unknown> }[] = [];
    mockIPC((cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      said.push({ cmd, args: a });
      if (cmd === "plane_at_launch") return { plane: null, from: null, why: null };
      if (cmd === "planes_to_restore")
        return { windows: [{ planes: [OTHER, PLANE], active: 1 }], dropped: [] };
      if (cmd === "open_plane") return { plane: a.path, ask: null };
      if (cmd === "plane_sidebar")
        return { root: a.plane, personas: [], persona: null, unfiled: [], workspaces: [] };
      if (cmd === "opened_chats") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "plane_pins") return { project: a.plane === OTHER, workspaces: [], missing: [] };
      if (cmd === "pin_project") return pinProject();
      return null;
    });
    return { last: (cmd: string) => said.filter((one) => one.cmd === cmd).at(-1)?.args };
  }

  const projects = () =>
    within(screen.getByRole("tablist", { name: "Projects" }))
      .getAllByRole("tab")
      .map((tab) => tab.querySelector(".project-name")?.textContent);

  it("is drawn among the pinned at once, while the core has not answered the pin", async () => {
    let answer: (said: unknown) => void = () => undefined;
    const window = twoProjects(() => new Promise((resolve) => (answer = resolve)));
    render(<App />);
    await waitFor(() => expect(projects()).toEqual(["other", "plane"]));
    await waitFor(() =>
      expect(
        within(tabOn("Projects", "other")).queryByRole("img", { name: /^pinned / }),
      ).toBeTruthy(),
    );

    tabOn("Projects", "plane").focus();
    await dragWithTheKeyboard("{ArrowLeft}");

    expect(window.last("pin_project")).toEqual({ plane: PLANE, pinned: true });
    expect(projects()).toEqual(["plane", "other"]);
    expect(within(tabOn("Projects", "plane")).getByRole("img", { name: /^pinned / })).toBeTruthy();
    answer(null);
  });

  it("goes back to its own group, and says why, when the core refuses the pin", async () => {
    twoProjects(() => Promise.reject("charter pins at most 32 projects."));
    render(<App />);
    await waitFor(() => expect(projects()).toEqual(["other", "plane"]));
    await waitFor(() =>
      expect(
        within(tabOn("Projects", "other")).queryByRole("img", { name: /^pinned / }),
      ).toBeTruthy(),
    );

    tabOn("Projects", "plane").focus();
    await dragWithTheKeyboard("{ArrowLeft}");

    expect(await screen.findByText("charter pins at most 32 projects.")).toBeInTheDocument();
    expect(projects()).toEqual(["other", "plane"]);
    expect(within(tabOn("Projects", "plane")).queryByRole("img", { name: /^pinned / })).toBeNull();
  });
});
