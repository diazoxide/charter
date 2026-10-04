import { describe, expect, it } from "vitest";
import { BUDGET_MS, OPEN, heldToBudget, summarise } from "./switchTimes.js";

/** Thirty switches, as `measureSwitches` takes them: ten projects, three rounds. */
function thirty(slowest: number[]): number[] {
  // 27 fresh-project switches at 100–126 ms, then the three slow ones (a round's first switch).
  return [...Array.from({ length: 27 }, (_, n) => 100 + n), ...slowest];
}

describe("summarise", () => {
  it("reads L9's p95 off thirty samples as the second-worst one", () => {
    const painted = thirty([190, 230, 260]);
    const switches = summarise(OPEN, painted, painted, painted);
    // 30 samples: floor(0.95 × 30) = 28th of 0..29, so the worst one alone never decides it.
    expect(switches.p95).toBe(230);
    expect(switches.worst).toBe(260);
    expect(switches.p50).toBe(115);
    expect(switches.samples).toBe(30);
    expect(switches.samples_ms).toEqual(painted);
  });
});

describe("heldToBudget", () => {
  it("is met when thirty switches among ten projects have a p95 within 200 ms", () => {
    const painted = thirty([150, 199, 260]);
    const verdict = heldToBudget(summarise(OPEN, painted, painted, painted), 3);
    expect(verdict.met).toBe(true);
    expect(verdict.sentence).toBe(
      "L9: p95 199 ms against a 200 ms budget, 30 switches among 10 projects: met",
    );
  });

  it("is met at exactly the budget, which the budgets of record write as ≤ 200 ms", () => {
    const painted = thirty([150, BUDGET_MS, 260]);
    expect(heldToBudget(summarise(OPEN, painted, painted, painted), 3).met).toBe(true);
  });

  it("is missed when the p95 is past 200 ms", () => {
    const painted = thirty([150, 201, 260]);
    const verdict = heldToBudget(summarise(OPEN, painted, painted, painted), 3);
    expect(verdict.met).toBe(false);
    expect(verdict.sentence).toBe(
      "L9: p95 201 ms against a 200 ms budget, 30 switches among 10 projects: MISSED",
    );
  });

  it("is missed when fewer than ten projects were open, however fast", () => {
    const painted = Array.from({ length: 27 }, () => 50);
    const verdict = heldToBudget(summarise(9, painted, painted, painted), 3);
    expect(verdict.met).toBe(false);
    expect(verdict.sentence).toContain("9 projects");
  });

  it("is missed when a switch is missing from the rounds, however fast", () => {
    const painted = Array.from({ length: 29 }, () => 50);
    expect(heldToBudget(summarise(OPEN, painted, painted, painted), 3).met).toBe(false);
  });
});
