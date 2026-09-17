import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import "./App.css";
import { commands } from "./bindings";
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

  // The arrangement as it is right now, so that what a button does is decided here and not
  // inside a state update. React may run an update again, and a session must not be opened or
  // ended twice because it did.
  const now = useRef(tabs);
  const change = useCallback((how: (tabs: Tabs) => Tabs): Tabs => {
    const next = how(now.current);
    now.current = next;
    setTabs(next);
    return next;
  }, []);

  // A reload — during development, or after a crash in the window — leaves the core holding
  // sessions no pane can reach. They are ended here, before any tab is opened, and once:
  // React runs an effect twice in development, and ending a session twice is an error.
  const swept = useRef(false);
  useEffect(() => {
    if (swept.current) return;
    swept.current = true;
    void commands
      .runningSessions()
      .then((left) => {
        for (const session of left) void commands.closeSession(session);
      })
      // Nothing to sweep is the ordinary case, and a window that cannot ask is still usable.
      .catch(() => undefined);
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

  /** Starts a session for a new pane, or says why it could not start. */
  const startSession = useCallback(async (): Promise<number | undefined> => {
    const opened = await commands
      .openSession(null, [], null, STARTING_SIZE.columns, STARTING_SIZE.rows)
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
    if (opened.status === "error") {
      setTrouble(opened.error);
      return undefined;
    }
    setTrouble(undefined);
    return opened.data;
  }, []);

  const newTab = useCallback(async () => {
    const session = await startSession();
    if (session !== undefined) change((tabs) => openTab(tabs, session));
  }, [change, startSession]);

  const split = useCallback(
    async (direction: Direction) => {
      const session = await startSession();
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

  return (
    <main className="window">
      <header className="bar">
        <div className="tabs" role="tablist">
          {tabs.order.map((id) => (
            <span className="tab" key={id}>
              <button
                role="tab"
                aria-selected={id === tabs.inFront}
                onClick={() => change((tabs) => selectTab(tabs, id))}
              >
                {id}
              </button>
              <button aria-label={`Close tab ${id}`} onClick={() => close(id)}>
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
    </main>
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
