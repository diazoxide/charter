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
