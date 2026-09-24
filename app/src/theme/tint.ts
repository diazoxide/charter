/**
 * **A workspace's colour, as a hue shift of one colour** (charter-app#281).
 *
 * The operator: *"workspace collor can add some filter and make same theme but with different
 * collor"*. So a colour is not a second theme and not a palette of its own: it is a **hue**, and
 * a token it tints keeps everything else it had. `theme.ts` decides WHICH tokens (the accent and
 * the tab shades, never the text or the terminal); this file is the arithmetic for one of them.
 *
 * # Why OKLCH, and why the luminance is kept exactly
 *
 * The shift is done in OKLCH — lightness, chroma, hue — because a hue turned there keeps its
 * perceived lightness, where HSL's does not (HSL's yellow and blue at one "lightness" are
 * nothing alike). But perceived lightness is still not WCAG's relative luminance, and WCAG's is
 * what `contrast.test.ts` holds every pair to. So after the hue is set, the lightness is searched
 * for until the tinted colour has **the relative luminance the original had**: every contrast
 * ratio a theme cleared, its tinted shades clear too, whatever the hue, up to the rounding of
 * one 8-bit channel. The test runs every palette hue on both built-ins to hold that.
 *
 * **A grey gets a little colour; a colour keeps its own amount.** The layer shades are neutral
 * greys with no hue to turn, so they are given {@link NEUTRAL_CHROMA} — a tint, felt rather than
 * noticed, which is ADR 0036's rule for the strips — and the accent keeps the chroma it had. A
 * colour the screen cannot show at that chroma is brought into sRGB by lowering the chroma, never
 * the lightness. White stays white: nothing brighter than white is a colour.
 *
 * Pure functions of strings and numbers; no theme is imported, so `theme.ts` can import this.
 */

/**
 * The palette a workspace names its colour from, as OKLCH hues in degrees
 * (`charter_core::extension::project::theme::PALETTE` names the same eight, and
 * `tint.test.ts` holds the two lists to each other).
 */
export const PALETTE: Readonly<Record<string, number>> = {
  red: 25,
  orange: 55,
  yellow: 95,
  green: 145,
  teal: 190,
  blue: 255,
  purple: 305,
  pink: 350,
};

/** How much colour a neutral grey is given: a step a person feels on a strip rather than sees. */
export const NEUTRAL_CHROMA = 0.03;

/** Below this chroma a colour counts as grey, and has no hue of its own worth keeping. */
const GREY = 0.02;

/** `#rrggbb` and nothing else: what a workspace writes for a colour of its own. */
const CUSTOM = /^#[0-9a-fA-F]{6}$/;

type Vec3 = [number, number, number];

/**
 * The hue a workspace's colour tints with: a {@link PALETTE} name's, or the OKLCH hue of a
 * `#rrggbb`. `undefined` for no colour, for one that is neither — the core has already said why
 * in the settings tab — and for a grey, which has no hue to give.
 */
export function hueOf(colour: string | null | undefined): number | undefined {
  if (colour === null || colour === undefined) return undefined;
  if (Object.prototype.hasOwnProperty.call(PALETTE, colour)) return PALETTE[colour];
  if (!CUSTOM.test(colour)) return undefined;
  const [, a, b] = oklab(parse(colour).rgb);
  if (Math.hypot(a, b) < GREY) return undefined;
  return degrees(Math.atan2(b, a));
}

/**
 * `hex` turned to `hue`, with the relative luminance it had. `hex` is a theme's value, so it is
 * `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`; an alpha it had, it keeps.
 */
export function tintHex(hex: string, hue: number): string {
  const { rgb, alpha } = parse(hex);
  const target = luminance(rgb);
  const [, a, b] = oklab(rgb);
  const had = Math.hypot(a, b);
  const chroma = had < GREY ? NEUTRAL_CHROMA : had;
  const at = (lightness: number) => shown(lightness, chroma, hue);
  // Luminance rises with lightness at a fixed chroma and hue, so the lightness that gives the
  // luminance the colour had is found by halving the interval.
  let low = 0;
  let high = 1;
  for (let step = 0; step < 32; step++) {
    const middle = (low + high) / 2;
    if (luminance(at(middle)) < target) low = middle;
    else high = middle;
  }
  return `#${at((low + high) / 2)
    .map(channel)
    .join("")}${alpha}`;
}

/** WCAG's relative luminance of a colour in linear sRGB. */
function luminance([r, g, b]: Vec3): number {
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The colour at this OKLCH, in linear sRGB — with the chroma lowered until the screen can
 *  show it, when it cannot. */
function shown(lightness: number, chroma: number, hue: number): Vec3 {
  const at = (c: number) => linear([lightness, c * cos(hue), c * sin(hue)]);
  const whole = at(chroma);
  if (inGamut(whole)) return whole;
  let low = 0;
  let high = chroma;
  for (let step = 0; step < 24; step++) {
    const middle = (low + high) / 2;
    if (inGamut(at(middle))) low = middle;
    else high = middle;
  }
  return at(low).map((v) => Math.min(1, Math.max(0, v))) as Vec3;
}

function inGamut(rgb: Vec3): boolean {
  return rgb.every((v) => v >= -1e-6 && v <= 1 + 1e-6);
}

const cos = (deg: number) => Math.cos((deg * Math.PI) / 180);
const sin = (deg: number) => Math.sin((deg * Math.PI) / 180);
const degrees = (rad: number) => ((rad * 180) / Math.PI + 360) % 360;

/** A theme value as linear sRGB, and its alpha as the two hex digits it was written with. */
function parse(hex: string): { rgb: Vec3; alpha: string } {
  const digits = hex.slice(1);
  const long =
    digits.length <= 4
      ? [...digits].map((d) => d + d).join("")
      : digits;
  const rgb = [0, 2, 4].map((at) => toLinear(Number.parseInt(long.slice(at, at + 2), 16) / 255));
  return { rgb: rgb as Vec3, alpha: long.slice(6, 8) };
}

/** One linear channel as two hex digits. */
function channel(linearValue: number): string {
  const v = Math.min(1, Math.max(0, linearValue));
  const gamma = v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
  return Math.round(gamma * 255)
    .toString(16)
    .padStart(2, "0");
}

function toLinear(v: number): number {
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

/** Linear sRGB to OKLab (Björn Ottosson's matrices). */
function oklab([r, g, b]: Vec3): Vec3 {
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

/** OKLab to linear sRGB. */
function linear([lightness, a, b]: Vec3): Vec3 {
  const l = (lightness + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (lightness - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (lightness - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}
