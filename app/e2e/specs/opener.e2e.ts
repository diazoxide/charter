import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { browser, expect } from "@wdio/globals";
import { copyFixturePlane } from "../harness.js";

/**
 * The gate in front of opening a project, in the built app.
 *
 * **What this is for.** `.charter/app/reopen.json` is an execution input — putting a record
 * back STARTS the programs it names, and for a chat that was not on a harness profile what
 * runs is decided from the record alone. A project is a DIRECTORY, and a directory arrives by
 * zip, by shared folder or on a stick as readily as by `git clone`. So "open this folder" must
 * not be able to mean "run what is written in it" (ADR 0035).
 *
 * It is driven through the window's own `invoke` rather than through the opener's buttons, and
 * that is deliberate rather than a shortcut. The opener only draws when the window has no
 * project, and this run's window has one from its launch; closing it would end the chats every
 * spec after this one shares an app process with. What the buttons do is pinned in
 * `src/Opener.test.tsx`; what the CORE does when they are pressed can only be pinned here,
 * against the real store, the real settings reader and the real registry.
 *
 * The window is left exactly as it found it: the second project is closed at the end, and the
 * one the launch opened is never touched.
 */

/** A project of this spec's own, with something worth being asked about in it. */
const stranger = copyFixturePlane();

/** Where the planted record lives, which this spec both writes and reads back. */
const record = join(stranger, ".charter", "app", "reopen.json");

/** What the app answered a command with: its value, or the refusal it gave. */
async function answering(
  command: string,
  args: Record<string, unknown> = {},
): Promise<{ ok?: unknown; trouble?: string }> {
  return browser.executeAsync(
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
}

/** What the app answered, insisting it answered at all. */
async function ask<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const answer = await answering(command, args);
  if (answer.trouble !== undefined) throw new Error(`${command} refused: ${answer.trouble}`);
  return answer.ok as T;
}

/** The refusal a command gave, insisting it refused. */
async function refusal(command: string, args: Record<string, unknown> = {}): Promise<string> {
  const answer = await answering(command, args);
  if (answer.trouble === undefined)
    throw new Error(`${command} answered ${JSON.stringify(answer.ok)} instead of refusing`);
  return answer.trouble;
}

/** What `open_plane` answers with: the project, or the question about it. */
type Opened = {
  plane: string | null;
  ask: {
    path: string;
    contributes: {
      plugins: [string, string][];
      env: [string, string][];
      starts: [string, string][];
      profiles: [string, string][];
    };
    changes: string[];
    first: boolean;
  } | null;
};

/** The question `open_plane` answered with, insisting it asked one. */
async function asking(path: string): Promise<NonNullable<Opened["ask"]>> {
  const answer = await ask<Opened>("open_plane", { path });
  if (answer.ask === null)
    throw new Error(`it opened ${String(answer.plane)} instead of asking about it`);
  return answer.ask;
}

/** Committed settings that choose plugins and set environment in every chat under this
 *  project (`layer.rs`'s WORKSPACE_KEYS). Written from outside the app, which is how a
 *  project that arrived in a tarball would have them. */
function settings(plugins: Record<string, boolean>): void {
  mkdirSync(join(stranger, ".claude"), { recursive: true });
  writeFileSync(
    join(stranger, ".claude", "settings.json"),
    JSON.stringify({ enabledPlugins: plugins, env: { STRANGER_SAYS: "hello" } }),
  );
}

describe("opening a project", () => {
  before(() => {
    settings({ "stranger@market": true });
    // And a record that would start a program the moment the project is put back.
    //
    // **`reopen`'s own on-disk shape, spelled exactly**: `version` and `at` beside the chats,
    // and every value a plain string rather than a null. A record charter cannot parse
    // contributes nothing and starts nothing — fail-closed, and right — so a planted record
    // in the wrong shape would make this spec pass its "nothing ran" half while proving
    // nothing at all about the half that matters.
    mkdirSync(join(stranger, ".charter", "app"), { recursive: true });
    writeFileSync(
      record,
      JSON.stringify({
        version: 1,
        at: 0,
        chats: [
          {
            program: "/bin/echo",
            args: ["planted"],
            cwd: stranger,
            name: "planted",
            resume: "",
            active: true,
            profile: "",
            persona: "",
            footer: "",
          },
        ],
      }),
    );
  });

  it("describes a project nobody has approved instead of opening it", async () => {
    const before = await ask<string[]>("open_planes");

    const question = await asking(stranger);

    expect(question.first).toBe(true);
    // The three things charter can enumerate, all present: a plugin, an environment variable,
    // and the program the record would run.
    expect(question.contributes.plugins.map(([name]) => name)).toEqual(["stranger@market"]);
    expect(question.contributes.env).toEqual([["STRANGER_SAYS", "hello"]]);
    expect(JSON.stringify(question.contributes.starts)).toContain("/bin/echo");
    // **And nothing was attached.** An ask that bound the project's hook socket would leave a
    // cancelled dialog holding a live, empty project in a stranger's directory.
    expect(await ask<string[]>("open_planes")).toEqual(before);
    // The record is exactly as it was planted: nothing ran, and nothing wrote over it.
    expect(readFileSync(record, "utf8")).toContain("planted");
  });

  it("walks up from a subfolder to the project, and refuses a path that is in none", async () => {
    // A picker pointed at `personas/` means the project, and the approval is recorded against
    // the ROOT — which is why the ask carries the root charter resolved rather than the path
    // that was handed in, and why the dialog shows it.
    const inside = await ask<Opened>("open_plane", { path: join(stranger, "personas") });
    expect(inside.ask?.path).toBe((await asking(stranger)).path);

    expect(await refusal("open_plane", { path: "/" })).toContain("charter.toml");
  });

  it("refuses an approval of something other than what was shown", async () => {
    // Between the dialog reading the project and the button being pressed, anything on the
    // machine — including a chat running in another project — can rewrite its settings.
    // Without this check the approval would record whatever was on disk at CLICK time, so the
    // operator could approve, and charter could then start, a program they never read.
    const question = await asking(stranger);

    settings({ "stranger@market": true, "slipped-in@market": true });

    const refused = await refusal("approve_plane", {
      path: question.path,
      contributes: question.contributes,
    });

    expect(refused).toContain("changed while you were reading it");
    // Nothing was written down either, so the next ask is still a first ask.
    expect((await asking(stranger)).first).toBe(true);
  });

  it("opens it on the operator's yes, and then opens it again without asking", async () => {
    const question = await asking(stranger);

    const opened = await ask<string>("approve_plane", {
      path: question.path,
      contributes: question.contributes,
    });

    expect(await ask<string[]>("open_planes")).toContain(opened);
    // The yes is remembered in this machine's store, so the same project opens straight
    // through the next time — and that is what stops the ask becoming a reflex.
    //
    // **Waited for rather than asked once.** Putting the record back starts the chat it
    // names, and charter rewrites the record — and re-fingerprints it — as that chat opens
    // and then ends. Asking in the middle of that is asking about a record charter is in the
    // act of writing, which is a real answer and a flaky test.
    let again: Opened = { plane: null, ask: null };
    await browser.waitUntil(
      async () => {
        again = await ask<Opened>("open_plane", { path: stranger });
        return again.plane !== null;
      },
      { timeoutMsg: "charter kept asking about a project the operator had just approved" },
    );
    expect(again.plane).toBe(opened);
    expect(again.ask).toBe(null);

    // And the window is left as it was found: this spec's project is let go of, the one the
    // launch opened is untouched, and nothing of either goes from disk.
    await ask("close_plane", { plane: opened });
    expect(await ask<string[]>("open_planes")).not.toContain(opened);
  });
});
