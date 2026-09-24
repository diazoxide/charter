/** What a chat is doing, as the tab draws it, and the queue of chats asking for you. */
import { Hand, X } from "lucide-react";
import * as Menu from "@radix-ui/react-dropdown-menu";
import { useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import type { Offer } from "./actions";
import { type State } from "./chatState";
import { useArrived } from "./lib/arrived";
import { moveAlong } from "./tabSequence";
import { deletes } from "./tabKeys";

/** The word beside a chat's name. */
const WORDS: Record<State, string> = {
  unknown: "unknown",
  running: "running",
  waiting: "waiting on you",
  done: "done",
  failed: "failed",
};

export function ChatState({ state }: { state: State }) {
  // A state this mark CHANGED to, rather than the one it was drawn in: only the first moves,
  // so a workspace's tabs coming back into view do not all pulse at once (`useArrived`).
  const arrived = useArrived(state);
  return (
    <span
      className={`state state-${state}${arrived ? " arrived" : ""}`}
      data-state={state}
      // The word, never only a colour: a state told apart by colour alone is no state at all
      // to anyone who cannot see it, and `title` alone is no use on a touch screen or to a
      // screen reader reading a list. The mark is decorative; the label carries the meaning.
      role="img"
      aria-label={WORDS[state]}
      title={WORDS[state]}
    />
  );
}

/**
 * **A needs-you item's Ignore** (charter-app#248): the catalogue's `needs.ignore:<session>` row
 * drawn as the `✕` a pointer wants, so its accessible name is the row's words — "Ignore ide.3
 * until it asks again" — and the glyph is only the glyph.
 *
 * It drops the request and not the chat: the chat is still waiting, and its next stop puts it
 * back. The core holds that (`ignore_needs_you`), and the red counts on the project and
 * workspace tabs go down with the item because they are read from the same queue.
 *
 * **Out of the keyboard's way**, for the reason a tab's `×` is (charter-app#189): in the title
 * bar's list each chat is ONE item for the arrows, and fifty chats asking must not become a
 * hundred. The keyboard's way to it is Delete on the item ([`ignoreOnDelete`]) or the
 * palette's row.
 */
export function Ignore({ offer, onPress }: { offer?: Offer; onPress: (offer: Offer) => void }) {
  if (!offer) return null;
  return (
    <button
      className="needs-you-ignore"
      tabIndex={-1}
      aria-label={offer.title}
      title={offer.title}
      // A press does not take the keyboard: the ✕ leaves with its item, and focus on an
      // element that goes is focus on the page. It stays in the list, where the arrows work.
      onPointerDown={(event) => event.preventDefault()}
      onClick={() => onPress(offer)}
    >
      <X />
    </button>
  );
}

/**
 * **Delete on a needs-you item ignores it**: the platform's delete key (`tabKeys.deletes`), as
 * it closes a focused tab (charter-app#239), pressing the same row the item's `✕` does.
 *
 * The keyboard moves to the next item — or the one before, for the last — as it goes. The item
 * leaves only when the core's answer arrives, and focus left on an element that then goes is
 * focus on the page, where the next Delete does nothing; so it moves now, while there is
 * somewhere to move it.
 */
export function ignoreOnDelete(
  event: KeyboardEvent<HTMLElement>,
  offer: Offer | undefined,
  onPress: (offer: Offer) => void,
) {
  if (offer === undefined || !deletes(event)) return;
  event.preventDefault();
  const row = event.currentTarget.closest(".needs-you-row");
  const rows = [...(row?.parentElement?.querySelectorAll(".needs-you-row") ?? [])];
  const at = row ? rows.indexOf(row) : -1;
  const neighbour = rows[at + 1] ?? rows[at - 1];
  onPress(offer);
  neighbour?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
}

/** One chat asking for the operator, as its project reports it to the window. */
export type Asking = {
  session: number;
  /** What the chat is called there (`nameOf`), so a chat's name reads the same everywhere. */
  name: string;
  /** The workspace it is filed in, already said as the strip says it. */
  workspace: string;
  /** The catalogue's `needs.show:<session>`: the chat to the front, its workspace with it. */
  go?: Offer;
  /** The catalogue's `needs.ignore:<session>`. */
  ignore?: Offer;
};

/** The same, as the window lists it: with the project it is in, whose rows these are. */
export type Needing = Asking & {
  plane: string;
  /** The project, named as its tab names it. */
  project: string;
};

/** A chat that can be waiting on the operator without being able to say so (charter-app#52):
 *  a shell, or a harness without charter's hooks. */
export type Quiet = {
  name: string;
  /** The project it is in, named as its tab names it. */
  project: string;
};

/** What the faint hand says, in its name and its tooltip. */
function quietSaid(quiet: readonly Quiet[]): string {
  return quiet.length === 1
    ? `Nothing has asked for you, but ${quiet[0].name} can't tell charter it's waiting`
    : `Nothing has asked for you, but ${quiet.length} chats can't tell charter they're waiting`;
}

/**
 * **The needs-you queue, in the title bar** (charter-app#249): a hand and a count, and nothing
 * at all when nothing needs you. Pressed, it drops a list of every chat asking across every
 * project the window holds — its name, then its workspace and project — and each one has
 * **Go** (that chat to the front, its project and workspace with it) and **✕** (Ignore).
 *
 * It is the queue's ONLY place. It lived in the Attention panel, which is one project's and
 * one workspace's; a chat asking in a project behind the one on screen was a red number on
 * that project's tab and nothing more. The title bar is the window's, so it can hold them all.
 *
 * **A Radix menu** (ADR 0037), with the show-more menu's two decisions (`docs/ui-primitives.md`):
 * not modal, and a click outside closes it. So the keyboard is the primitive's — Enter opens it
 * on its first chat, the arrows move, Escape closes it and puts the keyboard back on the button.
 * Each chat is ONE menu item, which is its Go; the `✕` beside it is [`Ignore`], out of the
 * arrows' way as it is out of Tab's in the queue it came from, and Delete on the item is the
 * keyboard's way to it ([`ignoreOnDelete`]).
 *
 * **Three states, and the middle one is the honest one** (the operator's ruling on #249):
 *
 * - a chat has asked: the hand, and the count;
 * - nothing has asked, but a chat that cannot report is open — a shell, a harness without
 *   charter's hooks (charter-app#52): a **faint hand with no number**, whose name, tooltip and
 *   list say which chats those are. "Nothing needs you" would be a claim about a chat charter
 *   cannot see, and a blank bar says it without words;
 * - neither: nothing at all.
 *
 * The faint hand is the same button with the same menu, so the keyboard reaches it the same way.
 */
export function NeedsYouMenu({
  items,
  quiet,
  onPress,
}: {
  items: readonly Needing[];
  /** The chats that can be waiting without saying so, across every project. */
  quiet: readonly Quiet[];
  /** Carries a row out in the project it belongs to. */
  onPress: (plane: string, offer: Offer) => void;
}) {
  /**
   * Whether the list is up — held here rather than left to Radix, for the show-more menu's
   * measured reason (`PlaneView.ShowMore`): Radix opens on `pointerdown`, and the WebView a
   * scenario drives answers a click with no pointer event at all.
   */
  const [open, setOpen] = useState(false);
  /** Whether the list closed because a Go put the keyboard in a chat (see `onCloseAutoFocus`). */
  const went = useRef(false);
  /**
   * **The keyboard, when the last chat leaves.** Focus on an element that goes is focus on the
   * page, where the next key does nothing — the reason [`ignoreOnDelete`] moves it before the
   * item leaves. With no item left to move to, it goes to the faint hand when there is one, and
   * otherwise to the next Tab stop after where the button was, as Tab would
   * (`tabSequence.moveAlong`).
   * `held` is whether the keyboard was on the button or in the
   * list (React's focus events travel out of the portal the list is drawn in); a blur towards
   * somewhere else clears it, and the element being taken away is no such blur.
   */
  const anchor = useRef<HTMLSpanElement>(null);
  const held = useRef(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const asked = items.length > 0;
  const none = !asked && quiet.length === 0;
  // The button appearing because a chat has just asked, as opposed to having been there when
  // the bar was drawn: only the first is a change worth drawing (`useArrived`).
  const arrived = useArrived(asked);
  // A list that emptied is a list that closed: the next chat to ask brings the button back,
  // not a menu the operator did not open. Adjusted while rendering, React's own way to reset
  // state on a change of props.
  if (none && open) setOpen(false);
  useLayoutEffect(() => {
    if (asked || !held.current) return;
    held.current = false;
    const lost = document.activeElement === null || document.activeElement === document.body;
    if (!lost) return;
    if (trigger.current) trigger.current.focus();
    else if (anchor.current) moveAlong(anchor.current, false);
  }, [asked]);
  const said = asked
    ? `${items.length} ${items.length === 1 ? "chat needs" : "chats need"} you`
    : quietSaid(quiet);
  return (
    // `display: contents`: a place to be next to, not a box in the bar's row.
    <span
      ref={anchor}
      className="needs-you-anchor"
      onFocus={() => {
        held.current = true;
      }}
      onBlur={(event) => {
        if (event.relatedTarget !== null) held.current = false;
      }}
    >
      {!none && (
        <Menu.Root modal={false} open={open} onOpenChange={setOpen}>
          <Menu.Trigger asChild>
            {/* `tabIndex={0}`: WebKit leaves a `<button>` out of the tab sequence unless its
            `tabindex` is written down (`docs/ui-primitives.md`, charter-app#186), and Tauri's
            drag handler stops at it either way because it is a `<button>`. */}
            <button
              ref={trigger}
              type="button"
              className={`needs-you-button${asked ? "" : " muted"}${arrived && asked ? " arrived" : ""}`}
              data-testid="needs-you-button"
              tabIndex={0}
              aria-label={said}
              title={said}
              onPointerDown={(event) => event.preventDefault()}
              onClick={() => setOpen((up) => !up)}
            >
              <Hand aria-hidden="true" />
              {asked && <span className="needs-you-number">{items.length}</span>}
            </button>
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Content
              className="more-menu needs-you-menu"
              align="end"
              sideOffset={4}
              collisionPadding={8}
              onCloseAutoFocus={(event) => {
                // **Go leaves the keyboard in the chat it went to.** A pane that becomes the
                // focused one takes the keyboard itself (`SessionPane`), and Radix putting it back
                // on this button a tick later would take it away again. When Go put it nowhere —
                // the chat was already in front — the button gets it, as Escape's close does.
                const kept = went.current && document.activeElement !== document.body;
                went.current = false;
                if (kept) event.preventDefault();
              }}
            >
              {items.map((item) => {
                const press = (offer: Offer) => onPress(item.plane, offer);
                const where = `${item.name} · ${item.workspace} · ${item.project}`;
                return (
                  <Menu.Group
                    key={`${item.plane}#${item.session}`}
                    className="needs-you-row"
                    aria-label={item.name}
                  >
                    <Menu.Item
                      className="more-tab needs-you-go"
                      aria-label={`Go to ${where}`}
                      // Not `disabled`: a disabled item is skipped by the arrows, and Delete on
                      // it is still the keyboard's way to its Ignore. It says it cannot go.
                      aria-disabled={item.go?.available === false || undefined}
                      aria-keyshortcuts="Delete"
                      title={item.go?.available === false ? item.go.reason : `Go to ${where}`}
                      onSelect={(event) => {
                        if (!item.go?.available) {
                          event.preventDefault();
                          return;
                        }
                        went.current = true;
                        press(item.go);
                      }}
                      onKeyDown={(event) => ignoreOnDelete(event, item.ignore, press)}
                    >
                      <span className="needs-you-name">{item.name}</span>
                      <span className="needs-you-where">
                        {item.workspace} · {item.project}
                      </span>
                      <span className="needs-you-word">Go</span>
                    </Menu.Item>
                    <Ignore offer={item.ignore} onPress={press} />
                  </Menu.Group>
                );
              })}
              {quiet.length > 0 && (
                <Menu.Group className="needs-you-quiet" aria-label="Can't say they're waiting">
                  {quiet.map((one) => (
                    <p key={`${one.project}#${one.name}`}>
                      <span className="needs-you-name">{one.name}</span>
                      {` · ${one.project} can't tell charter it's waiting`}
                    </p>
                  ))}
                </Menu.Group>
              )}
            </Menu.Content>
          </Menu.Portal>
        </Menu.Root>
      )}
    </span>
  );
}
