/**
 * **A piece's files, and one file of a piece, as views** (RC-5): what the light editor's two
 * tabs are about, as `ViewRef`s like every other view (`tabs.ts`), so they open, dedupe and
 * come back at a launch the way every view does.
 *
 * The key is the piece, `workspace/repo/piece`, and for a file the path after it. None of the
 * three names can hold a `/` (`worktree::path_for` refuses one), so the key splits back
 * exactly. This module draws nothing and imports no editor code: `Views.tsx` reads it to
 * decide which tab to draw before the editor's chunk is loaded.
 */
import type { Cut } from "./actions";
import type { ViewRef } from "./tabs";

const FILES = "piece-files";
const FILE = "piece-file";

/** The view listing a piece's files, with the light editor beside the list. */
export function pieceFilesView(cut: Cut): ViewRef {
  return { from: null, view: FILES, key: `${cut.workspace}/${cut.repo}/${cut.piece}` };
}

/** One file of a piece, in a tab of its own. */
export function pieceFileView(cut: Cut, path: string): ViewRef {
  return { from: null, view: FILE, key: `${cut.workspace}/${cut.repo}/${cut.piece}/${path}` };
}

/** What a piece tab is called. */
export function pieceFilesTitle(cut: Cut): string {
  return `Files · ${cut.piece}`;
}

/** What a file's own tab is called: its name, and the piece it is in. */
export function pieceFileTitle(cut: Cut, path: string): string {
  return `${path.slice(path.lastIndexOf("/") + 1)} · ${cut.piece}`;
}

/** The piece, and the file, a view is about; `undefined` for every other view. */
export function pieceOf(view: ViewRef): { cut: Cut; path?: string } | undefined {
  if (view.from !== null || (view.view !== FILES && view.view !== FILE)) return undefined;
  const [workspace, repo, piece, ...rest] = view.key.split("/");
  if (!workspace || !repo || !piece) return undefined;
  const cut = { workspace, repo, piece };
  if (view.view === FILES) return rest.length === 0 ? { cut } : undefined;
  const path = rest.join("/");
  return path === "" ? undefined : { cut, path };
}
