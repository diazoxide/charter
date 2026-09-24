import { useEffect, useState } from "react";
import {
  ChartColumn,
  Circle,
  CircleDashed,
  FileText,
  FolderGit2,
  GitBranch,
  KeyRound,
  LoaderCircle,
  TriangleAlert,
  UserRound,
} from "lucide-react";
import { NeedsYou } from "./NeedsYou";
import { Menued } from "./Menus";
import { PanelList } from "./PanelList";
import { Vaults, type VaultsSaid } from "./Vaults";
import { Chart, Facts } from "./Views";
import { commands, type ExtensionView, type PanelView } from "./bindings";
import type { Catalogued, Offer } from "./actions";
import type { WorkspaceState } from "./workspaceState";

/**
 * The right region: **what is asking for you** (ADR 0038).
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
  vaults,
}: {
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
   *  same reason. A view is offered on the heading of the panel about the same subject, and on
   *  the tab of a view about it — both charter's choice of where, never the extension's. */
  views?: readonly ExtensionView[];
  /**
   * The row whose card is open, as `<panel key>/<row key>`.
   *
   * **Held by the window rather than by the row.** It was held there so the palette could open a
   * persona's card from outside this panel (charter-app#174); a persona opens a tab of its own
   * now, and what is left here is the popover card of a row that has one — a todo, a
   * contributed row.
   */
  shownRow: string | undefined;
  onShowRow: (row: string | undefined) => void;
  /** The plane's vaults, drawn under the workspace's panels (`useVaults`, which the window
   *  holds). A vault is the plane's, so they are drawn with no workspace focused too. */
  vaults?: Pick<VaultsSaid, "vaults" | "trouble">;
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

  return (
    <aside
      className="panels"
      aria-label={workspace === undefined ? "Attention" : `Attention · ${workspace}`}
      data-testid="panels"
    >
      <NeedsYou
        queue={queue}
        quiet={quiet}
        nameOf={nameOf}
        show={showChat}
        offers={offers}
        onPress={onPress}
      />

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
                panel={panel}
                offers={offers}
                onPress={onPress}
                shownRow={shownRow}
                onShowRow={onShowRow}
                views={views.filter((view) => panel.about !== null && view.about === panel.about)}
              />
            ))
          )}
        </>
      )}

      {vaults !== undefined && <Vaults said={vaults} offers={offers} onPress={onPress} />}
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
  offers,
  onPress,
  shownRow,
  onShowRow,
  views,
}: {
  panel: PanelView;
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

  return (
    <section data-testid={`panel-${named(panel)}`} data-panel-from={panel.from ?? "charter"}>
      <div className="panel-head">
        <h2>
          <Mark className="node-icon" />
          {panel.title}
          {/* **What is in force, after approval and not only at it** — ADR 0041 item
              5. An operator has to be able to tell a panel his own charter draws from one a
              stranger's extension contributed, without opening a dialog to find out. */}
          {panel.from !== null && <span className="panel-from">{` · ${panel.from}`}</span>}
        </h2>
        {views.map((view) => (
          <ViewButton
            key={`${view.extension}/${view.id}`}
            offer={offers.get(`view.open:${view.extension}/${view.id}`)}
            onPress={onPress}
          />
        ))}
      </div>

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
            onOpen={(key) => onShowRow(key === undefined ? undefined : `${panel.key}/${key}`)}
            onRun={(id) => {
              // **The catalogue's row or nothing.** A row cannot invent a verb, and an id the
              // catalogue has stopped offering runs nothing rather than something else — which
              // is `Doer`'s rule for the bar's buttons, applied to a panel.
              const offer = offers.get(id);
              if (offer) onPress(offer);
            }}
            wrap={(row, item) =>
              panel.key === "charter/personas" ? (
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
