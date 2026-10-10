import { existsSync, realpathSync, renameSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { anEmptyRecord, copyFixturePlane } from "../harness.js";
import { closeProject } from "../opening.js";
import { ask } from "../switching.js";

/**
 * **Locate… re-points a remembered project that moved** (#1291's third line; NO-5 #1237 and
 * ST-2 #1226), in the built app against the real core and this run's own machine store: a
 * project opened once, closed, moved on disk, listed as gone in Settings › You › This machine,
 * located at its new place there, and the store read back from the core, never from the row.
 *
 * `GoneProjectNotice.test.tsx`, `ThisMachine.test.tsx` and the core's tests hold the rest against
 * a mocked side each. What only a real window can say is that the folder the picker answers with
 * goes through the window's Locate… to `locate_project`, and the store the next launch reads
 * holds the new place.
 *
 * **The native folder dialog is the one thing stood in for** (D-1291-1). It is the operating
 * system's window, which no WebDriver reaches, and no build of the app has a seam for it. So the
 * page's IPC request for `pick_project` alone is answered here, with the folder a person would
 * have picked, at the one place a page sends every command through: `fetch` to Tauri's `ipc:`
 * protocol. Every other command goes to the core, `locate_project` included. The stand-in counts
 * what it answered, so a build that sent the command some other way fails here saying so, rather
 * than as a dialog nobody can close.
 *
 * **Its own project, copied into the run's tree**, and nothing of it left behind: one app process
 * serves the whole run, so the project is closed, its Settings tab closed by its own ×, the stand-in
 * taken out, and the store's entry forgotten at the end.
 */

const PROJECTS = '[data-strip="Projects"]';
const WORKSPACES = '[data-strip="Workspaces"]';
const TABS = '[data-strip="Tabs"]';
const LEVEL = '[role="radiogroup"][aria-label="Level"]';

/** The name the project is remembered by: its directory's, before it moved. */
const NAME = "located";

/** This spec's own project, where it is first opened. */
const mine = (() => {
  const copied = copyFixturePlane();
  const renamed = join(dirname(copied), NAME);
  renameSync(copied, renamed);
  const root = realpathSync(renamed);
  // Nothing to put back, so opening it starts no chat for the next spec to inherit.
  anEmptyRecord(root);
  return root;
})();

/** Where it is moved to: beside where it was, under another name, as a person moves a folder. */
const moved = join(dirname(mine), `${NAME}-moved`);

type MachineProject = { path: string; name: string; gone: string | null };

const remembered = async () => (await ask<{ projects: MachineProject[] }>("this_machine")).projects;

/** The level the Settings tab on screen is at, by its switcher. */
async function levelBecomes(want: string): Promise<void> {
  let saw = "";
  await browser.waitUntil(
    async () => {
      saw = await browser.execute(
        (selector: string) =>
          document
            .querySelector(`${selector} [role="radio"][aria-checked="true"]`)
            ?.textContent?.trim() ?? "",
        LEVEL,
      );
      return saw === want;
    },
    { timeout: 30_000, timeoutMsg: `Settings stayed at ${JSON.stringify(saw)}, not ${want}` },
  );
}

/**
 * Answers the page's next `pick_project` with `folder`, as the native dialog would have, and
 * counts each answer in `window.__pickedFor`. Only that command: everything else is sent on.
 */
async function theDialogWillPick(folder: string): Promise<void> {
  await browser.execute((picked: string) => {
    const held = window as unknown as {
      __realFetch?: typeof fetch;
      __pickedFor?: number;
      __fetched?: string[];
    };
    held.__realFetch ??= window.fetch.bind(window);
    held.__pickedFor = 0;
    held.__fetched = [];
    const real = held.__realFetch;
    window.fetch = (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
      // What went through here, for a failure's message: whether the IPC is sent this way at all.
      held.__fetched = [...(held.__fetched ?? []), url.replace(/^.*\//, "")].slice(-8);
      if (/\/pick_project(\?|$)/.test(url)) {
        held.__pickedFor = (held.__pickedFor ?? 0) + 1;
        return Promise.resolve(
          new Response(JSON.stringify(picked), {
            headers: { "Content-Type": "application/json", "Tauri-Response": "ok" },
          }),
        );
      }
      return real(input, init);
    };
  }, folder);
}

/** The page's own `fetch` again. */
async function theDialogIsTheSystems(): Promise<void> {
  await browser.execute(() => {
    const held = window as unknown as { __realFetch?: typeof fetch };
    if (held.__realFetch) window.fetch = held.__realFetch;
    delete held.__realFetch;
  });
}

// Skipped until purlis/purlis#1680: the page cannot stand in for the native folder picker. The
// IPC does not go through the page's `fetch` (the stand-in below saw no request at all on macOS
// or Linux), and Tauri's invoke cannot be replaced, so `pick_project` opens the real dialog and
// nothing answers it. Take the skip off with the e2e-only seam that issue asks for.
describe.skip("Locate… for a remembered project that moved", function () {
  this.timeout(180_000);

  /** The project the launch opened, which this spec never touches. */
  let first = "";

  before(async () => {
    first = (await ask<string[]>("open_planes"))[0];
    // Through the opener, as a person opens one, which is what puts it among the recents.
    await $(`${PROJECTS} button[aria-label="Open a project…"]`).click();
    const box = await $("#open-by-path");
    await box.waitForDisplayed({ timeout: 20_000 });
    await box.addValue(mine);
    await $("button=Open").click();
    await $('[role="dialog"]').waitForExist({ timeout: 30_000 });
    await $("button=Open project").click();
    await browser.waitUntil(async () => (await ask<string[]>("open_planes")).includes(mine), {
      timeout: 30_000,
      timeoutMsg: "this spec's project never opened",
    });
    await $(`${WORKSPACES} [role="tab"].plane-root`).waitForExist({ timeout: 30_000 });

    // Let go of it, and move it while nothing holds it.
    const closer = `${PROJECTS} button[aria-label="Close project ${basename(mine)}"]`;
    await closeProject(closer);
    await browser.waitUntil(async () => !(await ask<string[]>("open_planes")).includes(mine), {
      timeout: 20_000,
      timeoutMsg: "this spec's project stayed open",
    });
    renameSync(mine, moved);
  });

  after(async () => {
    await theDialogIsTheSystems();
    // Each Settings tab this spec opened, closed by its own ×.
    await browser.execute((selector: string) => {
      for (const closer of document.querySelectorAll<HTMLElement>(
        `${selector} button[aria-label^="Close Settings"]`,
      ))
        closer.click();
    }, TABS);
    // Anything the window could not close is let go of in the core all the same.
    for (const plane of (await ask<string[]>("open_planes")).filter((one) => one !== first))
      await ask("close_plane", { plane });
    // The store as it was found: neither the old place nor the new one is remembered.
    for (const one of await remembered())
      if ([mine, moved].some((path) => samePlace(one.path, path)))
        await ask("forget_project", { path: one.path });
  });

  it("lists the moved project as gone in This machine, and Locate… re-points it in the store", async () => {
    const before = (await remembered()).find((one) => samePlace(one.path, mine));
    expect(before).toBeDefined();
    expect(before?.gone).not.toBeNull();

    // Settings, at the You level, on This machine.
    await browser.executeAsync((done: () => void) => {
      void window.__TAURI__.event.emit("settings-asked").then(done, done);
    });
    await $(LEVEL).waitForExist({ timeout: 30_000 });
    await browser.execute((selector: string) => {
      [...document.querySelectorAll<HTMLElement>(`${selector} [role="radio"]`)]
        .find((one) => one.textContent?.trim() === "You")
        ?.click();
    }, LEVEL);
    await levelBecomes("You");
    await $('nav[aria-label="Groups"]').$("button=This machine").click();

    const locate = await $(`button[aria-label="Locate ${NAME}"]`);
    await locate.waitForExist({
      timeout: 30_000,
      timeoutMsg: "This machine never offered Locate… for the project that moved",
    });

    await theDialogWillPick(moved);
    await browser.execute((selector: string) => {
      document.querySelector<HTMLElement>(selector)?.click();
    }, `button[aria-label="Locate ${NAME}"]`);

    await browser
      .waitUntil(async () => (await remembered()).some((one) => samePlace(one.path, moved)), {
        timeout: 30_000,
      })
      .catch(async () => {
        // Said in full, because a scenario's failure is read from its annotation: what the
        // store holds now, whether the stand-in answered, and what the window says.
        const [answered, fetched, replaced] = await browser.execute(() => {
          const held = window as unknown as { __pickedFor?: number; __fetched?: string[] };
          return [
            held.__pickedFor ?? 0,
            (held.__fetched ?? []).join(","),
            String(!/\[native code\]/.test(String(window.fetch))),
          ] as const;
        });
        throw new Error(
          `the machine store never held ${moved}; it holds ${JSON.stringify(
            (await remembered()).map((one) => `${one.path} (${one.gone ?? "here"})`),
          )}; the dialog's stand-in answered ${answered} time(s); fetch replaced: ${replaced}; it saw [${fetched}]; the window says: ${await windowSays()}`,
        );
      });
    const picked = await browser.execute(
      () => (window as unknown as { __pickedFor?: number }).__pickedFor ?? 0,
    );
    expect(picked).toBe(1);

    const now = await remembered();
    const located = now.find((one) => samePlace(one.path, moved));
    // A project again, where it is now, and not remembered where it was.
    expect(located?.gone).toBeNull();
    expect(now.some((one) => samePlace(one.path, mine))).toBe(false);
    // And the row says so: Locate… is offered for nothing that is a project.
    await $(`button[aria-label="Locate ${NAME}"]`).waitForExist({
      timeout: 20_000,
      reverse: true,
      timeoutMsg: "This machine still offers Locate… after the project was located",
    });
  });
});

/** What This machine's rows and any alert or status line say, for a failure's message. */
async function windowSays(): Promise<string> {
  return browser.execute(() =>
    [...document.querySelectorAll<HTMLElement>('.ui-machine-entry,[role="alert"],[role="status"]')]
      .map((one) => one.innerText.trim())
      .filter(Boolean)
      .join(" | "),
  );
}

/** Whether two paths name one place: as written, or as the file system resolves them. */
function samePlace(one: string, other: string): boolean {
  if (one === other) return true;
  const real = (path: string) => (existsSync(path) ? realpathSync(path) : path);
  return real(one) === real(other);
}
