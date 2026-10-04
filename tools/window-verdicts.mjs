// What the window's rows say against their budgets, as `tools/bench.mjs` prints them after the
// numbers (FR-27, row L9).
//
// A bench spec that holds a release absolute row records the row with `met` and then fails
// when it is missed (`app/e2e/bench/projects.bench.ts`). `bench.mjs` keeps a failed spec as
// `<file> failed` beside whatever it recorded, so without this a miss and a crash read the
// same. A row's `met` decides; a failed spec with no row carrying `met` is the crash.

const FAILED = " failed";

/**
 * One line per verdict, in the order the run recorded them.
 *
 * @param window `results.window`: each arm's specs, by the name each spec recorded under, plus
 *   `<file>.bench.ts failed` and `<file>.bench.ts skipped` entries.
 * @returns lines such as `webgl projects › project switch: met (p95 171 ms, budget 200 ms)`.
 */
export function windowVerdicts(window) {
  const lines = [];
  for (const [arm, specs] of Object.entries(window ?? {})) {
    const judged = new Set();
    for (const [spec, rows] of Object.entries(specs)) {
      if (typeof rows !== "object" || rows === null) continue;
      for (const [row, one] of Object.entries(rows)) {
        if (typeof one?.met !== "boolean") continue;
        judged.add(spec);
        const numbers =
          typeof one.p95 === "number" && typeof one.budgetMs === "number"
            ? ` (p95 ${Math.round(one.p95)} ms, budget ${one.budgetMs} ms)`
            : "";
        lines.push(`${arm} ${spec} › ${row}: ${one.met ? "met" : "MISSED"}${numbers}`);
      }
    }
    for (const [key, trouble] of Object.entries(specs)) {
      if (!key.endsWith(FAILED)) continue;
      const file = key.slice(0, -FAILED.length);
      if (judged.has(file.replace(/\.bench\.ts$/, ""))) continue;
      lines.push(`${arm} ${file}: failed, no verdict (${trouble})`);
    }
  }
  return lines;
}
