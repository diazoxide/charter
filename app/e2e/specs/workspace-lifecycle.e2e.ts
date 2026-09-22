import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { anEmptyRecord, cloneTheFixtureRepos, copyFixturePlane } from "../harness.js";

/**
 * Making a workspace and deleting one, in the built app, against the real core.
 *
 * **What this is for is the DELETE, and one claim about it: the core's guard decides.**
 * `charter workspace remove` refuses over work that removing the workspace would discard —
 * `wscmd::work_at_risk`, which runs inside that command between the name check and
 * `remove_dir_all`. `app/src/WorkspaceLifecycle.test.tsx` pins what the window does with the
 * refusal against a mocked core; nothing there can show that the guard is REALLY in the path,
 * because the mock is what refuses. Here the clone is a real git repository with a real
 * uncommitted file in it, the refusal is the one `wscmd::remove` wrote, and whether the
 * directory is still on disk afterwards is read off the disk.
 *
 * **Its own project, copied into the run's tree** (charter-app#129 and #132's fence): this
 * spec deletes directories, so it may only ever be pointed at a plane the run itself made.
 * The plane the launch opened is never touched, and the project is closed again at the end.
 *
 * **Driven from the palette, not from the context menu.** Both surfaces run the SAME catalogue
 * row — that is the whole design (`app/src/actions.ts`), and `Menus.test.tsx` is where the menu
 * being a view of that catalogue is asserted. What this spec needs is the path from a row to
 * the core, which the palette reaches by keystroke alone; one test below does right-click a
 * real tab, because whether a WebView context menu opens under WebDriver is a measurement this
 * repo has never taken and a claim in a PR is not one.
 */

const PROJECTS = '[role="tablist"][aria-label="Projects"]';
const WORKSPACES = '[role="tablist"][aria-label="Workspaces"]';
const PALETTE = '[role="dialog"][aria-label="Command palette"]';

/** This spec's own project: the fixture plane, with its repos made into real ones. */
const mine = (() => {
  const copied = copyFixturePlane();
  const renamed = join(dirname(copied), "lifecycle");
  renameSync(copied, renamed);
  const root = realpathSync(renamed);
  // Real git repositories, one of which (`tool`) is left with an uncommitted file — which is
  // exactly what `work_at_risk` refuses over, and the reason this fixture already does it.
  cloneTheFixtureRepos(root, "alpha");
  // Nothing to put back, so opening it starts no chat for the next spec to inherit.
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

/** The workspaces the strip is showing, left to right. */
async function stripNames(): Promise<string[]> {
  return browser.execute(
    (selector: string) =>
      [...(document.querySelector(selector)?.querySelectorAll('[role="tab"]') ?? [])].map(
        (tab) => tab.querySelector(".workspace-name")?.textContent ?? "",
      ),
    WORKSPACES,
  );
}

async function stripBecomes(want: string[]): Promise<void> {
  let saw: string[] = [];
  await browser.waitUntil(
    async () => {
      saw = await stripNames();
      return JSON.stringify(saw) === JSON.stringify(want);
    },
    { timeout: 30_000, timeoutMsg: `the workspace strip stayed ${JSON.stringify(saw)}` },
  );
}

/**
 * Sends the `contextmenu` a right-click sends, to the first element matching `selector`.
 *
 * Answers whether it found one, so a selector that stopped matching fails as itself rather
 * than as a menu that never opened. The coordinates are the element's own centre, because
 * Radix anchors the menu to the point the event carries.
 */
async function sendContextMenu(selector: string): Promise<boolean> {
  return browser.execute((css: string) => {
    const el = document.querySelector(css);
    if (!(el instanceof HTMLElement)) return false;
    const box = el.getBoundingClientRect();
    el.dispatchEvent(
      new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        clientX: Math.round(box.left + box.width / 2),
        clientY: Math.round(box.top + box.height / 2),
      }),
    );
    return true;
  }, selector);
}

/**
 * Runs one catalogue row through the palette, with no pointer.
 *
 * `query` and `title` are two arguments because they are two things: what is TYPED, which is
 * ASCII on purpose — `New workspace…` ends in an ellipsis and a run must not depend on a
 * WebDriver sending one — and what the row READS, which is what proves the right row is first.
 */
async function runRow(query: string, title: string): Promise<void> {
  await browser.keys(["F2"]);
  await (await $(PALETTE)).waitForDisplayed({ timeout: 20_000 });
  const box = await $("#palette-query");
  await browser.waitUntil(async () => box.isFocused(), {
    timeout: 10_000,
    timeoutMsg: "the palette did not take the keyboard when it opened",
  });
  // `narrow` ranks charter's own words above any row that merely carries a name, so a verb
  // typed in full comes first — and the wait below is what insists on it rather than assuming.
  await box.addValue(query);
  await browser.waitUntil(
    async () =>
      (await browser.execute(
        () =>
          document.querySelector('[role="option"]')?.querySelector(".palette-title")?.textContent ??
          "",
      )) === title,
    { timeout: 20_000, timeoutMsg: `the palette never put ${title} first` },
  );
  await browser.keys(["Enter"]);
}

describe("making a workspace and deleting one", function () {
  // On the describe, where WebdriverIO reads it: scaffolding a workspace writes a dozen files
  // and a delete runs `git status` in every clone.
  this.timeout(180_000);

  /** The project the launch opened, which this spec never touches. */
  let first = "";

  before(async () => {
    first = (await ask<string[]>("open_planes"))[0];
    // **Through the opener, not through the command.** A window learns it holds a project by
    // opening one; a driver that invoked `open_plane` itself would attach the plane in the
    // core and leave the strip showing the launch's project — and every assertion below would
    // be about the wrong plane. `projects.e2e.ts` opens a second project the same way.
    await $(`${PROJECTS} > button`).click();
    const box = await $("#open-by-path");
    await box.waitForDisplayed({ timeout: 20_000 });
    await box.addValue(mine);
    await $("button=Open").click();
    // A plane nobody has approved is described and not opened; there is no third way in
    // (ADR 0035). This run's store is its own, so this is always a first ask.
    const question = await $('[role="dialog"]');
    await question.waitForDisplayed({ timeout: 30_000 });
    await $("button=Open project").click();
    await browser.waitUntil(async () => (await ask<string[]>("open_planes")).includes(mine), {
      timeout: 30_000,
      timeoutMsg: "this spec's project never opened",
    });
    await stripBecomes(["alpha", "beta"]);
  });

  after(async () => {
    // The window as it was found. Nothing of the launch's project goes, and this spec's own
    // project is let go of — what is left of it on disk is the run's tree to clean up.
    for (const plane of (await ask<string[]>("open_planes")).filter((one) => one !== first)) {
      await ask("close_plane", { plane });
    }
  });

  it("answers the contextmenu a WebView sends with charter's own menu", async () => {
    // **A WebDriver right-click is not a `contextmenu`, and that is measured rather than
    // assumed.** `element.click({ button: "right" })` is a W3C pointer sequence — a
    // pointerDown and a pointerUp with `button: 2` — and the `contextmenu` event is a
    // platform default action the engine raises from a native right-click, below where a
    // synthesised sequence lands. Measured here, in this app, with a listener on the tab:
    //
    //   after a WebDriver right-click:      0 contextmenu events
    //   after a dispatched MouseEvent:      1
    //
    // (webkit 605.1.15 on macOS, run locally 2026-09-22; and run 35771806598 went red on
    // WebKitGTK 605.1.15 too, with the menu never opening on either engine.)
    //
    // **It is the driver and not the product**, which is the half that matters and is
    // measured somewhere a driver cannot reach: `src/Menus.test.tsx`, "a real contextmenu
    // event, with the suppressor live", dispatches one `MouseEvent` at a real workspace tab
    // of the real `App` with `useNoBrowserMenu` mounted and no WebDriver in the picture, and
    // charter's menu opens. Making that suppressor capture-phase — the one way charter could
    // swallow this event — turns that test, and only that test, red. The operator's own
    // report is the third leg: the WebView menu he saw before this work is the default action
    // of the very event the driver will not send.
    //
    // So this spec sends the event the platform sends, and asks charter the question a
    // scenario run can still ask: that the menu is the catalogue's, on the real window, with
    // the real core behind it. `Menus.test.tsx` owns the rows.
    const sent = await sendContextMenu(`${WORKSPACES} [role="tab"]`);
    expect(sent).toBe(true);

    const menu = await $('[role="menu"]');
    await menu.waitForDisplayed({ timeout: 20_000 });
    await expect(menu).toHaveText("New workspace…", { containing: true });
    await expect(menu).toHaveText("Delete workspace", { containing: true });
    await browser.keys(["Escape"]);
  });

  it("makes a workspace with the baseline charter gives it, and puts it on the strip", async () => {
    await runRow("New workspace", "New workspace…");

    const dialog = await $('[role="dialog"]');
    await dialog.waitForDisplayed({ timeout: 20_000 });
    // The first box in the dialog is the name: React owns the id, so the element is taken
    // by its place in a dialog with two fields rather than by an id nothing spells.
    const name = await $('[role="dialog"] input');
    await name.setValue("gamma");
    await $("button=Create workspace").click();

    await stripBecomes(["alpha", "beta", "gamma"]);
    // On disk, with the scaffolding `wscmd::ensure` gives it — not merely a directory. A
    // workspace whose layer is missing is a chat running with none of the plane's ask/deny
    // rules, and it looks exactly like a workspace that is fine.
    for (const rel of ["workspace.json", "workspace.md", "memory/MEMORY.md"]) {
      expect(existsSync(join(mine, "workspaces", "gamma", rel))).toBe(true);
    }
  });

  it("refuses a name the CLI refuses, in the CLI's own sentence, and makes nothing", async () => {
    // The window validates no name of its own: `workspace_create` runs `wscmd::ensure`, which
    // is where `contain::workspace_name_ok` is. `../escape` is what a second answer would get
    // wrong, and it would get it wrong by writing outside the plane.
    await runRow("New workspace", "New workspace…");
    const dialog = await $('[role="dialog"]');
    await dialog.waitForDisplayed({ timeout: 20_000 });
    const name = await $('[role="dialog"] input');
    await name.setValue("../escape");
    await $("button=Create workspace").click();

    const refusal = await $('[role="dialog"] [role="alert"]');
    await refusal.waitForDisplayed({ timeout: 20_000 });
    await expect(refusal).toHaveText("invalid workspace name", { containing: true });
    expect(existsSync(join(dirname(mine), "escape"))).toBe(false);
    expect(existsSync(join(mine, "escape"))).toBe(false);

    await $("button=Cancel").click();
  });

  it("refuses to delete a workspace holding uncommitted work, and deletes nothing", async () => {
    // **The guard, for real.** `workspaces/alpha/tool` is a git repository with an
    // uncommitted file in it, put there by `cloneTheFixtureRepos`. `wscmd::work_at_risk` is
    // what sees that, inside `wscmd::remove`, and the sentence below is the one it wrote.
    await runRow("Delete workspace alpha", "Delete workspace alpha");

    const dialog = await $('[role="alertdialog"]');
    await dialog.waitForDisplayed({ timeout: 20_000 });
    // The preview, which is the same guard read for drawing.
    await expect(dialog).toHaveText("tool: uncommitted changes", { containing: true });

    await $("button=Delete workspace").click();

    const refusal = await $('[role="alertdialog"] [role="alert"]');
    await refusal.waitForDisplayed({ timeout: 30_000 });
    await expect(refusal).toHaveText("this would discard work", { containing: true });
    await expect(refusal).toHaveText("tool: uncommitted changes", { containing: true });
    // Nothing was deleted, and the uncommitted file is still where it was.
    expect(existsSync(join(mine, "workspaces", "alpha", "tool", "scratch.txt"))).toBe(true);
    expect(readFileSync(join(mine, "workspaces", "alpha", "tool", "scratch.txt"), "utf8")).toBe(
      "not committed\n",
    );

    await $("button=Cancel").click();
    await stripBecomes(["alpha", "beta", "gamma"]);
  });

  it("deletes it once the work is committed, with no force anywhere in it", async () => {
    // The repair the refusal named, taken: commit the file. The guard then has nothing to
    // say, and the same button — the one that passes `force: false` — goes through.
    commit(join(mine, "workspaces", "alpha", "tool"));

    await runRow("Delete workspace alpha", "Delete workspace alpha");
    const dialog = await $('[role="alertdialog"]');
    await dialog.waitForDisplayed({ timeout: 20_000 });
    await expect(dialog).toHaveText("no uncommitted or unpushed work", { containing: true });

    await $("button=Delete workspace").click();

    await stripBecomes(["beta", "gamma"]);
    expect(existsSync(join(mine, "workspaces", "alpha"))).toBe(false);
  });

  it("offers a force only after a refusal, and it discards what it named", async () => {
    // A second workspace with work at risk, so the force path is exercised on something this
    // test made rather than on whatever is left of another one.
    await runRow("New workspace", "New workspace…");
    await (await $('[role="dialog"]')).waitForDisplayed({ timeout: 20_000 });
    await (await $('[role="dialog"] input')).setValue("doomed");
    await $("button=Create workspace").click();
    // The strip is the PLANE's own order, which is `read_dir_sorted`'s — so a workspace
    // made now lands where its name puts it and not at the end.
    await stripBecomes(["beta", "doomed", "gamma"]);
    aRepoWithWorkInIt(join(mine, "workspaces", "doomed", "svc"));

    await runRow("Delete workspace doomed", "Delete workspace doomed");
    const dialog = await $('[role="alertdialog"]');
    await dialog.waitForDisplayed({ timeout: 20_000 });
    // **No way to force is on screen yet.** There is nothing to warn about until the refusal
    // exists, and a button offering to discard work before anybody has read a refusal is a
    // destructive action nobody was warned about.
    expect(await (await $('[role="alertdialog"]')).getText()).not.toContain("anyway");

    await $("button=Delete workspace").click();
    await (await $('[role="alertdialog"] [role="alert"]')).waitForDisplayed({ timeout: 30_000 });

    const force = await dialog.$("button*=anyway");
    await force.waitForDisplayed({ timeout: 20_000 });
    // It names what it is about to discard, by the name the guard used.
    await expect(force).toHaveText("svc", { containing: true });
    await force.click();

    await stripBecomes(["beta", "gamma"]);
    expect(existsSync(join(mine, "workspaces", "doomed"))).toBe(false);
  });
});

/**
 * A repository with one commit and one uncommitted file in it — which is exactly what
 * `work_at_risk` refuses over, made where a workspace's clone goes.
 */
function aRepoWithWorkInIt(at: string): void {
  mkdirSync(at, { recursive: true });
  writeFileSync(join(at, "README.md"), "one\n");
  git(at, ["init", "-q", "-b", "main", "."]);
  git(at, ["add", "-A"]);
  git(at, ["commit", "-q", "-m", "one"]);
  writeFileSync(join(at, "wip.txt"), "unsaved\n");
}

/** Commits whatever is uncommitted in a repository, which is the repair a refusal names. */
function commit(at: string): void {
  git(at, ["add", "-A"]);
  git(at, ["commit", "-q", "-m", "the repair the refusal named"]);
}

/** git, with this run's identity and none of the machine's own configuration — `harness.ts`
 *  runs it the same way, and for the same reason. */
function git(cwd: string, args: string[]): void {
  execFileSync(
    "git",
    ["-c", "user.name=charter scenario", "-c", "user.email=scenario@example.invalid", ...args],
    {
      cwd,
      stdio: "pipe",
      env: {
        ...process.env,
        GIT_CONFIG_GLOBAL: "/dev/null",
        GIT_CONFIG_SYSTEM: "/dev/null",
      },
    },
  );
}
