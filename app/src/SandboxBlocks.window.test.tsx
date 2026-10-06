import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "./App";
import type { BlockReport, ChatBlocked, Moved } from "./bindings";
import { AT_MOST_PER_CHAT, blocked, putAway } from "./sandboxBlocks";

/**
 * **A sandbox block becomes a Notice on the chat's tab** (#1338), against the whole window: the
 * core sends `chat-sandbox-blocked` with the block's operation and kind, sorted by the chat's hook
 * from the harness's own violation lines (`sandboxblock_tests.rs` feeds those lines; this feeds
 * what they become). A block of purlis's own says it is a purlis bug and offers Report, whose
 * draft is shown before anything is sent and is filed only on File report.
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

const CHAT = {
  session: 4,
  name: "claude 4",
  cwd: `${ALPHA}/repo`,
  harness: "claude",
  in_front: true,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona: null,
  unreported: null,
  guessed: null,
  pinned: false,
  label: null,
  from: null,
};

/** What `purlis session record`'s violation lines become: a write to the project's files. */
const OURS: ChatBlocked = {
  plane: PLANE,
  session: 4,
  operation: "write",
  kind: "project-files",
  ours: true,
  harness: "claude",
  said: "a write to the project's own files",
  offer: "none",
  target: null,
  route: null,
  levels: [],
};

/** What `cargo build`'s refused cache write becomes: the chat's own work. */
const THEIRS: ChatBlocked = {
  ...OURS,
  kind: "toolchain-cache",
  ours: false,
  said: "a write to a toolchain's package cache",
};

/** Chat 4 mid-turn, then waiting for you: what the board says as its turn runs and ends. */
const RUNNING: Moved = {
  plane: PLANE,
  session: 4,
  state: "running",
  needs_you: false,
  queue: [],
  moved_at: 1,
  reports: [],
  refusals: [],
  children: [],
  sequence: 1,
};
const WAITING: Moved = { ...RUNNING, state: "waiting", moved_at: 2, sequence: 2 };

const DRAFT: BlockReport = {
  repository: "purlis/purlis",
  title: "Sandbox blocked purlis's own write (project-files)",
  body: "- **operation:** write\n- **kind of path or host:** project-files",
  digest: "0123456789ab",
};

function core(restart: { error?: string } = {}) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC(
    (cmd, args) => {
      asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return [CHAT];
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          personas: [],
          persona: null,
          unfiled: [],
          workspaces: [{ name: "alpha", path: ALPHA, vision: "", todos: [], chats: [CHAT] }],
        };
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "sandbox_block_report") return DRAFT;
      if (cmd === "file_sandbox_block_report") return "https://github.com/purlis/purlis/issues/9";
      if (cmd === "allow_sandbox_block")
        return {
          said: "Allowed for this chat. The chat restarts on the same conversation once its turn ends, and is told to retry.",
        };
      if (cmd === "restart_chat_for_grant") {
        if (restart.error !== undefined) throw new Error(restart.error);
        return { chat: { ...CHAT, session: 9, resumed: "c1" }, not_yet: null };
      }
      if (cmd === "restart_chat_without_sandbox") return { ...CHAT, session: 11, resumed: "c1" };
      if (cmd === "owed_restarts") return [];
      return null;
    },
    { shouldMockEvents: true },
  );
  return {
    asked: (cmd: string) => asked.filter((one) => one.cmd === cmd).map(({ args }) => args),
  };
}

async function aChat(restart: { error?: string } = {}) {
  const said = core(restart);
  render(<App />);
  // The chat is on screen: its pane is drawn.
  await screen.findByTestId("pane");
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  return said;
}

describe("a sandbox block on a chat's tab", () => {
  it("says a block of purlis's own is a purlis bug, and sends nothing until File report", async () => {
    const { asked } = await aChat();

    await act(() => emit("chat-sandbox-blocked", OURS));

    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(notice).toHaveTextContent(
      "The sandbox blocked a write to the project's own files that purlis itself ran. That is a purlis bug.",
    );
    expect(asked("sandbox_block_report")).toEqual([]);

    await userEvent.click(within(notice).getByRole("button", { name: "Report…" }));
    const draft = await screen.findByLabelText("Report draft");
    expect(draft).toHaveTextContent("Sandbox blocked purlis's own write (project-files)");
    expect(asked("sandbox_block_report")).toEqual([
      { operation: "write", kind: "project-files", harness: "claude" },
    ]);
    expect(asked("file_sandbox_block_report")).toEqual([]);

    await userEvent.click(screen.getByRole("button", { name: "File report" }));
    await waitFor(() =>
      expect(screen.getByRole("status", { name: "Sandbox block" })).toHaveTextContent(
        "Reported: https://github.com/purlis/purlis/issues/9.",
      ),
    );
    expect(asked("file_sandbox_block_report")).toEqual([
      { operation: "write", kind: "project-files", harness: "claude", digest: "0123456789ab" },
    ]);
  });

  it("files nothing when the draft is cancelled", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-sandbox-blocked", OURS));

    await userEvent.click(await screen.findByRole("button", { name: "Report…" }));
    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(screen.queryByLabelText("Report draft")).toBeNull();
    expect(asked("file_sandbox_block_report")).toEqual([]);
  });

  it("says a block of the chat's own work and offers no Report", async () => {
    await aChat();

    await act(() => emit("chat-sandbox-blocked", THEIRS));

    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(notice).toHaveTextContent("The sandbox blocked a write to a toolchain's package cache.");
    expect(within(notice).queryByRole("button", { name: "Report…" })).toBeNull();
    await userEvent.click(within(notice).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("status", { name: "Sandbox block" })).toBeNull();
  });

  it("shows the newest block and how many are behind it, and the next once it is put away", async () => {
    await aChat();
    await act(() => emit("chat-sandbox-blocked", OURS));
    await act(() => emit("chat-sandbox-blocked", THEIRS));

    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(notice).toHaveTextContent("toolchain's package cache. 1 more block behind this one.");
    await userEvent.click(within(notice).getByRole("button", { name: "Dismiss" }));

    expect(await screen.findByRole("status", { name: "Sandbox block" })).toHaveTextContent(
      "That is a purlis bug.",
    );
  });

  it("ignores a block of another project's chat", async () => {
    await aChat();

    await act(() => emit("chat-sandbox-blocked", { ...OURS, plane: "/somewhere/else" }));
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(screen.queryByRole("status", { name: "Sandbox block" })).toBeNull();
  });
});

describe("a block of the chat's own work is never a dead end (#1342)", () => {
  const HOST: ChatBlocked = {
    ...THEIRS,
    operation: "connect",
    kind: "host",
    said: "a connection to an internet host this project does not allow",
    offer: "host",
    target: "api.example.com:443",
    levels: ["chat", "you", "project"],
  };

  it("allows what it shows whole for this chat, then restarts the chat on its conversation", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-moved", WAITING));
    await act(() => emit("chat-sandbox-blocked", HOST));

    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(screen.getByText("api.example.com:443")).toBeInTheDocument();
    expect(asked("allow_sandbox_block")).toEqual([]);

    await userEvent.click(within(notice).getByRole("button", { name: "Allow for this chat" }));

    await waitFor(() =>
      expect(asked("allow_sandbox_block")).toEqual([
        { plane: PLANE, session: 4, what: "host", target: "api.example.com:443", level: "chat" },
      ]),
    );
    // Its turn has ended, so it restarts at once, in its own pane.
    await waitFor(() =>
      expect(asked("restart_chat_for_grant")).toEqual([
        { plane: PLANE, session: 4, columns: 80, rows: 24 },
      ]),
    );
    expect(await screen.findByText("session 9")).toBeInTheDocument();
  });

  it("Keep blocked puts it away and allows nothing", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-sandbox-blocked", HOST));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    await userEvent.click(within(notice).getByRole("button", { name: "Keep blocked" }));
    await waitFor(() => expect(screen.queryByRole("status", { name: "Sandbox block" })).toBeNull());
    expect(asked("allow_sandbox_block")).toEqual([]);
    expect(asked("restart_chat_for_grant")).toEqual([]);
  });

  it("offers Always for every chat here on this machine, or everyone in the project", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-sandbox-blocked", HOST));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });

    await userEvent.click(within(notice).getByRole("button", { name: "Always allow…" }));
    expect(
      await screen.findByRole("button", { name: "Allow for me on this machine" }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Allow for everyone in this project" }),
    );
    await waitFor(() =>
      expect(asked("allow_sandbox_block")).toEqual([
        { plane: PLANE, session: 4, what: "host", target: "api.example.com:443", level: "project" },
      ]),
    );
  });

  it("offers a folder for this machine, never for the project, and says it whole", async () => {
    await aChat();
    await act(() =>
      emit("chat-sandbox-blocked", {
        ...THEIRS,
        offer: "write",
        target: "/Users/dev/.cache/cargo",
        levels: ["chat", "you"],
      }),
    );
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(screen.getByText("/Users/dev/.cache/cargo")).toBeInTheDocument();
    expect(screen.getByText(/and everything in it/)).toBeInTheDocument();
    await userEvent.click(within(notice).getByRole("button", { name: "Always allow…" }));
    expect(
      await screen.findByRole("button", { name: "Allow for me on this machine" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Allow for everyone in this project" })).toBeNull();
  });

  it("says the way that works for what is never granted, with no Allow", async () => {
    await aChat();
    await act(() =>
      emit("chat-sandbox-blocked", {
        ...THEIRS,
        kind: "project-state",
        offer: "brokered",
        route: "Use purlis's own commands.",
      }),
    );
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(notice).toHaveTextContent(
      "purlis never allows that to a chat. Use purlis's own commands.",
    );
    expect(within(notice).queryByRole("button", { name: "Allow for this chat" })).toBeNull();
  });

  it("offers the chat without the sandbox, as your choice, where purlis grants nothing", async () => {
    const { asked } = await aChat();
    await act(() =>
      emit("chat-sandbox-blocked", {
        ...THEIRS,
        kind: "home",
        offer: "unsandboxed",
        target: "/Users/dev/Library/LaunchAgents",
        route: "purlis will not let a chat write that folder.",
      }),
    );
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(notice).toHaveTextContent(
      "Only you can choose to start this chat again without the sandbox: it restarts now, even mid-turn",
    );
    // The folder is the chat's own word, drawn apart from purlis's sentence.
    expect(screen.getByText("/Users/dev/Library/LaunchAgents").tagName).toBe("CODE");
    expect(within(notice).queryByRole("button", { name: "Allow for this chat" })).toBeNull();
    await userEvent.click(
      within(notice).getByRole("button", { name: "Start without the sandbox for this chat" }),
    );
    await waitFor(() =>
      expect(asked("restart_chat_without_sandbox")).toEqual([
        { plane: PLANE, session: 4, columns: 80, rows: 24 },
      ]),
    );
    expect(await screen.findByText("session 11")).toBeInTheDocument();
    expect(asked("allow_sandbox_block")).toEqual([]);
  });

  it("restarts a chat owed one only once its turn has ended, even after the Notice is gone", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-moved", RUNNING));
    await act(() => emit("chat-sandbox-blocked", HOST));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    await userEvent.click(within(notice).getByRole("button", { name: "Allow for this chat" }));
    await waitFor(() => expect(asked("allow_sandbox_block")).toHaveLength(1));
    // Put away while the chat is mid-turn: nothing restarts yet, and nothing is lost.
    await userEvent.click(within(notice).getByRole("button", { name: "Dismiss" }));
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(asked("restart_chat_for_grant")).toEqual([]);

    await act(() => emit("chat-moved", WAITING));
    await waitFor(() => expect(asked("restart_chat_for_grant")).toHaveLength(1));
    expect(await screen.findByText("session 9")).toBeInTheDocument();
  });
});

describe("a block policy forbids offers nothing it forbids and says who forbade it (#1343)", () => {
  const LOCKED = "Locked by policy, set by Platform team in /etc/purlis/policy.json.";
  const HOST: ChatBlocked = {
    ...THEIRS,
    operation: "connect",
    kind: "host",
    said: "a connection to an internet host this project does not allow",
    offer: "host",
    target: "api.example.com:443",
    levels: ["chat", "you", "project"],
  };

  it("offers Allow only at the levels policy leaves open", async () => {
    await aChat();
    await act(() => emit("chat-sandbox-blocked", { ...HOST, levels: ["project"] }));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(within(notice).queryByRole("button", { name: "Allow for this chat" })).toBeNull();
    await userEvent.click(within(notice).getByRole("button", { name: "Always allow…" }));
    expect(
      await screen.findByRole("button", { name: "Allow for everyone in this project" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Allow for me on this machine" })).toBeNull();
  });

  it("offers no Allow and no Start without the sandbox, and names the policy and its owner", async () => {
    const { asked } = await aChat();
    await act(() =>
      emit("chat-sandbox-blocked", {
        ...HOST,
        offer: "policy",
        levels: [],
        route: `api.example.com:443 is not a host policy allows. ${LOCKED} Policy forbids starting this chat without the sandbox too.`,
      }),
    );
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(notice).toHaveTextContent("is not a host policy allows");
    expect(notice).toHaveTextContent(LOCKED);
    for (const name of [
      "Allow for this chat",
      "Always allow…",
      "Start without the sandbox for this chat",
    ])
      expect(within(notice).queryByRole("button", { name })).toBeNull();
    // Never a dead end in silence: it can still be put away, and nothing was allowed.
    expect(within(notice).getByRole("button", { name: "Dismiss" })).toBeInTheDocument();
    expect(asked("allow_sandbox_block")).toEqual([]);
    expect(asked("restart_chat_without_sandbox")).toEqual([]);
  });

  it("offers Start without the sandbox, saying why, where policy forbids only the Allow", async () => {
    await aChat();
    await act(() =>
      emit("chat-sandbox-blocked", {
        ...THEIRS,
        kind: "home",
        offer: "unsandboxed",
        target: "/opt/cache",
        route: `Policy forbids allowing a chat to write a folder. ${LOCKED}`,
      }),
    );
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(notice).toHaveTextContent(LOCKED);
    expect(
      within(notice).getByRole("button", { name: "Start without the sandbox for this chat" }),
    ).toBeInTheDocument();
  });
});

describe("a restart a chat is owed for a grant (#1342)", () => {
  const HOST: ChatBlocked = {
    ...THEIRS,
    operation: "connect",
    kind: "host",
    offer: "host",
    target: "api.example.com:443",
    levels: ["chat", "you", "project"],
  };

  it("waits for the person where the harness says nothing of its turns", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-sandbox-blocked", HOST));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    await userEvent.click(within(notice).getByRole("button", { name: "Allow for this chat" }));
    const restart = await screen.findByRole("status", { name: "Restart" });
    expect(restart).toHaveTextContent("restart it when you are ready");
    expect(asked("restart_chat_for_grant")).toEqual([]);
    await userEvent.click(within(restart).getByRole("button", { name: "Restart now" }));
    expect(await screen.findByText("session 9")).toBeInTheDocument();
  });

  it("says a failed restart on the chat's pane, with Restart now", async () => {
    await aChat({ error: "purlis did not restart chat 4: it has no conversation to resume yet." });
    await act(() => emit("chat-moved", WAITING));
    await act(() => emit("chat-sandbox-blocked", HOST));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    await userEvent.click(within(notice).getByRole("button", { name: "Allow for this chat" }));
    const trouble = await screen.findByRole("status", { name: "Restart" });
    await waitFor(() => expect(trouble).toHaveTextContent("no conversation to resume yet"));
    expect(within(trouble).getByRole("button", { name: "Restart now" })).toBeInTheDocument();
  });
});

describe("the blocks a window holds", () => {
  it("holds a pattern once per chat, the newest last, within the bound", () => {
    let held = blocked({}, OURS);
    held = blocked(held, THEIRS);
    held = blocked(held, OURS);
    expect(held[4]).toEqual([THEIRS, OURS]);
    for (const kind of ["home", "temp", "system", "host", "local-socket", "chat-folder"])
      held = blocked(held, { ...THEIRS, kind });
    expect(held[4]).toHaveLength(AT_MOST_PER_CHAT);
    expect(putAway({ 4: [OURS] }, 4, OURS)).toEqual({});
  });
});
