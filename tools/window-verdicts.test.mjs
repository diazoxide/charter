// The window rows' verdicts as `tools/bench.mjs` prints them (FR-27, row L9). Run with
// `node --test tools/`.
import assert from "node:assert/strict";
import { test } from "node:test";

import { windowVerdicts } from "./window-verdicts.mjs";

test("a row that carries `met` prints met or MISSED with its p95 and budget", () => {
  const lines = windowVerdicts({
    webgl: {
      projects: { "project switch": { budgetMs: 200, met: true, p95: 171.4 } },
      panes: { "pane switch": { budgetMs: 100, met: false, p95: 130 } },
    },
  });
  assert.deepEqual(lines, [
    "webgl projects › project switch: met (p95 171 ms, budget 200 ms)",
    "webgl panes › pane switch: MISSED (p95 130 ms, budget 100 ms)",
  ]);
});

test("a spec that failed and recorded no verdict says so, apart from a miss", () => {
  const lines = windowVerdicts({
    webgl: {
      "projects.bench.ts failed": "Error: npx wdio … failed: 1",
      burst: { "2 MB": { ms: 40 } },
    },
  });
  assert.deepEqual(lines, ["webgl projects.bench.ts: failed, no verdict (Error: npx wdio … failed: 1)"]);
});

test("a failed spec whose record carries a verdict is that verdict, not a crash", () => {
  const lines = windowVerdicts({
    dom: {
      "projects.bench.ts failed": "Error: npx wdio … failed: 1",
      projects: { "project switch": { budgetMs: 200, met: false, p95: 240 } },
    },
  });
  assert.deepEqual(lines, ["dom projects › project switch: MISSED (p95 240 ms, budget 200 ms)"]);
});

test("rows without a verdict, skipped specs and an empty run print nothing", () => {
  assert.deepEqual(
    windowVerdicts({ webgl: { "load.bench.ts skipped": "the screen is locked", load: { x: { ms: 1 } } } }),
    [],
  );
  assert.deepEqual(windowVerdicts(undefined), []);
});
