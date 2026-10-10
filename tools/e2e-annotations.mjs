// Each scenario test that failed, as a GitHub annotation on its spec's line.
//
// Run by the `scenario` job when it fails: `node ../tools/e2e-annotations.mjs logs/failures.jsonl`
// from `app/`. The scenario configs' `afterTest` and `afterHook` write one line of JSON per
// failure into `app/logs/failures.jsonl` (`app/e2e/failures.ts`); this turns them into
// `::error file=…,line=…,title=…::message` lines. An annotation is read through the checks API,
// which is the one place a failure is visible without the job's log or its artifact.
//
// A mass failure stays readable: at most LIMIT failures, then a notice that counts the rest.
// GitHub keeps only ten error and ten warning annotations per step, so the first ten failures
// are errors and the next ten warnings (titled "also failed"); past that they would be dropped
// silently, and the count is the one line that still says how many there were. A job
// that failed with no test recorded (the build, a step that is not WebdriverIO, an app that never
// started a spec) gets one annotation that says so.

import { existsSync, readFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative, sep } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

/** The repository's root: annotations name files relative to it. */
const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

/** How many failures are annotated one by one: ten errors, then ten warnings. */
export const LIMIT = 20;

/** GitHub's own limit on annotations of one level in one step. */
const PER_LEVEL = 10;

/** How much of an assertion's message an annotation keeps. */
export const MESSAGE_CHARS = 300;

// Colour codes the spec reporter and `expect` put in their messages.
// eslint-disable-next-line no-control-regex
const ANSI = /\u001b\[[0-9;]*m/g;

/** The records in a `failures.jsonl`, skipping any line that is not JSON (a write cut short). */
export function readRecords(text) {
  const records = [];
  for (const line of text.split("\n")) {
    if (!line.trim()) continue;
    try {
      records.push(JSON.parse(line));
    } catch {
      // A worker killed mid-write leaves half a line; the other records still count.
    }
  }
  return records;
}

/** `file` as GitHub wants it: relative to the repository, with forward slashes. */
function repoPath(file, root) {
  const plain = file.startsWith("file://") ? fileURLToPath(file) : file;
  const rel = isAbsolute(plain) ? relative(root, plain) : plain;
  return rel.split(sep).join("/");
}

/**
 * The line in `file` the failure points at: the first stack frame in that file, or else the
 * line that names the test's title, or else 1.
 */
export function lineOf(record, root, read = readSource) {
  const file = record.file ?? "";
  const base = file.startsWith("file://") ? fileURLToPath(file) : file;
  if (base) {
    for (const frame of String(record.stack ?? "").split("\n")) {
      const at = frame.indexOf(base);
      if (at < 0) continue;
      const match = /^:(\d+)/.exec(frame.slice(at + base.length));
      if (match) return Number(match[1]);
    }
    const source = read(isAbsolute(base) ? base : join(root, base));
    if (source && record.title) {
      const index = source.split("\n").findIndex((line) => line.includes(record.title));
      if (index >= 0) return index + 1;
    }
  }
  return 1;
}

function readSource(path) {
  try {
    return existsSync(path) ? readFileSync(path, "utf8") : "";
  } catch {
    return "";
  }
}

/** The message an annotation carries: no colour codes, collapsed to MESSAGE_CHARS. */
export function trimMessage(message) {
  const plain = String(message ?? "")
    .replace(ANSI, "")
    .trim();
  if (plain.length <= MESSAGE_CHARS) return plain;
  return `${plain.slice(0, MESSAGE_CHARS - 1)}…`;
}

// Workflow command escaping: `%`, CR and LF in data; `:` and `,` too in a property.
const escapeData = (s) => s.replace(/%/g, "%25").replace(/\r/g, "%0D").replace(/\n/g, "%0A");
const escapeProperty = (s) => escapeData(s).replace(/:/g, "%3A").replace(/,/g, "%2C");

/**
 * The `::error` lines for `records`, at most `limit` of them and a last one counting the rest.
 *
 * @param records what `app/e2e/failures.ts` wrote: `{ file, title, parent, message, stack }`.
 */
export function annotations(records, { root = ROOT, limit = LIMIT, read = readSource } = {}) {
  if (records.length === 0) {
    return [
      "::error title=scenario tests failed with no test recorded::" +
        escapeData(
          "No scenario test recorded a failure: a build step, the relaunch test or the app's start failed. The step marked failed in this job says which.",
        ),
    ];
  }
  const lines = records.slice(0, limit).map((record, index) => {
    const level = index < PER_LEVEL ? "error" : "warning";
    const props = [];
    if (record.file) {
      props.push(`file=${escapeProperty(repoPath(record.file, root))}`);
      props.push(`line=${lineOf(record, root, read)}`);
    }
    const named = [record.parent, record.title].filter(Boolean).join(" › ") || "scenario test";
    const title = level === "error" ? named : `also failed › ${named}`;
    props.push(`title=${escapeProperty(title)}`);
    const message = trimMessage(record.message) || "failed with no message";
    return `::${level} ${props.join(",")}::${escapeData(message)}`;
  });
  const rest = records.length - lines.length;
  if (rest > 0) {
    lines.push(
      `::notice title=+${rest} more failed scenario tests::${escapeData(
        `${rest} more failed; ${records.length} in all. The first ${lines.length} are annotated.`,
      )}`,
    );
  }
  return lines;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const file = process.argv[2] ?? "logs/failures.jsonl";
  const records = existsSync(file) ? readRecords(readFileSync(file, "utf8")) : [];
  for (const line of annotations(records)) console.log(line);
}
