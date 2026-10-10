/// <reference types="node" />
/**
 * Test-only: **the rendered token check** (DS-1, #956), what `views.test.tsx` holds every view
 * to and `chrome.test.tsx` holds the window's chrome and its dialogs to. Its rules, and why each,
 * are `views.test.tsx`'s header; its own cases are there too.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { literalsIn } from "./literal";
import { MOTION_TOKENS, motionProperty } from "./motion";
import { TOKENS, inForce, property, tinted } from "./theme";

// ---------------------------------------------------------------------------------------------
// What a drawn element may say about colour.
// ---------------------------------------------------------------------------------------------

/** The custom properties the window sets for itself rather than a theme: lengths, never a
 *  colour. The same list, for the same reasons, as `literals.test.ts`'s `fromTheWindow`. */
const FROM_THE_WINDOW = new Set(["--least", "--root", "--chip", "--window-controls"]);

/** The custom properties `App.css` declares for itself — a length or a count, since a colour
 *  there would fail `literals.test.ts`. */
const OWN_PROPERTIES = new Set(
  [...readFileSync(join(process.cwd(), "src/App.css"), "utf8").matchAll(/^\s+(--[\w-]+):/gm)].map(
    (hit) => hit[1],
  ),
);

const COLOUR_TOKENS = new Set(TOKENS.map(property));
const MOTION = new Set(MOTION_TOKENS.map(motionProperty));

/** The SVG attributes that paint. */
const PAINT = ["fill", "stroke", "color", "stop-color", "flood-color", "lighting-color"];

/** A paint value that is no colour of charter's own: no paint, the inherited one, a gradient
 *  or pattern by reference, or a custom property — which is then held to the tokens like any
 *  other `var()`. */
const PAINT_OK = /^(?:none|currentColor|transparent|inherit|url\(#[\w-]+\)|var\(--[\w-]+\))$/i;

/** A custom property read, in any case: `--Text-Muted` is a different property from
 *  `--text-muted`, and a pattern that only saw lower case would let it through unread. */
const READS = /var\(\s*(--[\w-]+)/gi;

/**
 * Tailwind's arbitrary values, anywhere in a class: `bg-[#fff]`, behind a variant
 * (`hover:bg-[#fff]`), with `!` or a `/50` modifier, or a bare arbitrary property
 * (`[color:red]`). No class charter writes has a square bracket in it otherwise, so a bracket
 * at all is the tell — and none of them is a value a theme can reach.
 */
const ARBITRARY = /\[[^\]]*\]/;

/**
 * What is wrong with each `var()` that `value` reads: one that is not a token, or a token the
 * theme in force does not set.
 */
function unread(value: string, said: (what: string) => void, declared?: string): void {
  const set = document.documentElement.style;
  for (const hit of value.matchAll(READS)) {
    const read = hit[1];
    if (radixWiring(declared, read)) continue;
    if (COLOUR_TOKENS.has(read)) {
      if (set.getPropertyValue(read) === "") said(`reads ${read}, which the theme does not set`);
    } else if (!MOTION.has(read) && !FROM_THE_WINDOW.has(read) && !OWN_PROPERTIES.has(read)) {
      said(`reads ${read}, which is not a token`);
    }
  }
}

/**
 * Whether a `var()` is **Radix passing one of its own properties to another** (#956): a
 * popover's content sets `--radix-popover-content-available-width: var(--radix-popper-available-width)`
 * and the like, lengths and an origin its popper measured. Only a `--radix-` property read into
 * a `--radix-` property counts; reading one into `color` or any other property is still not a
 * token, and the value is still held to the literal check like any other.
 */
function radixWiring(declared: string | undefined, read: string): boolean {
  return declared?.startsWith("--radix-") === true && read.startsWith("--radix-");
}

/**
 * Whether `value`, set as the custom property `name` on `element`, is **the theme's own tint**:
 * the token of that name in the theme in force, turned to the hue the element declares in
 * `data-colour` (`theme.tinted`, which is what `tintVariables` writes for a workspace's colour
 * and a persona's, #1449). A theme changes it by changing the token, so it is a token's value
 * and no literal of the window's. Anything else written there is still a literal.
 */
function isTint(element: Element, name: string, value: string): boolean {
  const colour = element.getAttribute("data-colour");
  const token = TOKENS.find((one) => property(one) === name);
  if (colour === null || token === undefined) return false;
  const tint = tinted(inForce(), colour);
  return tint !== inForce() && tint.values[token] === value;
}

/**
 * Everything about `root`'s drawing that a theme could not change, one sentence each;
 * empty when every colour in it is a token the theme in force sets.
 */
export function complaints(root: Element): string[] {
  const said: string[] = [];
  for (const element of [root, ...root.querySelectorAll("*")]) {
    const where = describeElement(element);
    const style = (element as HTMLElement | SVGElement).style;
    if (style) {
      for (let at = 0; at < style.length; at += 1) {
        const name = style[at];
        const value = style.getPropertyValue(name);
        // A custom property's own name is not a colour, but its value can be one: the window
        // setting `--x: #fff` is the same defect as `color: #fff`. So is a colour inside an
        // inlined SVG's data URL.
        if (literalsIn(value).length > 0 && !isTint(element, name, value))
          said.push(`${where} style ${name}: ${value} is a colour literal`);
        unread(value, (what) => said.push(`${where} style ${name} ${what}`), name);
      }
    }
    if (element instanceof SVGElement) {
      for (const attribute of PAINT) {
        const value = element.getAttribute(attribute);
        if (value === null) continue;
        if (!PAINT_OK.test(value.trim()))
          said.push(`${where} ${attribute}="${value}" is not a token`);
        else unread(value, (what) => said.push(`${where} ${attribute}="${value}" ${what}`));
      }
    }
    for (const name of element.classList) {
      if (ARBITRARY.test(name)) said.push(`${where} class ${name} is an arbitrary value`);
    }
  }
  return said;
}

/** An element said the way a complaint can be found by: its tag and its first class. */
function describeElement(element: Element): string {
  const first = element.classList[0];
  return first === undefined
    ? `<${element.tagName.toLowerCase()}>`
    : `<${element.tagName.toLowerCase()}.${first}>`;
}
