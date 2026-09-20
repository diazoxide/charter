import { useCallback, useEffect, useRef, useState } from "react";
import { aim, narrow, PASS_THROUGH_ID, PASS_THROUGH_KEY, type Offer, type Ran } from "./actions";

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
 * **The key it claims, it can hand back.** `F2` is taken on the window, capture-phase, so a
 * harness that wants `F2` never sees it — and used to have no way to (charter-app#47). A
 * second `F2` closes the palette and sends `F2` to the chat in front, which is `send-prefix`,
 * the idiom of the frame this app replaces. It is the catalogue's own `pane.sendkey` row that
 * runs, not a second path to the same thing, and the palette says so on screen the moment it
 * opens — a way out nobody can find is the same bug with more code in it.
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

  /** The rows and the dispatcher as they are right now, for the ONE keydown listener: it is
   *  registered once and must not be rebuilt on every render, so it cannot close over
   *  either. */
  const latest = useRef({ offers, onRun });
  useEffect(() => {
    latest.current = { offers, onRun };
  });

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

  /**
   * Runs a row and leaves if it ran. What Enter does, what a click does, and what the second
   * `F2` does — one function, so the chord can never become a second implementation of a row
   * the catalogue already describes.
   */
  const runOffer = useCallback(
    (offer: Offer) => {
      if (!offer.available) {
        // Nothing runs, and the reason is said rather than left to the dimming.
        setHeld(offer.reason);
        return;
      }
      setHeld(undefined);
      void (async () => {
        const ran = await latest.current.onRun(offer);
        // A refusal is not an ending: the operator is still here, still choosing, and the
        // catalogue may now offer them the answer to it.
        if (ran.ok) close();
      })();
    },
    [close],
  );

  /** The second press of the key that opened it, as the row that sends it. */
  const handBack = useCallback(() => {
    const offer = latest.current.offers.find((row) => row.id === PASS_THROUGH_ID);
    if (offer) runOffer(offer);
  }, [runOffer]);

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
        // Pressed again while it is already up. tmux answers this with `send-prefix` and so
        // does this: the second `F2` is the operator asking for the key ITSELF, so the
        // palette gets out of the way and the chat in front receives it. `⌘K`/`Ctrl-K` do
        // not hand anything back here — what a terminal makes of those is not this key's
        // question, and it is filed on its own.
        if (up.current) {
          if (e.key === PASS_THROUGH_KEY) handBack();
          return;
        }
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
  }, [close, handBack, onOpened]);

  // The box takes the keyboard as soon as it exists, so the first thing typed narrows.
  useEffect(() => {
    up.current = open;
    if (open) box.current?.focus();
  }, [open]);

  const rows = open ? narrow(query, offers) : [];
  const aimed = at ?? aim(rows);
  const aimedId = aimed >= 0 ? rows[aimed]?.id : undefined;

  // The aimed row, brought on screen. The list scrolls at 55vh and the catalogue is 119 rows
  // with fifty chats open (charter-app#48), so past the first screenful the arrows were
  // moving `aria-selected` onto a row nobody could see — an aim an operator cannot read is
  // not an aim. `block: "nearest"` scrolls only when it has to, so the list does not jump
  // under a row that was already visible. Optional call because jsdom has no layout and
  // therefore no `scrollIntoView`: a unit test must not fail for want of a scrollbar.
  useEffect(() => {
    if (aimedId === undefined) return;
    document.getElementById(`palette-row-${aimedId}`)?.scrollIntoView?.({ block: "nearest" });
  }, [aimedId]);

  if (!open) return null;

  const move = (by: number) => {
    if (rows.length === 0) return;
    const from = aimed < 0 ? (by > 0 ? -1 : 0) : aimed;
    setAt((from + by + rows.length) % rows.length);
  };

  const showing = said && said.words !== "" ? said : undefined;
  // The way out of the key this palette claimed, said where the person who needs it is
  // standing: they pressed `F2` meaning to send `F2`, and this is on screen the instant it
  // opened. Only while there is somewhere to send it — otherwise the row below says why, and
  // a hint promising something that would refuse is worse than no hint.
  const handsBack = offers.some((row) => row.id === PASS_THROUGH_ID && row.available);

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
              if (row) runOffer(row);
            }
          }}
        />

        {handsBack && (
          <p className="palette-through">
            Press {PASS_THROUGH_KEY} again to send {PASS_THROUGH_KEY} to the chat in front.
          </p>
        )}

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
                onClick={() => runOffer(row)}
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
