// THROWAWAY PROFILING (#891). Not for merge.
export type Sent = { c: string; went: number; back?: number };
export const sent: Sent[] = [];
export function traceCommands(all: Record<string, unknown>): void {
  for (const [name, fn] of Object.entries(all)) {
    if (typeof fn !== "function") continue;
    all[name] = (...a: unknown[]) => {
      const one: Sent = { c: name, went: performance.timeOrigin + performance.now() };
      const back = (fn as (...b: unknown[]) => unknown)(...a);
      if (back instanceof Promise) {
        sent.push(one);
        if (sent.length > 400) sent.splice(0, 200);
        back.then(
          () => (one.back = performance.timeOrigin + performance.now()),
          () => undefined,
        );
      }
      return back;
    };
  }
}

const epoch = () => performance.timeOrigin + performance.now();
export const prof = {
  armed: false,
  marks: {} as Record<string, number>,
  rows: [] as { id: string; phase: string; actual: number; start: number; commit: number }[],
  lags: [] as { at: number; ms: number }[],
};
export function mark(name: string): void {
  if (!prof.armed) return;
  let k = name;
  let n = 1;
  while (k in prof.marks) k = `${name}#${++n}`;
  prof.marks[k] = epoch();
}
export function arm(): void {
  prof.armed = true;
  prof.marks = {};
  prof.rows = [];
  prof.lags = [];
  let last = epoch();
  const tick = () => {
    if (!prof.armed) return;
    const now = epoch();
    if (now - last > 6) prof.lags.push({ at: last, ms: now - last });
    last = now;
    setTimeout(tick, 0);
  };
  setTimeout(tick, 0);
  // A higher-priority probe than timers: MessageChannel tasks, which is what React schedules on.
  const ch = new MessageChannel();
  let mlast = epoch();
  ch.port1.onmessage = () => {
    if (!prof.armed) return;
    const now = epoch();
    if (now - mlast > 6) prof.lags.push({ at: mlast, ms: -(now - mlast) });
    mlast = now;
    ch.port2.postMessage(0);
  };
  ch.port2.postMessage(0);
  const frame = () => {
    if (!prof.armed) return;
    mark("raf");
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}
export function disarm(): void {
  prof.armed = false;
}
export function onRender(
  id: string,
  phase: string,
  actual: number,
  _base: number,
  start: number,
  commit: number,
): void {
  if (prof.armed)
    prof.rows.push({
      id,
      phase,
      actual,
      start: performance.timeOrigin + start,
      commit: performance.timeOrigin + commit,
    });
}
