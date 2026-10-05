import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { forgetThisLaunch } from "./regions";

/**
 * **The chat Notices get their way out** (NO-3, #1230), against the whole window.
 *
 * - A chat a launch could not start offers **Retry now** (the launch's own start, again) and
 *   **Forget this chat…** (its record dropped, after a question). It is kept until then on
 *   purpose: a directory that moved must never delete a chat.
 * - The notes about how a chat came back can be dismissed.
 * - A chat running on instructions the project changed since it started offers **Start fresh**,
 *   on its tab's mark and as a palette row: the same chat, started again on what is there now.
 *
 * The core is recorded answers; what is asserted is what the operator sees and what the core is
 * asked to do.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";

/** One chat as the core reports it. No persona, so its tab is its name alone. */
function chat(session: number, name: string, more: Record<string, unknown> = {}) {
  return {
    session,
    name,
    cwd: `${PLANE}/workspaces/alpha`,
    harness: null,
    in_front: session === 1,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
    guessed: null,
    card: null,
    pinned: false,
    label: null,
    from: null,
    ...more,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

/** What the core holds, and how it answers the three NO-3 commands. */
let open: ReturnType<typeof chat>[];
let waiting: [string, string][];
let updated: { session: number; files: string[] }[];
let refuse: Partial<Record<string, string>>;

function core(): Asked[] {
  const asked: Asked[] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        workspaces: [
          { name: "alpha", path: `${PLANE}/workspaces/alpha`, vision: "", todos: [], chats: open },
        ],
        personas: [],
        persona: null,
        unfiled: [],
      };
    if (cmd === "opened_chats") return open;
    if (cmd === "chat_states") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "chats_that_would_not_start") return waiting;
    if (cmd === "chats_plane_updated") return updated;
    if (cmd === "retry_chat_that_did_not_start") {
      // A refusal is a THROWN value, which is what arrives as `{ status: "error" }`.
      if (refuse[cmd] !== undefined) throw refuse[cmd];
      waiting = waiting.filter(([name]) => name !== given.name);
      const started = chat(7, given.name as string);
      open = [...open, started];
      return started;
    }
    if (cmd === "forget_chat_that_did_not_start") {
      if (refuse[cmd] !== undefined) throw refuse[cmd];
      waiting = waiting.filter(([name]) => name !== given.name);
      return null;
    }
    if (cmd === "start_chat_fresh") {
      if (refuse[cmd] !== undefined) throw refuse[cmd];
      const was = open.find((one) => one.session === given.session);
      const started = chat(9, was?.name ?? "?");
      updated = updated.filter((one) => one.session !== given.session);
      open = [...open.filter((one) => one.session !== given.session), started];
      return started;
    }
    if (cmd === "close_session") return null;
    return null;
  });
  return asked;
}

const sent = (asked: Asked[], cmd: string) =>
  asked.filter((one) => one.cmd === cmd).map((one) => one.args);
const notice = (cause: string) => document.querySelector(`[data-cause="${cause}"]`);
const tabNames = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .getAllByRole("tab")
    .filter((tab) => !tab.classList.contains("plane-root"))
    .map((tab) => tab.querySelector(".tab-name")?.textContent);

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
  open = [chat(1, "one")];
  waiting = [];
  updated = [];
  refuse = {};
});

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("a chat that did not start", () => {
  beforeEach(() => {
    waiting = [["ide", "no such directory: /home/dev/gone"]];
  });

  it("is started by Retry now, and its tab opens", async () => {
    const asked = core();
    render(<App />);
    const line = await screen.findByText(/did not start/);
    const said = line.closest("[data-cause]") as HTMLElement;

    await userEvent.click(within(said).getByRole("button", { name: "Retry now" }));

    await waitFor(() => expect(tabNames()).toContain("ide"));
    expect(sent(asked, "retry_chat_that_did_not_start")).toEqual([
      expect.objectContaining({ plane: PLANE, name: "ide" }),
    ]);
    expect(notice("chat-did-not-start:ide")).toBeNull();
  });

  it("stays, saying why, when Retry now is refused", async () => {
    refuse.retry_chat_that_did_not_start = "no such directory: /home/dev/still-gone";
    core();
    render(<App />);
    const said = (await screen.findByText(/did not start/)).closest("[data-cause]") as HTMLElement;

    await userEvent.click(within(said).getByRole("button", { name: "Retry now" }));

    await waitFor(() =>
      expect(notice("chat-did-not-start:ide")?.textContent).toContain("/home/dev/still-gone"),
    );
    expect(tabNames()).not.toContain("ide");
  });

  it("is forgotten only once the question is answered yes", async () => {
    const asked = core();
    render(<App />);
    const said = (await screen.findByText(/did not start/)).closest("[data-cause]") as HTMLElement;

    await userEvent.click(within(said).getByRole("button", { name: "Forget this chat…" }));
    const question = await screen.findByRole("alertdialog");
    expect(question.textContent).toContain("ide");
    await userEvent.click(within(question).getByRole("button", { name: "Cancel" }));

    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(sent(asked, "forget_chat_that_did_not_start")).toEqual([]);
    expect(notice("chat-did-not-start:ide")).not.toBeNull();

    await userEvent.click(within(said).getByRole("button", { name: "Forget this chat…" }));
    await userEvent.click(
      within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Forget chat" }),
    );

    await waitFor(() => expect(notice("chat-did-not-start:ide")).toBeNull());
    expect(sent(asked, "forget_chat_that_did_not_start")).toEqual([{ plane: PLANE, name: "ide" }]);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("is kept, and the question says why, when the core refuses to forget it", async () => {
    refuse.forget_chat_that_did_not_start = "the record could not be written";
    core();
    render(<App />);
    const said = (await screen.findByText(/did not start/)).closest("[data-cause]") as HTMLElement;

    await userEvent.click(within(said).getByRole("button", { name: "Forget this chat…" }));
    const question = await screen.findByRole("alertdialog");
    await userEvent.click(within(question).getByRole("button", { name: "Forget chat" }));

    expect((await within(question).findByRole("alert")).textContent).toContain(
      "the record could not be written",
    );
    await userEvent.click(within(question).getByRole("button", { name: "Cancel" }));
    expect(notice("chat-did-not-start:ide")).not.toBeNull();
  });
});

describe("how a chat came back", () => {
  it("is news that can be dismissed: resumed, or back as a new chat", async () => {
    open = [chat(1, "one", { harness: "claude-code", fresh: "no conversation was recorded" })];
    core();
    render(<App />);
    const said = (await screen.findByText(/came back as a new chat/)).closest(
      "[data-cause]",
    ) as HTMLElement;

    await userEvent.click(within(said).getByRole("button", { name: "Dismiss" }));

    expect(screen.queryByText(/came back as a new chat/)).toBeNull();
  });
});

describe("a chat the project's instructions changed under", () => {
  beforeEach(() => {
    updated = [{ session: 1, files: ["CLAUDE.md"] }];
  });

  it("is started fresh from its tab's mark, after the question", async () => {
    const asked = core();
    render(<App />);
    const mark = await screen.findByRole("button", { name: /Start chat one fresh/ });

    await userEvent.click(mark);
    const question = await screen.findByRole("alertdialog");
    expect(question.textContent).toContain("CLAUDE.md");
    await userEvent.click(within(question).getByRole("button", { name: "Start fresh" }));

    await waitFor(() => expect(screen.getByTestId("pane").textContent).toBe("session 9"));
    expect(sent(asked, "start_chat_fresh")).toEqual([
      expect.objectContaining({ plane: PLANE, session: 1 }),
    ]);
    expect(sent(asked, "close_session")).toEqual([{ plane: PLANE, session: 1 }]);
    expect(tabNames()).toEqual(["one"]);
  });

  it("is started fresh from the palette", async () => {
    const asked = core();
    render(<App />);
    await screen.findByRole("button", { name: /Start chat one fresh/ });

    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("Start chat one fresh");
    await userEvent.keyboard("{Enter}");
    await userEvent.click(
      within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Start fresh" }),
    );

    await waitFor(() =>
      expect(sent(asked, "start_chat_fresh")).toEqual([
        expect.objectContaining({ plane: PLANE, session: 1 }),
      ]),
    );
    await waitFor(() => expect(screen.getByTestId("pane").textContent).toBe("session 9"));
  });

  it("keeps the chat it has, and says why, when the fresh start is refused", async () => {
    refuse.start_chat_fresh = "the profile is gone";
    const asked = core();
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /Start chat one fresh/ }));
    const question = await screen.findByRole("alertdialog");
    await userEvent.click(within(question).getByRole("button", { name: "Start fresh" }));

    expect((await within(question).findByRole("alert")).textContent).toContain(
      "the profile is gone",
    );
    expect(sent(asked, "close_session")).toEqual([]);
    expect(screen.getByTestId("pane").textContent).toBe("session 1");
  });

  it("is not offered for a chat the instructions did not change under", async () => {
    updated = [];
    core();
    render(<App />);
    await waitFor(() => expect(tabNames()).toEqual(["one"]));

    expect(screen.queryByRole("button", { name: /fresh/ })).toBeNull();
  });
});
