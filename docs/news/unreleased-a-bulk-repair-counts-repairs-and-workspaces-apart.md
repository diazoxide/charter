---
version: unreleased
headline: '`charter workspace reinit --all` counts repairs and workspaces apart, so its closing line can no longer report healing more workspaces than exist'
adopt: workspace reinit --all
---

Run on a 17-workspace plane, the bulk repair ended like this:

```
• Healed 32 of 17 workspace(s); the rest were current.
```

**32 of 17.** The numerator was bigger than its own denominator, and "the rest were
current" was then a claim about −15 workspaces.

## One counter, two units

A workspace needs more than one repair routinely rather than exceptionally. The harness
layer is reported as a row per file charter writes, and the structure bump is a separate
row on top of it, so a workspace that predates both prints two lines:

```
✓ Reinitialized 'todos' → wrote .claude/settings.json (charter's harness layer).
✓ Reinitialized 'todos' → added structure v3 → v5.
```

The counter behind the summary was incremented once per row and then printed against the
number of workspaces, which is a different quantity. `workspace.json` becoming a required
component moved the structure version to 5, so on any plane that predates it most
workspaces need a manifest *and* a bump — the two-repairs shape is the common case, not
the exotic one, and every one of them contributed 2 to a number labelled "workspaces".

That line matters more than its size suggests: it is the only part of the report that
survives the run, because the per-workspace rows scroll away. It is what an operator reads
to confirm a bulk mutation landed, and a number that cannot be true there discredits
everything above it.

## Both numbers, each named

```
Applied 32 repair(s) across 15 of 17 workspace(s); the rest were current.
```

Nothing is thrown away. Deduping to a workspace count alone would have been correct and
would have lost the per-repair number, which is the one that says how much work the run
actually did; naming both says more than either.

The bound is structural rather than arithmetic. The workspaces repaired are accumulated as
a **set of names drawn from the list being iterated**, so a count above the total is not
something the summary avoids by getting a subtraction right — it is something it cannot
express.

A workspace charter could not repair is named rather than folded into "the rest were
current", which would have contradicted the error printed two lines above it:

```
✗ 'api': workspace.json could not be written — something is in the way at that path.
• Applied 4 repair(s) across 2 of 3 workspace(s); 1 could not be repaired; the rest were current.
```

## Nothing to adopt

The repair itself is unchanged — same components, same idempotence, same additive
promise. Only the sentence that closes it is different, so `charter workspace reinit --all`
is worth re-running only if you stopped believing the last report.
