import { describe, expect, it } from "vitest";
import { BACK_SAYS, OWN_TABS_SAYS, TASK_TABS_SAY } from "./EndingChat";

/**
 * **A close ends no task** (#1488): where tasks are among what a close closes, their tabs go
 * and they stay in the Chats list, and the close question says so in one line.
 */
describe("what a close says of the tasks among what it closes", () => {
  it("names one task, and says it is not ended", () => {
    expect(BACK_SAYS(["talk"])).toBe(
      "talk is a task: its tab goes, it is not ended, and it stays in the Chats list.",
    );
  });

  it("names several in one line", () => {
    expect(BACK_SAYS(["talk", "sweep"])).toBe(
      "talk, sweep are tasks: their tabs go, they are not ended, and they stay in the Chats list.",
    );
  });

  it("says the closing session's tasks in tabs of their own in the same one line (#1489)", () => {
    expect(TASK_TABS_SAY(["talk"], 0)).toBe(BACK_SAYS(["talk"]));
    expect(TASK_TABS_SAY([], 2)).toBe(OWN_TABS_SAYS(2));
    expect(TASK_TABS_SAY(["talk"], 1)).toBe(`${BACK_SAYS(["talk"])} ${OWN_TABS_SAYS(1)}`);
    expect(TASK_TABS_SAY([], 0)).toBe("");
  });
});
