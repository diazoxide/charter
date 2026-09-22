import { Bell } from "lucide-react";
import type { WorkspaceState } from "./workspaceState";

/**
 * **charter's status line**: one line at the very bottom of the window, under everything.
 *
 * The operator asked for it in as many words — *"project directory in right corner — lets move
 * to bottom, at most bottom of charter new one line bar, and it will be status line - and show
 * general infos"*. The project's path used to sit at the right-hand end of `header.bar`, in a
 * row with the tab strip, the `+`, the split buttons and the region toggles; it is here now,
 * and it is the only place it is drawn.
 *
 * # It is NOT a region, and that is a decision
 *
 * charter ADR 0038's window is four regions — left, centre, right, bottom — and
 * `app/src/regions.ts` makes adding one a line in a catalogue and a line in an arrangement.
 * This is deliberately not that, and the reasons are in the shape of the thing rather than in
 * taste:
 *
 * - **Every slot in the arrangement is sized as a percentage of its group.** `SLOTS`,
 *   `CATALOGUE.size`, `slotSize` and `startingSize` are all percentages, because that is what
 *   `react-resizable-panels` takes. A status line is one line of text: its height is a font
 *   and two paddings, and the percentage that happens to equal that at 1080 px is the wrong
 *   one at 4K. There is no honest number to put in the catalogue.
 * - **A region resizes and can be put away; this does neither.** The toggles on the bar are
 *   drawn from the arrangement (`SIDES.flatMap`), so a region arrives with a button that hides
 *   it — which is the right rule for a region and the wrong one for the line that says where
 *   you are and which project you are in. A status line an operator can lose is one they will
 *   lose and then report as a bug.
 * - **It is the frame, not a tenant of it.** `nav.projects` sits above the regions and is not
 *   in the arrangement either. The window is chrome, four regions, chrome; this is the bottom
 *   half of the chrome, and `RegionFrame` is untouched by it — which is also why this change
 *   moved no JSX inside the frame and added no fourth `Panel` to a live group
 *   (charter-app#141's throw is a thing to stay away from, not a thing to test against).
 *
 * **What that costs, said rather than hidden:** the line cannot be resized, put away, or moved
 * to another side, and nothing remembers anything about it. If it is ever wanted as a region,
 * `regions.ts` is exactly where it goes and nothing here blocks that — the content is already
 * a component taking props, which is the shape a region's content has.
 *
 * # What is on the line, and the rules it is composed by
 *
 * charter's own footer is the starting point rather than an invention:
 * `crates/charter-core/src/footer.rs` draws `⬢ alpha · todo 3 · pieces 2 1 done · ws 4` as
 * zone 1, *"where I am"*, and it arrived at that over a long time. Three of its rules are
 * taken whole:
 *
 * - **A count lives next to the thing it counts.** The todos are the focused workspace's, so
 *   they sit beside its name; the pieces are that workspace's worktrees; `ws N` is how many
 *   others there are.
 * - **Zero renders NOTHING.** A `todo 0` present every turn is furniture within a day, and a
 *   real `todo 7` in that spot then draws no more attention than the zero did. Presence is the
 *   signal.
 * - **Never a number charter cannot stand behind.** A count charter could not read is dropped,
 *   never rounded to zero — a partial total is a wrong total, and the two cannot be told apart
 *   once they are both a number on a line.
 *
 * **The one duplication, named rather than hidden**, the way ADR 0038 names its own: the
 * focused workspace is on the strip above too. They are not the same statement. The strip is
 * the *selector* — at ADR 0026's ten workspaces it scrolls, and reading it means finding the
 * selected tab among the others; this *states* the answer, in one place that never moves. That
 * is the same relationship charter's footer has to `charter ws list`, and it is the whole
 * reason zone 1 exists.
 *
 * **What is left off, and why.** The `ctx`/`cache` gauges and the usage trend are per-chat and
 * have no renderer ported — ADR 0038 names both as open, and a window-wide gauge would be one
 * conversation's number under fifty. News and `doctor` have no Tauri command at all. Repos and
 * CI are the bottom region's, and a status line that drew them would be a second bottom bar
 * one line below the first.
 */
export function StatusLine({
  plane,
  where,
  workspaces,
  state,
  alerts,
}: {
  /** The project's root directory — the path that used to sit in the top-right corner. */
  plane: string;
  /** The workspace the window is on, as it should be read. `undefined` until the plane has
   *  been read, which is a different claim from "no workspace". */
  where: string | undefined;
  /** How many workspaces this project has, for `ws N`. `undefined` until the plane has been
   *  read. */
  workspaces: number | undefined;
  /** What the core has said about the focused workspace. The counts are read off it and
   *  nothing extra is asked for: `useWorkspaceState` already makes these calls once for the
   *  three regions, and a status line that asked again would be `git status` per clone a
   *  second time. */
  state: WorkspaceState;
  /**
   * The alerts, when there are any to have.
   *
   * **`undefined` is today, and it is the honest value.** charter's alert row is
   * `charter/statusline.py:_alerts` and nothing of it is ported — `footer.rs` names the
   * omission in its own output, and `Panels`'s alerts area says the same thing in prose:
   * *"an empty list here would be a claim it has no way to make."* A `0` on this button
   * would be that claim with a number on it.
   *
   * This is the seam M6.5's drawer attaches to: a count to draw and something to call when
   * the button is pressed. Nothing else about the drawer is decided here.
   */
  alerts?: Alerts;
}) {
  const todos = todoCount(state);
  const pieces = pieceCount(state);
  return (
    <footer className="status-line" aria-label="Status" data-testid="status-line">
      {/* Where I am. First, because it is what the line is for, and because the row's order
          is its truncation order: what goes off the end is the least important thing. */}
      <span className="status-where">
        <span className="status-glyph" aria-hidden="true">
          ⬢
        </span>{" "}
        {where === undefined ? (
          <span className="pending">reading the plane…</span>
        ) : (
          <span className="status-workspace">{where}</span>
        )}
      </span>

      {todos !== undefined && (
        <span className="status-cell" data-testid="status-todos">
          <span className="status-label">todo</span> {todos}
        </span>
      )}

      {pieces !== undefined && (
        <span className="status-cell" data-testid="status-pieces">
          <span className="status-label">pieces</span> {pieces}
        </span>
      )}

      {workspaces !== undefined && (
        <span className="status-cell" data-testid="status-workspaces">
          <span className="status-label">ws</span> {workspaces}
        </span>
      )}

      <AlertsButton alerts={alerts} />

      {/* The project directory. `code`, because it is a path and the operator copies it out of
          here; the whole path rather than the directory's name, because two projects can share
          a name and this is the one place in the window that says which one is open. */}
      <span className="plane" title={plane}>
        <span className="status-label">project</span> <code>{plane}</code>
      </span>
    </footer>
  );
}

/** What M6.5's drawer hands the button. See {@link StatusLine}'s `alerts` for why the absence
 *  of this is a state with words of its own rather than a zero. */
export type Alerts = {
  /** How many there are. */
  count: number;
  /** Opens the drawer. */
  open: () => void;
};

/**
 * The alerts button, in the two states it can be in.
 *
 * **With no source it is disabled and says so**, rather than being a control that opens an
 * empty drawer. A button that answers a press with nothing teaches an operator that alerts are
 * quiet; the sentence teaches them that charter cannot tell. Those are the two claims
 * `footer.rs` refuses to let a reader confuse, and the disabled state is the only one of the
 * two a person can tell apart at a glance.
 */
function AlertsButton({ alerts }: { alerts?: Alerts }) {
  if (alerts === undefined) {
    return (
      <button
        type="button"
        className="status-alerts"
        data-testid="status-alerts"
        disabled
        aria-label="Alerts — not drawn by this build"
        title={
          "charter's alert row is not ported, so charter cannot tell you whether anything is " +
          "alerting. A count here — even a zero — would be a claim it has no way to make."
        }
      >
        <Bell aria-hidden="true" size="1em" /> Alerts{" "}
        <span className="status-unknown" aria-hidden="true">
          —
        </span>
      </button>
    );
  }
  return (
    <button
      type="button"
      className="status-alerts"
      data-testid="status-alerts"
      aria-label={`Alerts: ${alerts.count}`}
      title="Open the alerts drawer"
      onClick={alerts.open}
    >
      <Bell aria-hidden="true" size="1em" /> Alerts{" "}
      <span className="status-count">{alerts.count}</span>
    </button>
  );
}

/**
 * The focused workspace's open todos, or nothing.
 *
 * Nothing at zero (the footer's rule), nothing before the plane has been read, and nothing
 * when the store refused — the right-hand region draws that refusal in full, and a count here
 * that quietly said `0` would contradict it.
 */
export function todoCount(state: WorkspaceState): number | undefined {
  const panels = state.panels;
  if (panels === undefined || panels.todos_refused !== null) return undefined;
  return panels.todos.length === 0 ? undefined : panels.todos.length;
}

/**
 * How many worktrees the focused workspace has, across every clone in it — or nothing.
 *
 * **Every clone has to have answered.** `worktree_list` runs per clone and comes back per
 * clone, so a total taken while two of five are still listing is a smaller number than the
 * truth, drawn with no mark on it saying so. Zero is dropped for the footer's reason; a
 * partial or refused total is dropped for a stronger one — it would be wrong.
 */
export function pieceCount(state: WorkspaceState): number | undefined {
  const clones = state.panels?.repos;
  if (clones === undefined) return undefined;
  let total = 0;
  for (const clone of clones) {
    const listed = state.pieces[clone];
    if (listed === undefined) return undefined;
    total += listed.length;
  }
  return total === 0 ? undefined : total;
}
