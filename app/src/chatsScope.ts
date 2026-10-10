import type { ChatRow } from "./chatsTree";

/**
 * **The Chats list follows the focused workspace** (#1655, D-qw97-1): the rows of the trees
 * that started there, each with everything below it.
 *
 * A tree belongs to where its top row works: the chat a person opened, or a handoff, which
 * moved the work to where it now runs. A task stays under the chat that asked for it wherever
 * it works, so a tree is never cut in two (#1447). A chat started at the plane root belongs to
 * the root's view, by the word its rows say for it (D-qw97-2).
 *
 * The rows come back as they are where nothing is left out, so a list drawn from them is not
 * drawn again for nothing; otherwise the top rows are counted again among themselves, as a
 * screen reader reads them.
 */
export function inScope(rows: readonly ChatRow[], here: string): readonly ChatRow[] {
  const kept: ChatRow[] = [];
  let keeping = false;
  for (const row of rows) {
    if (row.level === 1) keeping = row.workspace === here;
    if (keeping) kept.push(row);
  }
  if (kept.length === rows.length) return rows;
  const tops = kept.filter((row) => row.level === 1).length;
  let at = 0;
  return kept.map((row) =>
    row.level === 1 ? { ...row, posinset: (at += 1), setsize: tops } : row,
  );
}
