/**
 * **What each chat's sandbox blocked, until the operator puts it away** (#1338).
 *
 * A chat's hook reads a block in what a command came back with, sorts it into an operation and
 * the kind of path or host, and the core sends `chat-sandbox-blocked`, which carries those words
 * and the sentence about them and nothing else: no path, argument, host or output. This keeps
 * them per chat, for the Notice on its tab. **In memory only**: the core kept its own count for
 * `purlis doctor`, and a relaunch starts with none to show.
 *
 * One block per operation and kind per chat — a command that failed on a hundred files is one
 * pattern — and at most {@link AT_MOST_PER_CHAT}, the oldest going first.
 */
import { useCallback, useEffect, useState } from "react";
import { listen } from "./here";
import type { ChatBlocked, PlaneId } from "./bindings";

/** The most blocks one chat holds at once. */
export const AT_MOST_PER_CHAT = 5;

/** Each chat's blocks, by session, newest last. */
export type Blocks = Readonly<Record<number, readonly ChatBlocked[]>>;

const same = (one: ChatBlocked, other: ChatBlocked) =>
  one.operation === other.operation && one.kind === other.kind && one.ours === other.ours;

/** `blocks` with `told` as its chat's newest, said once and within the bound. */
export function blocked(blocks: Blocks, told: ChatBlocked): Blocks {
  const mine = (blocks[told.session] ?? []).filter((one) => !same(one, told));
  return { ...blocks, [told.session]: [...mine, told].slice(-AT_MOST_PER_CHAT) };
}

/** `blocks` without chat `session`'s `block`. */
export function putAway(blocks: Blocks, session: number, block: ChatBlocked): Blocks {
  const left = (blocks[session] ?? []).filter((one) => !same(one, block));
  const others = Object.fromEntries(
    Object.entries(blocks).filter(([held]) => Number(held) !== session),
  );
  return left.length === 0 ? others : { ...others, [session]: left };
}

/** What `plane`'s chats' sandboxes blocked, and how one is put away. */
export function useSandboxBlocks(plane: PlaneId | undefined): {
  blocks: Blocks;
  dismiss: (session: number, block: ChatBlocked) => void;
} {
  // Held with the project it is about, so a window moved to another project shows none of the
  // last one's in the very render it moves.
  const [held, setHeld] = useState<{ plane?: PlaneId; blocks: Blocks }>({ blocks: NONE });
  const blocks = held.plane === plane ? held.blocks : NONE;

  useEffect(() => {
    if (plane === undefined) return;
    let gone = false;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unlisten = await listen<ChatBlocked>("chat-sandbox-blocked", (event) => {
          if (gone || event.payload.plane !== plane) return;
          setHeld((was) => ({
            plane,
            blocks: blocked(was.plane === plane ? was.blocks : NONE, event.payload),
          }));
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in: a unit test, or a webview being torn down.
      }
    })();
    return () => {
      gone = true;
      stop?.();
    };
  }, [plane]);

  const dismiss = useCallback(
    (session: number, block: ChatBlocked) =>
      setHeld((was) => ({ ...was, blocks: putAway(was.blocks, session, block) })),
    [],
  );
  return { blocks, dismiss };
}

/** No blocks. */
const NONE: Blocks = {};
