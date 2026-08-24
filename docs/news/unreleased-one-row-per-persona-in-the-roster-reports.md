---
version: unreleased
headline: a committed persona directory name could write its own row into `persona list` and `persona stats`
---

A persona is a directory under `personas/`, and that name is committed — it arrives from
whoever last pushed to the repo. `persona.list_personas()` globs `personas/*/`, asks only
for a leading underscore and a `persona.md`, and never asks `valid_name`; a filesystem
forbids `/` and NUL and nothing else. So a commit could add a directory whose name holds a
line separator, and `charter persona list` printed **two** table rows for it — the second
one entirely attacker-chosen and wearing charter's own column layout:

```
  PERSONA                             ROLE  VAULT  VAULT STATUS
  evil
  fake     Fake role   vault2        —      no vault
  good                                Good  v      not set up (local)
```

Two personas on disk, three rows in the report. `charter persona stats` did the same. It
works for `\n`, `\r`, U+2028, U+2029, U+0085 and every other separator `str.splitlines`
honours — which is the same mechanism as the `mcp.json` server-name hole 0.52.0 closed: a
committed value crossing into a format that has structure, without being escaped for it.

Nothing is executed off the back of a table row, so this is not a privilege boundary. What
a forged row can do is name a persona that does not exist, or hide one that does, in the
report a steward prunes the roster from.

Both renderers now map every committed value through `contain.one_line`, and print from
that. The role, the vault name, the vault status and the skills block get the same
treatment — all of them come out of committed files.

Where the bound goes differs, because only one of the two measures anything. `persona
list` sizes its PERSONA column from the names it is about to print, so it has to bound
them **before** it measures: `one_line` grows a name — a separator becomes a
four-character escape — and bounding at the `print` alone would leave every column after
PERSONA misaligned for every persona in the table. `persona stats` measures nothing; it
prints into a fixed 28-wide column, so it is bounded at the `print`. A name longer than 28
characters still pushes that row's later columns to the right there, exactly as it did
before — `one_line` bounds line STRUCTURE, which is what forged the extra row, not width
(#508).

The active marker still compares the **raw** names. Bounding is a display transform and two
different names can share one rendered form, so deciding the marker from the rendering would
mark every twin active as soon as one of them is — a report lying about which persona a
dispatch will actually use. Same rule for the dispatch tally, the draft check and the skills
lookup: the bound is what gets printed, never what gets looked up.
