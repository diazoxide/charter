/// <reference types="vite/client" />

/**
 * **Which of the two ways to end a task a breadcrumb's line draws** (#1488): each whole or
 * not at all, and only after everything that gives way before them has.
 *
 * The order a short line gives up its things in is: the harness's name first, cut to its
 * padding (the stylesheet: it shrinks a million times faster than anything else); then the
 * breadcrumb's names, each to an ellipsis; then the ways, the last first; and the state word
 * last of all. The first two are the stylesheet's, and this is the third. It is not left to
 * shrinking and wrapping: a line is made up by everything on it at once, so the group gave
 * a sliver while the harness still had room to give, and text is a fraction of a pixel
 * wider than any constant kept beside it. Measured in CI: a group 0.03px short of "Close
 * now", which a wrapping line then never drew. So the line is measured, as the tab strip is
 * (`fits.ts`), and a way that does not fit whole is not drawn.
 *
 * A way that is not drawn is set aside (`data-away`): out of the line's flow, below the
 * group's one line where its own clip hides it, and out of the tab sequence.
 */
export function fitWays(group: HTMLElement): void {
  const line = group.parentElement;
  const ways = [...group.querySelectorAll<HTMLElement>(":scope > .task-end")];
  if (line === null) return;
  const crumbs = line.querySelector<HTMLElement>(":scope > .pane-crumbs");
  const state = crumbs?.querySelector<HTMLElement>(".shown-state") ?? null;
  // Something that must not be given up for a way is: the state word is cut, or pushed out
  // of its breadcrumb, or a way runs past the line's end.
  setAsideUntilWhole(ways, () => {
    if (state !== null && crumbs !== null) {
      const word = state.querySelector<HTMLElement>(".word") ?? state;
      if (word.scrollWidth > word.clientWidth + 1) return true;
      if (state.getBoundingClientRect().right > crumbs.getBoundingClientRect().right + 0.5)
        return true;
    }
    return runsPast(ways, line);
  });
}

/**
 * **Which counts a tab's chip draws** (#1487): every one, until its tab's name has given up
 * all it can, and then whole and from the end (finished, then failed, then waiting), so the
 * count of what is still working is the last one standing. The name gives way first by the
 * stylesheet (it shrinks to its floor, and the chip does not shrink); a count goes only where
 * the cell still runs over after that. Measured for the reason {@link fitWays} gives: a chip
 * that shrank with the name gave a sliver while the name still had room, and a count a
 * fraction of a pixel short was not drawn (CI: "done" missing with room to spare).
 */
export function fitCounts(counts: HTMLElement): void {
  const cell = counts.closest<HTMLElement>(".tab");
  const each = [...counts.querySelectorAll<HTMLElement>(":scope > .count")];
  if (cell === null) return;
  // The first, what is still working, is never set aside: a cell too narrow even for it is
  // the stylesheet's (the hand alone, under 7.5rem).
  setAsideUntilWhole(each, () => runsPast([...cell.children] as HTMLElement[], cell), 1);
}

/** Fits `counts` now, and again whenever its tab's cell changes size or what it holds. */
export function fitCountsOn(counts: HTMLElement): () => void {
  const cell = counts.closest<HTMLElement>(".tab");
  return refitOn(counts, cell, cell, fitCounts);
}

/** Draws every one of `items`, then sets them aside from the last while `short()` says so,
 *  keeping the first `keep` whatever it says. */
function setAsideUntilWhole(items: readonly HTMLElement[], short: () => boolean, keep = 0): void {
  for (const item of items) delete item.dataset.away;
  for (let at = items.length - 1; at >= keep && short(); at--) items[at].dataset.away = "";
}

/** Whether any of `items` still drawn runs past the end of `box`. */
function runsPast(items: readonly HTMLElement[], box: HTMLElement): boolean {
  const end = box.getBoundingClientRect().right;
  return items.some(
    (item) => item.dataset.away === undefined && item.getBoundingClientRect().right > end + 0.5,
  );
}

/**
 * Fits `group` now, and again whenever its pane changes size or its line what it holds.
 * Answers how to stop.
 */
export function fitWaysOn(group: HTMLElement): () => void {
  const line = group.parentElement;
  return refitOn(group, group.closest<HTMLElement>(".pane-frame") ?? line, line, fitWays);
}

/**
 * Runs `fit` on `what` now, and once a frame later whenever `sized` changes size or `held`
 * what it holds. Not on an attribute: setting a thing aside is one, and must not set off
 * another fit.
 */
function refitOn(
  what: HTMLElement,
  sized: HTMLElement | null,
  held: HTMLElement | null,
  fit: (what: HTMLElement) => void,
): () => void {
  fit(what);
  if (sized === null || held === null) return () => undefined;
  let queued = 0;
  const again = () => {
    if (queued !== 0) return;
    queued = requestAnimationFrame(() => {
      queued = 0;
      fit(what);
    });
  };
  const resized = new ResizeObserver(again);
  resized.observe(sized);
  const changed = new MutationObserver(again);
  changed.observe(held, { childList: true, subtree: true, characterData: true });
  return () => {
    resized.disconnect();
    changed.disconnect();
    if (queued !== 0) cancelAnimationFrame(queued);
  };
}

// **The scenario specs draw a breadcrumb's line by hand** (`pane-crumbs.e2e.ts`): they fit
// the two ways they draw with this same function, so what they measure is what a pane draws.
// Only in the `e2e` build (`e2eTasks.ts` says how that is set).
if (import.meta.env.VITE_E2E)
  (window as unknown as { purlisE2eFitWays?: (group: HTMLElement) => void }).purlisE2eFitWays =
    fitWays;
