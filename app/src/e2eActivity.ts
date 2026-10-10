/// <reference types="vite/client" />
import { useEffect, useRef } from "react";
import type { Activity } from "./bindings";

/**
 * **A timeline the scenario specs pretend an Activity tab read** (#1457), so how the tab draws
 * one can be measured in the real WebView: a task's lines set in under the chat that asked, a
 * clipped line opening in full, and the "also names" mark.
 *
 * A timeline is read from dispatch records, and a record needs a persona chat and a core that
 * dispatched a task, which the specs' fake harness cannot make (`e2eTasks.ts` says the same of
 * a task). So the spec hands the open tab the core's answer, and from there every line is drawn
 * by the tab itself.
 *
 * **It exists only in the `e2e` build**: `VITE_E2E` is set by `vite build --mode e2e` alone, and
 * in every other build this hears nothing.
 */
export const PRETEND_ACTIVITY = "purlis-e2e-activity";

/** Hands `onRead` each timeline a spec pretends, while the tab calling it is drawn. */
export function usePretendActivity(onRead: (read: Activity) => void): void {
  const told = useRef(onRead);
  useEffect(() => {
    told.current = onRead;
  }, [onRead]);
  useEffect(() => {
    if (!import.meta.env.VITE_E2E) return;
    const hear = (event: Event) => {
      const said: unknown = (event as CustomEvent<unknown>).detail;
      if (
        typeof said === "object" &&
        said !== null &&
        Array.isArray((said as { lines?: unknown }).lines)
      )
        told.current(said as Activity);
    };
    window.addEventListener(PRETEND_ACTIVITY, hear);
    return () => window.removeEventListener(PRETEND_ACTIVITY, hear);
  }, []);
}
