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
 * - **Shown:** JSX text and a JSX child in braces; the value of an attribute a person reads or
 *   hears (`SHOWN_ATTRIBUTES`); a `label:` property and the others like it (`SHOWN_PROPERTIES`);
 *   and the catalogue's
 *   titles and reasons (`SHOWN_ARGUMENTS`). Through an expression, a conditional's branches
 *   and a logical operator's right-hand side are shown; its condition is source. A template's
 *   interpolations are read as `…`.
 * - **Source:** every other string literal and template. Copy built in TypeScript (an error
 *   handed to a status line, a panel's sentence) is here, mixed with ids and paths; a rule that
 *   applies to it has to be one no id or path can break.
 *
 * Module specifiers are neither.
 */
import ts from "typescript";
import type { Seen } from "./copy";

/** The attributes whose value is copy: read on screen or by a screen reader. `headline` and
 *  `body` are `EmptyState`'s; `help` and `hint` are a `SettingRow`'s. */
export const SHOWN_ATTRIBUTES = new Set([
  "aria-label",
  "title",
  "placeholder",
  "alt",
  "label",
  "headline",
  "body",
  "help",
  "hint",
]);

/**
 * Object properties whose value the window draws: a menu row's and a palette row's `label`; a
 * settings group's `title` and `note`, and a field's `help` and `hint`; a vault provider's
 * `says`; the sandbox table's `what` and `why`; an empty state's `headline` and `body` (#602).
 * A property of the same name that holds no copy holds nothing a rule refuses either: an id
 * has no spaces, and the rules read none.
 */
export const SHOWN_PROPERTIES = new Set([
  "label",
  "title",
  "note",
  "help",
  "hint",
  "says",
  "what",
  "why",
  "headline",
  "body",
]);

/**
 * Calls whose arguments, by position, are drawn: the catalogue's `can(id, title, does)` and
 * `cannot(id, title, reason)` in `actions.ts`, and the settings' control builders. The id and
 * the key path are not.
 */
export const SHOWN_ARGUMENTS: Record<string, readonly number[]> = {
  can: [1],
  cannot: [1, 2],
  // The settings' controls (`settings/fileControls.ts`): a label, a hint and an unset value.
  textAt: [1],
  listAt: [1, 2],
  pickAt: [1, 3],
  onOffAt: [1, 2, 3],
};

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

  /**
   * An expression whose value the window shows: its literals are shown, through parentheses,
   * a conditional's two branches and a logical operator's right-hand side. A condition, a
   * left-hand side and anything else are read as source.
   */
  const shownValue = (node: ts.Expression): void => {
    const text = literal(node);
    if (text !== undefined) {
      add(node, text, "shown");
      if (ts.isTemplateExpression(node)) ts.forEachChild(node, visit);
    } else if (ts.isParenthesizedExpression(node)) {
      shownValue(node.expression);
    } else if (ts.isConditionalExpression(node)) {
      visit(node.condition);
      shownValue(node.whenTrue);
      shownValue(node.whenFalse);
    } else if (
      ts.isBinaryExpression(node) &&
      [
        ts.SyntaxKind.AmpersandAmpersandToken,
        ts.SyntaxKind.BarBarToken,
        ts.SyntaxKind.QuestionQuestionToken,
      ].includes(node.operatorToken.kind)
    ) {
      visit(node.left);
      shownValue(node.right);
    } else {
      visit(node);
    }
  };

  const visit = (node: ts.Node): void => {
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) return;
    if (ts.isExternalModuleReference(node) || ts.isLiteralTypeNode(node)) return;
    if (ts.isJsxText(node)) {
      // JSX folds a run of whitespace with a line break in it, and so does this.
      add(node, node.text.replace(/\s+/g, " ").trim(), "shown");
      return;
    }
    // A JSX child in braces is drawn, as the text beside it is.
    if (
      ts.isJsxExpression(node) &&
      node.expression !== undefined &&
      !ts.isJsxAttribute(node.parent)
    ) {
      shownValue(node.expression);
      return;
    }
    if (ts.isJsxAttribute(node) && SHOWN_ATTRIBUTES.has(node.name.getText(file))) {
      const value = node.initializer;
      if (value === undefined) return;
      if (ts.isJsxExpression(value)) {
        if (value.expression !== undefined) shownValue(value.expression);
      } else if (ts.isStringLiteral(value)) {
        add(value, value.text, "shown");
      } else {
        visit(value);
      }
      return;
    }
    if (ts.isPropertyAssignment(node) && SHOWN_PROPERTIES.has(node.name.getText(file))) {
      shownValue(node.initializer);
      return;
    }
    if (ts.isCallExpression(node) && ts.isIdentifier(node.expression)) {
      // Own keys only: a function named `toString` or `valueOf` is an ordinary call.
      const name = node.expression.text;
      const shown = Object.prototype.hasOwnProperty.call(SHOWN_ARGUMENTS, name)
        ? SHOWN_ARGUMENTS[name]
        : undefined;
      if (shown !== undefined) {
        node.arguments.forEach((argument, at) =>
          shown.includes(at) ? shownValue(argument) : visit(argument),
        );
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
