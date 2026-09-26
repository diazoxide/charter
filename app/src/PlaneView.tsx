import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type CSSProperties,
  type ReactNode,
} from "react";
import clsx from "clsx";
import { listen } from "./here";
import { MAIN, thisWindow } from "./windows";
import { Group, Panel, Separator } from "react-resizable-panels";
import * as Menu from "@radix-ui/react-dropdown-menu";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { closestCenter, DndContext } from "@dnd-kit/core";
import { horizontalListSortingStrategy, SortableContext } from "@dnd-kit/sortable";
import {
  ChevronDown,
  FolderOpen,
  FolderPlus,
  MessageSquarePlus,
  Pin as PinMark,
  Plus,
  SquareSplitHorizontal,
  SquareSplitVertical,
  X,
} from "lucide-react";
import {
  commands,
  type AtRisk,
  type PlaneSaving,
  type ChatWorktree,
  type OpenChat,
  type PlaneId,
  type Refused,
  type Sidebar as SidebarModel,
  type StartOptions,
  type ViewTab,
} from "./bindings";
import {
  catalogue,
  catalogued,
  ignoreId,
  needsYouRows,
  OUTSIDE,
  OUTSIDE_TITLE,
  perform,
  showId,
  PASS_THROUGH_BYTES,
  PASS_THROUGH_KEY,
  RENAMES_ON_F2,
  type Clone,
  type Cut,
  type Doing,
  type Offer,
  type Project,
  type Ran,
} from "./actions";
import { usePlaneSaving, WAY_OUT, type WayOut } from "./saving";
import { LiveDialog, LiveMark } from "./LiveDialog";
import { DeleteWorkspace } from "./DeleteWorkspace";
import { Menued } from "./Menus";
import { afterDrop, reslotted } from "./reorder";
import {
  ALONG_THE_STRIP,
  keepsTheFocus,
  SortableTab,
  stripAccessibility,
  useStripSensors,
} from "./sortable";
import { NewWorkspace } from "./NewWorkspace";
import { RenameWorkspace } from "./RenameWorkspace";
import { cloneRepos } from "./repoClones";
import { StartChat } from "./StartChat";
import { SessionPane } from "./SessionPane";
import { Explorer, type Spot } from "./Explorer";
import { BottomBar } from "./BottomBar";
import { useWorkspaceState } from "./workspaceState";
import { useExtensionFacts } from "./extensionFacts";
import { usePlaneChanged } from "./planeChanged";
import { PlaneUpdatedMark, usePlaneUpdated, type PlaneUpdates } from "./PlaneUpdated";
import { inSlots, SIDES, useArrangement } from "./regions";
import { RegionFrame } from "./RegionFrame";
import { useDoctor } from "./Doctor";
import { ChatGauge, useChatUsage } from "./ChatGauge";
import { usePin } from "./Updates";
import { StatusLine, runningIn, type Alerts } from "./StatusLine";
import {
  byLastActivity,
  closeFocusedPane,
  closeTab,
  focusPane,
  noTabs,
  chatNameOf,
  chatOf,
  contentsOf,
  findView,
  openTab,
  openTabBehind,
  openView,
  panesOf,
  putViewBack,
  refileViews,
  followRename,
  PREFERENCES_TITLE,
  PREFERENCES_VIEW,
  SAVING_TITLE,
  SAVING_VIEW,
  SETTINGS_TITLE,
  SETTINGS_VIEW,
  workspaceSettingsTitle,
  workspaceSettingsView,
  renameTab,
  viewKey,
  selectTab,
  showWorkspace,
  splitFocusedPane,
  stopWaiting,
  tabsIn,
  workspaceOf,
  type Direction,
  type FiledIn,
  type LastMoved,
  type Layout,
  type Pinned,
  type Tabs,
  type ViewRef,
} from "./tabs";
import { ChatState, type Asking } from "./NeedsYou";
import { EndingChat } from "./EndingChat";
import { Panels } from "./Panels";
import { NewVault } from "./NewVault";
import { OpenVault, useVaults } from "./Vaults";
import { usePlaneEdits } from "./PlaneEdits";
import { ViewMark, ViewPane } from "./Views";
import { useTabStop } from "./roving";
import { closeOnDelete, renameOnF2 } from "./tabKeys";
import { TabRename } from "./TabRename";
import { EmptyState } from "./EmptyState";
import type { ExtensionCommand, ExtensionView, PanelView, RowAction } from "./bindings";
import { AskFirst, runExtensionAction } from "./ExtensionAction";
import { extensionsChanged, useExtensionsOn } from "./extensionsOn";
import { projectThemeChanged } from "./projectTheme";
import { inForce, onDrawn, TINTED_TABS, tintVariables } from "./theme/theme";
import { hueOf } from "./theme/tint";
import { handedFromNote, type HandedFrom } from "./handedFrom";
import { movedAt, quietOnes, stateOf, useChatStates, type ChatStates } from "./chatState";
import { fitting, LEAST, leastAt, useRoom } from "./fits";
import { useArrived } from "./lib/arrived";
import type { Ending } from "./QuitWarning";
import { useTextSizes } from "./textSize";

/** One empty list, so a prop left out is the same list at every render. */
const NONE: readonly never[] = [];

/** Where the picker's chat goes: a new tab — started in `in` when a row asked for one
 *  directory for that tab alone (charter-app#174), else where the explorer's pick says — or a
 *  split of the pane in front. */
type Where = { tab: true; in?: string } | { split: Direction };

/**
 * One project, with everything that belongs to it.
 *
 * **This is the extraction ADR 0033 costed and #121 named as what it was leaving behind.** The
 * window used to BE the project: one `App` held the tabs, the sidebar, the panels, the chat
 * states, the picker and the palette, so a second project had nowhere to put any of them and
 * the app could draw exactly one. Here a project holds its own, and the window holds projects.
 *
 * **A project that is not in front keeps everything and draws nothing.** It stays mounted and
 * returns `null`: its tabs, its splits, its focused workspace and its picker are React state
 * that simply is not rendered, and its `useChatStates` goes on listening, so the project tab
 * can say a chat over there needs you. That is the operator's own reason for wanting one
 * window per project in the first place — fifty chats in project A must not be torn down
 * because he glanced at project B.
 *
 * It draws nothing rather than being hidden with CSS: a hidden
 * `[role="tablist"][aria-label="Tabs"]` is a second tab strip for every query in this app and
 * in the scenario tests to trip over, and a hidden pane is a terminal being fitted to a box
 * with no size.
 *
 * **The palette is the window's, not a project's**, for the same reason turned round. It has
 * to be mounted before the core has said which project this launch opened — `F2` is a
 * keystroke it listens for itself — and mounted once, because it claims that key on the
 * window with a capturing listener. So what it lists travels up from here with the rest of
 * this project's report, and the window draws the one palette.
 *
 * Its panes come and go with it, and that costs a project nothing: only the tab in front has
 * panes on screen anyway (`tabs.ts`), the core has held every session's terminal all along,
 * and a view opened again is sent the screen as it already is.
 */
export function PlaneView({
  plane,
  inFront,
  projects,
  pinnedProjects,
  window: windowDoes,
  onReport,
  alerts,
  contributed: surveyedPanels = NONE,
  views: surveyedViews = NONE,
  commands: surveyedCommands = NONE,
  settingsAsked,
  savingAsked,
  preferencesAsked,
}: {
  plane: PlaneId;
  /** Whether this is the project the operator is looking at. */
  inFront: boolean;
  /** Every project this window holds, for the rows that switch between them. */
  projects: readonly Project[];
  /** Which of them this operator has pinned, by root. The window holds it, because the
   *  project strip is the window's; this project's catalogue lists the rows. */
  pinnedProjects: readonly string[];
  /** What the WINDOW does, which this project asks for rather than doing itself: opening
   *  another project, switching to one, letting one go, and quitting. */
  window: WindowDoing;
  /** What this project has open and whether it has found out yet, for the window's quit
   *  warning and for this project's own tab. */
  onReport: (plane: PlaneId, report: PlaneReport) => void;
  /** The window's alerts drawer, for the status line's button. The window's and not this
   *  project's: alerts cross projects, so the count is every open project's. */
  alerts?: Alerts;
  /** What approved extensions contribute to the side region. The window's, for the same reason
   *  the alerts are: an extension is installed per machine and never travels in a plane
   *  (ADR 0041), so one survey serves every project this window holds — and this project keeps
   *  of it what it has on (ADR 0048). */
  contributed?: readonly PanelView[];
  /** The views approved extensions offer (ADR 0041 stage 2), the window's for the
   *  same reason: one survey per window, not one per project. */
  views?: readonly ExtensionView[];
  /** The palette commands approved extensions add (charter-app#341), the window's for the same
   *  reason, and filtered here by what this project has on, as the views are. */
  commands?: readonly ExtensionCommand[];
  /** A count that goes up each time the window is asked for THIS project's settings tab
   *  (`WindowDoing.openSettings`); `undefined` until it is. */
  settingsAsked?: number;
  /** The same, for THIS project's Saving tab (`WindowDoing.openSaving`, charter-app#294). */
  savingAsked?: number;
  /** The same, for the Preferences tab (`WindowDoing.openPreferences`, charter-app#283): a
   *  count that goes up each time the window asks for it on THIS project's strip. */
  preferencesAsked?: number;
}) {
  const [tabs, setTabs] = useState<Tabs>(noTabs);
  /** What every chat is doing, in THIS project. Pushed from the core; nothing here polls.
   *  It keeps listening while the project is behind another one, which is what lets its tab
   *  say that something over there needs you. */
  const states = useChatStates(plane);
  const [trouble, setTrouble] = useState<string>();
  const [sidebar, setSidebar] = useState<SidebarModel>();
  /** This project's save standing (charter-app#302): every project reads its own, so the project
   *  strip can mark the ones with unsaved work and the title bar can show the one in front. */
  const { saving } = usePlaneSaving(plane);
  /** The workspace whose LIVE/LOCAL confirmation is open (charter-app#301). */
  const [liveAsk, setLiveAsk] = useState<string>();
  /**
   * The workspace the operator last PICKED, which is not always the one drawn.
   *
   * `focused` below is the one drawn, and it is derived from this and from the tab in front —
   * see it for why. This is only half the answer, so nothing outside those few lines reads
   * it: a workspace with no chats has nothing in front to be derived from, and this is what
   * keeps the window on it.
   */
  const [picked, setPicked] = useState<string>();
  /** The chats the core put back at this launch, so a pane can say which came back how. */
  const [reopened, setReopened] = useState<OpenChat[]>([]);
  /** The picker, when a new chat has been asked for, and what to do with the session it
   *  starts. A harness starts only once a row in it is picked: nothing is opened until then,
   *  so cancelling leaves nothing to tear down. Both a new tab and a split come through
   *  here, because both start a harness and ADR 0022 admits no path that does not pick. */
  const [picking, setPicking] = useState<{
    options: StartOptions;
    where: Where;
  }>();
  /** Why the last start did not happen, shown in the picker rather than behind it. */
  const [pickerTrouble, setPickerTrouble] = useState<string>();
  /** The chat tab whose name is open for editing on the strip, if one is (charter-app#254). */
  const [renaming, setRenaming] = useState<number>();
  /** Chats this launch could not start, by name and why. They are still recorded. */
  const [wouldNotStart, setWouldNotStart] = useState<[string, string][]>([]);
  /** What the core last said about where a chat is working, and which directory it was
   *  asked about — so an answer about the chat that WAS in front is never drawn under the
   *  one that is now. `workspaceState` keys its answers the same way, for the same reason. */
  const [located, setLocated] = useState<{ cwd: string; piece?: ChatWorktree }>();
  /** Bumped when something changed the answer, so it is asked again rather than guessed. */
  const [relocate, setRelocate] = useState(0);
  /**
   * Bumped when THIS window changed which workspaces the plane has.
   *
   * The sidebar is re-read whenever the chats change, because opening or ending a chat is what
   * this window could previously change about the answer. Making and deleting a workspace
   * changes it without touching a chat, so they say so — the plane is still the truth and it is
   * read again, rather than the window editing its own copy of what it thinks is there.
   */
  const [replan, setReplan] = useState(0);
  /**
   * Asked again when a worktree row this window ran changed what the focused workspace holds.
   *
   * `replan` re-reads the SIDEBAR, which is workspaces and chats; this re-reads the one
   * workspace record the three regions share, which is where the clones, their git state and
   * their pieces live. They are two asks and a worktree removal invalidates only the second —
   * the piece is off the tree and `git status` in that clone now says something else
   * (charter-app#174).
   */
  const [rereadWorkspace, setRereadWorkspace] = useState(0);
  /** Bumped when the core says this plane changed on disk (charter-app#264): a todo closed in
   *  a terminal, a workspace another chat made. The sidebar and the focused workspace's panels
   *  are read again on it. */
  const changesOnDisk = usePlaneChanged([plane]);
  /** The chats running on instructions the plane has changed since they started (charter#369),
   *  each marked on its tab. */
  const planeUpdates = usePlaneUpdated(plane, changesOnDisk);
  // The plane root is watched, so an edit to `charter.toml` or `charter.local.toml` — in an
  // editor, from a `git pull` — is one of these, and what this project has on may have moved
  // with it (charter-app#253). Not at the mount: `useExtensionsOn` asks then.
  useEffect(() => {
    if (changesOnDisk === 0) return;
    extensionsChanged(plane);
    // And the theme it draws, which the same two files and each `workspace.json` pick
    // (charter-app#273, #281).
    projectThemeChanged(plane);
  }, [changesOnDisk, plane]);
  /** Whether the new-workspace dialog is up, why the last attempt made nothing, and whether
   *  charter is making one right now. */
  const [makingWorkspace, setMakingWorkspace] = useState(false);
  const [workspaceTrouble, setWorkspaceTrouble] = useState<string>();
  const [busyMaking, setBusyMaking] = useState(false);
  /** The plane's vaults (`vault_list`), read here because four surfaces answer from them: the
   *  Vaults panel, the palette's `vault.open:<name>` rows, the picker, and — by asking for it
   *  again after a write — a vault's own tab (charter-app#235). */
  const vaults = useVaults(plane);
  const vaultNames = useMemo(() => vaults.vaults?.map((one) => one.name), [vaults.vaults]);
  const reloadVaults = vaults.reload;
  /** Whether the vault picker is up. */
  const [pickingVault, setPickingVault] = useState(false);
  /** Whether the new-vault dialog is up, why the last attempt made nothing, and whether
   *  charter is making one right now — `makingWorkspace`'s three, for a vault. */
  const [makingVault, setMakingVault] = useState(false);
  const [vaultTrouble, setVaultTrouble] = useState<string>();
  const [busyVault, setBusyVault] = useState(false);
  /** A palette command's action that asks first, waiting on the operator's answer
   *  (charter-app#341). */
  const [askingAction, setAskingAction] = useState<{ extension: string; action: RowAction }>();
  /**
   * The workspace the operator is being asked about deleting, if any.
   *
   * `atRisk` is `undefined` until the core has answered and is drawn as "still reading" — an
   * empty list and an unanswered question are the two states this must never merge, because
   * one of them says "nothing would be lost". `refusal` is what the core gave the last time
   * Delete was pressed, and its presence is the only thing that makes forcing reachable.
   *
   * **`refusal` is the whole `Refused` and not its sentence** (charter-app#182). It carries the
   * at-risk list the core read *inside* the delete, and that list — never `atRisk`, which is
   * the older reading taken when this dialog opened — is what the force button is drawn from
   * once there is one.
   */
  const [removing, setRemoving] = useState<{
    workspace: string;
    atRisk?: AtRisk[];
    unreadable?: string;
    refusal?: Refused;
    busy: boolean;
  }>();
  /** The workspace being renamed, while its dialog is up: whether charter is renaming it now,
   *  and why the last attempt renamed nothing, in the core's words (charter#367). */
  const [renamingWs, setRenamingWs] = useState<{
    workspace: string;
    busy: boolean;
    trouble?: string;
    /** The core's sentence naming the chats that will start a fresh conversation after it,
     *  `null` for none, `undefined` until the core has answered. */
    startsFresh?: string | null;
  }>();
  /** The same, for the pins: a pin is written by the core, so the window asks what the core
   *  now says rather than assuming its own write landed as it expected. */
  const [pinning, setPinning] = useState(0);
  /** What the last action answered: one line, or a refusal in the words it came in. The
   *  window draws it, beside the palette that shares it. */
  const [report, setReport] = useState<{ from: string; refused: boolean; words: string }>();
  /** Whether the core has answered what this project already has open. Until it has, "no
   *  tabs" is "not yet", which is not the same thing as "nothing is running" — and a quit
   *  decides on it. */
  const [settled, setSettled] = useState(false);
  /**
   * Where charter has just put a chat, until the plane says the same thing.
   *
   * The plane is what files a chat under a workspace — the directory it works in — and the
   * sidebar is read fresh off the disk a tick after a chat starts. For that one tick charter
   * knows perfectly well where it put the chat, because it chose the directory. Without this
   * the tab would be missing from its own strip for a frame, on the strip the operator is
   * looking at, which is the tab going missing.
   *
   * Written in the same handler that starts the chat, so the render that first draws the tab
   * already knows where it is filed — React puts both in one flush.
   *
   * **Nothing prunes it, and that is safe because a session number is never reused** — which
   * #133 asked to be verified rather than assumed, since a reused number would hand a dead
   * chat's workspace to a live one. It was verified in the core, and it rests on three
   * things together, not on the counter alone:
   *
   * - `Sessions::open` takes the number from `opened.fetch_add(1)`, an `AtomicU32` that is
   *   only ever incremented and never stored to. There is no reset path.
   * - That counter belongs to the plane's `Held`, which `Planes::open` mints once per plane
   *   and `Planes::close` removes whole. Re-opening a plane makes a new one counting from 1.
   * - And this component is mounted `key={plane}`, so a plane that was closed and opened
   *   again is a new `PlaneView` with an empty map. The counter and the map restart together.
   *
   * So the map only grows with chats STARTED from this window in one project's lifetime — an
   * entry is a number and a workspace name — and no entry it holds can ever be asked about
   * by a different chat.
   */
  const [startedIn, setStartedIn] = useState<Record<number, string>>({});
  /**
   * Where each handed-off chat came from, by session, as its tab's tooltip and its pane say it:
   * `↳ from steward 3 · platform-next` (charter-app#258). By the parent's name, never its
   * number. A chat no handoff opened has none.
   */
  const [handedFrom, setHandedFrom] = useState<Record<number, string>>({});
  /** The tab that was in front on each workspace's strip, so coming back to a workspace
   *  comes back to the chat that was on screen there rather than to its first. */
  const lastFront = useRef<Record<string, number>>({});
  /**
   * What this operator has pinned in this project (ADR 0039).
   *
   * **Two states and not one, because they are two stores** (ADR 0040). The workspaces come
   * from the machine store, which is where an arrangement of things the store already names
   * belongs; the chats come from the plane's own `.charter/app/reopen.json`, because ADR 0034
   * forbids a chat's name outside a plane. A design that held one "pins" object would be the
   * design ADR 0039 predicted would discover this in review.
   */
  const [pinnedWorkspaces, setPinnedWorkspaces] = useState<string[]>([]);
  /** Pins that no longer name a workspace on the plane, said rather than drawn. */
  const [danglingPins, setDanglingPins] = useState<string[]>([]);
  /** The chats this operator has pinned, by session. Seeded from the record the core put
   *  back, and kept current by the one handler that writes it. */
  const [pinnedChats, setPinnedChats] = useState<number[]>([]);
  /**
   * The view tabs this operator has pinned, by `tabs.viewKey`. A tab with no chat is pinned as
   * the view it opened on, and the pin rides that view tab's line of the plane's record — the
   * same file and the same reason as a chat's pin (ADR 0040).
   */
  const [pinnedViews, setPinnedViews] = useState<string[]>([]);
  /**
   * Whether the core has said which view tabs the record put back. Until it has, the window
   * says nothing about its own: an empty list sent first would be written over the record
   * before it was read.
   */
  const [viewsHeard, setViewsHeard] = useState(false);
  /**
   * The spot the explorer has picked, and the workspace it was picked in.
   *
   * Both together, so that moving to another workspace goes back to that workspace's own
   * directory rather than leaving the next chat pointed at a piece of the workspace the
   * operator has just left. Derived below rather than cleared in an effect: a `setState`
   * from inside an effect is a second render, and the answer is already here.
   */
  const [pickedSpot, setPickedSpot] = useState<{ workspace: string; spot: Spot }>();
  /**
   * The panel row whose card is open on the right-hand region, if any, as
   * `<panel key>/<row key>` — `charter/personas/steward`, `charter/todos/<slug>`,
   * `ext/acme/reviews/<key>`.
   *
   * **Held here rather than in the row that draws it**, for charter-app#174's reason: a persona's
   * card was a catalogue row the palette ran. A persona opens its own TAB now (`showView`,
   * the operator's ruling of 2026-09-23), so what this still holds is the popover card of a row
   * that has one — a todo's, a contributed row's.
   *
   * **It was `shownPersona` and is a row key now**, because the panels are contributions and
   * every contributed panel's rows open the same way. A second piece of window state per panel
   * would be the special case the contract exists to remove — and it could not exist at all for
   * a panel nobody has written yet.
   */
  const [shownRow, setShownRow] = useState<string>();
  /** How the window is laid out — which regions are drawn, on which side, in what order and
   *  how big (ADR 0038). Data rather than the shape of the JSX below; `regions.ts` says why. */
  const { arrangement, toggle: toggleRegion, resized } = useArrangement();
  /** The arrangement as the slots it draws, which is what both the toggles and the frame read. */
  const slots = inSlots(arrangement);

  // The arrangement as it is right now, so that what a button does is decided here and not
  // inside a state update. React may run an update again, and a session must not be opened or
  // ended twice because it did.
  const now = useRef(tabs);
  const change = useCallback(
    (how: (tabs: Tabs) => Tabs): Tabs => {
      const next = how(now.current);
      const wasInFront = now.current.inFront;
      now.current = next;
      setTabs(next);
      // The core records which chat was in front, so it is told whenever that changes — and
      // only then, rather than on every split and every keystroke. The plane travels with it:
      // "chat 3 is in front" belongs to a plane, and every plane numbers its chats from one.
      if (next.inFront !== wasInFront) {
        // A tab showing a view has no chat of its own, so nothing is in front as far as the
        // record of chats is concerned; the view tab says it is in front itself (`windowViews`).
        const chat = next.inFront === undefined ? undefined : chatOf(next, next.inFront);
        void commands.chatInFront(plane, chat ?? null).catch(() => undefined);
      }
      return next;
    },
    [plane],
  );

  // What the core already has open in this project, which at a launch is the record put back
  // before there was a window. The window draws them; it never ends them — a reload during
  // development, or a crash in the window, would otherwise take the day's sessions with it.
  //
  // Once per project, which a ref is what makes true: React runs an effect twice in
  // development, and a second pass would draw every chat again. The ref is this component's,
  // so a project that is closed and opened again is a fresh one and asks again — which it
  // must, because the core still holds whatever it put back.
  const adopted = useRef(false);
  useEffect(() => {
    if (adopted.current) return;
    adopted.current = true;
    void Promise.all([
      commands.openedChats(plane).catch(() => undefined),
      // The view tabs the record put back, beside the chats. **Asked for, never started**: a
      // view has no program of charter's, and an extension's view is not asked anything until
      // the operator presses for it (`Views.tsx`), so this is a list of tabs and nothing else.
      commands.reopenedViews(plane).catch(() => undefined),
    ])
      .then(([answer, viewAnswer]) => {
        setSettled(true);
        void commands
          // A window that cannot ask, or is answered with nothing, simply says nothing.
          .chatsThatWouldNotStart(plane)
          .then((trouble) => setWouldNotStart(trouble.status === "ok" ? (trouble.data ?? []) : []))
          .catch(() => undefined);
        const open = answer?.status === "ok" ? (answer.data ?? []) : [];
        const back = viewAnswer?.status === "ok" ? (viewAnswer.data ?? []) : [];
        if (open.length > 0) {
          setReopened(open);
          setHandedFrom((was) => ({
            ...was,
            ...Object.fromEntries(
              open.flatMap((chat) => {
                const note = handedFromNote(chat.from);
                return note ? [[chat.session, note]] : [];
              }),
            ),
          }));
          // A pinned chat comes back pinned: the pin rides the record it came back from.
          setPinnedChats(open.filter((chat) => chat.pinned).map((chat) => chat.session));
        }
        setPinnedViews(back.filter((view) => view.pinned).map((view) => viewKey(refOf(view))));
        // The persona comes with the chat, so a tab put back reads `steward 3` from its first
        // frame rather than reading `3` until the sidebar has been read (charter-app#130) — and
        // so does the name the operator gave it, which rides the record (charter-app#254).
        const chats = open.reduce(
          (tabs, chat) =>
            openTab(tabs, chat.session, chat.name, whoOf(chat.persona, chat.harness), chat.label),
          noTabs(),
        );
        // Each view at the place it had, in the order of those places, so a view recorded at 2
        // lands at 2 after the one at 1 is already in.
        const drawn = [...back]
          .sort((one, other) => one.at - other.at)
          .reduce(
            (tabs, view) =>
              putViewBack(tabs, refOf(view), view.title, view.workspace ?? OUTSIDE, view.at),
            chats,
          );
        const front = open.find((chat) => chat.in_front);
        const frontView = back.find((view) => view.active);
        const inFront =
          front !== undefined
            ? drawn.order.find((id) => chatOf(drawn, id) === front.session)
            : frontView !== undefined
              ? drawn.order.find((id) => {
                  const lead = contentsOf(drawn, id)[0]?.content;
                  return lead?.kind === "view" && viewKey(lead.view) === viewKey(refOf(frontView));
                })
              : undefined;
        if (drawn.order.length > 0)
          change(() => (inFront === undefined ? drawn : selectTab(drawn, inFront)));
        // Only now may the window say what view tabs it has: saying it before this point would
        // write an empty list over the record it is about to read.
        setViewsHeard(true);
      })
      // Nothing open is the ordinary first launch, and a window that cannot ask is still
      // a window the operator can open a chat in.
      .catch(() => {
        setSettled(true);
        setViewsHeard(true);
      });
  }, [change, plane]);

  /**
   * The window's view tabs, told to the core whenever they change — so the record brings them
   * back at the next launch, as it brings back chats (ADR 0043, as amended).
   *
   * **The whole list, and only when it differs from what was last said.** The core writes the
   * record when what it holds changes and not otherwise (`Chats::hold_views`), and this keeps a
   * chat's keystrokes and splits from being a command each.
   */
  const lastViewsSaid = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (!viewsHeard) return;
    const said = viewTabsOf(tabs, pinnedViews);
    const text = JSON.stringify(said);
    if (text === lastViewsSaid.current) return;
    lastViewsSaid.current = text;
    void commands.windowViews(plane, said).catch(() => undefined);
  }, [pinnedViews, plane, tabs, viewsHeard]);

  /**
   * The order the chats are in across every strip, told to the core whenever it changes — so
   * the record lists them in it, and the next launch and a reloaded window put them back in it
   * (SI-6). A tab's chats in its panes' order, each once.
   *
   * **After the record is heard, and only when it differs**, for the view tabs' two reasons
   * above: an order said before the adoption would be the empty strip's, and most changes to
   * `tabs` are not to the order.
   */
  const lastOrderSaid = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (!viewsHeard) return;
    const sessions = [
      ...new Set(tabs.order.flatMap((id) => panesOf(tabs, id).map((pane) => pane.session))),
    ];
    const text = JSON.stringify(sessions);
    if (text === lastOrderSaid.current) return;
    lastOrderSaid.current = text;
    void commands.chatOrder(plane, sessions).catch(() => undefined);
  }, [plane, tabs, viewsHeard]);

  // A chat a handoff opened (charter-app#204): the core has started it, and this puts it on
  // its workspace's strip. **Behind whatever is in front** (`openTabBehind`), and the window is
  // not raised: the handoff was work sent away from the chat on screen, and the tab on the
  // strip, whose first message says which chat it came from, is how it is seen.
  useEffect(() => {
    const listening = listen<Arrived>("handoff-arrived", (event) => {
      const arrived = event.payload;
      if (arrived.plane !== plane) return;
      // Already drawn: the adoption above can race the event and draw it first.
      if (alreadyShows(now.current, arrived.session)) return;
      setStartedIn((was) => ({ ...was, [arrived.session]: arrived.workspace }));
      const note = handedFromNote(arrived.from);
      if (note) setHandedFrom((was) => ({ ...was, [arrived.session]: note }));
      // Named for its task where the handoff named one, and `<persona> <N>` where it did not
      // (charter-app#258).
      change((tabs) =>
        openTabBehind(
          tabs,
          arrived.session,
          arrived.name,
          whoOf(arrived.persona, arrived.harness),
          arrived.label ?? null,
        ),
      );
    }).catch(() => undefined);
    return () => void listening.then((stop) => stop?.()).catch(() => undefined);
  }, [change, plane]);

  // The sidebar is read from the plane, and re-read whenever the chats change: the plane is a
  // directory the operator also edits by hand and another charter process writes, so there is
  // nothing to invalidate a cache of it. `tabs` is the dependency because opening or ending a
  // chat is what this window can change about the answer, and `changesOnDisk` because the core
  // says when something else changed it (charter-app#264).
  useEffect(() => {
    void commands
      .planeSidebar(plane)
      .then((answer) => {
        // Only an `ok` answer WITH a body is used. Both halves are load-bearing: the
        // state updater below runs on the NEXT render, outside this promise, so nothing
        // here catches a throw from it — and an answer whose `data` is absent reads as
        // `ok` all the same. The window must not go blank because one command answered
        // oddly; it has its panes to draw.
        const next = answer.status === "ok" ? answer.data : undefined;
        if (!next?.workspaces) {
          setSidebar(undefined);
          return;
        }
        setSidebar(next);
        // Focus follows the plane rather than being guessed: whichever workspace was picked,
        // while it still exists — and otherwise **the workspace of the chat in front**,
        // because the strip below shows that workspace's chats and a strip that opened on
        // another one would be hiding the chat the operator is looking at (ADR 0036). Only
        // then the first workspace, which is what a launch with nothing open lands on.
        const here = (session: number) =>
          next.workspaces.find((ws) => ws.chats.some((chat) => chat.session === session))?.name ??
          (next.unfiled.some((chat) => chat.session === session)
            ? OUTSIDE
            : (startedIn[session] ?? OUTSIDE));
        const front = now.current.inFront;
        const lead = front === undefined ? undefined : contentsOf(now.current, front)[0]?.content;
        const ofFront =
          lead === undefined
            ? undefined
            : lead.kind === "session"
              ? here(lead.session)
              : lead.workspace;
        const stands = (name: string) =>
          name === OUTSIDE
            ? next.unfiled.length > 0 || ofFront === OUTSIDE
            : next.workspaces.some((ws) => ws.name === name);
        setPicked((current) =>
          current !== undefined && stands(current)
            ? current
            : (ofFront ?? next.workspaces[0]?.name),
        );
        // A view tab is on the strip it was opened from, which it carries; when the plane stops
        // having that workspace nothing else would move it, and it would be on a strip that is
        // never drawn. It goes where a chat working in no workspace goes.
        change((tabs) =>
          refileViews(tabs, (name) => next.workspaces.some((ws) => ws.name === name), OUTSIDE),
        );
      })
      // A window with no readable plane still runs its panes; the header already says so.
      .catch(() => setSidebar(undefined));
  }, [change, changesOnDisk, plane, replan, startedIn, tabs]);

  /**
   * What the machine store says this operator has pinned here, and what it says is gone.
   *
   * Asked again whenever the plane is read again, for the same reason the sidebar is: which
   * workspaces exist is the plane's answer, and a pin that no longer names one has to stop
   * being drawn the moment the plane stops having it. `pinning` is bumped by a pin, so the
   * answer is the store's rather than this window's guess about the store.
   */
  useEffect(() => {
    let gone = false;
    void commands
      .planePins(plane)
      .then((answer) => {
        if (gone || answer.status !== "ok") return;
        // **Held to the shape, not merely to `ok`** — the same rule the sidebar's read
        // states. An `ok` answer with no body, or with a body of another shape, would put
        // `undefined` where a list belongs and take the strip down on the next render. A
        // window must not go blank because one command answered oddly.
        const said = answer.data as Partial<typeof answer.data> | null | undefined;
        setPinnedWorkspaces(Array.isArray(said?.workspaces) ? said.workspaces : []);
        setDanglingPins(Array.isArray(said?.missing) ? said.missing : []);
      })
      // A window that cannot ask draws nothing pinned: the workspace strip holds the one you
      // are in, and the rest are behind its show-more — the arrangement an operator who has
      // pinned nothing already has (ADR 0054).
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [pinning, plane, sidebar]);

  /**
   * Which workspace each chat is filed under — the strip its tab appears on.
   *
   * **The plane's answer**, through the sidebar the core reads off it: what relates a chat to
   * a workspace is the directory it works in, and nothing on the plane records a chat. What
   * charter has just started is laid under that, for the one tick before the plane has been
   * read again.
   *
   * A chat working in no workspace is `OUTSIDE`, which is a strip of its own. The sidebar has
   * always shown those rather than dropping them, and a strip per workspace has to have
   * somewhere to put them or they become unreachable.
   */
  const filedIn = useCallback<FiledIn>(
    (session) => {
      const workspace = sidebar?.workspaces.find((ws) =>
        ws.chats.some((chat) => chat.session === session),
      );
      if (workspace) return workspace.name;
      if (sidebar?.unfiled.some((chat) => chat.session === session)) return OUTSIDE;
      return startedIn[session] ?? OUTSIDE;
    },
    [sidebar, startedIn],
  );

  /** Whether a tab is pinned: its own chat is, which is its first pane's. */
  const isPinned = useCallback<Pinned>(
    (id) => {
      const lead = contentsOf(tabs, id)[0]?.content;
      if (lead === undefined) return false;
      return lead.kind === "session"
        ? pinnedChats.includes(lead.session)
        : pinnedViews.includes(viewKey(lead.view));
    },
    [pinnedChats, pinnedViews, tabs],
  );

  /**
   * The workspace the strip DRAWS, and the one axis rule: **the tab in front is on it.**
   *
   * Derived rather than maintained. Until #133 this held because `bringToFront`,
   * `focusWorkspace`, `closeTab`, `openTab` and the sidebar-read's focus rule each kept it —
   * five handlers agreeing, with no single place that re-establishes it, so a sixth that
   * forgot would break the axis silently and a reviewer of #131 said exactly that.
   *
   * It was already broken without a sixth handler. **Nothing on the plane records a chat**:
   * which workspace it is in is decided by the directory it works in, so a workspace added on
   * disk moves chats between strips with no handler involved at all. The window then drew the
   * strip the last handler had left it on, with the front chat's pane under a strip that did
   * not list it.
   *
   * So the front tab decides, and `picked` answers only when there is no front tab — which is
   * the workspace holding no chats, the one case the axis cannot be read off a tab.
   */
  const focused = useMemo(() => {
    if (sidebar === undefined || tabs.inFront === undefined) return picked;
    return workspaceOf(tabs, tabs.inFront, filedIn) ?? picked;
  }, [filedIn, picked, sidebar, tabs]);

  /** The workspace whose repos and worktrees the three regions read. The strip for chats
   *  outside every workspace is not a workspace on the plane, so there is no directory to
   *  read and every region says so rather than drawing another workspace's answer. */
  const ofWorkspace = focused === OUTSIDE ? undefined : focused;
  /**
   * **What this project has on in the focused workspace** (charter-app#253, #280, ADR 0048): the
   * core's answer for this plane's two files and the workspace's `workspace.json`, over this
   * machine's approvals. The window's survey is filtered by it here, once, so the side region,
   * the view buttons and the palette's view rows all read one list. Charter's own panels (`from`
   * null) are not an extension's and are never filtered.
   */
  const on = useExtensionsOn(plane, ofWorkspace);
  const contributed = useMemo(
    () => surveyedPanels.filter((panel) => panel.from === null || (on?.has(panel.from) ?? false)),
    [on, surveyedPanels],
  );
  const views = useMemo(
    () => surveyedViews.filter((view) => on?.has(view.extension) ?? false),
    [on, surveyedViews],
  );
  const extensionCommands = useMemo(
    () => surveyedCommands.filter((command) => on?.has(command.extension) ?? false),
    [on, surveyedCommands],
  );
  const workspaceState = useWorkspaceState(plane, ofWorkspace, rereadWorkspace, changesOnDisk);
  /** The badges and repo columns the extensions on here show (charter-app#340). */
  const facts = useExtensionFacts(plane, ofWorkspace);
  /** What `charter doctor` says about this project, run inside the app: the preflight when
   *  the project opens, the full doctor when the operator opens it (`Doctor.tsx`). */
  const doctor = useDoctor(plane);
  /**
   * This plane's pin (`Updates.tsx`).
   *
   * **The updater used to be here beside it and is not any more.** An offer is a fact about
   * the app, and this component is one project of as many as the window holds — so
   * `useUpdates` here was one updater client, three event listeners and an `update_channel`
   * call PER OPEN PROJECT, all reporting the same thing. It is called once in `App` now and
   * drawn once, on the title bar, which is the window's own chrome.
   *
   * The pin stays, because it is the opposite kind of fact: `charter version`'s verdict about
   * THIS plane's `[charter] version` (ADR 0030). It belongs beside the project it is
   * about, and two open projects can honestly disagree about it.
   */
  const pin = usePin(plane);

  /**
   * The piece the explorer has picked, when it is still a piece of the workspace on screen.
   *
   * Two things can make a pick stop standing, and they are different. Moving to another
   * workspace only SETS IT ASIDE — coming back brings it with you, the way coming back to a
   * workspace comes back to the tab that was in front there. A piece that is gone from the
   * listing is another matter: the worktree was removed while it was picked, and pointing the
   * next chat at a directory that is not there would make the operator read a refusal charter
   * could see coming. A listing that has not arrived yet is not evidence of either, so the
   * pick stands until git has answered.
   */
  const spot = useMemo(() => {
    if (pickedSpot === undefined || pickedSpot.workspace !== ofWorkspace) return undefined;
    const { repo, piece } = pickedSpot.spot;
    // A picked CLONE (charter-app#174) stops standing the same way, one level up: when the
    // plane is read again and the clone is not in it.
    if (piece === undefined) {
      const clones = workspaceState.panels?.repos;
      return clones !== undefined && !clones.includes(repo) ? undefined : pickedSpot.spot;
    }
    const listed = workspaceState.pieces[repo];
    if (listed !== undefined && !listed.some((one) => one.piece === piece)) return undefined;
    return pickedSpot.spot;
  }, [ofWorkspace, pickedSpot, workspaceState.panels, workspaceState.pieces]);

  // Where a chat starts: **the spot the explorer picked**, and the focused workspace's own
  // directory when nothing is picked — so the sidebar can file it under that workspace.
  // Nothing on the plane records a chat, so where it works is the only thing relating the
  // two, and a piece of a workspace is still in that workspace. Null when there is neither,
  // and the core starts that chat in the plane's own directory.
  const startIn = spot?.path ?? sidebar?.workspaces.find((ws) => ws.name === focused)?.path ?? null;

  /** What the explorer picks, remembered against the workspace it was picked in. */
  const pickSpot = useCallback(
    (next: Spot | undefined) => {
      if (ofWorkspace === undefined) return;
      setPickedSpot(next === undefined ? undefined : { workspace: ofWorkspace, spot: next });
    },
    [ofWorkspace],
  );

  /** Every workspace the window can bring forward: this project's, plus the one for chats
   *  outside them all when there are any: the pinned ones first, in the order they were
   *  pinned in, then the rest in the plane's own order, which is the sidebar's.
   *  The palette lists all of them; the strip draws fewer (`onWorkspaceStrip`, below). */
  const strips = useMemo(() => {
    if (sidebar === undefined) return [];
    const names = sidebar.workspaces.map((ws) => ws.name);
    const stray =
      sidebar.unfiled.length > 0 ||
      tabs.order.some((id) => workspaceOf(tabs, id, filedIn) === OUTSIDE);
    const all = stray ? [...names, OUTSIDE] : names;
    // **Pinned first, in the order they were pinned in, then the rest in the plane's own
    // order** (ADR 0039, ADR 0054, charter#402). The store answers the pins in that order, and
    // only the ones the plane still has.
    return [
      ...pinnedWorkspaces.filter((name) => all.includes(name)),
      ...all.filter((name) => !pinnedWorkspaces.includes(name)),
    ];
  }, [filedIn, pinnedWorkspaces, sidebar, tabs]);

  /**
   * What the workspace strip draws: **the pinned workspaces, and the one you are in** (ADR
   * 0054). Everything else is behind its show-more button.
   *
   * An operator with many workspaces works in about three, and filling the width with whatever
   * fits put the workspaces nobody was working in beside the three that mattered. So the pins
   * are the strip, in the order `strips` holds them, and the workspace in front is drawn after
   * them when it is not one of them — ADR 0039's "the selected tab is always drawn", which is
   * what says where you are — and goes back behind show-more when you leave it. Nothing else
   * earns a tab: a workspace that needs you is counted on the show-more button, never moved
   * onto the strip under the operator's hand.
   */
  const onWorkspaceStrip = useMemo(
    () => strips.filter((name) => pinnedWorkspaces.includes(name) || name === focused),
    [focused, pinnedWorkspaces, strips],
  );

  /** And which of those the strip has room to draw. The same rule as the chats' one level
   *  down: when the pins alone do not fit, what does not fit goes behind show-more too, so the
   *  strip never scrolls and never loses its `+`. */
  const { strip: workspaceStrip, width: workspaceRoom } = useRoom(onWorkspaceStrip.length);
  // The floors grow with the window's text (charter-app#283, `fits.leastAt`).
  const windowText = useTextSizes().window;
  const workspaceLeast = leastAt(LEAST.workspace, windowText);
  const chatLeast = leastAt(LEAST.chat, windowText);
  const workspacesShown = useMemo(() => {
    const { shown } = fitting(onWorkspaceStrip, focused, workspaceRoom, workspaceLeast);
    return { shown, hidden: strips.filter((name) => !shown.includes(name)) };
  }, [focused, onWorkspaceStrip, strips, workspaceRoom, workspaceLeast]);

  /**
   * **Each workspace's colour** (charter-app#281), as the core read it out of its
   * `workspace.json` with the sidebar — so it is read again whenever the plane changes on disk,
   * a save in the Workspace settings tab included. `null` for a workspace with none, for the
   * chats outside every workspace, which have no file to hold one, and for a grey `#rrggbb`,
   * which has no hue to tint with: no mark is drawn for a colour that tints nothing (the
   * Workspace settings tab says why).
   */
  const colourOf = (workspace: string | undefined): string | null =>
    colourWithHue(sidebar?.workspaces.find((ws) => ws.name === workspace)?.colour);
  /** The theme the window draws, which a colour is a hue shift of: a tab's tint follows it. */
  const drawnTheme = useSyncExternalStore(followTheme, inForce);
  /** What a workspace's own tab and its chat strip put on themselves: its colour, on the tab
   *  shades and the accent (`theme.TINTED_TABS`). Nothing for a workspace with no colour. */
  const tintOf = (workspace: string | undefined) =>
    tintVariables(drawnTheme, colourOf(workspace), TINTED_TABS) as CSSProperties;

  /**
   * What a workspace is drawn as: its name, its pin, and the two counts.
   *
   * **One definition, used by the strip and by the menu of what the strip is not drawing.**
   * They are the same workspace and a second copy of the markup is a second answer — the
   * rule the catalogue already follows for words, applied to marks.
   *
   * Counted here in the render body, once per workspace, and DELIBERATELY not memoised. It
   * looks quadratic and it is — ten workspaces × fifty tabs × a scan of the sidebar — so
   * charter-app#133 measured it at the limits before touching it: **0.022 ms at ADR 0026's
   * ten workspaces and fifty chats**, against a 16.7 ms frame, and half a percent of the
   * re-render it sits in. A memo over `tabs` would save that 22 µs on the one event it was
   * proposed for — `chat-moved` changes neither `tabs` nor `sidebar`, so the memo would hit
   * every time — and be paid for on every event that does change them. The measurement is
   * kept as assertions in `tabs.test.ts`, "the workspace strip at fifty chats", where a
   * third nested scan fails a test instead of being a surprise.
   */
  /** Whether `workspace` is LIVE, as the plane was last read (charter-app#301). */
  const liveOf = useCallback(
    (workspace: string) =>
      sidebar?.workspaces.some((ws) => ws.name === workspace && ws.live) ?? false,
    [sidebar],
  );
  const liveNames = useMemo(
    () => (sidebar?.workspaces ?? []).filter((ws) => ws.live).map((ws) => ws.name),
    [sidebar],
  );

  /** How many chats in `workspace` need you: its tab's count, and its share of the count on
   *  the show-more button when the strip is not drawing it (ADR 0054). One reading of the
   *  queue for both, so the button goes down exactly when the tab would. */
  const waitingIn = (workspace: string) =>
    states.needsYou.filter((session) => filedIn(session) === workspace).length;

  const workspaceMarks = (workspace: string) => {
    const waiting = waitingIn(workspace);
    const here = tabsIn(tabs, workspace, filedIn).length;
    const called = workspace === OUTSIDE ? OUTSIDE_TITLE : workspace;
    const colour = colourOf(workspace);
    return (
      <>
        {/* Its colour, as a mark in its own accent (charter-app#281) — on the strip and in the
            menu of what the strip is not drawing, which is why it is here and not a style of
            the tab alone. Hidden from a screen reader: the name says which workspace. */}
        {colour !== null && (
          <span className="workspace-mark" aria-hidden="true" style={tintOf(workspace)} />
        )}
        <span className="workspace-name">{called}</span>
        {/* LIVE, said: published with the plane (charter-app#301). LOCAL is the default and
            draws nothing. */}
        {liveOf(workspace) && <LiveMark />}
        <Pin held={pinnedWorkspaces.includes(workspace)} what="workspace" />
        {/* How many chats are open over there. With the strip below showing one workspace's
            chats, this is the answer to "where are the other forty". */}
        {here > 0 && (
          <span className="workspace-count" aria-label={`${here} chats`}>
            {here}
          </span>
        )}
        {/* And how many of them are asking for you. Scoping the chats to a workspace would
            otherwise hide a chat that needs you behind a strip nobody is looking at — the
            same hole the project tabs close one scope up. */}
        {waiting > 0 && (
          <span className="workspace-needs" aria-label={`${waiting} chats need you in ${called}`}>
            {waiting}
          </span>
        )}
      </>
    );
  };

  /**
   * The chats the strip shows: the focused workspace's.
   *
   * **Every one of them while charter has not read the plane yet.** With no sidebar there is
   * nothing that knows which workspace a chat is in, and a strip that showed none of them
   * would be hiding chats that are running — which is worse than a strip that shows them all
   * for the moment before the answer arrives.
   */
  const onStrip = useMemo(
    () => (sidebar === undefined ? tabs.order : tabsIn(tabs, focused, filedIn, isPinned)),
    [filedIn, focused, isPinned, sidebar, tabs],
  );

  /**
   * How much room the chat strip has, and therefore which of its tabs it draws.
   *
   * **What does not fit is not drawn** — the operator's call, reversing what ADR 0039 left
   * open. `fits.ts` holds the whole of why the answer is arithmetic over one measured width
   * rather than an intersection measurement over fifty tabs, and what it costs.
   *
   * The `+` and the show-more button are siblings of this tablist rather than children of
   * it, so its own width is already what is left for tabs and `controls` goes on nothing.
   */
  const { strip: measured, width: room } = useRoom(onStrip.length);
  /** The chat strip itself, for handing the keyboard back to a tab after a rename. */
  const chatStrip = useRef<HTMLElement | null>(null);
  const strip = useCallback(
    (element: HTMLElement | null) => {
      chatStrip.current = element;
      measured(element);
    },
    [measured],
  );
  const { shown, hidden } = useMemo(
    () => fitting(onStrip, tabs.inFront, room, chatLeast),
    [onStrip, room, tabs.inFront, chatLeast],
  );

  /**
   * When each chat last moved, as the CORE counts it (ADR 0039).
   *
   * The window cannot work this out: the strip is an opening order and the needs-you queue
   * is oldest-first, and neither is "when did this chat last do something". The count comes
   * down on `chat-moved` and in the first snapshot, so two windows on one plane agree and a
   * relaunch does not invent an order out of whatever it happened to draw first.
   */
  const lastMoved = useCallback<LastMoved>((session) => movedAt(states, session), [states]);

  /**
   * What the show-more menu lists: the tabs the strip has no room for, **most recently moved
   * first** — and nothing else.
   *
   * **The menu is not a find surface** (ADR 0039). It lists what the strip is hiding, not
   * every chat: the palette lists every chat with a search and a ranking over it, it is
   * better at finding than any menu will be, and a menu built as a second one of those is a
   * menu that should not have been built.
   *
   * **And now it is the only pointer route to a hidden tab, which is what the amendment to
   * ADR 0039 had to argue for.** Its rows bring a tab forward, and the tab it brings forward
   * is drawn on the strip with its own `×` (`fits.ts`, the selected tab is always drawn). So
   * ending a chat is still two presses and never one from a menu under the cursor, which is
   * the rule this menu was built with and did not have to change.
   */
  /** How many of a tab's chats need you: every one of its panes' that is in the queue, split
   *  or not. Its share of the chat strip's show-more count when the strip is not drawing it. */
  const waitingOn = (id: number) =>
    panesOf(tabs, id).filter((pane) => states.needsYou.includes(pane.session)).length;

  const notShowing = useMemo(
    () => byLastActivity(hidden, tabs, lastMoved),
    [hidden, lastMoved, tabs],
  );

  /**
   * And what the workspace strip's show-more menu lists: the workspaces it is not drawing —
   * the ones nobody pinned and you are not in, and any pins it had no room for — **most
   * recently moved first** — the chat strip's rule one level up (ADR 0039, ADR 0054).
   *
   * A workspace moved when the last of its chats did. One nothing has been heard about reads
   * `0` and keeps the strip's order, for `byLastActivity`'s reason.
   */
  const workspacesNotShowing = useMemo(() => {
    const movedIn = (workspace: string) =>
      Math.max(
        0,
        ...tabsIn(tabs, workspace, filedIn)
          .flatMap((id) => panesOf(tabs, id))
          .map((pane) => lastMoved(pane.session)),
      );
    return [...workspacesShown.hidden].sort((one, other) => movedIn(other) - movedIn(one));
  }, [filedIn, lastMoved, tabs, workspacesShown.hidden]);

  // The tab that was in front on this strip, remembered so that coming back to a workspace
  // comes back to the chat that was on screen there.
  useEffect(() => {
    const front = tabs.inFront;
    if (front === undefined || sidebar === undefined) return;
    const workspace = workspaceOf(tabs, front, filedIn);
    if (workspace !== undefined) lastFront.current[workspace] = front;
  }, [filedIn, sidebar, tabs]);

  /** Asks which profile and which persona. It starts nothing by itself. */
  const ask = useCallback(
    async (where: Where) => {
      setPickerTrouble(undefined);
      const options = await commands
        .startOptions(plane)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (options.status === "error") {
        setTrouble(options.error);
        return;
      }
      setPicking({ options: options.data, where });
    },
    [plane],
  );

  const newTab = useCallback(() => void ask({ tab: true }), [ask]);
  /** A new tab whose chat starts in that directory — this one, and not the next. */
  const newTabIn = useCallback((path: string) => void ask({ tab: true, in: path }), [ask]);

  /**
   * **A blocked save's two ways out** (charter-app#295), asked by the Saving tab: a chat started
   * in the plane — the picker, so the operator chooses who resolves it — or a plain terminal
   * there, a shell with no harness, for somebody who resolves a conflict with git by hand.
   */
  useEffect(() => {
    const out = (event: Event) => {
      const asked = (event as CustomEvent<WayOut>).detail;
      if (asked.plane !== plane || sidebar === undefined) return;
      if (asked.way === "chat") {
        newTabIn(sidebar.root);
        return;
      }
      void commands
        .openSession(
          plane,
          null,
          [],
          sidebar.root,
          "terminal",
          STARTING_SIZE.columns,
          STARTING_SIZE.rows,
        )
        .then((opened) => {
          if (opened.status === "error") {
            setTrouble(opened.error);
            return;
          }
          const session = opened.data;
          setStartedIn((was) => ({ ...was, [session]: OUTSIDE }));
          change((tabs) => openTab(tabs, session, "terminal", "shell"));
        })
        .catch((err: unknown) => setTrouble(String(err)));
    };
    window.addEventListener(WAY_OUT, out);
    return () => window.removeEventListener(WAY_OUT, out);
  }, [change, newTabIn, plane, sidebar]);

  /** A row was picked: the chat starts on that profile, with that persona, either drawing
   *  charter's footer in its pane or leaving it blank (ADR 0029), and under the name typed in
   *  the picker, if one was (charter-app#254). */
  const startPicked = useCallback(
    async (profile: string, persona: string | null, showFooter: boolean, label: string | null) => {
      const where = picking?.where;
      if (where === undefined) return;
      // A tab asked for in one directory starts there; everything else starts where the
      // explorer's pick says (charter-app#174).
      const cwd = ("in" in where ? where.in : undefined) ?? startIn;
      const inFrontTab = now.current.inFront;
      // The tab's CHAT name, not the sentence the tab bar draws: the core is being told what
      // this chat is called, and a split's chat is called what the tab's chat is called. The
      // persona the tab also shows is the operator's, not part of the chat's name.
      //
      // A tab that opened on a view has no chat to share a name with, so a chat started beside
      // it is named as a new tab's would be.
      const name =
        ("split" in where && inFrontTab !== undefined
          ? chatNameOf(now.current, inFrontTab)
          : undefined) ?? String(now.current.named.tabs + 1);
      const started = await commands
        .startChat(
          plane,
          profile,
          persona,
          cwd,
          name,
          label,
          showFooter,
          STARTING_SIZE.columns,
          STARTING_SIZE.rows,
        )
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (started.status === "error") {
        // In the picker, not behind it: the operator is still choosing, and a refusal they
        // cannot see beside the rows is one they cannot act on.
        setPickerTrouble(started.error);
        return;
      }
      setPicking(undefined);
      setPickerTrouble(undefined);
      const session = started.data.session;
      // Where charter put it, written down before the tab is drawn: the plane will say the
      // same thing a tick later, and until it does this is what keeps the tab on the strip
      // the operator is looking at.
      const filed = cwd === null ? OUTSIDE : (focused ?? OUTSIDE);
      setStartedIn((was) => ({ ...was, [session]: filed }));
      if ("tab" in where) {
        const kind = picking?.options.profiles.find((one) => one.name === profile)?.kind;
        const held = started.data.label ?? null;
        change((tabs) => openTab(tabs, session, name, whoOf(persona, kind), held));
        return;
      }
      const before = now.current;
      // The tab that was to be split can have closed while the picker was open. Nothing
      // would show that session, so it is ended rather than left running unseen.
      if (change((tabs) => splitFocusedPane(tabs, where.split, session, name)) === before) {
        void commands.closeSession(plane, session);
      }
    },
    [change, focused, picking, plane, startIn],
  );

  /** The approval IS this click. After it, the whole chain of checks runs again from the
   *  top before anything is exec'd, so a yes never walks past a refusal standing behind it. */
  const approveAndStart = useCallback(
    async (
      profile: string,
      persona: string | null,
      showFooter: boolean,
      shown: string,
      label: string | null,
    ) => {
      const said = await commands
        .approveProfile(plane, profile, shown)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") {
        setPickerTrouble(said.error);
        return;
      }
      // The persona the OPERATOR picked, carried up from the dialog with the profile. It
      // used to take the plane's default out of the options instead, which silently threw
      // away the choice on the one path where a profile is being used for the first time.
      // The footer choice rides the same path, and for the same reason: a first run of a
      // profile is exactly where a dropped choice would go unnoticed.
      await startPicked(profile, persona, showFooter, label);
    },
    [plane, startPicked],
  );

  const split = useCallback((direction: Direction) => void ask({ split: direction }), [ask]);

  const closePane = useCallback(() => {
    const tab =
      now.current.inFront === undefined ? undefined : now.current.byId[now.current.inFront];
    const going = tab && panesOf(now.current, tab.id).find((pane) => pane.pane === tab.focused);
    change((tabs) => closeFocusedPane(tabs, filedIn, isPinned));
    if (going) void commands.closeSession(plane, going.session);
  }, [change, filedIn, isPinned, plane]);

  const close = useCallback(
    (id: number) => {
      const ending = panesOf(now.current, id);
      change((tabs) => closeTab(tabs, id, filedIn, isPinned));
      for (const pane of ending) void commands.closeSession(plane, pane.session);
    },
    [change, filedIn, isPinned, plane],
  );

  /**
   * Brings a tab to the front — **and the workspace it is on with it**.
   *
   * The strip shows one workspace's chats, so bringing a chat forward from somewhere else
   * has to move the operator to where that chat lives; otherwise the pane would show a chat
   * whose tab is on a strip that is not drawn. Everything that shows a chat comes through
   * here: the strip itself, the palette's `Switch to tab` rows and the needs-you queue.
   *
   * **The strip no longer depends on this line getting it right** — `focused` derives it from
   * the tab in front. What `picked` is for is the moment AFTER this chat's tab closes and
   * there is no front tab left to read the axis off: the window stays in the workspace the
   * operator was in rather than jumping back to wherever they last pressed a strip.
   */
  const bringToFront = useCallback(
    (id: number) => {
      change((tabs) => selectTab(tabs, id));
      const workspace = sidebar === undefined ? undefined : workspaceOf(now.current, id, filedIn);
      if (workspace !== undefined) setPicked(workspace);
    },
    [change, filedIn, sidebar],
  );

  /** Brings the tab holding a chat to the front. The queue and the palette both use it. */
  const showChat = useCallback(
    (session: number) => {
      const tab = now.current.order.find((id) =>
        panesOf(now.current, id).some((pane) => pane.session === session),
      );
      if (tab !== undefined) bringToFront(tab);
    },
    [bringToFront],
  );

  /**
   * Opens a view in a tab of its own, **on the strip in front** — or brings forward the tab
   * already showing it, wherever that is, and its strip with it.
   *
   * The strip in front is where the operator opened it from: a persona row on the panel beside
   * this workspace, a palette row pressed while looking at it. A view has no directory to be
   * filed by, so it is filed there, explicitly (`tabs.Content`).
   *
   * **Opening one runs nothing.** An extension's view asks its program when its tab draws it,
   * which is a separate, visible step (`Views.tsx`); the persona view reads the plane.
   *
   * `workspace` files it on that strip instead: a workspace's own settings belong on its strip
   * whichever one is in front (charter-app#280).
   */
  const showView = useCallback(
    (view: ViewRef, title: string, on?: string) => {
      const next = change((tabs) => openView(tabs, view, title, on ?? focused ?? OUTSIDE));
      const workspace =
        next.inFront === undefined ? undefined : workspaceOf(next, next.inFront, filedIn);
      if (workspace !== undefined) setPicked(workspace);
    },
    [change, filedIn, focused],
  );

  /**
   * Closes the tab showing `view` — a vault or a persona that was just deleted — **when that tab
   * shows nothing else.** A tab holding a chat beside it is left as it is: closing it would end
   * the chat, and a deletion is never a reason to end one.
   */
  const closeView = useCallback(
    (view: ViewRef) => {
      const found = findView(now.current, view);
      if (found === undefined || panesOf(now.current, found.tab).length > 0) return;
      change((tabs) => closeTab(tabs, found.tab, filedIn, isPinned));
    },
    [change, filedIn, isPinned],
  );

  /** Making and deleting personas, vaults and todos (SI-3): the verbs, the dialogs, the box. */
  const rereadPanels = useCallback(() => setRereadWorkspace((asked) => asked + 1), []);
  const edits = usePlaneEdits({
    plane,
    showView,
    closeView,
    reread: rereadPanels,
    reloadVaults,
  });

  /**
   * The Project settings tab, opened when the window asks for it (charter-app#252). Asked
   * through the window even from this project's own palette, so there is one way in: the
   * window brings the project forward and this opens its tab. `handled` keeps a rebuilt
   * `showView` — it changes with the focused workspace — from opening it a second time for
   * the same ask.
   */
  const handled = useRef(settingsAsked);
  useEffect(() => {
    if (settingsAsked === undefined || handled.current === settingsAsked) return;
    handled.current = settingsAsked;
    showView(SETTINGS_VIEW, SETTINGS_TITLE);
  }, [settingsAsked, showView]);

  /** The Saving tab (charter-app#294), opened the same way and for the same reason. */
  const savingHandled = useRef(savingAsked);
  useEffect(() => {
    if (savingAsked === undefined || savingHandled.current === savingAsked) return;
    savingHandled.current = savingAsked;
    showView(SAVING_VIEW, SAVING_TITLE);
  }, [savingAsked, showView]);

  /** A workspace's settings tab (charter-app#280), on that workspace's strip. */
  const openWorkspaceSettings = useCallback(
    (workspace: string) =>
      showView(workspaceSettingsView(workspace), workspaceSettingsTitle(workspace), workspace),
    [showView],
  );

  /** The Preferences tab, opened the same way and for the same reason (charter-app#283). */
  const preferencesHandled = useRef(preferencesAsked);
  useEffect(() => {
    if (preferencesAsked === undefined || preferencesHandled.current === preferencesAsked) return;
    preferencesHandled.current = preferencesAsked;
    showView(PREFERENCES_VIEW, PREFERENCES_TITLE);
  }, [preferencesAsked, showView]);

  /**
   * Focuses a workspace: the strip below it shows that workspace's chats, and one of them
   * comes to the front — the one that was in front there last, or its first.
   *
   * **A workspace with no chats puts nothing in front.** Leaving another workspace's chat on
   * screen under this workspace's empty strip would be the app showing a chat the strip says
   * is not there. Nothing is ended and nothing is torn down: every chat in every workspace
   * keeps running, exactly as a project behind another one does (#125).
   */
  const focusWorkspace = useCallback(
    (workspace: string) => {
      setPicked(workspace);
      change((tabs) =>
        showWorkspace(tabs, workspace, filedIn, lastFront.current[workspace], isPinned),
      );
      // Reported to the extensions that hear it (charter-app#343). Not awaited: the core tells
      // them on a thread of its own, and the strip has already moved.
      void commands.workspaceFocused(plane, workspace).catch(() => undefined);
    },
    [change, filedIn, isPinned, plane],
  );

  /** What a chat is called here: the tab holding it, or its session number. */
  const nameOf = useCallback(
    (session: number) =>
      tabs.order
        .filter((id) => panesOf(tabs, id).some((pane) => pane.session === session))
        .map((id) => tabs.byId[id].name)[0] ?? String(session),
    [tabs],
  );

  const frontTab = tabs.inFront === undefined ? undefined : tabs.byId[tabs.inFront];
  // The session the next worktree question is about: the chat in the pane that has the
  // keyboard, which is the one "this chat's worktree" means.
  const frontSession = frontTab
    ? panesOf(tabs, frontTab.id).find((pane) => pane.pane === frontTab.focused)?.session
    : undefined;
  // Where that chat is working. The sidebar's chats carry it, and so does the record the
  // core put back at this launch; a chat the operator just opened is in the first.
  const frontCwd =
    frontSession === undefined
      ? null
      : ([...(sidebar?.workspaces.flatMap((ws) => ws.chats) ?? []), ...(sidebar?.unfiled ?? [])]
          .concat(reopened)
          .find((chat) => chat.session === frontSession)?.cwd ?? null);

  // Which piece the chat in front sits in. `worktree_of_chat` is path arithmetic plus one
  // git listing, asked only when the directory in front changes — never per keystroke, and
  // never for a palette that is not open.
  //
  // The plane travels with the directory (charter-app#127): the answer is about a piece of
  // THIS project, and the core used to derive the plane by walking up from `frontCwd` alone.
  useEffect(() => {
    if (frontCwd === null) return;
    let gone = false;
    void commands
      .worktreeOfChat(plane, frontCwd)
      .then((answer) => {
        if (gone) return;
        setLocated({
          cwd: frontCwd,
          piece: answer.status === "ok" ? (answer.data ?? undefined) : undefined,
        });
      })
      // A window that cannot ask simply offers no worktree row — which the catalogue then
      // lists with its reason rather than dropping.
      .catch(() => {
        if (!gone) setLocated({ cwd: frontCwd });
      });
    return () => {
      gone = true;
    };
  }, [frontCwd, plane, relocate]);

  // Only an answer about the directory in front. Nothing is cleared when the focus moves —
  // clearing state from inside an effect is a render the window does not need, and a stale
  // answer is simply not this chat's.
  const worktree = located?.cwd === frontCwd ? located.piece : undefined;

  /**
   * Removes the piece THE ROW NAMED. The core's refusal travels back whole.
   *
   * **It reads nothing off the window to work out which worktree was meant**
   * (charter-app#174). It used to take only `force` and act on whatever the chat in front was
   * working in, which is exactly why the explorer's rows had nothing to offer: a piece nobody
   * is running in is not in front of anything. The piece arrives on the row, so the front
   * chat's row and an explorer row are the same code with a different `cut` in them.
   */
  const removeWorktree = useCallback(
    async (cut: Cut, force: boolean): Promise<Ran> => {
      const answer = await commands
        .worktreeRemove(plane, cut.workspace, cut.repo, cut.piece, force)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      // Verbatim. The sentence names the repair, and an operator shown a reworded version of
      // it can neither follow that repair nor search for it.
      if (answer.status === "error") return { ok: false, refused: answer.error };
      // The core is asked again rather than the window assuming what it now says — both about
      // where the chat in front is working, and about the workspace, because the explorer and
      // the bottom bar are still drawing the row that has just gone.
      setRelocate((asked) => asked + 1);
      setRereadWorkspace((asked) => asked + 1);
      // The branch is not named any more: a row about a piece nothing is running in carries
      // no branch, and "its branch stays" is true of every worktree charter cuts.
      return { ok: true, said: `The worktree ${cut.piece} is gone. Its branch stays.` };
    },
    [plane],
  );

  /** Asks for a new workspace. It makes nothing: the dialog is what asks, and
   *  `workspace_create` is what makes one. */
  const createWorkspace = useCallback(() => {
    setWorkspaceTrouble(undefined);
    setMakingWorkspace(true);
  }, []);

  /** Pins or unpins one workspace. It goes in the machine store, so what the store now says
   *  is asked again rather than assumed — `pinning` is what asks. */
  const pinWorkspace = useCallback(
    async (workspace: string, pinned: boolean): Promise<Ran> => {
      const said = await commands
        .pinWorkspace(plane, workspace, pinned)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") return { ok: false, refused: said.error };
      setPinning((asked) => asked + 1);
      return { ok: true };
    },
    [plane],
  );

  /**
   * Makes it, through `charter workspace create`.
   *
   * **The name is not checked here.** `workspace_create` runs `wscmd::create`, which runs
   * `wscmd::ensure`, which is where `contain::workspace_name_ok` lives — so the window refuses
   * exactly the names a terminal refuses, in the same sentence. A refusal stays in the dialog,
   * where the operator is still standing.
   */
  const makeWorkspace = useCallback(
    async (name: string, vision: string, live: boolean, repos: string[]) => {
      setBusyMaking(true);
      const answer = await commands
        .workspaceCreate(plane, name, vision.trim() === "" ? null : vision, live)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      setBusyMaking(false);
      if (answer.status === "error") {
        setWorkspaceTrouble(answer.error);
        return;
      }
      setMakingWorkspace(false);
      setWorkspaceTrouble(undefined);
      // **Made here, so pinned** (ADR 0054): the operator made it in order to work in it, and
      // the workspace strip draws what is pinned. A pin the store refused leaves the
      // workspace made and says why, beside what charter said about making it.
      const pinned = await pinWorkspace(name, true);
      const words = pinned.ok ? answer.data : [...answer.data, pinned.refused];
      // charter's own lines, which say where it landed and whether it is LOCAL or LIVE.
      setReport({ from: "workspace.create", refused: false, words: words.join(" ") });
      // The plane is read again rather than this window writing the workspace into its own
      // copy of the sidebar, and the strip lands on what was just made: it holds no chats, so
      // `picked` is the only thing that can put the window in it.
      setPicked(name);
      setReplan((asked) => asked + 1);
      if (repos.length === 0) return;
      // The repos land after the workspace, one at a time and each on its own (ADR 0055):
      // the dialog is closed and the operator can start a chat while they clone.
      setReport({
        from: "workspace.create",
        refused: false,
        words: `Cloning ${repos.length} repo(s) into ${name}…`,
      });
      const failed = await cloneRepos(plane, name, repos);
      setReport({
        from: "workspace.create",
        refused: failed.length > 0,
        words:
          failed.length === 0
            ? `Cloned ${repos.join(", ")} into ${name}.`
            : `Could not clone ${failed.map((f) => f.repo).join(", ")} into ${name} — ` +
              `${failed[0].said} Retry from the workspace's settings.`,
      });
      setReplan((asked) => asked + 1);
    },
    [pinWorkspace, plane],
  );

  /** Asks for a new vault. It makes nothing: the dialog asks, and `vault_create` makes one. */
  const createVault = useCallback(() => {
    setVaultTrouble(undefined);
    setMakingVault(true);
  }, []);

  const pickVault = useCallback(() => setPickingVault(true), []);

  /**
   * Makes it, through `charter vault add`'s own path (`vault_create`), and opens its tab — a
   * vault is made to have secrets put in it, and the tab is where that is done. A refusal stays
   * in the dialog, in the core's words.
   */
  const makeVault = useCallback(
    async (name: string, provider: string, opVault: string | null) => {
      setBusyVault(true);
      const answer = await commands
        .vaultCreate(plane, name, provider, opVault)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      setBusyVault(false);
      if (answer.status === "error") {
        setVaultTrouble(answer.error);
        return;
      }
      setMakingVault(false);
      setVaultTrouble(undefined);
      reloadVaults();
      showView({ from: null, view: "vault", key: answer.data.name }, answer.data.name);
    },
    [plane, reloadVaults, showView],
  );

  /**
   * Asks about deleting one, and reads the core's guard so the dialog can show it first.
   *
   * The reading is for DRAWING. `workspace_remove` asks `work_at_risk` again, inside the core,
   * against the disk at the moment of the delete — this is what the operator sees before they
   * press, not what decides.
   */
  const removeWorkspace = useCallback(
    (workspace: string) => {
      setRemoving({ workspace, busy: false });
      void commands
        .workspaceAtRisk(plane, workspace)
        .then((answer) =>
          setRemoving((now) =>
            now?.workspace !== workspace
              ? now
              : answer.status === "ok"
                ? { ...now, atRisk: answer.data }
                : { ...now, unreadable: answer.error },
          ),
        )
        // A preview charter could not take is said as one. It is never drawn as an empty list:
        // "nothing would be lost" is a claim, and this is the absence of one.
        .catch((err: unknown) =>
          setRemoving((now) =>
            now?.workspace === workspace ? { ...now, unreadable: String(err) } : now,
          ),
        );
    },
    [plane],
  );

  /** Asks for a workspace's new name. Nothing is renamed until the dialog is answered. */
  const renameWorkspace = useCallback(
    (workspace: string) => {
      setRenamingWs({ workspace, busy: false });
      // Which chats will start fresh (charter#367, D10): asked of the core, which knows which
      // harness finds a conversation by its folder. A question that fails names nobody.
      void commands
        .workspaceStartsFresh(plane, workspace)
        .then((answer) => {
          const startsFresh = answer.status === "ok" ? answer.data : null;
          setRenamingWs((now) => (now?.workspace === workspace ? { ...now, startsFresh } : now));
        })
        // A question that fails names nobody, and does not hold the answer back.
        .catch(() =>
          setRenamingWs((now) =>
            now?.workspace === workspace ? { ...now, startsFresh: null } : now,
          ),
        );
    },
    [plane],
  );

  /**
   * Renames it, through `workspace_rename` — which is `charter workspace rename` — and nothing
   * else (charter#367).
   *
   * **The core decides and moves everything on disk**: the refusals (a taken or invalid name, a
   * chat running in it), the folder, the worktrees, every record, the pins and the save. What
   * the window follows is its own: the view tabs on the workspace's strip, the settings tab
   * keyed by its name and its pin, and the workspace it has picked. Then it reads the plane
   * again, as after any change to what workspaces there are.
   */
  const doRename = useCallback(
    async (workspace: string, name: string) => {
      setRenamingWs((now) => (now?.workspace === workspace ? { ...now, busy: true } : now));
      const answer = await commands
        .workspaceRename(plane, workspace, name)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (answer.status === "error") {
        setRenamingWs((now) =>
          now?.workspace === workspace ? { ...now, busy: false, trouble: answer.error } : now,
        );
        return;
      }
      setRenamingWs(undefined);
      setTabs((was) => followRename(was, workspace, name));
      const oldSettings = viewKey(workspaceSettingsView(workspace));
      setPinnedViews((was) =>
        was.map((key) => (key === oldSettings ? viewKey(workspaceSettingsView(name)) : key)),
      );
      setPicked((was) => (was === workspace ? name : was));
      setPickedSpot((was) => (was?.workspace === workspace ? undefined : was));
      setReport({
        from: `workspace.rename:${workspace}`,
        refused: false,
        words: answer.data.join(" "),
      });
      setPinning((asked) => asked + 1);
      setReplan((asked) => asked + 1);
    },
    [plane],
  );

  /**
   * Deletes it — **through `workspace_remove` and through nothing else**.
   *
   * There is one path from this window to a deleted workspace and the core's guard is inside
   * it (`wscmd::remove`, which runs `wscmd::work_at_risk` before `remove_dir_all`). `force` is
   * never passed on the operator's behalf: it arrives here only from the second button, which
   * does not exist until a refusal does and which names what it will discard.
   *
   * **And what it names comes back with the refusal** (charter-app#182). `workspace_remove`
   * answers with the at-risk list the core refused on, so the button and the sentence above it
   * are two readings of one moment rather than one reading and a memory.
   */
  const deleteWorkspace = useCallback(
    async (workspace: string, force: boolean) => {
      setRemoving((now) => (now?.workspace === workspace ? { ...now, busy: true } : now));
      const answer = await commands
        .workspaceRemove(plane, workspace, force)
        // A command that never reached the core said nothing about what is at risk, and an
        // empty list is the truthful shape for that: there is no refusal here to force past.
        .catch((err: unknown) => ({
          status: "error" as const,
          error: { said: String(err), at_risk: [] } satisfies Refused,
        }));
      if (answer.status === "error") {
        // Verbatim: the sentence names the repair — push or commit first — and the force
        // button is drawn beside it rather than instead of it.
        setRemoving((now) =>
          now?.workspace === workspace ? { ...now, busy: false, refusal: answer.error } : now,
        );
        return;
      }
      setRemoving(undefined);
      setReport({
        from: `workspace.remove:${workspace}`,
        refused: false,
        words: answer.data.join(" "),
      });
      // Nothing is picked any more: the workspace that was picked may be the one that has just
      // gone, and the sidebar's own focus rule decides what the window lands on.
      setPicked(undefined);
      setReplan((asked) => asked + 1);
    },
    [plane],
  );

  /** Lands the piece the row named in its clone, fast-forward only. The core never pushes. */
  const mergeWorktree = useCallback(
    async (cut: Cut): Promise<Ran> => {
      const answer = await commands
        .worktreeMerge(plane, cut.workspace, cut.repo, cut.piece)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (answer.status === "error") return { ok: false, refused: answer.error };
      // The clone has moved, so what the bottom bar says about it is a commit behind.
      setRereadWorkspace((asked) => asked + 1);
      return {
        ok: true,
        said: `${answer.data.branch} landed: ${answer.data.was} → ${answer.data.now}`,
      };
    },
    [plane],
  );

  /** Records the piece the row named as `done` in its log (charter#368). The tree stays. */
  const declareWorktreeDone = useCallback(
    async (cut: Cut): Promise<Ran> => {
      const answer = await commands
        .worktreeDone(plane, cut.workspace, cut.repo, cut.piece)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (answer.status === "error") return { ok: false, refused: answer.error };
      // The explorer's row reads what the piece said, so it is read again.
      setRereadWorkspace((asked) => asked + 1);
      return { ok: true, said: `${cut.repo} · ${cut.piece} — done` };
    },
    [plane],
  );

  /**
   * Hands a key the palette claimed to the chat in front.
   *
   * **It writes the bytes the pane's own terminal would have written.** The palette takes
   * `F2` on the window, capture-phase, so xterm never gets the keystroke to translate — and
   * re-dispatching the event is not a way out of that, because the focus is in the palette's
   * box by then. So what the terminal would have sent is sent, through the one path a pane's
   * input already takes (`send_input`). Nothing is read back: this is input, not a reading of
   * anything the harness said.
   */
  const sendKey = useCallback(
    async (key: string): Promise<Ran> => {
      if (frontSession === undefined)
        return { ok: false, refused: "No chat is in front, so there is nowhere to send it." };
      if (key !== PASS_THROUGH_KEY) return { ok: false, refused: `charter cannot send ${key}.` };
      const sent = await commands
        .sendInput(plane, frontSession, PASS_THROUGH_BYTES)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      // Verbatim: a session that has stopped reading its input says so in the core's words.
      if (sent.status === "error") return { ok: false, refused: sent.error };
      // Nothing is said. The chat's own answer to the key is the report, and a banner after
      // every press of a key an operator means to press repeatedly is noise.
      return { ok: true };
    },
    [frontSession, plane],
  );

  /**
   * Pins or unpins one chat.
   *
   * **The core's write is what makes it true**, and this waits for it: the pin is written
   * into the plane's own record, and a mark drawn before that landed would be a pin the next
   * launch does not have. The core's refusal travels back whole for the same reason a
   * worktree removal's does.
   */
  const pinTab = useCallback(
    async (id: number, pinned: boolean): Promise<Ran> => {
      const lead = contentsOf(now.current, id)[0]?.content;
      if (lead?.kind === "view") {
        // **Its own record line, written by the one effect that tells the core the view tabs**
        // — so the pin lands with the tab it pins, in the same write, and there is nothing
        // here for the core to refuse.
        const key = viewKey(lead.view);
        setPinnedViews((was) =>
          pinned ? (was.includes(key) ? was : [...was, key]) : was.filter((one) => one !== key),
        );
        return { ok: true };
      }
      const session = chatOf(now.current, id);
      if (session === undefined) return { ok: false, refused: "That tab has no chat to pin." };
      const said = await commands
        .pinChat(plane, session, pinned)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") return { ok: false, refused: said.error };
      setPinnedChats((was) =>
        pinned
          ? was.includes(session)
            ? was
            : [...was, session]
          : was.filter((one) => one !== session),
      );
      // Nothing is said: the mark appearing on the tab is the answer, and a banner after
      // every pin is noise about something the operator can already see.
      return { ok: true };
    },
    [plane],
  );

  /**
   * A chat or view tab dragged onto another on the chat strip (SI-6): moved to where it was put
   * down, and pinned or unpinned when it crossed from one group to the other (`reorder.ts`).
   *
   * **The order is the window's at once and the pin follows**, because the pin is written by
   * the core and a tab that waited for it would hang in the air under the pointer. A pin the
   * core refuses is said, as the tab's menu would say it, and the tab is drawn where its pin
   * says it goes.
   */
  const dragTab = useCallback(
    (moved: number, onto: number) => {
      const made = afterDrop({ whole: onStrip, drawn: shown, moved, onto, isPinned });
      if (made === undefined) return;
      change((tabs) => ({ ...tabs, order: reslotted(tabs.order, made.order) }));
      if (made.pinned === undefined) return;
      void pinTab(moved, made.pinned).then((ran) => {
        if (!ran.ok) setReport({ from: "tab.drag", refused: true, words: ran.refused });
      });
    },
    [change, isPinned, onStrip, pinTab, shown],
  );

  /**
   * A workspace tab dragged onto another on the workspace strip (SI-6): its pins put in the new
   * order in the machine store (ADR 0040), after pinning or unpinning it when it crossed.
   *
   * **Drawn in the new order at once**, from `pinnedWorkspaces`, which is what the strip is
   * drawn from; and then asked again of the store, which is the answer. A refusal is said, and
   * the store's own order comes back with the re-read.
   *
   * **Outside every workspace is fixed**: it is not a directory, so it cannot be pinned (the
   * store refuses a name with a `/` in it), and nothing is put down in its place. A later
   * change draws it first; `reorder.ts` already keeps a fixed tab where it is.
   */
  const dragWorkspace = useCallback(
    (moved: string, onto: string) => {
      const pinnedNow = (name: string) => pinnedWorkspaces.includes(name);
      const made = afterDrop({
        whole: onWorkspaceStrip,
        drawn: workspacesShown.shown,
        moved,
        onto,
        isPinned: pinnedNow,
        isFixed: (name) => name === OUTSIDE,
      });
      if (made === undefined) return;
      const pins = made.order.filter((name) =>
        name === moved && made.pinned !== undefined ? made.pinned : pinnedNow(name),
      );
      setPinnedWorkspaces(pins);
      /** The core's refusal of one write, or nothing when it was written. */
      const refusal = (asking: Promise<{ status: "ok" } | { status: "error"; error: string }>) =>
        asking.then(
          (said) => (said.status === "error" ? said.error : undefined),
          (err: unknown) => String(err),
        );
      void (async () => {
        // The pin first, when the drop crossed: arranging never pins (`Store::arrange_workspaces`).
        const failed =
          (made.pinned === undefined
            ? undefined
            : await refusal(commands.pinWorkspace(plane, moved, made.pinned))) ??
          (await refusal(commands.arrangeWorkspacePins(plane, pins)));
        if (failed !== undefined)
          setReport({ from: "workspace.drag", refused: true, words: failed });
        setPinning((asked) => asked + 1);
      })();
    },
    [onWorkspaceStrip, pinnedWorkspaces, plane, workspacesShown.shown],
  );

  /** What a screen reader hears while a tab is dragged on the chat and workspace strips. */
  const chatDragWords = useMemo(
    () => stripAccessibility("tab", (id) => tabs.byId[Number(id)]?.name ?? String(id)),
    [tabs.byId],
  );
  const workspaceDragWords = useMemo(
    () =>
      stripAccessibility("workspace", (id) =>
        String(id) === OUTSIDE ? OUTSIDE_TITLE : String(id),
      ),
    [],
  );
  const dragSensors = useStripSensors();

  /**
   * Ignores a queued chat's request until it asks again (charter-app#248).
   *
   * **Nothing is changed here.** The ignore is the core's, and the core answers it with a
   * `chat-moved` carrying the queue without this chat, which lowers the project's and the
   * workspace's red counts in the same render that drops the item — they are all read from
   * that one queue.
   */
  const ignoreNeedsYou = useCallback(
    async (session: number): Promise<Ran> => {
      const said = await commands
        .ignoreNeedsYou(plane, session)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      return said.status === "error" ? { ok: false, refused: said.error } : { ok: true };
    },
    [plane],
  );

  /**
   * Opens a chat tab's name for editing on the strip (charter-app#254) — what the tab's menu,
   * the palette's row, a double-click on the tab and `F2` on it all run, through the one
   * catalogue row. **The tab comes forward first**, because the strip always draws the tab in
   * front (`fits.ts`) and a name being typed has to be on screen.
   */
  const beginRename = useCallback(
    (id: number) => {
      if (chatOf(now.current, id) === undefined) return;
      bringToFront(id);
      setRenaming(id);
    },
    [bringToFront],
  );

  /**
   * Asks the core to hold `typed` as the chat's name, and draws what it answers.
   *
   * **The core's answer is the name, not what was typed**: it trims it, a blank is the default
   * back, and a name it will not draw is refused in its own words — which is what this answers,
   * for the box to say beside the name. Charter's label only: the harness keeps its own.
   */
  const saveName = useCallback(
    async (id: number, typed: string): Promise<string | undefined> => {
      const session = chatOf(now.current, id);
      if (session === undefined) return "That tab has no chat to rename.";
      const said = await commands
        .renameChat(plane, session, typed)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") return said.error;
      change((tabs) => renameTab(tabs, id, said.data));
      return undefined;
    },
    [change, plane],
  );

  /** The box is finished with. After Enter or Escape the keyboard goes back to the tab — the
   *  one in front, which the rename brought there — once it is drawn again. */
  const backToTheTab = useRef(false);
  const endRename = useCallback((back: boolean) => {
    backToTheTab.current = back;
    setRenaming(undefined);
  }, []);
  useEffect(() => {
    if (renaming !== undefined || !backToTheTab.current) return;
    backToTheTab.current = false;
    chatStrip.current?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
  }, [renaming]);

  /**
   * Runs an extension's action from the palette (charter-app#341): on nothing in particular, in
   * the workspace in front. **One that asks first is asked about**, and run from the dialog;
   * the core refuses it without the yes whatever this does.
   */
  const runAction = useCallback(
    async (extension: string, action: RowAction, name: string): Promise<Ran> => {
      if (action.asks_first) {
        setAskingAction({ extension, action });
        return { ok: true };
      }
      const outcome = await runExtensionAction(
        plane,
        extension,
        action,
        { view: null, key: "", row: null },
        ofWorkspace,
        false,
      );
      if ("refused" in outcome) return { ok: false, refused: outcome.refused };
      return {
        ok: true,
        said: outcome.answer.overreach ?? `${name}: “${action.title}” ran.`,
      };
    },
    [ofWorkspace, plane],
  );

  const doing = useMemo<Doing>(
    () => ({
      newChat: newTab,
      runAction,
      split,
      closePane,
      closeTab: close,
      selectTab: bringToFront,
      renameTab: beginRename,
      pinTab,
      pinWorkspace,
      pinProject: windowDoes.pinProject,
      focusWorkspace,
      createWorkspace,
      removeWorkspace,
      showChat,
      ignoreNeedsYou,
      // The verb still names a persona — that is what the catalogue row is about — and the
      // window turns it into the row it opens. `charter/personas` is charter's own panel's
      // key (`charter_core::panel::Panel::key`), and it is written here because the catalogue
      // is not a reader of the panel list.
      openView: showView,
      pickVault,
      createVault,
      ...edits.doing,
      removeWorktree,
      mergeWorktree,
      declareWorktreeDone,
      // A clone picked from its menu is the explorer's own pick one level up: the same state,
      // so the explorer marks it and `New tab` starts there (charter-app#174).
      pickClone: (repo, path) => pickSpot({ repo, path }),
      newChatIn: newTabIn,
      sendKey,
      openProject: windowDoes.openProject,
      createProject: windowDoes.createProject,
      showExtensions: windowDoes.showExtensions,
      installCli: windowDoes.installCli,
      selectProject: windowDoes.selectProject,
      closeProject: windowDoes.closeProject,
      moveProject: windowDoes.moveProject,
      openSettings: windowDoes.openSettings,
      openSaving: windowDoes.openSaving,
      openWorkspaceSettings,
      switchLive: (workspace: string) => setLiveAsk(workspace),
      renameWorkspace,
      openPreferences: windowDoes.openPreferences,
      quit: windowDoes.quit,
    }),
    [
      beginRename,
      bringToFront,
      close,
      closePane,
      createVault,
      createWorkspace,
      edits.doing,
      focusWorkspace,
      ignoreNeedsYou,
      mergeWorktree,
      declareWorktreeDone,
      newTab,
      pickVault,
      newTabIn,
      openWorkspaceSettings,
      pickSpot,
      pinTab,
      pinWorkspace,
      removeWorkspace,
      removeWorktree,
      renameWorkspace,
      runAction,
      sendKey,
      showChat,
      showView,
      split,
      windowDoes,
    ],
  );

  // The chats that can be waiting on the operator without saying so. Read from the sidebar,
  // which is the core's own list of what is open and what each chat runs. It is needed up
  // here as well as in the title bar's list: the palette's row for the queue must not claim
  // "Nothing needs you." over the top of a chat that cannot say it does (charter-app#52).
  //
  // Held, because it is one of the catalogue's inputs: a fresh array on every render would
  // rebuild all 117 rows of a fifty-chat catalogue for every keystroke in the palette.
  const quiet = useMemo(
    () =>
      sidebar
        ? quietOnes(
            [...sidebar.workspaces.flatMap((ws) => ws.chats), ...sidebar.unfiled],
            states,
            // The name its tab carries, as everywhere else a chat is named; the plane's own
            // name for a chat no tab here holds.
            (chat) => (alreadyShows(tabs, chat.session) ? nameOf(chat.session) : chat.name),
          )
        : [],
    [nameOf, sidebar, states, tabs],
  );

  /** The chats working in the focused workspace, which is what the explorer files under the
   *  spots they are working at. The plane's own answer, like everything else about where a
   *  chat is: nothing on the plane records a chat, so the directory it works in is it. */
  const workspaceChats = useMemo(
    () =>
      (sidebar?.workspaces.find((ws) => ws.name === ofWorkspace)?.chats ?? []).map((chat) => {
        // Named as its tab is — the name the operator gave it, or `steward 3` — rather than by
        // the harness's own name, which is a number (charter-app#254).
        const tab = tabs.order.find((id) => chatOf(tabs, id) === chat.session);
        return tab === undefined ? chat : { ...chat, name: tabs.byId[tab].name };
      }),
    [ofWorkspace, sidebar, tabs],
  );

  /**
   * The focused workspace's worktrees, as the catalogue names them (charter-app#174).
   *
   * **The clones in the plane's own order, and the pieces in git's** — the order the explorer
   * draws them in, so a palette listing them and a tree showing them agree. Held on the two
   * objects the core's answers arrive as (`panels`, `pieces`), both of which keep their
   * identity until the plane is read again, so this is rebuilt when a listing lands and on no
   * other render. That is what keeps ~100 more catalogue rows off the per-keystroke path.
   */
  const pieces = useMemo<Cut[]>(
    () =>
      ofWorkspace === undefined
        ? []
        : (workspaceState.panels?.repos ?? []).flatMap((repo) =>
            (workspaceState.pieces[repo] ?? []).map((piece) => ({
              workspace: ofWorkspace,
              repo,
              piece: piece.piece,
            })),
          ),
    [ofWorkspace, workspaceState.panels, workspaceState.pieces],
  );

  /**
   * The focused workspace's clones with the paths the core spelled (charter-app#174), for the
   * two rows each clone's menu lists. Held on `panels`, which keeps its identity until the
   * plane is read again, for the pieces' reason above.
   */
  const clones = useMemo<Clone[]>(() => {
    const panels = workspaceState.panels;
    if (ofWorkspace === undefined || panels === undefined) return [];
    return panels.repos.flatMap((repo) => {
      const path = panels.paths[repo];
      return path === undefined ? [] : [{ repo, path }];
    });
  }, [ofWorkspace, workspaceState.panels]);

  /** The plane's personas, straight off the plane's own answer — the array, not a copy of it,
   *  so the catalogue is rebuilt when the plane is read again and not per render. */
  const personas = workspaceState.panels?.personas;
  /** The focused workspace's open todos, the same way: one close and one forget row each. */
  const todos = workspaceState.panels?.todos;

  /**
   * Every action this project's window can do, in one list.
   *
   * **The bar's buttons are rows of THIS list, not a second one.** The tmux frame kept a
   * menu beside its palette once and the two drifted; here `New tab`, the splits, `Close
   * pane` and every tab's `×` are looked up by id out of the same catalogue the palette
   * draws, so a row that goes away takes its button with it. The project strip does the same
   * thing one scope up, through `actions.projectRows`.
   *
   * **Only for the project in front.** Nothing draws a catalogue for a project that is not on
   * screen — not its bar, and not the window's palette, which lists the front one's. At ADR
   * 0026's limits that is the difference between rebuilding 117 rows once per hook event and
   * rebuilding them once per hook event per project the window happens to hold.
   */
  const offers = useMemo(
    () =>
      !inFront
        ? []
        : catalogue({
            tabs,
            workspaces: strips,
            live: liveNames,
            focused,
            worktree,
            pieces,
            clones,
            startsIn: spot?.path,
            personas,
            vaults: vaultNames,
            todos,
            plane,
            projects,
            // Which window this is, for the rows that move a project between windows (charter#126).
            split: thisWindow() !== MAIN,
            // WHICH row was refused and is still on screen. The catalogue matches the ids it
            // wrote itself, so the discard row appears beside the removal that was refused and
            // beside no other — with one removal per piece that is the difference between one
            // offer to throw work away and fifty.
            refused: report?.refused ? report.from : undefined,
            needsYou: states.needsYou,
            quiet,
            nameOf,
            reportsTo: (session) => states.reports[session] ?? [],
            // The projects' pins are the WINDOW's, and travel down with the projects: a
            // project that is not in front draws nothing, so its pin cannot be held here.
            pinned: {
              chats: pinnedChats,
              views: pinnedViews,
              workspaces: pinnedWorkspaces,
              projects: pinnedProjects,
            },
            views,
            commands: extensionCommands,
          }),
    [
      clones,
      focused,
      inFront,
      nameOf,
      personas,
      pieces,
      pinnedChats,
      pinnedProjects,
      pinnedViews,
      pinnedWorkspaces,
      plane,
      projects,
      quiet,
      report,
      spot?.path,
      states.needsYou,
      states.reports,
      strips,
      tabs,
      vaultNames,
      todos,
      views,
      extensionCommands,
      worktree,
      liveNames,
    ],
  );

  /**
   * The catalogue by id, built once per catalogue rather than scanned per lookup.
   *
   * Every surface in this window that draws ONE row asks here: the tab strip (two lookups per
   * tab), the workspace strip, the show-more menus, each pane's own controls — and, since
   * #172, a context menu on every one of them, which is `menuRows` scanning the whole list
   * three times per tab per render. One render of a fifty-tab strip was 0.047 ms of scanning
   * and is 0.017 ms through this; what the 31 µs buys is not a speed anybody feels but a cost
   * that stops tracking the catalogue's length, which #174 is the change that grew.
   * `actions.catalogued` has the table and the method.
   */
  const found = useMemo(() => catalogued(offers), [offers]);

  const by = useCallback((id: string) => found.get(id), [found]);

  /** Carries a row out and keeps what it answered. Everything below this line has already
   *  been asked about, where asking was owed. */
  const carryOut = useCallback(
    async (offer: Offer): Promise<Ran> => {
      const answer = await perform(offer, doing);
      setReport(
        answer.ok
          ? answer.said
            ? { from: offer.id, refused: false, words: answer.said }
            : undefined
          : { from: offer.id, refused: true, words: answer.refused },
      );
      return answer;
    },
    [doing],
  );

  /**
   * The row waiting on an answer, when the operator has asked for something that ends a chat.
   *
   * Held here and not in the dialog, because the dialog is drawn only while there is one:
   * a component that is not mounted cannot be holding the question it is about to ask.
   */
  const [endingChat, setEndingChat] = useState<Offer>();

  /**
   * What every surface does with a row: ask first where a chat is about to end, then carry
   * it out.
   *
   * One function for the bar, the panes, the tabs and the palette. It is called from an
   * event handler and never while rendering, which is what lets the verbs it dispatches to
   * reach the window's live arrangement rather than a copy taken when the row was built.
   *
   * **The confirmation is HERE rather than on each button** (the operator: *"closing session
   * should ask confirmation"*). There are four ways to end a chat — a tab's `×`, a pane's
   * `×`, the palette's row and the palette's pane row — and a guard on three of them is a
   * guard an operator learns to trust and then walks past on the fourth.
   *
   * **It answers `{ ok: true }` for a row it has only ASKED about**, which is true: nothing
   * was refused and nothing has happened yet. The palette reads this answer to say what a row
   * did, and "nothing to say" is the right thing to say about a question still on screen.
   */
  const run = useCallback(
    async (offer: Offer): Promise<Ran> => {
      if (offer.available && endsAChat(offer.does)) {
        setEndingChat(offer);
        return { ok: true };
      }
      return carryOut(offer);
    },
    [carryOut, setEndingChat],
  );

  const press = useCallback(
    (offer: Offer) => {
      void run(offer);
    },
    [run],
  );

  /**
   * A pane's own button: that pane becomes the focused one, and then the row runs.
   *
   * **The row is the bar's row unchanged**, which is what keeps one list of actions. A split
   * and a close act on the focused pane (`tabs.ts`), and pressing a control ON a pane is the
   * operator saying "this one" — exactly what clicking anywhere in the pane already says. So
   * the target is not a new parameter on three catalogue rows; it is the focus, moved first.
   *
   * **The two are one update, not a race.** `change` is applied to a ref synchronously before
   * it reaches React state (see it above), so the row that runs on the next line reads the
   * pane this button belongs to. A `setState` here would leave the split acting on whatever
   * was focused before, which is the defect the operator is asking to be rid of.
   */
  const onPaneDoes = useCallback(
    (pane: number, offer: Offer | undefined) => {
      if (!offer?.available) return;
      change((tabs) => focusPane(tabs, pane));
      press(offer);
    },
    [change, press],
  );

  // **`scrollIntoView` on the selected tab is gone with the scroller.** It was how a chat
  // brought forward from somewhere that is not the strip — the palette, the needs-you queue,
  // a close taking the tab beside it — came back on screen. The strip does not scroll any
  // more, so there is nowhere to scroll it to; `fits.ts` draws the selected tab instead, and
  // that is the same promise kept by construction rather than by a side effect.

  // The chats still open, in the order the tab bar shows them: what a quit would end, with
  // what each one is doing already resolved. The state has to be looked up HERE, because a
  // session number names a chat only inside its own project.
  const ending = useMemo<Ending[]>(
    () =>
      tabs.order.flatMap((id) => {
        const filed = workspaceOf(tabs, id, filedIn);
        return panesOf(tabs, id).map(({ session }) => {
          const known = reopened.find((chat) => chat.session === session);
          return {
            key: `${plane}#${session}`,
            project: plane,
            // The tab's name — the one the operator gave it, or its default — and not the
            // harness's: this is a list the operator reads (charter-app#254).
            name: tabs.byId[id].name,
            harness: known?.harness ?? null,
            cwd: known?.cwd ?? null,
            workspace: filed === OUTSIDE ? OUTSIDE_TITLE : filed,
            state: stateOf(states, session),
          };
        });
      }),
    [filedIn, plane, reopened, states, tabs],
  );

  // **This project's chats asking, for the title bar's list** (charter-app#249), which is the
  // window's and holds every project's. Their rows are the catalogue's own (`needsYouRows`),
  // asked on their own because the catalogue is built only for the project in front.
  const asking = useMemo<Asking[]>(() => {
    const reportsTo = (session: number) => states.reports[session] ?? [];
    const rows = catalogued(needsYouRows(states.needsYou, nameOf, tabs, reportsTo));
    return states.needsYou.map((session) => {
      const filed = filedIn(session);
      return {
        session,
        name: nameOf(session),
        reported: reportsTo(session),
        workspace: filed === OUTSIDE ? OUTSIDE_TITLE : filed,
        go: rows.get(showId(session)),
        ignore: rows.get(ignoreId(session)),
      };
    });
  }, [filedIn, nameOf, states.needsYou, states.reports, tabs]);

  // What this project has open, told to the window: the quit warning lists every project's
  // chats, and this project's own tab says when one of them needs you.
  //
  // **Its catalogue travels with it, and that is what keeps the palette one palette.** The
  // palette is mounted on the WINDOW — always, so `F2` reaches it before the core has said
  // which project this launch opened, and once, so a project that is not in front is not a
  // second capturing listener for the same key. What it lists has to be the project in
  // front's, and this is how it gets there.
  const mine = useMemo<PlaneReport>(
    () => ({
      ending,
      asking,
      quiet,
      settled,
      offers,
      run,
      said: report,
      // Whether the plane has been read: until it has, the window does not know which
      // workspace's theme to draw (`App.tsx`'s `settledInFront`).
      read: sidebar !== undefined,
      // Which workspace, by name, and its colour: the window draws that workspace's theme and
      // tints its accent with that colour while this project is in front (charter-app#281).
      workspace: ofWorkspace,
      colour: colourWithHue(sidebar?.workspaces.find((ws) => ws.name === ofWorkspace)?.colour),
      saving,
      moved: Math.max(0, ...Object.values(states.movedAt)),
    }),
    [
      asking,
      ending,
      ofWorkspace,
      offers,
      quiet,
      report,
      run,
      saving,
      settled,
      sidebar,
      states.movedAt,
    ],
  );
  // **Before the paint, not after it.** A quit — Cmd-Q, the tray, the menu — arrives whenever
  // it arrives, and the window decides on what every project has told it: a report that
  // landed a frame late would let a quit warn about nothing, or worse, end a chat it had not
  // heard about yet. `useLayoutEffect` puts this in the same flush as the render that
  // produced it, which is the nearest thing to the synchronous ref the single-project window
  // used before there was anything to report to.
  useLayoutEffect(() => {
    onReport(plane, mine);
  }, [mine, onReport, plane]);

  const frontChat = frontTab && reopened.find((chat) => chat.session === chatOf(tabs, frontTab.id));

  // Each strip is ONE Tab stop, the selected tab, and the arrows move along it (charter-app#189,
  // `roving.ts`). Asked here rather than below the early return, because they are hooks.
  const workspaceStop = useTabStop(focused, workspacesShown.shown);
  const chatStop = useTabStop(
    tabs.inFront === undefined ? undefined : String(tabs.inFront),
    shown.map(String),
  );

  // A project the operator is not looking at keeps every piece of state above and draws none
  // of it. See this module's own docstring for why it is `null` and not `hidden`.
  if (!inFront) return null;

  return (
    <>
      {/* The workspaces of this project, as the second of the three strips (ADR 0036). It is
          the axis the tmux frame had and the port lost: a top-level tab there was a
          WORKSPACE and the sessions lived under it, and transposing the app onto projects
          left the workspace as a heading in the sidebar that nothing selected.
          One tablist for the axis, and it is this one — the sidebar lists the same
          workspaces, but as a listing of what each holds rather than as a second answer to
          "which workspace am I in". */}
      {/* A `div` and not a `nav`, deliberately: the sidebar is already
          `nav[aria-label="Workspaces"]`, and a second landmark by that name is two answers
          to one query — for a screen reader and for every scenario spec that reaches the
          sidebar by it. The tablist is what this is. */}
      {/* Drawn once the plane is read, workspaces or none: its `+` is how the first one is
          made, so a strip that waited for a workspace hid the way to make one. */}
      {sidebar !== undefined && (
        <div className="workspaces">
          {/* Draggable along the strip (SI-6, `sortable.tsx`): a drop moves a pinned workspace
              among the pins, and one carried across the boundary pins or unpins it. */}
          <DndContext
            sensors={dragSensors}
            collisionDetection={closestCenter}
            modifiers={ALONG_THE_STRIP}
            accessibility={workspaceDragWords}
            onDragEnd={({ active, over }) => {
              if (over) dragWorkspace(String(active.id), String(over.id));
            }}
          >
            <SortableContext items={workspacesShown.shown} strategy={horizontalListSortingStrategy}>
              <RovingFocusGroup.Root asChild orientation="horizontal" {...workspaceStop}>
                <div
                  className="workspaces-strip"
                  role="tablist"
                  aria-label="Workspaces"
                  ref={workspaceStrip}
                  style={{ "--least": `${workspaceLeast}px` } as CSSProperties}
                >
                  {workspacesShown.shown.map((workspace) => {
                    const offer = by(`workspace.focus:${workspace}`);
                    return (
                      <SortableTab key={workspace} id={workspace} fixed={workspace === OUTSIDE}>
                        {({ sortable, style }) => (
                          /* Right-click is the third reader of the catalogue (`Menus.tsx`):
                             focus, pin, make one, and — under the line — delete this one.
                             `asChild`, so the strip gains no wrapper: the trigger IS the tab,
                             which is what #171's `flex: 1 1 0` cells require. */
                          <Menued
                            on={{ on: "workspace", workspace }}
                            offers={found}
                            onPress={press}
                          >
                            <RovingFocusGroup.Item
                              asChild
                              tabStopId={workspace}
                              active={workspace === focused}
                            >
                              <button
                                ref={sortable.setNodeRef}
                                role="tab"
                                aria-selected={workspace === focused}
                                aria-describedby={
                                  workspace === OUTSIDE
                                    ? undefined
                                    : sortable.attributes["aria-describedby"]
                                }
                                data-dragging={sortable.isDragging || undefined}
                                title={offer?.title}
                                // Its own colour, in front or not (charter-app#281): its shade and its
                                // mark are its tint, set on the tab and nowhere else.
                                data-colour={colourOf(workspace) ?? undefined}
                                style={{ ...tintOf(workspace), ...style }}
                                {...sortable.listeners}
                                onKeyDown={(event) => {
                                  keepsTheFocus(event, sortable.isDragging);
                                  sortable.listeners?.onKeyDown?.(event);
                                }}
                                onClick={() => {
                                  if (offer?.available) press(offer);
                                }}
                              >
                                {workspaceMarks(workspace)}
                              </button>
                            </RovingFocusGroup.Item>
                          </Menued>
                        )}
                      </SortableTab>
                    );
                  })}
                </div>
              </RovingFocusGroup.Root>
            </SortableContext>
          </DndContext>
          {/* This strip's own controls, in the shape the project strip above already has
              (`App.tsx`): the `+` that makes one more of what the strip lists, then what the
              strip is not drawing. `.strip-doing` and not a `.more` of its own, because the
              two strips now hold the same two things and a second class name would be a
              second place to dress them.

              **The `+` is the operator's, and it is drawing a control over a row that was
              already there** — *"also no new workspace button in workspaces tab — it should
              be like projects tabs buttons"* (charter-app#193). `workspace.create` has been in
              the catalogue since #172, with the dialog behind it; the palette runs it and the
              tab's own menu lists it, and the one strip that is entirely about workspaces had
              no way to make one. Nothing here knows what it does: it is one `Doer` over that
              row, so its words, its availability and its refusal are the catalogue's, exactly
              as they are in the other two surfaces that offer it.

              **`Plus`, which is the chat strip's glyph and not the project strip's pair.**
              charter-app#178 split `project.open` and `project.create` into `FolderOpen` and
              `FolderPlus` because that strip draws two of them an inch apart and one glyph on
              both is a strip aimed at by memory. There is one control here, and the rule it
              falls under is the older one: the `+` at the end of a strip makes one more of
              what the strip lists, which an operator learns once for all three. */}
          <div className="strip-doing">
            <Doer offer={by("workspace.create")} onPress={press} iconOnly />
            <ShowMore
              noun="workspace"
              hidden={workspacesNotShowing.map((workspace) => ({
                key: workspace,
                offer: by(`workspace.focus:${workspace}`),
                needs: waitingIn(workspace),
                children: workspaceMarks(workspace),
              }))}
              onPress={press}
            />
          </div>
        </div>
      )}

      {/* The chat strip is the focused workspace's, so it is drawn in that workspace's colour
          (charter-app#281): its shade, its selected tab and its accent. */}
      <header className="bar" style={tintOf(ofWorkspace)}>
        {/* The chats of the FOCUSED WORKSPACE (ADR 0036), which is what the tmux frame's
            sessions-under-a-workspace was. Named, because the projects and the workspaces
            above are tablists too and a query for `role="tab"` across the whole window
            would mix all three. */}
        {/* Draggable along the strip (SI-6, `sortable.tsx`): a drop moves a tab within its
            group, and one carried across the pinned boundary pins or unpins it. */}
        <DndContext
          sensors={dragSensors}
          collisionDetection={closestCenter}
          modifiers={ALONG_THE_STRIP}
          accessibility={chatDragWords}
          onDragEnd={({ active, over }) => {
            if (over) dragTab(Number(active.id), Number(over.id));
          }}
        >
          <SortableContext items={shown.map(String)} strategy={horizontalListSortingStrategy}>
            <RovingFocusGroup.Root asChild orientation="horizontal" {...chatStop}>
              <div
                className="tabs"
                role="tablist"
                aria-label="Tabs"
                ref={strip}
                style={{ "--least": `${chatLeast}px` } as CSSProperties}
              >
                {shown.map((id) => (
                  <SortableTab key={id} id={String(id)}>
                    {({ sortable, style }) => (
                      /* Right-click is the third reader of the catalogue (`Menus.tsx`).
                         `asChild`, so the strip gains no wrapper element: the trigger IS the
                         tab.

                         No `data-tab` and no scroll-into-view ref any more: #171 deleted
                         `offscreen.ts` and the strip collapses rather than scrolls, so there is
                         nothing to scroll a tab into and nothing measuring tabs through the
                         markup. */
                      <Menued on={{ on: "chat", tab: id }} offers={found} onPress={press}>
                        <span
                          className="tab"
                          ref={sortable.setNodeRef}
                          style={style}
                          data-dragging={sortable.isDragging || undefined}
                        >
                          {renaming === id ? (
                            // The name, open for editing in the tab's place (charter-app#254). Not
                            // inside the tab's button: an input inside a button is two controls in one.
                            <TabRename
                              name={tabs.byId[id].name}
                              onSave={(typed) => saveName(id, typed)}
                              onDone={endRename}
                            />
                          ) : (
                            <RovingFocusGroup.Item
                              asChild
                              tabStopId={String(id)}
                              active={id === tabs.inFront}
                            >
                              <button
                                role="tab"
                                aria-selected={id === tabs.inFront}
                                aria-describedby={sortable.attributes["aria-describedby"]}
                                // Where a handed-off chat came from, by its parent's name (charter-app#258).
                                title={handedFrom[chatOf(tabs, id) ?? -1]}
                                // F2 renames here rather than opening the palette (`RENAMES_ON_F2`), on a
                                // tab that has a rename row — a chat's, and never a view's.
                                {...(by(`tab.rename:${id}`) ? { [RENAMES_ON_F2]: "" } : {})}
                                {...sortable.listeners}
                                onKeyDown={(event) => {
                                  // A tab that is up is being carried: its keys are the drag's.
                                  keepsTheFocus(event, sortable.isDragging);
                                  sortable.listeners?.onKeyDown?.(event);
                                  if (sortable.isDragging) return;
                                  closeOnDelete(event, by(`tab.close:${id}`), press);
                                  renameOnF2(event, by(`tab.rename:${id}`), press);
                                }}
                                // The catalogue's row, not a second copy of it. The tab already in front
                                // has a row that says so and cannot run — a tab is never disabled, because
                                // the selected tab is the one a keyboard has to be able to land on.
                                onClick={() => {
                                  const offer = by(`tab.select:${id}`);
                                  if (offer?.available) press(offer);
                                }}
                                // A double-click on the name renames it — the same row again.
                                onDoubleClick={() => {
                                  const offer = by(`tab.rename:${id}`);
                                  if (offer?.available) press(offer);
                                }}
                              >
                                <TabMarks
                                  tabs={tabs}
                                  id={id}
                                  states={states}
                                  updates={planeUpdates}
                                  pin={
                                    <Pin
                                      held={isPinned(id)}
                                      what={chatOf(tabs, id) === undefined ? "tab" : "chat"}
                                    />
                                  }
                                />
                              </button>
                            </RovingFocusGroup.Item>
                          )}
                          <Closer offer={by(`tab.close:${id}`)} onPress={press} />
                        </span>
                      </Menued>
                    )}
                  </SortableTab>
                ))}
              </div>
            </RovingFocusGroup.Root>
          </SortableContext>
        </DndContext>
        {/* The affordance that says the strip is not showing everything (ADR 0039). It is
            the first thing on the strip that says how many tabs there are past the edge —
            a scroller never did, which is the premise ADR 0036 was missing. It is absent
            when nothing is hidden, because then there is nothing for it to say.

            **Outside the strip, beside the `+` and for the same reason.** A control that
            appears exactly when the strip is full must not live inside the thing that is
            full (charter-app#130/#131) — and now that the strip collapses rather than
            scrolls, "inside" would mean the `+` could be collapsed away. */}
        <div className="more">
          <ShowMore
            noun="tab"
            hidden={notShowing.map((id) => ({
              key: String(id),
              offer: by(`tab.select:${id}`),
              needs: waitingOn(id),
              children: <TabMarks tabs={tabs} id={id} states={states} updates={planeUpdates} />,
            }))}
            onPress={press}
          />
        </div>
        {/* **Outside the strip, and now the `+` at the end of it rather than a labelled
            button** — the operator's words: *"open-project button is not looks like separate
            button, but it should looks like new tab, without label — just icon"*, said of the
            project strip's `+` and true of this one too. Its accessible name is still the
            catalogue's `New tab`, which is what a screen reader reads and what
            `pressOnly("New tab")` finds.

            **`New tab` stays HERE and did not move onto a pane**, where the splits and the
            close went. A pane action acts on one pane and a window has several, which is the
            whole of why those moved; `New tab` acts on the strip and there is one of those.
            It is the same control as the `+` at the end of the project strip, one level in:
            the `+` at the end of a strip makes one more of what the strip lists.

            It was the strip's last child once, so at fifty chats the way to open the
            fifty-first was to scroll right to find it (charter-app#130). */}
        <div className="adding">
          <Doer offer={by("chat.new")} onPress={press} iconOnly />
        </div>
        {/* **`Split right`, `Split down` and `End this pane's chat` were here and are on the
            panes now** — the operator: *"harnesses panes should each have close button and
            spliting buttons in pane right top corner … so this will fully replace separate
            buttons Split right, Split left, Exit this pane's chat buttons, and this will be
            clear for spliting — user will know what pane is spliting."*

            The argument is the one he gives. All three act on THE FOCUSED PANE, and with a
            window split four ways the bar gives no sign of which that is: the operator reads
            the layout, works out where the keyboard went last, and presses a button somewhere
            else entirely. A control on the pane names its own target.

            They are still catalogue rows and still in the palette, which is what a keyboard
            without a pointer uses: the palette acts on the focused pane, and a pane's own
            button focuses that pane before it runs the same row. */}
        {/* **The region toggles are NOT here any more.** They are on the status line at the
            bottom of the window, icon-only (`StatusLine.tsx`, `RegionFrame`'s `RegionToggle`),
            where the operator asked for them twice — *"show hide buttons can be movet to
            bottom status bar — again like ZED"*. The rule is unchanged and travels with them:
            one button per region in the arrangement, in the order the window draws them.

            The project's path is not here either, for the same reason and since #172. */}
      </header>

      {trouble && (
        <p className="trouble" role="alert">
          {trouble}
        </p>
      )}

      {/* What happened to the chat in front when it was put back. Only a chat that came from
          the record has either, so a chat the operator just opened says nothing. */}
      {frontChat?.resumed && (
        <p className="came-back">
          <strong>{frontChat.name}</strong> was resumed — conversation{" "}
          <code>{frontChat.resumed}</code>
        </p>
      )}
      {/* Only for a harness. Every chat is a shell until the harness picker lands, and a
          shell has no conversation to bring back — saying so on every relaunch, forever,
          is noise about the normal case. */}
      {frontChat?.fresh && frontChat.harness && (
        <p className="came-back">
          <strong>{frontChat.name}</strong> came back as a new chat: {frontChat.fresh}
        </p>
      )}

      {/* A pin that no longer names a workspace. **Said and never drawn**: a strip that
          showed it would be offering a workspace the plane does not have, and a pin that
          vanished with no word is an arrangement the operator will make again and lose
          again. It is news rather than a fault, so it is not an alert. */}
      {danglingPins.length > 0 && (
        <p className="came-back" role="status">
          {danglingPins.length === 1
            ? `The pinned workspace ${danglingPins[0]} is not on this plane any more.`
            : `${danglingPins.length} pinned workspaces are not on this plane any more: ${danglingPins.join(", ")}.`}{" "}
          Unpin from the palette, or put the workspace back.
        </p>
      )}

      {wouldNotStart.map(([name, why]) => (
        <p className="came-back trouble" role="status" key={name}>
          <strong>{name}</strong> did not start ({why}). It is still recorded, and will be tried
          again at the next launch.
        </p>
      ))}

      {/* **The four regions** (ADR 0038): by default the explorer on the left, the
          panes in the middle, what is asking for you on the right, and what the repos are
          doing along the bottom. Every one of them resizes, and each of the three around the
          centre can be put away — the centre cannot, because the terminal panes are the
          product.

          **By default, and no longer by shape.** Which side each region is on, what order it
          is in and how big it is are the arrangement (`regions.ts`); this is the content that
          goes in whichever slot the arrangement names.

          One workspace answer for all three (`useWorkspaceState`), not one per region: they
          draw the same workspace, and `workspace_repos` runs `git status` per clone. */}
      <RegionFrame
        arrangement={arrangement}
        onResized={resized}
        content={{
          explorer: (
            <Explorer
              workspace={ofWorkspace}
              live={ofWorkspace !== undefined && liveOf(ofWorkspace)}
              state={workspaceState}
              chats={workspaceChats}
              states={states}
              spot={spot}
              onPick={pickSpot}
              onShowChat={showChat}
              offers={found}
              onPress={press}
            />
          ),
          aside: (
            <Panels
              workspace={ofWorkspace}
              state={workspaceState}
              offers={found}
              onPress={press}
              contributed={contributed}
              views={views}
              shownRow={shownRow}
              onShowRow={setShownRow}
              vaults={vaults}
              onAddTodo={edits.addTodo}
            />
          ),
          bottom: (
            <BottomBar
              workspace={ofWorkspace}
              state={workspaceState}
              offers={found}
              onPress={press}
              columns={facts.columns}
            />
          ),
        }}
        centre={
          /* The centre is where a chat is, so its menu is the chat verbs the bar has: a new
             tab, the two splits, the key the palette claimed, and — under the line — ending
             this pane's chat. `asChild` again: the panes' box is measured, and it must not
             gain a wrapper. */
          <Menued on={{ on: "pane" }} offers={found} onPress={press}>
            <div className="panes">
              {frontTab ? (
                <LayoutPanes
                  plane={plane}
                  layout={frontTab.layout}
                  focused={frontTab.focused}
                  onFocus={(pane) => change((tabs) => focusPane(tabs, pane))}
                  offerFor={by}
                  onPaneDoes={onPaneDoes}
                  states={states}
                  name={frontTab.name}
                  handedFrom={handedFrom}
                  offered={views}
                  onOpenView={showView}
                  onAsk={(pane) => change((tabs) => stopWaiting(tabs, pane))}
                  onVaultChanged={reloadVaults}
                />
              ) : tabs.order.length > 0 ? (
                // Chats are running — just not in the workspace being looked at. Saying
                // "no sessions" here would be charter telling the operator that what it is
                // still drawing on the strip above does not exist.
                <EmptyState
                  mark={MessageSquarePlus}
                  headline="No chats in this workspace"
                  body="Other workspaces have some — pick one on the strip above, or start one here."
                  action={<Doer offer={by("chat.new")} onPress={press} words="Open a chat here" />}
                  testid="empty-workspace"
                />
              ) : /* **The empty window, centred, with a way out** — the operator's own
                   instruction: *"when opening empty workspace lets make open new tab button on
                   empty page center"*. It was one sentence in the top-left corner of a box the
                   size of the screen, and the thing to do about it was a menu item away.

                   The button is the catalogue's `chat.new` row drawn by `Doer`, so it carries
                   the same words the bar's button and the palette's row carry, and it stops
                   existing if the catalogue stops offering it. A second button with its own
                   label would be the second answer to "how do I start a chat" that
                   `actions.ts` exists to prevent. */
              sidebar !== undefined && sidebar.workspaces.length === 0 ? (
                /* **A plane with no workspace yet**: making one is the first thing to do,
                     and it was reachable only from the palette. Both buttons are catalogue
                     rows, as the one below is; a chat here starts in the plane itself. */
                <EmptyState
                  mark={FolderPlus}
                  headline="No workspaces yet"
                  body="A workspace holds the repos you work on and the chats about them."
                  action={
                    <>
                      <Doer
                        offer={by("workspace.create")}
                        onPress={press}
                        words="Create a workspace"
                      />
                      <Doer
                        offer={by("chat.new")}
                        onPress={press}
                        words="Open a chat in the plane"
                      />
                    </>
                  }
                  testid="empty-plane"
                />
              ) : (
                <EmptyState
                  mark={MessageSquarePlus}
                  headline="No chats yet"
                  body="charter runs each chat in its own pane. Open the first one here."
                  action={
                    <Doer offer={by("chat.new")} onPress={press} words="Open the first chat" />
                  }
                  testid="empty-window"
                />
              )}
            </div>
          </Menued>
        }
      />

      {/* **charter's status line**, under everything including the bottom region. It is not
          in the arrangement and `StatusLine.tsx` argues why at length: a slot is sized as a
          percentage of its group and this is one line of text, a region can be put away and
          this must not be, and the window is chrome · four regions · chrome — the project
          strip above is not a region either.

          It reads what is already known. `workspaceState` is the one ask the three regions
          share, and the sidebar has already been read for the strip, so the line costs no
          command of its own — which matters here more than anywhere, because it is the one
          surface that is drawn whatever else the window is doing. */}
      <StatusLine
        plane={plane}
        read={sidebar !== undefined}
        where={focused === OUTSIDE ? OUTSIDE_TITLE : focused}
        workspaces={sidebar?.workspaces.length}
        running={runningIn({ ending, settled })}
        state={workspaceState}
        doctor={doctor}
        pin={pin}
        alerts={alerts}
        badges={facts.badges}
        factNotes={facts.notes}
        /* Which regions are drawn (ADR 0038), handed over as the arrangement already reads
           them. **The slots are flattened here and not there**: the arrangement is this
           project's, `inSlots` is the module that knows what order a side's regions come in,
           and a status line that sorted regions would be a second place that decides. */
        regions={{
          placed: SIDES.flatMap((side) => slots[side]),
          onToggle: toggleRegion,
        }}
      />

      {/* Making a workspace, and deleting one. Mounted only while they are up, and drawn
          here rather than in the window: a workspace belongs to a project. */}
      {makingWorkspace && (
        <NewWorkspace
          plane={plane}
          trouble={workspaceTrouble}
          making={busyMaking}
          planeId={plane}
          onCreate={(name, vision, live, repos) => void makeWorkspace(name, vision, live, repos)}
          onCancel={() => {
            setMakingWorkspace(false);
            setWorkspaceTrouble(undefined);
          }}
        />
      )}

      {makingVault && (
        <NewVault
          plane={plane}
          trouble={vaultTrouble}
          making={busyVault}
          onCreate={(name, provider, opVault) => void makeVault(name, provider, opVault)}
          onCancel={() => {
            setMakingVault(false);
            setVaultTrouble(undefined);
          }}
        />
      )}

      {edits.dialogs}

      {pickingVault && (
        <OpenVault
          vaults={vaults.vaults ?? []}
          offers={found}
          onPress={press}
          onCancel={() => setPickingVault(false)}
        />
      )}

      {askingAction && (
        <AskFirst
          extension={askingAction.extension}
          action={askingAction.action}
          onRun={async () => {
            const outcome = await runExtensionAction(
              plane,
              askingAction.extension,
              askingAction.action,
              { view: null, key: "", row: null },
              ofWorkspace,
              true,
            );
            if ("refused" in outcome) return { refused: outcome.refused };
            // From the palette there is no view to say it above, so the dialog says it.
            if (outcome.answer.overreach !== null) return { seen: outcome.answer.overreach };
            setAskingAction(undefined);
            return undefined;
          }}
          onCancel={() => setAskingAction(undefined)}
        />
      )}

      {liveAsk !== undefined && (
        <LiveDialog
          plane={plane}
          workspace={liveAsk}
          onClose={() => setLiveAsk(undefined)}
          onDone={(said) => {
            setLiveAsk(undefined);
            setReport({ from: `workspace.live:${liveAsk}`, refused: false, words: said.join(" ") });
            // Read the plane again: the marks come from what is on disk, not from this press.
            setReplan((asked) => asked + 1);
          }}
        />
      )}

      {renamingWs && (
        <RenameWorkspace
          workspace={renamingWs.workspace}
          startsFresh={renamingWs.startsFresh}
          trouble={renamingWs.trouble}
          renaming={renamingWs.busy}
          onRename={(name) => void doRename(renamingWs.workspace, name)}
          onCancel={() => setRenamingWs(undefined)}
        />
      )}

      {removing && (
        <DeleteWorkspace
          workspace={removing.workspace}
          atRisk={removing.atRisk}
          unreadable={removing.unreadable}
          refusal={removing.refusal}
          deleting={removing.busy}
          onDelete={(force) => void deleteWorkspace(removing.workspace, force)}
          onCancel={() => setRemoving(undefined)}
        />
      )}

      {/* The one question charter asks before it ends a chat, wherever the row was pressed
          (the operator: *"closing session should ask confirmation"*). */}
      {endingChat && (
        <EndingChat
          offer={endingChat}
          onEnd={() => {
            const ending = endingChat;
            setEndingChat(undefined);
            void carryOut(ending);
          }}
          onCancel={() => setEndingChat(undefined)}
        />
      )}

      {picking && (
        <StartChat
          options={picking.options}
          trouble={pickerTrouble}
          onStart={(profile, persona, footer, label) =>
            void startPicked(profile, persona, footer, label)
          }
          onApprove={(profile, persona, footer, shown, label) =>
            void approveAndStart(profile, persona, footer, shown, label)
          }
          onCancel={() => {
            setPicking(undefined);
            setPickerTrouble(undefined);
          }}
        />
      )}
    </>
  );
}

/** What this project told the window about itself. */
/** Redraws a component when the theme in force changes: what a workspace's tint is taken from
 *  (charter-app#281). */
function followTheme(changed: () => void): () => void {
  return onDrawn(() => changed());
}

/** A workspace's colour when it has a hue to tint with, else `null` (charter-app#281). */
function colourWithHue(colour: string | null | undefined): string | null {
  return hueOf(colour) === undefined ? null : (colour ?? null);
}

export type PlaneReport = {
  /** Every chat it has open, with what each one is doing — what a quit would end. */
  ending: Ending[];
  /** Everything this project can do, for the window's one palette to list. */
  offers: Offer[];
  /** How the window carries a row out: this project's own dispatcher, so a row the palette
   *  runs reaches this project's live arrangement and no other's. */
  run: (offer: Offer) => Promise<Ran>;
  /** What its last action answered, drawn by the window beside the palette. */
  said?: { from: string; refused: boolean; words: string };
  /** Its chats asking for the operator: for its own tab to count, and for the title bar's
   *  list (charter-app#249). */
  asking: Asking[];
  /** Its chats that can be waiting without saying so (charter-app#52), by name: the title
   *  bar's faint hand. */
  quiet: readonly string[];
  /** Whether the core has answered what it already had open. Until it has, "no tabs" is
   *  "not yet", and a quit that read it as "nothing is running" would end the lot. */
  settled: boolean;
  /** Whether its plane has been read at all: until it has, which workspace it is on is not
   *  known, and the window waits before drawing that workspace's theme. */
  read: boolean;
  /** The workspace it is on by its name, `undefined` outside every workspace: whose
   *  `workspace.json` is a layer of the theme the window draws (charter-app#281). */
  workspace?: string;
  /** That workspace's colour, a palette name or `#rrggbb`, or `null`: what the window's accent
   *  and focus ring are tinted with while it is in front (charter-app#281). */
  colour?: string | null;
  /** Where this project's unsaved work sits (charter-app#302), once read. */
  saving?: PlaneSaving;
  /**
   * When anything in it last moved: the newest `movedAt` among its chats, `0` when nothing
   * has been heard. What the project strip's show-more menu orders its rows by after the ones
   * that need you (ADR 0054, charter#401). The core counts moves across every project's
   * board with one count, so this compares across projects.
   */
  moved: number;
};

/** What a project asks the WINDOW to do, because the window is what holds projects. */
export type WindowDoing = {
  openProject: () => void;
  /** Shows the dialog that makes a new project. The window's, like the opener: what it ends in
   *  is another project tab, and the open it ends in is the gated one (ADR 0035). */
  createProject: () => void;
  /** Shows what has contributed what to this window. The window's and not a project's: an
   *  extension is machine state (ADR 0041), so it is the same list behind every tab. */
  showExtensions: () => void;
  /** Puts the app's `charter` on a terminal's PATH. The window's: it is about the machine. */
  installCli: () => Promise<Ran>;
  selectProject: (plane: string) => void;
  closeProject: (plane: string) => Promise<Ran>;
  /** Moves a project into another window, or a new one (charter#126). The window's, because
   *  the window is what holds projects. */
  moveProject: (plane: string, to: string | null) => Promise<Ran>;
  /** Brings a project to the front and opens its Project settings tab (charter-app#252). The
   *  window's, because the project may not be the one in front, and only the window can bring
   *  it there. */
  openSettings: (plane: string) => void;
  /** Brings a project to the front and opens its Saving tab (charter-app#294). */
  openSaving: (plane: string) => void;
  /** Opens the Preferences tab (charter-app#283) on the project in front, or draws it where the
   *  opener is when there is none. The window's, because which project is in front is. */
  openPreferences: () => void;
  /** Pinning a PROJECT is the window's, because the project strip is: a project that is not
   *  in front draws nothing, and its pin still has to be on that strip (ADR 0039). */
  pinProject: (plane: string, pinned: boolean) => Promise<Ran>;
  quit: () => void;
};

/** The size a session starts at. The pane it lands in tells it the real one at once. */
const STARTING_SIZE = { columns: 80, rows: 24 };

/** What the core says when a handoff has opened a chat — `handoff::Arrived` in the app. */
type Arrived = {
  plane: string;
  session: number;
  name: string;
  workspace: string;
  persona: string | null;
  /** The harness it runs, for its default name when it adopted no persona. */
  harness?: string | null;
  /** The task name the handoff gave it, which its tab says instead (charter-app#258). */
  label?: string | null;
  /** The chat it was handed off from, by name, and that chat's workspace. */
  from?: HandedFrom | null;
};

/**
 * What a chat's default name puts before its number (charter-app#254): the persona it adopted,
 * or — with none — the program it runs, by the word the plane calls its harness. `steward 3`,
 * `claude 4`. Not the profile's name, which is the operator's word for an account (`work 4`
 * would name the account, not the program). A chat on neither is a shell, and says its own
 * name alone, as it always has. Every path a tab opens by goes through here.
 */
function whoOf(persona: string | null, harness: string | null | undefined): string | null {
  return persona ?? harness ?? null;
}

/** Whether any tab already shows `session`, in any of its panes. */
function alreadyShows(tabs: Tabs, session: number): boolean {
  return tabs.order.some((id) => panesOf(tabs, id).some((pane) => pane.session === session));
}

/**
 * Whether carrying a row out ends a chat, and therefore whether it is asked about first.
 *
 * **By what the row does and not by its id**, so a row added to the catalogue that ends a chat
 * is asked about without anybody remembering to add it here — the same rule the region toggles
 * follow. A close says whether it ends one (`Does.ends`): a pane or a tab showing only a view
 * closes, kills nothing, and is not asked about. `closeProject` is deliberately not one of
 * them: it ends every chat in a project, and the window is where that question belongs — it
 * asks it there (`ClosingProject`, charter-app#239) whenever the project has chats open.
 */
function endsAChat(does: Offer["does"]): boolean {
  return (does.verb === "closeTab" || does.verb === "closePane") && does.ends;
}

/**
 * Whether a row is a close that says it ends nothing — a pane or a tab showing only a view.
 *
 * Asked the other way round from {@link endsAChat} on purpose: the danger look stays on every
 * close that has not said it is harmless, including a row the catalogue cannot run right now,
 * because a look that goes quiet when a row is merely unavailable would teach the operator the
 * wrong thing about the day it is available.
 */
function closesOnly(does: Offer["does"]): boolean {
  return (does.verb === "closeTab" || does.verb === "closePane") && !does.ends;
}

/** A view tab the core put back, as the view it names. */
function refOf(view: ViewTab): ViewRef {
  return { from: view.from, view: view.view, key: view.key };
}

/**
 * The view tabs, as the record keeps them: every tab whose first pane is a view.
 *
 * A view on the far side of a split is not a tab of its own and is not recorded — a chat's
 * split is not recorded either, and both come back as what they are: the chat as its own tab,
 * the view as nothing, to be opened again.
 */
function viewTabsOf(tabs: Tabs, pinnedViews: readonly string[]): ViewTab[] {
  return tabs.order.flatMap((id, at) => {
    const lead = contentsOf(tabs, id)[0]?.content;
    if (lead?.kind !== "view") return [];
    return [
      {
        from: lead.view.from,
        view: lead.view.view,
        key: lead.view.key,
        title: tabs.byId[id].name,
        workspace: lead.workspace === OUTSIDE ? null : lead.workspace,
        at,
        active: tabs.inFront === id,
        pinned: pinnedViews.includes(viewKey(lead.view)),
      },
    ];
  });
}

/**
 * One pane's frame: the terminal, and what charter draws over it in the pane's two corners —
 * side by side with the terminal, so neither is ever a child of the element xterm draws into.
 *
 * **Two corners, and neither thing in them places itself** (charter-app#193). The gauge is
 * top-left and the controls top-right — the operator: *"context status indicator in pane right
 * corner can be moved to left corner. to not make split and close buttons uggly"*. They shared
 * one row in the right corner before that, because each had been written as the thing in the
 * pane's top-right. A corner positions; its contents do not, so a third thing arriving here
 * collides in review rather than at runtime. The controls keep their hover rule; the gauge,
 * always drawn, is outside it.
 *
 * **Both corners float over the terminal and neither takes a row.** #207 gave the gauge a row
 * of its own, and a pane with a gauge was then a row shorter than one without — the operator:
 * *"its changing harness container sizes"*. A pane's size is the layout's business alone.
 */
function PaneFrame({
  plane,
  session,
  moved,
  running,
  from,
  doing,
  children,
}: {
  plane: PlaneId;
  session: number;
  moved: number;
  running: boolean;
  /** Where a handed-off chat came from, `↳ from steward 3 · ops`, in the chat's own corner. */
  from?: string;
  doing: ReactNode;
  children: ReactNode;
}) {
  const usage = useChatUsage(plane, session, moved, running);
  return (
    <div className="pane-frame">
      {/* **The corners before the terminal, in the document**, although they are drawn over
          it: the tab order is the document's, and a terminal keeps Tab for its shell, so a
          control written after it is one Tab never reaches (charter-app#189). The pane's own
          controls are in its top corner, which is where reading order puts them anyway. */}
      <div className="pane-corner at-start">
        <ChatGauge usage={usage} />
        {from && <span className="pane-from">{from}</span>}
      </div>
      <div className="pane-corner at-end">{doing}</div>
      {children}
    </div>
  );
}

/** A button that IS a row of the catalogue: its words, its availability and its reason.
 *
 *  Nothing is drawn for an id the catalogue no longer has. That is the point: the bar cannot
 *  keep offering something the one list has stopped offering, because there is no second
 *  place for the words to live. */
export function Doer({
  offer,
  onPress,
  iconOnly,
  words,
}: {
  offer?: Offer;
  onPress: (offer: Offer) => void;
  /**
   * What this button SAYS, where the surface it is on asks a different question from the bar.
   *
   * **It overrides the words and nothing else.** What the row does, whether it can run and why
   * not are still the catalogue's, and a row the catalogue has stopped offering still draws no
   * button — which is the whole of what `actions.ts` being the one list buys. What it does not
   * buy, and never claimed to, is that one verb has one phrasing on every surface.
   *
   * It exists for the empty state, and the argument is an accessibility one rather than a
   * stylistic one: the strip's `+` is already named `New tab`, and a second button with the
   * identical accessible name on the same screen is two controls a screen reader cannot tell
   * apart. A toolbar button says *what this control is*; a call to action in the middle of an
   * empty page says *what to do now*. Those are different sentences about one verb.
   */
  words?: string;
  /**
   * Drawn as its mark alone, with the row's words carried by `aria-label`.
   *
   * **For the `+` at the end of a strip, and nothing else.** `docs/design-system.md` says an
   * icon goes *beside* words and never instead of them, with one exception — a control whose
   * accessible name is already `aria-label` — and this is that exception said out loud rather
   * than a second rule. It is the operator's own instruction for the project strip's opener
   * ("just icon"), and a `+` at the end of a row of tabs is the one glyph in this window that
   * every operator already reads, from every browser and from Zed.
   *
   * A row with no mark in `MARKS` keeps its words even here: an icon-only button with no icon
   * is an empty box, and the right way to fail is to look wrong rather than to disappear.
   */
  iconOnly?: boolean;
}) {
  if (!offer) return null;
  const Mark = MARKS[offer.id];
  const bare = iconOnly && Mark !== undefined;
  return (
    <button
      className={clsx(
        offer.id === "pane.close" && !closesOnly(offer.does) && "ends-a-chat",
        bare && "bare",
      )}
      // WebKit leaves a `<button>` out of the tab sequence unless its `tabindex` is written down
      // (`docs/ui-primitives.md`, charter-app#189).
      tabIndex={0}
      disabled={!offer.available}
      aria-label={bare ? (words ?? offer.title) : undefined}
      title={offer.reason || offer.note || (bare ? offer.title : undefined)}
      onClick={() => onPress(offer)}
    >
      {Mark && <Mark />}
      {!bare && (words ?? offer.title)}
    </button>
  );
}

/**
 * The icon beside each of the bar's own buttons, by catalogue row.
 *
 * **Beside the words, never instead of them.** An icon-only bar is a bar an operator has to
 * learn, and the words are what `pressOnly("New tab")` and a screen reader find — Lucide hides
 * a nameless icon from assistive technology by itself, so each button's name is its title
 * exactly as before. A row with no entry here draws its words alone, which is the right way to
 * fail: a missing icon is cosmetic, a missing button is not.
 *
 * `pane.close` ends a chat, so its mark is the same `X` a tab's close carries and it gets the
 * same danger hover (`App.css`, `.ends-a-chat`) — an icon may not make ending a chat look
 * lighter than it is.
 *
 * **The project strip draws two of these side by side, so they may not be the same glyph**
 * (charter-app#178). `FolderPlus` is the folder-with-a-plus every file manager puts on *New
 * folder*, and `project.create` is the row that writes a directory that was not there; opening
 * one that already exists is `FolderOpen`, which is that same universal pair's other half.
 * `project.open` wore `FolderPlus` only because it was the strip's one control when #171 drew
 * it — two icon-only buttons an inch apart carrying one glyph is a strip an operator has to
 * aim at by memory.
 *
 * **`workspace.create` and `chat.new` share `Plus`, and that is the rule rather than an
 * oversight of the one above** (charter-app#193). Each is the ONE control at the end of its
 * own strip, a whole row apart from the other, and what both mean is the same thing: make one
 * more of what this strip lists. #178's rule is about two controls side by side; this is the
 * `+` an operator learns once and then reads on every strip in the window.
 */
export const MARKS: Record<string, typeof Plus> = {
  "chat.new": Plus,
  "workspace.create": Plus,
  "pane.split.right": SquareSplitHorizontal,
  "pane.split.down": SquareSplitVertical,
  "pane.close": X,
  "project.create": FolderPlus,
  "project.open": FolderOpen,
};

/**
 * The show-more menu: the tabs the strip is not showing, most recently moved first.
 *
 * **It is not a find surface, and if it is built as one it should not have been built**
 * (ADR 0039). The palette lists every chat with a search and a ranking over it and is better
 * at finding than any menu will be. This lists what the strip is hiding and nothing else, so
 * that the strip has an affordance saying there is more — which a scrollbar never was.
 *
 * **Sorted by last activity — here, and nowhere else.** The strip's own order never moves
 * (`tabs.ts`), because a tab that moves under the cursor breaks aiming. A menu is a list you
 * read rather than a surface you aim at, and the boundary between the two rules is exactly
 * whether the thing moves under your hand.
 *
 * **And it carries the needs-you count of everything it hides** (ADR 0054). The operator's
 * constraint was that hiding a workspace must never hide a chat that needs you, so the
 * button draws the sum of its rows' counts in the red their own tabs draw it in, says it in
 * its name, and lists the rows that need you first. Nothing hidden is moved onto the strip
 * because it needs you: that would move tabs under the operator's hand, which is the one
 * thing ADR 0039 refuses, and the count and the title bar's ✋ menu already say it.
 *
 * **After those, the caller's order**, which on all three strips is most recently moved first:
 * a tab by its chats' newest move, a workspace by its chats', and a project by the newest move
 * its `PlaneView` reports (`PlaneReport.moved`, charter#401).
 *
 * **Only rows that bring a tab forward.** Every row is the catalogue's `tab.select:<id>`,
 * which is the same row the tab itself is and the same row the palette lists. Nothing
 * destructive is in here: a tab's `×` sits under the pointer on a surface the operator chose
 * to open, and a menu that pops up under the cursor with `End chat` in it is charter-app#130's
 * defect with a mouse attached.
 *
 * Radix's menu (ADR 0037, `docs/ui-primitives.md`), so the keyboard is the primitive's and not
 * a fifth hand-written `ArrowDown`.
 */
export function ShowMore({
  noun,
  hidden,
  onPress,
}: {
  /**
   * What one of these is, for the button's own words: `tab`, `workspace`, `project`.
   *
   * **Three strips, three nouns, one component.** A window drawing three of these owes an
   * operator — and a scenario spec — an answer to which strip is not showing everything, and
   * three buttons all saying "Show 3 more" is three answers to one query. `tab` is the chat
   * strip's, unchanged, because that is the name the operator reads on that strip.
   */
  noun: string;
  /** What to list, in the order it is listed in among the rows that need you and among the
   *  rows that do not. The first come first, whatever the order they were handed in. */
  hidden: readonly Hidden[];
  onPress: (offer: Offer) => void;
}) {
  /**
   * Whether the menu is up.
   *
   * **Held here rather than left to Radix, and the reason is measured.** Radix opens a menu
   * on `pointerdown`, which is right for a mouse and is not what every way of pressing a
   * button produces: the WebView the scenario tests drive answers a click with no pointer
   * event at all, so the menu never opened and the run reported "0 menus opened" on both
   * platforms. A control an automated press cannot open is one some input method cannot
   * open. So the trigger's `pointerdown` is refused — `composeEventHandlers` skips Radix's
   * own handler once the event is prevented — and the click is what toggles it.
   */
  const [open, setOpen] = useState(false);
  // The strip has just started hiding tabs, as opposed to having been hiding them when this
  // strip was drawn: only the first is a change worth drawing (`useArrived`, and the motion
  // section of `App.css`).
  const arrived = useArrived(hidden.length > 0);
  // Nothing is hidden, so there is nothing to say there is more OF.
  if (hidden.length === 0) return null;
  const many = hidden.length === 1 ? `1 ${noun}` : `${hidden.length} ${noun}s`;
  // What is waiting behind it, from the same queue each row's own count is read from.
  const needs = hidden.reduce((sum, one) => sum + one.needs, 0);
  const needsSaid =
    needs === 0 ? "" : `, where ${needs === 1 ? "1 chat needs" : `${needs} chats need`} you`;
  // What needs you first, and the caller's order inside each half: `sort` is stable.
  const listed = [...hidden].sort((one, other) => Number(other.needs > 0) - Number(one.needs > 0));
  return (
    // **Not modal.** A modal Radix surface marks the rest of the window `aria-hidden` (which
    // `docs/ui-primitives.md` records the dialogs doing), and this is a menu on a strip, not
    // a question that has to be answered before the window can be used again. A click outside
    // closes it, which is what every menu on every platform does — the dialogs' opposite rule
    // is about a surface that would lose an answer, and there is no answer to lose here.
    <Menu.Root modal={false} open={open} onOpenChange={setOpen}>
      <Menu.Trigger asChild>
        <button
          className={arrived ? "show-more arrived" : "show-more"}
          aria-label={`Show ${many} the strip is not showing${needsSaid}`}
          tabIndex={0}
          onPointerDown={(event) => event.preventDefault()}
          onClick={() => setOpen((up) => !up)}
        >
          {hidden.length} more
          {needs > 0 && <span className="show-more-needs">{needs}</span>}
          <ChevronDown />
        </button>
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Content className="more-menu" align="end" sideOffset={4} collisionPadding={8}>
          {listed.map(({ key, offer, children }) => {
            if (!offer) return null;
            return (
              <Menu.Item
                key={key}
                className="more-tab"
                disabled={!offer.available}
                title={offer.reason || undefined}
                onSelect={() => onPress(offer)}
              >
                {children}
              </Menu.Item>
            );
          })}
        </Menu.Content>
      </Menu.Portal>
    </Menu.Root>
  );
}

/** One row of a show-more menu: what it is drawn as, and the catalogue row it carries out.
 *
 *  **The strip and the menu draw the same thing**, so the caller hands the same markup to
 *  both rather than the menu having a second idea of what a workspace looks like. */
export type Hidden = {
  key: string;
  /** The row that brings it forward. Nothing is listed for an id the catalogue has dropped. */
  offer?: Offer;
  /** How many chats in it need you, read from the same queue as the tab counts: a
   *  workspace's or a project's is its tab's own count, and a chat tab's is how many of its
   *  panes' chats are in the queue. */
  needs: number;
  children: ReactNode;
};

/**
 * The mark on something the operator pinned (ADR 0039).
 *
 * **On the workspace strip a pin is also what puts a tab there at all** (ADR 0054): that strip
 * draws the pinned workspaces and the one you are in. On the other two a pin draws its tab
 * first.
 *
 * **A mark and not a button, and that is the whole of pinning's surface on a strip.** A `📌`
 * control on every tab is fifty more controls on the one strip that already broke at fifty
 * (charter-app#130), and a pin is a deliberate, occasional act — which is what the palette is
 * for. So the rows live in `actions.ts` like every other action, the palette is where they
 * are run, and this says which things carry one.
 *
 * **Lucide's pin, and not the `📌` this comment used to refuse.** The objection was to an
 * emoji — drawn by the operating system at its own size and in its own colours, louder than
 * the state dot beside it. A Lucide icon is none of those: a stroke in `currentColor` at
 * `1em`, so it is the accent colour the old dot was, at the size of the text it sits in.
 *
 * It is inside the tab's own button, so it can never be a second thing to click by accident
 * and there is no interactive element inside an interactive element for a screen reader to
 * have to explain. The glyph is decorative; `aria-label` is what carries the meaning, the
 * same split `ChatState` makes.
 */
export function Pin({ held, what }: { held: boolean; what: string }) {
  if (!held) return null;
  return (
    <span
      className="pinned"
      role="img"
      aria-label={`pinned ${what}`}
      title={`Pinned. Unpin it from the palette — yours, on this machine only.`}
    >
      <PinMark />
    </span>
  );
}

/** A tab's close button. The same row the palette lists, drawn as the `×` a pointer wants —
 *  so the accessible name is the catalogue's words and the glyph is only the glyph.
 *
 *  **The words are the whole guard** (charter-app#130). This `×` ends a chat: it calls
 *  `close_session`, which ends the program and takes the chat off the board. The glyph reads
 *  as "hide this tab" and there is no undo, so the name a screen reader and a keyboard get is
 *  `End chat 3 steward`, and the tooltip a pointer gets says what that costs. */
export function Closer({ offer, onPress }: { offer?: Offer; onPress: (offer: Offer) => void }) {
  if (!offer) return null;
  return (
    <button
      // A tab showing only a view closes and ends nothing, so its `×` does not wear the danger
      // hover a chat's does — the look may not say more than the act does, either way.
      className={clsx("closer", closesOnly(offer.does) && "keeps")}
      // **Not a Tab stop, and deliberately** (charter-app#189). The strip it sits on is ONE
      // stop, the WAI-ARIA "Tabs" pattern; a `×` per tab in the sequence would be fifty stops
      // again. A keyboard ends a chat from the palette's row for it, or from the tab's own menu
      // (the context-menu key), both of which read the same catalogue row this does — and
      // Delete on the focused tab (charter-app#239, `closeOnDelete`), which a Mac needs.
      tabIndex={-1}
      aria-label={offer.title}
      title={offer.note ? `${offer.title} — ${offer.note}` : offer.title}
      onClick={() => onPress(offer)}
    >
      <X />
    </button>
  );
}

/**
 * What a tab says about itself, on the strip and in the menu of what the strip has no room for.
 *
 * **A chat's tab is its name and what it is doing; a view's tab is a mark and its name**, with
 * no state — a view is not doing anything, and a dot beside it would be a claim about a chat
 * the tab does not have. The mark says what kind of thing the tab holds before the name is
 * read: a person for a persona, a puzzle piece for a view an extension offers.
 */
function TabMarks({
  tabs,
  id,
  states,
  updates,
  pin,
}: {
  tabs: Tabs;
  id: number;
  states: ChatStates;
  /** The chats the plane's instructions changed under, by session (charter#369). */
  updates: PlaneUpdates;
  /** The pin mark, on the strip; the menu of hidden tabs draws none. */
  pin?: ReactNode;
}) {
  const lead = contentsOf(tabs, id)[0]?.content;
  const chat = chatOf(tabs, id);
  if (lead?.kind === "view") {
    return (
      <>
        <ViewMark view={lead.view} />
        <span className="tab-name">{tabs.byId[id].name}</span>
        {pin}
      </>
    );
  }
  return (
    <>
      <span className="tab-name">{tabs.byId[id].name}</span>
      {pin}
      <PlaneUpdatedMark files={chat === undefined ? undefined : updates[chat]} />
      {/* The first pane's session is the tab's own chat. Its own element, so what a tab IS
          stays separate from what it is DOING — a tab whose text changed every time a turn
          began would be unreadable, and untestable. */}
      <ChatState state={stateOf(states, chatOf(tabs, id) ?? -1)} />
    </>
  );
}

/** A tab's layout, as panes with a handle between each split. */
function LayoutPanes({
  plane,
  layout,
  focused,
  onFocus,
  offerFor,
  onPaneDoes,
  states,
  name,
  handedFrom,
  offered,
  onOpenView,
  onAsk,
  onVaultChanged,
}: {
  /** Which plane's sessions these panes are showing. A session number belongs to a plane,
   *  and every command a pane makes carries it. */
  plane: PlaneId;
  layout: Layout;
  focused: number;
  onFocus: (pane: number) => void;
  /** The catalogue, by row id. There is one list of actions and the panes read it too. */
  offerFor: (id: string) => Offer | undefined;
  onPaneDoes: (pane: number, offer: Offer | undefined) => void;
  /** What every chat is doing, for the gauge in each pane's corner: it reads its record
   *  again when its chat moves, and keeps reading while the chat is mid-turn. */
  states: ChatStates;
  /** The tab's name, which is the title of the view it opened on. */
  name: string;
  /** Where each handed-off chat came from, by session (charter-app#258). */
  handedFrom: Readonly<Record<number, string>>;
  /** The views approved extensions offer, for the buttons a view draws beside itself. */
  offered: readonly ExtensionView[];
  onOpenView: (view: ViewRef, title: string) => void;
  /** The operator pressed to have a waiting view in this pane asked. */
  onAsk: (pane: number) => void;
  /** A vault's tab wrote to its vault: the plane's vault list is read again. */
  onVaultChanged: () => void;
}) {
  if (layout.kind === "pane") {
    const content = layout.content;
    if (content.kind === "view") {
      // **A view in a pane, with the pane's own corner** — split and close are the same rows a
      // chat's pane has, so a view can be split to start a chat beside it and closed from where
      // it is. Clicking anywhere in it focuses it, which is what a terminal's click does.
      return (
        <div className="pane-frame" onPointerDown={() => onFocus(layout.pane)}>
          {/* Before the view, for `PaneFrame`'s reason: the tab order is the document's. */}
          <div className="pane-corner at-end">
            <PaneDoing pane={layout.pane} offerFor={offerFor} onPaneDoes={onPaneDoes} />
          </div>
          <div
            className={layout.pane === focused ? "pane view focused" : "pane view"}
            onFocus={() => onFocus(layout.pane)}
          >
            <ViewPane
              plane={plane}
              view={content.view}
              title={name}
              workspace={content.workspace === OUTSIDE ? undefined : content.workspace}
              waits={content.waits === true}
              offered={offered}
              onOpenView={onOpenView}
              onAsk={() => onAsk(layout.pane)}
              onVaultChanged={onVaultChanged}
              offerFor={offerFor}
              onPress={(offer) => onPaneDoes(layout.pane, offer)}
            />
          </div>
        </div>
      );
    }
    return (
      <PaneFrame
        plane={plane}
        session={content.session}
        moved={movedAt(states, content.session)}
        running={stateOf(states, content.session) === "running"}
        from={handedFrom[content.session]}
        doing={<PaneDoing pane={layout.pane} offerFor={offerFor} onPaneDoes={onPaneDoes} />}
      >
        <SessionPane
          plane={plane}
          session={content.session}
          focused={layout.pane === focused}
          onFocus={() => onFocus(layout.pane)}
        />
      </PaneFrame>
    );
  }
  return (
    <Group orientation={layout.direction === "row" ? "horizontal" : "vertical"}>
      {layout.children.map((child, side) => (
        /* **Keyed by which side of the split it is, not by what is in it.**
         *
         * It used to be keyed by the panes underneath (`split-pane-3`), so a pane that
         * became a split changed its own key — React took the `Panel` out of a live `Group`
         * and put a new one back, and `react-resizable-panels` threw *"Panel constraints not
         * found for index 2"* from a document listener where no `try` can reach it. That is
         * the same hazard `docs/ui-primitives.md` records for the regions, and the same fix:
         * nothing is added to or removed from a live group.
         *
         * It was reachable before the panes got their own controls — click a pane that is
         * not the newest, then split from the bar or the palette — but it took three
         * deliberate steps and nobody had. A `+` on every pane makes it one press, which is
         * how it was found.
         *
         * A split has exactly two children and they never swap, so the side IS the identity.
         */
        <Fragment key={side}>
          {side === 1 && <Separator />}
          <Panel>
            <LayoutPanes
              plane={plane}
              layout={child}
              focused={focused}
              onFocus={onFocus}
              offerFor={offerFor}
              onPaneDoes={onPaneDoes}
              states={states}
              name={name}
              handedFrom={handedFrom}
              offered={offered}
              onOpenView={onOpenView}
              onAsk={onAsk}
              onVaultChanged={onVaultChanged}
            />
          </Panel>
        </Fragment>
      ))}
    </Group>
  );
}

/**
 * The controls in a pane's top right corner: split it two ways, and end its chat.
 *
 * **The operator's, in his own words**: *"harnesses panes should each have close button and
 * spliting buttons in pane right top corner — and its visible when hovering harness only …
 * buttons should not have texts — only tooltips on hovering — so this will fully replace
 * separate buttons Split right, Split left, Exit this pane's chat buttons, and this will be
 * clear for spliting — user will know what pane is spliting."*
 *
 * **Three things he did not say, which the rest of this repo does:**
 *
 * - **A tooltip is not an accessible name.** `title` is what a pointer gets and a screen
 *   reader may or may not read it; `aria-label` is what a keyboard and `pressOnly()` find.
 *   Both carry the catalogue's own words, so there is still one place they are written down.
 * - **Hover-only is invisible without a pointer**, so these are drawn for the FOCUSED pane as
 *   well as the hovered one. `App.css` has the rule and the reason it is `visibility` rather
 *   than `opacity`: an invisible button that can still be clicked is `End this pane's chat`
 *   under a stray press.
 * - **The close is the danger colour**, as the tab's `×` and the bar's button were, because
 *   it does the same thing. It asks first now (`EndingChat`), which is new and is not a
 *   licence for it to look lighter.
 *
 * Every button is a row of the catalogue, and the row is the same one the palette lists.
 * Nothing is drawn for a row the catalogue no longer has.
 */
function PaneDoing({
  pane,
  offerFor,
  onPaneDoes,
}: {
  pane: number;
  offerFor: (id: string) => Offer | undefined;
  onPaneDoes: (pane: number, offer: Offer | undefined) => void;
}) {
  const rows = ["pane.split.right", "pane.split.down", "pane.close"];
  return (
    <div className="pane-doing">
      {rows.map((id) => {
        const offer = offerFor(id);
        if (!offer) return null;
        const Mark = MARKS[offer.id];
        return (
          <button
            key={id}
            className={
              offer.id === "pane.close" && !closesOnly(offer.does) ? "ends-a-chat" : undefined
            }
            // WebKit leaves a `<button>` out of the tab sequence unless this is written down
            // (`docs/ui-primitives.md`). Only the focused pane's are drawn (`App.css`), so only
            // they are stops: a hidden control is not one.
            tabIndex={0}
            disabled={!offer.available}
            aria-label={offer.title}
            title={offer.reason || (offer.note ? `${offer.title} — ${offer.note}` : offer.title)}
            onClick={() => onPaneDoes(pane, offer)}
          >
            {Mark && <Mark />}
          </button>
        );
      })}
    </div>
  );
}
