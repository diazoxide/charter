import { describe, expect, it } from "vitest";
import { animated, svgSide } from "./imagePreview";

const ascii = (text: string) => [...text].map((c) => c.charCodeAt(0));

/** A GIF of `frames` 1×1 images, each behind a graphic control extension. */
function gif(frames: number): Uint8Array {
  const image = [
    ...[0x21, 0xf9, 0x04, 0x00, 0x0a, 0x00, 0x00, 0x00],
    ...[0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0x00],
    ...[0x02, 0x02, 0x4c, 0x01, 0x00],
  ];
  const looping = [0x21, 0xff, 0x0b, ...ascii("NETSCAPE2.0"), 0x03, 0x01, 0x00, 0x00, 0x00];
  return new Uint8Array([
    ...ascii("GIF89a"),
    ...[1, 0, 1, 0, 0x80, 0, 0],
    ...[0, 0, 0, 0xff, 0xff, 0xff],
    ...(frames > 1 ? looping : []),
    ...Array.from({ length: frames }, () => image).flat(),
    0x3b,
  ]);
}

/** A PNG's chunk, its CRC left as zeros (only its order is read). */
const chunk = (type: string, data: number[]) => [
  ...[0, 0, 0, data.length],
  ...ascii(type),
  ...data,
  ...[0, 0, 0, 0],
];

function png(animation: boolean): Uint8Array {
  return new Uint8Array([
    ...[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
    ...chunk("IHDR", [0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]),
    ...(animation ? chunk("acTL", [0, 0, 0, 2, 0, 0, 0, 0]) : []),
    ...chunk("IDAT", [0x78, 0x9c]),
    ...chunk("IEND", []),
  ]);
}

function webp(flags: number): Uint8Array {
  return new Uint8Array([
    ...ascii("RIFF"),
    ...[22, 0, 0, 0],
    ...ascii("WEBP"),
    ...ascii("VP8X"),
    ...[10, 0, 0, 0],
    ...[flags, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  ]);
}

describe("whether an image is animated, from its bytes (#1132)", () => {
  it("counts a GIF's images", () => {
    expect(animated(gif(2), "image/gif")).toBe(true);
    expect(animated(gif(1), "image/gif")).toBe(false);
  });

  it("finds a PNG's animation control before its image data (APNG)", () => {
    expect(animated(png(true), "image/png")).toBe(true);
    expect(animated(png(false), "image/png")).toBe(false);
  });

  it("reads a WebP's animation flag", () => {
    expect(animated(webp(0x02), "image/webp")).toBe(true);
    expect(animated(webp(0x10), "image/webp")).toBe(false);
  });

  it("says a JPEG, and bytes cut short or made up, are still", () => {
    expect(animated(new Uint8Array([0xff, 0xd8, 0xff, 0xe0]), "image/jpeg")).toBe(false);
    expect(animated(gif(2).slice(0, 40), "image/gif")).toBe(false);
    expect(animated(new Uint8Array(ascii("GIF89a")), "image/gif")).toBe(false);
    expect(animated(png(true).slice(0, 20), "image/png")).toBe(false);
    expect(animated(new Uint8Array([0x89, 0x50]), "image/png")).toBe(false);
  });
});

describe("the size an SVG declares (#1132)", () => {
  it("reads the root's width and height, in pixels", () => {
    expect(svgSide('<svg width="24" height="16px">')).toEqual({ width: 24, height: 16 });
    expect(svgSide('<svg width="1in" height="2.54cm">')).toEqual({ width: 96, height: 96 });
  });

  it("takes the missing side from the view box's proportions", () => {
    expect(svgSide('<svg width="100" viewBox="0 0 50 25">')).toEqual({ width: 100, height: 50 });
  });

  it("is no size for a relative length, or none declared", () => {
    expect(svgSide('<svg width="100%" height="10">')).toBeUndefined();
    expect(svgSide('<svg viewBox="0 0 10 10">')).toBeUndefined();
    expect(svgSide("not an svg")).toBeUndefined();
  });

  it("reads a hostile file in linear time", () => {
    const started = performance.now();
    expect(svgSide(`<svg width="${"9".repeat(200_000)}>`)).toBeUndefined();
    expect(svgSide("<svg ".repeat(100_000))).toBeUndefined();
    expect(svgSide(`<svg width="1${" ".repeat(200_000)}x>`)).toBeUndefined();
    expect(svgSide(`<svg width="1" viewBox="${"1 ".repeat(100_000)}>`)).toBeUndefined();
    expect(performance.now() - started).toBeLessThan(1000);
  });

  it("does not take another attribute's width for the root's", () => {
    expect(svgSide('<svg stroke-width="9" width="2" height="3">')).toEqual({
      width: 2,
      height: 3,
    });
  });
});
