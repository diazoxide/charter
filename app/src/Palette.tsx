import { useCallback, useEffect, useRef, useState } from "react";
import { aim, narrow, type Offer, type Ran } from "./actions";

/**
 * The command palette: every action the window can do, reachable by typing.
 *
 * **It is the primary input** (spec decision 1), and the tmux frame's `F2` before it. The
 * frame replaced a menu with it rather than putting one beside the other, because two
 * answers to "how do I do a thing" is how its single menu became weird in the first place —
 * so the bar's buttons here are not a second list, they are four rows of THIS one, drawn
 * permanently. `actions.catalogue` is the whole of the seam.
 *
 * **Keyboard first, and nothing in it needs a mouse.** `F2` or `⌘K`/`Ctrl-K` opens it, typing
 * narrows it, the arrows move over every row, Enter runs the one it is aimed at, Escape
 * leaves — and the focus goes back where it was, which for an operator mid-chat is the
 * terminal they were typing in. A click selects and runs too; that is a convenience, not the
 * path.
 *
 * **An unavailable row is listed WITH ITS REASON.** It is dimmed and `aria-disabled`, and the
 * reason is a visible sentence beside it rather than the dimming alone — the same rule M1.3b
 * set for chat state marks. Enter aimed at one runs nothing and says the reason.
 *
 * **A refusal keeps the palette open, in the core's own words.** `charter_core::worktree`
 * refuses a removal with a sentence naming the repair; the window does not reword it, does
 * not replace it with a generic failure, and shows it beside the rows rather than behind
 * them — the profile picker's rule, for the same reason: a refusal an operator cannot see
 * next to what they were doing is one they cannot act on.
 */
export function Palette({
  offers,
  said,
  onRun,
  onOpened,
}: {
  offers: readonly Offer[];
  /** What the last row answered, when it refused or had something to say. */
  said?: { from: string; refused: boolean; words: string };
  /** The window carrying out a row. It answers what happened; this decides what to draw. */
  onRun: (offer: Offer) => Ran | Promise<Ran>;
  /** Told whenever the palette opens or closes, so the window can say it is open. */
  onOpened?: (open: boolean) => void;
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  /** The row the arrows moved to, or nothing while Enter is aimed by `aim`. */
  const [at, setAt] = useState<number>();
  /** The reason the last Enter did nothing, for a row that cannot run. */
  const [held, setHeld] = useState<string>();
  /** Where the keyboard was when the palette opened, so Escape can give it back. */
  const came = useRef<Element | null>(null);
  const box = useRef<HTMLInputElement>(null);
  /** Whether it is up, for the one listener that is registered once and must not be torn
   *  down and rebuilt on every open. */
  const up = useRef(false);

  const close = useCallback(() => {
    setOpen(false);
    setQuery("");
    setAt(undefined);
    setHeld(undefined);
    onOpened?.(false);
    // Back to the terminal, the tab or the panel the operator was in. A pane's keyboard
    // lives in xterm's own textarea, which is an ordinary focusable element — so this is the
    // one line that makes the palette something an operator can open mid-sentence.
    const back = came.current;
    if (back instanceof HTMLElement && back.isConnected) back.focus();
  }, [onOpened]);

  // The key that opens it and the key that always leaves, listened for on the window and
  // CAPTURED. A pane's terminal would otherwise take the keystroke and send it to the
  // harness: xterm reads from its own textarea, and a capturing listener on the window runs
  // before that textarea's does.
  //
  // **Escape is here rather than on the box**, which is where it started. On the box it only
  // works while the box has the keyboard, and "the one key that always leaves" has to be true
  // wherever the focus has got to — a row reached by clicking, a browser that moved focus on
  // its own, or a surface that took it. A palette that cannot be left is the worst thing a
  // modal surface can be, and it is not a state to be one stray focus away from.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (opensIt(e)) {
        e.preventDefault();
        e.stopPropagation();
        setOpen((was) => {
          if (was) return was;
          came.current = document.activeElement;
          onOpened?.(true);
          return true;
        });
        return;
      }
      // Only while it is up: the picker and the quit warning listen for Escape too, and
      // swallowing theirs would leave them with no way out for the same reason.
      if (e.key === "Escape" && up.current) {
        e.preventDefault();
        e.stopPropagation();
        close();
      }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [close, onOpened]);

  // The box takes the keyboard as soon as it exists, so the first thing typed narrows.
  useEffect(() => {
    up.current = open;
    if (open) box.current?.focus();
  }, [open]);

  if (!open) return null;

  const rows = narrow(query, offers);
  const aimed = at ?? aim(rows);

  const run = (offer: Offer) => {
    if (!offer.available) {
      // Nothing runs, and the reason is said rather than left to the dimming.
      setHeld(offer.reason);
      return;
    }
    setHeld(undefined);
    void (async () => {
      const ran = await onRun(offer);
      // A refusal is not an ending: the operator is still here, still choosing, and the
      // catalogue may now offer them the answer to it.
      if (ran.ok) close();
    })();
  };

  const move = (by: number) => {
    if (rows.length === 0) return;
    const from = aimed < 0 ? (by > 0 ? -1 : 0) : aimed;
    setAt((from + by + rows.length) % rows.length);
  };

  const showing = said && said.words !== "" ? said : undefined;

  return (
    <div className="asking palette-over">
      <div className="warning palette" role="dialog" aria-modal="true" aria-label="Command palette">
        <label className="palette-ask" htmlFor="palette-query">
          Run an action
        </label>
        <input
          id="palette-query"
          ref={box}
          className="palette-query"
          type="text"
          role="combobox"
          autoComplete="off"
          aria-expanded="true"
          aria-controls="palette-rows"
          aria-activedescendant={
            aimed >= 0 && rows[aimed] ? `palette-row-${rows[aimed].id}` : undefined
          }
          placeholder="Type to narrow"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            // The aim goes back to `aim` on every keystroke: a row the arrows reached under
            // the last query is not the row that survived this one.
            setAt(undefined);
            setHeld(undefined);
          }}
          onKeyDown={(e) => {
            // Escape is not here: it is on the window, so it leaves from anywhere.
            if (e.key === "ArrowDown") {
              e.preventDefault();
              move(1);
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              move(-1);
            } else if (e.key === "Enter") {
              e.preventDefault();
              const row = aimed >= 0 ? rows[aimed] : undefined;
              if (row) run(row);
            }
          }}
        />

        {/* The core's sentence, unchanged, beside the rows. A refusal is an alert because it
            is the answer to something the operator just did; a report is a status. */}
        {showing && (
          <p
            className={showing.refused ? "refusal said" : "said"}
            role={showing.refused ? "alert" : "status"}
          >
            {showing.words}
          </p>
        )}
        {held && (
          <p className="held" role="status">
            {held}
          </p>
        )}

        {rows.length === 0 ? (
          <p className="none" role="status">
            No action matches what you typed.
          </p>
        ) : (
          <ul id="palette-rows" className="palette-rows" role="listbox" aria-label="Actions">
            {rows.map((row, index) => (
              <li
                key={row.id}
                id={`palette-row-${row.id}`}
                role="option"
                aria-selected={index === aimed}
                aria-disabled={row.available ? undefined : true}
                className={
                  (index === aimed ? "palette-row aimed" : "palette-row") +
                  (row.available ? "" : " refused")
                }
                onClick={() => run(row)}
              >
                <span className="palette-title">{row.title}</span>
                {/* The reason, as words. Dimming is decoration; this is the meaning, and it
                    is what a screen reader and a monochrome display both get. */}
                {!row.available && <span className="palette-why">{row.reason}</span>}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

/**
 * Whether this keystroke opens the palette.
 *
 * `F2` is the tmux frame's own key, carried over so an operator's fingers do not have to be
 * retrained, and it needs no modifier — which matters because a modifier's name differs by
 * platform and the scenario tests drive a real window. `⌘K` and `Ctrl-K` are what a desktop
 * app is expected to answer; both are accepted on both platforms rather than sniffing one,
 * because the wrong guess is an app with no palette at all.
 */
export function opensIt(e: KeyboardEvent): boolean {
  if (e.key === "F2") return !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey;
  return (e.key === "k" || e.key === "K") && (e.metaKey || e.ctrlKey) && !e.altKey;
}
