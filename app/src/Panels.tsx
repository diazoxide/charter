import { useEffect, useId, useMemo, useState } from "react";
import { useLentMemoryStores } from "./MemoryEdits";
import {
  Archive,
  ChartColumn,
  Circle,
  CircleDashed,
  Plus,
  FileText,
  FolderGit2,
  GitBranch,
  KeyRound,
  LoaderCircle,
  Send,
  TriangleAlert,
  UserRound,
  type LucideIcon,
} from "lucide-react";
import { Menued } from "./Menus";
import { PanelList } from "./PanelList";
import { HeadingOffer, PanelSection } from "./PanelSection";
import { Vaults, type VaultsSaid } from "./Vaults";
import { Chart, Facts } from "./Views";
import { commands, type ExtensionView, type PanelView } from "./bindings";
import {
  DISPATCHES_SHOW,
  listedMemoryOffers,
  memoryKeyRun,
  toKeep,
  type Catalogued,
  type Offer,
} from "./actions";
import type { WorkspaceState } from "./workspaceState";
import { isPanelView, panelKeyOf, panelView, type OwnViewId, type ViewId } from "./sideViews";
import type { PanelTab } from "./ActivityBar";

/**
 * One view of the right region: **what is asking for you** (ADR 0038).
 *
 * # One view at a time (#1678, ADR 0038 as amended 2026-10-10)
 *
 * The region was a stack of every panel. It is an activity bar's views now, one shown at a time:
 * Todos, Memory, Personas, Sessions and Vaults, then each approved extension's panel as a view of
 * its own, so a stranger's panel never crowds purlis's. `RegionFrame` draws the bar and keeps
 * every view mounted; this draws what one view holds — the one panel it is named for, with the
 * same heading, list and menus the stack gave it. {@link attentionPanels} says which extensions'
 * panels there are, which is what puts their tabs on the bar.
 *
 * **Alerts are not here any more, and that is a correction to ADR 0038, not an omission.** It
 * put them on this side, and this side is one project's: it follows the project in front and
 * the workspace focused in it. An alert is about a PLANE — a pin, a front door, a workspace's
 * layout, a plane root being worked in — and the plane that has one is usually not the one on
 * screen. So alerts are the window's: the status line's Alerts button, always on screen and
 * counting every open project, opens a drawer over the whole window (`AlertsDrawer.tsx`).
 *
 * # What changed: this file stopped being the panels and became the thing that draws them
 *
 * The workspace's todos and the plane's personas, and **neither is written here**. They are
 * *contributions* — `purlis_core::panel` values produced in `app/src-tauri/src/panels.rs` and
 * drawn by the loop below, through the same seam an extension's declared panel arrives on. This
 * component knows what a panel is; it does not know what a todo is.
 *
 * That is the point, and it is testable rather than aspirational: an approved extension's panel
 * appears in this region with a search box, a bound, a load-more and a card on every row,
 * because `PanelList` gives every list those and this file gives every panel a `PanelList`.
 *
 * **The needs-you queue is not here any more** (charter-app#249). It sat above every panel,
 * because ADR 0038 says nothing in this region may compete with it — and then it left the
 * region altogether, for the title bar: this region is one project's and one workspace's, and
 * the queue is every project's. `NeedsYou.NeedsYouMenu` is where it is.
 */
export function Panels({
  view,
  workspace,
  state,
  offers,
  onPress,
  contributed,
  views = [],
  shownRow,
  onShowRow,
  vaults,
  onAddTodo,
  atRoot = false,
  rootPanels,
}: {
  /** The view this draws: one of the right side's own, or an extension's panel (#1678). */
  view: ViewId;
  /** The focused workspace, whose todos these are. */
  workspace: string | undefined;
  state: WorkspaceState;
  /** The catalogue by id, which is what a row's verb is looked up in. */
  offers: Catalogued;
  onPress: (offer: Offer) => void;
  /** What approved extensions contribute, asked once per window (`extension_panels`) rather
   *  than once per workspace focus — a survey re-hashes every installed extension's directory,
   *  and that is not a cost the 100 ms of a workspace switch can carry. */
  contributed: readonly PanelView[];
  /** The views approved extensions offer (`extension_views`), asked once per window for the
   *  same reason. A view is offered on the heading of the panel about the same subject, and on
   *  the tab of a view about it — both charter's choice of where, never the extension's. */
  views?: readonly ExtensionView[];
  /**
   * The row whose card is open, as `<panel key>/<row key>`.
   *
   * **Held by the window rather than by the row.** It was held there so the palette could open a
   * persona's card from outside this panel (charter-app#174); a persona opens a tab of its own
   * now, and so does a todo (#1214); what is left here is the popover card of a contributed
   * row that has one.
   */
  shownRow: string | undefined;
  onShowRow: (row: string | undefined) => void;
  /** The plane's vaults, drawn under the workspace's panels (`useVaults`, which the window
   *  holds). A vault is the plane's, so they are drawn with no workspace focused too. */
  vaults?: Pick<VaultsSaid, "vaults" | "trouble" | "reload">;
  /**
   * Record a todo in the named workspace, answering the core's refusal or `undefined` (SI-3).
   * The workspace is named on every call, never implied: the box says which workspace it
   * writes to, and it sends that one.
   */
  onAddTodo?: (workspace: string, text: string) => Promise<string | undefined>;
  /**
   * The plane root is focused (SI-1). It is not a workspace — no charter, memory or todos —
   * so this region says so rather than drawing a workspace's panels, and there is no Todos box
   * to type into: a todo typed here would have to land in a workspace nobody chose.
   */
  atRoot?: boolean;
  /** The plane root's own panels — its Sessions (SI-8d) — drawn under the sentence saying what
   *  the plane root is, while it is focused (`sessions.usePlaneRootPanels`). */
  rootPanels?: readonly PanelView[];
}) {
  const { panels, trouble } = state;
  // Each view is still the "for you" side's (ADR 0038), and says whose: the tab names the view,
  // and this names the region and the workspace it is drawing, as the one region did.
  const named = workspace === undefined ? "Attention" : `Attention · ${workspace}`;

  if (view === "vaults") {
    // A vault is the plane's, so the view is drawn with no workspace focused too.
    return (
      <aside className="panels" data-view={view} aria-label={named}>
        {vaults !== undefined && <Vaults said={vaults} offers={offers} onPress={onPress} />}
      </aside>
    );
  }

  /** The panel this view shows, by its key: purlis's own by name, an extension's by its own. */
  const key = isPanelView(view) ? panelKeyOf(view) : OWN_PANELS[view];
  const all = [...(panels?.contributed ?? []), ...contributed];
  const draw = (panel: PanelView, at: string) => (
    <Contributed
      key={panel.key}
      panel={panel}
      workspace={at}
      onAddTodo={onAddTodo}
      offers={offers}
      onPress={onPress}
      shownRow={shownRow}
      onShowRow={onShowRow}
      views={views.filter((one) => panel.about !== null && one.about === panel.about)}
    />
  );

  if (workspace === undefined) {
    // The plane root is not a workspace: the view says so, and draws the root's own panel where
    // it has one by this view's key — its Sessions (SI-8d).
    const own = atRoot ? (rootPanels ?? []).find((panel) => panel.key === key) : undefined;
    return (
      <aside className="panels" data-view={view} aria-label={named}>
        <p className="empty">
          {atRoot
            ? "The project root is not a workspace: it has no todos or memory of its own. Chats here look after the project and its workspaces; focus a workspace to see its panels."
            : "No workspace focused."}
        </p>
        {own !== undefined && draw(own, "")}
      </aside>
    );
  }

  const panel = all.find((one) => one.key === key);
  return (
    <aside className="panels" data-view={view} aria-label={named}>
      {trouble && (
        <p className="trouble" role="alert">
          {trouble}
        </p>
      )}
      {panels === undefined ? (
        <p className="pending">
          <LoaderCircle className="node-icon spinning" />
          Reading the project…
        </p>
      ) : (
        panel !== undefined && draw(panel, workspace)
      )}
    </aside>
  );
}

/** Which of purlis's own panels each of the right side's views shows, by the panel's key. */
const OWN_PANELS: Partial<Record<OwnViewId, string>> = {
  todos: "charter/todos",
  memory: "charter/memory",
  personas: "charter/personas",
  sessions: "charter/sessions",
};

/**
 * **The extensions' panels, each a view of the right side** (#1678): every approved extension's
 * panel the window has, in the order their `order` hints sort them, with the name and mark each
 * declared. After purlis's own views on the bar, and never among them, so an extension that is
 * approved never moves an icon a person's hand already knows — VS Code's rule for a contributed
 * view container. A panel of purlis's own is not one of these: each has its view already.
 */
export function attentionPanels(
  state: WorkspaceState,
  contributed: readonly PanelView[],
): PanelTab[] {
  const own = new Set(Object.values(OWN_PANELS));
  const seen = new Set<string>();
  return [...(state.panels?.contributed ?? []), ...contributed]
    .filter((panel) => !own.has(panel.key) && !seen.has(panel.key) && seen.add(panel.key))
    .sort((a, b) => a.order - b.order)
    .map((panel) => ({
      view: panelView(panel.key),
      name: panel.title,
      mark: MARKS[panel.mark] ?? Circle,
    }));
}

/**
 * What approved extensions contribute, asked once for the window.
 *
 * **After the first frame, never before it**, which is `Extensions.tsx`'s
 * `drawWhatIsInForce` rule for the same reason at the same cost: a survey is a disk read and a
 * fingerprint of every installed extension's whole directory, and charter's own two panels are
 * produced from the plane read the region already waits on. An operator with no extensions pays
 * one command that answers with an empty list.
 *
 * **A refusal is an empty list and not a thrown promise.** The extension record can be
 * unreadable — a missing config home, a record charter will not parse — and every one of those
 * states already means *nothing is in force* (`purlis_core::extension`). The window that
 * reports them is `Extensions.tsx`'s dialog, which is where an operator goes to find out why
 * something is not contributing; a region that refused to draw charter's own panels over a
 * stranger's unreadable record would be the registry's failure taken out on the plane.
 */
export function useContributedPanels(): PanelView[] {
  const [contributed, setContributed] = useState<PanelView[]>([]);
  useEffect(() => {
    let gone = false;
    void commands
      .extensionPanels()
      .then((said) => {
        if (!gone && said.status !== "error") setContributed(said.data ?? []);
      })
      .catch(() => {
        // Nothing: see the docstring. The list stays empty and charter's own panels draw.
      });
    return () => {
      gone = true;
    };
  }, []);
  return contributed;
}

/** Every mark in `purlis_core::panel::Mark`, as the glyph a heading draws. */
const MARKS: Record<string, LucideIcon> = {
  todo: CircleDashed,
  persona: UserRound,
  repo: FolderGit2,
  piece: GitBranch,
  note: FileText,
  trouble: TriangleAlert,
  vault: KeyRound,
  dot: Circle,
};

/**
 * One panel, whoever contributed it.
 *
 * **Nothing in here asks which panel it is.** A persona's row opens the persona's tab because
 * the row carries the catalogue row that does (`persona.show:<name>`), not because anything here
 * knows it is a persona; the context menu is the one place a panel says what its rows are about.
 */
function Contributed({
  panel,
  workspace,
  onAddTodo,
  offers,
  onPress,
  shownRow,
  onShowRow,
  views,
}: {
  panel: PanelView;
  /** The focused workspace, which charter's todos panel writes to. */
  workspace: string;
  onAddTodo?: (workspace: string, text: string) => Promise<string | undefined>;
  offers: Catalogued;
  onPress: (offer: Offer) => void;
  shownRow: string | undefined;
  onShowRow: (row: string | undefined) => void;
  /** The views about this panel's subject, already filtered. */
  views: readonly ExtensionView[];
}) {
  const Mark = MARKS[panel.mark] ?? Circle;
  const open = shownRow?.startsWith(`${panel.key}/`)
    ? shownRow.slice(panel.key.length + 1)
    : undefined;
  // **A memory row's own rows** (SI-9c): Open, Edit and Delete for each memory this panel
  // lists, by the one function a persona's tab makes them with.
  // A Move row per store (#1190), from the stores the window lends.
  // And the LIVE names, for what a Move into a LIVE workspace's journal says.
  const { stores, live } = useLentMemoryStores();
  const memories = useMemo(
    () => listedMemoryOffers(panel.blocks, stores, live),
    [panel.blocks, stores, live],
  );
  const lookUp = (id: string) => memories.get(id) ?? offers.get(id);

  return (
    <PanelSection
      testid={`panel-${named(panel)}`}
      from={panel.from ?? "charter"}
      mark={Mark}
      title={panel.title}
      provenance={panel.from ?? undefined}
      actions={
        <>
          {views.map((view) => (
            <ViewButton
              key={`${view.extension}/${view.id}`}
              offer={offers.get(`view.open:${view.extension}/${view.id}`)}
              onPress={onPress}
            />
          ))}
          {/* charter's own panels are about things charter can make (SI-3): the heading's `+`
              is the catalogue's row for one more. By key, as the menus below are — the one
              place this file says what a panel is about. */}
          {panel.key === PERSONAS && (
            <HeadingOffer offer={offers.get("persona.create")} onPress={onPress} />
          )}
          {/* What the project's chats handed to other chats (#1452): the Dispatches tab. */}
          {panel.key === SESSIONS && (
            <HeadingOffer offer={offers.get(DISPATCHES_SHOW)} onPress={onPress} mark={Send} />
          )}
          {/* A new memory in the focused workspace's journal (SI-9c, ADR 0065 Q9). */}
          {panel.key === MEMORY && (
            <HeadingOffer
              offer={offers.get(`memory.new:workspace/${workspace}`)}
              onPress={onPress}
            />
          )}
          {/* And the journal's archive, to read and restore (KN-4). */}
          {panel.key === MEMORY && (
            <HeadingOffer
              offer={offers.get(`memory.archived:workspace/${workspace}`)}
              onPress={onPress}
              mark={Archive}
            />
          )}
        </>
      }
    >
      {panel.key === TODOS && onAddTodo !== undefined && (
        <AddTodo workspace={workspace} onAdd={onAddTodo} />
      )}
      {panel.blocks.map((block, at) =>
        block.kind === "chart" ? (
          <Chart key={at} chart={block} />
        ) : block.kind === "facts" ? (
          <Facts key={at} facts={block} />
        ) : block.kind === "note" ? (
          <p
            /* By position, which is the one place in this file that is right: a block has no
               identity of its own in the vocabulary, and a panel's block list is fixed for as
               long as the panel is — it arrives whole from one read and is never spliced. */
            key={at}
            className={block.tone === "trouble" ? "trouble" : "note"}
            role={block.tone === "trouble" ? "alert" : undefined}
          >
            {block.text}
          </p>
        ) : (
          <PanelList
            /* By position, as above. */
            key={at}
            rows={block.rows}
            empty={block.empty}
            label={panel.title}
            testid={`list-${named(panel)}`}
            open={open}
            // The empty list's way out (#1156): the catalogue's row, as a row's is.
            offerFor={lookUp}
            onOpen={(key) => onShowRow(key === undefined ? undefined : `${panel.key}/${key}`)}
            onRun={(id, kept) => {
              // **The catalogue's row or nothing.** A row cannot invent a verb, and an id the
              // catalogue has stopped offering runs nothing rather than something else — which
              // is `Doer`'s rule for the bar's buttons, applied to a panel. A double-click keeps
              // what a single click previews (`actions.toKeep`): this list stays on screen, so
              // a second click can land on it (ADR 0065, as built in SI-9b).
              const offer = lookUp(id);
              if (offer) onPress(kept ? toKeep(offer) : offer);
            }}
            wrap={(row, item) =>
              panel.key === TODOS ? (
                /* Open, Mark done, and Forget (SI-3, #1214): the catalogue's rows for this todo
                   in the focused workspace, which is the one this panel is about. */
                <Menued
                  key={row.key}
                  on={{ on: "todo", slug: row.key }}
                  offers={offers}
                  onPress={onPress}
                >
                  {item}
                </Menued>
              ) : panel.key === SESSIONS ? (
                /* Open and Resume (SI-8d): the catalogue's rows for this record, whose key is
                   its plane-relative path. */
                <Menued
                  key={row.key}
                  on={{ on: "session", path: row.key }}
                  offers={offers}
                  onPress={onPress}
                >
                  {item}
                </Menued>
              ) : panel.key === MEMORY ? (
                /* Open, Edit | Delete (ADR 0065 Q12): this memory's rows, from the lookup
                   above rather than the catalogue. */
                <Menued
                  key={row.key}
                  on={{ on: "memory", key: memoryKeyRun(row.runs) ?? "" }}
                  offers={memories}
                  onPress={onPress}
                >
                  {item}
                </Menued>
              ) : panel.key === PERSONAS && row.key !== SHARED_ROW ? (
                /* Right-click is the third reader of the catalogue (`Menus.tsx`), and on a
                   persona it has exactly one honest row: what the plane says this persona is.
                   `asChild`, so the list gains no element. */
                <Menued
                  key={row.key}
                  on={{ on: "persona", persona: row.key }}
                  offers={offers}
                  onPress={onPress}
                >
                  {item}
                </Menued>
              ) : (
                item
              )
            }
          />
        ),
      )}
    </PanelSection>
  );
}

/** charter's own panels, by the key `purlis_core::panel::Panel::key` gives them. */
const TODOS = "charter/todos";
const PERSONAS = "charter/personas";
const SESSIONS = "charter/sessions";
const MEMORY = "charter/memory";

/** The Personas panel's row for the shared store (`panels::SHARED_ROW`): not a persona, so it
 *  gets no persona's menu. */
const SHARED_ROW = "_shared";

/**
 * The Todos panel's box: **a todo typed here goes to the focused workspace, and the box says
 * which** (SI-3). Its accessible name and its placeholder both name the workspace, because a todo
 * recorded in the wrong one is a list that lies about what is left in both.
 *
 * The core decides what is recorded (`todo_add`, which is `Workspace::record_todo`): a todo with
 * no words, or one about work already on the list, is refused in its words, drawn under the box,
 * and the box keeps what was typed so the operator can change it. The list itself is redrawn
 * from the plane — the write changes the disk, and the window reads the disk again.
 */
function AddTodo({
  workspace,
  onAdd,
}: {
  workspace: string;
  onAdd: (workspace: string, text: string) => Promise<string | undefined>;
}) {
  const [text, setText] = useState("");
  const [trouble, setTrouble] = useState<string>();
  const [busy, setBusy] = useState(false);
  const refusal = useId();
  const add = async () => {
    const words = text.trim();
    if (words === "" || busy) return;
    setBusy(true);
    const refused = await onAdd(workspace, words);
    setBusy(false);
    setTrouble(refused);
    if (refused === undefined) setText("");
  };
  return (
    <>
      <form
        className="panel-search"
        onSubmit={(event) => {
          event.preventDefault();
          void add();
        }}
      >
        <Plus className="node-icon" aria-hidden="true" />
        <input
          value={text}
          aria-label={`New todo in ${workspace}`}
          aria-describedby={trouble === undefined ? undefined : refusal}
          placeholder={`Add a todo to ${workspace}…`}
          autoComplete="off"
          // Read-only rather than disabled while it is sent: a disabled box drops the keyboard,
          // and the next todo is typed into the same box.
          readOnly={busy}
          onChange={(event) => setText(event.target.value)}
        />
      </form>
      {trouble !== undefined && (
        <p className="trouble" role="alert" id={refusal}>
          {trouble}
        </p>
      )}
    </>
  );
}

/**
 * What a panel is called in a `data-testid`.
 *
 * **charter's own keep the bare name they have always had** — `panel-todos`,
 * `panel-personas` — because a scenario spec is a contract with the window and renaming one to
 * advertise an internal move is a change to what the spec is about. A contributed panel is
 * `panel-ext-<extension>-<id>`, which cannot collide with a built-in's because `ext` is not a
 * legal panel id (an id starts with a letter or a digit and `charter/` is stripped, so the one
 * name this could clash with is a built-in panel called `ext-…`, and charter has none).
 */
function named(panel: PanelView): string {
  return panel.key
    .replace(/^charter\//, "")
    .split("/")
    .join("-");
}

/**
 * One view's button on a panel's heading: **the catalogue's `view.open:<extension>/<view>` row,
 * drawn where a pointer looks for it.**
 *
 * The operator's *"button that will open statistics of personas"*, and it is on the heading
 * because an approved extension offers a view about the subject this panel is about — not
 * because anything here knows what statistics are. With no such extension there is no button,
 * which is what a *100% pluggable* personas panel means: the statistics are a plugin's, and so
 * is their absence. It opens the view in a tab of its own; the palette runs the same row.
 */
function ViewButton({ offer, onPress }: { offer?: Offer; onPress: (offer: Offer) => void }) {
  if (!offer) return null;
  return (
    <button
      type="button"
      className="panel-view"
      // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
      tabIndex={0}
      disabled={!offer.available}
      title={offer.title}
      onClick={() => onPress(offer)}
    >
      <ChartColumn className="node-icon" aria-hidden="true" />
      {offer.does.verb === "openView" ? offer.does.title : offer.title}
    </button>
  );
}
