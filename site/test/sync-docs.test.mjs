import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { syncDocs } from "../scripts/sync-docs.mjs";

function tree(files) {
  const root = mkdtempSync(join(tmpdir(), "sync-docs-"));
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(join(root, path, ".."), { recursive: true });
    writeFileSync(join(root, path), text);
  }
  return root;
}

test("a page's first heading becomes its title and leaves the body", () => {
  const from = tree({ "spec.md": "# The spec: one app\n\nBody text.\n" });
  const to = mkdtempSync(join(tmpdir(), "sync-out-"));
  syncDocs({ from, to });
  assert.equal(
    readFileSync(join(to, "spec.md"), "utf8"),
    '---\ntitle: "The spec: one app"\n---\n\nBody text.\n',
  );
});

test("every page in every subdirectory is copied to the same path", () => {
  const from = tree({
    "a.md": "# A\n",
    "adr/0001-x.md": "# X\n",
    "agents/deep/y.md": "# Y\n",
  });
  const to = mkdtempSync(join(tmpdir(), "sync-out-"));
  syncDocs({ from, to });
  for (const path of ["a.md", "adr/0001-x.md", "agents/deep/y.md"]) {
    assert.ok(existsSync(join(to, path)), path);
  }
});

test("a page with no first heading is refused, naming the page", () => {
  const from = tree({ "notes/untitled.md": "Just text.\n" });
  const to = mkdtempSync(join(tmpdir(), "sync-out-"));
  assert.throws(() => syncDocs({ from, to }), /notes\/untitled\.md/);
});

test("a page that already has frontmatter keeps it", () => {
  const text = '---\ntitle: "Given"\n---\n\n# Ignored? no, kept\n';
  const from = tree({ "g.md": text });
  const to = mkdtempSync(join(tmpdir(), "sync-out-"));
  syncDocs({ from, to });
  assert.equal(readFileSync(join(to, "g.md"), "utf8"), text);
});

test("a page synced earlier and since deleted from docs/ is removed", () => {
  const to = mkdtempSync(join(tmpdir(), "sync-out-"));
  writeFileSync(join(to, "gone.md"), "---\ntitle: Gone\n---\n");
  syncDocs({ from: tree({ "kept.md": "# Kept\n" }), to });
  assert.ok(!existsSync(join(to, "gone.md")));
  assert.ok(existsSync(join(to, "kept.md")));
});
