/**
 * **The icon theme** (FM-3, #1106; #1103 F5): what a file or a folder in a tree is drawn as.
 *
 * It is **data beside the colour theme**, and it is held to the colour theme's rules:
 *
 * - **A map, by name.** A file is looked up by its whole name first (`package.json`), then by
 *   its extensions, longest first (`d.ts` before `ts`); a folder by its name. Names are matched
 *   without regard to case. Whatever is not mapped is the theme's `file` or `folder`.
 * - **Colours are tokens, never literals.** Each shape of a symbol names one of the `icon.*`
 *   tokens (`theme.ts`), so the icons follow light and dark with the colour theme, a colour
 *   theme can recolour every icon set at once, and the literal guard covers the icons as it
 *   covers everything else. A shape with a colour of its own is not a shape.
 * - **Data, never markup.** A symbol is a view box and a list of path outlines, each checked
 *   against the path grammar (letters and numbers, nothing else) and drawn by `FileIcon.tsx` as
 *   React elements it builds itself. No SVG text is ever put in the page, so an icon theme an
 *   extension contributes cannot carry a script, a link, a style or a fetch: there is no field
 *   to put one in. That is ADR 0041's parse-and-re-emit, the rule `theme.ts`'s `load` keeps.
 * - **A bad theme never stops a tree.** `loadIcons` always answers with a complete theme: what
 *   it cannot read is said in a complaint, and the built-in's `file` and `folder` stand in for
 *   a theme that has none.
 *
 * charter's own, `charter-icons`, is a small subset of Material Icon Theme's file-type icons
 * (MIT), vendored under `app/icons/material-icon-theme/` and converted to this format —
 * `icons.vendor.test.ts` holds the conversion to those files.
 */

import charterIcons from "./charter-icons.json";
import { TOKENS, type Token } from "./theme";

/** The tokens an icon may be coloured with: the colour theme's `icon.*`. */
export const ICON_TOKENS = TOKENS.filter((token) => token.startsWith("icon.")) as Token[];

/** One outline of a symbol, filled with one token. */
export type Shape = {
  /** SVG path data: commands and numbers only. */
  d: string;
  tone: Token;
  /** Fill by the even-odd rule rather than nonzero. */
  evenOdd: boolean;
};

/** One icon: a view box and its outlines, drawn in order. */
export type IconSymbol = {
  /** The symbol's name in its theme, which the tree puts on the row for tests and styling. */
  name: string;
  viewBox: string;
  shapes: Shape[];
};

/** An icon theme, complete: it always has a `file` and a `folder`. */
export type IconTheme = {
  name: string;
  file: IconSymbol;
  folder: IconSymbol;
  folderOpen: IconSymbol;
  /** By lower-cased extension, without its first dot: `ts`, `d.ts`. */
  extensions: ReadonlyMap<string, IconSymbol>;
  /** By lower-cased whole file name. */
  filenames: ReadonlyMap<string, IconSymbol>;
  /** By lower-cased folder name. */
  folders: ReadonlyMap<string, IconSymbol>;
  foldersOpen: ReadonlyMap<string, IconSymbol>;
};

/** What a tree row is: a folder, opened or not, or anything else (a file or a link). */
export type Row = { name: string; folder: false } | { name: string; folder: true; open: boolean };

/** The symbol `row` is drawn as. */
export function iconFor(icons: IconTheme, row: Row): IconSymbol {
  const name = row.name.toLowerCase();
  if (row.folder) {
    if (row.open) return icons.foldersOpen.get(name) ?? icons.folders.get(name) ?? icons.folderOpen;
    return icons.folders.get(name) ?? icons.folder;
  }
  const whole = icons.filenames.get(name);
  if (whole !== undefined) return whole;
  // Each extension, longest first: `a.d.ts` tries `d.ts`, then `ts`. A leading dot is part of
  // the name (`.env` has no extension, as git and every shell read it).
  for (let at = name.indexOf(".", 1); at !== -1; at = name.indexOf(".", at + 1)) {
    const found = icons.extensions.get(name.slice(at + 1));
    if (found !== undefined) return found;
  }
  return icons.file;
}

/** The most a theme may hold: past these it is a theme written to stall a window. */
export const MOST_SYMBOLS = 2048;
export const MOST_SHAPES = 32;
export const MOST_PATH = 16384;
export const MOST_NAMES = 8192;
/** The longest view box read: four numbers fit in far less, and a field read before it is
 *  measured is a field that can cost what its writer likes. */
export const MOST_VIEW_BOX = 64;

/** SVG path data, and nothing that is not: command letters, numbers, signs, separators. */
const PATH = /^[Mm][MmZzLlHhVvCcSsQqTtAa0-9eE.,+\-\s]*$/;
/** One number of a view box. */
const NUMBER = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;

/** A theme, and what it cost to get one. */
export type LoadedIcons = { icons: IconTheme; complaints: string[] };

const isObject = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);

/** `raw`'s own entries, read into a `Map` by the callers: so a name like `__proto__` is a
 *  name, never the prototype of anything. */
const entries = (raw: unknown): [string, unknown][] => (isObject(raw) ? Object.entries(raw) : []);

/** A view box as four numbers, written back from them; `undefined` when it is not one. */
function viewBoxOf(raw: unknown): string | undefined {
  if (typeof raw !== "string" || raw.length > MOST_VIEW_BOX) return undefined;
  const parts = raw.trim().split(/[\s,]+/);
  if (parts.length !== 4 || !parts.every((part) => NUMBER.test(part))) return undefined;
  const [x, y, width, height] = parts.map(Number);
  if (![x, y, width, height].every(Number.isFinite) || width <= 0 || height <= 0) return undefined;
  return `${x} ${y} ${width} ${height}`;
}

/** One symbol, or why it is not one. */
function symbolOf(name: string, raw: unknown): IconSymbol | string {
  if (!isObject(raw)) return `the symbol ${name} is not an object`;
  const viewBox = viewBoxOf(raw.viewBox);
  if (viewBox === undefined) return `the symbol ${name} has no view box of four numbers`;
  if (!Array.isArray(raw.paths) || raw.paths.length === 0) return `the symbol ${name} has no paths`;
  if (raw.paths.length > MOST_SHAPES)
    return `the symbol ${name} has more than ${MOST_SHAPES} paths`;
  const tones = new Set<string>(ICON_TOKENS);
  const shapes: Shape[] = [];
  for (const path of raw.paths as unknown[]) {
    if (!isObject(path)) return `a path of ${name} is not an object`;
    const { d, tone } = path;
    if (typeof d !== "string" || d.length > MOST_PATH || !PATH.test(d))
      return `a path of ${name} is not path data`;
    if (typeof tone !== "string" || !tones.has(tone))
      return `a path of ${name} is coloured ${JSON.stringify(tone)}, which is not one of ${ICON_TOKENS.join(", ")}`;
    shapes.push({ d, tone: tone as Token, evenOdd: path.evenOdd === true });
  }
  return { name, viewBox, shapes };
}

/**
 * Reads whatever an icon theme file parsed to, and always answers with a complete theme.
 *
 * `raw` is untrusted: `JSON.parse` of a file an extension contributed. Nothing here throws, and
 * everything that reaches the page is a string this function checked or wrote.
 */
export function loadIcons(raw: unknown, fallback: IconTheme = DEFAULT_ICONS): LoadedIcons {
  const complaints: string[] = [];
  if (!isObject(raw)) {
    return { icons: fallback, complaints: ["an icon theme is a JSON object"] };
  }
  const symbols = new Map<string, IconSymbol>();
  const given = entries(raw.symbols);
  if (given.length > MOST_SYMBOLS)
    complaints.push(`only the first ${MOST_SYMBOLS} symbols are read`);
  for (const [name, value] of given.slice(0, MOST_SYMBOLS)) {
    const symbol = symbolOf(name, value);
    if (typeof symbol === "string") complaints.push(symbol);
    else symbols.set(name, symbol);
  }
  const named = (field: string, value: unknown): IconSymbol | undefined => {
    if (value === undefined) return undefined;
    const symbol = typeof value === "string" ? symbols.get(value) : undefined;
    if (symbol === undefined)
      complaints.push(`${field} names ${JSON.stringify(value)}, which is no symbol`);
    return symbol;
  };
  const map = (field: string): Map<string, IconSymbol> => {
    const out = new Map<string, IconSymbol>();
    const pairs = entries(raw[field]);
    if (raw[field] !== undefined && !isObject(raw[field]))
      complaints.push(`${field} is an object of names to symbols`);
    for (const [key, value] of pairs.slice(0, MOST_NAMES)) {
      const symbol = named(`${field}.${key}`, value);
      if (symbol !== undefined) out.set(key.toLowerCase(), symbol);
    }
    if (pairs.length > MOST_NAMES)
      complaints.push(`only the first ${MOST_NAMES} of ${field} are read`);
    return out;
  };
  const folder = named("folder", raw.folder) ?? fallback.folder;
  const icons: IconTheme = {
    name: typeof raw.name === "string" && raw.name.trim() !== "" ? raw.name.trim() : fallback.name,
    file: named("file", raw.file) ?? fallback.file,
    folder,
    folderOpen: named("folderOpen", raw.folderOpen) ?? folder,
    extensions: map("extensions"),
    filenames: map("filenames"),
    folders: map("folders"),
    foldersOpen: map("foldersOpen"),
  };
  return { icons, complaints };
}

/** A built-in, checked into this repo: a complaint about it is a defect, so it throws. */
function builtIn(raw: unknown, base?: IconTheme): IconTheme {
  const { icons, complaints } = loadIcons(raw, base);
  if (complaints.length > 0) throw new Error(`a built-in icon theme: ${complaints.join("; ")}`);
  return icons;
}

/** A stand-in for loading the default itself: never drawn, only there to be a fallback. */
const NOTHING: IconSymbol = { name: "", viewBox: "0 0 1 1", shapes: [] };
const BARE: IconTheme = {
  name: "",
  file: NOTHING,
  folder: NOTHING,
  folderOpen: NOTHING,
  extensions: new Map(),
  filenames: new Map(),
  folders: new Map(),
  foldersOpen: new Map(),
};

/** charter's own icon theme: what a project that picks none draws. */
export const DEFAULT_ICONS: IconTheme = builtIn(charterIcons, BARE);

/** The icon themes charter ships, by the name `[theme] icons` picks them by
 *  (`charter_core::extension::BUILT_IN_ICON_THEMES`). */
export const BUILT_IN_ICONS: Record<string, IconTheme> = { "charter-icons": DEFAULT_ICONS };

/** What a shape carries as `data-tone` for `App.css` to colour: the token's custom property
 *  without its dashes, `icon.blue` as `icon-blue`. */
export function toneOf(token: Token): string {
  return token.split(".").join("-");
}
