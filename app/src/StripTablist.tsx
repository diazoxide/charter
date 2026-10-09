import { useCallback, useId } from "react";

/**
 * **A strip's tablist, which owns its tabs and nothing else** (#1204, design R1).
 *
 * A `tablist` may own `tab` elements only (axe's `aria-required-children`), and what it owns
 * is every element under it in the DOM plus every element its `aria-owns` names: `aria-owns`
 * adds to a tablist's children and never takes them away. A strip's cells hold more than a
 * tab, though: a project's `×` and gear, a workspace's gear, a chat's `×`, fresh mark and task
 * chip, and the project strip's own `+` buttons. So the role is not on the element that holds
 * the cells. It is on this empty element inside it, which owns the strip's tabs by id, in the
 * order they are drawn.
 *
 * **Nothing else moves.** The strip element keeps its cells, its class, its ref (`useRoom`
 * measures it), its `RovingFocusGroup.Root` (the arrows, Home and End are the primitive's and
 * read the order from the document, not from a role) and its `DndContext`. It is marked
 * `data-strip="<name>"`, which is what the code, the stylesheet, the scenario specs and the
 * tests reach a strip's tabs through (`tabKeys.ts`, App.css, `e2e/`, `test-strips.ts`); the
 * tablist itself is found by its role and name, as before. The stylesheet lays this element over
 * the strip and lets the pointer through it, so a screen reader that draws its cursor around
 * the tablist draws it around the strip.
 *
 * **What it costs a screen reader**: nothing it had. The `×` and gear are still in the
 * document beside their tab, reachable by a virtual cursor, and the tabs are a list of tabs with
 * the right count. How WebKit reads `aria-owns` under VoiceOver is checked on real hardware.
 *
 * @param name The tablist's accessible name, and the strip's `data-strip`.
 * @param ids The ids of the tabs drawn, in the order they are drawn (`useTabIds`).
 */
export function StripTablist({ name, ids }: { name: string; ids: readonly string[] }) {
  return (
    <span
      className="strip-tablist"
      role="tablist"
      aria-label={name}
      aria-owns={ids.length > 0 ? ids.join(" ") : undefined}
    />
  );
}

/**
 * The id of a strip's tab by where it is drawn, for {@link StripTablist}'s `aria-owns`. By
 * place and not by what the tab shows, because a project's path or a workspace's name may hold
 * a space, which an id list cannot; the list is written again on every render, so a tab that
 * moves keeps being owned in its new place.
 */
export function useTabIds(): (at: number) => string {
  const base = useId();
  return useCallback((at: number) => `${base}tab${at}`, [base]);
}
