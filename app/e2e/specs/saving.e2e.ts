import { execFileSync } from "node:child_process";
import { mkdirSync, realpathSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { copyFixturePlane } from "../harness.js";
import { closeProject } from "../opening.js";

/**
 * **The save indicator and the save button, in the built app** (charter-app#294, ADR 0051).
 *
 * A copy of the `daily` fixture made into a git repository with one commit and no remote, opened
 * through the window's own gate. A file written into it is what the title bar then says is
 * unsaved; the bar's save button runs the core's one save function, and with no remote the
 * commit is as far as it goes — so the bar moves from *1 changed* back to *Saved*.
 *
 * What is proved here and not in jsdom: `plane_saving` reads a real repository, `save_plane`
 * commits in it through `planegit::save_as`, and the bar re-reads after the save.
 *
 * **It leaves the window as it found it**: the project it opens it lets go of.
 */

const PROJECTS = '[role="tablist"][aria-label="Projects"]';
const BAR = '[data-testid="title-bar"]';

/** git in the fixture: never the machine's own config, so never its signer. */
function git(dir: string, args: string[]): void {
  execFileSync("git", ["-c", "commit.gpgsign=false", ...args], {
    cwd: dir,
    env: {
      ...process.env,
      GIT_CONFIG_GLOBAL: "/dev/null",
      GIT_CONFIG_NOSYSTEM: "1",
      GIT_AUTHOR_NAME: "fixture",
      GIT_AUTHOR_EMAIL: "fixture@example.invalid",
      GIT_COMMITTER_NAME: "fixture",
      GIT_COMMITTER_EMAIL: "fixture@example.invalid",
    },
    stdio: "ignore",
  });
}

/** The words the bar's indicator carries, or `null` while it draws none. */
async function indicator(): Promise<string | null> {
  return browser.execute(
    (bar: string) =>
      document.querySelector(`${bar} button[aria-label^="Saving:"]`)?.getAttribute("aria-label") ??
      null,
    BAR,
  );
}

/** Waits until the bar says `want`, and when it never does, says what it said instead. */
async function untilTheBarSays(want: string, what: string): Promise<void> {
  let last: string | null = null;
  try {
    await browser.waitUntil(
      async () => {
        last = await indicator();
        return last === want;
      },
      { timeout: 30_000, interval: 250 },
    );
  } catch {
    throw new Error(`${what}: the bar said ${JSON.stringify(last)}, not ${JSON.stringify(want)}`);
  }
}

describe("saving the project from the title bar", function () {
  this.timeout(120_000);

  const plane = (() => {
    const copied = copyFixturePlane();
    const renamed = join(dirname(copied), "saving-plane");
    renameSync(copied, renamed);
    mkdirSync(join(renamed, ".charter"), { recursive: true });
    // The fixture's own `.gitignore` already leaves `.charter/` and `charter.local.toml` out, and
    // everything else it ignores is what a real plane ignores: kept as it is.
    git(renamed, ["init", "-q", "-b", "main"]);
    git(renamed, ["config", "user.name", "fixture"]);
    git(renamed, ["config", "user.email", "fixture@example.invalid"]);
    git(renamed, ["add", "-A"]);
    git(renamed, ["commit", "-q", "-m", "the fixture"]);
    return realpathSync(renamed);
  })();

  after(async () => {
    const selector = `${PROJECTS} button[aria-label="Close project saving-plane"]`;
    const closer = await $(selector);
    if (!(await closer.isExisting())) return;
    await closeProject(selector);
    await browser.waitUntil(async () => !(await closer.isExisting()), {
      timeout: 20_000,
      timeoutMsg: "the saving-plane project's tab stayed on the strip",
    });
  });

  it("says a file written into the plane is unsaved, and saves it from the bar", async () => {
    await $(`${PROJECTS} button[aria-label="Open a project…"]`).click();
    const box = await $("#open-by-path");
    await box.waitForDisplayed({ timeout: 20_000 });
    await box.addValue(plane);
    await $("button=Open").click();
    const question = await $('[role="dialog"]');
    await question.waitForDisplayed({ timeout: 30_000 });
    await $("button=Open project").click();

    await untilTheBarSays("Saving: Saved", "the bar never said the fresh plane was saved");

    writeFileSync(join(plane, "note.md"), "written by the scenario\n");
    // Focus is one of the reads' triggers, and the quickest one a spec can pull.
    await browser.execute(() => window.dispatchEvent(new Event("focus")));
    await untilTheBarSays("Saving: 1 changed", "the bar never said the written file was unsaved");

    await $(`${BAR} button[aria-label="Save the project"]`).click();

    // No remote: the commit is as far as a save goes, so once it is made the plane is saved
    // and there is nothing left for the button to take.
    await untilTheBarSays("Saving: Saved", "the bar never said the saved plane was saved");
    expect(await $(`${BAR} button[aria-label="Save the project"]`).isExisting()).toBe(false);
  });
});
