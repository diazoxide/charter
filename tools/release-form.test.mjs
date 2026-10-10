// The release checklist keeps every gate (DF-3, #634; DF-2, #633; QA-10, #597; QA-16, #599).
// Run with `node --test tools/`.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  FORM,
  GATES,
  PR_LINE,
  PR_TEMPLATE,
  faults,
  prFaults,
  readForm,
} from "./release-form.mjs";

const ROOT = new URL("../", import.meta.url);
const form = readFileSync(new URL(FORM, ROOT), "utf8");
const pr = readFileSync(new URL(PR_TEMPLATE, ROOT), "utf8");

/** The form with `from` replaced by `to`, failing loudly when `from` is not in it. */
function edited(from, to) {
  assert.ok(
    form.includes(from),
    `the form no longer holds ${JSON.stringify(from)}`,
  );
  return form.replace(from, to);
}

test("the repository's release form keeps every gate, each required, each naming its evidence", () => {
  assert.deepEqual(faults(form), []);
});

test("the pull request template asks for the test behind each acceptance line", () => {
  assert.deepEqual(prFaults(pr), []);
});

test("the form is read as GitHub reads it: ids, labels, required fields and required boxes", () => {
  const read = readForm(`name: A form
body:
  - type: markdown
    attributes:
      value: |
        Some words.
  - type: input
    id: version
    attributes:
      label: Version
      description: |
        Two lines
        of description.
    validations:
      required: true
  - type: checkboxes
    id: gate
    attributes:
      label: "A gate"
      options:
        - label: "One: the \`ci\` run."
          required: true
        - label: 'Two, not required'
        - Three, a bare option
`);
  assert.deepEqual(read, [
    {
      type: "markdown",
      id: undefined,
      label: undefined,
      text: "Some words.",
      required: false,
      options: [],
    },
    {
      type: "input",
      id: "version",
      label: "Version",
      text: "Two lines of description.",
      required: true,
      options: [],
    },
    {
      type: "checkboxes",
      id: "gate",
      label: "A gate",
      text: "",
      required: false,
      options: [
        { label: "One: the `ci` run.", required: true },
        { label: "Two, not required", required: false },
        { label: "Three, a bare option", required: false },
      ],
    },
  ]);
});

test("every gate the release checklist must hold is listed", () => {
  assert.deepEqual(
    GATES.map((gate) => gate.id),
    [
      "version",
      "commit",
      "technical-gate",
      "technical-evidence",
      "dogfood-gate",
      "dogfood-evidence",
      "user-signal-gate",
      "acceptance-tests",
      "outcome-bars",
      "notes-gate",
      "promotion",
    ],
  );
});

test("a gate taken out of the form is named", () => {
  const start = form.indexOf("  - type: textarea\n    id: user-signal-gate");
  const end = form.indexOf("  - type: textarea\n    id: acceptance-tests");
  assert.ok(start > 0 && end > start);
  assert.deepEqual(faults(form.slice(0, start) + form.slice(end)), [
    "user-signal-gate: missing (User-signal gate (LN-6))",
  ]);
});

test("a gate's box that is no longer required is named", () => {
  const changed = edited(
    'at least 3 days on the dev channel before it is promoted to stable."\n          required: true',
    'at least 3 days on the dev channel before it is promoted to stable."',
  );
  assert.deepEqual(faults(changed), [
    "dogfood-gate: a box is not required (The maintainer has run this version as their daily app for at least 3 days on the dev channel before it is promoted to stable.)",
  ]);
});

test("a field that is no longer required is named", () => {
  const start = form.indexOf("    id: acceptance-tests");
  const at = form.indexOf("    validations:\n      required: true", start);
  const changed =
    form.slice(0, at) +
    form.slice(at).replace("required: true", "required: false");
  assert.deepEqual(faults(changed), ["acceptance-tests: not required"]);
});

test("a gate that stops naming where its evidence is read is named", () => {
  const changed = edited("from the `SBOM` job", "from the release run");
  assert.deepEqual(faults(changed), [
    "technical-gate: no longer names the `SBOM` job",
  ]);
});

test("the dogfood gate keeps its three days on the dev channel", () => {
  const changed = edited(
    "for at least 3 days on the dev channel",
    "for a while",
  );
  assert.deepEqual(faults(changed), [
    "dogfood-gate: no longer names at least 3 days",
    "dogfood-gate: no longer names the dev channel",
  ]);
});

test("a gate whose field changed type is named", () => {
  const changed = edited(
    "  - type: textarea\n    id: outcome-bars",
    "  - type: input\n    id: outcome-bars",
  );
  assert.deepEqual(faults(changed), ["outcome-bars: an input, not a textarea"]);
});

test("a pull request template without the acceptance-line box is named", () => {
  assert.ok(pr.replace(/\s+/g, " ").includes(PR_LINE));
  assert.deepEqual(prFaults("## Checklist\n\n- [ ] Something else.\n"), [
    `the pull request template no longer asks: ${PR_LINE}`,
  ]);
});
