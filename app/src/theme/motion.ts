/**
 * Motion, as data: how long a change in the window takes and how it moves, named once and read
 * by name — which is what `theme.ts` already does for colour, for the same reasons.
 *
 * **Why this module exists.** The window had two motions before it, each spelled out where it
 * was used — `900ms linear` on the spinner, `120ms ease` on the explorer's twisty — and each
 * with its own `@media (prefers-reduced-motion)` block beside it. That is the state `App.css`
 * was in for colour before M6.1: every value a local decision, every guard something the next
 * component had to remember. Asking for more motion (the operator, twice: *"no icons, visual
 * components, animations"* and *"show pipelines with animation"*) on top of that would have
 * multiplied both. So the timing is a vocabulary first, and the motion is built on it (M7.2).
 *
 * **Semantic, not a scale.** A theme sets `duration.enter`, never `duration-200`. The names say
 * what the motion is FOR — a surface arriving, a mark settling on an answer, a mark saying
 * something is still going — so a theme that wants arrivals quicker changes one number and
 * does not also speed up the spinner that happened to share a rung of a scale with it.
 *
 * **Values are numbers, never text.** A duration is a count of milliseconds and an easing is
 * the four numbers of a cubic Bézier. `motionVariables` writes the CSS from those numbers, so
 * nothing a theme file says is ever copied into a declaration — the parse-and-re-emit rule
 * (charter ADR 0041, property 4) that makes a theme safe to take from an extension. `hex and
 * only hex` is the same boundary for colour; `"150ms; } body { display: none"` has nowhere to
 * go when the only thing read is a number.
 *
 * **Reduced motion is this module's job and nobody else's.** Under `prefers-reduced-motion:
 * reduce` every duration is written as `0ms`, so every transition and every animation in the
 * window that reads a token finishes as soon as it starts — including one written tomorrow by
 * somebody who never heard of the media query. A looping animation with a zero duration has a
 * zero active duration (Web Animations, "active duration"), so a spinner does not spin and a
 * pulse does not pulse: each is drawn in its resting state, which is why every looping mark in
 * `App.css` is designed to read correctly standing still. `literals.test.ts` refuses a
 * `prefers-reduced-motion` query anywhere else, so there is only ever this one.
 */

/**
 * How long each kind of change takes, in milliseconds.
 *
 * - `quick` — the window answering something the operator just did: a tab lit as it is
 *   selected, a chevron turning. Short enough that it can never be waited on.
 * - `enter` — a surface arriving: a menu, a popover, a dialog, a region brought back.
 * - `settle` — a mark arriving at an answer: a pipeline that has finished, a chat whose state
 *   has just changed. Longer than `enter`, because it is the one motion meant to be noticed.
 * - `spin` — one turn of a mark saying *this is still running*.
 * - `breathe` — one breath of a mark saying *this is queued and not yet running*.
 */
export const DURATIONS = [
  "duration.quick",
  "duration.enter",
  "duration.settle",
  "duration.spin",
  "duration.breathe",
] as const;

/**
 * How each kind of change moves, as a cubic Bézier.
 *
 * - `standard` — a state changing in place.
 * - `enter` — decelerating: fast out of nothing and slowing into place, which is how a thing
 *   that arrives reads as having arrived rather than as still coming.
 * - `steady` — no easing at all; a turn that eases reads as a stutter once a second.
 * - `breathe` — symmetric in and out, so a loop has no visible seam.
 */
export const EASINGS = [
  "easing.standard",
  "easing.enter",
  "easing.steady",
  "easing.breathe",
] as const;

export type Duration = (typeof DURATIONS)[number];
export type Easing = (typeof EASINGS)[number];
export type MotionToken = Duration | Easing;

/** The four numbers of `cubic-bezier(x1, y1, x2, y2)`. */
export type Bezier = readonly [number, number, number, number];

/** A theme's motion, complete: every duration and every easing has a value. */
export type Motion = {
  durations: Record<Duration, number>;
  easings: Record<Easing, Bezier>;
};

/** Every motion token, durations first — the order a theme file is easiest to read in. */
export const MOTION_TOKENS: readonly MotionToken[] = [...DURATIONS, ...EASINGS];

/**
 * The longest a duration may be. Ten seconds is far past anything a window should take to
 * change, and the ceiling is what stops a typo of `16000` for `1600` from leaving a menu
 * fading in for a quarter of a minute. `0` is allowed, and is how a theme turns motion off.
 */
export const LONGEST = 10_000;

/** Whether a value is a duration a theme may hold: a whole number of milliseconds, 0 to
 *  {@link LONGEST}. A string — `"150ms"` — is not one, on purpose: see this module's header. */
export function isDuration(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= LONGEST;
}

/**
 * Whether a value is an easing a theme may hold: four finite numbers, the two `x` in 0..1 —
 * CSS's own rule, and a declaration that breaks it is dropped whole, so the motion would
 * silently vanish — and the two `y` in -1..2, which leaves room for an overshoot and none for
 * a curve that flings a menu off the window and back.
 */
export function isEasing(value: unknown): value is Bezier {
  if (!Array.isArray(value) || value.length !== 4) return false;
  if (!value.every((n) => typeof n === "number" && Number.isFinite(n))) return false;
  const [x1, y1, x2, y2] = value as number[];
  const within = (n: number, low: number, high: number) => n >= low && n <= high;
  return within(x1, 0, 1) && within(x2, 0, 1) && within(y1, -1, 2) && within(y2, -1, 2);
}

/** Whether a name is one of the durations rather than one of the easings. */
export function isDurationToken(token: MotionToken): token is Duration {
  return (DURATIONS as readonly string[]).includes(token);
}

/** The custom property a motion token is written to: `duration.enter` becomes
 *  `--motion-duration-enter`. The prefix keeps the two vocabularies from ever sharing a
 *  property, so a colour token called `duration.*` could not collide with one of these. */
export function motionProperty(token: MotionToken): string {
  return `--motion-${token.split(".").join("-")}`;
}

/**
 * A built-in theme's motion, which is checked into this repo and therefore trusted to be
 * complete — and is still checked, because a built-in missing a token would otherwise show up
 * only as a transition that silently is not there.
 */
export function builtInMotion(name: string, raw: unknown): Motion {
  const from = (raw ?? {}) as Record<string, unknown>;
  const motion = { durations: {}, easings: {} } as Motion;
  for (const token of DURATIONS) {
    const value = from[token];
    if (!isDuration(value)) throw new Error(`the built-in theme ${name} has no ${token}`);
    motion.durations[token] = value;
  }
  for (const token of EASINGS) {
    const value = from[token];
    if (!isEasing(value)) throw new Error(`the built-in theme ${name} has no ${token}`);
    motion.easings[token] = [...value] as unknown as Bezier;
  }
  return motion;
}

/**
 * Reads the `motion` object of an untrusted theme, filling what it does not supply usably from
 * `base` and saying so. Nothing here throws, for `theme.load`'s reason: a window that will not
 * come up because an easing was mistyped is worse than every possible wrong easing.
 */
export function loadMotion(raw: unknown, base: Motion): { motion: Motion; said: string[] } {
  const said: string[] = [];
  if (raw === undefined) return { motion: base, said };
  const holds = raw !== null && typeof raw === "object" && !Array.isArray(raw);
  if (!holds) said.push("motion is an object of motion token names to values");
  const given = holds ? (raw as Record<string, unknown>) : {};

  const known = new Set<string>(MOTION_TOKENS);
  for (const name of Object.keys(given)) {
    if (!known.has(name)) said.push(`${name} is not a charter motion token`);
  }

  const motion = { durations: { ...base.durations }, easings: { ...base.easings } } as Motion;
  for (const token of DURATIONS) {
    const value = given[token];
    if (value === undefined) continue;
    if (isDuration(value)) motion.durations[token] = value;
    else
      said.push(
        `${token} is ${JSON.stringify(value)}, which is not a whole number of milliseconds from 0 to ${LONGEST}; used ${base.durations[token]}`,
      );
  }
  for (const token of EASINGS) {
    const value = given[token];
    if (value === undefined) continue;
    if (isEasing(value)) motion.easings[token] = [...value] as unknown as Bezier;
    else
      said.push(
        `${token} is ${JSON.stringify(value)}, which is not [x1, y1, x2, y2] with each x from 0 to 1 and each y from -1 to 2; used [${base.easings[token].join(", ")}]`,
      );
  }
  return { motion, said };
}

/**
 * The motion as the custom properties the stylesheet reads, written fresh from numbers.
 *
 * `reduced` is the operator's `prefers-reduced-motion: reduce`, and it collapses every
 * duration to `0ms` — see this module's header for why here and only here. The easings are
 * written either way: a curve over no time is no motion, and leaving them in place means a
 * reader of the document sees the same set of properties whatever the setting.
 */
export function motionVariables(motion: Motion, reduced: boolean): Record<string, string> {
  const written: Record<string, string> = {};
  for (const token of DURATIONS) {
    written[motionProperty(token)] = `${reduced ? 0 : motion.durations[token]}ms`;
  }
  for (const token of EASINGS) {
    written[motionProperty(token)] = `cubic-bezier(${motion.easings[token].join(", ")})`;
  }
  return written;
}

/** The media query the operator's reduced-motion setting is read through. */
export const REDUCE = "(prefers-reduced-motion: reduce)";

/** Whether the operator has asked for less motion. A webview without `matchMedia` (jsdom, for
 *  one) is read as not having asked, which is the platform's own default. */
export function motionReduced(): boolean {
  return typeof matchMedia === "function" && matchMedia(REDUCE).matches;
}
