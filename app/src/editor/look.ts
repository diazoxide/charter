/**
 * **How the light editor is drawn, from the theme's tokens** (`src/theme/`).
 *
 * CodeMirror puts its own stylesheet into the page at run time, outside every `@layer`, so a
 * rule in `App.css` (layer `charter`) can never outrank it (`literals.test.ts`, *every
 * stylesheet is in a layer*). Its colours are therefore replaced here, through
 * `EditorView.theme`, which CodeMirror scopes above its own, and every value is a theme
 * token's custom property: no colour is written in this file. `literals.test.ts` reads it
 * as it reads `App.css`.
 *
 * **The terminal's colours.** The editor is drawn on `terminal.background` in
 * `terminal.foreground`, and syntax in the terminal's ANSI palette: a theme already holds
 * those legible against each other, so a theme author gets code drawn in the colours they
 * chose for code everywhere else in the window.
 */
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags } from "@lezer/highlight";

export const look = [
  EditorView.theme({
    "&": {
      color: "var(--terminal-foreground)",
      backgroundColor: "var(--terminal-background)",
      height: "100%",
    },
    ".cm-content": { caretColor: "var(--terminal-cursor)" },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--terminal-cursor)" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
      backgroundColor: "var(--terminal-selection)",
    },
    ".cm-gutters": {
      color: "var(--text-muted)",
      backgroundColor: "var(--surface-raised)",
      borderRight: "1px solid var(--border-subtle)",
    },
    ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "var(--surface-hover)" },
    // The merge view (ADR 0084 §2): a changed line is washed, and the words git's hunk
    // changed inside it are marked more strongly. Base is side A, head is side B. The merge
    // view's own rules name a light or a dark editor, one class more than a theme's rule
    // would; `.cm-editor` (every editor has it) makes up the class, and at equal reach a
    // theme's rule is the one CodeMirror puts after its base rules.
    "&.cm-merge-a .cm-changedLine, .cm-deletedChunk": {
      backgroundColor: "var(--diff-deleted)",
    },
    "&.cm-merge-b .cm-changedLine, .cm-inlineChangedLine": {
      backgroundColor: "var(--diff-inserted)",
    },
    "&.cm-editor.cm-merge-a .cm-changedText, &.cm-editor .cm-deletedChunk .cm-deletedText": {
      background: "var(--diff-deleted-text)",
    },
    "&.cm-editor.cm-merge-b .cm-changedText": {
      background: "var(--diff-inserted-text)",
    },
    "&.cm-merge-b .cm-deletedText": { background: "var(--diff-deleted-text)" },
    "&.cm-editor.cm-merge-a .cm-changedLineGutter, &.cm-editor .cm-deletedLineGutter": {
      background: "var(--danger-base)",
    },
    "&.cm-editor.cm-merge-b .cm-changedLineGutter": {
      background: "var(--state-success)",
    },
    ".cm-inlineChangedLineGutter": { background: "var(--accent-base)" },
    "&.cm-editor .cm-collapsedLines": {
      color: "var(--text-muted)",
      background: "var(--surface-raised)",
    },
  }),
  syntaxHighlighting(
    HighlightStyle.define([
      {
        tag: [tags.keyword, tags.modifier, tags.operatorKeyword],
        color: "var(--terminal-ansi-magenta)",
      },
      {
        tag: [tags.string, tags.special(tags.string), tags.regexp],
        color: "var(--terminal-ansi-green)",
      },
      { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "var(--terminal-ansi-yellow)" },
      {
        tag: [tags.comment, tags.meta],
        color: "var(--terminal-ansi-bright-black)",
        fontStyle: "italic",
      },
      {
        tag: [tags.function(tags.variableName), tags.function(tags.propertyName)],
        color: "var(--terminal-ansi-blue)",
      },
      { tag: [tags.typeName, tags.className, tags.namespace], color: "var(--terminal-ansi-cyan)" },
      { tag: [tags.propertyName, tags.attributeName], color: "var(--terminal-ansi-bright-blue)" },
      { tag: [tags.tagName, tags.heading], color: "var(--terminal-ansi-red)", fontWeight: "bold" },
      { tag: tags.invalid, color: "var(--terminal-ansi-bright-red)" },
      { tag: tags.link, textDecoration: "underline" },
      { tag: tags.emphasis, fontStyle: "italic" },
      { tag: tags.strong, fontWeight: "bold" },
    ]),
  ),
];
