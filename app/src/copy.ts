/**
 * **The copy guide's rules a machine can hold** (`docs/ui-copy.md`, DS-2). The guide has more
 * rules than these; the rest need a reader, and DS-8's audit is that reader. These two are here
 * because each is cheap to check, has no false alarm in the window's copy as it stands, and is
 * the first thing a hurried change gets wrong:
 *
 * - **No stock phrase.** "Something went wrong" says that nothing was found out. A message names
 *   what happened, so a phrase that fits every failure is refused wherever it is written.
 * - **Sentence case on what the window shows.** "Open project…", never "Open Project…". And
 *   charter is lowercase, but for the About dialog's title.
 *
 * `copy.test.ts` runs both over every string in `app/src` (`uiStrings.ts` finds them).
 */

/** Where a string was found: drawn in the window, or anywhere else in the source. */
export type Seen = "shown" | "source";

const STOCK =
  /\b(something went wrong|an error (has )?occurred|unknown error|oops|please|successfully)\b|^error:/i;

/**
 * Names keep their own capitals, so they are taken out before case is judged. A name missing
 * here fails the guard on its first label; adding it is the fix, and it is the only list.
 */
const NAMES =
  /\b(Claude Code|Codex|opencode|GitHub|GitLab|Keychain|Touch ID|Windows Hello|LM Studio|Ollama|Finder|File Explorer|Files|macOS|Linux|Windows)\b/g;

/** A key chord — `Ctrl+Shift+F`, `CmdOrCtrl+W` — is keys, not words, so it has no case. */
const CHORD = /\b[A-Za-z]+(\+[A-Za-z0-9,.]+)+/g;

/**
 * **purlis is lowercase** everywhere, the About dialog's title too (V93a), so a capital
 * Charter or Purlis is a fault of its own. The title is still a label of its own.
 */
const ABOUT = "About purlis";

/** The labels a string holds: `·` and `—` split "Stopped · Re-arm" into a state and an action. */
function labels(text: string): string[] {
  return text.split(/\s[·—]\s/);
}

/**
 * A label is in title case when every word of four letters or more starts with a capital and
 * there are at least two of them. Short words are skipped because title case leaves some of
 * them small ("in", "to", "a"). A label with one long word cannot be told apart, so it passes.
 */
function titleCased(text: string): boolean {
  return labels(text).some((label) => {
    if (label.trim() === ABOUT) return false;
    const words = label
      .replace(CHORD, " ")
      .replace(NAMES, " ")
      // A capital Charter or Purlis is its own fault, below, and is said once.
      .replace(/\b(Charter|Purlis)\b/g, " ")
      .replace(/[^A-Za-z'-]+/g, " ")
      .split(" ")
      .filter((word) => word.length >= 4);
    return words.length >= 2 && words.every((word) => /^[A-Z][a-z]/.test(word));
  });
}

/** What is wrong with `text` by the guide's mechanical rules; empty when nothing is. */
export function copyFaults(text: string, seen: Seen = "source"): string[] {
  const faults: string[] = [];
  const said = text.trim();
  if (STOCK.test(said)) faults.push("a stock phrase: say what happened instead");
  if (seen === "shown") {
    if (/[A-Za-z]!$/.test(said)) faults.push("an exclamation mark: say it plainly");
    if (titleCased(said)) faults.push("title case: write labels in sentence case");
    if (/\b(Charter|Purlis)\b/.test(said))
      faults.push("a capital product name: purlis is lowercase, the About title too");
  }
  return faults;
}
