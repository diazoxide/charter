/// <reference types="vite/client" />
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { copyFaults } from "./copy";
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
  });

  it("finds 'please' in words a person reads, not in an id or a code span (#1156)", () => {
    expect(copyFaults("Pick a folder, please.")).toHaveLength(1);
    expect(copyFaults("ask.please")).toEqual([]);
    expect(copyFaults("hooks/please-hold.sh")).toEqual([]);
    expect(copyFaults("Run `please --now` to see it.", "shown")).toEqual([]);
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

describe("every string in app/src", () => {
  it("reads the source tree", () => {
    // A glob that matched nothing would make the next case pass by having nothing to fail.
    expect(SOURCES.length).toBeGreaterThan(100);
  });

  it("follows the copy guide's mechanical rules", () => {
    const faults = SOURCES.flatMap((path) =>
      uiStrings(readFileSync(join(process.cwd(), path), "utf8")).flatMap(({ text, seen, line }) =>
        copyFaults(text, seen).map((fault) => `${path}:${line} ${JSON.stringify(text)}: ${fault}`),
      ),
    );
    expect(faults).toEqual([]);
  });
});
