import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { ISearchOptions, SearchAddon } from "@xterm/addon-search";
import { ChevronDown, ChevronUp, X } from "lucide-react";
import { inForce, searchDecorations } from "./theme/theme";

/**
 * **The find bar over a chat's terminal** (SI-4): what `⌘F` opens on the focused pane.
 *
 * The finding is `@xterm/addon-search`'s, the terminal's own addon, so what it searches is what
 * the terminal holds — the screen and the scrollback the core keeps. Nothing here reads a
 * harness's output to decide anything; it only shows the operator where a word is.
 *
 * Native HTML and not a Radix primitive, by `docs/ui-primitives.md`'s own rule: a search field
 * and three buttons are elements the browser already has, and Radix has no primitive for a find
 * bar to take. `role="search"` is what names it as one.
 *
 * It is a sibling of the terminal and never inside it — the rule `.pane-frame` holds for
 * everything charter draws over a pane — so a key typed here is the bar's and not the chat's:
 * it is outside the element marked `CHAT_KEYBOARD`, and xterm never sees it.
 */
export function FindBar({
  search,
  asked,
  onClose,
}: {
  search: SearchAddon;
  /** How many times the bar has been asked for: each time, the keyboard goes to its field. */
  asked: number;
  onClose: () => void;
}) {
  const field = useRef<HTMLInputElement>(null);
  const [term, setTerm] = useState("");
  const [results, setResults] = useState<{ index: number; count: number } | null>(null);

  // The addon says where it is after every find; that is the whole of the count.
  useEffect(() => {
    const told = search.onDidChangeResults(({ resultIndex, resultCount }) =>
      setResults({ index: resultIndex, count: resultCount }),
    );
    return () => {
      told.dispose();
      // A closed bar leaves nothing drawn over the terminal.
      search.clearDecorations();
    };
  }, [search]);

  // Opened, or asked for again while open: the keyboard goes to the field, with what is in it
  // selected, so typing replaces it.
  useEffect(() => {
    field.current?.focus();
    field.current?.select();
  }, [asked]);

  /** Every match highlighted, in the theme in force when the find runs — so a theme switched
   *  while the bar is open colours the next find. */
  const options = (incremental: boolean): ISearchOptions => ({
    incremental,
    decorations: searchDecorations(inForce()),
  });

  const next = () => {
    if (term !== "") search.findNext(term, options(false));
  };
  const previous = () => {
    if (term !== "") search.findPrevious(term, options(false));
  };

  const typed = (value: string) => {
    setTerm(value);
    if (value === "") {
      search.clearDecorations();
      setResults(null);
      return;
    }
    search.findNext(value, options(true));
  };

  const key = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      if (event.shiftKey) previous();
      else next();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
  };

  return (
    <div className="pane-find" role="search" aria-label="Find in the chat">
      <input
        ref={field}
        type="search"
        aria-label="Find"
        placeholder="Find"
        value={term}
        spellCheck={false}
        autoComplete="off"
        onChange={(event) => typed(event.target.value)}
        onKeyDown={key}
      />
      <span className="pane-find-count" role="status" aria-live="polite">
        {told(results)}
      </span>
      <button type="button" aria-label="Previous match" onClick={previous}>
        <ChevronUp size={14} aria-hidden />
      </button>
      <button type="button" aria-label="Next match" onClick={next}>
        <ChevronDown size={14} aria-hidden />
      </button>
      <button type="button" aria-label="Close find" onClick={onClose}>
        <X size={14} aria-hidden />
      </button>
    </div>
  );
}

/** The count, in words. The addon answers an index of -1 past its highlight limit, where it
 *  stops saying which match is selected; the count is still true then. */
function told(results: { index: number; count: number } | null): string {
  if (results === null) return "";
  if (results.count === 0) return "No matches";
  if (results.index < 0) return `${results.count} matches`;
  return `${results.index + 1} of ${results.count}`;
}
