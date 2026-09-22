import { useEffect, useId, type ReactNode } from "react";
import * as ContextMenu from "@radix-ui/react-context-menu";
import { menuRows, type Catalogued, type MenuOn, type Offer } from "./actions";

/**
 * The window's context menus: right-click on a thing, and charter offers what it can do to it.
 *
 * **A menu is a third reader of `actions.ts`, and never a list of its own.** The palette reads
 * the catalogue and so does the bar; `actions.menuRows` filters the same catalogue to the item
 * the menu was opened on, and everything drawn here — the words, whether a row can run, the
 * reason it cannot, what it does — is the offer that came back. Nothing in this file knows what
 * any action means. That is the rule `actions.ts` opens with, and a menu is exactly the surface
 * the tmux frame grew a second answer on: `frame/tabmenu.py` and the palette drifted, and the
 * fix there was to make the menu a view of the one catalogue rather than to keep them in step.
 *
 * **Destructive rows are under a separator and marked.** The catalogue puts them last so that
 * Enter in the palette is never one keystroke from ending a chat. A context menu pops up under
 * the pointer, where the operator did not choose the layout — so the separator and the danger
 * colour are the same rule with more at stake, not less.
 *
 * **Not modal** (`modal={false}`), for the show-more menu's reason (`docs/ui-primitives.md`): a
 * modal Radix surface marks the rest of the window `aria-hidden`, which is right for a question
 * that must be answered and wrong for a menu on a strip. A click outside closes it, which is
 * what every menu on every platform does, and there is no answer to lose.
 *
 * Radix's `ContextMenu` (ADR 0037), so the keyboard, the roving focus, the dismissal and the
 * collision handling are the primitive's and not a fifth hand-written `ArrowDown`.
 */
export function Menued({
  on,
  offers,
  onPress,
  children,
}: {
  /** The item this menu is about. */
  on: MenuOn;
  /** The catalogue as it stands, by id — the same rows the palette lists and the bar reads,
   *  indexed once for the whole window. A menu on a strip is drawn per tab per render, so an
   *  array here would be a scan per tab per render over a list that #174 made half as long
   *  again; `actions.catalogued` has what that measured and why it was changed. */
  offers: Catalogued;
  onPress: (offer: Offer) => void;
  /** The element the menu belongs to. One element, because it is the trigger itself —
   *  `asChild`, so no wrapper is added to a strip whose layout is measured. */
  children: ReactNode;
}) {
  const rows = menuRows(on, offers);
  // Nothing to offer — a project this window holds while its catalogue has not been built yet,
  // for instance. The element is drawn exactly as it was, and `useNoBrowserMenu` still takes
  // the browser's own menu away: a surface with no charter menu must not fall back to
  // `Reload` and `Inspect Element`.
  if (rows.above.length === 0 && rows.below.length === 0) return <>{children}</>;
  return (
    <ContextMenu.Root modal={false}>
      <ContextMenu.Trigger asChild>{children}</ContextMenu.Trigger>
      <ContextMenu.Portal>
        <ContextMenu.Content className="item-menu" collisionPadding={8}>
          {rows.above.map((offer) => (
            <Row key={offer.id} offer={offer} onPress={onPress} />
          ))}
          {rows.above.length > 0 && rows.below.length > 0 && (
            <ContextMenu.Separator className="menu-line" />
          )}
          {rows.below.map((offer) => (
            <Row key={offer.id} offer={offer} onPress={onPress} dangerous />
          ))}
        </ContextMenu.Content>
      </ContextMenu.Portal>
    </ContextMenu.Root>
  );
}

/** One row of a menu: a catalogue offer, drawn.
 *
 *  Its words are `title` and its tooltip is the reason it cannot run or the note about what it
 *  costs — the same two strings, in the same order, that a bar button uses (`Doer`). A row that
 *  cannot run is drawn disabled with its reason rather than left out, which is the catalogue's
 *  own rule: an operator cannot ask about an option they cannot see. */
function Row({
  offer,
  onPress,
  dangerous,
}: {
  offer: Offer;
  onPress: (offer: Offer) => void;
  /** Whether this row is below the line — it loses something, and says so in the colour as
   *  well as in the words. */
  dangerous?: boolean;
}) {
  const note = offer.available ? offer.note : undefined;
  const noted = useId();
  return (
    <ContextMenu.Item
      className={dangerous ? "menu-row ends-it" : "menu-row"}
      disabled={!offer.available}
      title={offer.reason || offer.note || undefined}
      // **The row's NAME is the catalogue's title and nothing else**, and the note is its
      // description. Left to the content, an accessible name would be the title with the
      // consequence run onto the end of it — "End chat 3 steward Ends the program it runs." —
      // so every row would announce as a paragraph and no surface could ask for one by name.
      // The note is still read, as a description, which is where a consequence belongs.
      aria-label={offer.title}
      aria-describedby={note ? noted : undefined}
      onSelect={() => onPress(offer)}
    >
      <span className="menu-title">{offer.title}</span>
      {/* What the row costs, under its words. Only where the catalogue wrote one, and never
          for a row that cannot run — then the tooltip and the grey say what is true instead. */}
      {note && (
        <span className="menu-note" id={noted}>
          {note}
        </span>
      )}
    </ContextMenu.Item>
  );
}

/**
 * Takes the WebView's own context menu away from the whole window.
 *
 * **A shipped app that answers a right-click with `Reload` and `Inspect Element` is showing the
 * operator the browser it is built on.** charter's menus above are the answer where there is
 * one; this is the answer everywhere else, and it is one listener rather than an
 * `onContextMenu` on every element, because the thing being removed is a default and a default
 * is not per-element.
 *
 * **On the bubble phase, and that is load-bearing.** Radix opens its menu from an
 * `onContextMenu` composed with `composeEventHandlers`, which skips its own handler when the
 * event is already `defaultPrevented`. React 19 attaches its delegated listeners to the root
 * container, which is below `window`, so a CAPTURING listener here would run first, prevent the
 * default, and silently stop every menu in this file from ever opening. On the bubble phase
 * Radix has already opened — and already prevented the default itself — and this only covers
 * what it did not.
 *
 * **Except where the operator is editing text.** An `input`, a `textarea` or a contenteditable
 * answers a right-click with the platform's own Cut/Copy/Paste, and that menu is not the
 * browser showing through — it is the only pointer route to the clipboard this window has.
 * Taking it away to hide `Inspect Element` would cost paste in the new-workspace box and in the
 * palette to save a line an operator never reads in a release build, where the WebView's
 * inspector is off. Deliberate, and narrow: it is the editing menu that survives, on the
 * element being edited, and nothing else.
 */
export function useNoBrowserMenu() {
  useEffect(() => {
    const suppress = (event: MouseEvent) => {
      if (editing(event.target)) return;
      event.preventDefault();
    };
    window.addEventListener("contextmenu", suppress);
    return () => window.removeEventListener("contextmenu", suppress);
  }, []);
}

/** Whether the right-click landed on something the operator types into. */
function editing(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA";
}
