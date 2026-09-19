import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Group, Panel, Separator } from "react-resizable-panels";
import "./App.css";
import {
  commands,
  type ChatWorktree,
  type OpenChat,
  type Sidebar as SidebarModel,
  type StartOptions,
} from "./bindings";
import { catalogue, perform, type Doing, type Offer, type Ran } from "./actions";
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
import { stateOf, useChatStates } from "./chatState";

type Plane =
  { state: "loading" } | { state: "found"; root: string } | { state: "missing"; reason: string };

/** The size a session starts at. The pane it lands in tells it the real one at once. */
const STARTING_SIZE = { columns: 80, rows: 24 };

function App() {
  const [plane, setPlane] = useState<Plane>({ state: "loading" });
  const [tabs, setTabs] = useState<Tabs>(noTabs);
  /** What every chat is doing. Pushed from the core; nothing here polls. */
  const states = useChatStates();
  const [trouble, setTrouble] = useState<string>();
  const [sidebar, setSidebar] = useState<SidebarModel>();
  const [focused, setFocused] = useState<string>();
  /** The chats the core put back at this launch, so a pane can say which came back how. */
  const [reopened, setReopened] = useState<OpenChat[]>([]);
  /** Whether the operator is being asked about quitting. */
  const [asking, setAsking] = useState(false);
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
  const change = useCallback((how: (tabs: Tabs) => Tabs): Tabs => {
    const next = how(now.current);
    const wasInFront = now.current.inFront;
    now.current = next;
    setTabs(next);
    // The core records which chat was in front, so it is told whenever that changes — and
    // only then, rather than on every split and every keystroke.
    if (next.inFront !== wasInFront) {
      const front = next.inFront === undefined ? undefined : next.byId[next.inFront];
      const pane = front && panesOf(next, front.id)[0];
      void commands.chatInFront(pane ? pane.session : null).catch(() => undefined);
    }
    return next;
  }, []);

  // What the core already has open, which at a launch is the record put back before there
  // was a window. The window draws them; it never ends them — a reload during development,
  // or a crash in the window, would otherwise take the day's sessions with it.
  //
  // Once only: React runs an effect twice in development, and a second pass would draw
  // every chat again.
  const adopted = useRef(false);
  useEffect(() => {
    if (adopted.current) return;
    adopted.current = true;
    void commands
      .openedChats()
      .then((open) => {
        settled.current = true;
        void commands
          // A window that cannot ask, or is answered with nothing, simply says nothing.
          .chatsThatWouldNotStart()
          .then((trouble) => setWouldNotStart(trouble ?? []))
          .catch(() => undefined);
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
  }, [change]);

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
    void commands
      .planeSidebar()
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
  }, [tabs]);

  // Cold start ends when a person can see the window, which is the frame after the one this
  // paints in. Nothing happens on the other side unless the app was started to be measured,
  // and nothing about the window depends on the marker arriving: a window that cannot send it
  // is still a window.
  useEffect(() => {
    let gone = false;
    const frame = requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        if (!gone) void commands.firstFrame().catch(() => undefined);
      }),
    );
    return () => {
      gone = true;
      cancelAnimationFrame(frame);
    };
  }, []);

  useEffect(() => {
    void commands
      .planeRoot()
      .then((found) => {
        setPlane(
          found.status === "ok"
            ? { state: "found", root: found.data }
            : { state: "missing", reason: found.error },
        );
      })
      // A command can also fail outright, with no answer of its own to give.
      .catch((err: unknown) => setPlane({ state: "missing", reason: String(err) }));
  }, []);

  // Where a chat starts: the focused workspace's directory, so the sidebar can file it under
  // that workspace. Nothing on the plane records a chat, so where it works is the only thing
  // relating the two. Null — the operator's home — until a plane is read.
  const startIn = sidebar?.workspaces.find((ws) => ws.name === focused)?.path ?? null;

  /** Asks which profile and which persona. It starts nothing by itself. */
  const ask = useCallback(async (where: { tab: true } | { split: Direction }) => {
    setPickerTrouble(undefined);
    const options = await commands
      .startOptions()
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (options.status === "error") {
      setTrouble(options.error);
      return;
    }
    setPicking({ options: options.data, where });
  }, []);

  const newTab = useCallback(() => void ask({ tab: true }), [ask]);

  /** A row was picked: the chat starts on that profile, with that persona. */
  const startPicked = useCallback(
    async (profile: string, persona: string | null) => {
      const where = picking?.where;
      if (where === undefined) return;
      const inFront = now.current.inFront;
      const name =
        "split" in where && inFront !== undefined
          ? now.current.byId[inFront].name
          : String(now.current.named.tabs + 1);
      const started = await commands
        .startChat(profile, persona, startIn, name, STARTING_SIZE.columns, STARTING_SIZE.rows)
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
        void commands.closeSession(session);
      }
    },
    [change, picking, startIn],
  );

  /** The approval IS this click. After it, the whole chain of checks runs again from the
   *  top before anything is exec'd, so a yes never walks past a refusal standing behind it. */
  const approveAndStart = useCallback(
    async (profile: string, persona: string | null, shown: string) => {
      const said = await commands
        .approveProfile(profile, shown)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") {
        setPickerTrouble(said.error);
        return;
      }
      // The persona the OPERATOR picked, carried up from the dialog with the profile. It
      // used to take the plane's default out of the options instead, which silently threw
      // away the choice on the one path where a profile is being used for the first time.
      await startPicked(profile, persona);
    },
    [startPicked],
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
    if (going) void commands.closeSession(going.session);
  }, [change]);

  const close = useCallback(
    (id: number) => {
      const ending = panesOf(now.current, id);
      change((tabs) => closeTab(tabs, id));
      for (const pane of ending) void commands.closeSession(pane.session);
    },
    [change],
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
  const planeRoot = plane.state === "found" ? plane.root : undefined;

  /** Removes the piece the chat in front works in. The core's refusal travels back whole. */
  const removeWorktree = useCallback(
    async (force: boolean): Promise<Ran> => {
      if (!worktree || planeRoot === undefined)
        return { ok: false, refused: "There is no worktree in front to remove." };
      const answer = await commands
        .worktreeRemove(planeRoot, worktree.workspace, worktree.repo, worktree.piece, force)
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
    [planeRoot, worktree],
  );

  /** Lands the piece in its clone, fast-forward only. The core never pushes. */
  const mergeWorktree = useCallback(async (): Promise<Ran> => {
    if (!worktree || planeRoot === undefined)
      return { ok: false, refused: "There is no worktree in front to merge." };
    const answer = await commands
      .worktreeMerge(planeRoot, worktree.workspace, worktree.repo, worktree.piece)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (answer.status === "error") return { ok: false, refused: answer.error };
    return {
      ok: true,
      said: `${answer.data.branch} landed: ${answer.data.was} → ${answer.data.now}`,
    };
  }, [planeRoot, worktree]);

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
      quit: () => void commands.askToQuit().catch(() => undefined),
    }),
    [bringToFront, close, closePane, mergeWorktree, newTab, removeWorktree, showChat, split],
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
        plane: planeRoot,
        // Only a refusal the REMOVAL gave, and only while it is still on screen: the
        // discard row is the operator's answer to a sentence they have read.
        refusal: report?.refused && report.from === "worktree.remove" ? report.words : undefined,
        needsYou: states.needsYou,
        nameOf,
      }),
    [focused, nameOf, planeRoot, report, sidebar, states.needsYou, tabs, worktree],
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
        <NeedsYou queue={states.needsYou} nameOf={nameOf} show={showChat} />
        <span className="plane">
          {plane.state === "found" && <code>{plane.root}</code>}
          {plane.state === "missing" && <span role="alert">No plane: {plane.reason}</span>}
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

      <div className="body">
        {sidebar && (
          <Sidebar sidebar={sidebar} states={states} focused={focused} onFocus={setFocused} />
        )}
        <div className="panes">
          {inFront ? (
            <LayoutPanes
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

      {picking && (
        <StartChat
          options={picking.options}
          trouble={pickerTrouble}
          onStart={(profile, persona) => void startPicked(profile, persona)}
          onApprove={(profile, persona, shown) => void approveAndStart(profile, persona, shown)}
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
    }
  );
}

/** A tab's layout, as panes with a handle between each split. */
function LayoutPanes({
  layout,
  focused,
  onFocus,
}: {
  layout: Layout;
  focused: number;
  onFocus: (pane: number) => void;
}) {
  if (layout.kind === "pane") {
    return (
      <SessionPane
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
            <LayoutPanes layout={child} focused={focused} onFocus={onFocus} />
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
