import {
  readFileSync,
  readdirSync,
  realpathSync,
  renameSync,
  writeFileSync,
  mkdirSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import { copyFixturePlane } from "../harness.js";
import { closeProject } from "../opening.js";

/**
 * **A tab that holds something other than a chat**, in the built app (ADR 0043, as
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

/** The view tabs a plane's record names, read off the disk. */
function viewsIn(record: string): { view: string; key: string; active?: boolean }[] {
  try {
    return (JSON.parse(readFileSync(record, "utf8")) as { views?: [] }).views ?? [];
  } catch {
    return [];
  }
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
      expect(said).toMatch(/It remembers \d+ things?\./);
      // The definition is a `facts` block, drawn as a <dl>: read it as label -> value pairs,
      // which is what an operator reads, rather than as run-together text.
      // No named helpers inside `execute`: the spec's bundler wraps them in a `__name` the page
      // does not have.
      const facts: Record<string, string> = await browser.execute(() => {
        const out: Record<string, string> = {};
        for (const dt of document.querySelectorAll(
          '[data-testid="view-pane-charter-persona-devops"] dl dt',
        )) {
          const dd = dt.nextElementSibling;
          if (dd?.tagName === "DD")
            out[(dt.textContent ?? "").trim()] = (dd.textContent ?? "").trim();
        }
        return out;
      });
      // `delegate-when` is what makes a persona findable, and what a router reads.
      expect(facts["Delegate to it for"]).toContain("k8s deploys");
      // The vault's NAME, and nothing that is in it.
      expect(facts["Vault"]).toMatch(/^devops\b/);
      expect(facts["Defined in"]).toContain("personas/devops/persona.md");
    });

    it("says a persona holds no credentials where its definition says so", async () => {
      await untilThePersonasAreListed();

      await (await personaRow("steward")).click();

      await theView("steward", "holds no credentials of its own");
      expect(await inFront()).toBe("steward");
    });

    it("is brought forward, not opened twice, when its row is pressed again", async () => {
      await untilThePersonasAreListed();
      const record = join(
        (await ask<string[]>("open_planes"))[0],
        ".charter",
        "app",
        "reopen.json",
      );

      await (await personaRow("devops")).click();

      await browser.waitUntil(async () => (await inFront()) === "devops", {
        timeout: 20_000,
        timeoutMsg: "pressing devops again did not bring its tab forward",
      });
      // **Counted in the record, not on the strip.** The strip draws what fits and the tab in
      // front (`fits.ts`), so a second devops tab could be hidden past its edge; the core is told
      // every view tab the window has, drawn or not.
      await browser.waitUntil(
        async () => viewsIn(record).find((one) => one.key === "devops")?.active === true,
        {
          timeout: 20_000,
          timeoutMsg: "the record never said the devops tab was in front",
        },
      );
      expect(viewsIn(record).filter((one) => one.key === "devops")).toHaveLength(1);
    });

    it("is written into the plane's record, so the next launch can put it back", async () => {
      const plane = (await ask<string[]>("open_planes"))[0];
      const record = join(plane, ".charter", "app", "reopen.json");

      expect(viewsIn(record).map((one) => `${one.view}/${one.key}`)).toEqual(
        expect.arrayContaining(["persona/devops", "persona/steward"]),
      );
    });
  });

  describe("a memory's own tab (SI-9b, ADR 0065)", () => {
    const MEMORY = "Cluster prod-1 lives in eu-west-1";
    const FILE = join("personas", "devops", "memory", "cluster-prod-1-lives-in-eu-west-1.md");

    after(async () => {
      for (const name of [MEMORY, "devops"])
        if ((await tabNames()).includes(name)) await closeTheTab(name);
    });

    it("opens from a persona's memory row as a preview, read off the plane", async () => {
      await untilThePersonasAreListed();
      await (await personaRow("devops")).click();
      const persona = await theView("devops", MEMORY);

      await (await persona.$(`button*=${MEMORY}`)).click();

      const body = await $('[data-testid="memory-body"]');
      await body.waitForExist({ timeout: 20_000 });
      expect(await body.getText()).toContain(MEMORY);
      expect(await $('[data-testid="memory-meta"]').getText()).toContain(
        "personas/devops/memory/cluster-prod-1-lives-in-eu-west-1.md",
      );
      expect(await inFront()).toBe(MEMORY);
      // The strip's preview tab: italic until it is kept.
      expect(
        await browser.execute(
          () =>
            document.querySelector(
              '[role="tablist"][aria-label="Tabs"] [role="tab"][aria-selected="true"] .is-preview',
            ) !== null,
        ),
      ).toBe(true);
    });

    it("archives it on Delete, closes its tab, and Undo puts the file back", async () => {
      const plane = (await ask<string[]>("open_planes"))[0];
      const file = join(plane, FILE);
      const archived = join(plane, "personas", "devops", "memory", "archive");
      expect(readFileSync(file, "utf8")).toContain(MEMORY);

      await $(`button*=Delete memory`).click();

      const undo = await $('[data-testid="memory-undo"]');
      await undo.waitForExist({ timeout: 20_000 });
      await browser.waitUntil(async () => !(await tabNames()).includes(MEMORY), {
        timeout: 20_000,
        timeoutMsg: "the memory's tab did not close on Delete",
      });
      expect(() => readFileSync(file, "utf8")).toThrow();
      expect(
        readFileSync(join(archived, "cluster-prod-1-lives-in-eu-west-1.md"), "utf8"),
      ).toContain(MEMORY);

      await (await undo.$("button=Undo")).click();

      await browser.waitUntil(
        async () => {
          try {
            return readFileSync(file, "utf8").includes(MEMORY);
          } catch {
            return false;
          }
        },
        { timeout: 20_000, timeoutMsg: "Undo did not put the memory back under its own slug" },
      );
      expect(
        readFileSync(join(plane, "personas", "devops", "memory", "MEMORY.md"), "utf8"),
      ).toContain("(cluster-prod-1-lives-in-eu-west-1.md)");
    });
  });

  describe("a workspace's Memory section (SI-9c, ADR 0065 Q5, Q9)", () => {
    const TITLE = "Deploys freeze on Fridays";
    const BODY = "Nothing ships after Thursday noon.";
    const SECTION = '[data-testid="panel-memory"]';

    /** The journal's file for the memory this describe makes, or `undefined` before it is. */
    const made = (plane: string): string | undefined => {
      const dir = join(plane, "workspaces", "alpha", "memory");
      const file = readdirSync(dir).find((name) => name.endsWith("-deploys-freeze-on-fridays.md"));
      return file === undefined ? undefined : join(dir, file);
    };

    /** Focuses alpha from the workspace strip, by the tab's own name. */
    async function onAlpha(): Promise<void> {
      await browser.waitUntil(
        async () =>
          (await $$('[role="tablist"][aria-label="Workspaces"] .workspace-name').getElements())
            .length >= 2,
        { timeout: 30_000, timeoutMsg: "the workspace strip was never drawn" },
      );
      for (const tab of await $$(
        '[role="tablist"][aria-label="Workspaces"] [role="tab"]:not(.plane-root)',
      ).getElements()) {
        if ((await tab.$(".workspace-name").getText()) === "alpha") {
          await tab.click();
          return;
        }
      }
      throw new Error("no alpha on the workspace strip");
    }

    /** Waits until the Memory section's text does, or does not, include `words`. */
    async function untilTheSection(words: string, listed = true): Promise<void> {
      await browser.waitUntil(async () => (await $(SECTION).getText()).includes(words) === listed, {
        timeout: 20_000,
        interval: 250,
        timeoutMsg: `the Memory section ${listed ? "never listed" : "still lists"} ${words}`,
      });
    }

    after(async () => {
      if ((await tabNames()).includes(TITLE)) await closeTheTab(TITLE);
    });

    it("lists the focused workspace's journal, makes a memory from its +, opens and deletes it", async () => {
      const plane = (await ask<string[]>("open_planes"))[0];
      await onAlpha();
      await untilTheSection("The API returns 418 on Mondays");

      // Create: the heading's +, a new memory's tab in edit mode, and Save.
      await $(`${SECTION} button[aria-label="New memory in alpha…"]`).click();
      const editor = await $('form[aria-label="Editing workspace/alpha/+"]');
      await editor.waitForDisplayed({ timeout: 20_000 });
      await (await editor.$("input")).setValue(TITLE);
      await (await editor.$("textarea")).setValue(BODY);
      await (await editor.$("button=Save")).click();

      await untilTheSection(TITLE);
      const file = made(plane);
      expect(file).toBeDefined();
      expect(readFileSync(file ?? "", "utf8")).toContain(BODY);

      // Open: the row brings the memory's tab forward, read off the plane.
      await (await $(SECTION).$(`button*=${TITLE}`)).click();
      const body = await $('[data-testid="memory-body"]');
      await body.waitForExist({ timeout: 20_000 });
      await browser.waitUntil(async () => (await body.getText()).includes(BODY), {
        timeout: 20_000,
        timeoutMsg: "the memory's tab never showed its body",
      });
      expect(await inFront()).toBe(TITLE);

      // Delete, and Undo.
      await $(`button*=Delete memory: ${TITLE}`).click();
      const undo = await $('[data-testid="memory-undo"]');
      await undo.waitForExist({ timeout: 20_000 });
      await untilTheSection(TITLE, false);
      expect(made(plane)).toBeUndefined();

      await (await undo.$("button=Undo")).click();

      await untilTheSection(TITLE);
      expect(made(plane)).toBe(file);
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
      // **Let go of through the window, not behind its back.** `close_plane` asked directly
      // leaves the window drawing a project the core no longer holds, in front — and the next
      // spec file shares this app process (it cost `workspace-explorer.e2e.ts` its clones).
      const selector = `${PROJECTS} button[aria-label="Close project views-back"]`;
      const closer = await $(selector);
      if (!(await closer.isExisting())) return;
      await closeProject(selector);
      await browser.waitUntil(async () => !(await ask<string[]>("open_planes")).includes(other), {
        timeout: 20_000,
        timeoutMsg: "the views-back project was not let go of",
      });
      await browser.waitUntil(async () => !(await closer.isExisting()), {
        timeout: 20_000,
        timeoutMsg: "the views-back project's tab stayed on the strip",
      });
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
