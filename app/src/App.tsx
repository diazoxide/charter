import { useCallback, useEffect, useState } from "react";
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
    if (session !== undefined) setTabs((tabs) => openTab(tabs, session));
  }, [startSession]);

  const split = useCallback(
    async (direction: Direction) => {
      const session = await startSession();
      if (session !== undefined) setTabs((tabs) => splitFocusedPane(tabs, direction, session));
    },
    [startSession],
  );

  const closePane = useCallback(() => {
    setTabs((tabs) => {
      const tab = tabs.inFront === undefined ? undefined : tabs.byId[tabs.inFront];
      const going = tab && panesOf(tabs, tab.id).find((pane) => pane.pane === tab.focused);
      if (going) void commands.closeSession(going.session);
      return closeFocusedPane(tabs);
    });
  }, []);

  const close = useCallback((id: number) => {
    setTabs((tabs) => {
      for (const pane of panesOf(tabs, id)) void commands.closeSession(pane.session);
      return closeTab(tabs, id);
    });
  }, []);

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
                onClick={() => setTabs((tabs) => selectTab(tabs, id))}
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
            onFocus={(pane) => setTabs((tabs) => focusPane(tabs, pane))}
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
      <Panel>
        <LayoutPanes layout={layout.children[0]} focused={focused} onFocus={onFocus} />
      </Panel>
      <Separator />
      <Panel>
        <LayoutPanes layout={layout.children[1]} focused={focused} onFocus={onFocus} />
      </Panel>
    </Group>
  );
}

export default App;
