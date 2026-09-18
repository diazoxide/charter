import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Group, Panel, Separator } from "react-resizable-panels";
import "./App.css";
import { commands, type OpenChat } from "./bindings";
import { QuitWarning } from "./QuitWarning";
import { SessionPane } from "./SessionPane";
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

type Plane =
  { state: "loading" } | { state: "found"; root: string } | { state: "missing"; reason: string };

/** The size a session starts at. The pane it lands in tells it the real one at once. */
const STARTING_SIZE = { columns: 80, rows: 24 };

function App() {
  const [plane, setPlane] = useState<Plane>({ state: "loading" });
  const [tabs, setTabs] = useState<Tabs>(noTabs);
  const [trouble, setTrouble] = useState<string>();
  /** The chats the core put back at this launch, so a pane can say which came back how. */
  const [reopened, setReopened] = useState<OpenChat[]>([]);
  /** Whether the operator is being asked about quitting. */
  const [asking, setAsking] = useState(false);

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
        if (open.length === 0) return;
        setReopened(open);
        const drawn = open.reduce((tabs, chat) => openTab(tabs, chat.session, chat.name), noTabs());
        const front = open.findIndex((chat) => chat.in_front);
        change(() => (front < 0 ? drawn : selectTab(drawn, drawn.order[front])));
      })
      // Nothing open is the ordinary first launch, and a window that cannot ask is still
      // a window the operator can open a chat in.
      .catch(() => undefined);
  }, [change]);

  // Something asked the app to quit: the menu, the tray, or Cmd-Q. The answer is the
  // operator's, and it is given here because this is where what would be ended is known.
  useEffect(() => {
    const listening = listen("quit-asked", () => {
      // Nothing to end is nothing to warn about. A dialog listing no sessions would be a
      // dialog in the way of quitting.
      if (now.current.order.length === 0) void commands.quit();
      else setAsking(true);
    });
    return () => void listening.then((stop) => stop()).catch(() => undefined);
  }, []);

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

  /** Starts a session for a new pane, or says why it could not start. The name is what the
   *  tab will be called, so the record brings the chat back under it. */
  const startSession = useCallback(async (name: string): Promise<number | undefined> => {
    const opened = await commands
      .openSession(null, [], null, name, STARTING_SIZE.columns, STARTING_SIZE.rows)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (opened.status === "error") {
      setTrouble(opened.error);
      return undefined;
    }
    setTrouble(undefined);
    return opened.data;
  }, []);

  const newTab = useCallback(async () => {
    const name = String(now.current.named.tabs + 1);
    const session = await startSession(name);
    if (session !== undefined) change((tabs) => openTab(tabs, session, name));
  }, [change, startSession]);

  const quit = useCallback(() => {
    setAsking(false);
    void commands.quit();
  }, []);

  /** Not now: the core is told, so the next ask warns again instead of quitting outright. */
  const dontQuit = useCallback(() => {
    setAsking(false);
    void commands.quitCancelled().catch(() => undefined);
  }, []);

  const split = useCallback(
    async (direction: Direction) => {
      const name =
        now.current.inFront === undefined
          ? String(now.current.named.tabs + 1)
          : now.current.byId[now.current.inFront].name;
      const session = await startSession(name);
      if (session === undefined) return;
      const before = now.current;
      // The tab that was to be split can have closed while the session was starting. Nothing
      // would show that session, so it is ended rather than left running unseen.
      if (change((tabs) => splitFocusedPane(tabs, direction, session)) === before) {
        void commands.closeSession(session);
      }
    },
    [change, startSession],
  );

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
                {tabs.byId[id].name}
              </button>
              <button aria-label={`Close tab ${tabs.byId[id].name}`} onClick={() => close(id)}>
                ×
              </button>
            </span>
          ))}
          <button onClick={() => void newTab()}>New tab</button>
        </div>
        <div className="doing">
          <button disabled={!inFront} onClick={() => void split("row")}>
            Split right
          </button>
          <button disabled={!inFront} onClick={() => void split("column")}>
            Split down
          </button>
          <button disabled={!inFront} onClick={closePane}>
            Close pane
          </button>
        </div>
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

      {/* What happened to the chat in front when it was put back. Only a chat that came from
          the record has either, so a chat the operator just opened says nothing. */}
      {frontChat?.resumed && (
        <p className="came-back">
          <strong>{frontChat.name}</strong> was resumed — conversation{" "}
          <code>{frontChat.resumed}</code>
        </p>
      )}
      {frontChat?.fresh && (
        <p className="came-back">
          <strong>{frontChat.name}</strong> came back as a new chat: {frontChat.fresh}
        </p>
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

      {asking && <QuitWarning chats={open} onQuit={quit} onCancel={dontQuit} />}
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
