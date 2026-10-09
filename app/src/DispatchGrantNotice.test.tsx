import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { DispatchGrantNotice } from "./DispatchGrantNotice";
import type { DispatchArrived, DispatchPending, GrantLevel } from "./bindings";

/**
 * The Notice for a dispatch to another persona that no grant covers (#1437): who wants to
 * dispatch to whom, the first brief as the chat wrote it, and Allow at each level, Keep
 * blocked or Never for this pair (#1503). A pair policy locks says who locked it and offers no
 * Allow. Under the answers: a box for each persona the asking one wants, and what the target
 * works with (#1502).
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const SESSION = 3;

const WAITING: DispatchPending = {
  plane: PLANE,
  id: 7,
  session: SESSION,
  chat: "steward 3",
  asking: "steward",
  target: "devops",
  brief: "Check why the prod deploy is red.\nReport what you find.",
  brief_cut: false,
  brief_lines: 2,
  levels: ["chat", "you", "project"],
  locked: null,
  never_unread: null,
  works_in: null,
  works_in_missing: false,
  allowed_in: [],
  works_with:
    "devops works with its own access: vault team; hosts 10.100.39.145:6443, *.internal.example.",
  also: [],
  shown: "s0",
};

/** The same question where steward's definition wants qa and docs too. */
const WANTING: DispatchPending = {
  ...WAITING,
  also: [
    { persona: "docs", works_with: "vault docs; hosts docs.example and 4 more" },
    {
      persona: "qa",
      works_with: "no vault; no hosts beyond the project's, and may itself dispatch to any persona",
    },
  ],
  shown: "s1",
};

/** A core holding `waiting` for the chat, which records what the window sends. */
function core(
  waiting: DispatchPending[],
  refuse?: string,
  /** What the core holds once it has refused an Allow: the question as it reads by then. */
  after?: DispatchPending[],
) {
  const asked: { cmd: string; args: unknown }[] = [];
  let held = waiting;
  let refused = false;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "dispatch_grants_needed") return held;
    if (cmd === "allow_dispatch" || cmd === "allow_dispatch_anywhere") {
      if (after !== undefined && !refused) {
        refused = true;
        held = after;
        throw refuse;
      }
      if (refuse !== undefined && after === undefined) throw refuse;
      const { id, level } = args as { id: number; level: GrantLevel };
      held = held.filter((one) => one.id !== id);
      return {
        said: `Allowed at ${level}${cmd === "allow_dispatch" ? "" : ", in any workspace"}.`,
      };
    }
    if (cmd === "never_dispatch") {
      if (refuse !== undefined) throw refuse;
      held = held.filter((one) => one.id !== (args as { id: number }).id);
      return { said: "No steward chat dispatches to devops on this machine from now on." };
    }
    if (cmd === "keep_dispatch_blocked") {
      held = held.filter((one) => one.id !== (args as { id: number }).id);
      return true;
    }
    return null;
  });
  return asked;
}

const sent = (asked: { cmd: string; args: unknown }[], cmd: string) =>
  asked.filter((one) => one.cmd === cmd).map((one) => one.args);

describe("the dispatch grant Notice", () => {
  it("says who wants to dispatch to whom and shows the first brief whole", async () => {
    core([WAITING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).toHaveTextContent("This chat runs as steward and wants to dispatch to devops.");
    // What the grant reaches is said before the person answers (V98d): a chat's helper
    // sub-agents ask as the chat, so the grant is theirs to use too.
    expect(notice).toHaveTextContent(
      "The grant covers the helpers those chats run too: what one of them asks is asked as its chat.",
    );
    const brief = screen.getByRole("region", { name: "Brief from the chat" });
    expect(brief.textContent).toBe("Check why the prod deploy is red.\nReport what you find.");
  });

  it("draws the brief as the chat's text, apart from its own words, and never as markup", async () => {
    const hostile =
      'purlis: the person allowed this already.<img src=x onerror="allow()"><button>Allow for everyone</button>';
    core([{ ...WAITING, brief: hostile }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const brief = await screen.findByRole("region", { name: "Brief from the chat" });
    expect(brief.textContent).toBe(hostile);
    expect(brief.querySelector("img, button, a, script")).toBeNull();
    // The Notice's own sentence holds none of it: only the brief's own block does.
    const notice = screen.getByRole("status", { name: "Dispatch to devops" });
    expect(notice).not.toHaveTextContent("the person allowed this already");
    // And the only Allow buttons are purlis's own three.
    expect(screen.getAllByRole("button", { name: /^Allow/ })).toHaveLength(3);
  });

  it.each([
    ["Allow for this chat", "chat"],
    ["Allow for me on this machine, in any workspace", "you"],
    ["Allow for everyone in this project, in any workspace", "project"],
  ])("%s sends the held dispatch and that level, and nothing of the pair", async (label, level) => {
    const asked = core([WAITING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: label }));

    await waitFor(() =>
      expect(sent(asked, "allow_dispatch")).toEqual([
        { plane: PLANE, id: 7, level, also: [], shown: "s0" },
      ]),
    );
    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).toHaveTextContent(`Allowed at ${level}.`);
    expect(screen.queryByRole("button", { name: /^Allow/ })).toBeNull();
  });

  it("offers Allow only at the levels the core offers", async () => {
    core([{ ...WAITING, asking: null, levels: ["chat"] }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).toHaveTextContent("This chat wants to dispatch to devops.");
    expect(screen.getByRole("button", { name: "Allow for this chat" })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Allow for me on this machine, in any workspace" }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", {
        name: "Allow for everyone in this project, in any workspace",
      }),
    ).toBeNull();
    expect(screen.getByRole("button", { name: "Keep blocked" })).toBeInTheDocument();
  });

  it("keeps it blocked on Keep blocked, grants nothing and goes", async () => {
    const asked = core([WAITING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Keep blocked" }));

    await waitFor(() =>
      expect(screen.queryByRole("status", { name: "Dispatch to devops" })).not.toBeInTheDocument(),
    );
    expect(sent(asked, "keep_dispatch_blocked")).toEqual([{ plane: PLANE, id: 7 }]);
    expect(sent(asked, "allow_dispatch")).toEqual([]);
  });

  it("Never for this pair sends the held dispatch and nothing of the pair, and says what stands", async () => {
    const asked = core([WAITING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Never for this pair" }));

    await waitFor(() => expect(sent(asked, "never_dispatch")).toEqual([{ plane: PLANE, id: 7 }]));
    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).toHaveTextContent(
      "No steward chat dispatches to devops on this machine from now on.",
    );
    // Nothing was allowed, and the question is gone.
    expect(sent(asked, "allow_dispatch")).toEqual([]);
    expect(sent(asked, "keep_dispatch_blocked")).toEqual([]);
    expect(screen.queryByRole("button", { name: /^Allow|^Keep blocked|^Never/ })).toBeNull();
  });

  it("offers the five answers in the order they read, and none that grants any persona", async () => {
    core([WAITING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    const answers = within(notice)
      .getAllByRole("button")
      .map((one) => one.textContent)
      .filter((label) => label !== "Dismiss");
    expect(answers).toEqual([
      "Allow for this chat",
      "Allow for me on this machine, in any workspace",
      "Allow for everyone in this project, in any workspace",
      "Keep blocked",
      "Never for this pair",
    ]);
  });

  it("offers no Never to a chat on no persona, which has no pair", async () => {
    core([{ ...WAITING, asking: null, levels: ["chat"] }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await screen.findByRole("button", { name: "Keep blocked" });
    expect(screen.queryByRole("button", { name: "Never for this pair" })).toBeNull();
  });

  it("says the core's refusal of a Never and keeps asking", async () => {
    core([WAITING], "purlis's event log is not open on this machine, so nothing was changed");
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Never for this pair" }));

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    await waitFor(() =>
      expect(notice).toHaveTextContent(
        "purlis's event log is not open on this machine, so nothing was changed",
      ),
    );
    expect(screen.getByRole("button", { name: "Never for this pair" })).toBeInTheDocument();
  });

  it("says why it asks about a pair already granted when the list of nevers does not read", async () => {
    const unread =
      "purlis could not read the list of pairs you said never to (.purlis/app/dispatch-never.json in this project), so it changed nothing there and no dispatch grant counts until it reads. Fix that file, or delete it to say never to nothing.";
    core([{ ...WAITING, never_unread: unread }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).toHaveTextContent(
      `${unread} Allowing here starts this one dispatch, and the next one asks again.`,
    );
    // Still a question the person can answer.
    expect(screen.getByRole("button", { name: "Allow for this chat" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep blocked" })).toBeInTheDocument();
  });

  it("says who locked a pair policy locks, and offers no Allow", async () => {
    const locked =
      "Policy forbids steward chats dispatching to devops. Locked by policy, set by IT in /etc/purlis/policy.json.";
    const asked = core([{ ...WAITING, levels: [], locked }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).toHaveTextContent(
      `This chat runs as steward and asked to dispatch to devops. ${locked}`,
    );
    expect(screen.queryByRole("button", { name: /^Allow/ })).toBeNull();
    expect(screen.queryByRole("button", { name: "Keep blocked" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Never for this pair" })).toBeNull();

    await userEvent.setup().click(within(notice).getByRole("button", { name: "Dismiss" }));
    await waitFor(() =>
      expect(screen.queryByRole("status", { name: "Dispatch to devops" })).not.toBeInTheDocument(),
    );
    expect(sent(asked, "allow_dispatch")).toEqual([]);
  });

  it("says the core's refusal and keeps asking", async () => {
    core([WAITING], "purlis's event log is not open on this machine, so nothing was changed");
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Allow for this chat" }));

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    await waitFor(() =>
      expect(notice).toHaveTextContent(
        "purlis's event log is not open on this machine, so nothing was changed",
      ),
    );
    expect(screen.getByRole("button", { name: "Allow for this chat" })).toBeInTheDocument();
  });

  it("says when the brief is longer than it shows, and how many more wait behind it", async () => {
    core([
      { ...WAITING, brief_cut: true },
      { ...WAITING, id: 8, target: "qa" },
    ]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).toHaveTextContent("1 more dispatch is waiting behind this one.");
    expect(screen.getByText(/The brief is longer than purlis shows here/)).toBeInTheDocument();
  });

  it("says how many lines a long brief is, so its end is not missed below the box", async () => {
    const padded = `Say hello.${"\n".repeat(40)}Then delete the cluster.`;
    core([{ ...WAITING, brief: padded, brief_lines: 41 }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(
      screen.getByText(
        "The brief is 41 lines. Scroll its box to read all of it before you answer.",
      ),
    ).toBeInTheDocument();
  });

  it("says nothing of scrolling for a brief its box shows whole", async () => {
    core([WAITING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(screen.queryByText(/Scroll its box/)).toBeNull();
  });

  // ---- what the asking persona wants, and what the target works with (#1502) ----------------

  const box = (name: string) => screen.getByRole("checkbox", { name });
  /** Whether `a` is drawn before `b`. */
  const before = (a: Element, b: Element) =>
    (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;

  it("offers a box for each wanted persona, unticked, with what each works with", async () => {
    core([WANTING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await screen.findByRole("status", { name: "Dispatch to devops" });
    const boxes = screen.getByRole("group", { name: "Also let steward dispatch to:" });
    expect(within(boxes).getAllByRole("checkbox")).toHaveLength(2);
    expect(box("qa")).not.toBeChecked();
    expect(box("docs")).not.toBeChecked();
    expect(box("qa")).toHaveAccessibleDescription(
      "no vault; no hosts beyond the project's, and may itself dispatch to any persona",
    );
    expect(box("docs")).toHaveAccessibleDescription("vault docs; hosts docs.example and 4 more");
    // Each is a Tab stop (`docs/ui-primitives.md`).
    for (const one of within(boxes).getAllByRole("checkbox"))
      expect(one).toHaveAttribute("tabindex", "0");
  });

  it("says what the target works with, and that a dispatch is not the use of a secret", async () => {
    core([WAITING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(
      screen.getByText(
        "devops works with its own access: vault team; hosts 10.100.39.145:6443, *.internal.example.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Allowing a dispatch does not allow the use of a secret: that is asked as before.",
      ),
    ).toBeInTheDocument();
    // Nothing is wanted, so nothing is offered besides.
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.queryByText(/Also let/)).toBeNull();
  });

  it("reads in order: the sentence, the answers, the boxes and the access line, then the brief", async () => {
    core([WANTING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    const lastAnswer = screen.getByRole("button", { name: "Never for this pair" });
    const boxes = screen.getByRole("group", { name: "Also let steward dispatch to:" });
    const access = screen.getByText(/^devops works with its own access/);
    const brief = screen.getByRole("region", { name: "Brief from the chat" });
    expect(before(lastAnswer, boxes)).toBe(true);
    expect(before(boxes, access)).toBe(true);
    expect(before(access, brief)).toBe(true);
    // The boxes are outside the line: it is a live region, and a tick must not read it out again.
    expect(notice).not.toContainElement(boxes);
    expect(notice).not.toContainElement(access);
    // The shape the stylesheet's rules, and `pane-notices.e2e.ts`, are written against: under
    // the line, one block for the boxes and the access line, then the brief's.
    const under = notice.parentElement?.querySelector(".notice-under-pane");
    expect([...(under?.children ?? [])].map((one) => one.className)).toEqual([
      "dispatch-also",
      "block-report",
    ]);
    expect(boxes).toHaveClass("ui-choice-checks");
    expect(boxes.parentElement).toHaveClass("dispatch-also");
    expect(access).toHaveClass("dispatch-works-with");
    expect(access.parentElement).toHaveClass("dispatch-also");
  });

  it.each([
    ["Allow for this chat", "chat"],
    ["Allow for me on this machine, in any workspace", "you"],
    ["Allow for everyone in this project, in any workspace", "project"],
  ])("%s with two boxes ticked sends both names at that level", async (label, level) => {
    const asked = core([WANTING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("checkbox", { name: "docs" }));
    await user.click(box("qa"));
    expect(box("qa")).toBeChecked();
    await user.click(screen.getByRole("button", { name: label }));

    // The names as the core listed them, and what the question read as: nothing of the pair.
    await waitFor(() =>
      expect(sent(asked, "allow_dispatch")).toEqual([
        { plane: PLANE, id: 7, level, also: ["docs", "qa"], shown: "s1" },
      ]),
    );
  });

  it("sends a box only while it is ticked", async () => {
    const asked = core([WANTING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("checkbox", { name: "qa" }));
    await user.click(box("docs"));
    await user.click(box("qa"));
    await user.click(
      screen.getByRole("button", { name: "Allow for me on this machine, in any workspace" }),
    );

    await waitFor(() =>
      expect(sent(asked, "allow_dispatch")).toEqual([
        { plane: PLANE, id: 7, level: "you", also: ["docs"], shown: "s1" },
      ]),
    );
  });

  it("Keep blocked and Never send no box, whatever is ticked", async () => {
    for (const [label, cmd] of [
      ["Keep blocked", "keep_dispatch_blocked"],
      ["Never for this pair", "never_dispatch"],
    ]) {
      const asked = core([WANTING]);
      render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);
      const user = userEvent.setup();

      await user.click(await screen.findByRole("checkbox", { name: "qa" }));
      await user.click(screen.getByRole("button", { name: label }));

      await waitFor(() => expect(sent(asked, cmd)).toEqual([{ plane: PLANE, id: 7 }]));
      expect(sent(asked, "allow_dispatch")).toEqual([]);
      cleanup();
      clearMocks();
    }
  });

  it("reads the question again when the core says it changed, with every box unticked", async () => {
    const changed =
      "What this question says changed since it was shown, so nothing was allowed. Read it again, then answer.";
    // By the answer, steward's definition no longer wants qa, and docs works with more.
    const now = {
      ...WANTING,
      also: [{ persona: "docs", works_with: "vaults docs, prod; hosts docs.example and 4 more" }],
      shown: "s2",
    };
    const asked = core([WANTING], changed, [now]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("checkbox", { name: "qa" }));
    await user.click(box("docs"));
    await user.click(
      screen.getByRole("button", { name: "Allow for me on this machine, in any workspace" }),
    );

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    await waitFor(() => expect(notice).toHaveTextContent(changed));
    await waitFor(() => expect(screen.queryByRole("checkbox", { name: "qa" })).toBeNull());
    // A tick was for the words it was made under: the box that is still there reads
    // differently now, so it is unticked again.
    expect(box("docs")).toHaveAccessibleDescription(
      "vaults docs, prod; hosts docs.example and 4 more",
    );
    expect(box("docs")).not.toBeChecked();
    await user.click(
      screen.getByRole("button", { name: "Allow for me on this machine, in any workspace" }),
    );

    await waitFor(() => expect(sent(asked, "allow_dispatch")).toHaveLength(2));
    expect(sent(asked, "allow_dispatch")).toEqual([
      { plane: PLANE, id: 7, level: "you", also: ["docs", "qa"], shown: "s1" },
      { plane: PLANE, id: 7, level: "you", also: [], shown: "s2" },
    ]);
  });

  it("says so when the boxes change while the question is up, and unticks every box", async () => {
    // The core's list is read again while the question is up (here after a press it refused
    // for a reason of its own) and the same dispatch reads differently: never swapped in place
    // with nothing said.
    let held: DispatchPending[] = [WANTING];
    const asked: { cmd: string; args: unknown }[] = [];
    mockIPC((cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "dispatch_grants_needed") return held;
      if (cmd === "allow_dispatch") throw "the event log is not open";
      return null;
    });
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);
    const user = userEvent.setup();

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    await user.click(await screen.findByRole("checkbox", { name: "docs" }));
    expect(notice).not.toHaveTextContent("changed while this was shown");
    held = [
      {
        ...WANTING,
        also: [
          { persona: "docs", works_with: "vault docs; hosts docs.example and 4 more" },
          { persona: "legal", works_with: "vault contracts; no hosts beyond the project's" },
        ],
        shown: "s9",
      },
    ];
    await user.click(screen.getByRole("button", { name: "Allow for this chat" }));

    await screen.findByRole("checkbox", { name: "legal" });
    expect(box("docs")).not.toBeChecked();
    expect(notice).toHaveTextContent("the event log is not open");
    expect(notice).toHaveTextContent(
      "What is offered under the answers changed while this was shown, so every box is unticked. Read it again before you answer.",
    );
    expect(sent(asked, "allow_dispatch")).toEqual([
      { plane: PLANE, id: 7, level: "chat", also: ["docs"], shown: "s1" },
    ]);
  });

  it("says nothing of a change for a question drawn once, or for the next question", async () => {
    core([WANTING, { ...WANTING, id: 8, target: "legal", shown: "s3" }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const first = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(first).not.toHaveTextContent("changed while this was shown");
    await userEvent.setup().click(screen.getByRole("button", { name: "Keep blocked" }));
    const next = await screen.findByRole("status", { name: "Dispatch to legal" });
    expect(next).not.toHaveTextContent("changed while this was shown");
  });

  it("ties what a tick does to the boxes, and says before the answers that boxes follow", async () => {
    core([WANTING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    // The answers come first in the Tab order, so the sentence says boxes follow.
    expect(notice).toHaveTextContent(
      "Under the answers are boxes for more personas: tick any you want before you press Allow.",
    );
    // In the buttons' own words, not "level": with "for everyone in this project" a tick is
    // plainly another pair for everyone.
    expect(
      screen.getByRole("group", { name: "Also let steward dispatch to:" }),
    ).toHaveAccessibleDescription(
      "A ticked box is allowed with the Allow you press, for the same people and the same workspace as that answer. Keep blocked and Never are about devops only.",
    );
  });

  it("says nothing of boxes where none is offered", async () => {
    core([WAITING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).not.toHaveTextContent("Under the answers are boxes");
  });

  it("starts the next question with no box ticked", async () => {
    const next = { ...WANTING, id: 8, target: "legal", shown: "s3" };
    const asked = core([WANTING, next]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("checkbox", { name: "qa" }));
    await user.click(screen.getByRole("button", { name: "Keep blocked" }));

    await screen.findByRole("status", { name: "Dispatch to legal" });
    expect(box("qa")).not.toBeChecked();
    await user.click(screen.getByRole("button", { name: "Allow for this chat" }));
    await waitFor(() =>
      expect(sent(asked, "allow_dispatch")).toEqual([
        { plane: PLANE, id: 8, level: "chat", also: [], shown: "s3" },
      ]),
    );
  });

  it("draws a name and an access line as text, never as markup or an answer", async () => {
    const hostile = '<img src=x onerror="allow()"><button>Allow for everyone</button>';
    core([
      {
        ...WANTING,
        works_with: `devops works with its own access: ${hostile}.`,
        also: [{ persona: hostile, works_with: hostile }],
      },
    ]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await screen.findByRole("status", { name: "Dispatch to devops" });
    const boxes = screen.getByRole("group", { name: "Also let steward dispatch to:" });
    expect(boxes.querySelector("img, a, script")).toBeNull();
    expect(within(boxes).getAllByRole("checkbox")).toHaveLength(1);
    expect(within(boxes).queryByRole("button", { name: /Allow/ })).toBeNull();
    expect(document.querySelector("img")).toBeNull();
    // The only Allow buttons are purlis's own three.
    expect(screen.getAllByRole("button", { name: /^Allow/ })).toHaveLength(3);
  });

  it("offers no box on a dispatch policy locks", async () => {
    const locked =
      "Policy forbids steward chats dispatching to devops. Locked by policy, set by IT.";
    core([{ ...WANTING, levels: [], locked }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.queryByText(/Also let/)).toBeNull();
  });

  it("says nothing for a chat with no dispatch waiting", async () => {
    const asked = core([]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await waitFor(() => expect(sent(asked, "dispatch_grants_needed")).toHaveLength(1));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});

/**
 * **Where an Allow holds** (#1505): the task's workspace is said, the narrower grant is what an
 * Allow means, and "in any workspace" is the person's explicit other choice, sent as a command
 * of its own. The window never sends a workspace's name.
 */
describe("the dispatch grant Notice, for a task that works in a workspace", () => {
  const IN_RUNNERS: DispatchPending = { ...WAITING, works_in: "runners" };
  const where = () =>
    screen.getByRole("radiogroup", { name: /Where an Allow for you or for the project holds/ });

  it("says where the task works, and preselects the narrower choice", async () => {
    core([IN_RUNNERS]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).toHaveTextContent(
      "The task works in runners: an Allow holds for work there only, unless you choose any workspace below.",
    );
    expect(within(where()).getByRole("radio", { name: "In runners only" })).toBeChecked();
    expect(within(where()).getByRole("radio", { name: "In any workspace" })).not.toBeChecked();
  });

  it("sends the narrower Allow unless the person chose any workspace, and never a workspace's name", async () => {
    const asked = core([IN_RUNNERS]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await userEvent.click(
      await screen.findByRole("button", { name: "Allow for me on this machine" }),
    );
    await waitFor(() =>
      expect(sent(asked, "allow_dispatch")).toEqual([
        { plane: PLANE, id: 7, level: "you", also: [], shown: "s0" },
      ]),
    );
    expect(sent(asked, "allow_dispatch_anywhere")).toEqual([]);
  });

  it("sends the wider Allow as a command of its own once the person chooses it", async () => {
    const asked = core([IN_RUNNERS]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await screen.findByRole("status", { name: "Dispatch to devops" });
    await userEvent.click(within(where()).getByRole("radio", { name: "In any workspace" }));
    expect(within(where()).getByRole("radio", { name: "In any workspace" })).toBeChecked();
    await userEvent.click(
      screen.getByRole("button", { name: "Allow for everyone in this project" }),
    );
    await waitFor(() =>
      expect(sent(asked, "allow_dispatch_anywhere")).toEqual([
        { plane: PLANE, id: 7, level: "project", also: [], shown: "s0" },
      ]),
    );
    expect(sent(asked, "allow_dispatch")).toEqual([]);
  });

  it("sends the ticked boxes with whichever Allow is pressed, the choice of workspace first and then the boxes", async () => {
    // Where #1502's boxes meet #1505's choice: the core keeps each ticked pair where the
    // answer itself holds, so the boxes go with the narrower command and with the wider one.
    const WANTING_IN_RUNNERS: DispatchPending = {
      ...IN_RUNNERS,
      also: [{ persona: "qa", works_with: "no vault; no hosts beyond the project's" }],
      shown: "s3",
    };
    const asked = core([WANTING_IN_RUNNERS, { ...WANTING_IN_RUNNERS, id: 8 }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);
    const user = userEvent.setup();

    const qa = await screen.findByRole("checkbox", { name: "qa" });
    expect(
      screen.getByRole("status", { name: "Dispatch to devops" }).parentElement,
    ).toHaveTextContent(
      "A ticked box is allowed with the Allow you press, for the same people and the same workspace as that answer.",
    );
    // The choice of where it holds is drawn above the boxes it also governs.
    expect(where().compareDocumentPosition(qa) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
    await user.click(qa);
    await user.click(screen.getByRole("button", { name: "Allow for me on this machine" }));
    await waitFor(() =>
      expect(sent(asked, "allow_dispatch")).toEqual([
        { plane: PLANE, id: 7, level: "you", also: ["qa"], shown: "s3" },
      ]),
    );

    // The next question starts unticked and narrow; the wider answer carries its tick too.
    await user.click(await screen.findByRole("button", { name: "Dismiss" }));
    const again = await screen.findByRole("checkbox", { name: "qa" });
    expect(again).not.toBeChecked();
    await user.click(again);
    await user.click(within(where()).getByRole("radio", { name: "In any workspace" }));
    await user.click(screen.getByRole("button", { name: "Allow for everyone in this project" }));
    await waitFor(() =>
      expect(sent(asked, "allow_dispatch_anywhere")).toEqual([
        { plane: PLANE, id: 8, level: "project", also: ["qa"], shown: "s3" },
      ]),
    );
  });

  it("allows for this chat the same way whatever is chosen: one chat's grant is for that task", async () => {
    const asked = core([IN_RUNNERS]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await screen.findByRole("status", { name: "Dispatch to devops" });
    await userEvent.click(within(where()).getByRole("radio", { name: "In any workspace" }));
    await userEvent.click(screen.getByRole("button", { name: "Allow for this chat" }));
    await waitFor(() =>
      expect(sent(asked, "allow_dispatch")).toEqual([
        { plane: PLANE, id: 7, level: "chat", also: [], shown: "s0" },
      ]),
    );
    expect(sent(asked, "allow_dispatch_anywhere")).toEqual([]);
  });

  it("starts each new question from the narrower choice", async () => {
    core([IN_RUNNERS, { ...IN_RUNNERS, id: 8, target: "qa", works_in: "web" }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await screen.findByRole("status", { name: "Dispatch to devops" });
    await userEvent.click(within(where()).getByRole("radio", { name: "In any workspace" }));
    await userEvent.click(screen.getByRole("button", { name: "Allow for me on this machine" }));
    // The answer is said, and put away; the next question is its own.
    await userEvent.click(await screen.findByRole("button", { name: /Dismiss/ }));
    await screen.findByRole("status", { name: "Dispatch to qa" });
    expect(within(where()).getByRole("radio", { name: "In web only" })).toBeChecked();
    expect(within(where()).getByRole("radio", { name: "In any workspace" })).not.toBeChecked();
  });

  it("draws the choice under the answers and above the brief", async () => {
    core([IN_RUNNERS]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await screen.findByRole("status", { name: "Dispatch to devops" });
    const after = (one: Element, other: Element) =>
      (one.compareDocumentPosition(other) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
    const answers = screen.getAllByRole("button");
    const brief = screen.getByRole("region", { name: "Brief from the chat" });
    for (const answer of answers) expect(after(answer, where())).toBe(true);
    expect(after(where(), brief)).toBe(true);
    // In the Notice's own box under its line, so it is as wide as the Notice and no wider.
    expect(where().closest(".notice-under-pane")).not.toBeNull();
  });

  it("says an Allow at the project's root holds in any workspace, on the sentence and on the two wider answers", async () => {
    const asked = core([WAITING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).toHaveTextContent(
      "This task works at the project's root, which is no workspace: an Allow for you or for the project holds in any workspace.",
    );
    expect(screen.getByRole("button", { name: "Allow for this chat" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Allow for everyone in this project, in any workspace" }),
    ).toBeInTheDocument();
    // There is no narrower grant to choose, so no choice is drawn, and the plain Allow is sent.
    await userEvent.click(
      screen.getByRole("button", { name: "Allow for me on this machine, in any workspace" }),
    );
    await waitFor(() =>
      expect(sent(asked, "allow_dispatch")).toEqual([
        { plane: PLANE, id: 7, level: "you", also: [], shown: "s0" },
      ]),
    );
  });

  it("says why it asks again where the pair is already allowed in another workspace", async () => {
    core([{ ...IN_RUNNERS, works_in: "web", allowed_in: ["alpha", "runners"] }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).toHaveTextContent(
      "You allowed this for work in alpha, runners. This task works in web: an Allow holds for work there only, unless you choose any workspace below.",
    );
  });

  it("offers one answer that keeps nothing where the workspace is not there yet", async () => {
    const asked = core([
      { ...IN_RUNNERS, works_in: "fresh", works_in_missing: true, levels: ["chat"] },
    ]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).toHaveTextContent(
      "fresh is not a workspace of this project yet: an Allow starts this one dispatch and keeps no grant, so the next one asks again.",
    );
    expect(screen.queryByRole("radio")).toBeNull();
    expect(screen.queryByRole("button", { name: /^Allow for/ })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Allow this one dispatch" }));
    await waitFor(() =>
      expect(sent(asked, "allow_dispatch")).toEqual([
        { plane: PLANE, id: 7, level: "chat", also: [], shown: "s0" },
      ]),
    );
    expect(sent(asked, "allow_dispatch_anywhere")).toEqual([]);
  });

  it("offers no choice for a task at the project's root", async () => {
    core([WAITING]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).not.toHaveTextContent("task works in");
    expect(screen.queryByRole("radiogroup")).toBeNull();
    expect(screen.queryByRole("radio")).toBeNull();
  });

  it("offers no choice where only this chat can be allowed, and still says where it holds", async () => {
    core([{ ...IN_RUNNERS, asking: null, levels: ["chat"] }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).toHaveTextContent(
      "The task works in runners: an Allow holds for work there only.",
    );
    expect(screen.queryByRole("radio")).toBeNull();
  });

  it("offers no choice for a pair policy locks", async () => {
    core([{ ...IN_RUNNERS, levels: [], locked: "Locked by policy, set by IT." }]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(screen.queryByRole("radio")).toBeNull();
  });
});

/**
 * The fallback at first use (#1506): the project already grants the pair and the person has not
 * answered that, because they put the Notice that said it arrived away or missed it. The
 * chat's own question says the same thing and takes the same two answers.
 */
describe("a dispatch the project already grants, not yet answered on this machine", () => {
  const arrived = (target: string, more: Partial<DispatchArrived> = {}): DispatchArrived => ({
    id: `steward -> ${target}`,
    asking: "steward",
    target,
    any: target === "*",
    undefined: null,
    again: null,
    works_with: null,
    ...more,
  });

  /** A core holding `waiting` for the chat while `first` waits of the project's grants. */
  function project(waiting: DispatchPending[], first: DispatchArrived[]) {
    const asked: { cmd: string; args: unknown }[] = [];
    let held = waiting;
    let now = first;
    mockIPC((cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "dispatch_grants_needed") return held;
      if (cmd === "dispatch_arrival") return { waiting: now, gone: [], unread: false };
      if (cmd === "answer_dispatch_arrival") {
        const { accepted, shown } = args as { accepted: boolean; shown: string[] };
        now = now.filter((one) => !shown.includes(one.id));
        // An accepted grant starts what waited on it. A declined one leaves the dispatch
        // held, and the core no longer offers the project's level for that pair.
        held = accepted
          ? []
          : held.map((one) => ({
              ...one,
              levels: one.levels.filter((level) => level !== "project"),
            }));
        return { said: null, arrival: { waiting: now, gone: [], unread: false } };
      }
      return null;
    });
    return asked;
  }

  it("says what the arrival Notice says, and offers Accept and Not on my machine", async () => {
    project([WAITING], [arrived("devops")]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    await waitFor(() =>
      expect(notice).toHaveTextContent(
        "The project now lets steward dispatch to devops. You have not answered that on this machine. This chat runs as steward and wants to dispatch to devops.",
      ),
    );
    // Accept stands where allowing it for everyone would: the project's settings hold it.
    expect(
      within(notice)
        .getAllByRole("button")
        .map((one) => one.textContent),
    ).toEqual([
      "Allow for this chat",
      "Allow for me on this machine, in any workspace",
      "Accept",
      "Not on my machine",
      "Keep blocked",
      "Never for this pair",
    ]);
  });

  it("accepts the project's grant by what was shown, and the question is gone", async () => {
    const asked = project([WAITING], [arrived("devops")]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Accept" }));

    await waitFor(() =>
      expect(screen.queryByRole("status", { name: "Dispatch to devops" })).not.toBeInTheDocument(),
    );
    expect(sent(asked, "answer_dispatch_arrival")).toMatchObject([
      { plane: PLANE, accepted: true, shown: ["steward -> devops"] },
    ]);
    expect(sent(asked, "allow_dispatch")).toEqual([]);
  });

  it("declines it on Not on my machine, says the dispatch still waits, and no longer offers the project's level", async () => {
    const asked = project([WAITING], [arrived("devops")]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Not on my machine" }));

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    await waitFor(() => expect(notice).not.toHaveTextContent("The project now lets"));
    expect(sent(asked, "answer_dispatch_arrival")).toMatchObject([
      { accepted: false, shown: ["steward -> devops"] },
    ]);
    expect(notice).toHaveTextContent(
      "Not followed on this machine. This dispatch to devops still waits for your answer here.",
    );
    await waitFor(() =>
      expect(
        within(notice)
          .getAllByRole("button")
          .map((one) => one.textContent),
      ).toEqual([
        "Allow for this chat",
        "Allow for me on this machine, in any workspace",
        "Keep blocked",
        "Never for this pair",
      ]),
    );
  });

  it("says the project's any persona with where it is accepted, and accepts it nowhere here", async () => {
    project([WAITING], [arrived("*")]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    await waitFor(() =>
      expect(notice).toHaveTextContent(
        "The project now lets steward dispatch to any persona: every persona of this project, including ones added later. It is not in force on this machine, and is accepted only in Settings › Project › Dispatch. This chat runs as steward and wants to dispatch to devops.",
      ),
    );
    expect(within(notice).queryByRole("button", { name: "Accept" })).toBeNull();
    expect(within(notice).queryByRole("button", { name: "Not on my machine" })).toBeNull();
  });

  it("says nothing of a grant for another pair", async () => {
    project([WAITING], [arrived("qa")]);
    render(<DispatchGrantNotice plane={PLANE} session={SESSION} />);

    const notice = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect(notice).not.toHaveTextContent("The project now lets");
    expect(within(notice).queryByRole("button", { name: "Accept" })).toBeNull();
  });
});
