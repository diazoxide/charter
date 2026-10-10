import { useEffect, useRef } from "react";

/**
 * **One scheduler for the window's timed reads, paused while the window is hidden** (#1392,
 * SC-18's window half).
 *
 * Some of what the window draws is read again on a beat, because nothing tells the window it
 * changed: the save standing (`saving.ts`), a running chat's gauge (`ChatGauge.tsx`), a running
 * dispatch (`DispatchesTab.tsx`), the minutes a row has spent in its state (`stateClock.ts`).
 * Each read is cheap, but each is a wake-up, and a window nobody can see has no reason to wake.
 *
 * So every beat runs here:
 * - **one timer per interval**, shared by every read on that interval, and cleared with the last
 *   read that stops;
 * - **no beat while the window is hidden** (`document.hidden`: minimised, on another desktop,
 *   behind a full-screen app, as the engine says);
 * - **one read the moment it is shown again**, for every read on the books, so nothing the
 *   person should see waits a whole interval: a paused beat never loses a change, it is only
 *   late until the window can be seen, and then it is not late at all. The beat starts afresh
 *   from that read.
 *
 * What it does not do: decide when anything else reads. A read on focus, on a file event or on
 * a finished save stays with its caller; this only owns the beat.
 */

type Read = () => void;
type Beat = { readers: Set<{ read: Read }>; timer: ReturnType<typeof setInterval> | undefined };

const beats = new Map<number, Beat>();

const hidden = (): boolean => typeof document !== "undefined" && document.hidden;

/** Runs each read, so one that throws does not cost the others theirs; its error is thrown
 *  again outside the beat, where the window reports any other. */
function runAll(beat: Beat): void {
  for (const one of [...beat.readers]) {
    if (!beat.readers.has(one)) continue;
    try {
      one.read();
    } catch (err) {
      queueMicrotask(() => {
        throw err;
      });
    }
  }
}

function start(ms: number, beat: Beat): void {
  if (beat.timer !== undefined || hidden()) return;
  beat.timer = setInterval(() => {
    // Belt and braces: an engine that hid the window without saying so still reads nothing.
    if (!hidden()) runAll(beat);
  }, ms);
}

function halt(beat: Beat): void {
  if (beat.timer === undefined) return;
  clearInterval(beat.timer);
  beat.timer = undefined;
}

function onVisibility(): void {
  if (hidden()) {
    for (const beat of beats.values()) halt(beat);
    return;
  }
  for (const [ms, beat] of [...beats]) {
    // Already beating: this was not a return from hidden (an engine can say "visible" twice).
    if (beat.timer !== undefined) continue;
    runAll(beat);
    if (beats.get(ms) === beat && beat.readers.size > 0) start(ms, beat);
  }
}

/**
 * Calls `read` every `ms` while the window is shown, and once as soon as it is shown again after
 * being hidden. Returns the stop; stopping twice is stopping once. `read` is not called now:
 * the caller reads at once if it wants a first answer.
 */
export function everyWhileShown(ms: number, read: Read): () => void {
  if (beats.size === 0 && typeof document !== "undefined")
    document.addEventListener("visibilitychange", onVisibility);
  let beat = beats.get(ms);
  if (beat === undefined) {
    beat = { readers: new Set(), timer: undefined };
    beats.set(ms, beat);
  }
  const reader = { read };
  beat.readers.add(reader);
  start(ms, beat);
  const held = beat;
  return () => {
    if (!held.readers.delete(reader) || held.readers.size > 0) return;
    halt(held);
    beats.delete(ms);
    if (beats.size === 0 && typeof document !== "undefined")
      document.removeEventListener("visibilitychange", onVisibility);
  };
}

/**
 * {@link everyWhileShown} for a component: `read` on every beat of `ms` while `on` and mounted.
 * The latest `read` is the one called, so a caller need not keep it stable.
 */
export function useWhileShown(ms: number, read: Read, on = true): void {
  const latest = useRef(read);
  useEffect(() => {
    latest.current = read;
  });
  useEffect(() => {
    if (!on) return;
    return everyWhileShown(ms, () => latest.current());
  }, [ms, on]);
}
