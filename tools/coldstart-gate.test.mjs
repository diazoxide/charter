// The Linux cold-start gate's decision (ADR 0086 row L6, as amended by V39). Run with
// `node --test tools/`.
import assert from "node:assert/strict";
import { test } from "node:test";

import { coldStartGate } from "./coldstart-gate.mjs";

const gate = { limit: 2000, ceiling: 2500 };

test("one launch past the ceiling is reported and does not fail the gate", () => {
  // main's red run of 2026-10-02: a median of 1373 ms and one launch at 2687 ms.
  const result = coldStartGate({ launches: [1350, 1373, 2687, 1400, 1360], ...gate });
  assert.equal(result.met, true);
  assert.equal(result.median, 1373);
  assert.equal(result.worst, 2687);
  assert.deepEqual(result.ignored, [2687]);
});

test("two launches past the ceiling fail the gate", () => {
  const result = coldStartGate({ launches: [1350, 2537, 2687, 1400, 1360], ...gate });
  assert.equal(result.met, false);
});

test("a median past the limit fails the gate with no launch past the ceiling", () => {
  const result = coldStartGate({ launches: [2100, 2050, 2200, 1900, 2010], ...gate });
  assert.equal(result.met, false);
  assert.equal(result.median, 2050);
  assert.deepEqual(result.ignored, []);
});

test("five launches inside the limit meet the gate", () => {
  assert.equal(coldStartGate({ launches: [1350, 1373, 1560, 1400, 1360], ...gate }).met, true);
});

test("no launch at all does not meet the gate", () => {
  assert.equal(coldStartGate({ launches: [], ...gate }).met, false);
});

test("without a ceiling every launch is held to the limit, with no outlier ignored", () => {
  assert.equal(coldStartGate({ launches: [1350, 1373, 2001], limit: 2000 }).met, false);
  assert.equal(coldStartGate({ launches: [1350, 1373, 2000], limit: 2000 }).met, true);
});
