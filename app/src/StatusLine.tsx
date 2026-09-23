import { Bell } from "lucide-react";
import type { WorkspaceState } from "./workspaceState";
import { Health, type DoctorState } from "./Doctor";
import { RegionToggle } from "./RegionFrame";
import type { Placement, RegionId } from "./regions";
import { PinItem } from "./Updates";
import type { PinReport } from "./bindings";

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
 * - **A region resizes and can be put away; this does neither.** The toggles are drawn from the
 *   arrangement (`SIDES.flatMap`), so a region arrives with a button that hides it — which is
 *   the right rule for a region and the wrong one for the line that says where you are and
 *   which project you are in. A status line an operator can lose is one they will lose and then
 *   report as a bug. That those toggles are now drawn *on* this line does not weaken the
 *   argument, it sharpens it: the line carries every region's way back and has none of its own,
 *   which is what makes it the frame rather than a tenant. `FourRegions.test.tsx`'s *"cannot be
 *   put away, because it is not a region"* is the guard.
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
 * conversation's number under fifty. `doctor` is one button here (`Doctor.tsx` argues why the
 * frame and not a region) and the plane's pin is another (`Updates.tsx`). Repos and
 * CI are the bottom region's, and a status line that drew them would be a second bottom bar
 * one line below the first.
 *
 * **And the update offer is NOT here any more.** It was, beside the pin, and the two read as
 * one pair of "version facts" — but only one of them is about a project. The pin is
 * `charter version`'s verdict on THIS plane's `[charter] version` (charter ADR 0030), so two
 * open projects can honestly disagree about it and it belongs on the line that names the
 * project. An update offer is about the app: the same offer whichever project is in front, and
 * the line is drawn once per project, so eight projects meant eight `useUpdates` clients
 * listening for one event. It is on the title bar now — the window's own chrome, drawn once
 * (`TitleBar.tsx`). Moved rather than copied: two surfaces stating one fact is the shape
 * charter refuses, and the duplication this line already carries is named above and argued for.
 */
export function StatusLine({
  plane,
  read,
  where,
  workspaces,
  state,
  alerts,
  doctor,
  pin,
  regions,
}: {
  /** The project's root directory — the path that used to sit in the top-right corner. */
  plane: string;
  /**
   * Whether the plane has been read at all.
   *
   * **Separate from {@link where}, because "not yet" and "nowhere" are different claims** and
   * the workspace's name cannot carry both. A plane that holds no workspaces answers
   * perfectly well and leaves the window on none of them; a line that said *"reading the
   * plane…"* under it would be charter waiting for something that has already happened.
   */
  read: boolean;
  /** The workspace the window is on, as it should be read. `undefined` when it is on none. */
  where: string | undefined;
  /** How many workspaces this project has, for `ws N`. `undefined` until the plane has been
   *  read. Zero IS drawn: a plane with no workspaces is a fact, not an absence of one. */
  workspaces: number | undefined;
  /** What the core has said about the focused workspace. The counts are read off it and
   *  nothing extra is asked for: `useWorkspaceState` already makes these calls once for the
   *  three regions, and a status line that asked again would be `git status` per clone a
   *  second time. */
  state: WorkspaceState;
  /**
   * The window's alerts drawer: how many alerts every open project has, and the way to open it
   * (`AlertsDrawer.tsx`, `alerts.ts`).
   *
   * **The count is the window's, not this project's.** Alerts are about planes, and the drawer
   * lists every project the window holds, so the number on the button is that whole list's —
   * the same number whichever project is in front.
   *
   * `undefined` means no drawer was wired, which the window always does; a status line drawn
   * on its own says so rather than offering a button that answers a press with nothing.
   */
  alerts?: Alerts;
  /** What the doctor last said about this project, run inside the app (`Doctor.tsx`). Absent
   *  draws no button — a caller that has no doctor to offer offers none. */
  doctor?: DoctorState;
  /** What `charter version` says about this plane's pin, and a way to ask again. The item
   *  is drawn only when it drifts. */
  pin?: { pin?: PinReport; again: () => void };
  /**
   * Which regions the window is drawing, and the way to change that (charter ADR 0038).
   *
   * **The arrangement, already in the order the window draws it** — `PlaneView` flattens
   * `inSlots` and hands the result over. Nothing here sorts, filters or names a region: a
   * status line that decided which regions exist would be the second place that decides, and
   * the reason there is a catalogue at all is that there is one.
   *
   * Absent draws no toggles, which is a status line rendered on its own.
   */
  regions?: {
    /** One region each, in the order the window draws them. */
    placed: readonly Placement[];
    onToggle: (id: RegionId) => void;
  };
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
        {where !== undefined ? (
          <span className="status-workspace">{where}</span>
        ) : read ? (
          // The plane answered and the window is on no workspace — a plane that holds none,
          // or one whose workspaces all went away. Said, because it is the answer.
          <span className="none">no workspace</span>
        ) : (
          <span className="pending">reading the plane…</span>
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

      {doctor && <Health doctor={doctor} />}

      {pin && <PinItem pin={pin.pin} again={pin.again} />}

      {/* The project directory. `code`, because it is a path and the operator copies it out of
          here; the whole path rather than the directory's name, because two projects can share
          a name and this is the one place in the window that says which one is open. */}
      <span className="plane" title={plane}>
        <span className="status-label">project</span> <code>{plane}</code>
      </span>

      {/* **Which regions are drawn** (charter ADR 0038), at the right-hand end of the line —
          the operator's *"show hide buttons can be movet to bottom status bar — again like
          ZED"*, which is exactly where Zed keeps them. They were labelled buttons on
          `header.bar`, in a row with the tab strip, the `+` and the show-more, competing for
          the width that the strip needs most when it is fullest.

          **One button per region in the arrangement, in the order the window draws them.**
          That rule came with them unchanged and is the half worth protecting: a region added
          to the catalogue gets its own way back without anybody remembering to add one, which
          is what a list written out by hand kept getting wrong.

          **Last on the line, after the path.** The row's order is its truncation order, and
          the path is the one item allowed to shrink — so these sit at the window's own
          right-hand corner, where they do not move when the path does. */}
      {regions && (
        <span className="regions-doing">
          {regions.placed.map((placed) => (
            <RegionToggle
              key={placed.id}
              id={placed.id}
              shown={!placed.collapsed}
              onToggle={regions.onToggle}
            />
          ))}
        </span>
      )}
    </footer>
  );
}

/** What the window's drawer hands the button. */
export type Alerts = {
  /**
   * How many alerts every open project has — or `undefined` when charter cannot stand behind
   * a number: not every project has answered yet, or charter stopped looking in one of them
   * (`alerts.ts`'s `countOf`). **Dropped, never zero**: a partial total is a wrong total, and
   * once it is a number on a line nobody can tell the two apart.
   */
  count: number | undefined;
  /** Opens the drawer. */
  open: () => void;
};

/**
 * The alerts button, in the states it can be in.
 *
 * - **A count** — drawn in the badge, because something needs the operator.
 * - **Zero** — the word `Alerts` and no badge. The footer's rule: a `0` sitting there every
 *   day is furniture by the end of the week, and then a real `2` in that spot draws no more
 *   attention than the zero did. Presence is the signal. The button's NAME still says `none`,
 *   because a screen reader has no "absent badge" to notice.
 * - **No number charter can stand behind** — a dash, and the drawer still opens: it says,
 *   project by project, what charter could not read. A button that refused to open then would
 *   hide the one explanation there is.
 * - **No drawer wired** — disabled and said, rather than a control that answers a press with
 *   nothing. The window always wires one; this is a status line drawn on its own.
 */
function AlertsButton({ alerts }: { alerts?: Alerts }) {
  if (alerts === undefined) {
    return (
      <button
        type="button"
        className="status-alerts"
        data-testid="status-alerts"
        disabled
        aria-label="Alerts — nothing to open here"
        title="This status line has no alerts drawer behind it."
      >
        <Bell aria-hidden="true" size="1em" /> Alerts{" "}
        <span className="status-unknown" aria-hidden="true">
          —
        </span>
      </button>
    );
  }
  const { count } = alerts;
  return (
    <button
      type="button"
      className="status-alerts"
      data-testid="status-alerts"
      data-count={count ?? "unknown"}
      aria-label={
        count === undefined ? "Alerts: not counted" : `Alerts: ${count === 0 ? "none" : count}`
      }
      title={
        count === undefined
          ? "charter could not count every open project's alerts — open the drawer to see why"
          : "Open the alerts drawer: every open project's alerts"
      }
      onClick={alerts.open}
    >
      <Bell aria-hidden="true" size="1em" /> Alerts
      {count === undefined ? (
        <span className="status-unknown" aria-hidden="true">
          —
        </span>
      ) : (
        count > 0 && (
          <span className="status-count" aria-hidden="true">
            {count}
          </span>
        )
      )}
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
