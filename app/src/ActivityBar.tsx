import type { ReactNode } from "react";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { FolderTree, GitBranch, MessagesSquare, Search, type LucideIcon } from "lucide-react";
import { useTabStop } from "./roving";
import { VIEWS, type Side, type ViewId } from "./regions";

/**
 * **A side's activity bar: a strip of icons, one per view, that switches the side between them**
 * (ADR 0038 as amended 2026-10-10, B-1, #1673).
 *
 * VS Code's, in the window's own primitives. **It is a tab list, vertical**: the tabs are the
 * views, the selected tab is the open view, and the arrows, Home and End move along it with one
 * Tab stop (`roving.ts`, the strips' own `RovingFocusGroup`). Enter or Space presses the tab the
 * keyboard is on, and so does a click: activation is manual, as an editor's is, so walking the
 * bar with the arrows opens nothing. **The tab list owns tabs and nothing else** (#1204): each
 * tab's badge is inside its own tab.
 *
 * **What a press does is the arrangement's** (`regions.picked`): a press of the open view puts
 * the side away, any other opens. This draws and says; it decides nothing.
 *
 * **Icons, and the words are still there** (`docs/design-system.md` §Icons): each tab is its
 * view's mark, the view's name is its `aria-label`, and the tooltip says what pressing does,
 * with the key. The mark is what the view IS, never which edge it is on, so it is right on the
 * other side too.
 *
 * **Generic over the side.** It knows nothing of the left: the right side's bar (#1678) is this
 * component with the attention region's views, drawn by `RegionFrame` from the same data.
 */
export function ActivityBar({
  side,
  name,
  views,
  open,
  keys,
  badges,
  panelOf,
  tabOf,
  onPick,
}: {
  /** Which edge of the window it is drawn on. */
  side: Side;
  /** The region's name, which is the tab list's. */
  name: string;
  /** The views, in the bar's order. */
  views: readonly ViewId[];
  /** The open view, or nothing while the side is put away. */
  open: ViewId | undefined;
  /** How each view's key is spelled on this platform, for the tooltip. */
  keys?: Partial<Record<ViewId, string>>;
  /** A count or a mark beside a view's icon, drawn whether or not the side is out. */
  badges?: Partial<Record<ViewId, ReactNode>>;
  /** The id of each view's tab panel, which its tab controls. */
  panelOf: (view: ViewId) => string;
  /** The id of each view's tab, which names its panel. */
  tabOf: (view: ViewId) => string;
  onPick: (view: ViewId) => void;
}) {
  const stop = useTabStop(open, views);
  return (
    <div className="activity-bar" data-side={side}>
      <RovingFocusGroup.Root asChild orientation="vertical" loop={false} {...stop}>
        <div role="tablist" aria-orientation="vertical" aria-label={name}>
          {views.map((view) => {
            const Mark = VIEW_MARKS[view];
            const called = VIEWS[view].name;
            const selected = view === open;
            const key = keys?.[view];
            const badge = badges?.[view];
            const described = `${tabOf(view)}-badge`;
            return (
              <RovingFocusGroup.Item key={view} asChild tabStopId={view} active={selected}>
                <button
                  type="button"
                  role="tab"
                  id={tabOf(view)}
                  className="activity-tab"
                  aria-selected={selected}
                  aria-controls={panelOf(view)}
                  aria-label={called}
                  aria-describedby={badge === undefined ? undefined : described}
                  title={
                    (selected ? `Put the ${name} region away` : `Show the ${called} view`) +
                    (key === undefined ? "" : ` (${key})`)
                  }
                  onClick={() => onPick(view)}
                >
                  <Mark aria-hidden="true" />
                  {badge !== undefined && (
                    <span id={described} className="activity-badge">
                      {badge}
                    </span>
                  )}
                </button>
              </RovingFocusGroup.Item>
            );
          })}
        </div>
      </RovingFocusGroup.Root>
    </div>
  );
}

/**
 * **A count on a view's tab** (B-8): the number drawn, and the sentence a screen reader is told
 * as the tab's description, since the tab's name is the view's. The needs-you tone is the one
 * the project and workspace tabs' counts are drawn in, and it is only ever the count's, never a
 * word's (`docs/design-system.md` §Icons). Nothing is drawn for nothing: a zero is not a badge.
 */
export function ActivityCount({
  count,
  said,
  tone,
}: {
  count: number;
  said: string;
  tone?: "needs-you";
}) {
  if (count <= 0) return null;
  return (
    <span className="activity-count" data-tone={tone}>
      <span aria-hidden="true">{count}</span>
      <span className="activity-said">{said}</span>
    </span>
  );
}

/**
 * Each view's mark, by what it IS. A `Record` over `ViewId`, so a view added without a mark is
 * a type error rather than a tab with a hole in it.
 */
export const VIEW_MARKS: Record<ViewId, LucideIcon> = {
  chats: MessagesSquare,
  explorer: FolderTree,
  // #1676: a magnifier for finding in files, and a branch for what git says of the repos.
  search: Search,
  changes: GitBranch,
};
