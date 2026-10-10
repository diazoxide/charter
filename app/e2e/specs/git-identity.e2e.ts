import { realpathSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import {
  anEmptyRecord,
  copyFixturePlane,
  THE_RUNS_GIT_CONFIG,
  THE_RUNS_GIT_CONFIG_TEXT,
} from "../harness.js";
import { bandNotice } from "../reading.js";

/**
 * **A machine with no git identity is told so by a Notice** (#1250), in the built app against
 * the real core and the real doctor.
 *
 * `app/src/DoctorNotice.window.test.tsx` drives the Notice against a mocked core. What only a
 * real window can show is the whole path: git asked through charter's hardened runner, the
 * preflight the window runs when a project opens, and the finding standing in the band.
 *
 * **Every other spec runs with an identity** (`THE_RUNS_HOME`, D-1250-8), so this one takes it
 * away: it empties the run's own `.gitconfig` — never the runner's — opens a project of its
 * own, whose preflight then finds no identity, and puts the file back before anything else is
 * asked. One app process serves the whole run, so it also lets its project go again.
 */

const PROJECTS = '[data-strip="Projects"]';

const NAME = "no-identity";

const CAUSE = "doctor-finding:git-identity";

/** This spec's own project: the fixture plane, with nothing to put back on open. */
const mine = (() => {
  const copied = copyFixturePlane();
  const renamed = join(dirname(copied), NAME);
  renameSync(copied, renamed);
  const root = realpathSync(renamed);
  anEmptyRecord(root);
  return root;
})();

/** What the app answered a command with, insisting it answered at all. */
async function ask<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const answer = await browser.executeAsync(
    (
      name: string,
      passed: Record<string, unknown>,
      done: (out: { ok?: unknown; trouble?: string }) => void,
    ) => {
      void window.__TAURI__.core
        .invoke(name, passed)
        .then((ok) => done({ ok }))
        .catch((e: unknown) => done({ trouble: String(e) }));
    },
    command,
    args,
  );
  if (answer.trouble !== undefined) throw new Error(`${command} refused: ${answer.trouble}`);
  return answer.ok as T;
}

/** The run's identity, as every other spec has it. */
const putTheIdentityBack = () => writeFileSync(THE_RUNS_GIT_CONFIG, THE_RUNS_GIT_CONFIG_TEXT);

describe("a machine with no git identity", function () {
  this.timeout(180_000);

  /** The project the launch opened, which this spec never touches. */
  let first = "";

  before(async () => {
    first = (await ask<string[]>("open_planes"))[0];
  });

  after(async () => {
    putTheIdentityBack();
    const selector = `${PROJECTS} button[aria-label="Close project ${NAME}"]`;
    if ((await ask<string[]>("open_planes")).includes(mine))
      await ask("close_plane", { plane: mine });
    await browser.waitUntil(async () => !(await $(selector).isExisting()), {
      timeout: 20_000,
      timeoutMsg: "this spec's project's tab stayed on the strip",
    });
    expect((await ask<string[]>("open_planes"))[0]).toBe(first);
  });

  it("is shown the doctor's finding as a Notice, with its Fix", async () => {
    writeFileSync(THE_RUNS_GIT_CONFIG, "");
    try {
      // Through the opener, as `notices.e2e.ts` opens its own project: the preflight is asked
      // when a project opens, and that is the read this spec is about.
      await $(`${PROJECTS} button[aria-label="Open a project…"]`).click();
      const box = await $('[data-setting="open-by-path"] input');
      await box.waitForDisplayed({ timeout: 20_000 });
      await box.addValue(mine);
      // The path box's own submit, found by its form: its name is the trust
      // question's Open project too, which comes next.
      await $('form.by-path button[type="submit"]').click();
      const question = await $('[role="dialog"]');
      await question.waitForDisplayed({ timeout: 30_000 });
      await $("button=Open project").click();
      await browser.waitUntil(async () => (await ask<string[]>("open_planes")).includes(mine), {
        timeout: 30_000,
        timeoutMsg: "this spec's project never opened",
      });

      let said = "";
      await browser.waitUntil(
        async () => {
          const now = await bandNotice(CAUSE);
          if (now.state === "shown") said = now.text;
          return now.state === "shown";
        },
        { timeout: 60_000, timeoutMsg: "no Notice said the git identity is not set" },
      );
      expect(said).toContain("git identity: not set: user.name, user.email");
      await expect($(`[data-cause="${CAUSE}"]`).$("button=Fix")).toBeExisting();
    } finally {
      putTheIdentityBack();
    }
  });
});
