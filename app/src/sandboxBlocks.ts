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
 *
 * **Several hosts refused at once are one Notice** (#1637). The core sends one block for each
 * host it named; a host that arrives while the chat holds a block of the same operation and kind
 * joins it ({@link HeldBlock.hosts}), so the person reads and answers every host in one place.
 * Each host is still answered, and held by the app, on its own ({@link eachHost}).
 */
import { useCallback, useEffect, useState } from "react";
import { listen } from "./here";
import type { ChatBlocked, PlaneId } from "./bindings";

/** The most blocks one chat holds at once. */
export const AT_MOST_PER_CHAT = 5;

/** The most hosts one block lists, the oldest going first: as many as one result yields. */
export const AT_MOST_HOSTS = 8;

/**
 * A block as the window holds it: the core's, and for a block that names its host, every host
 * refused under its operation and kind while it was up, oldest first. Its `target` is the first
 * of them. `joined` is when a host last joined it (ms since the epoch), held with the block so
 * the Notice's guard on a press just after a host joined survives the Notice being drawn again.
 */
export type HeldBlock = ChatBlocked & {
  readonly hosts?: readonly string[];
  readonly joined?: number;
};

/** Each chat's blocks, by session, newest last. */
export type Blocks = Readonly<Record<number, readonly HeldBlock[]>>;

/**
 * **What a block names, as a grant matches it**, the core's rule (`taskblocks::normalised`): a
 * host lowered, without a trailing dot and without the default port (`:443`, `:80`); a folder
 * as the core offered it.
 */
export function matched(offer: "host" | "write", target: string): string {
  if (offer === "write") return target;
  const host = target
    .trim()
    .toLowerCase()
    .replace(/:(443|80)$/, "");
  return host.endsWith(".") ? host.slice(0, -1) : host;
}

/** Whether `block` offers Allow on a host its report named: one several may join. */
const namesAHost = (block: ChatBlocked) =>
  block.offer === "host" && !block.ours && block.target !== null;

/**
 * **The key of the Notice drawing `block`**: one Notice for a block listing hosts however its
 * hosts change (the first can be dropped at the bound or answered), so what it holds (the
 * hosts allowed already, when one joined) is never lost while it is up (#1637).
 */
export function noticeKey(block: ChatBlocked): string {
  const who = block.ours ? "ours" : "chat";
  const what = namesAHost(block) ? "hosts" : `:${block.target ?? ""}`;
  return `${block.operation}:${block.kind}:${who}:${what}`;
}

const same = (one: ChatBlocked, other: ChatBlocked) =>
  one.operation === other.operation &&
  one.kind === other.kind &&
  one.ours === other.ours &&
  namesAHost(one) === namesAHost(other);

/** The hosts `block` lists, oldest first; none for a block that names no host. */
export function hostsOf(block: HeldBlock): readonly string[] {
  if (!namesAHost(block) || block.target === null) return [];
  return block.hosts ?? [block.target];
}

/** `block` listing `hosts`: the first is its target. */
function listing(block: HeldBlock, hosts: readonly string[]): HeldBlock {
  return { ...block, target: hosts[0] ?? null, hosts };
}

/**
 * **Each host of `block` as a block of its own**, as the core holds and answers it: one per
 * host it lists, or `block` itself for one that names no host.
 */
export function eachHost(block: HeldBlock): ChatBlocked[] {
  const hosts = hostsOf(block);
  if (hosts.length === 0) return [block];
  return hosts.map((host): ChatBlocked => {
    const one: HeldBlock = { ...block, target: host };
    delete (one as { hosts?: readonly string[] }).hosts;
    return one;
  });
}

/** `block` without the hosts `of` lists, or nothing once none is left. */
function withoutHostsOf(block: HeldBlock, of: readonly string[]): HeldBlock | undefined {
  const gone = new Set(of.map((host) => matched("host", host)));
  const hosts = hostsOf(block);
  const left = hosts.filter((host) => !gone.has(matched("host", host)));
  if (left.length === hosts.length) return block;
  return left.length === 0 ? undefined : listing(block, left);
}

/**
 * `blocks` with `told` as its chat's newest, said once and within the bound. A host joins the
 * block of its operation and kind that is up, which then offers Allow only at the levels every
 * host it lists may be allowed at (#1343).
 */
export function blocked(blocks: Blocks, told: ChatBlocked, now = Date.now()): Blocks {
  const held = blocks[told.session] ?? [];
  const was = held.find((one) => same(one, told));
  const mine = held.filter((one) => one !== was);
  let newest: HeldBlock = told;
  const joining = told.target;
  if (was !== undefined && namesAHost(told) && joining !== null) {
    const hosts = hostsOf(was);
    const known = hosts.some((host) => matched("host", host) === matched("host", joining));
    newest = {
      ...listing(told, known ? hosts : [...hosts, joining].slice(-AT_MOST_HOSTS)),
      levels: was.levels.filter((level) => told.levels.includes(level)),
      ...(known ? (was.joined === undefined ? {} : { joined: was.joined }) : { joined: now }),
    };
  }
  return { ...blocks, [told.session]: [...mine, newest].slice(-AT_MOST_PER_CHAT) };
}

/**
 * `blocks` without chat `session`'s `block`. `exactly`: only while the chat's block is still
 * the one answered, naming the same host or folder; a newer one that arrived meanwhile stays
 * up, unanswered (#1508). A block listing hosts loses only the hosts `block` lists, whether
 * answered or put away: a host that joined it after it was drawn stays up (#1637).
 */
export function putAway(
  blocks: Blocks,
  session: number,
  block: HeldBlock,
  exactly = false,
): Blocks {
  const left = (blocks[session] ?? []).flatMap((one): HeldBlock[] => {
    if (!same(one, block)) return [one];
    if (namesAHost(one)) {
      const kept = withoutHostsOf(one, hostsOf(block));
      return kept === undefined ? [] : [kept];
    }
    return exactly && one.target !== block.target ? [one] : [];
  });
  const others = Object.fromEntries(
    Object.entries(blocks).filter(([held]) => Number(held) !== session),
  );
  return left.length === 0 ? others : { ...others, [session]: left };
}

/** What `plane`'s chats' sandboxes blocked, and how one is put away. */
export function useSandboxBlocks(plane: PlaneId | undefined): {
  blocks: Blocks;
  dismiss: (session: number, block: HeldBlock) => void;
  /** Puts `block` away only while it is still the chat's block, naming the same target. */
  answered: (session: number, block: HeldBlock) => void;
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
    (session: number, block: HeldBlock) =>
      setHeld((was) => ({ ...was, blocks: putAway(was.blocks, session, block) })),
    [],
  );
  const answered = useCallback(
    (session: number, block: HeldBlock) =>
      setHeld((was) => ({ ...was, blocks: putAway(was.blocks, session, block, true) })),
    [],
  );
  return { blocks, dismiss, answered };
}

/** No blocks. */
const NONE: Blocks = {};
