/**
 * Which of a project's chats the person is **stopping** (#1448), kept current by being told, as
 * `smartClose.ts` keeps the chats wrapping up.
 *
 * The core holds the truth (`stopping.rs`): it gives a chat another chat started one short turn
 * to write what it did, ends it, and tells the chat that asked. The window is told each step on
 * `chat-stop`, says the chat is stopping while it is, and takes its tab away when it has ended
 * (`onStopped`).
 */
import { useEffect, useRef, useState } from "react";
import { listen } from "./here";
import { commands, type ChatStop, type PlaneId } from "./bindings";

/** Folds one step into the chats being stopped. */
export function stepped(was: ReadonlySet<number>, step: ChatStop): ReadonlySet<number> {
  const now = new Set(was);
  if (step.phase === "stopping") now.add(step.session);
  else now.delete(step.session);
  return now;
}

const NOTHING_STOPPING: ReadonlySet<number> = new Set();

/**
 * The chats of `plane` being stopped, and every chat that has ended handed to `onStopped`.
 *
 * Listening starts at the mount and the snapshot (`stopping_chats`) is asked after it, so a step
 * that lands in between is not lost: `chatState.ts`'s order, for its reason.
 */
export function useStopping(
  plane: PlaneId,
  onStopped: (session: number) => void,
): ReadonlySet<number> {
  // Keyed by the project it is about: every project numbers its chats from one.
  const [known, setKnown] = useState<{ plane?: PlaneId; stopping: ReadonlySet<number> }>(() => ({
    stopping: NOTHING_STOPPING,
  }));

  const told = useRef(onStopped);
  useEffect(() => {
    told.current = onStopped;
  }, [onStopped]);

  useEffect(() => {
    let gone = false;
    let stop: (() => void) | undefined;
    const fold = (how: (was: ReadonlySet<number>) => ReadonlySet<number>) =>
      setKnown((was) => ({
        plane,
        stopping: how(was.plane === plane ? was.stopping : NOTHING_STOPPING),
      }));
    void (async () => {
      /** The chats a step said have ended, which an older snapshot must not put back. */
      const ended = new Set<number>();
      try {
        const unlisten = await listen<ChatStop>("chat-stop", (event) => {
          const step = event.payload;
          if (gone || step.plane !== plane) return;
          fold((was) => stepped(was, step));
          if (step.phase === "stopped") {
            ended.add(step.session);
            told.current(step.session);
          }
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in: a unit test, or a webview being torn down.
      }
      try {
        const now = await commands.stoppingChats(plane);
        if (gone || now.status !== "ok" || !Array.isArray(now.data)) return;
        const still = now.data.filter((session) => !ended.has(session));
        fold((was) => new Set([...was, ...still]));
      } catch {
        // Nothing to start from; the steps still arrive.
      }
    })();
    return () => {
      gone = true;
      stop?.();
    };
  }, [plane]);

  return known.plane === plane ? known.stopping : NOTHING_STOPPING;
}

/** What the person is asked before a stop, about the chat it was pressed on. */
export type StopAsked = {
  /** The chat, as its row names it. */
  name: string;
  /** With every chat below it. */
  below: boolean;
  /** How many running chats are below it. */
  under: number;
  /** Another chat started it, so it gets a last turn and that chat is told. */
  dispatched: boolean;
  /** It is already being stopped: the answer ends it without waiting. */
  already: boolean;
  /** How many of the chats below it are being stopped too. While any are, a chat that is
   *  already being stopped is held waiting for them, and has not been asked to write yet. */
  waitingOn: number;
};

/** The question's title. */
export function stopTitle(asked: StopAsked): string {
  if (asked.already && !asked.below) return `End chat ${asked.name} now?`;
  return asked.below
    ? `Stop chat ${asked.name} and everything below it?`
    : `Stop chat ${asked.name}?`;
}

/** What the answer does, said plainly, with its cost last. */
export function stopSays(asked: StopAsked): string {
  const { name, under } = asked;
  if (asked.already && !asked.below)
    return asked.waitingOn > 0
      ? `${name} is being stopped, and is waiting for the ${asked.waitingOn === 1 ? "1 chat" : `${asked.waitingOn} chats`} below it to end. This ends it now, without waiting for them. There is no undo.`
      : `${name} is being stopped, and has one short turn to write what it did. This ends it without waiting for that turn. There is no undo.`;
  if (asked.below && asked.already)
    return `${name} is being stopped already. The ${under === 1 ? "1 chat" : `${under} chats`} below it ${under === 1 ? "is" : "are"} stopped too, deepest first: each gets one short turn to write what it did, and the chat that asked is told you stopped it. Nothing outside them is touched. There is no undo.`;
  if (asked.below)
    return `${name} and the ${under === 1 ? "1 chat" : `${under} chats`} below it end, deepest first. A chat that another chat started gets one short turn first, to write what it did, and the chat that asked is told you stopped it. Nothing outside them is touched. There is no undo.`;
  return asked.dispatched
    ? `${name} gets one short turn to write what it did, then it ends. The chat that asked is told you stopped it. There is no undo.`
    : `${name} ends. There is no undo.`;
}

/** The button that does it. */
export function stopAnswer(asked: StopAsked): string {
  if (asked.already && !asked.below) return "End chat";
  if (asked.below && asked.already)
    return asked.under === 1 ? "Stop 1 chat" : `Stop ${asked.under} chats`;
  return asked.below ? `Stop ${asked.under + 1} chats` : "Stop chat";
}

/**
 * **What the person is asked before Stop all tasks** (#1498, V100-53): the session, and the
 * tasks at work below it that the answer ends, as the core read them (`all_tasks_ending`). The
 * answer ends those and no more, so the count the question names is the count that ends.
 */
export type StopAllAsked = {
  /** The session, as its row names it. */
  name: string;
  /** The tasks the answer ends, by number. */
  tasks: readonly number[];
};

function tasksSaid(count: number): string {
  return count === 1 ? "1 task" : `${count} tasks`;
}

/** The question's title. */
export function stopAllTitle(asked: StopAllAsked): string {
  return `Stop ${asked.tasks.length === 1 ? "the 1 task" : `all ${asked.tasks.length} tasks`} of ${asked.name}?`;
}

/** What the answer does, said plainly, with its cost last. */
export function stopAllSays(asked: StopAllAsked): string {
  const { name } = asked;
  const count = asked.tasks.length;
  return `The ${tasksSaid(count)} at work below ${name} ${count === 1 ? "ends" : "end, deepest first"}. Each gets one short turn to say what it did, where it can be given one, and the chat that asked is told you stopped it. ${name} keeps running. There is no undo.`;
}

/** The button that does it. */
export function stopAllAnswer(asked: StopAllAsked): string {
  return `Stop ${tasksSaid(asked.tasks.length)}`;
}
