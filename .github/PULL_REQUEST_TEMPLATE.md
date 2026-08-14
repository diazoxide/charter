<!--
Keep it short. The useful half of a PR description is the failure the change prevents,
not a restatement of the diff.
-->

## What this changes

## The failure it prevents

<!-- What goes wrong today, concretely. If it's a bug fix, the test below should reproduce it. -->

## Checks

- [ ] `python3 -m unittest discover -s tests` passes
- [ ] A test fails without this change (behavioural changes only)
- [ ] Docs updated in the same PR, if a command or flag changed
- [ ] No new runtime dependencies
- [ ] If this contradicts an ADR in `docs/adr/`, that's called out above
