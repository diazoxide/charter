import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useState,
  type FocusEvent,
  type KeyboardEvent,
} from "react";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { ChevronDown, ChevronRight, Hand, MessagesSquare, SquareTerminal } from "lucide-react";
import { BESIDE_ID, stopId, type Catalogued, type Offer } from "./actions";
import type { FinishedTask } from "./bindings";
import { ChatRowActivity } from "./ChatRowActivity";
import { ChatShownState } from "./ChatRows";
import { sameList, useChatsHere, useChatsSelect, type ChatStates } from "./chatState";
import {
  arranged,
  countsOf,
  filters,
  found,
  liveBelow,
  matches,
  sessionOrder,
  shownOfRow,
  sinceSaid,
  stamped,
  summaryOf,
  type Filter,
} from "./chatsList";
import { useChatsListPrefs } from "./chatsListPrefs";
import { needing, parentsIn, unfolded, type ChatRow } from "./chatsTree";
import { FinishedTasks } from "./FinishedTasks";
import type { ShownKind, TaskFacts } from "./shownState";
import { Menued } from "./Menus";
import { PersonaMark } from "./PersonaMark";
import { useTabStop } from "./roving";
import { stateClock, useStateSince, type StateClock } from "./stateClock";
import { SHAPES } from "./StateShown";

/** A row's id in the section's roving focus. */
const rowId = (session: number) => `chats:${session}`;

/** No rows of the catalogue: what a section drawn on its own, in a test, offers. */
const NO_OFFERS: Catalogued = new Map();

/** No finished tasks: what a section drawn without them lists. */
const NONE_FINISHED: ReadonlyMap<number, FinishedTask[]> = new Map();
const NO_TASKS: readonly FinishedTask[] = [];
const NOT_REOPENED = () => Promise.resolve<string | undefined>(undefined);
const NOT_CLEARED = () => undefined;

/** The states the filter's chips ask for, as each is said. */
const CHIPS: readonly { kind: ShownKind; says: string }[] = [
  { kind: "needs-you", says: "needs you" },
  { kind: "working", says: "working" },
];

/** What the list is drawn from that the chats' own moves change: held still while the pointer
 *  or the keyboard is in the list. */
type Moving = {
  /** The sessions, in the order they stand. */
  order: readonly number[];
  /** The sessions with a chat under them that is not over, which are open by themselves. */
  live: readonly number[];
  /** The chats the filter asks for, or nothing while it asks for none. */
  asked: readonly number[] | null;
};

const sameAsked = (one: readonly number[] | null, other: readonly number[] | null) =>
  one === other || (one !== null && other !== null && sameList(one, other));

/** What a key on a row asks for beside opening it. */
type Asked = "beside" | "stop";

/**
 * **Every running chat of the project, in one tree** (#1447), in the left region above the
 * explorer. The explorer answers what is running in this workspace; this answers who is doing
 * what across all of them, and which chat started which.
 *
 * **A row is a way to the chat.** Pressing one, or Enter on it, shows its chat (`onOpen`, the
 * one way a row opens): a task has no tab of its own, and is shown inside the tab of the
 * session that asked for it (#1486). Space asks for it beside the chat in front, and Delete on a task asks
 * to stop it: both are rows of the window's catalogue, and one that cannot run says why here.
 *
 * **At fifty chats** (#1499). The sessions that need you stand first, then the ones at work,
 * then the rest, and a session stands where the most urgent chat under it would; the chats
 * under a session stay in the order they were started. A session with chats under it is open
 * while one of them is not over, and folds by itself when all are, saying how its finished
 * tasks ended on its own row; **a fold set by hand wins** until it is set again. A filter over
 * the list finds a chat by its name, persona, workspace or state, keeps the rows above a match,
 * and says how many it hides. A row is two lines, or one (`chatsListPrefs.ts`).
 *
 * **Nothing moves under a resting pointer.** While the pointer is over the list, or the
 * keyboard is in it, the order, the folds the list makes by itself and the filter's answer are
 * held as they were, and are brought up to date when both have left. What the person does
 * themselves (a fold, a word typed in the filter) is applied at once.
 *
 * **A row says its chat's state in a word beside a mark** (#1484): `ChatShownState`, which the
 * explorer's rows draw too.
 *
 * **The hand rolls up** (#1448). A chat that needs you wears it on its own row, as its state's
 * mark, and so does every row above it, where it is a button that goes to that chat. A row
 * with chats under it folds, and a folded row still wears the hand for what it hides, so a
 * fold never hides a chat that needs you.
 *
 * **A task that has finished stays under the chat that asked** (#1485), as a finished entry
 * and not a chat: its program has ended (`FinishedTasks`). They are drawn under their chat's
 * row, after the chats still running under it, and are hidden with them when the row is folded.
 *
 * **A row's menu stops its chat** (#1448): Stop, and Stop with everything below it, from the
 * window's one catalogue, as a tab's menu has them.
 *
 * **A chat moving redraws its own marks and no row** (SC-3). The rows are held, on plain
 * values, and each state mark reads its own chat's state, so fifty chats cost one mark per
 * move. The hands are read here from the queue, which keeps its identity until it changes, and
 * the order, the folds and the filter's answer are each read through the store's selector, so
 * the list is drawn again only when one of them changes: a reorder is the one thing that moves
 * rows.
 */
export function ChatsSection({
  rows,
  front,
  onOpen,
  offers = NO_OFFERS,
  onPress,
  stopping,
  finished = NONE_FINISHED,
  onClearFinished = NOT_CLEARED,
  onReopen = NOT_REOPENED,
}: {
  rows: readonly ChatRow[];
  /** The chat in front, whose row is the current one. */
  front?: number;
  /** A row was pressed: go to that chat. A task is shown inside its session's tab (#1486). */
  onOpen: (session: number) => void;
  /** The catalogue as it stands, by id: what each row's menu reads its rows from. */
  offers?: Catalogued;
  /** Carries out a row of a menu. */
  onPress?: (offer: Offer) => void;
  /** The chats being stopped, whose rows say so. */
  stopping?: ReadonlySet<number>;
  /** Each chat's finished tasks, by its number (#1485). */
  finished?: ReadonlyMap<number, FinishedTask[]>;
  /** Clear finished: takes those rows away, and nothing else. */
  onClearFinished?: (ids: string[]) => void;
  /** Reopens a finished task as an ordinary chat; answers why not, where it could not. */
  onReopen?: (task: FinishedTask) => Promise<string | undefined>;
}) {
  const prefs = useChatsListPrefs();
  const chats = useChatsHere();
  const [text, setText] = useState("");
  const [kinds, setKinds] = useState<readonly ShownKind[]>([]);
  const filter = useMemo<Filter>(() => ({ text, kinds }), [text, kinds]);
  const filtering = filters(filter);
  /** The folds the person set, by chat: true is folded. This window's own, and forgotten with
   *  it. A chat that is not here folds and opens by itself. */
  const [hand, setHand] = useState<ReadonlyMap<number, boolean>>(() => new Map());
  /** Why a key pressed on a row did nothing, while that is worth saying. */
  const [said, setSaid] = useState<string>();

  // What the chats' own moves change, each read through the selector: a move that changes
  // none of them draws nothing here.
  const kindsOf = (states: ChatStates) => {
    const queue = new Set(states.needsYou);
    return (row: ChatRow) => shownOfRow(states, row, queue)?.kind;
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
      const queue = new Set(states.needsYou);
      return rows
        .filter((row) => matches(row, shownOfRow(states, row, queue), filter))
        .map((row) => row.session);
    },
    sameAsked,
  );

  // **Held still while the pointer is over the list or the keyboard is in it** (V100-47): no
  // row moves under a click. Taken as the pointer or the focus comes in, let go when both have
  // left.
  const [over, setOver] = useState(false);
  const [inside, setInside] = useState(false);
  // The list is not drawn with no chat to list, so nothing would say the pointer left it.
  if (rows.length === 0 && (over || inside)) {
    setOver(false);
    setInside(false);
  }
  const resting = over || inside;
  const [held, setHeld] = useState<Moving | null>(null);
  const moving: Moving = { order, live, asked };
  if (resting && held === null) setHeld(moving);
  if (!resting && held !== null) setHeld(null);
  const now = resting && held !== null ? held : moving;
  /** The filter was changed by the person: its answer is theirs to see at once. */
  const refilter = (how: () => void) => {
    how();
    setHeld(null);
  };

  /** What each chat's finished tasks come to: a folded row's summary, and whether one of them
   *  stands alone, which keeps its chat open. */
  const ended = useMemo(
    () =>
      new Map(
        [...finished]
          .filter(([, tasks]) => tasks.length > 0)
          .map(([session, tasks]) => [
            session,
            { summary: summaryOf(tasks), alone: tasks.some((task) => !task.folds) },
          ]),
      ),
    [finished],
  );
  /** The rows in the order they stand, less what the filter hides. */
  const base = useMemo(() => {
    const inOrder = arranged(rows, now.order);
    return stamped(now.asked === null ? inOrder : found(inOrder, new Set(now.asked)));
  }, [rows, now.order, now.asked]);
  const parents = useMemo(() => parentsIn(base), [base]);
  /**
   * Whether each row's own rows are drawn under it, for a row that has some (a chat or a
   * finished task). **A fold set by hand wins** (V100-48). Otherwise a row is open while a
   * chat under it is not over or a finished task of its own stands alone, and a filter opens
   * every row it kept a chat under.
   */
  const opens = useMemo(() => {
    const alive = new Set(now.live);
    const by = new Map<number, boolean>();
    for (const row of base) {
      const { session } = row;
      if (!parents.has(session) && !ended.has(session)) continue;
      const set = hand.get(session);
      by.set(
        session,
        set !== undefined
          ? !set
          : now.asked !== null
            ? parents.has(session)
            : alive.has(session) || ended.get(session)?.alone === true,
      );
    }
    return by;
  }, [base, parents, ended, hand, now.live, now.asked]);
  const folded = useMemo(
    () => new Set([...opens].filter(([, open]) => !open).map(([session]) => session)),
    [opens],
  );
  const drawn = useMemo(() => unfolded(base, folded), [base, folded]);
  const needsYou = useChatsSelect(chats, (states) => states.needsYou);
  const leads = useMemo(() => needing(rows, needsYou), [rows, needsYou]);
  const byNumber = useMemo(() => new Map(rows.map((row) => [row.session, row])), [rows]);

  // How long each chat has been in its state, as this window saw it: read off every chat, drawn
  // or not, so a row that was folded away says the same time when it is drawn again.
  const [clock] = useState(stateClock);
  useEffect(() => {
    const read = () => clock.read(chats.store.statesFor(chats.plane), rows, Date.now());
    read();
    return chats.store.subscribe(read);
  }, [chats, clock, rows]);

  const fold = useCallback((session: number, shut: boolean) => {
    setHand((was) => {
      if (was.get(session) === shut) return was;
      const set = new Map(was);
      set.set(session, shut);
      return set;
    });
  }, []);
  const press = useCallback((offer: Offer) => onPress?.(offer), [onPress]);
  const act = useCallback(
    (session: number, what: Asked) => {
      const offer = offers.get(what === "beside" ? BESIDE_ID : stopId(session));
      if (offer === undefined) return;
      if (!offer.available) {
        setSaid(offer.reason);
        return;
      }
      setSaid(undefined);
      onPress?.(offer);
    },
    [offers, onPress],
  );
  const stop = useTabStop(
    front === undefined ? undefined : rowId(front),
    drawn.map((row) => rowId(row.session)),
  );
  const left = (event: FocusEvent<HTMLElement>) => {
    const to = event.relatedTarget;
    if (to instanceof Node && event.currentTarget.contains(to)) return;
    setInside(false);
    setSaid(undefined);
  };
  const hidden = rows.length - base.length;
  return (
    <section className="chats-section" data-testid="chats-section" aria-labelledby="chats-title">
      <h2 className="sidebar-title" id="chats-title">
        <MessagesSquare className="node-icon" aria-hidden="true" />
        Chats
      </h2>
      {rows.length === 0 ? (
        <p className="empty">No chats are running in this project.</p>
      ) : (
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
                refilter(() => {
                  setText("");
                  setKinds([]);
                });
              }}
            />
            {/* The chips: a box each, since each is on or off, drawn as a chip. */}
            {CHIPS.map((chip) => (
              <label
                key={chip.kind}
                className="chats-chip"
                data-on={kinds.includes(chip.kind) || undefined}
              >
                <input
                  type="checkbox"
                  tabIndex={0}
                  checked={kinds.includes(chip.kind)}
                  onChange={() =>
                    refilter(() =>
                      setKinds((was) =>
                        was.includes(chip.kind)
                          ? was.filter((kind) => kind !== chip.kind)
                          : [...was, chip.kind],
                      ),
                    )
                  }
                />
                {chip.says}
              </label>
            ))}
          </div>
          {/* Said whenever a filter is on, so the rows it hides are never simply missing. */}
          <p className="chats-hidden" role="status">
            {!filtering
              ? ""
              : base.length === 0
                ? "No chat matches the filter."
                : hidden === 0
                  ? "The filter hides no chat."
                  : `The filter hides ${hidden} of ${rows.length} chats.`}
          </p>
          <p className="chats-said" role="status">
            {said ?? ""}
          </p>
          <div
            className="chats-list"
            onPointerEnter={() => setOver(true)}
            onPointerLeave={() => setOver(false)}
            onFocus={() => setInside(true)}
            onBlur={left}
            onKeyDown={(event) => {
              // Escape in the list takes the filter off, as it does in the filter's own box.
              if (event.key !== "Escape" || !filtering) return;
              event.preventDefault();
              refilter(() => {
                setText("");
                setKinds([]);
              });
            }}
          >
            {drawn.length > 0 && (
              <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
                <ul role="tree" aria-label="Chats of this project">
                  {drawn.map((row, at) => {
                    const lead = leads.get(row.session);
                    const open = opens.get(row.session);
                    const asker = row.parent === null ? undefined : byNumber.get(row.parent);
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
                        from={row.orphaned ? row.from : null}
                        tab={row.tab}
                        current={row.session === front}
                        open={open ?? null}
                        summary={open === false ? (ended.get(row.session)?.summary ?? null) : null}
                        needs={lead === undefined ? null : lead}
                        needsName={
                          lead === undefined || lead === row.session
                            ? null
                            : (byNumber.get(lead)?.name ?? null)
                        }
                        stopping={stopping?.has(row.session) ?? false}
                        offers={offers}
                        clock={clock}
                        onOpen={onOpen}
                        onFold={fold}
                        onPress={press}
                        onAct={act}
                      />,
                      // The finished tasks of each chat whose rows end here (#1485): this
                      // row's own, where no chat is drawn under it, then those of every chat
                      // above it that this row is the last one under.
                      ...endingAt(drawn, at, folded).map((one) => (
                        <FinishedTasks
                          key={`finished:${one.session}`}
                          asker={one.name}
                          level={one.level + 1}
                          tasks={finished.get(one.session) ?? NO_TASKS}
                          onClear={onClearFinished}
                          onReopen={onReopen}
                        />
                      )),
                    ];
                  })}
                </ul>
              </RovingFocusGroup.Root>
            )}
          </div>
        </>
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
 * what a screen reader is told. The second, dimmer, is what the chat is doing (`ChatRowActivity`,
 * which #1493 fills), where it works, how long it has been in its state, its own branch and
 * the closed chat it came from. **On one line the second is not drawn at all**, and what of it
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
  tab,
  current,
  open,
  summary,
  needs,
  needsName,
  stopping,
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
  /** The closed chat it came from, by name; nothing for a chat drawn under its parent. */
  from: string | null;
  tab: boolean;
  current: boolean;
  /** Whether its rows are drawn under it, for a row that has some; nothing for a leaf. */
  open: boolean | null;
  /** How its finished tasks ended (`summaryOf`), while it is folded over some. */
  summary: string | null;
  /** The chat its hand leads to: itself, a chat below it, or none when it wears no hand. */
  needs: number | null;
  /** That chat's name, when it is a chat below this one. */
  needsName: string | null;
  stopping: boolean;
  offers: Catalogued;
  clock: StateClock;
  onOpen: (session: number) => void;
  onFold: (session: number, shut: boolean) => void;
  onPress: (offer: Offer) => void;
  onAct: (session: number, what: Asked) => void;
}) {
  // The keys of a row. Enter is the button's own press, which opens it. Space asks for it
  // beside the chat in front, and Delete asks to stop a task: the catalogue's rows, which ask
  // first or say why not. The tree's own (WAI-ARIA "Tree View"): Right opens a folded row,
  // Left folds an open one. Up and Down are the roving group's.
  const keys = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === " ") onAct(session, "beside");
    else if (event.key === "Delete" && task) onAct(session, "stop");
    else if (open !== null && event.key === "ArrowRight" && !open) onFold(session, false);
    else if (open !== null && event.key === "ArrowLeft" && open) onFold(session, true);
    else return;
    event.preventDefault();
  };
  const counts = summary === null ? [] : countsOf(summary);
  const ownBranch = branch === null ? null : `own branch ${branch}`;
  const cameFrom = from === null ? null : `from ${from}`;
  /** What the second line says that does not change by itself: one line's tooltip. */
  const second = [elsewhere ? workspace : null, ownBranch, cameFrom].filter(
    (one): one is string => one !== null,
  );
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
      <Menued on={{ on: "listed", session }} offers={offers} onPress={onPress}>
        <RovingFocusGroup.Item asChild tabStopId={rowId(session)}>
          <button
            type="button"
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
            data-tab={tab}
            data-lines={lines}
            title={lines === 1 && second.length > 0 ? second.join(" · ") : undefined}
            onClick={() => onOpen(session)}
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
              <span className="session" title={name}>
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
              />
              {counts.length > 0 && (
                /* Folded over finished tasks: how they ended, in the marks a state has
                   (V100-48, "steward 4 · ✓5"). */
                <span
                  className="below-summary"
                  role="img"
                  aria-label={`${counts.map((one) => `${one.count} ${one.word}`).join(", ")}, folded`}
                >
                  {counts.map((one) => {
                    const Shape = SHAPES[one.shape];
                    return (
                      <span key={one.shape} className="counted" data-shape={one.shape}>
                        <Shape aria-hidden="true" />
                        {one.count}
                      </span>
                    );
                  })}
                </span>
              )}
              {stopping && <span className="stopping">Stopping…</span>}
            </span>
            {lines === 2 && (
              <span className="line two">
                <ChatRowActivity session={session} />
                {elsewhere && <span className="workspace">{workspace}</span>}
                <StateSince clock={clock} session={session} />
                {ownBranch !== null && (
                  <span
                    className="own-branch"
                    title="A branch of its own, which nothing merges for it"
                  >
                    {ownBranch}
                  </span>
                )}
                {cameFrom !== null && <span className="from">{cameFrom}</span>}
              </span>
            )}
          </button>
        </RovingFocusGroup.Item>
      </Menued>
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
