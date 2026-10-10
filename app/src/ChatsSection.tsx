import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type FocusEvent,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { ChevronDown, ChevronRight, Hand, MessagesSquare, SquareTerminal } from "lucide-react";
import {
  besideId,
  ONLY_A_TASK_OPENS_BESIDE,
  taskStopId,
  type Catalogued,
  type Offer,
  type TaskEndWay,
} from "./actions";
import { TaskEndConfirm, type TaskEndInline } from "./TaskEnd";
import type { AtLimit, FinishedTask } from "./bindings";
import { ChatRowActivity } from "./ChatRowActivity";
import { ChatRowHandedOff, goesTo } from "./ChatRowHandedOff";
import { HelpersSaid } from "./ExplorerChats";
import { chatDoingId, useDoingSaid } from "./chatDoing";
import { ChatShownState } from "./ChatRows";
import { sameList, useChatsHere, useChatsSelect, type ChatStates } from "./chatState";
import { useTokensOnHover } from "./tasksUsed";
import {
  arranged,
  filters,
  found,
  liveBelow,
  matches,
  matchesFinished,
  sessionOrder,
  sinceSaid,
  stamped,
  type Filter,
  type Rank,
} from "./chatsList";
import { useChatsListPrefs } from "./chatsListPrefs";
import { inScope } from "./chatsScope";
import { keepFolds, keptFolds } from "./chatFolds";
import {
  ASKED_BY_YOU,
  cameFromSaid,
  handedOff,
  handedOffSaid,
  needing,
  parentsIn,
  unfolded,
  type ChatRow,
} from "./chatsTree";
import { FinishedTasks } from "./FinishedTasks";
import type { TaskFacts } from "./shownState";
import { standingOfRows } from "./sessionTasks";
import { TaskCountShown } from "./TasksBelow";
import { Menued } from "./Menus";
import { PersonaMark } from "./PersonaMark";
import { useRevealedTask, type Reveal } from "./revealTask";
import { useTabStop } from "./roving";
import { stateClock, useStateSince, type StateClock } from "./stateClock";
import { deletes } from "./tabKeys";
import { askSettingsLink } from "./settings/links";
import { DISPATCH } from "./settings/dispatch";

/** A row's id in the section's roving focus. */
const rowId = (session: number) => `chats:${session}`;

/** No rows of the catalogue: what a section drawn on its own, in a test, offers. */
const NO_OFFERS: Catalogued = new Map();

/** No finished tasks: what a section drawn without them lists. */
/** No row is on screen: what the clock's reader is told once the list is gone. */
const NOTHING_DRAWN: ReadonlySet<number> = new Set();

const NONE_FINISHED: ReadonlyMap<number, FinishedTask[]> = new Map();
const NO_TASKS: readonly FinishedTask[] = [];
const NOT_REOPENED = () => Promise.resolve<string | undefined>(undefined);
const NOT_CLEARED = () => undefined;

/** The filter's chips: the rank each asks for (`chatsList.rankOf`), as each is said. */
const CHIPS: readonly { rank: Rank; says: string }[] = [
  { rank: 0, says: "needs you" },
  { rank: 1, says: "working" },
];

/** A chip's id in the chips' roving focus. */
const chipId = (rank: Rank) => `chats-chip:${rank}`;
/** The chip that shows every workspace's chats (#1655), in the chips' roving focus. */
const EVERYWHERE_CHIP = "chats-chip:everywhere";
const CHIP_IDS = [...CHIPS.map((chip) => chipId(chip.rank)), EVERYWHERE_CHIP];

/** What the list is drawn from that the chats' own moves change: held still while the pointer
 *  or the keyboard is in the list. */
type Moving = {
  /** The sessions, in the order they stand. */
  order: readonly number[];
  /** The sessions with a chat under them that is not over, which are open by themselves. */
  live: readonly number[];
  /** The chats the filter asks for, or nothing while it asks for none. */
  asked: readonly number[] | null;
  /** Every chat listed: one that arrives while the list is held is drawn when it is let go. */
  listed: readonly number[];
};

const sameAsked = (one: readonly number[] | null, other: readonly number[] | null) =>
  one === other || (one !== null && other !== null && sameList(one, other));

/** The chats a chat that handed nothing off handed off to. */
const NOT_HANDED_OFF: readonly number[] = [];

/** No restart has anything to say. */
const NO_RESTARTS: Readonly<Record<number, string>> = {};

/** No folds set by hand. */
const NO_FOLDS: ReadonlyMap<number, boolean> = new Map();

/** What a key on a row asks for beside opening it. */
type Asked = "beside" | "stop";

/** **What a chat's row says of its dispatches waiting on memory** (#1617): how many. */
function onMemoryWords(waiting: number): string {
  return waiting === 1 ? "1 dispatch waits on memory" : `${waiting} dispatches wait on memory`;
}

/** The whole sentence, on the line's hover: what they wait for, and where they are named. */
const ON_MEMORY_SAID =
  "This machine is short on memory: they start by themselves once it frees, and the chat that asked is told when each starts or gives up. The Dispatches tab lists them under Not started.";

/**
 * Whether the element that took the focus took it from the keyboard. A pointer's press focuses
 * a row too, on the WebViews that focus a button on a click, and a list held for that would
 * stay held after the pointer left. Where the engine cannot say, it is taken as the keyboard's.
 */
function byKeyboard(target: Element): boolean {
  try {
    return target.matches(":focus-visible");
  } catch {
    return true;
  }
}

/**
 * **The running chats of the focused workspace, in one tree** (#1447, #1655), in the left
 * region above the explorer. The explorer answers what is running at each place in this
 * workspace; this answers who is doing what, and which chat started which.
 *
 * **It follows the workspace in the strip** (#1655, ADR 0038's 2026-10-10 amendment): a tree is
 * listed where its top row works, with every task below it wherever that task works
 * (`chatsScope.ts`), and a chat started at the plane root is the root's. The **all workspaces**
 * chip lists every workspace's chats again, for as long as the window runs. A chat in another
 * workspace that needs you is never left out silently: the line under the filter says so and
 * goes to it, as it does for one the filter hides, and the title bar's queue lists it as ever.
 *
 * **A row is a way to the chat.** Pressing one, or Enter on it, shows its chat (`onOpen`, the
 * one way a row opens): a task has no tab of its own, and is shown inside the tab of the
 * session that asked for it (#1486). Space opens a task beside the session that asked for it,
 * inside that session's tab (#1489), and Delete on a task asks to stop it: both are rows of the
 * window's catalogue, and one that cannot run says why here.
 *
 * **At fifty chats** (#1499). The sessions that need you stand first, then the ones at work,
 * then the rest, and a session stands where the most urgent chat under it would; the chats
 * under a session stay in the order they were started. A session with chats under it is open
 * while one of them is not over, and folds by itself when all are, saying how its finished
 * tasks ended on its own row; **a fold set by hand wins** until it is set again. A filter over
 * the list finds a chat by its name, persona, workspace or state, keeps the rows above a match,
 * and says how many it hides. A row is two lines, or one (`chatsListPrefs.ts`).
 *
 * **No row is moved under a resting pointer.** While the pointer is over the list, or the
 * keyboard is in it, the order, the folds the list makes by itself, the filter's answer and
 * the chats that arrive are held as they were, and are brought up to date when both have left,
 * or when the window stops being the one in use. A row is the same height whatever it comes to
 * say, and what the section says about the filter or a key has a line of its own that is
 * always there. What the person does themselves (a fold, a word typed in the filter) is
 * applied at once. **One thing is not held: a chat that ends.** Its row goes, and the rows
 * below it move up, because a row kept for a chat that is gone would be a way to nothing.
 *
 * **A chat that needs you is never filtered away silently.** Where the filter hides one, the
 * line under the filter says so and has a button that goes to it. And a filter opens every row
 * above what it found, whatever fold was set by hand, which is back when the filter is cleared.
 *
 * **A row says its chat's state in a word beside a mark** (#1484): `ChatShownState`, which the
 * explorer's rows draw too.
 *
 * **The hand rolls up** (#1448). A chat that needs you wears it on its own row, as its state's
 * mark, and so does every row above it, where it is a button that goes to that chat. A row
 * with chats under it folds, and a folded row still wears the hand for what it hides, so a
 * fold never hides a chat that needs you.
 *
 * **A handoff is a row of its own at the top, never under the chat it came from** (#1492,
 * V100-69): the work moved, and its chat is a session with its own tab. Its row says `from
 * <chat>`, and that chat's row says `handed off to <chat>` while it is not working; a press on
 * those words goes there. Both name the other chat as its own row does, so a rename is followed.
 * **A task the person asked for themselves is marked `asked by you`** (V100-70).
 *
 * **A task that has finished stays under the chat that asked** (#1485), as a finished entry
 * and not a chat: its program has ended (`FinishedTasks`). They are drawn under their chat's
 * row, after the chats still running under it, and are hidden with them when the row is folded.
 *
 * **A row's menu stops its chat** (#1448): Stop, and Stop with everything below it, from the
 * window's one catalogue, as a tab's menu has them. **A task's has its own two rows instead**
 * (#1488): Stop and get its report, and Close now.
 *
 * **A chat moving redraws its own marks and no row** (SC-3). The rows are held, on plain
 * values, and each state mark reads its own chat's state, so fifty chats cost one mark per
 * move. The hands are read here from the queue, which keeps its identity until it changes, and
 * the order, the folds and the filter's answer are each read through the store's selector, so
 * the list is drawn again only when one of them changes: a reorder is the one thing that moves
 * rows.
 */
export function ChatsSection({
  rows: every,
  here,
  front,
  onOpen,
  offers = NO_OFFERS,
  onPress,
  stopping,
  restarts = NO_RESTARTS,
  finished = NONE_FINISHED,
  onClearFinished = NOT_CLEARED,
  onReopen = NOT_REOPENED,
  clock: given,
  onDrawn,
  reveal,
  onRevealed,
  onLookFinished,
  ending,
  onEndTask,
  onEndConfirm,
  onChanges,
}: {
  rows: readonly ChatRow[];
  /** The workspace in the strip, by the word a row says for where it works: the rows listed
   *  are the trees that started there (#1655). Left out, every workspace's. */
  here?: string;
  /** The chat in front, whose row is the current one. */
  front?: number;
  /** A row was pressed: go to that chat. A task is shown inside its session's tab (#1486).
   *  The one way a row opens. */
  onOpen: (session: number) => void;
  /** The catalogue as it stands, by id: what each row's menu reads its rows from. */
  offers?: Catalogued;
  /** Carries out a row of a menu. */
  onPress?: (offer: Offer) => void;
  /** The chats being stopped, whose rows say so. */
  stopping?: ReadonlySet<number>;
  /** What a restart asked for from a row says, by chat: why it was refused, or why it waits
   *  (#1462). Said on the row of a chat with no tab, which has no pane to say it on. */
  restarts?: Readonly<Record<number, string>>;
  /** Each chat's finished tasks, by its number (#1485). */
  finished?: ReadonlyMap<number, FinishedTask[]>;
  /** Clear finished: takes those rows away, and nothing else. */
  onClearFinished?: (ids: string[]) => void;
  /** Reopens a finished task as an ordinary chat; answers why not, where it could not. */
  onReopen?: (task: FinishedTask) => Promise<string | undefined>;
  /**
   * The project's clock of how long each chat has been in its state, where the window holds
   * one (#1487): the window reads it, so it runs while this list is not drawn, and a tab's
   * menu says the same times. Left out, the list keeps a clock of its own.
   */
  clock?: StateClock;
  /** Tells whoever reads that clock which chats' rows are on screen (`StateClock.read`): this
   *  list's rows as they are drawn, and none once it is gone. */
  onDrawn?: (drawn: ReadonlySet<number>) => void;
  /** A finished task to bring into view: one that failed, when the person goes to its
   *  needs-you item (#1491). */
  reveal?: Reveal;
  /** What became of `reveal`: its row was shown and has the keyboard, or it could not be
   *  shown and the list said so. Told once for each asking. */
  onRevealed?: (reveal: Reveal, shown: boolean) => void;
  /** The person opened a finished row to read it: a failed one has then been looked at. */
  onLookFinished?: (task: FinishedTask) => void;
  /** The second step of ending a task, while it is asked on that task's row (#1488). */
  ending?: TaskEndInline;
  /** A way to end a task was pressed on its row: its menu, or Delete. Asked about on the row. */
  onEndTask?: (session: number, way: TaskEndWay) => void;
  /** The second step was answered: end it, or keep it. */
  onEndConfirm?: (yes: boolean) => void;
  /** Opens what a finished task changed, in a tab of its own (#1511). */
  onChanges?: (task: FinishedTask) => void;
}) {
  const prefs = useChatsListPrefs();
  const chats = useChatsHere();
  /** Whether every workspace's chats are listed (#1655): the person's own choice, for as long
   *  as the window runs. */
  const [everywhere, setEverywhere] = useState(false);
  const scoped = !everywhere && here !== undefined;
  const rows = useMemo(
    () => (!scoped || here === undefined ? every : inScope(every, here)),
    [every, here, scoped],
  );
  const [text, setText] = useState("");
  const [ranks, setRanks] = useState<readonly Rank[]>([]);
  const filter = useMemo<Filter>(() => ({ text, ranks }), [text, ranks]);
  const filtering = filters(filter);
  /** The folds the person set, by chat: true is folded. Kept across a reload of the window
   *  and not across a launch (`chatFolds.ts`, #1459). A chat that is not here folds and opens
   *  by itself. */
  const [hand, setHand] = useState<ReadonlyMap<number, boolean>>(() => keptFolds(chats.plane));
  /** The folds the person set while a filter is on, which last as long as the filter does: a
   *  filter opens every row above what it found, whatever `hand` says, and leaves `hand` be. */
  const [handFiltered, setHandFiltered] = useState<ReadonlyMap<number, boolean>>(NO_FOLDS);
  if (!filtering && handFiltered.size > 0) setHandFiltered(NO_FOLDS);
  /** Why a key pressed on a row did nothing, until the next key. */
  const [said, setSaid] = useState<string>();

  /** The finished tasks the filter asks for, by the chat that asked for them. */
  const finishedFound = useMemo(() => {
    const by = new Map<number, FinishedTask[]>();
    if (!filtering) return by;
    for (const [session, tasks] of finished) {
      const asked = tasks.filter((task) => matchesFinished(task, filter));
      if (asked.length > 0) by.set(session, asked);
    }
    return by;
  }, [finished, filter, filtering]);

  // What the chats' own moves change, each read through the selector: a move that changes
  // none of them draws nothing here.
  // Every row's state from the one pass (`standingOfRows`), so the order, the folds and the
  // filter read the word each row draws, `waiting on n tasks` included (#1491).
  const kindsOf = (states: ChatStates) => {
    const stood = standingOfRows(states, rows);
    return (row: ChatRow) => stood.get(row.session)?.shown?.kind;
  };
  const order = useChatsSelect(
    chats,
    (states) => sessionOrder(rows, kindsOf(states), prefs.grouped),
    sameList,
  );
  const live = useChatsSelect(chats, (states) => liveBelow(rows, kindsOf(states)), sameList);
  const asked = useChatsSelect(
    chats,
    (states) => {
      if (!filtering) return null;
      const stood = standingOfRows(states, rows);
      return rows
        .filter(
          (row) =>
            matches(row, stood.get(row.session)?.shown, filter) || finishedFound.has(row.session),
        )
        .map((row) => row.session);
    },
    sameAsked,
  );
  const listed = useMemo(() => rows.map((row) => row.session), [rows]);

  // **Held still while the pointer is over the list or the keyboard is in it** (V100-47): no
  // row moves under a click. Taken as the pointer or the keyboard comes in, let go when both
  // have left.
  const list = useRef<HTMLDivElement>(null);
  const [over, setOver] = useState(false);
  const [inside, setInside] = useState(false);
  // The list is not drawn with no chat to list, so nothing would say the pointer left it.
  if (rows.length === 0 && (over || inside)) {
    setOver(false);
    setInside(false);
  }
  // And neither hold outlives the window being the one in use: a pointer parked over the
  // sidebar while the person works elsewhere sends no leave. The next move of the pointer, or
  // key in the list, takes it again.
  useEffect(() => {
    const away = () => {
      setOver(false);
      setInside(false);
    };
    window.addEventListener("blur", away);
    return () => window.removeEventListener("blur", away);
  }, []);
  const resting = over || inside;
  const [held, setHeld] = useState<Moving | null>(null);
  const moving: Moving = { order, live, asked, listed };
  // **A workspace focused, or the chip pressed, is the person's doing** (#1655): its rows are
  // theirs to see at once, as a filter's are, and not held as the last workspace's were.
  const scope = scoped ? here : null;
  const [heldScope, setHeldScope] = useState(scope);
  if (heldScope !== scope) {
    setHeldScope(scope);
    setHeld(resting ? moving : null);
  } else if (resting && held === null) setHeld(moving);
  if (!resting && held !== null) setHeld(null);
  const now = resting && held !== null ? held : moving;
  /** The filter was changed by the person: its answer is theirs to see at once. */
  const refilter = (how: () => void) => {
    how();
    setHeld(null);
  };
  const clear = () =>
    refilter(() => {
      setText("");
      setRanks([]);
    });

  /** What each chat's finished tasks come to: whether one of them stands alone, which keeps
   *  its chat open. What a folded row says of them is its count (`TaskCountShown`, #1491),
   *  the same on a folded row and an open one. */
  const ended = useMemo(
    () =>
      new Map(
        [...finished]
          .filter(([, tasks]) => tasks.length > 0)
          .map(([session, tasks]) => [session, { alone: tasks.some((task) => !task.folds) }]),
      ),
    [finished],
  );
  /** The rows the list is drawn from: every chat, less the ones that arrived while it is
   *  held. A chat that ended is not in `rows`, and is not kept. */
  const steady = useMemo(() => {
    if (now.listed === listed) return rows;
    const known = new Set(now.listed);
    return rows.filter((row) => known.has(row.session));
  }, [rows, listed, now.listed]);
  /** The rows in the order they stand, less what the filter hides. */
  const base = useMemo(() => {
    const inOrder = arranged(steady, now.order);
    return stamped(now.asked === null ? inOrder : found(inOrder, new Set(now.asked)));
  }, [steady, now.order, now.asked]);
  const parents = useMemo(() => parentsIn(base), [base]);
  /**
   * Whether each row's own rows are drawn under it, for a row that has some (a chat or a
   * finished task). **A fold set by hand wins over the folds the list makes** (V100-48):
   * without one, a row is open while a chat under it is not over or a finished task of its
   * own stands alone. **A filter wins over both**: every row above what it found is open, so
   * what it found is drawn, and only a fold set while it is on shuts one again.
   */
  const opens = useMemo(() => {
    const alive = new Set(now.live);
    const by = new Map<number, boolean>();
    for (const row of base) {
      const { session } = row;
      if (!parents.has(session) && !ended.has(session)) continue;
      const set = now.asked !== null ? handFiltered.get(session) : hand.get(session);
      by.set(
        session,
        set !== undefined
          ? !set
          : now.asked !== null
            ? parents.has(session) || finishedFound.has(session)
            : alive.has(session) || ended.get(session)?.alone === true,
      );
    }
    return by;
  }, [base, parents, ended, hand, handFiltered, finishedFound, now.live, now.asked]);
  const folded = useMemo(
    () => new Set([...opens].filter(([, open]) => !open).map(([session]) => session)),
    [opens],
  );
  const drawn = useMemo(() => unfolded(base, folded), [base, folded]);
  // **The keyboard's hold never outlives the keyboard being here.** A focused row that is
  // taken out of the document (its task stopped, or finished, or was filtered away) sends no
  // blur, so after every draw the hold is let go when the focus is no longer in the list.
  useLayoutEffect(() => {
    if (inside && list.current?.contains(document.activeElement) !== true) setInside(false);
  }, [inside, drawn, rows, finished]);
  const needsYou = useChatsSelect(chats, (states) => states.needsYou);
  const leads = useMemo(() => needing(rows, needsYou), [rows, needsYou]);
  const byNumber = useMemo(() => new Map(rows.map((row) => [row.session, row])), [rows]);
  /** Where each chat's work went by a handoff, the newest first (#1492). */
  const went = useMemo(() => handedOff(rows), [rows]);
  /** The same by number, for each row's menu: held, so a row is drawn again only for its own. */
  const wentTo = useMemo(
    () => new Map([...went].map(([from, chats]) => [from, chats.map((chat) => chat.session)])),
    [went],
  );
  /**
   * **The chats that need the person and that the filter hides**, longest waiting first
   * (#1499): their own rows and every row above them are filtered out, so no hand is drawn
   * for them anywhere in the list. Read off the queue as it stands, never held.
   */
  const hiddenNeeding = useMemo(() => {
    if (now.asked === null) return [];
    const kept = new Set(base.map((row) => row.session));
    return needsYou.filter((session) => byNumber.has(session) && !kept.has(session));
  }, [base, byNumber, needsYou, now.asked]);
  /** Every workspace's rows by number, for what is said of a chat this workspace's list
   *  leaves out (#1655). */
  const everyByNumber = useMemo(() => new Map(every.map((row) => [row.session, row])), [every]);
  /** **The chats that need the person in the workspaces not listed** (#1655), longest waiting
   *  first: never left out silently. */
  const elsewhereNeeding = useMemo(
    () =>
      rows === every
        ? []
        : needsYou.filter((session) => everyByNumber.has(session) && !byNumber.has(session)),
    [rows, every, needsYou, everyByNumber, byNumber],
  );

  // How long each chat has been in its state, as this window saw it: read off every chat, drawn
  // or not, so a row that was folded away says the same time when it is drawn again. One clock
  // per project: chats are numbered per project.
  const { store, plane } = chats;
  /** **Where a limit is changed** (#1498): Settings › Project › Dispatch, which a row at its
   *  task limit links to. */
  const limits = useCallback(() => {
    if (plane !== undefined) askSettingsLink(plane, { group: DISPATCH });
  }, [plane]);
  const own = useMemo(() => {
    void plane;
    return stateClock();
  }, [plane]);
  const clock = given ?? own;
  const onScreen = useMemo(() => new Set(drawn.map((row) => row.session)), [drawn]);
  useEffect(() => {
    // The window's clock is read by the window, which is told what is on screen here.
    if (given !== undefined) return;
    const read = () => clock.read(store.statesFor(plane), rows, Date.now(), onScreen);
    read();
    return store.subscribe(read);
  }, [store, plane, clock, given, rows, onScreen]);
  useEffect(() => {
    if (given === undefined || onDrawn === undefined) return;
    onDrawn(onScreen);
    return () => onDrawn(NOTHING_DRAWN);
  }, [given, onDrawn, onScreen]);

  // What the person folded outlives a reload of the window (#1459).
  useEffect(() => keepFolds(plane, hand), [plane, hand]);
  const fold = useCallback(
    (session: number, shut: boolean) => {
      (filtering ? setHandFiltered : setHand)((was) => {
        if (was.get(session) === shut) return was;
        const set = new Map(was);
        set.set(session, shut);
        return set;
      });
    },
    [filtering],
  );
  // **A way to end a task pressed on its row is asked about on that row** (#1488): the
  // window is told where the press was made. Every other row of a menu is carried out as it is.
  const press = useCallback(
    (offer: Offer) => {
      if (offer.does.verb === "endTask" && onEndTask !== undefined)
        onEndTask(offer.does.session, offer.does.way);
      else onPress?.(offer);
    },
    [onEndTask, onPress],
  );
  const confirm = useCallback((yes: boolean) => onEndConfirm?.(yes), [onEndConfirm]);
  const act = useCallback(
    (session: number, what: Asked) => {
      // Delete on a task asks to stop it and get its report (#1488, V100-17): the task's own
      // row of the catalogue, which asks first where the task is mid-turn. **Space opens the
      // row's own task beside its session** (#1489): that task's row of the catalogue, so what
      // opens is the row the key was pressed on. A chat that is not a task has neither row,
      // and Space says why.
      const offer = offers.get(what === "beside" ? besideId(session) : taskStopId(session));
      if (offer === undefined) {
        if (what === "beside") setSaid(ONLY_A_TASK_OPENS_BESIDE);
        return;
      }
      if (!offer.available) {
        setSaid(offer.reason);
        return;
      }
      press(offer);
    },
    [offers, press],
  );
  const section = useRef<HTMLElement>(null);
  // **A row asked for in another workspace lists every workspace** (#1655), once for each
  // asking, so the row can be shown: the chip says it is on, and the person takes it off.
  const [widenedFor, setWidenedFor] = useState<number>();
  if (
    reveal !== undefined &&
    reveal.at !== widenedFor &&
    scoped &&
    !byNumber.has(reveal.asker) &&
    everyByNumber.has(reveal.asker)
  ) {
    setWidenedFor(reveal.at);
    setEverywhere(true);
  }
  useRevealedTask(section, reveal, rows, {
    fold,
    shut: (session) => opens.get(session) === false,
    // A finished row, or a chat's own, that the filter does not ask for. And a chat's own row
    // asked for by the explorer's line (#1490) under any filter: the line counted the tasks
    // under it, and a filter that kept the row could still hide those.
    hides: (asked) =>
      filtering &&
      (asked.task === undefined ||
        !base.some((row) => row.session === asked.asker) ||
        !(finishedFound.get(asked.asker) ?? []).some((task) => task.name === asked.task)),
    unfilter: (name) => {
      clear();
      setSaid(`The filter was taken off to show ${name}.`);
    },
    // The row was shown, or could not be: said to whoever asked, once (#1491).
    shown: (asked) => onRevealed?.(asked, true),
    missed: (asked) => {
      setSaid(`${asked.task ?? "That chat"} has no row to show here.`);
      onRevealed?.(asked, false);
    },
  });
  /** The row the second step is asked on, where it is drawn. */
  const second = ending?.where === "row" ? ending : undefined;
  const stop = useTabStop(
    front === undefined ? undefined : rowId(front),
    drawn.map((row) => rowId(row.session)),
  );
  const chipStop = useTabStop(undefined, CHIP_IDS);
  const left = (event: FocusEvent<HTMLElement>) => {
    const to = event.relatedTarget;
    if (to instanceof Node && event.currentTarget.contains(to)) return;
    setInside(false);
    setSaid(undefined);
  };
  const hidden = steady.length - base.length;
  /** Where the Go button goes: a chat the filter hides that needs you, else one in another
   *  workspace that does (#1655). */
  const hiddenFirst =
    hiddenNeeding.length > 0
      ? { row: byNumber.get(hiddenNeeding[0]), why: "which needs you and the filter hides" }
      : elsewhereNeeding.length > 0
        ? {
            row: everyByNumber.get(elsewhereNeeding[0]),
            why: "which needs you in another workspace",
          }
        : undefined;
  const goesTo = hiddenFirst?.row;
  /** What the line says of the workspaces not listed: only a chat there that needs you. */
  const elsewhere =
    elsewhereNeeding.length === 0
      ? ""
      : elsewhereNeeding.length === 1
        ? "1 chat in another workspace needs you."
        : `${elsewhereNeeding.length} chats in other workspaces need you.`;
  /** What the line under the filter says of it. A chat that needs the person comes first. */
  const hides = !filtering
    ? elsewhere
    : [
        hiddenNeeding.length === 0
          ? ""
          : hiddenNeeding.length === 1
            ? "1 chat the filter hides needs you."
            : `${hiddenNeeding.length} chats the filter hides need you.`,
        base.length === 0
          ? "No chat matches the filter."
          : hidden === 0
            ? "The filter hides no chat."
            : `The filter hides ${hidden} of ${steady.length} chats.`,
        elsewhere,
      ]
        .filter((one) => one !== "")
        .join(" ");
  return (
    <section
      ref={section}
      className="chats-section"
      data-testid="chats-section"
      aria-labelledby="chats-title"
    >
      {/* The title and the filter stay at the top of the section while its rows scroll. */}
      <div className="chats-head">
        <h2 className="sidebar-title" id="chats-title">
          <MessagesSquare className="node-icon" aria-hidden="true" />
          Chats
        </h2>
        {every.length > 0 && (
          <>
            <div className="chats-filter" role="search" aria-label="Filter the chats">
              <input
                type="search"
                className="chats-filter-text"
                // #190: WebKit leaves a control out of the tab sequence without `tabIndex`.
                tabIndex={0}
                aria-label="Filter chats by name, persona, workspace or state"
                placeholder="Filter chats"
                value={text}
                onChange={(event) => {
                  const typed = event.target.value;
                  refilter(() => setText(typed));
                }}
                onKeyDown={(event) => {
                  if (event.key !== "Escape" || !filtering) return;
                  event.preventDefault();
                  event.stopPropagation();
                  clear();
                }}
              />
              {/* The chips: ONE Tab stop, with the arrows between them (`roving.ts`), named
                  as a group so each box is heard as what it narrows the list to. A box each,
                  since each is on or off, drawn as a chip. */}
              <RovingFocusGroup.Root asChild orientation="horizontal" {...chipStop}>
                <div className="chats-chips" role="group" aria-label="Show only">
                  {CHIPS.map((chip) => (
                    <label
                      key={chip.rank}
                      className="chats-chip"
                      data-on={ranks.includes(chip.rank) || undefined}
                    >
                      <RovingFocusGroup.Item asChild tabStopId={chipId(chip.rank)}>
                        <input
                          type="checkbox"
                          checked={ranks.includes(chip.rank)}
                          onChange={() =>
                            refilter(() =>
                              setRanks((was) =>
                                was.includes(chip.rank)
                                  ? was.filter((rank) => rank !== chip.rank)
                                  : [...was, chip.rank],
                              ),
                            )
                          }
                        />
                      </RovingFocusGroup.Item>
                      {chip.says}
                    </label>
                  ))}
                  {/* Every workspace's chats (#1655): off, the list follows the strip. */}
                  {here !== undefined && (
                    <label className="chats-chip" data-on={everywhere || undefined}>
                      <RovingFocusGroup.Item asChild tabStopId={EVERYWHERE_CHIP}>
                        <input
                          type="checkbox"
                          checked={everywhere}
                          onChange={() => setEverywhere((was) => !was)}
                        />
                      </RovingFocusGroup.Item>
                      all workspaces
                    </label>
                  )}
                </div>
              </RovingFocusGroup.Root>
            </div>
            {/* **One line, always there**, so what it comes to say is announced (a live region
                that enters the tree with its words is not) and pushes no row down. Two things
                are said on it: what the filter hides, and why a key just pressed on a row did
                nothing, which is drawn in the count's place until the next key. */}
            <div className="chats-notes">
              {hiddenFirst !== undefined && goesTo !== undefined && (
                <button
                  type="button"
                  className="chats-hidden-go"
                  // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
                  tabIndex={0}
                  data-leads-to={goesTo.session}
                  aria-label={`Go to ${goesTo.name}, ${hiddenFirst.why}`}
                  title={`Go to ${goesTo.name}, ${hiddenFirst.why}`}
                  onClick={() => onOpen(goesTo.session)}
                >
                  <Hand aria-hidden="true" />
                  Go
                </button>
              )}
              <p
                className={said === undefined ? "chats-hidden" : "chats-hidden away"}
                role="status"
                title={hides || undefined}
              >
                {hides}
              </p>
              <p className="chats-said" role="status" title={said}>
                {said ?? ""}
              </p>
            </div>
          </>
        )}
      </div>
      {rows.length === 0 ? (
        <p className="empty">
          {every.length === 0
            ? "No chats are running in this project."
            : "No chats are running here. All workspaces lists the others."}
        </p>
      ) : (
        <div
          ref={list}
          className="chats-list"
          onPointerEnter={() => setOver(true)}
          // Taken again by the pointer's next move, after the window was left and come back to.
          onPointerMove={() => {
            if (!over) setOver(true);
          }}
          onPointerLeave={() => setOver(false)}
          // Only the keyboard's focus holds the list: see `byKeyboard`.
          onFocus={(event) => {
            if (byKeyboard(event.target)) setInside(true);
          }}
          onBlur={left}
          // Before the row's own keys: what the last key left said is taken down by the next,
          // unless that is Space again, and a key in the list is the keyboard being here.
          onKeyDownCapture={(event) => {
            if (event.key !== " ") setSaid(undefined);
            if (!inside) setInside(true);
          }}
          onKeyDown={(event) => {
            // Escape in the list takes the filter off, as it does in the filter's own box, and
            // like that one it is taken here: nothing behind the list acts on it too.
            if (event.key !== "Escape" || !filtering) return;
            event.preventDefault();
            event.stopPropagation();
            clear();
          }}
        >
          {drawn.length > 0 && (
            <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
              <ul role="tree" aria-label="Chats of this project">
                {/* One flat list of keyed items, so a row that changes place is moved and not
                    made again: a list of lists would key each row by where it stands. */}
                {drawn.flatMap((row, at) => {
                  const lead = leads.get(row.session);
                  const open = opens.get(row.session);
                  const asker = row.parent === null ? undefined : byNumber.get(row.parent);
                  const handed = went.get(row.session);
                  const above = at === 0 ? undefined : topBefore(drawn, at);
                  return [
                    // **A workspace's sessions stand together** (V100-47, a setting): its
                    // name over the first of them. Not a row of the tree, and not said by
                    // it: each row says its own workspace.
                    prefs.grouped && row.level === 1 && above?.workspace !== row.workspace && (
                      <li
                        key={`group:${row.session}`}
                        role="none"
                        className="chats-group"
                        aria-hidden="true"
                      >
                        {row.workspace}
                      </li>
                    ),
                    <Row
                      key={row.session}
                      session={row.session}
                      name={row.name}
                      persona={row.persona}
                      workspace={row.workspace}
                      // A task says where it works only when that is not where the chat
                      // that asked works (V100-19); every other chat says it.
                      elsewhere={
                        row.mode !== "task" ||
                        asker === undefined ||
                        asker.workspace !== row.workspace
                      }
                      task={row.mode === "task"}
                      lines={prefs.lines}
                      branch={row.branch}
                      shell={row.shell}
                      report={row.report}
                      outcome={row.outcome}
                      asking={row.asking}
                      harness={row.harness}
                      level={row.level}
                      posinset={row.posinset}
                      setsize={row.setsize}
                      // A handoff says the chat it came from, by what that chat is called
                      // now, or was when it closed. A task says it only once its asker has
                      // closed: under its asker's row, the nesting says it.
                      from={
                        row.mode === "handoff"
                          ? (asker?.name ?? row.from)
                          : row.orphaned
                            ? row.from
                            : null
                      }
                      byYou={row.byYou === true}
                      handedTo={handed?.[0].session ?? null}
                      handedToName={handed?.[0].name ?? null}
                      handedMore={handed === undefined ? 0 : handed.length - 1}
                      handedAll={wentTo.get(row.session) ?? NOT_HANDED_OFF}
                      askerWaiting={row.askerWaiting === true}
                      tab={row.tab}
                      current={row.session === front}
                      open={open ?? null}
                      needs={lead === undefined ? null : lead}
                      needsName={
                        lead === undefined || lead === row.session
                          ? null
                          : (byNumber.get(lead)?.name ?? null)
                      }
                      stopping={stopping?.has(row.session) ?? false}
                      restartSaid={row.tab ? null : (restarts[row.session] ?? null)}
                      asks={second?.session === row.session ? second.says : null}
                      answer={second?.session === row.session ? second.answer : null}
                      busy={second?.session === row.session && second.busy}
                      trouble={second?.session === row.session ? (second.trouble ?? null) : null}
                      onConfirm={confirm}
                      atLimit={row.atLimit ?? null}
                      waitingOnMemory={row.waitingOnMemory ?? null}
                      onLimits={limits}
                      offers={offers}
                      clock={clock}
                      onOpen={onOpen}
                      onFold={fold}
                      onPress={press}
                      onAct={act}
                    />,
                    // The finished tasks of each chat whose rows end here (#1485): this
                    // row's own, where no chat is drawn under it, then those of every chat
                    // above it that this row is the last one under. Under a filter, the ones
                    // it asks for.
                    ...endingAt(drawn, at, folded).map((one) => (
                      <FinishedTasks
                        key={`finished:${one.session}`}
                        asker={one.name}
                        level={one.level + 1}
                        tasks={
                          (now.asked !== null
                            ? finishedFound.get(one.session)
                            : finished.get(one.session)) ?? NO_TASKS
                        }
                        onClear={onClearFinished}
                        onReopen={onReopen}
                        onLook={onLookFinished}
                        onChanges={onChanges}
                        offers={offers}
                        onPress={press}
                      />
                    )),
                  ];
                })}
              </ul>
            </RovingFocusGroup.Root>
          )}
        </div>
      )}
    </section>
  );
}

/** The session drawn before row `at`'s: the nearest row above it at the top level. */
function topBefore(drawn: readonly ChatRow[], at: number): ChatRow | undefined {
  for (let up = at - 1; up >= 0; up -= 1) if (drawn[up].level === 1) return drawn[up];
  return undefined;
}

/**
 * The chats whose last drawn row is row `at`, deepest first: row `at` itself, then each chat
 * above it that has no later row under it. Where a chat's finished tasks are drawn: after the
 * chats still running under it. A folded chat is left out: what is under it is folded away.
 */
function endingAt(drawn: readonly ChatRow[], at: number, folded: ReadonlySet<number>): ChatRow[] {
  const next = drawn[at + 1]?.level ?? 0;
  const ending: ChatRow[] = [];
  let level = drawn[at].level + 1;
  for (let up = at; up >= 0 && level > next; up -= 1) {
    const row = drawn[up];
    if (row.level >= level) continue;
    // `row` is the nearest row above at a shallower level: an ancestor, or row `at` itself.
    level = row.level;
    if (level < next) break;
    if (!folded.has(row.session)) ending.push(row);
  }
  return ending;
}

/**
 * One chat's row. Held on plain values, so only a row whose own facts changed is drawn again.
 *
 * **Two lines** (#1499, V100-50). The first is the persona's mark, the name and the state; the
 * state's word is never cut short, and the name is, with the whole of it as its tooltip and in
 * what a screen reader is told. The second, dimmer, is what the chat is doing while it works
 * and for how long (`SecondLine`, `ChatRowActivity`, #1493); otherwise where its work was
 * handed off to (`ChatRowHandedOff`, #1492), that the person asked for it, where it works, how
 * long it has been in its state, its own branch and the chat it came from. **On one line the
 * second is not drawn at all**, and what of it
 * does not change is the row's tooltip: a row is never two lines squeezed into one.
 */
const Row = memo(function Row({
  session,
  name,
  persona,
  workspace,
  elsewhere,
  task,
  lines,
  branch,
  shell,
  report,
  outcome,
  asking,
  harness,
  level,
  posinset,
  setsize,
  from,
  byYou,
  handedTo,
  handedToName,
  handedMore,
  handedAll,
  askerWaiting,
  tab,
  current,
  open,
  needs,
  needsName,
  stopping,
  restartSaid,
  asks,
  answer,
  busy,
  trouble,
  onConfirm,
  atLimit,
  waitingOnMemory,
  onLimits,
  offers,
  clock,
  onOpen,
  onFold,
  onPress,
  onAct,
}: {
  session: number;
  name: string;
  persona: string | null;
  workspace: string;
  /** Whether the row says its workspace: every chat but a task working where its asker does. */
  elsewhere: boolean;
  /** A task, which Delete asks to stop. */
  task: boolean;
  lines: 1 | 2;
  /** The branch of its own a task works on, where it was given one. */
  branch: string | null;
  shell: boolean;
  /** What its state is derived from beside the board's word for it (`ChatShownState`). */
  report: TaskFacts["report"] | null;
  outcome: string | null;
  asking: string | null;
  harness: string | null;
  level: number;
  posinset: number;
  setsize: number;
  /** The chat it came from, by name: a handoff's, or a task's whose asker has closed. Nothing
   *  for a chat drawn under its parent. */
  from: string | null;
  /** A task the person asked for themselves, from its session's tab (V100-70). */
  byYou: boolean;
  /** The newest open chat its work was handed off to, and that chat's name; nothing for a chat
   *  that handed nothing off. */
  handedTo: number | null;
  handedToName: string | null;
  /** How many other open chats it handed off to. */
  handedMore: number;
  /** Every open chat it handed off to, the newest first: its menu has a row to go to each. */
  handedAll: readonly number[];
  /** That chat waits to start after a launch, and has not closed (#1513). */
  askerWaiting: boolean;
  tab: boolean;
  current: boolean;
  /** Whether its rows are drawn under it, for a row that has some; nothing for a leaf. */
  open: boolean | null;
  /** The chat its hand leads to: itself, a chat below it, or none when it wears no hand. */
  needs: number | null;
  /** That chat's name, when it is a chat below this one. */
  needsName: string | null;
  stopping: boolean;
  /** What its restart says, where it has no pane to say it on (#1462). */
  restartSaid: string | null;
  /** The second step of ending this task, while it is asked here: what is asked, and the
   *  button that does it. Nothing while it is not. */
  asks: string | null;
  answer: string | null;
  busy: boolean;
  /** The core's refusal of the answer, which stays on the row. */
  trouble: string | null;
  onConfirm: (yes: boolean) => void;
  /** Its last dispatch was refused for a limit and no slot has freed since (#1498): the
   *  number, and the core's sentence of which limit and where it is changed. */
  atLimit: AtLimit | null;
  /** How many of its dispatches wait until this machine has memory to spare (#1617); nothing
   *  where none does. */
  waitingOnMemory: number | null;
  /** Opens Settings where the limits are changed. */
  onLimits: () => void;
  offers: Catalogued;
  clock: StateClock;
  onOpen: (session: number) => void;
  onFold: (session: number, shut: boolean) => void;
  onPress: (offer: Offer) => void;
  onAct: (session: number, what: Asked) => void;
}) {
  // The keys of a row. Enter is the button's own press, which opens it. Space opens a task
  // beside its session, and Delete (Backspace on a Mac, `tabKeys.deletes`) asks to stop
  // a task: the catalogue's rows, which ask
  // first or say why not. The tree's own (WAI-ARIA "Tree View"): Right opens a folded row,
  // Left folds an open one. Up and Down are the roving group's.
  const keys = (event: KeyboardEvent<HTMLElement>) => {
    // A key held down is one press: neither asks a second time.
    if (event.key === " ") {
      if (!event.repeat) onAct(session, "beside");
    } else if (task && deletes(event)) {
      if (!event.repeat) onAct(session, "stop");
    } else if (open !== null && event.key === "ArrowRight" && !open) onFold(session, false);
    else if (open !== null && event.key === "ArrowLeft" && open) onFold(session, true);
    else return;
    event.preventDefault();
  };
  /** The row itself, which takes the keyboard back when the second step is answered Keep. */
  const self = useRef<HTMLButtonElement>(null);
  // Asked once, as the row is drawn: whether its chat's state changed while it was not.
  const [changed] = useState(() => clock.missed(session));
  const ownBranch = branch === null ? null : `own branch ${branch}`;
  const cameFrom = from === null ? null : cameFromSaid(from, task, askerWaiting);
  const wentTo = handedToName === null ? null : handedOffSaid(handedToName, handedMore);
  /** What the second line says that does not change by itself: one line's tooltip. */
  const second = [
    wentTo,
    byYou ? ASKED_BY_YOU : null,
    elsewhere ? workspace : null,
    ownBranch,
    cameFrom,
  ].filter((one): one is string => one !== null);
  // A task's tokens, on its row's hover (#1500): read as the pointer comes on, never polled.
  const used = useTokensOnHover({ chat: session });
  const hover = [lines === 1 ? second.join(" · ") : "", task ? (used.said ?? "") : ""]
    .filter((one) => one !== "")
    .join("\n");
  return (
    <li role="none" data-level={level}>
      {open === null ? (
        <span className="twist" aria-hidden="true" />
      ) : (
        /* The pointer's way to fold a row. **Not in the accessibility tree**: the row itself
           says `aria-expanded` and folds on Left and Right, so a second control saying the same
           would be read twice and reached by nothing. */
        <button
          type="button"
          className="twist"
          tabIndex={-1}
          aria-hidden="true"
          data-fold={open ? "open" : "folded"}
          title={open ? `Fold the chats under ${name}` : `Show the chats under ${name}`}
          onClick={() => onFold(session, open)}
        >
          {open ? <ChevronDown aria-hidden="true" /> : <ChevronRight aria-hidden="true" />}
        </button>
      )}
      <Menued on={{ on: "listed", session, handed: handedAll }} offers={offers} onPress={onPress}>
        <RovingFocusGroup.Item asChild tabStopId={rowId(session)}>
          <button
            type="button"
            ref={self}
            className="chat"
            role="treeitem"
            aria-level={level}
            aria-posinset={posinset}
            aria-setsize={setsize}
            aria-current={current || undefined}
            aria-expanded={open ?? undefined}
            // A chat below it needs you: said on the row, which is where a screen reader is,
            // since the hand beside it is out of the keyboard's way.
            aria-description={
              needs !== null && needs !== session
                ? `${needsName ?? "A chat"} below it needs you`
                : undefined
            }
            // What it is doing is its description on demand (#1493), never announced as it
            // changes: the line is out of the tree where it stands, and named here. A chat
            // below that needs you is said first, so it is the one description then.
            aria-describedby={
              lines === 2 && !(needs !== null && needs !== session)
                ? chatDoingId(session)
                : undefined
            }
            data-tab={tab}
            data-lines={lines}
            // The chat's number, as a pane carries it: what a reveal finds the row by (#1490).
            data-session={session}
            title={hover || undefined}
            onPointerEnter={task ? used.onPointerEnter : undefined}
            onPointerLeave={task ? used.onPointerLeave : undefined}
            // The row goes to its chat, and the words that name another chat go to that one
            // (`ChatRowHandedOff`). Enter is a press on the row itself.
            onClick={(event) => onOpen(goesTo(event.target, event.currentTarget) ?? session)}
            onKeyDown={keys}
            // A button presses itself as Space comes up: Space is the row's own key here.
            onKeyUp={(event) => {
              if (event.key === " ") event.preventDefault();
            }}
          >
            <span className="line one">
              {persona === null ? (
                <SquareTerminal className="node-icon" aria-hidden="true" />
              ) : (
                <PersonaMark persona={persona} />
              )}
              {/* Cut short where the row is narrow, and whole here for a pointer that rests
                  on it; a screen reader is told the text, which is always the whole name. */}
              <span
                className="session"
                title={task && used.said !== undefined ? `${name}\n${used.said}` : name}
              >
                {name}
              </span>
              {/* Its state, a mark and a word (#1484): the hand of a chat that needs you is
                  this mark, so the row draws no second one. Never cut short. */}
              <ChatShownState
                session={session}
                shell={shell}
                report={report}
                outcome={outcome}
                asking={asking}
                harness={harness}
                changed={changed}
              />
              {/* How its tasks stand, where it has any (#1491): `2 working · 1 waiting ·
                  3 done`. It reads its own tasks, so one that changes state redraws this and
                  not the row. */}
              <TaskCountShown session={session} />
              {stopping && <span className="stopping">Stopping…</span>}
            </span>
            {lines === 2 && (
              <span className="line two">
                <SecondLine
                  session={session}
                  activity={<ChatRowActivity session={session} />}
                  since={<StateSince clock={clock} session={session} />}
                  before={
                    <>
                      {handedTo !== null && handedToName !== null && (
                        <ChatRowHandedOff
                          session={session}
                          shell={shell}
                          report={report}
                          outcome={outcome}
                          asking={asking}
                          harness={harness}
                          to={handedTo}
                          name={handedToName}
                          more={handedMore}
                        />
                      )}
                      {byYou && (
                        <span
                          className="by-you"
                          title="You asked for this task from its session's tab"
                        >
                          {ASKED_BY_YOU}
                        </span>
                      )}
                      {elsewhere && <span className="workspace">{workspace}</span>}
                    </>
                  }
                  after={
                    <>
                      {ownBranch !== null && (
                        <span
                          className="own-branch"
                          title="A branch of its own, which only you merge, from the task's Changes"
                        >
                          {ownBranch}
                        </span>
                      )}
                      {cameFrom !== null && <span className="from">{cameFrom}</span>}
                      {/* A task's helpers, as a count (#1490, V100-4): it has no row in the
                          explorer, where a chat's helpers unfold, so its own row says them.
                          Last on the line, and only where it has some. */}
                      {task && <HelpersSaid session={session} />}
                    </>
                  }
                />
              </span>
            )}
          </button>
        </RovingFocusGroup.Item>
      </Menued>
      {atLimit !== null && (
        /* **At its task limit** (#1498, V100-54): said where the limit binds, on a refusal,
           until a slot frees; with the number and the way to where it is changed. */
        <span className="at-limit" data-testid={`at-limit-${session}`} title={atLimit.said}>
          {/* Which limit binds, with its number: only its own is "its task limit". */}
          <span>{atLimit.row}</span>
          <button type="button" className="at-limit-settings" onClick={onLimits}>
            Dispatch settings
          </button>
        </span>
      )}
      {waitingOnMemory !== null && waitingOnMemory > 0 && (
        /* **Its dispatches waiting on memory** (#1617): said on the asking chat's row, in the
           at-limit line's place and style, as the Dispatches tab's Not started list names them. */
        <span className="at-limit" data-testid={`on-memory-${session}`} title={ON_MEMORY_SAID}>
          {onMemoryWords(waitingOnMemory)}
        </span>
      )}
      {restartSaid !== null && (
        /* **A restart asked for from this row, refused or waiting** (#1462): said here, in
           the core's words, for a chat with no pane to say it on. */
        <span className="restart-said" role="status" aria-label={`Restart of ${name}`}>
          {restartSaid}
        </span>
      )}
      {asks !== null && answer !== null && (
        /* **The second step of ending this task, on its own row** (#1488): nothing ends on
           one press. Keep has the keyboard, and gives it back to the row. */
        <TaskEndConfirm
          says={asks}
          answer={answer}
          busy={busy}
          trouble={trouble ?? undefined}
          onAnswer={() => onConfirm(true)}
          onKeep={() => {
            onConfirm(false);
            self.current?.focus();
          }}
        />
      )}
      {needs !== null && needs !== session && (
        /* **A chat below this one needs you**: the hand, and a press goes to that chat. Its
           own button beside the row, since the row goes to this row's chat. Out of the arrows'
           way; the keyboard's way to that chat is the title bar's list. */
        <button
          type="button"
          className="needs-you-mark rolled-up"
          data-mark="needs-you"
          data-leads-to={needs}
          tabIndex={-1}
          aria-label={`Go to ${needsName ?? "the chat"} below ${name}, which needs you`}
          title={`Go to ${needsName ?? "the chat"} below ${name}, which needs you`}
          onClick={() => onOpen(needs)}
        >
          <Hand aria-hidden="true" />
        </button>
      )}
    </li>
  );
});

/**
 * **What a row's second line holds.** While its chat works and something was heard of what it
 * is doing (#1493), that and how long it has been in its state, and nothing else: "running
 * cargo · 2m". The activity is cut short where the row is narrow and the time is not, since
 * the time is what tells a live "running cargo" from a stuck one. Otherwise where it works,
 * the time, its own branch and the chat it came from, as before.
 *
 * It reads its own chat, so the swap draws the second line's contents and not the row. The
 * line itself is always there and a fixed line high, so no row changes height (#1499).
 */
function SecondLine({
  session,
  activity,
  since,
  before,
  after,
}: {
  session: number;
  activity: ReactNode;
  since: ReactNode;
  before: ReactNode;
  after: ReactNode;
}) {
  if (useDoingSaid(session) !== undefined)
    return (
      <span className="doing-and-since">
        {activity}
        {since}
      </span>
    );
  return (
    <>
      {activity}
      {before}
      {since}
      {after}
    </>
  );
}

/**
 * How long a chat has been in its state (V100-19), where this window saw it come into it
 * (`stateClock.ts`). Reads its own chat's time, so a chat that moves, and the clock's tick,
 * redraw this and no row.
 */
const StateSince = memo(function StateSince({
  clock,
  session,
}: {
  clock: StateClock;
  session: number;
}) {
  const seconds = useStateSince(clock, session);
  if (seconds === null) return null;
  return (
    <span className="since" title="How long it has been in this state, as this window saw it">
      {sinceSaid(seconds)}
    </span>
  );
});
