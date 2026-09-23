import { existsSync, realpathSync, renameSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import { THE_RUNS_TREE, anEmptyRecord, copyFixturePlane } from "../harness.js";
import { answerTheAsk, pressAndStart } from "../opening.js";

/**
 * A window holding more than one project, in the built app (ADR 0033, decision 23).
 *
 * **What this is for.** The operator asked for Zed's project tabs by name, and the reason he
 * wanted one window per project before that was the same one: fifty chats in project A must
 * not be torn down because he glanced at project B. So the claim under test is not "a second
 * tab appears" — it is that the first project's sessions are *still the same sessions* after
 * the window has been somewhere else and come back. `running_sessions` is asked before and
 * after, because that is the only answer a tab strip cannot fake.
 *
 * It also pins the half of decision 28 a scenario run can reach. Restoring at a COLD launch
 * needs a second launch, and a run shares one app process — so what is proved here is the
 * other end of that record: the window writes down what it holds, into the real machine
 * store, and `planes_to_restore` reads it back. Whether the next launch opens what comes back
 * is `App`'s, in `src/Projects.test.tsx`; the store's own round trip is `machine.rs`'s.
 *
 * **It leaves the window as it found it.** Specs share one app process and run in name order:
 * the second project is closed at the end, the chat this spec opened is closed with it, and
 * the project the launch opened is never touched.
 */

const PROJECTS = '[role="tablist"][aria-label="Projects"]';
const TABS = '[role="tablist"][aria-label="Tabs"]';

/**
 * A project of this spec's own, under a name of its own, spelled the way charter will.
 *
 * **Renamed**, because `copyFixturePlane` names its copy after the fixture and the launch's
 * project is a copy of the same one: two tabs both reading `daily`, and a `Close project
 * daily` button that could be either. The path tells them apart on screen; the name has to
 * tell them apart for the driver.
 *
 * **Resolved**, because `Planes::open` canonicalises every root it takes and a `PlaneId` IS
 * that spelling — so on macOS, where `tmpdir()` is `/var/folders/…` and `/var` is a link into
 * `/private`, the path this spec would otherwise compare against is not the one the app is
 * holding. That is the same defect `PlaneId`'s own docstring is about, arrived at from a test.
 */
const second = (() => {
  const copied = copyFixturePlane();
  const renamed = join(dirname(copied), "second");
  renameSync(copied, renamed);
  return realpathSync(renamed);
})();

/**
 * Where the strip's *New project…* button is asked to write one — a directory that is not
 * there yet, which is the case that dialog is for.
 *
 * **Inside `THE_RUNS_TREE` and nowhere else.** That tree is `$CHARTER_PLANE_FENCE` for every
 * charter process this run starts, and the app under test is built with the fence compiled in
 * (charter-app#129). A plane scaffolded outside it does not quietly pollute somebody's real
 * control plane; the app dies naming it, in the job that did it — which is the arrangement
 * #132 bought after a run wrote 49 chats into the operator's own plane.
 */
const made = join(THE_RUNS_TREE, "made-by-the-window");

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

/** The project tabs, as the strip shows them: the path each names, and which is in front. */
async function strip(): Promise<{ path: string | null; front: boolean }[]> {
  const tabs = await $$(`${PROJECTS} [role="tab"]`).getElements();
  const rows = [];
  for (const tab of tabs) {
    rows.push({
      // The path, not the name: the tab carries the whole path because two projects can share
      // a directory name, and that is exactly the case this spec sets up.
      path: await tab.getAttribute("title"),
      front: (await tab.getAttribute("aria-selected")) === "true",
    });
  }
  return rows;
}

/** How many chat tabs the project in front is showing. */
async function chatTabs(): Promise<number> {
  return (await $$(`${TABS} [role="tab"]`).getElements()).length;
}

/** Waits for the strip to look like `want`, so nothing here races a click. */
async function stripBecomes(want: { path: string | null; front: boolean }[]): Promise<void> {
  await browser.waitUntil(async () => JSON.stringify(await strip()) === JSON.stringify(want), {
    timeout: 30_000,
    timeoutMsg: `the project strip never became ${JSON.stringify(want)}`,
  });
}

/**
 * Waits until the store holds `want` as the window set, and answers with the whole of it.
 *
 * **Waited for rather than asked once.** The window says what it holds by an `invoke` nothing
 * awaits — it is bookkeeping, not an action — so a read taken the instant a click settles is
 * a read taken before the write. `opener.e2e.ts` waits for the same class of reason.
 */
async function remembers(
  want: string[],
): Promise<{ planes: string[]; active: number | null; dropped: string[] }> {
  let back = { planes: [] as string[], active: null as number | null, dropped: [] as string[] };
  await browser.waitUntil(
    async () => {
      back = await ask("planes_to_restore");
      return JSON.stringify(back.planes) === JSON.stringify(want);
    },
    {
      timeout: 30_000,
      timeoutMsg: `charter never came to remember ${JSON.stringify(want)}`,
    },
  );
  return back;
}

/** Waits until the project in front shows `many` chat tabs. */
async function chatTabsBecome(many: number): Promise<void> {
  let saw = -1;
  await browser.waitUntil(
    async () => {
      saw = await chatTabs();
      return saw === many;
    },
    {
      timeout: 30_000,
      timeoutMsg: `the project in front showed ${saw} chat tabs, not ${many}`,
    },
  );
}

describe("a window holding more than one project", function () {
  // On the describe, where WebdriverIO reads it before the runnable is built — a
  // `this.timeout()` inside a test body is silently ignored (`e2e/budget.test.ts`). Opening a
  // second project reads its settings, its reopen record and this machine's store, and then
  // repaints the whole window; three minutes is mocha's own default and is plenty.
  this.timeout(180_000);

  /** The project the launch opened, which this spec never closes. */
  let first = "";
  /** What it had running before this spec touched anything. */
  let running: number[] = [];
  /** How many chat tabs it was showing then. Read rather than assumed: specs share one app
   *  process and run in name order, so whatever ran before this one decides the number. */
  let chatsBefore = 0;
  /** Whether this spec is the one that opened a chat, and so owes it a close. */
  let mine = false;

  before(async () => {
    // No record of its own, so opening it starts nothing: this spec is about tabs, and a chat
    // appearing in the second project would be a session the next spec inherits.
    anEmptyRecord(second);
    first = (await ask<string[]>("open_planes"))[0];
    // At least one chat in the launch's project, so "the first project kept its sessions" is
    // a claim about something rather than about an empty list.
    if ((await chatTabs()) === 0) {
      await pressAndStart("New tab");
      mine = true;
      await browser.waitUntil(async () => (await chatTabs()) > 0, {
        timeout: 30_000,
        timeoutMsg: "the chat this spec opened never got a tab",
      });
    }
    chatsBefore = await chatTabs();
    running = await ask<number[]>("running_sessions", { plane: first });
    expect(running.length).toBeGreaterThan(0);
  });

  after(async () => {
    // The window as it was found. Nothing of either project goes from disk.
    for (const plane of (await ask<string[]>("open_planes")).filter((one) => one !== first)) {
      await ask("close_plane", { plane });
    }
    if (!mine) return;
    const closer = await $(`${TABS} button[aria-label^="End chat "]`);
    if (!(await closer.isExisting())) return;
    // Ending a chat asks first, and a spec that leaves the question on screen hands the
    // next spec file a modal it never opened — one app process serves the whole run.
    const name = (await closer.getAttribute("aria-label")) ?? "";
    await closer.click();
    await answerTheAsk(name);
  });

  it("opens a second project beside the first, through the same trust gate", async () => {
    await stripBecomes([{ path: first, front: true }]);

    // `+` on the strip, which is the catalogue's own row drawn as a button.
    //
    // **By its accessible name, not by its position.** It was `${PROJECTS} > button`, the one
    // direct-child button of the strip — which stopped matching anything the moment the
    // strip's own controls were gathered into an element of their own, and a selector that
    // matches nothing is a spec that fails with "element not displayed" and says nothing
    // about why. The `+` carries the catalogue's words in `aria-label` precisely so that it
    // can be reached by what it means, and that is what an operator gets from it too.
    await $(`${PROJECTS} button[aria-label="Open a project…"]`).click();
    const box = await $("#open-by-path");
    await box.waitForDisplayed({ timeout: 20_000 });
    await box.addValue(second);
    await $("button=Open").click();

    // **A tab is an open, and an open goes through the gate.** A project nobody has approved
    // is described and not opened; there is no third way in (ADR 0035).
    const question = await $('[role="dialog"]');
    await question.waitForDisplayed({ timeout: 30_000 });
    await expect(question).toHaveText("Open this project?", { containing: true });
    await $("button=Open project").click();

    await stripBecomes([
      { path: first, front: false },
      { path: second, front: true },
    ]);
    // The second project is what is on screen now, and it has nothing open: its record was
    // emptied, so opening it started nothing.
    await chatTabsBecome(0);
  });

  it("leaves the first project's sessions running while the second is in front", async () => {
    // The whole reason the operator wanted one window per project, which tabs have to keep
    // rather than trade away. The tab strip cannot prove it; the core's own list can.
    expect(await ask<number[]>("running_sessions", { plane: first })).toEqual(running);
  });

  it("brings the first project back with its chats, having ended none of them", async () => {
    const tabs = await $$(`${PROJECTS} [role="tab"]`).getElements();
    await tabs[0].click();

    await stripBecomes([
      { path: first, front: true },
      { path: second, front: false },
    ]);
    await chatTabsBecome(chatsBefore);
    expect(await ask<number[]>("running_sessions", { plane: first })).toEqual(running);
  });

  it("writes down what it holds, so a cold launch can put it back", async () => {
    // The other end of decision 28, which is as far as one app process reaches: the window
    // said what it holds, the real machine store took it, and the core reads it back in the
    // order the tabs are in.
    const back = await remembers([first, second]);

    expect(back.active).toBe(0);
    expect(back.dropped).toEqual([]);
  });

  it("closes one project without disturbing the other", async () => {
    await $(`${PROJECTS} button[aria-label="Close project ${basename(second)}"]`).click();

    await stripBecomes([{ path: first, front: true }]);
    expect(await ask<string[]>("open_planes")).toEqual([first]);
    expect(await ask<number[]>("running_sessions", { plane: first })).toEqual(running);
    // And what it writes down follows, so the next launch does not put back a project the
    // operator closed.
    await remembers([first]);
  });

  /**
   * The other button on the strip: one that makes a project instead of finding one
   * (charter-app#178).
   *
   * **Last, and it leaves the strip as it found it**, because every spec above it is about the
   * two projects it set up and this one adds a third to the same window.
   *
   * **It writes for real**, which is the only reason this is a scenario test at all:
   * `NewProject.test.tsx` already proves the dialog, the command it sends and the gate it ends
   * at, all against a mocked core. What no mocked core can show is that the button an operator
   * presses reaches `scaffold::init` and that a directory appears with a plane in it. So the
   * folder is a path that does not exist yet, under `THE_RUNS_TREE` — the one tree this run
   * may write in, and the fence every charter process it starts is held to (charter-app#129,
   * #132). A plane made anywhere else would kill the app, in this job, naming it.
   */
  it("makes a project from the strip's other button, and opens it through the same gate", async () => {
    await $(`${PROJECTS} button[aria-label="New project…"]`).click();

    const dialog = await $('[role="dialog"]');
    await dialog.waitForDisplayed({ timeout: 20_000 });
    await expect(dialog).toHaveText("New project", { containing: true });
    // Typed rather than picked: Browse… opens the system's own folder dialog, which is not a
    // thing a driver inside the webview can answer. The first box in the dialog is the folder
    // — React owns the id, so the element is taken by position, as `workspace-lifecycle.e2e.ts`
    // takes the workspace name out of its sibling dialog.
    await $('[role="dialog"] input').setValue(made);
    await $("button=Create project").click();

    // **A plane charter made a second ago is still one this machine has approved nothing
    // about** (ADR 0035). There is no shortcut past the ask, and this is the assertion that
    // says so from the outside.
    const question = await $('[role="dialog"]');
    await question.waitForDisplayed({ timeout: 30_000 });
    await expect(question).toHaveText("Open this project?", { containing: true });
    await $("button=Open project").click();

    // The operator's second ruling on #178: it opens in THIS window, selected. Pressing create
    // was the asking; a second dialog would be a third confirmation of something asked twice.
    await stripBecomes([
      { path: first, front: false },
      { path: realpathSync(made), front: true },
    ]);
    expect(existsSync(join(made, "charter.toml"))).toBe(true);

    await $(`${PROJECTS} button[aria-label="Close project ${basename(made)}"]`).click();
    await stripBecomes([{ path: first, front: true }]);
  });
});
