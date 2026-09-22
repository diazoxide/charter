import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import clsx from "clsx";
import { Group, Panel, Separator } from "react-resizable-panels";
import * as Menu from "@radix-ui/react-dropdown-menu";
import {
  ChevronDown,
  FolderPlus,
  Pin as PinMark,
  Plus,
  SquareSplitHorizontal,
  SquareSplitVertical,
  X,
} from "lucide-react";
import {
  commands,
  type AtRisk,
  type ChatWorktree,
  type OpenChat,
  type PlaneId,
  type Sidebar as SidebarModel,
  type StartOptions,
} from "./bindings";
import {
  catalogue,
  OUTSIDE,
  OUTSIDE_TITLE,
  perform,
  PASS_THROUGH_BYTES,
  PASS_THROUGH_KEY,
  type Doing,
  type Offer,
  type Project,
  type Ran,
} from "./actions";
import { DeleteWorkspace } from "./DeleteWorkspace";
import { Menued } from "./Menus";
import { NewWorkspace } from "./NewWorkspace";
import { StartChat } from "./StartChat";
import { SessionPane } from "./SessionPane";
import { Explorer, type Spot } from "./Explorer";
import { BottomBar } from "./BottomBar";
import { useWorkspaceState } from "./workspaceState";
import { inSlots, SIDES, useArrangement } from "./regions";
import { RegionFrame, RegionToggle } from "./RegionFrame";
import { useDoctor } from "./Doctor";
import { PaneGauge } from "./ChatGauge";
import { usePin, useUpdates } from "./Updates";
import { StatusLine, type Alerts } from "./StatusLine";
import {
  byLastActivity,
  closeFocusedPane,
  closeTab,
  focusPane,
  noTabs,
  openTab,
  panesOf,
  selectTab,
  showWorkspace,
  splitFocusedPane,
  tabsIn,
  workspaceOf,
  type Direction,
  type FiledIn,
  type LastMoved,
  type Layout,
  type Pinned,
  type Tabs,
} from "./tabs";
import { ChatState } from "./NeedsYou";
import { EndingChat } from "./EndingChat";
import { Panels } from "./Panels";
import { movedAt, quietOnes, stateOf, useChatStates, type ChatStates } from "./chatState";
import { fitting, LEAST, useRoom } from "./fits";
import type { Ending } from "./QuitWarning";

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
}) {
  const [tabs, setTabs] = useState<Tabs>(noTabs);
  /** What every chat is doing, in THIS project. Pushed from the core; nothing here polls.
   *  It keeps listening while the project is behind another one, which is what lets its tab
   *  say that something over there needs you. */
  const states = useChatStates(plane);
  const [trouble, setTrouble] = useState<string>();
  const [sidebar, setSidebar] = useState<SidebarModel>();
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
    where: { tab: true } | { split: Direction };
  }>();
  /** Why the last start did not happen, shown in the picker rather than behind it. */
  const [pickerTrouble, setPickerTrouble] = useState<string>();
  /** The one line to say when charter wired a profile on its way to starting a chat. */
  const [wired, setWired] = useState<string>();
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
  /** Whether the new-workspace dialog is up, why the last attempt made nothing, and whether
   *  charter is making one right now. */
  const [makingWorkspace, setMakingWorkspace] = useState(false);
  const [workspaceTrouble, setWorkspaceTrouble] = useState<string>();
  const [busyMaking, setBusyMaking] = useState(false);
  /**
   * The workspace the operator is being asked about deleting, if any.
   *
   * `atRisk` is `undefined` until the core has answered and is drawn as "still reading" — an
   * empty list and an unanswered question are the two states this must never merge, because
   * one of them says "nothing would be lost". `refusal` is the sentence the core gave the last
   * time Delete was pressed, and its presence is the only thing that makes forcing reachable.
   */
  const [removing, setRemoving] = useState<{
    workspace: string;
    atRisk?: AtRisk[];
    unreadable?: string;
    refusal?: string;
    busy: boolean;
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
  /** The tab that was in front on each workspace's strip, so coming back to a workspace
   *  comes back to the chat that was on screen there rather than to its first. */
  const lastFront = useRef<Record<string, number>>({});
  /**
   * What this operator has pinned in this project (charter ADR 0039).
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
   * The spot the explorer has picked, and the workspace it was picked in.
   *
   * Both together, so that moving to another workspace goes back to that workspace's own
   * directory rather than leaving the next chat pointed at a piece of the workspace the
   * operator has just left. Derived below rather than cleared in an effect: a `setState`
   * from inside an effect is a second render, and the answer is already here.
   */
  const [pickedSpot, setPickedSpot] = useState<{ workspace: string; spot: Spot }>();
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
        const front = next.inFront === undefined ? undefined : next.byId[next.inFront];
        const pane = front && panesOf(next, front.id)[0];
        void commands.chatInFront(plane, pane ? pane.session : null).catch(() => undefined);
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
    void commands
      .openedChats(plane)
      .then((answer) => {
        setSettled(true);
        void commands
          // A window that cannot ask, or is answered with nothing, simply says nothing.
          .chatsThatWouldNotStart(plane)
          .then((trouble) => setWouldNotStart(trouble.status === "ok" ? (trouble.data ?? []) : []))
          .catch(() => undefined);
        const open = answer.status === "ok" ? (answer.data ?? []) : [];
        if (open.length === 0) return;
        setReopened(open);
        // A pinned chat comes back pinned: the pin rides the record it came back from.
        setPinnedChats(open.filter((chat) => chat.pinned).map((chat) => chat.session));
        // The persona comes with the chat, so a tab put back reads `3 steward` from its first
        // frame rather than reading `3` until the sidebar has been read (charter-app#130).
        const drawn = open.reduce(
          (tabs, chat) => openTab(tabs, chat.session, chat.name, chat.persona),
          noTabs(),
        );
        const front = open.findIndex((chat) => chat.in_front);
        change(() => (front < 0 ? drawn : selectTab(drawn, drawn.order[front])));
      })
      // Nothing open is the ordinary first launch, and a window that cannot ask is still
      // a window the operator can open a chat in.
      .catch(() => setSettled(true));
  }, [change, plane]);

  // The sidebar is read from the plane, and re-read whenever the chats change: the plane is a
  // directory the operator also edits by hand and another charter process writes, so there is
  // nothing to invalidate a cache of it. `tabs` is the dependency because opening or ending a
  // chat is what this window can change about the answer.
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
        const session = front === undefined ? undefined : panesOf(now.current, front)[0]?.session;
        const ofFront = session === undefined ? undefined : here(session);
        const stands = (name: string) =>
          name === OUTSIDE
            ? next.unfiled.length > 0 || ofFront === OUTSIDE
            : next.workspaces.some((ws) => ws.name === name);
        setPicked((current) =>
          current !== undefined && stands(current)
            ? current
            : (ofFront ?? next.workspaces[0]?.name),
        );
      })
      // A window with no readable plane still runs its panes; the header already says so.
      .catch(() => setSidebar(undefined));
  }, [plane, replan, startedIn, tabs]);

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
      // A window that cannot ask draws nothing pinned, which is the plane's own order — the
      // arrangement an operator who has pinned nothing already has.
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
      const session = panesOf(tabs, id)[0]?.session;
      return session !== undefined && pinnedChats.includes(session);
    },
    [pinnedChats, tabs],
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
  const workspaceState = useWorkspaceState(plane, ofWorkspace);
  /** What `charter doctor` says about this project, run inside the app: the preflight when
   *  the project opens, the full doctor when the operator opens it (`Doctor.tsx`). */
  const doctor = useDoctor(plane);
  /** The updater's offer (the whole app's) and this plane's pin (`Updates.tsx`). */
  const updates = useUpdates();
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
    const listed = workspaceState.pieces[pickedSpot.spot.repo];
    if (listed !== undefined && !listed.some((one) => one.piece === pickedSpot.spot.piece))
      return undefined;
    return pickedSpot.spot;
  }, [ofWorkspace, pickedSpot, workspaceState.pieces]);

  // Where a chat starts: **the spot the explorer picked**, and the focused workspace's own
  // directory when nothing is picked — so the sidebar can file it under that workspace.
  // Nothing on the plane records a chat, so where it works is the only thing relating the
  // two, and a piece of a workspace is still in that workspace. Null — the operator's
  // home — until a plane is read.
  const startIn = spot?.path ?? sidebar?.workspaces.find((ws) => ws.name === focused)?.path ?? null;

  /** What the explorer picks, remembered against the workspace it was picked in. */
  const pickSpot = useCallback(
    (next: Spot | undefined) => {
      if (ofWorkspace === undefined) return;
      setPickedSpot(next === undefined ? undefined : { workspace: ofWorkspace, spot: next });
    },
    [ofWorkspace],
  );

  /** The workspaces the strip shows: this project's, plus the one for chats outside them all
   *  when there are any. In the plane's own order, which is the sidebar's. */
  const strips = useMemo(() => {
    if (sidebar === undefined) return [];
    const names = sidebar.workspaces.map((ws) => ws.name);
    const stray =
      sidebar.unfiled.length > 0 ||
      tabs.order.some((id) => workspaceOf(tabs, id, filedIn) === OUTSIDE);
    const all = stray ? [...names, OUTSIDE] : names;
    // **Pinned first, and the plane's own order inside each group** (ADR 0039). A pin says
    // WHICH workspaces come first, never in what order they do — so two operators who pin
    // the same two see the same arrangement, which is the plane's.
    return [
      ...all.filter((name) => pinnedWorkspaces.includes(name)),
      ...all.filter((name) => !pinnedWorkspaces.includes(name)),
    ];
  }, [filedIn, pinnedWorkspaces, sidebar, tabs]);

  /** And which of them the workspace strip has room to draw. The same rule as the chats'
   *  one level up, because the operator's complaint was about all of them: a strip that
   *  scrolls says nothing about what is past its edge. */
  const { strip: workspaceStrip, width: workspaceRoom } = useRoom(strips.length);
  const workspacesShown = useMemo(
    () => fitting(strips, focused, workspaceRoom, LEAST.workspace),
    [focused, strips, workspaceRoom],
  );

  /**
   * What a workspace is drawn as: its name, its pin, and the two counts.
   *
   * **One definition, used by the strip and by the menu of what the strip has no room for.**
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
  const workspaceMarks = (workspace: string) => {
    const waiting = states.needsYou.filter((session) => filedIn(session) === workspace).length;
    const here = tabsIn(tabs, workspace, filedIn).length;
    const called = workspace === OUTSIDE ? OUTSIDE_TITLE : workspace;
    return (
      <>
        <span className="workspace-name">{called}</span>
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
  const { strip, width: room } = useRoom(onStrip.length);
  const { shown, hidden } = useMemo(
    () => fitting(onStrip, tabs.inFront, room, LEAST.chat),
    [onStrip, room, tabs.inFront],
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
  const notShowing = useMemo(
    () => byLastActivity(hidden, tabs, lastMoved),
    [hidden, lastMoved, tabs],
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
    async (where: { tab: true } | { split: Direction }) => {
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

  /** A row was picked: the chat starts on that profile, with that persona, and either
   *  drawing charter's footer in its pane or leaving it blank (charter ADR 0029). */
  const startPicked = useCallback(
    async (profile: string, persona: string | null, showFooter: boolean) => {
      const where = picking?.where;
      if (where === undefined) return;
      const inFrontTab = now.current.inFront;
      // The tab's CHAT name, not the sentence the tab bar draws: the core is being told what
      // this chat is called, and a split's chat is called what the tab's chat is called. The
      // persona the tab also shows is the operator's, not part of the chat's name.
      const name =
        "split" in where && inFrontTab !== undefined
          ? now.current.byId[inFrontTab].chat
          : String(now.current.named.tabs + 1);
      const started = await commands
        .startChat(
          plane,
          profile,
          persona,
          startIn,
          name,
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
      // Software went into somebody's config folder, so it is said.
      setWired(started.data.wired ?? undefined);
      const session = started.data.session;
      // Where charter put it, written down before the tab is drawn: the plane will say the
      // same thing a tick later, and until it does this is what keeps the tab on the strip
      // the operator is looking at.
      const filed = startIn === null ? OUTSIDE : (focused ?? OUTSIDE);
      setStartedIn((was) => ({ ...was, [session]: filed }));
      if ("tab" in where) {
        change((tabs) => openTab(tabs, session, name, persona));
        return;
      }
      const before = now.current;
      // The tab that was to be split can have closed while the picker was open. Nothing
      // would show that session, so it is ended rather than left running unseen.
      if (change((tabs) => splitFocusedPane(tabs, where.split, session)) === before) {
        void commands.closeSession(plane, session);
      }
    },
    [change, focused, picking, plane, startIn],
  );

  /** The approval IS this click. After it, the whole chain of checks runs again from the
   *  top before anything is exec'd, so a yes never walks past a refusal standing behind it. */
  const approveAndStart = useCallback(
    async (profile: string, persona: string | null, showFooter: boolean, shown: string) => {
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
      await startPicked(profile, persona, showFooter);
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
    },
    [change, filedIn, isPinned],
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

  /** Removes the piece the chat in front works in. The core's refusal travels back whole. */
  const removeWorktree = useCallback(
    async (force: boolean): Promise<Ran> => {
      if (!worktree) return { ok: false, refused: "There is no worktree in front to remove." };
      const answer = await commands
        .worktreeRemove(plane, worktree.workspace, worktree.repo, worktree.piece, force)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      // Verbatim. The sentence names the repair, and an operator shown a reworded version of
      // it can neither follow that repair nor search for it.
      if (answer.status === "error") return { ok: false, refused: answer.error };
      // The core is asked again rather than the window assuming what it now says.
      setRelocate((asked) => asked + 1);
      return {
        ok: true,
        said: `The worktree ${worktree.piece} is gone. The branch ${worktree.branch ?? worktree.piece} stays.`,
      };
    },
    [plane, worktree],
  );

  /** Asks for a new workspace. It makes nothing: the dialog is what asks, and
   *  `workspace_create` is what makes one. */
  const createWorkspace = useCallback(() => {
    setWorkspaceTrouble(undefined);
    setMakingWorkspace(true);
  }, []);

  /**
   * Makes it, through `charter workspace create`.
   *
   * **The name is not checked here.** `workspace_create` runs `wscmd::create`, which runs
   * `wscmd::ensure`, which is where `contain::workspace_name_ok` lives — so the window refuses
   * exactly the names a terminal refuses, in the same sentence. A refusal stays in the dialog,
   * where the operator is still standing.
   */
  const makeWorkspace = useCallback(
    async (name: string, vision: string) => {
      setBusyMaking(true);
      const answer = await commands
        .workspaceCreate(plane, name, vision.trim() === "" ? null : vision)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      setBusyMaking(false);
      if (answer.status === "error") {
        setWorkspaceTrouble(answer.error);
        return;
      }
      setMakingWorkspace(false);
      setWorkspaceTrouble(undefined);
      // charter's own lines, which say where it landed and whether it is LOCAL or LIVE.
      setReport({ from: "workspace.create", refused: false, words: answer.data.join(" ") });
      // The plane is read again rather than this window writing the workspace into its own
      // copy of the sidebar, and the strip lands on what was just made: it holds no chats, so
      // `picked` is the only thing that can put the window in it.
      setPicked(name);
      setReplan((asked) => asked + 1);
    },
    [plane],
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

  /**
   * Deletes it — **through `workspace_remove` and through nothing else**.
   *
   * There is one path from this window to a deleted workspace and the core's guard is inside
   * it (`wscmd::remove`, which runs `wscmd::work_at_risk` before `remove_dir_all`). `force` is
   * never passed on the operator's behalf: it arrives here only from the second button, which
   * does not exist until a refusal does and which names what it will discard.
   */
  const deleteWorkspace = useCallback(
    async (workspace: string, force: boolean) => {
      setRemoving((now) => (now?.workspace === workspace ? { ...now, busy: true } : now));
      const answer = await commands
        .workspaceRemove(plane, workspace, force)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
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

  /** Lands the piece in its clone, fast-forward only. The core never pushes. */
  const mergeWorktree = useCallback(async (): Promise<Ran> => {
    if (!worktree) return { ok: false, refused: "There is no worktree in front to merge." };
    const answer = await commands
      .worktreeMerge(plane, worktree.workspace, worktree.repo, worktree.piece)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (answer.status === "error") return { ok: false, refused: answer.error };
    return {
      ok: true,
      said: `${answer.data.branch} landed: ${answer.data.was} → ${answer.data.now}`,
    };
  }, [plane, worktree]);

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
      const session = panesOf(now.current, id)[0]?.session;
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

  const doing = useMemo<Doing>(
    () => ({
      newChat: newTab,
      split,
      closePane,
      closeTab: close,
      selectTab: bringToFront,
      pinTab,
      pinWorkspace,
      pinProject: windowDoes.pinProject,
      focusWorkspace,
      createWorkspace,
      removeWorkspace,
      showChat,
      removeWorktree,
      mergeWorktree,
      sendKey,
      openProject: windowDoes.openProject,
      createProject: windowDoes.createProject,
      showExtensions: windowDoes.showExtensions,
      selectProject: windowDoes.selectProject,
      closeProject: windowDoes.closeProject,
      quit: windowDoes.quit,
    }),
    [
      bringToFront,
      close,
      closePane,
      createWorkspace,
      focusWorkspace,
      mergeWorktree,
      newTab,
      pinTab,
      pinWorkspace,
      removeWorkspace,
      removeWorktree,
      sendKey,
      showChat,
      split,
      windowDoes,
    ],
  );

  // The chats that can be waiting on the operator without saying so. Read from the sidebar,
  // which is the core's own list of what is open and what each chat runs. It is needed up
  // here as well as beside the queue: the palette's row for the queue must not claim
  // "Nothing needs you." over the top of a chat that cannot say it does (charter-app#52).
  //
  // Held, because it is one of the catalogue's inputs: a fresh array on every render would
  // rebuild all 117 rows of a fifty-chat catalogue for every keystroke in the palette.
  const quiet = useMemo(
    () =>
      sidebar
        ? quietOnes([...sidebar.workspaces.flatMap((ws) => ws.chats), ...sidebar.unfiled], states)
        : [],
    [sidebar, states],
  );

  /** The chats working in the focused workspace, which is what the explorer files under the
   *  spots they are working at. The plane's own answer, like everything else about where a
   *  chat is: nothing on the plane records a chat, so the directory it works in is it. */
  const workspaceChats = useMemo(
    () => sidebar?.workspaces.find((ws) => ws.name === ofWorkspace)?.chats ?? [],
    [ofWorkspace, sidebar],
  );

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
            focused,
            worktree,
            plane,
            projects,
            // Only a refusal the REMOVAL gave, and only while it is still on screen: the
            // discard row is the operator's answer to a sentence they have read.
            refusal:
              report?.refused && report.from === "worktree.remove" ? report.words : undefined,
            needsYou: states.needsYou,
            quiet,
            nameOf,
            // The projects' pins are the WINDOW's, and travel down with the projects: a
            // project that is not in front draws nothing, so its pin cannot be held here.
            pinned: { chats: pinnedChats, workspaces: pinnedWorkspaces, projects: pinnedProjects },
          }),
    [
      focused,
      inFront,
      nameOf,
      pinnedChats,
      pinnedProjects,
      pinnedWorkspaces,
      plane,
      projects,
      quiet,
      report,
      states.needsYou,
      strips,
      tabs,
      worktree,
    ],
  );

  const by = useCallback((id: string) => offers.find((offer) => offer.id === id), [offers]);

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
      if (offer.available && ENDS_A_CHAT.has(offer.does.verb)) {
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
      tabs.order.flatMap((id) =>
        panesOf(tabs, id).map(({ session }) => {
          const known = reopened.find((chat) => chat.session === session);
          return {
            key: `${plane}#${session}`,
            project: plane,
            name: known?.name ?? tabs.byId[id].name,
            harness: known?.harness ?? null,
            cwd: known?.cwd ?? null,
            state: stateOf(states, session),
          };
        }),
      ),
    [plane, reopened, states, tabs],
  );

  // What this project has open, told to the window: the quit warning lists every project's
  // chats, and this project's own tab says when one of them needs you.
  //
  // **Its catalogue travels with it, and that is what keeps the palette one palette.** The
  // palette is mounted on the WINDOW — always, so `F2` reaches it before the core has said
  // which project this launch opened, and once, so a project that is not in front is not a
  // second capturing listener for the same key. What it lists has to be the project in
  // front's, and this is how it gets there.
  const mine = useMemo<PlaneReport>(
    () => ({ ending, needsYou: states.needsYou.length, settled, offers, run, said: report }),
    [ending, offers, report, run, settled, states.needsYou.length],
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

  const frontChat =
    frontTab && reopened.find((chat) => chat.session === panesOf(tabs, frontTab.id)[0]?.session);

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
      {strips.length > 0 && (
        <div className="workspaces">
          <div
            className="workspaces-strip"
            role="tablist"
            aria-label="Workspaces"
            ref={workspaceStrip}
            style={{ "--least": `${LEAST.workspace}px` } as CSSProperties}
          >
            {workspacesShown.shown.map((workspace) => {
              const offer = by(`workspace.focus:${workspace}`);
              return (
                /* Right-click is the third reader of the catalogue (`Menus.tsx`): focus, pin,
                   make one, and — under the line — delete this one. `asChild`, so the strip
                   gains no wrapper: the trigger IS the tab, which is what #171's `flex: 1 1 0`
                   cells require. */
                <Menued
                  key={workspace}
                  on={{ on: "workspace", workspace }}
                  offers={offers}
                  onPress={press}
                >
                  <button
                    role="tab"
                    aria-selected={workspace === focused}
                    title={offer?.title}
                    onClick={() => {
                      if (offer?.available) press(offer);
                    }}
                  >
                    {workspaceMarks(workspace)}
                  </button>
                </Menued>
              );
            })}
          </div>
          {/* And what it had no room for. One affordance per strip, with the strip's own
              noun in it: "workspaces" and not "tabs", because a window drawing three of
              these owes an operator — and a scenario spec — an answer to WHICH strip is
              not showing everything. */}
          <div className="more">
            <ShowMore
              noun="workspace"
              hidden={workspacesShown.hidden.map((workspace) => ({
                key: workspace,
                offer: by(`workspace.focus:${workspace}`),
                children: workspaceMarks(workspace),
              }))}
              onPress={press}
            />
          </div>
        </div>
      )}

      <header className="bar">
        {/* The chats of the FOCUSED WORKSPACE (ADR 0036), which is what the tmux frame's
            sessions-under-a-workspace was. Named, because the projects and the workspaces
            above are tablists too and a query for `role="tab"` across the whole window
            would mix all three. */}
        <div
          className="tabs"
          role="tablist"
          aria-label="Tabs"
          ref={strip}
          style={{ "--least": `${LEAST.chat}px` } as CSSProperties}
        >
          {shown.map((id) => (
            /* Right-click is the third reader of the catalogue (`Menus.tsx`). `asChild`, so
               the strip gains no wrapper element: the trigger IS the tab.

               No `data-tab` and no scroll-into-view ref any more: #171 deleted `offscreen.ts`
               and the strip collapses rather than scrolls, so there is nothing to scroll a
               tab into and nothing measuring tabs through the markup. */
            <Menued key={id} on={{ on: "chat", tab: id }} offers={offers} onPress={press}>
              <span className="tab">
                <button
                  role="tab"
                  aria-selected={id === tabs.inFront}
                  // The catalogue's row, not a second copy of it. The tab already in front
                  // has a row that says so and cannot run — a tab is never disabled, because
                  // the selected tab is the one a keyboard has to be able to land on.
                  onClick={() => {
                    const offer = by(`tab.select:${id}`);
                    if (offer?.available) press(offer);
                  }}
                >
                  <span className="tab-name">{tabs.byId[id].name}</span>
                  <Pin held={isPinned(id)} what="chat" />
                  {/* The first pane's session is the tab's own chat. Its own element, so what
                    a tab IS stays separate from what it is DOING — a tab whose text changed
                    every time a turn began would be unreadable, and untestable. */}
                  <ChatState state={stateOf(states, panesOf(tabs, id)[0]?.session ?? -1)} />
                </button>
                <Closer offer={by(`tab.close:${id}`)} onPress={press} />
              </span>
            </Menued>
          ))}
        </div>
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
              children: (
                <>
                  <span className="tab-name">{tabs.byId[id].name}</span>
                  <ChatState state={stateOf(states, panesOf(tabs, id)[0]?.session ?? -1)} />
                </>
              ),
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
        {/* Which regions are drawn (ADR 0038). Here rather than in each region, because a
            region that is not drawn has nowhere to put its own way back.

            **One button per region in the arrangement**, in the order the window draws them —
            so a region added to the catalogue gets its own way back without anybody
            remembering to add one, which is the half of this that a list written out by hand
            kept getting wrong. */}
        <div className="regions-doing">
          {SIDES.flatMap((side) => slots[side]).map((placed) => (
            <RegionToggle
              key={placed.id}
              id={placed.id}
              shown={!placed.collapsed}
              onToggle={toggleRegion}
            />
          ))}
        </div>
        {/* The project's path is NOT here any more. It is on the status line at the very
            bottom of the window (`StatusLine.tsx`), where the operator asked for it. */}
      </header>

      {trouble && (
        <p className="trouble" role="alert">
          {trouble}
        </p>
      )}

      {/* Software went into a config folder on the way to starting a chat, so it says what
          and where. Dismissible, because it is news and not a fault. */}
      {wired && (
        <p className="came-back" role="status">
          {wired} <button onClick={() => setWired(undefined)}>dismiss</button>
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

      {/* **The four regions** (charter ADR 0038): by default the explorer on the left, the
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
              state={workspaceState}
              chats={workspaceChats}
              states={states}
              spot={spot}
              onPick={pickSpot}
              onShowChat={showChat}
            />
          ),
          aside: (
            <Panels
              plane={plane}
              workspace={ofWorkspace}
              state={workspaceState}
              queue={states.needsYou}
              quiet={quiet}
              nameOf={nameOf}
              showChat={showChat}
            />
          ),
          bottom: <BottomBar workspace={ofWorkspace} state={workspaceState} />,
        }}
        centre={
          /* The centre is where a chat is, so its menu is the chat verbs the bar has: a new
             tab, the two splits, the key the palette claimed, and — under the line — ending
             this pane's chat. `asChild` again: the panes' box is measured, and it must not
             gain a wrapper. */
          <Menued on={{ on: "pane" }} offers={offers} onPress={press}>
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
                />
              ) : tabs.order.length > 0 ? (
                // Chats are running — just not in the workspace being looked at. Saying
                // "no sessions" here would be charter telling the operator that what it is
                // still drawing on the strip above does not exist.
                <p className="empty">
                  No chats in this workspace. Open one with New tab, or pick a workspace above.
                </p>
              ) : (
                <p className="empty">No sessions. Open one with New tab.</p>
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
        state={workspaceState}
        doctor={doctor}
        updates={updates}
        pin={pin}
        alerts={alerts}
      />

      {/* Making a workspace, and deleting one. Mounted only while they are up, and drawn
          here rather than in the window: a workspace belongs to a project. */}
      {makingWorkspace && (
        <NewWorkspace
          plane={plane}
          trouble={workspaceTrouble}
          making={busyMaking}
          onCreate={(name, vision) => void makeWorkspace(name, vision)}
          onCancel={() => {
            setMakingWorkspace(false);
            setWorkspaceTrouble(undefined);
          }}
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
          onStart={(profile, persona, footer) => void startPicked(profile, persona, footer)}
          onApprove={(profile, persona, footer, shown) =>
            void approveAndStart(profile, persona, footer, shown)
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
  /** How many of its chats are asking for the operator, for its own tab to say so. */
  needsYou: number;
  /** Whether the core has answered what it already had open. Until it has, "no tabs" is
   *  "not yet", and a quit that read it as "nothing is running" would end the lot. */
  settled: boolean;
};

/** What a project asks the WINDOW to do, because the window is what holds projects. */
export type WindowDoing = {
  openProject: () => void;
  /** Shows the dialog that makes a new project. The window's, like the opener: what it ends in
   *  is another project tab, and the open it ends in is the gated one (ADR 0035). */
  createProject: () => void;
  /** Shows what has contributed what to this window. The window's and not a project's: an
   *  extension is machine state (charter ADR 0041), so it is the same list behind every tab. */
  showExtensions: () => void;
  selectProject: (plane: string) => void;
  closeProject: (plane: string) => Promise<Ran>;
  /** Pinning a PROJECT is the window's, because the project strip is: a project that is not
   *  in front draws nothing, and its pin still has to be on that strip (charter ADR 0039). */
  pinProject: (plane: string, pinned: boolean) => Promise<Ran>;
  quit: () => void;
};

/** The size a session starts at. The pane it lands in tells it the real one at once. */
const STARTING_SIZE = { columns: 80, rows: 24 };

/**
 * The verbs that end a chat, and therefore the ones that are asked about first.
 *
 * **By verb and not by row id**, so a row added to the catalogue that ends a chat is asked
 * about without anybody remembering to add it here — the same rule the region toggles follow.
 * `closeProject` is deliberately not one of them: it ends every chat in a project and has its
 * own sentence on its own row, and the window is where that question belongs.
 */
const ENDS_A_CHAT = new Set(["closeTab", "closePane"]);

/** A button that IS a row of the catalogue: its words, its availability and its reason.
 *
 *  Nothing is drawn for an id the catalogue no longer has. That is the point: the bar cannot
 *  keep offering something the one list has stopped offering, because there is no second
 *  place for the words to live. */
export function Doer({
  offer,
  onPress,
  iconOnly,
}: {
  offer?: Offer;
  onPress: (offer: Offer) => void;
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
      className={clsx(offer.id === "pane.close" && "ends-a-chat", bare && "bare")}
      disabled={!offer.available}
      aria-label={bare ? offer.title : undefined}
      title={offer.reason || offer.note || (bare ? offer.title : undefined)}
      onClick={() => onPress(offer)}
    >
      {Mark && <Mark />}
      {!bare && offer.title}
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
 */
export const MARKS: Record<string, typeof Plus> = {
  "chat.new": Plus,
  "pane.split.right": SquareSplitHorizontal,
  "pane.split.down": SquareSplitVertical,
  "pane.close": X,
  "project.open": FolderPlus,
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
  /** What to list, already in the order it is listed in. */
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
  // Nothing is hidden, so there is nothing to say there is more OF.
  if (hidden.length === 0) return null;
  const many = hidden.length === 1 ? `1 ${noun}` : `${hidden.length} ${noun}s`;
  return (
    // **Not modal.** A modal Radix surface marks the rest of the window `aria-hidden` (which
    // `docs/ui-primitives.md` records the dialogs doing), and this is a menu on a strip, not
    // a question that has to be answered before the window can be used again. A click outside
    // closes it, which is what every menu on every platform does — the dialogs' opposite rule
    // is about a surface that would lose an answer, and there is no answer to lose here.
    <Menu.Root modal={false} open={open} onOpenChange={setOpen}>
      <Menu.Trigger asChild>
        <button
          className="show-more"
          aria-label={`Show ${many} the strip is not showing`}
          onPointerDown={(event) => event.preventDefault()}
          onClick={() => setOpen((up) => !up)}
        >
          {hidden.length} more
          <ChevronDown />
        </button>
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Content className="more-menu" align="end" sideOffset={4} collisionPadding={8}>
          {hidden.map(({ key, offer, children }) => {
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
  children: ReactNode;
};

/**
 * The mark on something the operator pinned (charter ADR 0039).
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
      className="closer"
      aria-label={offer.title}
      title={offer.note ? `${offer.title} — ${offer.note}` : offer.title}
      onClick={() => onPress(offer)}
    >
      <X />
    </button>
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
}) {
  if (layout.kind === "pane") {
    return (
      // The frame holds the terminal and what charter draws over it side by side, so neither
      // is ever a child of the element xterm draws into.
      <div className="pane-frame">
        <SessionPane
          plane={plane}
          session={layout.session}
          focused={layout.pane === focused}
          onFocus={() => onFocus(layout.pane)}
        />
        {/* **One corner, one row, because two changes landed in it at once.** The gauge
            (M6.10) and these controls were each written as the thing in the pane's top-right,
            and absolutely positioned there they would sit on top of each other. A row lays
            them out side by side without either having to know the other's width — and the
            gauge keeps the corner, because it is always drawn and the controls are not. */}
        <div className="pane-corner">
          <PaneDoing pane={layout.pane} offerFor={offerFor} onPaneDoes={onPaneDoes} />
          <PaneGauge
            plane={plane}
            session={layout.session}
            moved={movedAt(states, layout.session)}
            running={stateOf(states, layout.session) === "running"}
          />
        </div>
      </div>
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
            className={offer.id === "pane.close" ? "ends-a-chat" : undefined}
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
