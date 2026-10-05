#!/usr/bin/env node
// Renames the rest of the tracker text from charter to purlis (RN-12b, #1274; V93a, V93n):
// issue and pull request comments, reviews and review comments, pull request titles and bodies,
// label names and descriptions, and the repos' descriptions. Milestones and issues are
// tracker-rename.mjs's (RN-12); this script uses its rules, `renameText`, and works the same way.
// A line where two different old names would read the same afterwards is kept as written and
// listed (D-RN12b-6, in `renameText`):
//
//   node tools/tracker-rename-rest.mjs --repo purlis/purlis --repo purlis/purlis-plane \
//       --out tracker-dry-run-12b.json > tracker-dry-run-12b.txt
//     The dry run, and the default. It only reads: every intended edit as a diff, a summary by
//     kind and by rule, every occurrence accounted for, and the plan saved to --out.
//
//   node tools/tracker-rename-rest.mjs --apply --from tracker-dry-run-12b.json
//     Makes exactly the edits in that saved plan. It reads everything again first and refuses
//     the whole run, writing nothing, if any planned item changed or has gone. Just before each
//     write it reads that item once more, and stops if it changed in between. An item that
//     already reads as the plan's result is skipped, so an interrupted run is finished by running
//     it again. Each write sends only the fields that change. Writes are --delay-ms apart (7.5s
//     by default), and a rate limit is waited out as long as GitHub's Retry-After asks.
//
// What this script adds to the RN-12 rules, and nothing else:
// - (Now in the shared rules, for every kind: D-RN12b-9.) An HTML comment stays as written.
//   That keeps the markers charter recognises its own pull requests by, `<!-- charter-save -->`
//   (`is_ours`), and the `<!-- BEGIN charter change … -->` / `<!-- END charter change -->` block
//   that `charter change push` finds and replaces, byte for byte.
// - A label the code finds by its name keeps its name (CODE_LABELS); its description is renamed.
// - A repo description names the app repo by its new slug: `diazoxide/charter` is renamed there,
//   though it stays everywhere else as history (D-RN12b-3, the acceptance of #1274).
// Comments and pull requests on the rename's own milestone (M60) are left as written, as RN-12
// leaves its issues.
import { readFileSync, writeFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";

import {
  DEFAULT_SKIP_MILESTONES,
  diffLines,
  fmtCounts,
  ghRunner,
  lines,
  collapseLines,
  renameText,
} from "./tracker-rename.mjs";

/**
 * Labels the code of purlis/purlis and purlis/purlis-plane finds by name (grepped 2026-10-05).
 * None of them says charter today; they are here so a future one is never renamed under the code.
 * `--keep-label <name>` adds one.
 */
export const CODE_LABELS = [
  ["bug", ".github/ISSUE_TEMPLATE/bug.yml labels:"],
  ["enhancement", ".github/ISSUE_TEMPLATE/feature.yml labels:"],
  ["type:adr", ".github/ISSUE_TEMPLATE/adr.yml labels:"],
  ["dependencies", "Dependabot's default label (.github/dependabot.yml)"],
  ["github_actions", "Dependabot's default label (.github/dependabot.yml)"],
  [/^ws:/, "`ws todo promote` labels a GitHub issue ws:<name> (forge/backend.rs)"],
  [
    /^charter::ws::/,
    "`ws todo promote` labels a GitLab issue charter::ws::<name> (forge/backend.rs)",
  ],
];

// The repos' moves, renamed in a repo description only (D-RN12b-3).
const MOVED = [
  [/\bdiazoxide\/charter-plane\b/g, "purlis/purlis-plane"],
  [/\bdiazoxide\/charter(?:-app)?(?![\w-])/g, "purlis/purlis"],
];

// Each kind: how it is listed and read, which fields it has, and how it is written.
const KINDS = {
  repo: {
    fields: ["description"],
    path: (repo) => `repos/${repo}`,
    jq: "{description}",
  },
  label: {
    fields: ["name", "description"],
    path: (repo, id) => `repos/${repo}/labels/${encodeURIComponent(id)}`,
    jq: "{name, description}",
    // GitHub renames a label by `new_name`.
    patchField: (f) => (f === "name" ? "new_name" : f),
  },
  pr: {
    fields: ["title", "body"],
    path: (repo, id) => `repos/${repo}/pulls/${id}`,
    jq: "{title, body}",
  },
  comment: {
    fields: ["body"],
    path: (repo, id) => `repos/${repo}/issues/comments/${id}`,
    jq: "{body}",
  },
  "review-comment": {
    fields: ["body"],
    path: (repo, id) => `repos/${repo}/pulls/comments/${id}`,
    jq: "{body}",
  },
  // A review's own text, the one it was submitted with. GitHub edits it with PUT.
  review: {
    fields: ["body"],
    path: (repo, id, on) => `repos/${repo}/pulls/${on}/reviews/${id}`,
    jq: "{body}",
    method: "PUT",
  },
};
const ORDER = Object.keys(KINDS);

const pick = (obj, fields) => Object.fromEntries(fields.map((f) => [f, obj[f] ?? null]));

/** Everything this script renames in `repo`, as `{ kind, id, fields, ... }`. */
async function readRest(call, repo) {
  const list = async (path, jq) =>
    lines(await call(["api", "--paginate", `repos/${repo}${path}`, "--jq", `.[] | ${jq}`]));
  const issues = await list(
    "/issues?state=all&per_page=100",
    "{number, title, body, state, pull_request: (.pull_request != null), milestone: .milestone.title}",
  );
  const parent = new Map(issues.map((i) => [i.number, i]));
  const out = [];
  const [desc] = lines(await call(["api", `repos/${repo}`, "--jq", KINDS.repo.jq]));
  out.push({ kind: "repo", id: repo, fields: pick(desc, KINDS.repo.fields) });
  for (const l of await list("/labels?per_page=100", KINDS.label.jq))
    out.push({ kind: "label", id: l.name, fields: pick(l, KINDS.label.fields) });
  for (const i of issues.filter((i) => i.pull_request))
    out.push({
      kind: "pr",
      id: i.number,
      state: i.state,
      milestone: i.milestone ?? null,
      fields: pick(i, KINDS.pr.fields),
    });
  const tail = (url) => Number(String(url).split("/").pop());
  const comments = await list(
    "/issues/comments?per_page=100",
    "{id, body, parent: .issue_url, author: .user.login}",
  );
  for (const c of comments) {
    const on = parent.get(tail(c.parent));
    out.push({
      kind: "comment",
      id: c.id,
      on: tail(c.parent),
      onPr: Boolean(on?.pull_request),
      milestone: on?.milestone ?? null,
      author: c.author ?? null,
      fields: pick(c, KINDS.comment.fields),
    });
  }
  const reviews = await list(
    "/pulls/comments?per_page=100",
    "{id, body, parent: .pull_request_url, author: .user.login}",
  );
  for (const c of reviews) {
    const on = parent.get(tail(c.parent));
    out.push({
      kind: "review-comment",
      id: c.id,
      on: tail(c.parent),
      onPr: true,
      milestone: on?.milestone ?? null,
      author: c.author ?? null,
      fields: pick(c, KINDS["review-comment"].fields),
    });
  }
  // Reviews are listed per pull request, so only the pull requests that have any are asked: one
  // GraphQL listing finds them (a paginated read, 100 pull requests a page).
  const reviewed = lines(
    await call([
      "api",
      "graphql",
      "--paginate",
      "-f",
      `owner=${repo.split("/")[0]}`,
      "-f",
      `name=${repo.split("/")[1]}`,
      "-f",
      `query=${REVIEWED}`,
      "--jq",
      ".data.repository.pullRequests.nodes[] | select(.reviews.totalCount > 0) | .number",
    ]),
  );
  for (const n of reviewed) {
    const on = parent.get(n);
    for (const r of await list(
      `/pulls/${n}/reviews?per_page=100`,
      "{id, body, author: .user.login}",
    ))
      out.push({
        kind: "review",
        id: r.id,
        on: n,
        onPr: true,
        milestone: on?.milestone ?? null,
        author: r.author ?? null,
        fields: pick(r, KINDS.review.fields),
      });
  }
  return out;
}

const REVIEWED =
  "query($owner: String!, $name: String!, $endCursor: String) { repository(owner: $owner, name: $name) " +
  "{ pullRequests(first: 100, after: $endCursor) { pageInfo { hasNextPage endCursor } " +
  "nodes { number reviews { totalCount } } } } }";

const occurrences = (text) => text?.match(/charter/gi)?.length ?? 0;

const add = (into, from) => {
  for (const [k, v] of Object.entries(from)) into[k] = (into[k] ?? 0) + v;
};

/** Why a label name is one the code finds it by, or null. */
export function codeLabel(name, extra = []) {
  for (const [match, why] of [...CODE_LABELS, ...extra.map((n) => [n, "--keep-label"])]) {
    if (typeof match === "string" ? match === name : match.test(name)) return why;
  }
  return null;
}

/** The renamed fields of one item, with the counts. */
export function renameItem(item, { keepLabels = [] } = {}) {
  const renamed = {};
  const kept = {};
  const words = {};
  const collapsed = [];
  const after = {};
  for (const [field, text] of Object.entries(item.fields)) {
    if (item.kind === "label" && field === "name" && codeLabel(text, keepLabels)) {
      if (occurrences(text)) add(kept, { "code-label": occurrences(text) });
      after[field] = text;
      continue;
    }
    let source = text;
    if (item.kind === "repo" && source != null) {
      for (const [re, to] of MOVED) {
        source = source.replace(re, () => {
          add(renamed, { "repo-moved": 1 });
          return to;
        });
      }
    }
    const r = renameText(source);
    add(renamed, r.renamed);
    add(kept, r.kept);
    add(words, r.words);
    for (const line of r.collapsed) collapsed.push({ field, line });
    after[field] = r.text;
  }
  return { after, renamed, kept, words, collapsed };
}

const emptyCounts = () => ({
  scanned: 0,
  skipped: 0,
  items: 0,
  found: 0,
  renamed: {},
  kept: {},
});

/** The plan: every item the rules would change, with its fields before and after. */
export async function plan(call, repos, { skipMilestones, keepLabels = [] }) {
  const items = [];
  const byKind = Object.fromEntries(ORDER.map((k) => [k, emptyCounts()]));
  const words = {};
  const codeLabels = [];
  const collapsed = [];
  for (const repo of repos) {
    for (const item of await readRest(call, repo)) {
      const c = byKind[item.kind];
      c.scanned++;
      if (item.milestone && skipMilestones.includes(item.milestone)) {
        c.skipped++;
        continue;
      }
      if (item.kind === "label") {
        const why = codeLabel(item.id, keepLabels);
        if (why) codeLabels.push({ repo, name: item.id, why });
      }
      const r = renameItem(item, { keepLabels });
      for (const t of Object.values(item.fields)) c.found += occurrences(t);
      add(c.renamed, r.renamed);
      add(c.kept, r.kept);
      add(words, r.words);
      for (const x of r.collapsed) collapsed.push({ repo, kind: item.kind, id: item.id, ...x });
      if (Object.keys(item.fields).every((f) => item.fields[f] === r.after[f])) continue;
      c.items++;
      items.push({
        repo,
        kind: item.kind,
        id: item.id,
        ...(item.state ? { state: item.state } : {}),
        ...(item.on != null ? { on: item.on } : {}),
        ...(item.author ? { author: item.author } : {}),
        before: item.fields,
        after: r.after,
        rules: r.renamed,
      });
    }
  }
  items.sort(
    (a, b) =>
      ORDER.indexOf(a.kind) - ORDER.indexOf(b.kind) ||
      a.repo.localeCompare(b.repo) ||
      String(a.id).localeCompare(String(b.id), undefined, { numeric: true }),
  );
  return {
    tool: "tracker-rename-rest",
    version: 1,
    repos,
    skipMilestones,
    keepLabels,
    codeLabels,
    collapsed,
    items,
    byKind,
    words,
  };
}

const sum = (counts) => Object.values(counts).reduce((s, v) => s + v, 0);

const where = (it) =>
  it.kind === "repo"
    ? `${it.repo} repo description`
    : it.kind === "label"
      ? `${it.repo} label ${JSON.stringify(it.id)}`
      : it.kind === "pr"
        ? `${it.repo} pr #${it.id} (${it.state})`
        : `${it.repo} ${it.kind} ${it.id} on #${it.on}${it.author ? ` by ${it.author}` : ""}`;

export function report(p) {
  const out = [];
  for (const it of p.items) {
    const rules = Object.entries(it.rules)
      .map(([k, v]) => `${k} ${v}`)
      .join(", ");
    out.push(`== ${where(it)} [${rules}]`);
    for (const f of Object.keys(it.before)) {
      if (it.before[f] === it.after[f]) continue;
      if (f === "body") out.push(...diffLines(f, it.before[f], it.after[f]));
      else out.push(`- ${f}: ${it.before[f]}`, `+ ${f}: ${it.after[f]}`);
    }
    out.push("");
  }
  out.push("== summary", `repos: ${p.repos.join(", ")}`);
  let found = 0;
  let edits = 0;
  let leftAlone = 0;
  for (const kind of ORDER) {
    const c = p.byKind[kind];
    const e = sum(c.renamed);
    const k = sum(c.kept);
    found += c.found;
    edits += e;
    leftAlone += k;
    out.push(
      `-- ${kind}: ${c.scanned} scanned, ${c.skipped} skipped (milestones ${p.skipMilestones.join(", ")}), ` +
        `${c.items} to edit, ${e} edits`,
      "   edits by rule:",
      ...fmtCounts(c.renamed).map((l) => `  ${l}`),
      "   left alone on purpose (occurrences of charter):",
      ...fmtCounts(c.kept).map((l) => `  ${l}`),
      `   occurrences: ${c.found} = ${e} edited + ${k} left alone` +
        (c.found === e + k ? "" : `  MISMATCH: ${c.found - e - k} unaccounted`),
    );
  }
  const authors = {};
  for (const it of p.items) if (it.author) add(authors, { [it.author]: 1 });
  out.push(
    `items to edit: ${p.items.length}`,
    "comments to edit, by author:",
    ...fmtCounts(authors),
    "labels the code finds by name (their names are never renamed):",
    ...p.codeLabels.map((l) => `  ${l.repo} ${l.name}: ${l.why}`),
    "words containing charter, left alone (contains-charter above):",
    ...fmtCounts(p.words),
    ...collapseLines(p.collapsed),
    `occurrences of charter in the items scanned (skipped ones aside): ${found}` +
      ` = ${edits} edited + ${leftAlone} left alone` +
      (found === edits + leftAlone ? "" : `  MISMATCH: ${found - edits - leftAlone} unaccounted`),
  );
  return out.join("\n") + "\n";
}

const same = (now, side) =>
  now != null && Object.keys(side).every((f) => (now[f] ?? null) === (side[f] ?? null));

/** Where a planned item stands now: "done", "todo" or why it cannot be written. */
function standing(it, lookup) {
  if (it.kind === "label" && it.before.name !== it.after.name) {
    const old = lookup(it.before.name);
    const renamed = lookup(it.after.name);
    if (!old && same(renamed, it.after)) return "done";
    if (same(old, it.before) && !renamed) return "todo";
    if (!old && !renamed) return "gone";
    if (renamed) return `changed (a label ${JSON.stringify(it.after.name)} exists)`;
    return "changed";
  }
  const now = lookup(it.id);
  if (same(now, it.after)) return "done";
  if (same(now, it.before)) return "todo";
  return now ? "changed" : "gone";
}

/** One item as it reads now, or null when it has gone. */
async function readOne(call, repo, kind, id, on) {
  try {
    const out = await call(["api", KINDS[kind].path(repo, id, on), "--jq", KINDS[kind].jq]);
    return pick(lines(out)[0], KINDS[kind].fields);
  } catch (e) {
    if (/HTTP 404|HTTP 410/.test(e.message)) return null;
    throw e;
  }
}

/**
 * Makes the plan's edits. Refuses before writing anything if a planned item changed since the dry
 * run or has gone; and reads each item again just before its write, stopping the run if it
 * changed in between.
 */
export async function apply(call, p, { delayMs, say }) {
  const current = new Map();
  for (const repo of new Set(p.items.map((i) => i.repo))) {
    for (const item of await readRest(call, repo))
      current.set(`${repo} ${item.kind} ${item.id}`, item.fields);
  }
  const todo = [];
  const changed = [];
  let done = 0;
  for (const it of p.items) {
    const s = standing(it, (id) => current.get(`${it.repo} ${it.kind} ${id}`));
    if (s === "done") done++;
    else if (s === "todo") todo.push(it);
    else changed.push(`${where(it)} ${s}`);
  }
  if (changed.length) {
    throw new Error(
      `refused: ${changed.length} item(s) changed since the dry run or gone, so nothing was written. ` +
        `Run the dry run again and review it: ${changed.join(", ")}`,
    );
  }
  say(`${done} already done, ${todo.length} to edit`);
  let n = 0;
  for (const it of todo) {
    const kind = KINDS[it.kind];
    const body = {};
    for (const f of kind.fields) {
      if (it.before[f] !== it.after[f]) body[kind.patchField?.(f) ?? f] = it.after[f];
    }
    if (n > 0 && delayMs > 0) await sleep(delayMs);
    // Someone may have edited it since the check above.
    const fresh = new Map();
    const lookup = async (id) => {
      if (!fresh.has(id)) fresh.set(id, await readOne(call, it.repo, it.kind, id, it.on));
      return fresh.get(id);
    };
    await lookup(it.id);
    if (it.kind === "label") await lookup(it.after.name);
    const s = standing(it, (id) => fresh.get(id));
    if (s === "done") {
      done++;
      say(`already done: ${where(it)}`);
      continue;
    }
    if (s !== "todo") {
      throw new Error(
        `stopped: ${where(it)} ${s} since the dry run, after ${n} edit(s). ` +
          `Nothing more was written. Run the dry run again and review it.`,
      );
    }
    await call(
      [
        "api",
        "-X",
        kind.method ?? "PATCH",
        kind.path(it.repo, it.id, it.on),
        "--include",
        "--input",
        "-",
      ],
      body,
    );
    n++;
    say(`edited ${where(it)} (${n}/${todo.length})`);
  }
  return { done, edited: n };
}

function parseArgs(argv) {
  const o = {
    repos: [],
    skipMilestones: [...DEFAULT_SKIP_MILESTONES],
    keepLabels: [],
    gh: "gh",
    delayMs: 7500,
    retryWaitMs: 60000,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const val = () => {
      if (i + 1 >= argv.length) throw new Error(`${a} needs a value`);
      return argv[++i];
    };
    if (a === "--repo") o.repos.push(val());
    else if (a === "--out") o.out = val();
    else if (a === "--apply") o.apply = true;
    else if (a === "--dry-run") o.dryRun = true;
    else if (a === "--from") o.from = val();
    else if (a === "--gh") o.gh = val();
    else if (a === "--delay-ms") o.delayMs = Number(val());
    else if (a === "--retry-wait-ms") o.retryWaitMs = Number(val());
    else if (a === "--skip-milestone") o.skipMilestones.push(val());
    else if (a === "--keep-label") o.keepLabels.push(val());
    else throw new Error(`unknown argument ${a}`);
  }
  if (o.apply && o.dryRun) throw new Error("--apply and --dry-run are opposites; pick one");
  if (o.apply && !o.from)
    throw new Error("--apply makes the edits of a saved dry run: pass --from <dry-run file>");
  if (o.apply && o.repos.length)
    throw new Error("--apply takes its repos from the dry-run file, not --repo");
  if (!o.apply && !o.repos.length) throw new Error("name at least one --repo owner/name");
  if (!o.apply && o.from) throw new Error("--from is for --apply");
  return o;
}

async function main(argv) {
  const o = parseArgs(argv);
  const warn = (line) => process.stderr.write(line + "\n");
  const call = ghRunner({ gh: o.gh, retryWaitMs: o.retryWaitMs, warn });
  if (o.apply) {
    const p = JSON.parse(readFileSync(o.from, "utf8"));
    if (p.tool !== "tracker-rename-rest" || p.version !== 1)
      throw new Error(`${o.from} is not a tracker-rename-rest dry run`);
    const r = await apply(call, p, { delayMs: o.delayMs, say: warn });
    process.stdout.write(`applied: ${r.edited} edited, ${r.done} already done\n`);
    return;
  }
  const p = await plan(call, o.repos, o);
  process.stdout.write(report(p));
  if (o.out) writeFileSync(o.out, JSON.stringify(p, null, 2) + "\n");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((e) => {
    process.stderr.write(`tracker-rename-rest: ${e.message}\n`);
    process.exit(1);
  });
}
