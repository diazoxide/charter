import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { LoaderCircle } from "lucide-react";
import { useBranchFolders, type BranchFolderRef } from "../branchFolders";
import { branchKey, type Indexed, type StatusRead } from "../branchStatus";
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
import { commands, type OpenChat, type PlaneId } from "../bindings";
import { useFileIcons } from "../projectTheme";
import { placeName, type Place } from "../pieceViews";
import { useTabStop } from "../roving";
import { useReferenceChats } from "../references";
import { touchingIn, useTouching, type Touching, type Touches } from "../touching";

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
 * **What a chat is touching is marked live** (#1154), with the explorer's own dot (FM-6): the
 * file and each folder above it, naming the chat. The tab learns where its branch is and which
 * chats are open only once a chat touches something ({@link useTouchingHere}).
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
  const touching = useTouchingHere(plane, place);
  // The explorer's file rows, with no change marks and nothing narrowed: the file tab shows the
  // branch as it is (FM-4's marks and filter are the explorer's). What chats touch is marked.
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
    touching: touching.size === 0 ? undefined : new Map([[branchKey(place), touching]]),
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

/**
 * **What the chats are touching in `place`'s branch** (#1154): the explorer's `touchingIn` for
 * the one branch this tab draws.
 *
 * The explorer has the branch's folder and the workspace's chats to hand; a tab has neither, so
 * it asks the core — and only once a chat of the project has touched something, so a tab open
 * on a quiet project asks nothing. The folder is asked once per branch (`worktree_list` for a
 * worktree, `workspace_panels` for a repo's own folder), and the open chats again only when a
 * touch comes from a chat not yet known. Neither answer is final when it found nothing (#1605): a
 * folder lookup that failed, and a chat the open chats did not list yet (it touched a file
 * before the list had it), are asked again on a touch that comes {@link ASK_AGAIN_MS} or more
 * after the last ask. A chat is named as its tab is where the window lends the names
 * (`references.tsx`), else by the core's name for it.
 */
function useTouchingHere(plane: PlaneId, place: Place): Touching {
  const touches = useTouching(plane);
  const lent = useReferenceChats();
  const folder = useFolderOnce(plane, place, newest(touches));
  const open = useOpenChatsFor(plane, touches);
  const named =
    lent === undefined || lent.plane !== plane
      ? open
      : open.map((chat) => ({
          ...chat,
          name: lent.chats.find((one) => one.session === chat.session)?.name ?? chat.name,
        }));
  return touchingIn(touches, named, folder);
}

/** How long after an ask that found nothing a touch asks again (#1605): long enough that a chat
 *  touching files as fast as it can costs a few asks a minute, short enough that its mark comes
 *  well within the four seconds it is shown for (`touching.ts`'s `FADE_MS`). */
export const ASK_AGAIN_MS = 2000;

/** When the newest of `touches` was heard, or nothing when there is none. */
function newest(touches: Touches): number | undefined {
  return touches.length === 0 ? undefined : Math.max(...touches.map((one) => one.at));
}

/** Where `place`'s branch is on disk, asked once a touch was heard (`touched`, the newest's time);
 *  nothing while it is unknown or when the core could not say. A lookup that could not say is
 *  asked again on a touch {@link ASK_AGAIN_MS} or more after it. */
function useFolderOnce(
  plane: PlaneId,
  place: Place,
  touched: number | undefined,
): string | undefined {
  const { workspace, repo, piece } = place;
  const key = `${String(plane)}\n${workspace}\n${repo}\n${piece ?? ""}`;
  const [held, setHeld] = useState<{ key: string; folder?: string; at: number }>();
  const known = held?.key === key;
  const due =
    touched !== undefined &&
    (!known || (held.folder === undefined && touched >= held.at + ASK_AGAIN_MS));
  useEffect(() => {
    if (!due) return;
    let gone = false;
    const told = (folder?: string) => {
      if (!gone) setHeld({ key, folder, at: Date.now() });
    };
    const asked =
      piece === null
        ? commands
            .workspacePanels(plane, workspace)
            .then((said) => (said.status === "ok" ? said.data?.paths[repo] : undefined))
        : commands
            .worktreeList(plane, workspace, repo)
            .then((said) =>
              said.status === "ok"
                ? said.data?.find((one) => one.piece === piece)?.path
                : undefined,
            );
    // A refusal is an answer too: no marks, rather than an ask per render, until a later touch.
    asked.then(told, () => told(undefined));
    return () => {
      gone = true;
    };
  }, [due, key, plane, workspace, repo, piece]);
  return known ? held.folder : undefined;
}

/** The project's open chats, asked again when `touches` names a chat not yet known: at once for
 *  a new set of such chats, and for the same set on a touch {@link ASK_AGAIN_MS} or more after
 *  the last ask, since a chat can touch a file before the list has it. */
function useOpenChatsFor(plane: PlaneId, touches: Touches): readonly OpenChat[] {
  const [held, setHeld] = useState<{ plane: PlaneId; chats: readonly OpenChat[] }>();
  const chats = held?.plane === plane ? held.chats : NO_CHATS;
  const strangers = touches.filter((one) => !chats.some((chat) => chat.session === one.session));
  const unknown = [...new Set(strangers.map((one) => one.session))].sort((a, b) => a - b).join(",");
  const touched = newest(strangers);
  /** The unknown chats last asked about, and when, so a touch by a chat that has since closed
   *  is asked about once per {@link ASK_AGAIN_MS} at most, not on every render. */
  const asked = useRef<{ about: string; at: number }>(undefined);
  useEffect(() => {
    const about = `${String(plane)}\n${unknown}`;
    if (unknown === "" || touched === undefined) return;
    const last = asked.current;
    if (last?.about === about && touched < last.at + ASK_AGAIN_MS) return;
    asked.current = { about, at: Date.now() };
    let gone = false;
    commands.openedChats(plane).then(
      (said) => {
        if (!gone && said.status === "ok") setHeld({ plane, chats: said.data ?? NO_CHATS });
      },
      () => undefined,
    );
    return () => {
      gone = true;
    };
  }, [plane, unknown, touched]);
  return chats;
}

const NO_CHATS: readonly OpenChat[] = [];

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
