import { describe, expect, it } from "vitest";
import type { DispatchRow } from "./bindings";
import {
  askersOf,
  EVERY_DISPATCH,
  isDispatches,
  NO_PERSONA,
  personasOf,
  saidAt,
  shownDispatches,
} from "./dispatches";

function row(
  id: string,
  persona: string | null,
  asker: string,
  askerKey = `id of ${asker}`,
): DispatchRow {
  return {
    id,
    mode: "handoff",
    persona,
    task: id,
    asker,
    asker_key: askerKey,
    asker_persona: null,
    by_person: false,
    place: "alpha",
    folder: "workspaces/alpha",
    outcome: "done",
    started: "2026-10-07T12:00:00+00:00",
    ended: "2026-10-07T12:01:00+00:00",
    duration: "1m 0s",
    needed_you: 0,
    messages: 0,
    cost: null,
    tokens: null,
    brief: "",
    report: null,
    open_session: null,
    session_record: null,
  };
}

const ROWS = [
  row("c", "devops", "steward 3"),
  row("b", "qa", "steward 3"),
  row("a", "devops", "planner 2"),
  row("z", null, "planner 2"),
];

const ids = (rows: DispatchRow[]) => rows.map((one) => one.id);

describe("the Dispatches tab's filters", () => {
  it("keeps every dispatch, in the order they came, until one is chosen", () => {
    expect(ids(shownDispatches(ROWS, EVERY_DISPATCH))).toEqual(["c", "b", "a", "z"]);
  });

  it("narrows to one persona, to one asking chat, and to both at once", () => {
    expect(ids(shownDispatches(ROWS, { persona: "devops", asker: "" }))).toEqual(["c", "a"]);
    const planner = "id of planner 2";
    expect(ids(shownDispatches(ROWS, { persona: "", asker: planner }))).toEqual(["a", "z"]);
    expect(ids(shownDispatches(ROWS, { persona: "devops", asker: planner }))).toEqual(["a"]);
    expect(ids(shownDispatches(ROWS, { persona: "qa", asker: planner }))).toEqual([]);
  });

  it("finds the dispatches that went to no persona, which no persona's name could mean", () => {
    expect(ids(shownDispatches(ROWS, { persona: NO_PERSONA, asker: "" }))).toEqual(["z"]);
  });

  it("offers each persona and each asking chat once, by name", () => {
    expect(personasOf(ROWS)).toEqual(["devops", "qa", NO_PERSONA]);
    expect(personasOf(ROWS.slice(0, 3))).toEqual(["devops", "qa"]);
    expect(askersOf(ROWS)).toEqual([
      { key: "id of planner 2", name: "planner 2" },
      { key: "id of steward 3", name: "steward 3" },
    ]);
  });

  it("keeps two chats that were called the same apart, by the chat and not by its name", () => {
    const rows = [
      row("new", "devops", "steward 3", "01NEW"),
      row("old", "devops", "steward 3", "01OLD"),
    ];
    expect(askersOf(rows)).toEqual([
      { key: "01NEW", name: "steward 3" },
      { key: "01OLD", name: "steward 3" },
    ]);
    expect(ids(shownDispatches(rows, { persona: "", asker: "01OLD" }))).toEqual(["old"]);
    // A name is not a key: nothing is found by it.
    expect(ids(shownDispatches(rows, { persona: "", asker: "steward 3" }))).toEqual([]);
  });

  it("says a record's time to the minute, and leaves alone what is not one", () => {
    expect(saidAt("2026-10-07T12:04:30+00:00")).toBe("2026-10-07 12:04 UTC");
    expect(saidAt("not a time")).toBe("not a time");
  });

  it("is one view per project, which no extension's view of the same name is", () => {
    expect(isDispatches({ from: null, view: "dispatches", key: "" })).toBe(true);
    expect(isDispatches({ from: "someone", view: "dispatches", key: "" })).toBe(false);
    expect(isDispatches({ from: null, view: "session", key: "" })).toBe(false);
  });
});
