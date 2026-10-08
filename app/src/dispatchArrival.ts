import { useCallback, useSyncExternalStore } from "react";
import {
  commands,
  type DispatchAgain,
  type DispatchArrival,
  type DispatchArrived,
  type DispatchGone,
  type PlaneChanged,
  type PlaneId,
} from "./bindings";
import { listen } from "./here";
import { concerns, SIDEBAR } from "./planeChanged";

/**
 * **What the project's dispatch grants ask of the person now** (#1506), read once per project
 * for everything in the window that says something about it:
 *
 * - `ProjectDispatchNotice` says, at the window's level, that a teammate's grant arrived;
 * - `DispatchGrantNotice`, on the tab of a chat that needs one of those grants meanwhile, says
 *   the same thing in the same words ({@link arrivedSaid}).
 *
 * Two readers of one answer, so answering either clears both. The core holds what waits; this
 * reads it when the first reader mounts (the project opening), each time the core's watcher
 * says the project's file or its personas moved on disk (`plane-changed`: a pull, a branch
 * switched, a hand's edit), and each time the core says an answer was given anywhere
 * (`dispatch-arrival`: this window, another, Settings). A list that cannot be read is no list:
 * nothing is in force unasked either way.
 *
 * **An answer sends what was shown, never what is there now**: each grant's id, exactly as it
 * was listed. The core answers only what is still as it was shown and says so where the list
 * moved ({@link Arrival.said}).
 */
type Held = {
  arrival: DispatchArrival;
  /** What the last answer said, where the list had moved or the core refused. */
  said: string | undefined;
  readers: Set<() => void>;
  stops: (() => void)[];
  gone: boolean;
};

/** What a reader holds. */
export type Arrival = {
  waiting: readonly DispatchArrived[];
  gone: readonly DispatchGone[];
  /** Whether the project's git history could not be asked just now: no accepted project
   *  grant counts until it can, and nothing can be accepted. */
  unread: boolean;
  said: string | undefined;
};

const NOTHING: DispatchArrival = { waiting: [], gone: [], unread: false };
const held = new Map<string, Held>();
const snapshots = new WeakMap<Held, Arrival>();

function changed(mine: Held) {
  snapshots.delete(mine);
  for (const reader of mine.readers) reader();
}

function took(plane: PlaneId, arrival: DispatchArrival | null | undefined, said?: string) {
  const mine = held.get(plane);
  if (mine === undefined) return;
  // A core that answers nothing, or something that is not this, holds nothing.
  mine.arrival = {
    waiting: Array.isArray(arrival?.waiting) ? arrival.waiting : NOTHING.waiting,
    gone: Array.isArray(arrival?.gone) ? arrival.gone : NOTHING.gone,
    unread: arrival?.unread === true,
  };
  mine.said = said;
  changed(mine);
}

function read(plane: PlaneId) {
  void commands
    .dispatchArrival(plane)
    .then((answer) => {
      if (answer.status === "ok") took(plane, answer.data, held.get(plane)?.said);
    })
    // A project whose grants cannot be read says nothing here: none of them is in force.
    .catch(() => {});
}

function snapshot(plane: PlaneId): Arrival {
  const mine = held.get(plane);
  if (mine === undefined) return EMPTY;
  let made = snapshots.get(mine);
  if (made === undefined) {
    made = {
      waiting: mine.arrival.waiting,
      gone: mine.arrival.gone,
      unread: mine.arrival.unread,
      said: mine.said,
    };
    snapshots.set(mine, made);
  }
  return made;
}

const EMPTY: Arrival = {
  waiting: NOTHING.waiting,
  gone: NOTHING.gone,
  unread: false,
  said: undefined,
};

export function useDispatchArrival(plane: PlaneId): Arrival & {
  /** Accept (`true`) or Not on my machine, for the grants `shown` listed. */
  answer: (accepted: boolean, shown: readonly DispatchArrived[]) => Promise<void>;
  /** The person read that the project took `shown` away. */
  told: (shown: readonly DispatchGone[]) => void;
} {
  const subscribe = useCallback(
    (reader: () => void) => {
      let mine = held.get(plane);
      if (mine === undefined) {
        const made: Held = {
          arrival: NOTHING,
          said: undefined,
          readers: new Set(),
          stops: [],
          gone: false,
        };
        mine = made;
        held.set(plane, made);
        read(plane);
        const hear = (stop: () => void) => {
          if (made.gone) stop();
          else made.stops.push(stop);
        };
        void (async () => {
          try {
            hear(
              await listen<PlaneChanged>("plane-changed", (event) => {
                if (event.payload.plane !== plane) return;
                // The project's file and its personas both concern the sidebar: which grants
                // wait, and which of them name a persona that is not defined.
                if (concerns(event.payload.answers, SIDEBAR)) read(plane);
              }),
            );
            hear(
              await listen<{ plane: PlaneId }>("dispatch-arrival", (event) => {
                if (event.payload.plane === plane) read(plane);
              }),
            );
          } catch {
            // No window to listen in: a unit test, or a webview being torn down.
          }
        })();
      }
      const reading = mine;
      reading.readers.add(reader);
      return () => {
        reading.readers.delete(reader);
        if (reading.readers.size > 0) return;
        reading.gone = true;
        for (const stop of reading.stops) stop();
        if (held.get(plane) === reading) held.delete(plane);
      };
    },
    [plane],
  );
  const arrival = useSyncExternalStore(subscribe, () => snapshot(plane));

  const answer = useCallback(
    (accepted: boolean, shown: readonly DispatchArrived[]) =>
      commands
        .answerDispatchArrival(
          plane,
          accepted,
          shown.map((one) => one.id),
          // Everything listed, answered or not: what waits and was not listed arrived since.
          (held.get(plane)?.arrival.waiting ?? []).map((one) => one.id),
        )
        .then((done) => {
          if (done.status === "ok") took(plane, done.data?.arrival, done.data?.said ?? undefined);
          else {
            // Refused half way, some may be answered: say why, and read what waits now.
            const mine = held.get(plane);
            if (mine !== undefined) {
              mine.said = done.error;
              changed(mine);
            }
            read(plane);
          }
        })
        // Not recorded: it still waits, which is the safe way to be wrong.
        .catch(() => read(plane)),
    [plane],
  );
  const told = useCallback(
    (shown: readonly DispatchGone[]) =>
      void commands
        .dispatchGoneTold(
          plane,
          shown.map((one) => one.id),
        )
        .then((left) => {
          if (left.status === "ok") took(plane, left.data);
        })
        // Not recorded: it is told again next time.
        .catch(() => {}),
    [plane],
  );
  return { ...arrival, answer, told };
}

/** `names` as a sentence lists them: "a", "a and b", "a, b and c". */
export function listed(names: readonly string[]): string {
  if (names.length <= 1) return names.join("");
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** `items` by asking persona, in the order they were listed. */
function byAsking(items: readonly { asking: string; target: string }[]): [string, string[]][] {
  const out = new Map<string, string[]>();
  for (const one of items) out.set(one.asking, [...(out.get(one.asking) ?? []), one.target]);
  return [...out];
}

/** How many named pairs one Notice says each of, before it sums them ({@link arrivedSaid}). */
export const MOST_SAID = 6;

/** One grant as a sentence names it: "steward to devops", "steward to any persona". */
export function pairSaid(one: { asking: string; target: string; any: boolean }): string {
  return `${one.asking} to ${one.any ? "any persona" : one.target}`;
}

/** One grant as "The project lets …" goes on: "steward dispatch to devops". */
export function letsSaid(one: { asking: string; target: string; any: boolean }): string {
  return `${one.asking} dispatch to ${one.any ? "any persona" : one.target}`;
}

/** The key two causes share where they are one sentence. */
const causeOf = (again: DispatchAgain): string =>
  again.why === "persona" ? `persona ${again.name}` : again.why;

/**
 * **Why grants accepted before wait again, one true sentence per cause** (#1506):
 *
 * - a commit of the project's history took the grant out, and it is there again;
 * - purlis could not read the project's history since it was accepted, so it does not know;
 * - a persona it names was not in the project for a time, and one of that name is there now.
 *
 * Never one sentence for all three: only the first says anyone took anything out.
 */
export function againSaid(items: readonly DispatchArrived[]): string[] {
  const causes = new Map<string, DispatchArrived[]>();
  for (const one of items) {
    if (one.again === null) continue;
    const key = causeOf(one.again);
    causes.set(key, [...(causes.get(key) ?? []), one]);
  }
  return [...causes.values()].map((these) => {
    const again = these[0]?.again;
    const many = these.length > 1;
    const [it, waits] = many ? ["them", "they wait"] : ["it", "it waits"];
    const before = `You accepted ${listed(these.map(pairSaid))} before.`;
    if (again?.why === "takenOut")
      return `${before} The project took ${it} out of its settings and put ${it} back, so ${waits} for a new yes.`;
    if (again?.why === "persona")
      return `${before} The persona ${again.name} was not in this project for a time and one of that name is there now, so ${waits} for a new yes.`;
    return `${before} purlis could not read the project's history since then and cannot tell whether ${many ? "they were" : "it was"} there the whole time, so ${waits} for a new yes.`;
  });
}

/**
 * **What arrived, in the words both Notices use** (#1506). Each is one sentence, in this order:
 *
 * - every "any persona" grant, in its own plain words and never summed with anything, with
 *   where it is accepted: Settings, and no Notice (V100-23);
 * - the named pairs that can be used, by asking persona; more than {@link MOST_SAID} are
 *   summed, and the Notice shows each under its line;
 * - each grant that names a persona this project does not define, said as covering nothing;
 * - which of them the person accepted before, and why each waits again ({@link againSaid}).
 *
 * Names are the core's, drawn as text.
 */
export function arrivedSaid(items: readonly DispatchArrived[]): string[] {
  const usable = items.filter((one) => one.undefined === null);
  const out: string[] = [];
  for (const one of usable.filter((one) => one.any))
    out.push(
      `The project now lets ${one.asking} dispatch to any persona: every persona of this project, including ones added later. It is not in force on this machine, and is accepted only in Settings \u203a Project \u203a Dispatch.`,
    );
  const pairs = usable.filter((one) => !one.any);
  if (pairs.length > MOST_SAID) {
    const groups = byAsking(pairs);
    out.push(
      `The project now lets ${groups.length === 1 ? (groups[0]?.[0] ?? "") : `${groups.length} personas`} dispatch to others: ${pairs.length} pairs in all, each listed below.`,
    );
  } else if (pairs.length > 0) {
    out.push(
      `The project now lets ${byAsking(pairs)
        .map(([asking, targets]) => `${asking} dispatch to ${listed(targets)}`)
        .join("; ")}.`,
    );
  }
  for (const one of items.filter((one) => one.undefined !== null))
    out.push(
      `The project's settings also let ${letsSaid(one)}, but this project does not define a persona named ${one.undefined ?? ""}, so that covers nothing and is not accepted.`,
    );
  out.push(...againSaid(usable));
  return out;
}
