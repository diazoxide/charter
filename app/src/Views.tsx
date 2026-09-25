import { useEffect, useState } from "react";
import {
  ChartColumn,
  KeyRound,
  LoaderCircle,
  Puzzle,
  Save,
  Settings2,
  SlidersHorizontal,
  UserRound,
} from "lucide-react";
import { EmptyState } from "./EmptyState";
import { AskFirst, runExtensionAction } from "./ExtensionAction";
import { PanelList } from "./PanelList";
import { Preferences } from "./Preferences";
import { ProjectSettings, WorkspaceSettings } from "./ProjectSettings";
import {
  commands,
  type ExtensionCommand,
  type ExtensionView,
  type PanelBlock,
  type PanelPoint,
  type PanelRow,
  type PlaneId,
  type RowAction,
  type ViewAnswer,
} from "./bindings";
import { SavingView } from "./SavingView";
import { PREFERENCES_VIEW, SAVING_VIEW, SETTINGS_VIEW, viewKey, type ViewRef } from "./tabs";
import { VaultTab } from "./VaultTab";
import { factsChanged } from "./extensionFacts";

/** What `workspaceSettingsView` names a workspace's settings view (charter-app#280). */
const WORKSPACE_SETTINGS = "workspace-settings";

/**
 * **Views: what a tab shows when it does not show a chat** — ADR 0043 as amended
 * 2026-09-23, and ADR 0041 stage 2's window half.
 *
 * A tab is a layout of panes and a pane holds a session or a view (`tabs.ts`). A view is named
 * by data — who draws it, which of theirs, what it is about — and **opening one asks the core
 * one question** (`open_view`), whose answer is the panel vocabulary and nothing else: notes,
 * facts, lists and charts. charter's own persona view and an approved extension's statistics come
 * down that one command and are drawn by the code below; this file cannot tell them apart
 * except by whose they are, which it says. That is the operator's *"100% pluggable"*, as a
 * property of the code rather than a promise: the personas are a plugin like any other, and
 * what charter does for them an extension's view gets too.
 *
 * **Asked once per opening, and never on a timer.** ADR 0041's minimum capability is *one round
 * trip per deliberate human action*; a view that polled would be a program running on nobody's
 * say-so. Bringing the tab forward again is how the operator asks again. A tab a launch put back
 * waits for a press before an extension's program is asked anything (`tabs.Content.waits`).
 */

/**
 * The views approved extensions offer this window, asked once for the window after its first
 * frame — the same shape and the same reasons as `useContributedPanels`: a survey re-hashes
 * every installed extension's directory, and a refusal means nothing is offered rather than
 * that the region fails.
 */
export function useExtensionViews(): ExtensionView[] {
  const [views, setViews] = useState<ExtensionView[]>([]);
  useEffect(() => {
    let gone = false;
    void commands
      .extensionViews()
      .then((said) => {
        if (!gone && said.status !== "error") setViews(said.data ?? []);
      })
      .catch(() => {
        // Nothing: no view is offered, and `Extensions.tsx` is where the reason is read.
      });
    return () => {
      gone = true;
    };
  }, []);
  return views;
}

/**
 * The palette commands approved extensions add to this window (charter-app#341), asked once for
 * the window after its first frame on `useExtensionViews`'s terms: a refusal means none are
 * offered rather than that the palette fails.
 */
export function useExtensionCommands(): ExtensionCommand[] {
  const [offered, setOffered] = useState<ExtensionCommand[]>([]);
  useEffect(() => {
    let gone = false;
    void commands
      .extensionCommands()
      .then((said) => {
        if (!gone && said.status !== "error") setOffered(said.data ?? []);
      })
      .catch(() => {
        // Nothing: no command is offered, and `Extensions.tsx` is where the reason is read.
      });
    return () => {
      gone = true;
    };
  }, []);
  return offered;
}

/**
 * The questions in flight, by plane and view, so one opening asks once.
 *
 * **React's development double effect mounts a view, unmounts it and mounts it again**, and an
 * operator flicking between two tabs does the same thing by hand. Each of those used to be a
 * second question to the same program while the first was still being answered, which the
 * executor refuses as *"still answering"* — a refusal the operator did nothing to earn. A second
 * asker of the same view while the first is out shares the first's answer; the core queues
 * questions to one extension about different views (`views.rs`, the turn it takes).
 */
const inFlight = new Map<string, Promise<ViewAnswerOrRefusal>>();

type ViewAnswerOrRefusal = { answer: ViewAnswer } | { refused: string };

function ask(
  plane: PlaneId,
  view: ViewRef,
  workspace: string | undefined,
): Promise<ViewAnswerOrRefusal> {
  const key = `${plane}\u0000${workspace ?? ""}\u0000${viewKey(view)}`;
  const out = inFlight.get(key);
  if (out) return out;
  const asking = commands
    .openView(plane, view.from, view.view, view.key, workspace ?? null)
    .then((said): ViewAnswerOrRefusal => {
      if (said.status === "error") return { refused: said.error };
      // An extension rewrites its facts file whenever it answers (charter-app#340), so its
      // badges and repo cells are read again now.
      if (view.from !== null) factsChanged(plane);
      return { answer: said.data };
    })
    .catch((err: unknown): ViewAnswerOrRefusal => ({ refused: String(err) }))
    .finally(() => inFlight.delete(key));
  inFlight.set(key, asking);
  return asking;
}

/**
 * What a view is about, as `charter_core::panel::Subject` words it: charter's persona view is
 * about personas, and an extension's view says (`ExtensionView.about`). It decides which other
 * views a view offers beside itself — never the view's contributor.
 */
export function aboutOf(view: ViewRef, offered: readonly ExtensionView[]): string | undefined {
  if (view.from === null) return view.view === "persona" ? "personas" : undefined;
  return offered.find((one) => one.extension === view.from && one.id === view.view)?.about;
}

/** charter's own views' glyphs, by view. */
const OWN_MARKS: Record<string, React.ComponentType<{ className?: string }>> = {
  persona: UserRound,
  vault: KeyRound,
  settings: Settings2,
  saving: Save,
  [WORKSPACE_SETTINGS]: Settings2,
  preferences: SlidersHorizontal,
};

/** The glyph a view's tab carries: a person for a persona, a piece of a puzzle for a view an
 *  extension offers — which says *a plugin's* before any word is read. */
export function ViewMark({ view }: { view: ViewRef }) {
  const Mark = view.from !== null ? Puzzle : (OWN_MARKS[view.view] ?? ChartColumn);
  return <Mark className="tab-mark" aria-hidden="true" />;
}

/**
 * One view, in a pane: its heading, the views offered beside it, and its answer.
 *
 * **The heading is charter's.** The view's title, whose it is (ADR 0041 item 5 — what is in force
 * is shown after approval, not only at it), and a button for every approved extension's view
 * about the same subject — the operator's Statistics button on a persona's tab, drawn only when
 * an extension offers one, and opening its own tab rather than a section of this one. That is
 * the whole of the consent story: **reading a persona asks no extension anything**; pressing
 * Statistics does, and it is the operator's press.
 */
export function ViewPane({
  plane,
  view,
  title,
  workspace,
  waits,
  offered,
  onOpenView,
  onAsk,
  onVaultChanged,
}: {
  plane: PlaneId;
  view: ViewRef;
  /** What the tab is called — the heading says the same. */
  title: string;
  /** The workspace whose strip the view is on, or `undefined` outside every workspace: its
   *  settings are a layer of what an extension's view is asked under (charter-app#280). */
  workspace?: string;
  /** Put back by a launch and not asked yet (`tabs.Content.waits`). */
  waits: boolean;
  /** Every view approved extensions offer this window. */
  offered: readonly ExtensionView[];
  onOpenView: (view: ViewRef, title: string) => void;
  /** The operator pressed to have a waiting view asked. */
  onAsk: () => void;
  /** A vault's tab wrote to its vault. */
  onVaultChanged: () => void;
}) {
  // **A vault is charter's own view, and the one a panel answer cannot draw**: a table the
  // operator writes to (charter-app#235). Same tab, same path, same record — its own drawing.
  // Keyed by the vault, so a pane that comes to show another vault starts from "opening".
  if (view.from === null && view.view === "vault") {
    return (
      <VaultTab
        key={`${plane}\u0000${view.key}`}
        plane={plane}
        vault={view.key}
        onChanged={onVaultChanged}
      />
    );
  }
  const about = aboutOf(view, offered);
  const beside = offered.filter(
    (one) =>
      about !== undefined &&
      one.about === about &&
      !(one.extension === view.from && one.id === view.view),
  );
  // charter's own views run no program, so there is nothing a press would be consent to.
  const holding = waits && view.from !== null;
  return (
    <section
      className="view-pane"
      data-testid={`view-pane-${viewKey(view).replace(/\//g, "-")}`}
      aria-label={title}
    >
      <header className="view-head">
        <h2>
          <ViewMark view={view} />
          {title}
          {view.from !== null && <span className="panel-from">{` · ${view.from}`}</span>}
        </h2>
        {beside.map((one) => {
          // Opened about the same thing this view is about: statistics from steward's tab are
          // steward's statistics, and from the whole plane's view, the whole plane's.
          const key = view.key;
          const called = key === "" ? one.title : `${one.title} · ${key}`;
          return (
            <button
              key={`${one.extension}/${one.id}`}
              type="button"
              className="panel-view"
              // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
              tabIndex={0}
              title={`Asks ${one.extension}, in a tab of its own`}
              onClick={() => onOpenView({ from: one.extension, view: one.id, key }, called)}
            >
              <ChartColumn className="node-icon" aria-hidden="true" />
              {one.title}
            </button>
          );
        })}
      </header>
      <div className="view-body">
        {holding ? (
          <EmptyState
            mark={Puzzle}
            headline={`${title} was open when charter last quit`}
            body={`Showing it asks ${view.from}'s program again, and charter does not do that until you say so.`}
            action={
              <button type="button" tabIndex={0} onClick={onAsk}>
                {`Ask ${view.from}`}
              </button>
            }
            testid="view-waits"
          />
        ) : isSettings(view) ? (
          /* **The one built-in view that is not an answer.** Opened, filed, deduplicated and
             put back at a launch exactly as every other view is; what differs is its body: a
             form writes, and the panel vocabulary is for reading. Keyed by the plane, so a pane
             that comes to show another project's settings starts from its own read. */
          <ProjectSettings key={plane} plane={plane} />
        ) : isWorkspaceSettings(view) ? (
          /* A workspace's settings (charter-app#280): Project settings' body, for the one file
             a workspace holds. Keyed by both, for the same reason. */
          <WorkspaceSettings key={`${plane}\u0000${view.key}`} plane={plane} workspace={view.key} />
        ) : isSaving(view) ? (
          /* The plane's save standing and its save button (charter-app#294). Keyed by the
             plane, so a pane that comes to show another project's starts from its own read. */
          <SavingView key={plane} plane={plane} />
        ) : isPreferences(view) ? (
          /* The machine's, not the plane's (charter-app#283): the same surface whichever
             project's strip it was opened on. */
          <Preferences />
        ) : (
          /* Keyed by the view, so a pane that comes to show another view starts from "asking"
             rather than drawing the last view's answer under the new one's title. */
          <Answer
            key={`${plane}\u0000${workspace ?? ""}\u0000${viewKey(view)}`}
            plane={plane}
            view={view}
            title={title}
            workspace={workspace}
          />
        )}
      </div>
    </section>
  );
}

/** Whether `view` is the Project settings view (charter-app#252). */
function isSettings(view: ViewRef): boolean {
  return viewKey(view) === viewKey(SETTINGS_VIEW);
}

/** Whether `view` is a workspace's settings view (charter-app#280). */
function isWorkspaceSettings(view: ViewRef): boolean {
  return view.from === null && view.view === WORKSPACE_SETTINGS;
}

/** Whether `view` is the Saving view (charter-app#294). */
function isSaving(view: ViewRef): boolean {
  return viewKey(view) === viewKey(SAVING_VIEW);
}

/** Whether `view` is the Preferences view (charter-app#283). */
function isPreferences(view: ViewRef): boolean {
  return viewKey(view) === viewKey(PREFERENCES_VIEW);
}

/** A view asked now, and its answer, its refusal, or the sentence saying its source has gone. */
function Answer({
  plane,
  view,
  title,
  workspace,
}: {
  plane: PlaneId;
  view: ViewRef;
  title: string;
  workspace: string | undefined;
}) {
  const [said, setSaid] = useState<ViewAnswerOrRefusal>();
  const { from, view: id, key } = view;
  // **What an action on one of its rows answered** (charter-app#341): the view's blocks,
  // refreshed, when it answered them; and a sentence — its refusal, or what changed outside the
  // extension's declared paths — to say above the answer.
  const [refreshed, setRefreshed] = useState<readonly PanelBlock[]>();
  // `undefined` until an action has run; then what that action came to — which replaces the
  // question's own sentence, so the red line is always about the last thing the operator did.
  const [acted, setActed] = useState<{ said: string | null }>();
  const [asking, setAsking] = useState<{ row: PanelRow; action: RowAction }>();

  useEffect(() => {
    let gone = false;
    void ask(plane, { from, view: id, key }, workspace).then((answered) => {
      if (!gone) setSaid(answered);
    });
    return () => {
      gone = true;
    };
  }, [plane, from, id, key, workspace]);

  if (said === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        {from === null ? "Reading the plane…" : `Asking ${from}…`}
      </p>
    );
  }
  if ("refused" in said) {
    // The core's sentence, which names what refused and says what to do. A view that came up
    // empty would read as a plane with nothing in it.
    return (
      <p className="trouble" role="alert">
        {said.refused}
      </p>
    );
  }
  const answer = said.answer;
  /** Run `action` on `row`, and say the refusal, or draw what it answered. */
  const act = async (
    row: PanelRow,
    action: RowAction,
    confirmed: boolean,
  ): Promise<string | undefined> => {
    if (from === null) return "charter's own views offer no extension's actions.";
    const outcome = await runExtensionAction(
      plane,
      from,
      action,
      { view: id, key, row: row.key },
      workspace,
      confirmed,
    );
    if ("refused" in outcome) return outcome.refused;
    if (outcome.answer.blocks !== null) setRefreshed(outcome.answer.blocks);
    setActed({ said: outcome.answer.overreach });
    return undefined;
  };
  if (answer.kind === "gone") {
    // **Not an error.** A tab the last launch left open can name a persona since deleted or an
    // extension since uninstalled; there is nothing to repair, so it is a sentence in the middle
    // of the tab and the tab's `×` is the way out.
    return (
      <EmptyState headline={`${title} is not here any more`} body={answer.why} testid="view-gone" />
    );
  }
  const seen = acted === undefined ? answer.overreach : acted.said;
  return (
    <>
      {seen !== undefined && seen !== null && (
        /* What the core saw change outside the extension's declared paths, or an action's
           refusal: the core's sentence, naming the extension, above what it answered. */
        <p className="trouble" role="alert">
          {seen}
        </p>
      )}
      <AnsweredBlocks
        blocks={refreshed ?? answer.blocks}
        label={title}
        onAct={
          from === null
            ? undefined
            : (row, action) => {
                if (action.asks_first) {
                  setAsking({ row, action });
                  return;
                }
                void act(row, action, false).then((refused) => {
                  if (refused !== undefined) setActed({ said: refused });
                });
              }
        }
      />
      {asking !== undefined && from !== null && (
        <AskFirst
          extension={from}
          action={asking.action}
          onRun={() =>
            act(asking.row, asking.action, true).then((refused) => {
              // Ran: the dialog closes, and what it saw is said above the answer.
              if (refused === undefined) {
                setAsking(undefined);
                return undefined;
              }
              return { refused };
            })
          }
          onCancel={() => setAsking(undefined)}
        />
      )}
      <p className="view-took">
        {from === null ? `read in ${answer.took_ms} ms` : `answered in ${answer.took_ms} ms`}
      </p>
    </>
  );
}

/** An answer's blocks, each drawn with what a panel's is drawn with. */
function AnsweredBlocks({
  blocks,
  label,
  onAct,
}: {
  blocks: readonly PanelBlock[];
  label: string;
  /** Run one of the extension's actions a row offers; `undefined` for charter's own views. */
  onAct?: (row: PanelRow, action: RowAction) => void;
}) {
  const [open, setOpen] = useState<string>();
  return (
    <>
      {blocks.map((block, at) =>
        block.kind === "note" ? (
          <p
            /* By position: an answer's blocks arrive whole from one round trip and are never
               spliced, which is `Panels.tsx`'s reason for the same key. */
            key={at}
            className={block.tone === "trouble" ? "trouble" : "note"}
            role={block.tone === "trouble" ? "alert" : undefined}
          >
            {block.text}
          </p>
        ) : block.kind === "chart" ? (
          <Chart key={at} chart={block} />
        ) : block.kind === "facts" ? (
          <Facts key={at} facts={block} />
        ) : (
          <PanelList
            key={at}
            rows={block.rows}
            empty={block.empty}
            label={label}
            open={open}
            onOpen={setOpen}
            onAct={onAct}
            // A tab has the room a side region does not, so a page is twenty rather than twelve.
            page={20}
          />
        ),
      )}
    </>
  );
}

/** A facts block, as the wire carries it. */
export type FactsBlock = Extract<PanelBlock, { kind: "facts" }>;

/**
 * **Labelled facts, as a description list** — `charter_core::panel::Block::Facts`: what a
 * persona's definition says, label beside value, the two columns the persona card drew. A
 * `<dl>` because that is what it is, so a screen reader announces each value with its label.
 */
export function Facts({ facts }: { facts: FactsBlock }) {
  return (
    <dl className="facts" data-testid="facts">
      {facts.facts.map((fact, at) => (
        /* By position: a block's facts arrive whole and are never spliced, and two facts may
           share a label. */
        <div key={at} className="fact">
          <dt>{fact.label}</dt>
          <dd>{fact.value}</dd>
        </div>
      ))}
    </dl>
  );
}

/** A chart block, as the wire carries it. */
export type ChartBlock = Extract<PanelBlock, { kind: "chart" }>;

/**
 * **A chart, drawn by charter from numbers and words** — `charter_core::panel::Chart`.
 *
 * Everything that makes it look like something is charter's: the bars are one theme token
 * (`accent.base`) on another (`surface.sunken`), scaled to the largest value here, and every
 * label is a text node. The producer chose the numbers, the words and one of two shapes; it
 * could not choose a colour, a width, a font or an axis, because the vocabulary has no word for
 * any of them.
 *
 * **It is a list to a screen reader, and that is the accessible version of a bar chart**: each
 * point reads as its label, its value and its note — *"steward 30 · 71%"* — and the bar itself
 * is `aria-hidden`, because a length is the one thing about it that is not also said in words.
 *
 * **Still.** No transition and no entry animation: an answer arrives once per opening, and a
 * chart whose bars grew into place every time it was opened would be motion that says nothing
 * the numbers do not (charter-app#209's rule about what stays still).
 */
export function Chart({ chart }: { chart: ChartBlock }) {
  const most = Math.max(1, ...chart.points.map((point) => point.value));
  const columns = chart.shape === "columns";
  return (
    <figure className={columns ? "chart chart-columns" : "chart chart-bars"} data-testid="chart">
      <figcaption>
        {chart.title}
        {chart.unit !== null && <span className="chart-unit">{` · ${chart.unit}`}</span>}
      </figcaption>
      {chart.points.length === 0 ? (
        <p className="none">Nothing to draw.</p>
      ) : (
        <ol className="chart-points" aria-label={chart.title}>
          {chart.points.map((point, at) => (
            <ChartPoint
              /* By position: two points may share a label ("0 others" and a persona called
                 that), and the order is the producer's meaning. */
              key={at}
              point={point}
              share={point.value / most}
              columns={columns}
            />
          ))}
        </ol>
      )}
    </figure>
  );
}

function ChartPoint({
  point,
  share,
  columns,
}: {
  point: PanelPoint;
  share: number;
  columns: boolean;
}) {
  // A percentage of the track, which is a length and not a colour or a timing, so it is the
  // one inline style this file writes. Never zero-width for a non-zero value: a bar that
  // rounds to nothing reads as a count of nothing.
  const percent = point.value === 0 ? 0 : Math.max(2, Math.round(share * 100));
  const size = `${percent}%`;
  return (
    <li className="chart-point">
      <span className="chart-label" title={point.label}>
        {point.label}
      </span>
      <span className="chart-track" aria-hidden="true">
        <span className="chart-bar" style={columns ? { blockSize: size } : { inlineSize: size }} />
      </span>
      <span className="chart-value">
        {point.value}
        {point.note !== null && <span className="chart-note">{` · ${point.note}`}</span>}
      </span>
    </li>
  );
}
