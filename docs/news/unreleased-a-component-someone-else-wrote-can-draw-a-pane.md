---
version: unreleased
headline: A component someone else wrote can draw a pane
---

Charter could already find a component an installed package supplies, load it, check its
API version, refuse two packages claiming one name and stand in for one that failed. What
it could not do was **draw it**. Every step between a `[[frame.component]]` table and a
real tmux pane spoke the four committed slot names — `top`, `bottom`, `repos`, `right` —
so a component with no slot name fell out somewhere between your config and your screen,
silently, on every path.

Those steps speak a component **id** now. `charter panel <component-id>` is the argv a
panel pane runs, and a panel process resolves that id itself.

## Placing one

A provider is a Python distribution that declares a component:

```toml
[project.entry-points."charter.components"]
"acme.metrics" = "acme_charter.metrics:Component"
```

Install it, and your `charter.toml` can put it on the frame:

```toml
[[frame.component]]
use  = "identity"

[[frame.component]]
use  = "acme.metrics"
edge = "right"
size = 12

[[frame.component]]
use  = "attention"
```

File order is split order, as it already was. The pane is split at the edge and size **your
file** asks for, not the ones the package would prefer — arrangement is committed,
execution is local, and a committed file has to draw the same frame on every machine.

**`edge` and `size` are required for a component charter did not write.** The only way to
ask a package where it would like to sit is to import it, and `charter --version` resolves
your config too; charter will not run a stranger's code to answer a geometry question on
every command. Charter's own four still take theirs from their own declaration, and still
refuse a table that disagrees with it.

## When it does not work

**A component no installed distribution supplies refuses the whole arrangement**, exactly
as an unknown name always did, and the frame falls back to `slots`. Charter cannot place a
rectangle for a component it cannot find, and dropping just that one table would hand you a
frame with a panel silently missing from it.

**A provider that is installed and then fails costs its own pane and nothing else.** An
import that raises, an API version charter does not speak, two distributions claiming one
id, a `render` that throws — each of those is now a pane naming the distribution and the
reason, with the rest of the frame drawn around it. That is the surface the standin was
built for one step earlier in this same release and had nowhere to appear on.

## The slot names

`slots = ["top", "bottom", "repos", "right"]` still launches exactly the frame it always
did, and `charter panel top` is still the argv charter emits for the identity strip. Those
four names are shorthand for four built-in ids now — `identity`, `attention`, `repos`,
`sidebar` — and either spelling reaches the same component, so `charter panel identity`
draws what `charter panel top` draws.

## What to do

Nothing, unless you have a component provider to install. Every `slots` list, every
`density` level and every `[[frame.component]]` arrangement resolves to exactly the frame
it resolved to before — same panels, same split order, same widths, same rows.
