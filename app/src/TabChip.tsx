import {
  memo,
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import * as Menu from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, ChevronRight, Circle, Hand, SquareTerminal, X } from "lucide-react";
import { ChatShownState } from "./ChatRows";
import { sameList, useChatsHere, useChatsSelect, type Chats } from "./chatState";
import type { ChatRow, ListedChat } from "./chatsTree";
import { PersonaMark } from "./PersonaMark";
import { StateShown } from "./StateShown";
import {
  BUCKETS,
  GRACE_MS,
  REST_DRIFT,
  REST_MS,
  chipSaid,
  countsOf,
  kindsOf,
  menuOf,
  needsSaid,
  shownOf,
  sinceClock,
  sinceSaid,
  tasksIn,
  wearsChip,
  type Bucket,
  type Ended,
  type Line,
  type Needing,
  type SinceClock,
} from "./tabTasks";
import { property, type Token } from "./theme/theme";

/** Each count's shape and colour: the state's own (`shownState`), so a chip and a row agree. */
const DRAWN: Readonly<Record<Bucket, { Shape: typeof Circle; token: Token }>> = {
  working: { Shape: Circle, token: "state.running" },
  failed: { Shape: X, token: "state.failed" },
  done: { Shape: Check, token: "text.muted" },
};

type Props = {
  /** The session's name, as its tab says it: whose tasks these are. */
  name: string;
  /** The chats of the tab, as a tree (`tabChats.chatsOfTab`). */
  rows: readonly ChatRow[];
  /** Its tasks that have ended and still have a line. */
  ended: readonly Ended[];
  /** The chat the tab shows now. */
  current: number | undefined;
  /** Its chats waiting for the person and not on screen, the longest waiting first. */
  needs: readonly Needing[];
  /** Counts the times the window asked for this menu from the keyboard: a change opens it. */
  asked: number;
  /** The key that opens it, as this platform spells it. */
  keySaid: string;
  clock: SinceClock;
  /** Whether a tab is being carried along the strip: no menu opens under a drag. */
  dragging: () => boolean;
  /** Goes to a chat: the tab is switched to it, and its terminal takes the keyboard. */
  onShow: (session: number) => void;
  /** The menu's footer, for what later says it: the limits that bind (#1498) and what the
   *  tasks used (#1500). Nothing is drawn for a slot nobody fills. */
  limits?: ReactNode;
  totals?: ReactNode;
};

const ROW_FACTS = [
  "session",
  "name",
  "persona",
  "workspace",
  "level",
  "shell",
  "report",
  "outcome",
  "asking",
  "harness",
] as const;

function sameRows(one: readonly ChatRow[], other: readonly ChatRow[]): boolean {
  return (
    one === other ||
    (one.length === other.length &&
      one.every((row, at) => ROW_FACTS.every((fact) => row[fact] === other[at][fact])))
  );
}

function sameEnded(one: readonly Ended[], other: readonly Ended[]): boolean {
  return (
    one === other ||
    (one.length === other.length &&
      one.every((task, at) => {
        const with_ = other[at];
        return (
          task.key === with_.key &&
          task.session === with_.session &&
          task.name === with_.name &&
          task.shown.kind === with_.shown.kind &&
          task.shown.word === with_.shown.word
        );
      }))
  );
}

function sameNeeds(one: readonly Needing[], other: readonly Needing[]): boolean {
  return (
    one === other ||
    (one.length === other.length &&
      one.every((chat, at) => chat.session === other[at].session && chat.name === other[at].name))
  );
}

/** Whether a chip would be drawn the same: held on what it says, not on the lists' identity,
 *  which the window builds again whenever any tab changes. */
function sameChip(was: Props, now: Props): boolean {
  return (
    was.name === now.name &&
    was.current === now.current &&
    was.asked === now.asked &&
    was.keySaid === now.keySaid &&
    was.clock === now.clock &&
    was.dragging === now.dragging &&
    was.onShow === now.onShow &&
    was.limits === now.limits &&
    was.totals === now.totals &&
    sameRows(was.rows, now.rows) &&
    sameEnded(was.ended, now.ended) &&
    sameNeeds(was.needs, now.needs)
  );
}

/**
 * **The chip a session's tab wears for its tasks** (#1487, V100-32, V100-33, V100-37): how
 * many are working, failed and finished, each count with its state's shape, and the hand when
 * a chat of the tab is waiting for the person off screen. It opens the menu of the tab's
 * chats. A tab with no tasks draws nothing here, and is the tab it always was.
 *
 * **Beside the tab's button and not inside it**: the tab is a button, and a button holds no
 * button. The hand is its own button for the same reason: pressing it goes to the chat that
 * waits, and pressing the counts opens the menu.
 *
 * **It reads its own chats' states** (SC-3), so a task that moves redraws this chip, and only
 * when a count changes. The strip around it is not drawn again.
 *
 * **Opening.** A press opens it. The pointer resting {@link REST_MS} on the counts opens it; a
 * pointer passing over does not, and nothing opens under a drag. It closes on Escape, on a
 * press outside, on a pick, and {@link GRACE_MS} after the pointer has left both the chip and
 * the menu. A menu opened by a rest takes no keyboard: whoever was typing goes on typing.
 */
export const TabTasks = memo(function TabTasks(props: Props) {
  if (!wearsChip(props.rows, props.ended, props.needs)) return null;
  return <Chip {...props} />;
}, sameChip);

/** How a menu came to be open: what decides who has the keyboard and what closes it. */
type OpenedBy = "press" | "rest" | "keys";

function Chip({
  name,
  rows,
  ended,
  current,
  needs,
  asked,
  keySaid,
  clock,
  dragging,
  onShow,
  limits,
  totals,
}: Props) {
  // One reading of the store for the chip and its menu: each row's state, redrawn only when
  // one of them changes.
  const kinds = useChatsSelect(useChatsHere(), (states) => kindsOf(states, rows), sameList);
  const counts = countsOf(rows, kinds, ended);
  const [open, setOpen] = useState(false);
  const [unfolded, setUnfolded] = useState(false);
  // How it was opened: a ref for the handlers, which act in the turn it is set, and state for
  // what is drawn by it.
  const by = useRef<OpenedBy | undefined>(undefined);
  const [how, setHow] = useState<OpenedBy>();
  const resting = useRef<number | undefined>(undefined);
  const leaving = useRef<number | undefined>(undefined);
  const restedAt = useRef<{ x: number; y: number } | undefined>(undefined);
  /** Where the keyboard was when the menu opened: where it goes back to on a close. */
  const cameFrom = useRef<Element | null>(null);
  const pressedFrom = useRef<Element | null | undefined>(undefined);
  const picked = useRef(false);
  const outside = useRef(false);
  const content = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  const notResting = useCallback(() => {
    window.clearTimeout(resting.current);
    resting.current = undefined;
    restedAt.current = undefined;
  }, []);
  const notLeaving = useCallback(() => {
    window.clearTimeout(leaving.current);
    leaving.current = undefined;
  }, []);
  useEffect(
    () => () => {
      notResting();
      notLeaving();
    },
    [notLeaving, notResting],
  );

  const show = useCallback((opened: OpenedBy, from: Element | null) => {
    by.current = opened;
    setHow(opened);
    cameFrom.current = from;
    picked.current = false;
    outside.current = false;
    setOpen(true);
  }, []);
  const hide = useCallback(() => {
    notResting();
    notLeaving();
    setOpen(false);
    setUnfolded(false);
  }, [notLeaving, notResting]);

  // The window asked for this menu from the keyboard (the catalogue's row, its shortcut, or
  // Down on the tab). Held on the count, so being drawn is never being asked.
  const answered = useRef(asked);
  useEffect(() => {
    if (answered.current === asked) return;
    answered.current = asked;
    notResting();
    show("keys", document.activeElement);
  }, [asked, notResting, show]);

  const rest = (event: ReactPointerEvent) => {
    // Back on the chip: a close that was waiting for the pointer to be gone is off.
    notLeaving();
    if (event.pointerType === "touch" || open || dragging()) return;
    notResting();
    restedAt.current = { x: event.clientX, y: event.clientY };
    resting.current = window.setTimeout(() => {
      resting.current = undefined;
      if (dragging()) return;
      show("rest", document.activeElement);
    }, REST_MS);
  };
  const leaveSoon = () => {
    notResting();
    // A menu the keyboard opened is the keyboard's: a pointer that was never on it closes
    // nothing by wandering off.
    if (!open || by.current === "keys") return;
    notLeaving();
    leaving.current = window.setTimeout(hide, GRACE_MS);
  };

  const { lines, finished } = menuOf(rows, kinds, ended, current);
  // The fold is open where the chat on screen is in it, so its line is never hidden.
  const folded = !unfolded && !finished.some((line) => line.current);
  const quiet = how === "rest";
  const pick = (line: Line) => {
    picked.current = true;
    if (line.session !== undefined && line.row !== undefined) onShow(line.session);
  };
  const said = chipSaid(name, counts);

  return (
    <span className="tab-tasks" data-open={open || undefined}>
      {/* **A chat of this tab is waiting for you and is not on screen** (V100-37): the hand,
          and a press goes to the one that has waited longest. The tab stays there after. */}
      {needs.length > 0 && (
        <button
          type="button"
          className="tab-tasks-hand needs-you-mark"
          data-mark="needs-you"
          tabIndex={-1}
          aria-label={`${needsSaid(needs)}. Go to ${needs[0].name}`}
          title={`${needsSaid(needs)}. Press to go to ${needs[0].name}.`}
          onClick={() => onShow(needs[0].session)}
        >
          <Hand aria-hidden="true" />
        </button>
      )}
      {tasksIn(counts) > 0 && (
        <Menu.Root
          modal={false}
          open={open}
          onOpenChange={(next) => {
            // Radix's own asks: a key on the chip opens it, and Escape, a press outside and a
            // pick close it. The pointer's ways in are below.
            if (next) show("keys", document.activeElement);
            else hide();
          }}
        >
          <Menu.Trigger asChild>
            <button
              type="button"
              ref={trigger}
              className="tab-tasks-counts"
              // Not a Tab stop, as a tab's `×` is not: the strip is one stop. The keyboard
              // opens this menu with its key, with Down on the tab, or from the palette.
              tabIndex={-1}
              aria-label={said}
              title={`${said}. ${keySaid}`}
              // The click toggles it, not the pointer going down: not every way of pressing a
              // button sends a pointer event (`ShowMore` has the measurement).
              onPointerDown={(event) => {
                event.preventDefault();
                notResting();
                pressedFrom.current = document.activeElement;
              }}
              onClick={() => {
                const from = pressedFrom.current ?? document.activeElement;
                pressedFrom.current = undefined;
                notResting();
                notLeaving();
                if (dragging()) return;
                if (open && by.current === "rest") {
                  // A press on a menu the rest opened keeps it, and gives it the keyboard.
                  by.current = "press";
                  setHow("press");
                  content.current?.focus({ preventScroll: true });
                } else if (open) hide();
                else show("press", from === trigger.current ? null : from);
              }}
              onPointerEnter={rest}
              onPointerMove={(event) => {
                const at = restedAt.current;
                if (at === undefined) return;
                if (
                  Math.abs(event.clientX - at.x) > REST_DRIFT ||
                  Math.abs(event.clientY - at.y) > REST_DRIFT
                )
                  rest(event);
              }}
              onPointerLeave={leaveSoon}
            >
              {BUCKETS.filter((bucket) => counts[bucket] > 0).map((bucket) => {
                const { Shape, token } = DRAWN[bucket];
                return (
                  <span key={bucket} className="count" data-count={bucket} aria-hidden="true">
                    <Shape style={{ color: `var(${property(token)})` }} />
                    {counts[bucket]}
                  </span>
                );
              })}
            </button>
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Content
              ref={content}
              // Named by the chip that opened it, which Radix does: "Tasks of steward 4: …".
              className="tasks-menu"
              align="start"
              sideOffset={4}
              collisionPadding={8}
              onPointerEnter={notLeaving}
              onPointerLeave={leaveSoon}
              onInteractOutside={(event) => {
                // The chip is outside the menu and is not "outside": its own press says
                // what a press on it does, and a dismissal here would undo it.
                const on = event.target;
                if (on instanceof Node && trigger.current?.contains(on)) event.preventDefault();
                else outside.current = true;
              }}
              // A menu the rest opened took no keyboard, so its Escape is not the terminal's
              // too: it closes the menu and goes no further.
              onEscapeKeyDown={(event) => {
                if (by.current === "rest") event.stopPropagation();
              }}
              onCloseAutoFocus={(event) => {
                // A pick puts the keyboard in the chat picked (`onShow`), and a press outside
                // put it where it landed. Otherwise it goes back to where it was before the
                // menu, which for a rest is where it never left.
                const back = cameFrom.current;
                const gone = picked.current || outside.current;
                if (gone || back instanceof HTMLElement) event.preventDefault();
                if (gone || !(back instanceof HTMLElement) || !back.isConnected) return;
                const at = document.activeElement;
                if (at === null || at === document.body || !at.isConnected) back.focus();
              }}
              {...openFocus((event) => {
                // Radix puts the keyboard on the menu as it opens. A rest takes none; the
                // keyboard's own open lands on the chat the tab shows.
                if (how === "rest") event.preventDefault();
                else if (how === "keys" && event.target instanceof HTMLElement) {
                  const now = event.target.querySelector<HTMLElement>('[aria-current="true"]');
                  if (now) {
                    event.preventDefault();
                    now.focus({ preventScroll: true });
                  }
                }
              })}
            >
              {lines.map((line) => (
                <TaskLine key={line.key} line={line} clock={clock} quiet={quiet} onPick={pick} />
              ))}
              {finished.length > 0 && (
                <Menu.Item
                  className="tasks-menu-row tasks-menu-fold"
                  aria-expanded={!folded}
                  textValue="Finished"
                  onPointerMove={quiet ? keepsNoKeyboard : undefined}
                  onPointerLeave={quiet ? keepsNoKeyboard : undefined}
                  onSelect={(event) => {
                    // Unfolds in place: the menu stays.
                    event.preventDefault();
                    setUnfolded(folded);
                  }}
                >
                  {folded ? (
                    <ChevronRight aria-hidden="true" />
                  ) : (
                    <ChevronDown aria-hidden="true" />
                  )}
                  <span className="name">Finished ({finished.length})</span>
                </Menu.Item>
              )}
              {!folded &&
                finished.map((line) => (
                  <TaskLine key={line.key} line={line} clock={clock} quiet={quiet} onPick={pick} />
                ))}
              {(limits != null || totals != null) && (
                <div className="tasks-menu-foot">
                  {limits != null && <div className="tasks-menu-limits">{limits}</div>}
                  {totals != null && <div className="tasks-menu-totals">{totals}</div>}
                </div>
              )}
            </Menu.Content>
          </Menu.Portal>
        </Menu.Root>
      )}
    </span>
  );
}

/**
 * The menu's say in where the keyboard goes as it opens. Radix's menu takes this handler and
 * runs it before its own (`react-menu`, `onMountAutoFocus`); its dropdown's types leave it
 * out, so it is handed over under its name. `TabChip.window.test.tsx` holds it: a rest that
 * took the keyboard fails there.
 */
function openFocus(handler: (event: Event) => void): object {
  return { onOpenAutoFocus: handler };
}

/** A row under the pointer of a menu that took no keyboard is not given it either: Radix
 *  focuses the row the pointer comes onto and the menu as it leaves one, and here the
 *  stylesheet's hover says which row that is. */
function keepsNoKeyboard(event: ReactPointerEvent) {
  event.preventDefault();
}

/**
 * One chat of the tab, as its menu lists it (V100-19, V100-41): its persona's mark, its name,
 * the workspace it works in where that is not its session's, its state as a word that is never
 * cut, and how long it has been in it. Indented under the chat that asked for it. The chat the
 * tab shows now is marked, and said to be.
 */
function TaskLine({
  line,
  clock,
  quiet,
  onPick,
}: {
  line: Line;
  clock: SinceClock;
  /** Whether the menu took no keyboard (a rest opened it). */
  quiet: boolean;
  onPick: (line: Line) => void;
}) {
  const { row } = line;
  return (
    <Menu.Item
      className="tasks-menu-row"
      // How far in its line stands: one step per chat between it and the session.
      data-level={Math.min(line.level, DEEPEST)}
      aria-current={line.current ? "true" : undefined}
      // An ended task the tab is not showing has nothing to be shown.
      disabled={line.session === undefined}
      textValue={line.name}
      onPointerMove={quiet ? keepsNoKeyboard : undefined}
      onPointerLeave={quiet ? keepsNoKeyboard : undefined}
      onSelect={() => onPick(line)}
    >
      <span className="now" aria-hidden="true">
        {line.current && <ChevronRight />}
      </span>
      {line.persona === null ? (
        <SquareTerminal className="node-icon" aria-hidden="true" />
      ) : (
        <PersonaMark persona={line.persona} />
      )}
      {/* The spaces are for whoever reads the row as text: a screen reader says the name,
          then the state, and not the two run together. The row lays them out itself. */}
      <span className="name" title={line.name}>
        {line.name}
      </span>
      {line.current && <span className="hidden-words">, shown now</span>}
      {line.elsewhere !== null && <span className="where"> in {line.elsewhere}</span>}{" "}
      {row !== undefined ? (
        <ChatShownState
          session={row.session}
          shell={row.shell}
          report={row.report}
          outcome={row.outcome}
          asking={row.asking}
          harness={row.harness}
        />
      ) : (
        line.ended !== undefined && <StateShown shown={line.ended} />
      )}
      {row !== undefined && <Since clock={clock} session={row.session} />}
    </Menu.Item>
  );
}

/** The deepest step in a line is drawn at. A task of a task of a task of a task is rare, and
 *  one further in would leave its name no room. */
const DEEPEST = 6;

/** How often an open menu reads the time again. A line says seconds for its first minute. */
const TICK_MS = 10_000;

/** How long a chat has been in its state, where this window saw it come into it. */
function Since({ clock, session }: { clock: SinceClock; session: number }) {
  const at = useSyncExternalStore(clock.subscribe, () => clock.since(session));
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const tick = window.setInterval(() => setNow(Date.now()), TICK_MS);
    return () => window.clearInterval(tick);
  }, []);
  if (at === null) return null;
  return (
    <>
      {" "}
      <span className="since" title="How long it has been in this state, as this window saw it">
        {sinceSaid(Math.max(0, Math.floor((now - at) / 1000)))}
      </span>
    </>
  );
}

/**
 * **The project's clock of how long each chat has been in its state** (`tabTasks.sinceClock`),
 * kept by reading the chats' store itself: a chat that moves is timed, and whoever holds this
 * is not drawn again for it. A line of an open menu reads its own chat's time off it.
 */
export function useSinceClock(chats: Chats, listed: readonly ListedChat[]): SinceClock {
  const [clock] = useState(sinceClock);
  const { store, plane } = chats;
  useEffect(() => {
    const read = () => {
      const states = store.statesFor(plane);
      clock.read(
        new Map(listed.map((chat) => [chat.session, shownOf(states, chat)?.kind])),
        Date.now(),
      );
    };
    read();
    return store.subscribe(read);
  }, [clock, listed, plane, store]);
  return clock;
}
