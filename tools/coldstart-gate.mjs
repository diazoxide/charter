// Whether a cold-start run meets its limit (docs/spec.md row L6, ADR 0086 as amended by V39).
//
// With a ceiling (CI's Linux gate): the median of the launches must be within the limit, and at
// most one launch may go past the ceiling. That one is an outlier, reported and not gated: one
// slow launch on a shared runner is not a regression (main went red twice on 2026-10-02 from a
// single launch each, 2687 ms and 2537 ms, with medians of 1373 and 1560 ms). Two or more past
// the ceiling fail.
//
// Without a ceiling: every launch is held to the limit.
//
// `launches` is in ms; the result's `over` is every launch past the ceiling (or the limit), and
// `ignored` the one outlier the gate let through, if any.
export function coldStartGate({ launches, limit, ceiling }) {
  const sorted = [...launches].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  const median = sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
  const worst = sorted.at(-1) ?? 0;
  if (ceiling === undefined) {
    const over = launches.filter((ms) => ms > limit);
    return { met: launches.length > 0 && over.length === 0, median, worst, over, ignored: [] };
  }
  const over = launches.filter((ms) => ms > ceiling);
  const met = launches.length > 0 && median <= limit && over.length <= 1;
  return { met, median, worst, over, ignored: met ? over : [] };
}
