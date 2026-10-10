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
 * with the project, and so is a LIVE workspace's journal (charter-app#301), so a move into one
 * out of a LOCAL workspace's journal is read by everyone the project is, from its next save.
 * The tab's Move says it in its help ({@link movesPublishSaid}); a row that moves into one of
 * those stores says it as its note ({@link publishedSaid}). Both are worded here, once.
 */
export const PUBLISHED_WITH_THE_PROJECT =
  "Persona and shared memory are published with the project.";

/**
 * The project's stores a memory can move into, and the names of its LIVE workspaces: what a
 * Move is offered and worded from (#1190). The window reads both and lends them to the lists.
 */
export type MemoryTargets = {
  stores: readonly MemoryScope[];
  /** The workspaces that are LIVE: their journals are published with the project. */
  live: readonly string[];
};

/** No stores read yet, and no workspace known to be LIVE. */
export const NO_TARGETS: MemoryTargets = { stores: [], live: [] };

/** Whether a move into `to` puts the memory where the project publishes it. */
export function publishedStore(to: MemoryScope, live: readonly string[]): boolean {
  return to.kind !== "workspace" || live.includes(to.name);
}

/** What a move into `to` says of who reads it there: nothing for a LOCAL workspace's journal. */
export function publishedSaid(to: MemoryScope, live: readonly string[]): string | undefined {
  if (!publishedStore(to, live)) return undefined;
  return to.kind === "workspace"
    ? `${to.name} is LIVE, so its journal is published with the project.`
    : PUBLISHED_WITH_THE_PROJECT;
}

/**
 * What the tab's Move help says of the stores it can move into, `targets`: the persona and
 * shared sentence, naming the LIVE workspaces among them where there are any.
 */
export function movesPublishSaid(targets: readonly MemoryScope[], live: readonly string[]): string {
  const journals = targets.flatMap((to) =>
    to.kind === "workspace" && live.includes(to.name) ? [to.name] : [],
  );
  if (journals.length === 0) return PUBLISHED_WITH_THE_PROJECT;
  const which =
    journals.length === 1
      ? `the journal of ${journals[0]}, which is LIVE,`
      : `the journals of ${journals.slice(0, -1).join(", ")} and ${journals[journals.length - 1]}, which are LIVE,`;
  return `Persona and shared memory, and ${which} are published with the project.`;
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
