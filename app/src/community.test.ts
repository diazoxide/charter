import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * **The community and support channels** (FR-14, #609), as GitHub reads them from the
 * repository: the files its community profile and its "New issue" chooser look for, and the
 * rules they keep. The About dialog links to them (`About.tsx`), so a file that moved would be
 * a dead link in the app.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

const read = (path: string) => readFileSync(join(ROOT, path), "utf8");

/** The files a person reads before asking, contributing or reporting. */
const COMMUNITY = [
  "CODE_OF_CONDUCT.md",
  "CONTRIBUTING.md",
  "SUPPORT.md",
  ".github/PULL_REQUEST_TEMPLATE.md",
  ".github/ISSUE_TEMPLATE/config.yml",
  ".github/ISSUE_TEMPLATE/bug.yml",
  ".github/ISSUE_TEMPLATE/feature.yml",
  ".github/DISCUSSION_TEMPLATE/ideas.yml",
];

/** Every issue and discussion form, found rather than listed, so a new one keeps the rules. */
function forms(): string[] {
  const found: string[] = [];
  for (const dir of [".github/ISSUE_TEMPLATE", ".github/DISCUSSION_TEMPLATE"]) {
    if (!existsSync(join(ROOT, dir))) continue;
    for (const name of readdirSync(join(ROOT, dir))) {
      if (name.endsWith(".yml") && name !== "config.yml") found.push(`${dir}/${name}`);
    }
  }
  return found;
}

describe("the community files", () => {
  it("are where GitHub looks for them", () => {
    const missing = COMMUNITY.filter((path) => !existsSync(join(ROOT, path)));
    expect(missing).toEqual([]);
  });

  it("link only to files that exist", () => {
    // A Markdown link that is not a URL or an anchor is a path from the file's own directory.
    const dead: string[] = [];
    for (const path of COMMUNITY.filter((p) => p.endsWith(".md"))) {
      for (const [, target] of read(path).matchAll(/\]\(([^)\s]+)\)/g)) {
        if (/^(https?:|mailto:|#)/.test(target)) continue;
        const file = target.split("#")[0];
        if (!existsSync(join(ROOT, dirname(path), file))) dead.push(`${path} -> ${target}`);
      }
    }
    expect(dead).toEqual([]);
  });

  it('say "project", never "plane" (ADR 0072)', () => {
    const saying = [...COMMUNITY, ...forms()].filter((path) => /\bplanes?\b/i.test(read(path)));
    expect(saying).toEqual([]);
  });

  it("send security reports to SECURITY.md and never answer them in public", () => {
    expect(read("SUPPORT.md")).toContain("](SECURITY.md)");
    expect(read("CONTRIBUTING.md")).toContain("](SECURITY.md)");
    expect(read(".github/ISSUE_TEMPLATE/config.yml")).toContain(
      "https://github.com/diazoxide/charter/security/advisories/new",
    );
  });

  it("state how soon a question, a bug and a pull request get a first answer", () => {
    const support = read("SUPPORT.md");
    expect(support).toMatch(/## How soon you get an answer/);
    for (const channel of ["Discussions", "Bug reports", "Pull requests"]) {
      expect(support).toMatch(new RegExp(`\\| ${channel} \\| .*working days? \\|`));
    }
  });

  it("ask every commit to be signed off under the DCO", () => {
    const contributing = read("CONTRIBUTING.md");
    expect(contributing).toContain("https://developercertificate.org/");
    expect(contributing).toContain("git commit -s");
    expect(read(".github/PULL_REQUEST_TEMPLATE.md")).toMatch(/Signed-off-by/);
  });

  it("adopt the Contributor Covenant 2.1", () => {
    expect(read("CODE_OF_CONDUCT.md")).toContain(
      "https://www.contributor-covenant.org/version/2/1/code_of_conduct.html",
    );
  });
});

describe("the README", () => {
  it("links every community channel, so nobody has to know GitHub's file names", () => {
    const readme = read("README.md");
    for (const target of [
      "](SUPPORT.md)",
      "](CONTRIBUTING.md)",
      "](CODE_OF_CONDUCT.md)",
      "](SECURITY.md)",
      "](https://github.com/diazoxide/charter/discussions)",
    ]) {
      expect(readme).toContain(target);
    }
  });
});

describe("the issue chooser", () => {
  it("offers only its forms and its contact links, never a blank issue", () => {
    const config = read(".github/ISSUE_TEMPLATE/config.yml");
    expect(config).toMatch(/^blank_issues_enabled: false$/m);
    expect(config).toContain("https://github.com/diazoxide/charter/discussions");
  });

  it("gives every form a name, a description and a body, which GitHub needs to show it", () => {
    const incomplete = forms().filter((path) => {
      const form = read(path);
      const isIssue = path.includes("ISSUE_TEMPLATE");
      return (
        (isIssue && !/^name: \S/m.test(form)) ||
        (isIssue && !/^description: \S/m.test(form)) ||
        !/^body:$/m.test(form)
      );
    });
    expect(forms().length).toBeGreaterThan(0);
    expect(incomplete).toEqual([]);
  });
});
