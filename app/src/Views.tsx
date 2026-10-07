import {
  Fragment,
  Suspense,
  lazy,
  useEffect,
  useId,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  Archive,
  Brain,
  ChartColumn,
  CircleDashed,
  Cpu,
  Download,
  FileCode,
  GitCompare,
  FileText,
  FolderGit2,
  GitPullRequest,
  History,
  KeyRound,
  ListChecks,
  LoaderCircle,
  Puzzle,
  RefreshCw,
  Save,
  Search,
  Send,
  Settings2,
  UserRound,
} from "lucide-react";
import { PersonaMark } from "./PersonaMark";
import { PersonaMarkPicker } from "./PersonaMarkPicker";
import { EmptyState } from "./EmptyState";
import { AskFirst, runExtensionAction } from "./ExtensionAction";
import { LandAsk, PushAsk, askLand } from "./ChangeActions";
import { PanelList } from "./PanelList";
import { HarnessSetupTab } from "./HarnessSetupTab";
import { RepoInstructionsTab } from "./RepoInstructionsTab";
import { FirstTaskTab, type FirstTaskDoes } from "./FirstTaskTab";
import { SettingsTab } from "./settings/SettingsTab";
import { DispatchLimitsTable } from "./settings/dispatch";
import {
  commands,
  type ExtensionCommand,
  type ChangeList,
  type ExtensionView,
  type LandQuestion,
  type MemoryView,
  type PanelBlock,
  type PanelPoint,
  type PanelRow,
  type PlaneId,
  type RowAction,
  type ViewAnswer,
} from "./bindings";
import { MemoryTab } from "./MemoryTab";
import { MemoryArchiveTab } from "./MemoryArchiveTab";
import { Menued } from "./Menus";
import {
  ARCHIVE_VIEW,
  DRAFT,
  MEMORY_VIEW,
  SHARED_MEMORY_VIEW,
  archiveOf,
  isMemory,
  memoryRefOf,
} from "./memories";
import { HeadingOffer } from "./PanelSection";
import { SavingView } from "./SavingView";
import { SessionRecordTab } from "./SessionRecordTab";
import { TodoTab } from "./TodoTab";
import { TODO_VIEW, todoRefOf } from "./todos";
import { pieceDiffOf, pieceOf } from "./pieceViews";
import { SEARCH, isSearch } from "./contentSearch";
import { SearchTab } from "./SearchTab";
import { SESSION_VIEW, sessionTitle, sessionView } from "./sessions";
import { DispatchesTab } from "./DispatchesTab";
import { DISPATCHES_VIEW, isDispatches } from "./dispatches";
import {
  SAVING_VIEW,
  SETTINGS_TAB_TITLE,
  settingsLevelOf,
  settingsView,
  viewKey,
  workspaceSettingsTitle,
  workspaceSettingsView,
  type ViewRef,
} from "./tabs";
import { VaultTab } from "./VaultTab";
import { listedMemoryOffers, memoryKeyRun, toKeep, type Offer } from "./actions";
import { factsChanged } from "./extensionFacts";
import { usePlaneChanged, VIEWS } from "./planeChanged";

/** What `workspaceSettingsView` names Settings at a workspace's level (SE-20). */
const WORKSPACE_SETTINGS = "workspace-settings";

/** What `repoInstructionsView` names a workspace's repo instructions view (FR-18a). */
const REPO_INSTRUCTIONS = "repo-instructions";

/** What `firstTaskView` names the first task's view (FR-28). */
const FIRST_TASK = "first-task";
/** What `harnessSetupView` names the harness setup view (FR-29). */
const HARNESS_SETUP_VIEW = "harness-setup";

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

/**
 * **The last reading of each workspace's changes, kept until Refresh is pressed** (charter#470).
 * That view asks a forge, and its tab's pane is drawn again every time the operator switches back
 * to its workspace; asking again then would be a forge call on a switch, which the view promises
 * not to make. So its answer is kept here, by plane and view, and only Refresh replaces it.
 */
const kept = new Map<string, ViewAnswerOrRefusal>();

/** Whether `view` is a workspace's changes, the one built-in view that reads a forge. */
function isChanges(view: ViewRef): boolean {
  return view.from === null && view.view === "changes";
}

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
 * What a view is about, as `purlis_core::panel::Subject` words it: charter's persona view is
 * about personas, and an extension's view says (`ExtensionView.about`). It decides which other
 * views a view offers beside itself — never the view's contributor.
 */
export function aboutOf(view: ViewRef, offered: readonly ExtensionView[]): string | undefined {
  if (view.from === null) return view.view === "persona" ? "personas" : undefined;
  return offered.find((one) => one.extension === view.from && one.id === view.view)?.about;
}

/** charter's own views' glyphs, by view — and so the list of every view charter has, which
 *  `theme/views.test.tsx` holds its token test to: a view added here is a view it must draw. */
export const OWN_MARKS: Record<string, React.ComponentType<{ className?: string }>> = {
  persona: UserRound,
  vault: KeyRound,
  settings: Settings2,
  saving: Save,
  changes: GitPullRequest,
  [SESSION_VIEW]: History,
  /** The project's dispatches (#1452). */
  [DISPATCHES_VIEW.view]: Send,
  [MEMORY_VIEW]: Brain,
  /** One todo (#1214), drawn as the Todos panel marks one. */
  [TODO_VIEW]: CircleDashed,
  [SHARED_MEMORY_VIEW.view]: Brain,
  [ARCHIVE_VIEW]: Archive,
  [WORKSPACE_SETTINGS]: Settings2,
  [REPO_INSTRUCTIONS]: FileText,
  [FIRST_TASK]: ListChecks,
  [HARNESS_SETUP_VIEW]: Download,
  /** A harness's capability card (HP-19, `tabs.harnessCardView`). */
  harness: Cpu,
  "piece-files": FolderGit2,
  "piece-file": FileCode,
  /** One file against its branch's base (FM-11). */
  "piece-diff": GitCompare,
  [SEARCH]: Search,
};

/** The glyph a view's tab carries: a person for a persona, a piece of a puzzle for a view an
 *  extension offers — which says *a plugin's* before any word is read. */
export function ViewMark({ view }: { view: ViewRef }) {
  // A persona's own view wears that persona's mark (#1449), on its tab and its heading.
  if (view.from === null && view.view === "persona" && view.key !== "")
    return <PersonaMark persona={view.key} className="tab-mark" />;
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
  offerFor,
  onPress,
  changed = 0,
  onMemorySaved,
  onCloseView,
  firstTask,
  split,
  onSplit,
  onShowInstead,
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
  /** The catalogue, by row id: what the heading's own rows are looked up in (SI-3). */
  offerFor?: (id: string) => Offer | undefined;
  onPress?: (offer: Offer) => void;
  /**
   * Bumped when the plane changes on disk or a memory is written (SI-9b, ADR 0065 Q10): the
   * lists and memories charter's own views read are read again, on the trigger the Todos panel
   * refreshes on. An extension's view is never asked again for it — its program runs on a
   * press, not on a file changing.
   */
  changed?: number;
  /** A memory's tab wrote its memory: the tab follows it — its name, and its key when a new
   *  memory's tab has just made one. */
  onMemorySaved?: (from: ViewRef, memory: MemoryView) => void;
  /** Close the tab showing `view` — a new memory's Cancel. */
  onCloseView?: (view: ViewRef) => void;
  /** What the first task's tab asks the plane to do: start a run, show a run's diff (FR-28). */
  firstTask?: FirstTaskDoes;
  /** Where this view's divider was, as its first side's share in percent (FM-2). */
  split?: number;
  /** The operator moved this view's divider. */
  onSplit?: (split: number) => void;
  /** The pane now shows `to` rather than `from`, under `title`: a Search tab that asks
   *  something new (FM-8). */
  onShowInstead?: (from: ViewRef, to: ViewRef, title: string) => void;
}) {
  // **The view follows the disk itself** (FD-10), on every change: the lists and memories
  // charter's own views read are any of the plane's stores. Here and not in the window, so a
  // memory an agent saves redraws this pane and not everything around it.
  const onDisk = usePlaneChanged([plane], VIEWS);
  /** The level this is the Settings tab at, when it is that tab (SE-16, SE-17). */
  const settingsLevel = settingsLevelOf(view);
  // **What charter can do to the thing this tab is about, on its heading** (SI-3): a persona's
  // `persona.md` handed to the operator's editor and the persona deleted, a vault deleted. The
  // catalogue's rows, so the heading, the palette and a row's menu cannot disagree, and each
  // destructive one asks in its own dialog before anything goes.
  //
  // A memory list's `+` (SI-9c, ADR 0065 Q9) is one of these rows too, drawn as the glyph the
  // right column's section headings draw it as.
  const own = (view.from === null ? (OWN_ROWS[view.view]?.(view.key) ?? []) : []).map((id) =>
    id.startsWith("memory.new:") ? (
      onPress && <HeadingOffer key={id} offer={offerFor?.(id)} onPress={onPress} />
    ) : id.startsWith("memory.archived:") ? (
      // The store's archive (KN-4), beside its `+`, drawn as the Memory section draws it.
      onPress && <HeadingOffer key={id} offer={offerFor?.(id)} onPress={onPress} mark={Archive} />
    ) : (
      <OfferButton key={id} offer={offerFor?.(id)} onPress={onPress} />
    ),
  );
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
        actions={own}
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
  const memoryAt = isMemory(view) ? memoryRefOf(view.key) : undefined;
  const todoAt = view.from === null && view.view === TODO_VIEW ? todoRefOf(view.key) : undefined;
  const archiveAt = archiveOf(view);
  const piece = pieceOf(view);
  const pieceDiff = pieceDiffOf(view);
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
        {own}
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
      {/* A persona's icon and colour, picked here and written to its definition (#1449). */}
      {view.from === null && view.view === "persona" && view.key !== "" && (
        <PersonaMarkPicker key={`${plane}\u0000${view.key}`} plane={plane} persona={view.key} />
      )}
      <div className="view-body">
        {holding ? (
          <EmptyState
            mark={Puzzle}
            headline={`${title} was open when purlis last quit`}
            body={`Showing it asks ${view.from}'s program again, and purlis does not do that until you say so.`}
            action={
              <button type="button" tabIndex={0} onClick={onAsk}>
                {`Ask ${view.from}`}
              </button>
            }
            testid="view-waits"
          />
        ) : isRepoInstructions(view) ? (
          /* The agent instructions a workspace's repo carries, offered to its memory (FR-18a):
             a preview whose press is the only thing that writes. Keyed by both, as a
             workspace's settings are. */
          <RepoInstructionsTab
            key={`${plane}\u0000${view.key}`}
            plane={plane}
            workspace={view.key}
            onClose={() => onCloseView?.(view)}
          />
        ) : isFirstTask(view) ? (
          /* The first task (FR-28): two runs of one task, each on a branch of its own, keyed by
             the repo's clone the runs are cut from. */
          <FirstTaskTab
            key={`${plane}\u0000${view.key}`}
            plane={plane}
            clone={view.key}
            does={firstTask}
          />
        ) : isHarnessSetup(view) ? (
          /* No harness found (FR-29): each one's installer, run in a shell tab on a press.
             Keyed by both, as the repo's instructions are. */
          <HarnessSetupTab
            key={`${plane}\u0000${view.key}`}
            plane={plane}
            cwd={view.key}
            workspace={workspace ?? ""}
          />
        ) : isSaving(view) ? (
          /* The plane's save standing and its save button (charter-app#294). Keyed by the
             plane, so a pane that comes to show another project's starts from its own read. */
          <SavingView key={plane} plane={plane} workspace={workspace} />
        ) : isSearch(view) ? (
          /* ⌘⇧F's Search tab (FM-8). Keyed by the plane alone: its view follows what it asks,
             and a new query is the same tab asking again, not another tab. */
          <SearchTab key={plane} plane={plane} view={view} onAsk={onShowInstead} />
        ) : piece !== undefined ? (
          /* A piece's files, or one of them, in the light editor (RC-5). Keyed by the view, so
             a pane that comes to show another file starts from its own read. */
          <Suspense fallback={OPENING}>
            {piece.path === undefined ? (
              <PieceFilesTab
                key={`${plane}\u0000${view.key}`}
                plane={plane}
                cut={piece.place}
                onOpenView={onOpenView}
                split={split}
                onSplit={onSplit}
                onPress={onPress}
              />
            ) : (
              <PieceFileTab
                key={`${plane}\u0000${view.key}`}
                plane={plane}
                cut={piece.place}
                path={piece.path}
              />
            )}
          </Suspense>
        ) : pieceDiff !== undefined ? (
          /* One file against its branch's base (FM-11), in the light editor's merge view. */
          <Suspense fallback={OPENING}>
            <PieceDiffTab
              key={`${plane}\u0000${view.key}`}
              plane={plane}
              cut={pieceDiff.place}
              path={pieceDiff.path}
            />
          </Suspense>
        ) : isDispatches(view) ? (
          /* The project's dispatches (#1452), read from the app's own records. A row opens its
             chat through the catalogue's verb, or its session record as any record opens. */
          <DispatchesTab
            key={plane}
            plane={plane}
            changed={changed + onDisk}
            onShowChat={(session) =>
              onPress?.({
                id: `dispatch.chat:${session}`,
                title: "Show its chat",
                available: true,
                reason: "",
                does: { verb: "showChat", session },
              })
            }
            onOpenRecord={(path, title) => onOpenView(sessionView(path), sessionTitle(title))}
          />
        ) : isSession(view) ? (
          /* A session record (SI-8d): read-only Markdown the window renders from the core's
             `session_record`, keyed by the record so a pane that comes to show another starts
             from its own read. */
          <SessionRecordTab key={`${plane}\u0000${view.key}`} plane={plane} path={view.key} />
        ) : todoAt !== undefined ? (
          /* A todo (#1214): its facts and its whole text, read again when the plane changes,
             keyed by the todo so a pane that comes to show another starts from its own read. */
          <TodoTab key={`${plane}\u0000${view.key}`} plane={plane} at={todoAt} changed={onDisk} />
        ) : memoryAt !== undefined ? (
          /* A memory (SI-9b, ADR 0065): read as Markdown, edited in place, keyed by the memory
             so a pane that comes to show another — the preview tab, replaced — starts from its
             own read. */
          <MemoryTab
            key={`${plane}\u0000${view.key}`}
            plane={plane}
            at={memoryAt}
            changed={changed + onDisk}
            onSaved={(memory) => onMemorySaved?.(view, memory)}
            onClose={() => onCloseView?.(view)}
          />
        ) : archiveAt !== undefined ? (
          /* A store's archive (KN-4): what was deleted from it, read-only, each with Restore.
             Keyed by the store, so a pane that comes to show another store's starts from its
             own read. */
          <MemoryArchiveTab
            key={`${plane}\u0000${view.key}`}
            plane={plane}
            scope={archiveAt}
            changed={changed + onDisk}
          />
        ) : settingsLevel !== undefined ? (
          /* Settings (SE-16, SE-17, SE-20), at the level the tab is keyed by. Its switcher moves
             this tab to another level, or brings forward the tab already there (D-SE17a). The
             workspace it offers is the one it is at, else the one whose strip it is on. */
          <SettingsTab
            plane={plane}
            workspace={settingsLevel === "workspace" ? view.key : workspace}
            level={settingsLevel}
            onLevelChange={(to) => {
              const at = settingsLevel === "workspace" ? view.key : workspace;
              if (to === "you" || to === "project")
                onShowInstead?.(view, settingsView(to), SETTINGS_TAB_TITLE);
              else if (to === "workspace" && at !== undefined)
                onShowInstead?.(view, workspaceSettingsView(at), workspaceSettingsTitle(at));
            }}
          />
        ) : (
          /* Keyed by the view, so a pane that comes to show another view starts from "asking"
             rather than drawing the last view's answer under the new one's title. */
          <Answer
            key={`${plane}\u0000${workspace ?? ""}\u0000${viewKey(view)}`}
            plane={plane}
            view={view}
            title={title}
            workspace={workspace}
            changed={view.from === null ? changed + onDisk : 0}
            offerFor={offerFor}
            onPress={onPress}
          />
        )}
      </div>
    </section>
  );
}

/** The catalogue rows a charter view's heading offers, by view, for the thing it shows. */
const OWN_ROWS: Record<string, (key: string) => string[]> = {
  persona: (key) => [
    `persona.edit:${key}`,
    // The profile its chats start on (#1445): the view's Profile fact, set here.
    `persona.profile:${key}`,
    `persona.remove:${key}`,
    // The persona's memory list's `+` (SI-9c), and its archive (KN-4).
    `memory.new:persona/${key}`,
    `memory.archived:persona/${key}`,
  ],
  // The shared store's own list (ADR 0065 Q6): its `+`, and its archive (KN-4).
  [SHARED_MEMORY_VIEW.view]: () => ["memory.new:shared", "memory.archived:shared"],
  vault: (key) => [`vault.remove:${key}`],
  [SESSION_VIEW]: (key) => [`session.resume:${key}`],
  // A todo's Mark done and Forget (#1214): the focused workspace's rows, and a todo's tab is on
  // its workspace's strip, which is the focused one whenever the tab is in front.
  [TODO_VIEW]: (key) => {
    const at = todoRefOf(key);
    return at === undefined ? [] : [`todo.done:${at.slug}`, `todo.forget:${at.slug}`];
  },
  // A memory's Edit and Delete (ADR 0065 Q2) — a new memory's tab has neither yet.
  [MEMORY_VIEW]: (key) =>
    memoryRefOf(key)?.slug === DRAFT ? [] : [`memory.edit:${key}`, `memory.delete:${key}`],
};

/** One catalogue row as a heading's button, in its own words. A row the catalogue does not
 *  offer draws nothing. */
function OfferButton({ offer, onPress }: { offer?: Offer; onPress?: (offer: Offer) => void }) {
  if (!offer || !onPress) return null;
  return (
    <button
      type="button"
      className="panel-view"
      // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
      tabIndex={0}
      disabled={!offer.available}
      title={offer.available ? offer.note : offer.reason}
      onClick={() => onPress(offer)}
    >
      {offer.title}
    </button>
  );
}

/**
 * **The light editor's tabs, loaded the first time one is drawn** (RC-5). CodeMirror and its
 * grammars are a chunk of their own, so a window that never opens a file or a diff never
 * parses any of it (ADR 0086 rows M2 and L6).
 */
const PieceFilesTab = lazy(() =>
  import("./editor/PieceFiles").then((module) => ({ default: module.PieceFilesTab })),
);
const PieceFileTab = lazy(() =>
  import("./editor/PieceFiles").then((module) => ({ default: module.PieceFileTab })),
);

const PieceDiffTab = lazy(() =>
  import("./editor/PieceDiff").then((module) => ({ default: module.PieceDiffTab })),
);

/** What a light editor tab shows while its chunk arrives. */
const OPENING = <EmptyState mark={LoaderCircle} headline="Opening the light editor…" />;

/** Whether `view` is a workspace's repo instructions (FR-18a). */
function isRepoInstructions(view: ViewRef): boolean {
  return view.from === null && view.view === REPO_INSTRUCTIONS;
}

/** Whether `view` is the first task's view (FR-28). */
function isFirstTask(view: ViewRef): boolean {
  return view.from === null && view.view === FIRST_TASK;
}

/** Whether `view` is the harness setup view (FR-29). */
function isHarnessSetup(view: ViewRef): boolean {
  return view.from === null && view.view === HARNESS_SETUP_VIEW;
}

/** Whether `view` is the Saving view (charter-app#294). */
function isSaving(view: ViewRef): boolean {
  return viewKey(view) === viewKey(SAVING_VIEW);
}

/** Whether `view` is a session record (SI-8d). */
function isSession(view: ViewRef): boolean {
  return view.from === null && view.view === SESSION_VIEW;
}

/** A view asked now, and its answer, its refusal, or the sentence saying its source has gone. */
function Answer({
  plane,
  view,
  title,
  workspace,
  changed = 0,
  offerFor,
  onPress,
}: {
  plane: PlaneId;
  view: ViewRef;
  title: string;
  workspace: string | undefined;
  /** Asks again when it moves: charter's own views only (`ViewPane.changed`). */
  changed?: number;
  /** The catalogue, by row id, and what pressing one of its rows does — what a row of charter's
   *  own view runs and what its menu lists (a persona's memories, SI-9b). */
  offerFor?: (id: string) => Offer | undefined;
  onPress?: (offer: Offer) => void;
}) {
  const { from, view: id, key } = view;
  const dispatchHeading = useId();
  const keeps = isChanges(view);
  const keptAs = `${plane}\u0000${viewKey(view)}`;
  const [said, setSaid] = useState<ViewAnswerOrRefusal | undefined>(() =>
    keeps ? kept.get(keptAs) : undefined,
  );
  // Bumped by Refresh: the one thing that asks a kept view again.
  const [round, setRound] = useState(0);
  // **What an action on one of its rows answered** (charter-app#341): the view's blocks,
  // refreshed, when it answered them; and a sentence — its refusal, or what changed outside the
  // extension's declared paths — to say above the answer.
  const [refreshed, setRefreshed] = useState<readonly PanelBlock[]>();
  // `undefined` until an action has run; then what that action came to — which replaces the
  // question's own sentence, so the red line is always about the last thing the operator did.
  const [acted, setActed] = useState<{ said: string | null }>();
  const [asking, setAsking] = useState<{ row: PanelRow; action: RowAction }>();
  // **A change's Push and Land** (#474): which change's push is being asked about, which
  // member's landing, which Land is still running its gates, and each refusal, by row.
  const [pushing, setPushing] = useState<string>();
  const [landing, setLanding] = useState<{ change: string; question: LandQuestion }>();
  const [checking, setChecking] = useState<string>();
  const [refusedAt, setRefusedAt] = useState<ReadonlyMap<string, string>>(new Map());

  useEffect(() => {
    if (keeps && round === 0 && kept.has(keptAs)) return;
    let gone = false;
    void ask(plane, { from, view: id, key }, workspace).then((answered) => {
      if (keeps) kept.set(keptAs, answered);
      if (!gone) setSaid(answered);
    });
    return () => {
      gone = true;
    };
  }, [plane, from, id, key, workspace, keeps, keptAs, round, changed]);

  /** Ask a kept view again: its Refresh, and what a push or landing that ran comes to. */
  const askAgain = () => {
    kept.delete(keptAs);
    setRefusedAt(new Map());
    setSaid(undefined);
    setRound((n) => n + 1);
  };

  /** Refresh, for a kept view: its only way to be asked again. */
  const refresh = keeps ? (
    <button
      type="button"
      className="panel-view view-refresh"
      // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
      tabIndex={0}
      onClick={askAgain}
    >
      <RefreshCw className="node-icon" aria-hidden="true" />
      Refresh
    </button>
  ) : null;

  if (said === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        {keeps ? "Asking the forge…" : from === null ? "Reading the plane…" : `Asking ${from}…`}
      </p>
    );
  }
  if ("refused" in said) {
    // The core's sentence, which names what refused and says what to do. A view that came up
    // empty would read as a plane with nothing in it.
    return (
      <>
        {refresh}
        <p className="trouble" role="alert">
          {said.refused}
        </p>
      </>
    );
  }
  const answer = said.answer;
  /** Run `action` on `row`, and say the refusal, or draw what it answered. */
  const act = async (
    row: PanelRow,
    action: RowAction,
    confirmed: boolean,
  ): Promise<string | undefined> => {
    if (from === null) return "purlis's own views offer no extension's actions.";
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
  const lists: readonly ChangeList[] = (keeps ? answer.changes : undefined) ?? [];
  /** Land on a member row: its gates first, and a refusal beside the row; else the question. */
  const land = (row: PanelRow) => {
    const member = memberAt(lists, row.key);
    if (member === undefined || workspace === undefined || checking !== undefined) return;
    setChecking(row.key);
    void askLand(plane, workspace, member.change, member.repo).then((said) => {
      setChecking(undefined);
      setRefusedAt((was) => {
        const now = new Map(was);
        if ("refused" in said) now.set(row.key, said.refused);
        else now.delete(row.key);
        return now;
      });
      if ("ok" in said) setLanding({ change: member.change, question: said.ok });
    });
  };
  // **Push and Land are the changes view's**, switched on by its kind; which list and which
  // row each acts on is what the core sent beside the blocks (`change::view::ChangeAt`).
  const changeBlocks =
    keeps && workspace !== undefined ? withLand(answer.blocks, lists, checking) : answer.blocks;
  return (
    <>
      {refresh}
      {seen !== undefined && seen !== null && (
        /* What the core saw change outside the extension's declared paths, or an action's
           refusal: the core's sentence, naming the extension, above what it answered. */
        <p className="trouble" role="alert">
          {seen}
        </p>
      )}
      <AnsweredBlocks
        blocks={refreshed ?? changeBlocks}
        label={title}
        offerFor={from === null ? offerFor : undefined}
        onPress={from === null ? onPress : undefined}
        listHead={
          keeps && workspace !== undefined
            ? (at) => {
                const list = lists.find((one) => one.at === at);
                const change =
                  list !== undefined && list.members.length > 0 ? list.change : undefined;
                return change === undefined ? null : (
                  <button
                    type="button"
                    className="panel-view change-push"
                    tabIndex={0}
                    onClick={() => setPushing(change)}
                  >
                    Push {change}…
                  </button>
                );
              }
            : undefined
        }
        besideRow={(row) => refusedAt.get(row.key)}
        onAct={
          from === null
            ? keeps
              ? (row) => land(row)
              : undefined
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
      {from === null && id === "persona" && (
        /* This persona's own dispatch limits (#1440): kept in the project's file, never its
           own, and edited here as on Settings › Project › Dispatch. */
        <section className="view-dispatch" aria-labelledby={dispatchHeading}>
          <h3 id={dispatchHeading}>Dispatch limits</h3>
          <DispatchLimitsTable plane={plane} scope={{ kind: "persona", name: key }} />
        </section>
      )}
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
      {pushing !== undefined && workspace !== undefined && (
        <PushAsk
          plane={plane}
          workspace={workspace}
          change={pushing}
          onClose={(ran) => {
            setPushing(undefined);
            if (ran) askAgain();
          }}
        />
      )}
      {landing !== undefined && workspace !== undefined && (
        <LandAsk
          plane={plane}
          workspace={workspace}
          change={landing.change}
          question={landing.question}
          onClose={(ran) => {
            setLanding(undefined);
            if (ran) askAgain();
          }}
        />
      )}
      <p className="view-took">
        {from === null ? `read in ${answer.took_ms} ms` : `answered in ${answer.took_ms} ms`}
      </p>
    </>
  );
}

/** The Land a changes view offers on each member row (#474): charter's own button, put on the
 *  row here in the window and never by an answer, and pressed through `ChangeActions`. */
function withLand(
  blocks: readonly PanelBlock[],
  lists: readonly ChangeList[],
  checking: string | undefined,
): PanelBlock[] {
  return blocks.map((block, at) => {
    const list = lists.find((one) => one.at === at);
    if (block.kind !== "list" || list === undefined) return block;
    return {
      ...block,
      rows: block.rows.map((row) =>
        list.members.some((member) => member.key === row.key)
          ? {
              ...row,
              actions: [
                {
                  id: "land",
                  title: checking === row.key ? "Checking…" : "Land…",
                  asks_first: true,
                  deletes: false,
                },
              ],
            }
          : row,
      ),
    };
  });
}

/** The change and repo of the member row keyed `key`, as the core sent them. */
function memberAt(
  lists: readonly ChangeList[],
  key: string,
): { change: string; repo: string } | undefined {
  for (const list of lists) {
    const member = list.members.find((one) => one.key === key);
    if (member !== undefined) return { change: list.change, repo: member.repo };
  }
  return undefined;
}

/** An answer's blocks, each drawn with what a panel's is drawn with. */
function AnsweredBlocks({
  blocks,
  label,
  onAct,
  offerFor,
  onPress,
  listHead,
  besideRow,
}: {
  blocks: readonly PanelBlock[];
  label: string;
  /** What a list is headed by, from its rows: a change's Push (#474). */
  listHead?: (at: number) => ReactNode;
  /** A refusal to say beside a row, in the core's words: a Land's (#474). */
  besideRow?: (row: PanelRow) => string | undefined;
  /** Run one of the extension's actions a row offers; `undefined` for charter's own views. */
  onAct?: (row: PanelRow, action: RowAction) => void;
  /** The catalogue, for charter's own views only: an extension's rows run no charter verb. */
  offerFor?: (id: string) => Offer | undefined;
  onPress?: (offer: Offer) => void;
}) {
  const [open, setOpen] = useState<string>();
  // **A memory row's own rows** (SI-9b): Open, Edit and Delete for each memory this view lists
  // (`actions.listedMemoryOffers`, which a workspace's Memory section uses too).
  const memories = useMemo(() => listedMemoryOffers(blocks), [blocks]);
  const lookUp = (id: string) => memories.get(id) ?? offerFor?.(id);
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
          <Fragment key={at}>
            {listHead?.(at)}
            <PanelList
              rows={block.rows}
              empty={block.empty}
              label={label}
              open={open}
              onOpen={setOpen}
              onAct={onAct}
              onRun={
                onPress === undefined
                  ? undefined
                  : (runs, kept) => {
                      // The catalogue's row or nothing, as a panel's rows are (`Panels.tsx`); a
                      // double-click keeps what a single click previews (`actions.toKeep`).
                      const offer = lookUp(runs);
                      if (offer) onPress(kept ? toKeep(offer) : offer);
                    }
              }
              wrap={
                onPress === undefined && besideRow === undefined
                  ? undefined
                  : (row, item) => {
                      const refused = besideRow?.(row);
                      if (refused !== undefined) {
                        return (
                          <Fragment key={row.key}>
                            {item}
                            <li className="row-refusal trouble" role="alert">
                              {refused}
                            </li>
                          </Fragment>
                        );
                      }
                      const memory = onPress === undefined ? undefined : memoryKeyRun(row.runs);
                      return memory === undefined || onPress === undefined ? (
                        item
                      ) : (
                        /* Open, Edit | Delete (ADR 0065 Q12): this memory's rows. */
                        <Menued
                          key={row.key}
                          on={{ on: "memory", key: memory }}
                          offers={memories}
                          onPress={onPress}
                        >
                          {item}
                        </Menued>
                      );
                    }
              }
              // A tab has the room a side region does not, so a page is twenty rather than twelve.
              page={20}
            />
          </Fragment>
        ),
      )}
    </>
  );
}

/** A facts block, as the wire carries it. */
export type FactsBlock = Extract<PanelBlock, { kind: "facts" }>;

/**
 * **Labelled facts, as a description list** — `purlis_core::panel::Block::Facts`: what a
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
 * **A chart, drawn by charter from numbers and words** — `purlis_core::panel::Chart`.
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
