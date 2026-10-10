import {
  Fragment,
  memo,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ComponentProps,
  type CSSProperties,
  type ReactNode,
} from "react";
import clsx from "clsx";
import { listen } from "./here";
import { QueueRead } from "./QueueRead";
import { withQueue } from "./queueRows";
import { MAIN, thisWindow } from "./windows";
import { Group, Panel, Separator } from "react-resizable-panels";
import * as Menu from "@radix-ui/react-dropdown-menu";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { closestCenter, DndContext } from "@dnd-kit/core";
import { horizontalListSortingStrategy, SortableContext } from "@dnd-kit/sortable";
import {
  ArrowLeftRight,
  ChevronDown,
  FolderOpen,
  FolderPlus,
  FolderRoot,
  MessageSquarePlus,
  Minus,
  PanelRight,
  Pin as PinMark,
  Plus,
  Settings as SettingsMark,
  SquareSplitHorizontal,
  SquareSplitVertical,
  SquareArrowOutUpRight,
  SquareTerminal,
  X,
} from "lucide-react";
import {
  commands,
  type AtRisk,
  type AwayRefusal,
  type ByHand,
  type ChatBlocked,
  type PlaneSaving,
  type ChatWorktree,
  type HarnessGlance,
  type NeedsApproval,
  type NotStarted,
  type FinishedTask,
  type OpenChat,
  type PlaneId,
  type TheirAgentsMd,
  type Refused,
  type Sidebar as SidebarModel,
  type StartOptions,
  type MemoryView,
  type ViewTab,
} from "./bindings";
import {
  catalogue,
  catalogued,
  dismissId,
  ignoreId,
  needsYouRows,
  besideId,
  menuOn,
  ownTabId,
  paneCloseOf,
  OUTSIDE,
  OUTSIDE_TITLE,
  perform,
  ROOT_TIP,
  showId,
  stoppedRows,
  taskCloseId,
  taskEndIds,
  stopAllId,
  taskStopId,
  workItemSaid,
  PASS_THROUGH_BYTES,
  PASS_THROUGH_KEY,
  RENAMES_ON_F2,
  CHAT_KEYBOARD,
  type BranchPath,
  type Clone,
  type Cut,
  type Doing,
  type Catalogued,
  type Offer,
  type Project,
  type Ran,
  type TaskEndWay,
} from "./actions";
import { yourEditor } from "./yourEditor";
import { OPEN_SAVING, usePlaneSaving, useRepoSavingKept, WAY_OUT, type WayOut } from "./saving";
import { HARNESS_SETUP, type HarnessSetupAsk } from "./harnessSetup";
import { curationSubjects, useCurations } from "./curations";
import { LiveDialog, LiveMark } from "./LiveDialog";
import { RemoveFromWorkspace } from "./RemoveFromWorkspace";
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
import { FORGE_LOGIN, type LoginAsk } from "./RepoPicker";
import { NewBranch } from "./NewBranch";
import { LinkWorkItem } from "./LinkWorkItem";
import { RenameWorkspace } from "./RenameWorkspace";
import { cloneRepos, settleRepoClones, useRepoClones } from "./repoClones";
import { StartChat } from "./StartChat";
import { SessionPane } from "./SessionPane";
import {
  droppedReference,
  handReference,
  overChat,
  ReferenceChats,
  useReferenceChats,
  type ChatsForReferences,
} from "./references";
import { Explorer, type Spot } from "./Explorer";
import {
  pieceFileTitle,
  pieceFileView,
  pieceFilesTitle,
  pieceFilesView,
  type Place,
} from "./pieceViews";
import { isSearch, searchFromFocus, searchTitle, searchView } from "./contentSearch";
import { opensSearch } from "./searchKey";
import { BottomBar } from "./BottomBar";
import { useWorkspaceState, type WorkspaceState } from "./workspaceState";
import {
  heardFrom,
  lostOnResume,
  sessionTitle,
  sessionView,
  usePlaneRootPanels,
  type Resuming,
} from "./sessions";
import { useExtensionFacts } from "./extensionFacts";
import {
  CURATIONS,
  INSTRUCTIONS,
  panelsOf,
  ROOT_PANELS,
  SETTINGS,
  SIDEBAR,
  usePlaneChanged,
} from "./planeChanged";
import { FreshMark, freshMarkShown, usePlaneUpdated } from "./PlaneUpdated";
import { useHarnessCards } from "./harnessCards";
import { Notice, NoticeBand, NoticeOf } from "./Notice";
import { SandboxBlockNotice } from "./SandboxBlockNotice";
import { VaultRefusedNotice } from "./VaultRefusedNotice";
import { PersonaGrantsNotice } from "./PersonaGrantsNotice";
import { PersonaMarks, ReloadPersonaMarks, usePersonaMarks } from "./PersonaMark";
import { DispatchGrantNotice } from "./DispatchGrantNotice";
import { TasksSharingNotice } from "./TasksSharingNotice";
import { taskChangesTitle, taskChangesView } from "./taskChanges";
import { useSandboxBlocks, type Blocks } from "./sandboxBlocks";
import { taskBlockGroups, whoseOf, withoutGrouped, type TaskBlockGroup } from "./taskAsks";
import { TaskBlocksAnswered, TaskBlocksNotice } from "./TaskBlocksNotice";
import { TaskPromptNotice } from "./TaskPromptNotice";
import { usePermissionAsks } from "./permissionAsks";
import { useDismissals } from "./dismissals";
import { inSlots, SIDES, useArrangement } from "./regions";
import { RegionFrame } from "./RegionFrame";
import { DoctorNotices, useDoctor } from "./Doctor";
import { ChatGauge, useChatUsage } from "./ChatGauge";
import { usePin } from "./Updates";
import { StatusLine, runningIn, type Alerts } from "./StatusLine";
import {
  byLastActivity,
  closeChat,
  closeFocusedPane,
  closeTab,
  focusPane,
  noTabs,
  chatNameOf,
  chatInFrontOf,
  chatOf,
  contentsOf,
  findView,
  openTab,
  openTabBehind,
  openView,
  offerView,
  harnessSetupTitle,
  harnessCardView,
  harnessSetupView,
  repoInstructionsTitle,
  repoInstructionsView,
  firstTaskTitle,
  firstTaskView,
  panesOf,
  putViewBack,
  replaceSession,
  setSplit,
  showInstead,
  splitOf,
  refileViews,
  followRename,
  renamedViewKey,
  SAVING_TITLE,
  SAVING_VIEW,
  SETTINGS_TAB_TITLE,
  settingsView,
  viewNamedNow,
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
  sendToBackground,
  focusedChat,
  homeOf,
  layoutShown,
  moveToOwnTab,
  openBeside,
  placeBeside,
  restoreShown,
  sendBack,
  tasksPlacedBelow,
  showOwn,
  showOwnIn,
  panesSaid,
  shownIn,
  type PaneSaid,
  shownLive,
  switchTabTo,
  taskShownIn,
  type AskedBy,
  type Backgrounded,
  type Pinned,
  type Tabs,
  type ViewRef,
} from "./tabs";
import {
  askedByOf,
  chatsOfPane,
  chatsOfTabs,
  crumbsOf,
  hiddenNeeding,
  placedCrumbsOf,
  type Crumbs,
} from "./tabChats";
import { chatsListPrefs, useChatsListPrefs } from "./chatsListPrefs";
import { placeRowId, TabTasks, type Place as TaskPlace } from "./TabChip";
import { runningByTab } from "./taskLimits";
import { hasTasks, type Ended, type Needing } from "./tabTasks";
import { readUsed, type UsedAsk } from "./tasksUsed";
import { finishedBucketOf, taskBucketOf } from "./taskBuckets";
import { stateClock } from "./stateClock";
import { StripTablist, useTabIds } from "./StripTablist";
import { TASK_KEY_ROW, taskKeyOf } from "./taskKeys";
import { usePretendTasks } from "./e2eTasks";
import { BriefButton, BriefOpener, BriefPanel, type BriefAsk, type OpenBrief } from "./Brief";
import { AnswerOpener, AnswerPanel, type AnswerAsk, type OpenAnswer } from "./AnswerPanel";
import { activityTitle, activityView } from "./activity";
import { PaneCrumbs } from "./PaneCrumbs";
import { ChipExplained, type ChipToExplain } from "./ChipExplained";
import { AwaySummary, useAwaySummary } from "./AwaySummary";
import {
  asksInAModal,
  inPlace,
  TaskEndAsk,
  TaskEndContext,
  TaskEnds,
  type TaskEndAsked,
  type TaskEndFrom,
  type TaskEndHand,
  type TaskEndInline,
} from "./TaskEnd";
import { giveKeyboardTo, inADialog } from "./paneKeyboard";
import { endedState, TaskAway, type Away } from "./TaskAway";
import { StateShown } from "./StateShown";
import { useHeldAmong } from "./dispatchesHeld";
import { useRefusedAmong } from "./vaultRefusals";
import { chipSays, WRAPPING_UP, WrappingUp, type Asking } from "./NeedsYou";
import { EndingChat, type SmartAsk } from "./EndingChat";
import { ChatAsk, focusAfterNoticeGone } from "./ChatAsk";
import { ApprovalSentence, ProfileMeta } from "./ProfileApproval";
import { DID_NOT_START, saidWhenItEnds, stoppedWhy, useSmartClosing } from "./smartClose";
import { Panels } from "./Panels";
import { NewVault } from "./NewVault";
import { OpenVault, useVaults } from "./Vaults";
import { usePlaneEdits } from "./PlaneEdits";
import { useMemoryEdits } from "./MemoryEdits";
import { DRAFT, isMemory, memoryRefOf } from "./memories";
import { ViewPane } from "./Views";
import type { FirstTaskDoes } from "./FirstTaskTab";
import { useTabStop } from "./roving";
import { closeOnDelete, onAMac, renameOnF2 } from "./tabKeys";
import { opensAShell } from "./shellKey";
import { TabRename } from "./TabRename";
import { EmptyState } from "./EmptyState";
import { SandboxChangedNotice, useOlderSandbox } from "./SandboxChanged";
import { useSandboxCommands } from "./sandboxAsked";
import { useAskOffer } from "./askOffer";
import {
  AskPersona,
  AskPersonaOpener,
  type AskFrom,
  type AskPrefill,
  type OpenAskPersona,
} from "./AskPersona";
import { SandboxOffer } from "./SandboxOffer";
import { ProjectHostsNotice } from "./ProjectHostsNotice";
import { ProjectPresetsNotice } from "./ProjectPresetsNotice";
import { PersonaHostsNotice } from "./PersonaHostsNotice";
import { ProjectDispatchNotice } from "./ProjectDispatchNotice";
import type {
  ExtensionCommand,
  ExtensionView,
  PanelView,
  RowAction,
  SavedRecord,
  Shown,
  SmartClosing,
  WithoutSandbox,
} from "./bindings";
import { AskFirst, runExtensionAction } from "./ExtensionAction";
import { extensionsChanged, useExtensionsOn } from "./extensionsOn";
import { projectThemeChanged, useProjectThemeKept } from "./projectTheme";
import { inForce, onDrawn, TINTED_TABS, tintVariables } from "./theme/theme";
import { hueOf } from "./theme/tint";
import { handedFromNote, type HandedFrom } from "./handedFrom";
import { ChatsSection } from "./ChatsSection";
import { chatsListDrawn, type Reveal } from "./revealTask";
import { tasksBelowOf, type TasksBelow } from "./sessionTasks";
import { TasksBelowLent } from "./TasksBelow";
import { finishedOf, qualifierOf, shownOf, useFinishedTasks, useRowsUntilRead } from "./finished";
import { WaitingTaskWaysContext, type WaitingTaskWays } from "./waitingTasks";
import {
  below as chatsBelow,
  chatsTree,
  listedChat,
  type ChatRow,
  type ListedChat,
} from "./chatsTree";
import {
  stopAllAnswer,
  stopAllSays,
  stopAllTitle,
  stopAnswer,
  stopSays,
  stopTitle,
  useStopping,
  type StopAllAsked,
  type StopAsked,
} from "./stopping";
import { HarnessChip } from "./HarnessCard";
import { DoingsHere, useDoings, type DoingsOf } from "./chatDoing";
import {
  ChatsHere,
  isShell,
  movedAt,
  quietOnes,
  sameList,
  stateOf,
  useChats,
  useChatsHere,
  useChatsSelect,
  type ChatStates,
  type FailedBelow,
  type State,
} from "./chatState";
import { showsStopping, TabMarks } from "./ChatRows";
import { fitting, LEAST, LEAST_CHIP, LEAST_ROOT, leastAt, useRoom } from "./fits";
import { useArrived } from "./lib/arrived";
import { oneChatMidTurn, type Ending } from "./QuitWarning";
import { useTextSizes } from "./textSize";
import { focusStands } from "./Cockpit";
import {
  askSettingsLink,
  landing,
  linkToGroup,
  SETTINGS_ACTION,
  SETTINGS_LINK,
  type SettingsActionAsk,
  type SettingsLink,
  type SettingsLinkAsk,
} from "./settings/links";
import { enterSettings, placeOfView } from "./settings/entering";

/** Any C0 or C1 control character, or DEL: what a line typed and left unrun must not hold. */
// eslint-disable-next-line no-control-regex
const HAS_CONTROL = /[\u0000-\u001f\u007f-\u009f]/;

/** One empty list, so a prop left out is the same list at every render. */
const NONE: readonly never[] = [];

/** Where the picker's chat goes: a new tab — started in `in` when a row asked for one
 *  directory for that tab alone (charter-app#174), else where the explorer's pick says — or a
 *  split of the pane in front. `prefer` is the harness (a profile's `kind`) the picker starts
 *  on, where something already knows which one is wanted: a harness started by hand in a shell
 *  tab, opened as a chat instead (ADR 0062). */
type Where = { tab: true; in?: string; prefer?: string } | { split: Direction };

/** What a new shell tab has typed into it: a line spelled out (FR-4's `gh auth login`), a
 *  harness whose compiled-in installer the core types itself (FR-29, V65), or the sandbox's
 *  install command, which the core types and leaves for the person to run (SD-30, V78 c).
 *  `held` is a line typed and left for the person to run: the repo picker's forge login,
 *  whose host is the project's, read before Return (NO-8). */
type Typed =
  | { line: string; held?: boolean; installer?: undefined; sandboxInstall?: undefined }
  | { installer: string; line?: undefined; sandboxInstall?: undefined }
  | { sandboxInstall: true; line?: undefined; installer?: undefined };

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
 * that simply is not rendered, and its `useChats` goes on listening, so the project tab
 * can say a chat over there needs you. That is the operator's own reason for wanting one
 * window per project in the first place — fifty chats in project A must not be torn down
 * because he glanced at project B.
 *
 * It draws nothing rather than being hidden with CSS: a hidden
 * `[data-strip="Tabs"]` is a second tab strip for every query in this app and
 * in the scenario tests to trip over, and a hidden pane is a terminal being fitted to a box
 * with no size.
 *
 * **And not kept drawn under React's `<Activity mode="hidden">` either** (FR-27, #620). It was
 * prototyped around this view's chrome only, on CI: the switch it was meant to speed up moved
 * by less than CI's own spread from run to run, and what it costs in memory was not settled by
 * the one reading taken. What a switch costs is drawing this view and starting the pane's
 * terminal; what is kept small instead is the drawing again after that first draw
 * (`fits.useRoom`, `saving.ts`, `projectTheme.ts`).
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
 *
 * **Held (`memo`), so the window redrawing is not this view redrawing** (SC-3). Every chat move
 * reaches the window as this project's report — the quit warning's states, the project strip's
 * order — and the window redraws for it; a project view drawn again for that, with the same
 * props, would be every tab and pane redrawn for one chat's move.
 */
export const PlaneView = memo(function PlaneView({
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
  settingsTabAsked,
  settingsLinkAsked,
  yourSettingsAsked,
  firstChatAsked,
  shellAsked,
  fileAsked,
  awayRefused,
  onShowNeedsYou,
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
  /** A count that goes up each time the window is asked for THIS project's Settings tab at the
   *  Project level (`WindowDoing.openSettings`, SE-19); `undefined` until it is. */
  settingsAsked?: number;
  /** The same, for THIS project's Saving tab (`WindowDoing.openSaving`, charter-app#294). */
  savingAsked?: number;
  /** The same, for the Settings tab (`WindowDoing.openSettingsTab`, SE-16): a count that goes
   *  up each time the window asks for it on THIS project's strip. */
  settingsTabAsked?: number;
  /** A link into a Settings group the window followed (SE-22), once per `at`: Settings opens
   *  at the link's level and group on THIS project's strip. */
  settingsLinkAsked?: { link: SettingsLink; at: number };
  /** The same, for the Settings tab at the You level (`WindowDoing.openYourSettings`, SE-23). */
  yourSettingsAsked?: number;
  /** A file ⌘P found in THIS project (FM-7), opened in its file tab once per `at`; with a
   *  `line`, a jump to it (a search hit, FM-8), opened in the branch's file tab at that line. */
  fileAsked?: { place: Place; path: string; line?: number; at: number };
  /** This project's dispatches refused while nobody was at their chat (#1507), which the
   *  window holds for the title bar's needs-you list: a part of the away summary (#1551). */
  awayRefused?: readonly AwayRefusal[];
  /** Opens the title bar's needs-you list, where those are answered. */
  onShowNeedsYou?: () => void;
  /** The first chat a repository opened into this project asks for (FR-4): started in that
   *  repository's clone, on the workspace named after it. `at` counts the asks, so each is
   *  answered once. */
  firstChatAsked?: FirstChat;
  /** A shell tab at the project root with a command typed into it — `gh auth login` or `glab
   *  auth login` from the first run (FR-4) — asked for once per `at`. */
  shellAsked?: { typed: string; at: number };
}) {
  const [tabs, setTabs] = useState<Tabs>(noTabs);
  /**
   * **The breadcrumb each pane last drew for the task it shows**, by tab, pane and chat
   * (#1486). A pane never shows a task without its breadcrumb, and a task that has ended has
   * no path left to read off the list: this is what the pane goes on saying of it, as ended,
   * until the person leaves it. Forgotten with what it is about.
   */
  const [remembered, setRemembered] = useState<ReadonlyMap<string, Crumbs>>(() => new Map());
  /** What every chat is doing, in THIS project. Pushed from the core; nothing here polls.
   *  It keeps listening while the project is behind another one, which is what lets its tab
   *  say that something over there needs you. */
  const chats = useChats(plane);
  const doings = useDoings(plane);
  // **Only the shares this view draws from, each redrawing it only when it changes** (SC-3).
  // A chat's own state is not one of them: its tab, its explorer row and its pane read that
  // themselves, so a chat that only went from running to waiting redraws those and not this.
  //
  // **Nor is the needs-you queue** (#1034). The counts, the chips' hands and the show-more
  // counts read it where they are drawn (`QueueRead`), and the catalogue's rows for it and the
  // title bar's list are put in as this view reports, off the store (`withQueue`, `queued`). A
  // chat that starts asking for you redraws those and not every pane.
  /** The tab whose task menu the keyboard asked for, and a count of the asks (#1487): its
   *  chip opens when the count changes, so being drawn is never being asked. */
  const [tasksAsked, setTasksAsked] = useState<{ tab: number; count: number }>();
  const showTabTasks = useCallback(
    (tab: number) => setTasksAsked((was) => ({ tab, count: (was?.count ?? 0) + 1 })),
    [],
  );
  const pretended = usePretendTasks();
  const reports = useChatsSelect(chats, (states) => states.reports);
  const refusals = useChatsSelect(chats, (states) => states.refusals);
  const needs = useChatsSelect(chats, (states) => states.needs);
  const stoppedBelow = useChatsSelect(chats, (states) => states.stoppedBelow);
  const failedTasks = useChatsSelect(chats, (states) => states.failedTasks);
  /** The finished row the Chats list is asked to bring into view: a task that failed, when the
   *  person goes to its needs-you item (#1491). */
  const [revealed, setRevealed] = useState<Reveal>();
  /**
   * **What the last window action here refused, and which action** (NO-4): the picker's
   * options, or a shell. A Notice with Dismiss, and it clears itself when that same action next
   * succeeds — it used to stand until some other refusal replaced it.
   */
  const [trouble, setTrouble] = useState<{
    from: "start-options" | "shell" | "needs-you";
    said: string;
  }>();
  const refusedBy = useCallback(
    (from: "start-options" | "shell" | "needs-you", said: string) => setTrouble({ from, said }),
    [],
  );
  const succeeded = useCallback(
    (from: "start-options" | "shell" | "needs-you") =>
      setTrouble((was) => (was?.from === from ? undefined : was)),
    [],
  );
  const [sidebar, setSidebar] = useState<SidebarModel>();
  /** How many chats a stop has ended: the sidebar is read again for each, since a task chat
   *  with no tab leaves the list without any tab changing (#1448). */
  const [stopsEnded, setStopsEnded] = useState(0);
  /** This project's save standing (charter-app#302): every project reads its own, so the project
   *  strip can mark the ones with unsaved work and the title bar can show the one in front. */
  const { saving } = usePlaneSaving(plane);
  /** The workspace whose LIVE/LOCAL confirmation is open (charter-app#301). */
  const [liveAsk, setLiveAsk] = useState<string>();
  /** The repo the "Remove from workspace" question is about, while it is open (#1228). */
  const [membershipAsk, setMembershipAsk] = useState<{ workspace: string; repo: string }>();
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
  /** Whether the picker's fix (NO-8: its `local-ignore`) is running. */
  const [pickerFixing, setPickerFixing] = useState(false);
  /** The chat tab whose name is open for editing on the strip, if one is (charter-app#254). */
  const [renaming, setRenaming] = useState<number>();
  /** The work item each chat works on, by session, as the core answered (V60, ADR 0088). */
  const [workItems, setWorkItems] = useState<Record<number, string>>({});
  /** The chat tab the Link to work item dialog is asking about, while it is open. */
  const [linkingWork, setLinkingWork] = useState<{
    tab: number;
    trouble?: string;
    busy: boolean;
  }>();
  /** The chat the Ask {persona} dialog is asking from, and the persona, while it is open. */
  const [askingPersona, setAskingPersona] = useState<{
    session: number;
    persona: string;
    prefill?: AskPrefill;
    from?: AskFrom;
    trouble?: string;
    busy: boolean;
  }>();
  /**
   * **The Notices dismissed here until their cause changes** (`dismissals.ts`, V91j): kept on
   * this machine across relaunches, and let go once the core answers without the cause.
   */
  const { dismissed, dismiss, settle: settleNotices, showAgain } = useDismissals(plane);
  /** Chats this launch could not start, by name and why. They are still recorded. */
  const [wouldNotStart, setWouldNotStart] = useState<NotStarted[]>([]);
  /** **Forget this chat…** (NO-3): the chat whose record is about to be dropped, while the
   *  question is up, and the core's refusal of the last answer. */
  const [forgetting, setForgetting] = useState<{
    id: string;
    name: string;
    busy: boolean;
    trouble?: string;
  }>();
  /**
   * **Review and approve…** (#1246, D-1246-5): the waiting chat whose profile's command is being
   * approved, with the approval as the core said it when the question opened. It is never
   * updated while the question is up: the line approved is the line on screen, and the core
   * checks it against the file (`approve_profile`).
   */
  const [approving, setApproving] = useState<{
    id: string;
    name: string;
    approval: NeedsApproval;
    busy: boolean;
    trouble?: string;
  }>();
  /** The stop the person is being asked about, before anything is stopped (#1448). */
  const [stopAsk, setStopAsk] = useState<StopAsking>();
  /** The Stop all tasks the person is being asked about, before anything is stopped (#1498). */
  const [stopAllAsk, setStopAllAsk] = useState<StopAllAsking>();
  /** The task the person is being asked about ending, before anything is ended (#1488). */
  const [taskEndAsk, setTaskEndAsk] = useState<TaskEndAsked>();
  /** The second step of ending an idle task, asked where the press was made (#1488). */
  const [taskEndInline, setTaskEndInline] = useState<TaskEndInline>();
  /**
   * **Start fresh** (NO-3, charter#369): the tab whose chat is about to be started again on the
   * project's instructions as they are now, while the question is up. The tab mark and the
   * palette's row both come here (`tab.fresh:<id>`).
   */
  const [freshening, setFreshening] = useState<{
    tab: number;
    session: number;
    name: string;
    files: readonly string[];
    /** What the chat was doing when it was asked about, as the board said (#1246). */
    doing: State;
    busy: boolean;
    trouble?: string;
  }>();
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
  /** Bumped when the core says the instructions a chat reads at its start changed on disk
   *  (charter-app#264, FD-10d): `CLAUDE.md`, the harness settings, a persona's charter —
   *  never a todo, a memory or a session record. Every reader of the plane hears only what
   *  the core says concerns it (FD-10). */
  const instructionsChanges = usePlaneChanged([plane], INSTRUCTIONS);
  /** The same for the curation actions (FD-10d): the personas' curation files, the project's
   *  launch profile, the workspaces a subject can be. */
  const curationsChanges = usePlaneChanged([plane], CURATIONS);
  /** The changes what this project has on and its theme are made of (FD-10). */
  const settingsChanges = usePlaneChanged([plane], SETTINGS);
  /** Each harness's card, for the palette's *What <product> can do here* rows (#1134). */
  const harnessCards = useHarnessCards(plane, inFront, settingsChanges);
  /** The same, counting only the changes the sidebar is made of (FD-10): a memory an agent
   *  saves does not make it list every workspace's todos again. */
  const sidebarChanges = usePlaneChanged([plane], SIDEBAR);
  /** Every persona's icon and colour (#1449), read with the sidebar's changes: a persona's
   *  definition or its folder moving is one of them. */
  const personaMarks = usePersonaMarks(plane, sidebarChanges);
  /** The chats running on instructions the plane has changed since they started (charter#369),
   *  each marked on its tab. */
  const planeUpdates = usePlaneUpdated(plane, instructionsChanges);
  // The plane root is watched, so an edit to `charter.toml` or `charter.local.toml` — in an
  // editor, from a `git pull` — is one of these, and what this project has on may have moved
  // with it (charter-app#253). Not at the mount: `useExtensionsOn` asks then.
  useEffect(() => {
    if (settingsChanges === 0) return;
    extensionsChanged(plane);
    // And the theme it draws, which the same two files and each `workspace.json` pick
    // (charter-app#273, #281).
    projectThemeChanged(plane);
  }, [settingsChanges, plane]);
  /** Whether the new-workspace dialog is up, why the last attempt made nothing, and whether
   *  charter is making one right now. */
  const [makingWorkspace, setMakingWorkspace] = useState(false);
  const [workspaceTrouble, setWorkspaceTrouble] = useState<string>();
  const [busyMaking, setBusyMaking] = useState(false);
  /** The repo the New branch dialog is cutting in, and the workspace it is that repo of, while
   *  it is up — then `makingWorkspace`'s other two, for a branch (GL-1). */
  const [branching, setBranching] = useState<{ workspace: string; repo: string }>();
  const [branchTrouble, setBranchTrouble] = useState<string>();
  const [busyBranching, setBusyBranching] = useState(false);
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
  /**
   * The chats that are shell tabs (SI-5): the operator's own shell, no harness, no profile. Its
   * tab wears a terminal's mark rather than nothing, so a shell is told from a harness before
   * its name is read. Filled when one is opened here and when one comes back from the record.
   */
  const [shells, setShells] = useState<ReadonlySet<number>>(() => new Set());
  /**
   * A harness started by hand in a shell tab, by that tab's session, as its banner says it
   * (ADR 0062). The core says so over `harness-by-hand`; the banner stays until it is dismissed
   * or answered.
   */
  const [byHand, setByHand] = useState<Record<number, ByHandNote>>({});
  /** What each chat's start found to say, by session, until it is dismissed (ADR 0085). */
  const [startNotes, setStartNotes] = useState<Record<number, StartNotes>>({});
  /** What each chat's sandbox blocked, by session, for the Notice on its tab (#1338). */
  const {
    blocks: sandboxBlocks,
    dismiss: dismissBlock,
    answered: blockAnswered,
  } = useSandboxBlocks(plane);
  /** What one answer to several tasks' same block allowed, by the session they are tasks of,
   *  until it is put away (#1508). */
  const [taskBlocksAnswered, setTaskBlocksAnswered] = useState<
    readonly { at: number; session: number; target: string; said: string }[]
  >([]);
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
  /**
   * **Dormant pins** (V91c as amended, NO-1): pins whose workspace is gone. They stay in the
   * machine store, in their place in the order, and are never drawn; when the workspace comes
   * back (a pull, another branch, a relaunch) the store's answer draws them again, in place.
   */
  const [dormantPins, setDormantPins] = useState<string[]>([]);
  /** Every pin, dormant ones included, in the store's order (`plane_pins`' `order`). */
  const [pinOrder, setPinOrder] = useState<string[]>([]);
  /**
   * **Dormant pins the operator forgot this run**, each with the pins that came before it in the
   * order, nearest first: its Notice's Undo puts it back after the nearest of them still pinned,
   * so several Undos in any order, or an unpin made in between, give back the order they had.
   */
  const [forgottenPins, setForgottenPins] = useState<{ name: string; after: string[] }[]>([]);
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
   * The branch the explorer is focused on — its cockpit (FM-5, #1108) — with the workspace it
   * is in. Kept when the window moves to another workspace, and drawn again on coming back,
   * as the pick is; remembered with the window's views (`window_focus`), so one window can
   * stay on one branch across a relaunch.
   */
  const [focusedBranch, setFocusedBranch] = useState<Place>();
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
  /** The chats a scenario spec pretends (`e2eTasks.ts`, none in a shipped build): the core has
   *  none of them, and is never told one is in front. */
  const pretendedNow = useRef<ReadonlySet<number>>(new Set());
  useEffect(() => {
    pretendedNow.current = new Set(pretended.map((one) => one.chat.session));
  }, [pretended]);
  const change = useCallback(
    (how: (tabs: Tabs) => Tabs): Tabs => {
      const next = how(now.current);
      const frontOf = (tabs: Tabs) => {
        const chat = tabs.inFront === undefined ? undefined : chatInFrontOf(tabs, tabs.inFront);
        return chat === undefined || pretendedNow.current.has(chat) ? undefined : chat;
      };
      const was = frontOf(now.current);
      now.current = next;
      setTabs(next);
      // The core records which chat was in front, so it is told whenever that changes — and
      // only then, rather than on every split and every keystroke. The plane travels with it:
      // "chat 3 is in front" belongs to a plane, and every plane numbers its chats from one.
      // **The tab's own chat**, whatever the tab shows: which of its chats a tab shows is told
      // apart, per tab (`tab_shows`, below).
      //
      // **Compared by the chat, not by the tab** (#1489): the tab in front can come to be
      // another chat's without changing (its first pane went and the pane beside it is the
      // tab now; its chat was started again under a new number). The core holds a reported
      // task and withholds a notification by this, so a tab that kept its id and changed its
      // chat must be said too.
      //
      // A tab of views only has no chat, so nothing is in front as far as the record of chats
      // is concerned; the view tab says it is in front itself (`windowViews`). A chat beside a
      // view in a tab that opened on it is in front (`chatInFrontOf`, #1525), and is said
      // again when it is started there or closed, though the tab in front is the same.
      const chat = frontOf(next);
      if (chat !== was) void commands.chatInFront(plane, chat ?? null).catch(() => undefined);
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
  /** What the core holds of each chat's pane (`tab_shows`), by the pane's own chat: what it
   *  said at the launch, then what this window has told it since (#1486, #1489). The one
   *  mechanism: whatever was not put back is said to it as gone. */
  const lastShownSaid = useRef<ReadonlyMap<number, PaneSaid>>(new Map());
  /** Settles the notes a relaunch says about its chats (`resumed`, `guessed`, `fresh`) against
   *  the chats the core put back. */
  const settleChatNotes = useCallback(
    (open: readonly OpenChat[]) => {
      for (const family of CHAT_NOTES) {
        settleNotices(
          family,
          open.flatMap((one) => chatNote(family, one) ?? []),
        );
      }
    },
    [settleNotices],
  );
  useEffect(() => {
    if (adopted.current) return;
    adopted.current = true;
    void Promise.all([
      commands.openedChats(plane).catch(() => undefined),
      // The view tabs the record put back, beside the chats. **Asked for, never started**: a
      // view has no program of charter's, and an extension's view is not asked anything until
      // the operator presses for it (`Views.tsx`), so this is a list of tabs and nothing else.
      commands.reopenedViews(plane).catch(() => undefined),
      // The branch the window's explorer was focused on (FM-5), kept with the views.
      commands.reopenedFocus(plane).catch(() => undefined),
    ])
      .then(([answer, viewAnswer, focusAnswer]) => {
        setSettled(true);
        void commands
          // A window that cannot ask, or is answered with nothing, simply says nothing.
          .chatsThatWouldNotStart(plane)
          .then((trouble) => setWouldNotStart(trouble.status === "ok" ? (trouble.data ?? []) : []))
          .catch(() => undefined);
        const open = answer?.status === "ok" ? (answer.data ?? []) : [];
        // What the relaunch did to each chat is the core's answer: a dismissed note about a chat
        // that came back some other way, or not at all, is let go (`dismissals.ts`).
        if (answer?.status === "ok") settleChatNotes(open);
        // Each as it is named now: a record an older charter wrote can hold a view since renamed.
        const back = (viewAnswer?.status === "ok" ? (viewAnswer.data ?? []) : []).map((tab) => {
          const now = viewNamedNow(refOf(tab), tab.title);
          return { ...tab, ...now.view, title: now.title };
        });
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
          // A shell comes back a shell: on no profile, running no harness.
          setShells(new Set(open.filter(isShell).map((chat) => chat.session)));
        }
        setPinnedViews(back.filter((view) => view.pinned).map((view) => viewKey(refOf(view))));
        const focusedBack = focusAnswer?.status === "ok" ? focusAnswer.data : null;
        if (focusedBack !== null && focusedBack !== undefined)
          setFocusedBranch({
            workspace: focusedBack.workspace,
            repo: focusedBack.repo,
            piece: focusedBack.piece,
          });
        // The persona comes with the chat, so a tab put back reads `steward 3` from its first
        // frame rather than reading `3` until the sidebar has been read (charter-app#130) — and
        // so does the name the operator gave it, which rides the record (charter-app#254).
        // **A task chat nobody has opened is put back with no tab** (#1447): it is listed in
        // the Chats section, as it was, and its row opens one.
        const chats = open
          .filter((chat) => chat.from?.tab !== false)
          .reduce(
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
              putViewBack(
                tabs,
                refOf(view),
                view.title,
                view.workspace ?? OUTSIDE,
                view.at,
                view.split ?? undefined,
              ),
            chats,
          );
        const front = open.find((chat) => chat.in_front);
        const frontView = back.find((view) => view.active);
        const tasks = new Map(
          open.map((chat) => [chat.session, chat.from?.task ? chat.from.chat : undefined]),
        );
        const askedBy: AskedBy = (session) => tasks.get(session);
        // **A task that was open beside its session is put back beside it** (#1489), where
        // that session came back with a tab; otherwise it is in the Chats list, as a task
        // nobody opened is. A task in a tab of its own came back as that tab, above.
        const besideBack = open.reduce(
          (tabs, chat) =>
            chat.from?.task && (chat.beside ?? undefined) !== undefined
              ? placeBeside(tabs, chat.session, askedBy, chat.name)
              : tabs,
          drawn,
        );
        // **What each session's tab showed** (#1486): the record keeps it on the session's
        // own entry, and it is put back only where the chat it names is open and is a task
        // below that session. Anything else shows the session's own chat.
        const shownBack = open.reduce((tabs, chat) => {
          const shown = chat.shows ?? undefined;
          if (shown === undefined || homeOf(tabs, shown, askedBy)?.own !== chat.session)
            return tabs;
          return restoreShown(tabs, [shown], askedBy);
        }, besideBack);
        // What the core's record says of each pane, so whatever was not put back (a task that
        // is gone, a session that did not come back) is said to it as gone, below.
        lastShownSaid.current = new Map(
          open.flatMap((chat) => {
            const shown = chat.shows ?? null;
            const beside = chat.beside ?? null;
            return shown === null && beside === null
              ? []
              : [[chat.session, { shown, beside }] as const];
          }),
        );
        // **A view tab in front comes first**: the chat the core holds in front can be one
        // beside that view (`chatInFrontOf`), and that chat's own tab is not the one the person
        // left in front.
        const viewInFront =
          frontView === undefined
            ? undefined
            : drawn.order.find((id) => {
                const lead = contentsOf(drawn, id)[0]?.content;
                return lead?.kind === "view" && viewKey(lead.view) === viewKey(refOf(frontView));
              });
        const inFront =
          viewInFront ??
          (front !== undefined ? homeOf(shownBack, front.session, askedBy)?.tab : undefined);
        if (shownBack.order.length > 0)
          change(() => (inFront === undefined ? shownBack : selectTab(shownBack, inFront)));
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
  }, [change, plane, settleChatNotes]);

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

  /** The branch the explorer is focused on (FM-5), told to the core when it changes, after the
   *  record is heard for the views' reason: a focus said first would write over the record's. */
  const lastFocusSaid = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (!viewsHeard) return;
    const said = focusedBranch ?? null;
    const text = JSON.stringify(said);
    if (text === lastFocusSaid.current) return;
    lastFocusSaid.current = text;
    void commands.windowFocus(plane, said).catch(() => undefined);
  }, [focusedBranch, plane, viewsHeard]);

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

  /**
   * **Where each chat's pane is and what it shows, told to the core whenever it changes**
   * (#1486, #1489): the chat a pane shows in place of its own, and the chat whose tab a pane is
   * in where it is not that tab's own. The record keeps both, so the next launch, and a
   * reloaded window, put each pane back; and they are how the core knows every chat on screen
   * in the tab in front (`Chats::looked_at`). After the record is heard, and only what
   * differs, for the order's reasons above. A pane back on its own chat, and a chat whose pane
   * is gone, say so, with nothing.
   *
   * **The one command for it**: nothing else says to the core what a pane shows. What it says
   * is `panesSaid`'s: in a tab that opened on a view, the chat it puts in front stands for the
   * tab, so every other chat beside the view is looked at with it (#1525).
   */
  useEffect(() => {
    if (!viewsHeard) return;
    const said = panesSaid(tabs);
    const was = lastShownSaid.current;
    lastShownSaid.current = said;
    /** Says it, and where the core did not take it, forgets having said it: the next change
     *  to the tabs says it again. Left alone where something newer was said meanwhile. */
    const say = (own: number, now: PaneSaid) => {
      const unsaid = () => {
        const held = lastShownSaid.current.get(own);
        if (held?.shown !== now.shown || held.beside !== now.beside) return;
        const back = new Map(lastShownSaid.current);
        const before = was.get(own);
        if (before === undefined) back.delete(own);
        else back.set(own, before);
        lastShownSaid.current = back;
      };
      void commands
        .tabShows(plane, own, now.shown, now.beside)
        .then((answer) => {
          if (answer.status !== "ok") unsaid();
        })
        .catch(unsaid);
    };
    for (const [own, now] of said) {
      const before = was.get(own);
      if (before?.shown !== now.shown || before.beside !== now.beside) say(own, now);
    }
    // A pane that is gone, or back on its own chat in its own tab: said as nothing. A chat that
    // has ended is not open to be told of: the core says so, and it is nothing, so this one
    // is not said again.
    for (const own of was.keys())
      if (!said.has(own)) void commands.tabShows(plane, own, null, null).catch(() => undefined);
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
      // A task dispatched by a chat at the project's root is in no workspace, as that chat is.
      const filed = arrived.workspace ?? OUTSIDE;
      setStartedIn((was) => ({ ...was, [arrived.session]: filed }));
      const note = handedFromNote(arrived.from);
      if (note) setHandedFrom((was) => ({ ...was, [arrived.session]: note }));
      // **A task chat arrives with no tab** (#1447): the work was not sent for the person to
      // read, so it is listed in the Chats section and its row opens a tab. Filing it above is
      // what reads the list again.
      if (arrived.from?.tab === false) return;
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

  /**
   * The persona chats that still owe a report. A task's report is followed by the end of its
   * turn, and a task that dies ends: either moves `tasksMoved`, and the sidebar is read again,
   * which is where a task's reported mark comes from. **A turn beginning moves nothing**
   * (#1468): no report comes with it. Empty while no chat has a task in flight, so nothing is
   * read again for it then.
   */
  const unreported = useMemo(
    () =>
      sidebar === undefined
        ? []
        : [...sidebar.unfiled, ...sidebar.workspaces.flatMap((ws) => ws.chats)]
            .filter(
              (chat) =>
                chat.from?.task === true &&
                chat.from.reported !== true &&
                chat.from.unreported !== true,
            )
            .map((chat) => chat.session),
    [sidebar],
  );
  const tasksMoved = useTurnsEnded(chats, unreported);

  // The sidebar is read from the plane, and re-read whenever the chats change: the plane is a
  // directory the operator also edits by hand and another charter process writes, so there is
  // nothing to invalidate a cache of it. `tabs` is the dependency because opening or ending a
  // chat is what this window can change about the answer, and `sidebarChanges` because the core
  // says when something else changed it (charter-app#264) — a change of a kind the sidebar
  // reads (FD-10).
  //
  // **Only the newest read is drawn.** The core reads the sidebar off the window's thread
  // (SC-2), so two reads can answer in either order, and one asked before a workspace was
  // renamed must not put it back after the read that saw the rename. `gone` is set when a
  // newer read is asked, which is when this effect runs again.
  useEffect(() => {
    let gone = false;
    void commands
      .planeSidebar(plane)
      .then((answer) => {
        if (gone) return;
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
        // The plane root always stands: it is the plane's own directory (SI-1).
        const stands = (name: string) =>
          name === OUTSIDE || next.workspaces.some((ws) => ws.name === name);
        setPicked((current) =>
          current !== undefined && stands(current)
            ? current
            : (ofFront ?? next.workspaces[0]?.name ?? OUTSIDE),
        );
        // A view tab is on the strip it was opened from, which it carries; when the plane stops
        // having that workspace nothing else would move it, and it would be on a strip that is
        // never drawn. It goes where a chat working in no workspace goes.
        change((tabs) =>
          refileViews(tabs, (name) => next.workspaces.some((ws) => ws.name === name), OUTSIDE),
        );
      })
      // A window with no readable plane still runs its panes; the header already says so.
      .catch(() => {
        if (!gone) setSidebar(undefined);
      });
    return () => {
      gone = true;
    };
  }, [change, sidebarChanges, plane, replan, startedIn, stopsEnded, tabs, tasksMoved]);

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
        setPinOrder(Array.isArray(said?.order) ? said.order : []);
        // Named, never drawn, and never written away (V91c as amended): a dormant pin is the
        // operator's to forget.
        setDormantPins(Array.isArray(said?.missing) ? said.missing : []);
        // The store's answer is what says a gone pin came back, never a list not read yet
        // (`dismissals.ts`): a dismissal is let go only when the core answers without it.
        // **Only a certain answer**: with a rename between its steps, `workspaces/` not there or
        // a listing it could not read whole, the store names nothing gone and says it is not
        // sure, and a pin it leaves out has not come back (D-NO2-10).
        if (said?.certain === true && Array.isArray(said.missing))
          settleNotices(
            "pin-dormant",
            said.missing.map((name) => `pin-dormant:${name}`),
          );
      })
      // A window that cannot ask draws nothing pinned: the workspace strip holds the one you
      // are in, and the rest are behind its show-more — the arrangement an operator who has
      // pinned nothing already has (ADR 0054).
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [pinning, plane, settleNotices, sidebar]);

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
  /** The persona chat `session` runs as, as the plane's chats say it, or `null`. */
  const personaOf = useCallback(
    (session: number | undefined): string | null => {
      if (session === undefined || sidebar === undefined) return null;
      for (const chats of [...sidebar.workspaces.map((ws) => ws.chats), sidebar.unfiled]) {
        const found = chats.find((chat) => chat.session === session);
        if (found) return found.persona ?? null;
      }
      return null;
    },
    [sidebar],
  );
  const filedIn = useCallback<FiledIn>(
    (session) => {
      const at = (chat: number) => {
        const workspace = sidebar?.workspaces.find((ws) =>
          ws.chats.some((one) => one.session === chat),
        );
        if (workspace) return workspace.name;
        if (sidebar?.unfiled.some((one) => one.session === chat)) return OUTSIDE;
        return startedIn[chat];
      };
      // A task a scenario spec pretends (`e2eTasks.ts`, none in a shipped build) is in no
      // list of the core's: it is filed where the chat that asked for it is.
      const asker = pretended.find((one) => one.chat.session === session)?.chat.from?.chat;
      return at(session) ?? (asker === undefined ? undefined : at(asker)) ?? OUTSIDE;
    },
    [pretended, sidebar, startedIn],
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
   * **The chats whose Smart close the operator has just pressed**, until the core's first step
   * about them arrives (SI-8f). The press is what puts the tab into the background, so it goes
   * at once rather than a round trip later; a start the core refuses brings it back.
   */
  const [leaving, setLeaving] = useState<ReadonlySet<number>>(() => new Set());
  /** Why each chat's smart close stopped without its record, for the title bar's needs-you
   *  list (SI-8f) — until the operator goes to it or dismisses it, or the chat goes. */
  const [stopped, setStopped] = useState<Readonly<Record<number, string>>>({});
  /** The record the last smart close ended on, for the quiet "Session saved" notice. */
  const [savedNotice, setSavedNotice] = useState<{ record: SavedRecord | null }>();
  /** The chats that finished a Smart close with no pass, by session: their record is saved and
   *  their tab stayed open, so each has a Notice with Close tab (#1361, D-1361-7). */
  const [keptOpen, setKeptOpen] = useState<readonly { session: number; name: string }[]>([]);
  const settle = useCallback((session: number) => {
    setLeaving((was) => {
      if (!was.has(session)) return was;
      const now = new Set(was);
      now.delete(session);
      return now;
    });
  }, []);
  /** Every chat the core lists, by session, as of the last read: what {@link showChat} opens a
   *  tab from for a chat that has none. A ref, so a read of the list does not make a new
   *  `showChat` for every row and catalogue entry holding one. Kept up to date below, where
   *  the list is read. */
  const listedNow = useRef<ReadonlyMap<number, OpenChat>>(new Map());
  /** Which chat a task is a task of, as the core lists it now: what says which tab a task is
   *  shown in. Task links only (V100-69). */
  const askedByNow = useCallback<AskedBy>((session) => {
    const from = listedNow.current.get(session)?.from;
    return from?.task ? from.chat : undefined;
  }, []);
  /** {@link isBackground} as of the last render, for handlers the set it reads is made from,
   *  which so cannot take it as a dependency. */
  const inTheBackground = useRef<Backgrounded>(() => false);
  /**
   * **The panes of chats `gone` have gone: every task below one of them that has a pane of its
   * own is sent back to the list** (#1489, V100-39). **The one function for it, on every
   * path a chat's pane goes by**: its tab's close, a close of its pane, a Smart close whose
   * record landed, a stop, the chat ending by itself, and a close that stops what is below.
   * So no task's tab outlives the session it would go back into, the close dialog's sentence
   * about those tabs is true whichever answer is given, and a task never comes to be the own
   * chat of what was its session's tab.
   *
   * `before` is the tabs as they were with those panes still there. A task that ended with
   * its asker has no pane to send back, and the core says nothing of a chat it has closed.
   * **Nothing ends here**: a task sent back goes on, in the Chats list.
   */
  const tasksGoBack = useCallback(
    (gone: readonly number[], before: Tabs) => {
      const back = [
        ...new Set(
          gone.flatMap((chat) =>
            tasksPlacedBelow(before, chat, askedByNow).map((one) => one.session),
          ),
        ),
      ].filter((task) => !gone.includes(task));
      if (back.length === 0) return;
      change((tabs) =>
        back.reduce(
          (left, task) => sendBack(left, task, filedIn, isPinned, inTheBackground.current),
          tabs,
        ),
      );
      for (const task of back) void commands.closeChatTab(plane, task).catch(() => undefined);
    },
    [askedByNow, change, filedIn, isPinned, plane],
  );

  const stoppedFor = useCallback((session: number, why: string | undefined) => {
    setStopped((was) => {
      if (why === undefined) {
        if (!(session in was)) return was;
        return Object.fromEntries(Object.entries(was).filter(([one]) => Number(one) !== session));
      }
      return { ...was, [session]: why };
    });
  }, []);

  /**
   * **A smart close that has ended** (ADR 0064). The core closed the chat when its record
   * landed, so the window takes its pane away — `closeChat`, and never `close_session`, which
   * would end it a second time — and says so quietly, with the record a click away (SI-8f).
   * Given up, ended on its own, or its queued prompt could not be sent: the tab comes back where
   * it was and the window says why, in one sentence and in the needs-you list. Cancelled: the tab coming back says it all.
   */
  const smartCloseEnded = useCallback(
    (step: SmartClosing) => {
      settle(step.session);
      if (step.phase === "closed") {
        const before = now.current;
        change((tabs) => closeChat(tabs, step.session, filedIn, isPinned, inTheBackground.current));
        tasksGoBack([step.session], before);
        stoppedFor(step.session, undefined);
        setSavedNotice({ record: step.record });
        return;
      }
      const tab = now.current.order.find((id) =>
        panesOf(now.current, id).some((one) => one.session === step.session),
      );
      const name = tab === undefined ? `chat ${step.session}` : now.current.byId[tab].name;
      if (step.phase === "kept_open") {
        setKeptOpen((was) => [
          ...was.filter((one) => one.session !== step.session),
          { session: step.session, name },
        ]);
        return;
      }
      const said = saidWhenItEnds(step.phase, name);
      if (said !== undefined)
        setReport({ from: `smartclose:${step.session}`, refused: true, words: said });
      stoppedFor(step.session, stoppedWhy(step.phase));
    },
    [change, filedIn, isPinned, settle, stoppedFor, tasksGoBack],
  );
  /** The chats wrapping up, as the core tells it. */
  const told = useSmartClosing(plane, smartCloseEnded);
  /**
   * **A chat the person stopped has ended** (#1448). The core ended it, so the window takes its
   * pane away with `closeChat`, and never `close_session`, which would end it a second time.
   */
  const chatStopped = useCallback(
    (session: number) => {
      const before = now.current;
      change((tabs) => closeChat(tabs, session, filedIn, isPinned, inTheBackground.current));
      tasksGoBack([session], before);
      stoppedFor(session, undefined);
      setStopsEnded((count) => count + 1);
    },
    [change, filedIn, isPinned, stoppedFor, tasksGoBack],
  );
  /** The chats being stopped, as the core tells it. */
  const stopping = useStopping(plane, chatStopped);
  /** Every chat the Chats section lists, as of the last render: what a stop is asked about. A
   *  ref, so a read of the list does not make a new catalogue of the rows holding the ask. */
  const chatsListed = useRef<readonly ListedChat[]>([]);
  /** Each chat's finished rows as last read, for a press that asks for one (#1491). */
  const finishedNow = useRef<ReadonlyMap<number, readonly FinishedTask[]>>(new Map());
  /**
   * **Stop asks first** (#1448): a row of the catalogue opens the question, which says what
   * ends, and nothing is stopped until it is answered. A chat with no chat below it is asked
   * about alone, whichever row was pressed.
   */
  const askToStop = useCallback(
    (session: number, below: boolean) => {
      const chat = chatsListed.current.find((one) => one.session === session);
      if (chat === undefined) return;
      const under = chatsBelow(chatsListed.current, session);
      setStopAsk({
        session,
        name: chat.name,
        below: below && under.length > 0,
        under: under.length,
        dispatched: chat.parent !== null,
        already: stopping.has(session),
        waitingOn: under.filter((one) => stopping.has(one)).length,
        busy: false,
      });
    },
    [stopping],
  );
  /** The person answered: the core stops the chat, or says why not, in the question. */
  const stopAsked = useCallback(async () => {
    const asked = stopAsk;
    if (asked === undefined) return;
    setStopAsk({ ...asked, busy: true, trouble: undefined });
    const said = await commands
      .stopChat(plane, asked.session, asked.below)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    setStopAsk(
      said.status === "error" ? { ...asked, busy: false, trouble: said.error } : undefined,
    );
  }, [plane, stopAsk]);
  /**
   * **Stop all tasks asks once, naming how many** (#1498, V100-53): the core says which tasks
   * below the session are at work, and the answer stops those and no more. A session with none
   * left is said so on the window's report line, and nothing is asked.
   */
  const askToStopAll = useCallback(
    async (session: number) => {
      const read = await commands
        .allTasksEnding(plane, session)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (read.status === "error") {
        setReport({ from: `chat.stop.tasks:${session}`, refused: true, words: read.error });
        return;
      }
      const ending = read.data;
      // No answer is no leave to stop anything: nothing is guessed about a task.
      if (ending === null || typeof ending !== "object" || !Array.isArray(ending.tasks)) return;
      if (ending.tasks.length === 0) {
        setReport({
          from: `chat.stop.tasks:${session}`,
          refused: true,
          words: `No task below ${ending.name} is at work, so there is nothing to stop.`,
        });
        return;
      }
      setStopAllAsk({ session, name: ending.name, tasks: ending.tasks, busy: false });
    },
    [plane],
  );
  /** The person answered: the core stops the tasks the question named, or says why not. */
  const stopAllAsked = useCallback(async () => {
    const asked = stopAllAsk;
    if (asked === undefined) return;
    setStopAllAsk({ ...asked, busy: true, trouble: undefined });
    const said = await commands
      .stopAllTasks(plane, asked.session, [...asked.tasks])
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    setStopAllAsk(
      said.status === "error" ? { ...asked, busy: false, trouble: said.error } : undefined,
    );
  }, [plane, stopAllAsk]);
  /**
   * **Ends a task the way the person chose**, by the core (`end_task`), which tells the chat
   * that asked which way it was. A refusal is said where the answer was given: in the
   * question while one is up, else on the window's report line.
   */
  const endTaskNow = useCallback(
    async (session: number, way: TaskEndWay, below: boolean, asked?: TaskEndAsked) => {
      if (asked !== undefined) setTaskEndAsk({ ...asked, busy: true, trouble: undefined });
      const said = await commands
        .endTask(plane, session, way, below)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status !== "error") {
        setTaskEndAsk(undefined);
        return undefined;
      }
      if (asked !== undefined) setTaskEndAsk({ ...asked, busy: false, trouble: said.error });
      // Where no question is up, whoever asked says the refusal where the answer was given.
      return said.error;
    },
    [plane],
  );
  /**
   * **Stop and get its report, or Close now** (#1488, V100-5, V100-18): every press to end a
   * task comes here, and **none ends anything by itself**. The core is asked what ending the
   * task would do, and then the person is asked:
   *
   * - a task mid-turn, or with tasks of its own at work below it, in the one modal question;
   * - every other task **in place**, where the press was made: on its row for its menu and
   *   Delete, on the breadcrumb for its two controls ("Stop it" or "Close it", and Keep, which
   *   has the keyboard). A press from anywhere else is asked on the breadcrumb where a pane
   *   shows the task, and in the modal question where none does, so it is never asked
   *   somewhere the person is not looking.
   *
   * Where purlis may not type into the task, either says why and offers Close alone. The
   * standard close dialog is never part of this.
   */
  /** Whether a pane of tab `tab` draws a breadcrumb ending in task `session`: the one place
   *  a second step is drawn in place (`TaskEnds`). Held here for what is declared before the
   *  list is read. */
  const drawsCrumbFor = useRef<(tab: number, session: number) => boolean>(() => false);
  const askToEndTask = useCallback(
    async (session: number, way: TaskEndWay, from: TaskEndFrom) => {
      const read = await commands
        .taskEnding(plane, session)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (read.status === "error") {
        setReport({ from: `task.end:${session}`, refused: true, words: read.error });
        return;
      }
      const ending = read.data;
      // No answer is no leave to end anything: nothing is guessed about a task.
      if (ending === null || typeof ending !== "object") return;
      // Being stopped already: it has its one short turn, and is not asked a second time.
      if (ending.stopping && way === "report") return;
      // On screen: a pane of the tab in front draws a breadcrumb that ends in it, which is
      // where the second step is drawn. A tab behind draws none, and neither does a task's own
      // tab with no path to say (its asker is not listed), nor a task's pane switched to a
      // task of its own: a question set there would be asked where nothing draws it.
      const front = now.current.inFront;
      const shown = front !== undefined && drawsCrumbFor.current(front, session);
      const where = from === "elsewhere" ? (shown ? "crumb" : undefined) : from;
      if (asksInAModal(ending) || where === undefined) {
        setTaskEndInline(undefined);
        setTaskEndAsk({ ...ending, session, belowToo: true, busy: false });
        return;
      }
      setTaskEndInline({ session, where, busy: false, ...inPlace(ending, way) });
    },
    [plane],
  );
  /** Restarts a chat, as the Restart chat row does: held here for the second step's answer,
   *  which is declared before the restart is (`askRestart`). */
  const restartNow = useRef<(session: number) => void>(() => undefined);
  /** The second step was answered: the task is ended the way it said, or kept. */
  const answerTaskEnd = useCallback(
    async (yes: boolean) => {
      const asked = taskEndInline;
      if (asked === undefined) return;
      if (!yes) {
        setTaskEndInline(undefined);
        return;
      }
      // A task's Restart chat, asked the way an end is (#1489): the answer restarts it.
      if (asked.act === "restart") {
        setTaskEndInline(undefined);
        restartNow.current(asked.session);
        return;
      }
      setTaskEndInline({ ...asked, busy: true, trouble: undefined });
      const refused = await endTaskNow(asked.session, asked.way, false);
      // A refusal stays where the answer was given, in the core's words, and nothing ended.
      setTaskEndInline(
        refused === undefined ? undefined : { ...asked, busy: false, trouble: refused },
      );
    },
    [endTaskNow, taskEndInline],
  );
  /** A way to end a task was pressed on its row in the Chats list: asked about on that row.
   *  Held, so the list's rows are not drawn again for it. */
  const endTaskOnRow = useCallback(
    (session: number, way: TaskEndWay) => void askToEndTask(session, way, "row"),
    [askToEndTask],
  );
  const answerTaskEndOnRow = useCallback(
    (yes: boolean) => void answerTaskEnd(yes),
    [answerTaskEnd],
  );
  /** What every breadcrumb's two controls are handed (`TaskEndContext`). */
  const taskEndHand = useMemo<TaskEndHand>(
    () => ({
      confirming: taskEndInline,
      onEnd: (session, way) => void askToEndTask(session, way, "crumb"),
      onConfirm: (yes) => void answerTaskEnd(yes),
    }),
    [answerTaskEnd, askToEndTask, taskEndInline],
  );
  /** A kept-open chat's Notice put away, by Dismiss or by its Close tab. */
  const forgetKeptOpen = useCallback(
    (session: number) => setKeptOpen((was) => was.filter((one) => one.session !== session)),
    [],
  );
  /** **Close tab** on a kept-open chat's Notice: the plain Close, the person's own press. It
   *  ends the chat and grants nothing, as the close dialog's Close does. */
  const closeKeptOpen = useCallback(
    (session: number) => {
      forgetKeptOpen(session);
      const before = now.current;
      change((tabs) => closeChat(tabs, session, filedIn, isPinned, inTheBackground.current));
      tasksGoBack([session], before);
      void commands.closeSession(plane, session);
    },
    [change, filedIn, forgetKeptOpen, isPinned, plane, tasksGoBack],
  );
  /** …and the ones just pressed: their tabs, their explorer rows and their menus say so. */
  const wrapping = useMemo<ReadonlySet<number>>(
    () => (leaving.size === 0 ? told : new Set([...told, ...leaving])),
    [leaving, told],
  );
  /** A tab in the background: every pane of it is a chat wrapping up (`tabs.tabsIn`). */
  const isBackground = useCallback<Backgrounded>(
    (id) => {
      const all = contentsOf(tabs, id);
      return (
        all.length > 0 &&
        all.every(({ content }) => content.kind === "session" && wrapping.has(content.session))
      );
    },
    [tabs, wrapping],
  );
  useLayoutEffect(() => {
    inTheBackground.current = isBackground;
  }, [isBackground]);

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
  // **This project's theme and its repos' save rows stay known while the window holds it**
  // (FR-27), in front or not. The window reads both for the project in front, and an answer
  // nothing holds is forgotten (`projectTheme.ts`, `saving.ts`): held here, a switch back draws
  // them at once, rather than asking the core and drawing the whole window again when it
  // replies.
  useProjectThemeKept(plane, ofWorkspace);
  useRepoSavingKept(plane, ofWorkspace);
  /** The changes the focused workspace's panels are made of (FD-10): its own, and the
   *  personas'. A todo closed in another workspace does not read this one again. */
  const workspaceChanges = usePlaneChanged([plane], panelsOf(ofWorkspace ?? ""));
  const workspaceState = useWorkspaceState(plane, ofWorkspace, rereadWorkspace, workspaceChanges);
  /** The plane root's own panels — its session records (SI-8d) — while it is focused. */
  const rootChanges = usePlaneChanged([plane], ROOT_PANELS);
  const rootPanels = usePlaneRootPanels(plane, focused === OUTSIDE, rootChanges);
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
  //
  // **The plane root starts its chats in the plane's own directory, said out loud** (SI-1),
  // rather than as the `null` that happens to mean the same: the core tells a chat started
  // there that it is in no workspace.
  const startIn =
    spot?.path ??
    (focused === OUTSIDE
      ? sidebar?.root
      : sidebar?.workspaces.find((ws) => ws.name === focused)?.path) ??
    null;

  /**
   * The branch the explorer is focused on in the workspace in front, while the workspace still
   * lists it (`Explorer`'s own rule), and else none: what ⌘P and ⌘⇧F look in first.
   */
  const cockpit = useMemo(() => {
    if (focusedBranch === undefined || focusedBranch.workspace !== ofWorkspace) return undefined;
    return focusStands(workspaceState, focusedBranch) === undefined ? undefined : focusedBranch;
  }, [focusedBranch, ofWorkspace, workspaceState]);
  // **A focus whose branch was removed is forgotten** (#1152), once the workspace's listing has
  // answered without it, so the record the window keeps does not put it back at the next launch.
  // Never while the listing is still being read, and never on a refusal, which says nothing of
  // the branch. Only for the workspace in front: another one's listing is not read here.
  //
  // Set while rendering, React's way of adjusting state to what a render found: an effect would
  // draw the gone focus once more first, and the answer is already here.
  if (
    focusedBranch !== undefined &&
    focusedBranch.workspace === ofWorkspace &&
    focusGone(workspaceState, focusedBranch)
  )
    setFocusedBranch(undefined);
  /** The nearest branch to the operator: the cockpit's, else the one the explorer picked. */
  const nearBranch = useMemo<Place | undefined>(
    () =>
      cockpit ??
      (ofWorkspace !== undefined && spot !== undefined
        ? { workspace: ofWorkspace, repo: spot.repo, piece: spot.piece ?? null }
        : undefined),
    [cockpit, ofWorkspace, spot],
  );

  /** What the explorer picks, remembered against the workspace it was picked in. */
  const pickSpot = useCallback(
    (next: Spot | undefined) => {
      if (ofWorkspace === undefined) return;
      setPickedSpot(next === undefined ? undefined : { workspace: ofWorkspace, spot: next });
    },
    [ofWorkspace],
  );

  /** Every workspace the window can bring forward: **the plane root first, always** (SI-1),
   *  then this project's workspaces — the pinned ones first, in the order they were pinned in,
   *  then the rest in the plane's own order, which is the sidebar's.
   *  The palette lists all of them; the strip draws fewer (`onWorkspaceStrip`, below). */
  const strips = useMemo(() => {
    if (sidebar === undefined) return [];
    const names = sidebar.workspaces.map((ws) => ws.name);
    // **Pinned first, in the order they were pinned in, then the rest in the plane's own
    // order** (ADR 0039, ADR 0054, charter#402). The store answers the pins in that order, and
    // only the ones the plane still has. The root is never a pin: it is drawn before them.
    return [
      OUTSIDE,
      ...pinnedWorkspaces.filter((name) => names.includes(name)),
      ...names.filter((name) => !pinnedWorkspaces.includes(name)),
    ];
  }, [pinnedWorkspaces, sidebar]);

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
    () =>
      strips.filter(
        (name) => name === OUTSIDE || pinnedWorkspaces.includes(name) || name === focused,
      ),
    [focused, pinnedWorkspaces, strips],
  );

  /** And which of those the strip has room to draw. The same rule as the chats' one level
   *  down: when the pins alone do not fit, what does not fit goes behind show-more too, so the
   *  strip never scrolls and never loses its `+`. */
  const { strip: workspaceStrip, width: workspaceRoom } = useRoom(onWorkspaceStrip.length);
  /** The workspace tabs' ids, which the strip's tablist owns them by (#1204). */
  const workspaceTabId = useTabIds();
  // The floors grow with the window's text (charter-app#283, `fits.leastAt`).
  const windowText = useTextSizes().window;
  const workspaceLeast = leastAt(LEAST.workspace, windowText);
  const chatLeast = leastAt(LEAST.chat, windowText);
  // The plane root's tab is an icon, as wide as this and never wider (`.plane-root`): it is
  // taken off the room before the workspaces share what is left, so the arithmetic and the
  // stylesheet agree about it as they do about `--least`.
  const rootWidth = leastAt(LEAST_ROOT, windowText);
  // A chip is exactly this wide (`.tab.chip`), and the chat strip fits by it: a chip holding a
  // tab's floor would hide a tab the strip has room for.
  const chipWidth = leastAt(LEAST_CHIP, windowText);
  const workspacesShown = useMemo(() => {
    const { shown } = fitting(
      onWorkspaceStrip.filter((name) => name !== OUTSIDE),
      focused,
      workspaceRoom <= 0 ? workspaceRoom : Math.max(1, workspaceRoom - rootWidth),
      workspaceLeast,
    );
    // **The root is always drawn, and always first** (SI-1): it is never hidden behind
    // show-more, whatever else does not fit.
    const drawn = [OUTSIDE, ...shown];
    return { shown: drawn, hidden: strips.filter((name) => !drawn.includes(name)) };
  }, [focused, onWorkspaceStrip, rootWidth, strips, workspaceRoom, workspaceLeast]);

  /**
   * **Each workspace's colour** (charter-app#281), as the core read it out of its
   * `workspace.json` with the sidebar — so it is read again whenever the plane changes on disk,
   * a write in Settings at the Workspace level included. `null` for a workspace with none, for the
   * chats outside every workspace, which have no file to hold one, and for a grey `#rrggbb`,
   * which has no hue to tint with: no mark is drawn for a colour that tints nothing (the
   * Workspace level in Settings says why).
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

  /** Every chat the core lists, by number. */
  const chatsByNumber = useMemo(
    () =>
      new Map(
        [
          ...(sidebar?.workspaces.flatMap((ws) => ws.chats) ?? []),
          ...(sidebar?.unfiled ?? []),
          // Tasks a scenario spec pretends (`e2eTasks.ts`): none in a shipped build.
          ...pretended.map((one) => one.chat),
        ].map((chat) => [chat.session, chat]),
      ),
    [pretended, sidebar],
  );
  /**
   * **Where a chat came from**, as the core lists it; and **until that list is read, as the
   * launch put it back** (#1489). The tabs are drawn from what the launch said before the
   * list arrives, and a task's own tab must be a task's from its first frame: its `−`, never
   * a session's `×` for the moment in between.
   */
  const fromOf = useCallback(
    (session: number) =>
      chatsByNumber.get(session)?.from ??
      (sidebar === undefined ? reopened.find((chat) => chat.session === session)?.from : undefined),
    [chatsByNumber, reopened, sidebar],
  );
  /**
   * **Which chat a task is a task of**, as the core lists it (#1486). Task links only: a
   * handoff is a session of its own and is never shown inside another's tab (V100-69).
   */
  const askedBy = useCallback<AskedBy>(
    (session) => {
      const from = fromOf(session);
      return from?.task ? from.chat : undefined;
    },
    [fromOf],
  );
  /**
   * **What each pane of the tab in front shows, and whether it can be drawn** (`shownLive`,
   * #1486): a pane draws another chat's terminal only while that chat is open and at home in
   * that pane. Nothing is judged before the list of chats is read.
   */
  const frontShown = useMemo(
    () =>
      tabs.inFront === undefined
        ? []
        : shownLive(tabs, tabs.inFront, askedBy, (session) =>
            sidebar === undefined ? false : chatsByNumber.has(session),
          ),
    [askedBy, chatsByNumber, sidebar, tabs],
  );

  /** How many chats in `workspace` need you: its tab's count, and its share of the count on
   *  the show-more button when the strip is not drawing it (ADR 0054). One reading of the
   *  queue for both, so the button goes down exactly when the tab would. */
  //
  // **A chat is counted where its tab is** (#1486, V100-40): a task that works in another
  // workspace lives in the tab of the session that asked, so that session's workspace wears
  // it. A chat with no tab is counted where it works, as before.
  const waitingIn = (queue: readonly number[], workspace: string) =>
    queue.filter((session) => {
      const home = homeOf(tabs, session, askedBy);
      return (
        (home === undefined ? filedIn(session) : workspaceOf(tabs, home.tab, filedIn)) === workspace
      );
    }).length;

  const workspaceMarks = (workspace: string) => {
    const here = tabsIn(tabs, workspace, filedIn).length;
    const called = workspace === OUTSIDE ? OUTSIDE_TITLE : workspace;
    const colour = colourOf(workspace);
    const root = workspace === OUTSIDE;
    return (
      <>
        {/* Its colour, as a mark in its own accent (charter-app#281) — on the strip and in the
            menu of what the strip is not drawing, which is why it is here and not a style of
            the tab alone. Hidden from a screen reader: the name says which workspace. */}
        {colour !== null && (
          <span className="workspace-mark" aria-hidden="true" style={tintOf(workspace)} />
        )}
        {/* The plane root is drawn as an icon alone (SI-1): its tab's `aria-label` and tooltip
            say what it is. Everything else is drawn by name. */}
        {root ? (
          <FolderRoot className="node-icon" aria-hidden="true" />
        ) : (
          <span className="workspace-name">{called}</span>
        )}
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
        <QueueRead>
          {(queue) => {
            const waiting = waitingIn(queue, workspace);
            return (
              waiting > 0 && (
                <span
                  className="workspace-needs"
                  aria-label={`${waiting} chats need you in ${called}`}
                >
                  {waiting}
                </span>
              )
            );
          }}
        </QueueRead>
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
    () =>
      sidebar === undefined ? tabs.order : tabsIn(tabs, focused, filedIn, isPinned, isBackground),
    [filedIn, focused, isBackground, isPinned, sidebar, tabs],
  );

  /**
   * How much room the chat strip has, and therefore which of its tabs it draws.
   *
   * **What does not fit is not drawn** — the operator's call, reversing what ADR 0039 left
   * open. `fits.ts` holds the whole of why the answer is arithmetic over one measured width
   * rather than an intersection measurement over fifty tabs, and what it costs.
   *
   * The `+` and the show-more button are siblings of this strip rather than children of
   * it, so its own width is already what is left for tabs and `controls` goes on nothing.
   */
  const { strip: measured, width: room } = useRoom(onStrip.length);
  /** The chat tabs' ids, which the strip's tablist owns them by (#1204). */
  const chatTabId = useTabIds();
  /** Whether chat tab `id` is drawn as its name box rather than as a tab: it is being renamed
   *  and is not a chip, which is drawn as a tab whatever is open for renaming (a chat can go
   *  into the background with its name open). One answer for the strip and for its tablist's
   *  `aria-owns`, so the tablist never misses a tab it draws (#1204). */
  const nameBoxFor = (id: number) => renaming === id && !isBackground(id);
  /** The chat strip itself, for handing the keyboard back to a tab after a rename. */
  const chatStrip = useRef<HTMLElement | null>(null);
  /** Unique to this view, so two projects' tabs never name each other's fresh marks. */
  const freshMarks = useId();
  const freshMarkOf = (tab: number) => `${freshMarks}-fresh-${tab}`;
  /** The id of the counts on a tab's chip (#1487), which the tab is described by. */
  const tasksIdOf = (tab: number) => `${freshMarks}-tasks-${tab}`;
  const strip = useCallback(
    (element: HTMLElement | null) => {
      chatStrip.current = element;
      measured(element);
    },
    [measured],
  );
  const { shown, hidden } = useMemo(
    () => fitting(onStrip, tabs.inFront, room, chatLeast, { is: isBackground, width: chipWidth }),
    [onStrip, room, tabs.inFront, chatLeast, isBackground, chipWidth],
  );

  /**
   * When each chat last moved, as the CORE counts it (ADR 0039).
   *
   * The window cannot work this out: the strip is an opening order and the needs-you queue
   * is oldest-first, and neither is "when did this chat last do something". The count comes
   * down on `chat-moved` and in the first snapshot, so two windows on one plane agree and a
   * relaunch does not invent an order out of whatever it happened to draw first.
   */
  const lastMoved =
    (states: ChatStates): LastMoved =>
    (session) =>
      movedAt(states, session);

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
  const waitingOn = (queue: readonly number[], id: number) =>
    panesOf(tabs, id).filter((pane) => queue.includes(pane.session)).length +
    // And its tasks that need you and are not on screen (#1486): the tab wears them. Its own
    // chat is counted above, shown or not.
    hiddenNeeding(tabs, id, askedBy, queue).filter(
      (session) => !panesOf(tabs, id).some((pane) => pane.session === session),
    ).length;

  // Read as an order and redrawn only when the order changes: a move by a chat the strip is
  // drawing changes nothing in a menu of the ones it is not.
  const notShowing = useChatsSelect(
    chats,
    (states) => byLastActivity(hidden, tabs, lastMoved(states)),
    sameList,
  );

  /**
   * And what the workspace strip's show-more menu lists: the workspaces it is not drawing —
   * the ones nobody pinned and you are not in, and any pins it had no room for — **most
   * recently moved first** — the chat strip's rule one level up (ADR 0039, ADR 0054).
   *
   * A workspace moved when the last of its chats did. One nothing has been heard about reads
   * `0` and keeps the strip's order, for `byLastActivity`'s reason.
   */
  const workspacesNotShowing = useChatsSelect(
    chats,
    (states) => {
      const movedIn = (workspace: string) =>
        Math.max(
          0,
          ...tabsIn(tabs, workspace, filedIn)
            .flatMap((id) => panesOf(tabs, id))
            .map((pane) => movedAt(states, pane.session)),
        );
      return [...workspacesShown.hidden].sort((one, other) => movedIn(other) - movedIn(one));
    },
    sameList,
  );

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
        refusedBy("start-options", options.error);
        return;
      }
      succeeded("start-options");
      setPicking({ options: options.data, where });
    },
    [plane, refusedBy, succeeded],
  );

  /**
   * **A fix the picker's refusal offers** (NO-8, #1233): the doctor's fix by its id, through the
   * one entry point `charter doctor --fix` and the Doctor dialog use. Then the picker reads its
   * options again, so what it draws is what is true after the fix; a refusal is said in it.
   */
  const fixInPicker = useCallback(
    async (id: string) => {
      setPickerFixing(true);
      setPickerTrouble(undefined);
      const fixed = await commands
        .planeDoctorFix(plane, id)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      const refused =
        fixed.status === "error"
          ? fixed.error
          : (fixed.data.refused ?? (fixed.data.complete ? undefined : fixed.data.said.join(" ")));
      const options = await commands
        .startOptions(plane)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      setPickerFixing(false);
      if (options.status === "ok")
        setPicking((was) => (was === undefined ? was : { ...was, options: options.data }));
      setPickerTrouble(refused ?? (options.status === "error" ? options.error : undefined));
    },
    [plane],
  );

  const newTab = useCallback(() => void ask({ tab: true }), [ask]);
  /** A new tab whose chat starts in that directory — this one, and not the next. */
  const newTabIn = useCallback((path: string) => void ask({ tab: true, in: path }), [ask]);

  /**
   * **A shell tab** (SI-5): the operator's own `$SHELL` in `cwd`, filed on `filed`'s strip, in a
   * tab of its own in front. `open_session` with no program is the core's shell, and the core
   * puts charter's shims first on its `PATH` (ADR 0062). Nothing asks first: a shell starts no
   * harness, so there is no profile for the picker to ask about.
   *
   * Named `shell <N>` as the chat's own name, not only the tab's, so the tab that comes back
   * from the record reads the same: a chat's name is what its tab is drawn from at a relaunch.
   */
  const startShell = useCallback(
    (
      start: (name: string) => ReturnType<typeof commands.openSession>,
      filed: string,
      typed?: Typed,
    ) => {
      const name = `shell ${now.current.named.tabs + 1}`;
      void start(name)
        .then((opened) => {
          if (opened.status === "error") {
            refusedBy("shell", opened.error);
            return;
          }
          succeeded("shell");
          const session = opened.data;
          setShells((was) => new Set(was).add(session));
          setStartedIn((was) => ({ ...was, [session]: filed }));
          change((tabs) => openTab(tabs, session, name));
          // A command typed in for the operator, run as if they had typed it: the shell reads
          // it once it is up, and the tab is theirs to leave. A harness's installer is named,
          // never spelled: the core types its own compiled-in line (FR-29, V65).
          if (typed?.line !== undefined)
            void commands.sendInput(plane, session, typed.held ? typed.line : `${typed.line}\n`);
          else if (typed?.installer !== undefined)
            void commands
              .typeInstaller(plane, session, typed.installer)
              .then((said) => {
                if (said.status === "error") refusedBy("shell", said.error);
              })
              .catch((err: unknown) => refusedBy("shell", String(err)));
          // Typed and not run: installing needs sudo, so the person presses Return (V78 c).
          else if (typed?.sandboxInstall)
            void commands
              .typeSandboxInstall(plane, session)
              .then((said) => {
                if (said.status === "error") refusedBy("shell", said.error);
              })
              .catch((err: unknown) => refusedBy("shell", String(err)));
        })
        .catch((err: unknown) => refusedBy("shell", String(err)));
    },
    [change, plane, refusedBy, succeeded],
  );
  const openShell = useCallback(
    (cwd: string | null, filed: string, typed?: Typed) =>
      startShell(
        (name) =>
          commands.openSession(
            plane,
            null,
            [],
            cwd,
            name,
            STARTING_SIZE.columns,
            STARTING_SIZE.rows,
          ),
        filed,
        typed,
      ),
    [plane, startShell],
  );

  /**
   * **A shell tab in a folder of a branch** (FM-10): `openShell`'s tab, started by the core in
   * the folder it resolved — inside the branch, through no link — so the window never names the
   * directory. Filed on the branch's workspace's strip.
   */
  const shellInFolder = useCallback(
    (at: BranchPath) =>
      startShell(
        (name) =>
          commands.openShellInBranch(
            plane,
            at.workspace,
            at.repo,
            at.piece,
            at.path,
            name,
            STARTING_SIZE.columns,
            STARTING_SIZE.rows,
          ),
        at.workspace,
      ),
    [plane, startShell],
  );

  /**
   * **A file or folder row's own rows** (FM-10): its path copied, revealed, or handed to your
   * editor. Each is the core's to place and carry out; a refusal comes back in its sentence.
   */
  const fileDoing = useMemo(() => {
    const answered = (asked: Promise<{ status: "ok" } | { status: "error"; error: string }>) =>
      asked
        .then((said): Ran =>
          said.status === "error" ? { ok: false, refused: said.error } : { ok: true },
        )
        .catch((err: unknown): Ran => ({ ok: false, refused: String(err) }));
    return {
      copyPath: async (at: BranchPath, absolute: boolean): Promise<Ran> => {
        const said = await answered(
          commands.copyBranchPath(plane, at.workspace, at.repo, at.piece, at.path, absolute),
        );
        return said.ok
          ? { ok: true, said: `Copied ${absolute ? "the absolute" : "the"} path of ${at.path}.` }
          : said;
      },
      revealPath: (at: BranchPath) =>
        answered(commands.revealBranchPath(plane, at.workspace, at.repo, at.piece, at.path)),
      openInEditor: async (at: BranchPath, line: number): Promise<Ran> => {
        const editor = yourEditor();
        if (editor === undefined)
          return { ok: false, refused: "Choose your editor in Settings first." };
        return answered(
          commands.openInYourEditor(plane, at.workspace, at.repo, at.piece, at.path, line, editor),
        );
      },
    };
  }, [plane]);

  /**
   * A shell tab at the project root with `shellAsked`'s command typed in (FR-4: `gh auth
   * login` or `glab auth login`, from the first run's "Sign in to GitHub" or "Sign in to
   * GitLab"). Once the plane is read, so the root is known, and once per ask.
   */
  const shellHandled = useRef<number | undefined>(undefined);
  const root = sidebar?.root;
  useEffect(() => {
    if (shellAsked === undefined || root === undefined) return;
    if (shellHandled.current === shellAsked.at) return;
    shellHandled.current = shellAsked.at;
    openShell(root, OUTSIDE, { line: shellAsked.typed });
  }, [openShell, root, shellAsked]);

  /** A shell tab where a new chat would start — or in `workspace`'s own directory, filed under
   *  it, when a row names one. */
  const newShell = useCallback(
    (workspace?: string) => {
      if (workspace === undefined) {
        openShell(startIn, filedFor(startIn, focused));
        return;
      }
      // The plane root's own row (SI-1): the plane's directory, filed on the root's strip.
      if (workspace === OUTSIDE) {
        if (sidebar !== undefined) openShell(sidebar.root, OUTSIDE);
        return;
      }
      const path = sidebar?.workspaces.find((ws) => ws.name === workspace)?.path;
      if (path !== undefined) openShell(path, workspace);
    },
    [focused, openShell, sidebar, startIn],
  );

  /**
   * **A forge login the repo picker asked for** (NO-8, #1233): the forge CLI's own login, typed
   * in a shell tab at the project root and left for the operator to run — its host is the
   * project's, so they read the line before Return.
   */
  useEffect(() => {
    const login = (event: Event) => {
      const asked = (event as CustomEvent<LoginAsk>).detail;
      if (asked.plane !== plane || sidebar === undefined) return;
      // "Typed, not run" holds here, not only in the core's host check: a line with a control
      // character in it (a newline is a Return) is refused and typed nowhere.
      if (HAS_CONTROL.test(asked.line)) {
        refusedBy("shell", "purlis will not type a login line that holds a control character");
        return;
      }
      openShell(sidebar.root, OUTSIDE, { line: asked.line, held: true });
    };
    window.addEventListener(FORGE_LOGIN, login);
    return () => window.removeEventListener(FORGE_LOGIN, login);
  }, [openShell, plane, refusedBy, sidebar]);

  /**
   * **A blocked save's two ways out** (charter-app#295), asked by the Saving tab: a chat started
   * in the plane — the picker, so the operator chooses who resolves it — or a plain terminal
   * there, a shell with no harness, for somebody who resolves a conflict with git by hand. The
   * terminal is a shell tab like any other, opened by the same function.
   */
  useEffect(() => {
    const out = (event: Event) => {
      const asked = (event as CustomEvent<WayOut>).detail;
      if (asked.plane !== plane || sidebar === undefined) return;
      if (asked.way === "chat") {
        newTabIn(sidebar.root);
        return;
      }
      openShell(sidebar.root, OUTSIDE);
    };
    window.addEventListener(WAY_OUT, out);
    return () => window.removeEventListener(WAY_OUT, out);
  }, [newTabIn, openShell, plane, sidebar]);

  /**
   * **The harness setup tab's two asks** (FR-29): a harness's installer, typed by the core
   * into a shell tab at the project's root, or the picker to start a chat in the directory the
   * tab is about, on the harness it names, once one is installed.
   */
  useEffect(() => {
    const asked = (event: Event) => {
      const wanted = (event as CustomEvent<HarnessSetupAsk>).detail;
      if (wanted.plane !== plane) return;
      if (wanted.way === "chat") {
        void ask({ tab: true, in: wanted.cwd, prefer: wanted.harness });
        return;
      }
      // At the project's root, as FR-4's sign-in is, and never in the fresh clone: its own
      // shell hooks (direnv, mise) have nothing to do with installing a harness.
      // Filed on the workspace's strip, beside the setup tab the operator comes back to.
      if (root !== undefined) openShell(root, wanted.workspace, { installer: wanted.harness });
    };
    window.addEventListener(HARNESS_SETUP, asked);
    return () => window.removeEventListener(HARNESS_SETUP, asked);
  }, [ask, openShell, plane, root]);

  /**
   * A harness started by hand in one of this project's shell tabs (ADR 0062): the core says so,
   * and the tab's pane draws a banner until it is answered. Filtered on the plane, as
   * `chat-moved` is. A window that cannot listen (a unit test with no events) never hears one.
   */
  useEffect(() => {
    let gone = false;
    let stop: (() => void) | undefined;
    void (async () => {
      try {
        const unlisten = await listen<ByHand>("harness-by-hand", (event) => {
          const told = event.payload;
          if (gone || told.plane !== plane) return;
          setByHand((was) => ({
            ...was,
            [told.session]: { harness: told.harness, cwd: told.cwd },
          }));
        });
        if (gone) unlisten();
        else stop = unlisten;
      } catch {
        // No window to listen in.
      }
    })();
    return () => {
      gone = true;
      stop?.();
    };
  }, [plane]);

  /** A start notice put away: it is said once, and the operator has read it. */
  const dismissStartNote = useCallback((session: number) => {
    setStartNotes((was) =>
      Object.fromEntries(Object.entries(was).filter(([held]) => Number(held) !== session)),
    );
  }, []);

  /** The banner's two answers: open that harness as a chat where the shell was standing — the
   *  picker, started on that harness — or put the banner away. Either way it is answered. */
  const answerByHand = useCallback(
    (session: number, open: boolean) => {
      const note = byHand[session];
      setByHand((was) =>
        Object.fromEntries(Object.entries(was).filter(([held]) => Number(held) !== session)),
      );
      if (!open || note === undefined) return;
      void ask({ tab: true, in: note.cwd ?? startIn ?? undefined, prefer: note.harness });
    },
    [ask, byHand, startIn],
  );

  /**
   * Starts a chat on `profile` at `where`: the picker's Start, and the first chat of a repo
   * opened into this project when there was nothing to pick (FR-4). Answers the core's refusal,
   * or `undefined` once the chat is on a tab. `filedIn` names the strip the tab goes on when the
   * caller has just focused one this render cannot see yet.
   */
  const startOn = useCallback(
    async (
      where: Where,
      kind: string | undefined,
      profile: string,
      persona: string | null,
      showFooter: boolean,
      label: string | null,
      newBranch: boolean,
      filedIn?: string,
      withoutSandbox: WithoutSandbox | null = null,
    ): Promise<string | undefined> => {
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
          // The core cuts the branch only when `cwd` is a repo's clone (GL-1).
          // The opt-out is the picker's alone (ADR 0067 §7): every other start sends none.
          { show_footer: showFooter, new_branch: newBranch, without_sandbox: withoutSandbox },
          STARTING_SIZE.columns,
          STARTING_SIZE.rows,
        )
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (started.status === "error") return started.error;
      const session = started.data.session;
      // `?? []`, as the label's `?? null` below: an older core, or a stand-in, sends none.
      const notes = started.data.notices ?? [];
      const agentsMd = started.data.agents_md ?? [];
      if (notes.length > 0) setStartNotes((was) => ({ ...was, [session]: { notes, agentsMd } }));
      // Where charter put it, written down before the tab is drawn: the plane will say the
      // same thing a tick later, and until it does this is what keeps the tab on the strip
      // the operator is looking at.
      const filed = filedIn ?? filedFor(cwd, focused);
      setStartedIn((was) => ({ ...was, [session]: filed }));
      if ("tab" in where) {
        const held = started.data.label ?? null;
        change((tabs) => openTab(tabs, session, name, whoOf(persona, kind), held));
        return undefined;
      }
      const before = now.current;
      // The tab that was to be split can have closed while the picker was open. Nothing
      // would show that session, so it is ended rather than left running unseen.
      if (change((tabs) => splitFocusedPane(tabs, where.split, session, name)) === before) {
        void commands.closeSession(plane, session);
      }
      return undefined;
    },
    [change, focused, plane, startIn],
  );

  /** A row was picked: the chat starts on that profile, with that persona, either drawing
   *  charter's footer in its pane or leaving it blank (ADR 0029), and under the name typed in
   *  the picker, if one was (charter-app#254). */
  /** Whether the picker's start is in flight: the ref is the guard, the state is the button. */
  const startingNow = useRef(false);
  const [starting, setStarting] = useState(false);
  const startPicked = useCallback(
    async (
      profile: string,
      persona: string | null,
      showFooter: boolean,
      label: string | null,
      newBranch: boolean,
      withoutSandbox: WithoutSandbox | null,
    ) => {
      if (picking === undefined || startingNow.current) return;
      const kind = picking.options.profiles.find((one) => one.name === profile)?.kind;
      // **One start per picker at a time** (GL-1). A start runs off the main thread for as
      // long as its branch takes to check out, and a second press of Start in that time was a
      // second chat. The ref answers at once, where state would answer on the next render.
      startingNow.current = true;
      setStarting(true);
      const refused = await startOn(
        picking.where,
        kind,
        profile,
        persona,
        showFooter,
        label,
        newBranch,
        undefined,
        withoutSandbox,
      ).finally(() => {
        startingNow.current = false;
        setStarting(false);
      });
      if (refused !== undefined) {
        // In the picker, not behind it: the operator is still choosing, and a refusal they
        // cannot see beside the rows is one they cannot act on.
        setPickerTrouble(refused);
        return;
      }
      setPicking(undefined);
      setPickerTrouble(undefined);
    },
    [picking, startOn],
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
      newBranch: boolean,
      withoutSandbox: WithoutSandbox | null,
    ) => {
      // A start already running from this picker is the one start it gets (GL-1).
      if (startingNow.current) return;
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
      await startPicked(profile, persona, showFooter, label, newBranch, withoutSandbox);
    },
    [plane, startPicked],
  );

  const split = useCallback((direction: Direction) => void ask({ split: direction }), [ask]);

  useEffect(() => {
    listedNow.current = chatsByNumber;
  }, [chatsByNumber]);

  const closePane = useCallback(() => {
    const tab =
      now.current.inFront === undefined ? undefined : now.current.byId[now.current.inFront];
    // A pane showing a task has no close (#1486): the catalogue says why, and nothing here
    // ends the session under a task on the strength of a row built a moment earlier.
    if (tab?.shows?.[tab.focused] !== undefined) return;
    const going = tab && panesOf(now.current, tab.id).find((pane) => pane.pane === tab.focused);
    // Nor is a task's own pane ever closed (#1489): its control is the minimise, which is
    // another verb and ends nothing.
    if (going !== undefined && askedByNow(going.session) !== undefined) return;
    const before = now.current;
    change((tabs) => closeFocusedPane(tabs, filedIn, isPinned, isBackground));
    // Its tasks with a tab or a pane of their own go back to the list, the one beside it too:
    // a task is never left as the own chat of what was its session's tab.
    if (going) tasksGoBack([going.session], before);
    if (going) void commands.closeSession(plane, going.session);
  }, [askedByNow, change, filedIn, isBackground, isPinned, plane, tasksGoBack]);

  /**
   * **Closes a session's tab: the only close on the strip** (#1489, V100-39). Its chats end.
   * **A task never ends by a close**: one beside its session goes with the tab's panes and
   * goes on working, and every task below the session that has a tab of its own is sent back
   * to the list ({@link tasksGoBack}), since the tab it would go back into is the one closing.
   * Stopping them instead is the dialog's other answer, which the core carries out
   * (`closeStopping`).
   */
  const close = useCallback(
    (id: number) => {
      const before = now.current;
      const ending = panesOf(before, id)
        .map((one) => one.session)
        .filter((session) => askedByNow(session) === undefined);
      change((tabs) => closeTab(tabs, id, filedIn, isPinned, isBackground));
      // **A close ends a chat, and never a task** (#1488, V100-38): a task whose pane was in
      // the tab goes back to the Chats list, and the core is told it has no tab
      // (`close_chat_tab`, the one command for it).
      for (const one of panesOf(before, id))
        if (askedByNow(one.session) !== undefined)
          void commands.closeChatTab(plane, one.session).catch(() => undefined);
      tasksGoBack(ending, before);
      for (const session of ending) void commands.closeSession(plane, session);
    },
    [askedByNow, change, filedIn, isBackground, isPinned, plane, tasksGoBack],
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

  /**
   * **Gives a task a tab of its own** (#1489, V100-38): on the strip of the workspace it works
   * in, in front, with the keyboard in it. Asked for by the person: its row's menu, its line
   * in its tab's menu, its pane's controls, the palette; or by pressing it at all, where the
   * person set tasks to open that way (`chatsListPrefs`, V100-74). Also what a pressed task
   * falls back to when the session that asked has no tab here. **Nothing ends**, and the
   * session's tab is left on what it showed. The core is told, so the tab comes back at the
   * next launch.
   */
  const ownTab = useCallback(
    (session: number) => {
      const listed = listedNow.current.get(session);
      if (listed === undefined) return;
      const opened = change((tabs) =>
        moveToOwnTab(
          tabs,
          session,
          listed.name,
          whoOf(listed.persona, listed.harness),
          listed.label,
          askedByNow,
        ),
      );
      void commands.openChatTab(plane, session).catch(() => undefined);
      const tab = tabHolding(opened, session);
      if (tab !== undefined) bringToFront(tab);
      giveKeyboardTo(plane, session);
      stoppedFor(session, undefined);
    },
    [askedByNow, bringToFront, change, plane, stoppedFor],
  );

  /**
   * **Opens a task beside its session, inside the session's tab** (#1489, V100-38): the
   * session's chat on one side and the task on the other, with the keyboard in the task. A
   * tab it had of its own goes. Nothing for a task whose session has no tab here: its row
   * says why (`actions.placeRows`).
   */
  const besideOf = useCallback(
    (session: number) => {
      const before = now.current;
      const next = change((tabs) =>
        openBeside(tabs, session, askedByNow, listedNow.current.get(session)?.name),
      );
      if (next === before) return;
      // It is a pane of its session's tab now, and no tab of its own.
      void commands.closeChatTab(plane, session).catch(() => undefined);
      const tab = tabHolding(next, session);
      const workspace =
        sidebar === undefined || tab === undefined ? undefined : workspaceOf(next, tab, filedIn);
      if (workspace !== undefined) setPicked(workspace);
      giveKeyboardTo(plane, session);
      stoppedFor(session, undefined);
    },
    [askedByNow, change, filedIn, plane, sidebar, stoppedFor],
  );

  /**
   * **Sends a task back out of its own tab or its pane** (#1489): the minimise. The tab leaves
   * the strip, or the pane leaves the split; **the task goes on**, at home in its session's
   * tab again, whose chip lists it. No command that ends a chat is sent, and nothing is asked.
   * The front goes where closing that tab would send it; a split's other pane takes the room
   * and the keyboard.
   */
  const sendBackOf = useCallback(
    (session: number) => {
      const before = now.current;
      const from = tabHolding(before, session);
      const next = change((tabs) => sendBack(tabs, session, filedIn, isPinned, isBackground));
      if (next === before) return;
      void commands.closeChatTab(plane, session).catch(() => undefined);
      // What shared its split has the room now, and the keyboard with it.
      const beside = from !== undefined && next.inFront === from ? focusedChat(next) : undefined;
      if (beside !== undefined) giveKeyboardTo(plane, beside);
    },
    [change, filedIn, isBackground, isPinned, plane],
  );

  /**
   * **Goes to a chat: the one way** (#1486). Every surface that shows a chat comes through
   * here: a row of the Chats list and of the explorer, the hand on a row above it, the title
   * bar's list, the palette, a name in a pane's breadcrumb.
   *
   * **A chat is shown in its home** (`tabs.homeOf`). A session's own chat is its tab. A task is
   * shown INSIDE the tab of the session that asked for it: that tab comes forward and is
   * switched to the task (`tabs.switchTabTo`), no tab is added, and the keyboard goes into the
   * task's terminal. The same press on the session's own chat switches the tab back. A task
   * with a tab or a pane of its own (#1489) is at home there, and that comes forward.
   *
   * **Where the person set tasks to open in their own tabs** (V100-74), a task shown inside
   * its session's tab is given its tab instead ({@link ownTab}).
   *
   * **A task whose session has no tab in this window** gets a tab of its own too, since there
   * is no tab to show it in: the session that asked was closed and left it running.
   *
   * A chat listed because its Smart close stopped (SI-8f) has been looked at, so it leaves
   * the list.
   */
  const showChat = useCallback(
    (session: number, inside = false) => {
      const home = homeOf(now.current, session, askedByNow);
      const task = askedByNow(session) !== undefined;
      // `inside` is the next and the previous chat in a tab (V100-36): they move inside the
      // tab whatever the setting, or one key would give every task of a session a tab.
      const tabbed = !inside && chatsListPrefs().tabbed;
      if (home === undefined || (task && home.own !== session && tabbed)) {
        // A tab of its own from here on. The person pressed for this, so nothing moved by
        // itself.
        ownTab(session);
        return;
      }
      const was = shownIn(now.current, home.tab).find((one) => one.pane === home.pane)?.session;
      change((tabs) => switchTabTo(tabs, session, askedByNow));
      const workspace =
        sidebar === undefined ? undefined : workspaceOf(now.current, home.tab, filedIn);
      if (workspace !== undefined) setPicked(workspace);
      // The pane shows another chat now: whoever pressed for it types next.
      if (was !== session) giveKeyboardTo(plane, session);
      stoppedFor(session, undefined);
    },
    [askedByNow, change, filedIn, ownTab, plane, sidebar, stoppedFor],
  );

  /**
   * **Goes to a task that has ended**, for a needs-you item's Go and the away summary's link
   * (#1491, #1514): its chat, where that is still open (a task that reported blocked stays
   * one), and otherwise its finished row under the session `asker` that asked, which the Chats
   * list brings into view. Where there is nothing to show (no finished row, or the Chats list
   * is not on screen), this says so and shows the session. **A failure its session is flagged
   * for is looked at only once it was shown**: the core is told which, by its id.
   */
  const goToEnded = useCallback(
    (asker: number, ended: FailedBelow) => {
      const flagged = (failedTasks[asker] ?? []).some((one) => one.id === ended.id);
      const open =
        ended.chat === null
          ? undefined
          : chatsListed.current.find((chat) => chat.session === ended.chat);
      if (open !== undefined) {
        showChat(open.session);
        if (flagged) void commands.taskFailureSeen(plane, asker, ended.id).catch(() => undefined);
        return;
      }
      const row = (finishedNow.current.get(asker) ?? []).find((task) => task.id === ended.id);
      if (row === undefined || !chatsListDrawn()) {
        refusedBy(
          "needs-you",
          row === undefined
            ? `${ended.task} has no row left to show. Its chat is shown instead.`
            : `${ended.task} is in the Chats list, which is not on screen. Its chat is shown instead.`,
        );
        showChat(asker);
        return;
      }
      succeeded("needs-you");
      setRevealed((was) => ({
        asker,
        task: row.name,
        id: row.id,
        at: (was?.at ?? 0) + 1,
      }));
    },
    [failedTasks, plane, refusedBy, showChat, succeeded],
  );
  /**
   * **Go, on a needs-you item** (#1491, V100-15). An item that is there because a task failed,
   * ended without a report or did not start **goes to that task**: its chat, where that is
   * still open (a task that reported blocked stays one), and otherwise its finished row
   * under the session that asked, which the Chats list brings into view.
   *
   * **The failure is looked at only once it was shown**, and only that one: the core is told
   * which, by its id, when the chat was shown or the row was found (`failureShown`). Where
   * there is nothing to show (no finished row, or the Chats list is not on screen), this says
   * so, shows the session and leaves the hand where it is: one attempt, and no more.
   *
   * Every other item goes to its chat, as it always did.
   */
  const showNeeding = useCallback(
    (session: number) => {
      const failed = failedTasks[session] ?? [];
      if (failed.length === 0) {
        showChat(session);
        return;
      }
      // The latest, which is the one the item says.
      const latest = failed[failed.length - 1];
      goToEnded(session, latest);
    },
    [failedTasks, goToEnded, showChat],
  );
  /** **Goes to a finished task** from the away summary (#1514): as a needs-you item's Go. */
  const showFinished = useCallback(
    (task: FinishedTask) =>
      goToEnded(task.asker, { id: task.id, task: task.name, chat: task.chat }),
    [goToEnded],
  );
  /**
   * What became of a row the Chats list was asked to bring into view. A failed task's row
   * that was shown has been looked at, so its item goes; one that could not be shown (the
   * list said so) leaves its item, and the session is shown instead.
   */
  const revealedSettled = useCallback(
    (asked: Reveal, shown: boolean) => {
      if (asked.id === undefined) return;
      const id = asked.id;
      // Only a failure its session is flagged for has anything to be looked at.
      if (!shown) showChat(asked.asker);
      else if ((failedTasks[asked.asker] ?? []).some((one) => one.id === id))
        void commands.taskFailureSeen(plane, asked.asker, id).catch(() => undefined);
    },
    [failedTasks, plane, showChat],
  );
  /** A finished row was opened to be read: where it is a failure its session is flagged for,
   *  it has been looked at (#1491). */
  const lookedAtFinished = useCallback(
    (task: FinishedTask) => {
      if ((failedTasks[task.asker] ?? []).some((failed) => failed.id === task.id))
        void commands.taskFailureSeen(plane, task.asker, task.id).catch(() => undefined);
    },
    [failedTasks, plane],
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
  const present = useCallback(
    (open: (tabs: Tabs, strip: string) => Tabs) => {
      const next = change((tabs) => open(tabs, focused ?? OUTSIDE));
      const workspace =
        next.inFront === undefined ? undefined : workspaceOf(next, next.inFront, filedIn);
      if (workspace !== undefined) setPicked(workspace);
    },
    [change, filedIn, focused],
  );
  const showView = useCallback(
    (view: ViewRef, title: string, on?: string) => {
      present((tabs, strip) => openView(tabs, view, title, on ?? strip));
      // Opening Settings, or bringing it forward, is a way into it: the keyboard goes there too
      // (#1206, `settings/entering.ts`).
      const place = placeOfView(view, plane);
      if (place !== undefined) enterSettings(place);
    },
    [plane, present],
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
  /** A plane's memories (SI-9b): the preview tab, edits, Delete and its Undo. */
  const memoryEdits = useMemoryEdits({
    plane,
    present,
    update: change,
    closeView,
    reread: rereadPanels,
  });

  /**
   * Settings at the Project level (SE-19; the Project settings page before it, charter-app#252),
   * opened when the window asks for it — the project's menu and the palette. Asked
   * through the window even from this project's own palette, so there is one way in: the
   * window brings the project forward and this opens its tab. `handled` keeps a rebuilt
   * `showView` — it changes with the focused workspace — from opening it a second time for
   * the same ask.
   */
  const handled = useRef(settingsAsked);
  useEffect(() => {
    if (settingsAsked === undefined || handled.current === settingsAsked) return;
    handled.current = settingsAsked;
    showView(settingsView("project"), SETTINGS_TAB_TITLE);
  }, [settingsAsked, showView]);

  /** A file ⌘P found here (FM-7), opened in its file tab the way the explorer opens one. */
  const fileHandled = useRef(fileAsked?.at);
  useEffect(() => {
    if (fileAsked === undefined || fileHandled.current === fileAsked.at) return;
    fileHandled.current = fileAsked.at;
    // A jump to a line (a search hit) opens the branch's file tab, which picks the file and
    // lands on the line (`fileJump.ts`); a file ⌘P found opens in a tab of its own.
    const { place, path } = fileAsked;
    const jump = fileAsked.line !== undefined;
    showView(
      jump ? pieceFilesView(place) : pieceFileView(place, path),
      jump ? pieceFilesTitle(place) : pieceFileTitle(place, path),
    );
  }, [fileAsked, showView]);

  /** The Saving tab (charter-app#294), opened the same way and for the same reason. */
  const savingHandled = useRef(savingAsked);
  useEffect(() => {
    if (savingAsked === undefined || savingHandled.current === savingAsked) return;
    savingHandled.current = savingAsked;
    showView(SAVING_VIEW, SAVING_TITLE);
  }, [savingAsked, showView]);

  /** The same tab, asked by a dialog of this project (`saving.askSavingTab`, #1296). */
  useEffect(() => {
    const asked = (event: Event) => {
      if ((event as CustomEvent<{ plane: string }>).detail.plane !== plane) return;
      showView(SAVING_VIEW, SAVING_TITLE);
    };
    window.addEventListener(OPEN_SAVING, asked);
    return () => window.removeEventListener(OPEN_SAVING, asked);
  }, [plane, showView]);

  /** Settings at a workspace's level (SE-20; charter-app#280), on that workspace's strip: what
   *  its menu's and the palette's Workspace settings… open. */
  const openWorkspaceSettings = useCallback(
    (workspace: string) =>
      showView(workspaceSettingsView(workspace), workspaceSettingsTitle(workspace), workspace),
    [showView],
  );

  /**
   * **Follows a link into Settings** (SE-22, `settings/links.ts`): the group is shown at its
   * place, and the Settings tab for its level and target is opened — or brought forward, when
   * it is open. A link this window cannot follow opens nothing.
   */
  const openSettingsAt = useCallback(
    (link: SettingsLink) => {
      const to = landing(link, plane);
      if (to === undefined) return;
      linkToGroup(to.place, link.group, link.setting);
      showView(to.view, to.title, to.workspace);
    },
    [plane, showView],
  );

  /** A link a view of this project asked for — the Saving view's Notice (NO-7) — opened the
   *  same way. */
  useEffect(() => {
    const asked = (event: Event) => {
      const wanted = (event as CustomEvent<SettingsLinkAsk>).detail;
      if (wanted.plane === plane) openSettingsAt(wanted.link);
    };
    window.addEventListener(SETTINGS_LINK, asked);
    return () => window.removeEventListener(SETTINGS_LINK, asked);
  }, [plane, openSettingsAt]);

  /** A link the window followed, opened the same way and for the same reason (SE-22). */
  const settingsLinkHandled = useRef(settingsLinkAsked?.at);
  useEffect(() => {
    if (settingsLinkAsked === undefined || settingsLinkHandled.current === settingsLinkAsked.at)
      return;
    settingsLinkHandled.current = settingsLinkAsked.at;
    openSettingsAt(settingsLinkAsked.link);
  }, [settingsLinkAsked, openSettingsAt]);

  /**
   * The Settings tab, opened the same way and for the same reason (SE-16), **at the focused
   * level** (SE-23, V89g): the focused workspace's, else this project's. The app menu's
   * Settings… (`⌘,`, `Ctrl+,` off a Mac) and the palette's Settings… both land here; You is the
   * window's answer when no project is open (`App.tsx`), and one press of the level switcher
   * from here. The plane root is not a workspace, so it is the project's level.
   */
  const settingsTabHandled = useRef(settingsTabAsked);
  useEffect(() => {
    if (settingsTabAsked === undefined || settingsTabHandled.current === settingsTabAsked) return;
    settingsTabHandled.current = settingsTabAsked;
    const workspace = focused === OUTSIDE ? undefined : focused;
    const open =
      workspace === undefined
        ? () => showView(settingsView("project"), SETTINGS_TAB_TITLE)
        : () => openWorkspaceSettings(workspace);
    open();
  }, [focused, openWorkspaceSettings, settingsTabAsked, showView]);

  /** Settings at the You level (SE-23's Your settings…), opened the same way. */
  const yourSettingsHandled = useRef(yourSettingsAsked);
  useEffect(() => {
    if (yourSettingsAsked === undefined || yourSettingsHandled.current === yourSettingsAsked)
      return;
    yourSettingsHandled.current = yourSettingsAsked;
    showView(settingsView("you"), SETTINGS_TAB_TITLE);
  }, [yourSettingsAsked, showView]);

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

  /**
   * The first chat in a repository just opened into this project (FR-4, #603): its workspace
   * is focused and the picker is asked for, to start in the repository's clone. Only once the
   * plane has been read and lists that workspace, since focusing a workspace the strip has not
   * heard of would focus nothing. The picker still asks which harness (ADR 0022).
   */
  const firstChatHandled = useRef<number | undefined>(undefined);
  useEffect(() => {
    if (firstChatAsked === undefined || firstChatHandled.current === firstChatAsked.at) return;
    if (!sidebar?.workspaces.some((ws) => ws.name === firstChatAsked.workspace)) return;
    firstChatHandled.current = firstChatAsked.at;
    const { workspace, cwd, harness, noneInstalled, instructions } = firstChatAsked;
    const where: Where = { tab: true, in: cwd };
    // **The repo's agent instructions, offered beside the chat and not in front of it**
    // (FR-18a): a tab that asks nothing until the operator goes to it, so W10's budget —
    // the forge question when the repo's remote does not say, the trust question and, only
    // when there is a choice, the picker (`interruptBudget.ts`) — is not spent on it.
    // Nothing is written until its own press.
    // **The first task, offered the same way** (FR-28): a tab beside the first chat, so a
    // partner session has the script one press away and nobody is asked anything for it.
    const offerInstructions = () => {
      if (instructions > 0)
        change((tabs) =>
          offerView(
            tabs,
            repoInstructionsView(workspace),
            repoInstructionsTitle(workspace),
            workspace,
          ),
        );
    };
    // The first task (FR-28) is offered only beside a chat that can start: with no harness on
    // the machine (FR-29) its runs could not, so the setup tab offers the instructions alone.
    const offerBeside = () => {
      offerInstructions();
      if (cwd !== "")
        change((tabs) => offerView(tabs, firstTaskView(cwd), firstTaskTitle(workspace), workspace));
    };
    // `ask`'s steps, written out so the state is set in the command's callback: the
    // `react-hooks/set-state-in-effect` rule reads a call in an effect body as synchronous
    // however far the setState is from it (`Extensions.tsx` says the same).
    void commands
      .startOptions(plane)
      .then(async (options) => {
        if (options.status === "error") {
          refusedBy("start-options", options.error);
          return;
        }
        succeeded("start-options");
        focusWorkspace(workspace);
        // **No harness on this machine** (FR-29): the picker would list harnesses none of
        // which can start, so the first chat is the setup tab — each one's official installer,
        // run in a shell tab on a press — in front, and the picker comes from its Start a chat.
        if (noneInstalled) {
          change((tabs) =>
            openView(tabs, harnessSetupView(cwd), harnessSetupTitle(workspace), workspace),
          );
          offerInstructions();
          return;
        }
        // **Nothing to pick, so nothing is asked** (FR-4, W10's interrupt budget): one
        // harness signed in on this machine, and a profile of it that needs no approval. The
        // picker still comes up whenever there is a choice or a command to approve (ADR 0022).
        const ofIt = options.data.profiles.filter((row) => row.kind === harness);
        const only = ofIt.find((row) => row.is_default) ?? ofIt[0];
        if (harness !== null && only !== undefined && only.approval === null) {
          const persona =
            options.data.persona !== null && options.data.personas.includes(options.data.persona)
              ? options.data.persona
              : null;
          const refused = await startOn(
            where,
            only.kind,
            only.name,
            persona,
            false,
            null,
            // Nothing was asked, so the default stands: a first chat in a repo starts on a
            // branch of its own like any other (GL-1).
            true,
            workspace,
          );
          if (refused === undefined) {
            offerBeside();
            return;
          }
          setPickerTrouble(refused);
        } else setPickerTrouble(undefined);
        offerBeside();
        setPicking({ options: options.data, where });
      })
      .catch((err: unknown) => refusedBy("start-options", String(err)));
  }, [change, firstChatAsked, focusWorkspace, plane, refusedBy, sidebar, startOn, succeeded]);

  /** What each chat the project lists is called (`shownName`, #1484), by its number: the one
   *  rule, asked once per chat, for every surface that names a chat by number. */
  const chatNames = useMemo(
    () =>
      new Map(
        [...(sidebar?.workspaces.flatMap((ws) => ws.chats) ?? []), ...(sidebar?.unfiled ?? [])].map(
          (chat) => [chat.session, shownName(tabs, chat)],
        ),
      ),
    [sidebar, tabs],
  );
  /** {@link nameOf} as it stands, for what is decided in a handler and not while drawing. */
  const nameOfNow = useRef<(session: number) => string>((session) => String(session));
  /** What a chat the project lists is called now, or nothing for one it does not list. */
  const nameOfListed = useCallback((session: number) => chatNames.get(session), [chatNames]);
  /** What a chat is called here: as its row names it (`shownName`); for a chat the project
   *  does not list yet, the tab holding it; or its session number. */
  const nameOf = useCallback(
    (session: number) => {
      const listed = chatNames.get(session);
      if (listed !== undefined) return listed;
      const held = tabHolding(tabs, session);
      return held !== undefined ? tabs.byId[held].name : String(session);
    },
    [chatNames, tabs],
  );

  useEffect(() => {
    nameOfNow.current = nameOf;
  }, [nameOf]);

  const frontTab = tabs.inFront === undefined ? undefined : tabs.byId[tabs.inFront];
  // The session the next worktree question is about: the chat in the pane that has the
  // keyboard, which is the one "this chat's worktree" means.
  // The chat SHOWN there (#1486): while the tab shows a task, the task is what is typed into.
  // And nothing at all while that pane shows a task it cannot draw: there is no terminal there.
  const frontSession = frontShown.some((one) => one.pane === frontTab?.focused && !one.live)
    ? undefined
    : focusedChat(tabs);
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
      return { ok: true, said: `The folder of ${cut.piece} is gone. Its branch stays.` };
    },
    [plane],
  );

  /** Asks for a new branch in one of the focused workspace's repos. It cuts nothing: the
   *  dialog is what asks, and `worktree_add` is what cuts (GL-1). */
  const newBranch = useCallback(
    (repo: string) => {
      if (ofWorkspace === undefined) return;
      setBranchTrouble(undefined);
      setBranching({ workspace: ofWorkspace, repo });
    },
    [ofWorkspace],
  );

  /**
   * Cuts the branch the dialog named — `null` for charter's own `chat-<n>` — and makes it
   * where the next chat starts, the way picking its row would. A refusal stays in the dialog,
   * verbatim; what the cut found to say (a dirty clone, a layer that did not land) is reported.
   */
  const cutBranch = useCallback(
    async (branch: string | null) => {
      if (branching === undefined) return;
      const { workspace, repo } = branching;
      setBusyBranching(true);
      const answer = await commands
        .worktreeAdd(plane, workspace, repo, branch)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      setBusyBranching(false);
      if (answer.status === "error") {
        setBranchTrouble(answer.error);
        return;
      }
      setBranching(undefined);
      setBranchTrouble(undefined);
      setPickedSpot({
        workspace,
        spot: { repo, piece: answer.data.piece, path: answer.data.path },
      });
      setRereadWorkspace((asked) => asked + 1);
      setReport({
        from: "worktree.add",
        refused: false,
        words: [
          `New branch ${answer.data.branch} in ${repo}. New chats start on it.`,
          ...answer.data.warnings,
        ].join(" "),
      });
    },
    [branching, plane],
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
   * **Forgets a dormant pin**, for good: the existing unpin (`Store::pin_workspace`). The pins
   * before it are kept, nearest first, so this run's Undo can put it back in its place.
   */
  const forgetDormantPin = useCallback(
    async (name: string) => {
      const after = pinOrder.slice(0, Math.max(pinOrder.indexOf(name), 0)).reverse();
      const unpinned = await pinWorkspace(name, false);
      if (!unpinned.ok) {
        setReport({ from: "workspace.unpin", refused: true, words: unpinned.refused });
        return;
      }
      setForgottenPins((was) => [...was.filter((one) => one.name !== name), { name, after }]);
    },
    [pinOrder, pinWorkspace],
  );

  /**
   * **Undoes a Forget**: pins the workspace again and puts it back after the nearest pin that
   * came before it and is still pinned, or first when none is. Pinning appends, so the order is
   * written after it; arranging moves only the pins it names (`Store::arrange_workspaces`).
   */
  const undoForget = useCallback(
    async (name: string, after: string[]) => {
      /** Puts the Notice back, with what refused, so a refused Undo can be pressed again. */
      const refused = (words: string) => {
        setForgottenPins((was) => [...was.filter((one) => one.name !== name), { name, after }]);
        setReport({ from: "workspace.pin", refused: true, words });
      };
      setForgottenPins((was) => was.filter((one) => one.name !== name));
      const pinned = await pinWorkspace(name, true);
      if (!pinned.ok) return refused(pinned.refused);
      // The store's order now, read rather than remembered: another Undo may have just put a
      // neighbour back.
      const now = await commands
        .planePins(plane)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (now.status === "error") return refused(now.error);
      const said = now.data as Partial<typeof now.data> | null | undefined;
      const rest = (Array.isArray(said?.order) ? said.order : []).filter((one) => one !== name);
      const anchor = after.find((one) => rest.includes(one));
      const place = anchor === undefined ? 0 : rest.indexOf(anchor) + 1;
      const order = [...rest.slice(0, place), name, ...rest.slice(place)];
      const arranged = await commands
        .arrangeWorkspacePins(plane, order)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (arranged.status === "error")
        setReport({ from: "workspace.pin", refused: true, words: arranged.error });
      setPinning((asked) => asked + 1);
    },
    [pinWorkspace, plane],
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
   * the window follows is its own: the view tabs on the workspace's strip, every view keyed by
   * its name and those views' pins (#1248), and the workspace it has picked. Then it reads the
   * plane again, as after any change to what workspaces there are.
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
      // Through `change`, as every other write to the tabs is: a write past it is undone by the
      // next one, which starts from `now` (#1248).
      change((was) => followRename(was, workspace, name, root));
      setPinnedViews((was) => was.map((key) => renamedViewKey(key, workspace, name, root)));
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
    [change, plane, root],
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
      if (key !== PASS_THROUGH_KEY) return { ok: false, refused: `purlis cannot send ${key}.` };
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
   *
   * **`early` is the one exception: the mark is drawn before the write lands**, and taken back
   * if the core refuses. A drop across the pinned boundary asks for it (SI-6b), because the
   * drop has already moved the tab and a pin that waited would draw it in its old group until
   * the core answered — a jump under the pointer.
   */
  const pinTab = useCallback(
    async (id: number, pinned: boolean, { early = false } = {}): Promise<Ran> => {
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
      const mark = (on: boolean) =>
        setPinnedChats((was) =>
          on
            ? was.includes(session)
              ? was
              : [...was, session]
            : was.filter((one) => one !== session),
        );
      if (early) mark(pinned);
      const said = await commands
        .pinChat(plane, session, pinned)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") {
        if (early) mark(!pinned);
        return { ok: false, refused: said.error };
      }
      mark(pinned);
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
   * **The order and the pin are the window's at once**, because the pin is written by the core
   * and a tab that waited for it would be drawn in its old group until the answer came (SI-6b).
   * A pin the core refuses is taken back and said, as the tab's menu would say it, and the tab
   * is drawn where its pin says it goes.
   */
  const dragTab = useCallback(
    (moved: number, onto: number) => {
      // **A tab in the background is out of the arrangement** (SI-8f): it cannot be picked up,
      // nothing is put down on it, and the tabs dragged past it leave its place in the order
      // where it was — which is where it comes back if its smart close ends without a record.
      const loose = (id: number) => !isBackground(id);
      const made = afterDrop({
        whole: onStrip.filter(loose),
        drawn: shown.filter(loose),
        moved,
        onto,
        isPinned,
      });
      if (made === undefined) return;
      change((tabs) => ({ ...tabs, order: reslotted(tabs.order, made.order) }));
      if (made.pinned === undefined) return;
      void pinTab(moved, made.pinned, { early: true }).then((ran) => {
        if (!ran.ok) setReport({ from: "tab.drag", refused: true, words: ran.refused });
      });
    },
    [change, isBackground, isPinned, onStrip, pinTab, shown],
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
      stoppedFor(session, undefined);
      const said = await commands
        .ignoreNeedsYou(plane, session)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      return said.status === "error" ? { ok: false, refused: said.error } : { ok: true };
    },
    [plane, stoppedFor],
  );

  /** Cancels a chat's smart close: its tab's menu row, or the palette's. */
  const cancelSmartClose = useCallback(
    async (session: number): Promise<Ran> => {
      const said = await commands
        .cancelSmartClose(plane, session)
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

  /**
   * A curation action chosen from a "Curate ▸" menu or the palette (ADR 0061): the core opens
   * the chat — the default profile, the action's runner, where it runs — and holds its prompt
   * until the harness reports its start; this puts its tab on the strip it is filed under, in
   * front, named for the action and its subject. A refusal is the core's sentence, before
   * anything started.
   */
  const curate = useCallback(
    async (subject: string, action: string): Promise<Ran> => {
      const started = await commands
        .curate(plane, subject, action, STARTING_SIZE.columns, STARTING_SIZE.rows)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (started.status === "error") return { ok: false, refused: started.error };
      const chat = started.data;
      setStartedIn((was) => ({ ...was, [chat.session]: chat.workspace ?? OUTSIDE }));
      change((tabs) =>
        openTab(tabs, chat.session, chat.name, whoOf(chat.persona, chat.harness), chat.label),
      );
      return { ok: true };
    },
    [change, plane],
  );

  /**
   * **"Start a chat here"** on a file or folder row (FM-9): a chat the core starts on the
   * project's default profile in the branch's folder, with a reference to the row typed as its
   * first prompt once its harness has started, and never sent. Its tab goes on the strip it is
   * filed under, as a curation chat's does. Where the harness cannot be typed into, the core
   * copies the reference, and the window says so.
   */
  const startChatHere = useCallback(
    async (at: BranchPath): Promise<Ran> => {
      const started = await commands
        .startChatHere(
          plane,
          at.workspace,
          at.repo,
          at.piece,
          at.path,
          null,
          STARTING_SIZE.columns,
          STARTING_SIZE.rows,
        )
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (started.status === "error") return { ok: false, refused: started.error };
      const chat = started.data;
      setStartedIn((was) => ({ ...was, [chat.session]: chat.workspace ?? OUTSIDE }));
      change((tabs) =>
        openTab(tabs, chat.session, chat.name, whoOf(null, chat.harness), chat.label),
      );
      if (chat.copied === null) return { ok: true };
      const why = chat.copied.charAt(0).toUpperCase() + chat.copied.slice(1);
      return { ok: true, said: `${why}: ${chat.text}` };
    },
    [change, plane],
  );

  /**
   * **What the first task's tab asks for** (FR-28): a run started by the core — on a branch of its
   * own, with the task typed and unsent — whose tab goes on the strip it is filed under, in
   * front; and a run's diff, in a shell tab in its branch's folder with the core's diff command
   * run there.
   */
  const [firstTaskRuns, setFirstTaskRuns] = useState<FirstTaskDoes["runs"]>({});
  const firstTaskDoes = useMemo<FirstTaskDoes>(
    () => ({
      runs: firstTaskRuns,
      start: async (clone, profile, persona, run) => {
        const started = await commands
          .firstTaskRun(
            plane,
            clone,
            profile,
            persona,
            run,
            STARTING_SIZE.columns,
            STARTING_SIZE.rows,
          )
          .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
        if (started.status === "error") return started.error;
        const chat = started.data;
        setFirstTaskRuns((was) => ({ ...was, [clone]: { ...was[clone], [run]: chat } }));
        setStartedIn((was) => ({ ...was, [chat.session]: chat.workspace ?? OUTSIDE }));
        change((tabs) =>
          openTab(tabs, chat.session, chat.name, whoOf(chat.persona, chat.harness), chat.label),
        );
        return chat;
      },
      showDiff: (run) => openShell(run.folder, run.workspace ?? OUTSIDE, { line: run.diff }),
    }),
    [change, firstTaskRuns, openShell, plane],
  );

  /**
   * The chats a Resume started (SI-8d), by session, until each has said whether its harness
   * brought the conversation back: a ref, because what it holds decides a start and never a
   * render.
   */
  const resuming = useRef(new Map<number, Resuming>());

  /**
   * **Resume a session from its record** (SI-8d): the core starts a NEW chat in the record's
   * place, on its harness, given its conversation where it can be, and told the record in its
   * briefing (`resume_session`). Its tab opens in front, and the note above the panes says which
   * happened — *was resumed*, or *came back as a new chat: <why>* — from the same `reopened`
   * list a relaunch's chats are said from.
   *
   * `insteadOf` is the second ask, for a chat whose harness could not bring the conversation
   * back ({@link lostOnResume}): the same record, started fresh, as the same chat (ADR 0066).
   */
  const resumeSession = useCallback(
    async (path: string, insteadOf?: number): Promise<Ran> => {
      const name = String(now.current.named.tabs + 1);
      const afterFailure = insteadOf !== undefined;
      const said = await commands
        .resumeSession(
          plane,
          path,
          name,
          insteadOf ?? null,
          STARTING_SIZE.columns,
          STARTING_SIZE.rows,
        )
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") return { ok: false, refused: said.error };
      const chat = said.data;
      // A new occurrence: whatever was dismissed about this chat was about the one before.
      for (const family of CHAT_NOTES) {
        const cause = chatNote(family, chat);
        if (cause !== undefined) showAgain(cause);
      }
      resuming.current.set(chat.session, { path, afterFailure, heard: false });
      setStartedIn((was) => ({ ...was, [chat.session]: filedFor(chat.cwd, focused) }));
      setReopened((was) => [...was.filter((one) => one.session !== chat.session), chat]);
      change((tabs) =>
        openTab(tabs, chat.session, name, whoOf(chat.persona, chat.harness), chat.label),
      );
      return { ok: true };
    },
    [change, focused, plane, showAgain],
  );

  /**
   * **A chat the window started again** — a Retry now, or a Start fresh (NO-3) — known the way a
   * chat a relaunch put back is: where it is filed, its notes on how it came back, and its pin
   * and its shell's mark.
   */
  const noteStarted = useCallback(
    (chat: OpenChat) => {
      setStartedIn((was) => ({ ...was, [chat.session]: filedFor(chat.cwd, focused) }));
      setReopened((was) => [...was.filter((one) => one.session !== chat.session), chat]);
      if (chat.pinned) setPinnedChats((was) => [...was, chat.session]);
      if (isShell(chat)) setShells((was) => new Set(was).add(chat.session));
    },
    [focused],
  );

  /**
   * **Each chat's finished tasks** (#1485), read again whenever the sidebar is: that is when a
   * chat started, ended or closed, which is when a task finishes or its asking chat goes.
   */
  const {
    finished: finishedTasks,
    read: rereadFinished,
    settled: finishedSettled,
  } = useFinishedTasks(plane, sidebar);
  useEffect(() => {
    finishedNow.current = finishedTasks;
  }, [finishedTasks]);
  /** Clear finished: the rows go, and nothing else does. */
  const clearFinished = useCallback(
    (ids: string[]) => {
      void commands
        .clearFinishedTasks(plane, ids)
        .catch(() => undefined)
        .then(rereadFinished);
    },
    [plane, rereadFinished],
  );
  /**
   * **Reopen a finished task** (#1485): the core starts a new chat on the task's conversation,
   * and its tab opens in front as any chat's does. It is an ordinary chat from then on. A
   * refusal is the row's to say.
   */
  const reopenFinished = useCallback(
    async (task: FinishedTask): Promise<string | undefined> => {
      const said = await commands
        .reopenFinishedTask(plane, task.id, STARTING_SIZE.columns, STARTING_SIZE.rows)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") return said.error;
      const chat = said.data;
      noteStarted(chat);
      change((tabs) =>
        openTab(tabs, chat.session, chat.name, whoOf(chat.persona, chat.harness), chat.label),
      );
      rereadFinished();
      return undefined;
    },
    [change, noteStarted, plane, rereadFinished],
  );

  /**
   * **A chat a pane shows as a task was started again under a new number** (#1486): what the
   * pane remembers of it goes with it, so the pane keeps showing it whichever arrives first,
   * the new number in the tabs or in the core's list. Between the two it is drawn as it was
   * last known, never as the session's own chat.
   */
  const followRestart = useCallback((was: number, to: number) => {
    setRemembered((all) => {
      let moved: Map<string, Crumbs> | undefined;
      for (const [key, crumbs] of all) {
        const [tab, pane, session] = key.split(":");
        if (Number(session) !== was) continue;
        moved ??= new Map(all);
        moved.delete(key);
        moved.set(`${tab}:${pane}:${to}`, {
          ...crumbs,
          path: crumbs.path.map((chat) => (chat.session === was ? { ...chat, session: to } : chat)),
        });
      }
      return moved ?? all;
    });
  }, []);

  /**
   * **A chat restarted** (#1342, #1428): the same chat on its conversation, in a new session,
   * which takes the old one's place as Start fresh's does.
   */
  const chatRestarted = useCallback(
    (was: number, chat: OpenChat) => {
      setReopened((all) => all.filter((one) => one.session !== was));
      setPinnedChats((all) => all.filter((one) => one !== was));
      noteStarted(chat);
      followRestart(was, chat.session);
      change((tabs) => replaceSession(tabs, was, chat.session));
    },
    [change, followRestart, noteStarted],
  );

  /**
   * **The chats owed a restart** (#1342, #1428) — to take a sandbox grant, or because the person
   * asked with Restart chat — and the driver that restarts each once its turn has ended: here
   * and not in a Notice, so dismissing the Notice or redrawing the pane loses none. The core
   * holds the same list (`owed_restarts`), which is read again when the view is drawn anew.
   *
   * **One restart, whoever asked** (`restart_chat`): a block's Allow, Restart now on a Notice,
   * a tab's Restart chat and the Notice after a sandbox setting changed all come through here.
   */
  const [owedRestarts, setOwedRestarts] = useState<readonly number[]>([]);
  /** Why a restart did not happen, by session, until it is retried or put away. */
  const [restartTrouble, setRestartTrouble] = useState<Readonly<Record<number, string>>>({});
  /** Why a restart has not happened yet, in the core's sentence, by session: the chat waits on
   *  a permission prompt, and restarts once that is answered. */
  const [restartNotYet, setRestartNotYet] = useState<Readonly<Record<number, string>>>({});
  const restartingNow = useRef(new Set<number>());
  /** The chats a restart is under way for, which each one's pane says while it is. */
  const [restartsRunning, setRestartsRunning] = useState<readonly number[]>([]);
  useEffect(() => {
    let gone = false;
    void commands
      .owedRestarts(plane)
      .then((owed) => {
        // A list or nothing, as the view's other reads take one (#1342).
        if (!gone && owed.status === "ok")
          setOwedRestarts(Array.isArray(owed.data) ? owed.data : []);
      })
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [plane]);
  const oweRestart = useCallback(
    (session: number) =>
      setOwedRestarts((owed) => (owed.includes(session) ? owed : [...owed, session])),
    [],
  );
  /** Restarts chat `session`, which is owed it, now: the driver's step. */
  const restartOwed = useCallback(
    (session: number) => {
      if (restartingNow.current.has(session)) return;
      restartingNow.current.add(session);
      setRestartsRunning((running) => [...running, session]);
      const failed = (why: string) => {
        setOwedRestarts((owed) => owed.filter((one) => one !== session));
        setRestartNotYet((was) => withoutKey(was, session));
        setRestartTrouble((was) => ({ ...was, [session]: why }));
      };
      void commands
        .restartChat(plane, session, STARTING_SIZE.columns, STARTING_SIZE.rows)
        .then((done) => {
          if (done.status === "error") failed(done.error);
          else if (done.data.chat !== null) {
            const chat = done.data.chat;
            setOwedRestarts((owed) => owed.filter((one) => one !== session));
            setRestartTrouble((was) => withoutKey(was, session));
            setRestartNotYet((was) => withoutKey(was, session));
            chatRestarted(session, chat);
            // What the new run's start found to say: that a chat which ran without the
            // sandbox runs in it again. `?? []`, as a start's notices are read.
            const notes = done.data.notices ?? [];
            if (notes.length > 0)
              setStartNotes((was) => ({ ...was, [chat.session]: { notes, agentsMd: [] } }));
          } else {
            // Not yet (a permission prompt is open): still owed, asked again at its next move.
            // Said on its pane in the core's sentence, so a press is never answered by nothing.
            const why = done.data.not_yet ?? NOT_YET;
            setRestartNotYet((was) => (was[session] === why ? was : { ...was, [session]: why }));
          }
        })
        .catch((err: unknown) => failed(`purlis could not restart the chat: ${String(err)}`))
        .finally(() => {
          restartingNow.current.delete(session);
          setRestartsRunning((running) => running.filter((one) => one !== session));
        });
    },
    [plane, chatRestarted],
  );
  // **Only once the turn has ended** (#1342): the chat is waiting for you, or its program is
  // done. Mid-turn it would lose what it was doing; a harness that reports no state is never
  // restarted behind the person's back, and its pane offers Restart now instead.
  useEffect(() => {
    if (owedRestarts.length === 0) return;
    const drive = () => {
      const states = chats.store.statesFor(chats.plane);
      for (const session of owedRestarts) {
        const state = stateOf(states, session);
        if (state === "waiting" || state === "done" || state === "failed") restartOwed(session);
      }
    };
    drive();
    return chats.store.subscribe(drive);
  }, [owedRestarts, chats, restartOwed]);
  /**
   * What each chat's pane says of its restart ({@link RestartSaid}): under way, not yet, owed by
   * hand, or why it failed.
   */
  const restartsSaid = useMemo(() => {
    const states = chats.store.statesFor(chats.plane);
    const out: Record<number, RestartSaid> = {};
    for (const session of owedRestarts)
      out[session] = { owed: true, byHand: stateOf(states, session) === "unknown" };
    for (const session of restartsRunning) out[session] = { ...out[session], running: true };
    for (const [session, notYet] of Object.entries(restartNotYet))
      if (owedRestarts.includes(Number(session)))
        out[Number(session)] = { ...out[Number(session)], notYet };
    for (const [session, trouble] of Object.entries(restartTrouble))
      out[Number(session)] = { trouble };
    return out;
  }, [owedRestarts, restartsRunning, restartNotYet, restartTrouble, chats]);
  /** What a row of the Chats list says of its chat's restart (#1462): why it was refused, or
   *  why it waits. */
  const restartsOnRows = useMemo(() => {
    const out: Record<number, string> = {};
    for (const [session, said] of Object.entries(restartsSaid)) {
      const words = said.trouble ?? said.notYet;
      if (words !== undefined) out[Number(session)] = words;
    }
    return out;
  }, [restartsSaid]);
  /**
   * **The person asks for chat `session` to restart** (#1428, and #1362's Restart now): the core
   * owes it the restart (`ask_chat_restart`), and it happens when the chat's turn has ended.
   * Mid-turn it waits, as a restart for a grant does. A chat whose harness reports no state
   * restarts now: the person pressed for it, so there is nobody else to wait for.
   */
  const askRestart = useCallback(
    async (session: number) => {
      const said = await commands
        .askChatRestart(plane, session)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") {
        setRestartTrouble((was) => ({ ...was, [session]: said.error }));
        return;
      }
      setRestartTrouble((was) => withoutKey(was, session));
      oweRestart(session);
      if (stateOf(chats.store.statesFor(chats.plane), session) !== "running") restartOwed(session);
    },
    [chats, oweRestart, plane, restartOwed],
  );
  const answerRestart = useCallback(
    (session: number, act: "now" | "dismiss") => {
      setRestartTrouble((was) => withoutKey(was, session));
      setRestartNotYet((was) => withoutKey(was, session));
      // Asked again, so a restart the core no longer owes is owed once more.
      if (act === "now") void askRestart(session);
    },
    [askRestart],
  );
  /** Restart chat, from a tab's menu or the palette. */
  useEffect(() => {
    restartNow.current = (session) => void askRestart(session);
  }, [askRestart]);
  /** A task whose Restart chat is asked about in a dialog: its tab draws no breadcrumb. */
  const [restartingTask, setRestartingTask] = useState<{ session: number; name: string }>();
  const restartTab = useCallback(
    (tab: number) => {
      const session = chatOf(now.current, tab);
      if (session === undefined) return;
      // **A task's restart is asked about first** (#1489): a row in its tab's menu that ends
      // its program takes a second step, as its Stop does, and the same one (#1488): asked
      // on the task's own breadcrumb line, with Keep. Its tab comes in front first, so the
      // question is never asked where the person is not looking.
      if (askedByNow(session) !== undefined) {
        const name = now.current.byId[tab].name;
        bringToFront(tab);
        // Where its tab draws no breadcrumb for it, there is no line to ask on: a dialog.
        if (!drawsCrumbFor.current(tab, session)) {
          setTaskEndInline(undefined);
          setRestartingTask({ session, name });
          return;
        }
        setTaskEndInline({
          session,
          where: "crumb",
          ...restartAsked(name),
        });
        return;
      }
      void askRestart(session);
    },
    [askRestart, askedByNow, bringToFront, setRestartingTask],
  );
  /**
   * **Restart chat on a row of the Chats list**, for a chat with no tab (#1462). A task is
   * asked first, as its tab's row asks, on the row it was pressed on; the answer restarts it.
   * Any other chat restarts as a tab's row restarts it.
   */
  const restartListed = useCallback(
    (session: number) => {
      if (askedByNow(session) !== undefined) {
        const name =
          chatsListed.current.find((one) => one.session === session)?.name ?? `chat ${session}`;
        setTaskEndInline({ session, where: "row", ...restartAsked(name) });
        return;
      }
      void askRestart(session);
    },
    [askRestart, askedByNow],
  );
  /** Whether a chat has a Restart chat row: not a shell, which has no conversation. */
  const restartable = useCallback((session: number) => !shells.has(session), [shells]);
  /**
   * The chats open, as one word, for a read that follows it: those on the strip and those the
   * Chats list holds, so a task with no tab starting or ending moves it too (#1462), as it
   * moves the core's answer.
   */
  const chatsOpen = useMemo(() => {
    const open = new Set<number>();
    for (const tab of tabs.order) {
      const chat = chatOf(tabs, tab);
      if (chat !== undefined) open.add(chat);
    }
    for (const chat of sidebar?.unfiled ?? []) open.add(chat.session);
    for (const ws of sidebar?.workspaces ?? []) for (const chat of ws.chats) open.add(chat.session);
    return [...open].sort((a, b) => a - b).join(",");
  }, [sidebar, tabs]);
  /** Moves when one of the window's own sandbox commands returns: what those write is in the
   *  project's state folder, which the watcher of its root does not report (D-1428-10). */
  const sandboxCommands = useSandboxCommands();
  /**
   * The chats still running under an older sandbox than the project's settings decide now
   * (#1428), asked again when those settings change on disk, when the window changes them
   * itself, when its chats change, and when a chat comes to be owed a restart or stops being
   * (the core leaves out a chat that is owed one).
   */
  const olderSandbox = useOlderSandbox(
    plane,
    `${settingsChanges}:${sandboxCommands}:${chatsOpen}:${owedRestarts.join(",")}`,
  );
  const restartThem = useCallback(
    (sessions: readonly number[]) => {
      for (const session of sessions) void askRestart(session);
    },
    [askRestart],
  );

  /** The waiting chats a Retry now is under way for, by id, so a second press starts nothing. */
  const retrying = useRef(new Set<string>());
  /**
   * **Retry now** on a chat this launch could not start (NO-3), by its id: the core starts it
   * again the way the launch tried to (`retry_chat_that_did_not_start`). Started, its tab opens
   * in front and its Notice goes; refused, the Notice stays and says the new reason.
   */
  const retryChat = useCallback(
    async (id: string) => {
      if (retrying.current.has(id)) return;
      retrying.current.add(id);
      const said = await commands
        .retryChatThatDidNotStart(plane, id, STARTING_SIZE.columns, STARTING_SIZE.rows)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      retrying.current.delete(id);
      if (said.status === "error") {
        // The refusal's words now, and what the core read with it: the approval its profile
        // needs, which the window takes from the record and never from the words (#1246).
        const now = await commands
          .chatsThatWouldNotStart(plane)
          .then((read) =>
            read.status === "ok" ? read.data.find((one) => one.id === id) : undefined,
          )
          .catch(() => undefined);
        setWouldNotStart((was) =>
          was.map((one) =>
            one.id === id ? { ...one, why: said.error, approval: now?.approval ?? null } : one,
          ),
        );
        return;
      }
      const chat = said.data;
      setWouldNotStart((was) => was.filter((one) => one.id !== id));
      noteStarted(chat);
      change((tabs) =>
        openTab(tabs, chat.session, chat.name, whoOf(chat.persona, chat.harness), chat.label),
      );
    },
    [change, noteStarted, plane],
  );

  /**
   * **Forget this chat…** asked from a waiting chat's Notice: the Notice and the band it stands
   * in, found when the question opens (WebKit does not focus a pressed button, so the focus
   * cannot say where it was), and whether the answer was carried out (#1246).
   *
   * - **Forgotten:** the Notice has gone, so the keyboard goes on to the next Notice, or the strip.
   * - **Cancelled**, after a refusal too: back to the Notice's own Forget this chat….
   */
  const forgetAsked = useRef<{ notice?: Element; band: Element | null; forgot: boolean }>({
    band: null,
    forgot: false,
  });
  const askForget = useCallback((id: string, name: string) => {
    const notice = [...document.querySelectorAll("[data-cause]")].find(
      (one) => one.getAttribute("data-cause") === `chat-did-not-start:${id}`,
    );
    forgetAsked.current = {
      notice,
      band: notice?.closest(".notice-band-stack") ?? null,
      forgot: false,
    };
    setForgetting({ id, name, busy: false });
  }, []);
  const afterForgetAsk = useCallback((event: Event) => {
    event.preventDefault();
    const { notice, band, forgot } = forgetAsked.current;
    const back = [...(notice?.querySelectorAll<HTMLElement>("button") ?? [])].find(
      (button) => button.textContent === FORGET_THIS_CHAT,
    );
    if (!forgot && back?.isConnected) back.focus();
    else focusAfterNoticeGone(band, chatStrip.current);
  }, []);

  const forgetChat = useCallback(
    async (id: string, name: string) => {
      setForgetting({ id, name, busy: true });
      const said = await commands
        .forgetChatThatDidNotStart(plane, id)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") {
        setForgetting({ id, name, busy: false, trouble: said.error });
        return;
      }
      forgetAsked.current.forgot = true;
      setForgetting(undefined);
      setWouldNotStart((was) => was.filter((one) => one.id !== id));
    },
    [plane],
  );

  /**
   * **Review and approve…** on a waiting chat's Notice (#1246, D-1246-5): the question that
   * records the approval of its profile's command, with the picker's own sentence and line.
   *
   * - **Approved:** the approval is recorded and nothing starts. The Notice stays, without the
   *   offer, and the keyboard goes to its Retry now, which runs the whole start again.
   * - **Cancelled**, after a refusal too: nothing is recorded, and the keyboard goes back to
   *   Review and approve….
   */
  const approveAsked = useRef<{ id?: string; approved: boolean }>({ approved: false });
  const askApprove = useCallback((id: string, name: string, approval: NeedsApproval) => {
    approveAsked.current = { id, approved: false };
    setApproving({ id, name, approval, busy: false });
  }, []);
  const afterApproveAsk = useCallback((event: Event) => {
    event.preventDefault();
    const { id, approved } = approveAsked.current;
    const notice = [...document.querySelectorAll("[data-cause]")].find(
      (one) => one.getAttribute("data-cause") === `chat-did-not-start:${id}`,
    );
    const to = approved ? "Retry now" : REVIEW_AND_APPROVE;
    [...(notice?.querySelectorAll<HTMLElement>("button") ?? [])]
      .find((button) => button.textContent === to)
      ?.focus();
  }, []);
  const approveWaiting = useCallback(
    async (asked: { id: string; name: string; approval: NeedsApproval }) => {
      setApproving({ ...asked, busy: true });
      // The line on screen, which the core checks against the file again: a profile changed
      // since the question opened is refused there, and nothing is recorded.
      const said = await commands
        .approveProfile(plane, asked.approval.profile, asked.approval.shown)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") {
        setApproving({ ...asked, busy: false, trouble: said.error });
        return;
      }
      approveAsked.current.approved = true;
      setApproving(undefined);
      setWouldNotStart((was) =>
        was.map((one) => (one.id === asked.id ? { ...one, approval: null } : one)),
      );
      // A task drawn under the chat that asked (#1497): the core reads what its profile needs
      // again when it is next tried, so its row is read again and offers the try.
      rereadFinished();
    },
    [plane, rereadFinished],
  );
  /**
   * **What a task a launch could not start again offers on its row** (#1497): the same try and
   * the same approval the window's own "did not start" line has, and End task, which is the
   * one thing that ends it. The rows are the core's, so each is read again afterwards.
   */
  const waitingTaskWays = useMemo<WaitingTaskWays>(
    () => ({
      retry: async (task) => {
        await retryChat(task.id);
        rereadFinished();
      },
      approve: (task, approval) => askApprove(task.id, task.name, approval),
      end: async (task) => {
        const said = await commands
          .endTaskThatDidNotStart(plane, task.id)
          .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
        rereadFinished();
        return said.status === "error" ? said.error : undefined;
      },
    }),
    [askApprove, plane, rereadFinished, retryChat],
  );

  const askStartFresh = useCallback(
    (tab: number) => {
      const session = chatOf(now.current, tab);
      if (session === undefined) return;
      setFreshening({
        tab,
        session,
        name: now.current.byId[tab].name,
        files: planeUpdates[session] ?? [],
        // Asked now, as a close asks `smart_close_offer`: the program ends either way, and
        // the question says so when that interrupts a turn (#1246).
        // `stateOf`, as the quit warning reads it: a shell nothing has reported on is `unknown`
        // too, since a harness started by hand in it could be mid-turn.
        doing: stateOf(chats.store.statesFor(chats.plane), session),
        busy: false,
      });
    },
    [chats.plane, chats.store, planeUpdates],
  );
  /**
   * The core starts the same chat again, fresh (`start_chat_fresh`), and ends the old one itself
   * once the new one runs. **Only the pane that showed it changes** (D-NO3-8): the new session
   * takes that pane, so the tab keeps its place and a chat split beside it runs on, untouched.
   * Refused, nothing changes and the question says why.
   */
  const startFresh = useCallback(async () => {
    const asked = freshening;
    if (asked === undefined) return;
    setFreshening({ ...asked, busy: true, trouble: undefined });
    const said = await commands
      .startChatFresh(plane, asked.session, STARTING_SIZE.columns, STARTING_SIZE.rows)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (said.status === "error") {
      setFreshening({ ...asked, busy: false, trouble: said.error });
      return;
    }
    const chat = said.data;
    setFreshening(undefined);
    setReopened((was) => was.filter((one) => one.session !== asked.session));
    setPinnedChats((was) => was.filter((one) => one !== asked.session));
    noteStarted(chat);
    followRestart(asked.session, chat.session);
    change((tabs) => replaceSession(tabs, asked.session, chat.session));
  }, [change, followRestart, freshening, noteStarted, plane]);

  // A resumed chat whose harness ended without a word — it could not find the conversation — is
  // replaced by a fresh chat with the same record, once (SI-8d). Everything else a resumed chat
  // does only marks it heard, and a heard chat is the operator's from then on.
  //
  // Watched off the store rather than read in a render, so it is not a reason to draw this view.
  useEffect(() => {
    const watch = () => {
      const states = chats.store.statesFor(chats.plane);
      for (const [session, watched] of [...resuming.current]) {
        const state = stateOf(states, session);
        if (lostOnResume(watched, state)) {
          resuming.current.delete(session);
          const tab = now.current.order.find((id) =>
            panesOf(now.current, id).some((pane) => pane.session === session),
          );
          if (tab !== undefined) change((tabs) => closeTab(tabs, tab, filedIn, isPinned));
          void commands.closeSession(plane, session);
          setReopened((was) => was.filter((one) => one.session !== session));
          void resumeSession(watched.path, session);
        } else if (state === "done" || state === "failed") {
          resuming.current.delete(session);
        } else {
          resuming.current.set(session, heardFrom(watched, state));
        }
      }
    };
    watch();
    return chats.store.subscribe(watch);
  }, [change, chats, filedIn, isPinned, plane, resumeSession]);

  /**
   * A chat's work link (V60, ADR 0088 §3). **What each chat works on is asked once per chat**,
   * as it appears, and then follows what this window links and unlinks: the link is the
   * project's, and the core's answer is what the tab and the pane draw.
   */
  const askedWorkItem = useRef(new Set<number>());
  useEffect(() => {
    for (const id of tabs.order) {
      const session = chatOf(tabs, id);
      if (session === undefined || askedWorkItem.current.has(session)) continue;
      askedWorkItem.current.add(session);
      void commands
        .chatWorkItem(plane, session)
        .then((said) => {
          const item = said.status === "ok" ? said.data : null;
          if (item !== null) setWorkItems((was) => ({ ...was, [session]: item }));
        })
        .catch(() => undefined);
    }
  }, [plane, tabs]);

  /** Opens the dialog that asks which work item a chat tab's chat works on. */
  const linkWorkItem = useCallback((id: number) => {
    if (chatOf(now.current, id) === undefined) return;
    setLinkingWork({ tab: id, busy: false });
  }, []);

  /** Links the dialog's chat to `key`, through `chat_work_link`; a refusal stays in the dialog. */
  const saveWorkLink = useCallback(
    async (id: number, key: string) => {
      const session = chatOf(now.current, id);
      if (session === undefined) return;
      setLinkingWork({ tab: id, busy: true });
      const said = await commands
        .chatWorkLink(plane, session, key)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") {
        setLinkingWork({ tab: id, busy: false, trouble: said.error });
        return;
      }
      setWorkItems((was) => ({ ...was, [session]: said.data }));
      setLinkingWork(undefined);
    },
    [plane],
  );

  /** The task whose brief is being read, while its panel is open (#1494). */
  const [briefOf, setBriefOf] = useState<BriefAsk>();
  /** Opens the Brief panel for a task: the one way in, for the catalogue's rows, a finished
   *  row and the breadcrumb's button ({@link OpenBrief}). It reads when it opens. */
  const openBrief: OpenBrief = setBriefOf;
  /** The task whose question is being answered, while its dialog is open (#1551). */
  const [answerOf, setAnswerOf] = useState<AnswerAsk>();
  /** Opens the Answer dialog for a task: for the catalogue's rows and a tab menu's line. */
  const openAnswer: OpenAnswer = setAnswerOf;
  /**
   * Opens **Ask {persona}** for chat `session`: the one way in, for the catalogue's rows and
   * for a Notice that names the persona to ask ({@link OpenAskPersona}). Nothing starts here.
   */
  const openAskPersona = useCallback<OpenAskPersona>((session, persona, prefill, from) => {
    setAskingPersona({ session, persona, prefill, from, busy: false });
  }, []);
  /** Ask {persona}…, from a chat tab's menu or the palette. */
  const askPersona = useCallback(
    (id: number, persona: string) => {
      const session = chatOf(now.current, id);
      if (session !== undefined) openAskPersona(session, persona);
    },
    [openAskPersona],
  );
  /** Who already works where the dialog's task would, by the core's sentence (#1534). */
  const askingFrom = askingPersona?.session;
  const folderShared = useCallback(
    async (place: string | null): Promise<string | undefined> => {
      if (askingFrom === undefined) return undefined;
      const said = await commands.taskFolderShared(plane, askingFrom, place);
      return said.status === "ok" ? (said.data ?? undefined) : undefined;
    },
    [askingFrom, plane],
  );
  /**
   * The dialog's answer: the core starts a chat as that persona under the asking chat
   * (`ask_persona_chat`) and tells this window of it as it tells of any chat another chat
   * started, so its tab arrives behind the one being read. A refusal stays in the dialog.
   */
  const sendAsk = useCallback(
    async (name: string, ask: string, place: string | null) => {
      const asked = askingPersona;
      if (asked === undefined || asked.busy) return;
      setAskingPersona({ ...asked, trouble: undefined, busy: true });
      const said = await commands
        .askPersonaChat(
          plane,
          asked.session,
          asked.persona,
          name,
          ask,
          place,
          STARTING_SIZE.columns,
          STARTING_SIZE.rows,
        )
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") {
        // What was typed stays in the boxes: the dialog is still mounted, and holds it.
        setAskingPersona({ ...asked, trouble: said.error, busy: false });
        return;
      }
      setAskingPersona(undefined);
    },
    [askingPersona, plane],
  );

  /** Ends a chat tab's chat's work link, through `chat_work_unlink`. */
  const unlinkWorkItem = useCallback(
    async (id: number): Promise<Ran> => {
      const session = chatOf(now.current, id);
      if (session === undefined) return { ok: false, refused: "That tab has no chat to unlink." };
      const said = await commands
        .chatWorkUnlink(plane, session)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") return { ok: false, refused: said.error };
      setWorkItems((was) =>
        Object.fromEntries(Object.entries(was).filter(([one]) => Number(one) !== session)),
      );
      return { ok: true };
    },
    [plane],
  );

  /**
   * **A repo the workspace names, cloned from the window** (#1215): the explorer's row, its
   * Clone all, the bottom bar's menu and the palette all end here. The same `cloneRepos` a new
   * workspace's repos and Settings › Repos go through, so there is one clone path; each repo's
   * progress and failure is drawn on its own row from that store, and the panels are read
   * again so a repo that landed moves among the clones.
   */
  const cloneMissing = useCallback(
    async (workspace: string, repos: string[]): Promise<Ran> => {
      // Read again as each repo lands, so a repo that is cloned leaves "Not cloned here" while
      // the rest of a Clone all is still running. A clone the workspace does not then see is
      // a failure with a Retry — `cloneRepos` checks that for every caller (V91b).
      const failed = await cloneRepos(plane, workspace, repos, rereadPanels);
      return failed.length === 0
        ? { ok: true, said: `Cloned ${repos.join(", ")} into ${workspace}.` }
        : {
            ok: false,
            refused:
              `Could not clone ${failed.map((f) => f.repo).join(", ")} into ${workspace} — ` +
              failed.map((f) => f.said).join(" "),
          };
    },
    [plane, rereadPanels],
  );

  const doing = useMemo<Doing>(
    () => ({
      newChat: newTab,
      newShell,
      runAction,
      split,
      closePane,
      closeTab: close,
      selectTab: bringToFront,
      renameTab: beginRename,
      linkWorkItem,
      unlinkWorkItem,
      startFresh: askStartFresh,
      restartChat: restartTab,
      restartListed,
      askPersona,
      pinTab,
      pinWorkspace,
      pinProject: windowDoes.pinProject,
      focusWorkspace,
      createWorkspace,
      removeWorkspace,
      // The verb is only ever a needs-you item's Go, or the palette's row for one.
      showChat: showNeeding,
      ownTab,
      beside: besideOf,
      sendBack: sendBackOf,
      showTabTasks,
      showBrief: (session: number) =>
        openBrief({ chat: session, name: nameOfNow.current(session) }),
      answerQuestion: (session: number) =>
        openAnswer({ chat: session, name: nameOfNow.current(session) }),
      ignoreNeedsYou,
      cancelSmartClose,
      dismissStopped: (session: number) => stoppedFor(session, undefined),
      stopChat: askToStop,
      stopAllTasks: (session: number) => void askToStopAll(session),
      endTask: (session: number, way: TaskEndWay) => void askToEndTask(session, way, "elsewhere"),
      // The verb still names a persona — that is what the catalogue row is about — and the
      // window turns it into the row it opens. `charter/personas` is charter's own panel's
      // key (`purlis_core::panel::Panel::key`), and it is written here because the catalogue
      // is not a reader of the panel list.
      openView: showView,
      resumeSession: (path: string) => resumeSession(path),
      pickVault,
      createVault,
      ...edits.doing,
      ...memoryEdits.doing,
      removeWorktree,
      mergeWorktree,
      declareWorktreeDone,
      // A clone picked from its menu is the explorer's own pick one level up: the same state,
      // so the explorer marks it and `New tab` starts there (charter-app#174).
      pickClone: (repo, path) => pickSpot({ repo, path }),
      // The explorer becomes that branch's cockpit (FM-5).
      focusBranch: (cut) =>
        setFocusedBranch({ workspace: cut.workspace, repo: cut.repo, piece: cut.piece }),
      newBranch,
      cloneMissing,
      askDropMembership: (workspace: string, repo: string) => setMembershipAsk({ workspace, repo }),
      newChatIn: newTabIn,
      sendKey,
      openProject: windowDoes.openProject,
      createProject: windowDoes.createProject,
      showExtensions: windowDoes.showExtensions,
      installCli: windowDoes.installCli,
      selectProject: windowDoes.selectProject,
      switchProject: windowDoes.switchProject,
      closeProject: windowDoes.closeProject,
      moveProject: windowDoes.moveProject,
      openSettings: windowDoes.openSettings,
      openSaving: windowDoes.openSaving,
      openWorkspaceSettings,
      switchLive: (workspace: string) => setLiveAsk(workspace),
      renameWorkspace,
      openSettingsTab: windowDoes.openSettingsTab,
      openYourSettings: windowDoes.openYourSettings,
      curate,
      quit: windowDoes.quit,
      ...fileDoing,
      shellInFolder,
      startChatHere,
    }),
    [
      beginRename,
      restartTab,
      restartListed,
      linkWorkItem,
      unlinkWorkItem,
      askStartFresh,
      askPersona,
      askToStop,
      askToStopAll,
      askToEndTask,
      bringToFront,
      cancelSmartClose,
      close,
      closePane,
      curate,
      createVault,
      createWorkspace,
      edits.doing,
      memoryEdits.doing,
      fileDoing,
      shellInFolder,
      startChatHere,
      focusWorkspace,
      ignoreNeedsYou,
      mergeWorktree,
      newBranch,
      cloneMissing,
      declareWorktreeDone,
      newTab,
      newShell,
      pickVault,
      newTabIn,
      openAnswer,
      openBrief,
      openWorkspaceSettings,
      pickSpot,
      pinTab,
      pinWorkspace,
      removeWorkspace,
      removeWorktree,
      renameWorkspace,
      resumeSession,
      runAction,
      sendKey,
      showNeeding,
      ownTab,
      besideOf,
      sendBackOf,
      showTabTasks,
      showView,
      split,
      stoppedFor,
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
  const quiet = useChatsSelect(
    chats,
    (states) =>
      sidebar
        ? quietOnes(
            [...sidebar.workspaces.flatMap((ws) => ws.chats), ...sidebar.unfiled],
            states,
            // Named as its row names it (`shownName`, #1484): a task with no tab is never
            // its number here either.
            (chat) => shownName(tabs, chat),
          )
        : [],
    sameList,
  );

  /** Each open chat's harness card, at a glance, by session (HP-19): what its header draws. The
   *  core's list of open chats carries it, so a chat put back at a launch has it from its
   *  first frame. */
  const glances = useMemo(() => {
    const out: Record<number, HarnessGlance> = {};
    const open = sidebar
      ? [...reopened, ...sidebar.workspaces.flatMap((ws) => ws.chats), ...sidebar.unfiled]
      : reopened;
    for (const chat of open) if (chat.card) out[chat.session] = chat.card;
    return out;
  }, [sidebar, reopened]);

  /** The chats working in the focused workspace, which is what the explorer files under the
   *  spots they are working at. The plane's own answer, like everything else about where a
   *  chat is: nothing on the plane records a chat, so the directory it works in is it. */
  const workspaceChats = useMemo(
    () =>
      (sidebar?.workspaces.find((ws) => ws.name === ofWorkspace)?.chats ?? []).map((chat) => ({
        // Named as the Chats list names it (`shownName`, #1484), never by the harness's own
        // name, which is a number (charter-app#254): a task with no tab too.
        ...chat,
        name: shownName(tabs, chat),
      })),
    [ofWorkspace, sidebar, tabs],
  );

  /**
   * Every running chat of the project, as the Chats section lists it (#1447): the core's own
   * list, each named as its tab is, or as its tab would be for a task chat that has none yet.
   * Whether it has a tab is this window's answer, since the tabs are this window's.
   */
  const listedChats = useMemo(() => {
    if (sidebar === undefined) return [];
    const one = (chat: OpenChat, workspace: string) =>
      listedChat(
        chat,
        workspace,
        shownName(tabs, chat),
        tabHolding(tabs, chat.session) !== undefined,
        nameOfListed,
      );
    // By number, which is the order they were started in: the list arrives workspace by
    // workspace, and a chat's children are read in the order it asked for them.
    return [
      ...sidebar.workspaces.flatMap((ws) => ws.chats.map((chat) => one(chat, ws.name))),
      ...sidebar.unfiled.map((chat) => one(chat, ROOT_WORD)),
      // Tasks a scenario spec pretends a session has (`e2eTasks.ts`): none in a shipped build.
      ...pretended.map(({ chat, workspace }) => one(chat, workspace)),
    ].sort((a, b) => a.session - b.session);
  }, [nameOfListed, pretended, sidebar, tabs]);
  /** The chats of each tab, as its chip counts them and its menu lists them (#1487). */
  const chatsByTab = useMemo(() => chatsOfTabs(tabs, listedChats), [listedChats, tabs]);
  /** Each tab menu's footer: how many of its session's tasks run against its limit (#1498). */
  const runningOfTab = useMemo(() => runningByTab(chatsByTab), [chatsByTab]);
  useEffect(() => {
    chatsListed.current = listedChats;
  }, [listedChats]);
  const chatRows = useMemo(() => chatsTree(listedChats), [listedChats]);
  /** The Chats list's rows: a task that ended keeps its row until its finished row is read,
   *  so the rows below it move once and not up and then down (`useRowsUntilRead`). */
  const listRows = useRowsUntilRead(chatRows, finishedSettled);
  /**
   * **How long each chat has been in its state, as this window saw it** (`stateClock.ts`): one
   * clock for the project, held here so it runs whether or not the Chats list is drawn, and
   * read by that list's rows and by a tab's menu alike (#1487). The list says which rows are
   * on screen, which is what the clock calls a change somebody saw.
   */
  const chatClock = useMemo(() => {
    void plane;
    return stateClock();
  }, [plane]);
  const rowsOnScreen = useRef<ReadonlySet<number>>(NO_ROWS_DRAWN);
  const rowsDrawn = useCallback((drawn: ReadonlySet<number>) => {
    rowsOnScreen.current = drawn;
  }, []);
  const { store: chatStore, plane: chatsOf } = chats;
  useEffect(() => {
    const read = () =>
      chatClock.read(chatStore.statesFor(chatsOf), chatRows, Date.now(), rowsOnScreen.current);
    read();
    return chatStore.subscribe(read);
  }, [chatClock, chatRows, chatStore, chatsOf]);
  /**
   * **Each session's tasks** (#1491): the open tasks below it and the finished rows under it,
   * from the two lists the Chats section draws. Lent to everything this view draws
   * (`TasksBelowLent`), so a session's row, its state's word and its tab count the same rows.
   */
  const tasksBelow = useMemo(() => {
    const limits = new Map(listedChats.map((chat) => [chat.session, chat.tasksLimit ?? null]));
    return tasksBelowOf(listedChats, finishedTasks, (session) => limits.get(session) ?? null);
  }, [finishedTasks, listedChats]);
  /**
   * **The breadcrumb of every pane that shows a task and can be drawn**, by tab, pane and
   * chat (#1486): the path read off the core's list, for the panes `shownLive` says are live.
   */
  const liveCrumbs = useMemo(() => {
    const live = new Map<string, Crumbs>();
    if (sidebar === undefined) return live;
    const open = (session: number) => chatsByNumber.has(session);
    for (const id of tabs.order) {
      if (tabs.byId[id].shows === undefined) continue;
      const crumbs = crumbsOf(tabs, id, listedChats);
      for (const one of shownLive(tabs, id, askedBy, open)) {
        const said = crumbs[one.pane];
        if (one.session !== one.own && one.live && said !== undefined)
          live.set(`${id}:${one.pane}:${one.session}`, said);
      }
    }
    return live;
  }, [askedBy, chatsByNumber, listedChats, sidebar, tabs]);
  // What each pane shows of a task is remembered while it is live, and forgotten with the
  // pane's showing of it: a pane that left a task remembers nothing of it. Adjusted while
  // rendering, as React has state follow what it is drawn from.
  const recalled = useMemo(
    () => recalledCrumbs(remembered, liveCrumbs, tabs),
    [liveCrumbs, remembered, tabs],
  );
  if (recalled !== remembered) setRemembered(recalled);
  // **A task the record put back that is not there to be shown**, and that no pane ever drew:
  // its tab shows the session's own chat (#1486). This is the one place a pane goes back by
  // itself, and nobody was looking at it: a launch, before the first frame of that task.
  useEffect(() => {
    if (sidebar === undefined) return;
    const open = (session: number) => chatsByNumber.has(session);
    for (const id of now.current.order) {
      for (const one of shownLive(now.current, id, askedBy, open)) {
        if (one.live || recalled.has(`${id}:${one.pane}:${one.session}`)) continue;
        if (liveCrumbs.has(`${id}:${one.pane}:${one.session}`)) continue;
        change((tabs) => showOwnIn(tabs, id, one.pane));
      }
    }
    // `tabs` is read through `now`, and is here so a change to it is judged.
  }, [askedBy, change, chatsByNumber, liveCrumbs, recalled, sidebar, tabs]);
  /**
   * **What each pane of the tab in front draws in place of a terminal, and its breadcrumb**
   * (#1486): a live task has its path off the list; one that cannot be drawn has the path it
   * last had, and says why it is not drawn (`TaskAway`).
   */
  const { frontCrumbs, frontAway } = useMemo(() => {
    const crumbs: Record<number, Crumbs> = {};
    const away: Record<number, Away> = {};
    const id = tabs.inFront;
    if (id === undefined) return { frontCrumbs: crumbs, frontAway: away };
    for (const one of frontShown) {
      if (one.session === one.own) continue;
      const key = `${id}:${one.pane}:${one.session}`;
      const live = one.live ? liveCrumbs.get(key) : undefined;
      if (live !== undefined) {
        crumbs[one.pane] = live;
        continue;
      }
      const last = recalled.get(key);
      away[one.pane] =
        sidebar === undefined || last === undefined
          ? { why: "unread" }
          : { why: chatsByNumber.has(one.session) ? "moved" : "ended", crumbs: last };
    }
    return { frontCrumbs: crumbs, frontAway: away };
  }, [chatsByNumber, frontShown, liveCrumbs, recalled, sidebar, tabs.inFront]);
  /**
   * **The finished row of each task a pane was left on when it ended**, by pane (#1485 under
   * #1486's ended view): its report is drawn on the pane, and the pane says how it ended in
   * the row's words. Found by the number the task's chat had, and never by its name: a pane
   * with no row of its own draws no report.
   */
  const frontFinished = useMemo(() => {
    const rows: Record<number, FinishedTask> = {};
    for (const [pane, gone] of Object.entries(frontAway)) {
      if (gone.why !== "ended") continue;
      const [asker, task] = gone.crumbs.path.slice(-2);
      const row =
        task === undefined ? undefined : finishedOf(finishedTasks, asker.session, task.session);
      if (row !== undefined) rows[Number(pane)] = row;
    }
    return rows;
  }, [finishedTasks, frontAway]);
  /**
   * **The breadcrumb of each pane of the tab in front that shows its own chat and has a path
   * to say** (#1489): a task in a pane of its own, and the session it is beside.
   */
  const frontPlaced = useMemo(
    () => (tabs.inFront === undefined ? {} : placedCrumbsOf(tabs, tabs.inFront, listedChats)),
    [listedChats, tabs],
  );
  useEffect(() => {
    drawsCrumbFor.current = (tab, session) => {
      const endsInIt = (crumbs: Crumbs) => {
        const last = crumbs.path[crumbs.path.length - 1];
        return last.session === session && last.mode === "task";
      };
      const at = now.current;
      return (
        Object.values(crumbsOf(at, tab, listedChats)).some(endsInIt) ||
        Object.values(placedCrumbsOf(at, tab, listedChats)).some(endsInIt)
      );
    };
  }, [listedChats]);
  // A pane that could not draw its task and can again (a restart caught up) gives the
  // keyboard back to the task's terminal, where the pane has the keyboard.
  const wasAway = useRef<ReadonlySet<number>>(new Set());
  useEffect(() => {
    const front = tabs.inFront === undefined ? undefined : tabs.byId[tabs.inFront];
    for (const one of frontShown)
      if (
        wasAway.current.has(one.pane) &&
        frontAway[one.pane] === undefined &&
        one.session !== one.own &&
        front?.focused === one.pane
      )
        giveKeyboardTo(plane, one.session);
    wasAway.current = new Set(Object.keys(frontAway).map(Number));
  }, [frontAway, frontShown, plane, tabs]);

  /**
   * **The chats that live in each pane of the tab in front, other than the one it shows**
   * (#1486, the train-1 part of V100-56), by pane: the session's own chat while a task is
   * shown, and every task while it is not. Each says what it has to say on this pane, with
   * whose it is first, since nothing that waits for the person may be off screen.
   */
  const frontOthers = useMemo(() => {
    const others: Record<number, { session: number; whose: string; task: boolean }[]> = {};
    const id = tabs.inFront;
    if (id === undefined) return others;
    for (const one of frontShown) {
      const [own, ...tasks] = chatsOfPane(tabs, id, one.pane, listedChats);
      if (own === undefined) continue;
      others[one.pane] = [own, ...tasks]
        .filter((chat) => chat.session !== one.session || !one.live)
        .map((chat) => ({
          session: chat.session,
          // The whole path, from the core's record of who asked whom (#1508, V100-56).
          whose: whoseOf(chat, own, listedChats),
          task: chat.session !== own.session,
        }));
    }
    return others;
  }, [frontShown, listedChats, tabs]);

  /**
   * **The questions each pane of the tab in front asks for several of its tasks at once**
   * (#1508, V100-57), by pane: tasks of the pane's session blocked on the same host or folder,
   * asked about in one Notice. Their blocks are taken from the tasks' own Notices
   * (`frontBlocks`), so each is asked once.
   */
  const frontGroups = useMemo(() => {
    const groups: Record<number, TaskBlockGroup[]> = {};
    const id = tabs.inFront;
    if (id === undefined) return groups;
    for (const one of frontShown) {
      const [own, ...tasks] = chatsOfPane(tabs, id, one.pane, listedChats);
      if (own === undefined) continue;
      groups[one.pane] = taskBlockGroups(
        sandboxBlocks,
        own.session,
        tasks.map((chat) => ({ session: chat.session, whose: whoseOf(chat, own, listedChats) })),
      );
    }
    return groups;
  }, [frontShown, listedChats, sandboxBlocks, tabs]);
  const frontBlocks = useMemo(
    () => withoutGrouped(sandboxBlocks, Object.values(frontGroups).flat()),
    [frontGroups, sandboxBlocks],
  );
  /** What each pane of the tab in front asks for its tasks together, and what was answered. */
  const frontAsked = useMemo(() => {
    const asked: Record<number, ReactNode> = {};
    for (const one of frontShown) {
      const groups = frontGroups[one.pane] ?? [];
      const answered = taskBlocksAnswered.filter((said) => said.session === one.own);
      if (groups.length === 0 && answered.length === 0) continue;
      asked[one.pane] = (
        <>
          {groups.map((group) => (
            <TaskBlocksNotice
              key={group.key}
              plane={plane}
              group={group}
              onAnswered={(members, said) => {
                for (const member of members) {
                  blockAnswered(member.session, member.block);
                  oweRestart(member.session);
                }
                setTaskBlocksAnswered((was) => [
                  ...was,
                  { at: Date.now(), session: group.session, target: group.target, said },
                ]);
              }}
              onKeepBlocked={(members) => {
                for (const member of members) blockAnswered(member.session, member.block);
              }}
            />
          ))}
          {answered.map((said) => (
            <TaskBlocksAnswered
              key={said.at}
              target={said.target}
              said={said.said}
              onDismiss={() =>
                setTaskBlocksAnswered((was) => was.filter((one) => one.at !== said.at))
              }
            />
          ))}
        </>
      );
    }
    return asked;
  }, [blockAnswered, frontGroups, frontShown, oweRestart, plane, taskBlocksAnswered]);

  /** Every chat that lives in a tab of this window: a session's own chat, and each task
   *  below one. What a tab can be wearing the hand for. */
  const living = useMemo(() => {
    const byTask = askedByOf(listedChats);
    return listedChats
      .filter((chat) => homeOf(tabs, chat.session, byTask) !== undefined)
      .map((chat) => chat.session);
  }, [listedChats, tabs]);
  const heldFor = useHeldAmong(plane, living);
  /**
   * **This project's permission prompts held on their chats' hooks** (HP-6): a chat whose
   * harness asked the person's permission waits on them from the moment it asks, before its
   * harness says anything else, so it wears the hand at once (reported 2026-10-09: a task
   * stopped on one in a tab nobody was at, and nothing said so).
   */
  const planeOnly = useMemo(() => [plane], [plane]);
  const asksHere = usePermissionAsks(planeOnly)[plane] ?? NO_ASKS;
  const refusedFor = useRefusedAmong(plane, living);
  /**
   * **The chats waiting for the person beside the queue**: every chat with a Notice that waits
   * for an answer (#1486), a dispatch held for a grant, a vault it was refused, a sandbox block,
   * a restart that is owed or did not happen. A held dispatch is not in the core's queue, so
   * without this a chat stuck on one would say nothing anywhere. The queue goes first, where it
   * is read (`hiddenIn`).
   */
  const othersWaiting = useMemo(() => {
    const restarts = Object.entries(restartsSaid)
      .filter(([, said]) => said.trouble !== undefined || said.notYet !== undefined || said.byHand)
      .map(([session]) => Number(session));
    const blocked = Object.entries(sandboxBlocks)
      .filter(([, blocks]) => blocks.length > 0)
      .map(([session]) => Number(session));
    const prompted = asksHere.map((ask) => ask.session);
    return [...prompted, ...heldFor, ...refusedFor, ...blocked, ...restarts];
  }, [asksHere, heldFor, refusedFor, restartsSaid, sandboxBlocks]);
  /**
   * **Tab `id`'s chats that are waiting for the person and are not on screen**, by name, the
   * longest waiting first (#1486, V100-37): what the tab wears the hand for. A chat is on
   * screen only in the tab in front. Nothing switches a tab to one of them; going to it does.
   * Asked with the queue where the chip is drawn (`QueueRead`, #1034).
   */
  const hiddenIn = useCallback(
    (queue: readonly number[], id: number): readonly Needing[] => {
      if (!(id in tabs.byId)) return NO_NEEDS;
      const waitingForYou = [...new Set([...queue, ...othersWaiting])];
      const hidden = hiddenNeeding(tabs, id, askedBy, waitingForYou).flatMap(
        (session): Needing[] => {
          const chat = chatsByNumber.get(session);
          return chat === undefined ? [] : [{ session, name: shownName(tabs, chat) }];
        },
      );
      return hidden.length === 0 ? NO_NEEDS : hidden;
    },
    [askedBy, chatsByNumber, othersWaiting, tabs],
  );
  /**
   * **Each tab's tasks that have ended and still have a line** (#1487): they are not in the
   * list of open chats, so they are handed to the tab's chip beside its rows.
   *
   * - **The finished rows of every chat of the tab**, at every level, since a task's own tasks
   *   finish under it (`finished.ts`, #1485): each as its row in the Chats list says it, and
   *   folding as the core says it folds, and counted by how the core says it ended.
   * - **The task the tab still shows**, which stays on screen until the person goes back
   *   (`shownLive`, `TaskAway`): the same line as its finished row where it has one, found as
   *   the pane finds it (`finishedOf`), and a line of its own until that row is read.
   */
  const endedByTab = useMemo(() => {
    const ended = new Map<number, Ended[]>();
    if (sidebar === undefined) return ended;
    const open = (session: number) => chatsByNumber.has(session);
    for (const id of tabs.order) {
      const lines: Ended[] = [];
      let home: string | undefined;
      for (const row of chatsByTab.get(id) ?? NO_ROWS) {
        if (row.level === 1) home = row.workspace;
        // What finished under a task in a tab of its own is that tab's to list (#1489).
        if (row.placed === "tab") continue;
        for (const task of finishedTasks.get(row.session) ?? []) {
          const shown = shownOf(task);
          if (shown === undefined) continue;
          // The core's word for the root and this window's, as one.
          const place = task.place === CORE_ROOT_WORD ? ROOT_WORD : task.place;
          lines.push({
            key: `finished:${task.id}`,
            asker: task.asker,
            name: task.name,
            persona: task.persona,
            shown,
            qualifier: qualifierOf(task),
            folds: task.folds,
            bucket: finishedBucketOf(task.how),
            elsewhere: place === home ? null : place,
            report: task.report,
          });
        }
      }
      if (tabs.byId[id].shows !== undefined)
        for (const one of shownLive(tabs, id, askedBy, open)) {
          if (one.live || one.session === one.own || open(one.session)) continue;
          const last = recalled.get(`${id}:${one.pane}:${one.session}`);
          if (last === undefined) continue;
          const task = last.path[last.path.length - 1];
          const asker = last.path[last.path.length - 2]?.session ?? one.own;
          const row = finishedOf(finishedTasks, asker, one.session);
          const at =
            row === undefined ? -1 : lines.findIndex((line) => line.key === `finished:${row.id}`);
          if (at >= 0) {
            lines[at] = { ...lines[at], session: one.session };
            continue;
          }
          const shown = endedState(last);
          if (shown === undefined) continue;
          lines.push({
            key: `ended:${one.session}`,
            session: one.session,
            asker,
            name: task.name,
            persona: task.persona,
            shown,
            folds: shown.kind === "done" || shown.kind === "cancelled",
            bucket: taskBucketOf(shown.kind),
            elsewhere: last.elsewhere,
          });
        }
      if (lines.length > 0) ended.set(id, lines);
    }
    return ended;
  }, [askedBy, chatsByNumber, chatsByTab, finishedTasks, recalled, sidebar, tabs]);
  /** Whether tab `id` has tasks its chip counts: what Down on the tab opens the menu of, and
   *  what the tab says it has. The chip's own answer (`tabTasks.hasTasks`). */
  const tabHasTasks = (id: number) =>
    hasTasks(chatsByTab.get(id) ?? NO_ROWS, endedByTab.get(id) ?? NO_ENDED);
  /** **The tab in front, if its session has tasks to explain the chip by** (#1501): what the
   *  session's pane draws the first dispatch's Notice from (`ChipExplained`). Not a task's tab,
   *  since the session is what dispatched. */
  const frontTabId = frontTab?.id;
  const frontOwn = frontTabId === undefined ? undefined : chatOf(tabs, frontTabId);
  const frontRows = frontTabId === undefined ? undefined : chatsByTab.get(frontTabId);
  const frontEnded = frontTabId === undefined ? undefined : endedByTab.get(frontTabId);
  const frontIsTask = frontOwn !== undefined && chatsByNumber.get(frontOwn)?.from?.task === true;
  const chipToExplain = useMemo((): ChipToExplain | undefined => {
    if (frontTabId === undefined || frontOwn === undefined || frontIsTask) return undefined;
    const tasks =
      (frontRows ?? NO_ROWS).filter((row) => row.level > 1).length + (frontEnded?.length ?? 0);
    return tasks === 0
      ? undefined
      : { session: frontOwn, tasks, onShow: () => showTabTasks(frontTabId) };
  }, [frontEnded, frontIsTask, frontOwn, frontRows, frontTabId, showTabTasks]);
  /** Goes to a chat from a tab's chip or its menu (#1487): the tab is switched to it, and its
   *  terminal takes the keyboard whether or not the tab was already on it. */
  //
  // **The same function for the window's life**: `showChat` is made again whenever the list of
  // chats is read, and a chip handed a new function is a chip drawn again, on every tab, for
  // one task's move (SC-3). So the chips hold this, and this reads the one that is current.
  const showChatNow = useRef(showChat);
  useEffect(() => {
    showChatNow.current = showChat;
  }, [showChat]);
  const showFromChip = useCallback(
    (session: number) => {
      showChatNow.current(session);
      giveKeyboardTo(plane, session);
    },
    [plane],
  );

  /** The task tab `id` shows in place of its session's own chat, by name (#1486): what its
   *  label says after the session's name. Nothing while it shows its own chat. */
  const taskNameOf = (id: number) => {
    const task = taskShownIn(tabs, id);
    if (task === undefined) return undefined;
    const chat = chatsByNumber.get(task);
    if (chat !== undefined) return shownName(tabs, chat);
    // One that has ended is still what the tab shows, by the name it had.
    const lead = contentsOf(tabs, id)[0]?.pane;
    const last = recalled.get(`${id}:${lead}:${task}`);
    return last?.path[last.path.length - 1].name;
  };
  /** The explorer's lines ask the Chats list for chats' own rows (#1490): the one line for a
   *  session's tasks for the session's, and a line for the tasks working at a place for each
   *  of theirs. The list opens them and puts the keyboard on the first (`revealTask.ts`). */
  const revealChats = useCallback(
    (sessions: readonly number[]) =>
      setRevealed((was) =>
        sessions.length === 0
          ? was
          : { asker: sessions[0], also: sessions.slice(1), at: (was?.at ?? 0) + 1 },
      ),
    [],
  );
  /**
   * **Whether tab `id` is a task's own** (#1489), and then the name of the chat that asked for
   * it, where that chat is still listed: what the tab says after its own name. Nothing for a
   * session's tab and for a view's.
   */
  const taskTabOf = (id: number) => {
    const own = chatOf(tabs, id);
    const from = own === undefined ? undefined : fromOf(own);
    if (!from?.task) return undefined;
    const asker = chatsByNumber.get(from.chat);
    return { asker: asker === undefined ? undefined : shownName(tabs, asker) };
  };
  /** The Start fresh row a tab's mark presses: none for a task's own tab, which draws no
   *  such mark (its row is in the tab's menu, and says why it cannot run). */
  const freshOf = (id: number) => (taskTabOf(id) === undefined ? by(`tab.fresh:${id}`) : undefined);

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
              branch: piece.branch,
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

  /** What this window is cloning into the focused workspace (#1215), by repo — each row of
   *  "Not cloned here" draws its own progress and failure from it. */
  const cloning = useRepoClones(plane, ofWorkspace ?? OUTSIDE);
  const absentHere = workspaceState.panels?.absent;
  // A repo that has cloned stays busy until the panels drop it, so its Clone cannot be pressed
  // again in between; once they do, the window forgets it (`settleRepoClones`).
  const cloningNow = useMemo(
    () => [...cloning].filter(([, one]) => one.state !== "failed").map(([repo]) => repo),
    [cloning],
  );
  useEffect(() => {
    if (ofWorkspace !== undefined && absentHere !== undefined)
      settleRepoClones(plane, ofWorkspace, absentHere);
  }, [plane, ofWorkspace, absentHere]);

  /** The repo whose clone the chat being picked would start in, if it would: what the picker's
   *  branch box names (GL-1). The same directory `startOn` sends, compared with the path the
   *  core spelled for each clone. */
  const pickingInRepo = useMemo(() => {
    if (picking === undefined) return undefined;
    const cwd = ("in" in picking.where ? picking.where.in : undefined) ?? startIn;
    return clones.find((one) => one.path === cwd)?.repo;
  }, [clones, picking, startIn]);

  /** The plane's personas, straight off the plane's own answer — the array, not a copy of it,
   *  so the catalogue is rebuilt when the plane is read again and not per render. */
  const personas = workspaceState.panels?.personas;
  /** What Ask {persona}… offers on this project's chats, as the core answers. */
  const askOffer = useAskOffer(
    plane,
    personas,
    settingsChanges + curationsChanges,
    tabs.order
      .map((tab) => chatOf(tabs, tab))
      .filter((chat) => chat !== undefined)
      .join(","),
  );
  /** The focused workspace's open todos, the same way: one close and one forget row each. */
  const todos = workspaceState.panels?.todos;
  /** The session records the palette offers rows for: the place in front's (SI-8d). */
  const sessionRecords =
    focused === OUTSIDE ? rootPanels?.sessions : workspaceState.panels?.sessions;
  /** What the plane, its workspaces and its personas are offered to curate (ADR 0061), read
   *  again when those change and when the plane changes on disk. */
  const subjects = useMemo(
    () => curationSubjects(sidebar?.workspaces.map((ws) => ws.name) ?? [], personas ?? []),
    [personas, sidebar],
  );
  const curations = useCurations(plane, subjects, curationsChanges);

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
            absent: absentHere,
            cloning: cloningNow,
            startsIn: spot?.path,
            personas,
            vaults: vaultNames,
            todos,
            sessions: sessionRecords,
            plane,
            projects,
            // Which window this is, for the rows that move a project between windows (charter#126).
            split: thisWindow() !== MAIN,
            // WHICH row was refused and is still on screen. The catalogue matches the ids it
            // wrote itself, so the discard row appears beside the removal that was refused and
            // beside no other — with one removal per piece that is the difference between one
            // offer to throw work away and fifty.
            refused: report?.refused ? report.from : undefined,
            // **With no queue, and no Smart closes that stopped** (#1034): their rows are put in
            // as this view reports (`queued`), so a chat that starts asking for you neither
            // rebuilds the catalogue nor redraws the panes.
            needsYou: NO_QUEUE,
            quiet,
            nameOf,
            reportsTo: (session) => reports[session] ?? [],
            refusedIn: (session) => refusals[session] ?? [],
            neededFor: (session) => needs[session] ?? [],
            stoppedBelow: (session) => stoppedBelow[session] ?? [],
            listed: listedChats,
            askedBy,
            finished: finishedTasks,
            stopping: [...stopping],
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
            curations,
            wrappingUp: [...wrapping],
            workItems,
            // A chat filed in a workspace; one at the project root, or outside the project, is
            // offered no work link (ADR 0088 §4).
            linkable: (session) => filedIn(session) !== OUTSIDE,
            planeUpdated: planeUpdates,
            restartable,
            ask: askOffer,
            // Not from a shell: it is on no harness profile to start the persona's chat on.
            askable: (session) => !shells.has(session),
            branch: nearBranch,
            harnesses: harnessCards,
          }),
    [
      askedBy,
      clones,
      cloningNow,
      absentHere,
      curations,
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
      refusals,
      reports,
      needs,
      stoppedBelow,
      listedChats,
      finishedTasks,
      stopping,
      strips,
      tabs,
      vaultNames,
      todos,
      sessionRecords,
      views,
      extensionCommands,
      worktree,
      liveNames,
      wrapping,
      workItems,
      filedIn,
      planeUpdates,
      restartable,
      askOffer,
      shells,
      nearBranch,
      harnessCards,
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
  const [endingChat, setEndingChat] = useState<{
    offer: Offer;
    /** The one chat the close is about, where it is about one — what Smart close would close. */
    session?: number;
    /** Whether that chat is offered Smart close (ADR 0064), or none where charter cannot say. */
    smart?: SmartAsk;
    /** The persona chats that chat asked for that are still running, by name: what the dialog
     *  asks about once, keep them running or stop them. */
    running?: readonly string[];
    /** Its persona chats that have reported and close with it, by name. */
    closing?: readonly string[];
    /** The tasks among what is closed, by name: sent back to the list, not ended (#1488). */
    back?: readonly string[];
    /** For a close of several chats: whether any has chats at work below it, which keep
     *  running. */
    keeps?: boolean;
    /** The task the tab being closed shows, and the session that ends (#1486). */
    shows?: { task: string; session: string };
    /** How many tasks below the closing session have a tab of their own (#1489). */
    ownTabs?: number;
  }>();

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
        const ending = chatsEndedBy(offer.does, now.current, askedByNow);
        // **A close ends no task** (#1488): the tasks whose panes a tab's close takes go back
        // to the Chats list, are not asked about, and are named in the dialog's one line.
        const back =
          offer.does.verb === "closeTab"
            ? panesOf(now.current, offer.does.tab)
                .map((one) => one.session)
                .filter((session) => askedByNow(session) !== undefined)
                .map((session) => nameOfNow.current(session))
            : [];
        // Nothing but tasks: there is no chat to end, so there is nothing to ask.
        if (back.length > 0 && ending.length === 0) return carryOut(offer);
        /** What closing chat `session` would do with the chats below it, as the core says. A
         *  core that answers nothing (an older one) has none below it. */
        const below = async (session: number) => {
          const theirs = await commands
            .personaChatsOf(plane, session)
            .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
          const said = theirs.status === "ok" ? theirs.data : null;
          return {
            running: Array.isArray(said?.running) ? said.running : [],
            closing: Array.isArray(said?.tasks)
              ? said.tasks.filter((one) => one.closes_with_its_asker).map((one) => one.name)
              : [],
          };
        };
        if (ending.length !== 1) {
          // A close of several chats at once asks nothing about what each started: those
          // keep running, and the dialog says so where any is.
          const theirs = await Promise.all(ending.map(below));
          setEndingChat({
            offer,
            smart:
              ending.length > 1
                ? { available: false, why: MORE_THAN_ONE_CHAT, close_first: false }
                : undefined,
            keeps: theirs.some((one) => one.running.length > 0),
            back,
          });
          return { ok: true };
        }
        const session = ending[0];
        // The core's answer, asked now: whether the chat can write a record depends on what it
        // is doing this moment, and the command that starts a smart close asks the same thing.
        const asked = await commands
          .smartCloseOffer(plane, session)
          .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
        const smart =
          asked.status === "ok" && asked.data !== null && typeof asked.data === "object"
            ? asked.data
            : undefined;
        // And the chats at work below it, which the close asks about once, and its reported
        // persona chats, which close with it.
        const { running, closing } = await below(session);
        // **A tab that shows a task says which chat its close ends** (#1486): the session's,
        // not the task on screen.
        const does = offer.does;
        const task = does.verb === "closeTab" ? taskShownIn(now.current, does.tab) : undefined;
        const shows =
          does.verb !== "closeTab" || task === undefined
            ? undefined
            : {
                task: nameOfNow.current(task),
                session: now.current.byId[does.tab]?.name ?? "",
              };
        // **Its tasks that have a tab of their own** (#1489): the dialog says how many, since
        // they go back to the list or go with it by the answer given here.
        const ownTabs = tasksPlacedBelow(now.current, session, askedByNow).filter(
          (one) => one.placed === "tab",
        ).length;
        setEndingChat({ offer, session, smart, running, closing, shows, back, ownTabs });
        return { ok: true };
      }
      return carryOut(offer);
    },
    [askedByNow, carryOut, plane, setEndingChat],
  );

  /**
   * Smart close pressed: the core sends the chat its prompt, now or when its turn ends, and **the
   * tab goes into the background at once** (SI-8f) — a chip at the strip's left edge, and the
   * front where Close would have sent it (`tabs.sendToBackground`, which `closeTab` shares). A
   * tab with another chat beside it in a split stays where it is, wearing the wrapping-up mark.
   * A start the core refuses brings it back, says why, and lists it as needing the operator.
   */
  const beginSmartClose = useCallback(
    async (session: number) => {
      stoppedFor(session, undefined);
      setLeaving((was) => new Set([...was, session]));
      const tab = now.current.order.find((id) =>
        panesOf(now.current, id).some((one) => one.session === session),
      );
      const goes =
        tab !== undefined &&
        contentsOf(now.current, tab).every(
          ({ content }) =>
            content.kind === "session" &&
            (content.session === session || wrapping.has(content.session)),
        );
      if (goes) change((tabs) => sendToBackground(tabs, tab, filedIn, isPinned, isBackground));
      const began = await commands
        .smartClose(plane, session)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (began.status === "error") {
        settle(session);
        stoppedFor(session, DID_NOT_START);
        setReport({ from: `smartclose:${session}`, refused: true, words: began.error });
      }
    },
    [change, filedIn, isBackground, isPinned, plane, settle, stoppedFor, wrapping],
  );

  /**
   * **Stop them**, the person's answer about the chats at work below a closing chat: one
   * command to the core (`close_chat_stopping`), which ends them, deepest first, and then the
   * chat itself in the same step, so it cannot start another in between. For a Smart close
   * it ends them now and remembers the answer for the close the chat's record brings. The
   * window takes away the tabs of what the core closed — `closeChat`, never `close_session`,
   * which would end them a second time.
   */
  const closeStopping = useCallback(
    async (session: number, then: "close" | "smart_close") => {
      const stopped = await commands
        .closeChatStopping(plane, session, then)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (stopped.status === "error") {
        setReport({ from: `close-stopping:${session}`, refused: true, words: stopped.error });
        return false;
      }
      const gone = Array.isArray(stopped.data) ? stopped.data : [];
      if (gone.length > 0) {
        const before = now.current;
        change((tabs) =>
          gone.reduce(
            (left, one) => closeChat(left, one, filedIn, isPinned, inTheBackground.current),
            tabs,
          ),
        );
        // Whatever below them the core did not end (it ends what is at work) has no tab left.
        tasksGoBack(gone, before);
      }
      return true;
    },
    [change, filedIn, isPinned, plane, tasksGoBack],
  );

  const press = useCallback(
    (offer: Offer) => {
      void run(offer);
    },
    [run],
  );
  /**
   * **The two rows that end a task, for a tab chip's menu** (#1487 mounting #1488's
   * `taskEndIds`): read from the catalogue as the menu is drawn. One function for the life of
   * the view, so a chip is not drawn again each time the catalogue is made anew.
   */
  const byNow = useRef(by);
  const pressNow = useRef(press);
  useEffect(() => {
    byNow.current = by;
    pressNow.current = press;
  }, [by, press]);
  const pressTaskEnd = useCallback((offer: Offer) => pressNow.current(offer), []);
  /** A link out of Settings (#1387, #1388): the catalogue's row it names, run as the palette
   *  runs it. One the catalogue does not hold now does nothing. */
  useEffect(() => {
    const asked = (event: Event) => {
      const wanted = (event as CustomEvent<SettingsActionAsk>).detail;
      if (wanted.plane !== plane) return;
      const offer = byNow.current(wanted.action);
      if (offer !== undefined) pressNow.current(offer);
    };
    window.addEventListener(SETTINGS_ACTION, asked);
    return () => window.removeEventListener(SETTINGS_ACTION, asked);
  }, [plane]);
  /** Stop all tasks of a session, for its tab chip's menu (#1498): the catalogue's row, read
   *  as the menu is drawn, and one function for the life of the view. */
  const stopAllOf = useCallback((session: number) => byNow.current(stopAllId(session)), []);
  const taskEndsOf = useCallback(
    (session: number) =>
      taskEndIds(session).flatMap((id) => {
        const offer = byNow.current(id);
        return offer === undefined ? [] : [offer];
      }),
    [],
  );

  /** Moves a task from its line in a tab's menu (#1489): to a tab of its own, beside its
   *  session, or back. The catalogue's own rows, read as they stand when pressed, and held
   *  as one function for the window's life, as {@link showFromChip} is. */
  const placeNow = useRef<(session: number, where: TaskPlace) => void>(() => undefined);
  useEffect(() => {
    placeNow.current = (session, where) => {
      const offer = by(placeRowId(session, where));
      if (offer?.available) press(offer);
    };
  });
  const placeFromChip = useCallback(
    (session: number, where: TaskPlace) => placeNow.current(session, where),
    [],
  );
  /** What a tab's chats used, for its menu (#1500): read only as the window asks. */
  const usedFromChip = useCallback((ask: UsedAsk) => readUsed(plane, "menu", ask), [plane]);
  /** Opens a session's Activity from its tab's menu (#1495): the view the catalogue's
   *  `tab.activity` row opens, named as the chat is now. One function for the window's life. */
  const activityNow = useRef<(session: number) => void>(() => undefined);
  useEffect(() => {
    activityNow.current = (session) =>
      showView(activityView(session), activityTitle(nameOfNow.current(session)));
  });
  const activityFromChip = useCallback((session: number) => activityNow.current(session), []);

  /**
   * **A tab on the strip was pressed.** One that is behind comes forward, on whatever it was
   * left showing. **One already in front goes back to its session's own chat** (#1486,
   * V100-36), which is the way out of a task that needs no reading: the tab is the session.
   *
   * **Renaming a tab and moving it do not change what it shows.** A double-click renames, and
   * its first click is a click; a drag ends in a click too. So a pointer's press goes back a
   * moment later, unless a second click or a drag says it was not a press to go back. A key's
   * press is a press and nothing else, and goes back at once.
   */
  const backSoon = useRef<number | undefined>(undefined);
  const draggedAt = useRef(0);
  const dragging = useRef(false);
  /** Whether a tab is being carried along the strip: no task menu opens under a drag (#1487). */
  const isDragging = useCallback(() => dragging.current, []);
  const notGoingBack = useCallback(() => {
    window.clearTimeout(backSoon.current);
    backSoon.current = undefined;
  }, []);
  useEffect(() => notGoingBack, [notGoingBack]);
  const pressTab = useCallback(
    (id: number, select: Offer | undefined, byPointer: boolean) => {
      notGoingBack();
      if (now.current.inFront !== id) {
        if (select?.available) press(select);
        return;
      }
      const back = () => {
        const tab = now.current.byId[id];
        if (tab?.shows === undefined || now.current.inFront !== id) return;
        const own = panesOf(now.current, id).find((one) => one.pane === tab.focused)?.session;
        change((tabs) => showOwn(tabs, id));
        if (own !== undefined) giveKeyboardTo(plane, own);
      };
      if (now.current.byId[id]?.shows === undefined) return;
      // A tab being carried, or just put down, was not pressed: the keys and the click that
      // start and end a drag are the drag's.
      if (dragging.current || performance.now() - draggedAt.current < AFTER_A_DRAG) return;
      if (!byPointer) back();
      else backSoon.current = window.setTimeout(back, A_DOUBLE_CLICK);
    },
    [change, notGoingBack, plane, press],
  );

  /**
   * **The key that opens a shell tab** (`shellKey.ts`): the catalogue's `shell.new` row, pressed
   * as the palette would press it. Claimed on the window, capture-phase, for the reason the
   * palette's key is — a pane's terminal would otherwise have it first — and only by the
   * project in front, so one keypress is one shell.
   */
  useEffect(() => {
    if (!inFront) return;
    const key = (e: KeyboardEvent) => {
      if (!opensAShell(e, onAMac())) return;
      const offer = by("shell.new");
      if (offer === undefined) return;
      e.preventDefault();
      e.stopPropagation();
      // A held key is not a second press: one shell for one press.
      if (e.repeat) return;
      press(offer);
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [by, inFront, press]);

  /**
   * **The keys for the chats inside the tab in front** (`taskKeys.ts`, #1487): its task menu,
   * the next and the previous chat in it, and back to its session's own chat. Each presses the
   * catalogue's row, as the palette would. Claimed on the window, capture-phase, by the
   * project in front, as the shell's key is.
   *
   * **Not under a dialog**: a question the person is answering keeps every key. **Over a
   * terminal the chord is the window's whether or not its row can run**: none of the four is a
   * byte to a terminal, so nothing is kept from a chat, and a key that did something on one tab
   * and reached the program on the next would be worse. **In a text field or an editor a chord
   * whose row cannot run is left alone**: there it may be the field's own.
   */
  useEffect(() => {
    if (!inFront) return;
    const key = (e: KeyboardEvent) => {
      const which = taskKeyOf(e, onAMac());
      if (which === undefined || inADialog()) return;
      const offer = by(TASK_KEY_ROW[which]);
      if (offer?.available !== true && typedInto(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.repeat || offer?.available !== true) return;
      press(offer);
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [by, inFront, press]);

  /**
   * **The key that opens a Search tab** (`searchKey.ts`, FM-8): as narrow as the focus — the
   * branch the explorer picked, else the workspace in front, else the project — or the Search
   * tab already open for that, brought forward. Claimed on the window, capture-phase, by the
   * project in front, as the shell's key is; off a Mac, inside a chat, the chord is the chat's
   * find bar and is left to it.
   */
  useEffect(() => {
    if (!inFront) return;
    const key = (e: KeyboardEvent) => {
      const mac = onAMac();
      if (!opensSearch(e, mac)) return;
      const on = e.target;
      if (!mac && on instanceof Element && on.closest(`[${CHAT_KEYBOARD}]`) !== null) return;
      e.preventDefault();
      e.stopPropagation();
      if (e.repeat) return;
      const ask = searchFromFocus(nearBranch, ofWorkspace);
      showView(searchView(ask), searchTitle(ask));
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [inFront, nearBranch, ofWorkspace, showView]);

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
  /** The close of pane `pane` of the tab in front, decided for that pane (`paneCloseOf`). */
  const closeOfPane = useCallback(
    (pane: number) => paneCloseOf(tabs, pane, nameOf, askedBy),
    [askedBy, nameOf, tabs],
  );
  /** Pane `pane` of the tab in front goes back to its session's own chat (#1486): the person's
   *  way out of a task that has ended. Its terminal takes the keyboard. */
  const backInPane = useCallback(
    (pane: number) => {
      const id = now.current.inFront;
      if (id === undefined) return;
      const own = panesOf(now.current, id).find((one) => one.pane === pane)?.session;
      change((tabs) => focusPane(showOwnIn(tabs, id, pane), pane));
      if (own !== undefined) giveKeyboardTo(plane, own);
    },
    [change, plane],
  );
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

  // The chats still open, in the order the tab bar shows them: what a quit would end. What
  // each one is doing is resolved when the report is made, below, and has to be looked up in
  // this project, because a session number names a chat only inside its own project.
  const quitting = useMemo<Leaving[]>(
    () =>
      tabs.order.flatMap((id) => {
        const filed = workspaceOf(tabs, id, filedIn);
        return panesOf(tabs, id).map(({ session }) => {
          const known = reopened.find((chat) => chat.session === session);
          return {
            session,
            chat: {
              key: `${plane}#${session}`,
              project: plane,
              // The tab's name — the one the operator gave it, or its default — and not the
              // harness's: this is a list the operator reads (charter-app#254).
              name: tabs.byId[id].name,
              harness: known?.harness ?? null,
              cwd: known?.cwd ?? null,
              workspace: filed === OUTSIDE ? OUTSIDE_TITLE : filed,
            },
          };
        });
      }),
    [filedIn, plane, reopened, tabs],
  );

  // **This project's chats asking, for the title bar's list** (charter-app#249), which is the
  // window's and holds every project's. Their rows are the catalogue's own (`needsYouRows`),
  // asked on their own because the catalogue is built only for the project in front.
  //
  // **And the chats whose Smart close stopped without a record** (SI-8f): each says why, beside
  // its name. One already in the queue is one item that says it; one that is not is an item of
  // its own, whose ✕ dismisses it.
  //
  // **Asked with the queue as this view reports, and not as it draws** (#1034), with the
  // catalogue's queue rows: the queue changes whenever a chat starts or stops asking for you,
  // and nothing this view draws but the counts reads it.
  const queued = useCallback(
    (queue: readonly number[]): Pick<PlaneReport, "asking" | "offers"> => {
      const reportsTo = (session: number) => reports[session] ?? [];
      const refusedIn = (session: number) => refusals[session] ?? [];
      const neededFor = (session: number) => needs[session] ?? [];
      const rows = catalogued([
        ...needsYouRows(
          queue,
          nameOf,
          tabs,
          reportsTo,
          refusedIn,
          neededFor,
          (session) => listedChats.some((chat) => chat.session === session),
          (session) => stoppedBelow[session] ?? [],
        ),
        ...stoppedRows(stopped, queue, nameOf, tabs),
      ]);
      const item = (session: number, ignore: string): Asking => {
        const filed = filedIn(session);
        const persona = personaOf(session);
        return {
          session,
          name: nameOf(session),
          persona,
          mark: persona === null ? null : (personaMarks.marks.get(persona) ?? null),
          reported: reportsTo(session),
          stoppedBelow: stoppedBelow[session] ?? [],
          // What the app found the chat needs you for is said first (#1448).
          needed: neededFor(session)[neededFor(session).length - 1],
          // A Smart close that stopped says so; a refused commit (SQ-16) says its latest.
          why: stopped[session] ?? refusedIn(session)[refusedIn(session).length - 1],
          workspace: filed === OUTSIDE ? OUTSIDE_TITLE : filed,
          go: rows.get(showId(session)),
          ignore: rows.get(ignore),
        };
      };
      const alsoStopped = Object.keys(stopped)
        .map(Number)
        .filter((session) => rows.has(dismissId(session)));
      return {
        asking: [
          ...queue.map((session) => item(session, ignoreId(session))),
          ...alsoStopped.map((session) => item(session, dismissId(session))),
        ],
        offers: withQueue(offers, queue, {
          tabs,
          nameOf,
          reportsTo,
          refusedIn,
          neededFor,
          listed: listedChats,
          stoppedBelow: (session) => stoppedBelow[session] ?? [],
          stopped,
        }),
      };
    },
    [
      filedIn,
      listedChats,
      nameOf,
      needs,
      offers,
      personaMarks.marks,
      personaOf,
      refusals,
      reports,
      stopped,
      stoppedBelow,
      tabs,
    ],
  );

  // What this project has open, told to the window: the quit warning lists every project's
  // chats, and this project's own tab says when one of them needs you.
  //
  // **Its catalogue travels with it, and that is what keeps the palette one palette.** The
  // palette is mounted on the WINDOW — always, so `F2` reaches it before the core has said
  // which project this launch opened, and once, so a project that is not in front is not a
  // second capturing listener for the same key. What it lists has to be the project in
  // front's, and this is how it gets there.
  //
  // **Without what its chats are doing**, which is added as it is reported, below: that changes
  // with every move, and a report rebuilt here for each one was the whole view redrawn for each.
  // **Nor the queue** (#1034), for the same reason: what reads it is added there too (`queued`).
  const mine = useMemo<Omit<PlaneReport, "ending" | "moved" | "asking" | "offers">>(
    () => ({
      quiet,
      settled,
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
      // Where ⌘P's files are found first (FM-7): the cockpit's branch, else the one the
      // explorer picked (FM-5).
      branch: nearBranch,
    }),
    [nearBranch, ofWorkspace, quiet, report, run, saving, settled, sidebar],
  );
  // **Before the paint, not after it.** A quit — Cmd-Q, the tray, the menu — arrives whenever
  // it arrives, and the window decides on what every project has told it: a report that
  // landed a frame late would let a quit warn about nothing, or worse, end a chat it had not
  // heard about yet. `useLayoutEffect` puts this in the same flush as the render that
  // produced it, which is the nearest thing to the synchronous ref the single-project window
  // used before there was anything to report to.
  //
  // **And again whenever a chat moves, straight off the store** (SC-3): what each chat is doing
  // and when the newest of them moved are the report's, so the quit warning and the project
  // strip's order stay current, and this view is not redrawn to say so. A move that changes
  // neither is not reported at all.
  /** Its sessions, for the status line's count of the ones running. */
  const quitSessions = useMemo(() => quitting.map((chat) => chat.session), [quitting]);
  // **Who it was sent to, and for which project, are part of what was sent** (#1037): a new
  // callback, or this view told it is another project, has heard nothing yet, so it is told once
  // even when nothing else moved.
  const reported = useRef<
    | {
        to: typeof onReport;
        plane: PlaneId;
        mine: typeof mine;
        quitting: Leaving[];
        doing: State[];
        moved: number;
        queued: typeof queued;
        queue: readonly number[];
        fromQueue: ReturnType<typeof queued>;
      }
    | undefined
  >(undefined);
  useLayoutEffect(() => {
    const report = () => {
      const states = chats.store.statesFor(chats.plane);
      const doing = quitting.map((chat) => stateOf(states, chat.session));
      const moved = Math.max(0, ...Object.values(states.movedAt));
      const queue = states.needsYou;
      const was = reported.current;
      const sameQueue = was?.queued === queued && sameList(was.queue, queue);
      if (
        was?.to === onReport &&
        was.plane === plane &&
        was.mine === mine &&
        was.quitting === quitting &&
        was.moved === moved &&
        sameList(was.doing, doing) &&
        sameQueue
      )
        return;
      // The queue's rows are built again only when the queue, or what words them, changed.
      const fromQueue = sameQueue ? was.fromQueue : queued(queue);
      reported.current = {
        to: onReport,
        plane,
        mine,
        quitting,
        doing,
        moved,
        queued,
        queue,
        fromQueue,
      };
      onReport(plane, {
        ...mine,
        ...fromQueue,
        ending: quitting.map(({ chat }, at) => ({ ...chat, state: doing[at] })),
        moved,
      });
    };
    report();
    return chats.store.subscribe(report);
  }, [chats, quitting, mine, onReport, plane, queued]);

  const frontChat = frontTab && reopened.find((chat) => chat.session === chatOf(tabs, frontTab.id));

  /**
   * **Files into chats** (FM-9): every chat of this project, for the preview's *Ask a chat about
   * this* and *Add to a chat's context*, and the one way a reference reaches a chat — a drop on
   * its tab or pane included. The core types it, unsent, or copies it and says why; what it
   * answered is said where every row's answer is.
   */
  const referenceChats = useMemo<ChatsForReferences>(() => {
    const chatsOpen = tabs.order.flatMap((id) =>
      panesOf(tabs, id).map((pane) => ({ session: pane.session, name: tabs.byId[id].name })),
    );
    return {
      plane,
      chats: chatsOpen,
      hand: (r, session, how) => {
        // Named as its row names it: a task shown in a tab is not one of the tabs' own chats.
        const name =
          chatsOpen.find((chat) => chat.session === session)?.name ??
          nameOfListed(session) ??
          `chat ${session}`;
        void handReference(plane, session, name, r).then((ran) => {
          setReport(
            ran.ok
              ? { from: "reference", refused: false, words: ran.said ?? "" }
              : { from: "reference", refused: true, words: ran.refused },
          );
          if (how === "ask" && ran.ok) showChat(session);
        });
      },
    };
  }, [nameOfListed, plane, showChat, tabs]);

  // Each strip is ONE Tab stop, the selected tab, and the arrows move along it (charter-app#189,
  // `roving.ts`). Asked here rather than below the early return, because they are hooks.
  const workspaceStop = useTabStop(focused, workspacesShown.shown);
  const chatStop = useTabStop(
    tabs.inFront === undefined ? undefined : String(tabs.inFront),
    shown.map(String),
  );

  /** **While you were away** (#1514): kept for this project whether or not it is in front,
   *  so a project behind another has its own summary when the person switches to it. */
  const { away: awayOn } = useChatsListPrefs();
  const awayNow = useAwaySummary({
    chats,
    on: awayOn,
    finished: finishedTasks,
    nameOf,
    refusedAway: awayRefused,
  });

  // A project the operator is not looking at keeps every piece of state above and draws none
  // of it. See this module's own docstring for why it is `null` and not `hidden`.
  if (!inFront) return null;

  return (
    <Lent
      chats={chats}
      tasksBelow={tasksBelow}
      doings={doings}
      references={referenceChats}
      personas={personaMarks}
      askPersona={openAskPersona}
      brief={openBrief}
      answer={openAnswer}
    >
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
                  data-strip="Workspaces"
                  ref={workspaceStrip}
                  style={
                    {
                      "--least": `${workspaceLeast}px`,
                      "--root": `${rootWidth}px`,
                    } as CSSProperties
                  }
                >
                  {/* The tablist, which owns the tabs and not the gear beside one (#1204,
                      `StripTablist.tsx`). */}
                  <StripTablist
                    name="Workspaces"
                    ids={workspacesShown.shown.map((_, place) => workspaceTabId(place))}
                  />
                  {workspacesShown.shown.map((workspace, place) => {
                    const offer = by(`workspace.focus:${workspace}`);
                    // **The plane root** (SI-1): first, fixed, an icon with the operator's
                    // tooltip, and a menu of its own — it is not a workspace.
                    const root = workspace === OUTSIDE;
                    return (
                      <SortableTab key={workspace} id={workspace} fixed={workspace === OUTSIDE}>
                        {({ sortable, style }) => (
                          /* The cell: the tab and, on the focused workspace's, its gear
                             (SE-23). A wrapper, as a project's tab and its `×` have, because a
                             button cannot hold a button. It is what is dragged; it carries the
                             tab's tint too, so the gear sits on the tab's own shade. */
                          <span
                            className="workspace"
                            ref={sortable.setNodeRef}
                            data-dragging={sortable.isDragging || undefined}
                            style={{ ...tintOf(workspace), ...style }}
                          >
                            {/* Right-click is the third reader of the catalogue (`Menus.tsx`):
                                focus, pin, make one, and — under the line — delete this one.
                                `asChild`, and on the tab and not the cell: the context-menu
                                key opens a menu only on the trigger that has the keyboard
                                (`openFromTheKeyboard`), and that is the tab. */}
                            <Menued
                              on={root ? { on: "root" } : { on: "workspace", workspace }}
                              offers={found}
                              onPress={press}
                            >
                              <RovingFocusGroup.Item
                                asChild
                                tabStopId={workspace}
                                active={workspace === focused}
                              >
                                <button
                                  role="tab"
                                  id={workspaceTabId(place)}
                                  className={root ? "plane-root" : undefined}
                                  aria-label={root ? OUTSIDE_TITLE : undefined}
                                  aria-selected={workspace === focused}
                                  aria-describedby={
                                    root ? undefined : sortable.attributes["aria-describedby"]
                                  }
                                  title={root ? ROOT_TIP : offer?.title}
                                  // Its own colour, in front or not (charter-app#281): its shade and its
                                  // mark are its tint, set on the tab and the cell around it.
                                  data-colour={colourOf(workspace) ?? undefined}
                                  style={tintOf(workspace)}
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
                            {/* Its settings (SE-23, V89i): on the focused workspace only, and
                                quiet until its tab is under the pointer or the keyboard. The
                                row its menu's Workspace settings… runs. The plane root is not
                                a workspace and has none. */}
                            {workspace === focused && !root && (
                              <Gear offer={by(`workspace.settings:${workspace}`)} onPress={press} />
                            )}
                          </span>
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
            <QueueRead>
              {(queue) => (
                <ShowMore
                  noun="workspace"
                  hidden={workspacesNotShowing.map((workspace) => ({
                    key: workspace,
                    offer: by(`workspace.focus:${workspace}`),
                    needs: waitingIn(queue, workspace),
                    children: workspaceMarks(workspace),
                  }))}
                  onPress={press}
                />
              )}
            </QueueRead>
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
          onDragStart={() => {
            dragging.current = true;
            notGoingBack();
          }}
          onDragCancel={() => {
            dragging.current = false;
            draggedAt.current = performance.now();
          }}
          onDragEnd={({ active, over }) => {
            // The click that ends a drag is the drag's (#1486, `pressTab`).
            dragging.current = false;
            draggedAt.current = performance.now();
            notGoingBack();
            if (over) dragTab(Number(active.id), Number(over.id));
          }}
        >
          <SortableContext items={shown.map(String)} strategy={horizontalListSortingStrategy}>
            <RovingFocusGroup.Root asChild orientation="horizontal" {...chatStop}>
              <div
                className="tabs"
                data-strip="Tabs"
                ref={strip}
                style={{ "--least": `${chatLeast}px`, "--chip": `${chipWidth}px` } as CSSProperties}
              >
                {/* The tablist, which owns the tabs and not the `×`, fresh mark and task chip
                    beside each (#1204, `StripTablist.tsx`). Not a tab being renamed: its name
                    box is drawn in its place, and is not a tab. */}
                <StripTablist
                  name="Tabs"
                  ids={shown.flatMap((id, place) => (nameBoxFor(id) ? [] : [chatTabId(place)]))}
                />
                {shown.map((id, place) => (
                  <SortableTab key={id} id={String(id)} fixed={isBackground(id)}>
                    {({ sortable, style }) => (
                      /* Right-click is the third reader of the catalogue (`Menus.tsx`).
                         `asChild`, so the strip gains no wrapper element: the trigger IS the
                         tab.

                         No `data-tab` and no scroll-into-view ref any more: #171 deleted
                         `offscreen.ts` and the strip collapses rather than scrolls, so there is
                         nothing to scroll a tab into and nothing measuring tabs through the
                         markup. */
                      <Menued
                        on={{ on: "chat", tab: id, session: chatOf(tabs, id) }}
                        offers={found}
                        onPress={press}
                      >
                        {isBackground(id) ? (
                          /* **In the background** (SI-8f): the chat is wrapping up, so its tab is a
                             chip — the chat's icon and the breathing mark, its name in the tooltip —
                             at the strip's left edge, and fixed: it is not dragged, and nothing is
                             dropped on it. Pressed, it brings the chat forward to watch it work;
                             it stays a chip while it wraps up. Its menu still has Cancel smart close. */
                          <span
                            className="tab chip"
                            ref={sortable.setNodeRef}
                            style={style}
                            data-chip=""
                            data-wrapping-up=""
                          >
                            <RovingFocusGroup.Item
                              asChild
                              tabStopId={String(id)}
                              active={id === tabs.inFront}
                            >
                              <button
                                role="tab"
                                id={chatTabId(place)}
                                aria-selected={id === tabs.inFront}
                                aria-label={chipSays(tabs.byId[id].name)}
                                title={chipSays(tabs.byId[id].name)}
                                onKeyDown={(event) =>
                                  closeOnDelete(event, by(`tab.close:${id}`), press)
                                }
                                onClick={() => {
                                  const offer = by(`tab.select:${id}`);
                                  if (offer?.available) press(offer);
                                }}
                              >
                                <SquareTerminal
                                  className="tab-mark"
                                  data-mark="chat"
                                  aria-hidden="true"
                                />
                                <WrappingUp held />
                              </button>
                            </RovingFocusGroup.Item>
                          </span>
                        ) : (
                          <span
                            className="tab"
                            ref={sortable.setNodeRef}
                            style={style}
                            data-dragging={sortable.isDragging || undefined}
                            // A task's own tab (#1489): what its look is hung on.
                            data-task-tab={taskTabOf(id) === undefined ? undefined : ""}
                            // A file, a folder, lines or a search hit dropped on a chat's tab is
                            // typed into that chat as a reference, unsent (FM-9).
                            // **Into the chat the tab shows** (#1486): the tab reads
                            // `steward 4 › talk`, so what is dropped on it goes to talk.
                            onDragOver={(event) => {
                              if (chatShownBy(tabs, id) !== undefined) overChat(event);
                            }}
                            onDrop={(event) => {
                              const session = chatShownBy(tabs, id);
                              const r = droppedReference(event);
                              if (session === undefined || r === undefined) return;
                              event.preventDefault();
                              referenceChats.hand(r, session, "add");
                            }}
                            // A chat being smart-closed wears the closing look (ADR 0064).
                            data-wrapping-up={
                              panesOf(tabs, id).some((one) => wrapping.has(one.session)) ||
                              undefined
                            }
                            // A chat being stopped wears the stop's look (#1459).
                            data-stopping={showsStopping(tabs, id, stopping) || undefined}
                          >
                            {nameBoxFor(id) ? (
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
                                  id={chatTabId(place)}
                                  aria-selected={id === tabs.inFront}
                                  // What a press does that the tab's name does not say (#1486):
                                  // in front and showing a task, it goes back to the session.
                                  aria-description={
                                    id === tabs.inFront && taskNameOf(id) !== undefined
                                      ? `Press to go back to ${tabs.byId[id].name}`
                                      : undefined
                                  }
                                  // Down opens the menu of its tasks, where it has any (#1487).
                                  aria-haspopup={tabHasTasks(id) ? "menu" : undefined}
                                  // The fresh mark beside it as well (#1246): it is outside the tab.
                                  aria-describedby={
                                    [
                                      sortable.attributes["aria-describedby"],
                                      freshMarkShown(
                                        freshOf(id),
                                        planeUpdates[chatOf(tabs, id) ?? -1],
                                      )
                                        ? freshMarkOf(id)
                                        : undefined,
                                      // And its tasks' counts, which are beside it too
                                      // (#1487): "Tasks of steward 4: 2 working".
                                      tabHasTasks(id) ? tasksIdOf(id) : undefined,
                                    ]
                                      .filter(Boolean)
                                      .join(" ") || undefined
                                  }
                                  // Where a handed-off chat came from, by its parent's name (charter-app#258).
                                  title={
                                    panesOf(tabs, id).some((one) => wrapping.has(one.session))
                                      ? WRAPPING_UP
                                      : tabTip(
                                          handedFrom[chatOf(tabs, id) ?? -1],
                                          workItems[chatOf(tabs, id) ?? -1],
                                        )
                                  }
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
                                    // **Down on a tab with tasks opens their menu** (#1487),
                                    // as Down opens the menu under an item of a menu bar. The
                                    // strip runs across, so Down was nothing's.
                                    if (
                                      event.key === "ArrowDown" &&
                                      !(event.altKey || event.ctrlKey || event.metaKey) &&
                                      !event.shiftKey &&
                                      tabHasTasks(id)
                                    ) {
                                      event.preventDefault();
                                      showTabTasks(id);
                                    }
                                  }}
                                  // The catalogue's row, not a second copy of it. The tab already in front
                                  // has a row that says so and cannot run — a tab is never disabled, because
                                  // the selected tab is the one a keyboard has to be able to land on.
                                  //
                                  // **Pressed while it is in front, a tab showing a task goes
                                  // back to its session's own chat** (#1486, `pressTab`).
                                  onClick={(event) =>
                                    pressTab(id, by(`tab.select:${id}`), event.detail > 0)
                                  }
                                  // A double-click on the name renames it — the same row again.
                                  onDoubleClick={() => {
                                    // Not a press to go back to the main chat (#1486).
                                    notGoingBack();
                                    // …and keeps a preview tab, VS Code's double-click (SI-9b).
                                    const offer = by(`tab.rename:${id}`) ?? by(`tab.keep:${id}`);
                                    if (offer?.available) press(offer);
                                  }}
                                >
                                  {/* No `updates` here: on the strip the plane-updated mark
                                      is a button of its own beside the tab (`FreshMark`). */}
                                  <TabMarks
                                    tabs={tabs}
                                    id={id}
                                    persona={personaOf(chatOf(tabs, id))}
                                    shells={shells}
                                    wrapping={wrapping}
                                    stopping={stopping}
                                    task={taskNameOf(id)}
                                    taskOf={taskTabOf(id)}
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
                            {/* **The chip of the session's tasks** (#1487): their counts, the
                                hand where one waits off screen, and the menu to switch
                                between them. Beside the tab's button: a button holds no
                                button. Nothing for a tab with no tasks. */}
                            <QueueRead>
                              {(queue) => (
                                <TabTasks
                                  id={tasksIdOf(id)}
                                  name={tabs.byId[id].name}
                                  rows={chatsByTab.get(id) ?? NO_ROWS}
                                  ended={endedByTab.get(id) ?? NO_ENDED}
                                  current={
                                    shownIn(tabs, id).find(
                                      (one) => one.pane === tabs.byId[id].focused,
                                    )?.session ?? chatShownBy(tabs, id)
                                  }
                                  needs={hiddenIn(queue, id)}
                                  asked={tasksAsked?.tab === id ? tasksAsked.count : 0}
                                  clock={chatClock}
                                  dragging={isDragging}
                                  onShow={showFromChip}
                                  ends={taskEndsOf}
                                  onPress={pressTaskEnd}
                                  onPlace={placeFromChip}
                                  used={usedFromChip}
                                  limits={runningOfTab.get(id)}
                                  stopAll={stopAllOf}
                                  onActivity={activityFromChip}
                                />
                              )}
                            </QueueRead>
                            <FreshMark
                              id={freshMarkOf(id)}
                              // **Never on a task's own tab** (#1489): nothing drawn on
                              // that tab ends the task's program, and a fresh start does.
                              offer={freshOf(id)}
                              files={planeUpdates[chatOf(tabs, id) ?? -1]}
                              onPress={press}
                            />
                            <Closer offer={by(`tab.close:${id}`)} onPress={press} />
                          </span>
                        )}
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
          <QueueRead>
            {(queue) => (
              <ShowMore
                noun="tab"
                hidden={notShowing.map((id) => ({
                  key: String(id),
                  offer: by(`tab.select:${id}`),
                  needs: waitingOn(queue, id),
                  children: (
                    <TabMarks
                      tabs={tabs}
                      id={id}
                      persona={personaOf(chatOf(tabs, id))}
                      updates={planeUpdates}
                      shells={shells}
                      wrapping={wrapping}
                      stopping={stopping}
                      task={taskNameOf(id)}
                      taskOf={taskTabOf(id)}
                      needs={hiddenIn(queue, id).map((chat) => chat.name)}
                    />
                  ),
                }))}
                onPress={press}
              />
            )}
          </QueueRead>
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

      {/* **The window's standing lines are Notices** (`Notice.tsx`, V91a–d): each has a way
          out, and `Notice.guard.test.ts` fails on one drawn any other way. A refused action's
          line has Dismiss, and goes by itself when that action next succeeds (NO-4). */}
      {/* **Stacked** (V91i): at most two stand under the strip, the most important first, and
          the rest are behind "+N more" (`NoticeBand`). */}
      <NoticeBand>
        {trouble && (
          <Notice
            cause={`window-trouble:${trouble.from}`}
            tone="trouble"
            onDismiss={() => setTrouble(undefined)}
          >
            {trouble.said}
          </Notice>
        )}

        {/* What happened to the chat in front when it was put back. Only a chat that came from
          the record has either, so a chat the operator just opened says nothing. */}
        {/* Both notes name the chat by what its tab says — `frontTab.name`, the one field the
          strip prints — and not by its recorded number, which the operator never reads
          ("5 came back" beside a tab that says "steward 5"). */}
        {frontTab && frontChat?.resumed && !dismissed.has(resumedNote(frontChat)) && (
          <Notice cause={resumedNote(frontChat)} onDismiss={() => dismiss(resumedNote(frontChat))}>
            <strong>{frontTab.name}</strong> was resumed — conversation{" "}
            <code>{frontChat.resumed}</code>
          </Notice>
        )}
        {/* What a Resume from a session record had to guess because the record could not say
          it — its profile, its directory (SI-8e) — said beside what happened, never instead. */}
        {frontTab && frontChat?.guessed && !dismissed.has(`chat-guessed:${frontChat.session}`) && (
          <Notice
            cause={`chat-guessed:${frontChat.session}`}
            onDismiss={() => dismiss(`chat-guessed:${frontChat.session}`)}
          >
            <strong>{frontTab.name}</strong>: {frontChat.guessed}.
          </Notice>
        )}
        {/* Only for a harness. Every chat is a shell until the harness picker lands, and a
          shell has no conversation to bring back — saying so on every relaunch, forever,
          is noise about the normal case. */}
        {frontTab &&
          frontChat?.fresh &&
          frontChat.harness &&
          !dismissed.has(`chat-fresh:${frontChat.session}`) && (
            <Notice
              cause={`chat-fresh:${frontChat.session}`}
              onDismiss={() => dismiss(`chat-fresh:${frontChat.session}`)}
            >
              <strong>{frontTab.name}</strong> came back as a new chat: {frontChat.fresh}
            </Notice>
          )}

        {/* **A smart close that ended on its record** (SI-8f): its tab has gone, so this is where
          the window says so — quietly, as news and not as a question, with the record one
          press away in its own view tab (SI-8d). */}
        {/* **The sandbox's one-time offer** to a project made before it (ADR 0067 §1, V21 1):
          a notice like the one below, answered once, never a dialog. */}
        <SandboxOffer plane={plane} />
        {/* **A sandbox setting changed under running chats** (#1428): how many keep the old
          sandbox, and the restart that gives them the new one. */}
        <SandboxChangedNotice
          older={olderSandbox}
          dismissed={dismissed}
          dismiss={dismiss}
          settle={settleNotices}
          onRestart={restartThem}
        />
        {/* **The project's own hosts changed** (#1341): told once to each teammate. */}
        <ProjectHostsNotice
          plane={plane}
          onReview={() => openSettingsAt({ group: "project.sandbox" })}
        />
        {/* **The project's Internet access presets changed** (#1385): told once to each
          teammate, so a preset that widens never widens unseen. */}
        <ProjectPresetsNotice
          plane={plane}
          onReview={() => openSettingsAt({ group: "project.sandbox" })}
        />
        {/* **A persona's hosts wait for you** (#1362): asked on each machine, never told. */}
        <PersonaHostsNotice plane={plane} />
        {/* **A teammate's dispatch grant arrived** (#1506): said here, at the window's level,
          when the project's settings gain one this person has not answered. */}
        <ProjectDispatchNotice
          plane={plane}
          onReview={() => openSettingsAt({ group: "project.dispatch" })}
        />
        {/* The doctor's findings that stand as Notices, each with its fix (#1250). */}
        <DoctorNotices
          doctor={doctor}
          dismissed={dismissed}
          dismiss={dismiss}
          settle={settleNotices}
        />
        {savedNotice && (
          <Notice
            cause="session-saved"
            link={
              savedNotice.record
                ? {
                    label: "Open record",
                    onPress: () => {
                      const record = savedNotice.record;
                      setSavedNotice(undefined);
                      if (record) showView(sessionView(record.path), sessionTitle(record.title));
                    },
                  }
                : undefined
            }
            onDismiss={() => setSavedNotice(undefined)}
          >
            Session saved{savedNotice.record ? ` — ${savedNotice.record.title}` : "."}
          </Notice>
        )}
        {/* **A Smart close that saved its record with no pass** (#1361): the chat ran it, and
          purlis did not see /smart-close typed in its pane, so its tab stayed open. Never a
          silent miss: one press closes it, as Close would. Only while its tab is there. */}
        {keptOpen
          .filter(({ session }) =>
            tabs.order.some((id) => panesOf(tabs, id).some((pane) => pane.session === session)),
          )
          .map(({ session, name }) => (
            <Notice
              key={session}
              cause={`smart-close-kept-open:${session}`}
              fixes={[{ label: "Close tab", onPress: () => closeKeptOpen(session) }]}
              onDismiss={() => forgetKeptOpen(session)}
            >
              {name} saved its session record. purlis did not see <code>/smart-close</code> typed in
              it, so its tab stayed open.
            </Notice>
          ))}

        {/* **A pin whose workspace is gone is kept dormant** (V91c as amended): never drawn, since
          a strip that showed it would offer a workspace the project does not have (ADR 0034's
          hazard, one scope down), and never written away. It is drawn again in its place when
          the workspace comes back. Forget is the operator's, with an Undo for this run. */}
        {dormantPins
          .filter((name) => !dismissed.has(`pin-dormant:${name}`))
          .map((name) => (
            <Notice
              key={name}
              cause={`pin-dormant:${name}`}
              fixes={[{ label: "Forget", onPress: () => void forgetDormantPin(name) }]}
              onDismiss={() => dismiss(`pin-dormant:${name}`)}
            >
              {name} is gone, kept dormant: its pin comes back in its place when the workspace does.
            </Notice>
          ))}
        {forgottenPins.map(({ name, after }) => (
          <Notice
            key={name}
            cause={`pin-forgotten:${name}`}
            fixes={[{ label: "Undo", onPress: () => void undoForget(name, after) }]}
            onDismiss={() => setForgottenPins((was) => was.filter((one) => one.name !== name))}
          >
            Forgot the pin to {name}.
          </Notice>
        ))}

        {/* A memory's Delete, which can be undone for a few seconds (SI-9b, ADR 0065 Q8). */}
        {memoryEdits.undo}

        {/* By the chat's id, never its name: two waiting chats can share a name (a split's chat
            takes its tab's), and each Notice acts on its own chat alone. */}
        {wouldNotStart.map(({ id, name, why, approval }) => (
          <Notice
            key={id}
            cause={`chat-did-not-start:${id}`}
            tone="trouble"
            fixes={[
              // Never a one-press Approve here (D-1246-5): the press opens the question that
              // shows what would run, and the approval is that question's answer.
              approval
                ? { label: REVIEW_AND_APPROVE, onPress: () => askApprove(id, name, approval) }
                : { label: "Retry now", onPress: () => void retryChat(id) },
              ...(approval ? [{ label: "Retry now", onPress: () => void retryChat(id) }] : []),
              {
                label: FORGET_THIS_CHAT,
                onPress: () => askForget(id, name),
              },
            ]}
            onDismiss={() => setWouldNotStart((was) => was.filter((one) => one.id !== id))}
          >
            <strong>{name}</strong> did not start ({why}). It is still recorded, and will be tried
            again at the next launch.
          </Notice>
        ))}

        {/* **While you were away** (#1514): what the project's tasks did while the person was
            away from the window, one line with a link to each part. It answers nothing. */}
        <AwaySummary
          away={awayNow}
          onShowChat={showChat}
          onShowFinished={showFinished}
          onShowNeedsYou={onShowNeedsYou}
        />
      </NoticeBand>

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
            /* The left region is navigation (ADR 0038): the project's chats above, and the
               focused workspace's repos and branches under them. */
            <div className="left-region">
              <WaitingTaskWaysContext.Provider value={waitingTaskWays}>
                <ChatsSection
                  rows={listRows}
                  // The chat that has the keyboard: a task, while its pane shows it (#1486).
                  front={focusedChat(tabs)}
                  onOpen={showChat}
                  offers={found}
                  onPress={press}
                  stopping={stopping}
                  finished={finishedTasks}
                  onClearFinished={clearFinished}
                  onReopen={reopenFinished}
                  clock={chatClock}
                  onDrawn={rowsDrawn}
                  reveal={revealed}
                  onRevealed={revealedSettled}
                  onLookFinished={lookedAtFinished}
                  restarts={restartsOnRows}
                  ending={taskEndInline}
                  onEndTask={endTaskOnRow}
                  onEndConfirm={answerTaskEndOnRow}
                  onChanges={(task) =>
                    showView(taskChangesView(task.id), taskChangesTitle(task.name))
                  }
                />
              </WaitingTaskWaysContext.Provider>
              <Explorer
                plane={plane}
                workspace={ofWorkspace}
                live={ofWorkspace !== undefined && liveOf(ofWorkspace)}
                state={workspaceState}
                chats={workspaceChats}
                spot={spot}
                onPick={pickSpot}
                onShowChat={showChat}
                wrapping={wrapping}
                offers={found}
                onPress={press}
                onOpenFile={(place, path) =>
                  showView(pieceFileView(place, path), pieceFileTitle(place, path))
                }
                focus={cockpit}
                onFocus={setFocusedBranch}
                cloning={cloning}
                onReadAgain={rereadPanels}
                listed={listedChats}
                finished={finishedTasks}
                below={tasksBelow}
                onRevealChats={revealChats}
                nameOf={nameOfListed}
              />
            </div>
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
              atRoot={focused === OUTSIDE}
              rootPanels={rootPanels?.contributed}
            />
          ),
          bottom: (
            <BottomBar
              workspace={ofWorkspace}
              state={workspaceState}
              offers={found}
              onPress={press}
              columns={facts.columns}
              cloning={cloning}
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
                <TaskEndContext.Provider value={taskEndHand}>
                  <LayoutPanes
                    plane={plane}
                    // Each pane holding the chat it SHOWS (#1486): a task, in the pane of the
                    // session that asked, while the tab is switched to it.
                    layout={layoutShown(tabs, frontTab.id) ?? frontTab.layout}
                    crumbs={frontCrumbs}
                    placed={frontPlaced}
                    away={frontAway}
                    finished={frontFinished}
                    explaining={chipToExplain}
                    others={frontOthers}
                    asked={frontAsked}
                    asks={asksHere}
                    closeOf={closeOfPane}
                    onBack={backInPane}
                    onShowChat={showChat}
                    focused={frontTab.focused}
                    onFocus={(pane) => change((tabs) => focusPane(tabs, pane))}
                    offerFor={by}
                    onPaneDoes={onPaneDoes}
                    name={frontTab.name}
                    handedFrom={handedFrom}
                    glances={glances}
                    workItems={workItems}
                    byHand={byHand}
                    onByHand={answerByHand}
                    startNotes={startNotes}
                    onDismissStartNote={dismissStartNote}
                    onRestartChat={askRestart}
                    blocks={frontBlocks}
                    onDismissBlock={dismissBlock}
                    onRestarted={chatRestarted}
                    onAllowed={oweRestart}
                    restartsSaid={restartsSaid}
                    onRestartAnswer={answerRestart}
                    offered={views}
                    onOpenView={showView}
                    onAsk={(pane) => change((tabs) => stopWaiting(tabs, pane))}
                    onVaultChanged={reloadVaults}
                    memory={{
                      // The views follow the disk themselves (`ViewPane`), so a memory saved
                      // redraws the views and not this whole window (FD-10).
                      changed: memoryEdits.changed,
                      onSaved: memoryEdits.onSaved,
                      onClose: closeView,
                      showInstead: (from, to, title) => {
                        change((tabs) => showInstead(tabs, from, to, title));
                        // The level switcher moving Settings is a way into it (#1206).
                        const place = placeOfView(to, plane);
                        if (place !== undefined) enterSettings(place);
                      },
                    }}
                    firstTask={firstTaskDoes}
                    split={{
                      of: (view) => splitOf(tabs, view),
                      moved: (view, split) => change((tabs) => setSplit(tabs, view, split)),
                    }}
                  />
                </TaskEndContext.Provider>
              ) : tabs.order.some((id) => !isBackground(id)) ? (
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
                  body="purlis runs each chat in its own pane. Open the first one here."
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
      <StatusLineHere
        sessions={quitSessions}
        settled={settled}
        plane={plane}
        read={sidebar !== undefined}
        where={focused === OUTSIDE ? OUTSIDE_TITLE : focused}
        workspaces={sidebar?.workspaces.length}
        state={workspaceState}
        doctor={doctor}
        onOpenSettings={(group) => openSettingsAt({ group })}
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

      {branching && (
        <NewBranch
          repo={branching.repo}
          trouble={branchTrouble}
          making={busyBranching}
          onCut={(branch) => void cutBranch(branch)}
          onCancel={() => {
            setBranching(undefined);
            setBranchTrouble(undefined);
          }}
        />
      )}

      {makingVault && (
        <NewVault
          plane={plane}
          trouble={vaultTrouble}
          making={busyVault}
          onCreate={(name, provider, opVault) => void makeVault(name, provider, opVault)}
          onMade={(done) => {
            // The guided set-up made it in the core (#1527): the same ending as `makeVault`.
            setMakingVault(false);
            setVaultTrouble(undefined);
            reloadVaults();
            showView({ from: null, view: "vault", key: done.contents.name }, done.contents.name);
          }}
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

      {linkingWork && (
        <LinkWorkItem
          chat={tabs.byId[linkingWork.tab]?.name ?? ""}
          linked={workItems[chatOf(tabs, linkingWork.tab) ?? -1]}
          trouble={linkingWork.trouble}
          linking={linkingWork.busy}
          onLink={(key) => void saveWorkLink(linkingWork.tab, key)}
          onCancel={() => setLinkingWork(undefined)}
        />
      )}

      {briefOf && (
        <BriefPanel
          // Another task's brief is another panel: it reads again, from nothing.
          key={"chat" in briefOf ? `chat:${briefOf.chat}` : `dispatch:${briefOf.dispatch}`}
          plane={plane}
          of={briefOf}
          onClose={() => setBriefOf(undefined)}
        />
      )}

      {answerOf && (
        <AnswerPanel
          // Another task's question is another dialog: it reads again, from nothing.
          key={answerOf.chat}
          plane={plane}
          of={answerOf}
          asking={chatsByNumber.get(answerOf.chat)?.from?.asking === true}
          onClose={() => setAnswerOf(undefined)}
        />
      )}

      {askingPersona && (
        <AskPersona
          // A new question is a new dialog: its boxes start from its own prefill.
          key={`${askingPersona.session}:${askingPersona.persona}`}
          persona={askingPersona.persona}
          chat={nameOf(askingPersona.session)}
          // Every workspace but the one that chat works in: there, "this chat's folder" is it.
          workspaces={(sidebar?.workspaces ?? [])
            .filter((ws) => !ws.chats.some((chat) => chat.session === askingPersona.session))
            .map((ws) => ws.name)}
          prefill={askingPersona.prefill}
          from={askingPersona.from}
          trouble={askingPersona.trouble}
          asking={askingPersona.busy}
          folderShared={folderShared}
          onAsk={(name, ask, place) => void sendAsk(name, ask, place)}
          onCancel={() => setAskingPersona(undefined)}
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
            setReport({
              from: `workspace.live:${liveAsk}`,
              refused: false,
              words: said.join(" "),
            });
            // Read the plane again: the marks come from what is on disk, not from this press.
            setReplan((asked) => asked + 1);
          }}
        />
      )}

      {membershipAsk !== undefined && (
        <RemoveFromWorkspace
          plane={plane}
          workspace={membershipAsk.workspace}
          repo={membershipAsk.repo}
          onClose={() => setMembershipAsk(undefined)}
          onDone={(said) => {
            setMembershipAsk(undefined);
            setReport({
              from: `absent.drop:${membershipAsk.repo}`,
              refused: false,
              words: said.join(" "),
            });
            // The list is what is on disk: read again, so the repo leaves "Not cloned here".
            rereadPanels();
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
      {/* NO-3's two questions: a chat's record dropped, and a chat started again. */}
      {forgetting && (
        <ChatAsk
          title={`Forget ${forgetting.name}?`}
          says={`purlis stops keeping ${forgetting.name}, and no later launch tries to start it again. This cannot be undone.`}
          answer="Forget chat"
          trouble={forgetting.trouble}
          busy={forgetting.busy}
          onAnswer={() => void forgetChat(forgetting.id, forgetting.name)}
          onCancel={() => setForgetting(undefined)}
          onCloseAutoFocus={afterForgetAsk}
        />
      )}
      {approving && (
        <ChatAsk
          title={`Approve ${approving.approval.profile}?`}
          says={`${approving.name} starts on the profile ${approving.approval.profile}. Approving records that purlis may run its command on this machine, and starts nothing: Retry now starts the chat.`}
          answer="Approve"
          trouble={approving.trouble}
          busy={approving.busy}
          onAnswer={() => void approveWaiting(approving)}
          onCancel={() => setApproving(undefined)}
          onCloseAutoFocus={afterApproveAsk}
        >
          {/* The picker's own sentence, line and mark (`ProfileApproval.tsx`, ruling V69). */}
          <ApprovalSentence row={approving.approval} />
          <ProfileMeta row={approving.approval} />
        </ChatAsk>
      )}
      {stopAsk && (
        <ChatAsk
          title={stopTitle(stopAsk)}
          says={stopSays(stopAsk)}
          answer={stopAnswer(stopAsk)}
          trouble={stopAsk.trouble}
          busy={stopAsk.busy}
          onAnswer={() => void stopAsked()}
          onCancel={() => setStopAsk(undefined)}
        />
      )}
      {stopAllAsk && (
        <ChatAsk
          title={stopAllTitle(stopAllAsk)}
          says={stopAllSays(stopAllAsk)}
          answer={stopAllAnswer(stopAllAsk)}
          trouble={stopAllAsk.trouble}
          busy={stopAllAsk.busy}
          onAnswer={() => void stopAllAsked()}
          onCancel={() => setStopAllAsk(undefined)}
        />
      )}
      {taskEndAsk && (
        <TaskEndAsk
          asked={taskEndAsk}
          onBelow={(belowToo) => setTaskEndAsk({ ...taskEndAsk, belowToo })}
          onAnswer={(way) =>
            void endTaskNow(
              taskEndAsk.session,
              way,
              taskEndAsk.belowToo && taskEndAsk.below.length > 0,
              taskEndAsk,
            )
          }
          onCancel={() => setTaskEndAsk(undefined)}
        />
      )}
      {freshening && (
        <ChatAsk
          title={`Start ${freshening.name} fresh?`}
          says={`The project's instructions changed since it started (${freshening.files.join(", ")}). Its program ends, and it starts again on what is there now, as a new conversation.`}
          answer="Start fresh"
          warns={oneChatMidTurn(freshening.name, freshening.doing)}
          trouble={freshening.trouble}
          busy={freshening.busy}
          onAnswer={() => void startFresh()}
          onCancel={() => setFreshening(undefined)}
        />
      )}
      {restartingTask && (
        <ChatAsk
          title={`Restart ${restartingTask.name}?`}
          says="It is a task. Its program ends and starts again on the same conversation, once its turn has ended. It stays a task, and still owes its report."
          answer="Restart it"
          busy={false}
          onAnswer={() => {
            const { session } = restartingTask;
            setRestartingTask(undefined);
            restartNow.current(session);
          }}
          onCancel={() => setRestartingTask(undefined)}
        />
      )}
      {endingChat && (
        <EndingChat
          offer={endingChat.offer}
          smart={endingChat.smart}
          running={endingChat.running}
          closing={endingChat.closing}
          keeps={endingChat.keeps}
          shows={endingChat.shows}
          back={endingChat.back}
          ownTabs={endingChat.ownTabs}
          onEnd={(stop) => {
            const { offer: ending, session } = endingChat;
            setEndingChat(undefined);
            // Stop them: the core closes the chat too, in the one step. Keep them: the
            // ordinary close.
            if (stop && session !== undefined) void closeStopping(session, "close");
            else void carryOut(ending);
          }}
          onSmartClose={(stop) => {
            const session = endingChat.session;
            setEndingChat(undefined);
            if (session === undefined) return;
            void (async () => {
              if (stop && !(await closeStopping(session, "smart_close"))) return;
              await beginSmartClose(session);
            })();
          }}
          onCancel={() => setEndingChat(undefined)}
        />
      )}

      {picking && (
        <StartChat
          options={picking.options}
          repo={pickingInRepo}
          starting={starting}
          prefer={"prefer" in picking.where ? picking.where.prefer : undefined}
          trouble={pickerTrouble}
          onStart={(profile, persona, footer, label, newBranch, withoutSandbox) =>
            void startPicked(profile, persona, footer, label, newBranch, withoutSandbox)
          }
          onApprove={(profile, persona, footer, shown, label, newBranch, withoutSandbox) =>
            void approveAndStart(profile, persona, footer, shown, label, newBranch, withoutSandbox)
          }
          // SD-30's install action: a shell tab at the project root with the command typed and
          // not run. The picker closes, since the chat it was for starts after the install.
          onInstall={
            root === undefined
              ? undefined
              : () => {
                  setPicking(undefined);
                  setPickerTrouble(undefined);
                  openShell(root, OUTSIDE, { sandboxInstall: true });
                }
          }
          fixing={pickerFixing}
          onFix={(id) => void fixInPicker(id)}
          // A refused profile is mended in Settings › Harness (NO-8): the picker closes, as it
          // does for the install, since the chat it was for starts after the mend.
          onOpenSettings={(group) => {
            setPicking(undefined);
            setPickerTrouble(undefined);
            openSettingsAt({ group });
          }}
          onCancel={() => {
            setPicking(undefined);
            setPickerTrouble(undefined);
          }}
        />
      )}
    </Lent>
  );
});

/**
 * What a project's window lends everything it draws: its chats' states (`ChatsHere`), the
 * chats a file can be handed to (`ReferenceChats`, FM-9), the way to open Ask {persona}
 * for one of its chats (`useAskPersona`), which a Notice on a pane calls, and the way to open
 * a task's brief (`useOpenBrief`, #1494).
 */
function Lent({
  chats,
  tasksBelow,
  doings,
  references,
  personas,
  askPersona,
  brief,
  answer,
  children,
}: {
  chats: ComponentProps<typeof ChatsHere.Provider>["value"];
  /** Each session's tasks, by its number (#1491). */
  tasksBelow: ReadonlyMap<number, TasksBelow>;
  /** What each working chat is doing, for the one line under its name (#1493). */
  doings: DoingsOf;
  references: ChatsForReferences;
  /** Every persona's mark here, and how to read them again (#1449). */
  personas: ReturnType<typeof usePersonaMarks>;
  askPersona: OpenAskPersona;
  /** Opens the Brief panel for a task (`useOpenBrief`, #1494). */
  brief: OpenBrief;
  /** Opens the Answer dialog for a task's question (`useOpenAnswer`, #1551). */
  answer: OpenAnswer;
  children: ReactNode;
}) {
  return (
    <ChatsHere.Provider value={chats}>
      <TasksBelowLent below={tasksBelow}>
        <DoingsHere.Provider value={doings}>
          <ReferenceChats.Provider value={references}>
            <PersonaMarks.Provider value={personas.marks}>
              <ReloadPersonaMarks.Provider value={personas.reload}>
                <AskPersonaOpener value={askPersona}>
                  <BriefOpener value={brief}>
                    <AnswerOpener value={answer}>{children}</AnswerOpener>
                  </BriefOpener>
                </AskPersonaOpener>
              </ReloadPersonaMarks.Provider>
            </PersonaMarks.Provider>
          </ReferenceChats.Provider>
        </DoingsHere.Provider>
      </TasksBelowLent>
    </ChatsHere.Provider>
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

/** A first chat asked for in a repository opened into a project (FR-4). */
export type FirstChat = {
  /** The workspace named after the repository. */
  workspace: string;
  /** The repo's clone in it, where the chat starts. */
  cwd: string;
  /** The one harness signed in on this machine, which the chat starts on without the picker;
   *  `null` when there is a choice to make. */
  harness: string | null;
  /** Whether no harness is installed on this machine: the first chat is then the harness
   *  setup tab instead of the picker (FR-29). */
  noneInstalled: boolean;
  /** How many of the repo's agent instruction files can go into the workspace's memory
   *  (FR-18a): offered in a tab beside the chat when there are any. */
  instructions: number;
  /** Which ask this is, so each is answered once. */
  at: number;
};

/**
 * The status line, with how many of the project's chats are running read off the store here
 * (SC-3): the count changes with most moves, and the project view is not redrawn for it.
 */
function StatusLineHere({
  sessions,
  settled,
  ...line
}: Omit<ComponentProps<typeof StatusLine>, "running"> & {
  /** The project's open chats, in the quit warning's order. */
  sessions: readonly number[];
  settled: boolean;
}) {
  const running = useChatsSelect(useChatsHere(), (states) =>
    runningIn({
      ending: sessions.map((session) => ({ state: stateOf(states, session) })),
      settled,
    }),
  );
  return <StatusLine {...line} running={running} />;
}

/** A chat a quit would end, before what it is doing is looked up (`PlaneReport.ending`). */
type Leaving = { session: number; chat: Omit<Ending, "state"> };

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
  /** The branch the explorer has picked in the workspace in front, or none: the narrowest
   *  scope ⌘P finds files in (FM-7). */
  branch?: Place;
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
  /** Opens the project switcher (FR-27). The window's: the palette and the projects are. */
  switchProject: () => void;
  closeProject: (plane: string) => Promise<Ran>;
  /** Moves a project into another window, or a new one (charter#126). The window's, because
   *  the window is what holds projects. */
  moveProject: (plane: string, to: string | null) => Promise<Ran>;
  /** Brings a project to the front and opens Settings at its Project level (SE-19). The
   *  window's, because the project may not be the one in front, and only the window can bring
   *  it there. */
  openSettings: (plane: string) => void;
  /** Brings a project to the front and opens its Saving tab (charter-app#294). */
  openSaving: (plane: string) => void;
  /** Opens the Settings tab (SE-16) on the project in front, at its focused level (SE-23), or
   *  draws it at You where the opener is when there is none. The window's, because which
   *  project is in front is. */
  openSettingsTab: () => void;
  /** Opens the Settings tab at the You level (SE-23): on the project in front, or where the
   *  opener is when there is none. */
  openYourSettings: () => void;
  /** Pinning a PROJECT is the window's, because the project strip is: a project that is not
   *  in front draws nothing, and its pin still has to be on that strip (ADR 0039). */
  pinProject: (plane: string, pinned: boolean) => Promise<Ran>;
  quit: () => void;
};

/** The size a session starts at. The pane it lands in tells it the real one at once. */
const STARTING_SIZE = { columns: 80, rows: 24 };

/** `record` without `key`. */
function withoutKey<T>(record: Readonly<Record<number, T>>, key: number): Record<number, T> {
  return Object.fromEntries(Object.entries(record).filter(([held]) => Number(held) !== key));
}

/** The core's sentence for a restart held back by a permission prompt, where it sent none. */
const NOT_YET = "It is waiting on a permission prompt, so it restarts once that is answered.";

/**
 * What a chat's pane says of its restart (#1342, #1428), whoever asked for it. One line at a
 * time: why the last one failed, else why it has not happened yet, else that it is under way,
 * else that it waits for the person.
 */
type RestartSaid = {
  /** The window owes it a restart: one was asked for, or a grant needs it, and it is not made. */
  owed?: boolean;
  /** Its harness reports no state, so the person restarts it when ready. */
  byHand?: boolean;
  /** The restart is under way. */
  running?: boolean;
  /** Why it is not restarted yet, in the core's sentence: it waits on a permission prompt. */
  notYet?: string;
  /** Why the last restart did not happen. */
  trouble?: string;
};

/**
 * **This chat is restarting** (#1428, S5): said on its pane while the core starts its new run,
 * in place of the line that asks the person to restart it by hand. Put away, it stays away for
 * this restart: it is drawn only while one is under way.
 */
function RestartingLine({ session }: { session: number }) {
  const [away, setAway] = useState(false);
  if (away) return null;
  return (
    <Notice
      cause={`restart-running:${session}`}
      at="pane"
      label="Restart"
      onDismiss={() => setAway(true)}
    >
      Restarting this chat…
    </Notice>
  );
}

/** What the core says when a handoff has opened a chat — `handoff::Arrived` in the app. */
type Arrived = {
  plane: string;
  session: number;
  name: string;
  /** The workspace whose strip it is filed on, or none for one at the project's root. */
  workspace: string | null;
  persona: string | null;
  /** The harness it runs, for its default name when it adopted no persona. */
  harness?: string | null;
  /** The task name the handoff gave it, which its tab says instead (charter-app#258). */
  label?: string | null;
  /** The chat that started it, by name, and that chat's workspace; and, where the core says
   *  (#1447), whether it arrives with a tab. */
  from?: (HandedFrom & { tab?: boolean }) | null;
};

/** A harness started by hand in a shell tab, as its banner needs it (ADR 0062). */
type ByHandNote = { harness: string; cwd: string | null };

/** The notes a relaunch or a Resume says about a chat, by the family of their cause. */
const CHAT_NOTES = ["chat-resumed", "chat-guessed", "chat-fresh"] as const;

/**
 * **The cause of one note about a chat**, or none when the chat has nothing to say of that kind.
 *
 * A resumed chat's names the conversation it was resumed by (D-NO2-9), so a dismissal of one
 * conversation's note never hides another's. A new chat's and a guessed one's name the chat
 * alone: the core says why a conversation was lost and what was guessed in words, and exposes no
 * id of the conversation that was lost to name the occurrence by.
 */
/** A resumed chat's note, for a chat that was resumed. */
const resumedNote = (chat: OpenChat) => chatNote("chat-resumed", chat) ?? "chat-resumed";

function chatNote(family: (typeof CHAT_NOTES)[number], chat: OpenChat): string | undefined {
  if (family === "chat-resumed")
    return chat.resumed ? `chat-resumed:${chat.session}:${chat.resumed}` : undefined;
  if (family === "chat-guessed") return chat.guessed ? `chat-guessed:${chat.session}` : undefined;
  return chat.fresh ? `chat-fresh:${chat.session}` : undefined;
}

/**
 * The strip a chat started in `cwd` is filed on until the plane says: the focused workspace's,
 * or the one for chats outside every workspace when it starts in no directory at all. One
 * function for a chat the picker started and a shell tab, so the two are filed alike.
 */
function filedFor(cwd: string | null, focused: string | undefined): string {
  return cwd === null ? OUTSIDE : (focused ?? OUTSIDE);
}

/**
 * **What a shell tab says when a harness is started by hand in it** (ADR 0062): that it runs
 * outside charter's session tracking, and a way to open it as a chat instead.
 *
 * In the pane's own top-left corner, over the terminal and taking no row — the property the
 * gauge keeps, for the operator's reason (a pane must not change height when charter has
 * something to say). A `status`, not an `alert`: nothing is wrong and nothing is waiting on
 * the operator, and the harness is already running whatever they press.
 */
function ByHandBanner({ note, onAnswer }: { note: ByHandNote; onAnswer: (open: boolean) => void }) {
  return (
    <Notice
      cause={`by-hand:${note.harness}`}
      at="pane"
      label={`${note.harness} started by hand`}
      fixes={[{ label: "Open as chat", onPress: () => onAnswer(true) }]}
      onDismiss={() => onAnswer(false)}
    >
      {note.harness} runs outside purlis&apos;s session tracking here.
    </Notice>
  );
}

/** What Open file says when no editor is chosen: the words the file tab says it in. */
const NO_EDITOR = "Choose your editor in Settings first.";
/** Where an editor is chosen: Settings › You › Editor, its one setting focused (SE-22). */
const CHOOSE_EDITOR: SettingsLink = { group: "you.editor", setting: "you.editor.yours" };

/** What a chat's start found to say (ADR 0085, V35): its lines, and the branches whose
 *  `AGENTS.md` they name as the operator's and hidden by charter's line (NO-4). */
type StartNotes = { notes: readonly string[]; agentsMd: readonly TheirAgentsMd[] };

/** A branch in a sentence or a button: the repo for its own folder, `repo/piece` for a piece.
 *  Every file one start names is in the same repository, so this tells them apart. */
const branchName = (at: TheirAgentsMd) => (at.piece === null ? at.repo : `${at.repo}/${at.piece}`);
/** A branch's identity: what the moved set is kept by. */
const branchKey = (at: TheirAgentsMd) => JSON.stringify([at.workspace, at.repo, at.piece]);

/**
 * **What a chat's start found to say** (ADR 0085, V35): why its `AGENTS.md` was not written,
 * or an `AGENTS.md` of the operator's that charter's exclude line hides. One line each, in the
 * pane's corner beside the by-hand banner and drawn the same way: a `status`, because nothing is
 * waiting on the operator and the chat is already running.
 *
 * **The operator's hidden file has its way out here** (NO-4, V91o): **Open file** hands it to
 * their editor, and **Move aside…** renames it to `AGENTS.aside.md` so `git status` shows it
 * again. It is the operator's file, so Move aside… asks first, in this same line, and the core
 * moves nothing until the second press (never over another file, and never charter's own).
 */
function StartNotice({
  plane,
  found,
  onDismiss,
}: {
  plane: PlaneId;
  found: StartNotes;
  onDismiss: () => void;
}) {
  /** The branch whose file Move aside… is asking about. */
  const [asking, setAsking] = useState<TheirAgentsMd>();
  /** What the actions answered: where a file went, or the core's refusal. */
  const [said, setSaid] = useState<string>();
  /** The branches whose file has been moved aside, by {@link branchKey}: nothing more to do. */
  const [moved, setMoved] = useState<readonly string[]>([]);
  const left = found.agentsMd.filter((at) => !moved.includes(branchKey(at)));
  const one = left.length === 1;

  const openFile = (at: TheirAgentsMd) => {
    const editor = yourEditor();
    if (editor === undefined) {
      setSaid(NO_EDITOR);
      return;
    }
    void commands
      .openTheirAgentsMd(plane, at.workspace, at.repo, at.piece, editor)
      .then((done) => setSaid(done.status === "error" ? done.error : undefined))
      .catch((err: unknown) => setSaid(String(err)));
  };
  const moveAside = (at: TheirAgentsMd) => {
    setAsking(undefined);
    void commands
      .moveTheirAgentsMdAside(plane, at.workspace, at.repo, at.piece)
      .then((done) => {
        if (done.status === "error") {
          setSaid(done.error);
          return;
        }
        setMoved((was) => [...was, branchKey(at)]);
        setSaid(
          `The AGENTS.md of ${branchName(at)} was moved to ${done.data}: git status shows it again.`,
        );
      })
      .catch((err: unknown) => setSaid(String(err)));
  };

  if (asking !== undefined)
    return (
      <Notice
        cause="start-found"
        at="pane"
        label="What this chat's start found"
        fixes={[
          { label: "Move aside", onPress: () => moveAside(asking) },
          { label: "Keep it", onPress: () => setAsking(undefined) },
        ]}
      >
        Move the AGENTS.md of {branchName(asking)} aside? purlis renames it to AGENTS.aside.md (or
        the next free AGENTS.aside-N.md), never over a file, and git status shows it again.
      </Notice>
    );

  const fixes = left.flatMap((at) => [
    { label: one ? "Open file" : `Open ${branchName(at)}`, onPress: () => openFile(at) },
    {
      label: one ? "Move aside…" : `Move ${branchName(at)} aside…`,
      onPress: () => {
        setSaid(undefined);
        setAsking(at);
      },
    },
  ]);
  const [first, ...rest] = fixes;
  return (
    <Notice
      cause="start-found"
      at="pane"
      label="What this chat's start found"
      fixes={first === undefined ? undefined : [first, ...rest]}
      // With no editor chosen, Open file's answer goes where one is chosen (#1244, SE-22).
      link={
        said === NO_EDITOR
          ? { label: "Choose your editor", onPress: () => askSettingsLink(plane, CHOOSE_EDITOR) }
          : undefined
      }
      onDismiss={onDismiss}
    >
      {/* Once every named file is moved aside, the lines about it are no longer true. */}
      {found.agentsMd.length > 0 && left.length === 0
        ? said
        : [found.notes.join(" "), said].filter(Boolean).join(" ")}
    </Notice>
  );
}

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

/** Where a chat in no workspace works, in the words its handoff note uses (`plane root`). */
const ROOT_WORD = "plane root";

/** The core's word for the same place, where a finished task worked there
 *  (`FinishedTask.place`). */
const CORE_ROOT_WORD = "project root";

/** No row of the Chats list is on screen: what the clock is read with until the list says. */
const NO_ROWS_DRAWN: ReadonlySet<number> = new Set();

/**
 * Whether a key landed where the person types text that is not a chat's: a field, a text box
 * or an editor. A terminal reads from a text box of its own, inside a pane that says the chat
 * has the keyboard there (`CHAT_KEYBOARD`), and is not one of these.
 */
function typedInto(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.closest(`[${CHAT_KEYBOARD}]`) !== null) return false;
  return target.isContentEditable || target.tagName === "INPUT" || target.tagName === "TEXTAREA";
}

/**
 * **The rows of a task's menu in the Chats list, for a pane to open the same menu on its
 * breadcrumb** (#1489): the catalogue's own rows, picked out by the ids that menu lists
 * (`actions.menuOn`), so the two cannot differ.
 */
function listedMenuOf(session: number, offerFor: (id: string) => Offer | undefined): Catalogued {
  const { above, below } = menuOn({ on: "listed", session });
  return new Map(
    [...above, ...below].flatMap((id) => {
      const offer = offerFor(id);
      return offer === undefined ? [] : [[id, offer] as const];
    }),
  );
}

/** What a tab with no tasks hands its chip, the same lists every time. */
const NO_ROWS: readonly ChatRow[] = [];
const NO_ENDED: readonly Ended[] = [];
const NO_NEEDS: readonly Needing[] = [];
/**
 * Whether the listing of `focus`'s workspace has answered without its branch (#1152): its repo's
 * branches were listed and it is not among them, or the workspace no longer holds the repo. A
 * listing still being read, or refused, has not said so. A focus on a repo's own folder is not
 * judged here: it is no branch (`focusStands`).
 */
function focusGone(state: WorkspaceState, focus: Place): boolean {
  if (focus.piece === null) return false;
  if (state.piecesRefused[focus.repo] !== undefined) return false;
  const repos = state.panels?.repos;
  if (repos !== undefined && !repos.includes(focus.repo)) return true;
  const listed = state.pieces[focus.repo];
  return listed !== undefined && !listed.some((one) => one.piece === focus.piece);
}

/** The queue the catalogue is built with: none (#1034, `withQueue`). */
const NO_QUEUE: readonly number[] = [];
const NO_ASKS: readonly Shown[] = [];

/**
 * **The chat tab `id` shows** (#1486): the task its session's tab is switched to, and otherwise
 * the tab's own chat. Nothing for a tab that opened on a view. What a thing dropped on the tab
 * is handed to, since the tab reads as the chat it shows.
 */
function chatShownBy(tabs: Tabs, id: number): number | undefined {
  return taskShownIn(tabs, id) ?? chatOf(tabs, id);
}

/**
 * **What each pane remembers of the task it shows** (#1486), by tab, pane and chat: the path it
 * can read now where it can, and otherwise the one it last could. Only for what a pane shows
 * now. Answers `was` itself when nothing changed, so it can be held as state.
 */
function recalledCrumbs(
  was: ReadonlyMap<string, Crumbs>,
  live: ReadonlyMap<string, Crumbs>,
  tabs: Tabs,
): ReadonlyMap<string, Crumbs> {
  const next = new Map<string, Crumbs>();
  for (const id of tabs.order)
    for (const one of shownIn(tabs, id)) {
      if (one.session === one.own) continue;
      const key = `${id}:${one.pane}:${one.session}`;
      const said = live.get(key) ?? was.get(key);
      if (said !== undefined) next.set(key, said);
    }
  const same =
    next.size === was.size &&
    [...next].every(([key, said]) => {
      const had = was.get(key);
      return (
        had !== undefined &&
        had.elsewhere === said.elsewhere &&
        had.path.length === said.path.length &&
        had.path.every((chat, at) => chat === said.path[at])
      );
    });
  return same ? was : next;
}

/** How long a pointer's press on the tab in front waits before it goes back to the main chat:
 *  long enough for a second click to make it a double-click, which renames. */
const A_DOUBLE_CLICK = 250;
/** How long after a drag a click on a tab is the drag's own, and not a press. */
const AFTER_A_DRAG = 400;

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
 * The chats a close row would end: a tab's every session, or the focused pane's one. Smart close
 * is offered only where this is exactly one (ADR 0064) — a record is one chat's.
 */
function chatsEndedBy(does: Offer["does"], tabs: Tabs, askedBy: AskedBy): number[] {
  // **A task is never one of them** (#1489): one beside its session goes on working when the
  // session's tab closes, unless the person answers that the session's tasks stop.
  if (does.verb === "closeTab")
    return panesOf(tabs, does.tab)
      .map((one) => one.session)
      .filter((session) => askedBy(session) === undefined);
  if (does.verb !== "closePane" || tabs.inFront === undefined) return [];
  const tab = tabs.byId[tabs.inFront];
  const going = panesOf(tabs, tab.id).find((one) => one.pane === tab.focused);
  return going === undefined ? [] : [going.session];
}

/** Why Smart close is not offered on a tab holding more than one chat. */
const MORE_THAN_ONE_CHAT =
  "This tab holds more than one chat, and a session record is one chat's. Smart close each from its own pane.";

/**
 * Whether a row is a close that says it ends nothing — a pane or a tab showing only a view.
 *
 * Asked the other way round from {@link endsAChat} on purpose: the danger look stays on every
 * close that has not said it is harmless, including a row the catalogue cannot run right now,
 * because a look that goes quiet when a row is merely unavailable would teach the operator the
 * wrong thing about the day it is available.
 */
function closesOnly(does: Offer["does"]): boolean {
  // A task sent back out of its tab or pane ends nothing either (#1489).
  if (does.verb === "sendBack") return true;
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
    // A new memory's tab holds nothing on disk yet, so there is nothing to bring back.
    if (isMemory(lead.view) && memoryRefOf(lead.view.key)?.slug === DRAFT) return [];
    // A Search tab is what the operator typed, never written to the record (FM-8).
    if (isSearch(lead.view)) return [];
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
        split: splitOf(tabs, lead.view) ?? null,
      },
    ];
  });
}

/** A chat tab's tooltip: where a handed-off chat came from, and the work item it works on. */
function tabTip(from: string | undefined, workItem: string | undefined): string | undefined {
  const lines = [from, workItem === undefined ? undefined : workItemSaid(workItem)].filter(
    (line): line is string => line !== undefined,
  );
  return lines.length === 0 ? undefined : lines.join("\n");
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
  crumbs,
  ending,
  crumbMenu,
  onShowChat,
  from,
  harness,
  onOpenCard,
  workItem,
  notices,
  asked,
  others,
  doing,
  children,
}: {
  plane: PlaneId;
  session: number;
  /** While the pane shows a task of its tab's session: the path to it (#1486). Also for a task
   *  in a pane of its own, and the session it is beside (#1489). */
  crumbs?: Crumbs;
  /** While it does: the two ways to end that task, beside the path (#1488). The only ending
   *  control a pane showing a task has. */
  ending?: ReactNode;
  /** The menu a right-click on a task's breadcrumb opens: its row's in the Chats list. */
  crumbMenu?: { session: number; offers: Catalogued; onPress: (offer: Offer) => void };
  /** Goes to a chat: what a name in the breadcrumb does, and a hidden chat's Notice. */
  onShowChat: (session: number) => void;
  /** Where a handed-off chat came from, `↳ from steward 3 · ops`, in the chat's own corner. */
  from?: string;
  /** The harness the chat runs, at a glance, in the same corner (HP-19): none for a shell. */
  harness?: HarnessGlance;
  /** Opens that harness's card tab. */
  onOpenCard: (glance: HarnessGlance) => void;
  /** The work item this chat works on, `Work item: <key>`, in the same corner (V60). */
  workItem?: string;
  /** What purlis has to say of the chat the pane shows (`ChatNotices`). */
  notices: ReactNode;
  /** What it asks for several of the pane's tasks at once (#1508). */
  asked?: ReactNode;
  /** And of every other chat that lives in this pane (#1486): each with whose it is. */
  others: readonly HiddenChat[];
  doing: ReactNode;
  children: ReactNode;
}) {
  // **Read off the store, by the pane itself** (SC-3): the gauge reads its record again when its
  // chat moves and keeps reading while the chat is mid-turn, and a move by another chat is no
  // reason to redraw this pane.
  const chats = useChatsHere();
  const moved = useChatsSelect(chats, (states) => movedAt(states, session));
  const running = useChatsSelect(chats, (states) => stateOf(states, session) === "running");
  const usage = useChatUsage(plane, session, moved, running);
  const lent = useReferenceChats();
  return (
    <div
      className="pane-frame"
      // A reference dropped on the chat's pane is typed into it, unsent (FM-9).
      onDragOver={overChat}
      onDrop={(event) => {
        const r = droppedReference(event);
        if (r === undefined || lent === undefined) return;
        event.preventDefault();
        lent.hand(r, session, "add");
      }}
    >
      {/* **The corners before the terminal, in the document**, although they are drawn over
          it: the tab order is the document's, and a terminal keeps Tab for its shell, so a
          control written after it is one Tab never reaches (charter-app#189). The pane's own
          controls are in its top corner, which is where reading order puts them anyway. */}
      <div className="pane-corner at-start">
        {/* The chat at a glance, on one row. */}
        <div className="pane-chips">
          {/* **Which chat this is, while the tab shows a task** (#1486): first in the line, and
              in no row of its own. */}
          {crumbs &&
            (crumbMenu === undefined ? (
              <PaneCrumbs crumbs={crumbs} onShow={onShowChat} />
            ) : (
              <Menued
                on={{ on: "listed", session: crumbMenu.session }}
                offers={crumbMenu.offers}
                onPress={crumbMenu.onPress}
              >
                <PaneCrumbs crumbs={crumbs} onShow={onShowChat} />
              </Menued>
            ))}
          {/* **Brief** (#1494): what the task on screen was sent. Right after the breadcrumb,
              before anything else the line holds for a task. */}
          {crumbs && <BriefButton of={briefOfShown(crumbs)} />}
          {crumbs && ending}
          <ChatGauge usage={usage} />
          {harness && <HarnessChip glance={harness} onOpen={() => onOpenCard(harness)} />}
          {from && <span className="pane-from">{from}</span>}
          {workItem && <span className="pane-work-item">{workItemSaid(workItem)}</span>}
        </div>
        <PaneNotices notices={notices} asked={asked} others={others} onShowChat={onShowChat} />
      </div>
      <div className="pane-corner at-end">{doing}</div>
      {children}
    </div>
  );
}

/** The task a breadcrumb's pane shows, as the Brief panel is asked for it: the last chat of
 *  the path, by its number. */
function briefOfShown(crumbs: Crumbs): BriefAsk {
  const shown = crumbs.path[crumbs.path.length - 1];
  return { chat: shown.session, name: shown.name };
}

/** A chat that lives in a pane and is not the one the pane shows, with what purlis has to say
 *  of it. */
type HiddenChat = {
  session: number;
  /** Whose its Notices are, as each says first: `steward 4`, or a task by its whole path,
   *  `deep (a task of steward 4 › talk)` (#1508). */
  whose: string;
  /** Whether it is a task below the pane's session, not the session's own chat (#1601). */
  task: boolean;
  notices: ReactNode;
};

/**
 * **What purlis has to say on a pane, one Notice under another** (#1481), inside the pane at
 * any width and scrolling when together they are taller than it.
 *
 * **Of every chat that lives in the pane, whichever one it shows** (#1486, the train-1 part of
 * V100-56). A session's tab shows one of its chats at a time: the session's own, or a task
 * below it. A Notice that waits for the person belongs to one of them, and the other may be on
 * screen. So the shown chat's Notices come first, as they always read, and after them every
 * other chat's, each saying whose it is and with **Go to it** (`NoticeOf`). An answer on one of
 * those acts on its own chat: the Notice is that chat's, drawn here.
 */
function PaneNotices({
  notices,
  asked,
  others,
  onShowChat,
}: {
  notices: ReactNode;
  /** The questions for several of the pane's tasks at once (#1508): after the shown chat's
   *  own, before each other chat's, since each waits for the person. */
  asked?: ReactNode;
  others: readonly HiddenChat[];
  onShowChat: (session: number) => void;
}) {
  return (
    <div className="pane-notices">
      {notices}
      {asked}
      {others.map((other) => (
        <NoticeOfChat
          key={other.session}
          session={other.session}
          whose={other.whose}
          task={other.task}
          onShowChat={onShowChat}
        >
          {other.notices}
        </NoticeOfChat>
      ))}
    </div>
  );
}

/** The Notices of one chat that is not on screen: each says whose it is and goes to it. */
function NoticeOfChat({
  session,
  whose,
  task,
  onShowChat,
  children,
}: {
  session: number;
  whose: string;
  task: boolean;
  onShowChat: (session: number) => void;
  children: ReactNode;
}) {
  const of = useMemo(
    () => ({ whose, task, onGo: () => onShowChat(session) }),
    [onShowChat, session, task, whose],
  );
  return <NoticeOf.Provider value={of}>{children}</NoticeOf.Provider>;
}

/**
 * **What purlis has to say of one chat, on a pane** (#1481).
 *
 * **The order is who is waiting on whom**: first the Notice that waits for the person's answer
 * before anything starts (a dispatch no grant covers), then what purlis found or refused, in
 * the order they arrived here. The thing to press is the first thing read, and the first Tab
 * stop.
 *
 * **One per chat, and never one chat's state under another** (#1486): a pane's chat changes
 * when its tab is switched, so this is drawn by its chat (`key`), and what a Notice was told
 * or had answered goes with the chat it was about.
 */
function ChatNotices({
  plane,
  session,
  byHand,
  onByHand,
  startNotes,
  onDismissStartNote,
  blocks,
  onDismissBlock,
  onRestart,
  onRestarted,
  onAllowed,
  restartSaid,
  onRestartAnswer,
  asks,
}: {
  plane: PlaneId;
  session: number;
  /** A harness started by hand in this shell tab, while its banner is up (ADR 0062). */
  byHand?: ByHandNote;
  onByHand: (open: boolean) => void;
  /** What this chat's start found to say, while it is up (ADR 0085). */
  startNotes?: StartNotes;
  onDismissStartNote: () => void;
  /** Asks for this chat's restart on its conversation, in this pane (#1362: Restart now). */
  onRestart: () => void;
  /** What this chat's sandbox blocked, newest last, while any is up (#1338). */
  blocks?: readonly ChatBlocked[];
  onDismissBlock: (block: ChatBlocked) => void;
  /** This chat started again in its place, without the sandbox, from a block's Notice (#1342). */
  onRestarted: (chat: OpenChat) => void;
  /** Something was allowed for this chat: it is owed a restart once its turn has ended. */
  onAllowed: () => void;
  /** What this pane says of this chat's restart (#1342, #1428), while there is something to say. */
  restartSaid?: RestartSaid;
  onRestartAnswer: (act: "now" | "dismiss") => void;
  /** The permission prompts this project's chats hold open on their hooks. */
  asks: readonly Shown[];
}) {
  const running = useChatsSelect(
    useChatsHere(),
    (states) => stateOf(states, session) === "running",
  );
  const newest = blocks?.[blocks.length - 1];
  return (
    <>
      {/* A dispatch to another persona that no grant covers (#1437): asked once, here. */}
      <DispatchGrantNotice plane={plane} session={session} />
      {/* Two of this chat's tasks in one folder with no branch of their own (#1511). */}
      <TasksSharingNotice plane={plane} session={session} />
      {/* A chat off screen stopped on its harness's permission prompt: said where the person is. */}
      <TaskPromptNotice session={session} asks={asks} />
      {byHand && <ByHandBanner note={byHand} onAnswer={onByHand} />}
      {startNotes && (
        <StartNotice plane={plane} found={startNotes} onDismiss={onDismissStartNote} />
      )}
      <PersonaGrantsNotice
        plane={plane}
        session={session}
        running={running}
        owed={restartSaid?.owed === true}
        onRestart={onRestart}
      />
      {newest !== undefined && (
        <SandboxBlockNotice
          key={`${newest.operation}:${newest.kind}:${newest.ours ? "ours" : "chat"}:${newest.target ?? ""}`}
          block={newest}
          more={(blocks?.length ?? 1) - 1}
          onDismiss={() => onDismissBlock(newest)}
          onAllowed={onAllowed}
          onRestarted={onRestarted}
        />
      )}
      <VaultRefusedNotice plane={plane} session={session} />
      {restartSaid?.trouble !== undefined && (
        <Notice
          cause={`restart:${session}`}
          at="pane"
          tone="trouble"
          label="Restart"
          fixes={[{ label: "Restart now", onPress: () => onRestartAnswer("now") }]}
          onDismiss={() => onRestartAnswer("dismiss")}
        >
          {restartSaid.trouble}
        </Notice>
      )}
      {restartSaid?.trouble === undefined && restartSaid?.notYet !== undefined && (
        <Notice
          cause={`restart-not-yet:${session}`}
          at="pane"
          label="Restart"
          onDismiss={() => onRestartAnswer("dismiss")}
        >
          {restartSaid.notYet}
        </Notice>
      )}
      {restartSaid?.running && restartSaid.notYet === undefined && (
        <RestartingLine session={session} />
      )}
      {restartSaid?.byHand && !restartSaid.running && restartSaid.notYet === undefined && (
        <Notice
          cause={`restart-by-hand:${session}`}
          at="pane"
          label="Restart"
          fixes={[{ label: "Restart now", onPress: () => onRestartAnswer("now") }]}
        >
          What you allowed reaches this chat when it restarts on the same conversation. Its harness
          does not say when a turn ends, so restart it when you are ready.
        </Notice>
      )}
    </>
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
   * **For the controls at the end of a strip, and nothing else**: the `+`, the project strip's
   * open and create, and its switcher (FR-27). `docs/design-system.md` says an
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
 * **The project strip draws three of these side by side, so no two may be the same glyph**
 * (charter-app#178). The third is the switcher's `ArrowLeftRight` (FR-27), first of the three:
 * two ways along the strip, which is what it does. `FolderPlus` is the folder-with-a-plus every file manager puts on *New
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
  // The title bar's switcher (FR-27): two ways along the strip, which is what it does.
  "project.switch": ArrowLeftRight,
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

/** The stop the person is being asked about (#1448): what was asked, of which chat, and how
 *  the answer is going. */
type StopAsking = StopAsked & { session: number; busy: boolean; trouble?: string };
type StopAllAsking = StopAllAsked & { session: number; busy: boolean; trouble?: string };

/** What a chat with no tab is called: what its tab would say, were it opened (#1447). */
function untabbedName(chat: OpenChat): string {
  const who = whoOf(chat.persona, chat.harness);
  return chat.label ?? (who ? `${who} ${chat.name}` : chat.name);
}

/** The tab that holds chat `session`, in any of its panes, where one does. */
function tabHolding(tabs: Tabs, session: number): number | undefined {
  return tabs.order.find((id) => panesOf(tabs, id).some((pane) => pane.session === session));
}

/**
 * **What a chat is called in every list** (#1484): the name of the tab holding it, and for a
 * chat with no tab, what its tab would say. The Chats list, the explorer and the needs-you
 * list all read this, so a task is one name everywhere and never its number.
 */
function shownName(tabs: Tabs, chat: OpenChat): string {
  const held = tabHolding(tabs, chat.session);
  // **A task beside its session is not called what the session's tab is** (#1489): the tab's
  // name is its own chat's. A task says its own name wherever it is drawn.
  const beside =
    held !== undefined && chat.from?.task === true && chatOf(tabs, held) !== chat.session;
  return held !== undefined && !beside ? tabs.byId[held].name : untabbedName(chat);
}

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
  // **A task's own tab has a minimise where a session's has its close** (#1489, V100-38): `−`,
  // named for what it does, "Send talk back into steward 4's tab". It ends nothing.
  const minimise = offer.does.verb === "sendBack";
  return (
    <button
      // A tab showing only a view closes and ends nothing, so its `×` does not wear the danger
      // hover a chat's does — the look may not say more than the act does, either way.
      className={clsx("closer", closesOnly(offer.does) && "keeps", minimise && "minimise")}
      data-minimise={minimise ? "" : undefined}
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
      {minimise ? <Minus /> : <X />}
    </button>
  );
}

/**
 * **A tab's settings gear** (SE-23, #1173; V89i): the same catalogue row its right-click menu
 * and the palette run — `project.settings:<plane>` on the project in front's tab,
 * `workspace.settings:<name>` on the focused workspace's — drawn as a gear, so its accessible
 * name and its tooltip are the row's words and the glyph is only the glyph, as `Closer`'s `×` is.
 *
 * **Quiet**: drawn on one tab per strip, the one you are on, and invisible there until that tab
 * is under the pointer or the keyboard (`App.css`, `.gear`). No gear is always on, and the title
 * bar has no settings button of its own — the operator's "it must not be noisy".
 *
 * **A Tab stop, unlike `Closer`.** A `×` per tab in the sequence would be fifty stops; this is
 * one, right after the strip's one stop, because only the tab you are on carries one.
 */
export function Gear({ offer, onPress }: { offer?: Offer; onPress: (offer: Offer) => void }) {
  if (!offer) return null;
  return (
    <button
      className="gear"
      // WebKit leaves a `<button>` out of the tab sequence unless its `tabindex` is written down
      // (`docs/ui-primitives.md`, charter-app#189).
      tabIndex={0}
      disabled={!offer.available}
      aria-label={offer.title}
      title={offer.reason || offer.title}
      onClick={() => onPress(offer)}
    >
      <SettingsMark />
    </button>
  );
}

/** A tab's layout, as panes with a handle between each split. */
function LayoutPanes({
  plane,
  layout,
  crumbs,
  placed,
  away,
  finished,
  explaining,
  others,
  asked,
  asks,
  closeOf,
  onBack,
  onShowChat,
  focused,
  onFocus,
  offerFor,
  onPaneDoes,
  name,
  handedFrom,
  glances,
  workItems,
  byHand,
  onByHand,
  startNotes,
  onDismissStartNote,
  onRestartChat,
  blocks,
  onDismissBlock,
  onRestarted,
  onAllowed,
  restartsSaid,
  onRestartAnswer,
  offered,
  onOpenView,
  onAsk,
  onVaultChanged,
  memory,
  firstTask,
  split,
}: {
  /** Which plane's sessions these panes are showing. A session number belongs to a plane,
   *  and every command a pane makes carries it. */
  plane: PlaneId;
  /** The tab's layout as it is drawn: each pane holding the chat it shows (#1486). */
  layout: Layout;
  /** The breadcrumb of each pane that shows a task, by pane (#1486). */
  crumbs: Readonly<Record<number, Crumbs>>;
  /** The breadcrumb of each pane that shows its own chat and still has a path to say (#1489):
   *  a task in a pane of its own, and the session it is beside. */
  placed: Readonly<Record<number, Crumbs>>;
  /** The session of the tab whose chip the first dispatch explains, on its own pane (#1501). */
  explaining?: ChipToExplain;
  /** The panes that show a task they cannot draw, by pane, and why (`TaskAway`). */
  away: Readonly<Record<number, Away>>;
  /** The finished row of each task a pane was left on when it ended, by pane (#1485). */
  finished: Readonly<Record<number, FinishedTask>>;
  /** The chats that live in each pane other than the one it shows, by pane. */
  others: Readonly<Record<number, readonly { session: number; whose: string; task: boolean }[]>>;
  /** What each pane asks for several of its tasks at once, by pane (#1508). */
  asked: Readonly<Record<number, ReactNode>>;
  /** The permission prompts this project's chats hold open on their hooks, for the Notice of
   *  a chat off screen that is stopped on one. */
  asks: readonly Shown[];
  /** The close of pane `pane`, decided for that pane and not for the one in focus. */
  closeOf: (pane: number) => Offer | undefined;
  /** Pane `pane` goes back to its session's own chat. */
  onBack: (pane: number) => void;
  /** Goes to a chat: what a name in a breadcrumb does. */
  onShowChat: (session: number) => void;
  focused: number;
  onFocus: (pane: number) => void;
  /** The catalogue, by row id. There is one list of actions and the panes read it too. */
  offerFor: (id: string) => Offer | undefined;
  onPaneDoes: (pane: number, offer: Offer | undefined) => void;
  /** The tab's name, which is the title of the view it opened on. */
  name: string;
  /** Where each handed-off chat came from, by session (charter-app#258). */
  handedFrom: Readonly<Record<number, string>>;
  /** Each chat's harness card, at a glance, by session (HP-19). */
  glances: Readonly<Record<number, HarnessGlance>>;
  /** The work item each chat works on, by session (V60). */
  workItems: Readonly<Record<number, string>>;
  /** A harness started by hand in a shell tab, by session, for its banner (ADR 0062). */
  byHand: Readonly<Record<number, ByHandNote>>;
  /** The banner answered: `open` asks for that harness as a chat, else it is put away. */
  onByHand: (session: number, open: boolean) => void;
  /** What each chat's start found to say, by session (ADR 0085). */
  startNotes: Readonly<Record<number, StartNotes>>;
  onDismissStartNote: (session: number) => void;
  /** Starts chat `session` again, resuming its conversation, in its pane (#1362). */
  onRestartChat: (session: number) => void;
  /** What each chat's sandbox blocked, by session (#1338). */
  blocks: Blocks;
  onDismissBlock: (session: number, block: ChatBlocked) => void;
  /** A chat started again in its place from a sandbox block's Notice (#1342): `was` is its old
   *  session. */
  onRestarted: (was: number, chat: OpenChat) => void;
  /** Something was allowed for chat `session`: it is owed a restart (#1342). */
  onAllowed: (session: number) => void;
  /** What each chat's pane says of its restart, by session (#1342, #1428). */
  restartsSaid: Readonly<Record<number, RestartSaid>>;
  onRestartAnswer: (session: number, act: "now" | "dismiss") => void;
  /** The views approved extensions offer, for the buttons a view draws beside itself. */
  offered: readonly ExtensionView[];
  onOpenView: (view: ViewRef, title: string) => void;
  /** The operator pressed to have a waiting view in this pane asked. */
  onAsk: (pane: number) => void;
  /** A vault's tab wrote to its vault: the plane's vault list is read again. */
  onVaultChanged: () => void;
  /** What charter's own views re-read on, and what a memory's tab calls when it wrote or asks
   *  to close (SI-9b). */
  memory: {
    changed: number;
    onSaved: (from: ViewRef, memory: MemoryView) => void;
    onClose: (view: ViewRef) => void;
    /** A view that now shows something else: a Search tab asking a new query (FM-8). */
    showInstead: (from: ViewRef, to: ViewRef, title: string) => void;
  };
  /** What the first task's tab asks the plane to do (FR-28). */
  firstTask: FirstTaskDoes;
  /** Where a view's divider was, and where the operator moved it to (FM-2). */
  split: {
    of: (view: ViewRef) => number | undefined;
    moved: (view: ViewRef, split: number) => void;
  };
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
            <PaneDoing
              pane={layout.pane}
              offerFor={offerFor}
              closeOf={closeOf}
              onPaneDoes={onPaneDoes}
            />
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
              changed={memory.changed}
              onMemorySaved={memory.onSaved}
              onCloseView={memory.onClose}
              onShowInstead={memory.showInstead}
              firstTask={firstTask}
              split={split.of(content.view)}
              onSplit={(to) => split.moved(content.view, to)}
            />
          </div>
        </div>
      );
    }
    /** What purlis has to say of chat `session`, on this pane. By the chat, so no Notice's
     *  state is carried from one chat to another when the tab is switched (#1486). */
    const noticesOf = (session: number) => (
      <ChatNotices
        key={session}
        plane={plane}
        session={session}
        byHand={byHand[session]}
        onByHand={(open) => onByHand(session, open)}
        startNotes={startNotes[session]}
        onDismissStartNote={() => onDismissStartNote(session)}
        onRestart={() => onRestartChat(session)}
        blocks={blocks[session]}
        onDismissBlock={(block) => onDismissBlock(session, block)}
        onRestarted={(chat) => onRestarted(session, chat)}
        onAllowed={() => onAllowed(session)}
        restartSaid={restartsSaid[session]}
        onRestartAnswer={(act) => onRestartAnswer(session, act)}
        asks={asks}
      />
    );
    const hidden: HiddenChat[] = (others[layout.pane] ?? []).map((other) => ({
      ...other,
      notices: noticesOf(other.session),
    }));
    // **This pane shows a task of its tab's session** (#1486). The breadcrumb says where it
    // came from, so the note of the chat that started it is not said a second time.
    const crumb = crumbs[layout.pane];
    // **Or its own chat, with a path to say** (#1489): a task in a pane of its own, and the
    // session it is beside. The same line, so each side of a split says which chat it is.
    const path = crumb ?? placed[layout.pane];
    const doing = (
      <PaneDoing
        pane={layout.pane}
        offerFor={offerFor}
        closeOf={closeOf}
        onPaneDoes={onPaneDoes}
        // No close on a pane showing a task: nothing on screen ends the task, and the
        // session's close is its tab's (#1486).
        task={crumb !== undefined || away[layout.pane] !== undefined}
        // The task it shows inside its session's tab, which can be given a tab of its own or
        // opened beside its session from here (#1489).
        shows={crumb === undefined ? undefined : content.session}
      />
    );
    const gone = away[layout.pane];
    if (gone !== undefined) {
      // **A task this pane shows and cannot draw** (#1486): no terminal, so nothing typed
      // reaches a chat, and the pane says what it shows and why, with the way back.
      if (gone.why === "unread") return <div className="pane-frame" />;
      // How it ended, as its finished row says it where it has one: one word for one end.
      const row = finished[layout.pane];
      const ended = row === undefined ? endedState(gone.crumbs) : shownOf(row);
      const last = gone.crumbs.path[gone.crumbs.path.length - 1].session;
      return (
        <div className="pane-frame" onPointerDown={() => onFocus(layout.pane)}>
          <div className="pane-corner at-start">
            <div className="pane-chips">
              <PaneCrumbs
                crumbs={gone.crumbs}
                onShow={onShowChat}
                // One that is still running says what it is doing; one that ended, how.
                state={
                  gone.why === "ended" && ended !== undefined ? (
                    <StateShown shown={ended} />
                  ) : undefined
                }
                gone={(session) => !hidden.some((other) => other.session === session)}
              />
              {/* A task that has ended is read by its finished row's record (#1494), and has
                  no Brief here once that row is gone; one still running, by its chat. */}
              {(gone.why !== "ended" || row !== undefined) && (
                <BriefButton
                  of={
                    gone.why === "ended" && row !== undefined
                      ? { dispatch: row.id, name: row.name }
                      : briefOfShown(gone.crumbs)
                  }
                />
              )}
            </div>
            <PaneNotices
              notices={null}
              asked={asked[layout.pane]}
              others={hidden}
              onShowChat={onShowChat}
            />
          </div>
          <div className="pane-corner at-end">{doing}</div>
          <TaskAway
            away={gone}
            focused={layout.pane === focused}
            report={row?.report}
            finished={row && { state: ended, more: qualifierOf(row) }}
            onBack={() => onBack(layout.pane)}
            onOpenAlone={() => onShowChat(last)}
          />
        </div>
      );
    }
    return (
      <PaneFrame
        plane={plane}
        session={content.session}
        crumbs={path}
        // **The two ways to end the task this pane shows**, beside its path (#1488): on a
        // session's pane switched to a task, and on a task's own pane, in a tab of its own or
        // beside its session (#1489). Never on the session's own one-name path beside a task.
        ending={
          path !== undefined &&
          path.path[path.path.length - 1].mode === "task" && (
            <TaskEnds
              session={path.path[path.path.length - 1].session}
              stop={offerFor(taskStopId(path.path[path.path.length - 1].session))}
              close={offerFor(taskCloseId(path.path[path.path.length - 1].session))}
              // Keep gives the keyboard back to the task's terminal.
              focusBack={() => giveKeyboardTo(plane, content.session)}
            />
          )
        }
        // Right-click on a task's breadcrumb is its row's menu in the Chats list (#1489).
        crumbMenu={
          path !== undefined && path.path[path.path.length - 1].mode === "task"
            ? {
                session: path.path[path.path.length - 1].session,
                offers: listedMenuOf(path.path[path.path.length - 1].session, offerFor),
                onPress: (offer) => onPaneDoes(layout.pane, offer),
              }
            : undefined
        }
        onShowChat={onShowChat}
        from={path === undefined ? handedFrom[content.session] : undefined}
        harness={glances[content.session]}
        onOpenCard={(glance) => onOpenView(harnessCardView(glance.name), glance.label)}
        workItem={workItems[content.session]}
        notices={
          <>
            {noticesOf(content.session)}
            {/* The first dispatch on this machine explains the chip, on the session's own
                pane (#1501). */}
            {path === undefined && (
              <ChipExplained plane={plane} session={content.session} chip={explaining} />
            )}
          </>
        }
        asked={asked[layout.pane]}
        others={hidden}
        doing={doing}
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
              crumbs={crumbs}
              placed={placed}
              away={away}
              finished={finished}
              explaining={explaining}
              others={others}
              asked={asked}
              asks={asks}
              closeOf={closeOf}
              onBack={onBack}
              onShowChat={onShowChat}
              focused={focused}
              onFocus={onFocus}
              offerFor={offerFor}
              onPaneDoes={onPaneDoes}
              name={name}
              handedFrom={handedFrom}
              glances={glances}
              workItems={workItems}
              byHand={byHand}
              onByHand={onByHand}
              startNotes={startNotes}
              onDismissStartNote={onDismissStartNote}
              onRestartChat={onRestartChat}
              blocks={blocks}
              onDismissBlock={onDismissBlock}
              onRestarted={onRestarted}
              onAllowed={onAllowed}
              restartsSaid={restartsSaid}
              onRestartAnswer={onRestartAnswer}
              offered={offered}
              onOpenView={onOpenView}
              onAsk={onAsk}
              onVaultChanged={onVaultChanged}
              memory={memory}
              firstTask={firstTask}
              split={split}
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
  closeOf,
  onPaneDoes,
  task = false,
  shows,
}: {
  pane: number;
  offerFor: (id: string) => Offer | undefined;
  /** This pane's close, which is not the focused pane's where the two differ (#1486): a pane
   *  beside one that shows a task still closes. For a task's own pane it is the minimise
   *  (#1489), which sends the task back and ends nothing. */
  closeOf: (pane: number) => Offer | undefined;
  onPaneDoes: (pane: number, offer: Offer | undefined) => void;
  /** The pane shows a task of its tab's session (#1486): it draws no close. A close here
   *  would read as ending the task, and what it would end is the session under it. */
  task?: boolean;
  /** That task, where the pane can draw it: the pane offers it a tab of its own, and a pane
   *  beside its session (#1489). The task's own rows, not the focused pane's. */
  shows?: number;
}) {
  // **A task's own pane keeps its minimise whatever it shows** (#1489): switched to a task of
  // its own, it is still the pane that is sent back. Only a close is not drawn over a task.
  const minimise = closeOf(pane)?.does.verb === "sendBack";
  const rows = [
    ...(shows === undefined ? [] : [ownTabId(shows), besideId(shows)]),
    "pane.split.right",
    "pane.split.down",
    ...(task && !minimise ? [] : ["pane.close"]),
  ];
  // **The pane's top line ends before these controls, whatever their number** (#1486): their
  // width is measured and said to the pane's frame (`--pane-controls`, `App.css`), so the line
  // reserves the room the controls drawn here take, not a count somebody remembered.
  const at = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const controls = at.current;
    const frame = controls?.closest<HTMLElement>(".pane-frame");
    if (!controls || !frame) return;
    const say = () => {
      const width = controls.offsetWidth;
      // Nothing is laid out (a test's document): the stylesheet's own figure stands.
      if (width > 0) frame.style.setProperty("--pane-controls", `${width}px`);
    };
    say();
    if (typeof ResizeObserver === "undefined") return;
    const watching = new ResizeObserver(say);
    watching.observe(controls);
    return () => watching.disconnect();
  }, [rows.length]);
  return (
    <div className="pane-doing" ref={at}>
      {rows.map((id) => {
        const offer = id === "pane.close" ? closeOf(pane) : offerFor(id);
        if (!offer) return null;
        const Mark = markOf(offer);
        return (
          <button
            key={id}
            className={
              offer.id === "pane.close" && !closesOnly(offer.does) ? "ends-a-chat" : undefined
            }
            // A task's minimise, as its own tab draws it (#1489): never the close's look.
            data-minimise={offer.does.verb === "sendBack" ? "" : undefined}
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

/**
 * The glyph a row is drawn as where it is drawn as one: by what it does for the rows about
 * where a task is (#1489), which are one per task, and by its id otherwise ({@link MARKS}).
 * **A minimise is `−`, whatever id it has**: a task's own tab and its own pane keep the close's
 * id, so its key and its menu reach it, and must never draw the close's `×`.
 */
function markOf(offer: Offer): typeof Plus | undefined {
  switch (offer.does.verb) {
    case "sendBack":
      return Minus;
    case "ownTab":
      return SquareArrowOutUpRight;
    case "beside":
      return PanelRight;
    default:
      break;
  }
  if (offer.id.startsWith("chat.own:")) return SquareArrowOutUpRight;
  if (offer.id.startsWith("chat.beside:")) return PanelRight;
  return MARKS[offer.id];
}

/** The waiting chat's Notice's way out that asks first (NO-3), and where a Cancel goes back to. */
const FORGET_THIS_CHAT = "Forget this chat…";
/** The waiting chat's Notice's way to its profile's approval question (#1246), and where a
 *  Cancel goes back to. */
const REVIEW_AND_APPROVE = "Review and approve…";

/** A task's Restart chat, asked first where it was pressed (#1489, #1462): the question, and
 *  the answer that restarts it. */
function restartAsked(name: string) {
  return {
    way: "now" as const,
    says: `Restart ${name}? Its program ends and starts again on the same conversation, once its turn has ended. It stays a task, and still owes its report.`,
    answer: "Restart it",
    busy: false,
    act: "restart" as const,
  };
}

/**
 * **How many times one of `sessions` has moved to a state that is not mid-turn** (#1468): a
 * turn ending, a task ending or failing. A turn beginning is not counted, so a read that
 * follows this is not made again for what cannot have changed its answer.
 */
function useTurnsEnded(chats: ReturnType<typeof useChatsHere>, sessions: readonly number[]) {
  const [ended, setEnded] = useState(0);
  useEffect(() => {
    const was = new Map<number, string>();
    const read = () => {
      const states = chats.store.statesFor(chats.plane);
      let moved = false;
      for (const session of sessions) {
        const state = stateOf(states, session);
        const before = was.get(session);
        was.set(session, state);
        if (before !== undefined && before !== state && state !== "running") moved = true;
      }
      if (moved) setEnded((n) => n + 1);
    };
    read();
    return chats.store.subscribe(read);
  }, [chats, sessions]);
  return ended;
}
