/**
 * **The light editor** (RC-5, ADR 0081 §1, R4): charter's one CodeMirror 6 component, for
 * reading a file and, in its merge view, a diff. One editor stack, TypeScript only.
 *
 * - **Read only here.** RC-10 makes the head side editable while the chat is idle, with the
 *   stale check and the human-edit record (ADR 0084 §8); nothing in this file writes.
 * - **No code from an extension runs inside it** (ADR 0081 §2): its grammars are charter's
 *   fixed set (`languages.ts`) and its look is the theme's tokens (`look.ts`).
 * - **Content is data**: CodeMirror draws a document as text, never as markup (ADR 0084 §2).
 *
 * This module and everything it imports is its own chunk, loaded when the first file or diff
 * opens (`Views.tsx`), so a window that never opens one carries none of it (ADR 0086 row M2).
 */
import { useEffect, useRef } from "react";
import { MergeView } from "@codemirror/merge";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import {
  EditorView,
  highlightActiveLineGutter,
  highlightSpecialChars,
  lineNumbers,
} from "@codemirror/view";
import { changesOf, type GitHunk } from "./hunks";
import { grammarFor } from "./languages";
import { look } from "./look";

/** What every view of a file has: line numbers, the theme, read only. */
function reading(language: Compartment): Extension[] {
  return [
    lineNumbers(),
    highlightActiveLineGutter(),
    highlightSpecialChars(),
    EditorState.readOnly.of(true),
    language.of([]),
    look,
  ];
}

/** Loads the path's grammar into each view, once it has arrived. */
function colour(path: string, language: Compartment, views: EditorView[]): () => void {
  let gone = false;
  const load = grammarFor(path);
  if (load) {
    void load().then((grammar) => {
      if (gone) return;
      for (const view of views) view.dispatch({ effects: language.reconfigure(grammar) });
    });
  }
  return () => {
    gone = true;
  };
}

/** One file, read only. `line`, when given, is brought into view (a jump from a diff, a
 *  record or the knowledge graph). */
export function LightEditor({ path, text, line }: { path: string; text: string; line?: number }) {
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!host.current) return;
    const language = new Compartment();
    const view = new EditorView({
      parent: host.current,
      state: EditorState.create({ doc: text, extensions: reading(language) }),
    });
    if (line !== undefined && line >= 1 && line <= view.state.doc.lines) {
      const at = view.state.doc.line(line).from;
      view.dispatch({
        selection: { anchor: at },
        effects: EditorView.scrollIntoView(at, { y: "center" }),
      });
    }
    const stop = colour(path, language, [view]);
    return () => {
      stop();
      view.destroy();
    };
  }, [path, text, line]);
  return <div ref={host} className="light-editor" data-testid="light-editor" />;
}

/**
 * A diff in CodeMirror's merge view: the base on the left, the head on the right.
 *
 * **The lines it marks are git's** (ADR 0084 §2): `hunks` is what git reported, and the merge
 * view's own diff runs only inside each hunk, for the words (`hunks.ts`). Unchanged stretches
 * are collapsed to three lines of context.
 */
export function MergeViewer({
  path,
  base,
  head,
  hunks,
}: {
  path: string;
  base: string;
  head: string;
  hunks: readonly GitHunk[];
}) {
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!host.current) return;
    const language = new Compartment();
    const changes = changesOf(base, head, hunks);
    const merge = new MergeView({
      parent: host.current,
      a: { doc: base, extensions: reading(language) },
      b: { doc: head, extensions: reading(language) },
      gutter: true,
      collapseUnchanged: { margin: 3, minSize: 4 },
      diffConfig: { override: () => changes },
    });
    const stop = colour(path, language, [merge.a, merge.b]);
    return () => {
      stop();
      merge.destroy();
    };
  }, [path, base, head, hunks]);
  return <div ref={host} className="light-editor merge" data-testid="merge-viewer" />;
}
