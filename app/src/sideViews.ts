/**
 * **The views a side shows, one at a time** (ADR 0038 as amended 2026-10-10, #1673).
 *
 * A module of its own, with nothing imported, so the catalogue (`actions.ts`) can name a view on
 * its palette rows without importing the layout module, which imports what imports the
 * catalogue. Where each view is and which is open is `regions.ts`'s.
 */
/**
 * **A view: one of the things a region with an activity bar shows, one at a time** (ADR 0038 as
 * amended 2026-10-10, #1673). Data, as a region is: the catalogue says which region holds it,
 * the placement says which is open, and `RegionFrame` draws the bar and the views from that.
 *
 * The right-hand side's views (#1678) join this union and the attention region's `views`;
 * nothing else changes for them.
 */
export type ViewId = "chats" | "explorer" | "search" | "changes";

/** What a view is called: its tab's `aria-label`, and the palette row that opens it. */
export const VIEWS: Record<ViewId, { name: string }> = {
  chats: { name: "Chats" },
  explorer: { name: "Explorer" },
  // #1676: the Search tab's content, in the side; and the repos' state, which the bottom
  // region drew until it went (B-7).
  search: { name: "Search" },
  changes: { name: "Changes" },
};
