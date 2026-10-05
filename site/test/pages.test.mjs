import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pageSlug, pageHref, markdownPages } from "../src/pages.mjs";

// Expected values are what Astro's glob loader gives these entries (github-slugger per path
// segment, a trailing `/index` dropped), read off a real build of the site.
test("a page's slug is its path, each segment slugged the way Starlight slugs it", () => {
  assert.equal(pageSlug("spec.md"), "docs/spec");
  assert.equal(pageSlug("adr/0070-a-forge.md"), "docs/adr/0070-a-forge");
  assert.equal(pageSlug("v1.2-notes.md"), "docs/v12-notes");
  assert.equal(pageSlug("Getting Started/GitHub.md"), "docs/getting-started/github");
});

test("an index page is its directory's page", () => {
  assert.equal(pageSlug("guides/index.md"), "docs/guides");
  assert.equal(pageSlug("index.md"), "docs");
});

test("a page's address carries the site's base and a trailing slash", () => {
  assert.equal(pageHref("v1.2-notes.md"), "/purlis/docs/v12-notes/");
  assert.equal(pageHref("guides/index.md"), "/purlis/docs/guides/");
});

test("the pages of a docs tree are every .md file in it, by path, sorted", () => {
  const root = mkdtempSync(join(tmpdir(), "pages-"));
  for (const path of ["b.md", "adr/a.md", "notes.txt", "deep/er/c.md"]) {
    mkdirSync(join(root, path, ".."), { recursive: true });
    writeFileSync(join(root, path), "x");
  }
  assert.deepEqual(markdownPages(root), ["adr/a.md", "b.md", "deep/er/c.md"]);
});
