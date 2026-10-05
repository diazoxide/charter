// The tracker rename (RN-12, #1256): its rules, and its dry run and apply against a stand-in
// `gh`. Run with `node --test "tools/*.test.mjs"`.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, mkdtempSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { renameText } from "./tracker-rename.mjs";

const SCRIPT = join(dirname(fileURLToPath(import.meta.url)), "tracker-rename.mjs");

// A stand-in `gh`: it answers the list and single-item reads the script makes from a state file,
// applies PATCHes to that file, and logs every call.
// - FAKE_GH_FAIL_AFTER makes the nth PATCH fail, the way an interrupted run stops.
// - FAKE_GH_RATE_LIMIT_ONCE makes the first PATCH answer a rate limit, with a Retry-After header
//   of FAKE_GH_RETRY_AFTER seconds when that is set.
// - FAKE_GH_EDIT_AFTER_FIRST_PATCH="<repo> <issues|milestones> <n>" has someone edit that item
//   right after the first PATCH lands, between the script's check and its write.
const FAKE_GH = `#!/usr/bin/env node
const fs = require("node:fs");
const args = process.argv.slice(2);
const state = JSON.parse(fs.readFileSync(process.env.FAKE_GH_STATE, "utf8"));
const log = (entry) => fs.appendFileSync(process.env.FAKE_GH_LOG, JSON.stringify(entry) + "\\n");
const method = args.includes("-X") ? args[args.indexOf("-X") + 1] : "GET";
const path = args.find((a) => a.startsWith("repos/"));
let input = null;
if (args.includes("--input")) input = JSON.parse(fs.readFileSync(0, "utf8"));
log({ method, path, input });
const m = path.match(/^repos\\/([^/]+\\/[^/?]+)\\/(issues|milestones)(?:\\/(\\d+))?/);
const repo = state[m[1]];
if (!repo) { process.stderr.write("HTTP 404: Not Found"); process.exit(1); }
const list = repo[m[2]];
if (method === "GET" && m[3]) {
  const one = list.find((i) => i.number === Number(m[3]));
  if (!one) { process.stderr.write("HTTP 404: Not Found"); process.exit(1); }
  process.stdout.write(JSON.stringify(one) + "\\n");
  process.exit(0);
}
if (method === "GET") {
  for (const item of list) process.stdout.write(JSON.stringify(item) + "\\n");
  process.exit(0);
}
const patches = fs.existsSync(process.env.FAKE_GH_LOG)
  ? fs.readFileSync(process.env.FAKE_GH_LOG, "utf8").trim().split("\\n").map(JSON.parse).filter((e) => e.method === "PATCH").length
  : 0;
if (process.env.FAKE_GH_RATE_LIMIT_ONCE && patches === 1) {
  if (process.env.FAKE_GH_RETRY_AFTER) process.stdout.write("HTTP/2.0 403 Forbidden\\nRetry-After: " + process.env.FAKE_GH_RETRY_AFTER + "\\n\\n{}");
  process.stderr.write("HTTP 403: You have exceeded a secondary rate limit.");
  process.exit(1);
}
if (process.env.FAKE_GH_FAIL_AFTER && patches > Number(process.env.FAKE_GH_FAIL_AFTER)) {
  process.stderr.write("connection reset");
  process.exit(1);
}
const item = list.find((i) => i.number === Number(m[3]));
Object.assign(item, input);
if (process.env.FAKE_GH_EDIT_AFTER_FIRST_PATCH && patches === 1) {
  const [r, kind, n] = process.env.FAKE_GH_EDIT_AFTER_FIRST_PATCH.split(" ");
  const other = state[r][kind].find((i) => i.number === Number(n));
  other[kind === "milestones" ? "description" : "body"] += "\\n\\nEdited mid-run by someone else.";
}
fs.writeFileSync(process.env.FAKE_GH_STATE, JSON.stringify(state, null, 2));
process.stdout.write(JSON.stringify(item));
`;

function tracker(state) {
  const dir = mkdtempSync(join(tmpdir(), "tracker-rename-"));
  const gh = join(dir, "gh");
  writeFileSync(gh, FAKE_GH);
  chmodSync(gh, 0o755);
  const statePath = join(dir, "state.json");
  writeFileSync(statePath, JSON.stringify(state, null, 2));
  const logPath = join(dir, "calls.jsonl");
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
    existsSync(logPath)
      ? readFileSync(logPath, "utf8")
          .trim()
          .split("\n")
          .map((l) => JSON.parse(l))
      : [];
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

function aTracker() {
  return {
    "o/app": {
      issues: [
        issue(1, "charter save loses a file", "Run `charter save` and check `charter.toml`."),
        issue(2, "Unrelated", "Nothing to see here."),
        issue(3, "A pull request about charter", "charter", { pull_request: true }),
        issue(4, "RN-1: rename charter to purlis", "`charter.toml` → `purlis.toml`", {
          milestone: "M60 · Rename to purlis",
        }),
        issue(
          5,
          "Set CHARTER_ROOT",
          "The `.charter/` folder, see https://github.com/diazoxide/charter/issues/9",
          {
            state: "open",
          },
        ),
      ],
      milestones: [
        { number: 7, title: "M18 · charter foundations", description: "What charter needs first." },
      ],
    },
    "o/plane": { issues: [issue(1, "charter sync is slow", null)], milestones: [] },
  };
}

test("the dry run prints every intended edit and writes nothing", () => {
  const t = tracker(aTracker());
  const before = readFileSync(t.statePath, "utf8");
  const out = join(t.dir, "plan.json");
  const r = t.run(["--repo", "o/app", "--repo", "o/plane", "--out", out]);
  assert.equal(r.status, 0, r.stderr);
  assert.equal(readFileSync(t.statePath, "utf8"), before);
  assert.deepEqual(
    t.calls().filter((c) => c.method !== "GET"),
    [],
  );
  assert.match(r.stdout, /o\/app issue #1/);
  assert.match(r.stdout, /- title: charter save loses a file/);
  assert.match(r.stdout, /\+ title: purlis save loses a file/);
  assert.match(r.stdout, /\+ Run `purlis save` and check `purlis.toml`\./);
  assert.match(r.stdout, /o\/app milestone #7/);
  assert.match(r.stdout, /o\/plane issue #1/);
  assert.doesNotMatch(r.stdout, /issue #2\b/, "an item with nothing to rename is not listed");
  assert.doesNotMatch(r.stdout, /issue #3\b/, "pull requests are not renamed");
  assert.doesNotMatch(r.stdout, /issue #4\b/, "the rename's own milestone is left as written");
  assert.match(r.stdout, /items to edit: 4/);
  const plan = JSON.parse(readFileSync(out, "utf8"));
  assert.deepEqual(
    plan.items.map((i) => `${i.repo} ${i.kind} ${i.number}`),
    ["o/app milestone 7", "o/app issue 1", "o/app issue 5", "o/plane issue 1"],
  );
});

function dryRun(t) {
  const out = join(t.dir, "plan.json");
  const r = t.run(["--repo", "o/app", "--repo", "o/plane", "--out", out]);
  assert.equal(r.status, 0, r.stderr);
  return out;
}

const patches = (t) => t.calls().filter((c) => c.method === "PATCH");

test("apply makes exactly the dry run's edits, and no others", () => {
  const t = tracker(aTracker());
  const planFile = dryRun(t);
  const plan = JSON.parse(readFileSync(planFile, "utf8"));
  const r = t.run(["--apply", "--from", planFile]);
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(
    patches(t).map((c) => [c.path, c.input]),
    [
      [
        "repos/o/app/milestones/7",
        { title: "M18 · purlis foundations", description: "What purlis needs first." },
      ],
      [
        "repos/o/app/issues/1",
        { title: "purlis save loses a file", body: "Run `purlis save` and check `purlis.toml`." },
      ],
      [
        "repos/o/app/issues/5",
        {
          title: "Set PURLIS_ROOT",
          body: "The `.purlis/` folder, see https://github.com/diazoxide/charter/issues/9",
        },
      ],
      ["repos/o/plane/issues/1", { title: "purlis sync is slow" }],
    ],
  );
  // What the tracker now says is what the dry run said it would, item for item.
  const now = t.now();
  for (const it of plan.items) {
    const list = now[it.repo][it.kind === "milestone" ? "milestones" : "issues"];
    const got = list.find((i) => i.number === it.number);
    assert.equal(got.title, it.after.title);
    assert.equal(it.kind === "milestone" ? got.description : got.body, it.after.body);
  }
  assert.equal(now["o/app"].issues[1].body, "Nothing to see here.");
  assert.equal(now["o/app"].issues[2].body, "charter", "the pull request is untouched");
});

test("apply makes the edits written in the dry-run file, not ones it works out again", () => {
  const t = tracker(aTracker());
  const planFile = dryRun(t);
  const plan = JSON.parse(readFileSync(planFile, "utf8"));
  // The operator struck one edit out of the reviewed file.
  plan.items = plan.items.filter((i) => !(i.repo === "o/app" && i.number === 1));
  writeFileSync(planFile, JSON.stringify(plan));
  const r = t.run(["--apply", "--from", planFile]);
  assert.equal(r.status, 0, r.stderr);
  assert.ok(!patches(t).some((c) => c.path === "repos/o/app/issues/1"));
  assert.equal(t.now()["o/app"].issues[0].title, "charter save loses a file");
});

test("an edit the operator corrected in the dry-run file is applied as corrected", () => {
  const t = tracker(aTracker());
  const planFile = dryRun(t);
  const plan = JSON.parse(readFileSync(planFile, "utf8"));
  const one = plan.items.find((i) => i.repo === "o/app" && i.number === 1);
  one.after.title = "purlis save loses a file (corrected by hand)";
  writeFileSync(planFile, JSON.stringify(plan));
  const r = t.run(["--apply", "--from", planFile]);
  assert.equal(r.status, 0, r.stderr);
  assert.equal(t.now()["o/app"].issues[0].title, "purlis save loses a file (corrected by hand)");
});

test("apply refuses, writing nothing, when an item changed since the dry run", () => {
  const t = tracker(aTracker());
  const planFile = dryRun(t);
  const state = t.now();
  state["o/app"].issues[4].body += "\n\nEdited by someone after the dry run.";
  writeFileSync(t.statePath, JSON.stringify(state));
  const r = t.run(["--apply", "--from", planFile]);
  assert.equal(r.status, 1);
  assert.match(r.stderr, /refused: 1 item\(s\) changed since the dry run/);
  assert.match(r.stderr, /o\/app issue #5/);
  assert.deepEqual(patches(t), []);
});

test("an interrupted apply is finished by running it again, skipping what it already did", () => {
  const t = tracker(aTracker());
  const planFile = dryRun(t);
  const first = t.run(["--apply", "--from", planFile], { FAKE_GH_FAIL_AFTER: "2" });
  assert.equal(first.status, 1);
  assert.equal(patches(t).length, 3, "two edits landed, the third was cut off");
  const second = t.run(["--apply", "--from", planFile]);
  assert.equal(second.status, 0, second.stderr);
  assert.match(second.stderr, /2 already done, 2 to edit/);
  assert.deepEqual(
    patches(t)
      .slice(3)
      .map((c) => c.path),
    ["repos/o/app/issues/5", "repos/o/plane/issues/1"],
  );
  const third = t.run(["--apply", "--from", planFile]);
  assert.equal(third.status, 0, third.stderr);
  assert.match(third.stderr, /4 already done, 0 to edit/);
  assert.equal(patches(t).length, 5);
});

test("a rate limit is waited out and the edit tried again", () => {
  const t = tracker(aTracker());
  const planFile = dryRun(t);
  const r = t.run(["--apply", "--from", planFile], { FAKE_GH_RATE_LIMIT_ONCE: "1" });
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stderr, /secondary rate limit.*waiting/);
  assert.equal(patches(t).length, 5);
  assert.equal(t.now()["o/app"].milestones[0].title, "M18 · purlis foundations");
});

test("an item edited between the check and its write is not written, and the run stops", () => {
  const t = tracker(aTracker());
  const planFile = dryRun(t);
  // The second planned item, o/app issue 1, is edited right after the first PATCH lands.
  const r = t.run(["--apply", "--from", planFile], {
    FAKE_GH_EDIT_AFTER_FIRST_PATCH: "o/app issues 1",
  });
  assert.equal(r.status, 1);
  assert.match(r.stderr, /o\/app issue #1 changed since the dry run/);
  assert.deepEqual(
    patches(t).map((c) => c.path),
    ["repos/o/app/milestones/7"],
  );
});

test("apply refuses a planned item that has gone", () => {
  const t = tracker(aTracker());
  const planFile = dryRun(t);
  const state = t.now();
  state["o/app"].issues = state["o/app"].issues.filter((i) => i.number !== 5);
  writeFileSync(t.statePath, JSON.stringify(state));
  const r = t.run(["--apply", "--from", planFile]);
  assert.equal(r.status, 1);
  assert.match(r.stderr, /refused: .*o\/app issue #5 \(gone\)/);
  assert.deepEqual(patches(t), []);
});

test("a rate limit's Retry-After is honoured", () => {
  const t = tracker(aTracker());
  const planFile = dryRun(t);
  const r = t.run(["--apply", "--from", planFile], {
    FAKE_GH_RATE_LIMIT_ONCE: "1",
    FAKE_GH_RETRY_AFTER: "1",
  });
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stderr, /waiting 1s/);
  assert.equal(patches(t).length, 5);
});

test("apply needs a saved dry run", () => {
  const t = tracker(aTracker());
  const r = t.run(["--apply"]);
  assert.equal(r.status, 1);
  assert.match(r.stderr, /pass --from <dry-run file>/);
  assert.deepEqual(t.calls(), []);
});

// ---- The rules ----

const renamed = (text) => renameText(text).text;

test("the product name becomes lowercase purlis, wherever it starts a sentence", () => {
  assert.equal(
    renamed("Charter reads the plane. So does charter."),
    "purlis reads the plane. So does purlis.",
  );
  assert.equal(renamed("About Charter"), "About purlis");
  assert.deepEqual(renameText("Charter and charter").renamed, { "product-name": 2 });
});

test("CLI forms, file and folder names, env vars, crates and skill ids are renamed", () => {
  assert.equal(renamed("Run `charter workspace create x`."), "Run `purlis workspace create x`.");
  assert.equal(renamed("$ charter doctor --fix"), "$ purlis doctor --fix");
  assert.equal(
    renamed(
      "`charter.toml`, `charter.local.toml`, `.charter/vaults`, `.charter-scan-allow.toml`, ~/.config/charter",
    ),
    "`purlis.toml`, `purlis.local.toml`, `.purlis/vaults`, `.purlis-scan-allow.toml`, ~/.config/purlis",
  );
  assert.equal(
    renamed("CHARTER_ROOT and CHARTER_* and `CHARTER_`"),
    "PURLIS_ROOT and PURLIS_* and `PURLIS_`",
  );
  assert.equal(
    renamed("crates/charter-core/src/x.rs and charter_core::acp and charter-cli"),
    "crates/purlis-core/src/x.rs and purlis_core::acp and purlis-cli",
  );
  assert.equal(
    renamed("the `charter:browser` skill and mcp__charter__recall"),
    "the `purlis:browser` skill and mcp__purlis__recall",
  );
  assert.deepEqual(renameText("`charter save` writes charter.toml, CHARTER_HOME").renamed, {
    cli: 1,
    "file-name": 1,
    "env-var": 1,
  });
});

test("URLs and the repo slugs that record history are left alone", () => {
  for (const kept of [
    "https://github.com/diazoxide/charter/issues/98",
    "see diazoxide/charter#12 and diazoxide/charter-plane",
    "[the old notes](https://example.com/charter/notes)",
    "the log at https://ci.example.com/charter/run/4, and <https://x.dev/charter>",
  ]) {
    assert.equal(renamed(kept), kept);
  }
  assert.equal(
    renamed("[charter docs](https://x.dev/charter)"),
    "[purlis docs](https://x.dev/charter)",
  );
});

test("fenced code blocks quote history and are left as written (D-RN12-1)", () => {
  const body = [
    "charter failed:",
    "```",
    "$ charter doctor",
    "✗ charter.toml is missing",
    "```",
    "  ~~~python",
    "  from charter import hooks",
    "  ~~~",
    "After the block charter works.",
    "````",
    "```",
    "charter inside a longer fence",
    "````",
    "```",
    "an unclosed fence: charter",
  ].join("\n");
  const r = renameText(body);
  assert.equal(
    r.text,
    body
      .replace("charter failed:", "purlis failed:")
      .replace("block charter works", "block purlis works"),
  );
  assert.equal(r.kept["fenced-block"], 5);
});

test("words that merely contain charter are not renamed", () => {
  for (const kept of [
    "two different charters",
    "a chartered plane",
    "read_charter()",
    "@charter_chat",
    "Charterhouse",
  ]) {
    assert.equal(renamed(kept), kept);
  }
});

test("Charter-* commit trailers, old plugin ids and the retired Python package stay as history", () => {
  for (const kept of [
    "a `Charter-Chat: 3` trailer and `Charter-*` trailers",
    "Charter-Persona, Charter-Change",
    "enabled plugin charter@charter, and charter-app@inline",
    "charter/hooks.py:772-801 and charter/frame/state.py",
    "python3 -m charter doctor",
    "the charter.hooks.pretooluse entry",
  ]) {
    assert.equal(renamed(kept), kept);
  }
});

test("the persona's own charter is the English word and stays", () => {
  assert.equal(renamed("a persona charter widens nothing"), "a persona charter widens nothing");
  assert.equal(
    renamed("that persona's charter, and its own charter"),
    "that persona's charter, and its own charter",
  );
  assert.equal(renamed("a persona in charter"), "a persona in purlis");
});

test("sentences about the rename itself keep the old name", () => {
  assert.equal(
    renamed("rename charter to purlis, formerly charter"),
    "rename charter to purlis, formerly charter",
  );
  assert.equal(renamed("charter → purlis"), "charter → purlis");
});

test("a body with no charter in it, or no body at all, is unchanged", () => {
  assert.deepEqual(renameText(null), { text: null, renamed: {}, kept: {}, words: {} });
  assert.equal(renamed("purlis is fine\r\nas it is"), "purlis is fine\r\nas it is");
  assert.equal(renamed("line one charter\r\nline two"), "line one purlis\r\nline two");
});

test("a cross-repo reference and a released version keep the name they were made under", () => {
  for (const kept of [
    "the same class as charter#1173",
    "charter 0.54.0 · Python 3.14",
    "charter v0.32 owns it",
  ]) {
    assert.equal(renamed(kept), kept);
  }
});

test("the retired Python distribution and its packages stay; the app's own files are renamed", () => {
  for (const kept of [
    "~/.local/share/uv/tools/charter-cp/bin",
    "the charter/frame/ package and charter/harness/",
    "edm -> charter, and edm → charter",
  ]) {
    assert.equal(renamed(kept), kept);
  }
  assert.equal(renamed("charter.exe and charter.app"), "purlis.exe and purlis.app");
  assert.equal(renamed("branch charter/save/<host>"), "branch purlis/save/<host>");
});

test("charterd, the daemon, becomes purlisd (dispatcher ruling)", () => {
  assert.equal(renamed("charterd listens on charterd.sock"), "purlisd listens on purlisd.sock");
  assert.deepEqual(renameText("charterd").renamed, { daemon: 1 });
  assert.equal(
    renamed("[ADR 0012](docs/adr/0012-charterd.md)"),
    "[ADR 0012](docs/adr/0012-charterd.md)",
  );
});

test("only a Python import or -m keeps the name; 'from charter' in prose is renamed", () => {
  for (const kept of ["from charter import hooks", "import charter", "python3 -m charter doctor"]) {
    assert.equal(renamed(kept), kept);
  }
  assert.equal(renamed("items created from charter"), "items created from purlis");
});

test("a persona's own charter.toml is still a file name", () => {
  assert.equal(renamed("its own charter.toml"), "its own purlis.toml");
  assert.equal(renamed("its own charter."), "its own charter.");
});

test("1Password references, existing branches and the uv install path still say charter", () => {
  for (const kept of [
    "op://Eng/charter-devops/NEW_KEY",
    "would create 'chore/charter-pin-0.30.0'",
    "~/.local/share/uv/tools/charter-cp/bin/charter",
  ]) {
    assert.equal(renamed(kept), kept);
  }
});

test("an indented code block is kept like a fenced one; an indented list line is prose", () => {
  const body = [
    "charter refused:",
    "",
    "    would create 'x' in the PLANE ROOT, said charter",
    "    charter doctor",
    "",
    "Then charter went on.",
    "",
    "- a list item about charter",
    "",
    "    its continuation, about charter too",
  ].join("\n");
  const r = renameText(body);
  assert.equal(
    r.text,
    [
      "purlis refused:",
      "",
      "    would create 'x' in the PLANE ROOT, said charter",
      "    charter doctor",
      "",
      "Then purlis went on.",
      "",
      "- a list item about purlis",
      "",
      "    its continuation, about purlis too",
    ].join("\n"),
  );
  assert.equal(r.kept["indented-block"], 2);
});

test("the old repo names in prose become the new ones; in paths, URLs and plugin ids they stay (D-RN12-9)", () => {
  assert.equal(
    renamed("charter-app is standalone, and charter-plane is plane-only"),
    "purlis is standalone, and purlis-plane is plane-only",
  );
  assert.equal(
    renamed("charter-app#12, charter-plane#40 and charter-plane #41, but charter#7"),
    "purlis#12, purlis-plane#40 and purlis-plane#41, but charter#7",
  );
  for (const kept of [
    "/path/to/charter-app",
    "charter-app/src/main.ts",
    "https://github.com/diazoxide/charter-app/actions",
    "diazoxide/charter-plane",
    "charter@charter-app and charter-app@inline",
  ]) {
    assert.equal(renamed(kept), kept);
  }
});

test("every occurrence is accounted for: renamed, or left alone under a named reason", () => {
  const text =
    "charter and charters, _charter_argv, @charter_chat, `charter.toml`, https://x.dev/charter";
  const r = renameText(text);
  const edits = Object.values(r.renamed).reduce((a, b) => a + b, 0);
  const kept = Object.values(r.kept).reduce((a, b) => a + b, 0);
  assert.equal(edits + kept, text.match(/charter/gi).length);
  assert.equal(r.kept["contains-charter"], 3);
  assert.deepEqual(r.words, { charters: 1, _charter_argv: 1, charter_chat: 1 });
});
