// The release checklist keeps every gate (DF-3, #634).
//
//   node tools/release-form.mjs   exit 1 naming each gate the form or the PR template lost
//
// `.github/ISSUE_TEMPLATE/release.yml` is the checklist a maintainer fills for each release
// before it is promoted to stable. Its gates are standing rules: the technical gate, the
// dogfood gate (DF-2, #633), the user-signal gate (LN-6), the test behind each acceptance line
// (QA-10, #597), the outcome bars of what shipped (QA-16, #599), the changelog and the news
// entry. A form edited later could drop one in silence, or keep the box and lose the place its
// evidence is read. This holds the form to `GATES`: each gate there, of its type, required (every
// box of a checkbox group too), and naming each thing in its `names`.
//
// QA-10 asks the same of every pull request, so the pull request template's box for it is held
// here too (`PR_LINE`).
//
// **The form is read without a YAML library.** `tools/` has no dependencies. `readForm` reads the
// shape GitHub's issue forms take and nothing else: `body:` items, their `id`, the `label`, the
// `description` or `value` (plain or `|`), `options` and `required`. The app's community test
// parses every form with a real YAML parser and checks GitHub's schema; this reads the gates.
import { readFileSync, realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";

export const FORM = ".github/ISSUE_TEMPLATE/release.yml";
export const PR_TEMPLATE = ".github/PULL_REQUEST_TEMPLATE.md";

/** QA-10's box in the pull request template, whitespace made single spaces. */
export const PR_LINE =
  "- [ ] Each acceptance line this pull request meets names the test that checks it, under " +
  '"How it is tested", or says why it cannot have one.';

/**
 * Every gate the checklist holds, in its order. `names` are what the gate's words (its label,
 * description and boxes) must still say: where its evidence is read, or the rule it states.
 */
export const GATES = [
  { id: "version", type: "input", label: "Version" },
  { id: "commit", type: "input", label: "Commit" },
  {
    id: "technical-gate",
    type: "checkboxes",
    label: "Technical gate",
    names: [
      "the `ci` run",
      "Nightly",
      "Notarization",
      "ADR 0042",
      "the `build provenance` job",
      "the `SBOM` job",
    ],
  },
  { id: "technical-evidence", type: "textarea", label: "Technical evidence" },
  {
    id: "dogfood-gate",
    type: "checkboxes",
    label: "Dogfood gate (DF-2)",
    names: [
      "daily app",
      "at least 3 days",
      "the dev channel",
      "promoted to stable",
    ],
  },
  { id: "dogfood-evidence", type: "input", label: "Days on the dev channel" },
  {
    id: "user-signal-gate",
    type: "textarea",
    label: "User-signal gate (LN-6)",
    names: ["G3", "#634"],
  },
  {
    id: "acceptance-tests",
    type: "textarea",
    label: "The test behind each acceptance line (QA-10)",
    names: ["acceptance line", "the test that checks it"],
  },
  {
    id: "outcome-bars",
    type: "textarea",
    label: "Outcome bars of the features shipped (QA-16)",
    names: ["SD-2", "ADR 0067", "HP-5", '"built", not "done"'],
  },
  {
    id: "notes-gate",
    type: "checkboxes",
    label: "Changelog and news",
    names: [
      "node tools/changelog-fold.mjs --check",
      "purlis news --for <version>",
    ],
  },
  {
    id: "promotion",
    type: "checkboxes",
    label: "Promotion",
    names: ["pushes the `v*` tag"],
  },
];

const INDEFINITE = {
  input: "an input",
  textarea: "a textarea",
  checkboxes: "a checkbox group",
  markdown: "markdown",
  dropdown: "a dropdown",
};

/** A scalar as YAML writes it on one line: quoted with `"` or `'`, or plain. */
function scalar(raw) {
  const text = raw.trim();
  if (text.startsWith('"') && text.endsWith('"')) return JSON.parse(text);
  if (text.startsWith("'") && text.endsWith("'"))
    return text.slice(1, -1).replaceAll("''", "'");
  return text;
}

/** The lines after `at` indented deeper than `indent`, as a `|` block's text with lines joined by a space. */
function block(lines, at, indent) {
  const taken = [];
  for (let i = at + 1; i < lines.length; i++) {
    if (lines[i].trim() !== "" && lines[i].search(/\S/) <= indent) break;
    taken.push(lines[i].trim());
  }
  return taken.filter(Boolean).join(" ");
}

/**
 * The form's `body:` elements: `{ type, id, label, text, required, options }`, where `text` is the
 * description (or a markdown element's value) with its lines joined, `required` is the element's
 * own `validations.required`, and `options` are a checkbox group's or dropdown's boxes.
 */
export function readForm(text) {
  const lines = text.split("\n");
  const start = lines.findIndex((line) => /^body:\s*$/.test(line));
  if (start < 0) return [];
  const elements = [];
  let element = null;
  let section = null; // attributes, validations or options, by the key that opened it
  for (let i = start + 1; i < lines.length; i++) {
    const line = lines[i];
    if (line.trim() === "" || line.trim().startsWith("#")) continue;
    const indent = line.search(/\S/);
    if (indent === 0) break;
    const item = /^ {2}- type:\s*(.+)$/.exec(line);
    if (item) {
      element = {
        type: scalar(item[1]),
        id: undefined,
        label: undefined,
        text: "",
        required: false,
        options: [],
      };
      elements.push(element);
      section = null;
      continue;
    }
    if (!element) continue;
    const pair = /^(\s*)(?:- )?([\w-]+):\s*(.*)$/.exec(line);
    if (indent === 4 && pair) {
      if (pair[2] === "id") element.id = scalar(pair[3]);
      section = pair[2];
      continue;
    }
    if (indent === 6 && pair) {
      const [, , key, value] = pair;
      if (section === "validations" && key === "required")
        element.required = scalar(value) === "true";
      if (section !== "attributes") continue;
      if (key === "label") element.label = scalar(value);
      if (
        key === "description" ||
        (key === "value" && element.type === "markdown")
      )
        element.text =
          value.trim() === "|" ? block(lines, i, 6) : scalar(value);
      if (key === "options") section = "options";
      continue;
    }
    if (
      section === "options" &&
      indent === 8 &&
      line.trimStart().startsWith("- ")
    ) {
      const option = /^\s*- label:\s*(.+)$/.exec(line);
      element.options.push({
        label: scalar(option ? option[1] : line.trimStart().slice(2)),
        required: false,
      });
      continue;
    }
    if (section === "options" && indent === 10 && pair?.[2] === "required")
      element.options.at(-1).required = scalar(pair[3]) === "true";
  }
  return elements;
}

/** What the release form lost against `GATES`, one line per fault; empty when it kept them all. */
export function faults(text) {
  const elements = readForm(text);
  const found = [];
  for (const gate of GATES) {
    const element = elements.find((candidate) => candidate.id === gate.id);
    if (!element) {
      found.push(`${gate.id}: missing (${gate.label})`);
      continue;
    }
    if (element.type !== gate.type) {
      found.push(
        `${gate.id}: ${INDEFINITE[element.type] ?? element.type}, not ${INDEFINITE[gate.type]}`,
      );
      continue;
    }
    if (gate.type === "checkboxes") {
      if (element.options.length === 0) found.push(`${gate.id}: no boxes`);
      for (const option of element.options)
        if (!option.required)
          found.push(`${gate.id}: a box is not required (${option.label})`);
    } else if (!element.required) {
      found.push(`${gate.id}: not required`);
    }
    const words = [
      element.label,
      element.text,
      ...element.options.map((option) => option.label),
    ].join(" ");
    for (const name of gate.names ?? [])
      if (!words.toLowerCase().includes(name.toLowerCase()))
        found.push(`${gate.id}: no longer names ${name}`);
  }
  return found;
}

/** What the pull request template lost of QA-10's box; empty when it still asks. */
export function prFaults(text) {
  return text.replace(/\s+/g, " ").includes(PR_LINE)
    ? []
    : [`the pull request template no longer asks: ${PR_LINE}`];
}

if (
  process.argv[1] &&
  realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))
) {
  const root = new URL("../", import.meta.url);
  const found = [
    ...faults(readFileSync(new URL(FORM, root), "utf8")).map(
      (fault) => `${FORM}: ${fault}`,
    ),
    ...prFaults(readFileSync(new URL(PR_TEMPLATE, root), "utf8")),
  ];
  for (const fault of found) console.error(fault);
  if (found.length === 0)
    console.log(
      `${FORM} keeps every gate, and ${PR_TEMPLATE} asks for the test behind each acceptance line`,
    );
  process.exit(found.length === 0 ? 0 : 1);
}
