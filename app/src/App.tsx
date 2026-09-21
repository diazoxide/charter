import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import { commands, type Ask, type PlaneId } from "./bindings";
import {
  catalogue,
  perform,
  projectRows,
  type Doing,
  type Offer,
  type Project,
  type Ran,
} from "./actions";
import { ApprovePlane } from "./ApprovePlane";
import { Opener } from "./Opener";
import { Palette } from "./Palette";
import { QuitWarning, type Ending } from "./QuitWarning";
import { Closer, Doer, Pin, PlaneView, type PlaneReport, type WindowDoing } from "./PlaneView";
import { noTabs } from "./tabs";

/**
 * The window, which holds projects.
 *
 * **A project is a plane and a window may hold several** (charter ADR 0033, spec decision 23).
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
  /** What the launch resolved, asked once. `undefined` while the core has not answered. */
  const [launch, setLaunch] = useState<{ plane: PlaneId | null; here: boolean; reason: string }>();
  /** The projects this window holds, left to right as the strip shows them. */
  const [planes, setPlanes] = useState<PlaneId[]>([]);
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
  /** Why this launch took longer than the limit, when it did — and nothing when it did not
   *  (charter-app#24). The core decides that; the window only draws it. */
  const [slowStart, setSlowStart] = useState<string>();
  /** Projects the last quit had open that charter would not take back, each with its line.
   *  Never an error dialog: a restore is a convenience (ADR 0033). */
  const [notRestored, setNotRestored] = useState<string[]>([]);
  /** Whether the cold-launch restore is still going. Until it is done the window has not
   *  finished saying which projects it holds, so neither the quit nor the arrangement it
   *  writes down may act on what it holds so far. */
  const [restoring, setRestoring] = useState(true);
  /** Whether this window has ever held a project.
   *
   *  After it has, the opener is no longer the launch reporting what it could not resolve:
   *  the operator closed what was open, and "charter found no project here" would be charter
   *  answering a question nobody asked about a directory nobody is standing in. */
  const [heldSomething, setHeldSomething] = useState(false);
  useEffect(() => {
    if (planes.length > 0) setHeldSomething(true);
  }, [planes.length]);

  /** The projects, as the strip and the palette name them. */
  const projects = useMemo<Project[]>(
    () => planes.map((plane) => ({ plane, name: calledOn(plane) })),
    [planes],
  );

  /**
   * The projects this operator has pinned, by root (charter ADR 0039).
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

  /** Pins or unpins one project. The core's refusal travels back whole — the store is
   *  bounded, and "unpin one first" is a sentence the operator can act on. */
  const pinProject = useCallback(async (plane: string, pinned: boolean): Promise<Ran> => {
    const answer = await commands
      .pinProject(plane, pinned)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (answer.status === "error") return { ok: false, refused: answer.error };
    setPinnedProjects((was) =>
      pinned ? (was.includes(plane) ? was : [...was, plane]) : was.filter((one) => one !== plane),
    );
    return { ok: true };
  }, []);

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
   */
  const openInto = useCallback(
    async (path: string, andShow: boolean): Promise<PlaneId | undefined> => {
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
      // A project already in the strip keeps its place: "open it" for one this window holds
      // means "show me that project", which is what a recents row and a second launch both
      // mean when they name one that is already a tab.
      setPlanes((was) => (was.includes(plane) ? was : [...was, plane]));
      if (andShow) setShowing({ at: "plane", plane });
      return plane;
    },
    [],
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
  }, []);

  /** Lets go of one project. Its chats end, its record is written into it, and its tab goes.
   *  Nothing of the project on disk goes. */
  const closeProject = useCallback(async (plane: string): Promise<Ran> => {
    const answer = await commands
      .closePlane(plane)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (answer.status === "error") return { ok: false, refused: answer.error };
    setPlanes((was) => {
      const at = was.indexOf(plane);
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
    // just been ended, and a quit warning listing them would be listing nothing.
    setReports((was) => {
      const { [plane]: gone, ...rest } = was;
      void gone;
      return rest;
    });
    return { ok: true, said: `charter let go of ${plane}. Nothing in it was changed.` };
  }, []);

  const onReport = useCallback((plane: PlaneId, mine: PlaneReport) => {
    setReports((was) => (was[plane] === mine ? was : { ...was, [plane]: mine }));
  }, []);

  const windowDoes = useMemo<WindowDoing>(
    () => ({
      openProject: () => setShowing({ at: "opener" }),
      selectProject: (plane: string) => setShowing({ at: "plane", plane }),
      closeProject,
      pinProject,
      quit: () => void commands.askToQuit().catch(() => undefined),
    }),
    [closeProject, pinProject],
  );

  // Which plane this launch opened — asked once, and the answer the first tab is built from.
  // The core resolved the working directory once to get it; nothing asks again.
  useEffect(() => {
    void commands
      .planeAtLaunch()
      .then((it) =>
        setLaunch({
          plane: it.plane,
          // A directory it could read, that is in no plane, versus nothing to go on.
          here: it.from !== null,
          reason: it.why ?? "charter has no plane open.",
        }),
      )
      // A command can also fail outright, with no answer of its own to give.
      .catch((err: unknown) => setLaunch({ plane: null, here: true, reason: String(err) }));
  }, []);

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
   * opened by the core before there is a window, and its tab has to be the first one.
   */
  const restored = useRef(false);
  useEffect(() => {
    if (launch === undefined || restored.current) return;
    restored.current = true;
    void (async () => {
      try {
        // The launch's own project, which the core already opened and put the record back
        // for. Its tab is first and it is the one in front: the operator ran charter THERE.
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
        let front: PlaneId | undefined;
        for (const [at, path] of (back?.planes ?? []).entries()) {
          const plane = await openInto(path, false);
          if (plane !== undefined && at === back?.active) front = plane;
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
      }
    })();
  }, [launch, openInto]);

  // Nothing is ever drawn on a project this window does not hold. `showing` is set from
  // several places — a close, a restore, an approval — and a plane that went in between
  // would otherwise leave the window pointed at a project with no `PlaneView` behind it.
  const inFront =
    showing.at === "plane" && planes.includes(showing.plane) ? showing.plane : undefined;
  /** Whether the opener — and the window's own palette with it — is what this window draws.
   *  Only once the core has said what the launch resolved and the restore has finished
   *  opening what it remembered: both are about to decide whether there is a project here. */
  const openerUp = inFront === undefined && launch !== undefined && !restoring;
  /** What the project in front last said about itself, when it has said anything yet. The
   *  palette lists its catalogue and runs its rows, so a row reaches that project's live
   *  arrangement and no other's. */
  const saying = inFront === undefined ? undefined : reports[inFront];
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
      void openInto(event.payload, true);
    }).catch(() => undefined);
    return () => void listening.then((stop) => stop?.()).catch(() => undefined);
  }, [openInto]);

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
  // is registered once and never torn down and rebuilt mid-quit.
  const atQuit = useRef({ settled, ending });
  // In the same flush as the report that changed it, for the reason `PlaneView` reports in
  // one: a quit is not something the window gets to be a frame behind on.
  useLayoutEffect(() => {
    atQuit.current = { settled, ending };
  });

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
    refused: "charter has no plane open, so there is nowhere to start a chat.",
  });
  const windowDoing = useMemo<Doing>(
    () => ({
      // A refusal, not a silence: `perform` hands it back, `run` keeps it, and the palette
      // draws it beside the row that was pressed. Nothing is written to a second piece of
      // state that would then have to be cleared when a project arrives.
      newChat: () => undefined,
      split: () => undefined,
      closePane: () => undefined,
      closeTab: () => undefined,
      selectTab: () => undefined,
      focusWorkspace: () => undefined,
      showChat: () => undefined,
      pinTab: async () => nowhere(),
      pinWorkspace: async () => nowhere(),
      pinProject: windowDoes.pinProject,
      removeWorktree: async () => nowhere(),
      mergeWorktree: async () => nowhere(),
      sendKey: async () => nowhere(),
      openProject: windowDoes.openProject,
      selectProject: windowDoes.selectProject,
      closeProject: windowDoes.closeProject,
      quit: windowDoes.quit,
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

  /** The rows the project strip draws. The same rows `catalogue` splices into the palette —
   *  one place the words and the availability are written down (`actions.projectRows`). */
  const strip = useMemo(
    () => projectRows(drawn, inFront, pinnedProjects),
    [drawn, inFront, pinnedProjects],
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
      }),
    [drawn, pinnedProjects],
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

  return (
    <main className="window">
      {/* The projects this window holds, as top-level tabs (ADR 0033). Drawn whenever it
          holds any — including one, because `+` is how it gets a second and `×` is the way
          back to the opener. Named, because the chat tabs and the workspaces are tablists
          too and a query for `role="tab"` across the whole window would mix all three. */}
      {planes.length > 0 && (
        <nav className="projects" role="tablist" aria-label="Projects">
          {drawn.map((project, at) => (
            <span className="project" key={project.plane}>
              <button
                role="tab"
                aria-selected={project.plane === inFront}
                // The path, because two projects can share a directory name and the name is
                // all the tab has room for.
                title={project.plane}
                onClick={() => {
                  const offer = strip.switchTo[at];
                  if (offer.available) press(offer);
                }}
              >
                <span className="project-name">{project.name}</span>
                <Pin held={pinnedProjects.includes(project.plane)} what="project" />
                {/* What is waiting for you over there. It is the reason a project behind the
                    one on screen goes on listening rather than being torn down. */}
                {(reports[project.plane]?.needsYou ?? 0) > 0 && (
                  <span
                    className="project-needs"
                    data-needs={reports[project.plane]?.needsYou}
                    aria-label={`${reports[project.plane]?.needsYou} chats need you in ${project.name}`}
                  >
                    {reports[project.plane]?.needsYou}
                  </span>
                )}
              </button>
              <Closer offer={strip.close[at]} onPress={press} />
            </span>
          ))}
          <Doer offer={strip.open} onPress={press} />
        </nav>
      )}

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
          line charter writes while it waits goes to standard error, which they do not have.
          Dismissible, because the launch is over and the news does not improve. */}
      {slowStart && (
        <p className="came-back trouble" role="status">
          {slowStart}{" "}
          <button type="button" className="dismiss" onClick={() => setSlowStart(undefined)}>
            Dismiss
          </button>
        </p>
      )}

      {/* A project the last quit had open that charter would not take back. A line, never an
          error dialog: the record is a convenience and the project is the truth (ADR 0033).
          Said up here rather than on the opener, because the window may well have come back
          on another project and the operator would never see it there. */}
      {notRestored.map((line) => (
        <p className="came-back" role="status" key={line}>
          {line}
        </p>
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
        />
      ))}

      {/* No project in front: the opener, and nothing else. "No sessions" would be true and
          useless — there is nowhere to open one, and the thing the operator needs is the way
          to give the window a project.
          **Not before the core has answered, and not while the restore is still opening
          projects.** Both are about to decide whether this window has one, and an opener that
          flashed up in between would be charter telling a newcomer there is nothing here half
          a second before eight projects arrive. */}
      {openerUp && (
        <div className="body">
          <div className="panes">
            <Opener
              here={!heldSomething && (launch?.here ?? false)}
              reason={launch?.reason ?? ""}
              adding={planes.length > 0}
              onOpen={(path) => void openInto(path, true)}
              trouble={openTrouble}
            />
          </div>
        </div>
      )}

      {/* What opening a project puts in force, and the question about it (charter ADR 0035).
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
        said={said}
        onRun={saying?.run ?? run}
        onOpened={setPaletteOpen}
      />

      {asking && <QuitWarning chats={ending} onQuit={quit} onCancel={dontQuit} />}
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
