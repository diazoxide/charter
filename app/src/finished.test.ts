import { describe, expect, it } from "vitest";
import { rowsUntilRead } from "./finished";

/** A row as the Chats list has it, by its number and whether it is a task's. */
const row = (session: number, mode: string | null = null) => ({ session, mode });
const sessions = (rows: readonly { session: number }[]) => rows.map((one) => one.session);

describe("rowsUntilRead", () => {
  const before = [row(4), row(9, "task"), row(12)];

  it("keeps a task that left where it stood until the finished rows are read", () => {
    const rows = [row(4), row(12)];
    expect(sessions(rowsUntilRead(rows, before, false))).toEqual([4, 9, 12]);
    // Read: the rows as they are, the same array.
    expect(rowsUntilRead(rows, before, true)).toBe(rows);
  });

  it("holds only the task's row: a chat that arrives meanwhile is drawn at once", () => {
    const rows = [row(4), row(12), row(15)];
    expect(sessions(rowsUntilRead(rows, before, false))).toEqual([4, 9, 12, 15]);
    // And a chat that is not a task going goes at once.
    expect(sessions(rowsUntilRead([row(4)], before, false))).toEqual([4, 9]);
  });

  it("draws a renamed row as it is now", () => {
    const named = (session: number, name: string, mode: string | null = null) => ({
      session,
      mode,
      name,
    });
    const was = [named(4, "steward 4"), named(9, "probe", "task"), named(12, "steward 12")];
    const drawn = rowsUntilRead([named(4, "renamed"), named(12, "steward 12")], was, false);
    expect(drawn.map((one) => one.name)).toEqual(["renamed", "probe", "steward 12"]);
  });

  it("puts a kept row first where nothing above it is drawn", () => {
    expect(sessions(rowsUntilRead([row(12)], [row(9, "task"), row(12)], false))).toEqual([9, 12]);
  });

  it("hands the rows on as they are where nothing left", () => {
    const rows = [row(4), row(9, "task"), row(12), row(15)];
    expect(rowsUntilRead(rows, before, false)).toBe(rows);
  });
});
