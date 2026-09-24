/**
 * Where a handed-off chat came from, as the window says it (charter-app#258).
 *
 * **By the parent's name, never its number.** The operator saw four tabs all called "handoff
 * from 16" and could not tell who 16 was. The core sends the parent as the operator saw it —
 * the name it was given, or its default, `steward 3` — and the workspace it handed off from,
 * and this is the one sentence the tab's tooltip and the chat's pane both draw from them.
 */

/** The parent of a handed-off chat, as the core sends it (`HandedFromNote`). */
export type HandedFrom = { name: string; workspace: string };

/** `↳ from steward 3 · platform-next`, or nothing for a chat no handoff opened. */
export function handedFromNote(from: HandedFrom | null | undefined): string | undefined {
  return from ? `↳ from ${from.name} · ${from.workspace}` : undefined;
}
