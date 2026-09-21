import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { OpenChat } from "./bindings";

/**
 * The workspace axis: **projects, then workspaces, then chats** (charter ADR 0036).
 *
 * In the tmux frame the app replaces, a top-level tab WAS a workspace and the sessions lived
 * under it. The port made the top level a project and left the workspace as a heading in the
 * sidebar that nothing selected — not by decision, by transposition. These tests are the
 * decision: a strip of workspaces between the projects and the chats, and a chat strip that
 * shows the focused workspace's chats.
 *
 * Against the whole app, because the axis is three surfaces agreeing: the strip, the tabs
 * under it and the panes under those.
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

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;
const BETA = `${PLANE}/workspaces/beta`;

const START_OPTIONS = {
  profiles: [
    {
      name: "claude",
      kind: "claude",
      shown: "claude",
      source: "built-in",
      is_default: true,
      approval: null,
    },
  ],
  refused: [],
  personas: ["steward"],
  persona: "steward",
  ignore_fix: null,
  declares_none: true,
};

function chat(session: number, name: string, cwd: string | null, on: Partial<OpenChat> = {}) {
  return {
    session,
    name,
    cwd,
    harness: "claude",
    in_front: false,
    resumed: null,
    fresh: null,
    profile: "claude",
    persona: "steward",
    unreported: null,
    ...on,
  };
}

/**
 * The core, filing chats the way it really does: **by the directory each one works in**.
 *
 * Nothing on the plane records a chat, so the sidebar is the only thing relating one to a
 * workspace, and it does it by `cwd`. The mock does the same rather than being told where a
 * chat belongs — otherwise these tests would be asserting against their own bookkeeping.
 */
function core(opened: ReturnType<typeof chat>[] = []) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  const chats = [...opened];
  let next = Math.max(0, ...chats.map((one) => one.session));
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return chats.filter((one) => opened.includes(one));
    if (cmd === "start_chat") {
      const cwd = (args as { cwd: string | null }).cwd;
      const name = (args as { name: string }).name;
      chats.push(chat(++next, name, cwd));
      return { session: next, wired: null };
    }
    if (cmd === "close_session") {
      const session = (args as { session: number }).session;
      const at = chats.findIndex((one) => one.session === session);
      if (at >= 0) chats.splice(at, 1);
      return null;
    }
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: ["steward"],
        persona: "steward",
        unfiled: chats.filter((one) => one.cwd === null || !one.cwd.startsWith(`${PLANE}/`)),
        workspaces: [
          {
            name: "alpha",
            path: ALPHA,
            vision: "Ship it",
            todos: [],
            chats: chats.filter((one) => one.cwd?.startsWith(ALPHA)),
          },
          {
            name: "beta",
            path: BETA,
            vision: "Retire the importer",
            todos: [],
            chats: chats.filter((one) => one.cwd?.startsWith(BETA)),
          },
        ],
      };
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    return null;
  });
  return { asked };
}

/** The workspaces, as the strip lists them. */
const strip = () =>
  within(screen.getByRole("tablist", { name: "Workspaces" }))
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(".workspace-name")?.textContent);

const focused = () =>
  within(screen.getByRole("tablist", { name: "Workspaces" }))
    .getAllByRole("tab")
    .filter((tab) => tab.getAttribute("aria-selected") === "true")
    .map((tab) => tab.querySelector(".workspace-name")?.textContent);

/** The chats, as the strip under the workspaces lists them. */
const chatTabs = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .queryAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

const panes = () => screen.queryAllByTestId("pane").map((pane) => pane.textContent);

/** Opens a chat where the operator is standing: New tab, a row, Start. */
async function openAChat() {
  await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
  await userEvent.click(await screen.findByRole("button", { name: "Start" }));
}

/** Focuses a workspace from the strip, which is the axis. */
async function focus(workspace: string) {
  const tab = within(screen.getByRole("tablist", { name: "Workspaces" }))
    .getAllByRole("tab")
    .find((one) => one.querySelector(".workspace-name")?.textContent === workspace);
  if (!tab) throw new Error(`no ${workspace} on the strip; it lists ${strip().join(", ")}`);
  await userEvent.click(tab);
}

describe("the workspace strip", () => {
  it("lists the project's workspaces, with the first one focused", async () => {
    core();
    render(<App />);

    await vi.waitFor(() => expect(strip()).toEqual(["alpha", "beta"]));
    expect(focused()).toEqual(["alpha"]);
  });

  it("starts a chat in the focused workspace, and shows it there at once", async () => {
    // The plane is read fresh a tick later; until it answers, charter still knows where it
    // put the chat, because it chose the directory. A tab missing from the strip the
    // operator is looking at, even for a frame, is the tab going missing.
    const { asked } = core();
    render(<App />);
    await vi.waitFor(() => expect(strip()).toEqual(["alpha", "beta"]));

    await openAChat();

    expect(asked.find(({ cmd }) => cmd === "start_chat")?.args).toMatchObject({ cwd: ALPHA });
    expect(chatTabs()).toEqual(["1 steward"]);
  });

  it("shows the focused workspace's chats and no others", async () => {
    core();
    render(<App />);
    await vi.waitFor(() => expect(strip()).toEqual(["alpha", "beta"]));
    await openAChat();
    await focus("beta");
    await openAChat();

    expect(chatTabs()).toEqual(["2 steward"]);
    expect(panes()).toEqual(["session 2"]);

    await focus("alpha");

    expect(chatTabs()).toEqual(["1 steward"]);
    expect(panes()).toEqual(["session 1"]);
  });

  it("ends nothing when the operator looks at another workspace", async () => {
    // The same guarantee a project behind another one has (#125): switching is navigation,
    // never a teardown. Fifty chats in `alpha` must survive a glance at `beta`.
    const { asked } = core();
    render(<App />);
    await vi.waitFor(() => expect(strip()).toEqual(["alpha", "beta"]));
    await openAChat();

    await focus("beta");

    expect(asked.filter(({ cmd }) => cmd === "close_session")).toEqual([]);
    expect(panes()).toEqual([]);
  });

  it("says a workspace has no chats rather than leaving another workspace's on screen", async () => {
    core();
    render(<App />);
    await vi.waitFor(() => expect(strip()).toEqual(["alpha", "beta"]));
    await openAChat();

    await focus("beta");

    expect(await screen.findByText(/No chats in this workspace/)).toBeInTheDocument();
    expect(chatTabs()).toEqual([]);
  });

  it("comes back to the chat that was in front on that strip", async () => {
    core();
    render(<App />);
    await vi.waitFor(() => expect(strip()).toEqual(["alpha", "beta"]));
    await openAChat();
    await openAChat();
    // The second chat is in front in `alpha`. Go away, and come back.
    await focus("beta");
    await focus("alpha");

    expect(panes()).toEqual(["session 2"]);
  });

  it("follows a chat to its own workspace when something else brings it forward", async () => {
    // The palette and the needs-you queue both show a chat by bringing its tab to the front,
    // and that chat can be anywhere. A strip left on another workspace would be drawing a
    // pane whose tab it says is not there.
    core();
    render(<App />);
    await vi.waitFor(() => expect(strip()).toEqual(["alpha", "beta"]));
    await openAChat();
    await focus("beta");
    await openAChat();

    await userEvent.keyboard("{F2}");
    await userEvent.type(screen.getByRole("combobox"), "switch to tab 1 steward");
    await userEvent.keyboard("{Enter}");

    expect(focused()).toEqual(["alpha"]);
    expect(panes()).toEqual(["session 1"]);
  });

  it("gives the chats outside every workspace a strip of their own", async () => {
    // The sidebar has always shown them rather than dropping them. A strip per workspace has
    // to have somewhere to put them, or scoping the chats makes them unreachable — which is
    // the defect being fixed, not one to introduce.
    core([chat(9, "stray", "/tmp/elsewhere", { in_front: true })]);
    render(<App />);

    await vi.waitFor(() => expect(strip()).toEqual(["alpha", "beta", "Outside every workspace"]));
    // And the window opens on it, because that is where the chat in front is.
    expect(focused()).toEqual(["Outside every workspace"]);
    expect(chatTabs()).toEqual(["stray steward"]);
  });

  it("opens on the workspace of the chat that was in front at the last quit", async () => {
    // The record says which chat comes back in front. A strip that opened on the plane's
    // first workspace would hide it behind a strip nobody asked for.
    core([chat(5, "5", BETA, { in_front: true })]);
    render(<App />);

    await vi.waitFor(() => expect(focused()).toEqual(["beta"]));
    expect(chatTabs()).toEqual(["5 steward"]);
  });

  it("says how many chats are in a workspace that is not on screen", async () => {
    core([chat(5, "5", BETA), chat(6, "6", BETA)]);
    render(<App />);
    await vi.waitFor(() => expect(strip()).toEqual(["alpha", "beta"]));

    const beta = within(screen.getByRole("tablist", { name: "Workspaces" }))
      .getAllByRole("tab")
      .find((tab) => tab.querySelector(".workspace-name")?.textContent === "beta");

    expect(beta?.querySelector(".workspace-count")?.textContent).toBe("2");
  });
});

describe("the chat strip at fifty chats (charter-app#130)", () => {
  it("names a tab by the persona its chat adopted, not by a number alone", async () => {
    core();
    render(<App />);
    await vi.waitFor(() => expect(strip()).toEqual(["alpha", "beta"]));

    await openAChat();

    expect(chatTabs()).toEqual(["1 steward"]);
  });

  it("says on the close button that it ends the chat, because nothing else does", async () => {
    // `close_session` ends the program and takes the chat off the board. That is right, and
    // it is not changing — but the `×` read as "hide this tab", and with fifty tabs and no
    // undo an operator tidying up was ending fifty live harnesses.
    core();
    render(<App />);
    await vi.waitFor(() => expect(strip()).toEqual(["alpha", "beta"]));
    await openAChat();

    const closer = screen.getByRole("button", { name: "End chat 1 steward" });

    expect(closer).toHaveAttribute("title", expect.stringContaining("There is no undo"));
    expect(screen.queryByRole("button", { name: /Close tab/ })).toBeNull();
  });

  it("brings the tab in front into view, so a strip wider than the window is reachable", async () => {
    // The strip scrolls rather than growing past the window edge, and a chat is brought
    // forward from surfaces that are not the strip at all. jsdom has no `scrollIntoView`.
    const into = vi.fn();
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      configurable: true,
      value: into,
    });
    try {
      core();
      render(<App />);
      await vi.waitFor(() => expect(strip()).toEqual(["alpha", "beta"]));

      await openAChat();

      const front = within(screen.getByRole("tablist", { name: "Tabs" })).getByRole("tab", {
        selected: true,
      });
      expect(into.mock.instances).toContain(front);
    } finally {
      Reflect.deleteProperty(Element.prototype, "scrollIntoView");
    }
  });
});
