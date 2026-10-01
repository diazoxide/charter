// @ts-check
// charter's website and its docs, one site (FR-5). Every page under `/docs/` is the
// repository's `docs/`, copied in by `scripts/sync-docs.mjs`; the landing page is the only page
// written here.
import { readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { defineConfig, passthroughImageService } from "astro/config";
import { unified } from "@astrojs/markdown-remark";
import starlight from "@astrojs/starlight";
import starlightLinksValidator from "starlight-links-validator";
import { remarkRepoLinks } from "./src/repo-links.mjs";
import { BASE, REPO_URL, SITE, pageSlug } from "./src/pages.mjs";

const docs = fileURLToPath(new URL("../docs", import.meta.url));

// The sidebar is read from `docs/` itself, so a page added there is in it without an edit here.
// Each directory is a group; the pages at the top of `docs/` are "Reference".
const LABELS = { "getting-started": "Getting started", adr: "Decisions (ADRs)", agents: "For agents" };
const ORDER = ["getting-started", "", "adr", "agents"];
const entries = readdirSync(docs, { withFileTypes: true });
const groups = [
  "",
  ...entries.filter((e) => e.isDirectory()).map((e) => e.name),
].sort((a, b) => (ORDER.indexOf(a) + 1 || 99) - (ORDER.indexOf(b) + 1 || 99) || a.localeCompare(b));
const sidebar = groups.map((dir) =>
  dir === ""
    ? {
        label: "Reference",
        items: entries
          .filter((e) => e.isFile() && e.name.endsWith(".md"))
          .map((e) => ({ slug: pageSlug(e.name) })),
      }
    : {
        label: LABELS[dir] ?? dir,
        collapsed: dir !== "getting-started",
        // A directory's slug is its index page's.
        items: [{ autogenerate: { directory: pageSlug(`${dir}/index.md`) } }],
      },
);

export default defineConfig({
  site: SITE,
  base: BASE,
  trailingSlash: "always",
  // The site has no images to optimise. The default service is sharp, whose prebuilt libvips
  // is LGPL-3.0; package.json's `overrides` keeps it out of the install altogether.
  image: { service: passthroughImageService() },
  markdown: {
    processor: unified({
      remarkPlugins: [
        [
          remarkRepoLinks,
          {
            contentRoot: fileURLToPath(new URL("./src/content/docs", import.meta.url)),
            repoRoot: fileURLToPath(new URL("..", import.meta.url)),
            ref: process.env.DOCS_REF ?? "main",
          },
        ],
      ],
    }),
  },
  integrations: [
    starlight({
      title: "charter",
      description: "One desktop app for running many coding-agent chats in parallel.",
      social: [{ icon: "github", label: "GitHub", href: REPO_URL }],
      sidebar,
      plugins: [starlightLinksValidator()],
    }),
  ],
});
