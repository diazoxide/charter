import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { browser, expect, $ } from "@wdio/globals";
import { built, READY } from "../harness.js";
import { harnessRowsDrawn, pickAndStart, pressOnly } from "../opening.js";

/**
 * **A chat's `ctx`/`cache` gauge, end to end** — everything but Claude Code itself.
 *
 * charter ADR 0019 recorded the loss: a Claude Code session inside charter has no context
 * gauge on any surface. Three things have to line up for the app to give it back, and this
 * spec drives all three against the real app and the real `charter` binary:
 *
 * 1. **The feed.** The settings charter arms a Claude Code chat with name `charter statusline`
 *    as its `statusLine` — the one command Claude Code hands the context numbers to. The fake
 *    harness runs nothing, so this spec runs the command charter armed, with the turn's
 *    payload on stdin, the way Claude Code would.
 * 2. **The key.** That command files the turn under the payload's session id; the app reads
 *    the file for the conversation its board holds for the chat. A turn it cannot find is a
 *    gauge it never draws.
 * 3. **The drawing.** The chat's own pane shows `ctx 42%` — and nothing before the turn,
 *    never `ctx 0%`.
 *
 * **What is not here, said:** that a real Claude Code runs the `statusLine` a `--settings`
 * argument names. `--settings` MERGES and was measured doing so for hooks on claude 2.1.276
 * (`charter_core::harness`); for `statusLine` it is Claude Code's documented settings key,
 * and this run does not have a Claude Code to ask.
 */

const dialog = () => $('[role="dialog"]');

/** What the profile's wrapper wrote down for each chat (`harness.ts`). */
function seen(plane: string): Record<string, string> {
  const where = join(plane, ".charter", "scenario-harness");
  if (!existsSync(where)) return {};
  return Object.fromEntries(
    readdirSync(where).map((name) => [name, readFileSync(join(where, name), "utf8")]),
  );
}

async function planeRoot(): Promise<string> {
  const said = await $("span.plane code");
  await said.waitForDisplayed({ timeout: 30_000 });
  return (await said.getText()).trim();
}

async function tabNames(): Promise<string[]> {
  return browser.execute(() =>
    [
      ...(document
        .querySelector('[role="tablist"][aria-label="Tabs"]')
        ?.querySelectorAll('[role="tab"]') ?? []),
    ].map((tab) => tab.textContent ?? ""),
  );
}

async function aPaneShows(text: string): Promise<boolean> {
  const rows: string[] = await browser.execute(() =>
    [...document.querySelectorAll(".xterm-rows")].map((rows) => rows.textContent ?? ""),
  );
  return rows.some((row) => row.replace(/\s+/g, " ").includes(text));
}

/** The gauges on screen, as text. */
async function gauges(): Promise<string[]> {
  return browser.execute(() =>
    [...document.querySelectorAll('[data-testid="chat-gauge"]')].map(
      (gauge) => gauge.textContent ?? "",
    ),
  );
}

/** A hook, as Claude Code would fire it for this chat. */
function hook(event: string, chat: string, socket: string, sid: string): void {
  execFileSync(built("charter"), ["hook", event], {
    input: JSON.stringify({ session_id: sid }),
    env: {
      ...process.env,
      CHARTER_CHAT: chat,
      CHARTER_HOOK_SOCKET: socket,
      CLAUDE_CODE_SESSION_ID: sid,
      CLAUDE_PID: String(process.pid),
    },
  });
}

describe("a chat's context gauge", () => {
  let wereAlreadyOpen: string[] = [];

  before(async () => {
    wereAlreadyOpen = await tabNames();
  });

  after(async () => {
    for (const name of (await tabNames()).filter((tab) => !wereAlreadyOpen.includes(tab))) {
      await pressOnly(`End chat ${name}`);
    }
    await browser.waitUntil(
      async () => (await tabNames()).every((tab) => wereAlreadyOpen.includes(tab)),
      { timeout: 20_000, timeoutMsg: "the gauge spec left a chat open behind it" },
    );
  });

  afterEach(async () => {
    if (
      await dialog()
        .isDisplayed()
        .catch(() => false)
    ) {
      await browser.keys(["Escape"]);
    }
  });

  it("draws the context the chat's statusLine recorded, in the chat's own pane", async () => {
    const plane = await planeRoot();
    const before = seen(plane);

    await pressOnly("New tab");
    await harnessRowsDrawn();
    await pickAndStart();
    await browser.waitUntil(
      () =>
        Object.keys(seen(plane)).some((name) => name.startsWith("settings-") && !(name in before)),
      { timeout: 30_000, interval: 250, timeoutMsg: "the chat's harness never ran" },
    );
    await browser.waitUntil(() => aPaneShows(READY), {
      timeout: 30_000,
      interval: 250,
      timeoutMsg: "the harness the profile names never reached the pane",
    });

    const now = seen(plane);
    const settingsFile = Object.keys(now).find(
      (name) => name.startsWith("settings-") && !(name in before),
    );
    if (settingsFile === undefined) throw new Error("no settings were written for the new chat");
    const chat = settingsFile.slice("settings-".length);
    const sid = now[`session-${chat}`];
    const socket = now[`socket-${chat}`];
    const settings = JSON.parse(now[settingsFile]) as {
      statusLine?: { type: string; command: string };
    };

    // 1. The feed: charter armed its own statusline as this session's statusLine.
    expect(settings.statusLine?.type).toBe("command");
    expect(settings.statusLine?.command).toMatch(/charter'? statusline$/);
    // Nothing is drawn before a turn has been recorded — never `ctx 0%`.
    expect(await gauges()).toEqual([]);

    // A turn, as Claude Code hands it to its statusLine command.
    const statusLine = settings.statusLine?.command ?? "";
    execFileSync("/bin/sh", ["-c", statusLine], {
      input: JSON.stringify({
        session_id: sid,
        context_window: {
          current_usage: { cache_read_input_tokens: 9000, cache_creation_input_tokens: 1000 },
          used_percentage: 42,
        },
      }),
      env: { ...process.env, CHARTER_ROOT: plane },
    });
    // The turn starting, which is when the pane reads its record again.
    hook("userpromptsubmit", chat, socket, sid);

    // 2 and 3: filed under the conversation the board holds, and drawn in the pane.
    await browser.waitUntil(async () => (await gauges()).some((g) => g.includes("ctx 42%")), {
      timeout: 30_000,
      interval: 250,
      timeoutMsg: "the pane never drew the turn its statusLine recorded",
    });
    expect((await gauges()).join(" ")).toContain("cache 90%");

    // **No `stop` here, deliberately.** A `stop` puts the chat in the needs-you queue, and
    // `palette.e2e.ts` asserts that queue is empty in this run ("Nothing reports a hook in
    // this run") — it went red on CI when this spec ended the turn. The chat is ended by
    // `after` instead, mid-turn, which leaves the queue as it found it.
  });
});
