---
version: unreleased
headline: Every panel can have its own key, and density becomes a name for one arrangement
---

*"instead of having density - we need to have hotkeys to hide and show separately
components"*

Density was three names and nothing in between. `minimal` gave the two one-row strips,
`full` gave every edge charter draws, and if what you actually wanted was "everything
except the repo table, right now, for the next ten minutes" there was no way to say it.

**Give a component a key and that key is the panel's own switch:**

```toml
[[frame.component]]
use = "identity"

[[frame.component]]
use = "attention"

[[frame.component]]
use = "repos"
key = "F7"

[[frame.component]]
use = "sidebar"
key = "F8"
```

`F7` hides the repo table and hands its rows back to the agent session. `F7` again and the
table is back. **Hiding a panel does not delete it from your arrangement** — charter still
holds its edge, its size and its place in the order — which is exactly what taking a name
out of `slots` costs you: the position goes with the panel, and turning it back on later
means remembering where in the list it went.

**Density did not go away; it stopped being a separate thing.** A level is now a name for
one set of visible components, applied by the same mechanism a key uses. So the two
compose instead of overwriting each other:

```
F2 → density: minimal        the two strips, harness gets everything else
F8                           …and the sidebar back, and only the sidebar
F2 → density: full           every edge again — a level means what it names
```

What a level still does that no key can is set the *verbosity* — how much each panel says
— which is why the three levels are worth keeping and why the palette still offers them.

**Charter binds no key for this by default, and it will not start.** A tmux `bind -n`
intercepts the key before the pane underneath ever sees it, so four shipped defaults would
be four keys quietly taken away from Claude Code — or codex, or whatever you ran — on
every plane with a `charter.toml`. Keys are bound because you named them. Pick ones your
harness does not use.

The key is held to exactly the alphabet `[frame] hotkey` is held to, and that is one
function rather than two that look alike: a committed key is written into the tmux config
your frame loads, and a newline in one of those once ran a second tmux command at launch
with no keypress. A key charter will not bind — or one it has already bound: another
component's, your frame's own `hotkey`, or `F12`, the escape hatch that always gets you
back to your session — takes the whole arrangement out of play. That is the same
whole-arrangement refusal every other value charter cannot honour already gets, so you see
your arrangement ignored rather than one panel quietly missing a switch, or worse, an
escape hatch quietly missing.

Nothing here touches `charter.toml`. A key changes the frame you are in, for as long as it
runs; relaunch and you have the arrangement you committed.

To adopt it: write your arrangement out as `[[frame.component]]` tables if it is still
spelled `slots` — one table per name, same order, same frame — and add a `key` to the
panels you want a switch for.
