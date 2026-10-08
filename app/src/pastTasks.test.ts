import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { PastTask } from "./bindings";
import {
  askedBy,
  clipped,
  CLIP_CHARS,
  CLIP_LINES,
  dayOf,
  endSaid,
  localAt,
  zoneSaid,
  endsOf,
  EVERY_PAST,
  mergedPast,
  narrows,
  personasOfPast,
  shownPast,
} from "./pastTasks";
import { pastTasksTitle, pastTasksView } from "./tabs";

/** Four hours east of UTC, with no summer time: every time below is read there. */
const ZONE_BEFORE = process.env.TZ;
beforeAll(() => {
  process.env.TZ = "Asia/Yerevan";
});
afterAll(() => {
  if (ZONE_BEFORE === undefined) delete process.env.TZ;
  else process.env.TZ = ZONE_BEFORE;
});

function past(over: Partial<PastTask> & Pick<PastTask, "id">): PastTask {
  return {
    name: over.id,
    persona: "devops",
    asker: "steward 4",
    asker_persona: "steward",
    by_person: false,
    how: "done",
    outcome: "done",
    started: "2026-10-07T12:00:00+00:00",
    ended: "2026-10-07T12:04:30+00:00",
    duration: "4m 30s",
    place: "alpha",
    asked_from: null,
    branch: null,
    says: "",
    session_record: null,
    reopens: true,
    reopened: false,
    not_reopened: null,
    ...over,
  };
}

describe("a workspace's past tasks, narrowed", () => {
  const rows = [
    past({ id: "c", name: "Check prod", ended: "2026-10-07T19:59:59+00:00" }),
    past({ id: "b", name: "rotate the key", persona: "qa", asker_persona: "planner" }),
    past({ id: "a", name: "tidy", persona: null, asker_persona: null, ended: "not a time" }),
  ];
  const kept = (filter: Partial<typeof EVERY_PAST>) =>
    shownPast(rows, { ...EVERY_PAST, ...filter }).map((row) => row.id);

  it("keeps everything until something narrows it", () => {
    expect(narrows(EVERY_PAST)).toBe(false);
    expect(kept({})).toEqual(["c", "b", "a"]);
    expect(narrows({ ...EVERY_PAST, text: "x" })).toBe(true);
  });

  it("finds a persona as the one that asked and as the one that ran the task", () => {
    expect(kept({ persona: "planner" })).toEqual(["b"]);
    expect(kept({ persona: "steward" })).toEqual(["c"]);
    expect(kept({ persona: "devops" })).toEqual(["c"]);
    expect(personasOfPast(rows)).toEqual(["devops", "planner", "qa", "steward"]);
  });

  it("holds a day range to the day each ended, both ends included, and leaves out a day that does not read", () => {
    expect(dayOf(rows[0])).toBe("2026-10-07");
    expect(dayOf(rows[2])).toBe("");
    expect(kept({ from: "2026-10-07", to: "2026-10-07" })).toEqual(["c", "b"]);
    expect(kept({ from: "2026-10-08" })).toEqual([]);
    expect(kept({ to: "2026-10-06" })).toEqual([]);
  });

  it("searches the task's name whatever its case", () => {
    expect(kept({ text: "  check " })).toEqual(["c"]);
    expect(kept({ text: "KEY", persona: "qa" })).toEqual(["b"]);
    expect(kept({ text: "KEY", persona: "devops" })).toEqual([]);
  });
});

describe("a past task's time, where the person is", () => {
  it("puts a task that ended after midnight their time on their day, not on UTC's", () => {
    // 21:30 UTC on the 6th is 01:30 on the 7th, four hours east.
    const late = past({ id: "late", ended: "2026-10-06T21:30:00+00:00" });
    expect(dayOf(late)).toBe("2026-10-07");
    expect(localAt("2026-10-06T21:30:00+00:00")).toBe("2026-10-07 01:30");
    const only = (day: string) =>
      shownPast([late], { ...EVERY_PAST, from: day, to: day }).map((row) => row.id);
    expect(only("2026-10-07")).toEqual(["late"]);
    expect(only("2026-10-06")).toEqual([]);
  });

  it("says the zone once, by its name and how far it is from UTC, and leaves a text that is no time as it is", () => {
    expect(zoneSaid(new Date("2026-10-07T00:00:00Z"))).toBe("Asia/Yerevan, UTC+4");
    expect(localAt("not a time")).toBe("not a time");
    expect(dayOf({ ended: null })).toBe("");
  });
});

describe("how a past task is said", () => {
  it("says an end in the state's word, and the core's own where it says more", () => {
    expect(endSaid({ how: "done", outcome: "done" })).toBe("done");
    expect(endSaid({ how: "blocked", outcome: "blocked" })).toBe("failed (blocked)");
    expect(endSaid({ how: "unreported", outcome: "ended without a report" })).toBe(
      "ended without a report",
    );
    expect(
      endsOf([
        past({ id: "a" }),
        past({ id: "b", how: "failed", outcome: "failed" }),
        past({ id: "c" }),
      ]),
    ).toEqual([
      { how: "done", said: "done" },
      { how: "failed", said: "failed" },
    ]);
  });

  it("says the person where they asked themselves", () => {
    expect(askedBy({ asker: "steward 4", by_person: false })).toBe("steward 4");
    expect(askedBy({ asker: "steward 4", by_person: true })).toBe("you, from steward 4");
  });

  it("is a view keyed by its workspace, and by no name for the project's root", () => {
    expect(pastTasksView("alpha")).toEqual({ from: null, view: "past-tasks", key: "alpha" });
    expect(pastTasksTitle("alpha")).toBe("Past tasks · alpha");
    expect(pastTasksTitle("")).toBe("Past tasks · project root");
  });
});

describe("a later read of the list", () => {
  it("puts what ended since where its time says, and a row written again in its own place", () => {
    const held = [
      past({ id: "b", ended: "2026-10-07T12:00:00+00:00" }),
      past({ id: "a", ended: "2026-10-06T12:00:00+00:00" }),
    ];
    const merged = mergedPast(held, [
      past({ id: "c", ended: "2026-10-08T12:00:00+00:00" }),
      past({ id: "a", ended: "2026-10-06T12:00:00+00:00", reopened: true }),
    ]);
    expect(merged.map((row) => row.id)).toEqual(["c", "b", "a"]);
    expect(merged[2].reopened).toBe(true);
    // A read that answered nothing leaves the list as it was.
    expect(mergedPast(held, [])).toEqual(held);
  });
});

describe("a long text", () => {
  it("is left whole where it fits, and cut by lines and by characters where it does not", () => {
    expect(clipped("short")).toBeUndefined();
    const many = Array.from({ length: CLIP_LINES + 3 }, (_, at) => `line ${at}`).join("\n");
    expect(clipped(many)?.split("\n")).toHaveLength(CLIP_LINES);
    // Cut at a character, never inside one.
    const wide = "🙂".repeat(CLIP_CHARS + 10);
    expect([...(clipped(wide) ?? "")]).toHaveLength(CLIP_CHARS);
  });
});
