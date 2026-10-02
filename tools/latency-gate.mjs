// Whether a change made the session layer's latency worse than main's (SC-16, ADR 0086's CI
// relative rows, as the amendment SC-16 proposes words them).
//
// main and the change are measured in the same job, a round at a time, in alternating order
// (`tools/bench.mjs --only host --baseline`). A round is one run of each build, and each run's
// value is the median of its own samples. So the runner's speed, which on a shared runner moves
// by more than 20% between jobs, is in both halves of every round's ratio and cancels out.
//
// A row has REGRESSED only when all of these hold:
//   - the median of its rounds' ratios (the change over main) is more than 1.20;
//   - all its rounds but at most one are past 1.20, so one slow round is never the verdict;
//   - a second pass, with rounds of its own, says the same.
// A first pass that says so asks for the second (`confirm`); a second that does not is reported
// as `unconfirmed` and passes.
//
// A row is too NOISY to judge, and passes, when main does not agree with itself: its rounds,
// with the slowest and the fastest set aside, spread by more than the threshold.
//
// `passes` is a list of passes, each a list of rounds `{ base, head }` in ms. No rounds at all
// is `no baseline`: main has no such row yet, or its build of the bench did not run.
const THRESHOLD = 1.2;

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

function judge(rounds) {
  const ratios = rounds.map(({ base, head }) => head / base);
  const ratio = median(ratios);
  const over = ratios.filter((one) => one > THRESHOLD).length;
  const sustained = over >= Math.max(1, ratios.length - 1);
  const bases = rounds.map(({ base }) => base).sort((a, b) => a - b);
  const kept = bases.length >= 4 ? bases.slice(1, -1) : bases;
  const spread = kept.at(-1) / kept[0];
  return {
    ratio: Math.round(ratio * 1000) / 1000,
    spread: Math.round(spread * 1000) / 1000,
    over,
    noisy: spread > THRESHOLD,
    regressed: ratio > THRESHOLD && sustained,
  };
}

export function latencyGate({ passes }) {
  const judged = passes.filter((rounds) => rounds.length > 0).map(judge);
  if (judged.length === 0) return { verdict: "no baseline" };
  const last = judged.at(-1);
  const seen = { ratio: last.ratio, spread: last.spread, over: last.over };
  if (last.noisy) return { verdict: "noisy", ...seen };
  const unconfirmed = judged.slice(0, -1).some((one) => one.regressed);
  if (!last.regressed) return { verdict: "pass", ...seen, unconfirmed };
  if (judged.length >= 2 && judged.every((one) => one.regressed)) return { verdict: "regressed", ...seen };
  return { verdict: "confirm", ...seen };
}
