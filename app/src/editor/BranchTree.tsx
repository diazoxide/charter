import { useState, type KeyboardEvent } from "react";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { LoaderCircle } from "lucide-react";
import { useBranchFolders, type BranchFolderRef } from "../branchFolders";
import type { Indexed, StatusRead } from "../branchStatus";
import {
  childOf,
  fileFold,
  fileRow,
  fileTreeRows,
  FolderEntries,
  levelOf,
  treeKey,
  type FilesOf,
  type TreeItem,
} from "../Explorer";
import type { Offer } from "../actions";
import type { PlaneId } from "../bindings";
import { useFileIcons } from "../projectTheme";
import { placeName, type Place } from "../pieceViews";
import { useTabStop } from "../roving";

/**
 * **A branch's files as a tree, in its file tab** (FM-2, #1103 F3): the explorer's own file rows
 * (FM-1) — the same folders-first order, lazy reads, live updates, refused rows with their
 * reasons and *Show ignored files* — with the branch's folder as the root rather than a *Files*
 * row under a branch.
 *
 * It is the WAI-ARIA "Tree View" pattern the explorer is: one Tab stop (`roving.ts`), Up, Down,
 * Home and End from the roving focus, Right opening a folder or moving into it, Left closing it
 * or moving to its parent, and type-ahead. Enter or a click on a file picks it, and the file
 * picked is the tree's `aria-selected` row and where the keyboard comes back in.
 *
 * **No cap of its own.** The flat list it replaces drew 500 paths of the whole branch (#947);
 * a tree reads one folder when it is opened, and the core answers a folder's first 5,000
 * entries and counts the rest (FM-1's D-8).
 */
export function BranchTree({
  plane,
  place,
  picked,
  onPick,
  onPress,
}: {
  plane: PlaneId;
  place: Place;
  /** The file the preview shows, by its path in the branch. */
  picked?: string;
  onPick: (path: string) => void;
  /** A file or folder row's menu was used (FM-10). No menu without it. */
  onPress?: (offer: Offer) => void;
}) {
  const workspace = place.workspace;
  const top: BranchFolderRef = { repo: place.repo, piece: place.piece, folder: "" };
  const topKey = fileFold(workspace, top);
  /** The folders opened, by fold key. The branch's own folder is always open: it is the root. */
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set([topKey]));
  const [showIgnored, setShowIgnored] = useState(false);
  const reads = useBranchFolders(plane, workspace, openUnder(workspace, top, expanded));
  const icons = useFileIcons(plane, workspace);
  // The explorer's file rows, with no change marks and nothing narrowed: the file tab shows the
  // branch as it is (FM-4's marks and filter are the explorer's).
  const files: FilesOf = {
    workspace,
    expanded,
    reads,
    showIgnored,
    statuses: NONE_READ,
    indexes: NONE_INDEXED,
    changedOnly: false,
    filter: "",
    levels: new Map(),
  };
  const rows = fileTreeRows(workspace, top, files);
  const drawn = rows.filter((row) => row.drawn);
  const pickedRow = picked === undefined ? undefined : fileRow({ ...top, folder: picked });
  const stop = useTabStop(
    pickedRow,
    drawn.map((row) => row.id),
  );
  const byId = new Map(rows.map((row) => [row.id, row]));

  const fold = (key: string, open: boolean) =>
    setExpanded((was) => {
      if (key === topKey || open === was.has(key)) return was;
      const now = new Set(was);
      if (open) now.add(key);
      else now.delete(key);
      return now;
    });

  const treeitem = (id: string): TreeItem => {
    const row = byId.get(id);
    return {
      role: "treeitem",
      "aria-level": row?.level,
      "aria-posinset": row?.posinset,
      "aria-setsize": row?.setsize,
      "aria-expanded": row?.fold?.open,
      // A file is selectable and says whether it is the one shown; a folder only opens.
      "aria-selected": row !== undefined && row.fold === undefined ? id === pickedRow : undefined,
      "data-row": id,
    };
  };

  const onKey = (event: KeyboardEvent<HTMLElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const to = treeKey(drawn, (event.target as HTMLElement).dataset.row, event.key);
    if (to === "not-mine") return;
    event.preventDefault();
    if (to === "stay") return;
    if ("fold" in to) fold(to.fold, to.open);
    else
      [...event.currentTarget.querySelectorAll<HTMLElement>("[data-row]")]
        .find((el) => el.dataset.row === to.focus)
        ?.focus();
  };

  const level = levelOf(files, top);
  return (
    <div className="explorer piece-files-tree">
      <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
        <div
          role="tree"
          aria-label={`Files of ${placeName(place)}`}
          data-testid="piece-files-tree"
          onKeyDown={onKey}
        >
          {"pending" in level ? (
            <p className="pending" aria-busy="true">
              <LoaderCircle className="node-icon spinning" />
              Reading…
            </p>
          ) : "trouble" in level ? (
            <p className="trouble" role="alert" data-testid="piece-files-trouble">
              {level.trouble}
            </p>
          ) : (
            <FolderEntries
              branch={top}
              level={level}
              at={{
                plane,
                place,
                files,
                fold,
                treeitem,
                isDrawn: (id) => byId.get(id)?.drawn ?? false,
                onOpenFile: (_place, path) => onPick(path),
                icons,
                onPress,
              }}
            />
          )}
        </div>
      </RovingFocusGroup.Root>
      <button
        type="button"
        className="ignored-toggle"
        tabIndex={0}
        aria-pressed={showIgnored}
        onClick={() => setShowIgnored((was) => !was)}
      >
        Show ignored files
      </button>
    </div>
  );
}

const NONE_READ: ReadonlyMap<string, StatusRead> = new Map();
const NONE_INDEXED: ReadonlyMap<string, Indexed> = new Map();

/** The folders open under `top`, `top` first: each one whose every folder above is open too.
 *  These are what is read and watched — a folder inside a closed one is neither. */
function openUnder(
  workspace: string,
  top: BranchFolderRef,
  expanded: ReadonlySet<string>,
): BranchFolderRef[] {
  const out: BranchFolderRef[] = [];
  const walk = (ref: BranchFolderRef) => {
    if (!expanded.has(fileFold(workspace, ref))) return;
    out.push(ref);
    for (const key of expanded) {
      const child = childOf(key, workspace, ref);
      if (child !== undefined) walk(child);
    }
  };
  walk(top);
  return out;
}
