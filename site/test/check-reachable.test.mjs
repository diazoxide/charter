import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { unreachable } from "../scripts/check-reachable.mjs";

function tree(files) {
  const root = mkdtempSync(join(tmpdir(), "reachable-"));
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(join(root, path, ".."), { recursive: true });
    writeFileSync(join(root, path), text);
  }
  return root;
}

const sidebar = (...hrefs) =>
  `<html><nav class="sidebar print:hidden" aria-label="Main">${hrefs.map((h) => `<a href="${h}">x</a>`).join("")}</nav></html>`;

test("every page in docs/ built and in the sidebar is reachable", () => {
  const docs = tree({ "spec.md": "# S\n", "adr/0070-x.md": "# X\n" });
  const nav = sidebar("/charter/docs/spec/", "/charter/docs/adr/0070-x/");
  const dist = tree({ "docs/spec/index.html": nav, "docs/adr/0070-x/index.html": nav });
  assert.deepEqual(unreachable({ docs, dist }), []);
});

test("a page with no built page is unreachable", () => {
  const docs = tree({ "spec.md": "# S\n", "updating.md": "# U\n" });
  const nav = sidebar("/charter/docs/spec/", "/charter/docs/updating/");
  const dist = tree({ "docs/spec/index.html": nav });
  assert.deepEqual(unreachable({ docs, dist }), [
    "updating.md: no page was built at docs/updating/index.html",
  ]);
});

test("a built page no sidebar links to is unreachable", () => {
  const docs = tree({ "spec.md": "# S\n", "hidden.md": "# H\n" });
  const nav = sidebar("/charter/docs/spec/");
  const dist = tree({ "docs/spec/index.html": nav, "docs/hidden/index.html": nav });
  assert.deepEqual(unreachable({ docs, dist }), [
    "hidden.md: the sidebar does not link to /charter/docs/hidden/",
  ]);
});

test("a page is looked for at the address Starlight gives it", () => {
  const docs = tree({ "v1.2-notes.md": "# N\n", "guides/index.md": "# G\n" });
  const nav = sidebar("/charter/docs/v12-notes/", "/charter/docs/guides/");
  const dist = tree({ "docs/v12-notes/index.html": nav, "docs/guides/index.html": nav });
  assert.deepEqual(unreachable({ docs, dist }), []);
});
