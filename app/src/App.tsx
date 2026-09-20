import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Group, Panel, Separator } from "react-resizable-panels";
import "./App.css";
import {
  commands,
  type Ask,
  type ChatWorktree,
  type OpenChat,
  type PlaneId,
  type Sidebar as SidebarModel,
  type StartOptions,
} from "./bindings";
import {
  catalogue,
  perform,
  PASS_THROUGH_BYTES,
  PASS_THROUGH_KEY,
  type Doing,
  type Offer,
  type Ran,
} from "./actions";
import { ApprovePlane } from "./ApprovePlane";
import { Opener } from "./Opener";
import { Palette } from "./Palette";
import { StartChat } from "./StartChat";
import { QuitWarning } from "./QuitWarning";
import { SessionPane } from "./SessionPane";
import { Sidebar } from "./Sidebar";
import {
  closeFocusedPane,
  closeTab,
  focusPane,
  noTabs,
  openTab,
  panesOf,
  selectTab,
  splitFocusedPane,
  type Direction,
  type Layout,
  type Tabs,
} from "./tabs";
import { ChatState, NeedsYou } from "./NeedsYou";
import { Panels } from "./Panels";
import { quietOnes, stateOf, useChatStates } from "./chatState";

/**
 * What this window is showing, plane-wise.
 *
 * **"No plane" is a state and not a failure.** The core comes up holding none whenever the
 * launch had no plane to resolve, and the two ways that happens are different things to tell
 * an operator: `here` is a directory that is in no plane, and not-`here` is a launch that had
 * nothing to go on at all — an app started from the dock, whose working directory is `/`.
 * The `Opener` draws both, and it draws them differently: one is an answer to a question the
 * operator asked, and the other is a newcomer's first screen, which must not be an error about
 * a concept they do not have yet.
 */
type Plane =
  | { state: "loading" }
  | { state: "open"; plane: PlaneId }
  | { state: "none"; here: boolean; reason: string };

/** The size a session starts at. The pane it lands in tells it the real one at once. */
const STARTING_SIZE = { columns: 80, rows: 24 };

function App() {
  const [plane, setPlane] = useState<Plane>({ state: "loading" });
  /** The plane every command below names. There is no "current plane" in the core: a command
   *  that does not carry one cannot act on anything. */
  const planeId = plane.state === "open" ? plane.plane : undefined;
  const [tabs, setTabs] = useState<Tabs>(noTabs);
  /** What every chat is doing, in THIS plane. Pushed from the core; nothing here polls. */
  const states = useChatStates(planeId);
  const [trouble, setTrouble] = useState<string>();
  const [sidebar, setSidebar] = useState<SidebarModel>();
  const [focused, setFocused] = useState<string>();
  /** The chats the core put back at this launch, so a pane can say which came back how. */
  const [reopened, setReopened] = useState<OpenChat[]>([]);
  /** Whether the operator is being asked about quitting. */
  const [asking, setAsking] = useState(false);
  /** What opening a project will put in force, while the operator is being asked about it.
   *  Nothing is attached and nothing is started until they answer (charter ADR 0035). */
  const [approving, setApproving] = useState<Ask>();
  /** Why the last attempt to open a project opened nothing. Shown on the opener, which is
   *  where the operator is standing when it happens. */
  const [openTrouble, setOpenTrouble] = useState<string>();
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
  /** Why this launch took longer than the limit, when it did — and nothing when it did not
   *  (charter-app#24). The core decides that; the window only draws it. */
  const [slowStart, setSlowStart] = useState<string>();
  /** What the core last said about where a chat is working, and which directory it was
   *  asked about — so an answer about the chat that WAS in front is never drawn under the
   *  one that is now. `Panels` keys its answers the same way, for the same reason. */
  const [located, setLocated] = useState<{ cwd: string; piece?: ChatWorktree }>();
  /** Bumped when something changed the answer, so it is asked again rather than guessed. */
  const [relocate, setRelocate] = useState(0);
  /** What the last action answered: one line, or a refusal in the words it came in. */
  const [report, setReport] = useState<{ from: string; refused: boolean; words: string }>();
  /** Whether the palette is up, so the report is said in one place rather than two. */
  const [paletteOpen, setPaletteOpen] = useState(false);
  /** Whether the core has answered what it already has open. Until it has, "no tabs" is
   *  "not yet", which is not the same thing as "nothing is running". */
  const settled = useRef(false);

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
      if (next.inFront !== wasInFront && planeId !== undefined) {
        const front = next.inFront === undefined ? undefined : next.byId[next.inFront];
        const pane = front && panesOf(next, front.id)[0];
        void commands.chatInFront(planeId, pane ? pane.session : null).catch(() => undefined);
      }
      return next;
    },
    [planeId],
  );

  // What the core already has open, which at a launch is the record put back before there
  // was a window. The window draws them; it never ends them — a reload during development,
  // or a crash in the window, would otherwise take the day's sessions with it.
  //
  // Once per plane, and not once per window: a plane can arrive long after the launch now
  // that there is an opener, so this is keyed on WHICH plane was adopted rather than on
  // whether anything was. React also runs an effect twice in development, and the same ref
  // answers that — a second pass for the same plane would draw every chat again.
  const adopted = useRef<PlaneId | undefined>(undefined);
  useEffect(() => {
    // Nothing is asked until the core has said which plane this launch opened, if any:
    // there is no plane to ask about before that, and no default one to fall back on.
    if (plane.state === "loading") return;
    if (planeId === undefined) {
      // No plane, so nothing was put back and nothing is running. That is settled, and a
      // quit from here has nothing to warn about.
      settled.current = true;
      return;
    }
    if (adopted.current === planeId) return;
    adopted.current = planeId;
    void commands
      .openedChats(planeId)
      .then((answer) => {
        settled.current = true;
        void commands
          // A window that cannot ask, or is answered with nothing, simply says nothing.
          .chatsThatWouldNotStart(planeId)
          .then((trouble) => setWouldNotStart(trouble.status === "ok" ? (trouble.data ?? []) : []))
          .catch(() => undefined);
        const open = answer.status === "ok" ? (answer.data ?? []) : [];
        if (open.length === 0) return;
        setReopened(open);
        const drawn = open.reduce((tabs, chat) => openTab(tabs, chat.session, chat.name), noTabs());
        const front = open.findIndex((chat) => chat.in_front);
        change(() => (front < 0 ? drawn : selectTab(drawn, drawn.order[front])));
      })
      // Nothing open is the ordinary first launch, and a window that cannot ask is still
      // a window the operator can open a chat in.
      .catch(() => {
        settled.current = true;
      });
  }, [change, plane.state, planeId]);

  // Something asked the app to quit: the menu, the tray, or Cmd-Q. The answer is the
  // operator's, and it is given here because this is where what would be ended is known.
  useEffect(() => {
    // The catch is attached here and not in the cleanup: a window that cannot listen is
    // still a window, and a rejection nothing is holding yet is an unhandled one.
    const listening = listen("quit-asked", () => {
      // Nothing to end is nothing to warn about — but only once the core has said what it
      // has open. Before that, no tabs means "not yet", and quitting on it would end every
      // chat the window had not drawn.
      if (settled.current && now.current.order.length === 0)
        void commands.quit().catch(() => undefined);
      else setAsking(true);
    }).catch(() => undefined);
    // Unlistening can fail too — the window may be going away under it — and a cleanup
    // that throws into nothing is an unhandled rejection, not a diagnosis.
    return () => void listening.then((stop) => stop?.()).catch(() => undefined);
  }, []);

  // The sidebar is read from the plane, and re-read whenever the chats change: the plane is a
  // directory the operator also edits by hand and another charter process writes, so there is
  // nothing to invalidate a cache of it. `tabs` is the dependency because opening or ending a
  // chat is what this window can change about the answer.
  useEffect(() => {
    // A window with no plane has no sidebar to draw, and asking for one would be asking
    // which of no planes. Nothing is cleared here: the sidebar starts empty, and clearing
    // state from inside an effect is a render the window does not need.
    if (planeId === undefined) return;
    void commands
      .planeSidebar(planeId)
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
        // Focus follows the plane rather than being guessed: the first workspace, until
        // somebody picks another and while that one still exists.
        setFocused((current) =>
          current && next.workspaces.some((ws) => ws.name === current)
            ? current
            : next.workspaces[0]?.name,
        );
      })
      // A window with no readable plane still runs its panes; the header already says so.
      .catch(() => setSidebar(undefined));
  }, [planeId, tabs]);

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

  // Which plane this launch opened — asked once, and the answer every command below carries.
  // The core resolved the working directory once to get it; nothing asks again.
  useEffect(() => {
    void commands
      .planeAtLaunch()
      .then((launch) => {
        setPlane(
          launch.plane !== null
            ? { state: "open", plane: launch.plane }
            : {
                state: "none",
                // A directory it could read, that is in no plane, versus nothing to go on.
                here: launch.from !== null,
                reason: launch.why ?? "charter has no plane open.",
              },
        );
      })
      // A command can also fail outright, with no answer of its own to give.
      .catch((err: unknown) => setPlane({ state: "none", here: true, reason: String(err) }));
  }, []);

  /**
   * Asks the core to open a project, and draws whatever it answers with.
   *
   * **It never decides whether to ask.** The core reads this machine's record of what the
   * operator approved, against the project as it is on disk at that instant, and answers with
   * either the plane or the question. A window that formed its own opinion about trust would
   * be a second gate beside the one that bites.
   *
   * The one path for all four ways in: a recents row, a picked folder, a typed path, and a
   * second launch handing its directory to this process.
   */
  const openProject = useCallback(async (path: string) => {
    setOpenTrouble(undefined);
    const answer = await commands
      .openPlane(path)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    // Verbatim: the sentence names the path and what was wrong with it, and an operator shown
    // a reworded version of it can neither act on it nor search for it.
    if (answer.status === "error") {
      setOpenTrouble(answer.error);
      return;
    }
    if (answer.data.plane !== null) {
      setApproving(undefined);
      setPlane({ state: "open", plane: answer.data.plane });
      return;
    }
    setApproving(answer.data.ask ?? undefined);
  }, []);

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
    setApproving(undefined);
    if (answer.status === "error") {
      setOpenTrouble(answer.error);
      return;
    }
    setOpenTrouble(undefined);
    setPlane({ state: "open", plane: answer.data });
  }, []);

  /** Lets go of the project this window is showing. Its chats end, its record is written into
   *  it, and the opener comes back. Nothing of the project on disk goes. */
  const closeProject = useCallback(async (): Promise<Ran> => {
    if (planeId === undefined)
      return { ok: false, refused: "No project is open, so there is none to close." };
    const answer = await commands
      .closePlane(planeId)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (answer.status === "error") return { ok: false, refused: answer.error };
    // The window forgets what it was drawing for that plane. Its tabs named sessions in it,
    // and a session number means nothing without its plane.
    change(() => noTabs());
    setReopened([]);
    setWouldNotStart([]);
    setSidebar(undefined);
    setPlane({ state: "none", here: false, reason: "You closed the last project." });
    return { ok: true, said: `charter let go of ${planeId}. Nothing in it was changed.` };
  }, [change, planeId]);

  // Which plane this window has in front, told to the core so a notification about a chat in
  // a plane the operator is NOT looking at is sent rather than suppressed. The window is the
  // only thing that knows, which is why it says rather than being asked.
  useEffect(() => {
    void commands.windowShowsPlane(planeId ?? null).catch(() => undefined);
  }, [planeId]);

  // A second launch handed its directory over (ADR 0033). It goes through the same opener
  // every other path uses, so the trust ask is the same ask.
  useEffect(() => {
    const listening = listen<string>("open-plane", (event) => {
      // With one plane per window, a second launch naming the plane already open is
      // answered by the core with the id it already has and nothing is bound twice. Naming
      // a DIFFERENT one is what project tabs are for, and they are not here yet — so it is
      // said rather than silently thrown away, which is what used to happen to it.
      if (planeId !== undefined) {
        setTrouble(
          `charter is showing ${planeId}. Close this project to open ${event.payload}, or ` +
            `wait for project tabs to hold both.`,
        );
        return;
      }
      void openProject(event.payload);
    }).catch(() => undefined);
    return () => void listening.then((stop) => stop?.()).catch(() => undefined);
  }, [openProject, planeId]);

  // Where a chat starts: the focused workspace's directory, so the sidebar can file it under
  // that workspace. Nothing on the plane records a chat, so where it works is the only thing
  // relating the two. Null — the operator's home — until a plane is read.
  const startIn = sidebar?.workspaces.find((ws) => ws.name === focused)?.path ?? null;

  /** Asks which profile and which persona. It starts nothing by itself. */
  const ask = useCallback(
    async (where: { tab: true } | { split: Direction }) => {
      setPickerTrouble(undefined);
      if (planeId === undefined) {
        setTrouble("charter has no plane open, so there is nowhere to start a chat.");
        return;
      }
      const options = await commands
        .startOptions(planeId)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (options.status === "error") {
        setTrouble(options.error);
        return;
      }
      setPicking({ options: options.data, where });
    },
    [planeId],
  );

  const newTab = useCallback(() => void ask({ tab: true }), [ask]);

  /** A row was picked: the chat starts on that profile, with that persona, and either
   *  drawing charter's footer in its pane or leaving it blank (charter ADR 0029). */
  const startPicked = useCallback(
    async (profile: string, persona: string | null, showFooter: boolean) => {
      const where = picking?.where;
      if (where === undefined || planeId === undefined) return;
      const inFront = now.current.inFront;
      const name =
        "split" in where && inFront !== undefined
          ? now.current.byId[inFront].name
          : String(now.current.named.tabs + 1);
      const started = await commands
        .startChat(
          planeId,
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
      if ("tab" in where) {
        change((tabs) => openTab(tabs, session, name));
        return;
      }
      const before = now.current;
      // The tab that was to be split can have closed while the picker was open. Nothing
      // would show that session, so it is ended rather than left running unseen.
      if (change((tabs) => splitFocusedPane(tabs, where.split, session)) === before) {
        void commands.closeSession(planeId, session);
      }
    },
    [change, picking, planeId, startIn],
  );

  /** The approval IS this click. After it, the whole chain of checks runs again from the
   *  top before anything is exec'd, so a yes never walks past a refusal standing behind it. */
  const approveAndStart = useCallback(
    async (profile: string, persona: string | null, showFooter: boolean, shown: string) => {
      if (planeId === undefined) return;
      const said = await commands
        .approveProfile(planeId, profile, shown)
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
    [planeId, startPicked],
  );

  const quit = useCallback(() => {
    setAsking(false);
    void commands.quit().catch(() => undefined);
  }, []);

  /** Not now: the core is told, so the next ask warns again instead of quitting outright. */
  const dontQuit = useCallback(() => {
    setAsking(false);
    void commands.quitCancelled().catch(() => undefined);
  }, []);

  const split = useCallback((direction: Direction) => void ask({ split: direction }), [ask]);

  const closePane = useCallback(() => {
    const tab =
      now.current.inFront === undefined ? undefined : now.current.byId[now.current.inFront];
    const going = tab && panesOf(now.current, tab.id).find((pane) => pane.pane === tab.focused);
    change(closeFocusedPane);
    if (going && planeId !== undefined) void commands.closeSession(planeId, going.session);
  }, [change, planeId]);

  const close = useCallback(
    (id: number) => {
      const ending = panesOf(now.current, id);
      change((tabs) => closeTab(tabs, id));
      if (planeId === undefined) return;
      for (const pane of ending) void commands.closeSession(planeId, pane.session);
    },
    [change, planeId],
  );

  /** Brings the tab holding a chat to the front. The queue and the palette both use it. */
  const showChat = useCallback(
    (session: number) => {
      const tab = now.current.order.find((id) =>
        panesOf(now.current, id).some((pane) => pane.session === session),
      );
      if (tab !== undefined) change((tabs) => selectTab(tabs, tab));
    },
    [change],
  );

  /** What a chat is called here: the tab holding it, or its session number. */
  const nameOf = useCallback(
    (session: number) =>
      tabs.order
        .filter((id) => panesOf(tabs, id).some((pane) => pane.session === session))
        .map((id) => tabs.byId[id].name)[0] ?? String(session),
    [tabs],
  );

  const bringToFront = useCallback((id: number) => change((tabs) => selectTab(tabs, id)), [change]);

  const inFront = tabs.inFront === undefined ? undefined : tabs.byId[tabs.inFront];
  // The session the next worktree question is about: the chat in the pane that has the
  // keyboard, which is the one "this chat's worktree" means.
  const frontSession = inFront
    ? panesOf(tabs, inFront.id).find((pane) => pane.pane === inFront.focused)?.session
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
  useEffect(() => {
    if (frontCwd === null) return;
    let gone = false;
    void commands
      .worktreeOfChat(frontCwd)
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
  }, [frontCwd, relocate]);

  // Only an answer about the directory in front. Nothing is cleared when the focus moves —
  // clearing state from inside an effect is a render the window does not need, and a stale
  // answer is simply not this chat's.
  const worktree = located?.cwd === frontCwd ? located.piece : undefined;

  /** Removes the piece the chat in front works in. The core's refusal travels back whole. */
  const removeWorktree = useCallback(
    async (force: boolean): Promise<Ran> => {
      if (!worktree || planeId === undefined)
        return { ok: false, refused: "There is no worktree in front to remove." };
      const answer = await commands
        .worktreeRemove(planeId, worktree.workspace, worktree.repo, worktree.piece, force)
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
    [planeId, worktree],
  );

  /** Lands the piece in its clone, fast-forward only. The core never pushes. */
  const mergeWorktree = useCallback(async (): Promise<Ran> => {
    if (!worktree || planeId === undefined)
      return { ok: false, refused: "There is no worktree in front to merge." };
    const answer = await commands
      .worktreeMerge(planeId, worktree.workspace, worktree.repo, worktree.piece)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (answer.status === "error") return { ok: false, refused: answer.error };
    return {
      ok: true,
      said: `${answer.data.branch} landed: ${answer.data.was} → ${answer.data.now}`,
    };
  }, [planeId, worktree]);

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
      if (planeId === undefined)
        return { ok: false, refused: "charter has no plane open, so there is nowhere to send it." };
      const sent = await commands
        .sendInput(planeId, frontSession, PASS_THROUGH_BYTES)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      // Verbatim: a session that has stopped reading its input says so in the core's words.
      if (sent.status === "error") return { ok: false, refused: sent.error };
      // Nothing is said. The chat's own answer to the key is the report, and a banner after
      // every press of a key an operator means to press repeatedly is noise.
      return { ok: true };
    },
    [frontSession, planeId],
  );

  const doing = useMemo<Doing>(
    () => ({
      newChat: newTab,
      split,
      closePane,
      closeTab: close,
      selectTab: bringToFront,
      focusWorkspace: setFocused,
      showChat,
      removeWorktree,
      mergeWorktree,
      sendKey,
      closeProject,
      quit: () => void commands.askToQuit().catch(() => undefined),
    }),
    [
      bringToFront,
      close,
      closePane,
      closeProject,
      mergeWorktree,
      newTab,
      removeWorktree,
      sendKey,
      showChat,
      split,
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

  /**
   * Every action the window can do, in one list.
   *
   * **The bar's buttons are rows of THIS list, not a second one.** The tmux frame kept a
   * menu beside its palette once and the two drifted; here `New tab`, the splits, `Close
   * pane` and every tab's `×` are looked up by id out of the same catalogue the palette
   * draws, so a row that goes away takes its button with it.
   */
  const offers = useMemo(
    () =>
      catalogue({
        tabs,
        workspaces: sidebar?.workspaces.map((ws) => ws.name) ?? [],
        focused,
        worktree,
        plane: planeId,
        // Only a refusal the REMOVAL gave, and only while it is still on screen: the
        // discard row is the operator's answer to a sentence they have read.
        refusal: report?.refused && report.from === "worktree.remove" ? report.words : undefined,
        needsYou: states.needsYou,
        quiet,
        nameOf,
      }),
    [focused, nameOf, planeId, quiet, report, sidebar, states.needsYou, tabs, worktree],
  );

  const by = useCallback((id: string) => offers.find((offer) => offer.id === id), [offers]);

  /** What every surface does with a row: carry it out, and keep what it answered.
   *
   *  One function for the bar and for the palette. It is called from an event handler and
   *  never while rendering, which is what lets the verbs it dispatches to reach the window's
   *  live arrangement rather than a copy taken when the row was built. */
  const run = useCallback(
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

  const press = useCallback(
    (offer: Offer) => {
      void run(offer);
    },
    [run],
  );

  // The chats still open, in the order the tab bar shows them: what a quit would end.
  const open = tabs.order.flatMap((id) =>
    panesOf(tabs, id).map(({ session }) => chatOf(reopened, session, tabs.byId[id].name)),
  );
  const frontChat =
    inFront && reopened.find((chat) => chat.session === panesOf(tabs, inFront.id)[0]?.session);

  return (
    <main className="window">
      <header className="bar">
        {/* Named, because the sidebar lists workspaces as a tablist too and a query for
            `role="tab"` across the whole window would mix the two. */}
        <div className="tabs" role="tablist" aria-label="Tabs">
          {tabs.order.map((id) => (
            <span className="tab" key={id}>
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
                {/* The first pane's session is the tab's own chat. Its own element, so what
                    a tab IS stays separate from what it is DOING — a tab whose text changed
                    every time a turn began would be unreadable, and untestable. */}
                <ChatState state={stateOf(states, panesOf(tabs, id)[0]?.session ?? -1)} />
              </button>
              <Closer offer={by(`tab.close:${id}`)} onPress={press} />
            </span>
          ))}
          <Doer offer={by("chat.new")} onPress={press} />
        </div>
        <div className="doing">
          <Doer offer={by("pane.split.right")} onPress={press} />
          <Doer offer={by("pane.split.down")} onPress={press} />
          <Doer offer={by("pane.close")} onPress={press} />
        </div>
        <NeedsYou queue={states.needsYou} quiet={quiet} nameOf={nameOf} show={showChat} />
        <span className="plane">
          {plane.state === "open" && <code>{plane.plane}</code>}
          {/* Nothing is said here about a window with no project. The `Opener` below is what
              that window draws, and it says which of the two states it is in — the difference
              between a directory that is in no project and a launch that had nothing to go on
              is a difference an operator reads once, in the place they act on it. */}
        </span>
      </header>

      {trouble && (
        <p className="trouble" role="alert">
          {trouble}
        </p>
      )}

      {/* What the last action answered. Said HERE only while the palette is down: the
          palette is modal and draws over this line, so it shows the same words itself
          rather than leaving the operator to guess at a sentence behind the overlay. One
          state, two places it can be drawn — never two states. */}
      {report && !paletteOpen && (
        <p
          className={report.refused ? "trouble" : "came-back"}
          role={report.refused ? "alert" : "status"}
        >
          {report.words}
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

      {wouldNotStart.map(([name, why]) => (
        <p className="came-back trouble" role="status" key={name}>
          <strong>{name}</strong> did not start ({why}). It is still recorded, and will be tried
          again at the next launch.
        </p>
      ))}

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

      <div className="body">
        {sidebar && (
          <Sidebar sidebar={sidebar} states={states} focused={focused} onFocus={setFocused} />
        )}
        <div className="panes">
          {/* A window with no project draws the opener and nothing else. "No sessions" would
              be true and useless: there is nowhere to open one, and the thing the operator
              needs is the way to give the window a project. */}
          {plane.state === "none" ? (
            <Opener
              here={plane.here}
              reason={plane.reason}
              onOpen={(path) => void openProject(path)}
              trouble={openTrouble}
            />
          ) : inFront ? (
            <LayoutPanes
              plane={planeId}
              layout={inFront.layout}
              focused={inFront.focused}
              onFocus={(pane) => change((tabs) => focusPane(tabs, pane))}
            />
          ) : (
            <p className="empty">No sessions. Open one with New tab.</p>
          )}
        </div>
        {/* The right-hand side, which reads the plane for whichever workspace is focused.
            Its own component with its own state: it asks the core twice — once for what the
            plane holds and once for what git says — and neither ask belongs up here. */}
        <Panels workspace={focused} />
      </div>

      {/* What opening a project puts in force, and the question about it (charter ADR 0035).
          Nothing has been attached and nothing has been started while this is up: cancelling
          leaves the window exactly as it was. */}
      {approving && (
        <ApprovePlane
          ask={approving}
          onApprove={(ask) => void approveProject(ask)}
          onCancel={() => setApproving(undefined)}
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

      {/* The primary input (spec decision 1). Always mounted, because what opens it is a
          keystroke it listens for itself — a palette the window had to decide to render
          would be one the operator could not reach from inside a pane's terminal. */}
      <Palette offers={offers} said={report} onRun={run} onOpened={setPaletteOpen} />

      {asking && <QuitWarning chats={open} states={states} onQuit={quit} onCancel={dontQuit} />}
    </main>
  );
}

/** A button that IS a row of the catalogue: its words, its availability and its reason.
 *
 *  Nothing is drawn for an id the catalogue no longer has. That is the point: the bar cannot
 *  keep offering something the one list has stopped offering, because there is no second
 *  place for the words to live. */
function Doer({ offer, onPress }: { offer?: Offer; onPress: (offer: Offer) => void }) {
  if (!offer) return null;
  return (
    <button
      disabled={!offer.available}
      title={offer.reason || undefined}
      onClick={() => onPress(offer)}
    >
      {offer.title}
    </button>
  );
}

/** A tab's close button. The same row the palette lists, drawn as the `×` a pointer wants —
 *  so the accessible name is the catalogue's words and the glyph is only the glyph. */
function Closer({ offer, onPress }: { offer?: Offer; onPress: (offer: Offer) => void }) {
  if (!offer) return null;
  return (
    <button aria-label={offer.title} onClick={() => onPress(offer)}>
      ×
    </button>
  );
}

/** What the quit warning says about one session: what the core told us, or the little the
 *  window knows about a chat the operator opened itself. */
function chatOf(reopened: OpenChat[], session: number, name: string): OpenChat {
  return (
    reopened.find((chat) => chat.session === session) ?? {
      session,
      name,
      cwd: null,
      harness: null,
      in_front: false,
      resumed: null,
      fresh: null,
      profile: null,
      persona: null,
      unreported: null,
    }
  );
}

/** A tab's layout, as panes with a handle between each split. */
function LayoutPanes({
  plane,
  layout,
  focused,
  onFocus,
}: {
  /** Which plane's sessions these panes are showing. A pane without one draws nothing: a
   *  session number belongs to a plane, and there is no plane to ask. */
  plane: PlaneId | undefined;
  layout: Layout;
  focused: number;
  onFocus: (pane: number) => void;
}) {
  if (layout.kind === "pane") {
    if (plane === undefined) return null;
    return (
      <SessionPane
        plane={plane}
        session={layout.session}
        focused={layout.pane === focused}
        onFocus={() => onFocus(layout.pane)}
      />
    );
  }
  return (
    <Group orientation={layout.direction === "row" ? "horizontal" : "vertical"}>
      {layout.children.map((child, side) => (
        <Fragment key={nameOf(child)}>
          {side === 1 && <Separator />}
          <Panel>
            <LayoutPanes plane={plane} layout={child} focused={focused} onFocus={onFocus} />
          </Panel>
        </Fragment>
      ))}
    </Group>
  );
}

/** What a part of the layout is called, so that it keeps its place — and its pane keeps its
 *  terminal — when what is beside it changes. */
function nameOf(layout: Layout): string {
  return layout.kind === "pane" ? `pane-${layout.pane}` : `split-${nameOf(layout.children[0])}`;
}

export default App;
