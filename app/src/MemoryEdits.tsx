import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { Doing, Ran } from "./actions";
import { commands, type MemoryScope, type MemoryView, type PlaneId } from "./bindings";
import { settled } from "./PlaneEdits";
import {
  DRAFT_TITLE,
  draftView,
  memoryKey,
  memoryView,
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
  "openMemory" | "editMemory" | "archiveMemory" | "newMemory" | "keepTab"
>;

/** How long a Delete's Undo is offered (ADR 0065 Q8: "a few seconds"). */
export const UNDO_MS = 8_000;

/**
 * **A plane's memories, from the window** (SI-9b, ADR 0065): opening one in the strip's preview
 * tab or a kept one, starting an edit, making one, and Delete with its Undo.
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
  /** The Undo line, while a Delete can still be undone. */
  undo: ReactNode;
} {
  const [changed, setChanged] = useState(0);
  const [undoing, setUndoing] = useState<{
    ref: MemoryRef;
    title: string;
    archived: string;
    trouble?: string;
  }>();
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const wrote = useCallback(() => {
    setChanged((was) => was + 1);
    reread();
  }, [reread]);

  useEffect(() => () => clearTimeout(timer.current), []);

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
      clearTimeout(timer.current);
      setUndoing({ ref, title, archived: answer.data.archived });
      timer.current = setTimeout(() => setUndoing(undefined), UNDO_MS);
      return { ok: true };
    },
    [closeView, plane, wrote],
  );

  const undo = useCallback(async () => {
    if (undoing === undefined) return;
    clearTimeout(timer.current);
    const { ref, archived } = undoing;
    // Back under its own slug, which archiving may have had to number (`restore_as`).
    const answer = await settled(commands.memoryUnarchive(plane, ref.scope, archived, ref.slug));
    if (answer.status === "error") {
      setUndoing((now) => (now ? { ...now, trouble: answer.error } : now));
      return;
    }
    setUndoing(undefined);
    wrote();
  }, [plane, undoing, wrote]);

  const onSaved = useCallback(
    (from: ViewRef, memory: MemoryView) => {
      update((tabs) =>
        showInstead(
          tabs,
          from,
          memoryView({ scope: memory.scope, slug: memory.slug }),
          memory.title,
        ),
      );
      wrote();
    },
    [update, wrote],
  );

  const doing = useMemo<MemoryEditing>(
    () => ({ openMemory, editMemory, archiveMemory, newMemory, keepTab }),
    [openMemory, editMemory, archiveMemory, newMemory, keepTab],
  );

  const line =
    undoing === undefined ? null : (
      <p
        className={undoing.trouble === undefined ? "came-back" : "came-back trouble"}
        role="status"
        data-testid="memory-undo"
      >
        {undoing.trouble ??
          `Deleted “${undoing.title}” — it is in the archive now, out of every list.`}{" "}
        <button type="button" className="dismiss" tabIndex={0} onClick={() => void undo()}>
          Undo
        </button>
      </p>
    );

  return { doing, changed, onSaved, undo: line };
}
