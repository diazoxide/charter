/**
 * **The preview's images, from their bytes** (FM-2, #1132): what `PieceFiles.tsx`'s preview
 * needs to draw a picture without ever loading one.
 *
 * The window's CSP gives images no source but the app's own (`tauri.conf.json`), and a `data:`
 * or `blob:` source would be one more way for whatever the window shows to fetch. So nothing
 * here makes a URL or an `<img>`: every picture is decoded from bytes it is handed, and drawn on
 * a canvas.
 *
 * - **A still image** is decoded with `createImageBitmap`, as it always was.
 * - **An animated one** (GIF, APNG, animated WebP) plays frame by frame through WebCodecs'
 *   `ImageDecoder` where the webview has one ({@link openFrames}), and shows its first frame
 *   where it has not, saying so ({@link animated} tells from the bytes).
 * - **An SVG** is decoded as an image too, never put into the page as markup: as an image it
 *   runs no script and loads nothing it names. Where the webview will not decode SVG from bytes
 *   ({@link useSvgDraws}), the preview shows its text, as it did before.
 */
import { useEffect, useRef, useState, type RefObject } from "react";

/** The most pixels the preview draws: the core's own bound for an image (`files.rs`). */
export const MOST_PIXELS = 40_000_000;

/** The MIME type an SVG is decoded as. */
export const SVG = "image/svg+xml";

/** A file's bytes, from the base64 the core sends. */
export function bytesOf(base64: string): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
}

/** Whether a file is an SVG, by its name: what the preview draws rather than lists. */
export function isSvg(path: string): boolean {
  return /\.svg$/i.test(path);
}

/** Pixels per unit, for the absolute units an SVG's size can be given in. */
const UNIT: Record<string, number> = {
  "": 1,
  px: 1,
  in: 96,
  cm: 96 / 2.54,
  mm: 96 / 25.4,
  pt: 96 / 72,
  pc: 16,
};

/** One length attribute of the root's start tag, in pixels; `undefined` for none or a relative one. */
function length(tag: string, name: string): number | undefined {
  const found = new RegExp(
    `\\s${name}\\s*=\\s*["']\\s*((?:[0-9]+(?:\\.[0-9]*)?|\\.[0-9]+)(?:e[+-]?[0-9]+)?)([a-z]*)\\s*["']`,
    "i",
  ).exec(tag);
  if (found === null) return undefined;
  const per = UNIT[found[2].toLowerCase()];
  return per === undefined ? undefined : Number(found[1]) * per;
}

/**
 * **The size an SVG declares**, read from its root's start tag: its `width` and `height`, the
 * missing one of the two from `viewBox`'s proportions. `undefined` when it declares no size the
 * preview can read, which an SVG drawn as an image then takes as the platform's default.
 *
 * Read as text, never parsed into a document, so that a huge canvas is said rather than decoded.
 */
export function svgSide(text: string): { width: number; height: number } | undefined {
  const start = /<svg\b/i.exec(text)?.index;
  const end = start === undefined ? -1 : text.indexOf(">", start);
  if (start === undefined || end < 0) return undefined;
  const tag = text.slice(start, end + 1);
  let width = length(tag, "width");
  let height = length(tag, "height");
  const box = /\sviewBox\s*=\s*["']([^"']*)["']/i
    .exec(tag)?.[1]
    .trim()
    .split(/[\s,]+/)
    .map(Number);
  const ratio =
    box !== undefined && box.length === 4 && box[2] > 0 && box[3] > 0 ? box[3] / box[2] : undefined;
  if (width !== undefined && height === undefined && ratio !== undefined) height = width * ratio;
  if (height !== undefined && width === undefined && ratio !== undefined) width = height / ratio;
  if (width === undefined || height === undefined) return undefined;
  return { width: Math.ceil(width), height: Math.ceil(height) };
}

/** A small SVG every webview that decodes SVG from bytes decodes. */
const PROBE = '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>';

let probed: Promise<boolean> | undefined;
let answered: boolean | undefined;

/**
 * Whether this webview decodes an SVG from its bytes, asked once per window with a one-pixel
 * SVG. The webviews purlis ships on were not known to (WebView2 and WebKit took bitmap formats
 * only, when this was written), so SVG may stay text everywhere for now; a webview that answers
 * yes starts drawing SVG with no change here.
 */
function svgDraws(): Promise<boolean> {
  probed ??= (async () => {
    try {
      const bitmap = await createImageBitmap(new Blob([PROBE], { type: SVG }));
      bitmap.close();
      answered = true;
    } catch {
      answered = false;
    }
    return answered;
  })();
  return probed;
}

/** {@link svgDraws} for a component: `undefined` until the window has been asked. */
export function useSvgDraws(): boolean | undefined {
  const [draws, setDraws] = useState(answered);
  useEffect(() => {
    if (draws !== undefined) return;
    let gone = false;
    void svgDraws().then((got) => {
      if (!gone) setDraws(got);
    });
    return () => {
      gone = true;
    };
  }, [draws]);
  return draws;
}

/** Forgets what the webview answered, for tests that stub `createImageBitmap`. */
export function forgetSvgProbe(): void {
  probed = undefined;
  answered = undefined;
}

/** The ASCII of `text` starts at `at` in `bytes`. */
function says(bytes: Uint8Array, at: number, text: string): boolean {
  if (at + text.length > bytes.length) return false;
  for (let i = 0; i < text.length; i++) if (bytes[at + i] !== text.charCodeAt(i)) return false;
  return true;
}

/** A GIF holds two images or more: its blocks walked to the second image descriptor. */
function gifAnimated(bytes: Uint8Array): boolean {
  if (bytes.length < 13) return false;
  // The header and the logical screen, then its global colour table if the flags say so.
  let at = 13 + (bytes[10] & 0x80 ? 3 << ((bytes[10] & 0x07) + 1) : 0);
  /** Steps over a run of data sub-blocks; `undefined` when the bytes end inside it. */
  const skipBlocks = (from: number): number | undefined => {
    let i = from;
    while (i < bytes.length) {
      const size = bytes[i];
      if (size === 0) return i + 1;
      i += size + 1;
    }
    return undefined;
  };
  let images = 0;
  while (at < bytes.length) {
    const kind = bytes[at];
    if (kind === 0x21) {
      const next = skipBlocks(at + 2);
      if (next === undefined) return false;
      at = next;
    } else if (kind === 0x2c) {
      images += 1;
      if (images > 1) return true;
      if (at + 10 > bytes.length) return false;
      const flags = bytes[at + 9];
      at += 10 + (flags & 0x80 ? 3 << ((flags & 0x07) + 1) : 0);
      const next = skipBlocks(at + 1);
      if (next === undefined) return false;
      at = next;
    } else {
      return false;
    }
  }
  return false;
}

/** A PNG has an animation control chunk before its image data: an APNG. */
function pngAnimated(bytes: Uint8Array): boolean {
  let at = 8;
  while (at + 8 <= bytes.length) {
    const size =
      ((bytes[at] << 24) | (bytes[at + 1] << 16) | (bytes[at + 2] << 8) | bytes[at + 3]) >>> 0;
    if (says(bytes, at + 4, "acTL")) return true;
    if (says(bytes, at + 4, "IDAT")) return false;
    at += 12 + size;
  }
  return false;
}

/** A WebP's extended header has its animation flag set. */
function webpAnimated(bytes: Uint8Array): boolean {
  return says(bytes, 12, "VP8X") && bytes.length > 20 && (bytes[20] & 0x02) !== 0;
}

/**
 * **Whether an image is animated**, from its bytes alone: a GIF with a second image, a PNG with
 * an animation control chunk (APNG), a WebP with its animation flag. What lets the preview say
 * it shows an animated image's first frame where it cannot play it. Bytes cut short or made up
 * are still: the walk never reads past the end, and gives up on what it does not know.
 */
export function animated(bytes: Uint8Array, mime: string): boolean {
  switch (mime) {
    case "image/gif":
      return gifAnimated(bytes);
    case "image/png":
      return pngAnimated(bytes);
    case "image/webp":
      return webpAnimated(bytes);
    default:
      return false;
  }
}

/** One decoded frame: drawn, then let go. */
type Frame = { image: CanvasImageSource & { close(): void }; ms: number };

/** An animated image's frames, decoded one at a time as they are drawn. */
export type Frames = {
  count: number;
  /** How many times it plays again after the first; `Infinity` for for ever. */
  loops: number;
  frame(index: number): Promise<Frame>;
  close(): void;
};

/**
 * How long a frame stays, in milliseconds, from its duration in microseconds. A frame of 10 ms
 * or less stays 100 ms, as every browser has drawn a GIF that asked for no delay.
 */
export function frameDelay(microseconds: number | null): number {
  const ms = (microseconds ?? 0) / 1000;
  return ms <= 10 ? 100 : ms;
}

/**
 * **An animated image's frames**, through WebCodecs' `ImageDecoder` from the bytes it is
 * handed: like `createImageBitmap`, it loads nothing. `undefined` where the webview has no
 * `ImageDecoder` (WebKit, so macOS and Linux, had none when this was written; WebView2 has it),
 * cannot decode this type, or finds one frame only. The caller closes what it is given.
 */
export async function openFrames(
  bytes: Uint8Array<ArrayBuffer>,
  mime: string,
): Promise<Frames | undefined> {
  if (typeof ImageDecoder !== "function") return undefined;
  let decoder: ImageDecoder;
  try {
    if (!(await ImageDecoder.isTypeSupported(mime))) return undefined;
    decoder = new ImageDecoder({ data: bytes, type: mime });
  } catch {
    return undefined;
  }
  try {
    await decoder.tracks.ready;
    await decoder.completed;
    const track = decoder.tracks.selectedTrack;
    if (track === null || !track.animated || track.frameCount < 2) {
      decoder.close();
      return undefined;
    }
    return {
      count: track.frameCount,
      loops: track.repetitionCount,
      frame: async (index) => {
        const { image } = await decoder.decode({ frameIndex: index });
        return { image, ms: frameDelay(image.duration) };
      },
      close: () => decoder.close(),
    };
  } catch {
    decoder.close();
    return undefined;
  }
}

/**
 * **Plays `frames` through `draw` while `playing`**, each frame for its own delay, as many times
 * as the image asks. Stopping keeps the frame it was on, and playing again goes on from there;
 * one that had played to its end starts again.
 */
export function usePlayback(
  frames: Frames | undefined,
  draw: (image: CanvasImageSource) => void,
  playing: boolean,
): void {
  const latest = useRef(draw);
  useEffect(() => {
    latest.current = draw;
  });
  const at = useRef({ index: 0, pass: 0 });
  useEffect(() => {
    if (frames === undefined || !playing) return;
    if (at.current.pass > frames.loops) at.current = { index: 0, pass: 0 };
    let gone = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const step = async () => {
      let shown: Frame;
      try {
        shown = await frames.frame(at.current.index);
      } catch {
        return;
      }
      if (gone) {
        shown.image.close();
        return;
      }
      latest.current(shown.image);
      shown.image.close();
      const next = at.current.index + 1;
      at.current =
        next < frames.count
          ? { index: next, pass: at.current.pass }
          : { index: 0, pass: at.current.pass + 1 };
      if (at.current.pass <= frames.loops) timer = setTimeout(() => void step(), shown.ms);
    };
    void step();
    return () => {
      gone = true;
      clearTimeout(timer);
    };
  }, [frames, playing]);
}

/**
 * Whether `element` can be seen: in sight (`IntersectionObserver`; a view tab put behind
 * another, or a region put away, is `hidden` and so out of it) and the window not hidden
 * (`document.hidden`). A webview without `IntersectionObserver` is taken as in sight.
 */
export function useInSight(element: RefObject<Element | null>): boolean {
  const [seen, setSeen] = useState(true);
  const [shown, setShown] = useState(() => typeof document === "undefined" || !document.hidden);
  useEffect(() => {
    const target = element.current;
    if (target === null || typeof IntersectionObserver !== "function") return;
    const watch = new IntersectionObserver((entries) => {
      const last = entries.at(-1);
      if (last !== undefined) setSeen(last.isIntersecting);
    });
    watch.observe(target);
    return () => watch.disconnect();
  }, [element]);
  useEffect(() => {
    const told = () => setShown(!document.hidden);
    document.addEventListener("visibilitychange", told);
    return () => document.removeEventListener("visibilitychange", told);
  }, []);
  return seen && shown;
}
