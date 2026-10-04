/**
 * What a run of project switches measured, and whether that holds FR-27's acceptance (row L9 of
 * the spec's budgets of record): switching among ten open projects within 200 ms at the p95.
 *
 * `switching.ts` drives the window; this file only reads the numbers it took, so the reading
 * can be tested without an app (`memory.ts` is split from `processes.ts` the same way).
 */

/** How many projects the window holds while it is measured: the acceptance's ten. */
export const OPEN = 10;

/** FR-27's budget, in milliseconds: switching among ten open projects. */
export const BUDGET_MS = 200;

function percentile(sorted: number[], at: number): number {
  return sorted[Math.min(sorted.length - 1, Math.floor((at / 100) * sorted.length))];
}

function ascending(samples: readonly number[]): number[] {
  return [...samples].sort((a, b) => a - b);
}

/** What `measureSwitches` found. */
export type Switches = {
  projects: number;
  samples: number;
  /** From the press of the row to the paint of the pane of the project's chat. */
  p50: number;
  /** L9's statistic (D-0009). */
  p95: number;
  p99: number;
  worst: number;
  /** From the press to the frame after the project is in front, before its pane has painted. */
  shownP50: number;
  /** From the switcher's key to the frame its row is drawn in: getting to the row. */
  openP50: number;
  openWorst: number;
  samples_ms: number[];
};

/**
 * The switches' statistics, each array in the order the switches were taken.
 *
 * @param painted Press to the pane's paint, per switch: what L9 is held on.
 * @param shown Press to the frame the project is in front, per switch.
 * @param opening The switcher's key to its row drawn, per switch.
 */
export function summarise(
  projects: number,
  painted: readonly number[],
  shown: readonly number[],
  opening: readonly number[],
): Switches {
  const sorted = ascending(painted);
  const opened = ascending(opening);
  return {
    projects,
    samples: painted.length,
    p50: percentile(sorted, 50),
    p95: percentile(sorted, 95),
    p99: percentile(sorted, 99),
    worst: sorted[sorted.length - 1],
    shownP50: percentile(ascending(shown), 50),
    openP50: percentile(opened, 50),
    openWorst: opened[opened.length - 1],
    samples_ms: [...painted],
  };
}

/** Whether a run holds L9, and the sentence that says so beside its numbers. */
export type Verdict = { met: boolean; sentence: string };

/**
 * Holds a run to L9: **met** only when ten projects were open, every one of `rounds` rounds
 * switched to each, and the p95 is within the budget. The budgets of record write the row as
 * "≤ 200 ms at the p95", so a p95 of exactly 200 ms is met. A run with a project or a switch
 * missing is never met, however fast: it did not measure what the acceptance names.
 */
export function heldToBudget(switches: Switches, rounds: number): Verdict {
  const complete = switches.projects >= OPEN && switches.samples === switches.projects * rounds;
  const met = complete && switches.p95 <= BUDGET_MS;
  const sentence =
    `L9: p95 ${Math.round(switches.p95)} ms against a ${BUDGET_MS} ms budget, ` +
    `${switches.samples} switches among ${switches.projects} projects: ${met ? "met" : "MISSED"}`;
  return { met, sentence };
}
