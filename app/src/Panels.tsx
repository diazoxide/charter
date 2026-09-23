import { useEffect, useState, type ReactNode } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import {
  ChartColumn,
  Circle,
  CircleDashed,
  FileText,
  FolderGit2,
  GitBranch,
  LoaderCircle,
  TriangleAlert,
  UserRound,
  X,
} from "lucide-react";
import { NeedsYou } from "./NeedsYou";
import { Menued } from "./Menus";
import { PanelList } from "./PanelList";
import { Chart, OpenedView, centreOf } from "./Views";
import { EmptyState } from "./EmptyState";
import {
  commands,
  type ExtensionView,
  type PanelRow,
  type PanelView,
  type PersonaDetails,
  type PlaneId,
} from "./bindings";
import type { Catalogued, Offer } from "./actions";
import type { WorkspaceState } from "./workspaceState";

/**
 * The right region: **what is asking for you** (charter ADR 0038).
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
 * The needs-you queue, the workspace's todos and the plane's personas. **Two of those three are
 * no longer written here.** Todos and personas are *contributions* — `charter_core::panel`
 * values produced in `app/src-tauri/src/panels.rs` and drawn by the loop below, through the
 * same seam an extension's declared panel arrives on. This component knows what a panel is; it
 * does not know what a todo is.
 *
 * That is the point, and it is testable rather than aspirational: an approved extension's panel
 * appears in this region with a search box, a bound, a load-more and a card on every row,
 * because `PanelList` gives every list those and this file gives every panel a `PanelList`.
 *
 * **The queue is the one thing that is still written here, and that is a decision.** It is not
 * a contribution and must not become one: ADR 0038 says this region must never compete with it,
 * a contributed panel is sorted among the others by an `order` the contributor chooses, and a
 * queue that could be pushed below a stranger's panel is the one arrangement this region is not
 * allowed to have. So it is above them all, always, and the vocabulary has no way to say
 * otherwise.
 *
 * **Not read-only any more, and the queue is the reason.** Every chat in it is a button that
 * brings that chat forward. What stayed read-only is the bottom bar.
 */
export function Panels({
  plane,
  workspace,
  state,
  queue,
  quiet,
  nameOf,
  showChat,
  offers,
  onPress,
  contributed,
  views = [],
  shownRow,
  onShowRow,
}: {
  /** Which project's plane the personas belong to. A window holds several, and two of them
   *  can both have a `steward`. */
  plane: PlaneId;
  /** The focused workspace, whose todos these are. The queue is not its — it is every
   *  workspace's, because a chat asking for you in a workspace nobody is looking at is
   *  exactly the one that must not be hidden. */
  workspace: string | undefined;
  state: WorkspaceState;
  queue: readonly number[];
  quiet: readonly string[];
  nameOf: (session: number) => string;
  showChat: (session: number) => void;
  /** The catalogue by id, which is what a row's verb is looked up in. */
  offers: Catalogued;
  onPress: (offer: Offer) => void;
  /** What approved extensions contribute, asked once per window (`extension_panels`) rather
   *  than once per workspace focus — a survey re-hashes every installed extension's directory,
   *  and that is not a cost the 100 ms of a workspace switch can carry. */
  contributed: readonly PanelView[];
  /** The views approved extensions offer (`extension_views`), asked once per window for the
   *  same reason. A view is offered on the heading of the panel about the same subject, and
   *  inside that panel's cards — both charter's choice of where, never the extension's. */
  views?: readonly ExtensionView[];
  /**
   * The row whose card is open, as `<panel key>/<row key>`.
   *
   * **Held by the window rather than by the row**, because opening a persona's card is a
   * catalogue row (charter-app#174) and the palette and a context menu can run it from outside
   * this panel. It was `shownPersona`; it is a row key now because every panel's rows open the
   * same way and a second piece of window state per panel would be the special case this
   * change exists to remove.
   */
  shownRow: string | undefined;
  onShowRow: (row: string | undefined) => void;
}) {
  const { panels, trouble } = state;

  /**
   * charter's own panels, and every approved extension's, in one list in one order.
   *
   * **Merged here because they come from two commands** — one per workspace focus, one per
   * window (`Panels.tsx`'s `useContributedPanels` says why) — and each is sorted on its own in
   * the core. A contributed panel is therefore not appended after charter's: it is sorted among
   * them, which is what makes `order` a number rather than a flag. The tie-break is the core's
   * (`charter_core::panel::Panel::sort` — charter's own first, then by id), and a stable sort
   * preserves it here.
   */
  const all = [...(panels?.contributed ?? []), ...contributed].sort((a, b) => a.order - b.order);

  // Where a sheet is drawn: the centre region of the arrangement this aside is in, found once
  // the aside is in the document. A callback ref rather than an effect, because the answer is a
  // fact about where this element landed and that is exactly when a callback ref runs.
  const [centre, setCentre] = useState<HTMLElement | null>(null);

  return (
    <aside
      ref={(el) => setCentre(centreOf(el))}
      className="panels"
      aria-label={workspace === undefined ? "Attention" : `Attention · ${workspace}`}
      data-testid="panels"
    >
      <NeedsYou queue={queue} quiet={quiet} nameOf={nameOf} show={showChat} />

      {workspace === undefined ? (
        <p className="empty">No workspace focused.</p>
      ) : (
        <>
          {trouble && (
            <p className="trouble" role="alert">
              {trouble}
            </p>
          )}

          {panels === undefined ? (
            <p className="pending">
              <LoaderCircle className="node-icon spinning" />
              Reading the plane…
            </p>
          ) : (
            all.map((panel) => (
              <Contributed
                key={panel.key}
                plane={plane}
                panel={panel}
                offers={offers}
                onPress={onPress}
                shownRow={shownRow}
                onShowRow={onShowRow}
                views={views.filter((view) => panel.about !== null && view.about === panel.about)}
                centre={centre}
              />
            ))
          )}
        </>
      )}
    </aside>
  );
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
 * states already means *nothing is in force* (`charter_core::extension`). The window that
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

/** Every mark in `charter_core::panel::Mark`, as the glyph a heading draws. */
const MARKS: Record<string, React.ComponentType<{ className?: string }>> = {
  todo: CircleDashed,
  persona: UserRound,
  repo: FolderGit2,
  piece: GitBranch,
  note: FileText,
  trouble: TriangleAlert,
  dot: Circle,
};

/**
 * One panel, whoever contributed it.
 *
 * **Nothing in here asks which panel it is**, except the one place that has to: a row whose
 * card is a persona's needs a persona's card, and knowing how to draw one is code. The
 * vocabulary names that consumer (`PanelDetail`'s `persona` kind) rather than letting a panel
 * bring it, so the special case is a `switch` over a closed set and not a branch on an id.
 */
function Contributed({
  plane,
  panel,
  offers,
  onPress,
  shownRow,
  onShowRow,
  views,
  centre,
}: {
  plane: PlaneId;
  panel: PanelView;
  offers: Catalogued;
  onPress: (offer: Offer) => void;
  shownRow: string | undefined;
  onShowRow: (row: string | undefined) => void;
  /** The views about this panel's subject, already filtered. */
  views: readonly ExtensionView[];
  centre: HTMLElement | null;
}) {
  const Mark = MARKS[panel.mark] ?? Circle;
  const open = shownRow?.startsWith(`${panel.key}/`)
    ? shownRow.slice(panel.key.length + 1)
    : undefined;

  return (
    <section data-testid={`panel-${named(panel)}`} data-panel-from={panel.from ?? "charter"}>
      <div className="panel-head">
        <h2>
          <Mark className="node-icon" />
          {panel.title}
          {/* **What is in force, after approval and not only at it** — charter ADR 0041 item
              5. An operator has to be able to tell a panel his own charter draws from one a
              stranger's extension contributed, without opening a dialog to find out. */}
          {panel.from !== null && <span className="panel-from">{` · ${panel.from}`}</span>}
        </h2>
        {views.map((view) => (
          <ViewButton
            key={`${view.extension}/${view.id}`}
            plane={plane}
            view={view}
            about={panel.title}
            centre={centre}
          />
        ))}
      </div>

      {panel.blocks.map((block, at) =>
        block.kind === "chart" ? (
          <Chart key={at} chart={block} />
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
            onOpen={(key) => onShowRow(key === undefined ? undefined : `${panel.key}/${key}`)}
            onRun={(id) => {
              // **The catalogue's row or nothing.** A row cannot invent a verb, and an id the
              // catalogue has stopped offering runs nothing rather than something else — which
              // is `Doer`'s rule for the bar's buttons, applied to a panel.
              const offer = offers.get(id);
              if (offer) onPress(offer);
            }}
            detailOf={(row) => detailOf(plane, row, views)}
            /* The persona card is a sheet over the centre region; every other card is still
               #173's popover. `PersonaCard` below has the argument. */
            sheet={(row) => row.detail?.kind === "persona"}
            sheetIn={centre}
            wrap={(row, item) =>
              row.detail?.kind === "persona" ? (
                /* Right-click is the third reader of the catalogue (`Menus.tsx`), and on a
                   persona it has exactly one honest row: what the plane says this persona is.
                   `asChild`, so the list gains no element. */
                <Menued
                  key={row.key}
                  on={{ on: "persona", persona: row.detail.persona }}
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
    </section>
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

/** What a row's card holds, dispatched on the closed set the vocabulary publishes. */
function detailOf(plane: PlaneId, row: PanelRow, views: readonly ExtensionView[]): ReactNode {
  if (row.detail === null) return null;
  if (row.detail.kind === "text") return <p className="row-detail">{row.detail.text}</p>;
  return <PersonaCard plane={plane} persona={row.detail.persona} views={views} />;
}

/**
 * One view's button on a panel's heading, and the sheet it opens.
 *
 * **The operator's "button that will open statistics of personas"**, and it is on the heading
 * because an approved extension offers a view about the subject this panel is about — not
 * because anything here knows what statistics are. With no such extension there is no button,
 * which is what a *100% pluggable* personas panel means: the statistics are a plugin's, and
 * so is their absence.
 *
 * Uncontrolled, because nothing outside the heading opens it: unlike a persona's card, no
 * catalogue row runs "show the statistics", so there is no window state for it to share.
 */
function ViewButton({
  plane,
  view,
  about,
  centre,
}: {
  plane: PlaneId;
  view: ExtensionView;
  about: string;
  centre: HTMLElement | null;
}) {
  return (
    <Dialog.Root modal={false}>
      <Dialog.Trigger asChild>
        {/* #190: WebKit leaves a button out of the tab sequence without `tabIndex`. */}
        <button type="button" className="panel-view" tabIndex={0}>
          <ChartColumn className="node-icon" aria-hidden="true" />
          {view.title}
        </button>
      </Dialog.Trigger>
      <Dialog.Portal container={centre ?? undefined}>
        <Dialog.Content
          className="sheet"
          data-testid={`view-sheet-${view.extension}-${view.id}`}
          aria-describedby={undefined}
        >
          <header className="sheet-head">
            <Dialog.Title>{`${about} · ${view.title}`}</Dialog.Title>
            <Dialog.Close className="drawer-close" tabIndex={0}>
              <X aria-hidden="true" /> Close
            </Dialog.Close>
          </header>
          <div className="sheet-body">
            <OpenedView plane={plane} view={view} focus={null} />
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/**
 * What a persona is, what it remembers, and — when an extension offers one — its statistics.
 *
 * ## Why a sheet over the centre, and what it kept from the popover
 *
 * charter-app#173 chose a popover for this card, for six short rows that answer *what is this
 * one for*: anchored to the name, dismissible because nothing is lost, and — the argument that
 * mattered — **not modal**, because a dialog marks the needs-you queue `aria-hidden` and ADR
 * 0038 says this region must never compete with it. A sheet over the whole window was ruled out
 * too, because that is `AlertsDrawer`'s.
 *
 * The card then grew a searchable archive (#206) and now a statistics view, and a popover
 * anchored in a 260 px column is a thin surface for either: a memory's body is paragraphs, and
 * a chart needs width to say anything. So the card is a **non-modal Radix `Dialog` drawn over
 * the centre region** — the terminals — and every one of #173's reasons is kept:
 *
 * - **Not modal.** No overlay, no focus trap, nothing marked `aria-hidden`: the queue stays on
 *   screen, in the tab order and in reach of the pointer. A jsdom test holds that.
 * - **Not the whole window.** It is portalled into the centre region, so it covers the one
 *   region that is not asking for anything and leaves every side region where it was.
 * - **Dismissible as the popover was.** Escape, a click outside, or its Close button; focus
 *   goes back to the row that opened it, because the row is its `Dialog.Trigger`.
 * - **One piece of window state.** It is still `shownRow`, so `persona.show:<name>` from the
 *   palette or a context menu opens this same sheet (charter-app#174).
 *
 * What it gave up: being anchored to the row. The name is the sheet's title instead.
 *
 * ## Three asks, and none is cached
 *
 * A definition and a memory store are both files an operator edits — often while charter is
 * running, because `charter persona create` and `charter persona remember` are how they arrive.
 * The definition and the memories are read when the card opens; the statistics are asked of
 * the extension that offers them, when the card opens, through the executor's gate
 * (`OpenedView`). Nothing is kept for the next opening.
 */
function PersonaCard({
  plane,
  persona,
  views,
}: {
  plane: PlaneId;
  persona: string;
  views: readonly ExtensionView[];
}) {
  const [shown, setShown] = useState<PersonaDetails>();
  const [refused, setRefused] = useState<string>();
  const [memories, setMemories] = useState<PanelRow[]>();
  const [noMemories, setNoMemories] = useState<string>();
  const [openMemory, setOpenMemory] = useState<string>();

  useEffect(() => {
    let gone = false;
    void commands
      .personaDetails(plane, persona)
      .then((said) => {
        if (gone) return;
        if (said.status === "error") setRefused(said.error);
        else if (said.data) setShown(said.data);
      })
      .catch((err: unknown) => {
        if (!gone) setRefused(String(err));
      });
    void commands
      .personaMemories(plane, persona)
      .then((said) => {
        if (gone) return;
        if (said.status === "error") setNoMemories(said.error);
        else setMemories(said.data ?? []);
      })
      .catch((err: unknown) => {
        if (!gone) setNoMemories(String(err));
      });
    return () => {
      gone = true;
    };
  }, [persona, plane]);

  return (
    <div className="sheet-body persona-card-body">
      <div className="persona-about">
        <PersonaFacts shown={shown} refused={refused} />

        <section className="persona-stats">
          <h4>Statistics</h4>
          {views.length === 0 ? (
            /* **Said, not left blank.** No approved extension offers a view about personas, and
               the operator is owed where one would come from — the statistics are a plugin's,
               and so is their absence. */
            <EmptyState
              size="panel"
              headline="No statistics here"
              body="An approved extension with a view about personas draws them here. Extensions is where one is installed."
              testid={`stats-${persona}-empty`}
            />
          ) : (
            views.map((view) => (
              <OpenedView
                key={`${view.extension}/${view.id}`}
                plane={plane}
                view={view}
                focus={persona}
              />
            ))
          )}
        </section>
      </div>

      <section className="persona-memories">
        <h4>Memory</h4>
        {noMemories !== undefined ? (
          <p className="trouble" role="alert">
            {noMemories}
          </p>
        ) : memories === undefined ? (
          <p className="pending">
            <LoaderCircle className="node-icon spinning" />
            Reading what it remembers…
          </p>
        ) : (
          /* **The same primitive, one surface in.** A memory is a row, so the search, the
             bound, the load-more and the card-on-a-row are the ones the panel itself has — and
             nothing here had to be told what a memory is. In a sheet it has room, so a page is
             twenty rather than twelve. */
          <PanelList
            rows={memories}
            empty={{
              headline: "Nothing remembered yet",
              body: "`charter persona remember` is how a fact arrives.",
              offer: null,
            }}
            label={`${persona}'s memories`}
            testid={`memories-${persona}`}
            open={openMemory}
            onOpen={setOpenMemory}
            page={20}
          />
        )}
      </section>
    </div>
  );
}

/** What the card says about the definition, which is what `charter persona show` says. The
 *  name is the sheet's title, so it is not said twice. */
function PersonaFacts({
  shown,
  refused,
}: {
  shown: PersonaDetails | undefined;
  refused: string | undefined;
}) {
  if (refused !== undefined) {
    // charter's own sentence, which names the fix. Drawn rather than swallowed: a card that
    // came up empty reads as a persona with nothing in it.
    return (
      <p className="trouble" role="alert">
        {refused}
      </p>
    );
  }
  if (shown === undefined) {
    return (
      <p className="pending">
        <LoaderCircle className="node-icon spinning" />
        Reading the definition…
      </p>
    );
  }
  return (
    <>
      {shown.role !== null && shown.role !== "" && <p className="role">{shown.role}</p>}
      <dl>
        <dt>Delegate to it for</dt>
        <dd>
          {shown.delegate_when !== null && shown.delegate_when !== "" ? (
            shown.delegate_when
          ) : (
            // `delegate-when` is what makes a persona findable — it becomes the description
            // whoever is routing reads — so a definition without one is worth saying.
            <span className="none">nothing declared, so nothing routes here by itself</span>
          )}
        </dd>

        <dt>Tools</dt>
        <dd>
          {shown.tools.length > 0 ? (
            shown.tools.join(", ")
          ) : (
            <span className="none">none auto-approved</span>
          )}
        </dd>

        {/* **The vault's NAME, and never a thing inside it.** charter refuses a secret by
            kind and never echoes one; a panel does not get an exception. The three answers
            are three because "holds no credentials" and "nobody has said" are different
            facts — see `PersonaDetails::declares_no_vault` for why charter-app cannot yet
            collapse the second into the first. */}
        <dt>Vault</dt>
        <dd>
          {shown.vault !== null ? (
            <>
              <code>{shown.vault}</code>
              <span className="note"> — the name; what is in it is never shown here</span>
            </>
          ) : shown.declares_no_vault ? (
            <span className="none">none: this persona holds no credentials of its own</span>
          ) : (
            <span className="none">not declared in its definition</span>
          )}
        </dd>

        {shown.lineage.length > 1 && (
          <>
            <dt>Inherits</dt>
            <dd>{shown.lineage.join(" → ")}</dd>
          </>
        )}

        <dt>Defined in</dt>
        <dd>
          <code>{shown.file}</code>
        </dd>
      </dl>
    </>
  );
}
