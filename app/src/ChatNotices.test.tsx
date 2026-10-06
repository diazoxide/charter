import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { ApprovalSentence } from "./ProfileApproval";
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
let waiting: { id: string; name: string; why: string; approval?: Approval | null }[];
/** A waiting chat's profile approval, as the core says it (`NeedsApproval`, #1246). */
type Approval = { profile: string; kind: string; source: string; approval: string; shown: string };
/** The profiles the core has recorded an approval for, by name (`approve_profile`). */
let approved: string[];
let updated: { session: number; files: string[] }[];
let refuse: Partial<Record<string, string>>;
/** What the core says each chat is doing (`chat_states`). */
let states: { session: number; state: string }[];

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
    if (cmd === "chat_states")
      return states.map((one, at) => ({
        plane: PLANE,
        ...one,
        needs_you: false,
        queue: [],
        sequence: at + 1,
        moved_at: at + 1,
        reports: [],
        refusals: [],
        children: [],
      }));
    if (cmd === "running_sessions") return [];
    if (cmd === "chats_that_would_not_start") return waiting;
    if (cmd === "chats_plane_updated") return updated;
    if (cmd === "retry_chat_that_did_not_start") {
      // A refusal is a THROWN value, which is what arrives as `{ status: "error" }`.
      if (refuse[cmd] !== undefined) throw refuse[cmd];
      const was = waiting.find((one) => one.id === given.id);
      // The whole start gate, again: a profile nobody approved is refused, and the core reads
      // the approval it needs again with the refusal.
      if (was?.approval && !approved.includes(was.approval.profile)) {
        const why = `profile '${was.approval.profile}' is ${was.approval.approval}, and nobody has approved it — nothing was started.`;
        waiting = waiting.map((one) => (one.id === given.id ? { ...one, why } : one));
        throw why;
      }
      waiting = waiting.filter((one) => one.id !== given.id);
      const started = chat(7, was?.name ?? "?");
      open = [...open, started];
      return started;
    }
    if (cmd === "approve_profile") {
      if (refuse[cmd] !== undefined) throw refuse[cmd];
      approved.push(String(given.name));
      return null;
    }
    if (cmd === "forget_chat_that_did_not_start") {
      if (refuse[cmd] !== undefined) throw refuse[cmd];
      waiting = waiting.filter((one) => one.id !== given.id);
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
    // What a split asks, to start the chat beside the first one.
    if (cmd === "start_options") return START_OPTIONS;
    if (cmd === "start_chat") return { session: 2 };
    return null;
  });
  return asked;
}

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
  states = [];
  approved = [];
});

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("a chat that did not start", () => {
  beforeEach(() => {
    waiting = [{ id: "id-ide", name: "ide", why: "no such directory: /home/dev/gone" }];
  });

  it("is started by Retry now, and its tab opens", async () => {
    const asked = core();
    render(<App />);
    const line = await screen.findByText(/did not start/);
    const said = line.closest("[data-cause]") as HTMLElement;

    await userEvent.click(within(said).getByRole("button", { name: "Retry now" }));

    await waitFor(() => expect(tabNames()).toContain("ide"));
    expect(sent(asked, "retry_chat_that_did_not_start")).toEqual([
      expect.objectContaining({ plane: PLANE, id: "id-ide" }),
    ]);
    expect(notice("chat-did-not-start:id-ide")).toBeNull();
  });

  it("stays, saying why, when Retry now is refused", async () => {
    refuse.retry_chat_that_did_not_start = "no such directory: /home/dev/still-gone";
    core();
    render(<App />);
    const said = (await screen.findByText(/did not start/)).closest("[data-cause]") as HTMLElement;

    await userEvent.click(within(said).getByRole("button", { name: "Retry now" }));

    await waitFor(() =>
      expect(notice("chat-did-not-start:id-ide")?.textContent).toContain("/home/dev/still-gone"),
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
    expect(notice("chat-did-not-start:id-ide")).not.toBeNull();

    await userEvent.click(within(said).getByRole("button", { name: "Forget this chat…" }));
    await userEvent.click(
      within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Forget chat" }),
    );

    await waitFor(() => expect(notice("chat-did-not-start:id-ide")).toBeNull());
    expect(sent(asked, "forget_chat_that_did_not_start")).toEqual([{ plane: PLANE, id: "id-ide" }]);
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
    expect(notice("chat-did-not-start:id-ide")).not.toBeNull();
  });
});

describe("a chat waiting on its profile's approval (#1246)", () => {
  const WORK: Approval = {
    profile: "work",
    kind: "claude",
    source: "charter.local.toml",
    approval: "changed",
    shown: "ANTHROPIC_PROFILE=work ccs work --verbose",
  };
  const REVIEW = "Review and approve…";

  beforeEach(() => {
    waiting = [
      {
        id: "id-ide",
        name: "ide",
        why: "profile 'work' is changed, and nobody has approved it — nothing was started.",
        approval: WORK,
      },
    ];
  });

  const said = async () =>
    (await screen.findByText(/did not start/)).closest("[data-cause]") as HTMLElement;

  it("offers Review and approve…, never a one-press Approve, and only where it needs one", async () => {
    waiting = [...waiting, { id: "id-gone", name: "gone", why: "no such directory: /gone" }];
    core();
    render(<App />);
    await screen.findAllByText(/did not start/);

    const asking = notice("chat-did-not-start:id-ide") as HTMLElement;
    expect(within(asking).getByRole("button", { name: REVIEW })).toBeTruthy();
    expect(within(asking).queryByRole("button", { name: /^Approve/ })).toBeNull();
    const other = notice("chat-did-not-start:id-gone") as HTMLElement;
    expect(within(other).queryByRole("button", { name: REVIEW })).toBeNull();
  });

  it("opens the picker's own approval sentence, with the exact line, and approves nothing yet", async () => {
    const asked = core();
    render(<App />);

    await userEvent.click(within(await said()).getByRole("button", { name: REVIEW }));

    const question = await screen.findByRole("alertdialog");
    // The very component the new-chat picker draws, drawn from the core's line as it is.
    const picker = renderBare(<ApprovalSentence row={WORK} />).container.firstElementChild;
    const sentence = question.querySelector(".honest.approve");
    expect(sentence?.outerHTML).toBe(picker?.outerHTML);
    expect(sentence?.querySelector("code")?.textContent).toBe(WORK.shown);
    expect(question.querySelector(".needs-approval")?.textContent).toBe("changed");
    expect(sent(asked, "approve_profile")).toEqual([]);
    expect(sent(asked, "retry_chat_that_did_not_start")).toEqual([]);
  });

  it("changes nothing on Cancel", async () => {
    const asked = core();
    render(<App />);
    const line = await said();

    await userEvent.click(within(line).getByRole("button", { name: REVIEW }));
    await userEvent.click(
      within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Cancel" }),
    );

    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(sent(asked, "approve_profile")).toEqual([]);
    expect(sent(asked, "retry_chat_that_did_not_start")).toEqual([]);
    expect(within(line).getByRole("button", { name: REVIEW })).toBeTruthy();
  });

  it("is refused, in the question, when the file changed in between, and nothing starts", async () => {
    refuse.approve_profile =
      "profile 'work' changed while you were reading it, so nothing was approved and nothing was started. It now runs: ccs evil";
    const asked = core();
    render(<App />);
    const line = await said();

    await userEvent.click(within(line).getByRole("button", { name: REVIEW }));
    const question = await screen.findByRole("alertdialog");
    await userEvent.click(within(question).getByRole("button", { name: "Approve" }));

    expect(
      (await within(question).findByText(/changed while you were reading/)).textContent,
    ).toContain("ccs evil");
    // The line sent is the one on screen, for the core to check against the file.
    expect(sent(asked, "approve_profile")).toEqual([
      { plane: PLANE, name: "work", shown: WORK.shown },
    ]);
    expect(sent(asked, "retry_chat_that_did_not_start")).toEqual([]);
    await userEvent.click(within(question).getByRole("button", { name: "Cancel" }));
    expect(within(line).getByRole("button", { name: REVIEW })).toBeTruthy();
    expect(tabNames()).not.toContain("ide");
  });

  it("starts the chat on Retry now once approved, and the keyboard is on Retry now", async () => {
    const asked = core();
    render(<App />);
    const line = await said();

    await userEvent.click(within(line).getByRole("button", { name: REVIEW }));
    await userEvent.click(
      within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Approve" }),
    );

    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(sent(asked, "approve_profile")).toEqual([
      { plane: PLANE, name: "work", shown: WORK.shown },
    ]);
    // Approving starts nothing: Retry now runs the whole start again.
    expect(sent(asked, "retry_chat_that_did_not_start")).toEqual([]);
    expect(within(line).queryByRole("button", { name: REVIEW })).toBeNull();
    await waitFor(() => expect(document.activeElement?.textContent).toBe("Retry now"));

    await userEvent.click(within(line).getByRole("button", { name: "Retry now" }));

    await waitFor(() => expect(tabNames()).toContain("ide"));
    expect(notice("chat-did-not-start:id-ide")).toBeNull();
  });

  it("offers it once a Retry now is refused for it", async () => {
    // A chat that first waited for another reason: the core reads the approval again with the
    // refusal, and the window takes it from the core, never from the reason's words.
    waiting = [{ id: "id-ide", name: "ide", why: "no such directory: /home/dev/gone" }];
    core();
    render(<App />);
    const line = await said();
    expect(within(line).queryByRole("button", { name: REVIEW })).toBeNull();
    waiting = [{ ...waiting[0], approval: WORK }];

    await userEvent.click(within(line).getByRole("button", { name: "Retry now" }));

    expect(await within(line).findByRole("button", { name: REVIEW })).toBeTruthy();
    expect(line.textContent).toContain("nobody has approved it");
  });
});

describe("two waiting chats with one name (NO-3 review)", () => {
  it("are two Notices, and Forget on the second forgets only the second", async () => {
    // A split's chat takes its tab's name and tab numbers start again at every launch, so two
    // waiting chats can share a name. Each Notice is its chat's, by id.
    waiting = [
      { id: "id-a", name: "3", why: "no such directory: /a" },
      { id: "id-b", name: "3", why: "no such directory: /b" },
    ];
    const asked = core();
    render(<App />);
    await screen.findAllByText(/did not start/);
    const second = notice("chat-did-not-start:id-b") as HTMLElement;

    await userEvent.click(within(second).getByRole("button", { name: "Forget this chat…" }));
    await userEvent.click(
      within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Forget chat" }),
    );

    await waitFor(() => expect(notice("chat-did-not-start:id-b")).toBeNull());
    expect(sent(asked, "forget_chat_that_did_not_start")).toEqual([{ plane: PLANE, id: "id-b" }]);
    expect(notice("chat-did-not-start:id-a")?.textContent).toContain("/a");
  });

  it("are retried and dismissed one at a time", async () => {
    waiting = [
      { id: "id-a", name: "3", why: "no such directory: /a" },
      { id: "id-b", name: "3", why: "no such directory: /b" },
    ];
    const asked = core();
    render(<App />);
    await screen.findAllByText(/did not start/);

    await userEvent.click(
      within(notice("chat-did-not-start:id-a") as HTMLElement).getByRole("button", {
        name: "Dismiss",
      }),
    );
    expect(notice("chat-did-not-start:id-a")).toBeNull();
    await userEvent.click(
      within(notice("chat-did-not-start:id-b") as HTMLElement).getByRole("button", {
        name: "Retry now",
      }),
    );

    await waitFor(() => expect(notice("chat-did-not-start:id-b")).toBeNull());
    expect(sent(asked, "retry_chat_that_did_not_start")).toEqual([
      expect.objectContaining({ id: "id-b" }),
    ]);
  });
});

describe("the keyboard after a Forget (#1246)", () => {
  it("lands on the next Notice, and on the strip once there is none", async () => {
    // The dialog closes and the Notice it was asked from goes with it, so handing the focus
    // back to where it was would drop it on the page.
    waiting = [
      { id: "id-a", name: "3", why: "no such directory: /a" },
      { id: "id-b", name: "4", why: "no such directory: /b" },
    ];
    core();
    render(<App />);
    await screen.findAllByText(/did not start/);
    const forget = async (id: string) => {
      await userEvent.click(
        within(notice(`chat-did-not-start:${id}`) as HTMLElement).getByRole("button", {
          name: "Forget this chat…",
        }),
      );
      await userEvent.click(
        within(await screen.findByRole("alertdialog")).getByRole("button", {
          name: "Forget chat",
        }),
      );
      await waitFor(() => expect(notice(`chat-did-not-start:${id}`)).toBeNull());
    };

    await forget("id-a");
    await waitFor(() =>
      expect(document.activeElement?.closest("[data-cause]")?.getAttribute("data-cause")).toBe(
        "chat-did-not-start:id-b",
      ),
    );

    await forget("id-b");
    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(screen.getByRole("tablist", { name: "Tabs" }))
          .getAllByRole("tab")
          .find((tab) => tab.getAttribute("aria-selected") === "true"),
      ),
    );
  });
});

describe("the keyboard after a Cancel (#1246 review)", () => {
  it("goes back to the Notice's Forget this chat…, after a refusal too", async () => {
    waiting = [{ id: "id-ide", name: "ide", why: "no such directory: /home/dev/gone" }];
    core();
    render(<App />);
    await screen.findByText(/did not start/);
    const forgetOn = () =>
      within(notice("chat-did-not-start:id-ide") as HTMLElement).getByRole("button", {
        name: "Forget this chat…",
      });

    await userEvent.click(forgetOn());
    await userEvent.click(
      within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Cancel" }),
    );
    await waitFor(() => expect(document.activeElement).toBe(forgetOn()));

    refuse.forget_chat_that_did_not_start = "the record could not be written";
    await userEvent.click(forgetOn());
    const question = await screen.findByRole("alertdialog");
    await userEvent.click(within(question).getByRole("button", { name: "Forget chat" }));
    await within(question).findByRole("alert");
    await userEvent.click(within(question).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(document.activeElement).toBe(forgetOn()));
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
    // The core ends the old one itself; the window only puts the new one in its pane.
    expect(sent(asked, "close_session")).toEqual([]);
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

  it("on a split tab, replaces only its own pane and leaves the chat beside it running", async () => {
    // The reviewer's probe: the old tab closed, and the chat split beside it was left running
    // with no tab. Now the new session takes the old one's pane, and the tab stays (D-NO3-8).
    const asked = core();
    render(<App />);
    await screen.findByRole("button", { name: /Start chat one fresh/ });
    await userEvent.click(screen.getByRole("button", { name: "Split right" }));
    await userEvent.click(await screen.findByRole("button", { name: "Start" }));
    await waitFor(() =>
      expect(screen.getAllByTestId("pane").map((p) => p.textContent)).toEqual([
        "session 1",
        "session 2",
      ]),
    );

    await userEvent.click(screen.getByRole("button", { name: /Start chat one fresh/ }));
    const question = await screen.findByRole("alertdialog");
    expect(question.textContent).toContain("Start one fresh?");
    await userEvent.click(within(question).getByRole("button", { name: "Start fresh" }));

    await waitFor(() =>
      expect(screen.getAllByTestId("pane").map((p) => p.textContent)).toEqual([
        "session 9",
        "session 2",
      ]),
    );
    expect(sent(asked, "start_chat_fresh")).toEqual([
      expect.objectContaining({ plane: PLANE, session: 1 }),
    ]);
    expect(sent(asked, "close_session")).toEqual([]);
    expect(tabNames()).toEqual(["one"]);
  });

  it("is a mark its tab is described by, so a screen reader on the tab hears it (#1246)", async () => {
    core();
    render(<App />);
    const mark = await screen.findByRole("button", { name: /Start chat one fresh/ });
    const tab = within(screen.getByRole("tablist", { name: "Tabs" }))
      .getAllByRole("tab")
      .find((one) => one.querySelector(".tab-name")?.textContent === "one") as HTMLElement;

    const describedBy = (tab.getAttribute("aria-describedby") ?? "").split(" ");
    expect(mark.id).not.toBe("");
    expect(describedBy).toContain(mark.id);
  });

  it("asks, saying the turn will be interrupted, on a chat that is mid-turn (#1246)", async () => {
    states = [{ session: 1, state: "running" }];
    core();
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /Start chat one fresh/ }));

    expect((await screen.findByRole("alertdialog")).textContent).toContain(
      "one is mid-turn and will be interrupted.",
    );
  });

  it("says it cannot tell, on a shell tab nothing has reported on (#1246 review)", async () => {
    // A harness started by hand in a shell could be mid-turn, and purlis would never know.
    open = [chat(1, "one", { harness: null, profile: null })];
    states = [];
    core();
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /Start chat one fresh/ }));

    const question = await screen.findByRole("alertdialog");
    expect(question.textContent).toContain(
      "one reports no state, so purlis cannot tell whether it is mid-turn.",
    );
  });

  it("announces the turn it interrupts as part of the question's description (#1246 review)", async () => {
    states = [{ session: 1, state: "running" }];
    core();
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /Start chat one fresh/ }));

    const question = await screen.findByRole("alertdialog");
    const described = (question.getAttribute("aria-describedby") ?? "")
      .split(" ")
      .map((id) => document.getElementById(id)?.textContent ?? "")
      .join(" ");
    expect(described).toContain("one is mid-turn and will be interrupted.");
    expect(described).toContain("Its program ends");
  });

  it("asks without a word about a turn on a chat that is waiting for you (#1246)", async () => {
    states = [{ session: 1, state: "waiting" }];
    core();
    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /Start chat one fresh/ }));

    expect((await screen.findByRole("alertdialog")).textContent).not.toContain("mid-turn");
  });

  it("is not offered for a chat the instructions did not change under", async () => {
    updated = [];
    core();
    render(<App />);
    await waitFor(() => expect(tabNames()).toEqual(["one"]));

    expect(screen.queryByRole("button", { name: /fresh/ })).toBeNull();
  });
});
