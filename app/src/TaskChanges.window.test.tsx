import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type {
  BranchMerge,
  FinishedTask,
  OpenChat,
  SharedFolder,
  TaskChanges,
  TaskFile,
} from "./bindings";
import { forgetThisLaunch } from "./regions";

/**
 * **What a task changed, against the whole window** (#1511): Changes on a finished task's row
 * opens a tab with that task's files and no sibling's; a task on its own branch is offered
 * Merge and Discard there, each asked first; a merge the core refuses changes nothing and the
 * question says why; and two tasks of one chat in one folder are named on the asking chat's
 * pane.
 *
 * The core is a pretend one: it answers the task's changes, the merge question and the merge
 * as the real one would, and records what it was asked.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;

const STEWARD: OpenChat = {
  session: 4,
  name: "4",
  cwd: ALPHA,
  harness: "claude",
  in_front: true,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona: "steward",
  unreported: null,
  card: null,
  guessed: null,
  pinned: false,
  label: null,
  from: null,
};

function finished(id: string, name: string, more: Partial<FinishedTask> = {}): FinishedTask {
  return {
    id,
    asker: 4,
    name,
    chat: null,
    persona: "devops",
    how: "failed",
    outcome: "failed",
    folds: false,
    report: `${name}: see the branch.`,
    changed: null,
    ended: "2026-10-08T12:04:30+00:00",
    place: "alpha",
    branch: null,
    reopens: true,
    not_reopened: null,
    did_not_start: false,
    attempts: 0,
    waits: null,
    ...more,
  };
}

const BRANCH = "fix-the-queue-0000aaaa";

/** A task that worked on its own branch, and one that worked beside it in the clone. */
const ON_ITS_BRANCH = finished("01K6OWN", "fix the queue", {
  branch: BRANCH,
  changed: "the queue's retry",
});
const BESIDE = finished("01K6SHARED", "tidy the docs");
/** A task whose folder was discarded and whose branch, merged, is still in the repo (#1472). */
const LEFT_BRANCH = "left-behind-0000bbbb";
const LEFT = finished("01K6LEFT", "left behind", { branch: LEFT_BRANCH });

function file(path: string, more: Partial<TaskFile> = {}): TaskFile {
  return { path, mark: "changed", from: null, uncommitted: false, also: [], ...more };
}

/** What the core says each task changed. The sibling's own file is never in the other's. */
const CHANGES: Record<string, TaskChanges> = {
  [ON_ITS_BRANCH.id]: {
    id: ON_ITS_BRANCH.id,
    task: ON_ITS_BRANCH.name,
    running: false,
    own: { repo: "api", branch: BRANCH, standing: "kept", acts: true, left: null },
    places: [
      {
        workspace: "alpha",
        repo: "api",
        piece: BRANCH,
        base: "main",
        files: [file("src/queue.rs"), file("src/retry.rs", { mark: "added" })],
        more: 0,
        unread: null,
      },
    ],
    elsewhere: [],
    more: false,
    said: "the queue's retry",
    unknown: null,
  },
  [BESIDE.id]: {
    id: BESIDE.id,
    task: BESIDE.name,
    running: false,
    own: null,
    places: [
      {
        workspace: "alpha",
        repo: "api",
        piece: null,
        base: null,
        files: [file("docs/guide.md", { uncommitted: true, also: ["fix the queue"] })],
        more: 0,
        unread: null,
      },
    ],
    elsewhere: [],
    more: false,
    said: null,
    unknown: null,
  },
};

/** What the core says of LEFT: no folder to compare, and its branch still in the repo. */
const LEFT_CHANGES: TaskChanges = {
  id: LEFT.id,
  task: LEFT.name,
  running: false,
  own: {
    repo: "api",
    branch: LEFT_BRANCH,
    standing: "discarded",
    acts: false,
    left: { tip: "b".repeat(40), merged: true, ahead: 0 },
  },
  places: [],
  elsewhere: [],
  more: false,
  said: null,
  unknown:
    "Its branch's folder was discarded, so there is no folder to compare. A branch that held a commit is still in the repo.",
};

const MERGE: BranchMerge = {
  task: ON_ITS_BRANCH.name,
  repo: "api",
  branch: BRANCH,
  into: "main",
  tip: "a".repeat(40),
  ahead: 2,
  behind: 0,
  uncommitted: [],
};

/** Git's refusal, as the core says it: the branch does not apply cleanly. */
const DOES_NOT_APPLY = `'${BRANCH}' does not fast-forward into main: main has moved on. Merge main into '${BRANCH}' where its conflicts belong, then merge again. Nothing was merged.`;

type Asked = { cmd: string; args: Record<string, unknown> };

function core({
  merge = MERGE,
  merged,
  sharing = [],
  left = LEFT_CHANGES,
  deleted,
}: {
  merge?: BranchMerge;
  /** What the core says of LEFT. */
  left?: TaskChanges;
  /** The delete's refusal, or nothing for a delete that goes. */
  deleted?: string;
  /** The merge's refusal, or nothing for a merge that lands. */
  merged?: string;
  sharing?: SharedFolder[];
} = {}) {
  const asked: Asked[] = [];
  const chats = [STEWARD];
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      asked.push({ cmd, args: a });
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return chats;
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          personas: ["devops", "steward"],
          persona: "steward",
          unfiled: [],
          workspaces: [{ name: "alpha", path: ALPHA, vision: "", todos: [], chats: [...chats] }],
        };
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "stopping_chats") return [];
      if (cmd === "finished_tasks") return [ON_ITS_BRANCH, BESIDE, LEFT];
      if (cmd === "task_changes") return a.id === LEFT.id ? left : CHANGES[a.id as string];
      if (cmd === "task_branch_delete") {
        if (deleted !== undefined) throw new Error(deleted);
        return null;
      }
      if (cmd === "task_branch_merge_question") return merge;
      if (cmd === "task_branch_merge") {
        if (merged !== undefined) throw new Error(merged);
        return { branch: BRANCH, folder_removed: true };
      }
      if (cmd === "tasks_sharing_a_folder") return sharing;
      return null;
    },
    { shouldMockEvents: true },
  );
  return {
    asked: (cmd: string) => asked.filter((one) => one.cmd === cmd).map(({ args }) => args),
  };
}

const theirs = () => screen.findByRole("group", { name: "Finished tasks of steward 4" });

/** Opens the Changes tab of `task` from its finished row, as the person does. */
async function openChangesOf(label: string) {
  const group = await theirs();
  await userEvent.click(within(group).getByRole("button", { name: label }));
  return screen.findByTestId("task-changes");
}

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
});

describe("what a task changed", () => {
  it("opens a task's changes from its row, with its own files and no sibling's", async () => {
    const said = core();
    render(<App />);

    const tab = await openChangesOf("Review changes of fix the queue");

    // Asked by the dispatch's id alone (StrictMode reads it twice): no path, no branch.
    const reads = said.asked("task_changes");
    expect(reads.length).toBeGreaterThan(0);
    for (const read of reads) expect(read).toEqual({ plane: PLANE, id: ON_ITS_BRANCH.id });
    const files = within(tab)
      .getAllByRole("button", { name: /\.(rs|md)$/ })
      .map((one) => one.textContent);
    expect(files).toEqual(["src/queue.rs", "src/retry.rs"]);
    expect(within(tab).queryByText("docs/guide.md")).toBeNull();
    expect(tab).toHaveTextContent(`It worked on its own branch, ${BRANCH} in api.`);
    // The task's own words, beside what purlis found.
    expect(tab).toHaveTextContent("What it says changed");
    // A tab of its own, named for the task.
    const strip = screen.getByRole("tablist", { name: "Tabs" });
    expect(within(strip).getByText("Changes · fix the queue")).toBeInTheDocument();
  });

  it("lists a shared-folder task's files and marks one a sibling named too", async () => {
    core();
    render(<App />);

    const tab = await openChangesOf("Changes of tidy the docs");

    expect(within(tab).getByRole("button", { name: "docs/guide.md" })).toBeInTheDocument();
    expect(tab).toHaveTextContent("also written by fix the queue");
    // What it cannot see is said, not left for the person to assume.
    expect(tab).toHaveTextContent(
      "Not listed: edits made by a shell command, changes it already committed",
    );
    expect(tab).toHaveTextContent("not committed");
    // Nothing to merge or discard: it had no branch of its own.
    expect(within(tab).queryByRole("button", { name: "Merge…" })).toBeNull();
    expect(within(tab).queryByRole("button", { name: "Discard branch…" })).toBeNull();
  });

  it("says a file of the task's may be missing where the shared folder is past git's cap", async () => {
    const was = CHANGES[BESIDE.id];
    CHANGES[BESIDE.id] = { ...was, places: [{ ...was.places[0], more: 12 }] };
    try {
      core();
      render(<App />);

      const tab = await openChangesOf("Changes of tidy the docs");

      expect(tab).toHaveTextContent(
        "git found 12 more changes in this folder than it lists, so a file this task wrote may be among them and not shown.",
      );
      expect(tab).not.toHaveTextContent("… and 12 more");
    } finally {
      CHANGES[BESIDE.id] = was;
    }
  });

  it("opens the same tab from the report's line about what changed", async () => {
    core();
    render(<App />);
    const group = await theirs();
    const row = within(group)
      .getAllByRole("button")
      .find((one) => one.querySelector(".session")?.textContent === "fix the queue");
    if (row === undefined) throw new Error("no finished row is named fix the queue");
    await userEvent.click(row);

    await userEvent.click(within(group).getByRole("button", { name: "Changed:" }));

    expect(await screen.findByTestId("task-changes")).toHaveTextContent("src/queue.rs");
  });

  it("asks before it merges, and a merge that does not apply cleanly says why in the question", async () => {
    const said = core({ merged: DOES_NOT_APPLY });
    render(<App />);
    const tab = await openChangesOf("Review changes of fix the queue");

    await userEvent.click(within(tab).getByRole("button", { name: "Merge…" }));
    const question = await screen.findByRole("alertdialog", { name: "Merge this task's branch?" });
    expect(question).toHaveTextContent(
      `This lands 2 commits of ${BRANCH}, which purlis cut for fix the queue, in main in api.`,
    );
    // Nothing is merged by asking.
    expect(said.asked("task_branch_merge")).toEqual([]);

    await userEvent.click(within(question).getByRole("button", { name: "Merge" }));

    await waitFor(() =>
      expect(within(question).getByRole("alert")).toHaveTextContent(DOES_NOT_APPLY),
    );
    // The answer handed back exactly what the person was shown.
    expect(said.asked("task_branch_merge")).toEqual([
      expect.objectContaining({ id: ON_ITS_BRANCH.id, seen: MERGE }),
    ]);
  });

  it("says why a merge would be refused and asks nothing, where the branch no longer fast-forwards", async () => {
    const said = core({ merge: { ...MERGE, behind: 3 } });
    render(<App />);
    const tab = await openChangesOf("Review changes of fix the queue");

    await userEvent.click(within(tab).getByRole("button", { name: "Merge…" }));

    expect(
      await within(tab).findByText(
        `main has 3 commits the task's branch does not have, so the branch no longer fast-forwards. Merge main into ${BRANCH} where its conflicts belong, then merge again.`,
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(said.asked("task_branch_merge")).toEqual([]);
  });

  it("says what is left of a branch whose folder is gone, and deletes it only as shown (#1472)", async () => {
    const said = core();
    render(<App />);
    const tab = await openChangesOf("Review changes of left behind");

    expect(within(tab).getByTestId("task-changes-left")).toHaveTextContent(
      `The branch ${LEFT_BRANCH} is still in api, and the branch api is on holds every commit of it, so deleting it loses nothing.`,
    );
    // Its folder is gone: nothing to merge or discard.
    expect(within(tab).queryByRole("button", { name: "Merge…" })).toBeNull();
    expect(within(tab).queryByRole("button", { name: "Discard branch…" })).toBeNull();

    await userEvent.click(within(tab).getByRole("button", { name: "Delete branch…" }));
    const question = await screen.findByRole("alertdialog", { name: "Delete this task's branch?" });
    await userEvent.click(within(question).getByRole("button", { name: "Delete" }));

    // By the dispatch's id and the commit it was shown at, and nothing else.
    await waitFor(() =>
      expect(said.asked("task_branch_delete")).toEqual([
        { plane: PLANE, id: LEFT.id, tip: "b".repeat(40) },
      ]),
    );
  });

  it("offers no delete of a branch that holds work, and says it stays", async () => {
    core({
      left: {
        ...LEFT_CHANGES,
        own: {
          repo: "api",
          branch: LEFT_BRANCH,
          standing: "discarded",
          acts: false,
          left: { tip: "c".repeat(40), merged: false, ahead: 2 },
        },
      },
    });
    render(<App />);
    const tab = await openChangesOf("Review changes of left behind");

    expect(within(tab).getByTestId("task-changes-left")).toHaveTextContent(
      `The branch ${LEFT_BRANCH} is still in api, holding 2 commits the branch api is on does not have. It stays: merge it, or delete it with git, yourself.`,
    );
    expect(within(tab).queryByRole("button", { name: "Delete branch…" })).toBeNull();
  });

  it("names both tasks on the asking chat's pane when two work in one folder", async () => {
    core({
      sharing: [
        {
          folder: "workspaces/alpha",
          tasks: ["talk", "review"],
          says: "'talk' and 'review' both work in workspaces/alpha with no branch of their own. There are no file locks between tasks: a file two of them change cannot be told apart afterwards. To keep them apart, ask for one on a branch of its own.",
        },
      ],
    });
    render(<App />);

    const notice = await screen.findByRole("status", { name: "Tasks sharing workspaces/alpha" });

    expect(notice).toHaveTextContent("'talk' and 'review' both work in workspaces/alpha");
  });
});
