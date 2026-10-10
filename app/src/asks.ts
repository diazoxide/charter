/**
 * **The asks registry, as the window holds it** (#1690, spec #1688): everything that waits on
 * the person in each project, and how each is answered.
 *
 * The core derives a project's list from every source that waits today (`asks_waiting`): a
 * permission prompt held on a chat's hook, a dispatch held for a grant, a host a chat's sandbox
 * refused, a prompt in a harness's own terminal, a chat waiting on the person's reply. It keeps
 * no store, so the window reads the list again whenever one of those may have moved, and an
 * ask leaves it the moment its source stops waiting, wherever it was answered.
 *
 * **Answering goes through the path the ask names** ({@link answerThrough}): the command that
 * source's own Notice answers with, checked and audited as it is there. The registry adds no
 * route of its own. Every one of those commands is the window's alone.
 *
 * **What an ask says is data**: its chain of names, its line, a host. The window draws each as
 * text; none of it is ever markup or a control.
 */
import { useEffect, useState } from "react";
import { commands, type Asking, type GrantLevel, type Shown } from "./bindings";
import { listen } from "./here";
import { ASKS_CHANGED, answerAsk } from "./permissionAsks";

/**
 * The events after which a project's asks may read differently: its hooks' asks, a chat
 * moving on the board, a dispatch held for a grant, a block heard.
 */
export const ASKS_MAY_HAVE_MOVED = [
  ASKS_CHANGED,
  "chat-moved",
  "dispatch-grant-needed",
  "chat-sandbox-blocked",
] as const;

/** How long the window waits for a burst of moves to settle before it reads a list again. */
export const SETTLE_MS = 50;

/** Whatever reads the lists again when the window itself moved a source. */
const moved = new Set<(plane: string) => void>();

/**
 * **The window itself answered something `plane` waits on** (#1692): a Notice's Keep blocked,
 * an Allow in the Inbox, a dispatch answered on its tab. Some of those move the core with no
 * event of their own, so the window says so here and every list of `plane`'s asks is read
 * again: an answer in one place clears the ask everywhere it is drawn.
 */
export function asksMoved(plane: string): void {
  for (const reader of moved) reader(plane);
}

/** Each project's asks, by its id: the last whole list the core derived for it. */
export function useAsks(planes: readonly string[]): {
  held: Record<string, readonly Shown[]>;
  /** Reads `plane`'s list again: after an answer, whatever it answered. */
  reread: (plane: string) => void;
} {
  const [held, setHeld] = useState<Record<string, readonly Shown[]>>({});
  const [reader] = useState(() => listReader(setHeld));
  const key = planes.join("\n");
  useEffect(() => {
    const wanted = new Set(key === "" ? [] : key.split("\n"));
    reader.watch(wanted);
    for (const plane of wanted) reader.read(plane);
  }, [key, reader]);
  useEffect(() => {
    // Listening again after a cleanup (React's StrictMode runs every effect twice) reads what it
    // watches again, since a read made while it was stopped was dropped.
    reader.start();
    moved.add(reader.soon);
    const stops = ASKS_MAY_HAVE_MOVED.map((event) =>
      listen<{ plane: string }>(event, (said) => {
        const plane = said.payload?.plane;
        if (typeof plane === "string") reader.soon(plane);
      }).catch(() => undefined),
    );
    return () => {
      moved.delete(reader.soon);
      reader.stop();
      for (const stop of stops) void stop.then((off) => off?.()).catch(() => undefined);
    };
  }, [reader]);
  return { held, reread: reader.read };
}

/** Reads each watched project's list, at once or once a burst has settled. */
function listReader(
  setHeld: (
    change: (was: Record<string, readonly Shown[]>) => Record<string, readonly Shown[]>,
  ) => void,
) {
  let watched = new Set<string>();
  let live = true;
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  const read = (plane: string) => {
    if (!live || !watched.has(plane)) return;
    void commands.asksWaiting(plane).then(
      (answer) => {
        // A core that answers nothing for it (an older build, a test's mock) holds none.
        const asks = answer.status === "ok" ? (answer.data as Asking | null)?.asks : undefined;
        if (live && watched.has(plane) && asks) setHeld((was) => ({ ...was, [plane]: asks }));
      },
      () => {},
    );
  };
  return {
    read,
    start() {
      if (live) return;
      live = true;
      for (const plane of watched) read(plane);
    },
    soon(plane: string) {
      if (timers.has(plane)) return;
      timers.set(
        plane,
        setTimeout(() => {
          timers.delete(plane);
          read(plane);
        }, SETTLE_MS),
      );
    },
    watch(planes: Set<string>) {
      watched = planes;
      setHeld((was) => Object.fromEntries(Object.entries(was).filter(([one]) => planes.has(one))));
    },
    stop() {
      live = false;
      for (const timer of timers.values()) clearTimeout(timer);
      timers.clear();
    },
  };
}

const LEVELS: readonly GrantLevel[] = ["chat", "you", "project"];
const isLevel = (option: string): option is GrantLevel =>
  (LEVELS as readonly string[]).includes(option);

/** What is said when an ask names no path the window can send. */
export const IN_ITS_CHAT = "This is answered in its chat: go to it.";

/**
 * **Answers `ask` of `plane` with `option`, through the path it names**: nothing when it
 * applied, and the source's own sentence when it was refused. An option the ask does not
 * offer is refused here, before anything is sent.
 */
export async function answerThrough(
  plane: string,
  ask: Shown,
  option: string,
): Promise<string | undefined> {
  const path = ask.answer;
  if (path.via === "in-its-pane") return IN_ITS_CHAT;
  if (!ask.options.some((one) => one.id === option))
    return `${JSON.stringify(option)} is not one of the answers this ask offers.`;
  const sent = async (): Promise<{ status: "ok" } | { status: "error"; error: string }> => {
    switch (path.via) {
      case "hook": {
        const refused = await answerAsk(plane, ask.session, ask.ask, option);
        return refused === undefined ? { status: "ok" } : { status: "error", error: refused };
      }
      case "dispatch":
        if (option === "keep") return commands.keepDispatchBlocked(plane, path.id);
        if (option === "never") return commands.neverDispatch(plane, path.id);
        if (isLevel(option)) return commands.allowDispatch(plane, path.id, option, [], path.shown);
        break;
      case "sandbox-block":
        if (option === "keep") return commands.forgetSandboxBlock(plane, ask.session, path.shown);
        if (isLevel(option))
          return commands.allowSandboxBlock(plane, ask.session, path.shown, option);
        break;
    }
    return { status: "error", error: `${JSON.stringify(option)} is no answer purlis sends.` };
  };
  const answer = await sent().catch((err: unknown) => ({
    status: "error" as const,
    error: String(err),
  }));
  return answer.status === "ok" ? undefined : answer.error;
}
