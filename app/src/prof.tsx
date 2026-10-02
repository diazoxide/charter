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
