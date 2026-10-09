import { useEffect, type RefObject } from "react";
import { settingsLevelOf, type ViewRef } from "../tabs";
import { settingsPlace } from "./links";

/**
 * **The keyboard goes into Settings on every way in** (#1206; the spec on #558, user story 32).
 * The palette's Settings…, `⌘,` and the app menu's, the quiet gears, a link from a doctor row
 * or a notice, and the level switcher moving the tab to another level all open the Settings tab
 * — or bring forward the one already open — and each leaves the keyboard **on the group nav's
 * current group**: the one way in a keyboard user has to every group of the level, one arrow
 * key from the next. A link that names one setting puts it on that setting's control instead
 * (NO-7), which is inside the tab too.
 *
 * **Asked, then landed.** A way in asks for it ({@link enterSettings}) with the place the tab is
 * remembered at (`links.settingsPlace`); the tab at that place lands it once it is drawn
 * ({@link useEnteringFocus}). Between the two the tab may not be drawn yet (its level is still
 * being read), may be behind another tab, or a dialog the way in was pressed from may still be
 * up and holding the keyboard. So the ask stands, and is tried again, until the keyboard is
 * inside the tab or {@link PATIENCE} has passed.
 *
 * **A surface a way in closed does not hand the keyboard back** to what opened it — the doctor's
 * button, the status line, the terminal the palette was opened over: its closing move asks
 * {@link landSettingsFocus} first, and leaves the keyboard alone while Settings is taking it.
 *
 * **The person moving on drops it** (#1600): their next click or key, anywhere, ends the ask —
 * whether its tab has the keyboard yet or not — so a tab drawn late never takes the keyboard
 * from where they went, and a surface they close after that hands it back as it always does.
 *
 * **Two Settings tabs of one level side by side** (#1292): the one in the window's focused pane
 * — the one the way in brought forward — takes it; with neither focused, the first one opened.
 */

/** How long a way in waits for its tab to take the keyboard before it is dropped. */
export const PATIENCE = 2000;
/**
 * How long a way in stands once its tab has the keyboard: long enough for the surface it was
 * pressed in to finish closing — Radix hands the keyboard back a turn after the surface goes,
 * and later still when it animates out — so that closing move still finds it.
 */
export const LANDED = 500;
/** How often a standing ask is tried again. */
const AGAIN = 50;

/** One tab's way of taking the keyboard: whether it is inside the tab once it has tried, and
 *  whether the tab is in the window's focused pane. */
type Taker = { take: () => boolean; inFront: () => boolean };

/** The way in standing now: where, until when, and whether its tab has the keyboard yet. */
let asked: { place: string; until: number; landed: boolean } | undefined;
let again: ReturnType<typeof setTimeout> | undefined;
const takers = new Map<string, Set<Taker>>();

/** The place Settings at `view` is remembered at, in the project `plane`; `undefined` for a
 *  view that is not a Settings tab. */
export function placeOfView(view: ViewRef, plane?: string): string | undefined {
  const level = settingsLevelOf(view);
  if (level === undefined) return undefined;
  return settingsPlace(level, plane, level === "workspace" ? view.key : undefined);
}

/** A way into Settings at `place` was taken: the tab there takes the keyboard once it can. */
export function enterSettings(place: string): void {
  asked = { place, until: Date.now() + PATIENCE, landed: false };
  watchThePerson(true);
  // Tried once the window has drawn what the way in changed — the tab it brought forward and
  // the pane it focused — and not against the window as it was before (#1600).
  if (again !== undefined) clearTimeout(again);
  again = setTimeout(attempt, 0);
}

/** The way in standing now, if it has not run out of time. */
function standing(): typeof asked {
  if (asked !== undefined && Date.now() > asked.until) forgetEntering();
  return asked;
}

/** A key that is only a modifier — held for a shortcut, not yet a move of the person's own. */
const MODIFIERS = new Set(["Shift", "Control", "Alt", "Meta", "AltGraph", "CapsLock", "Fn"]);

/** The person's own move: a click or a key ends the standing ask (#1600). */
function movedOn(event: Event): void {
  if (event instanceof KeyboardEvent && MODIFIERS.has(event.key)) return;
  forgetEntering();
}

/**
 * Listens for the person's next move while an ask stands. Added during the event that asked —
 * the click or key of the way in itself — the capture listener on the window is already past,
 * so that event does not drop its own ask.
 */
function watchThePerson(on: boolean): void {
  const change = on ? window.addEventListener : window.removeEventListener;
  for (const type of ["pointerdown", "keydown"]) change.call(window, type, movedOn, true);
}

/** Tries every tab at the asked place; tries again shortly while none has taken it. */
function attempt(): void {
  if (again !== undefined) clearTimeout(again);
  again = undefined;
  const now = standing();
  if (now === undefined) return;
  const all = [...(takers.get(now.place) ?? [])];
  // The tab in the focused pane first; the rest, as they were opened.
  const front = all.filter((one) => one.inFront());
  for (const { take } of front.length > 0 ? front : all) {
    if (take()) {
      if (!now.landed) asked = { ...now, until: Date.now() + LANDED, landed: true };
      return;
    }
  }
  again = setTimeout(attempt, AGAIN);
}

/**
 * **A surface's closing move** — a dialog, a drawer or a menu that a way into Settings was
 * pressed in: `true` while Settings is taking the keyboard or has just taken it, so the surface
 * leaves it alone rather than handing it back to its trigger (and the keyboard is put back in
 * Settings if the surface pulled it out while it closed); `false` when nothing asked, and the
 * surface gives it back as it always does.
 */
export function landSettingsFocus(): boolean {
  if (standing() === undefined) return false;
  attempt();
  return true;
}

/** Forgets any standing ask: a fresh window's state, for tests. */
export function forgetEntering(): void {
  if (again !== undefined) clearTimeout(again);
  again = undefined;
  asked = undefined;
  watchThePerson(false);
}

/**
 * **The tab's half**: while a way in is asked at `place`, puts the keyboard on the nav's current
 * group inside `holder` — unless it is already inside, on the setting a link named for one.
 * Tried on every draw, so a tab whose groups arrive later lands it as they do.
 */
export function useEnteringFocus(place: string, holder: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const take = () => {
      const root = holder.current;
      if (root === null || !root.isConnected) return false;
      if (root.contains(document.activeElement)) return true;
      const nav = root.querySelector<HTMLElement>(".ui-settings-nav");
      const target =
        nav?.querySelector<HTMLElement>('[aria-current="true"]') ??
        nav?.querySelector<HTMLElement>("button");
      if (target === null || target === undefined) return false;
      target.focus({ preventScroll: true });
      return root.contains(document.activeElement);
    };
    // The window marks the pane that has its focus (`PlaneView`'s view panes).
    const inFront = () => holder.current?.closest(".pane.view.focused") != null;
    const taker: Taker = { take, inFront };
    const all = takers.get(place) ?? new Set<Taker>();
    takers.set(place, all.add(taker));
    return () => {
      all.delete(taker);
      if (all.size === 0) takers.delete(place);
    };
  }, [place, holder]);
  // Every draw: the groups may just have arrived. Nothing is done while nothing is asked.
  useEffect(() => {
    const now = standing();
    if (now?.place === place && !now.landed) attempt();
  });
}
