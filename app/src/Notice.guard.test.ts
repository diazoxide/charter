import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * **Every standing line in the window is a Notice** (NO-1 #1223, rulings V91b, V91d, V91q).
 *
 * A Notice is a standing line about something true now (CONTEXT.md), and `Notice.tsx` is the
 * one way to draw one: its props will not take a line with no way out. Before it, each line was
 * a `<p className="came-back" role="status">` written by hand, and about thirty of the window's
 * messages left the operator reading a problem with nothing to press. This fails when one is
 * built that way again:
 *
 * - **a hand-built standing line**: any live region (a `status` or `alert` role, one picked by
 *   an expression, or `aria-live`) in any source file under `src/`, whatever its class. The
 *   ones that are not Notices (a dialog's refusal, a live count) are listed in `NOT_NOTICES`,
 *   each with why, by exact count per file, so a new one fails and a removed one is crossed off;
 * - **a hand-built way out**: a `dismiss` or `offer` button class, or a `notice*` class, named
 *   anywhere but `Notice.tsx`, or a stylesheet rule that draws `dismiss` or `offer`.
 *
 * And it **lists the debt** (V91q): every Notice whose only remedy is Copy command, a command to
 * type somewhere else. The list is `COPY_ONLY`, exactly, so adding to it is a visible change.
 *
 * Read as text, as `settings/oldFormClasses.test.ts` (DS-3e) and `paint.test.ts` read theirs:
 * jsdom computes no stylesheet, and a rendered window shows only the lines its test set up.
 */

/*
 * **Why a live region outside a Notice has no link or fix of its own: NO-8's checklist**
 * (#1233, V91n). Each dialog or tab refusal was looked at for a cheap way out, and the entries
 * below say what each got, or which of these reasons it has none for.
 */
/** The refusal of the action just pressed, in the core's sentence, beside the button that
 *  pressed it: pressing again is the retry, with what was typed kept. */
const ACTION =
  "a dialog's or tab's refusal of the action just pressed: an inline error, not a Notice " +
  "(V91n). No cheap link (NO-8): the core's sentence names the cause, and the button retries";
/** A warning a dialog asks over: its own buttons are the way out. */
const WARNING =
  "a dialog's warning about what its button would do: its own Cancel and confirm are the way out";
/** A read refusal in a tab that reads again when the project changes on disk. */
const HEALS =
  "a view tab's read refusal, inside the tab. No link needed (NO-8): the tab reads again " +
  "whenever the project changes on disk, so mending the file clears it";
/** A read refusal read once: Read again is a follow-up on NO-8's list. */
const READ_ONCE =
  "a read refusal, inside its tab or dialog. Read again is NO-8's follow-up: it is read once " +
  "when it opens, and reopening it is the retry today";
/** A vault's read, health and write refusals. */
const VAULT =
  "a vault tab's read, health and write refusals: the provider's own sentence names what to " +
  "do (sign in, unlock). Read again is NO-8's follow-up; a write's refusal is ACTION's case";
/** An extension view's refusals. */
const VIEWS =
  "an extension view's refusals: its Refresh reads again, and the view is the extension's, so " +
  "charter has no fix of its own to offer (NO-8)";

/**
 * **Every live region that is not a Notice, with why. Exact counts per file**: a new one in any
 * file fails until it is a Notice or is listed here with its reason, and a removed one is
 * crossed off. A refusal inside a dialog stays an inline error (V91n), and a live value (a count,
 * progress) is not a standing line.
 */
const NOT_NOTICES: Record<string, { count: number; why: string }> = {
  "About.tsx": {
    count: 1,
    why: READ_ONCE,
  },
  "ActivityTab.tsx": {
    count: 2,
    why:
      "on a question's line (#1496): that the chat to answer in could not be asked for, in the " +
      "core's sentence beside Answer, which is the retry; and that the person's answer was " +
      "sent, said once for whoever cannot see the Answer control go. Neither stands for " +
      "something true now that has a way out of its own",
  },
  "AlertsDrawer.tsx": {
    count: 2,
    why:
      "the Alerts drawer's own read refusals, in place of a project's rows; the rows themselves " +
      "are Notices with their ways out (NO-6)",
  },
  "AnswerQuestion.tsx": {
    count: 2,
    why:
      "the refusal of the person's answer to a task's question (#1496), in the core's sentence " +
      "under the box, with what was typed kept: Send is the retry. And, where the form was " +
      "opened for a task that asks nothing now, the sentence that says so, beside Close",
  },
  "App.tsx": {
    count: 1,
    why: "what the last action answered: replaced by the next action, not about something true now",
  },
  "AskPersona.tsx": {
    count: 1,
    why: ACTION,
  },
  "Brief.tsx": {
    count: 2,
    why:
      READ_ONCE +
      "; and what Copy did in the Brief panel, beside its button: replaced by the next press, " +
      "not about something true now (#1494)",
  },
  "BottomBar.tsx": {
    count: 2,
    why:
      "the repos bar's read refusal and a branch it cannot read: the bottom region has nothing " +
      "to press (ADR 0038), so their way out is the explorer's Read again on the same read (NO-4)",
  },
  "ChangeActions.tsx": {
    count: 4,
    why: ACTION,
  },
  "Cockpit.tsx": {
    count: 1,
    why: "a branch's apart count, said as it changes: a live value, not a line",
  },
  "ChatAsk.tsx": {
    count: 1,
    why: ACTION,
  },
  "ChatsSection.tsx": {
    count: 2,
    why:
      "the Chats list's live count of what its filter hides, and why the key just pressed on " +
      "a row did nothing: a live value and an action's answer, replaced by the next (#1499)",
  },
  "DeleteVault.tsx": {
    count: 2,
    why: ACTION,
  },
  "DeleteWorkspace.tsx": {
    count: 2,
    why: ACTION,
  },
  "Doctor.tsx": {
    count: 2,
    why:
      "the Doctor dialog's own refusal, and what its last Fix changed or why it was refused " +
      "(FX-1): inside the dialog, over the rows it changed",
  },
  "Explorer.tsx": {
    count: 1,
    why:
      "a folder of a branch's tree that could not be read, in the tree where its entries go: a " +
      "row's own refusal, and opening the folder again reads it again (NO-4)",
  },
  "ExtensionAction.tsx": {
    count: 1,
    why: ACTION,
  },
  "FindBar.tsx": { count: 1, why: "the find bar's match count, a live value" },
  "FinishedTasks.tsx": {
    count: 1,
    why:
      ACTION +
      ": a finished task that could not be reopened, on its row beside Reopen. The core's " +
      "sentence says why (its folder is gone, its harness named no conversation), and its " +
      "report is still on the row to read (#1485)",
  },
  "FirstRun.tsx": { count: 2, why: "the first-run page's progress and refusal, inside its tab" },
  "FirstTaskTab.tsx": {
    count: 1,
    why:
      ACTION +
      ". Also its read of the start options, read once when it opens: Read again is NO-8's " +
      "follow-up (#1296)",
  },
  "HarnessSetupTab.tsx": {
    count: 1,
    why: ACTION,
  },
  "KillSwitch.tsx": {
    count: 1,
    why: "the kill switch's refusal, on the title bar control it belongs to",
  },
  "LinkWorkItem.tsx": {
    count: 1,
    why: ACTION,
  },
  "LiveDialog.tsx": {
    count: 2,
    why: ACTION,
  },
  "MemoryArchiveTab.tsx": {
    count: 3,
    why: HEALS,
  },
  "MemoryTab.tsx": {
    count: 4,
    why: HEALS,
  },
  "NewBranch.tsx": {
    count: 1,
    why: ACTION,
  },
  "NewPersona.tsx": {
    count: 1,
    why: ACTION,
  },
  "NewProject.tsx": {
    count: 1,
    why: ACTION,
  },
  "NewVault.tsx": {
    count: 1,
    why: ACTION,
  },
  "NewWorkspace.tsx": {
    count: 1,
    why: ACTION,
  },
  "NotCloned.tsx": {
    count: 1,
    why: "a clone that failed, said on its own row beside that row's Retry (#1215): inline, per repo",
  },
  "Opener.tsx": {
    count: 4,
    why: "the opener's explanation under its heading (three) and its open refusal: the page's own prose",
  },
  "Palette.tsx": {
    count: 7,
    why: "the palette's answers, counts and refusals, inside the palette dialog",
  },
  "Panels.tsx": { count: 3, why: "a panel's read refusal and its blocks' tone, inside the panel" },
  "PersonaProfile.tsx": {
    count: 2,
    why: ACTION + ". Also its read refusal: " + READ_ONCE,
  },
  "ProfileApproval.tsx": {
    count: 1,
    why: WARNING,
  },
  "QuitWarning.tsx": {
    count: 1,
    why: WARNING,
  },
  "RemoveFromWorkspace.tsx": {
    count: 1,
    why: ACTION,
  },
  "RemovePersona.tsx": {
    count: 1,
    why: ACTION,
  },
  "RenameWorkspace.tsx": {
    count: 1,
    why: ACTION,
  },
  "RepoInstructionsTab.tsx": {
    count: 2,
    why:
      ACTION +
      '. Also its "Added N files to memory": what the last press answered, not a standing line',
  },
  "RepoPicker.tsx": {
    count: 2,
    why:
      "the repo picker's refusals, inside the dialog that holds it. NO-8: a forge whose CLI " +
      "is not logged in offers its login, typed in a shell tab; the rest are the forge's sentence",
  },
  "SandboxOffer.tsx": { count: 1, why: "the answer's refusal, inside the sandbox offer's Notice" },
  "SavingView.tsx": {
    count: 3,
    why:
      ACTION +
      ". Also a save's own output, line by line: what the last press answered, not a standing line",
  },
  "SearchTab.tsx": { count: 1, why: "the search's progress, a live value" },
  "SessionRecordTab.tsx": {
    count: 1,
    why: READ_ONCE,
  },
  "StartChat.tsx": {
    count: 3,
    why:
      "a dialog's refusal: an inline error, not a Notice (V91n). NO-8: the local file git " +
      "would carry offers the doctor's local-ignore fix, refused profiles link to Settings › " +
      "Harness, a sandbox it cannot apply offers its install, and a start's refusal is ACTION's",
  },
  "TabRename.tsx": { count: 1, why: "a rename's refusal, beside the box being typed in" },
  "TaskEnd.tsx": {
    count: 2,
    why: ACTION + ". Twice: in the one modal question, and in the second step asked in place",
  },
  "TodoTab.tsx": {
    count: 1,
    why: HEALS,
  },
  "Updates.tsx": {
    count: 4,
    why:
      ACTION +
      ". Also its warning that installing ends every chat (WARNING's case), and the channel " +
      "it moved to: what the last press answered, not a standing line",
  },
  "VaultSignIn.tsx": {
    count: 5,
    why:
      ACTION +
      ". The set-up of a 1Password vault's sign-in (#1527): why the app's accounts could not " +
      "be listed, a sign-in refused as its vaults were listed, what the test just pressed " +
      "answered (passed, or the core's reason), and the refusal of the step just pressed",
  },
  "VaultTab.tsx": {
    count: 7,
    why: VAULT,
  },
  "Vaults.tsx": {
    count: 1,
    why: READ_ONCE,
  },
  "Views.tsx": {
    count: 5,
    why: VIEWS,
  },
  "editor/BranchTree.tsx": { count: 1, why: "the branch tree's read refusal, inside the editor" },
  "editor/PieceFiles.tsx": { count: 1, why: "the piece files' read refusal, inside the editor" },
  "references.tsx": {
    count: 1,
    why: "the reference picker saying it found no chat, inside its menu",
  },
  "settings/Collection.tsx": {
    count: 3,
    why:
      "a collection's Undo answer, a refused Remove with its users and a refused Add (ST-3): " +
      "inline, inside Settings, beside the entry or form they are about",
  },
  "settings/RawToml.tsx": {
    count: 2,
    why: "Edit as TOML's own answer and refusal, inside Settings (NO-7, #1232)",
  },
  "settings/SettingsTab.tsx": {
    count: 1,
    why: "Settings' read refusal, inside its tab (NO-7, #1232)",
  },
  "settings/components.tsx": {
    count: 3,
    why: "the settings set's filter count, a row's refusal and a range's value: inline, per field",
  },
  "settings/thisMachine.tsx": {
    count: 1,
    why: "This machine's read refusal, standing in for its list inside Settings (ST-2; NO-7, #1232)",
  },
};

/**
 * Notices whose only way out is Copy command (V91q's debt): `file: cause`, each with why the
 * window has no fix of its own.
 *
 * - `slow-start` (NO-4): the relaunch without the session bus. The launch is already made, and
 *   the window cannot start charter again in another environment for the operator.
 */
const COPY_ONLY: string[] = ["App.tsx: slow-start"];

const SRC = join(process.cwd(), "src");

function files(dir: string, keep: (name: string) => boolean): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return files(path, keep);
    return keep(entry.name) ? [path] : [];
  });
}

/** Every source file the window is drawn from: not tests, which name the classes to deny them. */
const sources = () =>
  files(SRC, (name) => /\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name)).map((path) => ({
    name: relative(SRC, path),
    text: readFileSync(path, "utf8"),
  }));

/**
 * Every JSX opening tag in `source` whose name `named` takes, as its tag name and its attribute
 * text. Braces are counted, so an arrow's `>` inside `onClick={() => …}` does not end the tag.
 */
function tags(source: string, named: (tag: string) => boolean): { tag: string; attrs: string }[] {
  const found: { tag: string; attrs: string }[] = [];
  for (const start of source.matchAll(/<([A-Za-z][\w.]*)(?=[\s>/])/g)) {
    const tag = start[1];
    if (!named(tag)) continue;
    let depth = 0;
    let at = (start.index ?? 0) + start[0].length;
    for (; at < source.length; at++) {
      const char = source[at];
      if (char === "{") depth++;
      else if (char === "}") depth--;
      else if (char === ">" && depth === 0) break;
    }
    found.push({ tag, attrs: source.slice((start.index ?? 0) + start[0].length, at) });
  }
  return found;
}

const html = (tag: string) => /^[a-z]/.test(tag);
/** A live region: a `status` or `alert` role (or one picked by an expression), or `aria-live`. */
const live = (attrs: string) =>
  /\brole=(?:"(?:status|alert)"|\{)/.test(attrs) || /\baria-live=/.test(attrs);

/** The class names an attribute text's `className` can hold: every string in its value. */
function classNames(attrs: string): string[] {
  const value = /className=(?:"([^"]*)"|\{([\s\S]*)\})/.exec(attrs);
  if (!value) return [];
  const strings =
    value[1] !== undefined
      ? [value[1]]
      : [...value[2].matchAll(/"([^"]*)"|'([^']*)'|`([^`]*)`/g)].map(([, a, b, c]) =>
          (a ?? b ?? c ?? "").replace(/\$\{[^}]*\}/g, " "),
        );
  return strings.flatMap((one) => one.split(/\s+/)).filter(Boolean);
}

/** The classes only `Notice.tsx` may name: its look and its ways out. */
const noticeOnly = (name: string) =>
  name === "dismiss" || name === "offer" || name === "notice" || name.startsWith("notice-");

/**
 * The Notices in `source` whose only remedy is Copy command, by cause: a `copy` with no fix and
 * no link. A Dismiss beside it does not count, because it hides the problem and fixes nothing.
 */
function copyOnly(source: string): string[] {
  return tags(source, (tag) => tag === "Notice")
    .filter(({ attrs }) => /\bcopy=/.test(attrs) && !/\b(?:fixes|link)=/.test(attrs))
    .map(
      ({ attrs }) =>
        /cause=(?:"([^"]*)"|\{`([^`]*)`\}|\{([^}]*)\})/.exec(attrs)?.slice(1).find(Boolean) ?? "?",
    );
}

/** The Notices in `source` built with a spread (`{...props}`), by cause. */
function spreadNotices(source: string): string[] {
  return tags(source, (tag) => tag === "Notice")
    .filter(({ attrs }) => /\{\s*\.\.\./.test(attrs))
    .map(
      ({ attrs }) =>
        /cause=(?:"([^"]*)"|\{`([^`]*)`\}|\{([^}]*)\})/.exec(attrs)?.slice(1).find(Boolean) ?? "?",
    );
}

describe("the window's standing lines", () => {
  it("are all Notices, but for the live regions listed with why", () => {
    const counted: Record<string, number> = {};
    for (const { name, text } of sources()) {
      if (name === "Notice.tsx") continue;
      const n = tags(text, (tag) => tag !== "Notice").filter(({ attrs }) => live(attrs)).length;
      if (n > 0) counted[name] = n;
    }
    const listed = Object.fromEntries(
      Object.entries(NOT_NOTICES).map(([name, { count }]) => [name, count]),
    );
    expect(counted).toEqual(listed);
  });

  it("have their ways out drawn by Notice alone", () => {
    const named = sources()
      .filter(({ name }) => name !== "Notice.tsx")
      .flatMap(({ name, text }) =>
        tags(text, html)
          .flatMap(({ attrs }) => classNames(attrs))
          .filter(noticeOnly)
          .map((one) => `${name}: ${one}`),
      );
    expect(named).toEqual([]);

    const rules = files(SRC, (name) => name.endsWith(".css")).flatMap((path) =>
      [
        ...readFileSync(path, "utf8")
          .replace(/\/\*[\s\S]*?\*\//g, "")
          .matchAll(/([^{}]+)\{/g),
      ]
        .map(([, selectors]) => selectors.trim())
        .filter((selectors) => /\.(?:dismiss|offer)\b/.test(selectors))
        .map((selectors) => `${relative(SRC, path)}: ${selectors}`),
    );
    expect(rules).toEqual([]);
  });

  it("name their ways out on the Notice, never spread into it", () => {
    // The debt list reads a Notice's own attributes, so one whose ways come in a spread could
    // be copy-only and never be listed (NO-6).
    const spread = sources().flatMap(({ name, text }) =>
      spreadNotices(text).map((cause) => `${name}: ${cause}`),
    );
    expect(spread).toEqual([]);
  });

  it("whose only way out is Copy command are listed as debt", () => {
    const debt = sources().flatMap(({ name, text }) =>
      copyOnly(text).map((cause) => `${name}: ${cause}`),
    );
    expect(debt).toEqual(COPY_ONLY);
  });

  it("would be caught if one came back", () => {
    // The readers above are what the guard rests on, so they are held to what a screen writes.
    const lines = (source: string) =>
      tags(source, (tag) => tag !== "Notice").filter(({ attrs }) => live(attrs));
    expect(lines(`<p className="came-back" role="status">Gone.</p>`)).toHaveLength(1);
    // The reviewer's injection into SessionBusNotice.tsx: any class, a live role.
    expect(lines(`<p className="warning" role="status">The bus is gone.</p>`)).toHaveLength(1);
    expect(lines(`<p className={bad ? "trouble" : "came-back"} role={r}>x</p>`)).toHaveLength(1);
    expect(lines(`<span aria-live="polite">3 found</span>`)).toHaveLength(1);
    expect(
      lines(`<AlertDialog.Description role="alert">No.</AlertDialog.Description>`),
    ).toHaveLength(1);
    // A muted sentence with no live role is not a standing line, and a Notice is the way to draw one.
    expect(lines(`<p className="came-back">Prose.</p>`)).toHaveLength(0);
    expect(lines(`<Notice cause="x" onDismiss={hide}>Gone.</Notice>`)).toHaveLength(0);
    const arrow = `<button className="dismiss" onClick={() => go(a > b)}>Dismiss</button>`;
    expect(tags(arrow, html).flatMap(({ attrs }) => classNames(attrs))).toEqual(["dismiss"]);
    expect(
      copyOnly(`<Notice cause="slow" copy="env X=1 charter" onDismiss={() => hide()}>x</Notice>`),
    ).toEqual(["slow"]);
    expect(
      copyOnly(`<Notice cause="slow" copy="env X=1 charter" fixes={[retry]}>x</Notice>`),
    ).toEqual([]);
    expect(copyOnly("<Notice cause={`gone:${name}`} copy={command}>x</Notice>")).toEqual([
      "gone:${name}",
    ]);
    expect(spreadNotices(`<Notice {...ways} cause="alert:x" at="drawer">x</Notice>`)).toEqual([
      "alert:x",
    ]);
    expect(spreadNotices(`<Notice cause="x" link={go}>x</Notice>`)).toEqual([]);
  });
});
