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
 * A chat's name, against the whole window (charter-app#254): the default `<persona> <N>`, the
 * picker's Name field, the three ways to rename a tab, and the name coming back at a relaunch.
 *
 * **What a rename changes is charter's label and nothing else.** The core is asked to hold the
 * name (`rename_chat`), and its answer is what the tab draws; the chat's own name — what its
 * harness was started with — is never sent again, so a running harness is not disturbed.
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

/** One chat the core put back at a launch. */
function putBack(session: number, name: string, label: string | null): OpenChat {
  return {
    session,
    name,
    cwd: `${PLANE}/workspaces/alpha`,
    harness: "claude",
    in_front: true,
    resumed: null,
    fresh: null,
    profile: "claude",
    persona: "steward",
    unreported: null,
    pinned: false,
    label,
    from: null,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

/**
 * The core. `rename_chat` answers the name as the core's rule holds it — trimmed, blank as
 * none — or throws `refuse`, the way a refusal arrives from the real one.
 */
function core({
  open = [],
  refuse,
}: { open?: ReturnType<typeof putBack>[]; refuse?: string } = {}): { asked: Asked[] } {
  const asked: Asked[] = [];
  let started = open.length;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "opened_chats") return open;
    if (cmd === "running_sessions") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "start_chat") {
      if (refuse !== undefined) throw refuse;
      const typed = (args as { label: string | null }).label?.trim() ?? "";
      return { session: ++started, label: typed === "" ? null : typed };
    }
    if (cmd === "rename_chat") {
      if (refuse !== undefined) throw refuse;
      const label = String((args as { label: string }).label).trim();
      return label === "" ? null : label;
    }
    return null;
  });
  return { asked };
}

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const tabNames = () =>
  within(strip())
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(".tab-name")?.textContent);
const theTab = () => within(strip()).getAllByRole("tab")[0];
const renameBox = () => within(strip()).getByRole("textbox", { name: /^Rename chat/ });
const asked = (asks: Asked[], cmd: string) => asks.filter((one) => one.cmd === cmd);

/** New tab, then Start — with `name` typed into the picker's Name field first, if given. */
async function openAChat(name?: string) {
  await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
  const picker = await screen.findByRole("dialog", { name: "Start a chat" });
  if (name !== undefined) await userEvent.type(within(picker).getByLabelText(/^Name/), name);
  await userEvent.click(within(picker).getByRole("button", { name: "Start" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
}

describe("a new chat's name", () => {
  it("is the persona and then the chat's number: `steward 1`", async () => {
    core();
    render(<App />);

    await openAChat();

    await waitFor(() => expect(tabNames()).toEqual(["steward 1"]));
  });

  it("is what was typed in the picker's Name field, and the core is told it", async () => {
    const { asked: asks } = core();
    render(<App />);

    await openAChat("billing bug");

    await waitFor(() => expect(tabNames()).toEqual(["billing bug"]));
    expect(asked(asks, "start_chat")[0].args).toMatchObject({ name: "1", label: "billing bug" });
  });

  it("is the default when the Name field is left empty, and no name is sent", async () => {
    const { asked: asks } = core();
    render(<App />);

    await openAChat();

    expect(asked(asks, "start_chat")[0].args).toMatchObject({ label: null });
  });

  it("is the harness and then the number for a chat that adopts no persona", async () => {
    core();
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
    const picker = await screen.findByRole("dialog", { name: "Start a chat" });
    await userEvent.click(within(picker).getByRole("radio", { name: "none" }));
    await userEvent.click(within(picker).getByRole("button", { name: "Start" }));

    await waitFor(() => expect(tabNames()).toEqual(["claude 1"]));
  });

  it("is refused in the picker, before anything starts, when the core refuses it", async () => {
    core({ refuse: "That name holds a control character or an invisible formatting one." });
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));
    const picker = await screen.findByRole("dialog", { name: "Start a chat" });
    await userEvent.type(within(picker).getByLabelText(/^Name/), "x");
    await userEvent.click(within(picker).getByRole("button", { name: "Start" }));

    expect(await within(picker).findByRole("alert")).toHaveTextContent(/invisible formatting/);
  });
});

describe("renaming a chat's tab", () => {
  async function oneChat() {
    const { asked: asks } = core();
    render(<App />);
    await openAChat();
    await waitFor(() => expect(tabNames()).toEqual(["steward 1"]));
    return asks;
  }

  it("opens the name for editing on a double-click, and Enter saves it", async () => {
    const asks = await oneChat();

    await userEvent.dblClick(theTab());
    const box = renameBox();
    expect(box).toHaveValue("steward 1");
    expect(box).toHaveFocus();
    await userEvent.keyboard("{Control>}a{/Control}billing bug{Enter}");

    await waitFor(() => expect(tabNames()).toEqual(["billing bug"]));
    // The session and the name, and nothing about the harness's own name.
    expect(asked(asks, "rename_chat").map((one) => one.args)).toEqual([
      { plane: PLANE, session: 1, label: "billing bug" },
    ]);
    // The keyboard is back on the tab it renamed.
    expect(theTab()).toHaveFocus();
  });

  it("leaves the name as it was on Escape, and asks the core nothing", async () => {
    const asks = await oneChat();

    await userEvent.dblClick(theTab());
    await userEvent.keyboard("{Control>}a{/Control}never mind{Escape}");

    expect(tabNames()).toEqual(["steward 1"]);
    expect(asked(asks, "rename_chat")).toEqual([]);
    expect(theTab()).toHaveFocus();
    // Escape belonged to the box, not to anything behind it.
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("saves what was typed when the box loses the keyboard", async () => {
    const asks = await oneChat();

    await userEvent.dblClick(theTab());
    await userEvent.keyboard("{Control>}a{/Control}billing bug");
    fireEvent.blur(renameBox());

    await waitFor(() => expect(tabNames()).toEqual(["billing bug"]));
    expect(asked(asks, "rename_chat")).toHaveLength(1);
  });

  it("takes the default back when the name is cleared", async () => {
    await oneChat();
    await userEvent.dblClick(theTab());
    await userEvent.keyboard("{Control>}a{/Control}billing bug{Enter}");
    await waitFor(() => expect(tabNames()).toEqual(["billing bug"]));

    await userEvent.dblClick(theTab());
    await userEvent.keyboard("{Control>}a{/Control}{Backspace}{Enter}");

    await waitFor(() => expect(tabNames()).toEqual(["steward 1"]));
  });

  it("keeps Delete and Backspace in the box: typing never ends the chat", async () => {
    const asks = await oneChat();

    await userEvent.dblClick(theTab());
    await userEvent.keyboard("{Backspace}{Backspace}{Delete}");

    expect(renameBox()).toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(asked(asks, "close_session")).toEqual([]);
  });

  it("opens from the tab's own menu", async () => {
    await oneChat();

    fireEvent.contextMenu(theTab());
    await userEvent.click(await screen.findByRole("menuitem", { name: "Rename chat steward 1…" }));

    await waitFor(() => expect(renameBox()).toHaveFocus());
    expect(renameBox()).toHaveValue("steward 1");
  });

  it("opens from the palette's row", async () => {
    await oneChat();

    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("rename chat steward 1{Enter}");

    await waitFor(() => expect(renameBox()).toHaveFocus());
  });

  it("opens with F2 on a focused tab, the platform's rename key", async () => {
    await oneChat();
    theTab().focus();

    await userEvent.keyboard("{F2}");

    await waitFor(() => expect(renameBox()).toHaveFocus());
    expect(screen.queryByRole("dialog", { name: "Command palette" })).not.toBeInTheDocument();
  });

  it("says why when the core refuses the name, and keeps the one it had", async () => {
    await oneChat();
    clearMocks();
    core({ refuse: "That name is 80 characters long, and a chat's name is at most 64." });

    await userEvent.dblClick(theTab());
    await userEvent.keyboard("{Control>}a{/Control}too long{Enter}");

    expect(await screen.findByRole("alert")).toHaveTextContent(/at most 64/);
    expect(renameBox()).toHaveAttribute("aria-invalid", "true");
    await userEvent.keyboard("{Escape}");
    expect(tabNames()).toEqual(["steward 1"]);
  });

  it("leaves the name as it was and closes when a refused name loses the keyboard", async () => {
    // A box left open behind an operator who went elsewhere holds the tab's place with
    // nothing to focus: the tab could not be clicked or reached by the arrows.
    await oneChat();
    clearMocks();
    core({ refuse: "That name is 80 characters long, and a chat's name is at most 64." });

    await userEvent.dblClick(theTab());
    await userEvent.keyboard("{Control>}a{/Control}too long");
    fireEvent.blur(renameBox());

    await waitFor(() => expect(tabNames()).toEqual(["steward 1"]));
    expect(within(strip()).queryByRole("textbox")).not.toBeInTheDocument();
  });

  it("keeps F2 typed into the box from opening the palette", async () => {
    const asks = await oneChat();

    await userEvent.dblClick(theTab());
    await userEvent.keyboard("{Control>}a{/Control}half{F2}");

    expect(screen.queryByRole("dialog", { name: "Command palette" })).not.toBeInTheDocument();
    expect(renameBox()).toHaveValue("half");
    expect(asked(asks, "rename_chat")).toEqual([]);
  });

  it("names the chat by it wherever the chat is named: the palette's rows", async () => {
    await oneChat();
    await userEvent.dblClick(theTab());
    await userEvent.keyboard("{Control>}a{/Control}billing bug{Enter}");
    await waitFor(() => expect(tabNames()).toEqual(["billing bug"]));

    // ⌘K, because the keyboard is back on the tab, where F2 renames it again.
    await userEvent.keyboard("{Meta>}k{/Meta}");
    const palette = await screen.findByRole("dialog", { name: "Command palette" });

    expect(within(palette).getByRole("option", { name: /^End chat billing bug/ })).toBeTruthy();
  });
});

describe("a chat's name at a relaunch", () => {
  it("comes back as the name it was given", async () => {
    core({ open: [putBack(3, "3", "billing bug")] });
    render(<App />);

    await waitFor(() => expect(tabNames()).toEqual(["billing bug"]));
  });

  it("comes back as the harness and then the number where it adopted no persona", async () => {
    core({ open: [{ ...putBack(3, "3", null), persona: null, profile: "work" }] });
    render(<App />);

    // The harness, not the profile: `work` is the operator's word for an account.
    await waitFor(() => expect(tabNames()).toEqual(["claude 3"]));
  });

  it("comes back as its default where none was given", async () => {
    core({ open: [putBack(3, "3", null)] });
    render(<App />);

    await waitFor(() => expect(tabNames()).toEqual(["steward 3"]));
  });
});
