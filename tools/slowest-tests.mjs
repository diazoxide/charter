// The slowest Rust tests and test binaries of a CI run, as two GitHub notices.
//
// Run by the `rust` job after its tests: `node tools/slowest-tests.mjs target/nextest/ci/junit.xml`.
// nextest writes that JUnit report (`.config/nextest.toml`, profile `ci`); this reads each
// testcase's binary, name and seconds. A notice is read through the checks API, which is the one
// place a run's numbers are visible without its log: the step timings say the tests take most of
// the job, and these say which tests.
//
// Two notices, each one line per entry: the LIMIT slowest tests, and the LIMIT binaries whose
// tests took longest in all. Tests run at once, so a binary's sum is machine time, not the wait.

import { existsSync, readFileSync } from "node:fs";
import process from "node:process";
import { fileURLToPath } from "node:url";

/** How many tests, and how many binaries, each notice lists. */
export const LIMIT = 30;

const ENTITIES = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };
const unescape = (s) => s.replace(/&(amp|lt|gt|quot|apos);/g, (_, name) => ENTITIES[name]);

/** Every `<testcase>` in a JUnit report: `{ binary, name, seconds }`, in the report's order. */
export function readCases(xml) {
  const cases = [];
  for (const [, attrs] of xml.matchAll(/<testcase\b([^>]*)>/g)) {
    const attr = (key) => {
      const match = new RegExp(`\\b${key}="([^"]*)"`).exec(attrs);
      return match ? unescape(match[1]) : "";
    };
    cases.push({ binary: attr("classname"), name: attr("name"), seconds: Number(attr("time")) || 0 });
  }
  return cases;
}

/** The `limit` slowest tests, and the `limit` binaries whose tests took longest in all. */
export function slowest(cases, { limit = LIMIT } = {}) {
  const tests = [...cases].sort((a, b) => b.seconds - a.seconds).slice(0, limit);
  const sums = new Map();
  for (const { binary, seconds } of cases) {
    const sum = sums.get(binary) ?? { binary, seconds: 0, tests: 0 };
    sum.seconds += seconds;
    sum.tests += 1;
    sums.set(binary, sum);
  }
  const binaries = [...sums.values()]
    .sort((a, b) => b.seconds - a.seconds)
    .slice(0, limit);
  return { tests, binaries };
}

// Workflow command escaping: `%`, CR and LF in data; `:` and `,` too in a property.
const escapeData = (s) => s.replace(/%/g, "%25").replace(/\r/g, "%0D").replace(/\n/g, "%0A");
const escapeProperty = (s) => escapeData(s).replace(/:/g, "%3A").replace(/,/g, "%2C");

const notice = (title, lines) => `::notice title=${escapeProperty(title)}::${escapeData(lines.join("\n"))}`;
const time = (seconds) => `${seconds.toFixed(1)} s`.padStart(8);

/** The two `::notice` lines for `cases`, or one that says there were none. */
export function annotations(cases, { limit = LIMIT } = {}) {
  if (cases.length === 0) return [notice("Slowest Rust tests", ["No JUnit report with tests was found."])];
  const { tests, binaries } = slowest(cases, { limit });
  return [
    notice(
      "Slowest Rust tests",
      tests.map((one) => `${time(one.seconds)}  ${one.binary}  ${one.name}`),
    ),
    notice(
      "Slowest Rust test binaries (their tests' time summed)",
      binaries.map((one) => `${time(one.seconds)}  ${one.binary} (${one.tests} tests)`),
    ),
  ];
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const file = process.argv[2] ?? "target/nextest/ci/junit.xml";
  const cases = existsSync(file) ? readCases(readFileSync(file, "utf8")) : [];
  for (const line of annotations(cases)) console.log(line);
}
