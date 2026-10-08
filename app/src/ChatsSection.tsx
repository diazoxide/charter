import { memo, useCallback, useMemo, useState, type KeyboardEvent } from "react";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { ChevronDown, ChevronRight, Hand, MessagesSquare, SquareTerminal } from "lucide-react";
import type { Catalogued, Offer } from "./actions";
import { ChatShownState } from "./ChatRows";
import { useChatsHere, useChatsSelect } from "./chatState";
import { needing, parentsIn, unfolded, type ChatRow } from "./chatsTree";
import type { TaskFacts } from "./shownState";
import { Menued } from "./Menus";
import { PersonaMark } from "./PersonaMark";
import { useTabStop } from "./roving";

/** A row's id in the section's roving focus. */
const rowId = (session: number) => `chats:${session}`;

/** No rows of the catalogue: what a section drawn on its own, in a test, offers. */
const NO_OFFERS: Catalogued = new Map();

/**
 * **Every running chat of the project, in one tree** (#1447), in the left region above the
 * explorer. The explorer answers what is running in this workspace; this answers who is doing
 * what across all of them, and which chat started which.
 *
 * **A row is a way to the chat.** Pressing one brings its tab forward, on whichever workspace's
 * strip it is. A task has no tab: its row says so, and pressing it opens an ordinary tab.
 *
 * **A row says its chat's state in a word beside a mark** (#1484): `ChatShownState`, which the
 * explorer's rows draw too.
 *
 * **The hand rolls up** (#1448). A chat that needs you wears it on its own row, as its state's
 * mark, and so does every row above it, where it is a button that goes to that chat. A row
 * with chats under it folds, and a folded row still wears the hand for what it hides, so a
 * fold never hides a chat that needs you.
 *
 * **A row's menu stops its chat** (#1448): Stop, and Stop with everything below it, from the
 * window's one catalogue, as a tab's menu has them.
 *
 * **A chat moving redraws its own marks and no row** (SC-3). The rows are held, on plain
 * values, and each state mark reads its own chat's state, so fifty chats cost one mark per
 * move. The hands are read here from the queue, which keeps its identity until it changes.
 */
export function ChatsSection({
  rows,
  front,
  onOpen,
  offers = NO_OFFERS,
  onPress,
  stopping,
}: {
  rows: readonly ChatRow[];
  /** The chat in front, whose row is the current one. */
  front?: number;
  /** A row was pressed: bring that chat forward, opening its tab when it has none. */
  onOpen: (session: number) => void;
  /** The catalogue as it stands, by id: what each row's menu reads its rows from. */
  offers?: Catalogued;
  /** Carries out a row of a menu. */
  onPress?: (offer: Offer) => void;
  /** The chats being stopped, whose rows say so. */
  stopping?: ReadonlySet<number>;
}) {
  /** The chats whose rows are folded. This window's own, and forgotten with it. */
  const [folded, setFolded] = useState<ReadonlySet<number>>(() => new Set());
  const parents = useMemo(() => parentsIn(rows), [rows]);
  const drawn = useMemo(() => unfolded(rows, folded), [rows, folded]);
  const needsYou = useChatsSelect(useChatsHere(), (states) => states.needsYou);
  const leads = useMemo(() => needing(rows, needsYou), [rows, needsYou]);
  const names = useMemo(() => new Map(rows.map((row) => [row.session, row.name])), [rows]);
  const fold = useCallback((session: number, shut: boolean) => {
    setFolded((was) => {
      if (was.has(session) === shut) return was;
      const now = new Set(was);
      if (shut) now.add(session);
      else now.delete(session);
      return now;
    });
  }, []);
  const press = useCallback((offer: Offer) => onPress?.(offer), [onPress]);
  const stop = useTabStop(
    front === undefined ? undefined : rowId(front),
    drawn.map((row) => rowId(row.session)),
  );
  return (
    <section className="chats-section" data-testid="chats-section" aria-labelledby="chats-title">
      <h2 className="sidebar-title" id="chats-title">
        <MessagesSquare className="node-icon" aria-hidden="true" />
        Chats
      </h2>
      {rows.length === 0 ? (
        <p className="empty">No chats are running in this project.</p>
      ) : (
        <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
          <ul role="tree" aria-label="Chats of this project">
            {drawn.map((row) => {
              const lead = leads.get(row.session);
              return (
                <Row
                  key={row.session}
                  session={row.session}
                  name={row.name}
                  persona={row.persona}
                  workspace={row.workspace}
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
                  open={parents.has(row.session) ? !folded.has(row.session) : null}
                  needs={lead === undefined ? null : lead}
                  needsName={
                    lead === undefined || lead === row.session ? null : (names.get(lead) ?? null)
                  }
                  stopping={stopping?.has(row.session) ?? false}
                  offers={offers}
                  onOpen={onOpen}
                  onFold={fold}
                  onPress={press}
                />
              );
            })}
          </ul>
        </RovingFocusGroup.Root>
      )}
    </section>
  );
}

/** One chat's row. Held on plain values, so only a row whose own facts changed is drawn again. */
const Row = memo(function Row({
  session,
  name,
  persona,
  workspace,
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
  needs,
  needsName,
  stopping,
  offers,
  onOpen,
  onFold,
  onPress,
}: {
  session: number;
  name: string;
  persona: string | null;
  workspace: string;
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
  /** The chat its hand leads to: itself, a chat below it, or none when it wears no hand. */
  needs: number | null;
  /** That chat's name, when it is a chat below this one. */
  needsName: string | null;
  stopping: boolean;
  offers: Catalogued;
  onOpen: (session: number) => void;
  onFold: (session: number, shut: boolean) => void;
  onPress: (offer: Offer) => void;
}) {
  // The tree's own keys (WAI-ARIA "Tree View"): Right opens a folded row, Left folds an open
  // one. Up and Down are the roving group's.
  const keys = (event: KeyboardEvent<HTMLElement>) => {
    if (open === null) return;
    if (event.key === "ArrowRight" && !open) onFold(session, false);
    else if (event.key === "ArrowLeft" && open) onFold(session, true);
    else return;
    event.preventDefault();
  };
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
            title={tab ? undefined : "No tab yet. Press to open it as a tab."}
            onClick={() => onOpen(session)}
            onKeyDown={keys}
          >
            {persona === null ? (
              <SquareTerminal className="node-icon" aria-hidden="true" />
            ) : (
              <PersonaMark persona={persona} />
            )}
            <span className="session">{name}</span>
            <span className="workspace">{workspace}</span>
            {branch !== null && (
              <span className="own-branch" title="A branch of its own, which nothing merges for it">
                own branch {branch}
              </span>
            )}
            {/* Its state, a mark and a word (#1484): the hand of a chat that needs you is
                this mark, so the row draws no second one. */}
            <ChatShownState
              session={session}
              shell={shell}
              report={report}
              outcome={outcome}
              asking={asking}
              harness={harness}
            />
            {from !== null && <span className="from">from {from}</span>}
            {!tab && <span className="no-tab">no tab</span>}
            {stopping && <span className="stopping">Stopping…</span>}
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
