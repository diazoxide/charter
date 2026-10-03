/**
 * **A piece's files, and one file of a piece, as views** (RC-5): what the light editor's two
 * tabs are about, as `ViewRef`s like every other view (`tabs.ts`), so they open, dedupe and
 * come back at a launch the way every view does.
 *
 * The key is the piece, `workspace/repo/piece`, and for a file the path after it. None of the
 * three names can hold a `/` (`worktree::path_for` refuses one), so the key splits back
 * exactly. The repo's own folder (#948) has no piece, and its key leaves that part empty —
 * `workspace/repo/` — which no piece's name can be. This module draws nothing and imports no
 * editor code: `Views.tsx` reads it to decide which tab to draw before the editor's chunk is
 * loaded.
 */
import type { ViewRef } from "./tabs";

/** Whose files: a piece of a repo, or — with no piece — the repo's own folder. A `Cut` is one. */
export type Place = { workspace: string; repo: string; piece: string | null };

/** What the place is called on screen: its branch folder's name, or the repo's. */
export function placeName(place: Place): string {
  return place.piece ?? place.repo;
}

/** The place's part of a key. */
function keyOf(place: Place): string {
  return `${place.workspace}/${place.repo}/${place.piece ?? ""}`;
}

const FILES = "piece-files";
const FILE = "piece-file";

/** The view listing a piece's files, with the light editor beside the list. */
export function pieceFilesView(place: Place): ViewRef {
  return { from: null, view: FILES, key: keyOf(place) };
}

/** One file of a piece, in a tab of its own. */
export function pieceFileView(place: Place, path: string): ViewRef {
  return { from: null, view: FILE, key: `${keyOf(place)}/${path}` };
}

/** What a piece tab is called. */
export function pieceFilesTitle(place: Place): string {
  return `Files · ${placeName(place)}`;
}

/** What a file's own tab is called: its name, and the piece it is in. */
export function pieceFileTitle(place: Place, path: string): string {
  return `${path.slice(path.lastIndexOf("/") + 1)} · ${placeName(place)}`;
}

/** The piece, and the file, a view is about; `undefined` for every other view. */
export function pieceOf(view: ViewRef): { place: Place; path?: string } | undefined {
  if (view.from !== null || (view.view !== FILES && view.view !== FILE)) return undefined;
  const [workspace, repo, piece, ...rest] = view.key.split("/");
  if (!workspace || !repo || piece === undefined) return undefined;
  const place = { workspace, repo, piece: piece === "" ? null : piece };
  if (view.view === FILES) return rest.length === 0 ? { place } : undefined;
  const path = rest.join("/");
  return path === "" ? undefined : { place, path };
}
