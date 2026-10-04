import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

// `sharp` is replaced by the local stand-in `no-sharp` (FR-5, #836), so the site installs no
// sharp at all. Advisory scanners (OSV-Scanner, and the OpenSSF Scorecard's Vulnerabilities
// check that runs it) read the lockfile, see a `sharp` with no version, and report every sharp
// advisory ever published. `osv-scanner.toml` skips that one package (#585). These tests keep
// the skip and the stand-in together: the skip must go the day real sharp comes back.
const site = new URL("..", import.meta.url);
const read = (path) => readFileSync(new URL(path, site), "utf8");

const sharpIsTheStandIn = () => {
  const pkg = JSON.parse(read("package.json"));
  const lock = JSON.parse(read("package-lock.json"));
  // npm installs a package once per place that asks for a different version, so a real sharp
  // can sit under another package (`node_modules/astro/node_modules/sharp`) while the top
  // level is still the stand-in. The `overrides` entry is what keeps every place on it.
  const nestedSharp = Object.keys(lock.packages ?? {}).filter(
    (key) => key !== "node_modules/sharp" && key.endsWith("node_modules/sharp"),
  );
  return (
    pkg.dependencies?.sharp === "file:no-sharp" &&
    pkg.overrides?.sharp === "$sharp" &&
    lock.packages?.["node_modules/sharp"]?.link === true &&
    lock.packages?.["node_modules/sharp"]?.resolved === "no-sharp" &&
    nestedSharp.length === 0
  );
};

// The `[[PackageOverrides]]` tables of an osv-scanner.toml, each as its `key = value` lines.
const packageOverrides = (toml) =>
  toml
    .split(/^\[\[PackageOverrides\]\]\s*$/m)
    .slice(1)
    .map((table) => table.split(/^\[/m)[0])
    .map((table) =>
      Object.fromEntries(
        table
          .split("\n")
          .map((line) => line.replace(/#.*$/, "").trim())
          .filter(Boolean)
          .map((line) => line.split(/\s*=\s*/, 2))
          .map(([key, value]) => [key, value.replace(/^"(.*)"$/, "$1")]),
      ),
    );

const skipsSharp = () =>
  existsSync(new URL("osv-scanner.toml", site)) &&
  packageOverrides(read("osv-scanner.toml")).some(
    (o) => o.name === "sharp" && o.ecosystem === "npm" && o.ignore === "true" && o.reason,
  );

test("sharp is the stand-in, so the site installs no sharp", () => {
  assert.ok(
    sharpIsTheStandIn(),
    "package.json overrides sharp with no-sharp, and the lockfile has no other sharp",
  );
});

test("the advisory scan skips sharp, with a reason, exactly while sharp is the stand-in", () => {
  assert.equal(skipsSharp(), sharpIsTheStandIn());
});
