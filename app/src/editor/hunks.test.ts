import { describe, expect, it } from "vitest";
import { changesOf } from "./hunks";
import { gitHunks } from "./gitdiff.testkit";

/** The 1-based line numbers a range of a document covers. */
function linesOf(doc: string, from: number, to: number): number[] {
  const first = doc.slice(0, from).split("\n").length;
  const last = doc.slice(0, Math.max(from, to - 1)).split("\n").length;
  return to > from ? Array.from({ length: last - first + 1 }, (_, i) => first + i) : [];
}

/** The lines git says changed, on each side. */
function gitLines(base: string, head: string) {
  const a = new Set<number>();
  const b = new Set<number>();
  for (const hunk of gitHunks(base, head)) {
    for (let i = 0; i < hunk.oldLines; i++) a.add(hunk.oldStart + i);
    for (let i = 0; i < hunk.newLines; i++) b.add(hunk.newStart + i);
  }
  return { a, b };
}

const CASES: [string, string, string][] = [
  ["a line changed", "one\ntwo\nthree\n", "one\nTWO\nthree\n"],
  ["a line added at the end", "one\ntwo\n", "one\ntwo\nthree\n"],
  ["a line added at the top", "two\nthree\n", "one\ntwo\nthree\n"],
  ["a line removed", "one\ntwo\nthree\n", "one\nthree\n"],
  ["no newline at the end", "one\ntwo", "one\ntwo\nthree"],
  ["two hunks far apart", "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n", "A\nb\nc\nd\ne\nf\ng\nh\ni\nJ\n"],
  // Found by comparing the two on generated files: the merge view's own diff, run over the
  // whole file, marks a line of the head that git leaves alone. Only git's answer is drawn.
  ["a file where the two diffs disagree", "x\n{\ny\nx\ny\n\ny\n}\n", "y\n}\ny\n"],
];

describe("the merge view marks what git says changed", () => {
  it.each(CASES)("%s: every change is inside a line git reported", (_, base, head) => {
    const git = gitLines(base, head);

    const changes = changesOf(base, head, gitHunks(base, head));

    for (const change of changes) {
      for (const line of linesOf(base, change.fromA, change.toA)) expect(git.a).toContain(line);
      for (const line of linesOf(head, change.fromB, change.toB)) expect(git.b).toContain(line);
    }
  });

  it.each(CASES)("%s: every hunk git reported is drawn", (_, base, head) => {
    const hunks = gitHunks(base, head);

    const changes = changesOf(base, head, hunks);

    for (const hunk of hunks) {
      const inside = changes.filter((c) => {
        const b = linesOf(head, c.fromB, c.toB);
        const a = linesOf(base, c.fromA, c.toA);
        return (
          b.some((line) => line >= hunk.newStart && line < hunk.newStart + hunk.newLines) ||
          a.some((line) => line >= hunk.oldStart && line < hunk.oldStart + hunk.oldLines)
        );
      });
      expect(inside.length, JSON.stringify(hunk)).toBeGreaterThan(0);
    }
  });

  it("marks only the word that changed inside a changed line", () => {
    const base = "let colour = red;\n";
    const head = "let colour = blue;\n";

    const changes = changesOf(base, head, gitHunks(base, head));

    const marked = changes.map((c) => head.slice(c.fromB, c.toB)).join("");
    expect(marked).toBe("blue");
  });

  it("marks nothing when git reports nothing", () => {
    expect(changesOf("same\n", "same\n", [])).toEqual([]);
  });
});
