import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
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
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "./App";
import type { ByHand, OpenChat } from "./bindings";
import { onAMac } from "./tabKeys";

/**
 * A plain shell tab, and what the window does when a harness is started by hand inside one
 * (SI-5, ADR 0062) — against the whole app, because it is four surfaces agreeing: the palette,
 * the key, the strip that files the tab and the pane that draws the banner.
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
    {
      name: "codex",
      kind: "codex",
      shown: "codex",
      source: "built-in",
      is_default: false,
      approval: null,
    },
  ],
  refused: [],
  personas: [],
  persona: null,
  ignore_fix: null,
  declares_none: true,
};

function opened(session: number, name: string, cwd: string | null, on: Partial<OpenChat> = {}) {
  return {
    session,
    name,
    cwd,
    harness: null,
    in_front: false,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
    pinned: false,
    label: null,
    from: null,
    ...on,
  };
}

/** The core, filing each chat by the directory it works in, as the real one does. */
function core(put: ReturnType<typeof opened>[] = []) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  const chats = [...put];
  let next = Math.max(0, ...chats.map((one) => one.session));
  mockIPC(
    (cmd, args) => {
      asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return put;
      if (cmd === "open_session") {
        const { cwd, name } = args as { cwd: string | null; name: string };
        chats.push(opened(++next, name, cwd));
        return next;
      }
      if (cmd === "start_chat") {
        const { cwd, name, profile } = args as {
          cwd: string | null;
          name: string;
          profile: string;
        };
        chats.push(opened(++next, name, cwd, { harness: profile, profile }));
        return { session: next, label: null };
      }
      if (cmd === "plane_pins")
        return { project: false, workspaces: ["alpha", "beta"], missing: [] };
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          personas: [],
          persona: null,
          unfiled: chats.filter((one) => one.cwd === null || !one.cwd.startsWith(`${PLANE}/`)),
          workspaces: [
            {
              name: "alpha",
              path: ALPHA,
              vision: "",
              todos: [],
              chats: chats.filter((one) => one.cwd?.startsWith(ALPHA)),
            },
            {
              name: "beta",
              path: BETA,
              vision: "",
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
    },
    { shouldMockEvents: true },
  );
  return {
    opens: () => asked.filter(({ cmd }) => cmd === "open_session").map(({ args }) => args),
    starts: () => asked.filter(({ cmd }) => cmd === "start_chat").map(({ args }) => args),
  };
}

/** The chats, as the strip under the workspaces lists them. */
const chatTabs = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .queryAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

const workspaces = () =>
  within(screen.getByRole("tablist", { name: "Workspaces" }))
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(".workspace-name")?.textContent);

const focusedWorkspace = () =>
  within(screen.getByRole("tablist", { name: "Workspaces" }))
    .getAllByRole("tab")
    .filter((tab) => tab.getAttribute("aria-selected") === "true")
    .map((tab) => tab.querySelector(".workspace-name")?.textContent);

/** The mark a shell tab wears: aria-hidden, as every tab's kind mark is, so looked up by what
 *  it says it is rather than by a name a screen reader would read twice. */
const shellMark = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .getByRole("tab")
    .querySelector('[data-mark="shell"]');

async function fromThePalette(row: string) {
  await userEvent.keyboard("{F2}");
  await screen.findByRole("dialog", { name: "Command palette" });
  await userEvent.keyboard(row);
  await userEvent.keyboard("{Enter}");
}

/** The listeners register asynchronously; give them a turn. */
async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

describe("a plain shell tab", () => {
  it("opens from the palette in the focused workspace, as a shell and not a harness", async () => {
    const { opens } = core();
    render(<App />);
    await waitFor(() => expect(workspaces()).toEqual(["alpha", "beta"]));

    await fromThePalette("New shell");

    await waitFor(() =>
      expect(opens()).toEqual([
        expect.objectContaining({ plane: PLANE, program: null, args: [], cwd: ALPHA }),
      ]),
    );
    await waitFor(() => expect(chatTabs()).toEqual(["shell 1"]));
    expect(shellMark()).not.toBeNull();
  });

  it("opens in the workspace a row names, and is filed under that workspace", async () => {
    const { opens } = core();
    render(<App />);
    await waitFor(() => expect(workspaces()).toEqual(["alpha", "beta"]));

    await fromThePalette("New shell in beta");

    await waitFor(() => expect(opens()).toEqual([expect.objectContaining({ cwd: BETA })]));
    // Filed under beta, and in front — so beta is the workspace on screen, as with any tab.
    await waitFor(() => expect(focusedWorkspace()).toEqual(["beta"]));
    expect(chatTabs()).toEqual(["shell 1"]);
  });

  it("opens from its key, wherever the keyboard is", async () => {
    const { opens } = core();
    render(<App />);
    await waitFor(() => expect(workspaces()).toEqual(["alpha", "beta"]));

    const mac = onAMac();
    fireEvent.keyDown(window, { key: "T", shiftKey: true, metaKey: mac, ctrlKey: !mac });

    await waitFor(() => expect(opens()).toEqual([expect.objectContaining({ cwd: ALPHA })]));
  });

  it("comes back from the record as a shell, with its mark", async () => {
    core([opened(4, "shell 4", ALPHA, { in_front: true })]);
    render(<App />);

    await waitFor(() => expect(chatTabs()).toEqual(["shell 4"]));
    expect(shellMark()).not.toBeNull();
  });

  it("marks no harness chat as a shell", async () => {
    core([opened(4, "1", ALPHA, { in_front: true, harness: "claude", profile: "claude" })]);
    render(<App />);

    await waitFor(() => expect(chatTabs()).toEqual(["claude 1"]));
    expect(shellMark()).toBeNull();
  });
});

describe("a harness started by hand in a shell tab", () => {
  const byHand = (over: Partial<ByHand> = {}): ByHand => ({
    plane: PLANE,
    session: 4,
    harness: "codex",
    cwd: `${ALPHA}/svc`,
    ...over,
  });

  async function aShellTab() {
    const core_ = core([opened(4, "shell 4", ALPHA, { in_front: true })]);
    render(<App />);
    await waitFor(() => expect(chatTabs()).toEqual(["shell 4"]));
    await settle();
    return core_;
  }

  it("puts a banner on that tab saying the harness runs outside session tracking", async () => {
    await aShellTab();

    await act(() => emit("harness-by-hand", byHand()));

    const banner = await screen.findByRole("status", { name: "codex started by hand" });
    expect(banner).toHaveTextContent("codex runs outside charter's session tracking");
  });

  it("opens the picker in the shell's directory, on that harness, from Open as chat", async () => {
    const { starts } = await aShellTab();
    await act(() => emit("harness-by-hand", byHand()));

    await userEvent.click(await screen.findByRole("button", { name: "Open as chat" }));
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));

    await waitFor(() =>
      expect(starts()).toEqual([
        expect.objectContaining({ profile: "codex", cwd: `${ALPHA}/svc` }),
      ]),
    );
  });

  it("goes away when it is dismissed, and nothing is started", async () => {
    const { starts } = await aShellTab();
    await act(() => emit("harness-by-hand", byHand()));

    await userEvent.click(await screen.findByRole("button", { name: "Dismiss" }));

    expect(screen.queryByRole("status", { name: "codex started by hand" })).toBeNull();
    expect(starts()).toEqual([]);
  });

  it("ignores a notice about another project's chat", async () => {
    await aShellTab();

    await act(() => emit("harness-by-hand", byHand({ plane: "/somewhere/else" })));
    await settle();

    expect(screen.queryByRole("status", { name: "codex started by hand" })).toBeNull();
  });
});
