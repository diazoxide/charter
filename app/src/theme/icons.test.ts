/// <reference types="node" />
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render } from "@testing-library/react";
import { createElement } from "react";
import { describe, expect, it } from "vitest";
import { FileIcon } from "../FileIcon";
import { DEFAULT_ICONS, ICON_TOKENS, iconFor, loadIcons, type IconTheme } from "./icons";
import { TOKENS } from "./theme";

/**
 * **The icon theme** (FM-3, #1106): which symbol a tree row is drawn as, that every colour is
 * a token, and that a contributed theme is data the loader re-emits — never markup.
 */

const file = (name: string) => iconFor(DEFAULT_ICONS, { name, folder: false }).name;
const folder = (name: string, open = false) =>
  iconFor(DEFAULT_ICONS, { name, folder: true, open }).name;

describe("which icon a row is drawn as", () => {
  it("finds a file by its extension", () => {
    expect(file("main.ts")).toBe("typescript");
    expect(file("App.tsx")).toBe("react_ts");
    expect(file("lib.rs")).toBe("rust");
    expect(file("photo.JPEG")).toBe("image");
    expect(file("bundle.tar.gz")).toBe("zip");
  });

  it("finds a file by its whole name before its extension, in any case", () => {
    expect(file("package.json")).toBe("nodejs");
    expect(file("README.md")).toBe("readme");
    expect(file("Makefile")).toBe("makefile");
    expect(file("Cargo.toml")).toBe("rust");
    expect(file(".gitignore")).toBe("git");
  });

  it("draws anything else as a plain file, and a dot file as its whole name", () => {
    expect(file("notes.unknown")).toBe("file");
    expect(file("LICENSE-THIRD-PARTY")).toBe("file");
    expect(file(".env")).toBe("tune");
    expect(file(".ts")).toBe("file");
  });

  it("finds a folder by its name, open or closed", () => {
    expect(folder("src")).toBe("folder-src");
    expect(folder("src", true)).toBe("folder-src-open");
    expect(folder(".github")).toBe("folder-github");
    expect(folder("anything")).toBe("folder");
    expect(folder("anything", true)).toBe("folder-open");
  });

  it("tries the longest extension first", () => {
    const { icons } = loadIcons({
      symbols: {
        ts: { viewBox: "0 0 16 16", paths: [{ d: "M0 0h16v16z", tone: "icon.blue" }] },
        def: { viewBox: "0 0 16 16", paths: [{ d: "M0 0h8v8z", tone: "icon.purple" }] },
      },
      extensions: { ts: "ts", "d.ts": "def" },
    });
    expect(iconFor(icons, { name: "types.d.ts", folder: false }).name).toBe("def");
    expect(iconFor(icons, { name: "main.ts", folder: false }).name).toBe("ts");
  });
});

describe("an icon is coloured by tokens only", () => {
  it("names only the colour theme's icon tokens", () => {
    expect(ICON_TOKENS.length).toBeGreaterThan(0);
    for (const token of ICON_TOKENS) expect(TOKENS).toContain(token);
    const symbols = [
      DEFAULT_ICONS.file,
      DEFAULT_ICONS.folder,
      DEFAULT_ICONS.folderOpen,
      ...DEFAULT_ICONS.extensions.values(),
      ...DEFAULT_ICONS.filenames.values(),
      ...DEFAULT_ICONS.folders.values(),
      ...DEFAULT_ICONS.foldersOpen.values(),
    ];
    for (const symbol of symbols)
      for (const shape of symbol.shapes) expect(ICON_TOKENS).toContain(shape.tone);
  });

  it("holds no colour written out in charter's own set", () => {
    const text = readFileSync(join(process.cwd(), "src/theme/charter-icons.json"), "utf8");
    expect(text).not.toMatch(/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/);
  });

  it("refuses a shape coloured anything but an icon token", () => {
    for (const tone of ["#ff0000", "red", "text.primary", "var(--icon-blue)", undefined]) {
      const { icons, complaints } = loadIcons({
        symbols: { x: { viewBox: "0 0 1 1", paths: [{ d: "M0 0h1z", tone }] } },
        file: "x",
      });
      expect(icons.file).toBe(DEFAULT_ICONS.file);
      expect(complaints.length).toBeGreaterThan(0);
    }
  });
});

describe("a contributed icon theme is data, never markup", () => {
  const one = (path: Record<string, unknown>, viewBox: unknown = "0 0 16 16") =>
    loadIcons({ symbols: { x: { viewBox, paths: [path] } }, file: "x" });

  it("refuses path data that is not commands and numbers", () => {
    for (const d of [
      'M0 0"/><script>alert(1)</script>',
      "M0 0 url(https://example.com/x)",
      "javascript:alert(1)",
      "<path d='M0 0'/>",
      "",
    ]) {
      const { icons, complaints } = one({ d, tone: "icon.blue" });
      expect(icons.file).toBe(DEFAULT_ICONS.file);
      expect(complaints[0]).toBe("a path of x is not path data");
    }
  });

  it("refuses a view box that is not four numbers", () => {
    for (const viewBox of ["0 0 16", "0 0 16 16; x", "0 0 0 16", 16, "a b c d"]) {
      expect(one({ d: "M0 0h1z", tone: "icon.blue" }, viewBox).complaints[0]).toBe(
        "the symbol x has no view box of four numbers",
      );
    }
  });

  it("refuses a huge view box at once, without reading it number by number", () => {
    const viewBox = `0 0 1 ${"1".repeat(1_000_000)}x`;
    const started = performance.now();
    expect(one({ d: "M0 0h1z", tone: "icon.blue" }, viewBox).complaints[0]).toBe(
      "the symbol x has no view box of four numbers",
    );
    expect(performance.now() - started).toBeLessThan(250);
  });

  it("keeps nothing it was not asked to: no attribute, style or link of the theme's", () => {
    const { icons, complaints } = one({
      d: "M0 0h16v16z",
      tone: "icon.blue",
      onload: "alert(1)",
      style: "fill: url(https://example.com/x)",
      href: "https://example.com/x",
    });
    expect(complaints).toEqual([]);
    const { container } = render(createElement(FileIcon, { symbol: icons.file }));
    const svg = container.querySelector("svg");
    expect(svg?.outerHTML).toBe(
      '<svg class="node-icon file-icon" viewBox="0 0 16 16" aria-hidden="true" ' +
        'focusable="false" data-icon="x"><path d="M0 0h16v16z" data-tone="icon-blue"></path></svg>',
    );
  });

  it("reads no inherited name, so a file called __proto__ is a plain file", () => {
    const { icons } = loadIcons(JSON.parse('{"filenames": {"__proto__": "nothing"}}') as unknown);
    expect(iconFor(icons, { name: "__proto__", folder: false })).toBe(DEFAULT_ICONS.file);
    expect(iconFor(icons, { name: "constructor", folder: false })).toBe(DEFAULT_ICONS.file);
  });

  it("always answers with a complete theme", () => {
    for (const raw of [null, [], "x", 3, {}]) {
      const { icons } = loadIcons(raw);
      const complete: IconTheme = icons;
      expect(complete.file.shapes.length).toBeGreaterThan(0);
      expect(complete.folder.shapes.length).toBeGreaterThan(0);
    }
  });
});
