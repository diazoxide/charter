import { browser, expect, $, $$ } from "@wdio/globals";
import { chmodSync, existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import process from "node:process";
import { A_FINDER_LAUNCHS_PATH, built, READY } from "../harness.js";
import { THEIR_STATUS_LINE } from "../wdio.finder.conf.js";
import { harnessRowsDrawn, pickAndStart, pressOnly } from "../opening.js";

/**
 * A chat started on a BUILT-IN profile, in an app launched the way Finder launches one.
 *
 * `wdio.finder.conf.ts` gives the app `PATH=/usr/bin:/bin:/usr/sbin:/sbin` and a `$HOME`
 * whose only `claude` is in `.local/bin`. Nothing is declared in the plane, so the profile
 * the picker offers is charter's own built-in, whose command is the bare word `claude`.
 *
 * Before charter-app#134 was fixed this could not get past the picker: the harness could not
 * be found, `wiring` answered `State::Unknown`, and the app refused with *"an unknown is not a
 * pass — nothing was started"*. Every other spec in this suite passed throughout, because
 * they all run under CI's own `PATH` against an absolute command.
 */
const dialog = () => $('[role="dialog"]');
const TABS = '[role="tablist"][aria-label="Tabs"] [role="tab"]';

/** The plane and `$HOME` the launcher gave the app (`wdio.finder.conf.ts`). */
function given(name: "CHARTER_FINDER_PLANE" | "CHARTER_FINDER_HOME"): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is not set — the spec is not running under its config`);
  return value;
}

/** What the NEWEST tab says its chat is doing, by the accessible name on its state mark. */
async function theNewestTabSays(): Promise<string> {
  const tabs = await $$(TABS).getElements();
  if (tabs.length === 0) return "(no tab)";
  const mark = await tabs[tabs.length - 1].$(".state");
  return (await mark.getAttribute("aria-label")) ?? "";
}

/** Waits for the newest tab to say `state`, and says what it said instead if it never does. */
async function theNewestTabComesToSay(state: string, why: string): Promise<void> {
  let last = "";
  try {
    await browser.waitUntil(
      async () => {
        last = await theNewestTabSays();
        return last === state;
      },
      { timeout: 30_000, interval: 250 },
    );
  } catch {
    const panes = await $$('[data-testid="pane"]').getElements();
    // The terminal's rows and not the whole pane: the pane also holds xterm's own stylesheet.
    const screen = panes.length
      ? await panes[panes.length - 1].$(".xterm-rows").getText()
      : "(no pane)";
    throw new Error(
      `${why}: the newest chat showed ${JSON.stringify(last)}, never ${JSON.stringify(state)}. ` +
        `Its pane said:\n${screen}`,
    );
  }
}

/**
 * Starts one more chat on the built-in `claude` and waits for its tab. A tab and not a pane:
 * a new tab takes the front, so the number of panes on screen need not change.
 */
async function startAChatOnTheBuiltIn(): Promise<void> {
  const before = (await $$(TABS).getElements()).length;
  await pressOnly("New tab");
  await harnessRowsDrawn();
  await (await $("label*=claude")).click();
  await pickAndStart();
  await browser.waitUntil(async () => (await $$(TABS).getElements()).length === before + 1, {
    timeout: 30_000,
    interval: 250,
    timeoutMsg: "no tab opened for the new chat",
  });
}

/** The `PATH` the newest chat's harness wrote down (`writeAPluginHookingShell`). */
function theNewestChatsPath(): string[] {
  const where = join(given("CHARTER_FINDER_PLANE"), ".charter", "scenario");
  const recorded = existsSync(where)
    ? readdirSync(where)
        .filter((name) => name.startsWith("path-"))
        .map((name) => join(where, name))
        .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)
    : [];
  if (recorded.length === 0) throw new Error(`no chat wrote its PATH down under ${where}`);
  return readFileSync(recorded[0], "utf8").split(":");
}

describe("an app opened from Finder", () => {
  it("offers the built-in profile whose program only a shell's PATH would find", async () => {
    await pressOnly("New tab");

    await expect(dialog()).toBeDisplayed();
    // The built-in is listed only when charter can find its program, so the row being here is
    // already the search answering.
    await expect(dialog()).toHaveText(expect.stringContaining("claude"));

    await browser.keys(["Escape"]);
    await expect(dialog()).not.toBeDisplayed();
  });

  it("starts the chat rather than refusing over a harness it could not look for", async () => {
    const before = (await $$('[data-testid="pane"]').getElements()).length;

    await pressOnly("New tab");
    await harnessRowsDrawn();
    await (await $("label*=claude")).click();
    await pickAndStart();

    await browser.waitUntil(
      async () => (await $$('[data-testid="pane"]').getElements()).length === before + 1,
      {
        timeout: 30_000,
        interval: 250,
        timeoutMsg:
          "charter-app#134: no pane opened, so the app launched from Finder refused the chat",
      },
    );
    const panes = await $$('[data-testid="pane"]').getElements();
    const last = panes[panes.length - 1];
    await browser.waitUntil(async () => (await last.getText()).includes(READY), {
      timeout: 30_000,
      interval: 250,
      timeoutMsg: "charter-app#134: a pane opened but the harness in ~/.local/bin never reached it",
    });
  });

  // charter-app#136. The chat above is running; its harness then runs `charter hook
  // sessionstart` by the bare word, as a plane's own `.claude/settings.json` spells it. The
  // hooks charter arms on a chat itself name the binary by its absolute path, so they reach the
  // board from any launch; this one reaches it only if the chat's own `PATH` can find a
  // `charter`. From Finder, before #136, it could not: `/bin/sh: charter: command not found`,
  // and the tab stayed `unknown`.
  it("lets a hook that names charter by its bare word reach charter", async () => {
    // No `charter` anywhere in this `$HOME`: the one found is the app's own, beside its
    // executable — the floor under a machine where the app is the only charter there is.
    await theNewestTabComesToSay(
      "waiting on you",
      "charter-app#136: the plugin-spelled `charter hook sessionstart` never reached the board",
    );
  });

  it("gives the chat charter's own directory, then its inherited PATH, then where a shell finds things", () => {
    const path = theNewestChatsPath();
    const home = given("CHARTER_FINDER_HOME");
    // FIRST: nothing the app ships runs on a Python fallback (ADR 0025), so a bare `charter`
    // in a chat is the one this app was built with (the next test).
    expect(path[0]).toBe(dirname(built("charter-app")));
    // Then strictly additive: what the app inherited, in its own order.
    expect(path.slice(1, 5).join(":")).toBe(A_FINDER_LAUNCHS_PATH);
    // `programs::USER_BIN` and `SYSTEM_BIN` — the list that found the harness is the list the
    // harness then searches, so a `node`, `gh` or `uv` installed through Homebrew or a user
    // installer is not "missing from this machine" to the model.
    for (const dir of [join(home, ".local", "bin"), "/opt/homebrew/bin", "/usr/local/bin"]) {
      expect(path).toContain(dir);
    }
  });

  it("runs the app's own charter even where the operator installed another", async () => {
    // The operator's own shape: the Python charter in `~/.local/bin`, where `uv`/`pipx` put
    // it. An app chat no longer loads that charter's plugin, and nothing the app ships runs on
    // a Python fallback (ADR 0025) — so a bare `charter` in the chat is the app's, and the
    // installed one is never asked. `programs::chat_path_from` carries the argument. A
    // stand-in that says it was asked, then hands the call to the real binary.
    const home = given("CHARTER_FINDER_HOME");
    const asked = join(home, "the-installed-charter-was-asked");
    const installed = join(home, ".local", "bin", "charter");
    writeFileSync(
      installed,
      [
        "#!/bin/sh",
        `printf '%s\\n' "$*" >> ${JSON.stringify(asked)}`,
        `exec ${JSON.stringify(built("charter"))} "$@"`,
        "",
      ].join("\n"),
    );
    chmodSync(installed, 0o755);

    await startAChatOnTheBuiltIn();
    await theNewestTabComesToSay(
      "waiting on you",
      "charter-app#136: the second chat's bare `charter` hook never reached the board",
    );
    expect(existsSync(asked) ? readFileSync(asked, "utf8") : "").toBe("");
  });

  it("has a doctor that answers for the app, with the PATH Finder gave it", async () => {
    // The incident `app/src-tauri/src/doctor.rs` exists for. The doctor was ported and CLI
    // only, so the one way to ask it was a terminal — whose shell has the operator's whole
    // PATH and cannot reproduce a Finder launch. Asked from the window, it runs in the app's
    // own process: the PATH it reports is the four-directory one, and the built-in `claude`
    // it lists is the one only charter's fixed search (charter-app#134) could find.
    const button = await $('[data-testid="status-doctor"]');
    await button.waitForExist({ timeout: 30_000 });
    await button.click();

    await expect(dialog()).toBeDisplayed();
    await browser.waitUntil(async () => (await dialog().getText()).includes("profile claude"), {
      timeout: 60_000,
      interval: 250,
      timeoutMsg: "the full doctor, opened from the window, listed no built-in claude profile",
    });
    await expect(dialog()).toHaveText(expect.stringContaining(A_FINDER_LAUNCHS_PATH));
    await expect(dialog()).toHaveText(expect.stringContaining("each harness profile probed"));

    await browser.keys(["Escape"]);
    await expect(dialog()).not.toBeDisplayed();
  });

  it("says, in the doctor, that it left the operator's own status line alone", async () => {
    // **The other half of the 2026-09-22 ruling.** This launch's `$CLAUDE_CONFIG_DIR` carries
    // a `statusLine` of the operator's (`wdio.finder.conf.ts`). One charter armed through
    // `--settings` would shadow it silently — measured on Claude Code 2.1.280 — so charter
    // arms none, and the cost (this chat records no turn, so its ctx/cache gauge stays dark)
    // is said by the doctor rather than left looking like breakage.
    const button = await $('[data-testid="status-doctor"]');
    await button.waitForExist({ timeout: 30_000 });
    await button.click();
    await expect(dialog()).toBeDisplayed();

    await browser.waitUntil(async () => (await dialog().getText()).includes("chat footer"), {
      timeout: 60_000,
      interval: 250,
      timeoutMsg: "the doctor drew no `chat footer` row",
    });
    const said = await dialog().getText();
    // It names the file in force, not merely that something is.
    expect(said).toContain("settings.json fills Claude Code's status line");
    expect(said).toContain("stays dark");
    expect(said).toContain("will not replace a status line you wrote");
    // And it is under the app's own heading, because `charter doctor` does not print it.
    await expect(dialog().$('section[aria-label="This app"]')).toBeDisplayed();
    // The launcher wrote a command that would genuinely have run.
    expect(THEIR_STATUS_LINE.startsWith("/bin/echo")).toBe(true);

    await browser.keys(["Escape"]);
    await expect(dialog()).not.toBeDisplayed();
  });
});
