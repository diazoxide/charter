import { LiveMark } from "./LiveDialog";
import { Notice } from "./Notice";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type DragEvent,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import {
  ChevronRight,
  Files,
  FileSymlink,
  FileX,
  FolderGit2,
  Folders,
  ListFilter,
  GitBranch,
  LoaderCircle,
  SquareTerminal,
  TriangleAlert,
} from "lucide-react";
import type { FolderEntry, OpenChat, PlaneId } from "./bindings";
import { FileIcon } from "./FileIcon";
import { useFileIcons } from "./projectTheme";
import { iconFor, type IconTheme } from "./theme/icons";
import {
  folderKey,
  useBranchFolders,
  type BranchFolderRef,
  type FolderRead,
} from "./branchFolders";
import {
  branchKey,
  indexed,
  useBranchStatus,
  type BranchRef,
  type Indexed,
  type Marked,
  type StatusRead,
} from "./branchStatus";
import type { Place } from "./pieceViews";
import { WRAPPING_UP, WrappingUp } from "./NeedsYou";
import { PersonaMark } from "./PersonaMark";
import { Menued } from "./Menus";
import { NotClonedHere, type Cloning } from "./NotCloned";
import { WorktreeMark } from "./Worktree";
import { isShell } from "./chatState";
import { ChatShownState, ChildAgents, StartedElsewhere, childAgentsId } from "./ChatRows";
import { rowFactsOf } from "./shownState";
import type { ListedChat } from "./chatsTree";
import type { BranchPath, Catalogued, FileOn, Offer } from "./actions";
import type { WorkspaceState } from "./workspaceState";
import { useTabStop } from "./roving";
import { Breadcrumb, CockpitHeader, focusStands, useAheadBehind } from "./Cockpit";
import { dragReference } from "./references";
import { touchingIn, touchSaid, useTouching, type Touching } from "./touching";

/** No chat started another, for a window that has not said. */
const NONE_STARTED: ReadonlyMap<number, readonly ListedChat[]> = new Map();

/** No chat is named: what an explorer drawn on its own, in a test, is given. */
const NO_NAME = () => undefined;

/** No chat wrapping up, for a window that has not said. */
const NONE_WRAPPING: ReadonlySet<number> = new Set();

/**
 * The left region: the repo and worktree **explorer** (ADR 0038).
 *
 * **It replaces the workspace listing, it does not extend it.** What used to be here was
 * every workspace with its vision text and its chats — the axis ADR 0036 had just given the
 * strip above, answered a second time in more words. That duplication is what made the
 * operator ask what the left sidebar was for.
 *
 * What the left gets instead is the level the three strips do not reach. A workspace holds
 * several clones and several worktrees, and until now nothing in the window selected one.
 *
 * **Picking something here decides where the next chat starts**, which is what makes this a
 * selector rather than a second listing. `New tab` starts in the focused workspace's own
 * directory until a piece is picked, and in that piece afterwards. Nothing is started by
 * picking: the picker still asks which profile and which persona, and the core still decides
 * whether that directory can be started in — it writes the harness layer there or refuses
 * with a sentence naming what stopped it.
 *
 * **A clone is a heading, and it is picked from its menu rather than by a click.** A click on
 * the heading opens and closes it, which is what a heading does; the pick is `Start new chats
 * in <repo>` on the clone's context menu, beside `New tab in <repo>` (charter-app#174). Both
 * carry the path the core spelled (`Panels.paths`), so nothing here joins one together, and a
 * picked clone is marked the way a picked piece is.
 *
 * **Every row that is a place has a context menu, and each is the catalogue filtered to that
 * place** (charter-app#174). A piece's is merge above the line and remove below it; a clone's
 * is the two rows above and nothing below, because nothing in this window writes to a clone.
 * Nothing here says what those rows mean — `Menus.tsx` draws whatever `actions.ts` has, and
 * Shift+F10 or the menu key opens the same menu on the row the arrows are on.
 *
 * **It is drawn as a tree, and the lines are drawn by the rows rather than by the lists.**
 * The nesting was always here — workspace, clone, piece, chat — and nothing said so: four
 * levels of `padding-inline-start` and no line to follow, which is what the operator meant by
 * *"trees are not looking like tree"*. Each row draws its own vertical segment and its own
 * elbow (`App.css`, `.explorer .pieces > li::before`), so the last row's segment simply stops
 * at the elbow. The usual trick — a line on the list, masked at the bottom by a rectangle in
 * the background colour — cannot be used here, because **a region moves** (ADR 0038): the
 * explorer put in the bottom slot sits on `surface.deep`, and a mask painted in `surface.base`
 * would be a visible block. A row that draws its own line has no background to know.
 *
 * **A row never folds, and the region scrolls sideways instead.** The operator's words:
 * *"all trees components texts should not be breakable to new line — it should be horizontal
 * scrollable."* A row names one thing, and a name broken across two lines takes the tree with
 * it — the guides, the indent and the eye all read down a column of first lines. The rule is
 * in `App.css` beside the guides, because it had to be written without touching the padding
 * and the font size those elbows are tuned to. Sentences charter says about a FAILURE still
 * wrap; they are not rows. Neither half of this can be asserted in jsdom, which lays nothing
 * out: `workspace-explorer.e2e.ts` measures the rows and the elbows in the real WebView.
 *
 * **It is a tree, the WAI-ARIA "Tree View" pattern, whole** (charter-app#238). #189 gave it
 * the half that needs no tree semantics; a screen reader was still told it was a list and a
 * `<details>`, and Left and Right did nothing. Now:
 *
 * - **`role="tree"`, and every row a `treeitem`** with its `aria-level` and its place among
 *   its siblings (`aria-posinset` / `aria-setsize`). Those are written down rather than left to
 *   the DOM, because the rows are the buttons and a button cannot hold its children: the
 *   nesting a screen reader would otherwise infer runs through lists, `<details>` and wrappers
 *   that are there for the guides. {@link treeOf} is the one place the shape is decided.
 * - **`aria-expanded` on every row with children**, as the pattern asks of a parent: a clone
 *   says whether it is open, and the workspace row and a worktree with chats working in it say
 *   `true`, because they are parents that are always open. A leaf says nothing.
 * - **Right** opens a closed clone, or moves to a row's first child. **Left** closes an open
 *   clone, or moves to the row's parent — which is also what it does on a parent that cannot
 *   close. The fold is the same state a click on the clone's heading changes.
 * - **Up, Down, Home and End** are the roving focus's, as since #189: ONE Tab stop, through
 *   `roving.ts`, and the stop at rest is the current row — the picked worktree, or the
 *   workspace itself.
 * - **Type-ahead**: a printable key moves to the next row whose name starts with it, wrapping.
 *   One key and not a typed prefix — the names are short and few, and cycling on a repeated
 *   key finds any of them.
 * - **Enter** does what a click does: it picks a worktree, brings a chat forward, and on a
 *   clone's heading opens or closes it, which `<summary>` does natively.
 *
 * Left and Right are taken from the region's sideways scroll while a row has the keyboard. A
 * focused row is scrolled into view by the engine, so nothing the keyboard can reach is lost.
 *
 * **A branch expands into its files** (FM-1, #1103). Under each branch, and under each repo for
 * the repo's own folder (#948), a *Files* row folds open onto that folder's first level, and
 * each folder onto the next — read only when it is opened (`branchFolders.ts`), and read again
 * when an agent adds or removes a file in it. They are rows of the same tree: the same levels,
 * the same arrows, Home and End, and Enter on a file opens it in its file tab. Folders come
 * first, in the order a person reads names. What git ignores is hidden until *Show ignored
 * files* is pressed, and then drawn dimmed and never opened; a file charter will not open — a
 * link out of the branch, git's own folder, a FIFO — is drawn with the reason. The *Files* row
 * is a child of the branch rather than the branch row folding itself, because a click on a
 * branch picks where the next chat starts and the chats working in it stay drawn under it.
 *
 * **What a branch changed is marked on it** (FM-4, #1107): each file it changed, added,
 * deleted or renamed against the branch it was cut from, committed or not, and each folder
 * holding such a file with how many, read again as agents write (`branchStatus.ts`). A file
 * the branch deleted is drawn where it was, and does not open. *Changed only* collapses every
 * open branch's files to what it changed, every folder of it open. The *Filter files* box
 * narrows the file rows drawn to names holding what is typed, keeping the folders on the way to
 * one; Esc clears it.
 *
 * **A file or folder row has a menu that changes nothing** (FM-10, #1113): copy its path,
 * relative or absolute, reveal it in the file manager, your editor for a file and a shell tab
 * for a folder (`actions.fileRows`). Never create, rename, move or delete (ADR 0081, V86 F8):
 * charter does not race an agent writing the same tree. Each row names the branch and the path
 * inside it; the core places it (`files::place`), so no path the window joined leaves it.
 *
 * **Focused on one branch, it is that branch's cockpit** (FM-5, #1108; V86 F2): a breadcrumb
 * back out, the branch's state — how far it is from its base, how many files it changed, and
 * Merge and Done — then the chats working in it and its files, as rows of a tree of their own
 * with the same keys. Esc, or the breadcrumb, steps back out to the whole workspace, and the
 * keyboard lands on the branch's row. Which branch is the window's (`PlaneView.tsx`), and it is
 * remembered with the window's views.
 *
 * **A chat that started one in another workspace says so under its row** (#1447), after its
 * sub-agents: the started chat by name, with what it is doing and the workspace it went to.
 * One that works here has its own row at the place it works, because this tree's axis is the
 * place. The project-wide tree of who started whom is the Chats section above this region's
 * explorer (`ChatsSection.tsx`).
 *
 * **What a chat is touching right now is marked live** (FM-6, #1109; V86 F6): a file a chat's
 * tool reads or edits, and every folder above it up to the branch's *Files* row, carry a dot
 * naming the chat on hover, which fades a few seconds after the chat goes quiet on it
 * (`touching.ts`). The core confined the path to the chat's own folder, and it was written
 * nowhere (D-86a).
 */
export function Explorer({
  plane,
  workspace,
  live = false,
  state,
  chats,
  spot,
  onPick,
  onShowChat,
  wrapping = NONE_WRAPPING,
  offers,
  onPress,
  onOpenFile,
  focus,
  onFocus,
  cloning,
  onReadAgain,
  started = NONE_STARTED,
  nameOf = NO_NAME,
}: {
  /** The project, for reading a branch's folders. Without one no folder is read. */
  plane?: PlaneId;
  /** The focused workspace, or nothing when the strip is on the chats that are in none. */
  workspace: string | undefined;
  /** Whether that workspace is LIVE (charter-app#301): its row carries the mark. */
  live?: boolean;
  state: WorkspaceState;
  /** The chats working in this workspace, so a piece can say what is already running in it. */
  chats: readonly OpenChat[];
  /** The piece picked, or nothing for the workspace's own directory. */
  spot: Spot | undefined;
  onPick: (spot: Spot | undefined) => void;
  onShowChat: (session: number) => void;
  /** The chats wrapping up — being smart-closed (ADR 0064) — whose rows say so. */
  wrapping?: ReadonlySet<number>;
  /** The catalogue by id, which is what a piece row's menu is drawn out of. */
  offers: Catalogued;
  onPress: (offer: Offer) => void;
  /** A file of a branch was opened from the tree: its file tab comes forward. */
  onOpenFile?: (place: Place, path: string) => void;
  /** The branch the explorer is focused on, its cockpit (FM-5), or nothing for the whole
   *  workspace. Drawn only while the workspace lists it. */
  focus?: Place;
  /** Focus on a branch, or step back out with nothing. */
  onFocus?: (focus: Place | undefined) => void;
  /** What this window is cloning into the workspace, so a repo that is not cloned here shows
   *  its clone under way, or why it failed (#1215). */
  cloning?: Cloning;
  /** Asks the workspace's reads again: the way out of a refused one (NO-4). It is this
   *  region's to offer, for the bottom bar's lines too, since that region is not pressed. */
  onReadAgain: () => void;
  /** The chats each chat here started that work in another workspace, by its number (#1447):
   *  drawn under its row, naming that workspace. */
  started?: ReadonlyMap<number, readonly ListedChat[]>;
  /** What a chat of the project is called now, by its number, whichever workspace it works
   *  in (#1484): a task that is asking the chat that dispatched it names that chat as its own
   *  row does. Nothing for a chat that has closed. */
  nameOf?: (session: number) => string | undefined;
}) {
  /** The clones the operator folded, by workspace and name: a row inside one is not drawn, so
   *  it cannot be where the keyboard comes back in. */
  const [folded, setFolded] = useState<ReadonlySet<string>>(new Set());
  /** The folders of branches the operator opened, by {@link fileFold}: closed until opened. */
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  /** Whether what git ignores is drawn, dimmed. */
  const [showIgnored, setShowIgnored] = useState(false);
  /** Whether a branch's files are collapsed to what it changed (FM-4). */
  const [changedOnly, setChangedOnly] = useState(false);
  /** What the file rows are narrowed to: names holding it, any case. */
  const [filter, setFilter] = useState("");
  /** The cockpit's *Files* rows the operator closed: open until they do (FM-5). */
  const [shut, setShut] = useState<ReadonlySet<string>>(new Set());
  const cockpit = cockpitOf(workspace, state, focus);
  const cockpitFiles =
    cockpit === undefined || workspace === undefined
      ? undefined
      : fileFold(workspace, { ...cockpit.ref, folder: "" });
  const expandedNow =
    cockpitFiles !== undefined && !shut.has(cockpitFiles) && !expanded.has(cockpitFiles)
      ? new Set([...expanded, cockpitFiles])
      : expanded;
  const open = openFolders(workspace, state, expandedNow);
  const branches: BranchRef[] = open
    .filter((ref) => ref.folder === "")
    .map(({ repo, piece }) => ({ repo, piece }));
  // The cockpit's branch is read whether or not its files are open: its header counts them.
  if (cockpit !== undefined && !branches.some((one) => branchKey(one) === branchKey(cockpit.ref)))
    branches.push(cockpit.ref);
  const statuses = useBranchStatus(plane, workspace, branches);
  const cockpitStatus = cockpit === undefined ? undefined : statuses.get(branchKey(cockpit.ref));
  const apart = useAheadBehind(plane, workspace, cockpit?.ref, cockpitStatus);
  // Indexed once per answer, not per folder per keystroke: a branch of 10,000 changes is walked
  // when its status arrives, and each folder after that is one lookup.
  const indexes = useMemo(
    () =>
      new Map(
        [...statuses].flatMap(([key, read]) =>
          read.status === undefined ? [] : [[key, indexed(read.status)] as const],
        ),
      ),
    [statuses],
  );
  const reads = useBranchFolders(plane, workspace, open);
  const touches = useTouching(plane);
  // What the chats are touching in each branch read here: at most a few hundred paths over the
  // open branches, so worked out each render rather than remembered.
  const touching = new Map<string, Touching>();
  for (const ref of touches.length === 0 ? [] : branches) {
    const folder =
      ref.piece === null
        ? state.panels?.paths[ref.repo]
        : state.pieces[ref.repo]?.find((one) => one.piece === ref.piece)?.path;
    const marks = touchingIn(touches, chats, folder);
    if (marks.size > 0) touching.set(branchKey(ref), marks);
  }
  const icons = useFileIcons(plane, workspace);
  const files: FilesOf = {
    workspace: workspace ?? "",
    expanded: expandedNow,
    reads,
    showIgnored,
    statuses,
    indexes,
    changedOnly,
    filter: filter.trim().toLocaleLowerCase(),
    levels: new Map(),
    touching,
  };
  const tree =
    cockpit === undefined || workspace === undefined
      ? treeOf(workspace, state, chats, folded, files)
      : cockpitTreeOf(workspace, cockpit, chats, files);
  const drawn = tree.filter((row) => row.drawn);
  const picked =
    cockpit !== undefined
      ? undefined
      : spot === undefined
        ? ROOT
        : spot.piece === undefined
          ? cloneRow(spot.repo)
          : pieceRow(spot.repo, spot.piece);
  const stop = useTabStop(
    picked,
    drawn.map((row) => row.id),
  );

  /** Opens or closes a clone or a branch's folder: the one fold state of each, whether a click
   *  or a key asked. A clone is open until folded; a folder is closed until opened. */
  const fold = (key: string, open: boolean) => {
    if (key === cockpitFiles)
      setShut((was) => {
        if (open !== was.has(key)) return was;
        const now = new Set(was);
        if (open) now.delete(key);
        else now.add(key);
        return now;
      });
    foldHeld(key, open);
  };
  const foldHeld = (key: string, open: boolean) =>
    key.startsWith(FILE_FOLD)
      ? setExpanded((was) => {
          if (open === was.has(key)) return was;
          const now = new Set(was);
          if (open) now.add(key);
          else now.delete(key);
          return now;
        })
      : setFolded((was) => {
          if (open === !was.has(key)) return was;
          const now = new Set(was);
          if (open) now.delete(key);
          else now.add(key);
          return now;
        });

  /** Whether a row is drawn. One inside a folded clone is still in the document, and the
   *  roving focus is told to pass it by: jsdom focuses it, and the arrows would stop on it. */
  const byId = new Map(tree.map((row) => [row.id, row]));
  const isDrawn = (id: string) => byId.get(id)?.drawn ?? false;

  /** What a row says to a screen reader about where it is in the tree. */
  const treeitem = (id: string): TreeItem => {
    const row = byId.get(id);
    return {
      role: "treeitem",
      "aria-level": row?.level,
      "aria-posinset": row?.posinset,
      "aria-setsize": row?.setsize,
      "aria-expanded": row?.fold?.open ?? (row?.parents ? true : undefined),
      "data-row": id,
    };
  };

  // Going in or coming back out of the cockpit moves the
  // keyboard, when it was in the explorer or nowhere, to where the explorer now is: the
  // cockpit's first row, or the branch's own row in the whole workspace.
  const cockpitKey = cockpit === undefined ? undefined : branchKey(cockpit.ref);
  const was = useRef(cockpitKey);
  const navRef = useRef<HTMLElement>(null);
  useEffect(() => {
    if (workspace === undefined) return;
    const left = was.current;
    was.current = cockpitKey;
    if (left === cockpitKey) return;
    const nav = navRef.current;
    const active = document.activeElement;
    if (nav === null || (active !== document.body && active !== null && !nav.contains(active)))
      return;
    const wanted =
      cockpit !== undefined
        ? drawn[0]?.id
        : left === undefined
          ? undefined
          : pieceRow(left.slice(0, left.indexOf("/")), left.slice(left.indexOf("/") + 1));
    [...nav.querySelectorAll<HTMLElement>("[data-row]")]
      .find((el) => el.dataset.row === wanted)
      ?.focus();
    // Only when the cockpit comes or goes; what it draws is read as it is then.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cockpitKey, workspace]);

  /** Steps back out of the cockpit to the whole workspace. */
  const leave = () => onFocus?.(undefined);

  /** Left, Right and type-ahead (#238). Up, Down, Home and End are the roving focus's. */
  const onTreeKey = (event: KeyboardEvent<HTMLElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const target = event.target as HTMLElement;
    const to = treeKey(drawn, target.dataset.row, event.key);
    if (to === "not-mine") return;
    event.preventDefault();
    if (to === "stay") return;
    if ("fold" in to) fold(to.fold, to.open);
    else
      [...event.currentTarget.querySelectorAll<HTMLElement>("[data-row]")]
        .find((el) => el.dataset.row === to.focus)
        ?.focus();
  };

  if (workspace === undefined) {
    return (
      // Inside the same roving group as the full explorer below, so that the `nav` is the SAME
      // element when a workspace arrives: a different parent would have React remount it, and
      // everything holding the old one — a scenario, a screen reader's place — would lose it.
      <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
        <nav className="explorer" aria-label="Explorer" data-testid="explorer">
          {/* The strip for chats outside every workspace is not a workspace on the plane, so
            there is no directory to explore and nothing honest to draw. */}
          <p className="empty">No workspace focused, so there is nothing to explore.</p>
        </nav>
      </RovingFocusGroup.Root>
    );
  }

  const { panels, pieces, piecesRefused } = state;
  const clones = panels?.repos ?? [];
  /** A branch's *Files* row, and its folders under it as they are opened. */
  const filesOf = (repo: string, piece: string | null) => (
    <FilesRow
      branch={{ repo, piece, folder: "" }}
      at={{
        plane,
        place: { workspace, repo, piece },
        files,
        fold,
        treeitem,
        isDrawn,
        onOpenFile,
        onPress,
        icons,
      }}
    />
  );
  // The chats that are in no piece of this workspace: they work in the workspace itself or
  // in a clone, and they are listed under the workspace row rather than dropped. `treeOf`
  // decided which they are, and the render reads it rather than deciding a second time.
  const atTheRoot = chats.filter((chat) => byId.get(chatRow(chat.session))?.parent === ROOT);
  // A task is listed under the chat that asked for it (#1436), which `treeOf` decided too: a
  // list draws the chats whose row is not under another chat's, and each row draws its own.
  const askerRow = (chat: OpenChat) => byId.get(chatRow(chat.session))?.parent;
  const isTop = (chat: OpenChat) => !askerRow(chat)?.startsWith(CHAT_ROW);
  const dispatched = (session: number) =>
    chats.filter((chat) => askerRow(chat) === chatRow(session));

  const filterBox = open.length > 0 && (
    // Only while some branch's files are open: it narrows the file rows and nothing else.
    <div className="files-filter">
      <ListFilter className="node-icon" />
      <input
        type="search"
        value={filter}
        aria-label="Filter files"
        placeholder="Filter files"
        onChange={(event) => setFilter(event.target.value)}
        onKeyDown={(event) => {
          if (event.key !== "Escape" || filter === "") return;
          // Taken here: an Esc that cleared the box has done its job, and nothing behind
          // the explorer should act on it too.
          event.preventDefault();
          event.stopPropagation();
          setFilter("");
        }}
      />
    </div>
  );
  const toggles = open.length > 0 && (
    // Only while some branch's files are open: with none, there is nothing they change.
    <div className="files-toggles">
      <button
        type="button"
        className="changed-toggle"
        aria-pressed={changedOnly}
        onClick={() => setChangedOnly((was) => !was)}
      >
        Changed only
      </button>
      <button
        type="button"
        className="ignored-toggle"
        aria-pressed={showIgnored}
        onClick={() => setShowIgnored((was) => !was)}
      >
        Show ignored files
      </button>
    </div>
  );

  if (cockpit !== undefined) {
    const { ref, name } = cockpit;
    const here = chats.filter(
      (chat) => byId.get(chatRow(chat.session)) !== undefined && isTop(chat),
    );
    return (
      <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
        <nav
          ref={navRef}
          className="explorer cockpit"
          aria-label="Explorer"
          data-testid="explorer"
          onKeyDown={(event) => {
            // Esc steps back out; one something inside already took (the filter box clearing
            // itself, a menu closing) has done its job.
            if (event.key !== "Escape" || event.defaultPrevented) return;
            event.preventDefault();
            leave();
          }}
        >
          <Breadcrumb workspace={workspace} repo={ref.repo} name={name} onLeave={leave} />
          <CockpitHeader
            name={name}
            repo={ref.repo}
            piece={ref.piece}
            apart={apart}
            status={cockpitStatus}
            offers={offers}
            onPress={onPress}
          />
          {state.trouble && (
            <ReadRefused cause={`workspace-read:${workspace}`} onReadAgain={onReadAgain}>
              {state.trouble}
            </ReadRefused>
          )}
          {filterBox}
          <div role="tree" aria-label={`Chats and files of ${name}`} onKeyDown={onTreeKey}>
            <ChatList
              chats={here}
              started={started}
              dispatched={dispatched}
              nameOf={nameOf}
              wrapping={wrapping}
              onShow={onShowChat}
              treeitem={treeitem}
              isDrawn={isDrawn}
            />
            <ul className="files" role="group">
              <li role="none" data-testid={`files-${ref.repo}-${ref.piece}`}>
                {filesOf(ref.repo, ref.piece)}
              </li>
            </ul>
          </div>
          {toggles}
        </nav>
      </RovingFocusGroup.Root>
    );
  }

  return (
    <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
      <nav ref={navRef} className="explorer" aria-label="Explorer" data-testid="explorer">
        {state.trouble && (
          <ReadRefused cause={`workspace-read:${workspace}`} onReadAgain={onReadAgain}>
            {state.trouble}
          </ReadRefused>
        )}

        {filterBox}

        {/* The tree is the rows and what holds them. The sentences about the whole region — the
          trouble above, the pending and empty notes and what purlis would not read below —
          are outside it. The ones about ONE clone (its worktrees could not be listed, are
          still coming, or are none) stay inside that clone's `<details>`, beside the row they
          explain, and so inside the tree: moving them out would take them away from it. */}
        <div role="tree" aria-label="Repos and branches" onKeyDown={onTreeKey}>
          <RovingFocusGroup.Item asChild tabStopId={ROOT} active={spot === undefined}>
            <button
              type="button"
              className="spot spot-root"
              {...treeitem(ROOT)}
              // Not `aria-selected`, even in a tree: the three tablists in this window are
              // where it selects (ADR 0036), and a picked spot is not a selection the keyboard
              // moves but the place the next chat starts — the current item, which is what
              // `aria-current` is for and what the old sidebar's workspace rows used.
              aria-current={spot === undefined ? "true" : undefined}
              onClick={() => onPick(undefined)}
            >
              <Folders className="node-icon" />
              <span className="spot-name">{workspace}</span>
              {live && <LiveMark />}
              <span className="spot-what">the workspace itself</span>
            </button>
          </RovingFocusGroup.Item>
          <ChatList
            chats={atTheRoot}
            started={started}
            dispatched={dispatched}
            nameOf={nameOf}
            wrapping={wrapping}
            onShow={onShowChat}
            treeitem={treeitem}
            isDrawn={isDrawn}
          />

          {clones.length > 0 && (
            // The clones are the workspace row's children, and the wrapper is what lets them be
            // drawn as such — the tree lines hang off it, one level in from the root row.
            <div className="clones" role="group">
              {clones.map((repo) => (
                // `<details>` and not a primitive: the browser has a collapsible and
                // `docs/ui-primitives.md` says native HTML that already does the job is not what
                // the Radix rule is about. Open by default — a closed explorer explores nothing.
                <details
                  className="clone"
                  key={repo}
                  data-testid={`clone-${repo}`}
                  // Held by the fold state rather than by the element, so that Left and Right
                  // (#238) open and close it through the same state a click on the heading does.
                  open={!folded.has(foldKey(workspace, repo))}
                  onToggle={(event) => fold(foldKey(workspace, repo), event.currentTarget.open)}
                >
                  {/* The clone's menu: a new tab in it, and picking it as where new chats
                    start (charter-app#174). On the heading, for the piece rows' reason — the
                    `<details>` also holds every row inside the clone. */}
                  <Menued on={{ on: "clone", repo }} offers={offers} onPress={onPress}>
                    <RovingFocusGroup.Item
                      asChild
                      tabStopId={cloneRow(repo)}
                      active={picked === cloneRow(repo)}
                    >
                      <summary
                        {...treeitem(cloneRow(repo))}
                        aria-current={picked === cloneRow(repo) ? "true" : undefined}
                      >
                        {/* The twisty says which way the disclosure goes, which the default marker
                    said in the platform's own glyph at the platform's own size. It turns
                    with `[open]`, and the turn is the one motion here that is a direct
                    answer to a click — `prefers-reduced-motion` stops it all the same. */}
                        <ChevronRight className="twisty" />
                        <FolderGit2 className="node-icon" />
                        <span className="repo">{repo}</span>
                        <PieceCount pieces={pieces[repo]} refused={piecesRefused[repo]} />
                      </summary>
                    </RovingFocusGroup.Item>
                  </Menued>
                  {/* The repo's own folder (#948), before its branches: the files of whatever
                    the repo has checked out. */}
                  <ul className="pieces" role="group">
                    <li role="none" data-testid={`files-${repo}`}>
                      {filesOf(repo, null)}
                    </li>
                  </ul>
                  {piecesRefused[repo] ? (
                    // Said, never swallowed: a clone with no rows otherwise reads as a clone
                    // nobody has cut a branch in.
                    <ReadRefused
                      cause={`branches-read:${workspace}/${repo}`}
                      onReadAgain={onReadAgain}
                    >
                      purlis could not list the branches of <code>{repo}</code>:{" "}
                      {piecesRefused[repo]}
                    </ReadRefused>
                  ) : pieces[repo] === undefined ? (
                    <Pending>Asking git…</Pending>
                  ) : pieces[repo].length === 0 ? (
                    <p className="none">No branches cut here</p>
                  ) : (
                    <ul className="pieces" role="group">
                      {pieces[repo].map((piece) => {
                        const working = chats.filter((chat) => under(chat.cwd, piece.path));
                        const isPicked = spot?.repo === repo && spot.piece === piece.piece;
                        return (
                          <li
                            key={piece.piece}
                            role="none"
                            data-testid={`piece-${repo}-${piece.piece}`}
                          >
                            {/* **Right-click is what these rows were missing** (charter-app#174).
                            The menu is the catalogue filtered to this piece — merge above the
                            line, remove below it, and the discard row that only exists while
                            the core has refused THIS removal. Nothing here says what those
                            rows mean; `Menus.tsx` draws whatever `actions.ts` has.

                            On the button and not on the `<li>`: the `<li>` also holds the
                            chats running in this piece, and each of those is its own row with
                            its own identity. `asChild`, so the row gains no element. */}
                            <Menued
                              on={{ on: "worktree", repo, piece: piece.piece }}
                              offers={offers}
                              onPress={onPress}
                            >
                              <RovingFocusGroup.Item
                                asChild
                                tabStopId={pieceRow(repo, piece.piece)}
                                active={isPicked}
                                focusable={isDrawn(pieceRow(repo, piece.piece))}
                              >
                                <button
                                  type="button"
                                  className="spot"
                                  aria-current={isPicked ? "true" : undefined}
                                  {...treeitem(pieceRow(repo, piece.piece))}
                                  // The whole path, because two clones in one workspace can hold a
                                  // piece of the same name and the row has room for one word.
                                  title={piece.path}
                                  onClick={() =>
                                    onPick({ repo, piece: piece.piece, path: piece.path })
                                  }
                                >
                                  <GitBranch className="node-icon" />
                                  <span className="spot-name">{piece.piece}</span>
                                </button>
                              </RovingFocusGroup.Item>
                            </Menued>
                            {/* The branch, and the two states the operator has to see BEFORE they
                          start a chat in a tree: `unwired` and `stale`. The same component
                          the palette's worktree rows are written against. */}
                            <WorktreeMark
                              worktree={{
                                workspace,
                                repo,
                                piece: piece.piece,
                                branch: piece.branch,
                                wired: piece.wired,
                                stale: piece.stale,
                              }}
                            />
                            {/* What the piece said about itself — `done`, `abandoned: <why>` or
                          `silent 3d` — so a finished piece and a quiet one do not look alike
                          (charter#368). An age, never a verdict. */}
                            {piece.said && (
                              <span className="label said" data-testid="piece-said">
                                {piece.said}
                              </span>
                            )}
                            {/* Its files (FM-1), first among its children. */}
                            <ul className="files" role="group">
                              <li role="none" data-testid={`files-${repo}-${piece.piece}`}>
                                {filesOf(repo, piece.piece)}
                              </li>
                            </ul>
                            <ChatList
                              chats={working.filter(isTop)}
                              started={started}
                              dispatched={dispatched}
                              nameOf={nameOf}
                              wrapping={wrapping}
                              onShow={onShowChat}
                              treeitem={treeitem}
                              isDrawn={isDrawn}
                            />
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </details>
              ))}
            </div>
          )}
        </div>

        {panels === undefined ? (
          <Pending>Reading the plane…</Pending>
        ) : (
          clones.length === 0 && <p className="none">No repos in this workspace</p>
        )}

        {/* Membership without a clone. There is nothing to explore in it and nothing to start
            a chat in, so it is named and not made a heading — and it can be cloned (#1215). */}
        <NotClonedHere
          absent={panels?.absent ?? []}
          cloning={cloning}
          offers={offers}
          onPress={onPress}
        />

        {panels?.refused.map(([name, why]) => (
          <ReadRefused
            key={`refused-${name}`}
            cause={`repo-refused:${workspace}/${name}`}
            onReadAgain={onReadAgain}
          >
            purlis will not read <code>{name}</code>: {why}
          </ReadRefused>
        ))}

        {toggles}
      </nav>
    </RovingFocusGroup.Root>
  );
}

/** **A read this region was refused, as a Notice** (NO-4, V91b): the sentence, and **Read
 *  again**, which asks the workspace's reads again and clears the line once one goes through. */
function ReadRefused({
  cause,
  onReadAgain,
  children,
}: {
  cause: string;
  onReadAgain: () => void;
  children: ReactNode;
}) {
  return (
    <Notice cause={cause} tone="trouble" fixes={[{ label: "Read again", onPress: onReadAgain }]}>
      {children}
    </Notice>
  );
}

/** A folder of a branch's tree that could not be read, said in the tree where its entries
 *  would be: a row's own refusal, not a standing line about the window (folding the folder and
 *  opening it again reads it again). The icon is decorative and Lucide hides it. */
function FolderRefused({ children }: { children: ReactNode }) {
  return (
    <p className="trouble" role="alert">
      <TriangleAlert className="node-icon" />
      <span>{children}</span>
    </p>
  );
}

/** Something charter is still reading.
 *
 *  **The one place in this region an animation earns its place.** A spinner here means "this
 *  is still happening", which is a state no colour and no word can distinguish from "this
 *  stopped and nothing came back" — the reason the operator asked for motion on the pipelines.
 *  It is a state that ends, and it is drawn at most twice. `prefers-reduced-motion` stops the
 *  spin and leaves the mark. */
function Pending({ children }: { children: ReactNode }) {
  return (
    <p className="pending">
      <LoaderCircle className="node-icon spinning" />
      <span>{children}</span>
    </p>
  );
}

/** Where the next chat starts, when it is not the workspace's own directory: a piece, or — with
 *  no `piece` — the clone itself, picked from its menu (charter-app#174).
 *
 *  It carries the path the CORE spelled — `worktree_list` answers with a piece's, and
 *  `Panels.paths` with a clone's — so nothing here ever joins one together. */
export type Spot = { repo: string; piece?: string; path: string };

/** How many pieces a clone has, on its heading, so a closed one still says whether there is
 *  anything in it. */
function PieceCount({ pieces, refused }: { pieces?: readonly unknown[]; refused?: string }) {
  if (refused !== undefined) return <span className="piece-count none">unreadable</span>;
  if (pieces === undefined) return <span className="piece-count pending">…</span>;
  return (
    <span className="piece-count" aria-label={`${pieces.length} branches`}>
      {pieces.length}
    </span>
  );
}

/** The chats working at a spot, each a button that brings its tab to the front.
 *
 *  This is not the sidebar's old listing coming back: it is every chat in ONE place, under
 *  the place, which is what answers "is anything already running in this worktree" before
 *  the operator starts a second one in it. */
function ChatList({
  chats,
  started,
  dispatched,
  nameOf,
  wrapping,
  onShow,
  treeitem,
  isDrawn,
}: {
  chats: readonly OpenChat[];
  /** The chats each chat started that work in another workspace, by its number. */
  started: ReadonlyMap<number, readonly ListedChat[]>;
  /** The persona chats a chat dispatched as tasks, which are listed under it (#1436). */
  dispatched: (session: number) => readonly OpenChat[];
  /** What a chat of the project is called now, by its number. */
  nameOf: (session: number) => string | undefined;
  wrapping: ReadonlySet<number>;
  onShow: (session: number) => void;
  /** What a row says about its place in the tree. */
  treeitem: (id: string) => TreeItem;
  /** Whether a row is drawn, and so whether the arrows stop on it. */
  isDrawn: (id: string) => boolean;
}) {
  if (chats.length === 0) return null;
  return (
    <ul className="here" role="group">
      {chats.map((chat) => (
        <li key={chat.session} role="none">
          <RovingFocusGroup.Item
            asChild
            tabStopId={chatRow(chat.session)}
            focusable={isDrawn(chatRow(chat.session))}
          >
            <button
              type="button"
              className="chat"
              data-wrapping-up={wrapping.has(chat.session) || undefined}
              title={wrapping.has(chat.session) ? WRAPPING_UP : undefined}
              // What it spawned, read as its description: an id that names no element, for a
              // chat with no helpers, describes it with nothing.
              aria-describedby={childAgentsId(chat.session)}
              {...treeitem(chatRow(chat.session))}
              onClick={() => onShow(chat.session)}
            >
              {/* What tells a chat leaf from a worktree leaf at a glance. The tree has two
                kinds of leaf under one kind of parent, and at fifty chats the indent alone
                stopped being enough to tell them apart. */}
              {/* Its persona's mark where it runs as one (#1449): which persona, read before
                the name is. A chat with none keeps the terminal. */}
              {chat.persona ? (
                <PersonaMark persona={chat.persona} className="node-icon" />
              ) : (
                <SquareTerminal className="node-icon" />
              )}
              <span className="session">{chat.name}</span>
              {/* Its own chat's state, a mark and a word, read by the mark itself (SC-3): the
                value the Chats list's row draws (#1484). A task that has reported says how,
                and one that ended without a report says that. */}
              <ChatShownState
                session={chat.session}
                shell={isShell(chat)}
                {...rowFactsOf(chat, nameOf)}
              />
              <WrappingUp held={wrapping.has(chat.session)} />
              {/* The PROFILE where there is one, and the harness otherwise. A profile is what
                the operator picked and what a relaunch looks up again; the kind is what the
                plane calls the harness. Showing the profile alone would hide which harness
                it runs, and showing the kind alone would hide which account. */}
              {chat.profile ? (
                <span className="harness">
                  {chat.profile}
                  {chat.harness && <span className="kind"> ({chat.harness})</span>}
                </span>
              ) : (
                chat.harness && <span className="harness">{chat.harness}</span>
              )}
              {chat.persona && <span className="persona">{chat.persona}</span>}
            </button>
          </RovingFocusGroup.Item>
          {/* What its harness cannot tell charter, on the chat itself (#27). A Codex chat
              reads `unknown` until its first prompt and never says it is waiting on an
              approval; without this it looks like purlis is broken. */}
          {chat.unreported && <p className="unreported">{chat.unreported}</p>}
          {/* What it spawned, under it (FD-18): its helpers, each with its state. */}
          <ChildAgents session={chat.session} name={chat.name} />
          {/* The chats it started that went to another workspace (#1447), each naming it. */}
          <StartedElsewhere chats={started.get(chat.session)} name={chat.name} onShow={onShow} />
          {/* The tasks it dispatched, under it: each a chat of its own, by name and state. */}
          <ChatList
            chats={dispatched(chat.session)}
            started={started}
            dispatched={dispatched}
            nameOf={nameOf}
            wrapping={wrapping}
            onShow={onShow}
            treeitem={treeitem}
            isDrawn={isDrawn}
          />
        </li>
      ))}
    </ul>
  );
}

/** What the tree knows about branches' folders: which are open, what each holds, and whether
 *  what git ignores is drawn. */
export type FilesOf = {
  workspace: string;
  expanded: ReadonlySet<string>;
  reads: ReadonlyMap<string, FolderRead>;
  showIgnored: boolean;
  /** What each open branch changed, by {@link branchKey}. */
  statuses: ReadonlyMap<string, StatusRead>;
  /** The same, indexed by path and by folder, by {@link branchKey}. */
  indexes: ReadonlyMap<string, Indexed>;
  /** Whether each branch's files are collapsed to what it changed. */
  changedOnly: boolean;
  /** What the file rows are narrowed to, lower-cased; `""` narrows nothing. */
  filter: string;
  /** Each folder's {@link levelOf}, worked out once per render. */
  levels: Map<string, Level>;
  /** What the chats are touching in each open branch, by {@link branchKey} (FM-6). */
  touching?: ReadonlyMap<string, Touching>;
};

/** One row under a folder of a branch, as both the render and {@link treeOf} walk it. */
export type Child = {
  name: string;
  kind: FolderEntry["kind"];
  ignored: boolean;
  refused: string | undefined;
  /** A folder that opens onto what it holds. */
  expands: boolean;
  /** A folder that is always open: one of *Changed only*'s. */
  always: boolean;
  marked: Marked | undefined;
};

/** What a folder of a branch draws under it: still being read, refused, or its rows. */
export type Level =
  { pending: true } | { trouble: string } | { children: Child[]; more: number; narrowed: boolean };

/** What a file the branch deleted says, drawn where it was. */
const DELETED = "deleted on this branch";

/** Whether a folder of a branch is open: one of *Changed only*'s always is. */
function isOpen(files: FilesOf, ref: BranchFolderRef, always: boolean): boolean {
  return always || files.expanded.has(fileFold(files.workspace, ref));
}

/**
 * What an OPEN folder of a branch draws under it: its entries — or, under *Changed only*, the
 * paths below it the branch changed — each with its mark, narrowed to the filter.
 *
 * **The one answer the render and {@link treeOf} both read**, so a row drawn is a row the
 * keyboard and a screen reader know, and the other way round.
 */
export function levelOf(files: FilesOf, ref: BranchFolderRef): Level {
  const cached = files.levels.get(folderKey(ref));
  if (cached !== undefined) return cached;
  const level = levelOfUncached(files, ref);
  files.levels.set(folderKey(ref), level);
  return level;
}

function levelOfUncached(files: FilesOf, ref: BranchFolderRef): Level {
  const key = branchKey(ref);
  const index = files.indexes.get(key);
  let children: Child[];
  let more = 0;
  if (files.changedOnly) {
    const read = files.statuses.get(key);
    if (read === undefined) return { pending: true };
    if (read.trouble !== undefined) return { trouble: read.trouble };
    children = changedUnder(index, ref.folder);
    // Past the most the core marks, the rest are counted at the branch's top.
    if (ref.folder === "") more = index?.status.more ?? 0;
  } else {
    const read = files.reads.get(folderKey(ref));
    if (read === undefined) return { pending: true };
    if (read.trouble !== undefined) return { trouble: read.trouble };
    more = read.more ?? 0;
    const entries = shown(read.entries ?? [], files.showIgnored);
    children = entries.map((entry): Child => ({
      name: entry.name,
      kind: entry.kind,
      ignored: entry.ignored,
      refused: entry.refused ?? undefined,
      expands: expands(entry),
      always: false,
      marked: index?.marks.get(joined(ref.folder, entry.name)),
    }));
    // What the branch deleted here is not on disk to list: it is drawn where it was.
    const listed = new Set(entries.map((entry) => entry.name));
    const gone = changedUnder(index, ref.folder).filter(
      (child) => child.marked?.mark === "deleted" && !child.expands && !listed.has(child.name),
    );
    children = [...children, ...gone];
  }
  if (files.filter === "") return { children, more, narrowed: false };
  const kept = children.filter((child) => {
    if (child.name.toLocaleLowerCase().includes(files.filter)) return true;
    if (!child.expands) return false;
    const here = { ...ref, folder: joined(ref.folder, child.name) };
    if (!isOpen(files, here, child.always)) return false;
    const below = levelOf(files, here);
    return "children" in below && below.children.length > 0;
  });
  return { children: kept, more, narrowed: true };
}

/** The rows directly under `folder` that lead to a change: each changed file there, and each
 *  folder holding one, folders first, in the order a person reads names. */
function changedUnder(index: Indexed | undefined, folder: string): Child[] {
  return (index?.under.get(folder) ?? []).map((seed) => ({
    name: seed.name,
    kind: seed.folder ? "folder" : "file",
    ignored: false,
    refused: !seed.folder && seed.mark === "deleted" ? DELETED : undefined,
    expands: seed.folder,
    always: seed.folder,
    marked: index?.marks.get(joined(folder, seed.name)),
  }));
}

/** The words a mark is said in. */
const MARK_SAID: Record<Marked["mark"], string> = {
  changed: "changed",
  added: "added",
  deleted: "deleted",
  renamed: "renamed",
};

/** A file's or a folder's mark: a letter for a file, how many for a folder, and the words for a
 *  screen reader and a hover. */
function ChangeBadge({ marked, folder }: { marked: Marked | undefined; folder: boolean }) {
  if (marked === undefined) return null;
  const said = folder
    ? `${marked.count ?? 0} ${marked.count === 1 ? "change" : "changes"}`
    : marked.from
      ? `renamed from ${marked.from}`
      : MARK_SAID[marked.mark];
  return (
    <span className="change-mark" data-mark={marked.mark} title={said} aria-label={said}>
      {folder ? marked.count : marked.mark[0].toUpperCase()}
    </span>
  );
}

/** The live mark on a file or folder a chat is touching right now (FM-6): a dot, and on hover
 *  and to a screen reader which chats. */
function TouchMark({ names }: { names: readonly string[] | undefined }) {
  if (names === undefined || names.length === 0) return null;
  const said = touchSaid(names);
  return <span className="touch-mark" role="img" title={said} aria-label={said} />;
}

/** Everything a branch's file rows share: the branch they are of, what the tree knows about
 *  its folders, and what a row does to the tree and to a screen reader. The file tab (FM-2) draws
 *  its tree out of the same rows. */
export type FileRows = {
  /** The project the rows are of: what a row dragged onto a chat names (FM-9). No drag
   *  without it. */
  plane?: PlaneId;
  place: Place;
  files: FilesOf;
  fold: (key: string, open: boolean) => void;
  treeitem: (id: string) => TreeItem;
  isDrawn: (id: string) => boolean;
  onOpenFile?: (place: Place, path: string) => void;
  /** The project's icon theme (FM-3), which every file and folder row is drawn from. */
  icons: IconTheme;
  /** A file or folder row's menu was used (FM-10): copy its path, reveal it, your editor or a
   *  shell tab there. No menu without it. */
  onPress?: (offer: Offer) => void;
};

/** What a file row's menu is looked up in: nothing, because its rows are its own
 *  (`actions.fileRows`). */
const NO_CATALOGUE: Catalogued = new Map();

/**
 * A file or folder row with its menu (FM-10): the rows `actions.fileRows` builds for it — never
 * listed in the catalogue — and nothing that creates, renames, moves or deletes (ADR 0081). The row is drawn as it was when the tree
 * has no way to carry a row out.
 */
function FileMenu({ on, at, children }: { on: FileOn; at: FileRows; children: ReactNode }) {
  if (at.onPress === undefined) return <>{children}</>;
  return (
    <Menued on={on} offers={NO_CATALOGUE} onPress={at.onPress}>
      {children}
    </Menued>
  );
}

/**
 * What makes a file or folder row draggable onto a chat (FM-9): it carries the row's branch and
 * path, which the core places and renders in the chat's own syntax. Nothing on a refused row,
 * or where the tree does not know its project.
 */
function draggedAs(at: FileRows, path: string, folder: boolean, refused?: string) {
  const plane = at.plane;
  if (plane === undefined || refused !== undefined || path === "") return {};
  return {
    draggable: true,
    onDragStart: (event: DragEvent) => dragReference(event, { plane, ...at.place, path, folder }),
  };
}

/** A file row's path in the branch, as its menu names it. */
function pathOn(at: FileRows, path: string): BranchPath {
  return { ...at.place, path };
}

/** A branch folder's *Files* row, or a folder's row, with what it holds under it once opened. */
function FilesRow({
  branch,
  name = "Files",
  always = false,
  at,
}: {
  branch: BranchFolderRef;
  /** Its name: *Files* for the branch's own folder, the folder's name below it. */
  name?: string;
  /** Always open: a folder of *Changed only*'s, which does not fold. */
  always?: boolean;
  at: FileRows;
}) {
  const id = fileRow(branch);
  const key = fileFold(at.place.workspace, branch);
  const open = isOpen(at.files, branch, always);
  const level = open ? levelOf(at.files, branch) : undefined;
  const marked = at.files.indexes.get(branchKey(branch))?.marks.get(branch.folder);
  const touchedBy = at.files.touching?.get(branchKey(branch))?.get(branch.folder);
  const row = (
    <RovingFocusGroup.Item asChild tabStopId={id} focusable={at.isDrawn(id)}>
      <button
        type="button"
        className="file-node"
        {...at.treeitem(id)}
        {...draggedAs(at, branch.folder, true)}
        onClick={() => {
          if (!always) at.fold(key, !open);
        }}
      >
        <ChevronRight className="twisty" data-open={open || undefined} />
        {/* The branch's own folder is the *Files* row; a folder under it is drawn by name. */}
        {branch.folder === "" ? (
          <Files className="node-icon" />
        ) : (
          <FileIcon symbol={iconFor(at.icons, { name, folder: true, open })} />
        )}
        <span className="spot-name">{name}</span>
        {/* A space, so a screen reader says the mark as its own word; flex drops it. */}{" "}
        <TouchMark names={touchedBy} />
        <ChangeBadge marked={marked} folder />
      </button>
    </RovingFocusGroup.Item>
  );
  return (
    <>
      {/* A folder's menu (FM-10); the branch's own *Files* row has none, its branch's row
          being where that folder is acted on. */}
      {branch.folder === "" ? (
        row
      ) : (
        <FileMenu on={{ on: "file", at: pathOn(at, branch.folder), kind: "folder" }} at={at}>
          {row}
        </FileMenu>
      )}
      {level !== undefined &&
        ("pending" in level ? (
          <Pending>Reading…</Pending>
        ) : "trouble" in level ? (
          <FolderRefused>{level.trouble}</FolderRefused>
        ) : (
          <FolderEntries branch={branch} level={level} at={at} />
        ))}
    </>
  );
}

/** What one opened folder holds, a row each: folders that open in turn, and files; and how many
 *  more it holds than the core lists. */
export function FolderEntries({
  branch,
  level,
  at,
}: {
  branch: BranchFolderRef;
  level: { children: Child[]; more: number; narrowed: boolean };
  at: FileRows;
}) {
  const { children: entries, more, narrowed } = level;
  if (entries.length === 0 && more === 0) {
    // A folder the filter emptied says nothing: the rows that match say where to look.
    if (narrowed) return null;
    return (
      <p className="none">
        {at.files.changedOnly && branch.folder === "" ? "Nothing changed" : "Nothing here"}
      </p>
    );
  }
  return (
    <>
      <ul className="files" role="group">
        {entries.map((entry) => {
          const path = joined(branch.folder, entry.name);
          const here = { ...branch, folder: path };
          if (entry.expands) {
            return (
              <li
                key={`d${entry.name}`}
                role="none"
                data-ignored={entry.ignored || undefined}
                data-mark={entry.marked?.mark}
              >
                <FilesRow branch={here} name={entry.name} always={entry.always} at={at} />
              </li>
            );
          }
          const id = fileRow(here);
          const refused = entry.refused;
          const touchedBy = at.files.touching?.get(branchKey(branch))?.get(path);
          // Refused and linked files keep their own marks: what they say is that this row is
          // not an ordinary file, which matters more than what kind of file it would be.
          const Mark = refused !== undefined ? FileX : entry.kind === "link" ? FileSymlink : null;
          return (
            <li
              key={`f${entry.name}`}
              role="none"
              data-ignored={entry.ignored || undefined}
              data-mark={entry.marked?.mark}
            >
              <FileMenu
                on={{
                  on: "file",
                  at: pathOn(at, path),
                  kind: entry.kind,
                  refused,
                }}
                at={at}
              >
                <RovingFocusGroup.Item asChild tabStopId={id} focusable={at.isDrawn(id)}>
                  <button
                    type="button"
                    className="file-node"
                    data-refused={refused !== undefined || undefined}
                    aria-disabled={refused !== undefined || undefined}
                    title={refused ?? path}
                    {...at.treeitem(id)}
                    {...draggedAs(at, path, false, refused)}
                    onClick={() => {
                      if (refused === undefined) at.onOpenFile?.(at.place, path);
                    }}
                  >
                    {Mark === null ? (
                      <FileIcon symbol={iconFor(at.icons, { name: entry.name, folder: false })} />
                    ) : (
                      <Mark className="node-icon" />
                    )}
                    <span className="spot-name">{entry.name}</span> <TouchMark names={touchedBy} />
                    <ChangeBadge marked={entry.marked} folder={false} />
                    {/* Why it does not open, said beside it — an ignored file's too, which is
                      drawn only once the operator asked to see what git ignores. */}
                    {refused !== undefined && <span className="spot-what">{refused}</span>}
                  </button>
                </RovingFocusGroup.Item>
              </FileMenu>
            </li>
          );
        })}
      </ul>
      {more > 0 && (
        // A folder of tens of thousands of generated files is drawn as its first entries; the
        // rest are counted, never drawn.
        <p className="none">
          {`${more.toLocaleString("en")} ${at.files.changedOnly ? "more changes" : "more"} not shown`}
        </p>
      )}
    </>
  );
}

/** Whether an entry is a folder that opens onto what it holds: one charter does not refuse. */
function expands(entry: FolderEntry): boolean {
  return entry.kind === "folder" && entry.refused === null;
}

/** The entries drawn: every one when ignored files are shown, else those git does not ignore. */
export function shown(entries: readonly FolderEntry[], showIgnored: boolean): FolderEntry[] {
  return showIgnored ? [...entries] : entries.filter((entry) => !entry.ignored);
}

/** A folder's path and a name in it, as a path inside the branch. */
function joined(folder: string, name: string): string {
  return folder === "" ? name : `${folder}/${name}`;
}

/**
 * The folders the operator opened that are drawn: each one whose *Files* row and every folder
 * above it is open too, of a branch the workspace still has. These are what is read and
 * watched — a folder inside a closed one is neither.
 */
function openFolders(
  workspace: string | undefined,
  state: WorkspaceState,
  expanded: ReadonlySet<string>,
): BranchFolderRef[] {
  if (workspace === undefined) return [];
  const out: BranchFolderRef[] = [];
  const walk = (ref: BranchFolderRef) => {
    if (!expanded.has(fileFold(workspace, ref))) return;
    out.push(ref);
    for (const key of expanded) {
      const child = childOf(key, workspace, ref);
      if (child !== undefined) walk(child);
    }
  };
  for (const repo of state.panels?.repos ?? []) {
    walk({ repo, piece: null, folder: "" });
    for (const piece of state.pieces[repo] ?? []) walk({ repo, piece: piece.piece, folder: "" });
  }
  return out;
}

/** The folder a fold key names when it is a folder directly inside `parent`. */
export function childOf(
  key: string,
  workspace: string,
  parent: BranchFolderRef,
): BranchFolderRef | undefined {
  const prefix = fileFold(workspace, parent);
  const into = parent.folder === "" ? prefix : `${prefix}/`;
  if (!key.startsWith(into) || key === prefix) return undefined;
  const rest = key.slice(into.length);
  if (rest === "" || rest.includes("/")) return undefined;
  return { ...parent, folder: joined(parent.folder, rest) };
}

/** Whether a chat's directory is this piece's, or inside it.
 *
 *  By path components and never by string prefix: `…/piece-two` starts with `…/piece` and is
 *  a different tree. */
function under(cwd: string | null, path: string): boolean {
  if (cwd === null) return false;
  if (cwd === path) return true;
  return cwd.startsWith(`${path}/`) || cwd.startsWith(`${path}\\`);
}

/** The workspace's own row, as a stop in the explorer's roving focus. */
const ROOT = "root";
/** What starts every chat's row id. */
const CHAT_ROW = "chat:";
const chatRow = (session: number) => `${CHAT_ROW}${session}`;
const cloneRow = (repo: string) => `clone:${repo}`;
const pieceRow = (repo: string, piece: string) => `piece:${repo}/${piece}`;
/** A folded clone, by workspace as well as name: two workspaces can each clone `svc`. */
const foldKey = (workspace: string, repo: string) => `${workspace}/${repo}`;
/** A branch's folder, or one of its files, as a row. */
export const fileRow = (ref: BranchFolderRef) => `file:${folderKey(ref)}`;
/** What starts every opened folder's fold key, so `fold` knows it from a clone's. */
const FILE_FOLD = "files:";
/** An opened folder of a branch, by workspace as well: two workspaces can each clone `svc`. */
export const fileFold = (workspace: string, ref: BranchFolderRef) =>
  `${FILE_FOLD}${workspace}\u0000${folderKey(ref)}`;

/** The branch the explorer is focused on, while the workspace still lists it: its name as a
 *  branch, and as the explorer names branches. */
type Cockpit = { ref: { repo: string; piece: string }; name: string; path: string | undefined };

/**
 * The cockpit to draw for `focus` (FM-5), or none: a focus on a branch of this workspace that
 * still stands by {@link focusStands}, the rule the window reads too.
 */
function cockpitOf(
  workspace: string | undefined,
  state: WorkspaceState,
  focus: Place | undefined,
): Cockpit | undefined {
  if (workspace === undefined || focus === undefined || focus.workspace !== workspace) return;
  const stands = focusStands(state, focus);
  if (stands === undefined || focus.piece === null) return;
  const { piece } = stands;
  return {
    ref: { repo: focus.repo, piece: focus.piece },
    name: piece?.branch || focus.piece,
    path: piece?.path,
  };
}

/** The cockpit's rows (FM-5): the chats working in the branch, then its *Files* row and what it
 *  holds, each a top-level row of the cockpit's own tree. */
function cockpitTreeOf(
  workspace: string,
  cockpit: Cockpit,
  chats: readonly OpenChat[],
  files: FilesOf,
): Row[] {
  const { path } = cockpit;
  const working = path === undefined ? [] : chats.filter((chat) => under(chat.cwd, path));
  const kids: TreeNode[] = [
    ...chatNodes(working),
    folderNode(workspace, { ...cockpit.ref, folder: "" }, "Files", files),
  ];
  const rows: Row[] = [];
  kids.forEach((kid, i) => walkRows(kid, undefined, true, i, kids.length, rows));
  return rows;
}

/** One row of the tree, as the keyboard and a screen reader know it (#238). */
export type Row = {
  id: string;
  /** What type-ahead matches: the row's own name, which is its first word on screen. */
  name: string;
  level: number;
  parent: string | undefined;
  /** Its place among its siblings, from 1, and how many siblings there are. */
  posinset: number;
  setsize: number;
  /** A clone's fold, by {@link foldKey}, and whether it is open. Nothing on a row that cannot
   *  fold. */
  fold?: { key: string; open: boolean };
  /** Whether it is drawn: a row inside a folded clone is not, so it cannot be the stop and
   *  no key moves to it. */
  drawn: boolean;
  /** Whether it has children, drawn or not: a parent says `aria-expanded`, a leaf does not. */
  parents: boolean;
};

/** The attributes a row carries as a `treeitem`, and the `data-row` the keys find it by. */
export type TreeItem = {
  role: "treeitem";
  /** The file the file tab previews (FM-2); the explorer selects no file. */
  "aria-selected"?: boolean;
  "aria-level"?: number;
  "aria-posinset"?: number;
  "aria-setsize"?: number;
  "aria-expanded"?: boolean;
  "data-row": string;
};

/**
 * Every row of the explorer's tree, in the order it is drawn — for `useTabStop`, for the
 * attributes a screen reader reads and for the keys that move.
 *
 * **The same walk the render does, and it has to stay so**: a row missing here can never be
 * the stop and says nothing about where it is, and a row listed as drawn that is not could be
 * the only stop, which would leave the explorer with none at all.
 *
 * The shape: the workspace, then the chats in no worktree and the clones as its children, the
 * worktrees as a clone's, and the chats working in a worktree as its. A clone whose worktrees
 * could not be listed has none drawn.
 */
function treeOf(
  workspace: string | undefined,
  state: WorkspaceState,
  chats: readonly OpenChat[],
  folded: ReadonlySet<string>,
  files: FilesOf,
): Row[] {
  if (workspace === undefined) return [];
  const { panels, pieces } = state;
  const inAPiece = new Set<number>();
  const clones = (panels?.repos ?? []).map((repo): TreeNode => {
    const branches = (pieces[repo] ?? []).map((piece): TreeNode => {
      const working = chats.filter((chat) => under(chat.cwd, piece.path));
      for (const chat of working) inAPiece.add(chat.session);
      return {
        id: pieceRow(repo, piece.piece),
        name: piece.piece,
        kids: [
          folderNode(workspace, { repo, piece: piece.piece, folder: "" }, "Files", files),
          ...chatNodes(working),
        ],
        shows: true,
      };
    });
    const kids = [
      folderNode(workspace, { repo, piece: null, folder: "" }, "Files", files),
      ...branches,
    ];
    const key = foldKey(workspace, repo);
    const open = !folded.has(key);
    return {
      id: cloneRow(repo),
      name: repo,
      fold: { key, open },
      kids,
      shows: open,
    };
  });
  const root: TreeNode = {
    id: ROOT,
    name: workspace,
    kids: [...chatNodes(chats.filter((chat) => !inAPiece.has(chat.session))), ...clones],
    shows: true,
  };

  const rows: Row[] = [];
  walkRows(root, undefined, true, 0, 1, rows);
  return rows;
}

/**
 * **The chats at one spot as nodes, each task under the chat that asked for it** (#1436): a
 * persona chat a dispatch started is a child of its asking chat's row when both work at this
 * spot. One whose asking chat is not listed here (it closed, or works elsewhere) is listed
 * beside the others, as any chat is.
 */
function chatNodes(here: readonly OpenChat[]): TreeNode[] {
  const listed = new Set(here.map((chat) => chat.session));
  const asker = (chat: OpenChat) =>
    chat.from?.task && chat.from.chat !== chat.session && listed.has(chat.from.chat)
      ? chat.from.chat
      : undefined;
  // `above` is the chats already on the way down, so a record that names a loop draws each
  // chat once and stops.
  const node = (chat: OpenChat, above: ReadonlySet<number>): TreeNode => ({
    id: chatRow(chat.session),
    name: chat.name,
    kids: here
      .filter((kid) => asker(kid) === chat.session && !above.has(kid.session))
      .map((kid) => node(kid, new Set([...above, chat.session]))),
    shows: true,
  });
  return here.filter((chat) => asker(chat) === undefined).map((chat) => node(chat, new Set()));
}

/** `shows` is whether its children are drawn when it is: not in a folded clone or a closed
 *  folder. A clone whose branches could not be listed has none to draw, and says why. */
type TreeNode = { id: string; name: string; fold?: Row["fold"]; kids: TreeNode[]; shows: boolean };

/** A branch's folder and, once opened, what it holds: the same walk `FilesRow` draws, through
 *  {@link levelOf}. A folder of *Changed only* is `always` open and does not fold. */
function folderNode(
  workspace: string,
  ref: BranchFolderRef,
  name: string,
  files: FilesOf,
  always = false,
): TreeNode {
  const key = fileFold(workspace, ref);
  const open = isOpen(files, ref, always);
  const level = open ? levelOf(files, ref) : undefined;
  const children = level !== undefined && "children" in level ? level.children : [];
  const kids = children.map((child): TreeNode => {
    const here = { ...ref, folder: joined(ref.folder, child.name) };
    return child.expands
      ? folderNode(workspace, here, child.name, files, child.always)
      : { id: fileRow(here), name: child.name, kids: [], shows: true };
  });
  return {
    id: fileRow(ref),
    name,
    fold: always ? undefined : { key, open },
    kids,
    shows: open,
  };
}

/** A node and everything under it, as rows, appended to `rows` in the order they are drawn. */
function walkRows(
  node: TreeNode,
  parent: Row | undefined,
  drawn: boolean,
  at: number,
  of: number,
  rows: Row[],
) {
  const row: Row = {
    id: node.id,
    name: node.name,
    level: (parent?.level ?? 0) + 1,
    parent: parent?.id,
    posinset: at + 1,
    setsize: of,
    fold: node.fold,
    drawn,
    parents: node.kids.length > 0,
  };
  rows.push(row);
  node.kids.forEach((kid, i) => walkRows(kid, row, drawn && node.shows, i, node.kids.length, rows));
}

/**
 * The rows of one branch's tree with its own folder as the root, which is not a row itself: what
 * the file tab draws (FM-2). Its folder's entries are the first level.
 */
export function fileTreeRows(workspace: string, top: BranchFolderRef, files: FilesOf): Row[] {
  const rows: Row[] = [];
  const root = folderNode(workspace, top, "", files);
  root.kids.forEach((kid, i) => walkRows(kid, undefined, root.shows, i, root.kids.length, rows));
  return rows;
}

/** What a key does to the tree: move to a row, open or close a clone, `stay` — a key the tree
 *  takes and has nothing to do with here, such as Right on a leaf — or `not-mine`, a key the
 *  tree leaves to whoever else wants it. */
type TreeMove = { focus: string } | { fold: string; open: boolean } | "stay" | "not-mine";

/**
 * What a key does on a row of the tree, by the WAI-ARIA "Tree View" pattern — or nothing, for
 * a key the tree leaves alone.
 *
 * @param drawn The rows drawn, in order.
 * @param from The row the key was pressed on.
 */
export function treeKey(drawn: readonly Row[], from: string | undefined, key: string): TreeMove {
  const at = drawn.findIndex((row) => row.id === from);
  if (at < 0) return "not-mine";
  const row = drawn[at];
  if (key === "ArrowRight") {
    if (row.fold && !row.fold.open) return { fold: row.fold.key, open: true };
    // The next row drawn is the first child exactly when its parent is this one.
    const child = drawn[at + 1];
    return child?.parent === row.id ? { focus: child.id } : "stay";
  }
  if (key === "ArrowLeft") {
    if (row.fold?.open) return { fold: row.fold.key, open: false };
    return row.parent === undefined ? "stay" : { focus: row.parent };
  }
  // Type-ahead: one printable character, and never Space, which is a button's own.
  if (!/^\S$/u.test(key)) return "not-mine";
  const wanted = key.toLocaleLowerCase();
  for (let step = 1; step < drawn.length; step++) {
    const next = drawn[(at + step) % drawn.length];
    if (next.name.toLocaleLowerCase().startsWith(wanted)) return { focus: next.id };
  }
  return "stay";
}
