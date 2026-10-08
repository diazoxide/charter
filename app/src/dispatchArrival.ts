import { useCallback, useSyncExternalStore } from "react";
import {
  commands,
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
  said: string | undefined;
};

const NOTHING: DispatchArrival = { waiting: [], gone: [] };
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
    made = { waiting: mine.arrival.waiting, gone: mine.arrival.gone, said: mine.said };
    snapshots.set(mine, made);
  }
  return made;
}

const EMPTY: Arrival = { waiting: NOTHING.waiting, gone: NOTHING.gone, said: undefined };

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

/**
 * **What arrived, in the words both Notices use** (#1506). Each is one sentence, in this order:
 *
 * - every "any persona" grant, in its own plain words and never summed with anything;
 * - the named pairs that can be used, by asking persona; more than {@link MOST_SAID} are
 *   summed, and the Notice shows each under its line;
 * - each grant that names a persona this project does not define, said as covering nothing;
 * - which of them the person accepted before, and why they are asked again.
 *
 * Names are the core's, drawn as text.
 */
export function arrivedSaid(items: readonly DispatchArrived[]): string[] {
  const usable = items.filter((one) => one.undefined === null);
  const out: string[] = [];
  for (const one of usable.filter((one) => one.any))
    out.push(
      `The project now lets ${one.asking} dispatch to any persona: every persona of this project, including ones added later.`,
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
      `The project's settings also let ${one.asking} dispatch to ${one.any ? "any persona" : one.target}, but this project does not define a persona named ${one.undefined ?? ""}, so that covers nothing and is not accepted.`,
    );
  const again = usable.filter((one) => one.again);
  if (again.length > 0)
    out.push(
      `You accepted ${listed(again.map(pairSaid))} before. The project's settings were without ${again.length === 1 ? "it" : "them"} for a time since, so ${again.length === 1 ? "it waits" : "they wait"} for your yes again.`,
    );
  return out;
}

/** One grant as a sentence names it: "steward to devops", "steward to any persona". */
export function pairSaid(one: { asking: string; target: string; any: boolean }): string {
  return `${one.asking} to ${one.any ? "any persona" : one.target}`;
}
