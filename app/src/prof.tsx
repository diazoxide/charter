// LOCAL PROFILING ONLY (FR-27b). Not committed.
import { Profiler, type ReactNode } from "react";

type Row = { id: string; phase: string; actual: number; start: number; commit: number };
const rows: Row[] = [];
const marks: Record<string, number> = {};
declare global {
  interface Window {
    __prof?: { rows: Row[]; marks: Record<string, number>; clear(): void };
  }
}
window.__prof = {
  rows,
  marks,
  clear() {
    armed = true;
    setTimeout(() => (armed = false), 2000);
    rows.length = 0;
    for (const k of Object.keys(marks)) delete marks[k];
  },
};

let armed = false;
export function arm(on: boolean): void {
  armed = on;
}
export function note(name: string): void {
  if (!armed) return;
  const at = Math.round(performance.now());
  let k = name;
  let n = 1;
  while (k in marks) k = `${name}#${++n}`;
  marks[k] = at;
}

// Every command the window sends while armed: its name, when it went, how long it took.
export function traceCommands(all: Record<string, unknown>): void {
  for (const [name, fn] of Object.entries(all)) {
    if (typeof fn !== "function") continue;
    all[name] = (...a: unknown[]) => {
      const went = performance.now();
      const back = (fn as (...b: unknown[]) => unknown)(...a);
      if (armed && back instanceof Promise) {
        note(`ipc:${name}>`);
        back.then(
          () => note(`ipc:${name}=${Math.round(performance.now() - went)}ms`),
          () => undefined,
        );
      }
      return back;
    };
  }
}

export function mark(name: string): void {
  marks[name] ??= performance.now();
}

export function P({ id, children }: { id: string; children: ReactNode }) {
  return (
    <Profiler
      id={id}
      onRender={(id, phase, actual, _base, start, commit) => {
        if (armed) rows.push({ id, phase, actual, start, commit });
      }}
    >
      {children}
    </Profiler>
  );
}
