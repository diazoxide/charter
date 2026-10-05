// The rest of the tracker rename (RN-12b, #1274): comments, review comments, pull requests,
// labels and repo descriptions, in a dry run and an apply against a stand-in `gh`.
// Run with `node --test "tools/*.test.mjs"`.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, mkdtempSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { codeLabel, renameItem } from "./tracker-rename-rest.mjs";

const SCRIPT = join(dirname(fileURLToPath(import.meta.url)), "tracker-rename-rest.mjs");

// A stand-in `gh` over a state file of repos, each with a description, labels, issues (pull
// requests among them), comments and review comments. It answers the lists and single reads the
// script makes, applies PATCHes to the state, and logs every call.
// - FAKE_GH_FAIL_AFTER makes every PATCH after the nth fail, the way an interrupted run stops.
// - FAKE_GH_RATE_LIMIT_ONCE makes the first PATCH answer a rate limit, with a Retry-After header
//   of FAKE_GH_RETRY_AFTER seconds when that is set.
// - FAKE_GH_EDIT_AFTER_FIRST_PATCH="<repo> <comments|reviewComments> <id>" has someone edit that
//   comment right after the first PATCH lands, between the script's check and its write.
const FAKE_GH = `#!/usr/bin/env node
const fs = require("node:fs");
const args = process.argv.slice(2);
const state = JSON.parse(fs.readFileSync(process.env.FAKE_GH_STATE, "utf8"));
const log = (entry) => fs.appendFileSync(process.env.FAKE_GH_LOG, JSON.stringify(entry) + "\\n");
const method = args.includes("-X") ? args[args.indexOf("-X") + 1] : "GET";
if (args[1] === "graphql") {
  log({ method: "GRAPHQL", path: "graphql", input: null });
  const arg = (k) => args.find((a) => a.startsWith(k + "=")).slice(k.length + 1);
  const r = state[arg("owner") + "/" + arg("name")];
  for (const n of new Set((r.reviews ?? []).map((x) => x.pr))) process.stdout.write(n + "\\n");
  process.exit(0);
}
const path = args.find((a) => a.startsWith("repos/"));
let input = null;
if (args.includes("--input")) input = JSON.parse(fs.readFileSync(0, "utf8"));
log({ method, path, input });
const notFound = () => { process.stderr.write("HTTP 404: Not Found"); process.exit(1); };
const m = path.match(/^repos\\/([^/]+\\/[^/?]+)(?:\\/([^?]*))?/);
const repo = state[m[1]];
if (!repo) notFound();
const rest = m[2] ?? "";
const say = (x) => process.stdout.write(JSON.stringify(x) + "\\n");
const prs = () => repo.issues.filter((i) => i.pull_request);
let coll, one, view;
if (rest === "") { coll = null; one = repo; view = (r) => ({ description: r.description }); }
else if (rest === "labels") coll = repo.labels;
else if (rest.startsWith("labels/")) { const n = decodeURIComponent(rest.slice(7)); one = repo.labels.find((l) => l.name === n) ?? notFound(); }
else if (rest === "issues") coll = repo.issues;
else if (rest === "issues/comments") coll = repo.comments;
else if (rest.startsWith("issues/comments/")) one = repo.comments.find((c) => c.id === Number(rest.slice(16))) ?? notFound();
else if (rest === "pulls/comments") coll = repo.reviewComments;
else if (/^pulls\\/\\d+\\/reviews$/.test(rest)) coll = repo.reviews.filter((x) => x.pr === Number(rest.split("/")[1]));
else if (/^pulls\\/\\d+\\/reviews\\/\\d+$/.test(rest)) { const [, n, , id] = rest.split("/"); one = repo.reviews.find((x) => x.pr === Number(n) && x.id === Number(id)) ?? notFound(); }
else if (rest.startsWith("pulls/comments/")) one = repo.reviewComments.find((c) => c.id === Number(rest.slice(15))) ?? notFound();
else if (rest.startsWith("pulls/")) { one = prs().find((i) => i.number === Number(rest.slice(6))) ?? notFound(); view = (i) => ({ title: i.title, body: i.body }); }
else notFound();
if (method === "GET") {
  if (coll) for (const item of coll) say(item);
  else say(view ? view(one) : one);
  process.exit(0);
}
const patches = fs.readFileSync(process.env.FAKE_GH_LOG, "utf8").trim().split("\\n").map(JSON.parse).filter((e) => e.method === "PATCH" || e.method === "PUT").length;
if (process.env.FAKE_GH_RATE_LIMIT_ONCE && patches === 1) {
  if (process.env.FAKE_GH_RETRY_AFTER) process.stdout.write("HTTP/2.0 403 Forbidden\\nRetry-After: " + process.env.FAKE_GH_RETRY_AFTER + "\\n\\n{}");
  process.stderr.write("HTTP 403: You have exceeded a secondary rate limit.");
  process.exit(1);
}
if (process.env.FAKE_GH_FAIL_AFTER && patches > Number(process.env.FAKE_GH_FAIL_AFTER)) {
  process.stderr.write("connection reset");
  process.exit(1);
}
if (input.new_name !== undefined) { one.name = input.new_name; delete input.new_name; }
Object.assign(one, input);
if (process.env.FAKE_GH_EDIT_AFTER_FIRST_PATCH && patches === 1) {
  const [r, c, id] = process.env.FAKE_GH_EDIT_AFTER_FIRST_PATCH.split(" ");
  state[r][c].find((x) => x.id === Number(id)).body += "\\n\\nEdited mid-run by someone else.";
}
fs.writeFileSync(process.env.FAKE_GH_STATE, JSON.stringify(state, null, 2));
say(one);
`;

function tracker(state) {
  const dir = mkdtempSync(join(tmpdir(), "tracker-rename-rest-"));
  const gh = join(dir, "gh");
  writeFileSync(gh, FAKE_GH);
  chmodSync(gh, 0o755);
  const statePath = join(dir, "state.json");
  writeFileSync(statePath, JSON.stringify(state, null, 2));
  const logPath = join(dir, "calls.jsonl");
  writeFileSync(logPath, "");
  const run = (args, env = {}) =>
    spawnSync(
      process.execPath,
      [SCRIPT, "--gh", gh, "--delay-ms", "0", "--retry-wait-ms", "0", ...args],
      {
        encoding: "utf8",
        env: { ...process.env, FAKE_GH_STATE: statePath, FAKE_GH_LOG: logPath, ...env },
      },
    );
  const calls = () =>
    readFileSync(logPath, "utf8")
      .trim()
      .split("\n")
      .filter(Boolean)
      .map((l) => JSON.parse(l));
  const now = () => JSON.parse(readFileSync(statePath, "utf8"));
  return { dir, run, calls, now, statePath, logPath };
}

const issue = (number, title, body, extra = {}) => ({
  number,
  title,
  body,
  state: "closed",
  pull_request: false,
  milestone: null,
  ...extra,
});
const pr = (number, title, body, extra = {}) =>
  issue(number, title, body, { pull_request: true, ...extra });
const comment = (repo, id, on, body, author = "operator") => ({
  id,
  body,
  parent: `https://api.github.com/repos/${repo}/issues/${on}`,
  author,
});

const SAVE = "<!-- charter-save -->";
const BLOCK =
  "<!-- BEGIN charter change — GENERATED by `charter change push`; do not edit by hand. -->\n" +
  "charter changed 2 files\n" +
  "<!-- END charter change -->";

function aTracker() {
  return {
    "o/app": {
      description: "charter as one desktop app. The plane is diazoxide/charter-plane.",
      labels: [
        { name: "bug", description: "Something charter gets wrong" },
        { name: "via-charter-report", description: "Drafted by charter itself" },
        { name: "wontfix", description: "Will not be worked on" },
      ],
      issues: [
        issue(1, "An issue", "charter is RN-12's, not ours"),
        pr(2, "charter save: 1 file (dispatch 1)", `Saved by charter.\n\n${SAVE}\n\n${BLOCK}`, {
          state: "open",
        }),
        pr(3, "Unrelated", "Nothing."),
        issue(4, "RN-1", "`charter.toml` → `purlis.toml`", { milestone: "M60 · Rename to purlis" }),
        pr(5, "RN-2: rename charter crates", "body", { milestone: "M60 · Rename to purlis" }),
      ],
      comments: [
        comment("o/app", 101, 1, "Run `charter doctor` and check `.charter/`."),
        comment("o/app", 102, 1, "Nothing to see."),
        comment("o/app", 103, 2, `Pushed by charter.\n${SAVE}`, "someone"),
        comment("o/app", 104, 4, "charter.toml stays as written here"),
        comment("o/app", 105, 1, "```\n$ charter save\n```\ncharter saved it."),
      ],
      reviews: [
        { id: 301, pr: 2, body: "Looks right; charter keeps the marker.", author: "operator" },
        { id: 302, pr: 3, body: "", author: "operator" },
      ],
      reviewComments: [
        {
          id: 201,
          body: "Use CHARTER_ROOT here",
          parent: "https://api.github.com/repos/o/app/pulls/2",
          author: "operator",
        },
      ],
    },
    "o/plane": {
      description: "The charter project's own plane. The app is diazoxide/charter.",
      labels: [{ name: "gap", description: "A capability charter does not have yet" }],
      issues: [],
      comments: [],
      reviewComments: [],
    },
  };
}

function dryRun(t) {
  const out = join(t.dir, "plan.json");
  const r = t.run(["--repo", "o/app", "--repo", "o/plane", "--out", out]);
  assert.equal(r.status, 0, r.stderr);
  return { out, r, plan: JSON.parse(readFileSync(out, "utf8")) };
}

const patches = (t) => t.calls().filter((c) => c.method === "PATCH" || c.method === "PUT");

test("the dry run plans every kind of edit and writes nothing", () => {
  const t = tracker(aTracker());
  const before = readFileSync(t.statePath, "utf8");
  const { r, plan } = dryRun(t);
  assert.equal(readFileSync(t.statePath, "utf8"), before);
  assert.deepEqual(patches(t), []);
  assert.deepEqual(
    plan.items.map((i) => `${i.repo} ${i.kind} ${i.id}`),
    [
      "o/app repo o/app",
      "o/plane repo o/plane",
      "o/app label bug",
      "o/app label via-charter-report",
      "o/plane label gap",
      "o/app pr 2",
      "o/app comment 101",
      "o/app comment 103",
      "o/app comment 105",
      "o/app review-comment 201",
      "o/app review 301",
    ],
  );
  assert.match(r.stdout, /- name: via-charter-report\n\+ name: via-purlis-report/);
  assert.match(r.stdout, /\+ description: A capability purlis does not have yet/);
  assert.match(r.stdout, /\+ title: purlis save: 1 file \(dispatch 1\)/);
  assert.match(r.stdout, /o\/app comment 103 on #2 by someone/);
  assert.match(r.stdout, /items to edit: 11/);
  assert.doesNotMatch(r.stdout, /MISMATCH/);
});

test("a comment is renamed by RN-12's rules: its code blocks stay", () => {
  const t = tracker(aTracker());
  const { plan } = dryRun(t);
  const c = plan.items.find((i) => i.id === 105);
  assert.equal(c.after.body, "```\n$ charter save\n```\npurlis saved it.");
  assert.equal(
    plan.items.find((i) => i.id === 101).after.body,
    "Run `purlis doctor` and check `.purlis/`.",
  );
});

test("comments and pull requests on the rename's own milestone are left as written", () => {
  const t = tracker(aTracker());
  const { plan } = dryRun(t);
  assert.ok(!plan.items.some((i) => i.id === 104 || (i.kind === "pr" && i.id === 5)));
  assert.equal(plan.byKind.comment.skipped, 1);
  assert.equal(plan.byKind.pr.skipped, 1);
});

test("a pull request's markers stay byte for byte; its other text is renamed", () => {
  const t = tracker(aTracker());
  const { plan } = dryRun(t);
  const p = plan.items.find((i) => i.kind === "pr" && i.id === 2);
  assert.ok(p.after.body.includes(SAVE));
  assert.ok(p.after.body.includes(BLOCK.split("\n")[0]));
  assert.ok(p.after.body.includes("<!-- END charter change -->"));
  assert.match(p.after.body, /^Saved by purlis\./);
  assert.match(p.after.body, /\npurlis changed 2 files\n/);
  assert.equal(plan.byKind.pr.kept["html-comment"], 4);
  // A comment on a pull request keeps them too.
  assert.equal(plan.items.find((i) => i.id === 103).after.body, `Pushed by purlis.\n${SAVE}`);
});

test("an HTML comment in an issue's comment stays as written too (D-RN12b-9)", () => {
  const r = renameItem({ kind: "comment", onPr: false, fields: { body: "<!-- charter note -->" } });
  assert.equal(r.after.body, "<!-- charter note -->");
});

test("a label the code finds by name keeps it; its description is renamed", () => {
  assert.ok(codeLabel("bug"));
  assert.ok(codeLabel("ws:alpha"));
  assert.ok(codeLabel("charter::ws::alpha"));
  assert.equal(codeLabel("via-charter-report"), null);
  const r = renameItem({
    kind: "label",
    fields: { name: "charter::ws::alpha", description: "charter's workspace" },
  });
  assert.equal(r.after.name, "charter::ws::alpha");
  assert.equal(r.after.description, "purlis's workspace");
  assert.equal(r.kept["code-label"], 1);
  const k = renameItem(
    { kind: "label", fields: { name: "via-charter-report", description: null } },
    { keepLabels: ["via-charter-report"] },
  );
  assert.equal(k.after.name, "via-charter-report");
});

test("a repo description names the moved repos by their new slugs", () => {
  const t = tracker(aTracker());
  const { plan } = dryRun(t);
  const desc = (repo) =>
    plan.items.find((i) => i.kind === "repo" && i.id === repo).after.description;
  assert.equal(desc("o/plane"), "The purlis project's own plane. The app is purlis/purlis.");
  assert.equal(desc("o/app"), "purlis as one desktop app. The plane is purlis/purlis-plane.");
  // Everywhere else the old slug stays as history.
  assert.equal(
    renameItem({ kind: "comment", fields: { body: "see diazoxide/charter#3" } }).after.body,
    "see diazoxide/charter#3",
  );
});

test("apply makes exactly the dry run's edits, sending only the fields that change", () => {
  const t = tracker(aTracker());
  const { out } = dryRun(t);
  const r = t.run(["--apply", "--from", out]);
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /applied: 11 edited, 0 already done/);
  const sent = Object.fromEntries(patches(t).map((c) => [c.path, c.input]));
  assert.deepEqual(sent["repos/o/app/labels/via-charter-report"], {
    new_name: "via-purlis-report",
    description: "Drafted by purlis itself",
  });
  assert.deepEqual(sent["repos/o/app/labels/bug"], { description: "Something purlis gets wrong" });
  assert.deepEqual(sent["repos/o/plane/labels/gap"], {
    description: "A capability purlis does not have yet",
  });
  assert.deepEqual(Object.keys(sent["repos/o/app/pulls/2"]), ["title", "body"]);
  assert.deepEqual(Object.keys(sent["repos/o/app/issues/comments/101"]), ["body"]);
  assert.deepEqual(Object.keys(sent["repos/o/app/pulls/comments/201"]), ["body"]);
  assert.deepEqual(sent["repos/o/app/pulls/2/reviews/301"], {
    body: "Looks right; purlis keeps the marker.",
  });
  assert.ok(
    patches(t).some((c) => c.method === "PUT" && c.path === "repos/o/app/pulls/2/reviews/301"),
    "a review's text is edited with PUT",
  );
  assert.deepEqual(sent["repos/o/plane"], {
    description: "The purlis project's own plane. The app is purlis/purlis.",
  });
  const now = t.now();
  assert.deepEqual(
    now["o/app"].labels.map((l) => l.name),
    ["bug", "via-purlis-report", "wontfix"],
  );
  assert.ok(now["o/app"].issues[1].body.includes(SAVE));
  // A second run finds it all done and writes nothing.
  const again = t.run(["--apply", "--from", out]);
  assert.equal(again.status, 0, again.stderr);
  assert.match(again.stdout, /applied: 0 edited, 11 already done/);
});

test("apply refuses, writing nothing, when an item changed since the dry run", () => {
  const t = tracker(aTracker());
  const { out } = dryRun(t);
  const s = t.now();
  s["o/app"].comments[0].body += " (edited)";
  writeFileSync(t.statePath, JSON.stringify(s));
  const r = t.run(["--apply", "--from", out]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /refused: 1 item\(s\) changed.*comment 101/);
  assert.deepEqual(patches(t), []);
});

test("apply refuses a planned item that has gone, or a label whose new name is taken", () => {
  const t = tracker(aTracker());
  const { out } = dryRun(t);
  const s = t.now();
  s["o/app"].comments = s["o/app"].comments.filter((c) => c.id !== 105);
  s["o/app"].labels.push({ name: "via-purlis-report", description: "someone made it" });
  writeFileSync(t.statePath, JSON.stringify(s));
  const r = t.run(["--apply", "--from", out]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /comment 105 on #1 by operator gone/);
  assert.match(r.stderr, /via-charter-report" changed \(a label "via-purlis-report" exists\)/);
  assert.deepEqual(patches(t), []);
});

test("an interrupted apply is finished by running it again, a renamed label included", () => {
  const t = tracker(aTracker());
  const { out } = dryRun(t);
  const first = t.run(["--apply", "--from", out], { FAKE_GH_FAIL_AFTER: "4" });
  assert.notEqual(first.status, 0);
  assert.equal(t.now()["o/app"].labels[1].name, "via-purlis-report");
  const again = t.run(["--apply", "--from", out]);
  assert.equal(again.status, 0, again.stderr);
  assert.match(again.stdout, /applied: 7 edited, 4 already done/);
});

test("an item edited between the check and its write is not written, and the run stops", () => {
  const t = tracker(aTracker());
  const { out } = dryRun(t);
  const r = t.run(["--apply", "--from", out], {
    FAKE_GH_EDIT_AFTER_FIRST_PATCH: "o/app comments 103",
  });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /stopped: o\/app comment 103 .*changed since the dry run/);
  assert.ok(!patches(t).some((c) => c.path.endsWith("/103")));
});

test("a rate limit's Retry-After is honoured, and the edit tried again", () => {
  const t = tracker(aTracker());
  const { out } = dryRun(t);
  const r = t.run(["--apply", "--from", out], {
    FAKE_GH_RATE_LIMIT_ONCE: "1",
    FAKE_GH_RETRY_AFTER: "1",
  });
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stderr, /waiting 1s, then trying again/);
  assert.match(r.stdout, /applied: 11 edited/);
});

test("apply needs a saved dry run of this script, and --delay-ms paces the writes", () => {
  const t = tracker(aTracker());
  assert.match(t.run(["--apply"]).stderr, /pass --from/);
  const other = join(t.dir, "other.json");
  writeFileSync(other, JSON.stringify({ tool: "tracker-rename", version: 1, items: [] }));
  assert.match(t.run(["--apply", "--from", other]).stderr, /not a tracker-rename-rest dry run/);
  const { out } = dryRun(t);
  const started = Date.now();
  const r = spawnSync(
    process.execPath,
    [SCRIPT, "--gh", join(t.dir, "gh"), "--delay-ms", "150", "--apply", "--from", out],
    {
      encoding: "utf8",
      env: { ...process.env, FAKE_GH_STATE: t.statePath, FAKE_GH_LOG: t.logPath },
    },
  );
  assert.equal(r.status, 0, r.stderr);
  assert.ok(Date.now() - started >= 10 * 150, "ten pauses between eleven writes");
});

test("every occurrence is accounted for, per kind", () => {
  const t = tracker(aTracker());
  const { plan } = dryRun(t);
  const sum = (c) => Object.values(c).reduce((s, v) => s + v, 0);
  for (const [kind, c] of Object.entries(plan.byKind)) {
    assert.equal(c.found, sum(c.renamed) + sum(c.kept), kind);
  }
  assert.equal(plan.byKind.label.found, 4);
  assert.ok(existsSync(t.statePath));
});

test("reviews are listed only for the pull requests that have any", () => {
  const t = tracker(aTracker());
  dryRun(t);
  const asked = t
    .calls()
    .map((c) => c.path)
    .filter((p) => /\/reviews/.test(p));
  assert.deepEqual(asked, [
    "repos/o/app/pulls/2/reviews?per_page=100",
    "repos/o/app/pulls/3/reviews?per_page=100",
  ]);
});

test("a review changed between the dry run and the apply refuses the run", () => {
  const t = tracker(aTracker());
  const { out } = dryRun(t);
  const s = t.now();
  s["o/app"].reviews[0].body += " (edited)";
  writeFileSync(t.statePath, JSON.stringify(s));
  const r = t.run(["--apply", "--from", out]);
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /review 301 on #2 by operator changed/);
  assert.deepEqual(patches(t), []);
});

test("a line where two old names would read the same is kept and listed (D-RN12b-6)", () => {
  const r = renameItem({
    kind: "comment",
    fields: { body: "The app repo is charter-app, the product charter.\ncharter saves." },
  });
  assert.equal(r.after.body, "The app repo is charter-app, the product charter.\npurlis saves.");
  assert.equal(r.kept.collapse, 2);
  assert.deepEqual(r.collapsed, [
    { field: "body", line: "The app repo is charter-app, the product charter." },
  ]);
});
