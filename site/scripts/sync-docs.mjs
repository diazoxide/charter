// Copies the repository's `docs/` into the site's content collection, so that `docs/` stays
// the one source of every page and is read the same on GitHub and on the site.
//
// Starlight needs a `title` in each page's frontmatter; the pages in `docs/` carry theirs as
// their first `# ` heading instead, which is what GitHub shows. The copy moves that heading into
// the frontmatter. Links are not touched here: `src/repo-links.mjs` rewrites them while the
// site is built, where it sees the parsed page rather than its text.
import { readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { markdownPages } from "../src/pages.mjs";

function withTitle(text, page) {
  if (text.startsWith("---\n")) return text;
  const heading = /^# (.+)\n?/m.exec(text);
  if (!heading || text.slice(0, heading.index).trim() !== "") {
    throw new Error(`${page}: a page in docs/ must start with a "# " heading, its title`);
  }
  const body = text.slice(heading.index + heading[0].length);
  return `---\ntitle: ${JSON.stringify(heading[1].trim())}\n---\n${body}`;
}

/** Writes every `*.md` under `from` to the same path under `to`, which it empties first. */
export function syncDocs({ from, to }) {
  rmSync(to, { recursive: true, force: true });
  for (const page of markdownPages(from)) {
    const out = join(to, page);
    mkdirSync(dirname(out), { recursive: true });
    writeFileSync(out, withTitle(readFileSync(join(from, page), "utf8"), page));
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const site = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  syncDocs({ from: resolve(site, "../docs"), to: resolve(site, "src/content/docs/docs") });
}
