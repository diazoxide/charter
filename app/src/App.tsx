import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { listen } from "./here";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { closestCenter, DndContext } from "@dnd-kit/core";
import { horizontalListSortingStrategy, SortableContext } from "@dnd-kit/sortable";
import { afterDrop } from "./reorder";
import {
  ALONG_THE_STRIP,
  keepsTheFocus,
  SortableTab,
  stripAccessibility,
  useStripSensors,
} from "./sortable";
import "./styles.css";
import {
  commands,
  type Ask,
  type ForgeRow,
  type ForgeWord,
  type FoundFile,
  type GoneProject,
  type OpenedRepo,
  type PlaneId,
  type RelaunchChoice,
  type RelaunchQuestion,
  type SlowStart,
  type TemplateChoice,
} from "./bindings";
import type { ForgeAsk } from "./ForgeQuestion";

/** What trying to open a repo came to: refused, a question about its forge, or opened. */
type RepoTried = { refused?: string; asksForge?: string };
import { UnsavedMark } from "./SavingView";
import { SessionBusNotice } from "./SessionBusNotice";
import { VaultsWaitingNotice } from "./VaultsWaitingNotice";
import { Notice } from "./Notice";
import { GoneProjectNotice } from "./GoneProjectNotice";
import { tellSaved, useRepoSaving } from "./saving";
import {
  catalogue,
  catalogued,
  perform,
  projectRows,
  type Doing,
  type Offer,
  type Project,
  type Ran,
} from "./actions";
import { countOf, useAlerts } from "./alerts";
import { AlertsDrawer, type AlertsDrawerDoes } from "./AlertsDrawer";
import { useAboutThisMachine } from "./windowprefs";
import { ApprovePlane } from "./ApprovePlane";
import { drawThemeFor, Extensions } from "./Extensions";
import { Opener } from "./Opener";
import { SettingsTab } from "./settings/SettingsTab";
import { levelOf, linkToGroup, settingsPlace, type SettingsLink } from "./settings/links";
import { useExtensionsOn } from "./extensionsOn";
import { useProjectTheme } from "./projectTheme";
import { drawTint } from "./theme/theme";
import { useContributedPanels } from "./Panels";
import { useTabStop } from "./roving";
import { closeOnDelete } from "./tabKeys";
import { useExtensionCommands, useExtensionViews } from "./Views";
import { Palette } from "./Palette";
import { placeOf, scopeLadder } from "./fileFind";
import { useJumpAsks, type Pending } from "./fileJump";
import type { Place } from "./pieceViews";
import { ClosingProject } from "./ClosingProject";
import { QuitWarning, type Ending } from "./QuitWarning";
import { RelaunchAsk } from "./RelaunchAsk";
import { fitting, LEAST, leastAt, useRoom } from "./fits";
import { Menued, useNoBrowserMenu } from "./Menus";
import { NewProject } from "./NewProject";
import {
  Closer,
  Doer,
  Gear,
  Pin,
  PlaneView,
  ShowMore,
  type FirstChat,
  type PlaneReport,
  type WindowDoing,
} from "./PlaneView";
import type { Alerts } from "./StatusLine";
import { TitleBar, useTitleBarRoom } from "./TitleBar";
import type { Needing, PermissionAsk, Quiet } from "./NeedsYou";
import { answerAsk, usePermissionAsks } from "./permissionAsks";
import { useUpdates } from "./Updates";
import { noTabs, SETTINGS_TAB_TITLE } from "./tabs";
import { useTextSizes } from "./textSize";
import { MAIN, runElsewhere, thisWindow, useOtherWindows, useRunHere } from "./windows";

/**
 * Puts the app's own `charter` on a terminal's `PATH`, and answers what the core said — the
 * link it made, or why it made none. Nothing else in the window is involved, so it is not a
 * hook: the core decides and the operating system asks for the password.
 */
async function installCli(): Promise<Ran> {
  const answer = await commands
    .installCliOnPath()
    .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
  return answer.status === "ok"
    ? { ok: true, said: answer.data }
    : { ok: false, refused: answer.error };
}

/**
 * The window, which holds projects.
 *
 * **A project is a plane and a window may hold several** (ADR 0033, spec decision 23).
 * The operator asked for Zed's shape by name and gave the reason Zed has it: eight projects is
 * eight things to arrange, and the thing an operating system gives you to arrange is a window.
 * So the top-level tabs here are projects; everything inside one is `PlaneView`'s, which is
 * the extraction that made a second project possible at all.
 *
 * **Switching projects is navigation, never a teardown.** The project left behind stays
 * mounted, keeps its tabs, its splits and its focused workspace, and goes on being told what
 * its chats are doing — fifty chats in project A are not torn down because the operator
 * glanced at project B, which is the whole reason he wanted one window per project.
 *
 * What lives up here is what belongs to the WINDOW: which projects it holds, which one is in
 * front, the opener and the trust ask in front of every open, the cold-launch restore, a
 * second launch handing its directory over, and the quit that ends every project's chats at
 * once.
 */
function App() {
  // The WebView's own menu, taken away from the whole window (`Menus.tsx`). One listener, on
  // the window, so it covers every surface including the ones with no charter menu of their
  // own — a shipped app that answers a right-click with `Reload` and `Inspect Element` is
  // showing the operator the browser it is built on.
  useNoBrowserMenu();
  /** Which window this is (charter#126): the main window, or a split window a project tab was
   *  moved into. Tauri's label, read once — a window never becomes another. */
  const own = useMemo(() => thisWindow(), []);
  const split = own !== MAIN;
  /** What the launch resolved, asked once. `undefined` while the core has not answered. */
  const [launch, setLaunch] = useState<{ plane: PlaneId | null; here: boolean; reason: string }>();
  /** The projects this window holds, left to right as the strip shows them. */
  const [planes, setPlanes] = useState<PlaneId[]>([]);
  /** The same, as of the last render, for a verb that is kept stable across renders. */
  const planesNow = useRef(planes);
  useLayoutEffect(() => {
    planesNow.current = planes;
  });
  /** What is on screen: one of the projects, or the opener. The opener is not only the state
   *  of an empty window — it is also how a window holding eight gets a ninth. */
  const [showing, setShowing] = useState<{ at: "opener" } | { at: "plane"; plane: PlaneId }>({
    at: "opener",
  });
  /** What each project has open and whether it has found out yet. Reported by the project. */
  const [reports, setReports] = useState<Record<string, PlaneReport>>({});
  /** Whether the operator is being asked about quitting. */
  const [asking, setAsking] = useState(false);
  /** The trust asks waiting to be answered, oldest first. A restore can raise several at
   *  once — one project's committed settings changed while charter was not running — and
   *  they are answered one at a time rather than drawn on top of one another. */
  const [approving, setApproving] = useState<Ask[]>([]);
  /** Why the last attempt to open a project opened nothing. Shown on the opener, which is
   *  where the operator is standing when it happens. */
  const [openTrouble, setOpenTrouble] = useState<string>();
  /** What a window-level action answered, when it had something to say. */
  const [report, setReport] = useState<{ from: string; refused: boolean; words: string }>();
  /** Whether the palette is up, so what an action answered is said in one place rather than
   *  two: the palette is modal and draws over the line below it. */
  const [paletteOpen, setPaletteOpen] = useState(false);
  /** Whether the new-project dialog is up, and why the last attempt made nothing. The
   *  window's, like the opener: what it ends in is a project this window holds. */
  const [creating, setCreating] = useState(false);
  const [createTrouble, setCreateTrouble] = useState<string>();
  /** Whether charter is scaffolding one right now, so the answer cannot be given twice. */
  const [makingProject, setMakingProject] = useState(false);
  /** Whether the extension list is up. The window's, not a project's: an extension is machine
   *  state, so it is the same list whichever project is in front. */
  const [extensions, setExtensions] = useState(false);
  /**
   * The last ask for a project's Settings tab at the Project level (SE-19, #1213; the retired
   * Project settings page before it, charter-app#252): which project, and a count so
   * that asking twice opens it twice — the second ask brings forward a tab the operator may
   * have left behind. The project's own `PlaneView` opens it, because its tabs are its own.
   */
  const [settingsAsk, setSettingsAsk] = useState<{ plane: PlaneId; at: number }>();
  /**
   * How many times the project switcher has been asked for from outside the palette (FR-27) —
   * the title bar's button, or the row run from a menu. A count, `settingsAsk`'s shape, so that
   * asking twice opens it twice; the palette opens on each new value.
   */
  const [switcherAsk, setSwitcherAsk] = useState(0);
  /**
   * The last file ⌘P was asked to open (FM-7): its project, its branch, its path and a count,
   * the shape `settingsAsk` has. The project's own `PlaneView` opens its file tab, because its
   * tabs are its own; the window brings the project to the front first.
   */
  const [fileAsk, setFileAsk] = useState<{
    plane: PlaneId;
    place: Place;
    path: string;
    /** A jump to a line (a search hit, FM-8): the branch's file tab lands on it. */
    line?: number;
    at: number;
  }>();
  /**
   * A jump to a file at a line (FM-8, `fileJump.ts`): a search hit, possibly another project's.
   * That project comes to the front and opens the branch's file tab, which lands on the line.
   */
  const jumped = useCallback((jump: Pending) => {
    setShowing({ at: "plane", plane: jump.plane });
    setFileAsk((was) => ({
      plane: jump.plane,
      place: jump.place,
      path: jump.path,
      line: jump.line,
      at: (was?.at ?? 0) + 1,
    }));
  }, []);
  useJumpAsks(jumped);
  /** The last ask for a project's Saving tab (charter-app#294), the same shape as `settingsAsk`. */
  const [savingAsk, setSavingAsk] = useState<{ plane: PlaneId; at: number }>();
  /**
   * The last ask for the Settings tab (SE-16), the same shape as `settingsAsk`: the project in
   * front when it was asked, whose strip the tab opens on. It opens at that project's focused
   * level (SE-23) — its focused workspace's, else the project's — which only that project knows,
   * so the project answers the ask (`PlaneView`). With no project open it is You, at the opener.
   */
  const [settingsTabAsk, setSettingsTabAsk] = useState<{ plane: PlaneId; at: number }>();
  /** The same, for the Settings tab at the You level (SE-23's Your settings…): on the project in
   *  front's strip, since You is the machine's and any strip will do. */
  const [yourSettingsAsk, setYourSettingsAsk] = useState<{ plane: PlaneId; at: number }>();
  /**
   * The last link into a Settings group the window followed (SE-22): the project in front, the
   * link, and a count. Its own ask and not `settingsTabAsk`, because a link names its level and
   * Settings… opens at whatever level ⌘, decides.
   */
  const [settingsLinkAsk, setSettingsLinkAsk] = useState<{
    plane: PlaneId;
    link: SettingsLink;
    at: number;
  }>();
  /**
   * The first chat a repo opened into the local project asks for (FR-4): the workspace named
   * after the repo, the clone the chat starts in, and a count, the shape `settingsAsk` has.
   * `plane` is the project once it is open; until the trust question about it is answered it
   * is `null` and `asking` names the path that question is about. The project's own
   * `PlaneView` starts the chat, because the picker and the tabs are its own.
   */
  const [firstChat, setFirstChat] = useState<
    FirstChat & { plane: PlaneId | null; asking: string | null }
  >();
  /**
   * A shell tab asked for with a command already typed in it — `gh auth login` or `glab auth
   * login`, from the first run's "Sign in to GitHub" or "Sign in to GitLab" (FR-4, W10) — in
   * the same two stages as `firstChat`.
   */
  const [shellAsk, setShellAsk] = useState<{
    plane: PlaneId | null;
    asking: string | null;
    typed: string;
    at: number;
  }>();
  /** Whether a repo is being opened right now, and why the last one opened nothing. */
  const [openingRepo, setOpeningRepo] = useState(false);
  const [repoTrouble, setRepoTrouble] = useState<string>();
  /** The first run's question about the forge, when the repo's remote did not say (#839). */
  const [repoForgeAsk, setRepoForgeAsk] = useState<ForgeAsk>();
  /** The New project dialog's, for either of its forms. */
  const [createForgeAsk, setCreateForgeAsk] = useState<ForgeAsk>();
  /** Settings asked for with no project in front: drawn where the opener is, because there is
   *  no strip to open a tab on and a text size is still worth changing. */
  const [settingsAlone, setSettingsAlone] = useState(false);
  /** The project in front, for a verb that is kept stable across renders. */
  const inFrontNow = useRef<PlaneId | undefined>(undefined);
  /** Why this launch took longer than the limit, when it did — and nothing when it did not
   *  (charter-app#24). The core decides that; the window only draws it. */
  const [slowStart, setSlowStart] = useState<SlowStart>();
  /** Projects the last quit had open that charter would not take back, each with its line.
   *  Never an error dialog: a restore is a convenience (ADR 0033). */
  const [notRestored, setNotRestored] = useState<string[]>([]);
  /** The projects the last quit had open that have moved or gone: Locate… and Forget (NO-5). */
  const [goneAtLaunch, setGoneAtLaunch] = useState<GoneProject[]>([]);
  /** The gone projects the opener is drawing now. It owns those, so they are drawn once. */
  const [openerGone, setOpenerGone] = useState<readonly string[] | "unread">("unread");
  /** Bumped when a gone project is located or forgotten up here, so an opener reads again. */
  const [goneChanged, setGoneChanged] = useState(0);
  const goneSettled = useCallback(
    (path: string) => setGoneAtLaunch((was) => was.filter((one) => one.path !== path)),
    [],
  );
  /** Whether the cold-launch restore is still going. Until it is done the window has not
   *  finished saying which projects it holds, so neither the quit nor the arrangement it
   *  writes down may act on what it holds so far. */
  const [restoring, setRestoring] = useState(true);
  /** The launch's question, while it waits for the operator (charter-app#250): what would be
   *  put back, and how the answer reaches the restore that is waiting on it. */
  const [relaunchAsking, setRelaunchAsking] = useState<{
    question: RelaunchQuestion;
    answer: (choice: RelaunchChoice) => void;
  }>();
  /** Settled when the cold-launch restore is over, so a second launch's directory waits for
   *  it: opening a project starts its chats, and nothing may start while the launch's
   *  question is up. */
  const restoreOver = useMemo(() => {
    let settle = () => {};
    const done = new Promise<void>((resolve) => (settle = resolve));
    return { done, settle };
  }, []);
  /** Whether this window has ever held a project.
   *
   *  After it has, the opener is no longer the launch reporting what it could not resolve:
   *  the operator closed what was open, and "charter found no project here" would be charter
   *  answering a question nobody asked about a directory nobody is standing in. */
  const [heldSomething, setHeldSomething] = useState(false);
  useEffect(() => {
    if (planes.length > 0) setHeldSomething(true);
  }, [planes.length]);

  /**
   * charter's alerts, for every project this window holds, and whether their drawer is up.
   *
   * **The window's, not a project's** — the operator's correction that moved alerts out of
   * the right-hand region: an alert is about a plane, and the plane that matters is usually
   * not the one on screen. So the reading and the drawer live up here, and every project's
   * status line draws the same count. Opening the drawer asks again, so what it lists is what
   * is true when it is looked at rather than up to a minute ago.
   */
  const { reading: alertsRead, reread: rereadAlerts } = useAlerts(planes);
  const [alertsOpen, setAlertsOpen] = useState(false);
  /** What the window says about this machine rather than a project — a layout or theme file it
   *  could not use as written (`windowprefs.ts`). Drawn in the same drawer, counted in the same
   *  number. */
  const aboutThisMachine = useAboutThisMachine();
  const alertCount = countOf(alertsRead, planes, aboutThisMachine.length);
  const alerts = useMemo<Alerts>(
    () => ({
      count: alertCount,
      open: () => {
        rereadAlerts();
        setAlertsOpen(true);
      },
    }),
    [alertCount, rereadAlerts],
  );

  /**
   * The panels approved extensions contribute to every project's side region.
   *
   * **Here and not in each `PlaneView`, for the reason the alerts are here**: this is about the
   * MACHINE and not about a project. An extension is installed per machine (ADR 0041 —
   * *an extension never travels in a plane*), so a window holding eight projects would
   * otherwise take the same survey eight times — and a survey re-hashes every installed
   * extension's whole directory, which is 0041's named cost of fingerprinting code.
   *
   * Asked once, after the first frame. Nothing waits on it: a window with no extensions gets an
   * empty list and charter's own two panels, which is every window until one is installed.
   */
  const contributedPanels = useContributedPanels();
  /** The views approved extensions offer — the persona statistics button is one — asked once
   *  per window for the same reason as the panels above. */
  const extensionViews = useExtensionViews();
  const extensionCommands = useExtensionCommands();

  /** The projects, as the strip and the palette name them. */
  const projects = useMemo<Project[]>(
    () => planes.map((plane) => ({ plane, name: calledOn(plane) })),
    [planes],
  );

  /**
   * The projects this operator has pinned, by root (ADR 0039).
   *
   * **The window's and not a project's**, because the project strip is the window's: a
   * project that is not in front draws nothing, and its own pin still has to be on the strip.
   * Each `PlaneView` reports its own, and this is where they meet.
   *
   * It is a list of roots rather than a flag per project for the reason `actions.ts` gives:
   * a pin is the operator's arrangement of the projects and not a property of one, so it is
   * held once, here, instead of copied onto each.
   */
  const [pinnedProjects, setPinnedProjects] = useState<string[]>([]);

  // What this machine already remembers as pinned, asked once per project it holds. A pin
  // outlives the app, so a window that did not ask would draw an operator's arrangement as
  // if they had never made it. Asked per plane rather than as one list, because the machine
  // store's answer for a plane is what `plane_pins` gives and there is no second reader of
  // that file in the app.
  useEffect(() => {
    let gone = false;
    for (const plane of planes) {
      void commands
        .planePins(plane)
        .then((answer) => {
          if (gone || answer.status !== "ok" || !answer.data.project) return;
          setPinnedProjects((was) => (was.includes(plane) ? was : [...was, plane]));
        })
        // A window that cannot ask simply draws nothing pinned. Every project is still there.
        .catch(() => undefined);
    }
    return () => {
      gone = true;
    };
  }, [planes]);

  /**
   * Pins or unpins one project. The core's refusal travels back whole — the store is
   * bounded, and "unpin one first" is a sentence the operator can act on.
   *
   * **`early` draws the pin before the core has written it**, and takes it back if the core
   * refuses. A drop across the pinned boundary asks for it (SI-6b): the strip's order changes
   * with the drop, and a pin that waited for the core would draw the tab in its old group
   * until the answer came — a jump under the pointer. Every other way in waits, so a mark is
   * never drawn that the store does not have.
   */
  const pinProject = useCallback(
    async (plane: string, pinned: boolean, { early = false } = {}): Promise<Ran> => {
      const mark = (on: boolean) =>
        setPinnedProjects((was) =>
          on ? (was.includes(plane) ? was : [...was, plane]) : was.filter((one) => one !== plane),
        );
      if (early) mark(pinned);
      const answer = await commands
        .pinProject(plane, pinned)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (answer.status === "error") {
        if (early) mark(!pinned);
        return { ok: false, refused: answer.error };
      }
      mark(pinned);
      return { ok: true };
    },
    [],
  );

  /**
   * Takes a project into this window, as a tab.
   *
   * **The one path for all five ways in**: a recents row, a picked folder, a typed path, a
   * second launch handing its directory over, and the cold-launch restore. Every one of them
   * reaches `open_plane`, so the trust gate (ADR 0035) is the same gate — the core reads this
   * machine's record against the project as it is on disk at that instant and either opens it
   * or answers with the question. **A window that formed its own opinion about trust would be
   * a second gate beside the one that bites.**
   *
   * Answers with the project when one opened, so a restore can decide which of several ends
   * up in front rather than landing on whichever answered last.
   *
   * **A project another window holds is not taken in twice** (charter#126): one project is
   * one tab in one window, so opening it here brings that window to the front instead. And
   * `into: false` opens it without taking it into this window at all, for a restore that is
   * about to move it into a window of its own.
   */
  const openInto = useCallback(
    async (path: string, andShow: boolean, into = true): Promise<PlaneId | undefined> => {
      setOpenTrouble(undefined);
      const answer = await commands
        .openPlane(path)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      // Verbatim: the sentence names the path and what was wrong with it, and an operator shown
      // a reworded version of it can neither act on it nor search for it.
      if (answer.status === "error") {
        setOpenTrouble(answer.error);
        return undefined;
      }
      if (answer.data.plane === null) {
        // The operator has to be asked first. Nothing was attached and nothing was started.
        const ask = answer.data.ask;
        if (ask)
          setApproving((queue) =>
            queue.some((q) => q.path === ask.path) ? queue : [...queue, ask],
          );
        return undefined;
      }
      const plane = answer.data.plane;
      if (!planesNow.current.includes(plane)) {
        const holder = await commands.showWindowHolding(plane).catch(() => null);
        if (holder !== null && holder !== own) return undefined;
      }
      if (!into) return plane;
      // A project already in the strip keeps its place: "open it" for one this window holds
      // means "show me that project", which is what a recents row and a second launch both
      // mean when they name one that is already a tab.
      setPlanes((was) => (was.includes(plane) ? was : [...was, plane]));
      if (andShow) setShowing({ at: "plane", plane });
      return plane;
    },
    [own],
  );

  /**
   * The operator said yes. The approval carries back the contribution they were shown, so it
   * is an answer to the question that was asked and not to whatever the project says by the
   * time the button is pressed.
   *
   * A refusal closes the dialog rather than keeping it up with a message: the one refusal
   * this command gives is "it changed while you were reading it", and the only honest repair
   * is to ask again about what it says now — which is what opening it again does.
   */
  const approveProject = useCallback(async (ask: Ask) => {
    const answer = await commands
      .approvePlane(ask.path, ask.contributes)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    setApproving((queue) => queue.filter((q) => q.path !== ask.path));
    if (answer.status === "error") {
      setOpenTrouble(answer.error);
      return;
    }
    setOpenTrouble(undefined);
    const plane = answer.data;
    setPlanes((was) => (was.includes(plane) ? was : [...was, plane]));
    setShowing({ at: "plane", plane });
    // A repo opened into this project was waiting on this yes for its first chat.
    setFirstChat((was) =>
      was?.asking === ask.path ? { ...was, plane, asking: null, at: was.at + 1 } : was,
    );
    setShellAsk((was) =>
      was?.asking === ask.path ? { ...was, plane, asking: null, at: was.at + 1 } : was,
    );
  }, []);

  /** Lets go of one project. Its chats end, its record is written into it, and its tab goes.
   *  Nothing of the project on disk goes. Reached through {@link closeProject}, which asks
   *  first when there is anything to end. */
  /** Takes one project's tab out of this window, and nothing else: closing it and moving it to
   *  another window both end here. */
  const takeOut = useCallback((plane: string) => {
    setPlanes((was) => {
      const at = was.indexOf(plane);
      // Twice is once: the window that asked the close takes the tab out, and then hears the
      // core say `plane-closed` about the same project (#1242).
      if (at < 0) return was;
      const left = was.filter((held) => held !== plane);
      // The tab beside it comes to the front, which is `closeTab`'s rule one scope up. With
      // nothing left, the opener — the window has to stay useful with no project (#111).
      setShowing((on) =>
        on.at === "plane" && on.plane === plane
          ? left.length === 0
            ? { at: "opener" }
            : { at: "plane", plane: left[Math.max(at - 1, 0)] }
          : on,
      );
      return left;
    });
    // And the window forgets what that project had open: its report is about chats that have
    // just been ended, or that another window now draws, and a quit warning listing them here
    // would list them twice or list nothing.
    setReports((was) => {
      const { [plane]: gone, ...rest } = was;
      void gone;
      return rest;
    });
  }, []);

  const letGoOf = useCallback(
    async (plane: string): Promise<Ran> => {
      const answer = await commands
        .closePlane(plane)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (answer.status === "error") return { ok: false, refused: answer.error };
      takeOut(plane);
      return { ok: true, said: `purlis let go of ${plane}. Nothing in it was changed.` };
    },
    [takeOut],
  );

  /**
   * Moves one project into another window — a new one when `to` is null (charter#126).
   *
   * **Nothing is closed.** Its chats go on running in the core, and the window it arrives in
   * draws them from what the core has open, as a window that reloaded would. This window only
   * takes its tab out. A split window left holding nothing goes (`windows.rs`), and the main
   * window left holding nothing shows the opener.
   */
  const moveProject = useCallback(
    async (plane: string, to: string | null): Promise<Ran> => {
      const answer = await commands
        .moveProjects([plane], plane, to)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (answer.status === "error") return { ok: false, refused: answer.error };
      takeOut(plane);
      return { ok: true };
    },
    [takeOut],
  );

  /** The project whose close is waiting on the operator's answer (`ClosingProject`). */
  const [closing, setClosing] = useState<string>();
  /** Every project's report as of the last render, read when a close arrives rather than
   *  closed over, so the verb below stays one function. */
  const reportsNow = useRef(reports);
  useLayoutEffect(() => {
    reportsNow.current = reports;
  });

  /**
   * The `project.close:<plane>` row's verb: **ask first when the project has chats open**, and
   * let go of it at once when it has none — the operator's ruling on charter-app#239, where
   * Delete on a project's tab put "end every chat in it" one key away.
   *
   * Here, where the row is carried out, and not on any one button, so the `×`, Delete, the
   * tab's menu and the palette all ask it (`EndingChat` is the same rule one scope down). A
   * project that has not yet said what it has open is asked about too: "no tabs" there is
   * "not yet", the quit warning's rule. It answers `{ ok: true }` for a row it has only asked
   * about, as `PlaneView`'s `run` does.
   */
  const closeProject = useCallback(
    async (plane: string): Promise<Ran> => {
      const said = reportsNow.current[plane];
      if (said?.settled && said.ending.length === 0) return letGoOf(plane);
      setClosing(plane);
      return { ok: true };
    },
    [letGoOf],
  );

  /**
   * Scaffolds a new project and takes it into this window — **through the same gate**.
   *
   * `create_project` writes the plane and then answers with `open_if_approved`'s own answer,
   * so this handles it exactly as `openInto` handles a recents row: a project, or the question
   * to ask first. A plane charter has just made is still one this machine has approved nothing
   * about (ADR 0035), so the ordinary end of this dialog is the trust dialog.
   *
   * A refusal stays IN the dialog rather than behind it: `init`'s refusal in a repository is
   * four lines naming what to do instead, and the operator is still standing at the box.
   */
  const makeProject = useCallback(
    async (
      path: string,
      planeIsThisRepo: boolean,
      adopt: string,
      forge: ForgeWord | null = null,
    ) => {
      setMakingProject(true);
      setCreateForgeAsk(undefined);
      const answer = await commands
        .createProject(path, planeIsThisRepo, adopt === "" ? null : adopt, forge)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      setMakingProject(false);
      if (answer.status === "error") {
        setCreateTrouble(answer.error);
        return;
      }
      const made = answer.data.opened;
      if (made === null) {
        // The repo's remote did not say which forge (#839): asked in the dialog, and the
        // answer makes the project.
        const why = answer.data.asks_forge ?? "";
        setCreateForgeAsk({
          why,
          answer: (named) => void makeProject(path, planeIsThisRepo, adopt, named),
        });
        return;
      }
      setCreating(false);
      setCreateTrouble(undefined);
      if (made.plane === null) {
        const ask = made.ask;
        if (ask)
          setApproving((queue) =>
            queue.some((q) => q.path === ask.path) ? queue : [...queue, ask],
          );
        return;
      }
      const opened = made.plane;
      setPlanes((was) => (was.includes(opened) ? was : [...was, opened]));
      setShowing({ at: "plane", plane: opened });
    },
    [],
  );

  /** What an opened repo puts in the window: its first chat, through the trust gate. */
  const openedRepo = useCallback((repo: OpenedRepo) => {
    const { opened, workspace, cwd, harness, none_installed, instructions } = repo;
    const plane = opened.plane;
    const ask = opened.ask;
    setFirstChat((was) => ({
      workspace,
      cwd,
      harness,
      noneInstalled: none_installed,
      instructions,
      plane,
      asking: plane === null ? (ask?.path ?? null) : null,
      at: (was?.at ?? 0) + 1,
    }));
    if (plane === null) {
      if (ask)
        setApproving((queue) => (queue.some((q) => q.path === ask.path) ? queue : [...queue, ask]));
      return;
    }
    setPlanes((was) => (was.includes(plane) ? was : [...was, plane]));
    setShowing({ at: "plane", plane });
  }, []);

  /**
   * Opens a repo into this machine's local project and asks for the first chat in it
   * (FR-4, #603). Nothing asks where the project goes: the core makes it in charter's own
   * directory when there is none, clones the repo into a workspace named after it, and
   * answers as `create_project` does — a project, or the trust question to ask first.
   *
   * Answers why nothing was opened (`refused`), or that the forge has to be asked first
   * (`asksForge`, #839), or neither when something was opened, so the first run and the New
   * project dialog can each keep the refusal or the question where the operator is standing.
   * `forge` is the answer to that question, on the call after it.
   */
  const openRepo = useCallback(
    async (
      path: string,
      template: TemplateChoice,
      forge: ForgeWord | null = null,
    ): Promise<RepoTried> => {
      setOpeningRepo(true);
      const answer = await commands
        .openRepo(path, template, forge)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      setOpeningRepo(false);
      if (answer.status === "error") return { refused: answer.error };
      if (answer.data.opened === null) return { asksForge: answer.data.asks_forge ?? "" };
      openedRepo(answer.data.opened);
      return {};
    },
    [openedRepo],
  );

  /**
   * The first run's repo, opened, with its refusal or its forge question kept on the first-run
   * screen. The question's answer opens it again, with the forge named.
   */
  const firstRunRepo = useCallback(
    (path: string, template: TemplateChoice, forge: ForgeWord | null = null) => {
      setRepoTrouble(undefined);
      setRepoForgeAsk(undefined);
      void openRepo(path, template, forge).then(({ refused, asksForge }) => {
        setRepoTrouble(refused);
        if (asksForge !== undefined)
          setRepoForgeAsk({
            why: asksForge,
            answer: (named) => firstRunRepo(path, template, named),
          });
      });
    },
    [openRepo],
  );

  /** The same, from the New project dialog, which closes once the repo is open. */
  const dialogRepo = useCallback(
    (path: string, forge: ForgeWord | null = null) => {
      setCreateTrouble(undefined);
      setCreateForgeAsk(undefined);
      // The New project dialog lays out no template: the first run is where one is chosen
      // (FR-17).
      void openRepo(path, { kind: "no-template" }, forge).then(({ refused, asksForge }) => {
        if (asksForge !== undefined)
          setCreateForgeAsk({ why: asksForge, answer: (named) => dialogRepo(path, named) });
        else if (refused === undefined) setCreating(false);
        else setCreateTrouble(refused);
      });
    },
    [openRepo],
  );

  /**
   * "Sign in to GitHub" or "Sign in to GitLab" on the first run (FR-4, W10's "detected and
   * offered"): the local project is opened — made first when there is none, and through the
   * trust gate — and `<cli> auth login` is typed into a shell tab at its root. The forge CLI's
   * own login, in a tab the operator can leave; nothing here asks anything. `cli` is the one the
   * core's first-run row names (`gh`, `glab`). With no repo to read, the forge whose sign-in
   * was pressed is the one a project made here tracks (#839).
   */
  const signInToForge = useCallback(async (row: ForgeRow): Promise<string | undefined> => {
    const cli = row.cli;
    const answer = await commands
      .openLocalProject(row.forge)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (answer.status === "error") return answer.error;
    const { plane, ask } = answer.data;
    setShellAsk((was) => ({
      plane,
      asking: plane === null ? (ask?.path ?? null) : null,
      typed: `${cli} auth login`,
      at: (was?.at ?? 0) + 1,
    }));
    if (plane === null) {
      if (ask)
        setApproving((queue) => (queue.some((q) => q.path === ask.path) ? queue : [...queue, ask]));
      return undefined;
    }
    setPlanes((was) => (was.includes(plane) ? was : [...was, plane]));
    setShowing({ at: "plane", plane });
    return undefined;
  }, []);

  const onReport = useCallback((plane: PlaneId, mine: PlaneReport) => {
    setReports((was) => (was[plane] === mine ? was : { ...was, [plane]: mine }));
  }, []);

  const windowDoes = useMemo<WindowDoing>(
    () => ({
      openProject: () => setShowing({ at: "opener" }),
      createProject: () => {
        setCreateTrouble(undefined);
        setCreating(true);
      },
      showExtensions: () => setExtensions(true),
      installCli,
      selectProject: (plane: string) => setShowing({ at: "plane", plane }),
      switchProject: () => setSwitcherAsk((was) => was + 1),
      closeProject,
      moveProject,
      pinProject,
      openSettings: (plane: string) => {
        setShowing({ at: "plane", plane });
        setSettingsAsk((was) => ({ plane, at: (was?.at ?? 0) + 1 }));
      },
      openSaving: (plane: string) => {
        setShowing({ at: "plane", plane });
        setSavingAsk((was) => ({ plane, at: (was?.at ?? 0) + 1 }));
      },
      openSettingsTab: () => {
        const plane = inFrontNow.current;
        if (plane === undefined) setSettingsAlone(true);
        else setSettingsTabAsk((was) => ({ plane, at: (was?.at ?? 0) + 1 }));
      },
      openYourSettings: () => {
        const plane = inFrontNow.current;
        if (plane === undefined) setSettingsAlone(true);
        else setYourSettingsAsk((was) => ({ plane, at: (was?.at ?? 0) + 1 }));
      },
      quit: () => void commands.askToQuit().catch(() => undefined),
    }),
    [closeProject, moveProject, pinProject],
  );

  /**
   * **Follows a link into a You group, or into a project's** (SE-22, `settings/links.ts`). A You
   * group opens on the strip of the project in front (`PlaneView`'s `settingsLinkAsked`); with
   * none, the group is shown at You's place and Settings is drawn where the opener is. A
   * project's group — what an Alerts-drawer row about that project names (NO-6) — brings that
   * project to the front and opens its Settings there. Never through Settings… (⌘,), which
   * opens at the level that is focused rather than the one the link names.
   */
  const openSettingsAt = useCallback((group: string, about?: PlaneId) => {
    const level = levelOf(group);
    if (level === "project" && about !== undefined) {
      setShowing({ at: "plane", plane: about });
      setSettingsLinkAsk((was) => ({ plane: about, link: { group }, at: (was?.at ?? 0) + 1 }));
      return;
    }
    if (level !== "you") return;
    const plane = inFrontNow.current;
    if (plane === undefined) {
      linkToGroup(settingsPlace("you"), group);
      setSettingsAlone(true);
    } else setSettingsLinkAsk((was) => ({ plane, link: { group }, at: (was?.at ?? 0) + 1 }));
  }, []);

  /** What the Alerts drawer's rows do in the window (NO-6). */
  const alertsDo = useMemo<AlertsDrawerDoes>(
    () => ({
      openSettings: openSettingsAt,
      openProject: (path: string) => void openInto(path, true),
      openSaving: windowDoes.openSaving,
      reread: rereadAlerts,
    }),
    [openInto, openSettingsAt, rereadAlerts, windowDoes],
  );

  // Which plane this launch opened — asked once, and the answer the first tab is built from.
  // The core resolved the working directory once to get it; nothing asks again.
  useEffect(() => {
    // A split window is not a launch: it was made to hold what was moved into it, and the
    // working directory was resolved once, for the main window (ADR 0034).
    if (split) {
      setLaunch({ plane: null, here: false, reason: "" });
      return;
    }
    void commands
      .planeAtLaunch()
      .then((it) =>
        setLaunch({
          plane: it.plane,
          // A directory it could read, that is in no plane, versus nothing to go on.
          here: it.from !== null,
          reason: it.why ?? "purlis has no plane open.",
        }),
      )
      // A command can also fail outright, with no answer of its own to give.
      .catch((err: unknown) => setLaunch({ plane: null, here: true, reason: String(err) }));
  }, [split]);

  /**
   * The window set the last quit left behind, put back (ADR 0033, spec decision 28).
   *
   * **Each project goes through `openInto`, which is the gate.** Opening one starts the
   * programs its reopen record names, so a restore may not be a way past the ask — a project
   * that has started contributing more than what was approved raises the same dialog it would
   * have raised from a recents row, and the rest of the restore carries on around it.
   *
   * **A project that has moved or is gone is dropped with a line saying so, never an error
   * dialog.** The core decides that; this draws the lines.
   *
   * Run once, after the launch has answered, and only then — the launch's own project is
   * attached by the core before there is a window, and its tab has to be the first one.
   *
   * **And not before the operator has answered the launch's question** (charter-app#250):
   * reopen every session, or start fresh. The core holds the launch's own project back until
   * `relaunch` has the answer, and the projects below are opened only after it, so nothing
   * starts while the question is up. A launch with nothing to put back asks nothing and
   * answers "Reopen all" itself — which is also what a question the core could not read
   * answers, because the other answer is the one that throws work away.
   */
  const restored = useRef(false);
  useEffect(() => {
    if (launch === undefined || restored.current) return;
    restored.current = true;
    // **A split window restores nothing and asks nothing.** It draws what was moved into it,
    // which the core already holds for it (`projects_handed`) — and asks again after a reload,
    // which is how a reloaded split window comes back with its tabs.
    if (split) {
      void (async () => {
        try {
          const held = await commands.projectsHanded().catch(() => null);
          if (held) {
            setPlanes(held.planes);
            const front = held.active === null ? undefined : held.planes[held.active];
            if (front !== undefined) setShowing({ at: "plane", plane: front });
          }
        } finally {
          setRestoring(false);
          restoreOver.settle();
        }
      })();
      return;
    }
    void (async () => {
      try {
        const asked = await commands
          .relaunchAsk()
          .catch(() => ({ status: "error" as const, error: "" }));
        const question = asked.status === "ok" ? asked.data : null;
        const choice: RelaunchChoice = question
          ? await new Promise<RelaunchChoice>((answer) =>
              setRelaunchAsking({
                question,
                answer: (chosen) => {
                  setRelaunchAsking(undefined);
                  answer(chosen);
                },
              }),
            )
          : "ReopenAll";
        // Awaited, so the launch's project has its chats before its tab asks what it holds.
        await commands.relaunch(choice).catch(() => undefined);
        // The launch's own project, which the core already opened and has now put the record
        // back for. Its tab is first and it is the one in front: the operator ran charter THERE.
        const opened = launch.plane;
        if (opened !== null) {
          setPlanes((was) => (was.includes(opened) ? was : [...was, opened]));
          setShowing({ at: "plane", plane: opened });
        }
        const answer = await commands
          .planesToRestore()
          .catch(() => ({ status: "error" as const, error: "" }));
        const back = answer.status === "ok" ? answer.data : undefined;
        setNotRestored(back?.dropped ?? []);
        setGoneAtLaunch(back?.gone ?? []);
        const [first, ...splits] = back?.windows ?? [];
        let front: PlaneId | undefined;
        for (const [at, path] of (first?.planes ?? []).entries()) {
          const plane = await openInto(path, false);
          if (plane !== undefined && at === first?.active) front = plane;
        }
        // **Every other remembered window comes back as a window of its own** (ADR 0033,
        // amended 2026-09-26). Each project is still opened here, through the same gate — a
        // project that raises the trust question asks it in this window, and lands here once
        // approved — and only then moved into its window. The launch's own project stays here.
        for (const window of splits) {
          const opened: PlaneId[] = [];
          let inFrontThere: PlaneId | null = null;
          for (const [at, path] of window.planes.entries()) {
            const plane = await openInto(path, false, false);
            if (plane === undefined || plane === launch.plane) continue;
            opened.push(plane);
            if (at === window.active) inFrontThere = plane;
          }
          if (opened.length > 0)
            await commands.moveProjects(opened, inFrontThere, null).catch(() => undefined);
        }
        // The remembered front tab, unless the launch already named one: a terminal launch
        // inside a project is the operator saying which project he means, and it outranks an
        // arrangement from yesterday.
        if (opened === null && front !== undefined) setShowing({ at: "plane", plane: front });
      } finally {
        // Whatever happened, the restore is over. A window that stayed `restoring` would
        // never write its arrangement down and would warn on every quit for the rest of the
        // day, which is a worse failure than the one that caused it.
        setRestoring(false);
        restoreOver.settle();
      }
    })();
  }, [launch, openInto, restoreOver, split]);

  // Nothing is ever drawn on a project this window does not hold. `showing` is set from
  // several places — a close, a restore, an approval — and a plane that went in between
  // would otherwise leave the window pointed at a project with no `PlaneView` behind it.
  const inFront =
    showing.at === "plane" && planes.includes(showing.plane) ? showing.plane : undefined;
  /** Whether the opener — and the window's own palette with it — is what this window draws.
   *  Only once the core has said what the launch resolved and the restore has finished
   *  opening what it remembered: both are about to decide whether there is a project here. */
  const openerUp = inFront === undefined && launch !== undefined && !restoring;
  useLayoutEffect(() => {
    inFrontNow.current = inFront;
    // A project arriving ends the stand-in: from here Settings is a tab, and the opener a later
    // close brings back must be the opener rather than a Settings left behind.
    if (inFront !== undefined) setSettingsAlone(false);
  }, [inFront]);
  /** What the project in front last said about itself, when it has said anything yet. The
   *  palette lists its catalogue and runs its rows, so a row reaches that project's live
   *  arrangement and no other's. */
  const saying = inFront === undefined ? undefined : reports[inFront];
  /** The workspace the project in front is on, when it is on one (charter-app#281): its
   *  `workspace.json` is a layer of the theme the window draws, between the project's two files. */
  const workspaceInFront = saying?.workspace;
  /** The project in front once it knows which workspace it is on — its plane read — and not
   *  before: asked for the project alone first, the window would draw the project's theme and
   *  then repaint in the workspace's a moment later, a flash at every launch. */
  const settledInFront = saying?.read === true ? inFront : undefined;
  /** What the project in front has on in that workspace (charter-app#253, #280), for the one
   *  thing that is the window's and not a project's to draw: the theme. */
  const onInFront = useExtensionsOn(settledInFront, workspaceInFront);
  /** The theme the project in front picked there (charter-app#273, #281): `null` when nothing
   *  picked one. */
  const pickInFront = useProjectTheme(settledInFront, workspaceInFront);
  // The theme the project and workspace in front have, drawn once they have said what they have
  // on and what they picked — and every approved extension's with no project in front, which is
  // what the window drew before projects had a say (ADR 0048). After the first frame, like every
  // extension theme (`Extensions.tsx`). A project or workspace switch redraws it, and
  // `theme.onDrawn` hands it to every terminal on screen (#216).
  useEffect(() => {
    if (inFront === undefined) void drawThemeFor("every");
    else if (onInFront !== undefined && pickInFront !== undefined)
      void drawThemeFor(onInFront, pickInFront);
  }, [inFront, onInFront, pickInFront]);
  // And the workspace in front's colour on the window's accent and focus ring (charter-app#281),
  // live on every switch. The terminal is not told: nothing it draws is tinted.
  const colourInFront = saying?.colour ?? null;
  useEffect(() => {
    drawTint(colourInFront);
  }, [colourInFront]);
  /** What the last action answered — the project in front's, or this window's own when there
   *  is no project in front to have one. */
  const said = saying?.said ?? report;

  // What this window holds and what it has in front, told to the core. It buys two things: a
  // notification about a chat in a project the operator is NOT looking at is sent rather than
  // suppressed, and the arrangement is written into this machine's store so the next cold
  // launch puts it back.
  //
  // **Not while the restore is still running.** The store is where the arrangement comes
  // FROM, so a window that reported "I hold nothing" on its first render would wipe the very
  // record it is about to read.
  useEffect(() => {
    if (restoring) return;
    const at = inFront === undefined ? -1 : planes.indexOf(inFront);
    void commands.windowHoldsPlanes({ planes, active: at < 0 ? null : at }).catch(() => undefined);
  }, [inFront, planes, restoring]);

  // A second launch handed its directory over (ADR 0033). It goes through the same opener
  // every other path uses, so the trust ask is the same ask — and it opens as another project
  // tab rather than being said on screen, which is what tabs were the missing half of.
  useEffect(() => {
    const listening = listen<string>("open-plane", (event) => {
      void restoreOver.done.then(() => openInto(event.payload, true));
    }).catch(() => undefined);
    return () => void listening.then((stop) => stop?.()).catch(() => undefined);
  }, [openInto, restoreOver]);

  // Projects moved into this window from another, or handed back by a split window that was
  // closed (charter#126). They are already open in the core; this window only draws them.
  useEffect(() => {
    const listening = listen<{ planes: PlaneId[]; front: PlaneId | null }>(
      "projects-arrived",
      (event) => {
        const { planes: arrived, front } = event.payload;
        setPlanes((was) => [...was, ...arrived.filter((plane) => !was.includes(plane))]);
        if (front !== null) setShowing({ at: "plane", plane: front });
      },
    ).catch(() => undefined);
    return () => void listening.then((stop) => stop?.()).catch(() => undefined);
  }, []);

  // The core let go of a project (#1242): by this window's own `×`, which has already taken
  // it out, or behind this window's back — the UI RPC, another window. Either way it is not
  // drawn a moment longer: its tab, its chats and its views go, and what comes next comes to
  // the front, as a close from here would.
  useEffect(() => {
    const listening = listen<{ plane: PlaneId }>("plane-closed", (event) =>
      takeOut(event.payload.plane),
    ).catch(() => undefined);
    return () => void listening.then((stop) => stop?.()).catch(() => undefined);
  }, [takeOut]);

  // Cold start ends when a person can see the window, which is the frame after the one this
  // paints in. The core answers with why that took as long as it did, when it took longer
  // than the limit, and with nothing at all otherwise — so on an ordinary launch this is one
  // IPC call that changes nothing. Nothing about the window depends on the marker arriving:
  // a window that cannot send it is still a window.
  useEffect(() => {
    let gone = false;
    const frame = requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        if (!gone)
          void commands
            .firstFrame()
            .then((why) => {
              if (!gone && why) setSlowStart(why);
            })
            .catch(() => undefined);
      }),
    );
    return () => {
      gone = true;
      cancelAnimationFrame(frame);
    };
  }, []);

  // What a quit would end, across every project this window holds. A warning that counted
  // only the project on screen would understate what it is about to end by however many
  // projects the operator had merged into the window.
  const ending = useMemo<Ending[]>(
    () => planes.flatMap((plane) => reports[plane]?.ending ?? []),
    [planes, reports],
  );
  /** Whether the core has answered, for every project, what it already had open. Before that
   *  "no tabs" is "not yet", and quitting on it would end chats the window had not drawn. */
  const settled =
    launch !== undefined && !restoring && planes.every((plane) => reports[plane]?.settled);

  // Read at the moment the event arrives rather than closed over, so the one listener below
  // is registered once and never torn down and rebuilt mid-quit. Every window's, not only this
  // one's (charter#126): it is set below, once the other windows' share is known.
  const atQuit = useRef({ settled, ending });

  // Something asked the app to quit: the menu, the tray, or Cmd-Q. The answer is the
  // operator's, and it is given here because this is where what would be ended is known.
  useEffect(() => {
    // The catch is attached here and not in the cleanup: a window that cannot listen is
    // still a window, and a rejection nothing is holding yet is an unhandled one.
    const listening = listen("quit-asked", () => {
      // Nothing to end is nothing to warn about — but only once every project has said what
      // it has open. Before that, no tabs means "not yet", and quitting on it would end every
      // chat the window had not drawn.
      if (atQuit.current.settled && atQuit.current.ending.length === 0)
        void commands.quit().catch(() => undefined);
      else setAsking(true);
    }).catch(() => undefined);
    // Unlistening can fail too — the window may be going away under it — and a cleanup
    // that throws into nothing is an unhandled rejection, not a diagnosis.
    return () => void listening.then((stop) => stop?.()).catch(() => undefined);
  }, []);

  // The app menu's Settings… (`⌘,`), which is the core's (`lifecycle.rs`) and says so with an
  // event, as its Quit does. The same verb the palette row runs.
  useEffect(() => {
    const listening = listen("settings-asked", () => windowDoes.openSettingsTab()).catch(
      () => undefined,
    );
    return () => void listening.then((stop) => stop?.()).catch(() => undefined);
  }, [windowDoes]);

  const quit = useCallback(() => {
    setAsking(false);
    void commands.quit().catch(() => undefined);
  }, []);

  /** Not now: the core is told, so the next ask warns again instead of quitting outright. */
  const dontQuit = useCallback(() => {
    setAsking(false);
    void commands.quitCancelled().catch(() => undefined);
  }, []);

  /**
   * What the window itself can do, for the surfaces that are drawn with no project in front.
   *
   * **The refusals are the point** (charter-app#111): outside a project the app stays useful
   * and refuses to start a chat, in charter's own words, rather than opening one somewhere it
   * guessed. Every other verb here belongs to a row the catalogue already marks unavailable
   * with no tabs and no plane, so it can only be reached by a surface that ignored
   * `available` — which `perform` checks again anyway.
   */
  const nowhere = (): Ran => ({
    ok: false,
    refused: "purlis has no plane open, so there is nowhere to start a chat.",
  });
  const windowDoing = useMemo<Doing>(
    () => ({
      // A refusal, not a silence: `perform` hands it back, `run` keeps it, and the palette
      // draws it beside the row that was pressed. Nothing is written to a second piece of
      // state that would then have to be cleared when a project arrives.
      newChat: () => undefined,
      newShell: () => undefined,
      split: () => undefined,
      closePane: () => undefined,
      closeTab: () => undefined,
      selectTab: () => undefined,
      renameTab: () => undefined,
      linkWorkItem: () => undefined,
      startFresh: () => undefined,
      restartChat: () => undefined,
      askPersona: () => undefined,
      unlinkWorkItem: async () => nowhere(),
      focusWorkspace: () => undefined,
      pickClone: () => undefined,
      focusBranch: () => undefined,
      newBranch: () => undefined,
      cloneMissing: async () => nowhere(),
      askDropMembership: () => undefined,
      newChatIn: () => undefined,
      // Both are rows the catalogue marks unavailable with no plane — there is nowhere to make
      // a workspace and no workspace to delete — so `perform` refuses them before either of
      // these is reached. They exist because `Doing` is one shape for every surface.
      createWorkspace: () => undefined,
      removeWorkspace: () => undefined,
      showChat: () => undefined,
      showTabTasks: () => undefined,
      // The queue is a project's, and there is no project here to have one.
      ignoreNeedsYou: async () => nowhere(),
      cancelSmartClose: async () => nowhere(),
      dismissStopped: () => undefined,
      stopChat: () => undefined,
      endTask: () => undefined,
      // A view is shown in a project's tab, and there is no project here. The rows that open one
      // do not exist without a plane, for the same reason the workspace rows above do not.
      openView: () => undefined,
      // A session record is resumed into a project, and there is no project here: its rows are
      // a project's catalogue's, and this one lists none.
      resumeSession: async () => nowhere(),
      // A memory is a project's, and its tab is shown in one: there is no project here, and no
      // memory rows in this catalogue.
      openMemory: () => undefined,
      editMemory: () => undefined,
      archiveMemory: async () => nowhere(),
      newMemory: () => undefined,
      keepTab: () => undefined,
      // An extension's action runs in a project, and there is no project here: its rows are
      // a project's catalogue's, and this one lists none.
      runAction: async () => nowhere(),
      // No plane, no vaults: both rows are unavailable without one.
      pickVault: () => undefined,
      createVault: () => undefined,
      removeVault: () => undefined,
      // No plane, no personas and no workspace: every row these answer is unavailable.
      createPersona: () => undefined,
      removePersona: () => undefined,
      setPersonaProfile: () => undefined,
      editPersona: async () => nowhere(),
      closeTodo: async () => nowhere(),
      forgetTodo: async () => nowhere(),
      pinTab: async () => nowhere(),
      pinWorkspace: async () => nowhere(),
      pinProject: windowDoes.pinProject,
      removeWorktree: async () => nowhere(),
      mergeWorktree: async () => nowhere(),
      declareWorktreeDone: async () => nowhere(),
      sendKey: async () => nowhere(),
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
      // A workspace is a project's, and there is no project here to have one.
      openWorkspaceSettings: () => undefined,
      switchLive: () => undefined,
      renameWorkspace: () => undefined,
      openSettingsTab: windowDoes.openSettingsTab,
      openYourSettings: windowDoes.openYourSettings,
      // A curation chat is opened in a project, and there is no project here.
      curate: async () => nowhere(),
      quit: windowDoes.quit,
      // A file row is a project's: there is no branch here to have one.
      copyPath: async () => nowhere(),
      revealPath: async () => nowhere(),
      openInEditor: async () => nowhere(),
      shellInFolder: () => undefined,
      startChatHere: async () => nowhere(),
    }),
    [windowDoes],
  );

  /**
   * The projects in the order the strip draws them: **pinned first** (ADR 0039).
   *
   * The same rule the chat strip follows (`tabs.tabsIn`) and for the same reason: nothing
   * moves that the operator did not move, and within each group the order is untouched.
   */
  const drawn = useMemo(
    () => [
      ...projects.filter((one) => pinnedProjects.includes(one.plane)),
      ...projects.filter((one) => !pinnedProjects.includes(one.plane)),
    ],
    [pinnedProjects, projects],
  );

  /**
   * The projects in the order the operator was last in them, the one in front first (FR-27).
   *
   * **The switcher's order, and nothing else's.** The strip's order never moves under the
   * operator's hand (ADR 0039); a switcher is a list read once, aimed at the last project, so
   * that the key and Enter is a switch back. Held for this window and never written down: it
   * is what this sitting did, and a cold launch starts it again from the project in front.
   */
  const [recent, setRecent] = useState<PlaneId[]>([]);
  useEffect(() => {
    if (inFront === undefined) return;
    setRecent((was) => (was[0] === inFront ? was : [inFront, ...was.filter((p) => p !== inFront)]));
  }, [inFront]);

  /** The rows the project strip draws. The same rows `catalogue` splices into the palette —
   *  one place the words and the availability are written down (`actions.projectRows`). */
  const strip = useMemo(
    () => projectRows(drawn, inFront, pinnedProjects, split),
    [drawn, inFront, pinnedProjects, split],
  );

  /**
   * **The switcher's rows** (FR-27): the strip's own `Switch to project …` rows — so the words,
   * the reason the one in front cannot run and what a row does are written once — in the order
   * the operator was last in each, with the path as the note, because two projects can share a
   * directory name and the name is all the row shows.
   */
  /**
   * ⌘P's files (FM-7): the scope ladder from the window's focus — the branch the project in
   * front has picked, that project, every open project — and a found file opened in its own
   * project, brought to the front. None while no project is open.
   */
  const branchInFront = saying?.branch;
  const files = useMemo(
    () =>
      planes.length === 0
        ? undefined
        : {
            ladder: scopeLadder(inFront, branchInFront, planes.length),
            nameOf: calledOn,
            onOpen: (file: FoundFile) => {
              setShowing({ at: "plane", plane: file.plane });
              setFileAsk((was) => ({
                plane: file.plane,
                place: placeOf(file),
                path: file.path,
                at: (was?.at ?? 0) + 1,
              }));
            },
          },
    [branchInFront, inFront, planes.length],
  );

  const switcherRows = useMemo(() => {
    const held = drawn.map((project) => project.plane);
    const order = [
      ...recent.filter((plane) => held.includes(plane)),
      ...held.filter((plane) => !recent.includes(plane)),
    ];
    return order.map((plane) => ({ ...strip.switchTo[held.indexOf(plane)], note: plane }));
  }, [drawn, recent, strip]);

  /**
   * The project strip's rows as one list, for the context menu on a project tab.
   *
   * **The strip's own rows and not a second reading of anything.** `strip` is
   * `actions.projectRows`, which is what the tabs, the `×` and the palette all draw; the menu
   * is `menuRows` filtering this same list to the project it was opened on. So a project in
   * front has "It is already in front." on its greyed row here for the same reason its tab
   * does not react, and neither fact is written twice.
   */
  const stripOffers = useMemo(
    () => [
      strip.open,
      strip.create,
      ...strip.switchTo,
      ...strip.pin,
      ...strip.settings,
      ...strip.saving,
      ...strip.window,
      ...strip.back,
      ...strip.close,
    ],
    [strip],
  );

  /** The same rows by id, which is what a project tab's menu looks one up in. Built once for
   *  the strip rather than scanned per tab per render — `actions.catalogued` has the numbers,
   *  measured on the chat strip where fifty of them are drawn at ADR 0026's limits. */
  const stripFound = useMemo(() => catalogued(stripOffers), [stripOffers]);

  /**
   * How wide the project strip is, less its own `+`, and therefore which projects it draws.
   *
   * The widest of the three (`LEAST.project`), because a project is the thing that holds the
   * other two: the operator asked for *"PROJECT is holder of workspaces, workspaces are
   * holder of sessions"*, and the shape is what says so before any word is read.
   */
  const { strip: projectStrip, controls: projectControls, width: room } = useRoom(drawn.length);
  const projectLeast = leastAt(LEAST.project, useTextSizes().window);
  const projectsShown = useMemo(
    () =>
      fitting(
        drawn,
        drawn.find((one) => one.plane === inFront),
        room,
        projectLeast,
      ),
    [drawn, inFront, room, projectLeast],
  );
  /**
   * A project tab dragged onto another (SI-6): the window's projects put in the new order, and
   * the project pinned or unpinned when it was carried across the boundary (`reorder.ts`).
   *
   * **The order is `planes`, and nothing new holds it.** The strip draws the window's projects
   * pinned first and otherwise in the order the window holds them, and that order is already
   * this machine's record of the window (`window_holds_planes`, ADR 0033) — so a dragged strip
   * is put back at the next cold launch by the record that already puts it back. A split
   * window's strip is its own window's, and is remembered as its own.
   */
  const dragProject = useCallback(
    (moved: string, onto: string) => {
      const made = afterDrop({
        whole: drawn.map((one) => one.plane),
        drawn: projectsShown.shown.map((one) => one.plane),
        moved,
        onto,
        isPinned: (plane) => pinnedProjects.includes(plane),
      });
      if (made === undefined) return;
      setPlanes(made.order as PlaneId[]);
      if (made.pinned === undefined) return;
      void pinProject(moved, made.pinned, { early: true }).then((ran) => {
        if (!ran.ok) setReport({ from: "project.drag", refused: true, words: ran.refused });
      });
    },
    [drawn, pinProject, pinnedProjects, projectsShown.shown],
  );
  const dragSensors = useStripSensors();
  const projectDragWords = useMemo(
    () => stripAccessibility("project", (id) => calledOn(String(id))),
    [],
  );

  const projectStop = useTabStop(
    inFront,
    projectsShown.shown.map((project) => project.plane),
  );

  /**
   * What the project strip's show-more menu lists: the projects it is not drawing, **most
   * recently moved first** — the workspace strip's rule one level up (ADR 0039, ADR 0054,
   * charter#401). `ShowMore` puts the ones that need you before these.
   *
   * A project moved when the newest of its chats did (`PlaneReport.moved`). One nothing has
   * been heard about reads `0` and keeps the strip's order: `sort` is stable. **The cost,
   * stated:** the core's count lives as long as the app, and reopening a chat is a move, so
   * right after a relaunch the projects rank by the order their chats were put back in until
   * one of them does something. The chat and workspace menus pay the same (ADR 0054).
   */
  const projectsNotShowing = useMemo(() => {
    const movedIn = (project: Project) => reports[project.plane]?.moved ?? 0;
    return [...projectsShown.hidden].sort((one, other) => movedIn(other) - movedIn(one));
  }, [projectsShown.hidden, reports]);

  /** How many chats in `project` need you: its tab's count, and its share of the count on the
   *  show-more button when the strip is not drawing it (ADR 0054). Its own report's queue for
   *  both, so the button goes down exactly when the tab would. */
  const askingIn = (project: Project) => reports[project.plane]?.asking.length ?? 0;

  /** What a project is drawn as, on the strip and in the menu of what the strip had no room
   *  for. One definition, because they are the same project. */
  const projectMarks = (project: Project) => (
    <>
      <span className="project-name">{project.name}</span>
      <Pin held={pinnedProjects.includes(project.plane)} what="project" />
      {/* Whether it has work not yet saved, or a save that is blocked (charter-app#302) —
          every project's, not only the one in front, because a project behind is where work
          is forgotten. */}
      <UnsavedMark saving={reports[project.plane]?.saving} name={project.name} />
      {/* What is waiting for you over there. It is the reason a project behind the one on
          screen goes on listening rather than being torn down. */}
      {askingIn(project) > 0 && (
        <span
          className="project-needs"
          data-needs={askingIn(project)}
          aria-label={`${askingIn(project)} chats need you in ${project.name}`}
        >
          {askingIn(project)}
        </span>
      )}
    </>
  );

  /** Every action a window with no project in front can do. The project's own catalogue is
   *  `PlaneView`'s; this is the one for the opener, and its refusals are #111's. */
  const openerOffers = useMemo(
    () =>
      catalogue({
        tabs: noTabs(),
        workspaces: [],
        projects: drawn,
        needsYou: [],
        pinned: { chats: [], workspaces: [], projects: pinnedProjects },
        nameOf: String,
        split,
      }),
    [drawn, pinnedProjects, split],
  );

  const run = useCallback(
    async (offer: Offer): Promise<Ran> => {
      const answer = await perform(offer, windowDoing);
      setReport(
        answer.ok
          ? answer.said
            ? { from: offer.id, refused: false, words: answer.said }
            : undefined
          : { from: offer.id, refused: true, words: answer.refused },
      );
      return answer;
    },
    [windowDoing],
  );

  const press = useCallback(
    (offer: Offer) => {
      void run(offer);
    },
    [run],
  );

  /**
   * The updater, **once for the whole window** (`Updates.tsx`, `TitleBar.tsx`).
   *
   * It used to be called inside every `PlaneView`, which made a window holding eight projects
   * hold eight updater clients for one app-wide fact — eight `update_channel` calls at open
   * and eight listeners for each `update://checked`. An offer is about the app, so it is asked
   * once, here, where the things that belong to the WINDOW live.
   */
  const updates = useUpdates();
  const titleBarRoom = useTitleBarRoom();

  /**
   * **Every project's chats asking, for the title bar's list** (charter-app#249), in the order
   * the project strip draws the projects and each project's queue in its own order.
   *
   * Out of the reports the window already holds, so the list costs no command of its own —
   * and a project behind the one on screen, which draws nothing, still reports its queue.
   */
  const needing = useMemo<Needing[]>(
    () =>
      planes.flatMap((plane) =>
        (reports[plane]?.asking ?? []).map((one) => ({ ...one, plane, project: calledOn(plane) })),
      ),
    [planes, reports],
  );

  /**
   * **Every project's permission prompts, answered from the same list** (HP-6): a chat's
   * `PermissionRequest` held on its hook, named as its tab names it.
   */
  const heldAsks = usePermissionAsks(planes);
  const asks = useMemo<PermissionAsk[]>(
    () =>
      planes.flatMap((plane) =>
        (heldAsks[plane] ?? []).map((one) => ({
          plane,
          project: calledOn(plane),
          session: one.session,
          name:
            reports[plane]?.ending.find((chat) => chat.key === `${plane}#${one.session}`)?.name ??
            `chat ${one.session}`,
          ask: one.ask,
          says: one.says,
          options: one.options,
        })),
      ),
    [planes, heldAsks, reports],
  );
  const answer = useCallback((ask: PermissionAsk, option: string) => {
    void answerAsk(ask.plane, ask.session, ask.ask, option).then((refused) => {
      if (refused !== undefined) setReport({ from: "needs.answer", refused: true, words: refused });
    });
  }, []);

  /** And the chats that can be waiting without saying so, for the faint hand (charter-app#52). */
  const quiet = useMemo<Quiet[]>(
    () =>
      planes.flatMap((plane) =>
        (reports[plane]?.quiet ?? []).map((name) => ({ name, project: calledOn(plane) })),
      ),
    [planes, reports],
  );

  /**
   * **What the other windows have asking and open** (charter#126). The ✋ list is every chat,
   * in every project, asking for the operator (ADR 0054) — and a project split into a window of
   * its own reports to that window, not to this one. So each window tells the others its share,
   * and draws theirs beside its own. A row a window has not caught up on — a project that has
   * just moved in here — is drawn once, from this window's own report.
   */
  const others = useOtherWindows({ needing, quiet, ending, settled });
  const everyNeeding = useMemo(
    () => [...needing, ...others.needing.filter((one) => !planes.includes(one.plane))],
    [needing, others.needing, planes],
  );
  const everyQuiet = useMemo(() => [...quiet, ...others.quiet], [quiet, others.quiet]);
  /** What a quit would end in every window: the main window is the one asked (`lifecycle.rs`),
   *  and a warning that left out a split window's chats would end them unannounced. */
  const everyEnding = useMemo(() => [...ending, ...others.ending], [ending, others.ending]);
  // In the same flush as the report that changed it, for the reason `PlaneView` reports in
  // one: a quit is not something the window gets to be a frame behind on. Settled only once
  // every other window has said it is: one that has said nothing yet is "not yet".
  useLayoutEffect(() => {
    atQuit.current = { settled: settled && others.settled, ending: everyEnding };
  });

  /**
   * A row off that list, carried out by the project it is about — through that project's own
   * `run`, so a Go and an Ignore are exactly the palette's rows.
   *
   * **A row that shows a chat shows its project first.** Go is "that chat, in front", and the
   * chat is only in front when its project is: the project's own `showChat` brings the tab and
   * its workspace forward, and this brings the project.
   */
  /**
   * **The project in front's save standing, for the title bar** (charter-app#294). The bar's
   * save button saves with the generated message; a refusal opens the project's Saving tab,
   * whose journal holds the refusal's own words.
   */
  // Read by the project itself and reported (charter-app#302), so the strip and the bar are
  // one reading of each project, not two.
  const saving = inFront === undefined ? undefined : reports[inFront]?.saving;
  /** The workspace in front's repos, which the indicator counts in (charter-app#299). Its save
   *  never saves them: a repo is a developer's, and pushing one is the Saving tab's Save all,
   *  asked first (ADR 0051, amended 2026-09-25). */
  const repoWorkspace = saying?.workspace;
  const repos = useRepoSaving(inFront, repoWorkspace);
  /** The project a save from the bar is running in: busy is that project's, not the window's. */
  const [savingIn, setSavingIn] = useState<PlaneId>();
  const saveInFront = useCallback(() => {
    const plane = inFrontNow.current;
    if (plane === undefined) return;
    setSavingIn(plane);
    void commands
      .savePlane(plane, null)
      .then((answer) => {
        if (answer.status === "error") windowDoes.openSaving(plane);
      })
      .finally(() => {
        setSavingIn(undefined);
        tellSaved();
      });
  }, [windowDoes]);

  const pressHere = useCallback((plane: string, offer: Offer) => {
    if (offer.does.verb === "showChat") setShowing({ at: "plane", plane });
    void reportsNow.current[plane]?.run(offer);
  }, []);
  /** A row for a chat in ANOTHER window is carried out there, and that window comes to the
   *  front (charter#126): the chat is in front only in the window holding its project. */
  const pressNeeding = useCallback(
    (plane: string, offer: Offer) => {
      if (planesNow.current.includes(plane)) pressHere(plane, offer);
      else void runElsewhere(plane, offer);
    },
    [pressHere],
  );
  useRunHere(pressHere);

  return (
    <main className="window">
      {/* The window's own title bar, and the project strip is in it (ADR 0054): on macOS it
          IS the title bar — the system's traffic lights float over it — and on every other
          platform it is the window's first row under the system's own bar. `TitleBar.tsx`
          argues the shape, the drag region and what moved here.

          The projects this window holds, as top-level tabs (ADR 0033). Drawn whenever it holds
          any — including one, because `+` is how it gets a second and `×` is the way back to
          the opener. Named, because the chat tabs and the workspaces are tablists too and a
          query for `role="tab"` across the whole window would mix all three. */}
      <TitleBar
        projects={
          planes.length > 0 && (
            // One Tab stop for the strip, the project in front, and the arrows along it
            // (charter-app#189, `roving.ts`). Its own controls after the tabs are stops of
            // their own.
            // Draggable along the strip (SI-6, `sortable.tsx`): a drop puts the window's
            // projects in a new order, and one carried across the pinned boundary pins or
            // unpins it. Every tab is a `<button>`, so Tauri's window drag never starts on one.
            <DndContext
              sensors={dragSensors}
              collisionDetection={closestCenter}
              modifiers={ALONG_THE_STRIP}
              accessibility={projectDragWords}
              onDragEnd={({ active, over }) => {
                if (over) dragProject(String(active.id), String(over.id));
              }}
            >
              <SortableContext
                items={projectsShown.shown.map((project) => project.plane)}
                strategy={horizontalListSortingStrategy}
              >
                <RovingFocusGroup.Root asChild orientation="horizontal" {...projectStop}>
                  <nav
                    className="projects"
                    role="tablist"
                    aria-label="Projects"
                    ref={projectStrip}
                    style={{ "--least": `${projectLeast}px` } as CSSProperties}
                  >
                    {projectsShown.shown.map((project) => {
                      const at = drawn.indexOf(project);
                      return (
                        <SortableTab key={project.plane} id={project.plane}>
                          {({ sortable, style }) => (
                            /* The cell: the tab, its gear on the project in front (SE-23)
                               and its `×`. It is what is dragged. */
                            <span
                              className="project"
                              ref={sortable.setNodeRef}
                              style={style}
                              data-dragging={sortable.isDragging || undefined}
                            >
                              {/* Right-click is the third reader of the same catalogue
                                  (`Menus.tsx`). `asChild`, and on the tab and not the cell: the
                                  context-menu key opens a menu only on the trigger that has the
                                  keyboard (`openFromTheKeyboard`), and that is the tab. */}
                              <Menued
                                on={{ on: "project", plane: project.plane }}
                                offers={stripFound}
                                onPress={press}
                              >
                                <RovingFocusGroup.Item
                                  asChild
                                  tabStopId={project.plane}
                                  active={project.plane === inFront}
                                >
                                  <button
                                    role="tab"
                                    aria-selected={project.plane === inFront}
                                    aria-describedby={sortable.attributes["aria-describedby"]}
                                    {...sortable.listeners}
                                    onKeyDown={(event) => {
                                      // A tab that is up is being carried: its keys are the drag's.
                                      keepsTheFocus(event, sortable.isDragging);
                                      sortable.listeners?.onKeyDown?.(event);
                                      if (!sortable.isDragging)
                                        closeOnDelete(event, strip.close[at], press);
                                    }}
                                    // The path, because two projects can share a directory name and
                                    // the name is all the tab has room for.
                                    title={project.plane}
                                    onClick={() => {
                                      const offer = strip.switchTo[at];
                                      if (offer.available) press(offer);
                                    }}
                                  >
                                    {projectMarks(project)}
                                  </button>
                                </RovingFocusGroup.Item>
                              </Menued>
                              {/* Its settings (SE-23, V89i): on the project in front only,
                                  and quiet until its tab is under the pointer or the
                                  keyboard. The row its menu's Project settings… runs. */}
                              {project.plane === inFront && (
                                <Gear offer={strip.settings[at]} onPress={press} />
                              )}
                              <Closer offer={strip.close[at]} onPress={press} />
                            </span>
                          )}
                        </SortableTab>
                      );
                    })}
                    {/* The strip's own controls, and the one part of this strip that never
                        collapses. They are inside the tablist because a `role="tab"` has to be owned
                        by the tablist it belongs to, so `useRoom` is told to take their width off the
                        room the tabs get rather than leaving the tabs to be squeezed under them.

                        **A `+` and not a labelled button** — the operator's: *"open-project button
                        is not looks like separate button, but it should looks like new tab, without
                        label — just icon."* It is the same shape as the chat strip's `New tab` one
                        level down: the `+` at the end of a strip makes one more of what the strip
                        lists. Its accessible name is still the catalogue's `Open a project…`.

                        **And beside it, the other half of the same sentence** — *"also we need to
                        have create new project button too"* (charter-app#178). Two controls and not
                        one menu: opening a project the operator already has and making one that does
                        not exist yet are different acts, and the second writes to disk. It is drawn
                        exactly as its neighbour is — one `Doer` over the catalogue's
                        `project.create`, icon-only, named `New project…` by the same row the palette
                        and the tab's menu read — so there is still one place those words are written
                        down. It is second because opening one is the commoner act; both are always
                        available, including with no project open, which is exactly the window that
                        needs them.

                        And the projects there was no room for, in the same component the chat strip
                        uses, so an operator learns one control for all three strips. */}
                    <span className="strip-doing" ref={projectControls}>
                      {/* The switcher (FR-27), first of the strip's controls because it is
                          about the tabs before it. Only once there is somewhere to switch to:
                          with one project it could only ever say so, and the palette's row
                          already does. */}
                      {drawn.length > 1 && <Doer offer={strip.switcher} onPress={press} iconOnly />}
                      <Doer offer={strip.open} onPress={press} iconOnly />
                      <Doer offer={strip.create} onPress={press} iconOnly />
                      <ShowMore
                        noun="project"
                        hidden={projectsNotShowing.map((project) => ({
                          key: project.plane,
                          offer: strip.switchTo[drawn.indexOf(project)],
                          needs: askingIn(project),
                          children: projectMarks(project),
                        }))}
                        onPress={press}
                      />
                    </span>
                  </nav>
                </RovingFocusGroup.Root>
              </SortableContext>
            </DndContext>
          )
        }
        updates={updates}
        room={titleBarRoom}
        chats={ending}
        needing={{
          items: everyNeeding,
          quiet: everyQuiet,
          onPress: pressNeeding,
          asks,
          onAnswer: answer,
          onOpen: (ask: PermissionAsk) =>
            pressNeeding(ask.plane, {
              id: `needs.show:${ask.session}`,
              title: `Show ${ask.name}, which asks for your permission`,
              available: true,
              reason: "",
              does: { verb: "showChat", session: ask.session },
              name: ask.name,
            }),
        }}
        save={
          inFront !== undefined && saving !== undefined
            ? {
                saving,
                repos,
                busy: savingIn === inFront,
                onOpen: () => windowDoes.openSaving(inFront),
                onSave: saveInFront,
              }
            : undefined
        }
      />
      {/* What the last action answered. Said here only while the palette is down: it is modal
          and draws over this line, and shows the same words itself rather than leaving the
          operator to guess at a sentence behind the overlay. One state, two places it can be
          drawn — never two states. */}
      {said && !paletteOpen && (
        <p
          className={said.refused ? "trouble" : "came-back"}
          role={said.refused ? "alert" : "status"}
        >
          {said.words}
        </p>
      )}

      {/* A launch nobody could see. The core only answers here when the start passed the
          spec's limit, so on an ordinary launch there is nothing to draw and nothing to
          dismiss. It is the one place an operator who clicked an icon can be told — the
          line purlis writes while it waits goes to standard error, which they do not have.
          Dismissible, because the launch is over and the news does not improve. Where the
          core knows the relaunch that avoids the wait, it is offered as Copy command (NO-4):
          the window has no fix for a launch already made, so this is V91q's last resort and
          `Notice.guard.test.ts` lists it as debt. */}
      {slowStart && (
        <Notice
          cause="slow-start"
          tone="trouble"
          copy={slowStart.relaunch ?? undefined}
          onDismiss={() => setSlowStart(undefined)}
        >
          {slowStart.said}
        </Notice>
      )}

      {/* A launch without the session bus, and what that run has not got (charter#746). */}
      <SessionBusNotice chats={ending} />

      {/* Vaults the launch left under the old name because macOS would have asked (#1306). */}
      <VaultsWaitingNotice />

      {/* A project the last quit had open that has moved or gone, with Locate… and Forget
          (NO-5). A line, never an error dialog: the record is a convenience and the project
          is the truth (ADR 0033). Said up here, because the window may well have come back on
          another project; when the opener is drawn and lists the same project, the opener's
          line is the one, and settling it there settles it here. */}
      {goneAtLaunch
        .filter(
          (gone) =>
            !(openerUp && !settingsAlone) ||
            (openerGone !== "unread" && !openerGone.includes(gone.path)),
        )
        .map((gone) => (
          <GoneProjectNotice
            key={gone.path}
            gone={gone}
            cause={`project-gone:${gone.path}`}
            onLocated={(found) => {
              goneSettled(gone.path);
              setGoneChanged((n) => n + 1);
              void openInto(found, true);
            }}
            onForgotten={() => {
              goneSettled(gone.path);
              setGoneChanged((n) => n + 1);
            }}
            onDismiss={() => goneSettled(gone.path)}
          />
        ))}
      {/* An entry the store itself would not take back, said the same way. */}
      {notRestored.map((line) => (
        <Notice
          key={line}
          cause={`not-restored:${line}`}
          onDismiss={() => setNotRestored((was) => was.filter((one) => one !== line))}
        >
          {line}
        </Notice>
      ))}

      {/* Every project this window holds. Only the one in front draws anything; the rest keep
          their tabs, their splits and their chat states and render nothing at all. */}
      {planes.map((plane) => (
        <PlaneView
          key={plane}
          plane={plane}
          inFront={plane === inFront}
          projects={drawn}
          pinnedProjects={pinnedProjects}
          window={windowDoes}
          onReport={onReport}
          alerts={alerts}
          contributed={contributedPanels}
          views={extensionViews}
          commands={extensionCommands}
          settingsAsked={settingsAsk?.plane === plane ? settingsAsk.at : undefined}
          savingAsked={savingAsk?.plane === plane ? savingAsk.at : undefined}
          settingsTabAsked={settingsTabAsk?.plane === plane ? settingsTabAsk.at : undefined}
          settingsLinkAsked={settingsLinkAsk?.plane === plane ? settingsLinkAsk : undefined}
          yourSettingsAsked={yourSettingsAsk?.plane === plane ? yourSettingsAsk.at : undefined}
          firstChatAsked={firstChat?.plane === plane ? firstChat : undefined}
          shellAsked={shellAsk?.plane === plane ? shellAsk : undefined}
          fileAsked={fileAsk?.plane === plane ? fileAsk : undefined}
        />
      ))}

      {/* What charter says is wrong in every project this window holds — over the whole
          window, opened from the status line's Alerts button. Drawn only while it is open. */}
      <AlertsDrawer
        open={alertsOpen}
        onOpenChange={setAlertsOpen}
        reading={alertsRead}
        aboutThisMachine={aboutThisMachine}
        planes={planes}
        nameOf={calledOn}
        does={alertsDo}
      />

      {/* No project in front: the opener, and nothing else. "No sessions" would be true and
          useless — there is nowhere to open one, and the thing the operator needs is the way
          to give the window a project.
          **Not before the core has answered, and not while the restore is still opening
          projects.** Both are about to decide whether this window has one, and an opener that
          flashed up in between would be purlis telling a newcomer there is nothing here half
          a second before eight projects arrive. */}
      {openerUp && (
        <div className="body">
          <div className="panes">
            {settingsAlone ? (
              <section className="view-pane" aria-label={SETTINGS_TAB_TITLE}>
                <header className="view-head">
                  <h2>{SETTINGS_TAB_TITLE}</h2>
                  <button type="button" tabIndex={0} onClick={() => setSettingsAlone(false)}>
                    Done
                  </button>
                </header>
                <div className="view-body">
                  <SettingsTab />
                </div>
              </section>
            ) : (
              <Opener
                here={!heldSomething && (launch?.here ?? false)}
                reason={launch?.reason ?? ""}
                adding={planes.length > 0}
                onOpen={(path) => void openInto(path, true)}
                onGoneListed={setOpenerGone}
                onGoneSettled={goneSettled}
                goneChanged={goneChanged}
                trouble={openTrouble}
                // The first run is for a window that has never held a project: one whose last
                // project was closed is somebody who has had one, and gets the opener.
                onOpenRepo={
                  heldSomething ? undefined : (path, template) => firstRunRepo(path, template)
                }
                onSignInToForge={(row) => {
                  setRepoTrouble(undefined);
                  void signInToForge(row).then(setRepoTrouble);
                }}
                openingRepo={openingRepo}
                repoTrouble={repoTrouble}
                repoForgeAsk={repoForgeAsk}
              />
            )}
          </div>
        </div>
      )}

      {/* What opening a project puts in force, and the question about it (ADR 0035).
          Nothing has been attached and nothing has been started while this is up: cancelling
          leaves the window exactly as it was. One at a time, oldest first. */}
      {approving[0] && (
        <ApprovePlane
          ask={approving[0]}
          onApprove={(ask) => void approveProject(ask)}
          onCancel={() =>
            setApproving((queue) => queue.filter((q) => q.path !== approving[0].path))
          }
        />
      )}

      {/* Making a project: a plane charter scaffolds, opened through the gate like any other
          (ADR 0035, spec decision 27). The window's, like the opener — what it ends in
          is a project this window holds — and mounted only while it is up. */}
      {creating && (
        <NewProject
          trouble={createTrouble}
          forgeAsk={createForgeAsk}
          onEdit={() => setCreateForgeAsk(undefined)}
          making={makingProject}
          onCreate={(path, planeIsThisRepo, adopt) =>
            void makeProject(path, planeIsThisRepo, adopt)
          }
          onOpenRepo={(path) => dialogRepo(path)}
          opening={openingRepo}
          onCancel={() => {
            setCreating(false);
            setCreateTrouble(undefined);
            setCreateForgeAsk(undefined);
          }}
        />
      )}

      {/* The primary input (spec decision 1), and there is exactly one of it.
          **Always mounted**, because what opens it is a keystroke it listens for itself, on
          the window, capture-phase — a palette the window had to decide to render would be
          one the operator could not reach from inside a pane's terminal, and one that waited
          for the core to answer would swallow the first `F2` of every launch.
          **Once**, because that listener claims `F2` from the whole window: a second palette
          mounted behind a project tab would open two on one keypress.
          What it lists is the project in front's own catalogue, which travels up with the
          rest of that project's report, and the window's own when there is none — whose
          refusals are #111's. */}
      <Palette
        offers={saying?.offers ?? openerOffers}
        projects={{ rows: switcherRows, asked: switcherAsk }}
        files={files}
        said={said}
        onRun={saying?.run ?? run}
        onOpened={setPaletteOpen}
      />

      {/* What has contributed what to this window (ADR 0041 item 5). Mounted only
          while it is asked for: it reads every installed extension's files to re-take its
          fingerprint, and a launch does not pay for that unless somebody looked. */}
      {extensions && <Extensions onClose={() => setExtensions(false)} />}

      {asking && <QuitWarning chats={everyEnding} onQuit={quit} onCancel={dontQuit} />}

      {closing !== undefined && (
        <ClosingProject
          name={calledOn(closing)}
          chats={reports[closing]?.ending ?? []}
          heard={reports[closing]?.settled ?? false}
          onClose={() => {
            const plane = closing;
            setClosing(undefined);
            void letGoOf(plane).then((answer) =>
              setReport(
                answer.ok
                  ? answer.said
                    ? { from: `project.close:${plane}`, refused: false, words: answer.said }
                    : undefined
                  : { from: `project.close:${plane}`, refused: true, words: answer.refused },
              ),
            );
          }}
          onCancel={() => setClosing(undefined)}
        />
      )}

      {relaunchAsking && (
        <RelaunchAsk
          question={relaunchAsking.question}
          nameOf={calledOn}
          onAnswer={relaunchAsking.answer}
        />
      )}
    </main>
  );
}

/**
 * What a project is called on its tab: the directory's own name.
 *
 * Two projects can share one, so the tab carries the whole path as its title and the strip is
 * never the only way to tell them apart. Both separators, because a `PlaneId` is a root as the
 * operating system spells it and Windows spells it with backslashes.
 */
function calledOn(plane: string): string {
  const parts = plane.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? plane;
}

export default App;
