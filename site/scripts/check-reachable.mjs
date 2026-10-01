// After `astro build`: every page in the repository's `docs/` has a built page, and the sidebar
// links to it. starlight-links-validator proves that the links which exist lead somewhere; this
// proves that no page in `docs/` is left with no way to it.
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { markdownPages, pageHref, pageSlug } from "../src/pages.mjs";

function sidebarLinks(html) {
  const nav = /<nav\b[^>]*aria-label="Main"[^>]*>([\s\S]*?)<\/nav>/.exec(html);
  return new Set([...(nav?.[1] ?? "").matchAll(/href="([^"]+)"/g)].map((m) => m[1]));
}

/** What is wrong, one line per page in `docs` that a reader of `dist` cannot reach. */
export function unreachable({ docs, dist }) {
  const all = markdownPages(docs);
  const built = (page) => join(pageSlug(page), "index.html");
  const first = all.map(built).find((path) => existsSync(join(dist, path)));
  const links = first ? sidebarLinks(readFileSync(join(dist, first), "utf8")) : new Set();
  const problems = [];
  for (const page of all) {
    const html = built(page).split("\\").join("/");
    if (!existsSync(join(dist, html))) problems.push(`${page}: no page was built at ${html}`);
    else if (!links.has(pageHref(page))) {
      problems.push(`${page}: the sidebar does not link to ${pageHref(page)}`);
    }
  }
  return problems;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const site = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const problems = unreachable({ docs: resolve(site, "../docs"), dist: resolve(site, "dist") });
  for (const line of problems) console.error(line);
  if (problems.length) process.exit(1);
  console.log("every page in docs/ is built and in the sidebar");
}
