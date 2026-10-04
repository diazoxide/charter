/**
 * Which files the chats are touching right now (FM-6, #1109; V86 F6).
 *
 * A chat's file tool — a read, an edit, a write — names its path to the app, which confines it
 * to the chat's own folder, rates it, and sends `chat-touching`. This keeps each one for a few
 * seconds and lets it fade once the chat has been quiet on it for {@link FADE_MS}, so the tree
 * shows the present and not a history. **In memory only**: nothing here is saved, and the core
 * wrote it nowhere either (D-86a).
 *
 * Bounded twice, so a chat that touches files as fast as it can costs the window a fixed amount:
 * at most {@link AT_MOST_PER_CHAT} paths per chat (its oldest goes first), and at most
 * {@link AT_MOST} in all. The core already caps each chat's rate.
 */
import { useEffect, useState } from "react";
import { listen } from "./here";
import type { ChatTouching, OpenChat, PlaneId } from "./bindings";

/** How long a touched file stays marked after its chat last touched it. */
export const FADE_MS = 4000;

/** The most paths one chat has marked at once. */
export const AT_MOST_PER_CHAT = 8;

/** The most paths marked at once, across every chat. */
export const AT_MOST = 256;

/** One file a chat touched, and when this window last heard so, in ms. */
export type Touch = { readonly session: number; readonly path: string; readonly at: number };

/** Every file being touched, oldest first. */
export type Touches = readonly Touch[];

/** `touches` with chat `session`'s touch of `path` at `now` as its newest, within the bounds. */
export function touched(touches: Touches, session: number, path: string, now: number): Touches {
  const others = touches.filter((one) => one.session !== session || one.path !== path);
  let next = [...others, { session, path, at: now }];
  const mine = next.filter((one) => one.session === session);
  if (mine.length > AT_MOST_PER_CHAT) {
    const dropped = new Set(mine.slice(0, mine.length - AT_MOST_PER_CHAT));
    next = next.filter((one) => !dropped.has(one));
  }
  return next.length > AT_MOST ? next.slice(next.length - AT_MOST) : next;
}

/** `touches` without what has faded by `now`: the same list when nothing has. */
export function faded(touches: Touches, now: number): Touches {
  const kept = touches.filter((one) => now - one.at < FADE_MS);
  return kept.length === touches.length ? touches : kept;
}

/** When the next touch fades, or nothing when none is held. */
export function nextFade(touches: Touches): number | undefined {
  if (touches.length === 0) return undefined;
  return Math.min(...touches.map((one) => one.at)) + FADE_MS;
}

/** The files `plane`'s chats are touching, fading as they go quiet. */
export function useTouching(plane: PlaneId | undefined): Touches {
  // Held with the project it is about, so a window moved to another project draws none of the
  // last one's in the very render it moves.
  const [held, setHeld] = useState<{ plane?: PlaneId; touches: Touches }>({ touches: [] });
  const touches = held.plane === plane ? held.touches : NONE;

  useEffect(() => {
    if (plane === undefined) return;
    let gone = false;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unlisten = await listen<ChatTouching>("chat-touching", (event) => {
          if (gone || event.payload.plane !== plane) return;
          const { session, path } = event.payload;
          setHeld((was) => {
            const now = Date.now();
            const from = was.plane === plane ? was.touches : NONE;
            return { plane, touches: touched(faded(from, now), session, path, now) };
          });
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

  // One timer, for the next touch to fade; it is set again whenever the list changes.
  const due = nextFade(touches);
  useEffect(() => {
    if (due === undefined) return;
    const timer = setTimeout(
      () => setHeld((was) => ({ ...was, touches: faded(was.touches, Date.now()) })),
      Math.max(0, due - Date.now()),
    );
    return () => clearTimeout(timer);
  }, [due]);

  return touches;
}

/** No files touched. */
const NONE: Touches = [];

/** The chats touching each path of one branch, by the path inside it. */
export type Touching = ReadonlyMap<string, readonly string[]>;

/** A path with `/` between its parts, whichever the platform wrote, and no `/` at its end. */
function slashed(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "");
}

/**
 * Which paths of the branch whose folder is `folder` are being touched, and by which chats, by
 * name: each file touched, each folder above it, and `""` for the branch's own folder.
 *
 * A touch's path is inside its chat's folder (the core confined it), so it is placed under the
 * branch by that folder: a chat working in a branch, or in a folder holding it, marks the files
 * of it it touches, and no other.
 */
export function touchingIn(
  touches: Touches,
  chats: readonly OpenChat[],
  folder: string | undefined,
): Touching {
  const out = new Map<string, string[]>();
  if (folder === undefined || touches.length === 0) return out;
  const top = slashed(folder);
  const bySession = new Map(chats.map((chat) => [chat.session, chat]));
  for (const touch of touches) {
    const chat = bySession.get(touch.session);
    if (chat?.cwd == null) continue;
    const whole = `${slashed(chat.cwd)}/${touch.path}`;
    if (!whole.startsWith(`${top}/`)) continue;
    const inside = whole.slice(top.length + 1);
    const parts = inside.split("/");
    const places = parts.map((_, at) => parts.slice(0, at + 1).join("/"));
    for (const place of ["", ...places]) {
      const names = out.get(place) ?? [];
      if (!names.includes(chat.name)) names.push(chat.name);
      out.set(place, names);
    }
  }
  return out;
}

/** What a marker says on hover and to a screen reader. */
export function touchSaid(names: readonly string[]): string {
  const who =
    names.length === 1
      ? names[0]
      : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
  return `${who} ${names.length === 1 ? "is" : "are"} working here now`;
}
