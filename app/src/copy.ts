/**
 * **The copy guide's rules a machine can hold** (`docs/ui-copy.md`, DS-2). The guide has more
 * rules than these; the rest need a reader, and DS-8's audit is that reader. These four are here
 * because each is cheap to check, has no false alarm in the window's copy as it stands, and is
 * the first thing a hurried change gets wrong:
 *
 * - **No stock phrase.** "Something went wrong" says that nothing was found out. A message names
 *   what happened, so a phrase that fits every failure is refused wherever it is written.
 * - **Sentence case on what the window shows.** "Open project…", never "Open Project…". And
 *   charter is lowercase, but for the About dialog's title.
 * - **No raised voice on what the window shows** (#1156): no sentence ends in "!", and no word
 *   of four letters or more is written in capitals ("NEVER close"). Acronyms keep theirs.
 * - **No retired term on what the window shows** (FR-3, ADR 0072, #602): "plane" is a project,
 *   a "worktree" or "piece" is a branch or its folder, and "sync" is only ever `purlis sync`'s
 *   (*Sync repos* in the window). These four are the glossary's _Avoid_ words a machine can
 *   hold without a false alarm; "clone" and "session" are English as often as they are terms,
 *   so they need a reader (DS-8). The code and the format still say plane until the rename
 *   lands, so the rule reads only what the window shows.
 *
 * A code span (`` `git log` ``) and an id (`ask.please`, `hooks/please-hold.sh`) are what a
 * person types or what the code calls something, not words, so none of the rules reads them.
 *
 * `copy.test.ts` runs them over every string in `app/src`, and `copy.rust.test.ts` over the
 * window's copy written in Rust. The retired terms are `retiredTerms`, a function of their own:
 * the ones still in files other work holds are `copy.test.ts`'s debt list, exactly, so paying
 * one off or adding one is a visible change.
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
  /\b(Claude Code|Codex|opencode|GitHub|GitLab|Keychain|Touch ID|Windows Hello|LM Studio|Ollama|Finder|File Explorer|Files|macOS|Linux|Windows|IntelliJ IDEA|JetBrains)\b/g;

/**
 * Acronyms of four letters or more, the environment variables the window names, and a
 * workspace's LIVE and LOCAL (ADR 0072's own labels, written in capitals), which keep their
 * capitals. Shorter ones (`PR`, `CLI`, `SSH`) are under the length the capitals rule reads. A
 * missing one fails the guard on its first label; adding it here is the fix, and it is the only
 * list.
 */
const ACRONYMS = /\b(JSON|YAML|TOML|HTML|HTTP|HTTPS|README|UUID|ASCII|PATH|LIVE|LOCAL)\b/g;

/** A code span: what a person types, read as a whole and never as words. */
const CODE_SPAN = /`[^`]*`/g;

/**
 * An id, a path or an assignment: letters or digits joined by `.`, `/`, `:`, `_`, `=` or `-`
 * with no space, such as `ask.please`, `hooks/please-hold.sh` or `NAME=value`. "Re-arm" is one
 * too, which is harmless: a stock phrase is never hyphenated.
 */
const ID = /\S*[A-Za-z0-9][./:_=-][A-Za-z0-9]\S*/g;

/** The words of `text` a person reads: without its code spans and its ids. */
function words(text: string): string {
  return text.replace(CODE_SPAN, " ").replace(ID, " ").trim();
}

/** A sentence that ends in "!": after a letter, before a space, a closing mark or the end. */
const EXCLAIMED = /[A-Za-z]!+(?=[\s)"'\u2019\u201d]|$)/;

/**
 * A word in capitals: four letters or more, none of them small. Code spans, ids and file names
 * (`AGENTS.md`), acronyms, names and chords are taken out first; a placeholder is read as `…`,
 * so it is no word.
 */
function shouted(text: string): boolean {
  const said = text
    .replace(CODE_SPAN, " ")
    .replace(ID, " ")
    .replace(CHORD, " ")
    .replace(NAMES, " ")
    .replace(ACRONYMS, " ");
  return /\b[A-Z]{4,}\b/.test(said);
}

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
 * A path to a Settings group (`Settings › Project › Dispatch`, `Settings › You › This machine`):
 * the window's own labels, each written as it reads, so the path has no case of its own.
 */
const SETTINGS_PATH = /\bSettings(?: › [A-Z][a-z]+(?: [a-z]+)?)+/g;

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
      .replace(SETTINGS_PATH, " ")
      // A capital Charter or Purlis is its own fault, below, and is said once.
      .replace(/\b(Charter|Purlis)\b/g, " ")
      .replace(/[^A-Za-z'-]+/g, " ")
      .split(" ")
      .filter((word) => word.length >= 4);
    return words.length >= 2 && words.every((word) => /^[A-Z][a-z]/.test(word));
  });
}

/**
 * The retired terms, each with what to say instead (CONTEXT.md's _Avoid_ lines). "Piece of" is
 * English ("a piece of work"), not the format's piece.
 */
const RETIRED: readonly { term: RegExp; fault: string }[] = [
  { term: /\bplanes?\b/i, fault: "a retired term: say project, not plane" },
  { term: /\bworktrees?\b/i, fault: "a retired term: say branch or folder, not worktree" },
  { term: /\bpieces?\b(?! of\b)/i, fault: "a retired term: say branch or folder, not piece" },
  { term: /\bsync(s|ed|ing)?\b/i, fault: "a retired term: sync is purlis sync's alone" },
];

/** The two ways the window may say sync: the command, and its row's label, each as written. */
const SYNC_ITSELF = /\b(purlis|charter) sync\b|\bSync repos\b/g;

/**
 * An id or a path, for the retired terms: `ID` without the hyphen. A hyphenated word is still
 * words a person reads ("a plane-wide setting", "re-sync"), so a retired term in one is said.
 */
const ID_NOT_HYPHENATED = /\S*[A-Za-z0-9][./:_=][A-Za-z0-9]\S*/g;

/**
 * **The retired terms `text` uses in the window's words** (FR-3, #602): empty for a string the
 * window does not show. It is a rule of its own, beside `copyFaults` rather than inside it,
 * because the window still says some of them in files other work holds: each guard that runs
 * it lists its own debt (`copy.test.ts`'s `RETIRED_TERM_DEBT`), and `copyFaults` stays a rule
 * set with none.
 */
export function retiredTerms(text: string, seen: Seen = "source"): string[] {
  if (seen !== "shown") return [];
  const said = text
    .trim()
    .replace(CODE_SPAN, " ")
    .replace(ID_NOT_HYPHENATED, " ")
    .replace(SYNC_ITSELF, " ");
  return RETIRED.filter(({ term }) => term.test(said)).map(({ fault }) => fault);
}

/** What is wrong with `text` by the guide's mechanical rules; empty when nothing is. */
export function copyFaults(text: string, seen: Seen = "source"): string[] {
  const faults: string[] = [];
  const said = text.trim();
  if (STOCK.test(words(said))) faults.push("a stock phrase: say what happened instead");
  if (seen === "shown") {
    if (EXCLAIMED.test(said.replace(CODE_SPAN, " ")))
      faults.push("an exclamation mark: say it plainly");
    if (shouted(said)) faults.push("capitals: write the word in lowercase, or name it");
    if (titleCased(said)) faults.push("title case: write labels in sentence case");
    if (/\b(Charter|Purlis)\b/.test(said))
      faults.push("a capital product name: purlis is lowercase, the About title too");
  }
  return faults;
}
