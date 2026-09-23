import { readFileSync, realpathSync, renameSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import { copyFixturePlane } from "../harness.js";

/**
 * **A tab that holds something other than a chat**, in the built app (charter ADR 0043, as
 * amended 2026-09-23). The operator, choosing a tab for the persona card: *"we dont have other
 * tabs then sessions, and this can be good example for us - that in tabs we can have what we
 * want - not only harnesses"*.
 *
 * Against the `daily` fixture's two real personas — `devops` (`vault: devops`, a role, a
 * `delegate-when`) and `steward` (`vault: none`, the plane's default) — through the real core:
 * the persona view is `open_view` answered by `panels::persona_view` off the plane on disk.
 *
 * What is proved here and not in jsdom: the command exists and answers off a real plane, a
 * row's click reaches a built window's tab strip, the record on disk carries the tab, and a
 * project opened with such a record puts the tab back. **A cold relaunch is not reachable
 * from here** — one app process serves the run — so "comes back" is proved the way
 * `projects.e2e.ts` proves its half of decision 28: a project opened through the gate with a
 * record that names a view tab, which is the same `put_back` and the same `reopened_views` a
 * launch goes through.
 *
 * **It leaves the window as it found it**: every tab it opens it closes, and the project it
 * opens it lets go of.
 */

const TABS = '[role="tablist"][aria-label="Tabs"]';
const PROJECTS = '[role="tablist"][aria-label="Projects"]';

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

/** The names on the chat strip, left to right. */
async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [
      ...(document
        .querySelector('[role="tablist"][aria-label="Tabs"]')
        ?.querySelectorAll('[role="tab"]') ?? []),
    ].map((tab) => tab.textContent ?? ""),
  );
}

/** The name of the tab in front. */
async function inFront(): Promise<string | null> {
  return browser.execute(
    () =>
      document.querySelector(
        '[role="tablist"][aria-label="Tabs"] [role="tab"][aria-selected="true"]',
      )?.textContent ?? null,
  );
}

/** Waits until the plane's panels are drawn, so a persona's row can be pressed. */
async function untilThePersonasAreListed(): Promise<void> {
  await browser.waitUntil(
    async () => (await $('[data-testid="panel-personas"]').getText()).includes("devops"),
    { timeout: 30_000, interval: 250, timeoutMsg: "the personas panel never listed devops" },
  );
}

/** A persona's row in the right-hand region, by its own name. */
async function personaRow(persona: string) {
  const rows = await $$('[data-testid="panel-personas"] button').getElements();
  for (const row of rows) {
    if ((await row.getText()).startsWith(persona)) return row;
  }
  throw new Error(`no ${persona} among the personas the panel lists`);
}

/** The persona's view, once it has answered. */
async function theView(persona: string, saying: string) {
  const view = await $(`[data-testid="view-pane-charter-persona-${persona}"]`);
  await view.waitForExist({ timeout: 20_000 });
  await browser.waitUntil(async () => (await view.getText()).includes(saying), {
    timeout: 20_000,
    timeoutMsg: `${persona}'s tab never said ${JSON.stringify(saying)}`,
  });
  return view;
}

/** Closes the view tab called `name`, which asks nothing because it ends nothing. */
async function closeTheTab(name: string): Promise<void> {
  await $(`${TABS} button[aria-label="Close ${name}"]`).click();
  await browser.waitUntil(async () => !(await tabNames()).includes(name), {
    timeout: 20_000,
    timeoutMsg: `the ${name} tab did not close`,
  });
  // No question: closing a view ends no chat.
  expect(await $('[role="alertdialog"]').isExisting()).toBe(false);
}

// On the outermost describe, where WebdriverIO reads it before any runnable is built — a
// `this.timeout()` anywhere after the first test is silently ignored (`e2e/budget.test.ts`).
// Opening a second project reads its settings, its record and this machine's store, and then
// repaints the whole window; three minutes is mocha's own default and is plenty.
describe("view tabs", function () {
  this.timeout(180_000);

  describe("a persona's own tab", () => {
    after(async () => {
      for (const name of ["devops", "steward"])
        if ((await tabNames()).includes(name)) await closeTheTab(name);
    });

    it("opens from the persona's row, in front, and says what the definition says", async () => {
      await untilThePersonasAreListed();

      await (await personaRow("devops")).click();

      const view = await theView("devops", "DevOps Engineer");
      expect(await inFront()).toBe("devops");
      const said = await view.getText();
      // `delegate-when` is what makes a persona findable, and what a router reads.
      expect(said).toContain("k8s deploys");
      // The vault's NAME, and nothing that is in it.
      expect(said).toContain("Vault: devops");
      expect(said).toContain("personas/devops/persona.md");
      expect(said).toMatch(/It remembers \d+ things?\./);
    });

    it("says a persona holds no credentials where its definition says so", async () => {
      await untilThePersonasAreListed();

      await (await personaRow("steward")).click();

      await theView("steward", "holds no credentials of its own");
      expect(await inFront()).toBe("steward");
    });

    it("is brought forward, not opened twice, when its row is pressed again", async () => {
      await untilThePersonasAreListed();
      const before = (await tabNames()).filter((name) => name === "devops").length;
      expect(before).toBe(1);

      await (await personaRow("devops")).click();

      await browser.waitUntil(async () => (await inFront()) === "devops", {
        timeout: 20_000,
        timeoutMsg: "pressing devops again did not bring its tab forward",
      });
      expect((await tabNames()).filter((name) => name === "devops")).toHaveLength(1);
    });

    it("is written into the plane's record, so the next launch can put it back", async () => {
      const plane = (await ask<string[]>("open_planes"))[0];
      const record = join(plane, ".charter", "app", "reopen.json");

      let views: { view: string; key: string; active?: boolean }[] = [];
      await browser.waitUntil(
        async () => {
          views =
            (JSON.parse(readFileSync(record, "utf8")) as { views?: typeof views }).views ?? [];
          return views.some((one) => one.key === "devops");
        },
        { timeout: 20_000, timeoutMsg: "the record never named the devops tab" },
      );

      expect(views.map((one) => `${one.view}/${one.key}`)).toEqual(
        expect.arrayContaining(["persona/devops", "persona/steward"]),
      );
      expect(views.find((one) => one.key === "devops")?.active).toBe(true);
    });
  });

  describe("view tabs a record names", () => {
    /** A project of this spec's own, whose record names one view tab and no chat. */
    const other = (() => {
      const copied = copyFixturePlane();
      const renamed = join(dirname(copied), "views-back");
      renameSync(copied, renamed);
      mkdirSync(join(renamed, ".charter", "app"), { recursive: true });
      writeFileSync(
        join(renamed, ".charter", "app", "reopen.json"),
        `${JSON.stringify({
          version: 1,
          at: 0,
          chats: [],
          views: [
            {
              from: "",
              view: "persona",
              key: "steward",
              title: "steward",
              workspace: "alpha",
              at: 0,
              active: true,
            },
          ],
        })}\n`,
      );
      return realpathSync(renamed);
    })();

    after(async () => {
      if ((await ask<string[]>("open_planes")).includes(other))
        await ask("close_plane", { plane: other });
    });

    it("come back in front when their project is opened, and read the plane again", async () => {
      await $(`${PROJECTS} button[aria-label="Open a project…"]`).click();
      const box = await $("#open-by-path");
      await box.waitForDisplayed({ timeout: 20_000 });
      await box.addValue(other);
      await $("button=Open").click();
      // Through the gate, which is what reads the record (ADR 0035).
      const question = await $('[role="dialog"]');
      await question.waitForDisplayed({ timeout: 30_000 });
      await $("button=Open project").click();

      await theView("steward", "holds no credentials of its own");
      expect(await tabNames()).toEqual(["steward"]);
      expect(await inFront()).toBe("steward");
    });
  });
});
