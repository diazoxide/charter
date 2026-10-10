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
import type { BlockReport, ChatBlocked, Moved } from "./bindings";
import { AT_MOST_HOSTS, AT_MOST_PER_CHAT, blocked, hostsOf, putAway } from "./sandboxBlocks";
import { SETTLE_MS } from "./TaskBlocksNotice";
import { onAMac } from "./tabKeys";

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
  held: false,
  ruled: null,
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

function core(
  restart: { error?: string; live?: boolean } = {},
  /** What the asks registry derives for the project, read each time it is asked (#1690). */
  asksWaiting: () => unknown[] = () => [],
) {
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
        return restart.live === true
          ? {
              said: "Allowed for me on this machine. The command that asked carries on now; nothing restarts.",
              live: true,
            }
          : {
              said: "Allowed for this chat. The chat restarts on the same conversation once its turn ends, and is told to retry.",
              live: false,
            };
      if (cmd === "restart_chat") {
        if (restart.error !== undefined) throw new Error(restart.error);
        return { chat: { ...CHAT, session: 9, resumed: "c1" }, notices: [], not_yet: null };
      }
      if (cmd === "restart_chat_without_sandbox") return { ...CHAT, session: 11, resumed: "c1" };
      if (cmd === "owed_restarts") return [];
      if (cmd === "asks_waiting") return { plane: PLANE, asks: asksWaiting() };
      return null;
    },
    { shouldMockEvents: true },
  );
  return {
    asked: (cmd: string) => asked.filter((one) => one.cmd === cmd).map(({ args }) => args),
  };
}

async function aChat(
  restart: { error?: string; live?: boolean } = {},
  asksWaiting?: () => unknown[],
) {
  const said = core(restart, asksWaiting);
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

    await userEvent.click(
      within(notice).getByRole("button", { name: "Allow for me on this machine" }),
    );

    await waitFor(() =>
      expect(asked("allow_sandbox_block")).toEqual([
        {
          plane: PLANE,
          session: 4,
          // The block it showed, so the core answers only that one (#1538).
          shown: {
            operation: "connect",
            kind: "host",
            what: "host",
            target: "api.example.com:443",
          },
          level: "you",
        },
      ]),
    );
    // Its turn has ended, so it restarts at once, in its own pane.
    await waitFor(() =>
      expect(asked("restart_chat")).toEqual([{ plane: PLANE, session: 4, columns: 80, rows: 24 }]),
    );
    expect(await screen.findByText("session 9")).toBeInTheDocument();
  });

  it("sends the host the person typed for a block that named none, with the block it showed", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-sandbox-blocked", { ...HOST, target: null }));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    // Set, not typed: jsdom gives every element a zero-size box, so the pane's resizable
    // panels take focus on every press and typed keys never reach the box. The real WebView
    // keeps focus in it.
    fireEvent.change(screen.getByRole("textbox", { name: "Host to allow" }), {
      target: { value: "api.example.com" },
    });

    await userEvent.click(
      within(notice).getByRole("button", { name: "Allow for me on this machine" }),
    );

    await waitFor(() =>
      expect(asked("allow_sandbox_block")).toEqual([
        {
          plane: PLANE,
          session: 4,
          shown: { operation: "connect", kind: "host", what: "host", target: "api.example.com" },
          level: "you",
        },
      ]),
    );
  });

  it("Keep blocked puts it away and allows nothing", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-sandbox-blocked", HOST));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    await userEvent.click(within(notice).getByRole("button", { name: "Keep blocked" }));
    await waitFor(() => expect(screen.queryByRole("status", { name: "Sandbox block" })).toBeNull());
    expect(asked("allow_sandbox_block")).toEqual([]);
    expect(asked("restart_chat")).toEqual([]);
  });

  it("offers only this chat or everyone in the project under Other scopes (#1666)", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-sandbox-blocked", HOST));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });

    await userEvent.click(within(notice).getByRole("button", { name: "Other scopes…" }));
    expect(
      await screen.findByRole("button", { name: "Allow only for this chat" }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Allow for everyone in this project" }),
    );
    await waitFor(() =>
      expect(asked("allow_sandbox_block")).toEqual([
        {
          plane: PLANE,
          session: 4,
          // The block it showed, so the core answers only that one (#1538).
          shown: {
            operation: "connect",
            kind: "host",
            what: "host",
            target: "api.example.com:443",
          },
          level: "project",
        },
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
    expect(
      within(notice).queryByRole("button", { name: "Allow for me on this machine" }),
    ).toBeNull();
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
    expect(
      within(notice).queryByRole("button", { name: "Allow for me on this machine" }),
    ).toBeNull();
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

  it("names the host a refused lookup was of, and offers no Allow for it", async () => {
    // #1663: a database client looked its host up itself, past the proxy; the host travels
    // on the block and is shown, never offered.
    await aChat();
    await act(() =>
      emit("chat-sandbox-blocked", {
        ...THEIRS,
        operation: "lookup",
        kind: "host",
        said: "a lookup of an internet host by a program that does not go through the sandbox's proxy",
        offer: "unsandboxed",
        target: "db.prod.example.com",
        route: "This program looks its host up itself.",
      }),
    );
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(notice).toHaveTextContent(
      "does not go through the sandbox's proxy: db.prod.example.com. This program looks",
    );
    expect(screen.getByText("db.prod.example.com").tagName).toBe("CODE");
    expect(
      within(notice).queryByRole("button", { name: "Allow for me on this machine" }),
    ).toBeNull();
  });

  it("restarts a chat owed one only once its turn has ended, even after the Notice is gone", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-moved", RUNNING));
    await act(() => emit("chat-sandbox-blocked", HOST));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    await userEvent.click(
      within(notice).getByRole("button", { name: "Allow for me on this machine" }),
    );
    await waitFor(() => expect(asked("allow_sandbox_block")).toHaveLength(1));
    // Put away while the chat is mid-turn: nothing restarts yet, and nothing is lost.
    await userEvent.click(within(notice).getByRole("button", { name: "Dismiss" }));
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(asked("restart_chat")).toEqual([]);

    await act(() => emit("chat-moved", WAITING));
    await waitFor(() => expect(asked("restart_chat")).toHaveLength(1));
    expect(await screen.findByText("session 9")).toBeInTheDocument();
  });
});

describe("several hosts refused at once are one Notice (#1637)", () => {
  const ON = (host: string): ChatBlocked => ({
    ...THEIRS,
    operation: "connect",
    kind: "host",
    said: "a connection to an internet host this project does not allow",
    offer: "host",
    target: host,
    levels: ["chat", "you", "project"],
  });
  const HOSTS = ["a.example.com:443", "b.example.com:443", "c.example.com:443"];
  /** Past the guard on a press just after a host joined. */
  const settle = () =>
    act(async () => {
      await new Promise((resolve) => setTimeout(resolve, SETTLE_MS + 50));
    });

  it("lists every host, allows each with one press, and restarts the chat once", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-moved", WAITING));
    for (const host of HOSTS) await act(() => emit("chat-sandbox-blocked", ON(host)));

    const notices = await screen.findAllByRole("status", { name: "Sandbox block" });
    expect(notices).toHaveLength(1);
    const [notice] = notices;
    for (const host of HOSTS) expect(screen.getByText(host).tagName).toBe("CODE");
    expect(notice).toHaveTextContent("3 hosts were refused");
    expect(notice).not.toHaveTextContent("more block");

    // A press just after a host joined allows nothing, and says why.
    await userEvent.click(
      within(notice).getByRole("button", { name: "Allow for me on this machine" }),
    );
    expect(notice).toHaveTextContent("A host joined this Notice just now, so nothing was allowed.");
    expect(asked("allow_sandbox_block")).toEqual([]);
    await settle();
    await userEvent.click(
      within(notice).getByRole("button", { name: "Allow for me on this machine" }),
    );

    await waitFor(() =>
      expect(asked("allow_sandbox_block")).toEqual(
        HOSTS.map((host) => ({
          plane: PLANE,
          session: 4,
          shown: { operation: "connect", kind: "host", what: "host", target: host },
          level: "you",
        })),
      ),
    );
    await waitFor(() => expect(asked("restart_chat")).toHaveLength(1));
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(asked("restart_chat")).toHaveLength(1);
  });

  it("guards a press after a host joins at the bound, where the first host is dropped", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-moved", WAITING));
    const full = Array.from({ length: AT_MOST_HOSTS }, (_, at) => `h${at}.example.com:443`);
    for (const host of full) await act(() => emit("chat-sandbox-blocked", ON(host)));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    await settle();
    // The ninth host drops the first, which named the block: still the same Notice, still guarded.
    await act(() => emit("chat-sandbox-blocked", ON("late.example.com:443")));
    expect(screen.queryByText(full[0])).not.toBeInTheDocument();
    expect(screen.getByText("late.example.com:443").tagName).toBe("CODE");
    await userEvent.click(
      within(notice).getByRole("button", { name: "Allow for me on this machine" }),
    );
    expect(notice).toHaveTextContent("A host joined this Notice just now, so nothing was allowed.");
    expect(asked("allow_sandbox_block")).toEqual([]);
  });

  it("keeps a host that arrives while the answer is on its way up, asking", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-moved", RUNNING));
    for (const host of HOSTS) await act(() => emit("chat-sandbox-blocked", ON(host)));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    await settle();
    await userEvent.click(
      within(notice).getByRole("button", { name: "Allow for me on this machine" }),
    );
    await act(() => emit("chat-sandbox-blocked", ON("d.example.com:443")));
    await waitFor(() => expect(asked("allow_sandbox_block")).toHaveLength(3));
    expect(
      asked("allow_sandbox_block").map((one) => (one.shown as { target: string }).target),
    ).toEqual(HOSTS);

    // The fourth is still asked, in the same Notice, with what was allowed said.
    const now = await screen.findByRole("status", { name: "Sandbox block" });
    await waitFor(() => expect(now).toHaveTextContent("Allowed already"));
    expect(screen.getByText("d.example.com:443")).toBeInTheDocument();
    await settle();
    await userEvent.click(
      within(now).getByRole("button", { name: "Allow for me on this machine" }),
    );
    await waitFor(() => expect(asked("allow_sandbox_block")).toHaveLength(4));
    expect((asked("allow_sandbox_block")[3].shown as { target: string }).target).toBe(
      "d.example.com:443",
    );
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
    expect(
      within(notice).queryByRole("button", { name: "Allow for me on this machine" }),
    ).toBeNull();
    // The one scope left is the main button, with no menu.
    expect(
      within(notice).getByRole("button", { name: "Allow for everyone in this project" }),
    ).toBeInTheDocument();
    expect(within(notice).queryByRole("button", { name: "Other scopes…" })).toBeNull();
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
      "Allow for me on this machine",
      "Other scopes…",
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
    await userEvent.click(
      within(notice).getByRole("button", { name: "Allow for me on this machine" }),
    );
    const restart = await screen.findByRole("status", { name: "Restart" });
    expect(restart).toHaveTextContent("restart it when you are ready");
    expect(asked("restart_chat")).toEqual([]);
    await userEvent.click(within(restart).getByRole("button", { name: "Restart now" }));
    expect(await screen.findByText("session 9")).toBeInTheDocument();
  });

  it("says a failed restart on the chat's pane, with Restart now", async () => {
    await aChat({ error: "purlis did not restart chat 4: it has no conversation to resume." });
    await act(() => emit("chat-moved", WAITING));
    await act(() => emit("chat-sandbox-blocked", HOST));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    await userEvent.click(
      within(notice).getByRole("button", { name: "Allow for me on this machine" }),
    );
    const trouble = await screen.findByRole("status", { name: "Restart" });
    await waitFor(() => expect(trouble).toHaveTextContent("no conversation to resume"));
    expect(within(trouble).getByRole("button", { name: "Restart now" })).toBeInTheDocument();
  });
});

describe("a new host is asked live (#1666)", () => {
  const HELD: ChatBlocked = {
    ...THEIRS,
    operation: "connect",
    kind: "host",
    said: "a connection to an internet host this project does not allow",
    offer: "host",
    target: "api.example.com:443",
    levels: ["chat", "you", "project"],
    held: true,
  };

  it("says the command waits, and Allow lets it carry on with nothing restarting", async () => {
    const { asked } = await aChat({ live: true });
    await act(() => emit("chat-moved", WAITING));
    await act(() => emit("chat-sandbox-blocked", HELD));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(notice).toHaveTextContent("purlis holds the connection while you answer");
    // The main button is this project on this machine.
    await userEvent.click(
      within(notice).getByRole("button", { name: "Allow for me on this machine" }),
    );
    await waitFor(() =>
      expect(asked("allow_sandbox_block")).toEqual([
        {
          plane: PLANE,
          session: 4,
          shown: {
            operation: "connect",
            kind: "host",
            what: "host",
            target: "api.example.com:443",
          },
          level: "you",
        },
      ]),
    );
    expect(await screen.findByText(/carries on now; nothing restarts/)).toBeInTheDocument();
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(asked("restart_chat")).toEqual([]);
  });

  it("Keep blocked answers the held connection in the core, and allows nothing", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-sandbox-blocked", HELD));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    await userEvent.click(within(notice).getByRole("button", { name: "Keep blocked" }));
    await waitFor(() =>
      expect(asked("keep_sandbox_block")).toEqual([
        {
          plane: PLANE,
          session: 4,
          shown: {
            operation: "connect",
            kind: "host",
            what: "host",
            target: "api.example.com:443",
          },
        },
      ]),
    );
    await waitFor(() => expect(screen.queryByRole("status", { name: "Sandbox block" })).toBeNull());
    expect(asked("allow_sandbox_block")).toEqual([]);
  });

  it("says what policy ruled out on the Notice", async () => {
    await aChat();
    await act(() =>
      emit("chat-sandbox-blocked", {
        ...HELD,
        held: false,
        levels: ["you"],
        ruled:
          "Policy removed Allow for this chat. Locked by policy, set by IT in /etc/purlis/policy.json.",
      }),
    );
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(notice).toHaveTextContent("Policy removed Allow for this chat");
    expect(within(notice).queryByRole("button", { name: "Other scopes…" })).toBeNull();
  });

  it("offers no Allow for a host allowed already, only Restart this chat (#1666 fold-in)", async () => {
    const { asked } = await aChat();
    await act(() =>
      emit("chat-sandbox-blocked", {
        ...HELD,
        held: false,
        offer: "allowed",
        target: "productionresultssa15.blob.core.windows.net:443",
        levels: [],
        route:
          "It is allowed already, for me on this machine. This chat started before that, so it reaches it once it restarts on the same conversation.",
      }),
    );
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    expect(notice).toHaveTextContent("It is allowed already");
    for (const name of [
      "Allow for me on this machine",
      "Allow for everyone in this project",
      "Other scopes…",
    ])
      expect(within(notice).queryByRole("button", { name })).toBeNull();
    await userEvent.click(within(notice).getByRole("button", { name: "Restart this chat" }));
    await waitFor(() => expect(asked("ask_chat_restart")).toEqual([{ plane: PLANE, session: 4 }]));
    await waitFor(() => expect(screen.queryByRole("status", { name: "Sandbox block" })).toBeNull());
    expect(asked("allow_sandbox_block")).toEqual([]);
  });

  it("closes an Allow whose host was allowed already, saying so (#1666 fold-in)", async () => {
    const { asked } = await aChat();
    await act(() => emit("chat-sandbox-blocked", { ...HELD, held: false }));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    await userEvent.click(
      within(notice).getByRole("button", { name: "Allow for me on this machine" }),
    );
    await waitFor(() => expect(asked("allow_sandbox_block")).toHaveLength(1));
    // The answer is said, and the ask is no longer offered.
    await waitFor(() =>
      expect(
        within(notice).queryByRole("button", { name: "Allow for me on this machine" }),
      ).toBeNull(),
    );
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

  it("holds the hosts of one block together, each once, within the bound (#1637)", () => {
    const on = (host: string, levels: ChatBlocked["levels"] = ["chat", "you", "project"]) => ({
      ...THEIRS,
      operation: "connect",
      kind: "host",
      offer: "host" as const,
      target: host,
      levels,
    });
    let held = blocked({}, on("a.example.com"));
    held = blocked(held, on("b.example.com", ["chat", "you"]));
    held = blocked(held, on("A.example.com:443"));
    expect(held[4]).toHaveLength(1);
    expect(hostsOf(held[4][0])).toEqual(["a.example.com", "b.example.com"]);
    // The first host names it, so its Notice stays the one drawn as hosts join.
    expect(held[4][0].target).toBe("a.example.com");
    // Allow only where every host it lists may be allowed.
    expect(held[4][0].levels).toEqual(["chat", "you"]);
    // A host the report did not name is a block of its own.
    held = blocked(held, { ...on("x"), target: null });
    expect(held[4]).toHaveLength(2);
    for (let at = 0; at < AT_MOST_HOSTS + 2; at += 1)
      held = blocked(held, on(`h${at}.example.com`));
    const hosts = held[4].find((one) => one.target !== null);
    expect(hosts !== undefined && hostsOf(hosts)).toHaveLength(AT_MOST_HOSTS);
    // When a host joined is held with the block, for the Notice's guard; a host it lists already
    // joins nothing.
    const first = blocked({}, on("a.example.com"), 1000);
    expect(first[4][0].joined).toBeUndefined();
    const second = blocked(first, on("b.example.com"), 2000);
    expect(second[4][0].joined).toBe(2000);
    expect(blocked(second, on("B.example.com."), 3000)[4][0].joined).toBe(2000);
    // Answered, it loses only the hosts the answer named.
    const shown = blocked({}, on("a.example.com"));
    const more = blocked(shown, on("b.example.com"));
    expect(putAway(more, 4, shown[4][0], true)[4]?.map(hostsOf)).toEqual([["b.example.com"]]);
    expect(putAway(more, 4, more[4][0])).toEqual({});
  });
});

describe("a refused host answered in the Inbox (#1692)", () => {
  const HOST: ChatBlocked = {
    ...THEIRS,
    operation: "connect",
    kind: "host",
    said: "a connection to an internet host this project does not allow",
    offer: "host",
    target: "api.example.com:443",
    levels: ["chat", "you", "project"],
  };
  /** The same block, as the asks registry lists it while the core holds it. */
  const ASKED = {
    session: 4,
    ask: "block:4:connect:host:api.example.com:443",
    says: "The sandbox refused api.example.com:443",
    options: [
      { id: "chat", label: "Allow for this chat", allows: true },
      { id: "keep", label: "Keep blocked", allows: false },
    ],
    source: "sandbox-host",
    chain: ["claude 4"],
    answer: {
      via: "sandbox-block",
      shown: { operation: "connect", kind: "host", what: "host", target: "api.example.com:443" },
    },
  };

  it("clears the block's Notice on the pane, and restarts the chat to take the grant", async () => {
    let held = false;
    const { asked } = await aChat({}, () => (held ? [ASKED] : []));
    // Mid-turn, so the restart waits for the turn's end and the pane is still this chat's.
    await act(() => emit("chat-moved", RUNNING));
    held = true;
    await act(() => emit("chat-sandbox-blocked", HOST));
    await screen.findByRole("status", { name: "Sandbox block" });

    fireEvent.keyDown(document.body, {
      key: "I",
      shiftKey: true,
      ...(onAMac() ? { metaKey: true } : { ctrlKey: true }),
    });
    const inbox = await screen.findByRole("tabpanel", { name: "Inbox" });
    const allow = await within(inbox).findByRole("button", { name: "Allow for this chat" });
    held = false;
    await userEvent.click(allow);

    await waitFor(() => expect(asked("allow_sandbox_block")).toHaveLength(1));
    // One answer, both places: the pane's copy is put away, and the Inbox's ask is gone.
    await waitFor(() => expect(screen.queryByRole("status", { name: "Sandbox block" })).toBeNull());
    await waitFor(() =>
      expect(within(inbox).queryByRole("button", { name: "Allow for this chat" })).toBeNull(),
    );
    // As the Notice's own Allow does: the chat is owed a restart, taken once its turn ends.
    expect(asked("restart_chat")).toEqual([]);
    await act(() => emit("chat-moved", WAITING));
    await waitFor(() =>
      expect(asked("restart_chat")).toEqual([{ plane: PLANE, session: 4, columns: 80, rows: 24 }]),
    );
  });

  it("owes the chat no restart when its proxy took the Allow live (#1666)", async () => {
    let held = false;
    const { asked } = await aChat({ live: true }, () => (held ? [ASKED] : []));
    await act(() => emit("chat-moved", RUNNING));
    held = true;
    await act(() => emit("chat-sandbox-blocked", HOST));
    await screen.findByRole("status", { name: "Sandbox block" });
    fireEvent.keyDown(document.body, {
      key: "I",
      shiftKey: true,
      ...(onAMac() ? { metaKey: true } : { ctrlKey: true }),
    });
    const inbox = await screen.findByRole("tabpanel", { name: "Inbox" });
    const allow = await within(inbox).findByRole("button", { name: "Allow for this chat" });
    held = false;
    await userEvent.click(allow);

    await waitFor(() => expect(asked("allow_sandbox_block")).toHaveLength(1));
    await waitFor(() => expect(screen.queryByRole("status", { name: "Sandbox block" })).toBeNull());
    // The command that asked carried on: the turn's end restarts nothing.
    await act(() => emit("chat-moved", WAITING));
    await new Promise((settled) => setTimeout(settled, 50));
    expect(asked("restart_chat")).toEqual([]);
  });

  it("keeps it blocked from the Inbox by the Notice's own command, which refuses what the proxy holds", async () => {
    let held = false;
    const { asked } = await aChat({}, () => (held ? [ASKED] : []));
    await act(() => emit("chat-moved", WAITING));
    held = true;
    await act(() => emit("chat-sandbox-blocked", HOST));
    await screen.findByRole("status", { name: "Sandbox block" });
    fireEvent.keyDown(document.body, {
      key: "I",
      shiftKey: true,
      ...(onAMac() ? { metaKey: true } : { ctrlKey: true }),
    });
    const inbox = await screen.findByRole("tabpanel", { name: "Inbox" });
    const keep = await within(inbox).findByRole("button", { name: "Keep blocked" });
    held = false;
    await userEvent.click(keep);

    await waitFor(() =>
      expect(asked("keep_sandbox_block")).toEqual([
        { plane: PLANE, session: 4, shown: ASKED.answer.shown },
      ]),
    );
    await waitFor(() => expect(screen.queryByRole("status", { name: "Sandbox block" })).toBeNull());
    expect(asked("restart_chat")).toEqual([]);
  });

  it("drops the Inbox's ask when the pane's Notice keeps it blocked", async () => {
    let held = false;
    await aChat({}, () => (held ? [ASKED] : []));
    await act(() => emit("chat-moved", WAITING));
    held = true;
    await act(() => emit("chat-sandbox-blocked", HOST));
    const notice = await screen.findByRole("status", { name: "Sandbox block" });
    fireEvent.keyDown(document.body, {
      key: "I",
      shiftKey: true,
      ...(onAMac() ? { metaKey: true } : { ctrlKey: true }),
    });
    const inbox = await screen.findByRole("tabpanel", { name: "Inbox" });
    await within(inbox).findByRole("button", { name: "Keep blocked" });

    held = false;
    await userEvent.click(within(notice).getByRole("button", { name: "Keep blocked" }));

    await waitFor(() =>
      expect(within(inbox).getByText("Nothing is waiting on you")).toBeInTheDocument(),
    );
  });
});
