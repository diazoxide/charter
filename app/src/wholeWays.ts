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
  const ways = [...group.querySelectorAll<HTMLElement>(":scope > .task-end")];
  for (const way of ways) delete way.dataset.away;
  const line = group.parentElement;
  if (line === null) return;
  const crumbs = line.querySelector<HTMLElement>(":scope > .pane-crumbs");
  const state = crumbs?.querySelector<HTMLElement>(".shown-state") ?? null;
  // Something that must not be given up for a way is: the state word is cut, or pushed out
  // of its breadcrumb, or a way runs past the line's end.
  const short = () => {
    const end = line.getBoundingClientRect().right;
    if (state !== null && crumbs !== null) {
      const word = state.querySelector<HTMLElement>(".word") ?? state;
      if (word.scrollWidth > word.clientWidth + 1) return true;
      if (state.getBoundingClientRect().right > crumbs.getBoundingClientRect().right + 0.5)
        return true;
    }
    return ways.some(
      (way) => way.dataset.away === undefined && way.getBoundingClientRect().right > end + 0.5,
    );
  };
  for (let at = ways.length - 1; at >= 0 && short(); at--) ways[at].dataset.away = "";
}

/**
 * Fits `group` now, and again whenever its pane changes size or its line what it holds.
 * Answers how to stop.
 */
export function fitWaysOn(group: HTMLElement): () => void {
  fitWays(group);
  const line = group.parentElement;
  const pane = group.closest<HTMLElement>(".pane-frame") ?? line;
  if (line === null || pane === null) return () => undefined;
  let queued = 0;
  const again = () => {
    if (queued !== 0) return;
    queued = requestAnimationFrame(() => {
      queued = 0;
      fitWays(group);
    });
  };
  const sized = new ResizeObserver(again);
  sized.observe(pane);
  // What the line holds: a name that changed, a state word, a chip that came or went. Not
  // its attributes: setting a way aside is one, and must not set off another fit.
  const changed = new MutationObserver(again);
  changed.observe(line, { childList: true, subtree: true, characterData: true });
  return () => {
    sized.disconnect();
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
