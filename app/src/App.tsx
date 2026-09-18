import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Group, Panel, Separator } from "react-resizable-panels";
import "./App.css";
import {
  commands,
  type OpenChat,
  type Sidebar as SidebarModel,
  type StartOptions,
} from "./bindings";
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
    async (profile: string) => {
      const said = await commands
        .approveProfile(profile)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (said.status === "error") {
        setPickerTrouble(said.error);
        return;
      }
      const options = await commands
        .startOptions()
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (options.status === "ok" && picking)
        setPicking({ options: options.data, where: picking.where });
      const persona = picking?.options.persona ?? null;
      await startPicked(profile, persona);
    },
    [picking, startPicked],
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

  const inFront = tabs.inFront === undefined ? undefined : tabs.byId[tabs.inFront];
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
                onClick={() => change((tabs) => selectTab(tabs, id))}
              >
                <span className="tab-name">{tabs.byId[id].name}</span>
                {/* The first pane's session is the tab's own chat. Its own element, so what
                    a tab IS stays separate from what it is DOING — a tab whose text changed
                    every time a turn began would be unreadable, and untestable. */}
                <ChatState state={stateOf(states, panesOf(tabs, id)[0]?.session ?? -1)} />
              </button>
              <button aria-label={`Close tab ${tabs.byId[id].name}`} onClick={() => close(id)}>
                ×
              </button>
            </span>
          ))}
          <button onClick={() => void newTab()}>New tab</button>
        </div>
        <div className="doing">
          <button disabled={!inFront} onClick={() => split("row")}>
            Split right
          </button>
          <button disabled={!inFront} onClick={() => split("column")}>
            Split down
          </button>
          <button disabled={!inFront} onClick={closePane}>
            Close pane
          </button>
        </div>
        <NeedsYou
          queue={states.needsYou}
          nameOf={(session: number) =>
            tabs.order
              .filter((id) => panesOf(tabs, id).some((pane) => pane.session === session))
              .map((id) => tabs.byId[id].name)[0] ?? String(session)
          }
          show={(session: number) => {
            const tab = tabs.order.find((id) =>
              panesOf(tabs, id).some((pane) => pane.session === session),
            );
            if (tab !== undefined) change((tabs) => selectTab(tabs, tab));
          }}
        />
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
      </div>

      {picking && (
        <StartChat
          options={picking.options}
          trouble={pickerTrouble}
          onStart={(profile, persona) => void startPicked(profile, persona)}
          onApprove={(profile) => void approveAndStart(profile)}
          onCancel={() => {
            setPicking(undefined);
            setPickerTrouble(undefined);
          }}
        />
      )}

      {asking && <QuitWarning chats={open} states={states} onQuit={quit} onCancel={dontQuit} />}
    </main>
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
