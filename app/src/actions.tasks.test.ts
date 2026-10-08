import { describe, expect, it } from "vitest";
import { BESIDE_ID, catalogue, taskRows, taskShowId } from "./actions";
import type { ListedChat } from "./chatsTree";
import { noTabs } from "./tabs";

const listed = (session: number, more: Partial<ListedChat> = {}): ListedChat => ({
  session,
  name: `devops ${session}`,
  persona: "devops",
  workspace: "beta",
  shell: false,
  parent: 1,
  mode: "task",
  from: "steward 1",
  tab: false,
  branch: null,
  report: null,
  outcome: null,
  asking: null,
  harness: "claude",
  ...more,
});

describe("the palette's rows for tasks (#1499, V100-49)", () => {
  it("has a row for each task, which shows it, and says where it works and who asked", () => {
    const rows = taskRows([listed(2), listed(3, { name: "live check talk" })]);

    expect(rows.map((row) => [row.id, row.title, row.available, row.does])).toEqual([
      [
        taskShowId(2),
        "Show task devops 2 in beta, asked by steward 1",
        true,
        { verb: "showChat", session: 2 },
      ],
      [
        taskShowId(3),
        "Show task live check talk in beta, asked by steward 1",
        true,
        { verb: "showChat", session: 3 },
      ],
    ]);
    // The name is the row's own, so the palette puts a name that was typed first.
    expect(rows[1].name).toBe("live check talk");
  });

  it("has one for a task that is on screen too, and none for a chat that is not a task", () => {
    // A task shown in its session's tab has no tab row of its own to be found by (#1486).
    expect(taskRows([listed(2, { tab: true })])).toHaveLength(1);
    expect(taskRows([listed(4, { mode: "handoff" }), listed(5, { mode: null })])).toEqual([]);
  });
});

describe("opening a chat beside the one in front (#1499)", () => {
  it("is one row of the catalogue that cannot run yet, and says why", () => {
    const rows = catalogue({
      tabs: noTabs(),
      workspaces: [],
      needsYou: [],
      nameOf: (session) => String(session),
      listed: [listed(2), listed(3)],
    }).filter((row) => row.id === BESIDE_ID);

    expect(rows).toHaveLength(1);
    expect(rows[0].available).toBe(false);
    expect(rows[0].reason).toBe(
      "purlis cannot open a chat beside another yet. Press Enter to open it in front.",
    );
  });
});
