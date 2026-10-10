/**
 * **What a memory's Move says, wherever it is made** (KN-3, #1190): in its tab, from its row's
 * menu and from the palette. One module, so the tab's help, a menu row and a palette row cannot
 * drift apart in what they tell a person about who will read a memory once it has moved.
 */
import type { MemoryScope } from "./bindings";

/** What a Move does to the file. */
export const MOVES_WHOLE = "Moves the file whole, with its title and date. Nothing is copied.";

/**
 * **Who reads the store a memory goes to**: a persona's memory and shared memory are published
 * with the project, so a move into one out of a LOCAL workspace's journal is read by everyone the
 * project is, from its next save. The tab's Move says it in its help; a row that moves into one
 * of those stores says it as its note.
 */
export const PUBLISHED_WITH_THE_PROJECT =
  "Persona and shared memory are published with the project.";

/** Whether a move into `to` puts the memory where the project publishes it. */
export function publishedStore(to: MemoryScope): boolean {
  return to.kind !== "workspace";
}

/** What the Move choice, and a Move submenu's row, calls a store. */
export function storeLabel(scope: MemoryScope): string {
  switch (scope.kind) {
    case "workspace":
      return `${scope.name} — workspace`;
    case "persona":
      return `${scope.name} — persona`;
    case "shared":
      return "shared";
  }
}
