/// <reference types="vite/client" />
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { copyFaults, retiredTerms } from "./copy";
import { OUTSIDE_THE_FIRST_HOUR } from "./firstHour";
import { uiStrings } from "./uiStrings";

describe("the copy guide's mechanical rules (docs/ui-copy.md)", () => {
  it("refuses an error that says nothing about what happened", () => {
    expect(copyFaults("Something went wrong")).toEqual([
      "a stock phrase: say what happened instead",
    ]);
    expect(copyFaults("An error occurred while saving")).toHaveLength(1);
    expect(copyFaults("Unknown error")).toHaveLength(1);
    expect(copyFaults("Oops, that did not work")).toHaveLength(1);
    expect(copyFaults("Error: the vault is locked")).toHaveLength(1);
  });

  it("refuses filler that makes a message longer and no clearer", () => {
    expect(copyFaults("Please pick a folder")).toEqual([
      "a stock phrase: say what happened instead",
    ]);
    expect(copyFaults("Saved successfully")).toHaveLength(1);
  });

  it("passes copy that says what happened and what to do", () => {
    expect(copyFaults("purlis could not read the alerts: the file is gone")).toEqual([]);
    expect(copyFaults("No vaults yet. Make one with New vault… in the palette.")).toEqual([]);
    // A word that only contains a stock one is not it.
    expect(copyFaults("The errors pane is empty")).toEqual([]);
    expect(copyFaults("pleased")).toEqual([]);
  });

  it("refuses an exclamation mark only where the window shows the string", () => {
    expect(copyFaults("Saved!", "shown")).toEqual(["an exclamation mark: say it plainly"]);
    // Outside the window's text a `!` is an id or a glyph, not a tone of voice.
    expect(copyFaults("curate.workspace!")).toEqual([]);
    expect(copyFaults("!", "shown")).toEqual([]);
  });

  it("refuses a label in title case", () => {
    expect(copyFaults("Open Project…", "shown")).toEqual([
      "title case: write labels in sentence case",
    ]);
    expect(copyFaults("Create New Workspace", "shown")).toHaveLength(1);
  });

  it("passes sentence case, product names and a control named mid-sentence", () => {
    expect(copyFaults("Open project…", "shown")).toEqual([]);
    expect(copyFaults("Create workspace", "shown")).toEqual([]);
    // Names keep their own capitals.
    expect(copyFaults("Sign in to GitHub", "shown")).toEqual([]);
    expect(copyFaults("Claude Code", "shown")).toEqual([]);
    // A control named in a sentence is written as its label reads.
    expect(copyFaults("start one and press Check again.", "shown")).toEqual([]);
    // A state and an action side by side are two labels, each in sentence case.
    expect(copyFaults("Stopped · Re-arm", "shown")).toEqual([]);
    // One word is no case at all.
    expect(copyFaults("Delete", "shown")).toEqual([]);
  });

  it("takes the names of other programs' places as names", () => {
    expect(copyFaults("Reveal in File Explorer", "shown")).toEqual([]);
    expect(copyFaults("Show in Files", "shown")).toEqual([]);
  });

  it("reads a key chord as a key, not as words", () => {
    expect(copyFaults("Find (Ctrl+Shift+F)", "shown")).toEqual([]);
    expect(copyFaults("Close (CmdOrCtrl+Option+W)", "shown")).toEqual([]);
    expect(copyFaults("Back to the whole workspace (Esc)", "shown")).toEqual([]);
  });

  it("allows no capital product name, not even in the About dialog's title", () => {
    expect(copyFaults("About purlis", "shown")).toEqual([]);
    expect(copyFaults("About purlis — what this version brought", "shown")).toEqual([]);
    expect(copyFaults("This is Purlis 0.4.0.", "shown")).toEqual([
      "a capital product name: purlis is lowercase, the About title too",
    ]);
    expect(copyFaults("About Charter", "shown")).toEqual([
      "a capital product name: purlis is lowercase, the About title too",
    ]);
    expect(copyFaults("This is purlis 0.4.0.", "shown")).toEqual([]);
    // The source is not the window: a type or a component may be called Charter.
    expect(copyFaults("AboutCharter")).toEqual([]);
  });

  it("leaves case alone outside the window's text", () => {
    expect(copyFaults("Open Project")).toEqual([]);
  });

  it("refuses an exclamation mark that ends any sentence, not only the last (#1156)", () => {
    expect(copyFaults("Saved! Open it from the list.", "shown")).toEqual([
      "an exclamation mark: say it plainly",
    ]);
    expect(copyFaults("Done!) Nothing else to do.", "shown")).toHaveLength(1);
    // Said once however many there are.
    expect(copyFaults("Saved! It is in the list!", "shown")).toHaveLength(1);
    // A `!` inside a word or a code span is not a tone of voice.
    expect(copyFaults("Run `git log -1 !main` to see it.", "shown")).toEqual([]);
    expect(copyFaults("The value is a!=b in the file.", "shown")).toEqual([]);
  });

  it("refuses a word shouted in capitals in what the window shows (#1156)", () => {
    expect(copyFaults("Never close this window while it saves", "shown")).toEqual([]);
    expect(copyFaults("NEVER close this window while it saves", "shown")).toEqual([
      "capitals: write the word in lowercase, or name it",
    ]);
    expect(copyFaults("WARNING", "shown")).toHaveLength(1);
    // Acronyms, names and chords keep their capitals.
    expect(copyFaults("Open the JSON file", "shown")).toEqual([]);
    expect(copyFaults("Open a PR on GitHub", "shown")).toEqual([]);
    expect(copyFaults("Read the README first.", "shown")).toEqual([]);
    expect(copyFaults("This app's PATH", "shown")).toEqual([]);
    // ADR 0072's labels for a workspace's save mode are written in capitals.
    expect(copyFaults("the workspace is LIVE", "shown")).toEqual([]);
    // A file's name is a name: `AGENTS.md` is what is on disk.
    expect(copyFaults("Move the AGENTS.md aside", "shown")).toEqual([]);
    expect(copyFaults("Find (Ctrl+Shift+F)", "shown")).toEqual([]);
    // A code span or a placeholder is what the person types, not a word.
    expect(copyFaults("Set `TMPDIR` and try again.", "shown")).toEqual([]);
    // Outside the window, an all-caps word is a constant.
    expect(copyFaults("NOT_FOUND")).toEqual([]);
    // An assignment is typed as written, and a product keeps its own capitals.
    expect(copyFaults("NAME=value, one per line.", "shown")).toEqual([]);
    expect(copyFaults("IntelliJ IDEA, or another JetBrains IDE", "shown")).toEqual([]);
  });

  it("finds 'please' in words a person reads, not in an id or a code span (#1156)", () => {
    expect(copyFaults("Pick a folder, please.")).toHaveLength(1);
    expect(copyFaults("ask.please")).toEqual([]);
    expect(copyFaults("hooks/please-hold.sh")).toEqual([]);
    expect(copyFaults("Run `please --now` to see it.", "shown")).toEqual([]);
  });
});

describe("no retired term on what the window shows (FR-3, ADR 0072, #602)", () => {
  const PLANE = "a retired term: say project, not plane";
  const WORKTREE = "a retired term: say branch or folder, not worktree";
  const PIECE = "a retired term: say branch or folder, not piece";
  const SYNC = "a retired term: sync is purlis sync's alone";

  it("refuses plane, worktree and piece in the window's words", () => {
    expect(retiredTerms("Reading the plane…", "shown")).toEqual([PLANE]);
    expect(retiredTerms("This plane's owners", "shown")).toEqual([PLANE]);
    expect(retiredTerms("Plane updated since this chat started", "shown")).toEqual([PLANE]);
    expect(retiredTerms("No planes open", "shown")).toEqual([PLANE]);
    expect(retiredTerms("Remove the worktree", "shown")).toEqual([WORKTREE]);
    expect(retiredTerms("Clones and worktrees", "shown")).toEqual([WORKTREE]);
    expect(retiredTerms("pieces", "shown")).toEqual([PIECE]);
    // Each term is said once, and two terms are two faults.
    expect(retiredTerms("The plane holds a plane", "shown")).toEqual([PLANE]);
    expect(retiredTerms("Every worktree in the plane", "shown")).toEqual([PLANE, WORKTREE]);
  });

  it("refuses Sync for anything but purlis sync", () => {
    expect(retiredTerms("Sync settings across devices", "shown")).toEqual([SYNC]);
    expect(retiredTerms("Saved and synced", "shown")).toEqual([SYNC]);
    expect(retiredTerms("Syncing…", "shown")).toEqual([SYNC]);
    // The command itself, and the window's own name for it.
    expect(retiredTerms("Run purlis sync to fetch every clone", "shown")).toEqual([]);
    expect(retiredTerms("charter sync fetches every clone", "shown")).toEqual([]);
    expect(retiredTerms("Sync repos", "shown")).toEqual([]);
  });

  it("passes the words that replace them, and English that only looks like them", () => {
    expect(retiredTerms("Reading the project…", "shown")).toEqual([]);
    expect(retiredTerms("Remove folder", "shown")).toEqual([]);
    // "A piece of work" is English, not the format's piece.
    expect(retiredTerms("A named piece of work", "shown")).toEqual([]);
    // A word that only contains one is not it.
    expect(retiredTerms("Planed and planet", "shown")).toEqual([]);
    expect(retiredTerms("asynchronous", "shown")).toEqual([]);
  });

  it("leaves code spans, ids and the source alone", () => {
    // What a person types, and what the code or the format calls a thing.
    expect(retiredTerms("Run `purlis worktree list` to see them.", "shown")).toEqual([]);
    expect(retiredTerms("Set `[plane] worktrees` in the file.", "shown")).toEqual([]);
    expect(retiredTerms("plane-updated fresh-mark", "shown")).toEqual([]);
    expect(retiredTerms("workspaces/alpha/.worktrees/svc", "shown")).toEqual([]);
    // Outside the window, plane is still the code's word until the rename lands.
    expect(retiredTerms("purlis found no plane")).toEqual([]);
  });

  it("is a rule of its own, so the other rules' guards carry no debt for it", () => {
    expect(copyFaults("Reading the plane…", "shown")).toEqual([]);
  });
});

describe("the strings the guard reads", () => {
  it("finds what the window shows: text, and the attributes a person reads or hears", () => {
    const found = uiStrings(
      `const a = <button aria-label="Close find" title={\`Save \${repo}\`} className="x-y">
         Open project…
       </button>;
       const b = "purlis could not read it";
       import { x } from "./not-copy";`,
    );
    expect(found).toEqual([
      { text: "Close find", seen: "shown", line: 1 },
      { text: "Save …", seen: "shown", line: 1 },
      // Every other string is source: copy built in code, and ids and classes beside it.
      { text: "x-y", seen: "source", line: 1 },
      { text: "Open project…", seen: "shown", line: 2 },
      { text: "purlis could not read it", seen: "source", line: 4 },
    ]);
  });
});

describe("shown text reached through an expression", () => {
  const shown = (source: string) =>
    uiStrings(source)
      .filter((one) => one.seen === "shown")
      .map((one) => one.text);

  it("reads a JSX child's literals, through a conditional and a logical branch", () => {
    expect(
      shown(`<p>{busy ? "Reading it…" : \`Read \${n}\`}{error && "purlis could not read it"}</p>`),
    ).toEqual(["Reading it…", "Read …", "purlis could not read it"]);
  });

  it("reads a shown attribute's conditional branches, and not its condition", () => {
    expect(shown(`<b title={mode === "on" ? "Sandbox on" : "Sandbox off"} />`)).toEqual([
      "Sandbox on",
      "Sandbox off",
    ]);
  });

  it("reads an offer's title and reason, and not its id", () => {
    expect(
      shown(
        `can("chat.new", "New chat…", does); cannot("vault.new", "New vault…", "No project is open.");`,
      ),
    ).toEqual(["New chat…", "New vault…", "No project is open."]);
  });

  it("reads a call to a function named like an Object member as an ordinary call", () => {
    expect(
      uiStrings(`toString("Read it"); valueOf("x"); constructor("y");`).map(
        ({ text, seen }) => `${seen}: ${text}`,
      ),
    ).toEqual(["source: Read it", "source: x", "source: y"]);
  });

  it("reads a label property as shown", () => {
    expect(shown(`const row = { id: "x", label: "Open in its pane" };`)).toEqual([
      "Open in its pane",
    ]);
  });

  it("reads the properties and attributes that carry a setting's or a row's words as shown", () => {
    // A settings group's title and note, a field's hint and help, a provider's line, the
    // sandbox table's rows and an empty state's: each is drawn as written (#602).
    expect(
      shown(
        `const g = { id: "g", title: "Plane", note: "How it is saved.", hint: "Under the project." };
         const f = { help: "Where folders go.", says: "A plaintext file.", what: "Clones", why: "It asks." };
         const e = { headline: "Nothing yet", body: "Make one." };`,
      ),
    ).toEqual([
      "Plane",
      "How it is saved.",
      "Under the project.",
      "Where folders go.",
      "A plaintext file.",
      "Clones",
      "It asks.",
      "Nothing yet",
      "Make one.",
    ]);
    expect(shown(`<SettingRow label="Folder" help="It is made for you." hint="A path" />`)).toEqual(
      ["Folder", "It is made for you.", "A path"],
    );
  });

  it("reads a settings control's label, hint and unset value, and not its key", () => {
    expect(
      shown(
        `textAt(key("plane", "worktrees"), "Branch folders");
         listAt(key("chat_env", "pass"), "Environment passed", "One per line.");
         pickAt(key("harness", "default"), "Default profile", "profile", "Where it starts.");
         onOffAt(key("x", "y"), "Hold it", "When on, it holds.", "not set");`,
      ),
    ).toEqual([
      "Branch folders",
      "Environment passed",
      "One per line.",
      "Default profile",
      "Where it starts.",
      "Hold it",
      "When on, it holds.",
      "not set",
    ]);
  });
});

/**
 * Every file the window is built from, as text — the same reading `theme/literals.test.ts`
 * does, for the same reason: a file added to `src` is covered without anybody listing it.
 * Tests are left out (they quote copy to find it) and so are the generated bindings (their
 * strings are the Rust side's names).
 */
const SOURCES = Object.keys({
  ...import.meta.glob("./**/*.ts"),
  ...import.meta.glob("./**/*.tsx"),
})
  .filter((path) => !/\.test\.tsx?$|^\.\/(bindings|uiRpc)\.ts$/.test(path))
  .map((path) => `src/${path.slice(2)}`)
  .sort();

/** Why a retired term is still in a file: the file is another branch's while it is being built. */
const TRAIN_27 = "train 27 (impl/qw76-qw79) is rebuilding this dialog";
const CC_DA = "CC and DA (impl/qw72, qw75) hold this file, and EE after them";

/**
 * **The retired terms still in the window, and why** (#602): `file: "the string"`, one entry
 * per string, exactly, so paying one off or adding one is a visible change — the way
 * `Notice.guard.test.ts` keeps its `COPY_ONLY`. Each is in a file another branch holds while
 * this rule landed, so it is fixed by that file's next owner, never by widening the rule. The
 * reason is beside each, to be read at that change.
 */
const RETIRED_TERM_DEBT: readonly (readonly [string, string])[] = [
  [
    "src/DeleteWorkspace.tsx: and everything in it: every repo cloned there, every worktree cut in it, its memory and its todos. There is no undo.",
    TRAIN_27,
  ],
  ["src/Explorer.tsx: Reading the plane…", CC_DA],
  [
    "src/NewProject.tsx: Make this repo itself the plane",
    "the label is free, but Modals.keyboard.test.tsx (train 27) finds the box by it",
  ],
  [
    "src/Panels.tsx: The plane root is not a workspace: it has no todos or memory of its own. Chats here look after the plane and its workspaces; focus a workspace to see its panels.",
    CC_DA,
  ],
  ["src/Panels.tsx: Reading the plane…", CC_DA],
  ["src/SavingView.tsx: Reading the plane", TRAIN_27],
  [
    "src/StartChat.tsx: This plane declares no profiles of its own, so these are purlis&apos;s built-ins. Declare your own in",
    TRAIN_27,
  ],
  ["src/Updates.tsx: The plane&apos;s pin", TRAIN_27],
  ["src/Vaults.tsx: No vaults on this plane", TRAIN_27],
  ["src/Views.tsx: Reading the plane…", CC_DA],
  [
    "src/actions.ts: Makes a plane in a directory of its own. It never writes into a repo you point at.",
    CC_DA,
  ],
  ["src/actions.ts: New chat at the plane root", CC_DA],
  ["src/actions.ts: New chat at the plane root", CC_DA],
  [
    "src/actions.ts: In no workspace: it looks after the plane and names a workspace with -w.",
    CC_DA,
  ],
  ["src/actions.ts: New shell at the plane root", CC_DA],
  ["src/actions.ts: New shell at the plane root", CC_DA],
  ["src/actions.ts: purlis found no plane, so there is nowhere to make a workspace.", CC_DA],
  ["src/actions.ts: What every persona on this plane reads, in a tab of its own.", CC_DA],
  ["src/actions.ts: This plane has no vaults yet. New vault… makes one.", CC_DA],
  ["src/actions.ts: purlis found no plane, so it cannot reach a branch.", CC_DA],
];

/** Every fault `rule` finds in `app/src`, with where it is and the string itself. */
const FAULTS = (rule: typeof copyFaults) =>
  SOURCES.flatMap((path) =>
    uiStrings(readFileSync(join(process.cwd(), path), "utf8")).flatMap(({ text, seen, line }) =>
      rule(text, seen).map((fault) => ({ path, line, text, fault })),
    ),
  );

describe("every string in app/src", () => {
  it("reads the source tree", () => {
    // A glob that matched nothing would make the next case pass by having nothing to fail.
    expect(SOURCES.length).toBeGreaterThan(100);
  });

  it("follows the copy guide's mechanical rules", () => {
    const faults = FAULTS(copyFaults).map(
      ({ path, line, text, fault }) => `${path}:${line} ${JSON.stringify(text)}: ${fault}`,
    );
    expect(faults).toEqual([]);
  });

  it("says no retired term but the ones listed as debt (#602)", () => {
    const retired = FAULTS(retiredTerms).map(({ path, text }) => `${path}: ${text}`);
    expect(retired).toEqual(RETIRED_TERM_DEBT.map(([where]) => where));
  });

  it("would refuse a retired term added to a label (#602)", () => {
    // The mutation the rule is for: one word, in the place a hurried change puts it.
    const added = uiStrings(`const row = { id: "plane.open", label: "Open the plane…" };`);
    expect(added.flatMap(({ text, seen }) => retiredTerms(text, seen))).toEqual([
      "a retired term: say project, not plane",
    ]);
  });

  it("keeps the retired terms among the first hour's (ADR 0072 §3)", () => {
    // FR-3's three words are retired everywhere on screen, so the first hour never says them
    // either: one family of lists, not two that drift.
    for (const term of ["plane", "worktree", "piece"])
      expect(OUTSIDE_THE_FIRST_HOUR).toContain(term);
  });
});
