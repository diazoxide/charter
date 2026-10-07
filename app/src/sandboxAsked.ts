import { useSyncExternalStore } from "react";

/**
 * **The window changed what a chat's sandbox is compiled from** (#1428, D-1428-10): one of its
 * own sandbox commands returned, such as a folder every chat may write being revoked in
 * Settings' Granted list, a folder listed or unlisted, a host added, removed or confirmed, a
 * block allowed past this chat, or the sandbox's offer answered.
 *
 * What those write is kept in the project's state folder, which the watcher of the project's
 * root does not report. So the window says so itself, and whatever reads the chats left on an
 * older sandbox (`useOlderSandbox`) asks again. A count and not an event: a reader that mounts
 * later still sees that it moved.
 */
let asked = 0;
const listeners = new Set<() => void>();

/** One of the window's sandbox commands returned: anything that follows it reads again. */
export function sandboxCommandReturned(): void {
  asked += 1;
  for (const listener of [...listeners]) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

/** How many of the window's sandbox commands have returned: it moves when one does. */
export function useSandboxCommands(): number {
  return useSyncExternalStore(
    subscribe,
    () => asked,
    () => asked,
  );
}
