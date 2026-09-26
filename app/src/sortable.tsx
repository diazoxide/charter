import type { CSSProperties, KeyboardEvent, ReactNode } from "react";
import {
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type Announcements,
  type KeyboardSensorOptions,
  type ScreenReaderInstructions,
  type UniqueIdentifier,
} from "@dnd-kit/core";
import { restrictToHorizontalAxis, restrictToParentElement } from "@dnd-kit/modifiers";
import { sortableKeyboardCoordinates, useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

/**
 * **What makes a strip's tabs draggable** (SI-6): the three strips' shared settings for
 * `@dnd-kit/core` and `@dnd-kit/sortable`, and nothing in front of them. Each strip still writes
 * its own `DndContext` and `SortableContext`, and each tab its own `useSortable` (through
 * `SortableTab`, which only exists because a hook cannot be called inside a `.map`), so the
 * library is what the call site reads. What a drop *means* is `reorder.ts`'s.
 *
 * **Why these settings, each of which is a thing the strips already had to be true of:**
 *
 * - **A click is still a click.** The pointer has to travel {@link DRAG_AFTER_PX} before a
 *   press becomes a drag, so pressing a tab selects it, a double-click renames it, and the
 *   `×` closes it exactly as before. After a real drag, `dnd-kit` swallows the click the
 *   release would otherwise send, so dropping a tab does not also select it.
 * - **The title bar is still a handle for the window.** Tauri's drag region starts a window
 *   drag on a `mousedown` that reaches it without meeting a clickable element first, and a tab
 *   is a `<button>` (`docs/ui-primitives.md`). So a press on a project tab never moves the
 *   window, and a press on the bar's empty stretch never moves a tab.
 * - **Space and Enter still select.** A focused tab is selected with either (the WAI-ARIA
 *   "Tabs" pattern, and `Window.keyboard.test.tsx` holds the window to it), so the keyboard
 *   picks a tab up with **Shift+Space** instead of `dnd-kit`'s bare Space. Everything after
 *   that is the library's own: the arrows carry it, Space or Enter drops it, Escape puts it
 *   back. See {@link ShiftSpaceSensor}.
 * - **The arrows carry a tab, not the focus, while it is up.** The strip is a roving-focus
 *   group whose arrows move the focus along it; a lifted tab's arrow keys are the drag's
 *   ({@link keepsTheFocus}).
 * - **Along the strip and inside it.** A tab moves on one axis and never out of its row: there
 *   is no dragging a tab out into a window of its own, or from one strip to another.
 */

/** How far the pointer moves, in CSS pixels, before a press on a tab becomes a drag. */
export const DRAG_AFTER_PX = 5;

/** A tab moves along its strip and stays inside it. */
export const ALONG_THE_STRIP = [restrictToHorizontalAxis, restrictToParentElement];

/** What the keyboard does once a tab is up: `dnd-kit`'s defaults, less picking one up with a
 *  bare Space, which selects (see this module). */
const KEYS: KeyboardSensorOptions = {
  coordinateGetter: sortableKeyboardCoordinates,
  keyboardCodes: {
    start: ["Space"],
    cancel: ["Escape"],
    end: ["Space", "Enter", "Tab"],
  },
};

/**
 * `dnd-kit`'s keyboard sensor, which picks a tab up on **Shift+Space** rather than on Space.
 *
 * The library's own activator decides everything else — which element may start it, and what
 * the event is handed on as — and this only asks for Shift first. A custom activator is how the
 * library's documentation says to change what starts a sensor.
 */
export class ShiftSpaceSensor extends KeyboardSensor {
  static activators = [
    {
      eventName: "onKeyDown" as const,
      handler: (...args: Parameters<(typeof KeyboardSensor.activators)[0]["handler"]>) =>
        args[0].shiftKey && KeyboardSensor.activators[0].handler(...args),
    },
  ];
}

/** The sensors every strip uses. */
export function useStripSensors() {
  return useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: DRAG_AFTER_PX } }),
    useSensor(ShiftSpaceSensor, KEYS),
  );
}

/**
 * What a screen reader is told about dragging on a strip of `noun`s, with each tab called by
 * `nameOf` rather than by the key it is sorted under — a project's path, a chat's number.
 */
export function stripAccessibility(
  noun: string,
  nameOf: (id: UniqueIdentifier) => string,
): { announcements: Announcements; screenReaderInstructions: ScreenReaderInstructions } {
  return {
    screenReaderInstructions: {
      draggable:
        `To move this ${noun}, press Shift and Space, then the arrow keys. Space puts it ` +
        `down, and Escape puts it back. Put down among the pinned ones, it is pinned.`,
    },
    announcements: {
      onDragStart: ({ active }) => `Picked up ${nameOf(active.id)}.`,
      onDragOver: ({ active, over }) =>
        over ? `${nameOf(active.id)} is over ${nameOf(over.id)}.` : undefined,
      onDragEnd: ({ active, over }) =>
        over ? `${nameOf(active.id)} was put down at ${nameOf(over.id)}.` : undefined,
      onDragCancel: ({ active }) => `${nameOf(active.id)} was put back.`,
    },
  };
}

/**
 * Keeps the strip's roving focus from moving while a tab is up, so the arrow keys carry the tab
 * and the keyboard stays on it. Radix's roving-focus group gives way to a keystroke whose default
 * was prevented, and `dnd-kit` still reads it: it listens on the document, after the tab.
 */
export function keepsTheFocus(event: KeyboardEvent, lifted: boolean) {
  if (
    lifted &&
    ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)
  )
    event.preventDefault();
}

/** What a tab that can be dragged is handed: `useSortable`'s answer, and the style it implies. */
export interface Sortable {
  sortable: ReturnType<typeof useSortable>;
  /** Where the tab is drawn while something is moving: `useSortable`'s transform, as a
   *  translation only, because tabs are one height and a scale would stretch their text. */
  style: CSSProperties;
}

/**
 * One tab that can be dragged. It draws nothing of its own: `children` is handed
 * `useSortable`'s answer and puts it on the tab's own elements.
 *
 * `fixed` is a tab that cannot be picked up and that nothing is put down on (`reorder.ts`).
 */
export function SortableTab({
  id,
  fixed = false,
  children,
}: {
  id: string;
  fixed?: boolean;
  children: (tab: Sortable) => ReactNode;
}) {
  const sortable = useSortable({ id, disabled: fixed });
  const style: CSSProperties = {
    transform: CSS.Translate.toString(sortable.transform),
    transition: sortable.transition,
  };
  return children({ sortable, style });
}
