#!/usr/bin/env node
// Renames the tracker from charter to purlis (RN-12, #1256; operator rulings V93a, V93d, V93n).
//
// It edits milestone titles and descriptions, and issue titles and bodies, open and closed, in
// the repos it is given. Pull requests, comments, labels and repo descriptions are
// tracker-rename-rest.mjs's (RN-12b), with these same rules. It has two modes:
//
//   node tools/tracker-rename.mjs --repo purlis/purlis --repo purlis/purlis-plane \
//       --out tracker-dry-run.json > tracker-dry-run.txt
//     The dry run, and the default. It only reads: it prints every intended edit as a diff,
//     a summary and a count by rule, and saves the plan to --out.
//
//   node tools/tracker-rename.mjs --apply --from tracker-dry-run.json
//     Makes exactly the edits in that saved plan and nothing else. Before writing anything it
//     reads every planned item again, and it refuses the whole run if any of them has changed
//     since the dry run or has gone. Just before each write it reads that item once more, and
//     stops if it changed in between. An item that already reads as the plan's result is
//     skipped, so a run that was interrupted is finished by running it again. Writes are
//     --delay-ms apart (7.5s by default), and a rate limit is waited out, as long as GitHub's
//     Retry-After asks.
//
// The rules are `renameText` below. What they leave alone is as deliberate as what they change:
// fenced and indented code blocks (they quote output, logs and code as they were), URLs and
// 1Password references, the old repo slugs and bare `charter#12` references, the old repo names
// inside paths, existing branch names, versions released under the old name, old plugin ids,
// `Charter-*` commit trailers, the persona's own "charter" (the English word), the retired
// Python package and distribution, and words that merely contain "charter" (`charters`), which
// the summary counts so that every occurrence is accounted for. In prose the old repo names
// become the new ones (`charter-app#4` is `purlis#4`, D-RN12-9) and `charterd` is `purlisd`.
// Pull requests and the rename's own milestone (M60) are not touched here.
import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";

// The rename scope's own milestone. Its issues describe the old names on purpose
// ("`charter.toml` → `purlis.toml`"), so they are left as written.
export const DEFAULT_SKIP_MILESTONES = ["M60 · Rename to purlis"];

// Spans that stay as written, each with the name it is counted under. Order matters only for
// overlaps: the first rule to claim a character keeps it.
const KEEP = [
  // An HTML comment is a record something reads back, byte for byte (D-RN12b-9): the PR markers
  // `<!-- charter-save -->` and `<!-- BEGIN/END charter change -->`, the
  // `<!-- mutants-report dirty: […] -->` record tools/mutants-report.py parses, quoted fixtures.
  ["html-comment", /<!--[\s\S]*?-->/g],
  // Names the code or a vault still writes under the old name: the GitLab label `ws todo promote`
  // writes, and 1Password item titles (#1275).
  ["gitlab-label", /(?<![\w-])charter::ws::[\w<>.-]*/g],
  ["op-item", /\bop-item:\s*[`"']?charter-[\w<>.-]+/g],
  // The retired Python implementation, said as such ("the Python charter", "the old charter").
  ["retired-python", /\b(?:Python|old)\s+charter\b/gi],
  // A URL records where something was. `diazoxide/charter` redirects, and is never reused.
  ["url", /\b(?:https?|ftp):\/\/[^\s<>()[\]"'`]+/g],
  ["url", /\]\([^)\s]*\)/g],
  // A 1Password reference names an item that is still called what it was called.
  ["op-reference", /\bop:\/\/[^\s<>()[\]"'`]+/g],
  ["repo-slug", /\b(?:diazoxide|purlis)\/charter(?:-plane|-app)?(?![\w-])(?:#\d+)?/g],
  // A bare `charter#N` names the old repo's issue. (`charter-app#N` and `charter-plane#N` are
  // renamed to the repos' new names, D-RN12-9.)
  ["repo-slug", /(?<![\w./-])charter#\d+/g],
  // The old repo names inside a path stay (D-RN12-9); in prose they are renamed below.
  [
    "retired-repo-name",
    /(?<=\/)charter-(?:app|plane)(?![\w-])|(?<![\w.-])charter-(?:app|plane)(?=\/)/g,
  ],
  // The retired Python distribution, and the path uv installed it under.
  ["retired-python-dist", /[\w~./-]*uv\/tools\/charter-cp\/[\w./-]*/g],
  ["retired-python-dist", /(?<![\w.-])charter-cp(?![\w-])/g],
  // A branch that exists already keeps its name, as PR markers do (V93j).
  [
    "branch-ref",
    /\b(?:chore|fix|feat|feature|impl|docs|refactor|test|ci|build|perf|release|hotfix|bump)\/charter[\w./-]*/g,
  ],
  // A release made under the old name keeps it: there never was a purlis 0.54.0.
  ["old-version", /\bcharter\s+v?\d+\.\d+(?:\.\d+)?\b/gi],
  // Old plugin ids, which are pinned off by those names (ADR 0056's FORMERLY).
  [
    "old-plugin-id",
    /(?<![\w.-])charter(?:-app)?@[\w.-]+|(?<![\w.-])[\w-]+@charter(?:-app)?(?![\w-])/g,
  ],
  // The plugin's old name, `charter-app`, said as a plugin's name (#408: "the plugin is called
  // charter, not charter-app"; "the bundled `charter-app` plugin"). Short of a sentence's end.
  [
    "old-plugin-id",
    /(?<=\bplugins?\b[^.\n?!;:]{0,30})(?<![\w./@-])charter-app(?![\w@:/-])|(?<![\w./@-])charter-app(?=[`*]*\s+plugins?\b)/g,
  ],
  // `Charter-*` trailers are in git history for good and are recognised, never rewritten (V93j).
  ["commit-trailer", /(?<![\w-])Charter-(?:[A-Z][\w-]*|\*)?(?![a-z])/g],
  // The persona's charter is the English word: the role a persona plays, not the product.
  [
    "persona-charter",
    /\bthe\s+charter(?=\s+(?:body|prose|concatenation)\b|-format\b|\s*\(or\s+personas\/)/gi,
  ],
  [
    "persona-charter",
    /\b(?:persona(?:'s|s'|s)?|role(?:'s)?|(?:its|their|his|her|your|my|own))\s+(?:own\s+)?charters?\b(?!\.\w)/gi,
  ],
  // Sentences about the rename itself would otherwise read "rename purlis to purlis".
  ["rename-history", /\bcharter\s*(?:→|->|=>|to)\s*purlis\b|\bedm\s*(?:→|->|=>|to)\s*charter\b/gi],
  [
    "rename-history",
    /\b(?:formerly|previously|renamed from|was called|old name)\s+[`"']?charter\b/gi,
  ],
  // The retired Python implementation (tag cli-final): its files and modules never get new names.
  ["python-package", /(?<![\w./-])(?:\.\.\.\/|…\/)?charter\/[\w./-]*\.py(?::[\d-]+)?/g],
  ["python-package", /(?<![\w./-])charter\/(?:frame|harness|secrets)\//g],
  ["python-package", /\bfrom\s+charter\s+import\b|\bimport\s+charter\b|(?<![\w-])-m\s+charter\b/g],
  [
    "python-package",
    /(?<![\w./-])charter\.(?!(?:local\.)?toml\b|md\b|json\b|ya?ml\b|sock\b|log\b|txt\b|lock\b|exe\b|app\b)[a-z_]+(?:\.[a-z_]+)*/g,
  ],
  // A clone directory on someone's machine, named before the rename.
  ["clone-path", /(?:IdeaProjects|workspaces\/[\w.-]+)\/charter(?![\w-])/g],
];

// What is renamed. Each match is replaced and counted under the rule's name.
const RENAME = [
  // The app crate, named to cargo, becomes purlis-app (RN-13, V93c), not the repo (D-RN12b-4).
  ["crate", /(?<=(?:^|\s)(?:-p|--package)[\s=])charter-app(?![\w-])/g, () => "purlis-app"],
  // The repos' new names, in prose (D-RN12-9): charter-app was the app repo, now purlis/purlis.
  ["repo-name", /(?<![\w./@-])charter-app\s?#(\d+)/g, (_m, n) => `purlis#${n}`],
  ["repo-name", /(?<![\w./@-])charter-plane\s?#(\d+)/g, (_m, n) => `purlis-plane#${n}`],
  ["repo-name", /(?<![\w./@-])charter-app(?![\w/@-])/g, () => "purlis"],
  ["repo-name", /(?<![\w./@-])charter-plane(?![\w/@-])/g, () => "purlis-plane"],
  // The daemon (dispatcher ruling), with its socket.
  ["daemon", /(?<![A-Za-z0-9_])charterd(?![A-Za-z0-9_])/gi, () => "purlisd"],
  ["env-var", /(?<![A-Za-z0-9_])CHARTER_/g, () => "PURLIS_"],
  ["mcp-tool", /\bmcp__charter__/g, () => "mcp__purlis__"],
  [
    "crate",
    /(?<![\w./-])charter([-_])(core|cli|session[-_]protocol|same[-_]user|app[-_]lib|site)(?![\w-])/g,
    (_m, sep, rest) => `purlis${sep}${rest}`,
  ],
  ["file-name", /(?<![A-Za-z0-9_])\.(charter)(?![A-Za-z0-9_])/gi, (_m, w) => `.${caseOf(w)}`],
  ["file-name", /(?<![\w.-])charter(?=\.(?:(?:local\.)?toml|exe|app)\b)/g, () => "purlis"],
  ["skill-id", /(?<![\w.-])charter(?=:[a-z])/g, () => "purlis"],
  // `charter <word>`, a CLI form: in a code span or at a shell prompt.
  ["cli", /(?<=(?:^|`|\$ )\s*)charter(?= [a-z][a-z-]*)/gm, () => "purlis"],
  // The product name, always lowercase (V93n). Whole words only: `charters` and identifiers
  // such as `read_charter` stay as they are, counted under contains-charter.
  ["product-name", /(?<![A-Za-z0-9_])charter(?![A-Za-z0-9_])/gi, (m) => caseOf(m)],
];

function caseOf(word) {
  return word === word.toUpperCase() ? "PURLIS" : "purlis";
}

// A fence opens on a line of three or more backticks or tildes (also inside a list or a quote)
// and closes on a line of the same character at least as long. An unclosed fence runs to the end.
const FENCE = /^[ \t>]*(`{3,}|~{3,})/;
// An indented code block (D-RN12-1, as the review asked): lines indented four spaces or a tab,
// after a blank line, outside a list (where that indent is the list item's own text).
const INDENTED = /^(?: {4}|\t)/;
const LIST_ITEM = /^ {0,3}(?:[-*+]|\d+[.)])\s/;

/** The segments of `text`: `{ text, kind }` with kind prose, fenced or indented, joining back to `text`. */
function segments(text) {
  const out = [];
  const push = (kind, chunk) => {
    if (!chunk) return;
    const last = out[out.length - 1];
    if (last && last.kind === kind) last.text += chunk;
    else out.push({ kind, text: chunk });
  };
  let fence = null;
  let indented = false;
  let afterBlank = true;
  let inList = false;
  for (const line of text.split(/(?<=\n)/)) {
    const bare = line.replace(/\r?\n$/, "");
    const blank = bare.trim() === "";
    const m = bare.match(FENCE);
    if (fence !== null) {
      push("fenced", line);
      if (
        m &&
        m[1][0] === fence[0] &&
        m[1].length >= fence.length &&
        /^[ \t>]*[`~]+\s*$/.test(bare)
      ) {
        fence = null;
      }
      afterBlank = false;
      continue;
    }
    if (indented && (blank || INDENTED.test(bare))) {
      push("indented", line);
      afterBlank = blank;
      continue;
    }
    indented = false;
    if (m) {
      fence = m[1];
      push("fenced", line);
      afterBlank = false;
      continue;
    }
    if (!blank && !inList && afterBlank && INDENTED.test(bare)) {
      indented = true;
      push("indented", line);
      afterBlank = false;
      continue;
    }
    if (LIST_ITEM.test(bare)) inList = true;
    else if (!blank && !/^\s/.test(bare)) inList = false;
    push("prose", line);
    afterBlank = blank;
  }
  return out;
}

const bump = (counts, key, n = 1) => {
  if (n) counts[key] = (counts[key] ?? 0) + n;
};

function renameProse(text, renamed, kept, words, keep, collapsed) {
  const claimed = new Uint8Array(text.length);
  for (const [rule, re] of [...keep, ...KEEP]) {
    for (const m of text.matchAll(re)) {
      if (!/charter/i.test(m[0])) continue;
      const end = m.index + m[0].length;
      if (claimed.subarray(m.index, end).some((c) => c)) continue;
      claimed.fill(1, m.index, end);
      bump(kept, rule, m[0].match(/charter/gi).length);
    }
  }
  const edits = [];
  for (const [rule, re, to] of RENAME) {
    for (const m of text.matchAll(re)) {
      const end = m.index + m[0].length;
      if (claimed.subarray(m.index, end).some((c) => c)) continue;
      claimed.fill(1, m.index, end);
      edits.push({ at: m.index, end, to: to(...m), rule });
    }
  }
  // A line where two different old names would read the same afterwards ("charter, not
  // charter-app" as "purlis, not purlis") loses its meaning. It stays as written, counted as
  // `collapse` and listed, for a person to word by hand (D-RN12b-6).
  const lineOf = (at) => text.slice(0, at).split("\n").length - 1;
  const byLine = new Map();
  for (const e of edits) {
    // A command (`charter save`) names no product or repo, so it never collides (D-RN12b-10).
    if (e.rule === "cli") continue;
    const line = lineOf(e.at);
    if (!byLine.has(line)) byLine.set(line, new Map());
    const olds = byLine.get(line);
    if (!olds.has(e.to)) olds.set(e.to, new Set());
    olds.get(e.to).add(text.slice(e.at, e.end).toLowerCase().replace(/\s+/g, ""));
  }
  const dropped = new Set(
    [...byLine]
      .filter(([, olds]) => [...olds.values()].some((names) => names.size > 1))
      .map(([line]) => line),
  );
  if (dropped.size) {
    const all = text.split("\n");
    for (const line of dropped) collapsed.push(all[line]);
    for (let i = edits.length - 1; i >= 0; i--) {
      const e = edits[i];
      if (!dropped.has(lineOf(e.at))) continue;
      bump(kept, "collapse", text.slice(e.at, e.end).match(/charter/gi).length);
      edits.splice(i, 1);
    }
  }
  // Whatever no rule claimed is part of a longer word (`charters`, `_charter_argv`). It is left
  // alone, and counted, so every occurrence is accounted for.
  for (const m of text.matchAll(/[A-Za-z0-9_]*charter[A-Za-z0-9_]*/gi)) {
    for (const c of m[0].matchAll(/charter/gi)) {
      if (claimed[m.index + c.index]) continue;
      bump(kept, "contains-charter");
      bump(words, m[0]);
    }
  }
  edits.sort((a, b) => a.at - b.at);
  let out = "";
  let last = 0;
  for (const e of edits) {
    out += text.slice(last, e.at) + e.to;
    last = e.end;
    bump(renamed, e.rule);
  }
  return out + text.slice(last);
}

/**
 * Applies the rename rules to one title or body. Returns the new text, the edits counted by
 * rule (`renamed`), the occurrences left alone on purpose, counted by why (`kept`), and the
 * longer words containing "charter" that were left alone (`words`), and the lines kept because
 * two old names in them would have read the same (`collapsed`). `keep` adds `[rule, regex]`
 * spans to leave alone, claimed before the rules' own (tracker-rename-rest.mjs's PR markers).
 */
export function renameText(text, { keep = [] } = {}) {
  const renamed = {};
  const kept = {};
  const words = {};
  const collapsed = [];
  if (text == null) return { text, renamed, kept, words, collapsed };
  let out = "";
  for (const seg of segments(text)) {
    if (seg.kind === "prose") {
      out += renameProse(seg.text, renamed, kept, words, keep, collapsed);
    } else {
      bump(kept, `${seg.kind}-block`, seg.text.match(/charter/gi)?.length ?? 0);
      out += seg.text;
    }
  }
  return { text: out, renamed, kept, words, collapsed };
}

// ---- GitHub, through `gh` ----

const ISSUE =
  "{number, title, body, state, pull_request: (.pull_request != null), milestone: .milestone.title}";
const MILESTONE = "{number, title, description, state}";
const ISSUE_FIELDS = `.[] | ${ISSUE}`;
const MILESTONE_FIELDS = `.[] | ${MILESTONE}`;

/** How long GitHub asked us to wait, from the headers `--include` prints, in ms; or null. */
function askedToWait(output, now = Date.now()) {
  const after = output.match(/^retry-after:\s*(\d+)\s*$/im);
  if (after) return Number(after[1]) * 1000;
  const remaining = output.match(/^x-ratelimit-remaining:\s*(\d+)\s*$/im);
  const reset = output.match(/^x-ratelimit-reset:\s*(\d+)\s*$/im);
  if (remaining && Number(remaining[1]) === 0 && reset) {
    return Math.max(0, Number(reset[1]) * 1000 - now) + 1000;
  }
  return null;
}

export function ghRunner({ gh, retryWaitMs, warn }) {
  return async function call(args, input) {
    for (let attempt = 0; ; attempt++) {
      const r = spawnSync(gh, args, {
        encoding: "utf8",
        input: input == null ? undefined : JSON.stringify(input),
        maxBuffer: 1 << 30,
      });
      if (r.status === 0) return r.stdout;
      const why = `${r.stderr || r.error?.message || ""}`.trim();
      if (/rate limit|abuse|HTTP 429|HTTP 502|HTTP 503/i.test(why) && attempt < 6) {
        const wait = askedToWait(r.stdout ?? "") ?? retryWaitMs * 2 ** attempt;
        warn(
          `tracker-rename: ${why.split("\n")[0]}; waiting ${Math.round(wait / 1000)}s, then trying again`,
        );
        await sleep(wait);
        continue;
      }
      throw new Error(`gh ${args.join(" ")} failed: ${why}`);
    }
  };
}

export const lines = (out) =>
  out
    .split("\n")
    .filter((l) => l.trim())
    .map((l) => JSON.parse(l));

/** Every milestone and issue (not pull request) of `repo`, as `{ kind, number, title, body }`. */
async function readTracker(call, repo) {
  const milestones = lines(
    await call([
      "api",
      "--paginate",
      `repos/${repo}/milestones?state=all&per_page=100`,
      "--jq",
      MILESTONE_FIELDS,
    ]),
  ).map((m) => ({
    kind: "milestone",
    number: m.number,
    state: m.state,
    title: m.title,
    body: m.description ?? null,
  }));
  const issues = lines(
    await call([
      "api",
      "--paginate",
      `repos/${repo}/issues?state=all&per_page=100`,
      "--jq",
      ISSUE_FIELDS,
    ]),
  )
    .filter((i) => !i.pull_request)
    .map((i) => ({
      kind: "issue",
      number: i.number,
      state: i.state,
      title: i.title,
      body: i.body ?? null,
      milestone: i.milestone ?? null,
    }));
  milestones.sort((a, b) => a.number - b.number);
  issues.sort((a, b) => a.number - b.number);
  return [...milestones, ...issues];
}

const add = (into, from) => {
  for (const [k, v] of Object.entries(from)) into[k] = (into[k] ?? 0) + v;
};

/** One milestone or issue as it reads now, or null when it has gone. */
async function readOne(call, it) {
  const issue = it.kind === "issue";
  let out;
  try {
    out = await call(["api", patchPath(it), "--jq", issue ? ISSUE : MILESTONE]);
  } catch (e) {
    if (/HTTP 404|HTTP 410/.test(e.message)) return null;
    throw e;
  }
  const one = lines(out)[0];
  return { title: one.title, body: (issue ? one.body : one.description) ?? null };
}

const occurrences = (text) => text?.match(/charter/gi)?.length ?? 0;

/** The plan: every item the rules would change, with its text before and after. */
export async function plan(call, repos, skipMilestones) {
  const items = [];
  const renamed = {};
  const kept = {};
  const words = {};
  const collapsed = [];
  let found = 0;
  let scanned = 0;
  let skipped = 0;
  for (const repo of repos) {
    for (const item of await readTracker(call, repo)) {
      scanned++;
      const own = item.kind === "milestone" ? item.title : item.milestone;
      if (skipMilestones.includes(own)) {
        skipped++;
        continue;
      }
      const title = renameText(item.title);
      const body = renameText(item.body);
      found += occurrences(item.title) + occurrences(item.body);
      add(kept, title.kept);
      add(kept, body.kept);
      add(words, title.words);
      add(words, body.words);
      for (const [field, r] of [
        ["title", title],
        ["body", body],
      ])
        for (const line of r.collapsed)
          collapsed.push({ repo, kind: item.kind, number: item.number, field, line });
      add(renamed, title.renamed);
      add(renamed, body.renamed);
      if (title.text === item.title && body.text === item.body) continue;
      const rules = {};
      add(rules, title.renamed);
      add(rules, body.renamed);
      items.push({
        repo,
        kind: item.kind,
        number: item.number,
        state: item.state,
        before: { title: item.title, body: item.body },
        after: { title: title.text, body: body.text },
        rules,
      });
    }
  }
  return {
    tool: "tracker-rename",
    version: 1,
    repos,
    skipMilestones,
    scanned,
    skipped,
    items,
    renamed,
    kept,
    words,
    collapsed,
    found,
  };
}

export function diffLines(label, before, after) {
  if (before === after) return [];
  const a = (before ?? "").split("\n");
  const b = (after ?? "").split("\n");
  const out = [];
  // The rules never add or remove a line, so line n before is line n after.
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    if (a[i] === b[i]) continue;
    out.push(`@@ ${label} line ${i + 1}`, `- ${a[i] ?? ""}`, `+ ${b[i] ?? ""}`);
  }
  return out;
}

export const fmtCounts = (counts) =>
  Object.entries(counts)
    .sort((x, y) => y[1] - x[1])
    .map(([k, v]) => `  ${k.padEnd(20)} ${v}`);

/** The lines kept as written because two old names in them would read the same (collapse). */
export function collapseLines(list = []) {
  return [
    `lines kept because two old names would read the same (collapse): ${list.length}`,
    ...list.map(
      (c) => `  ${c.repo} ${c.kind} ${c.number ?? c.id} ${c.field}: ${c.line.trim().slice(0, 200)}`,
    ),
  ];
}

export function report(p) {
  const out = [];
  for (const it of p.items) {
    const rules = Object.entries(it.rules)
      .map(([k, v]) => `${k} ${v}`)
      .join(", ");
    out.push(`== ${it.repo} ${it.kind} #${it.number} (${it.state}) [${rules}]`);
    if (it.before.title !== it.after.title)
      out.push(`- title: ${it.before.title}`, `+ title: ${it.after.title}`);
    out.push(
      ...diffLines(it.kind === "milestone" ? "description" : "body", it.before.body, it.after.body),
    );
    out.push("");
  }
  const sum = (counts) => Object.values(counts).reduce((s, v) => s + v, 0);
  const edits = sum(p.renamed);
  const leftAlone = sum(p.kept);
  out.push(
    "== summary",
    `repos: ${p.repos.join(", ")}`,
    `items scanned: ${p.scanned} (pull requests not counted)`,
    `items skipped (milestones ${p.skipMilestones.join(", ")}): ${p.skipped}`,
    `items to edit: ${p.items.length}`,
    `  milestones: ${p.items.filter((i) => i.kind === "milestone").length}`,
    `  issues: ${p.items.filter((i) => i.kind === "issue").length}`,
    `edits: ${edits}`,
    "edits by rule:",
    ...fmtCounts(p.renamed),
    "left alone on purpose (occurrences of charter):",
    ...fmtCounts(p.kept),
    "words containing charter, left alone (contains-charter above):",
    ...fmtCounts(p.words),
    ...collapseLines(p.collapsed),
    `occurrences of charter in the items scanned (skipped ones aside): ${p.found}` +
      ` = ${edits} edited + ${leftAlone} left alone` +
      (p.found === edits + leftAlone
        ? ""
        : `  MISMATCH: ${p.found - edits - leftAlone} unaccounted`),
  );
  return out.join("\n") + "\n";
}

const sameAs = (now, side) => now != null && now.title === side.title && now.body === side.body;

const patchPath = (it) =>
  `repos/${it.repo}/${it.kind === "milestone" ? "milestones" : "issues"}/${it.number}`;

/**
 * Makes the plan's edits. Refuses before writing anything if a planned item changed since the dry
 * run or has gone; and reads each item again just before its write, stopping the run if it
 * changed in between.
 */
export async function apply(call, p, { delayMs, say }) {
  const current = new Map();
  for (const repo of new Set(p.items.map((i) => i.repo))) {
    for (const item of await readTracker(call, repo))
      current.set(`${repo} ${item.kind} ${item.number}`, item);
  }
  const todo = [];
  const changed = [];
  let done = 0;
  for (const it of p.items) {
    const now = current.get(`${it.repo} ${it.kind} ${it.number}`);
    if (sameAs(now, it.after)) done++;
    else if (sameAs(now, it.before)) todo.push(it);
    else changed.push(`${it.repo} ${it.kind} #${it.number}${now ? "" : " (gone)"}`);
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
    const body = {};
    if (it.before.title !== it.after.title) body.title = it.after.title;
    if (it.before.body !== it.after.body)
      body[it.kind === "milestone" ? "description" : "body"] = it.after.body;
    if (n > 0 && delayMs > 0) await sleep(delayMs);
    // Someone may have edited it since the check above.
    const now = await readOne(call, it);
    if (sameAs(now, it.after)) {
      done++;
      say(`already done: ${it.repo} ${it.kind} #${it.number}`);
      continue;
    }
    if (!sameAs(now, it.before)) {
      throw new Error(
        `stopped: ${it.repo} ${it.kind} #${it.number} ${now ? "changed" : "is gone"} since the dry run, ` +
          `after ${n} edit(s). Nothing more was written. Run the dry run again and review it.`,
      );
    }
    await call(["api", "-X", "PATCH", patchPath(it), "--include", "--input", "-"], body);
    n++;
    say(`edited ${it.repo} ${it.kind} #${it.number} (${n}/${todo.length})`);
  }
  return { done, edited: n };
}

function parseArgs(argv) {
  const o = {
    repos: [],
    skipMilestones: [...DEFAULT_SKIP_MILESTONES],
    gh: "gh",
    // GitHub asks for a second between writes at least; a slower pace keeps a ~1,000-item run
    // clear of its secondary limits.
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
    if (p.tool !== "tracker-rename" || p.version !== 1)
      throw new Error(`${o.from} is not a tracker-rename dry run`);
    const r = await apply(call, p, { delayMs: o.delayMs, say: warn });
    process.stdout.write(`applied: ${r.edited} edited, ${r.done} already done\n`);
    return;
  }
  const p = await plan(call, o.repos, o.skipMilestones);
  process.stdout.write(report(p));
  if (o.out) writeFileSync(o.out, JSON.stringify(p, null, 2) + "\n");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((e) => {
    process.stderr.write(`tracker-rename: ${e.message}\n`);
    process.exit(1);
  });
}
