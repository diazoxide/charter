import type { ViewRef } from "./tabs";

/**
 * **A todo in the window** (#1214): one open todo is one view tab —
 * `{ from: null, view: "todo", key: "<workspace>/<slug>" }` — on its workspace's strip, which
 * reads it (`todo_read`) and draws its title, its whole text, when it was opened, its state and
 * its actions (`TodoTab.tsx`). The operator's ruling of 2026-09-23: tabs hold views, and a todo
 * is an entity with a page, like a memory or a persona.
 *
 * What opens one is the catalogue row `todo.open:<slug>` (`actions.catalogue`), so the Todos
 * panel's row, its menu and the palette are one verb; a second open brings the tab already
 * open forward (`tabs.openView`).
 */

/** The view a todo is opened as. */
export const TODO_VIEW = "todo";

/** One todo: the workspace it is in and its slug, the file stem the core closes it by. */
export type TodoRef = { workspace: string; slug: string };

/** The view one todo is opened as, keyed `<workspace>/<slug>`. */
export function todoView({ workspace, slug }: TodoRef): ViewRef {
  return { from: null, view: TODO_VIEW, key: `${workspace}/${slug}` };
}

/** The todo a {@link todoView}'s key names, or `undefined` for one that names none. A
 *  workspace name and a slug are each one path segment, so the key is exactly two. */
export function todoRefOf(key: string): TodoRef | undefined {
  const parts = key.split("/");
  if (parts.length !== 2 || parts[0] === "" || parts[1] === "") return undefined;
  return { workspace: parts[0], slug: parts[1] };
}

/** The catalogue's id for the row that opens the focused workspace's todo `slug`. */
export function todoOpenId(slug: string): string {
  return `todo.open:${slug}`;
}
