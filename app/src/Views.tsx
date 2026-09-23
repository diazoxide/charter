import { useEffect, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { PanelList } from "./PanelList";
import {
  commands,
  type ExtensionView,
  type PanelBlock,
  type PanelPoint,
  type PlaneId,
  type ViewAnswer,
} from "./bindings";

/**
 * **What an extension's program answers, drawn by charter** — charter ADR 0041 stage 2, the
 * window's half.
 *
 * A view is a surface the operator opens; opening it asks the extension's program one question
 * (`open_view`, which re-takes the approval gate in the core at that moment) and this draws the
 * answer. The answer is the panel vocabulary — notes, lists and charts — and nothing else, so
 * everything here is built from what a contributed panel is already built from: the list is
 * `PanelList`, a note is a paragraph, and a chart is {@link Chart}.
 *
 * **Asked once per opening, and never on a timer.** ADR 0041's minimum capability is *one round
 * trip per deliberate human action*; a view that polled would be a program running on nobody's
 * say-so. Closing the surface and opening it again is how the operator asks again.
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
 * The region a sheet is drawn over: the centre, where the terminals are, in the same window
 * arrangement as the element asking.
 *
 * **Not the whole window, and that is the argument for the sheet** (`Panels.tsx`'s persona card
 * has it in full): the needs-you queue is in a side region, and a surface drawn over the centre
 * leaves it on screen, reachable, and not `aria-hidden`. `null` when there is no arrangement to
 * find — a jsdom test rendering one region alone — and Radix then portals to the body.
 */
export function centreOf(near: Element | null): HTMLElement | null {
  return near?.closest(".regions")?.querySelector<HTMLElement>(".region-centre") ?? null;
}

/**
 * One view, opened: asks its program now, and draws the answer or the refusal.
 *
 * `focus` is the persona whose card it was opened from, when it was.
 */
export function OpenedView({
  plane,
  view,
  focus,
}: {
  plane: PlaneId;
  view: ExtensionView;
  focus: string | null;
}) {
  const [said, setSaid] = useState<ViewAnswer>();
  const [refused, setRefused] = useState<string>();

  useEffect(() => {
    let gone = false;
    void commands
      .openView(plane, view.extension, view.id, focus)
      .then((answered) => {
        if (gone) return;
        if (answered.status === "error") setRefused(answered.error);
        else setSaid(answered.data);
      })
      .catch((err: unknown) => {
        if (!gone) setRefused(String(err));
      });
    return () => {
      gone = true;
    };
  }, [plane, view.extension, view.id, focus]);

  return (
    <section
      className="opened-view"
      data-testid={`view-${view.extension}-${view.id}`}
      aria-busy={said === undefined && refused === undefined}
    >
      <h4>
        {view.title}
        {/* Whose program this is — ADR 0041 item 5, after approval and not only at it. */}
        <span className="panel-from">{` · ${view.extension}`}</span>
      </h4>
      {refused !== undefined ? (
        // The core's sentence, which names the extension and says what to do. A view that
        // came up empty would read as a plane with nothing in it.
        <p className="trouble" role="alert">
          {refused}
        </p>
      ) : said === undefined ? (
        <p className="pending">
          <LoaderCircle className="node-icon spinning" />
          {`Asking ${view.extension}…`}
        </p>
      ) : (
        <>
          <AnsweredBlocks blocks={said.blocks} label={view.title} />
          <p className="view-took">{`answered in ${said.took_ms} ms`}</p>
        </>
      )}
    </section>
  );
}

/** An answer's blocks, each drawn with what a panel's is drawn with. */
function AnsweredBlocks({ blocks, label }: { blocks: readonly PanelBlock[]; label: string }) {
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
        ) : (
          <PanelList
            key={at}
            rows={block.rows}
            empty={block.empty}
            label={label}
            open={open}
            onOpen={setOpen}
          />
        ),
      )}
    </>
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
