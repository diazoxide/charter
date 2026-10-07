import type { DispatchRow } from "./bindings";
import type { ViewRef } from "./tabs";

/**
 * **Dispatches in the window** (#1452): every dispatch a project's chats made — a handoff today,
 * a task once chats dispatch tasks — with who asked, which persona it went to, where it worked,
 * how it came out, how long it took, how often it needed you and what its harness said it cost.
 *
 * It is a view tab, `{ from: null, view: "dispatches", key: "" }`: one per project, opened from
 * the palette's `dispatches.show` row and from the Sessions panel's heading. The records are the
 * app's own, kept on this machine and never committed, so the tab lists what this machine saw.
 */
export const DISPATCHES_VIEW: ViewRef = { from: null, view: "dispatches", key: "" };

/** What the Dispatches tab is called. */
export const DISPATCHES_TITLE = "Dispatches";

/** Whether `view` is the Dispatches tab. */
export function isDispatches(view: ViewRef): boolean {
  return view.from === null && view.view === DISPATCHES_VIEW.view;
}

/** What the list is narrowed to: a persona, an asking chat, both or neither (`""`). The asking
 *  chat is named by its key (`DispatchRow.asker_key`), never by the name it was called: two
 *  chats called the same are two chats. */
export type DispatchFilter = { persona: string; asker: string };

/** Everything, which is how the tab opens. */
export const EVERY_DISPATCH: DispatchFilter = { persona: "", asker: "" };

/** How a filter names the dispatches that went to no persona. */
export const NO_PERSONA = "\u0000none";

/** What the window says of a dispatch that went to no persona, wherever it says it. */
export const NO_PERSONA_SAID = "No persona";

/** The rows `filter` keeps, in the order they came: newest first. */
export function shownDispatches(
  rows: readonly DispatchRow[],
  filter: DispatchFilter,
): DispatchRow[] {
  return rows.filter(
    (row) =>
      (filter.persona === "" || (row.persona ?? NO_PERSONA) === filter.persona) &&
      (filter.asker === "" || row.asker_key === filter.asker),
  );
}

/** The personas the rows went to, by name; then {@link NO_PERSONA} where a row went to none. */
export function personasOf(rows: readonly DispatchRow[]): string[] {
  const named = [...new Set(rows.flatMap((row) => (row.persona === null ? [] : [row.persona])))];
  named.sort((a, b) => a.localeCompare(b));
  return rows.some((row) => row.persona === null) ? [...named, NO_PERSONA] : named;
}

/** A chat that asked: the key a filter holds it by, and the name it is offered under. */
export type Asker = { key: string; name: string };

/** The chats that asked, each once, by name. A chat is its key: the name is the one its newest
 *  dispatch saw it under. */
export function askersOf(rows: readonly DispatchRow[]): Asker[] {
  const seen = new Map<string, string>();
  for (const row of rows) if (!seen.has(row.asker_key)) seen.set(row.asker_key, row.asker);
  return [...seen]
    .map(([key, name]) => ({ key, name }))
    .sort((a, b) => a.name.localeCompare(b.name) || a.key.localeCompare(b.key));
}

/** A record's time (UTC, RFC 3339) as the window says it: `2026-10-07 12:00 UTC`. The text as
 *  it is where it is not one. */
export function saidAt(stamp: string): string {
  const read = /^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2})/.exec(stamp);
  return read ? `${read[1]} ${read[2]} UTC` : stamp;
}
