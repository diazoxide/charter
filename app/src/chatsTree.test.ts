import { describe, expect, it } from "vitest";
import { chatsTree, startedBy, startedElsewhere, type ListedChat } from "./chatsTree";

/** A chat started by `parent`, where one started it. */
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
  };
}

/** The tree as `level:session`, top to bottom. */
const drawn = (chats: ListedChat[]) => chatsTree(chats).map((row) => `${row.level}:${row.session}`);

describe("the project's chats as a tree", () => {
  it("draws each chat under the chat that started it, depth first", () => {
    expect(drawn([listed(1), listed(2, 1), listed(3), listed(4, 2), listed(5, 1)])).toEqual([
      "1:1",
      "2:2",
      "3:4",
      "2:5",
      "1:3",
    ]);
  });

  it("says where each row is among its siblings", () => {
    const rows = chatsTree([listed(1), listed(2, 1), listed(3, 1), listed(4)]);
    expect(rows.map((row) => `${row.session} ${row.posinset}/${row.setsize}`)).toEqual([
      "1 1/2",
      "2 1/2",
      "3 2/2",
      "4 2/2",
    ]);
  });

  it("leaves a chat whose parent has closed at the top, marked, with its own children under it", () => {
    const rows = chatsTree([listed(1), listed(8, 7), listed(9, 8)]);
    expect(rows.map((row) => `${row.level}:${row.session}:${row.orphaned}`)).toEqual([
      "1:1:false",
      "1:8:true",
      "2:9:false",
    ]);
  });

  it("draws every chat once when the lineage loops back on itself", () => {
    // A number dealt again could make two chats each other's parent. Neither may vanish.
    expect(drawn([listed(1), listed(2, 3), listed(3, 2), listed(4, 4)]).sort()).toEqual([
      "1:1",
      "1:2",
      "1:4",
      "2:3",
    ]);
  });

  it("lists what each chat started, by the asking chat's number, open parents only", () => {
    const by = startedBy([listed(1), listed(2, 1), listed(3, 1), listed(8, 7)]);
    expect([...by.keys()]).toEqual([1]);
    expect(by.get(1)?.map((chat) => chat.session)).toEqual([2, 3]);
  });

  it("keeps, for the explorer, only the started chats that work in another workspace", () => {
    const away = { ...listed(3, 1), workspace: "beta" };
    const by = startedElsewhere([
      listed(1),
      listed(2, 1),
      away,
      { ...listed(4, 3), workspace: "beta" },
    ]);
    expect([...by.keys()]).toEqual([1]);
    expect(by.get(1)).toEqual([away]);
  });
});
