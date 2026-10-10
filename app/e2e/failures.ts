import { logLine } from "./processes.js";

/**
 * Every failed scenario test and hook, one line of JSON each in `logs/failures.jsonl`, for
 * `tools/e2e-annotations.mjs` to turn into GitHub annotations when the `scenario` job fails.
 * An annotation is the one place a failure can be read without the job's log or its artifact.
 *
 * A hook in the config rather than a reporter: a reporter would import `@wdio/reporter`, which
 * the app has only as another package's dependency.
 */
export const FAILURES = "failures.jsonl";

/** What `afterTest` and `afterHook` are handed about the test or hook that just ran. */
interface Ran {
  title: string;
  parent?: string;
  file?: string;
}

/** Writes `ran` down if it failed; a passing test or hook writes nothing. */
export function recordFailure(ran: Ran, error: unknown): void {
  if (!error) return;
  // An `Error`, or an object shaped like one once it has crossed a worker boundary.
  const shaped = typeof error === "object" ? (error as { message?: unknown; stack?: unknown }) : {};
  const message = typeof shaped.message === "string" ? shaped.message : String(error);
  const stack = typeof shaped.stack === "string" ? shaped.stack : undefined;
  logLine(FAILURES, {
    file: ran.file,
    parent: ran.parent,
    title: ran.title,
    message,
    stack,
  });
}
