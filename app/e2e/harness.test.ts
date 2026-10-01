import { execFileSync } from "node:child_process";
import { describe, expect, it } from "vitest";
import { singleQuoted } from "./harness.js";

/**
 * A path a launcher writes into a script reaches `sh` as one literal word (SC-15). A run's tree
 * is under the system's temporary directory, so a path is not ours to promise anything about.
 */
describe("a word written into a launcher", () => {
  it.each([
    "/plain/path",
    "/with space/app",
    "/it's/quoted",
    "/$HOME/and/`date`/and/$(id)",
    '/a "double" quote and a \\ backslash',
  ])("reaches sh exactly as written: %s", (word) => {
    const said = execFileSync("sh", ["-c", `printf %s ${singleQuoted(word)}`], {
      encoding: "utf8",
    });
    expect(said).toBe(word);
  });
});
