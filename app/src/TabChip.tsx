import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import * as Menu from "@radix-ui/react-dropdown-menu";
import {
  Check,
  ChevronDown,
  ChevronRight,
  Circle,
  FileText,
  Hand,
  Minus,
  PanelRight,
  Pause,
  SquareArrowOutUpRight,
  SquareTerminal,
  X,
  type LucideIcon,
} from "lucide-react";
import type { Offer } from "./actions";
import { backId, besideId, ownTabId } from "./actions";
import { onAMac } from "./tabKeys";
import { briefTitle, useOpenBrief, type BriefAsk } from "./Brief";
import { ChatDoingLine } from "./ChatRowActivity";
import { ChatShownState } from "./ChatRows";
import { sameList, useChatsHere, useChatsSelect } from "./chatState";
import { sinceSaid } from "./chatsList";
import type { ChatRow } from "./chatsTree";
import { PersonaMark } from "./PersonaMark";
import { useStateSince, type StateClock } from "./stateClock";
import { StateShown } from "./StateShown";
import {
  GRACE_MS,
  REST_DRIFT,
  REST_MS,
  chipSaid,
  countsOf,
  kindsOf,
  menuOf,
  needsSaid,
  wearsChip,
  type Ended,
  type Line,
  type Needing,
} from "./tabTasks";
import { TASK_BUCKETS, TASK_BUCKET_DRAWN, tasksIn, type TaskBucket } from "./taskBuckets";
import type { TasksUsed } from "./bindings";
import { askOf, tokensOfLine, tokensSaid, type UsedAsk, type UsedReader } from "./tasksUsed";
import { property } from "./theme/theme";
import { fitCountsOn } from "./wholeWays";

/** Each count's mark: its shape as it is drawn (`StateShown` draws the same four for a row). */
const MARKS: Readonly<Record<TaskBucket, LucideIcon>> = {
  working: Circle,
  waiting: Pause,
  failed: X,
  done: Check,
};

type Props = {
  /** The id of the counts button: what the tab is described by, so a screen reader on the
   *  tab hears that it has tasks. */
  id: string;
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
  /** How many times the window has asked for a tab's menu from the keyboard, while this tab's
   *  is the one asked for, and 0 while another's is. A rise opens it, or closes it if it is
   *  open; a fall to 0 closes it, so one menu is open at a time. */
  asked: number;
  /** How long each chat has been in its state, as this window saw it. */
  clock: StateClock;
  /** Whether a tab is being carried along the strip: no menu opens under a drag. */
  dragging: () => boolean;
  /** Goes to a chat: the tab is switched to it, and its terminal takes the keyboard. */
  onShow: (session: number) => void;
  /** Moves a task (#1489): to a tab of its own, beside its session, or back out of either.
   *  Left out, a task's line offers none of them. */
  onPlace?: (session: number, where: Place) => void;
  /** The menu's footer, for what later says it: the limits that bind (#1498). Nothing is
   *  drawn for a slot nobody fills. */
  limits?: ReactNode;
  /**
   * Reads what the tab's chats used (#1500): each line's tokens, on its hover, and the total
   * line at the menu's foot. Read as the menu opens and when a chat of it changes state while
   * it is open, never on a timer. Left out, the menu says nothing of tokens.
   */
  used?: UsedReader;
  /**
   * The two rows that end task `session`, from the window's catalogue (#1488,
   * `actions.taskEndIds`): Stop and get its report, then Close now. Read as the menu is
   * drawn. Left out, the menu offers no way to end a task.
   */
  ends?: (session: number) => readonly Offer[];
  /** Presses one of those rows: the window asks its second step, and nothing ends on the
   *  press (`TaskEnd.tsx`). */
  onPress?: (offer: Offer) => void;
  /**
   * Stop all tasks of session `session`, from the window's catalogue (#1498,
   * `actions.stopAllId`): the last row of "End a task", under a line of its own. Pressed
   * through `onPress`, so the window asks its one question first. Left out, or not available,
   * it is not drawn.
   */
  stopAll?: (session: number) => Offer | undefined;
  /** Opens the Activity of chat `session`, the tab's own (#1495): what it and its tasks said
   *  to each other. Left out, the menu has no Activity line. */
  onActivity?: (session: number) => void;
};

/** Where a task's line in a tab's menu offers to move it (#1489). */
export type Place = "own" | "beside" | "back";

/** The catalogue's row for moving task `session` to `where`: the one its line presses. */
export function placeRowId(session: number, where: Place): string {
  return where === "own"
    ? ownTabId(session)
    : where === "beside"
      ? besideId(session)
      : backId(session);
}

const ROW_FACTS = [
  "session",
  "name",
  "persona",
  "workspace",
  "level",
  "placed",
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

const ENDED_FACTS = [
  "key",
  "session",
  "asker",
  "name",
  "persona",
  "qualifier",
  "folds",
  "bucket",
  "elsewhere",
  "report",
] as const;

function sameEnded(one: readonly Ended[], other: readonly Ended[]): boolean {
  return (
    one === other ||
    (one.length === other.length &&
      one.every((task, at) => {
        const against = other[at];
        return (
          ENDED_FACTS.every((fact) => task[fact] === against[fact]) &&
          task.shown.kind === against.shown.kind &&
          task.shown.word === against.shown.word
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
    was.id === now.id &&
    was.name === now.name &&
    was.current === now.current &&
    was.asked === now.asked &&
    was.clock === now.clock &&
    was.dragging === now.dragging &&
    was.onShow === now.onShow &&
    was.onPlace === now.onPlace &&
    was.limits === now.limits &&
    was.used === now.used &&
    was.ends === now.ends &&
    was.onPress === now.onPress &&
    was.stopAll === now.stopAll &&
    was.onActivity === now.onActivity &&
    sameRows(was.rows, now.rows) &&
    sameEnded(was.ended, now.ended) &&
    sameNeeds(was.needs, now.needs)
  );
}

/**
 * **The chip a session's tab wears for its tasks** (#1487, V100-32, V100-33, V100-37): how
 * many are working, waiting, failed and finished (`taskCounts.ts`), each count with its
 * state's shape, and the hand when a chat of the tab is waiting for the person off screen. It
 * opens the menu of the tab's chats. A tab with no tasks draws nothing here, and is the tab it
 * always was.
 *
 * **Beside the tab's button and not inside it**: the tab is a button, and a button holds no
 * button. The hand is its own button for the same reason: pressing it goes to the chat that
 * waits, and pressing the counts opens the menu.
 *
 * **It reads its own chats' states** (SC-3), so a task that moves redraws this chip, and only
 * when a state changes. The strip around it is not drawn again.
 *
 * **Three ways in, and each closes its own way.**
 * - **A press**, and **the keyboard** (the window's key, Down on the tab, the palette): the
 *   menu has the keyboard, and stays until Escape, a pick, a press outside, or the same ask
 *   again. A pointer wandering off closes neither: the next keys would go to the terminal.
 * - **A rest**: the pointer, moved onto the counts by the person's hand and then still for
 *   {@link REST_MS}. That menu takes no keyboard, so whoever was typing goes on typing, and it
 *   also closes {@link GRACE_MS} after the pointer has left both the chip and the menu.
 *
 * **A rest is only ever the person's.** The engine sends "the pointer came onto this" when a
 * chip appears, widens or slides under a pointer that has not moved. So coming on arms
 * nothing: the rest starts at the first move to a new point, with no button held. A pointer
 * passing over is on the chip for less than a rest, and nothing opens under a drag.
 */
export const TabTasks = memo(function TabTasks(props: Props) {
  if (!wearsChip(props.rows, props.ended, props.needs)) return null;
  return <Chip {...props} />;
}, sameChip);

/** How a menu came to be open: what decides who has the keyboard and what closes it. */
type OpenedBy = "press" | "rest" | "keys";

type Point = { x: number; y: number };

const NO_REPORTS: ReadonlySet<string> = new Set();

function Chip({
  id,
  name,
  rows,
  ended,
  current,
  needs,
  asked,
  clock,
  dragging,
  onShow,
  onPlace,
  limits,
  used,
  ends,
  onPress,
  stopAll,
  onActivity,
}: Props) {
  // One reading of the store for the chip and its menu: each row's state, redrawn only when
  // one of them changes.
  const kinds = useChatsSelect(useChatsHere(), (states) => kindsOf(states, rows), sameList);
  const counts = countsOf(rows, kinds, ended);
  // The Brief panel of a task (#1494), where the window has one to open.
  const openBrief = useOpenBrief();
  const [open, setOpen] = useState(false);
  const [unfolded, setUnfolded] = useState(false);
  /** The ended lines whose reports are open, by the line's key. */
  const [reports, setReports] = useState(NO_REPORTS);
  // How it was opened: a ref for the handlers, which act in the turn it is set, and state for
  // what is drawn by it.
  const by = useRef<OpenedBy | undefined>(undefined);
  const [how, setHow] = useState<OpenedBy>();
  // Whether it is open, for the effect that answers the window's ask in the turn it lands.
  const isOpen = useRef(false);
  const resting = useRef<number | undefined>(undefined);
  const leaving = useRef<number | undefined>(undefined);
  /** Where the pointer was last seen over the counts, and where its rest is counted from. */
  const seenAt = useRef<Point | undefined>(undefined);
  const restedAt = useRef<Point | undefined>(undefined);
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
    isOpen.current = true;
    setOpen(true);
  }, []);
  /** Whether "End a task" has its rows open (`Menu.Sub`). Held here, so a menu the rest
   *  opened, which takes no keyboard, can open it by the pointer too. */
  const [endsOpen, setEndsOpen] = useState(false);
  const endsSoon = useRef(0);
  const hide = useCallback(() => {
    notResting();
    notLeaving();
    isOpen.current = false;
    setOpen(false);
    window.clearTimeout(endsSoon.current);
    setEndsOpen(false);
    setUnfolded(false);
    setReports(NO_REPORTS);
  }, [notLeaving, notResting]);

  // **The window asked for a tab's menu from the keyboard** (the catalogue's row, its key, or
  // Down on the tab). The count only grows, and is this tab's only while this tab's menu is
  // the one asked for. So a rise is an ask of this tab: it opens, or closes if it is open, as
  // a second press does. A fall to 0 is an ask of another tab: one menu at a time.
  const answered = useRef(asked);
  useEffect(() => {
    const was = answered.current;
    answered.current = asked;
    if (asked === was) return;
    notResting();
    if (asked < was || isOpen.current) hide();
    else show("keys", document.activeElement);
  }, [asked, hide, notResting, show]);

  /** The pointer moved over the counts: a rest is counted from where it stopped. */
  const moved = (event: ReactPointerEvent) => {
    const at = { x: event.clientX, y: event.clientY };
    const last = seenAt.current;
    seenAt.current = at;
    // The engine's own "it is still here", sent when the layout moves under a pointer that
    // has not: the same point again. Not the person's hand, and it arms nothing.
    if (last !== undefined && last.x === at.x && last.y === at.y) return;
    // A button is down: a selection being dragged over the strip, or the press that begins a
    // tab's drag. Not a pointer coming to rest.
    if (event.buttons !== 0 || event.pointerType === "touch" || isOpen.current || dragging()) {
      notResting();
      return;
    }
    const from = restedAt.current;
    if (
      from !== undefined &&
      Math.abs(at.x - from.x) <= REST_DRIFT &&
      Math.abs(at.y - from.y) <= REST_DRIFT
    )
      return;
    notResting();
    restedAt.current = at;
    resting.current = window.setTimeout(() => {
      resting.current = undefined;
      if (dragging() || isOpen.current) return;
      show("rest", document.activeElement);
    }, REST_MS);
  };
  /** The pointer left the chip or the menu: a menu its rest opened closes soon. */
  const leaveSoon = () => {
    notResting();
    seenAt.current = undefined;
    // Only a menu the rest opened. A pressed or a key-opened menu has the keyboard, and the
    // keys typed after the pointer drifts off must find it still there.
    if (!isOpen.current || by.current !== "rest") return;
    notLeaving();
    leaving.current = window.setTimeout(hide, GRACE_MS);
  };

  const { lines, finished } = menuOf(rows, kinds, ended, current);
  const figures = useUsed(used, open, [...lines, ...finished], kinds);
  // The fold is open where the chat on screen is in it, so its line is never hidden.
  const folded = !unfolded && !finished.some((line) => line.current);
  const quiet = how === "rest";
  const pick = (line: Line): boolean => {
    if (line.row === undefined) {
      // No chat to go to: the task has ended. Its line opens its report, where it has one,
      // and the menu stays with the keyboard where it is.
      if (line.report !== undefined)
        setReports((was) => {
          const next = new Set(was);
          if (!next.delete(line.key)) next.add(line.key);
          return next;
        });
      return false;
    }
    picked.current = true;
    onShow(line.row.session);
    return true;
  };
  const said = chipSaid(name, counts);
  // **Each count whole or not at all**, and only once the name has given up what it can:
  // measured, as the tab strip is (`wholeWays.ts`).
  const anyTasks = tasksIn(counts) > 0;
  useLayoutEffect(() => {
    if (!anyTasks || trigger.current === null) return;
    return fitCountsOn(trigger.current);
  }, [anyTasks]);
  /** The tab's open tasks, each with the rows that end it: what "End a task" lists. Never a
   *  pane's own chat, which is no task, and never a task that has ended. */
  const endable =
    ends === undefined || onPress === undefined
      ? []
      : lines.flatMap((one) =>
          one.row !== undefined && one.row.mode === "task"
            ? [{ key: one.key, offers: ends(one.row.session) }]
            : [],
        );
  /** The tab's own session: whose tasks Stop all ends (#1498) and whose Activity this menu
   *  opens (#1495). */
  const own = rows.find((row) => row.level === 1)?.session;
  const all =
    stopAll === undefined || onPress === undefined || own === undefined ? undefined : stopAll(own);
  const allOffer = all?.available === true ? all : undefined;
  const line = (one: Line) => (
    <TaskLine
      key={one.key}
      line={one}
      clock={clock}
      quiet={quiet}
      tokens={tokensSaid(tokensOfLine(figures, one))}
      reportOpen={reports.has(one.key)}
      onPick={pick}
      onBrief={
        openBrief === undefined
          ? undefined
          : (ask) => {
              // The panel takes the keyboard: the menu is done.
              picked.current = true;
              hide();
              openBrief(ask);
            }
      }
      onPlace={
        onPlace === undefined
          ? undefined
          : (session, where) => {
              // The task is gone to where it was sent, with the keyboard: the menu is done.
              picked.current = true;
              onPlace(session, where);
              hide();
            }
      }
    />
  );

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
          aria-label={`${needsSaid(needs.map((chat) => chat.name))}. Go to ${needs[0].name}`}
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
              id={id}
              ref={trigger}
              className="tab-tasks-counts"
              // Not a Tab stop, as a tab's `×` is not: the strip is one stop. The keyboard
              // opens this menu with its key, with Down on the tab, or from the palette.
              // No `title`: a tooltip would come up over the menu the same rest opened.
              tabIndex={-1}
              aria-label={said}
              // The click toggles it, not the pointer going down: not every way of pressing a
              // button sends a pointer event (`ShowMore` has the measurement).
              onPointerDown={(event) => {
                event.preventDefault();
                notResting();
                pressedFrom.current = document.activeElement;
              }}
              // A press that did not end in a click (a right press, one let go elsewhere) is
              // over: where the keyboard was then says nothing of the next press.
              onPointerUp={() => {
                window.setTimeout(() => (pressedFrom.current = undefined));
              }}
              onPointerCancel={() => {
                pressedFrom.current = undefined;
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
              // Coming on arms nothing (see `TabTasks`): it is only where the pointer is.
              onPointerEnter={(event) => {
                notLeaving();
                seenAt.current = { x: event.clientX, y: event.clientY };
              }}
              onPointerMove={moved}
              onPointerLeave={leaveSoon}
            >
              {TASK_BUCKETS.filter((bucket) => counts[bucket] > 0).map((bucket) => {
                const Mark = MARKS[bucket];
                return (
                  <span key={bucket} className="count" data-count={bucket} aria-hidden="true">
                    <Mark style={{ color: `var(${property(TASK_BUCKET_DRAWN[bucket].token)})` }} />
                    {counts[bucket]}
                  </span>
                );
              })}
            </button>
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Content
              ref={content}
              // Named by the chip that opened it: "Tasks of steward 4: …". Said here because
              // the chip's id is the window's (the tab is described by it), not Radix's own.
              aria-labelledby={id}
              className="tasks-menu"
              align="start"
              sideOffset={4}
              collisionPadding={8}
              onPointerEnter={notLeaving}
              onPointerLeave={leaveSoon}
              // In a menu that took no keyboard, Radix closes the ways to end a task when the
              // keyboard leaves them, which it never came to: a move onto another of this
              // menu's rows closes them instead. Moves in the ways' own rows, which are this
              // menu's in the tree, and on the line that opens them, keep them.
              onPointerMove={(event) => {
                if (!quiet || !endsOpen) return;
                const on = event.target;
                if (on instanceof Element && on.closest(".tasks-menu-end, .tasks-menu-ends"))
                  return;
                setEndsOpen(false);
              }}
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
                  const now = event.target.querySelector<HTMLElement>("[data-current]");
                  if (now) {
                    event.preventDefault();
                    now.focus({ preventScroll: true });
                  }
                }
              })}
            >
              {lines.map(line)}
              {(endable.some((task) => task.offers.length > 0) || allOffer !== undefined) && (
                /* **Ending a task from here** (#1488): the two rows the catalogue has for each
                   open task, by their own titles, so this menu, a row's menu, the breadcrumb
                   and the palette say one thing. One line of this menu, which opens to them:
                   a way to end a task is never one stray press from a way to go to it. A
                   press ends nothing: the window asks its second step. */
                <Menu.Sub
                  open={endsOpen}
                  onOpenChange={(next) => {
                    window.clearTimeout(endsSoon.current);
                    setEndsOpen(next);
                  }}
                >
                  <Menu.SubTrigger
                    className="tasks-menu-row tasks-menu-end"
                    textValue="End a task"
                    // **In a menu the rest opened, the pointer opens these rows too.** The
                    // move is kept from Radix there (it would give the row the keyboard), and
                    // that kept Radix's own opening as well: this is it, after the same
                    // moment Radix waits.
                    onPointerMove={
                      quiet
                        ? (event) => {
                            keepsNoKeyboard(event);
                            if (endsOpen || endsSoon.current !== 0) return;
                            endsSoon.current = window.setTimeout(() => {
                              endsSoon.current = 0;
                              setEndsOpen(true);
                            }, ENDS_OPEN_AFTER);
                          }
                        : undefined
                    }
                    onPointerLeave={
                      quiet
                        ? (event) => {
                            keepsNoKeyboard(event);
                            window.clearTimeout(endsSoon.current);
                            endsSoon.current = 0;
                          }
                        : undefined
                    }
                  >
                    <span className="name">End a task</span>
                    <ChevronRight aria-hidden="true" />
                  </Menu.SubTrigger>
                  <Menu.Portal>
                    <Menu.SubContent
                      // Named by the line that opened it, which is Radix's own doing.
                      className="tasks-menu tasks-menu-ends"
                      collisionPadding={8}
                    >
                      {endable.flatMap((task) =>
                        task.offers.map((offer) => (
                          <Menu.Item
                            key={offer.id}
                            className="tasks-menu-row"
                            textValue={offer.title}
                            // A row that cannot run stays in the arrows' way and says why.
                            aria-disabled={offer.available ? undefined : true}
                            onSelect={(event) => {
                              if (!offer.available) {
                                event.preventDefault();
                                return;
                              }
                              // The keyboard goes to the question the press opens, not
                              // back to where it was before the menu.
                              picked.current = true;
                              onPress?.(offer);
                            }}
                          >
                            <span className="name">{offer.title}</span>
                            {!offer.available && <span className="where"> {offer.reason}</span>}
                          </Menu.Item>
                        )),
                      )}
                      {allOffer !== undefined && (
                        /* **Stop all tasks** (#1498, V100-53): its own row, last, under a line,
                           since it ends every task above it. The session keeps running. A
                           press stops nothing: the window asks once, naming how many. */
                        <>
                          <Menu.Separator className="tasks-menu-line" />
                          <Menu.Item
                            className="tasks-menu-row tasks-menu-stop-all"
                            textValue="Stop all tasks"
                            onSelect={() => {
                              picked.current = true;
                              onPress?.(allOffer);
                            }}
                          >
                            <span className="name">Stop all tasks</span>
                          </Menu.Item>
                        </>
                      )}
                    </Menu.SubContent>
                  </Menu.Portal>
                </Menu.Sub>
              )}
              {own !== undefined && onActivity !== undefined && (
                /* **The session's Activity** (#1495, V100-44): what its chat and its tasks
                   said to each other, in a tab of its own. It only reads. */
                <Menu.Item
                  className="tasks-menu-row tasks-menu-activity"
                  textValue="Activity"
                  onPointerMove={quiet ? keepsNoKeyboard : undefined}
                  onPointerLeave={quiet ? keepsNoKeyboard : undefined}
                  onSelect={() => {
                    picked.current = true;
                    onActivity(own);
                  }}
                >
                  <span className="name">Activity</span>
                  <span className="where">what its chats said to each other</span>
                </Menu.Item>
              )}
              {finished.length > 0 && (
                <Menu.Item
                  className="tasks-menu-row tasks-menu-fold"
                  aria-expanded={!folded}
                  textValue="Finished"
                  onPointerMove={quiet ? keepsNoKeyboard : undefined}
                  onPointerLeave={quiet ? keepsNoKeyboard : undefined}
                  // Right opens it and Left closes it, as on a row of a tree.
                  onKeyDown={(event: ReactKeyboardEvent) => {
                    if (event.key === "ArrowRight" && folded) setUnfolded(true);
                    else if (event.key === "ArrowLeft" && !folded) setUnfolded(false);
                    else return;
                    event.preventDefault();
                  }}
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
              {!folded && finished.map(line)}
              {(limits != null || figures?.total != null) && (
                <div className="tasks-menu-foot">
                  {limits != null && <div className="tasks-menu-limits">{limits}</div>}
                  {/* What the tab's chats used (#1500, V100-43): one line, added up and
                      spelled by the core, with what it adds up on its title. No money. */}
                  {figures?.total != null && (
                    <div
                      className="tasks-menu-totals"
                      data-testid="tasks-used"
                      title={figures.total.explained}
                    >
                      {figures.total.said}
                    </div>
                  )}
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

/** How long the pointer rests on "End a task" before its rows open: Radix's own wait. */
const ENDS_OPEN_AFTER = 100;

/** The deepest step in a line is drawn at. A task of a task of a task of a task is rare, and
 *  one further in would leave its name no room. */
const DEEPEST = 6;

/**
 * One chat of the tab, as its menu lists it (V100-19, V100-41): its persona's mark, its name,
 * the workspace it works in where that is not its session's, its state as a word that is never
 * cut, and how long it has been in it. Indented under the chat that asked for it, and said to
 * be asked by it where that is not the session itself. The chat the tab shows now is marked,
 * and said to be.
 *
 * **A line with a chat goes to it. A line with none opens its report**, in place and as text,
 * as its finished row in the Chats list does: a task that has ended has nothing to be shown,
 * and its line is still reached by the arrows and read like any other.
 */
function TaskLine({
  line,
  clock,
  quiet,
  tokens,
  reportOpen,
  onPick,
  onBrief,
  onPlace,
}: {
  line: Line;
  clock: StateClock;
  /** Whether the menu took no keyboard (a rest opened it). */
  quiet: boolean;
  /** What the line's chat used, as its hover says it (#1500); nothing before it is read. */
  tokens: string | undefined;
  reportOpen: boolean;
  /** Answers whether the menu is done with: it went to a chat. */
  onPick: (line: Line) => boolean;
  /** Opens the Brief panel of the line's task (#1494). */
  onBrief?: (ask: BriefAsk) => void;
  /** Moves the line's task, where it is an open task and the menu was handed the way. */
  onPlace?: (session: number, where: Place) => void;
}) {
  const { row } = line;
  // **Its Brief** (#1494): an open task by its chat, an ended one by its dispatch record while
  // it has a finished row. A session's own chat was sent nothing.
  const brief: BriefAsk | undefined =
    onBrief === undefined
      ? undefined
      : row !== undefined
        ? row.mode === "task"
          ? { chat: row.session, name: line.name }
          : undefined
        : line.key.startsWith(FINISHED_KEY)
          ? { dispatch: line.key.slice(FINISHED_KEY.length), name: line.name }
          : undefined;
  // **Where a task is drawn, from its own line** (#1489, V100-38): an open task only. The
  // session's own chat is its tab, and an ended task is nowhere.
  const moves = onPlace !== undefined && row !== undefined && row.mode === "task" ? row : undefined;
  return (
    <>
      <Menu.Item
        className="tasks-menu-row"
        // How far in its line stands: one step per chat between it and the session.
        data-level={Math.min(line.level, DEEPEST)}
        // What the menu's own open lands on and the stylesheet weighs. Said in words below:
        // `aria-current` in a menu is read unevenly, and both would say it twice.
        data-current={line.current ? "" : undefined}
        aria-expanded={line.report === undefined ? undefined : reportOpen}
        textValue={line.name}
        title={tokens}
        onPointerMove={quiet ? keepsNoKeyboard : undefined}
        onPointerLeave={quiet ? keepsNoKeyboard : undefined}
        // **The keyboard's way to move a task from its line** (#1489): Enter goes to it, and
        // Enter with a modifier opens it elsewhere, as a link is opened in a new tab. ⌘ or
        // Ctrl is a tab of its own; Alt is beside its session. Before Radix's own Enter.
        aria-keyshortcuts={moves === undefined ? undefined : "Control+Enter Meta+Enter Alt+Enter"}
        onKeyDown={(event: ReactKeyboardEvent) => {
          if (moves === undefined || onPlace === undefined || event.key !== "Enter") return;
          const where: Place | undefined =
            event.metaKey || event.ctrlKey ? "own" : event.altKey ? "beside" : undefined;
          if (where === undefined) return;
          event.preventDefault();
          if (!event.repeat) onPlace(moves.session, where);
        }}
        onSelect={(event) => {
          // Not a pick of a chat: the menu stays, with the keyboard on this line.
          if (!onPick(line)) event.preventDefault();
        }}
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
        <span className="name" title={tokens === undefined ? line.name : `${line.name}\n${tokens}`}>
          {line.name}
        </span>
        {line.current && <span className="hidden-words">, shown now</span>}
        {/* Who asked for it, where the indent is all that draws it: said, since an indent is
            not read; and drawn too in the fold, where every line stands at one step. */}
        {line.askedBy !== null && (
          <span className={line.level > 2 ? "hidden-words" : "asked"}>
            , asked by {line.askedBy}
          </span>
        )}
        {line.elsewhere !== null && <span className="where"> in {line.elsewhere}</span>}
        {/* A task with a pane of its own is still this tab's (V100-38), and says where it is:
            picking it brings that forward. */}
        {line.placed !== undefined && (
          <span className="where placed" data-placed={line.placed}>
            {line.placed === "tab" ? ", in its own tab" : ", beside it"}
          </span>
        )}{" "}
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
        {line.qualifier !== undefined && <span className="where"> {line.qualifier}</span>}
        {row !== undefined && <Since clock={clock} session={row.session} />}
        {/* What it is doing (#1493): one dim line, cut short; nothing while it is not
            working. It reads its own chat, so this line is not drawn again for it. */}
        {row !== undefined && (
          <span className="doing">
            <ChatDoingLine session={row.session} />
          </span>
        )}
        {brief !== undefined && moves === undefined && (
          <span className="tasks-menu-places" aria-hidden="true">
            <BriefPlace ask={brief} onBrief={onBrief} />
          </span>
        )}
        {moves !== undefined && onPlace !== undefined && (
          /* **The pointer's way to move a task from its line** (#1489), drawn at the line's
             end while the pointer is on it: to a tab of its own and beside its session, or
             back for one that has a pane of its own. **Not in the accessibility tree**, as a
             list row's fold is not: a button inside a menu's item is reached by no arrow, so
             the keyboard's way is the line's own keys above, and the task's menu in the Chats
             list. */
          <span className="tasks-menu-places" aria-hidden="true">
            {line.placed !== "tab" && (
              <PlaceButton
                says={`Move ${line.name} to its own tab`}
                keys={onAMac() ? "⌘Enter" : "Ctrl+Enter"}
                onPress={() => onPlace(moves.session, "own")}
              >
                <SquareArrowOutUpRight />
              </PlaceButton>
            )}
            {line.placed !== "beside" && (
              <PlaceButton
                says={`Open ${line.name} beside its session`}
                keys={onAMac() ? "⌥Enter" : "Alt+Enter"}
                onPress={() => onPlace(moves.session, "beside")}
              >
                <PanelRight />
              </PlaceButton>
            )}
            {line.placed !== undefined && (
              <PlaceButton
                says={`Send ${line.name} back into this tab`}
                onPress={() => onPlace(moves.session, "back")}
              >
                <Minus />
              </PlaceButton>
            )}
            {/* Last: it reads, where the others move the task (#1494). */}
            {brief !== undefined && <BriefPlace ask={brief} onBrief={onBrief} />}
          </span>
        )}
      </Menu.Item>
      {reportOpen && line.report !== undefined && (
        <div
          className="tasks-menu-report"
          role="region"
          aria-label={`Report of ${line.name}`}
          data-level={Math.min(line.level, DEEPEST)}
        >
          {/* Text nodes, every one: a report is a chat's words, and is never markup here. */}
          <p className="report-text">{line.report}</p>
        </div>
      )}
    </>
  );
}

/** The key of an ended task's line that has a finished row: `finished:<dispatch id>`. */
const FINISHED_KEY = "finished:";

/** The Brief button at the end of a task's line (#1494): the pointer's way to it from here;
 *  the keyboard's is the task's row menu, its finished row and the palette. */
function BriefPlace({ ask, onBrief }: { ask: BriefAsk; onBrief?: (ask: BriefAsk) => void }) {
  if (onBrief === undefined) return null;
  return (
    <PlaceButton
      says={briefTitle(ask.name)}
      // The keyboard's way to the same panel, since this button is the pointer's alone.
      also={
        "chat" in ask
          ? "also in its row's menu in the Chats list"
          : "also on its finished row in the Chats list"
      }
      onPress={() => onBrief(ask)}
    >
      <FileText />
    </PlaceButton>
  );
}

/**
 * One of the buttons at the end of a task's line. Its press is its own: it never reaches the
 * line, which would go to the task (Radix's item presses itself on a click and on a pointer
 * coming up over it).
 */
function PlaceButton({
  says,
  keys,
  also,
  onPress,
  children,
}: {
  says: string;
  /** The line's own key for the same thing, said in the tooltip: where the keyboard's way to
   *  it is learned. None for a move the line has no key for. */
  keys?: string;
  /** Where else the same thing is, for a button with no key on its line. */
  also?: string;
  onPress: () => void;
  children: ReactNode;
}) {
  const own = (event: { stopPropagation: () => void }) => event.stopPropagation();
  return (
    <button
      type="button"
      className="tasks-menu-place"
      tabIndex={-1}
      title={
        keys !== undefined
          ? `${says} (${keys} on its line)`
          : also !== undefined
            ? `${says} (${also})`
            : says
      }
      data-says={says}
      onPointerDown={own}
      onPointerUp={own}
      onClick={(event) => {
        event.stopPropagation();
        event.preventDefault();
        onPress();
      }}
    >
      {children}
    </button>
  );
}

/** How long a chat has been in its state, where this window saw it come into it: the Chats
 *  list's own clock and its own words (`stateClock.ts`, `chatsList.sinceSaid`). */
function Since({ clock, session }: { clock: StateClock; session: number }) {
  const seconds = useStateSince(clock, session);
  if (seconds === null) return null;
  return (
    <>
      {" "}
      <span className="since" title="How long it has been in this state, as this window saw it">
        {sinceSaid(seconds)}
      </span>
    </>
  );
}

/**
 * **What the open menu's chats used** (#1500): asked as the menu opens, and again when a chat
 * of it changes state while it is open (a turn ending is when a harness's figure moves). Never
 * on a timer, and never while the menu is closed. An answer to an ask that was replaced is
 * dropped.
 */
function useUsed(
  read: UsedReader | undefined,
  open: boolean,
  lines: readonly Line[],
  kinds: readonly (string | undefined)[],
): TasksUsed | undefined {
  const [figures, setFigures] = useState<TasksUsed>();
  const asked = JSON.stringify(askOf(lines));
  const moved = kinds.join(",");
  useEffect(() => {
    if (read === undefined || !open) return;
    let gone = false;
    void read(JSON.parse(asked) as UsedAsk).then((used) => {
      if (!gone && used !== undefined) setFigures(used);
    });
    return () => {
      gone = true;
    };
  }, [read, open, asked, moved]);
  // The last answer stays drawn until the next lands, so a line's figure does not blink out
  // as a state moves; a line it does not name says nothing.
  return open ? figures : undefined;
}
