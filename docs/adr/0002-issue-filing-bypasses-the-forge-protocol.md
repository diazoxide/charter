# Upstream issue filing bypasses the Forge protocol

Filing and searching upstream issues is done with a flat `gh` call inside the reporting
module, **not** by adding `create_issue`/`search_issues` to the `Forge` protocol in
`charter/forge/`. This looks wrong at a glance — a raw `gh` invocation sitting beside a
full forge abstraction — so it is written down to stop someone "fixing" it.

The `Forge` protocol exists to abstract over *the Reporter's own* forges: their GitHub
Enterprise, their GitLab, whatever they clone from. Upstream reporting has no such
variation. It always targets one specific repo on github.com, so it is not polymorphic and
gains nothing from the abstraction. Putting `create_issue` on the protocol would instead
imply GitLab needs an implementation, which it never will: nobody files charter bugs on
GitLab.

The second reason is `_api`'s contract. It is documented as *"Best-effort JSON GET.
Returns None on any failure"* precisely so the status line — which renders every turn —
can never crash. Those semantics are correct there and catastrophic here: applied to
filing an issue, "swallow the failure and return None" means the Reporter's report
vanishes while they are told nothing. A write needs to fail loudly, which means it needs a
path that is not `_api`.

## Consequences

The reporting module is the only place in charter that writes to a forge, and the only
place that shells out to `gh` outside `charter/forge/`. That concentration is deliberate:
it is a single seam, which is what makes the feature testable without touching the network.

## Amended by the cross-repo change surface

**The sentence above became false, and it is corrected here rather than left to be
discovered.** `charter change push` and `charter change land` write to a forge — opening a
request, replacing a request body between charter's own markers, and merging one member —
so there are now **two** write seams: `report`, and the `Forge` protocol itself.

The decision is unchanged. This ADR forbids putting *upstream issue filing* on the protocol,
because that targets one repository on github.com and is not polymorphic. A change's requests
land on **the operator's own forges**, which is the exact axis the protocol exists for, so
they belong on it. `planegit._compare_url`'s comment — *"charter has no PR-creation
capability in any forge adapter"* — records a scope decision it made for itself, not a
prohibition on anyone else.

What survives verbatim is the reasoning about the write path, which this ADR states better
than the later spec did: *"A write needs to fail loudly, which means it needs a path that is
not `_api`."* The protocol's writes raise `ForgeWriteError` — modelled on
`report.ReportingError`, and asserted against the syntax tree so a write that routed through
`_api` reddens a test rather than a review.

The concentration argument survives in spirit too: both seams are narrow, both are stdlib
subprocess calls to the forge's own CLI, and both are testable without a network. What no
longer holds is the count, and an ADR left quietly false is a silent reversal wearing good
manners.
