import { describe, expect, it } from "vitest";
import { chatsTree, type ListedChat } from "./chatsTree";
import { chatRowOf, finishedRowOf, rowsAbove } from "./revealTask";

/** Bringing a failed task's finished row into view (#1491). */

function listed(session: number, parent: number | null = null): ListedChat {
  return {
    session,
    name: `chat ${session}`,
    persona: null,
    workspace: "alpha",
    shell: false,
    parent,
    mode: parent === null ? null : "task",
    from: parent === null ? null : `chat ${parent}`,
    tab: parent === null,
    branch: null,
    report: null,
    outcome: null,
    asking: null,
    harness: null,
  };
}

describe("the rows a finished row is drawn under", () => {
  const rows = chatsTree([listed(1), listed(2, 1), listed(3, 2), listed(4, 1), listed(5)]);

  it("are its session's and every chat's above that one", () => {
    expect(rowsAbove(rows, 3)).toEqual([3, 2, 1]);
    expect(rowsAbove(rows, 4)).toEqual([4, 1]);
    expect(rowsAbove(rows, 5)).toEqual([5]);
  });

  it("are none for a chat that is not listed", () => {
    expect(rowsAbove(rows, 99)).toEqual([]);
  });
});

describe("a finished row, found by what the list says of itself", () => {
  const list = () => {
    const section = document.createElement("section");
    section.innerHTML = `
      <div role="group" aria-label="Finished tasks of steward 1">
        <button class="finished-name"><span class="session">check prod</span></button>
        <button class="finished-name"><span class="session">check staging</span></button>
      </div>
      <div role="group" aria-label="Finished tasks of steward 2">
        <button class="finished-name"><span class="session">check prod</span></button>
      </div>`;
    return section;
  };

  it("is the row of that name under that session, and no other session's", () => {
    const section = list();
    const row = finishedRowOf(section, "steward 2", "check prod");
    expect(row?.closest('[role="group"]')?.getAttribute("aria-label")).toBe(
      "Finished tasks of steward 2",
    );
    expect(finishedRowOf(section, "steward 1", "check staging")?.textContent).toBe("check staging");
  });

  it("is nothing while the row is not drawn", () => {
    expect(finishedRowOf(list(), "steward 1", "check the queue")).toBeNull();
    expect(finishedRowOf(list(), "steward 3", "check prod")).toBeNull();
    expect(finishedRowOf(null, "steward 1", "check prod")).toBeNull();
  });
});

describe("a chat's own row, found by its number (#1490)", () => {
  it("is the row of the tree that carries it, and no other element that does", () => {
    const section = document.createElement("section");
    section.innerHTML = `
      <div data-session="7">a pane</div>
      <button role="treeitem" data-session="7">steward 7</button>
      <button role="treeitem" data-session="70">steward 70</button>`;

    expect(chatRowOf(section, 7)?.textContent).toBe("steward 7");
    expect(chatRowOf(section, 8)).toBeNull();
    expect(chatRowOf(null, 7)).toBeNull();
  });
});
