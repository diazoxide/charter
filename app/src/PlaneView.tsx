import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import {
  commands,
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
  type Project,
  type Ran,
} from "./actions";
import { StartChat } from "./StartChat";
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
  window: windowDoes,
  onReport,
}: {
  plane: PlaneId;
  /** Whether this is the project the operator is looking at. */
  inFront: boolean;
  /** Every project this window holds, for the rows that switch between them. */
  projects: readonly Project[];
  /** What the WINDOW does, which this project asks for rather than doing itself: opening
   *  another project, switching to one, letting one go, and quitting. */
  window: WindowDoing;
  /** What this project has open and whether it has found out yet, for the window's quit
   *  warning and for this project's own tab. */
  onReport: (plane: PlaneId, report: PlaneReport) => void;
}) {
  const [tabs, setTabs] = useState<Tabs>(noTabs);
  /** What every chat is doing, in THIS project. Pushed from the core; nothing here polls.
   *  It keeps listening while the project is behind another one, which is what lets its tab
   *  say that something over there needs you. */
  const states = useChatStates(plane);
  const [trouble, setTrouble] = useState<string>();
  const [sidebar, setSidebar] = useState<SidebarModel>();
  const [focused, setFocused] = useState<string>();
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
   *  one that is now. `Panels` keys its answers the same way, for the same reason. */
  const [located, setLocated] = useState<{ cwd: string; piece?: ChatWorktree }>();
  /** Bumped when something changed the answer, so it is asked again rather than guessed. */
  const [relocate, setRelocate] = useState(0);
  /** What the last action answered: one line, or a refusal in the words it came in. The
   *  window draws it, beside the palette that shares it. */
  const [report, setReport] = useState<{ from: string; refused: boolean; words: string }>();
  /** Whether the core has answered what this project already has open. Until it has, "no
   *  tabs" is "not yet", which is not the same thing as "nothing is running" — and a quit
   *  decides on it. */
  const [settled, setSettled] = useState(false);

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
        const drawn = open.reduce((tabs, chat) => openTab(tabs, chat.session, chat.name), noTabs());
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
  }, [plane, tabs]);

  // Where a chat starts: the focused workspace's directory, so the sidebar can file it under
  // that workspace. Nothing on the plane records a chat, so where it works is the only thing
  // relating the two. Null — the operator's home — until a plane is read.
  const startIn = sidebar?.workspaces.find((ws) => ws.name === focused)?.path ?? null;

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
      const name =
        "split" in where && inFrontTab !== undefined
          ? now.current.byId[inFrontTab].name
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
      if ("tab" in where) {
        change((tabs) => openTab(tabs, session, name));
        return;
      }
      const before = now.current;
      // The tab that was to be split can have closed while the picker was open. Nothing
      // would show that session, so it is ended rather than left running unseen.
      if (change((tabs) => splitFocusedPane(tabs, where.split, session)) === before) {
        void commands.closeSession(plane, session);
      }
    },
    [change, picking, plane, startIn],
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
    change(closeFocusedPane);
    if (going) void commands.closeSession(plane, going.session);
  }, [change, plane]);

  const close = useCallback(
    (id: number) => {
      const ending = panesOf(now.current, id);
      change((tabs) => closeTab(tabs, id));
      for (const pane of ending) void commands.closeSession(plane, pane.session);
    },
    [change, plane],
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
      openProject: windowDoes.openProject,
      selectProject: windowDoes.selectProject,
      closeProject: windowDoes.closeProject,
      quit: windowDoes.quit,
    }),
    [
      bringToFront,
      close,
      closePane,
      mergeWorktree,
      newTab,
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
            workspaces: sidebar?.workspaces.map((ws) => ws.name) ?? [],
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
          }),
    [
      focused,
      inFront,
      nameOf,
      plane,
      projects,
      quiet,
      report,
      sidebar,
      states.needsYou,
      tabs,
      worktree,
    ],
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
          <code>{plane}</code>
        </span>
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

      {wouldNotStart.map(([name, why]) => (
        <p className="came-back trouble" role="status" key={name}>
          <strong>{name}</strong> did not start ({why}). It is still recorded, and will be tried
          again at the next launch.
        </p>
      ))}

      <div className="body">
        {sidebar && (
          <Sidebar sidebar={sidebar} states={states} focused={focused} onFocus={setFocused} />
        )}
        <div className="panes">
          {frontTab ? (
            <LayoutPanes
              plane={plane}
              layout={frontTab.layout}
              focused={frontTab.focused}
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
  selectProject: (plane: string) => void;
  closeProject: (plane: string) => Promise<Ran>;
  quit: () => void;
};

/** The size a session starts at. The pane it lands in tells it the real one at once. */
const STARTING_SIZE = { columns: 80, rows: 24 };

/** A button that IS a row of the catalogue: its words, its availability and its reason.
 *
 *  Nothing is drawn for an id the catalogue no longer has. That is the point: the bar cannot
 *  keep offering something the one list has stopped offering, because there is no second
 *  place for the words to live. */
export function Doer({ offer, onPress }: { offer?: Offer; onPress: (offer: Offer) => void }) {
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
export function Closer({ offer, onPress }: { offer?: Offer; onPress: (offer: Offer) => void }) {
  if (!offer) return null;
  return (
    <button aria-label={offer.title} onClick={() => onPress(offer)}>
      ×
    </button>
  );
}

/** A tab's layout, as panes with a handle between each split. */
function LayoutPanes({
  plane,
  layout,
  focused,
  onFocus,
}: {
  /** Which plane's sessions these panes are showing. A session number belongs to a plane,
   *  and every command a pane makes carries it. */
  plane: PlaneId;
  layout: Layout;
  focused: number;
  onFocus: (pane: number) => void;
}) {
  if (layout.kind === "pane") {
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
        <Fragment key={nameOfLayout(child)}>
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
function nameOfLayout(layout: Layout): string {
  return layout.kind === "pane"
    ? `pane-${layout.pane}`
    : `split-${nameOfLayout(layout.children[0])}`;
}
