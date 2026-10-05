import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { TheirAgentsMd } from "./bindings";
import { forgetYourEditor, setYourEditor } from "./yourEditor";

/**
 * What a chat's start hands the window to say (ADR 0085, V35): one line each, on the chat's own
 * pane, non-modal, until the operator dismisses it. Most starts have nothing to say, and then
 * nothing is drawn.
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
  forgetYourEditor();
});

const PLANE = "/home/dev/plane";

const SIDEBAR = {
  root: PLANE,
  workspaces: [
    { name: "alpha", path: `${PLANE}/workspaces/alpha`, vision: "", todos: [], chats: [] },
  ],
  personas: ["steward"],
  persona: "steward",
  unfiled: [],
};

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

const HIDDEN =
  "/home/dev/plane/workspaces/alpha/svc/AGENTS.md: not written by charter, and hidden from git " +
  "status by the /AGENTS.md line charter keeps in that repository's info/exclude.";

type Asked = { cmd: string; args: Record<string, unknown> };

/** The clone whose AGENTS.md the start names as the operator's (NO-4). */
const SVC: TheirAgentsMd = { workspace: "alpha", repo: "svc", piece: null };

function core(
  notices: string[],
  agentsMd: TheirAgentsMd[] = [],
  { refuseMove }: { refuseMove?: string } = {},
): Asked[] {
  let started = 0;
  const asked: Asked[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    if (cmd === "open_their_agents_md") return null;
    if (cmd === "move_their_agents_md_aside") {
      if (refuseMove !== undefined) throw refuseMove;
      return "AGENTS.aside.md";
    }
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "opened_chats") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "start_chat")
      return { session: ++started, label: null, notices, agents_md: agentsMd };
    return null;
  });
  return asked;
}

const startNotice = () => screen.findByRole("status", { name: "What this chat's start found" });
const named = (asked: Asked[], cmd: string) => asked.filter((one) => one.cmd === cmd);

async function openAChat() {
  await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
  const picker = await screen.findByRole("dialog", { name: "Start a chat" });
  await userEvent.click(within(picker).getByRole("button", { name: "Start" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
}

describe("a chat's start notice", () => {
  it("names a hidden AGENTS.md on the chat's pane until it is dismissed", async () => {
    core([HIDDEN]);
    render(<App />);

    await openAChat();

    const note = await screen.findByRole("status", { name: "What this chat's start found" });
    expect(note).toHaveTextContent("workspaces/alpha/svc/AGENTS.md");
    await userEvent.click(within(note).getByRole("button", { name: "Dismiss" }));
    await waitFor(() =>
      expect(
        screen.queryByRole("status", { name: "What this chat's start found" }),
      ).not.toBeInTheDocument(),
    );
  });

  it("is not drawn when the start had nothing to say", async () => {
    core([]);
    render(<App />);

    await openAChat();

    await screen.findByTestId("pane");
    expect(
      screen.queryByRole("status", { name: "What this chat's start found" }),
    ).not.toBeInTheDocument();
  });

  it("opens the operator's hidden AGENTS.md in their editor from Open file", async () => {
    setYourEditor("zed");
    const asked = core([HIDDEN], [SVC]);
    render(<App />);
    await openAChat();

    await userEvent.click(within(await startNotice()).getByRole("button", { name: "Open file" }));

    await waitFor(() =>
      expect(named(asked, "open_their_agents_md").map((one) => one.args)).toEqual([
        expect.objectContaining({ workspace: "alpha", repo: "svc", piece: null, editor: "zed" }),
      ]),
    );
  });

  it("asks before Move aside… moves anything, and moves nothing on Keep it", async () => {
    const asked = core([HIDDEN], [SVC]);
    render(<App />);
    await openAChat();

    await userEvent.click(within(await startNotice()).getByRole("button", { name: "Move aside…" }));

    const asking = await startNotice();
    expect(asking).toHaveTextContent(/AGENTS\.aside\.md/);
    expect(named(asked, "move_their_agents_md_aside")).toEqual([]);
    await userEvent.click(within(asking).getByRole("button", { name: "Keep it" }));
    expect(within(await startNotice()).getByRole("button", { name: "Move aside…" })).toBeVisible();
    await new Promise((settle) => setTimeout(settle, 20));
    expect(named(asked, "move_their_agents_md_aside")).toEqual([]);
  });

  it("moves the file aside on the second press, and says where it went", async () => {
    const asked = core([HIDDEN], [SVC]);
    render(<App />);
    await openAChat();

    await userEvent.click(within(await startNotice()).getByRole("button", { name: "Move aside…" }));
    await userEvent.click(within(await startNotice()).getByRole("button", { name: "Move aside" }));

    await waitFor(() =>
      expect(named(asked, "move_their_agents_md_aside").map((one) => one.args)).toEqual([
        expect.objectContaining({ workspace: "alpha", repo: "svc", piece: null }),
      ]),
    );
    const told = await startNotice();
    await waitFor(() => expect(told).toHaveTextContent(/moved to AGENTS\.aside\.md/));
    expect(within(told).queryByRole("button", { name: "Move aside…" })).toBeNull();
    expect(within(told).getByRole("button", { name: "Dismiss" })).toBeVisible();
  });

  it("says the core's refusal when the move is refused", async () => {
    core([HIDDEN], [SVC], { refuseMove: "charter wrote it: it is not yours to move aside" });
    render(<App />);
    await openAChat();

    await userEvent.click(within(await startNotice()).getByRole("button", { name: "Move aside…" }));
    await userEvent.click(within(await startNotice()).getByRole("button", { name: "Move aside" }));

    await waitFor(async () =>
      expect(await startNotice()).toHaveTextContent("it is not yours to move aside"),
    );
  });

  it("offers no file actions when the start names no branch of the project", async () => {
    core([HIDDEN], []);
    render(<App />);
    await openAChat();

    const note = await startNotice();
    expect(within(note).queryByRole("button", { name: "Open file" })).toBeNull();
    expect(within(note).queryByRole("button", { name: "Move aside…" })).toBeNull();
  });

  it("tells apart a repo's own folder and a branch of the same name", async () => {
    const asked = core([HIDDEN], [SVC, { workspace: "alpha", repo: "svc", piece: "svc" }]);
    render(<App />);
    await openAChat();

    await userEvent.click(
      within(await startNotice()).getByRole("button", { name: "Move svc aside…" }),
    );
    await userEvent.click(within(await startNotice()).getByRole("button", { name: "Move aside" }));

    await waitFor(() =>
      expect(named(asked, "move_their_agents_md_aside").map((one) => one.args)).toEqual([
        expect.objectContaining({ repo: "svc", piece: null }),
      ]),
    );
    const left = await startNotice();
    await waitFor(() =>
      expect(within(left).queryByRole("button", { name: "Move svc aside…" })).toBeNull(),
    );
    // The one left is the branch's: its Move aside… reaches `svc/svc`, not the folder again.
    await userEvent.click(within(left).getByRole("button", { name: "Move aside…" }));
    await userEvent.click(within(await startNotice()).getByRole("button", { name: "Move aside" }));
    await waitFor(() =>
      expect(named(asked, "move_their_agents_md_aside").at(-1)?.args).toEqual(
        expect.objectContaining({ repo: "svc", piece: "svc" }),
      ),
    );
  });
});
