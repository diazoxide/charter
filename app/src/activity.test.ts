import { describe, expect, it } from "vitest";
import type { Activity, ActivityLine } from "./bindings";
import {
  activityView,
  alsoSaid,
  answerable,
  closedWhy,
  heard,
  namedByOthers,
  timelineOf,
  unkeptSaid,
  type Drawn,
} from "./activity";
import { contentsOf, noTabs, openTab, openView, replaceSession } from "./tabs";

function line(over: Partial<ActivityLine> & Pick<ActivityLine, "dispatch" | "n">): ActivityLine {
  return {
    at: "2026-10-08T09:00:00+00:00",
    kind: "note",
    from: "talk",
    from_key: "01K6TALK",
    from_session: 7,
    to: "steward 3",
    to_key: "01K6STEWARD",
    text: "",
    by_person: false,
    asks: null,
    answers: null,
    unread: false,
    by_purlis: false,
    expired: false,
    left_out: false,
    unkept: null,
    unkept_why: null,
    outcome: null,
    files: [],
    task: "talk",
    place: "alpha\u0000workspaces/alpha/svc",
    depth: 1,
    ...over,
  };
}

function read(lines: ActivityLine[]): Activity {
  return { name: "steward 3", key: "01K6STEWARD", lines, undrawn: 0 };
}

const drawn = (lines: ActivityLine[]): Drawn[] => lines.map((one) => ({ line: one, depth: 1 }));

describe("the timeline the core answered", () => {
  it("draws every line the core listed, in its order and at its depth", () => {
    // A task of a task whose own line the core put BEFORE its asker's dispatch (the clock
    // stepped back), and a record whose chats the core matched by number: nothing here can
    // re-derive either, and nothing is dropped for it.
    const lines = [
      line({
        dispatch: "01K6D2",
        n: 0,
        kind: "dispatched",
        from_key: "#7",
        to_key: "#9",
        depth: 2,
      }),
      line({ dispatch: "01K6D1", n: 0, kind: "dispatched", from_key: "#3", to_key: "#7" }),
      line({ dispatch: "01K6D2", n: 1, depth: 2 }),
    ];

    const made = timelineOf(read(lines));

    expect(made.lines.map((one) => [one.line.dispatch, one.line.n, one.depth])).toEqual([
      ["01K6D2", 0, 2],
      ["01K6D1", 0, 1],
      ["01K6D2", 1, 2],
    ]);
  });

  it("adds a line told later only where its task is on the timeline", () => {
    const made = timelineOf(
      read([
        line({
          dispatch: "01K6D1",
          n: 0,
          kind: "dispatched",
          from_key: "01K6STEWARD",
          to_key: "01K6TALK",
        }),
      ]),
    );

    const more = heard(made, line({ dispatch: "01K6D1", n: 1, depth: 0 }));
    const others = heard(more, line({ dispatch: "01K6D9", n: 1, depth: 0 }));
    const under = heard(
      others,
      line({
        dispatch: "01K6D3",
        n: 0,
        kind: "dispatched",
        from_key: "01K6TALK",
        to_key: "01K6DIG",
        depth: 0,
      }),
    );

    expect(others).toBe(more);
    expect(under.lines.map((one) => [one.line.dispatch, one.line.n, one.depth])).toEqual([
      ["01K6D1", 0, 1],
      ["01K6D1", 1, 1],
      ["01K6D3", 0, 2],
    ]);
  });

  it("puts a line told again in its own place, with what it says now", () => {
    const gap = line({ dispatch: "01K6D1", n: 2, kind: "not listed", unkept: 1, depth: 0 });
    const made = [
      line({ dispatch: "01K6D1", n: 0, kind: "dispatched", from_key: "01K6STEWARD" }),
      line({ dispatch: "01K6D1", n: 1 }),
      gap,
    ].reduce(heard, timelineOf(read([])));
    const later = heard(made, line({ dispatch: "01K6D1", n: 3, kind: "report", depth: 0 }));

    const told = heard(later, { ...gap, unkept: 4 });

    expect(told.lines.map((one) => [one.line.n, one.line.unkept])).toEqual([
      [0, null],
      [1, null],
      [2, 4],
      [3, null],
    ]);
  });
});

describe("a file more than one report names", () => {
  const report = (dispatch: string, over: Partial<ActivityLine> = {}) =>
    line({ dispatch, n: 1, kind: "report", files: ["src/app.rs"], ...over });

  it("marks two tasks of the same name, which are two dispatches", () => {
    const also = namedByOthers(drawn([report("01K6D1"), report("01K6D2")]));

    expect(also.get("01K6D1")).toEqual([{ file: "src/app.rs", others: ["talk"] }]);
    expect(also.get("01K6D2")).toEqual([{ file: "src/app.rs", others: ["talk"] }]);
  });

  it("marks nothing between tasks that worked in different folders", () => {
    const also = namedByOthers(
      drawn([
        report("01K6D1"),
        // On a branch of its own, and in another workspace.
        report("01K6D2", { place: "alpha\u0000workspaces/alpha/.worktrees/svc/lint-0def" }),
        report("01K6D3", { place: "beta\u0000workspaces/beta/svc" }),
      ]),
    );

    expect([...also.keys()]).toEqual([]);
  });

  it("marks only tasks that ran at the same time as each other (#1520)", () => {
    const at = (time: string) => `2026-10-08T${time}:00+00:00`;
    const task = (dispatch: string, from: string, to: string) => [
      line({ dispatch, n: 0, kind: "dispatched", at: at(from), task: dispatch }),
      report(dispatch, { at: at(to), task: dispatch }),
    ];
    const also = namedByOthers(
      drawn([
        ...task("A", "09:00", "09:10"),
        // Started while A ran.
        ...task("C", "09:05", "09:20"),
        // Long after both ended: a file it shares with them is not a clash.
        ...task("B", "10:00", "10:05"),
      ]),
    );

    expect(also.get("A")).toEqual([{ file: "src/app.rs", others: ["C"] }]);
    expect(also.get("C")).toEqual([{ file: "src/app.rs", others: ["A"] }]);
    expect(also.get("B")).toBeUndefined();
  });

  it("says what it is: what another report names", () => {
    expect(alsoSaid({ file: "src/app.rs", others: ["lint"] })).toBe(
      "lint's report also names src/app.rs",
    );
    expect(alsoSaid({ file: "src/app.rs", others: ["lint", "talk"] })).toBe(
      "The reports of lint, talk also name src/app.rs",
    );
  });
});

describe("what is said of messages a record did not keep", () => {
  const gap = (unkept: number, unkept_why: string) =>
    unkeptSaid(line({ dispatch: "01K6D1", n: 1, kind: "not listed", unkept, unkept_why }));

  it("is true of a record from before text was kept, and names no cap", () => {
    expect(gap(4, "before")).toBe(
      "4 messages of talk are not listed: they were sent before purlis kept what tasks say.",
    );
    expect(gap(1, "before")).toBe(
      "1 message of talk is not listed: it was sent before purlis kept what tasks say.",
    );
  });

  it("names the cap that bound", () => {
    expect(gap(2, "count")).toBe(
      "2 messages of talk are not listed: purlis keeps the first 500 messages of a task.",
    );
    expect(gap(2, "size")).toBe(
      "2 messages of talk are not listed: purlis keeps the first 256 KiB of what a task and its asking chat say.",
    );
  });
});

describe("a chat that is restarted", () => {
  it("takes its Activity tab with it to its new number", () => {
    let tabs = openTab(noTabs(), 3, "steward 3");
    tabs = openView(tabs, activityView(3), "Activity · steward 3", "alpha");
    tabs = openView(tabs, activityView(8), "Activity · lint", "alpha");

    const now = replaceSession(tabs, 3, 12);

    const shown = now.order.flatMap((id) =>
      contentsOf(now, id).map(({ content }) =>
        content.kind === "session" ? `chat ${content.session}` : content.view.key,
      ),
    );
    expect(shown).toEqual(["chat 12", "12", "8"]);
  });

  it("leaves the tabs as they are where it shows nowhere", () => {
    const tabs = openView(openTab(noTabs(), 3), activityView(4), "Activity · other", "alpha");

    expect(replaceSession(tabs, 9, 12)).toBe(tabs);
  });
});

describe("the questions the person may answer (#1496)", () => {
  /** A question the app says is open, with number `asks`; `null` for one it says is not. */
  const question = (dispatch: string, n: number, asks: number | null = 5) =>
    line({ dispatch, n, kind: "question", text: "Which host?", asks });

  it("is the question the app says its task is paused on", () => {
    const lines = drawn([
      line({ dispatch: "01K6D1", n: 0, kind: "dispatched" }),
      question("01K6D1", 1),
      // Not open by the app's word: answered before the tab read it, or never held.
      question("01K6D2", 1, null),
      // A note is not a question, whatever the app says of it.
      line({ dispatch: "01K6D3", n: 1, kind: "note", asks: 6 }),
    ]);

    expect([...answerable(lines)]).toEqual(["01K6D1:1"]);
  });

  it("is closed by an answer told for its number, whoever gave it, and by its task's ending", () => {
    const answered = (over: Parameters<typeof line>[0]) =>
      drawn([question("01K6D1", 1), line(over)]);
    expect([
      ...answerable(answered({ dispatch: "01K6D1", n: 2, kind: "answer", answers: 5 })),
    ]).toEqual([]);
    expect([
      ...answerable(
        answered({ dispatch: "01K6D1", n: 2, kind: "answer", answers: 5, by_person: true }),
      ),
    ]).toEqual([]);
    // The record kept none of the answer's words: the line that stands for it closes it too.
    expect([
      ...answerable(answered({ dispatch: "01K6D1", n: 2, kind: "not listed", answers: 5 })),
    ]).toEqual([]);
    for (const kind of ["report", "stopped"])
      expect([...answerable(answered({ dispatch: "01K6D1", n: 2, kind }))], kind).toEqual([]);
  });

  it("is closed by number and never by where a line stands", () => {
    // F8. The task had its first answer from its waiting command and asked again at once, so
    // the second question was recorded before the first answer: the answer's line stands
    // after the question it does not answer.
    const lines = drawn([
      question("01K6D1", 1, null),
      question("01K6D1", 2, 6),
      line({ dispatch: "01K6D1", n: 3, kind: "answer", answers: 5 }),
    ]);

    expect([...answerable(lines)]).toEqual(["01K6D1:2"]);
    expect(closedWhy(lines[1].line, lines)).toBeUndefined();
  });

  it("is still one after a note or a follow-up, and after another task's answer", () => {
    const lines = drawn([
      question("01K6D1", 1),
      line({ dispatch: "01K6D1", n: 2, kind: "note" }),
      line({ dispatch: "01K6D1", n: 3, kind: "follow-up" }),
      line({ dispatch: "01K6D2", n: 4, kind: "answer", answers: 5 }),
      // An answer that was read and not told says nothing of which question it closed.
      line({ dispatch: "01K6D1", n: 5, kind: "answer" }),
    ]);

    expect([...answerable(lines)]).toEqual(["01K6D1:1"]);
  });

  it("says why a question can no longer be answered, in a sentence for who was answering", () => {
    const asked = question("01K6D1", 1);
    const closed = (over: Parameters<typeof line>[0]) =>
      closedWhy(asked, drawn([asked, line(over)]));

    expect(closedWhy(asked, drawn([asked]))).toBeUndefined();
    expect(
      closed({ dispatch: "01K6D1", n: 2, kind: "answer", answers: 5, from: "steward 3" }),
    ).toBe("The chat steward 3 answered this question first, so there is nothing left to send.");
    expect(closed({ dispatch: "01K6D1", n: 2, kind: "answer", answers: 5, by_person: true })).toBe(
      "You have already answered this question, and one answer is final.",
    );
    expect(closed({ dispatch: "01K6D1", n: 2, kind: "not listed", answers: 5 })).toBe(
      "This question was answered first, so there is nothing left to send.",
    );
    expect(closed({ dispatch: "01K6D1", n: 2, kind: "report" })).toBe(
      "talk has ended, so an answer would reach no turn of its work.",
    );
  });
});
