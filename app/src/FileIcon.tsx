import { toneOf, type IconSymbol } from "./theme/icons";

/**
 * **One icon of the icon theme** (FM-3, #1106), as React elements this component builds: an
 * `<svg>` with the symbol's view box and a `<path>` per outline, each carrying its token as
 * `data-tone` for `App.css` to colour. Nothing else from the theme reaches the page — no SVG
 * text, no attribute it named, no style — so a contributed icon theme cannot carry a script,
 * a link or a fetch (`theme/icons.ts` says why the data has no room for one).
 *
 * Decorative, like every icon beside a name: hidden from assistive technology, so a row is
 * named by its words.
 */
export function FileIcon({ symbol }: { symbol: IconSymbol }) {
  return (
    <svg
      className="node-icon file-icon"
      viewBox={symbol.viewBox}
      aria-hidden="true"
      focusable="false"
      data-icon={symbol.name}
    >
      {symbol.shapes.map((shape, at) => (
        <path
          key={at}
          d={shape.d}
          data-tone={toneOf(shape.tone)}
          fillRule={shape.evenOdd ? "evenodd" : undefined}
        />
      ))}
    </svg>
  );
}
