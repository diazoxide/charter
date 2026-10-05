// The one place that says where the site lives and where a page in `docs/` ends up on it.
// astro.config.mjs, the link rewrite and the reachability check all ask this module, so the
// three cannot disagree about an address.
import { readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { slug } from "github-slugger";

/** GitHub Pages serves a project repository's site under the repository's name. */
export const SITE = "https://purlis.github.io";
export const BASE = "/purlis";
export const REPO_URL = "https://github.com/purlis/purlis";

/** Every `*.md` file under `dir`, as a `/`-separated path relative to it, sorted. */
export function markdownPages(dir) {
  return readdirSync(dir, { withFileTypes: true, recursive: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".md"))
    .map((entry) => relative(dir, join(entry.parentPath, entry.name)).split("\\").join("/"))
    .sort();
}

/**
 * The collection id, and so the URL path under `BASE`, of `page` (a path inside `docs/`). It is
 * Astro's glob loader's own rule: the extension dropped, each segment through github-slugger, a
 * trailing `/index` dropped (`astro/dist/content/utils.js`, `getContentEntryIdAndSlug`).
 */
export function pageSlug(page) {
  const segments = `docs/${page}`.replace(/\.md$/, "").split("/").map((s) => slug(s));
  return segments.join("/").replace(/\/index$/, "");
}

/** The address a link to `page` uses on the site. */
export function pageHref(page) {
  return `${BASE}/${pageSlug(page)}/`;
}
