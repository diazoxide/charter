import { useMemo, useState, type ReactNode } from "react";
import * as Popover from "@radix-ui/react-popover";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import clsx from "clsx";
import {
  CircleDashed,
  Circle,
  FileText,
  FolderGit2,
  GitBranch,
  KeyRound,
  Search,
  Star,
  TriangleAlert,
  UserRound,
} from "lucide-react";
import type { PanelEmpty, PanelRow, RowAction } from "./bindings";
import { EmptyState } from "./EmptyState";
import { useTabStop } from "./roving";

/**
 * The list a panel is drawn from, and the one the operator asked for by name.
 *
 * His four sentences about the todos panel, each of which is a property below:
 *
 * > *"rows are too long, they are wrapping in 3 lines — should be shortened automatically"*
 * > *"on clicking we should show full body — like persona description"*
 * > *"max 10 or 20 todos should be loaded, with scroll inside panel and load more"*
 * > *"search input should be visible when count is bigger than N"*
 *
 * …and the sentence that made it a primitive rather than four fixes to one panel: *"same
 * load-more and search for personas will be better to have"*.
 *
 * # It knows what a row is, and nothing else
 *
 * A [`PanelRow`] is `charter_core::panel::Row` on the wire. This component takes rows and never
 * asks where they came from, which is what makes it the vocabulary a panel is drawn *with*
 * rather than advice a panel is written *to*. Its consumers on the day it was written:
 *
 * - **charter's todos panel** and **charter's personas panel**, which are contributions now
 *   (`Panels.tsx`);
 * - **a persona's memories**, in the persona's own tab — a list in a view rather than in a
 *   panel, searched and paged by this same code, answered by `open_view` like any view;
 * - **a contributed panel**, whose rows an extension declared in its manifest and which gets
 *   every one of the four properties for free.
 *
 * That last one is the test of whether the contract was worth defining, and the third is the
 * test of whether this is a primitive or a panel with a general-sounding name.
 *
 * # Shortening is done twice, and the two do different work
 *
 * **CSS does the pixel-exact fit** — one line, `text-overflow: ellipsis` (`App.css`). It is the
 * only thing that can, because the width is a region the operator drags and a font the theme
 * picks, and neither is a number this component has.
 *
 * **[`SHORTEST`] does the DOM.** A 400-character title inside an ellipsis is drawn correctly and
 * is still 400 characters in the accessible name, in the tree and in every `getByText` — so a
 * row is cut to a generous line's worth before it is rendered, and the whole of it lives in the
 * card the row opens. That cut is what a jsdom test can see: `docs/ui-primitives.md` records
 * that jsdom lays nothing out and every element measures zero, so a claim about wrapping can
 * only be a claim about characters here, and a claim about pixels is the scenario run's.
 */
export const SHORTEST = 64;

/** How many rows are drawn before the list stops and offers the rest. */
export const PAGE = 12;

/**
 * Every mark in `charter_core::panel::Mark`, as the glyph charter already ships for it.
 *
 * **A word out of a closed set, resolved here** — ADR 0041's third crossing is *a reference to
 * a file*, and an icon is the shape that invites one. A word this map does not know draws the
 * plain circle rather than nothing, which is `PlaneView`'s `MARKS` rule: a missing icon is
 * cosmetic and a missing row is not.
 */
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
 * A context menu around a row, supplied by the panel drawing it.
 *
 * **A panel's and not this component's, because a menu names what the row is ABOUT.**
 * `Menus.tsx` takes `{ on: "persona", persona }`, and there is no way to derive that from a row:
 * a row is words, a key and at most a catalogue id. charter's personas panel knows its rows are
 * personas and says so; a contributed panel cannot, and gets no context menu — which is the same
 * asymmetry `charter_core::panel`'s header names, one notch smaller, and is written down here
 * rather than in a comment on the day somebody notices.
 */
export type RowMenu = (row: PanelRow, item: ReactNode) => ReactNode;

export function PanelList({
  rows,
  empty,
  label,
  open,
  onOpen,
  detailOf,
  onRun,
  onAct,
  wrap,
  page = PAGE,
  testid,
}: {
  rows: readonly PanelRow[];
  /** What to say when there are none — the panel's own sentence, not a shared default. */
  empty: PanelEmpty;
  /** What this list is, for the search box and for a screen reader. */
  label: string;
  /** The key of the row whose card is open, held by whoever owns that state. */
  open: string | undefined;
  onOpen: (key: string | undefined) => void;
  /** What goes in an open row's card. The default is the row's own `detail`; a panel whose
   *  rows open something richer — the persona card — supplies it. */
  detailOf?: (row: PanelRow) => ReactNode;
  /** Run the catalogue row this row names, if the catalogue still offers it. */
  onRun?: (id: string) => void;
  /** Run one of the extension's own actions this row offers (charter-app#341). A list with no
   *  handler draws no action buttons: a button that does nothing is not drawn. */
  onAct?: (row: PanelRow, action: RowAction) => void;
  /** A context menu around each row, where the panel can name what the row is about. */
  wrap?: RowMenu;
  page?: number;
  testid?: string;
}) {
  const [query, setQuery] = useState("");
  const [limit, setLimit] = useState(page);

  /**
   * **The search is over what the row SAYS**, its words and its note, and not over the whole of
   * its card. A reader typing into a box beside a list is looking for a row they can see; a
   * match that lands on a row whose visible text does not hold the word reads as a bug, and
   * there is nowhere on the row to show them why it matched.
   */
  const matching = useMemo(() => {
    const wanted = query.trim().toLowerCase();
    if (wanted === "") return rows;
    return rows.filter((row) => `${row.text} ${row.note ?? ""}`.toLowerCase().includes(wanted));
  }, [query, rows]);

  const drawn = matching.slice(0, limit);
  const more = matching.length - drawn.length;
  // **Once there is more than a page**, which is the operator's own rule. Against the whole
  // list and never the filtered one: a box that vanished when a search narrowed the list to
  // eight rows would take away the control being used.
  const searchable = rows.length > page;
  // The rows are ONE Tab stop, and Up and Down move along them (charter-app#189, `roving.ts`):
  // the row whose card is open, or the first that does anything. A read-only row is a `<span>`
  // and is no stop at all.
  const stop = useTabStop(
    open,
    drawn.filter((row) => row.detail !== null || row.runs !== null).map((row) => row.key),
  );

  if (rows.length === 0) {
    return (
      <EmptyState
        size="panel"
        headline={empty.headline}
        body={empty.body ?? undefined}
        testid={testid === undefined ? undefined : `${testid}-empty`}
      />
    );
  }

  return (
    <div className="panel-list" data-testid={testid}>
      {searchable && (
        <div className="panel-search">
          <Search className="node-icon" />
          <input
            type="search"
            value={query}
            aria-label={`Search ${label}`}
            placeholder={`Search ${rows.length}`}
            onChange={(e) => {
              setQuery(e.target.value);
              // Back to one page: a search that kept a grown limit would show forty matches
              // for one word and then twelve for the next, which reads as rows going missing.
              setLimit(page);
            }}
          />
        </div>
      )}

      {matching.length === 0 ? (
        // Not the empty state: the list is not empty, the search found nothing in it, and
        // those are different facts about a panel whose rows the reader can still get back.
        <p className="none">Nothing here matches “{query.trim()}”.</p>
      ) : (
        <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
          <ul className="panel-rows" aria-label={label}>
            {drawn.map((row) => (
              <Row
                key={row.key}
                row={row}
                open={open === row.key}
                onOpen={(opening) => onOpen(opening ? row.key : undefined)}
                detail={detailOf?.(row) ?? defaultDetail(row)}
                onRun={onRun}
                onAct={onAct}
                wrap={wrap}
              />
            ))}
          </ul>
        </RovingFocusGroup.Root>
      )}

      {more > 0 && (
        <button
          type="button"
          className="panel-more"
          // #190: WebKit leaves a button out of the tab sequence without this.
          tabIndex={0}
          onClick={() => setLimit((was) => was + page)}
        >
          {`Show ${Math.min(more, page)} more`}
          <span className="note">{` · ${more} left`}</span>
        </button>
      )}
    </div>
  );
}

/** The card a row opens when the panel does not supply a richer one: the row's own words. */
function defaultDetail(row: PanelRow): ReactNode {
  if (row.detail === null) return null;
  return <p className="row-detail">{row.detail.text}</p>;
}

/**
 * One row: a mark, one line of words, a quiet note — and a card, when there is one to open.
 *
 * **A button exactly when it does something**, which is what keeps a list of read-only rows out
 * of the tab sequence. A row with neither a card nor a catalogue row is a `<span>`, and a
 * screen reader is not told it can be pressed.
 */
function Row({
  row,
  open,
  onOpen,
  detail,
  onRun,
  onAct,
  wrap,
}: {
  row: PanelRow;
  open: boolean;
  onOpen: (opening: boolean) => void;
  detail: ReactNode;
  onRun?: (id: string) => void;
  onAct?: (row: PanelRow, action: RowAction) => void;
  wrap?: RowMenu;
}) {
  const Mark = MARKS[row.mark] ?? Circle;
  const shortened = shorten(row.text);
  const body = (
    <>
      <Mark className="node-icon" />
      <span className="row-text" title={row.text === shortened ? undefined : row.text}>
        {shortened}
      </span>
      {row.note !== null && <span className="row-note">{` · ${row.note}`}</span>}
      {row.tone === "default" && <Star className="node-icon" />}
    </>
  );

  // **The catalogue row runs on the way open and never on the way shut.** A persona row's
  // `persona.show:<name>` is the same verb the palette and a context menu run (charter-app#174),
  // so the three are one state rather than three that look alike — and running it again on
  // dismissal would re-open what was just closed.
  const opened = (opening: boolean) => {
    onOpen(opening);
    if (opening && row.runs !== null) onRun?.(row.runs);
  };

  const inner =
    detail === null && row.runs === null ? (
      <span className="row">{body}</span>
    ) : detail === null && row.runs !== null ? (
      /* **A row that does something and opens nothing is a plain button that does it.** A
         persona's row opens that persona's tab (`persona.show:<name>`, the operator's ruling of
         2026-09-23): a card beside the row as well would be two surfaces for one persona. */
      <RovingFocusGroup.Item asChild tabStopId={row.key}>
        <button
          type="button"
          className="row"
          onClick={() => {
            if (row.runs !== null) onRun?.(row.runs);
          }}
        >
          {body}
        </button>
      </RovingFocusGroup.Item>
    ) : (
      <Popover.Root open={open} onOpenChange={opened}>
        <Popover.Trigger asChild>
          <RovingFocusGroup.Item asChild tabStopId={row.key} active={open}>
            <button type="button" className="row">
              {body}
            </button>
          </RovingFocusGroup.Item>
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content
            className="row-card"
            data-testid={`row-detail-${row.key}`}
            /* `side="left"` is where it opens from in the default arrangement and no more than
               that: a region MOVES (ADR 0038), and Radix flips to the other side when
               there is no room, which is what makes naming a side safe. */
            side="left"
            align="start"
            sideOffset={6}
            collisionPadding={8}
            aria-label={row.text}
          >
            {detail}
            <Popover.Arrow className="row-card-arrow" />
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>
    );

  // **The extension's own actions, beside the row and never inside its button** (charter-app#341):
  // pressing the row opens its card, and pressing an action asks the extension's program — two
  // different things a click could mean, so they are two different controls. The titles and the
  // asking are the manifest's, which the core put on the row.
  const acts =
    onAct !== undefined && row.actions.length > 0 ? (
      <span className="row-actions">
        {row.actions.map((action) => (
          <button
            key={action.id}
            type="button"
            className="row-action"
            // #190: WebKit leaves a button out of the tab sequence without this.
            tabIndex={0}
            onClick={() => onAct(row, action)}
          >
            {action.title}
          </button>
        ))}
      </span>
    ) : null;

  const item = (
    <li className={clsx("panel-row", row.tone !== "plain" && `is-${row.tone}`)}>
      {inner}
      {acts}
    </li>
  );
  return <>{wrap ? wrap(row, item) : item}</>;
}

/**
 * One line's worth of a row's words, with the rest left to the card.
 *
 * Cut on a word boundary where there is one near the end, because a cut mid-word reads as a
 * truncated *word* rather than a truncated line — and the ellipsis is then doing two jobs.
 */
export function shorten(text: string, most = SHORTEST): string {
  if (text.length <= most) return text;
  const cut = text.slice(0, most);
  const space = cut.lastIndexOf(" ");
  return `${(space > most * 0.6 ? cut.slice(0, space) : cut).trimEnd()}…`;
}
