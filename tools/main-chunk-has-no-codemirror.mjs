// The window's main bundle carries no CodeMirror (#987, RC-5's review).
//
//   node tools/main-chunk-has-no-codemirror.mjs [dist]   exit 1 naming each chunk that does
//
// RC-5 loads the light editor and its grammars as lazy chunks, so a window that opens no file
// pays nothing for them (ADR 0086 rows M2 and L6). A static import added later would quietly
// pull the whole editor into what every window parses at start. This reads a built frontend
// (`app/dist` by default, which `npm run build` and `tauri build` leave) and fails when CodeMirror
// is in it.
//
// **What "the main bundle" is.** The entry chunk `index.html` loads, every chunk it preloads, and
// every chunk those import statically, followed to the end. A chunk reached only through
// `import(...)` is lazy and may hold CodeMirror; that is the point.
//
// **What "CodeMirror" is.** A module path under `@codemirror/`, or `cm-editor`, the class name
// CodeMirror's view gives its root element, which survives minification. So that the check cannot
// pass because the marker went stale, some lazy chunk must still carry `cm-editor`: a build with
// no CodeMirror anywhere fails too, and says to update this script.
import { readFileSync, readdirSync, realpathSync } from "node:fs";
import { join, posix } from "node:path";
import { fileURLToPath } from "node:url";

export const MARKERS = ["@codemirror/", "cm-editor"];

/** The chunk paths (relative to the build's root) that `index.html` loads at start. */
export function entriesOf(html) {
  const tags = html.match(/<(?:script|link)\b[^>]*>/g) ?? [];
  const paths = [];
  for (const tag of tags) {
    const module =
      /^<script\b/.test(tag) && /\btype=["']?module\b/.test(tag)
        ? /\bsrc=["']([^"']+)["']/.exec(tag)
        : /^<link\b/.test(tag) && /\brel=["']?modulepreload\b/.test(tag)
          ? /\bhref=["']([^"']+)["']/.exec(tag)
          : null;
    if (module) paths.push(module[1].replace(/^\.?\//, ""));
  }
  return paths;
}

/** The relative chunks `code` imports or re-exports statically; `import(...)` is left out. */
export function staticImportsOf(code) {
  const pattern =
    /\b(?:import|export)\s*(?:[^"'();]*?\bfrom\s*)?["'](\.{1,2}\/[^"']+?\.m?js)["']/g;
  return [...code.matchAll(pattern)].map((hit) => hit[1]);
}

/**
 * The verdict for a build: `chunks` maps each chunk's path (relative to the build's root, as
 * `index.html` names it) to its code. `failures` is empty when the main bundle holds none of
 * `MARKERS` and some other chunk holds CodeMirror.
 */
export function check({ html, chunks }) {
  const failures = [];
  const main = new Set();
  const queue = entriesOf(html);
  if (queue.length === 0)
    failures.push(
      "index.html loads no module script, so there is no entry chunk to check",
    );
  while (queue.length > 0) {
    const path = queue.shift();
    if (main.has(path)) continue;
    const code = chunks.get(path);
    if (code === undefined) {
      failures.push(
        `index.html's main bundle names ${path}, which the build does not have`,
      );
      continue;
    }
    main.add(path);
    for (const imported of staticImportsOf(code))
      queue.push(posix.normalize(posix.join(posix.dirname(path), imported)));
  }
  let carried = false;
  for (const path of main) {
    const found = MARKERS.filter((marker) => chunks.get(path).includes(marker));
    if (found.length > 0) {
      carried = true;
      failures.push(
        `${path} is in the window's main bundle and carries CodeMirror (${found.join(", ")}): load the editor through import(...), as Views.tsx does`,
      );
    }
  }
  const lazy = [...chunks].some(
    ([path, code]) => !main.has(path) && code.includes("cm-editor"),
  );
  // Where the main bundle already carries it, the editor moved there and the markers are fine.
  if (!lazy && !carried)
    failures.push(
      "no lazy chunk carries cm-editor, so this check can no longer see CodeMirror: update its markers",
    );
  return { failures, main: [...main] };
}

/** Every `.js` chunk under `dir`, keyed by its path relative to `dir`. */
function chunksIn(dir, under = "") {
  const chunks = new Map();
  for (const entry of readdirSync(join(dir, under), { withFileTypes: true })) {
    const path = under === "" ? entry.name : `${under}/${entry.name}`;
    if (entry.isDirectory())
      for (const [inner, code] of chunksIn(dir, path)) chunks.set(inner, code);
    else if (/\.m?js$/.test(entry.name))
      chunks.set(path, readFileSync(join(dir, path), "utf8"));
  }
  return chunks;
}

/** Whether this file is the script node was asked to run, however the path to it was spelled. */
function runAsScript() {
  try {
    return (
      realpathSync(process.argv[1]) ===
      realpathSync(fileURLToPath(import.meta.url))
    );
  } catch {
    return false;
  }
}

if (runAsScript()) {
  const dist =
    process.argv[2] ?? fileURLToPath(new URL("../app/dist", import.meta.url));
  const said = check({
    html: readFileSync(join(dist, "index.html"), "utf8"),
    chunks: chunksIn(dist),
  });
  for (const failure of said.failures) console.error(`error: ${failure}`);
  if (said.failures.length > 0) process.exit(1);
  console.log(
    `the window's main bundle (${said.main.join(", ")}) carries no CodeMirror`,
  );
}
