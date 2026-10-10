import type { HarnessRow } from "./bindings";

/**
 * **What a harness row says of itself**, once (#1719): the first run's "On this machine" list
 * and the harness set-up tab say a harness the same way, so the two cannot drift.
 */
export function harnessSays(row: HarnessRow): string {
  if (!row.installed) return "not installed";
  return row.signed_in ? "ready" : "installed; it asks you to sign in when its chat starts";
}
