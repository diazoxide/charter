/// <reference types="node" />
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import charterIcons from "./charter-icons.json";
import { fromSvg } from "./icons.vendor";

/**
 * **charter's own icons are the vendored files, converted** (FM-3, #1106): each symbol in
 * `charter-icons.json` is what `fromSvg` makes of the Material Icon Theme file of the same
 * name under `app/icons/material-icon-theme/`, so the provenance and the licence of every
 * shape the window draws is one directory listing away.
 *
 * To add an icon: copy its SVG from the same release into that directory, map a name to it in
 * `charter-icons.json`, and run this file with `CHARTER_ICONS_WRITE=1` to write its symbol.
 */

const VENDORED = join(process.cwd(), "icons/material-icon-theme");
const FILE = join(process.cwd(), "src/theme/charter-icons.json");

const vendored = Object.fromEntries(
  readdirSync(VENDORED)
    .filter((name) => name.endsWith(".svg"))
    .sort()
    .map((name) => [
      name.slice(0, -".svg".length),
      fromSvg(name.slice(0, -".svg".length), readFileSync(join(VENDORED, name), "utf8")),
    ]),
);

if (process.env.CHARTER_ICONS_WRITE === "1") {
  writeFileSync(FILE, `${JSON.stringify({ ...charterIcons, symbols: vendored }, null, 2)}\n`);
}

describe("charter's own icon theme", () => {
  it("is every vendored file converted, and nothing else", () => {
    expect(charterIcons.symbols).toEqual(vendored);
  });

  it("names only symbols it has, and uses every one it has", () => {
    const named = new Set<string>([
      charterIcons.file,
      charterIcons.folder,
      charterIcons.folderOpen,
      ...Object.values(charterIcons.extensions),
      ...Object.values(charterIcons.filenames),
      ...Object.values(charterIcons.folders),
      ...Object.values(charterIcons.foldersOpen),
    ]);
    expect([...named].sort()).toEqual(Object.keys(vendored).sort());
  });

  it("ships the set's licence beside it", () => {
    const licence = readFileSync(join(VENDORED, "LICENSE"), "utf8");
    expect(licence).toMatch(/^The MIT License/);
    expect(licence).toContain("Copyright (c) 2025 Material Extensions");
  });
});
