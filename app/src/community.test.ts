import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { parse } from "yaml";
import { HELP } from "./About";

/**
 * **The community and support channels** (FR-14, #609), as GitHub reads them from the
 * repository: the files its community profile and its "New issue" chooser look for, and the
 * rules they keep. The About dialog links to them (`About.tsx`), so a file that moved would be
 * a dead link in the app.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

const read = (path: string) => readFileSync(join(ROOT, path), "utf8");

/** A file's words with each run of whitespace made one space, so rewrapping a line is free. */
const prose = (path: string) => read(path).replace(/\s+/g, " ");

/** The documents a person reads before asking, contributing or reporting. */
const DOCS = [
  "CODE_OF_CONDUCT.md",
  "CONTRIBUTING.md",
  "SUPPORT.md",
  ".github/PULL_REQUEST_TEMPLATE.md",
];

const ISSUE_FORMS = ".github/ISSUE_TEMPLATE";
const DISCUSSION_FORMS = ".github/DISCUSSION_TEMPLATE";
const CHOOSER = `${ISSUE_FORMS}/config.yml`;

/** Every form, found in the directories GitHub reads, so a new one keeps the rules unlisted. */
function forms(): string[] {
  return [ISSUE_FORMS, DISCUSSION_FORMS].flatMap((dir) =>
    existsSync(join(ROOT, dir))
      ? readdirSync(join(ROOT, dir))
          .filter((name) => name.endsWith(".yml") && `${dir}/${name}` !== CHOOSER)
          .map((name) => `${dir}/${name}`)
      : [],
  );
}

/** Everything this file holds to its rules: the documents, the chooser and every form. */
const community = () => [...DOCS, CHOOSER, ...forms()];

describe("the community files", () => {
  it("are where GitHub looks for them, with bug, feature, ticket, ADR and ideas forms", () => {
    const missing = DOCS.concat(CHOOSER).filter((path) => !existsSync(join(ROOT, path)));
    expect(missing).toEqual([]);
    expect(forms().sort()).toEqual(
      [
        `${ISSUE_FORMS}/bug.yml`,
        `${ISSUE_FORMS}/feature.yml`,
        `${ISSUE_FORMS}/ticket.yml`,
        `${ISSUE_FORMS}/adr.yml`,
        `${DISCUSSION_FORMS}/ideas.yml`,
      ].sort(),
    );
  });

  it("link only to files that exist", () => {
    // A Markdown link that is not a URL or an anchor is a path from the file's own directory.
    const dead: string[] = [];
    for (const path of DOCS) {
      for (const [, target] of read(path).matchAll(/\]\(([^)\s]+)\)/g)) {
        if (/^(https?:|mailto:|#)/.test(target)) continue;
        const file = target.split("#")[0];
        if (!existsSync(join(ROOT, dirname(path), file))) dead.push(`${path} -> ${target}`);
      }
    }
    expect(dead).toEqual([]);
  });

  it('say "project", never "plane" or "piece" (ADR 0072)', () => {
    // A label and a milestone the repository already has, which the ticket and ADR forms
    // must spell exactly as they are named there.
    const named = ["area:plane", "M40 · Plane knowledge & memory governance"];
    const words = (path: string) =>
      named.reduce((text, name) => text.split(name).join(""), read(path));
    const saying = community().filter((path) => /\b(planes?|pieces?)\b/i.test(words(path)));
    expect(saying).toEqual([]);
  });

  it("send security reports to SECURITY.md and never answer them in public", () => {
    expect(read("SUPPORT.md")).toContain("](SECURITY.md)");
    expect(read("CONTRIBUTING.md")).toContain("](SECURITY.md)");
  });

  it("state the first response we aim for on each public channel", () => {
    const support = read("SUPPORT.md");
    expect(support).toMatch(/## How soon we aim to answer/);
    const rows = Object.fromEntries(
      [...support.matchAll(/^\| ([A-Z][\w ]+) \| (within \d+ working days?) \|$/gm)].map(
        ([, channel, target]) => [channel, target],
      ),
    );
    expect(rows).toEqual({
      Discussions: "within 3 working days",
      "Bug reports": "within 3 working days",
      "Feature requests": "within 5 working days",
      "Pull requests": "within 5 working days",
    });
  });

  it("ask every outside contributor's commit to be signed off under the DCO", () => {
    const contributing = read("CONTRIBUTING.md");
    expect(contributing).toContain("https://developercertificate.org/");
    expect(contributing).toContain("git commit -s");
    expect(prose("CONTRIBUTING.md")).toContain(
      "Every commit in a pull request from a contributor outside the maintainers carries",
    );
    expect(read(".github/PULL_REQUEST_TEMPLATE.md")).toMatch(/Signed-off-by/);
  });

  it("exempt the maintainers' own pull requests from the sign-off, agents' commits included", () => {
    // The operator's ruling V32b: the DCO is for outside contributors. A pull request a
    // maintainer opens needs no sign-off, and neither do the commits their agents make in it.
    expect(prose("CONTRIBUTING.md")).toContain(
      "**Maintainers are exempt.** A pull request opened by a maintainer (someone with write " +
        "access to this repository) needs no sign-off, and that includes the commits the " +
        "maintainer's coding agents make in it.",
    );
    expect(prose(".github/PULL_REQUEST_TEMPLATE.md")).toContain(
      "per the DCO in CONTRIBUTING.md (not needed when a maintainer opens the pull request).",
    );
  });

  it("keep the Contributor Covenant 2.1 word for word, but for where a report goes", () => {
    // The hash is of the Covenant's own text (EthicalSource/contributor_covenant,
    // content/version/2/1/code_of_conduct.md, without its front matter), with its contact
    // placeholder. Putting charter's contact back to the placeholder must give that text.
    const contact = "privately, as [SUPPORT.md](SUPPORT.md#reporting-a-conduct-problem) describes.";
    const coc = read("CODE_OF_CONDUCT.md");
    expect(coc).toContain(contact);
    const covenant = coc.replace(contact, "at [INSERT CONTACT METHOD].");
    expect(createHash("sha256").update(covenant).digest("hex")).toBe(
      "369bf7301883368fc19203bd0f1233fed2b83f0378ad19c4d0708bf61925339b",
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
      `](${HELP.discussions})`,
      `](${HELP.newIssue})`,
    ]) {
      expect(readme).toContain(target);
    }
  });
});

describe("the links About Charter draws", () => {
  it("are the repository's Discussions, its issue chooser and its SUPPORT.md", () => {
    expect(HELP).toEqual({
      discussions: "https://github.com/purlis/purlis/discussions",
      newIssue: "https://github.com/purlis/purlis/issues/new/choose",
      support: "https://github.com/purlis/purlis/blob/main/SUPPORT.md",
    });
    expect(existsSync(join(ROOT, "SUPPORT.md"))).toBe(true);
  });
});

/** The body element types GitHub's form schema accepts. */
const ELEMENTS = new Set(["markdown", "textarea", "input", "dropdown", "checkboxes"]);

type Element = {
  type?: string;
  id?: string;
  attributes?: { label?: string; value?: string; options?: unknown[] };
};

/** What is wrong with one parsed form, against GitHub's form schema; empty when nothing is. */
function schemaFaults(path: string, form: Record<string, unknown>): string[] {
  const faults: string[] = [];
  if (path.startsWith(ISSUE_FORMS)) {
    if (typeof form.name !== "string" || form.name === "") faults.push("no name");
    if (typeof form.description !== "string" || form.description === "")
      faults.push("no description");
  }
  const body = form.body;
  if (!Array.isArray(body) || body.length === 0) return [...faults, "no body"];
  const ids = new Set<string>();
  (body as Element[]).forEach((element, at) => {
    const where = `body[${at}]`;
    if (!ELEMENTS.has(element.type ?? "")) faults.push(`${where}: type ${element.type}`);
    if (element.type === "markdown") {
      if (!element.attributes?.value) faults.push(`${where}: markdown with no value`);
      return;
    }
    if (!element.attributes?.label) faults.push(`${where}: no label`);
    if (element.id !== undefined) {
      if (ids.has(element.id)) faults.push(`${where}: id ${element.id} twice`);
      ids.add(element.id);
    }
    if (element.type === "dropdown" || element.type === "checkboxes") {
      const options = element.attributes?.options;
      if (!Array.isArray(options) || options.length === 0) faults.push(`${where}: no options`);
    }
  });
  if (!(body as Element[]).some((element) => element.type !== "markdown"))
    faults.push("nothing to fill in");
  return faults;
}

describe("the issue chooser and the forms", () => {
  it("offers only its forms and its contact links, never a blank issue", () => {
    const config = parse(read(CHOOSER)) as {
      blank_issues_enabled: unknown;
      contact_links: { name: string; url: string; about: string }[];
    };
    expect(config.blank_issues_enabled).toBe(false);
    expect(config.contact_links.map((link) => link.url)).toEqual([
      "https://github.com/diazoxide/charter/discussions/categories/q-a",
      "https://github.com/diazoxide/charter/discussions/categories/ideas",
      "https://github.com/diazoxide/charter/security/advisories/new",
    ]);
    for (const link of config.contact_links) {
      expect(link.name, link.url).toBeTruthy();
      expect(link.about, link.url).toBeTruthy();
    }
  });

  it("parse as YAML and keep GitHub's form schema, which GitHub needs to show them", () => {
    const faults = forms().flatMap((path) =>
      schemaFaults(path, parse(read(path)) as Record<string, unknown>).map(
        (fault) => `${path}: ${fault}`,
      ),
    );
    expect(faults).toEqual([]);
  });

  it("offer the systems charter runs on and no other", () => {
    // SUPPORT.md asks for "macOS or Linux"; nothing has been ported to Windows (README).
    const bug = parse(read(`${ISSUE_FORMS}/bug.yml`)) as { body: Element[] };
    const system = bug.body.find((element) => element.id === "system");
    expect(system?.attributes?.options).toEqual(["macOS", "Linux", "Other"]);
    expect(read("SUPPORT.md")).toContain("macOS or Linux");
  });
});

describe("the ticket and ADR forms (HY-18, #1097)", () => {
  type Form = { labels?: string[]; body: Element[] };
  const form = (name: string) => parse(read(`${ISSUE_FORMS}/${name}`)) as Form;
  const field = (f: Form, id: string) => f.body.find((element) => element.id === id);

  it("ask for what a filed ticket's body holds, in its order", () => {
    // The sections every ticket filed from the program map carries (V20).
    const labels = form("ticket.yml")
      .body.filter((element) => element.type !== "markdown")
      .map((element) => element.attributes?.label);
    expect(labels.slice(0, 6)).toEqual([
      "Outcome / what",
      "Acceptance",
      "Blocked by",
      "Blocks",
      "Source",
      "Decisions",
    ]);
  });

  it("ask for the milestone and for each label family HY-18 set up", () => {
    for (const name of ["ticket.yml", "adr.yml"]) {
      const f = form(name);
      for (const id of ["milestone", "area", "horizon", "tier", "forge", "concept"]) {
        expect(field(f, id)?.type, `${name} ${id}`).toBe("dropdown");
      }
      const milestones = field(f, "milestone")?.attributes?.options as string[];
      expect(
        milestones.every((m) => /^M\d+ · /.test(m)),
        name,
      ).toBe(true);
      for (const id of ["area", "horizon", "tier", "forge", "concept"]) {
        const options = field(f, id)?.attributes?.options as string[];
        expect(
          options.every((o) => o.startsWith(`${id}:`) || o === "none"),
          `${name} ${id}`,
        ).toBe(true);
      }
    }
    expect(field(form("ticket.yml"), "type")?.attributes?.options).toEqual([
      "type:feature",
      "type:chore",
      "type:test",
      "type:epic",
    ]);
  });

  it("file an ADR proposal as one, and leave a ticket's type to its field", () => {
    expect(form("adr.yml").labels).toEqual(["type:adr"]);
    expect(form("ticket.yml").labels ?? []).toEqual([]);
  });
});
