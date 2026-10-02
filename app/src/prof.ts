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
