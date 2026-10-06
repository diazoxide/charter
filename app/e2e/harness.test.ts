import { execFileSync } from "node:child_process";
import { join } from "node:path";
import process from "node:process";
import { describe, expect, it } from "vitest";
import { singleQuoted, theRunsEnvironment, THE_RUNS_GIT_IDENTITY } from "./harness.js";

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

/**
 * Every app a run launches commits as the run, whatever the runner's git config says (#1250):
 * a Linux runner has no identity, and the doctor's finding then stood in every spec's band.
 */
describe("the home a launched app is given", () => {
  it("holds the run's own git identity, the one the doctor's cleared git reads", () => {
    const env = theRunsEnvironment("/unused");
    // As charter's hardened runner asks it: a cleared environment, `HOME` alone.
    const ask = (key: string) =>
      execFileSync("git", ["config", "--global", "--get", key], {
        encoding: "utf8",
        env: { HOME: env.HOME, PATH: process.env.PATH ?? "" },
      }).trim();
    expect(ask("user.name")).toBe(THE_RUNS_GIT_IDENTITY.name);
    expect(ask("user.email")).toBe(THE_RUNS_GIT_IDENTITY.email);
    expect(env.GIT_CONFIG_GLOBAL).toBe(join(env.HOME, ".gitconfig"));
    expect(env.HOME.startsWith(env.CHARTER_PLANE_FENCE)).toBe(true);
  });
});
