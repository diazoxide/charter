// The main-bundle CodeMirror check (#987). Run with `node --test tools/`.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import {
  check,
  entriesOf,
  staticImportsOf,
} from "./main-chunk-has-no-codemirror.mjs";

const HTML = `<!doctype html><html><head>
<script type="module" crossorigin src="/assets/index-a1.js"></script>
<link rel="modulepreload" crossorigin href="/assets/shared-b2.js">
<link rel="stylesheet" crossorigin href="/assets/index-c3.css">
</head><body><div id="root"></div></body></html>`;

const EDITOR = `var v=class{constructor(){this.dom.className="cm-editor"}};export{v as t};`;

const lazyEditor = () =>
  new Map([
    [
      "assets/index-a1.js",
      `import{t as e}from"./shared-b2.js";const p=()=>import("./PieceFiles-d4.js");e(p);`,
    ],
    ["assets/shared-b2.js", `function t(e){return e}export{t};`],
    ["assets/PieceFiles-d4.js", EDITOR],
  ]);

test("index.html's module script and its preloads are the entry", () => {
  assert.deepEqual(entriesOf(HTML), [
    "assets/index-a1.js",
    "assets/shared-b2.js",
  ]);
});

test("static imports and re-exports are followed, import(...) is not", () => {
  assert.deepEqual(
    staticImportsOf(
      `import{a as b}from"./one.js";import"./two.js";export{c}from"../three.js";import("./lazy.js");x.import("./not.js")`,
    ),
    ["./one.js", "./two.js", "../three.js"],
  );
});

test("a build whose editor is lazy passes", () => {
  const said = check({ html: HTML, chunks: lazyEditor() });
  assert.deepEqual(said.failures, []);
  assert.deepEqual(said.main, ["assets/index-a1.js", "assets/shared-b2.js"]);
});

test("CodeMirror in the entry chunk fails, naming the chunk", () => {
  const chunks = lazyEditor();
  chunks.set(
    "assets/index-a1.js",
    `${chunks.get("assets/index-a1.js")}${EDITOR}`,
  );
  const said = check({ html: HTML, chunks });
  assert.equal(said.failures.length, 1);
  assert.match(said.failures[0], /assets\/index-a1\.js .*cm-editor/);
});

test("CodeMirror reached through a static import of the entry fails too", () => {
  const chunks = lazyEditor();
  chunks.set("assets/shared-b2.js", `import"./editor-e5.js";export{};`);
  chunks.set(
    "assets/editor-e5.js",
    `// node_modules/@codemirror/view\n${EDITOR}`,
  );
  const said = check({ html: HTML, chunks });
  assert.equal(said.failures.length, 1);
  assert.match(
    said.failures[0],
    /assets\/editor-e5\.js .*@codemirror\/, cm-editor/,
  );
});

test("an editor moved whole into the entry fails once, not as a stale marker", () => {
  const chunks = lazyEditor();
  chunks.set(
    "assets/index-a1.js",
    `${chunks.get("assets/index-a1.js")}${EDITOR}`,
  );
  chunks.delete("assets/PieceFiles-d4.js");
  const said = check({ html: HTML, chunks });
  assert.equal(said.failures.length, 1, said.failures.join("\n"));
  assert.match(said.failures[0], /assets\/index-a1\.js .*cm-editor/);
});

test("a build with no CodeMirror anywhere fails, so a stale marker is never a pass", () => {
  const chunks = lazyEditor();
  chunks.set("assets/PieceFiles-d4.js", `export{};`);
  const said = check({ html: HTML, chunks });
  assert.equal(said.failures.length, 1);
  assert.match(said.failures[0], /no lazy chunk carries cm-editor/);
});

test("an index.html with no module script, or naming a missing chunk, fails", () => {
  assert.match(
    check({ html: "<html></html>", chunks: lazyEditor() }).failures[0],
    /no module script/,
  );
  const chunks = lazyEditor();
  chunks.delete("assets/shared-b2.js");
  assert.match(
    check({ html: HTML, chunks }).failures.join("\n"),
    /names assets\/shared-b2\.js, which the build does not have/,
  );
});

test("the script reads a build folder and exits 1 when the entry carries CodeMirror", () => {
  const script = fileURLToPath(
    new URL("./main-chunk-has-no-codemirror.mjs", import.meta.url),
  );
  const dist = mkdtempSync(join(tmpdir(), "main-chunk-"));
  mkdirSync(join(dist, "assets"));
  writeFileSync(join(dist, "index.html"), HTML);
  for (const [path, code] of lazyEditor())
    writeFileSync(join(dist, path), code);
  const passed = spawnSync(process.execPath, [script, dist], {
    encoding: "utf8",
  });
  assert.equal(passed.status, 0, passed.stderr + passed.stdout);
  writeFileSync(join(dist, "assets/index-a1.js"), EDITOR);
  const failed = spawnSync(process.execPath, [script, dist], {
    encoding: "utf8",
  });
  assert.equal(failed.status, 1, failed.stdout);
  assert.match(
    failed.stderr,
    /assets\/index-a1\.js is in the window's main bundle/,
  );
});
