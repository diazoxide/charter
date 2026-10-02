import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { OpenChat } from "./bindings";

/**
 * A chat's work link, against the whole window (V60, ADR 0088 §3, §4): **Link to work item…**
 * and **Unlink work item** on the chat tab's menu and in the palette, and **Work item: `<key>`**
 * in a linked chat's tab tooltip and its pane's corner. A chat at the project root is offered
 * neither. The rules are the core's (`chat_work_link`, `chat_work_unlink`), so a refusal is its
 * sentence, unchanged.
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
const ISSUE = "github:github.com/acme/api#12";

/** One chat the core put back at a launch, working in `cwd`. */
function putBack(session: number, cwd: string): OpenChat {
  return {
    session,
    name: String(session),
    cwd,
    harness: "claude",
    in_front: true,
    resumed: null,
    fresh: null,
    profile: "claude",
    persona: "steward",
    unreported: null,
    guessed: null,
    pinned: false,
    label: null,
    from: null,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

/**
 * The core, holding one chat: filed in `alpha`, or at the project root. `chat_work_item`
 * answers what the chat is linked to, which `chat_work_link` and `chat_work_unlink` change, or
 * `refuse` is thrown the way a refusal arrives from the real one.
 */
function core({
  atRoot = false,
  linked = null,
  refuse,
}: { atRoot?: boolean; linked?: string | null; refuse?: string } = {}): { asked: Asked[] } {
  const asked: Asked[] = [];
  const chat = putBack(1, atRoot ? PLANE : `${PLANE}/workspaces/alpha`);
  let item = linked;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        workspaces: [
          {
            name: "alpha",
            path: `${PLANE}/workspaces/alpha`,
            vision: "",
            todos: [],
            chats: atRoot ? [] : [chat],
            colour: null,
          },
        ],
        personas: ["steward"],
        persona: "steward",
        unfiled: atRoot ? [chat] : [],
      };
    if (cmd === "opened_chats") return [chat];
    if (cmd === "running_sessions") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "chat_work_item") return item;
    if (cmd === "chat_work_link") {
      if (refuse !== undefined) throw refuse;
      item = String((args as { item: string }).item);
      return item;
    }
    if (cmd === "chat_work_unlink") {
      const was = item;
      item = null;
      return was;
    }
    return null;
  });
  return { asked };
}

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const theTab = () => within(strip()).getAllByRole("tab")[0];
const asked = (asks: Asked[], cmd: string) => asks.filter((one) => one.cmd === cmd);

async function fromTheMenu(row: string) {
  await waitFor(() => expect(theTab()).toBeTruthy());
  fireEvent.contextMenu(theTab());
  await userEvent.click(await screen.findByRole("menuitem", { name: row }));
}

describe("a chat's work link", () => {
  it("is linked from the tab's menu, and then shows on the tab and in the pane", async () => {
    const { asked: asks } = core();
    render(<App />);

    await fromTheMenu("Link to work item…");
    const dialog = await screen.findByRole("dialog", { name: "Link to work item" });
    await userEvent.type(within(dialog).getByLabelText("Tracker key"), ISSUE);
    await userEvent.click(within(dialog).getByRole("button", { name: "Link" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(asked(asks, "chat_work_link").map((one) => one.args)).toEqual([
      { plane: PLANE, session: 1, item: ISSUE },
    ]);
    await waitFor(() => expect(theTab().getAttribute("title")).toContain(`Work item: ${ISSUE}`));
    expect(screen.getByText(`Work item: ${ISSUE}`)).toBeTruthy();
  });

  it("is unlinked from the tab's menu", async () => {
    const { asked: asks } = core({ linked: ISSUE });
    render(<App />);
    await waitFor(() => expect(theTab().getAttribute("title")).toContain(`Work item: ${ISSUE}`));

    await fromTheMenu("Unlink work item");

    await waitFor(() => expect(asked(asks, "chat_work_unlink")).toHaveLength(1));
    await waitFor(() => expect(theTab().getAttribute("title") ?? "").not.toContain("Work item"));
    expect(screen.queryByText(`Work item: ${ISSUE}`)).not.toBeInTheDocument();
  });

  it("sends the key as it was typed, so the core refuses one with spaces", async () => {
    const { asked: asks } = core();
    render(<App />);

    await fromTheMenu("Link to work item…");
    const dialog = await screen.findByRole("dialog", { name: "Link to work item" });
    await userEvent.type(within(dialog).getByLabelText("Tracker key"), ` ${ISSUE} `);
    await userEvent.click(within(dialog).getByRole("button", { name: "Link" }));

    await waitFor(() => expect(asked(asks, "chat_work_link")).toHaveLength(1));
    expect(asked(asks, "chat_work_link")[0].args.item).toBe(` ${ISSUE} `);
  });

  it("says the core's refusal in the dialog and links nothing", async () => {
    const refused = "github:x is not a tracker key";
    core({ refuse: refused });
    render(<App />);

    await fromTheMenu("Link to work item…");
    const dialog = await screen.findByRole("dialog", { name: "Link to work item" });
    await userEvent.type(within(dialog).getByLabelText("Tracker key"), "github:x");
    await userEvent.click(within(dialog).getByRole("button", { name: "Link" }));

    expect((await within(dialog).findByRole("alert")).textContent).toBe(refused);
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(theTab().getAttribute("title") ?? "").not.toContain("Work item");
  });

  it("is reached from the palette too", async () => {
    core();
    render(<App />);
    await waitFor(() => expect(theTab()).toBeTruthy());

    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("link to work item{Enter}");

    expect(await screen.findByRole("dialog", { name: "Link to work item" })).toBeTruthy();
  });

  it("is not offered to a chat at the project root", async () => {
    core({ atRoot: true });
    render(<App />);
    await waitFor(() => expect(theTab()).toBeTruthy());

    fireEvent.contextMenu(theTab());
    await screen.findByRole("menuitem", { name: /^Rename chat/ });
    expect(screen.queryByRole("menuitem", { name: "Link to work item…" })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Unlink work item" })).not.toBeInTheDocument();
  });
});
