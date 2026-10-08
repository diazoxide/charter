import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import type { FinishedTask } from "./bindings";
import { ChatsSection } from "./ChatsSection";
import { setChatsListPrefs } from "./chatsListPrefs";
import {
  ChatsHere,
  fixedChats,
  moved,
  nothingKnown,
  type ChatStates,
  type State,
} from "./chatState";
import { chatsTree, type ListedChat } from "./chatsTree";
import { forgetThisLaunch, SLOTS } from "./regions";
import { MOST_TEXT } from "./textSize";

/**
 * **The Chats list as the e2e measures it** (#1499): the real component, drawn here with a
 * chat in every shape a row has, and kept as markup in `e2e/fixtures/`.
 * `e2e/specs/chats-list.e2e.ts` puts that markup into the real window and measures it against
 * the built stylesheet in the real engine.
 *
 * The e2e suite's fake harness cannot make these chats (a task five levels down that ended
 * without a report, a session folded over five finished tasks), and jsdom cannot lay anything
 * out. So each does its half, and **this file is what ties them**: the markup the e2e measures
 * is what the component draws, or this test fails. After a change to what a row draws, run
 * `npx vitest run src/ChatsList.fixture.test.tsx -u` and commit the fixtures.
 */

const LONG = "a_name_with_no_space_in_it_that_is_far_wider_than_a_sidebar";

function listed(session: number, more: Partial<ListedChat> = {}): ListedChat {
  return {
    session,
    name: `steward ${session}`,
    persona: "steward",
    workspace: "smart-ide",
    shell: false,
    parent: null,
    mode: null,
    from: null,
    tab: true,
    branch: null,
    report: null,
    outcome: null,
    asking: null,
    harness: "Claude Code",
    ...more,
  };
}

const task = (session: number, parent: number, more: Partial<ListedChat> = {}) =>
  listed(session, {
    persona: "devops",
    parent,
    mode: "task",
    from: `chat ${parent}`,
    tab: false,
    report: "owed",
    ...more,
  });

/** A chat in every shape a row has: five levels, each state's longest word, every extra. */
const CHATS: ListedChat[] = [
  // Working, with a task below it that needs the person: it wears the rolled-up hand.
  listed(1, { name: "cancel mid-turn smart-ide" }),
  // Needs the person; works elsewhere, on a branch of its own: the longest second line.
  task(2, 1, {
    name: "live check of the staging cluster after the release",
    workspace: "volaticloud",
    branch: "purlis/dispatch/live-check-of-the-staging-cluster",
  }),
  // Asking a chat whose name has no space to break at.
  task(3, 2, { name: "devops 3", asking: LONG }),
  // Ended without a report: the longest word a state has.
  task(4, 3, { name: "devops 4", report: "failed" }),
  // The same, five levels down, under a name that cannot break.
  task(5, 4, { name: LONG, report: "failed" }),
  // Beside it, a task still at work: what keeps the four rows above it open.
  task(9, 4, { name: "devops 9" }),
  // Nothing heard from its harness, and being stopped.
  listed(6),
  // Idle, folded by itself over five finished tasks.
  listed(7),
  // Idle, open over a finished task that ended without a report.
  listed(8),
];

/** The core's typed end for each of its words for one (`FinishedTask.how`, #1485). */
const HOW: Record<string, FinishedTask["how"]> = {
  done: "done",
  cancelled: "cancelled",
  blocked: "blocked",
  failed: "failed",
  "ended without a report": "unreported",
  "closed by the person": "stopped_by_person",
};

function finished(id: string, asker: number, more: Partial<FinishedTask> = {}): FinishedTask {
  return {
    how: HOW[more.outcome ?? "done"],
    chat: null,
    not_reopened: null,
    did_not_start: false,
    attempts: 0,
    waits: null,
    id,
    asker,
    name: `check ${id}`,
    persona: "devops",
    outcome: "done",
    folds: true,
    report: "All good.",
    changed: null,
    ended: "2026-10-08T12:04:30+00:00",
    place: "smart-ide",
    branch: null,
    reopens: true,
    ...more,
  };
}

const FINISHED = new Map<number, FinishedTask[]>([
  [7, ["a", "b", "c", "d", "e"].map((id) => finished(id, 7))],
  [
    8,
    [
      finished("f", 8, {
        name: "live check of the staging cluster after the release",
        outcome: "ended without a report",
        folds: false,
      }),
      finished("g", 8, { name: LONG, outcome: "blocked", folds: false }),
      finished("h", 8),
    ],
  ],
]);

/** What each chat is doing, as the core would have said it. */
function states(): ChatStates {
  const doing: [number, State][] = [
    [1, "running"],
    [2, "waiting"],
    [3, "running"],
    [4, "failed"],
    [5, "failed"],
    [9, "running"],
    [7, "waiting"],
    [8, "waiting"],
  ];
  return doing.reduce(
    (known, [session, state], at) =>
      moved(known, {
        plane: "/plane",
        session,
        state,
        needs_you: session === 2,
        queue: at >= 1 ? [2] : [],
        moved_at: at + 1,
        sequence: at + 1,
        reports: [],
        refusals: [],
        children: [],
        needs: null,
        stopped: null,
      }),
    nothingKnown,
  );
}

function markup(): string {
  const { container } = render(
    <ChatsHere.Provider value={fixedChats(states())}>
      <ChatsSection
        rows={chatsTree(CHATS)}
        front={1}
        onOpen={() => {}}
        stopping={new Set([6])}
        finished={FINISHED}
      />
    </ChatsHere.Provider>,
  );
  const section = container.querySelector(".chats-section");
  if (section === null) throw new Error("no Chats section was drawn");
  // One element per line, so a change to a row is a few lines of the fixture's diff.
  return `${section.outerHTML.replace(/></g, ">\n<")}\n`;
}

beforeEach(forgetThisLaunch);
afterEach(cleanup);

describe("the Chats list the e2e measures", () => {
  it("is the component's own markup, on two lines", async () => {
    const html = markup();

    // What the e2e's cases are about is in it.
    for (const drawn of [
      "ended without a report",
      `asking ${LONG}`,
      'data-level="5"',
      "rolled-up",
      "Stopping…",
      "below-summary",
      "finished-task",
      'class="line two"',
    ])
      expect(html, drawn).toContain(drawn);
    await expect(html).toMatchFileSnapshot("../e2e/fixtures/chats-list.two-lines.html");
  });

  it("is the component's own markup, on one line", async () => {
    act(() => setChatsListPrefs({ lines: 1 }));
    const html = markup();

    expect(html).not.toContain('class="line two"');
    await expect(html).toMatchFileSnapshot("../e2e/fixtures/chats-list.one-line.html");
  });

  it("is measured at the least width the left region has, at the largest text", () => {
    // From the app's folder, which is where vitest runs.
    const spec = readFileSync("e2e/specs/chats-list.e2e.ts", "utf8");

    // The e2e cannot import the app's modules, so its two numbers are held to them here.
    expect(spec).toContain(`const FLOOR = "${SLOTS.left.floor}";`);
    expect(spec).toContain(`const MOST_TEXT = ${MOST_TEXT};`);
  });
});
