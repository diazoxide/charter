import * as Menu from "@radix-ui/react-dropdown-menu";
import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AwayRefusal, FinishedTask } from "./bindings";
import { useChatsSelect, type Chats } from "./chatState";
import { Notice } from "./Notice";
import {
  AWAY_AFTER_MS,
  awayPartsSaid,
  awaySaid,
  awaySummaryOf,
  cameToNeedOf,
  countedAway,
  NOTHING_READ,
  type AwaySummaryOf,
  sameRead,
  readOf,
  type Away,
  type AwayGo,
  type AwayItem,
  type Queued,
} from "./awayCounts";

/**
 * **While you were away: one summary of what happened** (#1514, V100-65).
 *
 * One Notice under the strip of each project, drawn when the person comes back after
 * {@link AWAY_AFTER_MS} or more away from this window, and only when something happened
 * meanwhile: "While you were away: 7 tasks done, 1 failed, 2 waiting on you". What it counts
 * is `awayCounts.ts`'s; when the person was away is {@link useTimeAway}'s, which the project's
 * view holds whether or not it is in front ({@link useAwaySummary}), so each project of a
 * window has its own.
 *
 * **Each part is a link to the chats it counts.** A part of one goes straight there; a part of
 * several opens a list of them, each of which goes to its own. A chat waiting on you is shown
 * where it lives (its question, its Notice, its prompt are there, answerable as they always
 * are); a finished task goes where a needs-you item's Go takes it: its chat while that is
 * open, else its finished row under the session that asked, where its report is. A dispatch
 * refused while nobody was at its chat (#1507) opens the title bar's needs-you list, where it
 * is answered (#1551). Every line
 * the tasks and their sessions said stays in each session's Activity.
 *
 * **It answers nothing and hides nothing.** Going to a failed task marks that failure looked
 * at, as the needs-you item's Go does, which takes its hand off the title bar; that is the one
 * thing a press here writes. Dismiss puts the summary away and nothing else: a question still
 * waits where it was asked, a needs-you item stays on the title bar's list, a failed task's row
 * stands.
 *
 * **Off in Settings** (`chats.away`, "Away summary"), nothing is watched and nothing is drawn,
 * and the window is as it was before it.
 */
export function AwaySummary({
  away,
  onShowChat,
  onShowFinished,
  onShowNeedsYou,
}: {
  /** What the project's summary counts now ({@link useAwaySummary}). */
  away: AwayNow;
  /** Shows an open chat where it lives. */
  onShowChat: (session: number) => void;
  /** Goes to a finished task, as a needs-you item's Go does. */
  onShowFinished: (task: FinishedTask) => void;
  /** Opens the title bar's needs-you list, where a refused dispatch is answered. */
  onShowNeedsYou?: () => void;
}) {
  const { summary, dismiss } = away;
  if (countedAway(summary) === 0) return null;
  const go = (where: AwayGo) =>
    where.to === "chat"
      ? onShowChat(where.session)
      : where.to === "finished"
        ? onShowFinished(where.task)
        : onShowNeedsYou?.();
  const parts = awayPartsSaid(summary);
  return (
    <Notice cause="away-summary" label={awaySaid(summary)} onDismiss={dismiss}>
      While you were away:{" "}
      {parts.map(({ part, says }, at) => (
        <Fragment key={part}>
          {at > 0 && ", "}
          <AwayPart says={says} items={summary[part]} onGo={go} />
        </Fragment>
      ))}
    </Notice>
  );
}

/** A project's summary as it stands, and the way to put it away. */
export type AwayNow = { summary: AwaySummaryOf; dismiss: () => void };

/**
 * **A project's summary, kept whether or not the project is in front**: the project's view
 * calls this above where it stops drawing a project that is behind another, so each project of
 * a window has its own times away, counted as they happen, and its summary is there when the
 * person switches to it.
 */
export function useAwaySummary({
  chats,
  on,
  finished,
  nameOf,
  refusedAway,
}: {
  /** The project's chats, which the queue is read from. */
  chats: Chats;
  /** Whether the person wants a summary at all (`chats.away`). */
  on: boolean;
  /** The finished tasks of the project's open chats, by asking chat. */
  finished: ReadonlyMap<number, readonly FinishedTask[]>;
  /** What a chat is called here. */
  nameOf: (session: number) => string;
  /** The project's dispatches refused while nobody was at their chat (#1507, #1551). */
  refusedAway?: readonly AwayRefusal[];
}): AwayNow {
  const { away, cameToNeed, dismiss, counts } = useTimeAway(chats, on);
  // Read only while there is a time away to count, and only the shares counted: otherwise no
  // move of a chat redraws the project for this.
  const none = away.length === 0;
  const states = useChatsSelect(chats, (now) => (none ? NOTHING_READ : readOf(now)), sameRead);
  const tasks = useMemo(() => [...finished.values()].flat(), [finished]);
  const summary = useMemo(
    () => awaySummaryOf({ away, finished: tasks, cameToNeed, states, nameOf, refusedAway }),
    [away, tasks, cameToNeed, states, nameOf, refusedAway],
  );
  const counted = countedAway(summary);
  useEffect(() => counts(counted), [counts, counted]);
  return useMemo(() => ({ summary, dismiss }), [summary, dismiss]);
}

/** One part of the summary: a link to its one chat, or to the list of its several. */
function AwayPart({
  says,
  items,
  onGo,
}: {
  says: string;
  items: readonly AwayItem[];
  onGo: (where: AwayGo) => void;
}) {
  // Held here and opened on the press itself, as the title bar's list is (`NeedsYou.tsx`): a
  // link in a sentence is pressed, never opened by a pointer going down on it.
  const [open, setOpen] = useState(false);
  if (items.length === 1)
    return (
      // Where it goes is in its name: a title alone is read by few screen readers.
      <button
        type="button"
        className="away-part"
        aria-label={`${says}: ${items[0].says}`}
        title={items[0].says}
        onClick={() => onGo(items[0].go)}
      >
        {says}
      </button>
    );
  return (
    <Menu.Root modal={false} open={open} onOpenChange={setOpen}>
      <Menu.Trigger asChild>
        <button
          type="button"
          className="away-part"
          aria-haspopup="menu"
          onPointerDown={(event) => event.preventDefault()}
          onClick={() => setOpen((up) => !up)}
        >
          {says}
        </button>
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Content
          className="more-menu away-menu"
          align="start"
          sideOffset={4}
          collisionPadding={8}
        >
          {items.map((item) => (
            <Menu.Item key={item.key} className="more-tab" onSelect={() => onGo(item.go)}>
              {item.says}
            </Menu.Item>
          ))}
        </Menu.Content>
      </Menu.Portal>
    </Menu.Root>
  );
}

/** When the person was away from a project's window, and what that summary is to count. */
export type TimeAway = {
  /** The times away the summary counts, oldest first. */
  away: readonly Away[];
  /** The chats that came into the needs-you queue during them. */
  cameToNeed: ReadonlySet<number>;
  /** Puts the summary away: forgets the times, and nothing else. */
  dismiss: () => void;
  /** Told by the summary what it counts now, after each change. */
  counts: (counted: number) => void;
};

/** How long after a return a summary that counts nothing yet waits for what the core has not
 *  said yet (a finished row read a moment after the return) before its time away is let go. */
export const SETTLES_MS = 10 * 1000;

type Held = { away: readonly Away[]; cameToNeed: ReadonlySet<number> };
type Seen = { at: number; queue: Queued };

const NOT_AWAY: Held = { away: [], cameToNeed: new Set() };

/** What the person's input is: any of these on the window is them being at it. */
const INPUTS = ["pointerdown", "pointermove", "keydown", "wheel"] as const;

/**
 * **When the person was away from this project's window**, and the chats that came to need
 * them meanwhile. Held by the project's view above where it stops drawing a project that is not
 * in front, so a project behind another has its own times away and its summary is there when
 * the person switches to it.
 *
 * **Away is any of three things, for {@link AWAY_AFTER_MS} or more**, as the window reads the
 * person's presence elsewhere (the Chats list lets go on `blur`; a project fetches on `focus`):
 *
 * - the window not the one in use: from `blur` to `focus`;
 * - the window hidden: from `visibilitychange` to hidden, to visible again;
 * - **no input while it is in use**: no pointer, key or wheel on it. The time away began at the
 *   last input, and the first input after it is the return. A display that slept over a window
 *   left in front fires neither `blur` nor `focus`.
 *
 * A window that starts without the focus is away from its start.
 *
 * The queue is read at the last sign of the person and again at the return: a chat in it on the
 * way back that was not on the way out, or that was and has something new for them since,
 * came to need them while they were away ({@link cameToNeedOf}).
 *
 * **Only the current time away is ever counted with what is still standing.** Another time
 * away adds to a summary that still says something; a summary that says nothing any more (its
 * parts were answered or cleared) lets its times go, as does one that still says nothing
 * {@link SETTLES_MS} after its return, so nothing read later can bring an old time back. A
 * reload or a relaunch starts with none.
 *
 * Nothing is watched while `on` is false, and turning it off forgets what was kept.
 */
export function useTimeAway(chats: Chats, on: boolean): TimeAway {
  const [held, setHeld] = useState<Held>(NOT_AWAY);
  // Turned off: what was kept is forgotten, adjusted while drawing as React has state follow
  // what it is drawn from.
  if (!on && held !== NOT_AWAY) setHeld(NOT_AWAY);
  const counted = useRef(0);
  /** When the person last came back, while what it summed up has not been settled. */
  const backAt = useRef<number | null>(null);
  const letGo = useCallback(() => {
    backAt.current = null;
    setHeld(NOT_AWAY);
  }, []);
  useEffect(() => {
    if (!on) return;
    const queue = (): Queued => chats.store.statesFor(chats.plane);
    /** The last sign of the person at the window. */
    let seen: Seen = { at: Date.now(), queue: queue() };
    /** Where the person went away from the window, while they are away. */
    let left: Seen | null = null;
    const idle = (now: number) => now - seen.at >= AWAY_AFTER_MS;
    const leave = () => {
      if (left !== null) return;
      const now = Date.now();
      // Away already, with no input: the time away began at the last one.
      left = idle(now) ? seen : { at: now, queue: queue() };
    };
    const back = () => {
      const now = Date.now();
      const was = left ?? (idle(now) ? seen : null);
      left = null;
      seen = { at: now, queue: queue() };
      if (was === null || now - was.at < AWAY_AFTER_MS) return;
      const time = { from: was.at, to: now };
      const came = cameToNeedOf(was.queue, seen.queue);
      backAt.current = now;
      setHeld((kept) =>
        counted.current === 0
          ? { away: [time], cameToNeed: new Set(came) }
          : { away: [...kept.away, time], cameToNeed: new Set([...kept.cameToNeed, ...came]) },
      );
    };
    const input = () => {
      // Input reaching a window that is not in use is not the person coming back to it.
      if (left !== null) return;
      const now = Date.now();
      if (idle(now)) return back();
      seen = { at: now, queue: queue() };
      const since = backAt.current;
      if (since !== null && now - since >= SETTLES_MS) {
        backAt.current = null;
        if (counted.current === 0) setHeld(NOT_AWAY);
      }
    };
    const shown = () => (document.visibilityState === "hidden" ? leave() : back());
    if (!document.hasFocus()) leave();
    window.addEventListener("blur", leave);
    window.addEventListener("focus", back);
    document.addEventListener("visibilitychange", shown);
    for (const kind of INPUTS)
      window.addEventListener(kind, input, { capture: true, passive: true });
    return () => {
      window.removeEventListener("blur", leave);
      window.removeEventListener("focus", back);
      document.removeEventListener("visibilitychange", shown);
      for (const kind of INPUTS) window.removeEventListener(kind, input, { capture: true });
    };
  }, [chats, on]);
  const counts = useCallback(
    (now: number) => {
      const was = counted.current;
      counted.current = now;
      // Everything it counted has gone: its times go with it.
      if (was > 0 && now === 0) letGo();
    },
    [letGo],
  );
  return useMemo(
    () => ({ away: held.away, cameToNeed: held.cameToNeed, dismiss: letGo, counts }),
    [held, letGo, counts],
  );
}
