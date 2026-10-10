import type { Jump } from "./fileJump";
import type { Place } from "./pieceViews";

/**
 * **A file and line a session record names** (RC-5a #984, RC-20a #1043; D-984-3): what an
 * inline code span of the record's text opens, if anything.
 *
 * - **The form** is `<path>:<line>`, or `<path>` alone, which is line 1. A path is relative,
 *   with segments of letters, digits and `._@+-` only, no `.` or `..` segment, and either a
 *   folder or a dot in it, so `npm test`, `true` or `foo()` stay code.
 * - **The repo** is the path's first segment when that names one of the workspace's repos;
 *   otherwise the workspace's only repo, when it has exactly one. Anything else names nothing.
 * - **The place** is that repo's own folder (piece `null`): a record does not say which branch
 *   it was written on, and the file tab reads through `piece_file`, which refuses a path the
 *   folder does not offer.
 *
 * A record is a file a chat wrote, so what it names is a request, never a trusted path: the
 * core places and refuses it, and this only decides which spans are worth a button.
 */
export type RecordRef = Omit<Jump, "plane">;

const SEGMENT = /^[A-Za-z0-9._@+-]+$/;
const REF = /^([^:]+)(?::([1-9][0-9]*))?$/;

/** The file `code` names in the workspace `workspace` whose repos are `repos`, if it names one. */
export function recordRef(
  code: string,
  workspace: string,
  repos: readonly string[],
): RecordRef | undefined {
  const form = REF.exec(code);
  if (form === null) return undefined;
  const [, named, line] = form;
  const segments = named.split("/");
  if (!segments.every((one) => SEGMENT.test(one) && one !== "." && one !== "..")) return undefined;
  if (segments.length === 1 && !named.includes(".")) return undefined;
  const at = (repo: string, path: string[]): RecordRef => ({
    place: { workspace, repo, piece: null } satisfies Place,
    path: path.join("/"),
    line: line === undefined ? 1 : Number(line),
  });
  if (segments.length > 1 && repos.includes(segments[0])) return at(segments[0], segments.slice(1));
  if (repos.length === 1) return at(repos[0], segments);
  return undefined;
}

const FENCE = /^ {0,3}(`{3,}|~{3,})/;
const SPAN = /`([^`\n]+)`/g;

/**
 * The inline code spans of a record's Markdown, each once, in the order they first appear:
 * what {@link recordRef} is asked about for the record's list of files. Fenced blocks are
 * skipped, as the record's rendering draws them as blocks and never as references.
 */
export function codeSpans(body: string): string[] {
  const spans = new Set<string>();
  let fence: string | undefined;
  for (const line of body.split("\n")) {
    const opens = FENCE.exec(line)?.[1];
    if (fence !== undefined) {
      if (opens !== undefined && opens[0] === fence[0] && opens.length >= fence.length)
        fence = undefined;
      continue;
    }
    if (opens !== undefined) {
      fence = opens;
      continue;
    }
    for (const [, code] of line.matchAll(SPAN)) spans.add(code);
  }
  return [...spans];
}
