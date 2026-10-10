/**
 * **The views a side shows, one at a time** (ADR 0038 as amended 2026-10-10, #1673).
 *
 * A module of its own, with nothing imported, so the catalogue (`actions.ts`) can name a view on
 * its palette rows without importing the layout module, which imports what imports the
 * catalogue. Where each view is and which is open is `regions.ts`'s.
 */
/**
 * **One of purlis's own views**: the left side's Chats and Explorer (#1673), and the right
 * side's Todos, Memory, Personas, Sessions and Vaults (#1678). Data, as a region is: the
 * catalogue says which region holds it, the placement says which is open, and `RegionFrame`
 * draws the bar and the views from that.
 */
export type OwnViewId =
  "chats" | "explorer" | "todos" | "memory" | "personas" | "sessions" | "vaults";

/**
 * **An approved extension's panel, as a view of its own** (#1678, spec #1671 story 33): its
 * panel key (`ext/<extension>/<id>`, `purlis_core::panel::Panel::key`) behind `panel:`. Not a
 * closed set: which panels exist is what the approved extensions contribute, so the bar learns
 * them as the window does and the layout file keeps one by name.
 */
export type PanelViewId = `panel:${string}`;

/** A view of a side: purlis's own, or an extension's panel. */
export type ViewId = OwnViewId | PanelViewId;

/** The view that shows the panel `key`. */
export const panelView = (key: string): PanelViewId => `panel:${key}`;

/** The panel key a panel's view shows. */
export const panelKeyOf = (view: PanelViewId): string => view.slice("panel:".length);

/** Whether `view` is an extension's panel rather than one of purlis's own views. */
export const isPanelView = (view: string): view is PanelViewId =>
  view.startsWith("panel:") && view.length > "panel:".length;

/** The right side's own views, in its bar's order (#1678): the attention region's. */
export const ATTENTION_VIEWS: readonly OwnViewId[] = [
  "todos",
  "memory",
  "personas",
  "sessions",
  "vaults",
];

/** What a view is called: its tab's `aria-label`, and the palette row that opens it. */
export const VIEWS: Record<OwnViewId, { name: string }> = {
  chats: { name: "Chats" },
  explorer: { name: "Explorer" },
  todos: { name: "Todos" },
  memory: { name: "Memory" },
  personas: { name: "Personas" },
  sessions: { name: "Sessions" },
  vaults: { name: "Vaults" },
};
