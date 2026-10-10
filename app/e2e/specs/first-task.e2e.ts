import { execFileSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { basename, join } from "node:path";
import process from "node:process";
import { $, browser, expect } from "@wdio/globals";
import {
  built,
  READY,
  singleQuoted,
  THE_RUNS_GIT_CONFIG,
  THE_RUNS_HOME,
  THE_RUNS_TREE,
} from "../harness.js";
import { closeProject } from "../opening.js";
import { ask } from "../switching.js";

/**
 * **The first task, end to end, in the window** (FR-28 #621; #1091's line from #944): a fresh
 * repo opened from New project into this machine's local project, the First task tab offered
 * beside its first chat, both of the task's chats started from that tab on the stand-in harness,
 * the second one briefed with what the first one recorded, and **Show its diff** for each.
 *
 * `crates/purlis-cli/tests/first_task_script.rs` runs the same script through the core's paths.
 * What only a window can show is the tab's own path: the press that approves and starts each
 * chat, the task typed into it and not sent, each chat on a branch of its own, the second
 * chat's briefing as its harness received it, and the shell tab each diff opens.
 *
 * **The stand-in is found where Claude Code's installer puts it** (`~/.local/bin/claude` in the
 * run's own `$HOME`), because the first run offers the task only beside a harness this machine
 * has (FR-29): the local project declares no profile, so its chats run charter's built-in
 * Claude Code profile, which finds the stand-in by that search. The stand-in reports its
 * `SessionStart` through the real `charter hook`, as Claude Code does, so the core types the
 * task once it is at its prompt; and it says in its own pane whether the briefing that hook
 * answered carries the first chat's lesson.
 *
 * **What a harness does with the task is the model's**, so the first chat's lesson is recorded
 * here, on its behalf, with the command the task asks for (`purlis persona remember`), run in the
 * first chat's own folder as its agent would run it.
 *
 * **One app process serves the whole run**, so everything this spec adds goes again: the
 * stand-in is taken out of the run's `$HOME` (it would otherwise be a built-in row in every
 * picker after this spec), the local project is closed by its own `×`, which ends its chats,
 * and the machine store forgets it.
 */

const PROJECTS = '[data-strip="Projects"]';
const TABS = '[data-strip="Tabs"]';

/** A word nobody else's briefing has, so the stand-in can say whether its briefing holds it. */
const LESSON_MARK = "e2e-first-task-lesson";
const LESSON = `shop's checks run with make test (${LESSON_MARK})`;

/** The stand-in Claude Code, where its installer puts it. */
const STAND_IN = join(THE_RUNS_HOME, ".local", "bin", "claude");

/** A repo nobody has opened in purlis: one commit, and an `origin` that names its forge. */
const repo = (() => {
  const at = join(mkdtempSync(join(THE_RUNS_TREE, "first-task-")), "shop");
  mkdirSync(at, { recursive: true });
  const git = (args: string[]) =>
    execFileSync(
      "git",
      ["-c", "user.name=purlis scenario", "-c", "user.email=scenario@example.invalid", ...args],
      {
        cwd: at,
        stdio: "pipe",
        env: { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_SYSTEM: "/dev/null" },
      },
    );
  git(["init", "-q", "-b", "main", "."]);
  writeFileSync(join(at, "README.md"), "# shop\n");
  git(["add", "-A"]);
  git(["commit", "-q", "-m", "start"]);
  // Only so the first run need not ask which forge the project's repos are on (#839): nothing
  // here reaches it, and the repo is cloned from this folder.
  git(["remote", "add", "origin", "https://github.com/scenario/shop.git"]);
  return at;
})();

/**
 * The stand-in: Claude Code's flags dropped but its `--session-id`, its `SessionStart` reported
 * through the hook binary the app armed it with, and then a raw prompt that echoes what is typed
 * and answers a line only on Enter. The hook's answer is the chat's briefing; the pane is told
 * whether it carries the lesson, and nothing else of it.
 */
function theStandIn(): string {
  const sessionStart = [
    `printf '{"session_id":"%s","hook_event_name":"SessionStart","source":"startup"}'`,
    '"$CLAUDE_CODE_SESSION_ID"',
    '| CLAUDE_PID=$PPID "${CHARTER_HOOK_BINARY:-charter}" hook sessionstart',
    `| grep -c ${singleQuoted(LESSON_MARK)}`,
    "| sed 's/^/lesson in the briefing: /'",
  ].join(" ");
  return [
    "#!/bin/sh",
    "# Written by the scenario tests: Claude Code, as far as the first task needs it.",
    'while [ $# -gt 0 ]; do [ "$1" = "--session-id" ] && CLAUDE_CODE_SESSION_ID=$2; shift; done',
    "export CLAUDE_CODE_SESSION_ID",
    `exec ${singleQuoted(built("fake-harness"))} --sentinel ${singleQuoted(READY)} \\`,
    `  --hook ${singleQuoted(sessionStart)} --interactive --raw`,
    "",
  ].join("\n");
}

/** Every terminal's rows on screen, with the terminal's no-break spaces read as spaces. */
async function rows(): Promise<string[]> {
  return browser.execute(() =>
    [...document.querySelectorAll(".pane-frame .xterm-rows")].map((r) =>
      (r.textContent ?? "").replace(/\s+/g, " "),
    ),
  );
}

/** Waits until some terminal on screen shows `text`, and says what they showed if none does. */
async function aPaneShows(text: string, why: string): Promise<void> {
  await browser
    .waitUntil(async () => (await rows()).some((one) => one.includes(text)), {
      timeout: 60_000,
      interval: 250,
    })
    .catch(async () => {
      throw new Error(
        `${why}: no pane showed ${JSON.stringify(text)}; panes: ${(await rows()).join("|")}; the window says: ${await windowSays()}`,
      );
    });
}

/**
 * What the window says, for a failure's message: the tabs, the one in front, the First task
 * tab's text, and every alert, status line and dialog on screen.
 */
async function windowSays(): Promise<string> {
  return browser.execute((selector: string) => {
    const tabs = [...document.querySelectorAll(`${selector} [role="tab"]`)].map(
      (tab) => `${tab.getAttribute("aria-selected") === "true" ? "*" : ""}${tab.textContent ?? ""}`,
    );
    const said = [
      ...document.querySelectorAll<HTMLElement>(
        '.first-task,[role="alert"],[role="status"],[role="dialog"],[role="alertdialog"]',
      ),
    ].map((one) => one.innerText.trim().replace(/\s+/g, " "));
    return `tabs [${tabs.join(", ")}]; ${said.filter(Boolean).join(" | ")}`;
  }, TABS);
}

/** The tab on the strip whose name holds `name`, brought to the front. */
async function toTheTab(name: string): Promise<void> {
  await browser.waitUntil(
    () =>
      browser.execute(
        (selector: string, wanted: string) =>
          [...document.querySelectorAll(`${selector} [role="tab"]`)].some((tab) =>
            (tab.textContent ?? "").includes(wanted),
          ),
        TABS,
        name,
      ),
    { timeout: 30_000, timeoutMsg: `no ${name} tab came onto the strip` },
  );
  await browser.execute(
    (selector: string, wanted: string) => {
      [...document.querySelectorAll<HTMLElement>(`${selector} [role="tab"]`)]
        .find((tab) => (tab.textContent ?? "").includes(wanted))
        ?.click();
    },
    TABS,
    name,
  );
  await browser.waitUntil(async () => (await inFront()).includes(name), {
    timeout: 20_000,
    timeoutMsg: `the ${name} tab did not come to the front; ${await inFront()} is there`,
  });
}

/** The name of the tab in front. */
async function inFront(): Promise<string> {
  return browser.execute(
    (selector: string) =>
      document.querySelector(`${selector} [role="tab"][aria-selected="true"]`)?.textContent ?? "",
    TABS,
  );
}

/**
 * Presses the First task tab's button whose words end with `words`, once it can be pressed: a
 * profile not approved yet reads "Approve and start …", and one approved reads "Start …".
 */
async function pressInTheTab(words: string): Promise<void> {
  const button = await $(
    `//div[@class="first-task"]//button[substring(normalize-space(), string-length(normalize-space()) - ${words.length - 1}) = "${words}"]`,
  );
  await button.waitForExist({ timeout: 30_000, timeoutMsg: `the First task tab has no ${words}` });
  await browser.waitUntil(async () => await button.isEnabled(), {
    timeout: 30_000,
    timeoutMsg: `${words} never became pressable`,
  });
  await browser.execute((el: HTMLElement) => el.click(), button as unknown as HTMLElement);
}

/** What the First task tab says about its runs, in one read. */
async function theTabSays(): Promise<string> {
  return browser.execute(
    () => (document.querySelector(".first-task") as HTMLElement | null)?.innerText ?? "",
  );
}

type Chat = { session: number; cwd: string | null; label: string | null };

// The local project is in the data home, where a sandboxed chat starts (#1670): both of the
// task's chats run under the sandbox the local project turns on.
describe("the first task, from its tab", function () {
  this.timeout(600_000);

  /** The project the launch opened, which this spec never touches. */
  let first = "";
  /** The local project the repo was opened into. */
  let local = "";
  /** A `claude` the run's `$HOME` already had, put back afterwards. */
  let hadOne: string | undefined;

  before(async () => {
    first = (await ask<string[]>("open_planes"))[0];
    if (existsSync(STAND_IN)) hadOne = readFileSync(STAND_IN, "utf8");
    mkdirSync(join(THE_RUNS_HOME, ".local", "bin"), { recursive: true });
    writeFileSync(STAND_IN, theStandIn());
    chmodSync(STAND_IN, 0o755);
  });

  after(async () => {
    if (hadOne === undefined) rmSync(STAND_IN, { force: true });
    else writeFileSync(STAND_IN, hadOne);
    // A dialog a failure left up is over everything the next spec reaches for.
    for (let left = 0; left < 3; left++) {
      const up = await $('[role="dialog"]')
        .isExisting()
        .catch(() => false);
      if (!up) break;
      await browser.keys(["Escape"]);
    }
    if (local !== "") {
      const closer = `${PROJECTS} button[aria-label="Close project ${basename(local)}"]`;
      if (await $(closer).isExisting()) await closeProject(closer);
    }
    for (const plane of (await ask<string[]>("open_planes")).filter((one) => one !== first))
      await ask("close_plane", { plane });
    if (local !== "") await ask("forget_project", { path: local }).catch(() => undefined);
  });

  it("starts both chats from the First task tab, briefs the second with the first's lesson, and shows each diff", async () => {
    const before = await ask<string[]>("open_planes");

    // New project, its default form: the repo, opened into this machine's local project.
    await $(`${PROJECTS} button[aria-label="New project…"]`).click();
    const dialog = await $('[role="dialog"][aria-labelledby="new-project"]');
    await dialog.waitForExist({ timeout: 20_000 });
    // The repo's box is the dialog's first; the Advanced form's boxes are folded away.
    const box = await dialog.$('input[placeholder="/where/the/repo/is"]');
    await box.waitForExist({ timeout: 20_000 });
    await box.addValue(repo);
    await dialog.$("button=Open repo").click();

    // The local project is a project this machine has approved nothing about yet.
    const approve = await $("button=Open project");
    await approve.waitForExist({ timeout: 60_000 });
    await approve.click();
    await browser.waitUntil(
      async () => (await ask<string[]>("open_planes")).some((one) => !before.includes(one)),
      { timeout: 60_000, timeoutMsg: "the local project never opened" },
    );
    local = (await ask<string[]>("open_planes")).find((one) => !before.includes(one)) ?? "";

    // The first chat's picker comes up with the First task tab beside it. The picker is not
    // the task's, so it is let go; the tab stays.
    const picker = await $('[role="dialog"][aria-labelledby="start-chat"]');
    if (
      await picker
        .waitForExist({ timeout: 30_000 })
        .then(() => true)
        .catch(() => false)
    ) {
      await browser.keys(["Escape"]);
      await picker.waitForExist({ timeout: 20_000, reverse: true });
    }

    await toTheTab("First task");
    await $(".first-task").waitForExist({ timeout: 20_000 });

    // The first chat: approved by this press, on a branch of its own, the task typed unsent.
    await pressInTheTab("tart the first chat");
    await aPaneShows("first task, on this chat", "the task was never typed into the first chat");
    await aPaneShows("lesson in the briefing: 0", "the first chat's harness was never briefed");
    expect((await rows()).some((one) => one.includes("you said:"))).toBe(false);

    const chats = () => ask<Chat[]>("opened_chats", { plane: local });
    const firstRun = (await chats()).find((one) => one.label === "first task 1");
    expect(firstRun?.cwd).toBeTruthy();

    // Its agent records what it learned, with the command the task names, in its own folder.
    execFileSync(built("purlis"), ["persona", "remember", LESSON], {
      cwd: firstRun?.cwd ?? undefined,
      stdio: "pipe",
      env: {
        PATH: process.env.PATH ?? "",
        HOME: THE_RUNS_HOME,
        GIT_CONFIG_GLOBAL: THE_RUNS_GIT_CONFIG,
      },
    });

    // The second chat, from the same tab: its briefing carries the first chat's lesson.
    await toTheTab("First task");
    await browser.waitUntil(
      async () => (await theTabSays()).includes("Started on the branch first-task-1."),
      {
        timeout: 20_000,
        timeoutMsg: `the tab never said where the first chat started: ${await theTabSays()}`,
      },
    );
    await pressInTheTab("tart the second chat");
    await aPaneShows(
      "lesson in the briefing: 1",
      "the second chat's briefing did not carry the first chat's lesson",
    );
    const secondRun = (await chats()).find((one) => one.label === "first task 2");
    expect(secondRun?.cwd).toBeTruthy();
    // Each on a branch of its own, in a folder of its own.
    expect(secondRun?.cwd).not.toBe(firstRun?.cwd);

    // Show its diff, for each: a shell tab in that chat's folder, with the diff command run.
    await toTheTab("First task");
    await browser.waitUntil(
      async () => (await theTabSays()).includes("Started on the branch first-task-2."),
      {
        timeout: 20_000,
        timeoutMsg: `the tab never said where the second chat started: ${await theTabSays()}`,
      },
    );
    for (const [at, run] of [
      [0, firstRun],
      [1, secondRun],
    ] as const) {
      const shells = (await chats()).filter((one) => one.label === null).length;
      await toTheTab("First task");
      await browser.execute((index: number) => {
        const diffs = [
          ...document.querySelectorAll<HTMLButtonElement>(".first-task button"),
        ].filter((button) => button.textContent === "Show its diff");
        diffs[index]?.click();
      }, at);
      await browser.waitUntil(
        async () =>
          (await chats()).filter((one) => one.label === null && one.cwd === run?.cwd).length > 0 &&
          (await chats()).filter((one) => one.label === null).length > shells,
        {
          timeout: 30_000,
          timeoutMsg: `Show its diff opened no shell in ${run?.cwd ?? "the chat's folder"}`,
        },
      );
      await aPaneShows("git add -N -A && git diff", "the diff command was never run in the shell");
    }
  });
});
