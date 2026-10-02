// Removing the bench's throwaway profile (#954).
//
// After the app exits, a helper it started (WebKitGTK's network or web process) can still be
// writing into the profile for a moment, so one recursive remove can meet ENOTEMPTY. Cleanup is
// housekeeping, never part of a measurement: it retries while something is still writing, and a
// directory it still cannot remove is reported and left behind rather than failing the gate.
import { rmSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";

/**
 * Remove `dir` and everything in it. Resolves true once it is gone, false if it could not be
 * removed after `retries` tries `delayMs` apart (`warn` is told why). Never throws. `remove` is
 * the remover, swapped only by tests.
 */
export async function removeProfile(
  dir,
  { retries = 20, delayMs = 100, warn = console.warn, remove = rmSync } = {},
) {
  let last;
  for (let attempt = 0; attempt <= retries; attempt++) {
    try {
      remove(dir, { recursive: true, force: true });
      return true;
    } catch (error) {
      last = error;
      await sleep(delayMs);
    }
  }
  warn(`charter-bench: could not remove the throwaway profile ${dir} (${last?.code ?? last}); left behind`);
  return false;
}
