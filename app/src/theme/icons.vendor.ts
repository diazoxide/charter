/**
 * **How charter's own icons were made from the vendored set** (FM-3, #1106): Material Icon
 * Theme's SVGs (MIT, `app/icons/material-icon-theme/`) turned into `charter-icons.json`'s
 * symbols, each literal colour replaced by the `icon.*` token nearest it.
 *
 * Only `icons.vendor.test.ts` imports this, to hold the JSON to the files it came from; the
 * window never does. It lives in `src/theme/` because it names the vendored colours, and this
 * directory is the one place allowed to.
 */

import type { Token } from "./theme";

/**
 * Every colour the vendored files use, and the token it is drawn with. A file with a colour
 * not listed here fails the conversion, so a new icon is a decision about its colour too.
 * The light tints Material draws a folder's emblem in are all `icon.motive`.
 */
export const TONES: Record<string, Token> = {
  "#90a4ae": "icon.grey",
  "#546e7a": "icon.grey",
  "#35495e": "icon.grey",
  "#0288d1": "icon.blue",
  "#0277bd": "icon.blue",
  "#1e88e5": "icon.blue",
  "#42a5f5": "icon.blue",
  "#ffca28": "icon.yellow",
  "#fdd835": "icon.yellow",
  "#f9a825": "icon.yellow",
  "#ffb300": "icon.yellow",
  "#ffd54f": "icon.yellow",
  "#fbc02d": "icon.yellow",
  "#ff7043": "icon.orange",
  "#ff6e40": "icon.orange",
  "#e65100": "icon.orange",
  "#ff5722": "icon.orange",
  "#e64a19": "icon.orange",
  "#f44336": "icon.red",
  "#ef5350": "icon.red",
  "#ff5252": "icon.red",
  "#e53935": "icon.red",
  "#e57373": "icon.red",
  "#00bcd4": "icon.teal",
  "#00acc1": "icon.teal",
  "#26a69a": "icon.teal",
  "#00bfa5": "icon.teal",
  "#8bc34a": "icon.green",
  "#41b883": "icon.green",
  "#4caf50": "icon.green",
  "#00e676": "icon.green",
  "#afb42b": "icon.green",
  "#7e57c2": "icon.purple",
  "#7c4dff": "icon.purple",
  "#a0f": "icon.purple",
  "#ec407a": "icon.pink",
  "#c8e6c9": "icon.motive",
  "#a7ffeb": "icon.motive",
  "#b3e5fc": "icon.motive",
  "#eceff1": "icon.motive",
  "#dcedc8": "icon.motive",
  "#ffcdd2": "icon.motive",
  "#cfd8dc": "icon.motive",
  "#80deea": "icon.motive",
};

/** The plain folder is drawn in its own token, so a colour theme can set folders apart from
 *  the grey of a plain file that Material gives both. */
const WHOLE: Record<string, Token> = { folder: "icon.folder", "folder-open": "icon.folder" };

/** A symbol as `charter-icons.json` holds it. */
export type Converted = { viewBox: string; paths: { d: string; tone: Token }[] };

/**
 * One vendored file as a symbol. It refuses anything but `<svg>` holding `<path>`s with a
 * `d` and a `fill`: the subset was chosen from files of that shape, and a file of any other
 * shape would need a reader this is not.
 */
export function fromSvg(name: string, text: string): Converted {
  let viewBox: string | undefined;
  /** The fill a path without one inherits: `none` draws nothing, so such a path is dropped. */
  let inherited: string | undefined;
  const paths: Converted["paths"] = [];
  for (const tag of text.matchAll(/<(\/?)([\w:-]+)([^>]*?)\/?>/g)) {
    const [, closing, element, rest] = tag;
    if (closing === "/") continue;
    const attributes = Object.fromEntries(
      [...rest.matchAll(/([\w:-]+)="([^"]*)"/g)].map(([, key, value]) => [key, value]),
    );
    if (element === "svg") {
      viewBox = attributes.viewBox;
      inherited = attributes.fill;
      continue;
    }
    if (element !== "path") throw new Error(`${name} has a <${element}>`);
    const fill = (attributes.fill ?? inherited)?.toLowerCase();
    if (fill === "none") continue;
    const tone = WHOLE[name] ?? (fill === undefined ? undefined : TONES[fill]);
    if (attributes.d === undefined || tone === undefined)
      throw new Error(`${name} has a path coloured ${fill ?? "nothing"}`);
    paths.push({ d: attributes.d, tone });
  }
  if (viewBox === undefined) throw new Error(`${name} has no view box`);
  return { viewBox, paths };
}
