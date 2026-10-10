import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * **Every question dialog is answered from one bar** (DS-3e's follow-up, #1210, D-1210-1).
 *
 * A question with a way out and an act or two, and nothing to fill in, ends in an `AnswerBar`
 * (`AnswerBar.tsx`). Before it, two hand-built rows drew the same buttons with two layouts:
 * `answer`, right-aligned with a gap, and `doing`, with no layout of its own, whose buttons sat
 * wherever the dialog's flow put them. This fails on a dialog that builds either row by hand
 * again: `answer` is named by no `className` but the bar's own, and `doing` by none in a file that
 * draws a Radix dialog. (`doing` stays a word elsewhere: a tab chip's `<span>` saying what a task
 * is doing, the opener page's row, a tab's own buttons. None of those is a dialog's answer, so
 * only a `<div>` row in a dialog's file counts.)
 *
 * The source is read as text, as `settings/oldFormClasses.test.ts` reads it.
 */

const SRC = join(process.cwd(), "src");

/** Where the bar is drawn, and the one file allowed to name its class. */
const BAR = "AnswerBar.tsx";

/** Every file under `dir` whose name `keep` takes. */
function files(dir: string, keep: (name: string) => boolean): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return files(path, keep);
    return keep(entry.name) ? [path] : [];
  });
}

/** Every source file the window is drawn from: not tests, which name the classes to find them. */
const sources = () =>
  files(SRC, (name) => /\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name)).map((path) => ({
    name: relative(SRC, path),
    text: readFileSync(path, "utf8"),
  }));

/** The class names a `className` attribute can hold: every string in its value. */
export function classNames(source: string): string[] {
  const values = [
    ...source.matchAll(/className=(?:"([^"]*)"|\{([^{}]*(?:\{[^{}]*\}[^{}]*)*)\})/g),
  ].flatMap(([, plain, expression]) =>
    plain !== undefined
      ? [plain]
      : [...(expression ?? "").matchAll(/"([^"]*)"|'([^']*)'|`([^`]*)`/g)].map(([, a, b, c]) =>
          (a ?? b ?? c ?? "").replace(/\$\{[^}]*\}/g, " "),
        ),
  );
  return values.flatMap((value) => value.split(/\s+/)).filter(Boolean);
}

/** The opening `<div …>` tags in `source`: a row of buttons is a `div`, a line of text is not. */
const divs = (source: string) => [...source.matchAll(/<div\b[^>]*>/g)].map(([tag]) => tag);

/** Whether a file draws a dialog: it imports one of Radix's two. */
export const drawsADialog = (source: string) =>
  /from\s+["']@radix-ui\/react-(?:alert-)?dialog["']/.test(source);

/** What a file builds by hand that the bar draws, as `file: class`. */
export function handBuilt({ name, text }: { name: string; text: string }): string[] {
  if (name === BAR) return [];
  const named = classNames(text);
  return [
    ...named.filter((one) => one === "answer"),
    ...(drawsADialog(text)
      ? divs(text)
          .flatMap(classNames)
          .filter((one) => one === "doing")
      : []),
  ].map((one) => `${name}: ${one}`);
}

describe("a question dialog's answer bar", () => {
  it("is built by hand in no dialog", () => {
    const all = sources();
    // The bar itself and a dialog or two at least: a reader that found none would pass on nothing.
    expect(all.map(({ name }) => name)).toEqual(
      expect.arrayContaining([BAR, "QuitWarning.tsx", "DeleteWorkspace.tsx"]),
    );
    expect(all.flatMap(handBuilt)).toEqual([]);
  });

  it("is still drawn by the bar, with the class the stylesheet lays out", () => {
    const bar = readFileSync(join(SRC, BAR), "utf8");
    expect(classNames(bar)).toContain("answer");
    expect(readFileSync(join(SRC, "App.css"), "utf8")).toMatch(/\n\.answer \{[^}]*display: flex/);
  });

  it("would be caught if one came back", () => {
    // The reader above is what the guard rests on, so it is held to the shapes a dialog writes.
    const dialog = `import * as AlertDialog from "@radix-ui/react-alert-dialog";\n`;
    expect(handBuilt({ name: "X.tsx", text: `${dialog}<div className="doing" />` })).toEqual([
      "X.tsx: doing",
    ]);
    expect(handBuilt({ name: "X.tsx", text: `<div className="answer" />` })).toEqual([
      "X.tsx: answer",
    ]);
    expect(
      handBuilt({
        name: "X.tsx",
        text: `import { Dialog } from "@radix-ui/react-dialog";\n<div className={on ? "doing" : "x"}>`,
      }),
    ).toEqual(["X.tsx: doing"]);
    // A tab chip's line of what a task is doing is no dialog's answer, even beside a dialog.
    expect(
      handBuilt({ name: "TabChip.tsx", text: `${dialog}<span className="doing">x</span>` }),
    ).toEqual([]);
    // Nor is the opener page's row, which draws no dialog.
    expect(handBuilt({ name: "Opener.tsx", text: `<div className="doing">` })).toEqual([]);
    // `answer-question` is a different class, and `answer` alone is the bar's.
    expect(handBuilt({ name: "X.tsx", text: `<div className="answer-question" />` })).toEqual([]);
  });
});
