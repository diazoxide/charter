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
import type { BlockReport, ChatBlocked } from "./bindings";
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
};

/** What `cargo build`'s refused cache write becomes: the chat's own work. */
const THEIRS: ChatBlocked = {
  ...OURS,
  kind: "toolchain-cache",
  ours: false,
  said: "a write to a toolchain's package cache",
};

const DRAFT: BlockReport = {
  repository: "purlis/purlis",
  title: "Sandbox blocked purlis's own write (project-files)",
  body: "- **operation:** write\n- **kind of path or host:** project-files",
  digest: "0123456789ab",
};

function core() {
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
      return null;
    },
    { shouldMockEvents: true },
  );
  return {
    asked: (cmd: string) => asked.filter((one) => one.cmd === cmd).map(({ args }) => args),
  };
}

async function aChat() {
  const said = core();
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
