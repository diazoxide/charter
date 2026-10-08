import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Explorer } from "./Explorer";
import {
  ChatsHere,
  moved,
  nothingKnown,
  type Chats,
  type ChatStates,
  type State,
} from "./chatState";
import { listedChat, type ListedChat } from "./chatsTree";
import type { FinishedTask, Moved, OpenChat, Panels as PanelsModel, Piece } from "./bindings";
import type { Place } from "./pieceViews";
import type { WorkspaceState } from "./workspaceState";

/**
 * **The explorer lists a workspace's own chats, and counts the rest** (#1490, V100-4,
 * V100-14): a session's tasks are one line under its row that goes to the Chats list, and a
 * chat's helpers are a count on its row that unfolds. What a person sees and presses, against
 * the explorer drawn on its own; `ChatsSection.window.test.tsx` holds the same against the
 * whole window, where the line reaches the Chats list.
 */

/** How often the tree itself was drawn: a branch row's mark is drawn once per branch each
 *  time the explorer is, and by nothing else. */
const drawn = vi.hoisted(() => ({ tree: 0, marks: [] as string[] }));

vi.mock("./Worktree", () => ({
  WorktreeMark: () => {
    drawn.tree += 1;
    return null;
  },
}));

vi.mock("./StateShown", async (original) => {
  const real = await original<typeof import("./StateShown")>();
  return {
    ...real,
    StateShown: (props: Parameters<typeof real.StateShown>[0]) => {
      drawn.marks.push(props.shown.word);
      return <real.StateShown {...props} />;
    },
  };
});

afterEach(() => {
  cleanup();
  drawn.tree = 0;
  drawn.marks.length = 0;
});

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;
const BETA = `${PLANE}/workspaces/beta`;
const CUT = `${ALPHA}/.worktrees/svc`;

const PANELS: PanelsModel = {
  workspace: "alpha",
  repos: ["svc"],
  paths: {},
  absent: [],
  refused: [],
  todos: [],
  todos_refused: null,
  personas: ["steward"],
  persona: "steward",
  sessions: [],
  contributed: [],
};

const piece = (name: string): Piece => ({
  piece: name,
  path: `${CUT}/${name}`,
  branch: name,
  wired: true,
  stale: false,
  said: "",
});

const STATE: WorkspaceState = {
  panels: PANELS,
  repos: { workspace: "alpha", repos: [], cache_refused: null },
  pieces: { svc: [piece("one")] },
  piecesRefused: {},
  reading: false,
};

function chat(session: number, name: string, cwd: string, on: Partial<OpenChat> = {}): OpenChat {
  return {
    session,
    name,
    cwd,
    harness: "claude",
    in_front: false,
    resumed: null,
    fresh: null,
    profile: "claude",
    persona: null,
    unreported: null,
    card: null,
    guessed: null,
    pinned: false,
    label: null,
    from: null,
    ...on,
  };
}

type Lineage = NonNullable<OpenChat["from"]>;

/** Started by chat `asker` as a task, with no tab of its own. */
const taskOf = (asker: number, on: Partial<Lineage> = {}): Lineage => ({
  chat: asker,
  name: `steward ${asker}`,
  workspace: "alpha",
  task: true,
  tab: false,
  reported: false,
  unreported: false,
  ...on,
});

/** A session in alpha and five tasks of it, as the operator's five "live check" tasks. */
const FIVE: OpenChat[] = [
  chat(1, "steward 1", ALPHA),
  ...["talk", "queue", "17", "18", "19"].map((name, at) =>
    chat(at + 2, name, ALPHA, { from: taskOf(1) }),
  ),
];

function finished(asker: number, name: string, how: FinishedTask["how"]): FinishedTask {
  return {
    id: `${asker}-${name}`,
    asker,
    name,
    persona: null,
    how,
    outcome: how,
    folds: how === "done" || how === "cancelled",
    report: "",
    changed: null,
    ended: null,
    place: "",
    branch: null,
    reopens: false,
    not_reopened: null,
  };
}

/** One move of the board. */
function move(session: number, state: State, sequence: number, on: Partial<Moved> = {}): Moved {
  return {
    plane: PLANE,
    session,
    state,
    needs_you: (on.queue ?? []).includes(session),
    queue: [],
    moved_at: sequence,
    reports: [],
    refusals: [],
    sequence,
    children: [],
    ...on,
  };
}

/** The project's chats as a store that moves, as `PlaneView`'s does. */
function board(...moves: Moved[]) {
  let states: ChatStates = moves.reduce(moved, nothingKnown);
  const listeners = new Set<() => void>();
  const chats: Chats = {
    store: {
      subscribe: (listener) => {
        listeners.add(listener);
        return () => void listeners.delete(listener);
      },
      statesFor: () => states,
    },
    plane: undefined,
  };
  return {
    chats,
    /** The core says a chat moved. */
    move: (next: Moved) =>
      act(() => {
        states = moved(states, next);
        for (const listener of [...listeners]) listener();
      }),
  };
}

type Drawn = {
  chats?: OpenChat[];
  /** The project's chats, where they are more than this workspace's. */
  listed?: ListedChat[];
  finished?: ReadonlyMap<number, FinishedTask[]>;
  onShowChat?: (session: number) => void;
  onRevealChats?: (sessions: readonly number[]) => void;
  focus?: Place;
  on?: ReturnType<typeof board>;
};

function explorer(on: Drawn) {
  return (
    <ChatsHere.Provider value={(on.on ?? board()).chats}>
      <Explorer
        workspace="alpha"
        state={STATE}
        chats={on.chats ?? FIVE}
        listed={on.listed}
        finished={on.finished}
        spot={undefined}
        onPick={() => {}}
        onShowChat={on.onShowChat ?? (() => {})}
        onRevealChats={on.onRevealChats}
        offers={new Map()}
        onPress={() => {}}
        onReadAgain={() => {}}
        focus={on.focus}
      />
    </ChatsHere.Provider>
  );
}

const draw = (on: Drawn = {}) => render(explorer(on));

const tree = () => screen.getByRole("tree", { name: "Repos and branches" });
const item = (name: RegExp) => within(tree()).getByRole("treeitem", { name });
const items = () => within(tree()).getAllByRole("treeitem");
/** A row's level and its place among its siblings. */
const place = (row: HTMLElement) =>
  `${row.getAttribute("aria-level")} ${row.getAttribute("aria-posinset")}/${row.getAttribute("aria-setsize")}`;

describe("a session's tasks in the explorer", () => {
  it("are one line under the session, not a row each", () => {
    draw();

    const rows = items().map((row) => row.textContent);
    expect(rows.filter((text) => /talk|queue|17|18|19/.test(text ?? ""))).toEqual([]);
    const line = item(/5 tasks/);
    const session = item(/steward 1/);
    // Its one child, under its row.
    expect(session.closest("li")).toContainElement(line);
    expect(Number(line.getAttribute("aria-level"))).toBe(
      Number(session.getAttribute("aria-level")) + 1,
    );
    expect(`${line.getAttribute("aria-posinset")}/${line.getAttribute("aria-setsize")}`).toBe(
      "1/1",
    );
    expect(within(tree()).getAllByRole("treeitem", { name: /tasks/ })).toHaveLength(1);
  });

  it("draws no line under a session with no tasks", () => {
    draw({ chats: [chat(1, "steward 1", ALPHA), chat(2, "steward 2", ALPHA)] });

    expect(within(tree()).queryByRole("treeitem", { name: /task/ })).toBeNull();
    // A leaf, as before: nothing under it, and it says nothing of being open.
    expect(item(/steward 1/)).not.toHaveAttribute("aria-expanded");
  });

  it("says how many work, are done and failed, in the words the rows use", () => {
    const on = board(move(2, "running", 1), move(3, "running", 2), move(4, "done", 3));
    draw({
      chats: FIVE.slice(0, 4),
      finished: new Map([[1, [finished(1, "18", "done"), finished(1, "19", "done")]]]),
      on,
    });

    // Three still running (one whose program ended owing its report) and two finished.
    expect(item(/5 tasks/)).toHaveTextContent(/^5 tasks · 2 working · 2 done · 1 failed$/);
  });

  it("counts every task in one of four buckets, as the session's row and its tab do", () => {
    // Two working, one asking the chat that asked for it, one idle, one cancelled.
    const on = board(
      move(2, "running", 1),
      move(3, "running", 2),
      move(4, "running", 3),
      move(5, "waiting", 4),
      move(6, "waiting", 5),
    );
    draw({
      chats: [
        chat(1, "steward 1", ALPHA),
        chat(2, "talk", ALPHA, { from: taskOf(1) }),
        chat(3, "queue", ALPHA, { from: taskOf(1) }),
        chat(4, "asks", ALPHA, { from: taskOf(1, { asking: true }) }),
        chat(5, "rests", ALPHA, { from: taskOf(1) }),
        chat(6, "halted", ALPHA, {
          from: taskOf(1, { reported: true, outcome: "cancelled" }),
        }),
      ],
      on,
    });

    expect(item(/5 tasks/)).toHaveTextContent(/^5 tasks · 3 working · 1 waiting · 1 done$/);
  });

  it("draws each count with the mark its state has on a row, and never mutes a failure", () => {
    const on = board(move(2, "running", 1), move(3, "failed", 2), move(4, "waiting", 3));
    draw({
      chats: FIVE.slice(0, 4),
      finished: new Map([[1, [finished(1, "18", "done")]]]),
      on,
    });

    const parts = [...item(/4 tasks/).querySelectorAll<HTMLElement>(".counted")].map((part) => [
      part.getAttribute("data-bucket"),
      part.querySelector(".shape")?.getAttribute("data-shape"),
    ]);
    expect(parts).toEqual([
      ["working", "ring"],
      ["waiting", "pause"],
      ["done", "tick"],
      ["failed", "cross"],
    ]);
    // The marks are decoration: the words are the line's text.
    expect(item(/4 tasks/)).toHaveAccessibleName(
      "4 tasks · 1 working · 1 waiting · 1 done · 1 failed",
    );
  });

  it("counts the tasks of a task with the session's, at any depth", () => {
    draw({
      chats: [
        chat(1, "steward 1", ALPHA),
        chat(2, "talk", ALPHA, { from: taskOf(1) }),
        chat(3, "deeper", ALPHA, { from: taskOf(2) }),
      ],
    });

    expect(item(/2 tasks/)).toBeInTheDocument();
    expect(within(tree()).queryByRole("treeitem", { name: /deeper/ })).toBeNull();
  });

  it("asks for the session in the Chats list when the line is pressed, by pointer or by key", async () => {
    const onRevealChats = vi.fn();
    const onShowChat = vi.fn();
    draw({ onRevealChats, onShowChat });

    await userEvent.click(item(/5 tasks/));
    expect(onRevealChats).toHaveBeenLastCalledWith([1]);

    item(/5 tasks/).focus();
    await userEvent.keyboard("{Enter}");
    expect(onRevealChats).toHaveBeenCalledTimes(2);
    // It goes to the list. It opens no chat.
    expect(onShowChat).not.toHaveBeenCalled();
  });

  it("keeps a row for a task with a tab of its own, and for one whose asking chat has closed", () => {
    draw({
      chats: [
        chat(1, "steward 1", ALPHA),
        chat(2, "moved out", ALPHA, { from: taskOf(1, { tab: true }) }),
        chat(3, "left behind", ALPHA, { from: taskOf(9) }),
        chat(4, "handed on", ALPHA, { from: taskOf(1, { task: false, tab: true }) }),
      ],
    });

    expect(item(/moved out/)).toBeInTheDocument();
    expect(item(/left behind/)).toBeInTheDocument();
    expect(item(/handed on/)).toBeInTheDocument();
    // The task with a tab is still one of the session's, as its row in the Chats list
    // counts it: the line says so, and a press lands on a session with that row under it.
    expect(within(tree()).getAllByRole("treeitem", { name: /task/ })).toHaveLength(1);
    expect(item(/steward 1/).closest("li")).toContainElement(item(/^1 task · 1 working$/));
    // Beside the session, not under it: who asked whom is the Chats list's to say.
    expect(item(/moved out/).getAttribute("aria-level")).toBe(
      item(/steward 1/).getAttribute("aria-level"),
    );
  });

  it("counts a session's tasks that work in another workspace on its line", () => {
    const here = [chat(1, "steward 1", ALPHA), chat(2, "talk", ALPHA, { from: taskOf(1) })];
    const away = chat(3, "queue", BETA, { from: taskOf(1) });
    draw({
      chats: here,
      listed: [
        ...here.map((one) => listedChat(one, "alpha", one.name, one.session === 1)),
        listedChat(away, "beta", away.name, false),
      ],
    });

    expect(item(/2 tasks/)).toBeInTheDocument();
  });
});

describe("tasks working here that another workspace's chat asked for", () => {
  const asker = chat(9, "devops 9", BETA);
  const here = [
    chat(1, "steward 1", ALPHA),
    chat(7, "check prod", ALPHA, { from: taskOf(9, { name: "devops 9", workspace: "beta" }) }),
    chat(8, "check staging", ALPHA, { from: taskOf(9, { name: "devops 9", workspace: "beta" }) }),
  ];
  const listed = [
    listedChat(here[0], "alpha", "steward 1", true),
    listedChat(here[1], "alpha", "check prod", false),
    listedChat(here[2], "alpha", "check staging", false),
    listedChat(asker, "beta", "devops 9", true),
  ];

  it("are one line under the workspace, which goes to them in the Chats list", async () => {
    const onRevealChats = vi.fn();
    const on = board(move(7, "running", 1));
    draw({ chats: here, listed, onRevealChats, on });

    const line = item(/2 tasks from other places/);
    // One the board has heard from and one it has not: both are at work.
    expect(line).toHaveTextContent(/^2 tasks from other places · 2 working$/);
    expect(within(tree()).queryByRole("treeitem", { name: /check prod/ })).toBeNull();
    // A child of the workspace's row, after its chats.
    expect(line.getAttribute("aria-level")).toBe(item(/steward 1/).getAttribute("aria-level"));
    expect(place(item(/steward 1/))).toBe("2 1/3");
    expect(place(line)).toBe("2 2/3");

    await userEvent.click(line);
    expect(onRevealChats).toHaveBeenLastCalledWith([7, 8]);
  });

  it("is not drawn where every task here was asked for here", () => {
    draw();

    expect(within(tree()).queryByRole("treeitem", { name: /other workspaces/ })).toBeNull();
  });
});

describe("a branch a task works in", () => {
  const chats = [
    chat(1, "steward 1", ALPHA),
    chat(2, "talk", `${CUT}/one`, { from: taskOf(1) }),
    chat(3, "queue", `${CUT}/one`, { from: taskOf(1) }),
  ];

  it("says so under its row in the workspace's tree, in the cockpit's words", async () => {
    const onRevealChats = vi.fn();
    draw({ chats, on: board(move(2, "running", 1), move(3, "waiting", 2)), onRevealChats });

    const branch = within(tree()).getByRole("treeitem", { name: "one" });
    const line = item(/2 tasks in this branch/);
    expect(line).toHaveTextContent(/^2 tasks in this branch · 1 working · 1 waiting$/);
    // Under the branch, after its files: a child of its row.
    expect(branch.closest("li")).toContainElement(line);
    expect(Number(line.getAttribute("aria-level"))).toBe(
      Number(branch.getAttribute("aria-level")) + 1,
    );
    expect(place(line)).toMatch(/ 2\/2$/);
    // The session's own line still counts them, where the session works.
    expect(item(/steward 1/).closest("li")).toContainElement(item(/^2 tasks · /));

    await userEvent.click(line);
    expect(onRevealChats).toHaveBeenLastCalledWith([2, 3]);
  });

  it("says nothing under a branch whose tasks' session has its row there", () => {
    draw({
      chats: [
        chat(1, "steward 1", `${CUT}/one`),
        chat(2, "talk", `${CUT}/one`, { from: taskOf(1) }),
      ],
    });

    expect(within(tree()).queryByRole("treeitem", { name: /in this branch/ })).toBeNull();
    expect(item(/^1 task · /)).toBeInTheDocument();
  });
});

describe("a chat's helpers in the explorer", () => {
  const TWO = [chat(1, "ide.1", `${CUT}/one`), chat(2, "ide.2", `${CUT}/one`)];
  const helpers = (...ids: [string, string][]) =>
    move(1, "running", ids.length + 1, {
      children: ids.map(([agent, state]) => ({ agent, state })),
    });
  const THREE: [string, string][] = [
    ["a3882da5acba68a4f00d", "failed"],
    ["thread-1", "done"],
    ["thread-10", "running"],
  ];

  it("are a count on its row, folded, and no row of hashes", () => {
    draw({ chats: TWO, on: board(helpers(...THREE)) });

    const count = item(/3 helpers/);
    expect(count).toHaveAttribute("aria-expanded", "false");
    // On the chat's own line: in the chat's item, and before anything drawn under it.
    expect(item(/^ide\.1/).closest("li")).toContainElement(count);
    expect(item(/^ide\.1/).nextElementSibling).toBe(count);
    expect(within(tree()).queryByRole("treeitem", { name: /helper a3882da5|thread/ })).toBeNull();
    expect(tree()).not.toHaveTextContent("a3882da5");
    // A chat with none says nothing of helpers.
    expect(item(/^ide\.2/).closest("li")).not.toHaveTextContent(/helper/);
  });

  it("is heard on the chat's row and on the count itself", () => {
    draw({ chats: TWO, on: board(helpers(...THREE)) });

    expect(item(/^ide\.1/)).toHaveAccessibleDescription(/3 helpers/);
    expect(item(/3 helpers/)).toHaveAccessibleName("3 helpers of ide.1, 1 working, 1 failed");
    expect(item(/^ide\.2/)).not.toHaveAccessibleDescription(/helper/);
  });

  it("counts right as helpers come and go", () => {
    const on = board();
    draw({ chats: TWO, on });
    expect(within(tree()).queryByRole("treeitem", { name: /helper/ })).toBeNull();

    on.move(helpers(["thread-1", "running"]));
    expect(item(/1 helper/)).toHaveTextContent(/^1 helper/);

    on.move(move(1, "running", 5, { children: THREE.map(([agent, state]) => ({ agent, state })) }));
    expect(item(/3 helpers/)).toBeInTheDocument();

    on.move(move(1, "running", 6, { children: [] }));
    expect(within(tree()).queryByRole("treeitem", { name: /helper/ })).toBeNull();
  });

  it("says how they stand, since a helper that has ended stays counted", () => {
    // As the core sends them: an ended helper stays in the list for the conversation's life.
    const on = board(
      helpers(["thread-1", "running"], ["thread-2", "running"], ["thread-3", "done"]),
    );
    draw({ chats: TWO, on });
    expect(item(/3 helpers/)).toHaveTextContent(/^3 helpers · 2 working$/);

    on.move(
      move(1, "running", 9, {
        children: [
          { agent: "thread-1", state: "done" },
          { agent: "thread-2", state: "failed" },
          { agent: "thread-3", state: "done" },
        ],
      }),
    );
    // The total stays, the working fall away, and a failure is not behind the fold.
    expect(item(/3 helpers/)).toHaveTextContent(/^3 helpers · 1 failed$/);
    expect(item(/3 helpers/)).toHaveAccessibleName("3 helpers of ide.1, 1 failed");

    on.move(
      move(1, "running", 10, {
        children: ["thread-1", "thread-2", "thread-3"].map((agent) => ({ agent, state: "done" })),
      }),
    );
    expect(item(/3 helpers/)).toHaveTextContent(/^3 helpers$/);
  });

  it("unfolds on a press to a row per helper, by a short id and its state, and folds again", async () => {
    draw({ chats: TWO, on: board(helpers(...THREE)) });

    await userEvent.click(item(/3 helpers/));

    expect(item(/3 helpers/)).toHaveAttribute("aria-expanded", "true");
    const rows = within(tree()).getAllByRole("treeitem", { name: /^helper / });
    expect(rows.map((row) => row.textContent)).toEqual([
      "helper a3882da5… failed",
      "helper thread-1 done",
      "helper thread-10 working",
    ]);
    // The whole id is there for whoever needs it.
    expect(rows[0].querySelector(".session")).toHaveAttribute("title", "a3882da5acba68a4f00d");
    // Not on the row, where a screen reader would read a hash as its description.
    expect(rows[0]).not.toHaveAccessibleDescription();
    expect(within(tree()).getByRole("group", { name: "Helpers of ide.1" })).toBeInTheDocument();

    await userEvent.click(item(/3 helpers/));
    expect(within(tree()).queryByRole("treeitem", { name: /^helper / })).toBeNull();
  });

  it("says each unfolded helper's place in the tree", async () => {
    draw({ chats: TWO, on: board(helpers(...THREE)) });
    await userEvent.click(item(/3 helpers/));

    const level = Number(item(/^ide\.1/).getAttribute("aria-level"));
    expect(place(item(/3 helpers/))).toBe(`${level + 1} 1/1`);
    expect(
      within(tree())
        .getAllByRole("treeitem", { name: /^helper / })
        .map(place),
    ).toEqual([`${level + 2} 1/3`, `${level + 2} 2/3`, `${level + 2} 3/3`]);
  });

  it("follows a helper's state while unfolded, and a new one arriving", async () => {
    const on = board(helpers(["thread-1", "running"]));
    draw({ chats: TWO, on });
    await userEvent.click(item(/1 helper/));
    expect(item(/^helper thread-1/)).toHaveTextContent("working");

    on.move(
      move(1, "running", 9, {
        children: [
          { agent: "thread-1", state: "done" },
          { agent: "thread-2", state: "running" },
        ],
      }),
    );

    expect(item(/^helper thread-1/)).toHaveTextContent("done");
    expect(item(/^helper thread-2/)).toHaveTextContent("working");
    expect(item(/2 helpers/)).toHaveAttribute("aria-expanded", "true");
  });

  it("opens and closes from the keyboard as the rest of the tree does", async () => {
    draw({ chats: TWO, on: board(helpers(...THREE)) });
    item(/^ide\.1/).focus();

    // Right on the chat goes to its first child, which is the count.
    await userEvent.keyboard("{ArrowRight}");
    expect(item(/3 helpers/)).toHaveFocus();
    // Right opens it, and Right again goes in.
    await userEvent.keyboard("{ArrowRight}");
    expect(item(/3 helpers/)).toHaveAttribute("aria-expanded", "true");
    expect(item(/3 helpers/)).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    expect(item(/^helper a3882da5/)).toHaveFocus();
    // Down walks the helpers, then leaves them for the next chat.
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(item(/^helper thread-10/)).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(item(/^ide\.2/)).toHaveFocus();
    // Left on a helper climbs to the count, and Left there folds it.
    item(/^helper thread-1 /).focus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(item(/3 helpers/)).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(item(/3 helpers/)).toHaveAttribute("aria-expanded", "false");
    await userEvent.keyboard("{ArrowLeft}");
    expect(item(/^ide\.1/)).toHaveFocus();
    // Folded, the arrows pass what it hides.
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(item(/^ide\.2/)).toHaveFocus();
  });

  it("remembers a chat's fold while the window lives, whatever the explorer draws meanwhile", async () => {
    const on = board(helpers(...THREE));
    const view = draw({ chats: TWO, on });
    await userEvent.click(item(/3 helpers/));

    // Another workspace's chats, then this one's again.
    view.rerender(explorer({ chats: [], on }));
    expect(within(tree()).queryByRole("treeitem", { name: /helper/ })).toBeNull();
    view.rerender(explorer({ chats: TWO, on }));

    expect(item(/3 helpers/)).toHaveAttribute("aria-expanded", "true");
    expect(within(tree()).getAllByRole("treeitem", { name: /^helper / })).toHaveLength(3);
  });
});

describe("a task that needs you, with its own row gone", () => {
  it("puts the hand on its session's row, and a press goes to the task", async () => {
    const onShowChat = vi.fn();
    const on = board(move(3, "waiting", 1, { queue: [3] }));
    draw({ onShowChat, on });

    const session = item(/steward 1/);
    const hand = session.closest("li")?.querySelector<HTMLElement>("button.rolled-up");
    expect(hand).toHaveAttribute("data-mark", "needs-you");
    expect(hand).toHaveAccessibleName("Go to queue, a task of steward 1, which needs you");
    // Said on the row, where a screen reader is.
    expect(session).toHaveAccessibleDescription(/queue.*needs you/);
    // And counted on the line.
    expect(item(/5 tasks/)).toHaveTextContent("1 waiting");

    await userEvent.click(hand as HTMLElement);
    expect(onShowChat).toHaveBeenCalledWith(3);
  });

  it("leads to the task that has waited longest, and comes off when none waits", () => {
    const on = board(move(3, "waiting", 1, { queue: [3] }));
    draw({ on });
    const hand = () =>
      item(/steward 1/)
        .closest("li")
        ?.querySelector<HTMLElement>("button.rolled-up") ?? null;

    on.move(move(5, "waiting", 2, { queue: [5, 3] }));
    expect(hand()).toHaveAttribute("data-leads-to", "5");

    on.move(move(5, "running", 3, { queue: [] }));
    expect(hand()).toBeNull();
    expect(item(/steward 1/)).not.toHaveAccessibleDescription(/needs you/);
  });

  it("wears no hand for a session that itself needs you: its own state says so", () => {
    const on = board(move(1, "waiting", 1, { queue: [1] }));
    draw({ on });

    expect(item(/steward 1/)).toHaveTextContent("needs you");
    expect(
      item(/steward 1/)
        .closest("li")
        ?.querySelector("button.rolled-up"),
    ).toBeNull();
  });
});

describe("a task changing state (SC-3)", () => {
  it("redraws its session's one line, and neither the tree nor another row's state", () => {
    const on = board(move(2, "running", 1));
    draw({ on });
    expect(item(/5 tasks/)).toHaveTextContent(/^5 tasks · 5 working$/);
    const before = drawn.tree;
    expect(before).toBeGreaterThan(0);
    drawn.marks.length = 0;

    on.move(move(3, "running", 2));
    on.move(move(2, "failed", 3));

    expect(item(/5 tasks/)).toHaveTextContent(/^5 tasks · 4 working · 1 failed$/);
    expect(drawn.tree).toBe(before);
    expect(drawn.marks).toEqual([]);
  });

  it("does not redraw the tree for a folded chat's helpers coming and going", () => {
    const on = board(
      move(1, "running", 1, { children: [{ agent: "thread-1", state: "running" }] }),
    );
    draw({ on });
    const before = drawn.tree;

    on.move(
      move(1, "running", 2, {
        children: [
          { agent: "thread-1", state: "done" },
          { agent: "thread-2", state: "running" },
        ],
      }),
    );

    expect(item(/2 helpers/)).toBeInTheDocument();
    expect(drawn.tree).toBe(before);
  });
});

describe("a branch's cockpit", () => {
  const focus: Place = { workspace: "alpha", repo: "svc", piece: "one" };

  it("lists the chats with a tab that work in the branch, and one line for the tasks there", async () => {
    const onRevealChats = vi.fn();
    draw({
      chats: [
        chat(1, "steward 1", ALPHA),
        chat(2, "talk", `${CUT}/one`, { from: taskOf(1) }),
        chat(3, "ide.3", `${CUT}/one`),
        chat(4, "lint", `${CUT}/one`, { from: taskOf(3) }),
      ],
      focus,
      onRevealChats,
    });

    const cockpit = screen.getByRole("tree", { name: "Chats and files of one" });
    const names = within(cockpit)
      .getAllByRole("treeitem")
      .map((row) => row.textContent?.trim());
    expect(names).toEqual([
      expect.stringContaining("ide.3"),
      "1 task · 1 working",
      "1 task in this branch · 1 working",
      "Files",
    ]);

    await userEvent.click(within(cockpit).getByRole("treeitem", { name: /in this branch/ }));
    expect(onRevealChats).toHaveBeenLastCalledWith([2]);
  });
});
