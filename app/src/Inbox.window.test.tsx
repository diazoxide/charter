import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Inbox, NOTHING_WAITS, UPDATES, type UpdateRow, type Updates } from "./Inbox";
import { DispatchGrantNotice } from "./DispatchGrantNotice";
import { forgetInbox, replyBytes } from "./inboxRules";
import type { DispatchPending, InboxUpdate, Shown } from "./bindings";

/**
 * **The Inbox** (#1692, spec #1688): a project's asks grouped by chat, oldest first, each under
 * its chain, answered in place through the path its source's Notice answers with, Go to chat on
 * every one, and the keys of I-11. Driven as the person drives it: what they read and press.
 */

afterEach(() => {
  cleanup();
  clearMocks();
  forgetInbox();
});

const PLANE = "/home/dev/plane";

const permission = (session: number, ask: string, chain: string[], says = "Run cargo test") =>
  ({
    session,
    ask,
    says,
    options: [
      { id: "allow", label: "Allow", allows: true },
      { id: "deny", label: "Deny", allows: false },
    ],
    source: "permission",
    chain,
    answer: { via: "hook" },
  }) satisfies Shown;

const HOST: Shown = {
  session: 5,
  ask: "block:5:connect:host:api.example.com",
  says: "The sandbox refused api.example.com",
  options: [
    { id: "chat", label: "Allow for this chat", allows: true },
    { id: "keep", label: "Keep blocked", allows: false },
  ],
  source: "sandbox-host",
  chain: ["steward 5"],
  answer: {
    via: "sandbox-block",
    shown: { operation: "connect", kind: "host", what: "host", target: "api.example.com" },
  },
};

const DISPATCH: Shown = {
  session: 4,
  ask: "dispatch:7",
  says: "Wants to hand a task to devops",
  options: [
    { id: "chat", label: "Allow for this chat", allows: true },
    { id: "keep", label: "Keep blocked", allows: false },
  ],
  source: "dispatch",
  chain: ["steward 4"],
  answer: { via: "dispatch", id: 7, shown: "s0" },
};

const HELD: DispatchPending = {
  plane: PLANE,
  id: 7,
  session: 4,
  chat: "steward 4",
  asking: "steward",
  target: "devops",
  brief: "Check why the prod deploy is red.",
  brief_cut: false,
  brief_lines: 1,
  levels: ["chat"],
  locked: null,
  never_unread: null,
  works_in: null,
  works_in_missing: false,
  allowed_in: [],
  works_with: "devops works with its own access: vault team.",
  also: [],
  shown: "s0",
  task: null,
  task_cut: false,
  profile: null,
};

const QUESTION: Shown = {
  session: 6,
  ask: "question:6",
  says: "Waiting on your reply",
  options: [],
  source: "question",
  chain: ["steward 6"],
  answer: { via: "in-its-pane" },
};

const TERMINAL: Shown = {
  session: 8,
  ask: "terminal:8",
  says: "Waiting in its terminal: a permission prompt",
  options: [],
  source: "terminal",
  chain: ["steward 8"],
  answer: { via: "in-its-pane" },
};

/** A core that records what the window sends and answers each command from `answers`. */
function core(answers: Record<string, unknown> = {}) {
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args: args as Record<string, unknown> });
    if (cmd in answers) {
      const answer = answers[cmd];
      if (answer instanceof Error) throw answer.message;
      return typeof answer === "function" ? (answer as (a: unknown) => unknown)(args) : answer;
    }
    return null;
  });
  return calls;
}

function draw(asks: readonly Shown[] | undefined, more: Partial<Parameters<typeof Inbox>[0]> = {}) {
  const props = {
    plane: PLANE,
    asks,
    onGo: vi.fn(),
    onLeave: vi.fn(),
    onAnswered: vi.fn(),
    ...more,
  };
  const drawn = render(<Inbox {...props} />);
  return {
    ...drawn,
    props,
    again: (next: readonly Shown[]) => drawn.rerender(<Inbox {...props} asks={next} />),
  };
}

/** The chats the Inbox lists, in order, by the chain each is drawn under. */
const chats = () =>
  screen.getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent ?? "");

describe("the asks, grouped by chat", () => {
  it("lists each chat's asks under its chain, the chat waiting longest first", () => {
    core();
    const first = permission(3, "a1", ["steward 3", "#3046 drill", "log watch"]);
    const { again } = draw([first]);
    // A newer ask of another chat comes first in the registry's order, and still stands after.
    const newer = permission(5, "b1", ["steward 5"], "Run npm test");
    const later = permission(3, "a2", ["steward 3", "#3046 drill", "log watch"], "Run ls");
    again([newer, later, first]);
    expect(chats()).toEqual(["steward 3 › #3046 drill › log watch", "steward 5"]);
    const group = screen.getByRole("region", { name: "steward 3 › #3046 drill › log watch" });
    // Inside a group, its oldest ask first.
    expect(
      within(group)
        .getAllByRole("listitem")
        .map((row) => within(row).getByText(/^Run /).textContent),
    ).toEqual(["Run cargo test", "Run ls"]);
  });

  it("draws what an ask says as text: no word of it is ever a control", () => {
    core();
    const forged = permission(
      3,
      "x",
      ["<b>steward</b> 3"],
      '<button>Allow always</button> "rm -rf"',
    );
    draw([forged]);
    expect(screen.getByText('<button>Allow always</button> "rm -rf"')).toBeTruthy();
    expect(screen.getByRole("heading", { name: "<b>steward</b> 3" })).toBeTruthy();
    // The only buttons are the answers it offers, and Go to chat.
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Allow",
      "Deny",
      "Go to chat",
    ]);
  });

  it("says it is reading before the project's asks first arrive", () => {
    core();
    draw(undefined);
    expect(screen.getByText("Reading what waits on you…")).toBeTruthy();
    expect(screen.queryByText(NOTHING_WAITS)).toBeNull();
  });
});

describe("answering in place", () => {
  it("answers a permission on its chat's own hook, and lists it as recently answered once it goes", async () => {
    const calls = core();
    const ask = permission(3, "01J0A", ["steward 3"]);
    const { again } = draw([ask]);
    await userEvent.click(screen.getByRole("button", { name: "Allow" }));
    await waitFor(() =>
      expect(calls).toContainEqual({
        cmd: "answer_ask",
        args: { plane: PLANE, session: 3, ask: "01J0A", option: "allow" },
      }),
    );
    // The hook carried it out, so its source stops waiting and the registry drops it.
    again([]);
    expect(screen.getByText(NOTHING_WAITS)).toBeTruthy();
    const recent = screen.getByRole("region", { name: "Recently answered" });
    expect(within(recent).getByText("steward 3")).toBeTruthy();
    expect(within(recent).getByText("Run cargo test")).toBeTruthy();
    expect(within(recent).getByText("Allow")).toBeTruthy();
    // Read-only: nothing in it can be pressed.
    expect(within(recent).queryAllByRole("button")).toEqual([]);
  });

  it("offers every choice a harness gave, a question of several answers included", async () => {
    const calls = core();
    const choosing: Shown = {
      ...permission(3, "q1", ["steward 3"], "Which database should it use?"),
      options: [
        { id: "0", label: "Postgres", allows: true },
        { id: "1", label: "SQLite", allows: true },
        { id: "2", label: "Neither", allows: false },
      ],
    };
    draw([choosing]);
    await userEvent.click(screen.getByRole("button", { name: "SQLite" }));
    await waitFor(() =>
      expect(calls.find((one) => one.cmd === "answer_ask")?.args.option).toBe("1"),
    );
  });

  it("allows a refused host by the block Notice's own command, and owes the chat its restart", async () => {
    const calls = core({ allow_sandbox_block: { said: "Allowed." } });
    const { props } = draw([HOST]);
    await userEvent.click(screen.getByRole("button", { name: "Allow for this chat" }));
    await waitFor(() =>
      expect(calls).toContainEqual({
        cmd: "allow_sandbox_block",
        args: {
          plane: PLANE,
          session: 5,
          shown: HOST.answer.via === "sandbox-block" ? HOST.answer.shown : null,
          level: "chat",
        },
      }),
    );
    // What the block Notice does after an Allow, the window does too: its pane's copy is put
    // away and the chat is owed a restart to take the grant.
    await waitFor(() =>
      expect(props.onAnswered).toHaveBeenCalledWith(HOST, HOST.options[0], false),
    );
  });

  it("says the source's own sentence when it refuses, and lists the ask still", async () => {
    core({ answer_ask: new Error("This ask was answered elsewhere.") });
    const { props } = draw([permission(3, "late", ["steward 3"])]);
    await userEvent.click(screen.getByRole("button", { name: "Deny" }));
    expect(await screen.findByText("This ask was answered elsewhere.")).toBeTruthy();
    expect(props.onAnswered).not.toHaveBeenCalled();
  });

  it("draws a dispatch grant as its own Notice, brief and all, and allows it by its digest", async () => {
    let held = [HELD];
    const calls = core({
      dispatch_grants_needed: () => held,
      allow_dispatch: () => {
        held = [];
        return { said: "Allowed for this chat." };
      },
    });
    draw([DISPATCH]);
    // What the Allow is bound to is on screen before it: the brief as the chat wrote it.
    expect(await screen.findByText("Check why the prod deploy is red.")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Allow for this chat" }));
    await waitFor(() =>
      expect(calls).toContainEqual({
        cmd: "allow_dispatch",
        args: { plane: PLANE, id: 7, level: "chat", also: [], shown: "s0" },
      }),
    );
  });

  it("clears the dispatch's Notice on its chat's pane when it is answered here, and the other way", async () => {
    let held = [HELD];
    core({
      dispatch_grants_needed: () => held,
      allow_dispatch: () => {
        held = [];
        return { said: "Allowed for this chat." };
      },
      keep_dispatch_blocked: () => {
        held = [];
        return true;
      },
    });
    const props = { plane: PLANE, onGo: vi.fn(), onLeave: vi.fn() };
    const both = (asks: readonly Shown[]) => (
      <>
        <section aria-label="pane">
          <DispatchGrantNotice plane={PLANE} session={4} />
        </section>
        <Inbox {...props} asks={asks} />
      </>
    );
    const { rerender } = render(both([DISPATCH]));
    const pane = screen.getByRole("region", { name: "pane" });
    const inbox = screen.getByRole("region", { name: "Inbox" });
    await within(pane).findByText("Check why the prod deploy is red.");
    await within(inbox).findByText("Check why the prod deploy is red.");

    await userEvent.click(within(inbox).getByRole("button", { name: "Allow for this chat" }));

    // Both are drawn from the one list the core holds: the pane's question is gone with it.
    await waitFor(() =>
      expect(within(pane).queryByText("Check why the prod deploy is red.")).toBeNull(),
    );
    rerender(both([]));
    // And the Inbox keeps what it answered.
    expect(within(inbox).getByText(NOTHING_WAITS)).toBeTruthy();
    const recent = within(inbox).getByRole("region", { name: "Recently answered" });
    // With the words of the way out that was pressed.
    expect(within(recent).getByText("Allow for this chat")).toBeTruthy();
  });

  it("gives a chat waiting on a reply a box, and sends what is typed as the person's message", async () => {
    const calls = core();
    draw([QUESTION]);
    await userEvent.type(
      screen.getByRole("textbox", { name: "Reply to steward 6" }),
      "Yes, ship it",
    );
    await userEvent.click(screen.getByRole("button", { name: "Send reply" }));
    await waitFor(() =>
      expect(calls).toContainEqual({
        cmd: "send_input",
        args: { plane: PLANE, session: 6, text: replyBytes("Yes, ship it") },
      }),
    );
    expect((screen.getByRole("textbox") as HTMLInputElement).value).toBe("");
  });

  it("says why a chat waits where it asked nothing, and offers it no reply box", () => {
    core();
    draw([QUESTION], { whyOf: (session) => (session === 6 ? "deep failed" : undefined) });
    expect(screen.getByText("deep failed")).toBeTruthy();
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByRole("button", { name: "Go to chat steward 6" })).toBeTruthy();
  });

  it("sends nothing for an empty reply", () => {
    core();
    draw([QUESTION]);
    expect(screen.getByRole("button", { name: "Send reply" })).toHaveProperty("disabled", true);
  });

  it("names a prompt waiting in a harness's terminal, with only the way to it", () => {
    core();
    draw([TERMINAL]);
    expect(screen.getByText("Waiting in its terminal: a permission prompt")).toBeTruthy();
    expect(screen.getAllByRole("button").map((one) => one.textContent)).toEqual(["Go to chat"]);
  });
});

describe("ignoring a chat that waits on a reply", () => {
  it("offers the queue's own Ignore on a chat waiting on a reply, and on nothing that asks a decision", async () => {
    core();
    const ignored: number[] = [];
    draw([QUESTION, TERMINAL, permission(3, "p", ["steward 3"])], {
      onIgnore: (session) => ignored.push(session),
    });
    const ignores = screen.getAllByRole("button", { name: /^Ignore .+ until it asks again$/ });
    // A question and a prompt in its terminal: the chat asks again at its next stop. A
    // permission is a decision, and is never put away unanswered.
    expect(ignores.map((one) => one.getAttribute("aria-label"))).toEqual([
      "Ignore steward 6 until it asks again",
      "Ignore steward 8 until it asks again",
    ]);
    await userEvent.click(ignores[0]);
    expect(ignored).toEqual([6]);
  });
});

describe("updates, after the asks (#1693)", () => {
  const update = (key: string, at: number, more: Partial<InboxUpdate> = {}): InboxUpdate => ({
    key,
    kind: "task-done",
    at,
    session: 3,
    chain: ["steward 3"],
    says: `${key}: done`,
    read: false,
    ...more,
  });
  const rows = (list: InboxUpdate[], more: Partial<UpdateRow> = {}): UpdateRow[] =>
    list.map((one) => ({ update: one, dismiss: vi.fn(), ...more }));
  const updates = (list: UpdateRow[]): Updates => ({
    rows: list,
    onMarkAllRead: vi.fn(),
    onDismissAll: vi.fn(),
  });

  it("lists them after the asks, in the order given, newest first, each with its kind and words", () => {
    core();
    draw([permission(3, "a1", ["steward 3"])], {
      updates: updates(
        rows([
          update("task:b", 2_000, { kind: "task-failed", says: "drill: failed" }),
          update("doctor:git:fail", 1_000, { kind: "doctor", chain: [], says: "git: no identity" }),
        ]),
      ),
    });
    const list = screen.getByRole("region", { name: UPDATES });
    // After the asks: the ask's group comes first in the list.
    expect(chats()).toEqual(["steward 3", UPDATES]);
    expect(
      within(list)
        .getAllByRole("listitem")
        .map((row) => row.getAttribute("data-kind")),
    ).toEqual(["task-failed", "doctor"]);
    expect(within(list).getByText("steward 3: drill: failed")).toBeTruthy();
    expect(within(list).getByText("git: no identity")).toBeTruthy();
    expect(within(list).getAllByText(/Task failed|Doctor/).length).toBe(2);
  });

  it("draws an update's words as text, never as a control", () => {
    core();
    draw([], {
      updates: updates(rows([update("x", 1, { says: "<button>Allow</button>" })])),
    });
    expect(screen.getByText("steward 3: <button>Allow</button>")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Allow" })).toBeNull();
  });

  it("stands under the empty Inbox's sentence: an update is nothing that waits", () => {
    core();
    draw([], { updates: updates(rows([update("task:a", 1)])) });
    expect(screen.getByText(NOTHING_WAITS)).toBeTruthy();
    expect(screen.getByRole("region", { name: UPDATES })).toBeTruthy();
  });

  it("marks every update read and dismisses every one, and answers no ask doing either", async () => {
    const calls = core();
    const these = updates(rows([update("task:a", 1)]));
    draw([permission(3, "a1", ["steward 3"])], { updates: these });
    await userEvent.click(screen.getByRole("button", { name: "Mark all read" }));
    await userEvent.click(screen.getByRole("button", { name: "Dismiss all" }));
    expect(these.onMarkAllRead).toHaveBeenCalledOnce();
    expect(these.onDismissAll).toHaveBeenCalledOnce();
    // No ask was answered in bulk: nothing went to any source.
    expect(calls.filter((one) => one.cmd === "answer_ask")).toEqual([]);
    expect(screen.getByRole("button", { name: "Allow" })).toBeTruthy();
  });

  it("offers Mark all read only while something is unread", () => {
    core();
    draw([], { updates: updates(rows([update("task:a", 1, { read: true })])) });
    expect(
      (screen.getByRole("button", { name: "Mark all read" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(screen.queryByText(/· new/)).toBeNull();
  });

  it("says nothing of updates while there are none", () => {
    core();
    draw([], { updates: updates([]) });
    expect(screen.queryByRole("region", { name: UPDATES })).toBeNull();
    expect(screen.queryByRole("button", { name: "Dismiss all" })).toBeNull();
  });

  it("dismisses one, goes to its chat, and presses a source's own answers", async () => {
    core();
    const go = vi.fn();
    const never = vi.fn();
    const [row] = rows([update("away:steward#devops#", 1, { kind: "refused-away", chain: [] })], {
      go,
      more: "devops works with its own access.",
      answers: [
        {
          label: "Never for this pair",
          name: "Never for this pair: steward",
          title: "",
          press: never,
        },
      ],
    });
    draw([], { updates: updates([row]) });
    expect(screen.getByText("devops works with its own access.")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Never for this pair: steward" }));
    await userEvent.click(screen.getByRole("button", { name: /^Go to chat/ }));
    await userEvent.click(screen.getByRole("button", { name: /^Dismiss: / }));
    expect(never).toHaveBeenCalledOnce();
    expect(go).toHaveBeenCalledOnce();
    expect(row.dismiss).toHaveBeenCalledOnce();
  });

  it("is reached by ↓ from the last ask, its two buttons first, and Enter presses one", async () => {
    core();
    const these = updates(rows([update("task:a", 1)]));
    draw([permission(3, "a1", ["steward 3"])], { updates: these });
    const ask = screen.getAllByRole("listitem")[0];
    act(() => ask.focus());
    // The ask, its two answers and Go to chat; then the updates' two buttons.
    for (let step = 0; step < 4; step += 1) await userEvent.keyboard("{ArrowDown}");
    expect(document.activeElement?.textContent).toBe("Mark all read");
    await userEvent.keyboard("{ArrowDown}");
    await userEvent.keyboard("{Enter}");
    expect(these.onDismissAll).toHaveBeenCalledOnce();
  });
});

describe("going to the chat", () => {
  it("has Go to chat on every ask, a task's too, which goes to the chat that asked", async () => {
    core({ dispatch_grants_needed: [HELD] });
    const task = permission(9, "t1", ["steward 3", "#3046 drill"]);
    const { props } = draw([task, HOST, DISPATCH, QUESTION, TERMINAL]);
    const goes = screen.getAllByRole("button", { name: /^Go to chat / });
    expect(goes).toHaveLength(5);
    await userEvent.click(
      screen.getByRole("button", { name: "Go to chat steward 3 › #3046 drill" }),
    );
    expect(props.onGo).toHaveBeenCalledWith(9);
  });
});

describe("the keyboard (I-11)", () => {
  it("moves with ↑ and ↓ from the ask to its buttons and on, and Enter presses the one focused", async () => {
    const calls = core();
    draw([permission(3, "k1", ["steward 3"]), permission(5, "k2", ["steward 5"], "Run ls")]);
    const [first, second] = screen.getAllByRole("listitem");
    act(() => first.focus());
    await userEvent.keyboard("{ArrowDown}");
    expect(document.activeElement?.textContent).toBe("Allow");
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}");
    expect(document.activeElement).toBe(second);
    await userEvent.keyboard("{ArrowUp}");
    expect(document.activeElement?.textContent).toBe("Go to chat");
    await userEvent.keyboard("{ArrowUp}{ArrowUp}{Enter}");
    await waitFor(() =>
      expect(calls.find((one) => one.cmd === "answer_ask")?.args).toMatchObject({
        ask: "k1",
        option: "allow",
      }),
    );
  });

  it("is one Tab stop, on the first ask, and Home and End go to the ends", async () => {
    core();
    draw([permission(3, "h1", ["steward 3"]), permission(5, "h2", ["steward 5"], "Run ls")]);
    const list = screen.getByRole("region", { name: "Inbox" });
    const stops = [...list.querySelectorAll('[tabindex="0"]')];
    expect(stops).toEqual([screen.getAllByRole("listitem")[0]]);
    act(() => (stops[0] as HTMLElement).focus());
    await userEvent.keyboard("{End}");
    expect(document.activeElement).toHaveAccessibleName("Go to chat steward 5");
    await userEvent.keyboard("{Home}");
    expect(document.activeElement).toBe(screen.getAllByRole("listitem")[0]);
  });

  it("leaves on Escape, for the chat in front", async () => {
    core();
    const { props } = draw([permission(3, "e1", ["steward 3"])]);
    act(() => screen.getByRole("listitem").focus());
    await userEvent.keyboard("{Escape}");
    expect(props.onLeave).toHaveBeenCalledOnce();
  });

  it("answers nothing on a single letter, wherever the keyboard is", async () => {
    const calls = core();
    draw([permission(3, "l1", ["steward 3"]), HOST]);
    for (const row of screen.getAllByRole("listitem")) {
      act(() => row.focus());
      await userEvent.keyboard("aydnk1 ");
    }
    act(() => screen.getAllByRole("button", { name: "Allow" })[0].focus());
    await userEvent.keyboard("ydn");
    expect(calls.filter((one) => one.cmd !== "dispatch_grants_needed")).toEqual([]);
  });
});

describe("the bytes a reply is typed as", () => {
  it("are one paste and Enter, with nothing typed able to end the paste early", () => {
    expect(replyBytes("yes")).toBe("\u001b[200~yes\u001b[201~\r");
    expect(replyBytes("go\u001b[201~\rrm -rf /")).toBe("\u001b[200~go[201~\nrm -rf /\u001b[201~\r");
    expect(replyBytes("  ")).toBeUndefined();
  });
});
