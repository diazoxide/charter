# charter-app's design record lives in charter-app

Until this record, the reasons for charter-app were kept in another repository. The spec, the
plane-format contract and ADRs 0025 to 0043 were files in `diazoxide/charter-plane`, the Python
charter's repository. `AGENTS.md` told every agent to read the spec "in the charter repo", and
`README.md` linked there for the decision the whole app rests on. A change to the app's design
was a pull request in a repository that ships none of the app: ADR 0043 and the amendments to
ADR 0041 were open as charter#1182 and charter#1184 while the code they governed was reviewed
in this one.

On 2026-09-23 the operator ruled that *"charter-app should be standalone application without
any dependency from old charter"*, and, asked where the design record should live, chose
**"Move into charter-app"**. This record is that move: what came over, what stays behind, and
how the two are cited from now on.

## What moved

Copied whole, under `docs/`:

| Here | From `diazoxide/charter-plane` |
| --- | --- |
| `docs/adr/0025-…` to `docs/adr/0043-…` | `docs/adr/`, the same filenames |
| `docs/spec.md` | `docs/superpowers/specs/2026-09-17-charter-app.md` |
| `docs/plane-format.md` | `docs/plane-format.md` |

The text is the latest there was, including amendments that had not merged in that repository:
ADR 0041 and ADR 0043 come from the `adr/0041-stage-2-executor` branch (charter#1184), which
carries ADR 0043 from charter#1182 plus the executor amendments to 0041 and the view-tab
amendment to 0043. **ADR 0043 is still a DRAFT.** It moved with its status line, and moving it
is not a sign-off.

Two things were edited on the way in, both mechanically. Links to ADRs numbered below 0025 point
at those files in `diazoxide/charter-plane` at a fixed commit. The spec and the plane format
each carry a short note saying where they came from and where a path they name that is not here lives.

The pages `charter docs show` serves (`crates/charter-core/docs/`) are part of the same move.
They began as a byte-for-byte copy of the Python charter's `docs/*.md`, pinned to a commit and
compared against it on every CI run. They now describe this app and are written here, so the pin
and the comparison are gone.

## The numbering continues

This record is 0044, and the next one is 0045. Nothing is renumbered, so "ADR 0041" means the
same file in both repositories, and every citation of an ADR already in this repository's code,
comments and pull requests still resolves.

**How an ADR is cited from now on:**

- **0025 and above** are this repository's `docs/adr/`. Cite them as "ADR 00NN", and link them
  relatively from a document.
- **0001 to 0024** are the Python charter's and stay in `diazoxide/charter-plane` as history.
  Cite them by number and, wherever a reader would follow the citation, link them by URL at a
  fixed commit, such as
  [ADR 0013](https://github.com/diazoxide/charter-plane/blob/0ae0961d8a6a8e59b48ba43b10d28de8fd87afb7/docs/adr/0013-success-is-checked-divergence-is-named.md).
  Several of them still bind this app. ADR 0013 and ADR 0009 are cited in the core, and the move
  does not change that. They are read where they are.

References to issues in the old repository (`charter#NNN`) and "port of `charter/x.py`" notes in
the source are history too. They say where a behaviour came from, not what it depends on, and
they are left as they are.

## What stays behind, and what it means for the copies there

`diazoxide/charter-plane` keeps its own copies of 0025 to 0042, the spec and the plane format, as
the record of where they were written. **This repository's copies are the ones that are amended.**
If the two ever differ, this one is right. charter#1182 and charter#1184 are closed with a
pointer here rather than merged there.

## What this rules out

- Writing a charter-app decision, or amending one, in `diazoxide/charter-plane`.
- Telling a reader, human or agent, to read the spec or an ADR "in the charter repo".
- Renumbering, or starting a second sequence.
- Serving a `charter docs show` page that is a copy of the Python charter's and has to be kept in
  step with it.

## What it does not settle

The Python charter was still this app's differential oracle in CI (spec decision 14) when this
was written. Where that oracle is fetched from was left open here; [ADR
0046](0046-the-python-oracle-is-frozen-into-recorded-fixtures.md) answered it by retiring the
oracle: its answers are recorded fixtures now, and nothing fetches it. This record only moves the
documents.
