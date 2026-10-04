/**
 * **Every string a source file writes, and whether the window shows it** — for the tests that
 * hold the window's copy to a rule: the copy guide's (`copy.test.ts`, DS-2) and the first
 * hour's words (ADR 0072 §3 and §5, FR-3), which reads the same strings.
 *
 * It parses with TypeScript's own parser rather than a regular expression, because the
 * question is about the syntax tree: a text node, an attribute's value, a string literal. A
 * pattern over the text cannot tell `aria-label="Close"` from the same characters in a comment.
 * Test-only: nothing the window bundles imports this file.
 *
 * - **Shown:** JSX text, and the string value of an attribute a person reads or hears
 *   (`SHOWN_ATTRIBUTES`). A template's interpolations are read as `…`.
 * - **Source:** every other string literal and template. Copy built in TypeScript (an error
 *   handed to a status line, a panel's sentence) is here, mixed with ids and paths; a rule that
 *   applies to it has to be one no id or path can break.
 *
 * Module specifiers are neither.
 */
import ts from "typescript";
import type { Seen } from "./copy";

/** The attributes whose value is copy: read on screen or by a screen reader. `headline` and
 *  `body` are `EmptyState`'s. */
export const SHOWN_ATTRIBUTES = new Set([
  "aria-label",
  "title",
  "placeholder",
  "alt",
  "label",
  "headline",
  "body",
]);

export type UiString = { text: string; seen: Seen; line: number };

/** A template's text with each interpolation read as `…`. */
function templateText(node: ts.TemplateExpression): string {
  return [node.head.text, ...node.templateSpans.map((span) => span.literal.text)].join("…");
}

/** The strings in one file's source, in the order they are written. */
export function uiStrings(source: string): UiString[] {
  const file = ts.createSourceFile(
    "copy.tsx",
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TSX,
  );
  const found: UiString[] = [];
  const add = (node: ts.Node, text: string, seen: Seen) => {
    if (text.trim() === "") return;
    const line = file.getLineAndCharacterOfPosition(node.getStart(file)).line + 1;
    found.push({ text, seen, line });
  };
  const literal = (node: ts.Node): string | undefined =>
    ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)
      ? node.text
      : ts.isTemplateExpression(node)
        ? templateText(node)
        : undefined;

  const visit = (node: ts.Node): void => {
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) return;
    if (ts.isExternalModuleReference(node) || ts.isLiteralTypeNode(node)) return;
    if (ts.isJsxText(node)) {
      // JSX folds a run of whitespace with a line break in it, and so does this.
      add(node, node.text.replace(/\s+/g, " ").trim(), "shown");
      return;
    }
    if (ts.isJsxAttribute(node) && SHOWN_ATTRIBUTES.has(node.name.getText(file))) {
      const value = node.initializer;
      const inner = value !== undefined && ts.isJsxExpression(value) ? value.expression : value;
      const text = inner === undefined ? undefined : literal(inner);
      if (inner !== undefined && text !== undefined) {
        add(inner, text, "shown");
        return;
      }
    }
    const text = literal(node);
    if (text !== undefined) {
      add(node, text, "source");
      // A template's interpolations can hold strings of their own.
      if (ts.isTemplateExpression(node)) ts.forEachChild(node, visit);
      return;
    }
    ts.forEachChild(node, visit);
  };
  visit(file);
  return found;
}
