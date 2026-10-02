/**
 * **git's hunks, as the merge view's changes** (RC-5, ADR 0084 §2).
 *
 * *"CodeMirror's merge view draws git's hunks. Which lines changed is git's answer. The merge
 * view's own diffing is used only inside a hunk git reported, for R3's word-level
 * highlighting."* So the merge view is handed this function as its `diffConfig.override`, and
 * never diffs the two files itself: each hunk git reported is a range of lines on each side,
 * and inside that range alone the merge view's own diff, cleaned to word boundaries, finds the
 * words that changed.
 */
import { Change, presentableDiff } from "@codemirror/merge";

/**
 * One hunk, in git's own numbers: `@@ -oldStart,oldLines +newStart,newLines @@`. Lines are
 * counted from 1. A side with no lines (`oldLines` 0 for an insertion) names the line the hunk
 * comes AFTER, so `0` is the top of the file, exactly as `git diff` writes it.
 */
export type GitHunk = { oldStart: number; oldLines: number; newStart: number; newLines: number };

/** Where each line of a document starts, by its 0-based index. */
function starts(doc: string): number[] {
  const out = [0];
  for (let at = doc.indexOf("\n"); at !== -1 && at + 1 < doc.length; at = doc.indexOf("\n", at + 1))
    out.push(at + 1);
  return out;
}

/** The character range git's `start,lines` covers in a document. */
function range(doc: string, lineStarts: number[], start: number, lines: number): [number, number] {
  const at = (line: number) => (line - 1 < lineStarts.length ? lineStarts[line - 1] : doc.length);
  if (lines === 0) {
    const after = at(start + 1);
    return [after, after];
  }
  return [at(start), at(start + lines)];
}

/** The merge view's changes for `base` → `head`, where git reported `hunks`. */
export function changesOf(base: string, head: string, hunks: readonly GitHunk[]): Change[] {
  const a = starts(base);
  const b = starts(head);
  const out: Change[] = [];
  for (const hunk of hunks) {
    const [fromA, toA] = range(base, a, hunk.oldStart, hunk.oldLines);
    const [fromB, toB] = range(head, b, hunk.newStart, hunk.newLines);
    const inner = presentableDiff(base.slice(fromA, toA), head.slice(fromB, toB));
    if (inner.length === 0) {
      // git reported a change the words do not show (a line ending, say): the whole hunk.
      out.push(new Change(fromA, toA, fromB, toB));
      continue;
    }
    for (const one of inner) {
      out.push(new Change(one.fromA + fromA, one.toA + fromA, one.fromB + fromB, one.toB + fromB));
    }
  }
  return out;
}
