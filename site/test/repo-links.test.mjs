import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { siteLink } from "../src/repo-links.mjs";

const repoRoot = mkdtempSync(join(tmpdir(), "repo-links-"));
for (const path of [
  "docs/spec.md",
  "docs/adr/0070-a-forge.md",
  "crates/purlis-core/src/curation.rs",
  "CONTRIBUTING.md",
  "docs/v1.2-notes.md",
  "docs/guides/index.md",
]) {
  mkdirSync(join(repoRoot, path, ".."), { recursive: true });
  writeFileSync(join(repoRoot, path), "x");
}

const at = (page) => ({ page, repoRoot, ref: "v1.2.3" });

test("a link to another page in docs/ goes to that page on the site, anchor kept", () => {
  assert.equal(
    siteLink("adr/0070-a-forge.md#the-decision", at("docs/spec.md")),
    "/purlis/docs/adr/0070-a-forge/#the-decision",
  );
  assert.equal(siteLink("../spec.md", at("docs/adr/0070-a-forge.md")), "/purlis/docs/spec/");
});

test("a link to a page goes to the address Starlight gives that page", () => {
  assert.equal(siteLink("v1.2-notes.md#x", at("docs/spec.md")), "/purlis/docs/v12-notes/#x");
  assert.equal(siteLink("guides/index.md", at("docs/spec.md")), "/purlis/docs/guides/");
});

test("a link to a file outside docs/ goes to that file on the forge at the built ref", () => {
  assert.equal(
    siteLink("../../crates/purlis-core/src/curation.rs", at("docs/adr/0070-a-forge.md")),
    "https://github.com/purlis/purlis/blob/v1.2.3/crates/purlis-core/src/curation.rs",
  );
  assert.equal(
    siteLink("../CONTRIBUTING.md", at("docs/spec.md")),
    "https://github.com/purlis/purlis/blob/v1.2.3/CONTRIBUTING.md",
  );
});

test("a link to a directory goes to its tree on the forge", () => {
  assert.equal(
    siteLink("adr/", at("docs/spec.md")),
    "https://github.com/purlis/purlis/tree/v1.2.3/docs/adr",
  );
});

test("a link to a path that does not exist fails the build, naming page and target", () => {
  assert.throws(() => siteLink("adr/0099-gone.md", at("docs/spec.md")), /docs\/spec\.md.*adr\/0099-gone\.md/);
  assert.throws(() => siteLink("../../../outside.md", at("docs/spec.md")), /docs\/spec\.md/);
});

test("a link to the directory above the repository is refused, not sent to the forge", () => {
  assert.throws(() => siteLink("../..", at("docs/spec.md")), /docs\/spec\.md.*does not exist/);
  assert.throws(() => siteLink("../../", at("docs/spec.md")), /docs\/spec\.md.*does not exist/);
});

test("web addresses and same-page anchors are left as written", () => {
  for (const target of ["https://example.org/a.md", "mailto:x@example.org", "#the-decision"]) {
    assert.equal(siteLink(target, at("docs/spec.md")), target);
  }
});
