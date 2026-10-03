import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { browser, expect, $, $$ } from "@wdio/globals";
import { pressAndStart } from "../opening.js";
import { textOfEach } from "../reading.js";
import { ask } from "../switching.js";

/**
 * The left region — the repo and worktree **explorer** (ADR 0038) — against the real
 * app started in a copy of the `daily` fixture plane, with real clones in it and one real
 * piece cut off `svc`. Nothing here is stubbed: the app reads the files and runs git.
 *
 * This file replaces `sidebar.e2e.ts`. What that spec asserted was the old left sidebar
 * listing every workspace with its vision text, which is the duplication ADR 0038 removed —
 * the strip above is the axis, and the workspace assertions that still matter moved to the
 * strip's own queries below.
 *
 * **Its name is why it runs last, and that is deliberate.** WebdriverIO's Tauri service keeps
 * ONE app process for the whole run, so a spec that starts chats leaves them for every spec
 * after it — and `lifecycle.e2e.ts` counts what a relaunch puts back. `sidebar.e2e.ts` sorted
 * after every other spec in `wdio.conf.ts`'s glob, which is the only reason its two chats were
 * never counted by anything; calling this file `explorer.e2e.ts` sorted it FIRST and turned
 * lifecycle's "2 sessions" into 4, measured on both platforms. The name keeps the position.
 */

/** The workspaces the strip is listing, in order (ADR 0036).
 *
 *  The names alone: a strip tab also says how many chats are in a workspace that is not on
 *  screen, and WebdriverIO's Tauri service keeps ONE app process for the whole run, so a
 *  chat another spec started would otherwise land in this list. */
async function listed(): Promise<string[]> {
  // In one pass: the strip is drawn while it is polled (charter#506, `reading.ts`).
  return textOfEach('[role="tablist"][aria-label="Workspaces"] [role="tab"] .workspace-name');
}

/** Waits until the strip has read the plane, and says what it found if it never does. */
async function untilListed(expected: string[]): Promise<void> {
  let last: string[] = [];
  try {
    await browser.waitUntil(
      async () => {
        last = await listed();
        return last.join(",") === expected.join(",");
      },
      { timeout: 20_000, interval: 250 },
    );
  } catch {
    throw new Error(
      `the strip never listed ${JSON.stringify(expected)}; it listed ${JSON.stringify(last)}`,
    );
  }
}

/**
 * Puts the window on `alpha`, which is the workspace most of the assertions are about.
 *
 * Focused rather than assumed, for the reason in this file's own docstring: one app process
 * serves the whole run, so what this spec finds depends on what ran before it.
 */
async function onAlpha(): Promise<void> {
  await untilListed(["alpha", "beta"]);
  await focus("alpha");
}

/**
 * Focuses a workspace from the strip, which is the axis.
 *
 * By the tab's own `.workspace-name` and not by `button=<name>`: a strip tab carries counts
 * beside its name, the explorer carries a workspace row of its own, and a text match across
 * the window would pick whichever came first.
 */
async function focus(workspace: string): Promise<void> {
  const tabs = await $$(
    // Not the plane root's icon tab (SI-1), which has no drawn name to compare.
    '[role="tablist"][aria-label="Workspaces"] [role="tab"]:not(.plane-root)',
  ).getElements();
  for (const tab of tabs) {
    if ((await tab.$(".workspace-name").getText()) === workspace) {
      await tab.click();
      return;
    }
  }
  throw new Error(`no ${workspace} on the strip; it lists ${(await listed()).join(", ")}`);
}

/**
 * Sends the `contextmenu` a right-click sends, to the first element matching `selector`.
 *
 * Answers whether it found one, so a selector that stopped matching fails as itself rather
 * than as a menu that never opened. The coordinates are the element's own centre, because
 * Radix anchors the menu to the point the event carries. The same helper
 * `workspace-lifecycle.e2e.ts` uses, and its docstring has the measurement behind it.
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

describe("the explorer", () => {
  it("lists the focused workspace's clones", async () => {
    await onAlpha();

    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });
    await expect(await $('[data-testid="clone-tool"]')).toBeExisting();
  });

  it("lists the worktrees cut off a clone, with the branch each is on", async () => {
    await onAlpha();

    const cut = await $('[data-testid="piece-svc-fix-login"]');
    await cut.waitForExist({ timeout: 20_000 });
    await expect(cut).toHaveText(expect.stringContaining("fix-login"));
  });

  it("says a worktree carries no charter layer before a chat is started in it", async () => {
    // The fixture's piece is cut with plain git, which is exactly the tree a chat would run
    // in with none of the plane's ask/deny rules, no persona agents and no $CHARTER_HARNESS.
    await onAlpha();

    const cut = await $('[data-testid="piece-svc-fix-login"]');
    await cut.waitForExist({ timeout: 20_000 });
    await expect(cut).toHaveText(expect.stringContaining("unwired"));
  });

  it("answers a right-click on a piece with the catalogue's rows for that piece", async () => {
    // charter-app#174, against the real core: the rows in this menu exist because the plane
    // on disk has `fix-login` cut off `svc` and `worktree_list` said so — nothing here is
    // stubbed, and a catalogue built from a plane that had not been read would draw no menu
    // at all. What the rows SAY is `src/Menus.test.tsx`'s and `src/actions.test.ts`'s.
    //
    // **The event is dispatched rather than right-clicked**, for the reason
    // `workspace-lifecycle.e2e.ts` measures at length: a WebDriver right-click is a
    // synthesised pointer sequence and raises no `contextmenu` on either engine, so what is
    // sent here is the event the platform would have sent.
    await onAlpha();
    await $('[data-testid="piece-svc-fix-login"]').waitForExist({ timeout: 20_000 });

    const sent = await sendContextMenu('[data-testid="piece-svc-fix-login"] .spot');
    expect(sent).toBe(true);

    const menu = await $('[role="menu"]');
    await menu.waitForDisplayed({ timeout: 20_000 });
    await expect(menu).toHaveText("Merge branch fix-login into svc", { containing: true });
    await expect(menu).toHaveText("Remove folder fix-login in svc", {
      containing: true,
    });
    // Closed again, because one app process serves the whole run and a menu left up is over
    // every row the specs after this one reach for.
    await browser.keys(["Escape"]);
    await menu.waitForDisplayed({ timeout: 20_000, reverse: true });
  });

  it("says a clone has no branches cut rather than drawing nothing under it", async () => {
    await onAlpha();

    const tool = await $('[data-testid="clone-tool"]');
    await browser.waitUntil(async () => (await tool.getText()).includes("No branches cut"), {
      timeout: 20_000,
      timeoutMsg: "the explorer never said whether `tool` has branches cut",
    });
  });

  /**
   * **A branch expands into its files** (FM-1, #1104), against the real core: `branch_tree`
   * lists the fixture branch's folder with git, and the file opens in its file tab through
   * `piece_file`. Nothing is stubbed.
   */
  it("expands the fixture branch into its files and opens one in its file tab", async () => {
    await onAlpha();
    const files = await $('[data-testid="files-svc-fix-login"] .file-node');
    await files.waitForExist({ timeout: 20_000 });

    await files.click();

    await expect(files).toHaveAttribute("aria-expanded", "true");
    const readme = await fileRow("fix-login", "README.md");
    await readme.click();

    await browser.waitUntil(async () => (await tabInFront()) === "README.md · fix-login", {
      timeout: 20_000,
      timeoutMsg: `the file tab never came forward; in front is ${await tabInFront()}`,
    });
    // Markdown, so it is drawn rendered (FM-2); `# svc` is its heading.
    const rendered = await $('[data-testid="piece-markdown"]');
    await rendered.waitForDisplayed({ timeout: 20_000 });
    await expect(rendered).toHaveText(expect.stringContaining("svc"));
  });

  /**
   * **⌘P finds a file by fuzzy name** (FM-7, #1110), against the real core: `find_files` lists
   * the picked branch with git and ranks its paths, and the file opens in its file tab. The
   * key is sent as the window's own event, with the platform's modifiers, because a WebDriver
   * chord does not carry them reliably on both engines (#176).
   */
  it("finds a file of the picked branch by fuzzy name on ⌘P and opens it in its file tab", async () => {
    await onAlpha();
    const plane = (await ask<string[]>("open_planes"))[0];
    const branch = join(plane, "workspaces", "alpha", ".worktrees", "svc", "fix-login");
    // In a folder of this test's own, which it removes again: the fixture's own files stay.
    mkdirSync(join(branch, "found-by-name", "auth"), { recursive: true });
    writeFileSync(
      join(branch, "found-by-name", "auth", "login-form.ts"),
      "export const form = 1;\n",
    );
    // **It leaves the window as it found it**, whatever happens in between: one app process
    // serves the whole run, and a branch left picked here is where the next spec's "New tab"
    // starts its chat — `files a chat under the workspace it was started in` needs nothing
    // picked, and a palette left up would be a dialog over every row after this.
    try {
      const spot = await $('[data-testid="piece-svc-fix-login"] .spot');
      await spot.waitForExist({ timeout: 20_000 });
      await spot.click();

      await browser.execute(() => {
        const mac = navigator.platform.startsWith("Mac");
        window.dispatchEvent(
          new KeyboardEvent("keydown", {
            key: "p",
            metaKey: mac,
            ctrlKey: !mac,
            shiftKey: !mac,
            bubbles: true,
            cancelable: true,
          }),
        );
      });
      const scope = await $('[role="status"][aria-label="Where files are found"]');
      await scope.waitForDisplayed({ timeout: 20_000 });
      await expect(scope).toHaveText(expect.stringContaining("Files in branch fix-login"));
      await browser.keys("lgnform");
      const hit = await $('[role="listbox"][aria-label="Files"] [role="option"]');
      await hit.waitForExist({ timeout: 20_000 });
      await expect(hit).toHaveText(expect.stringContaining("login-form.ts"));
      await expect(hit).toHaveText(expect.stringContaining("found-by-name/auth · fix-login"));

      // Tab widens the scope to the project and says so, and the file still leads.
      // The rung past it, every open project, is there only while another project is open,
      // which depends on the specs before this one, so it is `Palette.files.test.tsx`'s.
      await browser.keys(["Tab"]);
      await expect(scope).toHaveText(expect.stringContaining("Files in project"));
      await expect($('[role="listbox"][aria-label="Files"] [role="option"]')).toHaveText(
        expect.stringContaining("found-by-name/auth · fix-login"),
      );

      await browser.keys(["Enter"]);

      await browser.waitUntil(async () => (await tabInFront()) === "login-form.ts · fix-login", {
        timeout: 20_000,
        timeoutMsg: `the file tab never came forward; in front is ${await tabInFront()}`,
      });
      const editor = await $('[data-testid="light-editor"]');
      await editor.waitForDisplayed({ timeout: 20_000 });
      await expect(editor).toHaveText(expect.stringContaining("export const form"));

      // The palette closed on Enter.
      await expect($('[role="dialog"]')).not.toBeDisplayed();
    } finally {
      if (await $('[role="dialog"]').isDisplayed()) await browser.keys(["Escape"]);
      const root = await $('[data-testid="explorer"] button.spot-root');
      await root.click();
      await expect(root).toHaveAttribute("aria-current", "true");
      rmSync(join(branch, "found-by-name"), { recursive: true, force: true });
    }
  });

  /**
   * **⌘⇧F searches the content of the files and opens a hit at its line** (FM-8, #1111), against
   * the real core: `search_files` walks the picked branch in-process and streams its hits, and
   * Enter on the one stepped to opens the branch's file tab with the preview on that line. The
   * key is sent as the window's own event, with the platform's modifiers, for ⌘P's reason
   * (#176).
   */
  it("finds a fixture string on ⌘⇧F and opens the hit at its line in the file tab", async () => {
    // One app serves the whole run: a palette a spec before this one left open is closed first.
    if (await $('[role="dialog"]').isExisting()) await browser.keys(["Escape"]);
    await onAlpha();
    const plane = (await ask<string[]>("open_planes"))[0];
    const branch = join(plane, "workspaces", "alpha", ".worktrees", "svc", "fix-login");
    // In a folder of this test's own, which it removes again: the fixture's own files stay.
    mkdirSync(join(branch, "found-by-content"), { recursive: true });
    writeFileSync(
      join(branch, "found-by-content", "notes.md"),
      "# Notes\n\nnothing here\n\nthe quokka-sentinel lives on line five\n",
    );
    // **It leaves the window as it found it**, for ⌘P's reason above: nothing picked in the
    // explorer, and no dialog up.
    try {
      const spot = await $('[data-testid="piece-svc-fix-login"] .spot');
      await spot.waitForExist({ timeout: 20_000 });
      await spot.click();

      await browser.execute(() => {
        const mac = navigator.platform.startsWith("Mac");
        window.dispatchEvent(
          new KeyboardEvent("keydown", {
            key: "F",
            metaKey: mac,
            ctrlKey: !mac,
            shiftKey: true,
            bubbles: true,
            cancelable: true,
          }),
        );
      });
      await browser.waitUntil(async () => (await tabInFront()) === "Search", {
        timeout: 20_000,
        timeoutMsg: `no Search tab came forward; in front is ${await tabInFront()}`,
      });
      const box = await $('[role="searchbox"][aria-label="Search the files"]');
      await box.waitForDisplayed({ timeout: 20_000 });
      await expect($('[role="combobox"][aria-label="Where to search"]')).toHaveValue("branch");
      await box.click();
      await browser.keys("quokka-sentinel");

      const hit = await $('[role="listbox"][aria-label="Search results"] [role="option"]');
      await hit.waitForExist({ timeout: 20_000 });
      await expect(hit).toHaveText(expect.stringContaining("the quokka-sentinel lives"));
      await expect(
        $('[role="listbox"][aria-label="Search results"] [role="group"]'),
      ).toHaveAttribute("aria-label", expect.stringContaining("notes.md, found-by-content"));
      await browser.waitUntil(async () => (await tabInFront()) === "Search · quokka-sentinel", {
        timeout: 20_000,
        timeoutMsg: `the tab is called ${await tabInFront()}`,
      });

      // Into the hits from the box, and the one stepped to opens.
      await browser.keys(["ArrowDown"]);
      await browser.keys(["Enter"]);

      await browser.waitUntil(async () => (await tabInFront()) === "Files · fix-login", {
        timeout: 20_000,
        timeoutMsg: `the file tab never came forward; in front is ${await tabInFront()}`,
      });
      const editor = await $('[data-testid="light-editor"]');
      await editor.waitForDisplayed({ timeout: 20_000 });
      await expect(editor).toHaveText(expect.stringContaining("quokka-sentinel"));
      // The cursor is on the hit's line, which the gutter marks.
      const marked = await $('[data-testid="light-editor"] .cm-activeLineGutter');
      await expect(marked).toHaveText("5");
    } finally {
      if (await $('[role="dialog"]').isDisplayed()) await browser.keys(["Escape"]);
      const root = await $('[data-testid="explorer"] button.spot-root');
      await root.click();
      await expect(root).toHaveAttribute("aria-current", "true");
      rmSync(join(branch, "found-by-content"), { recursive: true, force: true });
    }
  });

  it("shows a file an agent creates in an expanded folder without a refresh", async () => {
    await onAlpha();
    const files = await $('[data-testid="files-svc-fix-login"] .file-node');
    await files.waitForExist({ timeout: 20_000 });
    if ((await files.getAttribute("aria-expanded")) !== "true") await files.click();
    await fileRow("fix-login", "README.md");

    // What an agent writing in the branch does: a file appears in its folder on disk.
    const plane = (await ask<string[]>("open_planes"))[0];
    writeFileSync(
      join(plane, "workspaces", "alpha", ".worktrees", "svc", "fix-login", "agent-wrote.md"),
      "written by an agent\n",
    );

    // FSEvents gives no bound on when it delivers on a busy Mac (#577), so the wait is long.
    await fileRow("fix-login", "agent-wrote.md", 60_000);

    // And one it removes goes.
    rmSync(join(plane, "workspaces", "alpha", ".worktrees", "svc", "fix-login", "agent-wrote.md"));
    await $('[data-row="file:svc/fix-login:agent-wrote.md"]').waitForExist({
      timeout: 60_000,
      reverse: true,
      timeoutMsg: "the explorer still draws a file the agent removed",
    });
  });

  it("shows a file an agent creates in an expanded folder inside the branch", async () => {
    await onAlpha();
    const plane = (await ask<string[]>("open_planes"))[0];
    const branch = join(plane, "workspaces", "alpha", ".worktrees", "svc", "fix-login");
    mkdirSync(join(branch, "notes"), { recursive: true });
    writeFileSync(join(branch, "notes", "first.md"), "one\n");
    const files = await $('[data-testid="files-svc-fix-login"] .file-node');
    await files.waitForExist({ timeout: 20_000 });
    if ((await files.getAttribute("aria-expanded")) !== "true") await files.click();
    const notes = await fileRow("fix-login", "notes", 60_000);
    await notes.click();
    await fileRow("fix-login", "notes/first.md");

    writeFileSync(join(branch, "notes", "second.md"), "two\n");

    await fileRow("fix-login", "notes/second.md", 60_000);
  });

  /**
   * **What a branch changed, marked as an agent writes** (FM-4, #1107), against the real core:
   * `branch_status` runs git in the fixture branch, and the folder watch hears the write.
   */
  it("marks a file an agent adds or changes without a refresh", async () => {
    await onAlpha();
    const plane = (await ask<string[]>("open_planes"))[0];
    const branch = join(plane, "workspaces", "alpha", ".worktrees", "svc", "fix-login");
    const files = await $('[data-testid="files-svc-fix-login"] .file-node');
    await files.waitForExist({ timeout: 20_000 });
    if ((await files.getAttribute("aria-expanded")) !== "true") await files.click();
    await explorerRow("fix-login", "README.md");
    const readme = readFileSync(join(branch, "README.md"), "utf8");

    try {
      writeFileSync(join(branch, "agent-marked.md"), "written by an agent\n");
      writeFileSync(join(branch, "README.md"), `${readme}changed by an agent\n`);

      await marked("fix-login", "agent-marked.md", "added");
      await marked("fix-login", "README.md", "changed");
    } finally {
      // One app process serves the whole run: the branch is left as it was found.
      writeFileSync(join(branch, "README.md"), readme);
      rmSync(join(branch, "agent-marked.md"), { force: true });
    }
    await $(inExplorer('li[data-mark] > [data-row="file:svc/fix-login:README.md"]')).waitForExist({
      timeout: 60_000,
      reverse: true,
      timeoutMsg: "README.md is still marked after the agent put it back",
    });
  });

  it("marks an agent's first write into a folder nobody opened", async () => {
    await onAlpha();
    const plane = (await ask<string[]>("open_planes"))[0];
    const branch = join(plane, "workspaces", "alpha", ".worktrees", "svc", "fix-login");
    const files = await $('[data-testid="files-svc-fix-login"] .file-node');
    await files.waitForExist({ timeout: 20_000 });
    if ((await files.getAttribute("aria-expanded")) !== "true") await files.click();
    // `.claude` holds a file git tracks, is closed, and the branch has changed nothing in it.
    const folder = await explorerRow("fix-login", ".claude");
    await expect(folder).toHaveAttribute("aria-expanded", "false");
    await $(inExplorer('li[data-mark] > [data-row="file:svc/fix-login:.claude"]')).waitForExist({
      timeout: 20_000,
      reverse: true,
      timeoutMsg: ".claude is marked before anything was written in it",
    });

    try {
      writeFileSync(join(branch, ".claude", "agent-first.md"), "the agent's first write here\n");

      await marked("fix-login", ".claude", "added");
    } finally {
      rmSync(join(branch, ".claude", "agent-first.md"), { force: true });
    }
  });

  it("collapses to what the branch changed, and filters the tree by name until Esc", async () => {
    await onAlpha();
    const plane = (await ask<string[]>("open_planes"))[0];
    const branch = join(plane, "workspaces", "alpha", ".worktrees", "svc", "fix-login");
    mkdirSync(join(branch, "changed-dir"), { recursive: true });
    writeFileSync(join(branch, "changed-dir", "deep.md"), "changed\n");
    const files = await $('[data-testid="files-svc-fix-login"] .file-node');
    await files.waitForExist({ timeout: 20_000 });
    if ((await files.getAttribute("aria-expanded")) !== "true") await files.click();
    await explorerRow("fix-login", "README.md");
    // By role and name: a toggle button, and the search box beside the tree.
    const changedOnly = await $("aria/Changed only");
    await expect(changedOnly).toHaveAttribute("aria-pressed", "false");

    try {
      await changedOnly.click();

      // Every folder of it open, with nothing clicked; and what the branch did not change gone.
      await explorerRow("fix-login", "changed-dir/deep.md", 60_000);
      await $(inExplorer('[data-row="file:svc/fix-login:README.md"]')).waitForExist({
        timeout: 20_000,
        reverse: true,
        timeoutMsg: "Changed only still draws README.md, which the branch did not change",
      });
      await changedOnly.click();
      await explorerRow("fix-login", "README.md");

      const filter = await $("aria/Filter files");
      await expect(filter).toHaveAttribute("type", "search");
      await filter.setValue("readme");
      await $(inExplorer('[data-row="file:svc/fix-login:changed-dir"]')).waitForExist({
        timeout: 20_000,
        reverse: true,
        timeoutMsg: "the filter still draws a folder whose name and rows do not match",
      });
      await expect(await explorerRow("fix-login", "README.md")).toBeExisting();

      await browser.keys(["Escape"]);

      await expect(filter).toHaveValue("");
      await explorerRow("fix-login", "changed-dir");
    } finally {
      if ((await changedOnly.getAttribute("aria-pressed")) === "true") await changedOnly.click();
      rmSync(join(branch, "changed-dir"), { recursive: true, force: true });
    }
  });

  it("expands a repo into the repo's own files", async () => {
    await onAlpha();
    const files = await $('[data-testid="files-tool"] .file-node');
    await files.waitForExist({ timeout: 20_000 });

    await files.click();

    // `scratch.txt` is in the clone and not committed: the repo's own folder, as it is.
    await expect(await fileRow(null, "scratch.txt", 20_000, "tool")).toBeExisting();
  });

  /**
   * **The file tab is the branch's tree beside a resizable preview** (FM-2, #1105), against the
   * real core: the tree is `branch_tree`'s, the preview `piece_file`'s, and the divider's place
   * is written to the plane's record the next launch reads. Nothing is stubbed.
   */
  it("opens the fixture branch's file tab as a tree, drags its divider and keeps it", async () => {
    await onAlpha();
    await browseTheFiles();
    const tree = await $('[data-testid="piece-files-tree"]');
    await tree.waitForExist({ timeout: 20_000 });

    // A file opens in the preview beside the tree.
    const readme = await tree.$('[data-row="file:svc/fix-login:README.md"]');
    await readme.waitForExist({ timeout: 20_000 });
    await readme.click();
    const preview = await $('[aria-label="README.md"]');
    await browser.waitUntil(async () => (await preview.getText()).includes("svc"), {
      timeout: 20_000,
      timeoutMsg: "the preview never drew README.md",
    });
    await expect(readme).toHaveAttribute("aria-selected", "true");

    // The divider is dragged with the pointer…
    const divider = await $('[role="separator"][aria-label="Resize the file tree"]').getElement();
    await divider.waitForDisplayed({ timeout: 20_000 });
    const before = await valueOf(divider);
    expect(await dragTheDivider(120)).toBe(true);
    await browser.waitUntil(async () => (await valueOf(divider)) > before, {
      timeout: 20_000,
      timeoutMsg: `dragging the divider never moved it from ${before}%`,
    });
    // …and moved from the keyboard, saying where it is.
    const dragged = await valueOf(divider);
    await divider.click();
    await browser.keys(["ArrowLeft"]);
    await browser.waitUntil(async () => (await valueOf(divider)) < dragged, {
      timeout: 20_000,
      timeoutMsg: "the arrow key never moved the divider",
    });
    const left = await valueOf(divider);

    // The record the next launch reads holds the tab and where its divider was.
    const record = join((await ask<string[]>("open_planes"))[0], ".charter", "app", "reopen.json");
    await browser.waitUntil(
      async () =>
        filesTabsIn(record).some(
          (one) => one.key === "alpha/svc/fix-login" && Math.abs((one.split ?? 0) - left) <= 1,
        ),
      { timeout: 20_000, timeoutMsg: "the record never kept the file tab's divider" },
    );

    // Opening the same branch's files again keeps one tab…
    await browseTheFiles();
    expect(filesTabsIn(record).filter((one) => one.key === "alpha/svc/fix-login")).toHaveLength(1);
    expect((await tabNames()).filter((name) => name === "Files · fix-login")).toHaveLength(1);

    // …and closing it and opening it again brings the divider back where it was.
    await $(
      '[role="tablist"][aria-label="Tabs"] button[aria-label="Close Files · fix-login"]',
    ).click();
    await browser.waitUntil(async () => !(await tabNames()).includes("Files · fix-login"), {
      timeout: 20_000,
      timeoutMsg: "the file tab did not close",
    });
    await browseTheFiles();
    const again = await $('[role="separator"][aria-label="Resize the file tree"]').getElement();
    await again.waitForDisplayed({ timeout: 20_000 });
    await browser.waitUntil(async () => Math.abs((await valueOf(again)) - left) <= 1, {
      timeout: 20_000,
      timeoutMsg: `the divider came back at ${await valueOf(again)}%, not the ${left}% it was left at`,
    });

    // Put away, so the specs after this one find the explorer's rows and no tab's.
    await $(
      '[role="tablist"][aria-label="Tabs"] button[aria-label="Close Files · fix-login"]',
    ).click();
    await browser.waitUntil(async () => !(await tabNames()).includes("Files · fix-login"), {
      timeout: 20_000,
      timeoutMsg: "the file tab did not close",
    });
  });

  it("does not list every workspace, because the strip above already answers that", async () => {
    // ADR 0038, and the reason this region was rewritten: the old sidebar drew every
    // workspace with its vision text under the strip that had just been made the axis.
    await onAlpha();
    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });

    const explorer = await $('[data-testid="explorer"]');
    await expect(explorer).not.toHaveText(expect.stringContaining("Retire the old importer"));
    await expect(explorer).not.toHaveText(expect.stringContaining("Ship the widget"));
  });

  it("has one thing called Workspaces in the window, and it is the strip", async () => {
    // Two `[aria-label="Workspaces"]` broke a spec when the second appeared. The old left
    // sidebar was that second one.
    await untilListed(["alpha", "beta"]);

    const named = await $$('[aria-label="Workspaces"]').getElements();
    expect(named.length).toBe(1);
    await expect(named[0]).toHaveAttribute("role", "tablist");
  });

  it("follows the focus to another workspace", async () => {
    await onAlpha();

    await focus("beta");

    // `beta` holds no repos at all.
    const explorer = await $('[data-testid="explorer"]');
    await browser.waitUntil(async () => (await explorer.getText()).includes("No repos"), {
      timeout: 20_000,
      timeoutMsg: "the explorer never followed the focus to beta",
    });

    await focus("alpha");
    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });
  });

  it("files a chat under the workspace it was started in", async () => {
    await onAlpha();
    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });

    await pressAndStart("New tab");

    // Nothing in the explorer is picked, so the chat starts in the workspace's own directory
    // and the explorer lists it there — under the workspace row, not under a piece.
    const explorer = await $('[data-testid="explorer"]');
    await browser.waitUntil(async () => (await explorer.getText()).includes("1"), {
      timeout: 20_000,
      timeoutMsg: "the chat never appeared under the workspace it was started in",
    });
  });

  it("shows one workspace's chats on the strip, and keeps the others running", async () => {
    // The axis the tmux frame had and the port lost (ADR 0036): the chat strip shows
    // the focused workspace's chats — and a glance at another workspace ends nothing, which
    // is the same guarantee a project behind another one has (#125).
    await untilListed(["alpha", "beta"]);
    await focus("beta");
    await pressAndStart("New tab");

    const started = await tabInFront();
    expect(started).not.toBe("");

    await focus("alpha");

    expect(await chatTabs()).not.toContain(started);

    await focus("beta");

    // Still there, still running: nothing was torn down by looking away.
    await browser.waitUntil(async () => (await chatTabs()).includes(started), {
      timeout: 20_000,
      timeoutMsg: `the chat ${started} did not come back when its workspace was focused again`,
    });
  });

  /**
   * **A shell tab in a folder of a branch, from the folder's menu** (FM-10, #1113), against the
   * real core, and last, so the shell it leaves running is counted by no scenario here:
   * `open_shell_in_branch` resolves the folder inside the branch and starts the operator's shell
   * there. The window names the branch and the folder, never a directory, and the chat the core
   * opened works in that folder. The event is dispatched, for the piece menu's reason above.
   */
  it("opens a shell tab in a folder of the branch from the folder's menu", async () => {
    await onAlpha();
    const plane = (await ask<string[]>("open_planes"))[0];
    const branch = join(plane, "workspaces", "alpha", ".worktrees", "svc", "fix-login");
    mkdirSync(join(branch, "shell-here"), { recursive: true });
    writeFileSync(join(branch, "shell-here", "keep.md"), "kept\n");
    const files = await $('[data-testid="files-svc-fix-login"] .file-node');
    await files.waitForExist({ timeout: 20_000 });
    if ((await files.getAttribute("aria-expanded")) !== "true") await files.click();
    await fileRow("fix-login", "shell-here", 60_000);

    const sent = await sendContextMenu('[data-row="file:svc/fix-login:shell-here"]');
    expect(sent).toBe(true);
    const menu = await $('[role="menu"]');
    await menu.waitForDisplayed({ timeout: 20_000 });
    await expect(menu).toHaveText("Copy absolute path", { containing: true });
    await menu.$('[role="menuitem"][aria-label="Open a shell tab here"]').click();
    await menu.waitForDisplayed({ timeout: 20_000, reverse: true });

    await browser.waitUntil(async () => (await tabInFront()).startsWith("shell"), {
      timeout: 20_000,
      timeoutMsg: `no shell tab came forward; in front is ${await tabInFront()}`,
    });
    const chats = await ask<{ cwd: string | null }[]>("opened_chats", { plane });
    expect(
      chats.some((one) => one.cwd?.split("\\").join("/").endsWith("/fix-login/shell-here")),
    ).toBe(true);
  });
});

/**
 * **The tree's rows, measured — because jsdom lays nothing out.**
 *
 * Every assertion in this block is about a used value: a height in pixels, a `scrollWidth`, the
 * position an `::after` was actually painted at. `getComputedStyle` in jsdom answers from the
 * cascade it managed to build and gives every box a size of zero, so a vitest assertion that a
 * row "does not wrap" or that the region "scrolls" checks nothing at all. These need a window
 * that really lays out, which is why they are here.
 *
 * They also catch the trap `docs/design-system.md` names: a class that does not exist emits no
 * CSS and nothing goes red. A rule that never took would show up here as a row two lines tall.
 */
describe("the explorer's rows, in a region too narrow for them", () => {
  /**
   * Narrows the explorer, measures, and puts it back.
   *
   * **The region's own box is narrowed, not the separator dragged.** The question is whether
   * this scroll container folds its rows or scrolls them when it is narrower than they are, and
   * the container is `.explorer` itself — so its width is the input, whatever produced it. A
   * pointer gesture on the separator would ask the same question through a drag whose pixels
   * differ per platform, and `react-resizable-panels`' own inline style is a shape this spec
   * would then be pinning. Everything is read back inside one script, before React can render
   * again and take the style away.
   */
  async function narrowed(): Promise<{
    scrollWidth: number;
    clientWidth: number;
    scrolledTo: number;
    rows: { what: string; height: number; limit: number; overflow: number; wraps: string }[];
  }> {
    return browser.execute(() => {
      const explorer = document.querySelector<HTMLElement>('[data-testid="explorer"]');
      if (!explorer) throw new Error("no explorer to narrow");
      const was = explorer.getAttribute("style") ?? "";
      explorer.style.width = "120px";

      /** What one line of THIS row would measure, from its own font and its own padding.
       *
       *  The slack is half a line, which is the gap between the two answers rather than a
       *  guess: a row that wrapped is a WHOLE line taller, and a row that did not can still
       *  be a few pixels over its own line box because a chip inside it carries a border and
       *  its own padding. Measuring to the pixel would make this a test of `.label`'s border
       *  width; every row still clears this limit by more than three pixels on both platforms,
       *  and every wrapped one exceeds it by more than three. */
      const oneLine = (el: Element, what: string) => {
        const css = getComputedStyle(el);
        const line = Number.parseFloat(css.lineHeight);
        const height = Number.isFinite(line) ? line : Number.parseFloat(css.fontSize) * 1.5;
        const padding = Number.parseFloat(css.paddingTop) + Number.parseFloat(css.paddingBottom);
        const border =
          Number.parseFloat(css.borderTopWidth) + Number.parseFloat(css.borderBottomWidth);
        return {
          what,
          height: el.getBoundingClientRect().height,
          limit: height * 1.5 + padding + border + 4,
          // **What `min-width: max-content` is for, and the only thing that holds it.** A row
          // told only `white-space: nowrap` also stays on one line — its text simply overflows
          // its own box — and then the band behind a hovered or current row stops at the
          // region's edge while the name runs on past it. Zero here is the row's box having
          // grown to hold its content.
          overflow: el.scrollWidth - el.clientWidth,
          wraps: css.whiteSpace,
        };
      };

      const rows = [
        ...[...explorer.querySelectorAll(".spot")].map((el) => oneLine(el, "a spot")),
        ...[...explorer.querySelectorAll(".clone > summary")].map((el) => oneLine(el, "a clone")),
        ...[...explorer.querySelectorAll(".chat")].map((el) => oneLine(el, "a chat")),
        ...[...explorer.querySelectorAll(".worktree")].map((el) => oneLine(el, "a worktree")),
      ];
      const measured = {
        scrollWidth: explorer.scrollWidth,
        clientWidth: explorer.clientWidth,
        scrolledTo: 0,
        rows,
      };
      // It really scrolls, rather than merely having something to scroll: a region whose
      // overflow is hidden clips the row and answers 0 here.
      explorer.scrollLeft = 10_000;
      measured.scrolledTo = explorer.scrollLeft;
      explorer.scrollLeft = 0;

      explorer.setAttribute("style", was);
      return measured;
    });
  }

  it("keeps every row on one line, and scrolls sideways instead of folding it", async () => {
    // The operator's own report, and his screenshot: `ai-assistant` and `the workspace itself`
    // on two lines in a narrow explorer.
    await onAlpha();
    await $('[data-testid="clone-svc"]').waitForExist({ timeout: 20_000 });

    const measured = await narrowed();

    expect(measured.rows.length).toBeGreaterThan(3);
    // Reported as a list rather than asserted one at a time, so a failure names every row that
    // folded and what it measured instead of stopping at the first.
    const wrapped = measured.rows
      .filter((row) => row.height > row.limit)
      .map((row) => `${row.what}: ${row.height}px, and one line of it is ${row.limit}px`);
    expect(wrapped).toEqual([]);
    // A row's own box holds its own name, so the band behind a hovered or current row reaches
    // the end of it rather than stopping at the region's edge.
    const clipped = measured.rows
      .filter((row) => row.overflow > 1)
      .map((row) => `${row.what}: ${row.overflow}px of it is outside its own box`);
    expect(clipped).toEqual([]);
    // And the rule that says a name is never broken, as the engine resolved it. A rule that
    // emitted no CSS at all — the trap `docs/design-system.md` names — reads `normal` here.
    expect([...new Set(measured.rows.map((row) => row.wraps))]).toEqual(["nowrap"]);
    // Nothing to scroll means the rows were folded to fit instead.
    expect(measured.scrollWidth).toBeGreaterThan(measured.clientWidth);
    // And it really scrolls: a region whose overflow is hidden answers 0 here.
    expect(measured.scrolledTo).toBeGreaterThan(0);
  });

  /**
   * **The tree's elbows, against the line they are drawn for.**
   *
   * #154 draws them per row at a fixed offset (`0.9em` for a clone and a piece, `0.72em` for a
   * chat) tuned to the padding and the font size of each of those rows, and its author wrote
   * that *"any padding change misaligns them, and no test would notice"*. This is the test that
   * notices: the elbow's painted position against the middle of the name it points at, read off
   * the real WebView. A row's padding changed by 0.2rem moves one and not the other.
   */
  it("draws each elbow at the middle of the line it points at", async () => {
    await onAlpha();
    await $('[data-testid="piece-svc-fix-login"]').waitForExist({ timeout: 20_000 });

    const elbows = await browser.execute(() => {
      const explorer = document.querySelector<HTMLElement>('[data-testid="explorer"]');
      if (!explorer) throw new Error("no explorer");

      /** Where the row's `::after` — its elbow — was actually painted. */
      const elbowOf = (li: Element) => {
        const css = getComputedStyle(li, "::after");
        // The rule is written in logical properties; a computed style answers in whichever of
        // the two this engine resolves, so both are asked and the first number wins.
        for (const value of [css.insetBlockStart, css.top]) {
          const at = Number.parseFloat(value);
          if (Number.isFinite(at)) return li.getBoundingClientRect().top + at;
        }
        return Number.NaN;
      };

      const measure = (selector: string, name: string, what: string) =>
        [...explorer.querySelectorAll(selector)].flatMap((li) => {
          const label = li.querySelector(name);
          if (!label) return [];
          const box = label.getBoundingClientRect();
          return [
            {
              what: `${what} (${label.textContent ?? ""})`,
              elbow: elbowOf(li),
              middle: box.top + box.height / 2,
            },
          ];
        });

      return [
        ...measure(".clones > .clone", "summary .repo", "a clone's elbow"),
        ...measure(".pieces > li", ".spot-name", "a piece's elbow"),
        ...measure(".here > li", ".session", "a chat's elbow"),
      ];
    });

    // A run where a level drew nothing is a run that proved nothing about it.
    expect(elbows.length).toBeGreaterThan(1);
    const crooked = elbows
      .filter((at) => !(Math.abs(at.elbow - at.middle) <= 2.5))
      .map((at) => `${at.what}: drawn at ${at.elbow}px, its line centred on ${at.middle}px`);
    expect(crooked).toEqual([]);
  });
});

/** A file's row in a branch's folder — or, with no piece, in the repo's own — once it is drawn. */
async function fileRow(
  piece: string | null,
  path: string,
  timeout = 20_000,
  repo = "svc",
): Promise<WebdriverIO.Element> {
  const row = await $(`[data-row="file:${repo}/${piece ?? ""}:${path}"]`).getElement();
  await row.waitForExist({
    timeout,
    timeoutMsg: `the explorer never drew ${path} under ${piece ?? repo}`,
  });
  return row;
}

/**
 * `selector` inside the explorer. The file tab (FM-2) draws a branch's tree with the same row ids,
 * and a tab opened by an earlier spec is still there: a row the explorer stopped drawing can still
 * be in the file tab.
 */
function inExplorer(selector: string): string {
  return `[data-testid="explorer"] ${selector}`;
}

/** A file's row in a branch's folder in the explorer, once it is drawn there. */
async function explorerRow(
  piece: string,
  path: string,
  timeout = 20_000,
): Promise<WebdriverIO.Element> {
  const row = await $(inExplorer(`[data-row="file:svc/${piece}:${path}"]`)).getElement();
  await row.waitForExist({
    timeout,
    timeoutMsg: `the explorer never drew ${path} under ${piece}`,
  });
  return row;
}

/** Waits until a file's row in a branch's folder in the explorer carries `mark`. */
async function marked(piece: string, path: string, mark: string): Promise<void> {
  await $(
    inExplorer(`li[data-mark="${mark}"] > [data-row="file:svc/${piece}:${path}"]`),
  ).waitForExist({
    // FSEvents gives no bound on when it delivers on a busy Mac (#577), so the wait is long.
    timeout: 60_000,
    timeoutMsg: `the explorer never marked ${path} as ${mark}`,
  });
}

/** The chats on the strip, which is the focused workspace's and no other's. */
async function chatTabs(): Promise<string[]> {
  // In one pass: a chat is being added to the strip while it is polled (charter#506).
  return textOfEach('[role="tablist"][aria-label="Tabs"] [role="tab"] .tab-name');
}

/**
 * Opens the fixture branch's file tab from its row's menu — *Browse the files of fix-login* —
 * and waits for it to come forward.
 */
async function browseTheFiles(): Promise<void> {
  await $('[data-testid="piece-svc-fix-login"]').waitForExist({ timeout: 20_000 });
  expect(await sendContextMenu('[data-testid="piece-svc-fix-login"] .spot')).toBe(true);
  const menu = await $('[role="menu"]');
  await menu.waitForDisplayed({ timeout: 20_000 });
  let item: WebdriverIO.Element | undefined;
  for (const one of await $$('[role="menu"] [role="menuitem"]').getElements()) {
    if ((await one.getText()).includes("Browse the files of fix-login")) item = one;
  }
  if (item === undefined)
    throw new Error(`the branch's menu offers no file tab: ${await menu.getText()}`);
  await item.click();
  await browser.waitUntil(async () => (await tabInFront()) === "Files · fix-login", {
    timeout: 20_000,
    timeoutMsg: `the file tab never came forward; in front is ${await tabInFront()}`,
  });
}

/**
 * Drags the file tab's divider `by` pixels to the right, as the pointer events a drag sends.
 *
 * **Dispatched rather than performed**, for the reason `sendContextMenu` gives: a WebDriver
 * pointer action on the embedded driver raises no pointer events the page sees, measured on
 * macOS. What is sent is the sequence the platform would send — down on the divider's centre,
 * a move, and up — to the document `react-resizable-panels` listens on.
 */
async function dragTheDivider(by: number): Promise<boolean> {
  return browser.execute(async (dx: number) => {
    const handle = document.querySelector('[role="separator"][aria-label="Resize the file tree"]');
    if (!(handle instanceof HTMLElement)) return false;
    const box = handle.getBoundingClientRect();
    const x = box.left + box.width / 2;
    const y = box.top + box.height / 2;
    const at = (type: string, clientX: number, target: EventTarget) =>
      target.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          cancelable: true,
          pointerId: 1,
          pointerType: "mouse",
          isPrimary: true,
          button: 0,
          buttons: type === "pointerup" ? 0 : 1,
          clientX,
          clientY: y,
        }),
      );
    const frame = () => new Promise((done) => requestAnimationFrame(done));
    at("pointerdown", x, handle);
    await frame();
    for (let step = 1; step <= 6; step++) {
      at("pointermove", x + (dx * step) / 6, document);
      await frame();
    }
    at("pointerup", x + dx, document);
    return true;
  }, by);
}

/** Where a divider says it is, in percent. */
async function valueOf(divider: WebdriverIO.Element): Promise<number> {
  return Number(await divider.getAttribute("aria-valuenow"));
}

/** The file tabs a plane's record names, read off the disk. */
function filesTabsIn(record: string): { key: string; split?: number | null }[] {
  try {
    const views =
      (
        JSON.parse(readFileSync(record, "utf8")) as {
          views?: { view: string; key: string; split?: number | null }[];
        }
      ).views ?? [];
    return views.filter((one) => one.view === "piece-files");
  } catch {
    return [];
  }
}

/** Every tab on the strip, by name. */
async function tabNames(): Promise<string[]> {
  return textOfEach('[role="tablist"][aria-label="Tabs"] [role="tab"] .tab-name');
}

/** What the tab in front is called. */
async function tabInFront(): Promise<string> {
  const name = await $(
    '[role="tablist"][aria-label="Tabs"] [role="tab"][aria-selected="true"] .tab-name',
  );
  await name.waitForExist({ timeout: 20_000 });
  return name.getText();
}
