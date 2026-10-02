/**
 * **What a colour written out looks like**, for the two guards that refuse one outside a theme
 * file: `literals.test.ts`, which reads the source, and `views.test.tsx`, which reads what every
 * view draws. One definition, so the two guards cannot disagree about what a colour is.
 *
 * It lives in `src/theme/` because it spells colours out by name, and this directory is the
 * one place allowed to. Nothing in the window imports it.
 */

/** Every named colour CSS Color 4 defines, lower-cased. `transparent` and `currentColor` are
 *  not here: they are the absence of a colour and the inherited one, and the tokens use both. */
export const NAMED_COLOURS: readonly string[] = [
  "aliceblue",
  "antiquewhite",
  "aqua",
  "aquamarine",
  "azure",
  "beige",
  "bisque",
  "black",
  "blanchedalmond",
  "blue",
  "blueviolet",
  "brown",
  "burlywood",
  "cadetblue",
  "chartreuse",
  "chocolate",
  "coral",
  "cornflowerblue",
  "cornsilk",
  "crimson",
  "cyan",
  "darkblue",
  "darkcyan",
  "darkgoldenrod",
  "darkgray",
  "darkgreen",
  "darkgrey",
  "darkkhaki",
  "darkmagenta",
  "darkolivegreen",
  "darkorange",
  "darkorchid",
  "darkred",
  "darksalmon",
  "darkseagreen",
  "darkslateblue",
  "darkslategray",
  "darkslategrey",
  "darkturquoise",
  "darkviolet",
  "deeppink",
  "deepskyblue",
  "dimgray",
  "dimgrey",
  "dodgerblue",
  "firebrick",
  "floralwhite",
  "forestgreen",
  "fuchsia",
  "gainsboro",
  "ghostwhite",
  "gold",
  "goldenrod",
  "gray",
  "green",
  "greenyellow",
  "grey",
  "honeydew",
  "hotpink",
  "indianred",
  "indigo",
  "ivory",
  "khaki",
  "lavender",
  "lavenderblush",
  "lawngreen",
  "lemonchiffon",
  "lightblue",
  "lightcoral",
  "lightcyan",
  "lightgoldenrodyellow",
  "lightgray",
  "lightgreen",
  "lightgrey",
  "lightpink",
  "lightsalmon",
  "lightseagreen",
  "lightskyblue",
  "lightslategray",
  "lightslategrey",
  "lightsteelblue",
  "lightyellow",
  "lime",
  "limegreen",
  "linen",
  "magenta",
  "maroon",
  "mediumaquamarine",
  "mediumblue",
  "mediumorchid",
  "mediumpurple",
  "mediumseagreen",
  "mediumslateblue",
  "mediumspringgreen",
  "mediumturquoise",
  "mediumvioletred",
  "midnightblue",
  "mintcream",
  "mistyrose",
  "moccasin",
  "navajowhite",
  "navy",
  "oldlace",
  "olive",
  "olivedrab",
  "orange",
  "orangered",
  "orchid",
  "palegoldenrod",
  "palegreen",
  "paleturquoise",
  "palevioletred",
  "papayawhip",
  "peachpuff",
  "peru",
  "pink",
  "plum",
  "powderblue",
  "purple",
  "rebeccapurple",
  "red",
  "rosybrown",
  "royalblue",
  "saddlebrown",
  "salmon",
  "sandybrown",
  "seagreen",
  "seashell",
  "sienna",
  "silver",
  "skyblue",
  "slateblue",
  "slategray",
  "slategrey",
  "snow",
  "springgreen",
  "steelblue",
  "tan",
  "teal",
  "thistle",
  "tomato",
  "turquoise",
  "violet",
  "wheat",
  "white",
  "whitesmoke",
  "yellow",
  "yellowgreen",
];

/** CSS's system colours: the platform's palette, which a theme cannot reach any more than it
 *  can reach a hex value. The current set only — CSS Color 4's deprecated ones (`background`,
 *  `menu`, `window`…) are also property names and ordinary words, so a guard that refused them
 *  would trip on `transition-property: background`; a browser maps each to one of these. */
export const SYSTEM_COLOURS: readonly string[] = [
  "accentcolor",
  "accentcolortext",
  "activetext",
  "buttonborder",
  "buttonface",
  "buttontext",
  "canvas",
  "canvastext",
  "field",
  "fieldtext",
  "graytext",
  "highlight",
  "highlighttext",
  "linktext",
  "mark",
  "marktext",
  "selecteditem",
  "selecteditemtext",
  "visitedtext",
];

const WORDS = [...NAMED_COLOURS, ...SYSTEM_COLOURS].join("|");

/** One kind of colour literal, and the pattern that finds it. Keyword matching ignores case,
 *  because CSS does: `DarkSlateGray` is `darkslategray`. */
export const LITERALS: readonly { what: string; pattern: RegExp }[] = [
  { what: "a hex colour", pattern: /#[0-9a-fA-F]{3,8}\b/g },
  {
    what: "a colour function",
    pattern: /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color|color-mix|light-dark)\s*\(/gi,
  },
  { what: "a named CSS colour", pattern: new RegExp(`(?<![\\w-])(?:${WORDS})(?![\\w-])`, "gi") },
];

/**
 * What every `data:` URL in `text` holds, decoded — percent-encoding and base64 both — so a
 * colour inside an inlined SVG (`fill='%23fff'`) is read as the `#fff` it is.
 */
export function dataUrls(text: string): string[] {
  const found: string[] = [];
  // Quoted, the URL runs to its own closing quote — an SVG inside one quotes with the other.
  for (const hit of text.matchAll(
    /url\(\s*(?:"(data:[^"]*)"|'(data:[^']*)'|(data:[^)\s]*))\s*\)/gi,
  )) {
    const url = hit[1] ?? hit[2] ?? hit[3];
    const comma = url.indexOf(",");
    if (comma < 0) continue;
    const head = url.slice(0, comma);
    const body = url.slice(comma + 1);
    try {
      found.push(/;base64$/i.test(head) ? atob(body) : decodeURIComponent(body));
    } catch {
      // Not decodable: say it as it is, which still catches a literal written in plain.
      found.push(body);
    }
  }
  return found;
}

/** Every colour literal in `value`, including those inside its data URLs, as `what: literal`. */
export function literalsIn(value: string): { what: string; literal: string; index: number }[] {
  const out: { what: string; literal: string; index: number }[] = [];
  for (const { what, pattern } of LITERALS) {
    for (const hit of value.matchAll(pattern))
      out.push({ what, literal: hit[0], index: hit.index });
  }
  for (const decoded of dataUrls(value)) {
    for (const { what, pattern } of LITERALS) {
      for (const hit of decoded.matchAll(pattern))
        out.push({ what: `${what} in a data URL`, literal: hit[0], index: -1 });
    }
  }
  return out;
}
