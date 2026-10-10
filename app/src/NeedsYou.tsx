/** What a chat is doing, as the tab draws it, and the queue of chats asking for you. */
import { Hand, X } from "lucide-react";
import * as Menu from "@radix-ui/react-dropdown-menu";
import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import type { Offer } from "./actions";
import { backSaid, type State } from "./chatState";
import { PersonaMark, type PersonaMarkData } from "./PersonaMark";
import { AwayRows, GONE, awayKey, type AwayItem } from "./AwayRefusals";
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

/** {@link ChatState} where a chat has a mark to draw, and nothing where it has none (`markOf`). */
export function ChatMark({ state }: { state: State | undefined }) {
  return state === undefined ? null : <ChatState state={state} />;
}

/** What a wrapping-up chat's tab and explorer row say about it, as their tooltip. */
export const WRAPPING_UP =
  "Wrapping up: writing its session record, then it closes. Typing into it, or Cancel smart close on its menu, stops this.";

/** What a tab in the background says, as its tooltip and its name (SI-8f): the chip draws no
 *  name of its own. */
export function chipSays(name: string): string {
  return `${name} — wrapping up`;
}

/**
 * **A chat wrapping up** — being smart-closed (ADR 0064): a mark that breathes, as a queued
 * pipeline's does, because the chat is doing its last turn and will go. `charter-breathe` reads
 * its timing from the motion tokens, so under reduced motion it stands still at full weight.
 * The word is its accessible name, never only the colour.
 */
export function WrappingUp({ held }: { held: boolean }) {
  if (!held) return null;
  return (
    <span
      className="wrapping-up breathing"
      data-mark="wrapping-up"
      role="img"
      aria-label="wrapping up"
      title={WRAPPING_UP}
    />
  );
}

/** What a stopping chat's tab says about it, as its tooltip (#1459): what its row says, in full. */
export const STOPPING = "Stopping: it is writing what it did, then it ends. Its menu ends it now.";

/**
 * **A chat being stopped** (#1448, #1459): its tab's mark, as its row in the Chats list says
 * "Stopping…". A square, the stop's own shape, so it is told from a state and from a wrap-up's
 * diamond without colour; it breathes while the last turn runs and stands still under reduced
 * motion. The word is its accessible name.
 */
export function StoppingMark({ held }: { held: boolean }) {
  if (!held) return null;
  return (
    <span
      className="stopping-mark breathing"
      data-mark="stopping"
      role="img"
      aria-label="stopping"
      title={STOPPING}
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
  /** The persona it runs as, when it runs as one, and that persona's mark (#1449). Handed
   *  over whole, because the list is the window's and reads no one project's marks. */
  persona?: string | null;
  mark?: PersonaMarkData | null;
  /**
   * The chats that have reported back to this one and not been read, by name (charter-app#259).
   * A report is no item of its own (#1448); a chat that is here for another reason says
   * `<child> reported back` —
   * and Go still opens THIS chat, the one that asked, whose next turn is handed the report.
   */
  reported?: readonly string[];
  /** The chats it started that the operator stopped, by name (#1448): its row says
   *  `<child> was stopped`. */
  stoppedBelow?: readonly string[];
  /** Why the app found the chat needs the operator (#1448): a report of its own with nowhere to
   *  go. Said first, before anything about the chats it started. */
  needed?: string;
  /**
   * Why the chat needs the operator when it is not that it asked: its Smart close stopped
   * without a record (SI-8f). Its row then says `<name>: <why>`, and Go is still the chat.
   */
  why?: string;
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

/**
 * **A chat's permission prompt, held open for the operator** (HP-6): Claude Code asking to run
 * a tool, as the chat's own `PermissionRequest` hook handed it to the core. Answered here, it
 * goes back on that hook and the harness carries it out — the chat's pane never needs focus.
 */
export type PermissionAsk = {
  plane: string;
  /** The project, named as its tab names it. */
  project: string;
  session: number;
  /** What the chat is called there, as its needs-you row would say it. */
  name: string;
  /** The ask's id, which the answer names. */
  ask: string;
  /** What it asks, in one line, every credential shape masked. */
  says: string;
  /** The answers the harness offers, in its order and words. */
  options: readonly { id: string; label: string; allows: boolean }[];
};

/**
 * **Another ask from the registry** (#1690): a dispatch held for a grant, or a host a chat's
 * sandbox refused. Listed here so the number and the list agree; answered in its chat's Notice,
 * where what it asks is shown whole, until the Inbox answers it in place (#1692).
 */
export type OtherAsk = {
  plane: string;
  project: string;
  session: number;
  /** Who is asking, the session first: names chats chose, drawn as text. */
  chain: readonly string[];
  /** The registry's key for it. */
  ask: string;
  /** What it asks, in one line: data, never markup. */
  says: string;
};

/** A chat that can be waiting on the operator without being able to say so (charter-app#52):
 *  a shell, or a harness without charter's hooks. */
export type Quiet = {
  name: string;
  /** The project it is in, named as its tab names it. */
  project: string;
};

/** What the list held when it was opened: what it draws, in that order, until it closes. */
type Opened = {
  items: readonly Needing[];
  asks: readonly PermissionAsk[];
  away: readonly AwayItem[];
  /** The chats the person ignored from the list since it opened: they leave, never dimmed. */
  ignored: readonly string[];
};

const itemKey = (item: Needing) => `${item.plane}#${item.session}`;
const askKey = (ask: PermissionAsk) => `${ask.plane}#${ask.ask}`;

/**
 * The rows `drawn` in their order, each as it is `now` where it is still listed, and as it was
 * drawn — `gone` — where it is not. A row in `now` and not in `drawn` waits for the next
 * opening.
 */
function inPlace<T>(
  drawn: readonly T[],
  now: readonly T[],
  key: (row: T) => string,
): { row: T; gone: boolean }[] {
  const listed = new Map(now.map((row) => [key(row), row]));
  return drawn.map((row) => {
    const current = listed.get(key(row));
    return current === undefined ? { row, gone: true } : { row: current, gone: false };
  });
}

/** What the faint hand says, in its name and its tooltip. */
function quietSaid(quiet: readonly Quiet[]): string {
  return quiet.length === 1
    ? `Nothing has asked for you, but ${quiet[0].name} can't tell purlis it's waiting`
    : `Nothing has asked for you, but ${quiet.length} chats can't tell purlis they're waiting`;
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
 * **Where the window has a project in front, a press opens the Inbox instead** (#1692, I-2,
 * ADR 0038 as amended 2026-10-11): the hand is the Inbox's count and its way in, and this list
 * opens only where something asks it open (`openAsked`), until #1693 and #1695 retire it.
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
  asks = [],
  onAnswer,
  onOpen,
  away = [],
  onAllowAway,
  onDismissAway,
  onNeverAway,
  onLook,
  openAsked,
  asked,
  others = [],
  onOpenOther,
  onInbox,
}: {
  items: readonly Needing[];
  /** The chats that can be waiting without saying so, across every project. */
  quiet: readonly Quiet[];
  /** Carries a row out in the project it belongs to. */
  onPress: (plane: string, offer: Offer) => void;
  /** The permission prompts held open for the operator (HP-6), across every project. */
  asks?: readonly PermissionAsk[];
  /** Answers one of `asks` with the option chosen. */
  onAnswer?: (ask: PermissionAsk, option: string) => void;
  /** Puts the chat that asked in front, where its own prompt shows the ask whole. */
  onOpen?: (ask: PermissionAsk) => void;
  /**
   * The dispatches refused while nobody was there (#1507), across every project: items of
   * their own, attached to no chat. Counted in the hand's number and listed nowhere else.
   */
  away?: readonly AwayItem[];
  /** Allow from now on: the standing grant for the one pair the item names. */
  onAllowAway?: (item: AwayItem) => void;
  /** Dismiss: the item is put away and nothing is granted. */
  onDismissAway?: (item: AwayItem) => void;
  /** Never for this pair: the person's never, on this machine. */
  onNeverAway?: (item: AwayItem) => void;
  /** The person is coming to the list, or leaving it: what it holds is read again. */
  onLook?: () => void;
  /** A count that goes up each time the list is asked open from elsewhere: the away
   *  summary's part for the dispatches refused while nobody was there (#1551). */
  openAsked?: number;
  /**
   * **The registry's count of asks** (#1690): what the hand's number says, across every
   * project. Where the core has said none yet, the number counts the rows, as it always did.
   */
  asked?: number;
  /** The registry's other asks, each with Go to its chat. */
  others?: readonly OtherAsk[];
  /** Puts the chat an other ask came from in front, where its Notice asks it whole. */
  onOpenOther?: (ask: OtherAsk) => void;
  /**
   * **Opens the Inbox** (#1692, I-2): where there is one, a press of the hand, or Enter on it,
   * opens the Inbox and not this list. The list is still opened where something asks for it
   * (`openAsked`): the away summary's dispatches refused while nobody was there, until they
   * move into the Inbox too (#1693, #1695).
   */
  onInbox?: () => void;
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
  const chats = items.length + asks.length + others.length;
  const rows = chats + away.length;
  // The registry's count where it has one; a refusal kept while nobody was there is no ask.
  const count = asked ?? rows;
  const asking = rows > 0 || count > 0;
  const none = !asking && quiet.length === 0;
  // The button appearing because a chat has just asked, as opposed to having been there when
  // the bar was drawn: only the first is a change worth drawing (`useArrived`).
  const arrived = useArrived(asking);
  // A list that emptied is a list that closed: the next chat to ask brings the button back,
  // not a menu the operator did not open. Adjusted while rendering, React's own way to reset
  // state on a change of props.
  if (none && open) setOpen(false);
  useLayoutEffect(() => {
    if (asking || !held.current) return;
    held.current = false;
    const lost = document.activeElement === null || document.activeElement === document.body;
    if (!lost) return;
    if (trigger.current) trigger.current.focus();
    else if (anchor.current) moveAlong(anchor.current, false);
  }, [asking]);
  // A refusal kept while nobody was there is no chat needing you: the chat was told no and
  // went on. Alone, the hand says what they are; beside chats, it counts things.
  // What the rows are, where the registry counts none of them: a refusal kept while nobody
  // was there, or a row the registry has not read yet. Never "0 things".
  const rowsSaid =
    away.length === 0
      ? `${rows} ${rows === 1 ? "chat needs" : "chats need"} you`
      : chats === 0
        ? `${rows} ${rows === 1 ? "dispatch was" : "dispatches were"} refused while you were away`
        : `${rows} things need you`;
  const said = !asking
    ? quietSaid(quiet)
    : asked !== undefined && count > 0
      ? `${count} ${count === 1 ? "thing waits" : "things wait"} on you`
      : rowsSaid;
  /**
   * **The rows as they were when the list was opened** (#1507, #1146). A chat can ask, or a
   * refused chat ask again, at any moment, and the core then says the list anew; drawn at once,
   * that would move what is under the pointer, and a one-click answer — a standing grant among
   * them — would land on a row that moved. So the rows drawn are the ones the list opened on,
   * in their places, until it closes: one that went meanwhile is marked and its answers are
   * off, and one that came is drawn at the next opening. A row still listed is drawn as it is
   * now. The hand's number is not held still.
   */
  const [frozen, setFrozen] = useState<Opened | null>(null);
  const show = (up: boolean) => {
    setFrozen(up ? { items, asks, away, ignored: [] } : null);
    onLook?.();
    setOpen(up);
  };
  if (!open && frozen !== null) setFrozen(null);
  // **Asked open from elsewhere** (#1551): opened as a press opens it, on what it holds now;
  // a list with nothing in it stays shut. Adjusted while rendering, as the rest of it is; and,
  // as a press does, what it holds is read again (`onLook`), after the render.
  const [openedFor, setOpenedFor] = useState(openAsked);
  if (openAsked !== openedFor) {
    setOpenedFor(openAsked);
    if (!none) {
      setFrozen({ items, asks, away, ignored: [] });
      setOpen(true);
    }
  }
  const lookedFor = useRef(openAsked);
  useEffect(() => {
    if (openAsked === lookedFor.current) return;
    lookedFor.current = openAsked;
    onLook?.();
  }, [openAsked, onLook]);
  const heldRows = open ? frozen : null;
  const drawnAway = heldRows?.away ?? away;
  const listedNow = new Set(away.map(awayKey));
  // With nothing left to answer, nothing is held: the chats that went leave, and the keyboard
  // goes back to the hand as it did before anything was held.
  // Let go for good, not for as long as nothing is asked: a chat that asks while the list is
  // still open (the faint hand keeps it up) would otherwise bring the dimmed rows back.
  if (open && !asking && frozen !== null && frozen.items.length + frozen.asks.length > 0) {
    setFrozen({ ...frozen, items: [], asks: [] });
  }
  const heldChats = asking ? heldRows : null;
  const drawnItems = inPlace(heldChats?.items ?? items, items, itemKey).filter(
    ({ row, gone }) => !(gone && heldChats?.ignored.includes(itemKey(row))),
  );
  const drawnAsks = inPlace(heldChats?.asks ?? asks, asks, askKey);
  /**
   * **A row the person put away here is not held** (ruling on #1146): only a row that goes for
   * another reason — answered elsewhere, ended, moved on — stays in its place, dimmed. Ignore is
   * the one act that leaves the list open; its row leaves as it always did, when the core's
   * answer takes it off the list, and is not drawn dimmed in the meantime or after.
   */
  const ignore = (item: Needing, offer: Offer) => {
    const key = itemKey(item);
    setFrozen((held) => (held === null ? held : { ...held, ignored: [...held.ignored, key] }));
    onPress(item.plane, offer);
  };
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
        <Menu.Root
          modal={false}
          open={open}
          onOpenChange={(up) => (up && onInbox !== undefined ? onInbox() : show(up))}
        >
          <Menu.Trigger asChild>
            {/* `tabIndex={0}`: WebKit leaves a `<button>` out of the tab sequence unless its
            `tabindex` is written down (`docs/ui-primitives.md`, charter-app#186), and Tauri's
            drag handler stops at it either way because it is a `<button>`. */}
            <button
              ref={trigger}
              type="button"
              className={`needs-you-button${asking ? "" : " muted"}${arrived && asking ? " arrived" : ""}`}
              data-testid="needs-you-button"
              tabIndex={0}
              aria-label={said}
              title={said}
              // Opening the Inbox, it pops nothing up, so it does not say it does.
              aria-haspopup={onInbox === undefined ? "menu" : undefined}
              onPointerDown={(event) => event.preventDefault()}
              // Read before the list is drawn, so what it opens on is as things stand.
              onPointerEnter={() => onLook?.()}
              onFocus={() => onLook?.()}
              onClick={() => (onInbox !== undefined && !open ? onInbox() : show(!open))}
            >
              <Hand aria-hidden="true" />
              {asking && count > 0 && <span className="needs-you-number">{count}</span>}
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
              {drawnItems.map(({ row: item, gone }) => {
                const press = (offer: Offer) => onPress(item.plane, offer);
                const ignored = (offer: Offer) => ignore(item, offer);
                const go = gone ? undefined : item.go;
                const back =
                  item.needed ?? backSaid(item.reported ?? [], item.stoppedBelow) ?? item.why;
                const where = `${back ? `${item.name}: ${back}` : item.name} · ${item.workspace} · ${item.project}`;
                return (
                  <Menu.Group
                    key={itemKey(item)}
                    className={`needs-you-row${gone ? " gone" : ""}`}
                    aria-label={item.name}
                  >
                    <Menu.Item
                      className="more-tab needs-you-go"
                      aria-label={`Go to ${where}`}
                      // Not `disabled`: a disabled item is skipped by the arrows, and Delete on
                      // it is still the keyboard's way to its Ignore. It says it cannot go.
                      aria-disabled={gone || go?.available === false || undefined}
                      aria-keyshortcuts={gone ? undefined : "Delete"}
                      title={gone ? GONE : go?.available === false ? go.reason : `Go to ${where}`}
                      onSelect={(event) => {
                        if (!go?.available) {
                          event.preventDefault();
                          return;
                        }
                        went.current = true;
                        press(go);
                      }}
                      onKeyDown={(event) =>
                        ignoreOnDelete(event, gone ? undefined : item.ignore, ignored)
                      }
                    >
                      {item.persona != null && (
                        <PersonaMark persona={item.persona} mark={item.mark} />
                      )}
                      <span className="needs-you-name">{back ?? item.name}</span>
                      <span className="needs-you-where">
                        {item.workspace} · {item.project}
                      </span>
                      <span className="needs-you-word">Go</span>
                    </Menu.Item>
                    <Ignore offer={gone ? undefined : item.ignore} onPress={ignored} />
                  </Menu.Group>
                );
              })}
              {drawnAsks.map(({ row: ask, gone }) => (
                // **Answered here, not in the pane** (HP-6): each option the harness offered
                // is an item, and choosing it sends that answer back on the chat's own hook.
                // A press never moves the keyboard to the chat, which is the point.
                <Menu.Group
                  key={askKey(ask)}
                  className={`needs-you-row needs-you-permission${gone ? " gone" : ""}`}
                  aria-label={`${ask.name}: ${ask.says} · ${ask.project}`}
                >
                  <p className="needs-you-says">
                    <span className="needs-you-name">{`${ask.name}: ${ask.says}`}</span>
                    <span className="needs-you-where">{ask.project}</span>
                  </p>
                  {gone && <p className="needs-you-says needs-you-allows">{GONE}</p>}
                  {ask.options.map((option) => (
                    <Menu.Item
                      key={option.id}
                      disabled={gone}
                      className={`more-tab needs-you-answer${option.allows ? " allows" : ""}`}
                      aria-label={`${option.label}: ${ask.name}, ${ask.says}`}
                      onSelect={() => onAnswer?.(ask, option.id)}
                    >
                      {option.label}
                    </Menu.Item>
                  ))}
                  {/* The core offers Allow only where the line above IS the whole action
                      (`hooked::shown_in_full`); otherwise the pane shows it whole. */}
                  <Menu.Item
                    disabled={gone}
                    className="more-tab needs-you-open"
                    aria-label={`Open ${ask.name} in its pane`}
                    onSelect={() => {
                      went.current = true;
                      onOpen?.(ask);
                    }}
                  >
                    Open in its pane
                  </Menu.Item>
                </Menu.Group>
              ))}
              {others.map((ask) => {
                // **Every name and line is text** (#1690): a chain a chat named, or what it
                // asked, is drawn as words and is never a control of the list's.
                const who = ask.chain.join(" › ");
                return (
                  <Menu.Group
                    key={`${ask.plane}#${ask.ask}`}
                    className="needs-you-row needs-you-other"
                    aria-label={`${who}: ${ask.says} · ${ask.project}`}
                  >
                    <Menu.Item
                      className="more-tab needs-you-go"
                      aria-label={`Go to ${who}: ${ask.says} · ${ask.project}`}
                      onSelect={() => {
                        went.current = true;
                        onOpenOther?.(ask);
                      }}
                    >
                      <span className="needs-you-name">{`${who}: ${ask.says}`}</span>
                      <span className="needs-you-where">{ask.project}</span>
                      <span className="needs-you-word">Go</span>
                    </Menu.Item>
                  </Menu.Group>
                );
              })}
              <AwayRows
                items={drawnAway}
                gone={(item) => !listedNow.has(awayKey(item))}
                onAllow={onAllowAway}
                onDismiss={onDismissAway}
                onNever={onNeverAway}
              />
              {quiet.length > 0 && (
                <Menu.Group className="needs-you-quiet" aria-label="Can't say they're waiting">
                  {quiet.map((one) => (
                    <p key={`${one.project}#${one.name}`}>
                      <span className="needs-you-name">{one.name}</span>
                      {` · ${one.project} can't tell purlis it's waiting`}
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
