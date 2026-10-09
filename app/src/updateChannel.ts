import { useSyncExternalStore } from "react";
import { commands } from "./bindings";

/**
 * **The update channel, one value for the window** (#1240): the title bar's updater
 * (`Updates.tsx`) and Settings › You › This machine both read it here, and both say a move here
 * once the core has written it, so a change made in one is shown by the other at once rather
 * than after its next read.
 *
 * Read from the core when the first reader comes, and again whenever {@link readChannel} asks
 * (a window coming back into focus, where something else may have moved it). `undefined` until
 * the core has answered.
 */

let channel: string | undefined;
const listeners = new Set<() => void>();
/** The newest read out: an answer to an older one is dropped. */
let reading = 0;

function set(to: string | undefined) {
  if (to === channel) return;
  channel = to;
  for (const one of listeners) one();
}

/** Asks the core which channel this machine is on, and says the answer to every reader. */
export function readChannel(): void {
  const mine = ++reading;
  void commands
    .updateChannel()
    .then((now) => {
      if (mine === reading && typeof now === "string") set(now);
    })
    .catch(() => undefined);
}

/** The core has written `to` as this machine's channel: every reader shows it now. */
export function channelMoved(to: string): void {
  // A read already out may answer with the channel from before the write.
  reading += 1;
  set(to);
}

function subscribe(listener: () => void) {
  if (listeners.size === 0) readChannel();
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The channel this machine is on, once the core has said. */
export function useUpdateChannel(): string | undefined {
  return useSyncExternalStore(subscribe, () => channel);
}
