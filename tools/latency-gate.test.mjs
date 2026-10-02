// The latency gate's decision (SC-16, ADR 0086's CI relative rows). Run with
// `node --test tools/`.
import assert from "node:assert/strict";
import { test } from "node:test";

import { latencyGate } from "./latency-gate.mjs";

/** Five interleaved rounds, each a median of one run's samples, main's and the change's. */
const rounds = (base, head) => base.map((b, n) => ({ base: b, head: head[n] }));

test("a change that measures the same as main passes", () => {
  const pass = rounds([2.0, 2.1, 1.9, 2.0, 2.05], [2.02, 2.08, 1.95, 2.01, 2.0]);
  assert.equal(latencyGate({ passes: [pass] }).verdict, "pass");
});

test("a 30% regression in every round asks for a second pass before it fails", () => {
  const slow = rounds([2.0, 2.1, 1.9, 2.0, 2.05], [2.6, 2.73, 2.47, 2.6, 2.67]);
  const first = latencyGate({ passes: [slow] });
  assert.equal(first.verdict, "confirm");
  assert.equal(first.ratio, 1.3);
});

test("a regression the second pass confirms fails", () => {
  const slow = rounds([2.0, 2.1, 1.9, 2.0, 2.05], [2.6, 2.73, 2.47, 2.6, 2.67]);
  assert.equal(latencyGate({ passes: [slow, slow] }).verdict, "regressed");
});

test("a regression the second pass does not see is reported, and passes", () => {
  const slow = rounds([2.0, 2.1, 1.9, 2.0, 2.05], [2.6, 2.73, 2.47, 2.6, 2.67]);
  const same = rounds([2.0, 2.1, 1.9, 2.0, 2.05], [2.02, 2.08, 1.95, 2.01, 2.0]);
  const result = latencyGate({ passes: [slow, same] });
  assert.equal(result.verdict, "pass");
  assert.equal(result.unconfirmed, true);
});

test("one slow round is not a sustained regression, even when it is far past 20%", () => {
  // Four rounds at 22% and one at 300%: the median of the ratios is past the threshold, but a
  // regression the change made shows in every round, and only one of these is far out.
  const spiky = rounds([2.0, 2.0, 2.0, 2.0, 2.0], [2.0, 2.0, 2.44, 6.0, 2.44]);
  assert.equal(latencyGate({ passes: [spiky] }).verdict, "pass");
});

test("a run in which main does not agree with itself is too noisy to judge, and passes", () => {
  // main's own rounds, its slowest and fastest set aside, still spread from 2.0 to 2.5, more
  // than the threshold: the runner cannot tell a 20% change from its own noise right now, so
  // nothing it says about the change counts.
  const noisy = rounds([2.0, 2.6, 2.1, 2.0, 2.5], [2.9, 3.4, 3.0, 2.9, 3.3]);
  const result = latencyGate({ passes: [noisy] });
  assert.equal(result.verdict, "noisy");
  assert.equal(result.spread, 1.25);
});

test("one slow round of main's does not make the row too noisy to judge", () => {
  // A local run under load: main's first round paid for a cold cache (36 ms against 18).
  const regressed = rounds([36.0, 18.6, 18.5, 18.3, 18.0], [46.8, 24.2, 24.1, 23.8, 23.4]);
  const result = latencyGate({ passes: [regressed] });
  assert.equal(result.verdict, "confirm");
  assert.equal(result.spread, 1.016);
});

test("a row main cannot measure has no baseline, and passes", () => {
  // The change adds the row, or main's build of the bench failed: nothing to compare with.
  assert.equal(latencyGate({ passes: [] }).verdict, "no baseline");
});

test("exactly 20% worse is not more than 20% worse", () => {
  const edge = rounds([2.0, 2.0, 2.0, 2.0, 2.0], [2.4, 2.4, 2.4, 2.4, 2.4]);
  assert.equal(latencyGate({ passes: [edge] }).verdict, "pass");
});
