import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { Notice } from "./Notice";
import type { Doing, Ran } from "./actions";
import { commands, type MemoryScope, type MemoryView, type PlaneId } from "./bindings";
import { settled } from "./PlaneEdits";
import { NO_TARGETS, type MemoryTargets } from "./memoryMoves";
import {
  DRAFT,
  DRAFT_TITLE,
  draftOf,
  draftView,
  memoryKey,
  memoryOf,
  memoryRefOf,
  memoryView,
  scopeKey,
  setDraft,
  wantEdit,
  type MemoryRef,
} from "./memories";
import {
  contentsOf,
  keepView,
  openPreview,
  openView,
  showInstead,
  type Tabs,
  type ViewRef,
} from "./tabs";

/** The verbs of `Doing` this hook carries out (SI-9b). */
export type MemoryEditing = Pick<
  Doing,
  "openMemory" | "editMemory" | "archiveMemory" | "moveMemory" | "newMemory" | "keepTab"
>;

/** What the Undo line can take back: a Delete (from the archive, under its own slug) or a Move
 *  (from the store it went to, back to the one it came from). */
type Undoable = { title: string; trouble?: string } & (
  | { kind: "deleted"; ref: MemoryRef; archived: string }
  | { kind: "moved"; from: MemoryRef; at: MemoryRef }
);

/** How long a Delete's or a Move's Undo is offered (ADR 0065 Q8: "a few seconds"). */
export const UNDO_MS = 8_000;

/**
 * **A plane's memories, from the window** (SI-9b, ADR 0065): opening one in the strip's preview
 * tab or a kept one, starting an edit, making one, and Delete and Move, each with its Undo.
 *
 * A hook of its own, as `PlaneEdits` is, because none of it is the window's arrangement. What it
 * needs from the window is how to put a tab on the strip in front (`present`), how to change
 * the tabs where they are (`update`), how to close a view's tab, and how to have the plane
 * read again; it hands back the verbs, the counter the memory lists and tabs re-read on, what a
 * memory's tab calls when it saved, and the Undo line to draw.
 *
 * **Every write is the core's** (`memories.rs`), and every answer ends in the lists reading the
 * disk again (`changed`), so they say what is there and not what this hook believes it did.
 */
export function useMemoryEdits({
  plane,
  present,
  update,
  closeView,
  reread,
}: {
  plane: PlaneId;
  /** Put a tab on the strip in front, or bring one forward: `open` is given the tabs and that
   *  strip, and the window follows the tab it answers in front. */
  present: (open: (tabs: Tabs, strip: string) => Tabs) => void;
  /** Change the tabs where they are. */
  update: (change: (tabs: Tabs) => Tabs) => void;
  /** Close the tab showing that view, where it shows nothing else. */
  closeView: (view: ViewRef) => void;
  /** Read the focused workspace's panels again: a persona's count of memories is on its row. */
  reread: () => void;
}): {
  doing: MemoryEditing;
  /** Bumped on every memory write, so the memory lists and tabs read again. */
  changed: number;
  onSaved: (from: ViewRef, memory: MemoryView) => void;
  /** The Undo line, while a Delete or a Move can still be undone. */
  undo: ReactNode;
} {
  const [changed, setChanged] = useState(0);
  const [undoing, setUndoing] = useState<Undoable>();
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  /** The line whose Undo is on its way: a second press on it sends nothing (#1190). */
  const sending = useRef<Undoable>(undefined);
  /** The same, for the line to draw: its Undo is disabled while it is on its way (#1190). */
  const [sent, setSent] = useState<Undoable>();

  const wrote = useCallback(() => {
    setChanged((was) => was + 1);
    reread();
  }, [reread]);

  useEffect(() => () => clearTimeout(timer.current), []);

  /** Offers `what`'s Undo for {@link UNDO_MS}, in place of any Undo offered before it. */
  const offerUndo = useCallback((what: Undoable) => {
    clearTimeout(timer.current);
    setUndoing(what);
    timer.current = setTimeout(() => setUndoing(undefined), UNDO_MS);
  }, []);

  const openMemory = useCallback(
    (ref: MemoryRef, title: string, keep: boolean) => {
      const view = memoryView(ref);
      present((tabs, strip) =>
        keep
          ? keepView(openView(tabs, view, title, strip), view)
          : openPreview(tabs, view, title, strip),
      );
    },
    [present],
  );

  /** Starting an edit keeps the tab (ADR 0065 Q1), and asks it for its editor. */
  const editMemory = useCallback(
    (ref: MemoryRef, title: string) => {
      const view = memoryView(ref);
      present((tabs, strip) => keepView(openView(tabs, view, title, strip), view));
      wantEdit(plane, memoryKey(ref));
    },
    [plane, present],
  );

  const newMemory = useCallback(
    (scope: MemoryScope) =>
      present((tabs, strip) => openView(tabs, draftView(scope), DRAFT_TITLE, strip)),
    [present],
  );

  const keepTab = useCallback(
    (tab: number) =>
      update((tabs) => {
        const lead = contentsOf(tabs, tab)[0]?.content;
        return lead?.kind === "view" ? keepView(tabs, lead.view) : tabs;
      }),
    [update],
  );

  /** Delete: archived, its tab closed, and an Undo offered for {@link UNDO_MS}. No question
   *  first — nothing is lost (ADR 0065 Q8). */
  const archiveMemory = useCallback(
    async (ref: MemoryRef, title: string): Promise<Ran> => {
      const answer = await settled(commands.memoryArchive(plane, ref.scope, ref.slug));
      if (answer.status === "error") return { ok: false, refused: answer.error };
      setDraft(plane, memoryKey(ref), undefined);
      closeView(memoryView(ref));
      wrote();
      offerUndo({ kind: "deleted", ref, title, archived: answer.data.archived });
      return { ok: true };
    },
    [closeView, offerUndo, plane, wrote],
  );

  /** A memory's tab now shows `to`, under its title. */
  const follow = useCallback(
    (from: ViewRef, to: MemoryView) =>
      update((tabs) =>
        showInstead(tabs, from, memoryView({ scope: to.scope, slug: to.slug }), to.title),
      ),
    [update],
  );

  /**
   * **Move, from a row's menu or the palette** (#1190): the tab's own Move — the same core move,
   * whole, its tab (where one is open) following it, and the same Undo the tab's offers. An edit
   * in progress is refused rather than moved from under: the tab hides its Move while it edits,
   * and a move would leave what was typed behind on a name that is gone.
   */
  const moveMemory = useCallback(
    async (ref: MemoryRef, title: string, to: MemoryScope): Promise<Ran> => {
      if (typeof draftOf(plane, memoryKey(ref)) === "object")
        return {
          ok: false,
          refused: `“${title}” is being edited: save or cancel the edit, then move it.`,
        };
      const answer = await settled(commands.memoryMove(plane, ref.scope, ref.slug, to, null));
      if (answer.status === "error") return { ok: false, refused: answer.error };
      follow(memoryView(ref), answer.data);
      wrote();
      offerUndo({
        kind: "moved",
        from: ref,
        at: { scope: answer.data.scope, slug: answer.data.slug },
        title: answer.data.title,
      });
      return { ok: true };
    },
    [follow, offerUndo, plane, wrote],
  );

  const undo = useCallback(async () => {
    // A second press while this line's Undo is on its way sends nothing: the core would refuse
    // it, the memory having moved already.
    if (undoing === undefined || sending.current === undoing) return;
    clearTimeout(timer.current);
    // What this press undoes. An act made while it was on its way has its own line by the time
    // it lands, and neither its refusal nor its end may say anything over that line.
    const pressed = undoing;
    sending.current = pressed;
    setSent(pressed);
    try {
      const refused = (why: string) =>
        setUndoing((now) => (now === pressed ? { ...now, trouble: why } : now));
      if (pressed.kind === "deleted") {
        const { ref, archived } = pressed;
        // Back under its own slug, which archiving may have had to number (`restore_as`).
        const answer = await settled(
          commands.memoryUnarchive(plane, ref.scope, archived, ref.slug),
        );
        if (answer.status === "error") {
          refused(answer.error);
          return;
        }
      } else {
        // Moved back the way it came, whole, by the same core move, and under the exact name it
        // had (`restore_as`): a journal's name to the second, not only to its stamp's minute. A
        // memory written under that name in the old store since is refused there, and said here.
        const { from, at } = pressed;
        const answer = await settled(
          commands.memoryMove(plane, at.scope, at.slug, from.scope, from.slug),
        );
        if (answer.status === "error") {
          refused(answer.error);
          return;
        }
        follow(memoryView(at), answer.data);
      }
      setUndoing((now) => (now === pressed ? undefined : now));
      wrote();
    } finally {
      // Cleared on every way out, so a refused Undo can be pressed again.
      if (sending.current === pressed) sending.current = undefined;
      setSent((now) => (now === pressed ? undefined : now));
    }
  }, [follow, plane, undoing, wrote]);

  /**
   * A memory's tab wrote it: a save, a create or a Move. **A Move is the one that changed its
   * store** — a save keeps the memory where it is, and a create comes from a draft — so that is
   * how it is told apart, and it alone offers Undo (#1190), as Delete does.
   */
  const onSaved = useCallback(
    (from: ViewRef, memory: MemoryView) => {
      follow(from, memory);
      wrote();
      const was = memoryRefOf(from.key);
      if (was !== undefined && was.slug !== DRAFT && scopeKey(was.scope) !== scopeKey(memory.scope))
        offerUndo({
          kind: "moved",
          from: was,
          at: { scope: memory.scope, slug: memory.slug },
          title: memory.title,
        });
    },
    [follow, offerUndo, wrote],
  );

  const doing = useMemo<MemoryEditing>(
    () => ({ openMemory, editMemory, archiveMemory, moveMemory, newMemory, keepTab }),
    [openMemory, editMemory, archiveMemory, moveMemory, newMemory, keepTab],
  );

  const line =
    undoing === undefined ? null : (
      <Notice
        cause={undoing.kind === "deleted" ? "memory-deleted" : "memory-moved"}
        tone={undoing.trouble === undefined ? "news" : "trouble"}
        fixes={[{ label: "Undo", onPress: () => void undo(), busy: sent === undoing }]}
      >
        {undoing.trouble ??
          (undoing.kind === "deleted"
            ? `Deleted “${undoing.title}” — it is in the archive now, out of every list.`
            : `Moved “${undoing.title}” to ${memoryOf(undoing.at.scope)}.`)}
      </Notice>
    );

  return { doing, changed, onSaved, undo: line };
}

/**
 * **The project's memory stores** (`memory_scopes`, #1190): where a memory's Move rows can move
 * it. Read when the project is, and again whenever `again` changes — a memory written, or the
 * workspaces and personas the stores are, changed. **An answer that is not a list is no
 * answer**, the rule `useVaults` keeps, and neither is a refusal: then there are no Move rows,
 * and the tab's Move is still there.
 */
export function useMemoryStores(plane: PlaneId, again: unknown): readonly MemoryScope[] {
  const [said, setSaid] = useState<{ plane: PlaneId; stores: readonly MemoryScope[] }>();
  useEffect(() => {
    let gone = false;
    void commands
      .memoryScopes(plane)
      .then((answer) => {
        if (gone || answer.status !== "ok" || !Array.isArray(answer.data)) return;
        const stores = answer.data;
        // The same stores read again are the same list: the catalogue is built from it, and a
        // memory written is no reason to build the whole of it again.
        setSaid((was) =>
          was?.plane === plane && JSON.stringify(was.stores) === JSON.stringify(stores)
            ? was
            : { plane, stores },
        );
      })
      .catch(() => {});
    return () => {
      gone = true;
    };
  }, [plane, again]);
  return said?.plane === plane ? said.stores : NO_STORES;
}

const NO_STORES: readonly MemoryScope[] = [];

/**
 * What a project's window lends the memory lists and tabs it draws, for their Move: the stores
 * (their rows' Move rows) and the LIVE workspaces (what a move into one's journal says, #1190).
 */
export const MemoryStores = createContext<MemoryTargets>(NO_TARGETS);

/** The stores and LIVE names lent, or none in a test or a window that lends none. */
export function useLentMemoryStores(): MemoryTargets {
  return useContext(MemoryStores);
}
