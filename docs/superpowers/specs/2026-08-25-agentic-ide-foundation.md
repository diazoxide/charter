# charter as an agentic IDE — foundation spec

**Date:** 2026-08-25 · **Status:** approved in outline, unimplemented
**Supersedes in part:** the density presets shipped in #387

---

## 1. What this is for

charter today is a control plane with a TUI attached. The goal is a **harness-agnostic
agentic IDE**: the place a person works when the work spans many repositories, several
agents, and more state than one status line can hold.

The distinction that matters: an IDE is not a status display with more panels. It is a
surface where you *do* things — switch context, hide what is irrelevant, start work, see
what agents are doing, act on what they found. Everything below follows from that.

---

## 2. Where we actually are

Honest assessment, because the plan depends on it.

**What works.** The frame runs a harness inside a tmux-composed layout with charter panels
around it, on charter's own server or the operator's. Panels repaint from a cache on a
version bump, at a measured idle cost of one `stat` per tick. Identity travels explicitly.
Chrome is charter's own. The repo table renders at the width it was designed for.

**What does not.** Three things, and they are the reason this spec exists:

1. **There is no model of what the frame is made of.** Slots are a list of four strings
   whose ORDER is load-bearing geometry. There is nothing that knows a component has an
   identity, a visibility, a size policy, or a key. Every feature that wants to *change what
   is on screen* has had to invent its own mechanism — and three have: `[frame] slots`,
   `[frame] density`, and `cmd_density`'s live re-layout.

2. **Density is the wrong abstraction, and it shipped anyway.** `minimal` / `normal` /
   `full` are bundles somebody else chose. The operator's actual request is per-component:
   *hide the repo table, keep the sidebar.* A preset cannot express that, and adding levels
   does not fix it — it multiplies it.

3. **`display-menu` is not a command surface.** It was the cheapest thing that worked when
   the frame had two slots. It has since produced: a `-` key that silently ran a command
   because tmux treats it as a real key; a nine-row cap with no way to page; no filtering,
   no live state, no way to show *why* an action is unavailable. It is a confirm dialog
   being asked to be a command palette.

**And a fourth thing, which is not the frame's fault but blocks everything:** the test suite
reads the developer's machine. Ambient environment (#519, #521, #528 — 108 call sites), the
real plane (#527), the filesystem (#532), wall-clock sleeps (#494). A green suite currently
means "green on this machine, this minute, in this shell". Nothing built on top of that is
trustworthy.

---

## 3. Three decisions

Taken deliberately, with their reasoning, so they can be argued with later.

### 3.1 tmux stays for panes. Charter draws its own overlays.

The engine choice was made on measurement — tmux at 25.2 MB/s end-to-end against
Textual+pyte at 1.85, with a 13 MB log freezing the latter for ~7s versus 0.51s. That
result is about *throughput of harness output*, and it has not changed.

What has changed is that everything *interactive* has been a fight with `display-menu`. So
the split moves:

- **tmux owns panes** — splitting, sizing, resize hooks, the harness's own terminal.
- **charter owns overlays** — menus, pickers, palettes, confirmations, anything with
  selection, filtering, or state. Rendered by charter into a pane charter controls.

This keeps the property the engine was chosen for (the harness's output path is untouched)
and removes the constraint that has cost us most. `display-menu` may remain for a
two-option confirm; nothing richer.

### 3.2 The IDE's unit of work is a change that spans repos

Today the model is: a plane holds workspaces; a workspace holds repo clones. That is a
*place to work*, not a *piece of work*.

For "monorepo on top of many repos", the missing concept is a **cross-repo change**: one
intent, several repositories, several branches, several PRs — tracked as one thing, with
one status. It is what a person means when they say "the auth migration": not a branch, not
a PR, but the whole of it across four services.

This is a data-model addition, not a UI feature, and the UI follows from it: the frame's job
becomes showing *the change you are in* and what it touches, rather than a flat list of
repositories that happen to be cloned.

### 3.3 The test cluster is paid down first, alone

Seven issues, one structural shape. Until it is done, neither an operator nor an agent can
distinguish a real failure from an ambient one — and the evidence is concrete: a false green
hid a real defect through two separate investigations, and a suite run inside a frame
reports ~17 failures that do not exist.

Everything in this spec is verified by that suite. Building on it before it is trustworthy
means every later result carries an asterisk.

---

## 4. The component model

The core new concept. Everything in §5 is a view over it.

Today a slot is a string in a list. A **component** is a named thing the frame can show,
with:

| property | meaning |
|---|---|
| `id` | stable, e.g. `repos`, `personas`, `todos`, `attention`, `identity` |
| `title` | what a menu calls it |
| `edge` | which side it attaches to, and in what order |
| `size` | fixed rows/cols, or content-sized with a floor and a cap |
| `visible` | per-frame runtime state, not config |
| `key` | its own toggle binding |
| `renderer` | the function that draws it |
| `needs` | what it reads (the gather cache, personas, todos) — so cost is declarable |

From that registry, three things that are currently three mechanisms become one:

- **`[frame] slots`** becomes the *initial visibility* of components, not a geometry list.
- **Density** becomes a **named arrangement** — a saved set of visibilities — rather than a
  bundle with its own expansion rules. `minimal` and `full` survive as conveniences; they
  stop being the only way to change the layout.
- **Live re-layout** becomes "set `visible`, recompute, apply", once, instead of a
  special-cased command.

**Order is geometry and must stay explicit.** The measurement that produced the current
layout — a 200-column bottom row versus 154, depending only on split order — is a property
of tmux splitting, not a detail. The registry stores the edge and the split order; nothing
derives it from list position by accident.

**Cost is declarable.** `needs` exists so the idle-tick property survives growth: a
component that reads nothing new adds no filesystem work, and one that does is visible as
such rather than discovered by a reviewer counting `stat`s.

---

## 4b. Third-party components — the extension model

**Decided 2026-08-25: components may be supplied by installed providers, not only by
charter itself.** A repo customises its frame through `charter.toml`; the code behind a
component comes from something the operator installed.

### The safety principle, and why this is the safe option

> **Arrangement is committed. Execution is local.**

`charter.toml` is committed and shared — it arrives from someone else's machine. So it may
say *which* components to place, *where*, *in what order*, and *whether they are visible*.
It may never say *what code runs*.

This is why binding by **name to an installed provider** is safer than the obvious middle
option of a `command = "..."` string. A command string is executable content in a committed
file: clone the repo, open charter, run their code — no prompt, no consent. That is exactly
`[frame] hotkey` (a newline reaching tmux config text achieved code execution at launch) and
#453 (a committed `mcp.json` key spent any vault silently). A name that resolves against
installed code cannot do that: if the provider is absent, nothing runs and charter says so.

### Discovery

A provider is a Python distribution declaring an entry point:

```
[project.entry-points."charter.components"]
metrics = "acme_charter.metrics:Component"
```

charter discovers them with `importlib.metadata.entry_points(group="charter.components")` —
**stdlib, so `dependencies = []` is untouched.** Nothing is imported until a component is
actually placed, so an installed-but-unused provider costs nothing.

This is deliberately not a harness plugin. The frame is harness-agnostic; a component
provider must work under Claude Code, codex and opencode alike, so it binds to charter, not
to a harness.

### The provider contract

A component declares its identity and its cost, and renders on request:

| it declares | why |
|---|---|
| `id`, `title` | placement and menu text |
| `default_edge`, `default_size` | a sensible arrangement before anyone configures one |
| `needs` | what it reads — the gather cache, personas, todos, nothing |
| `render(ctx) -> list[str]` | given width, height, and the caches it declared |

`ctx` hands over what the component is allowed to read. It does **not** hand over a way to
run a subprocess or reach the network on the repaint path.

### Four properties that must survive a stranger's code

These are what make the extension model real rather than a hole:

1. **A component that raises must not kill the frame.** Same posture hooks already have: it
   may cost its own pane, never the session. The pane shows that the component failed and
   names it — a blank pane is the confidently-wrong output the left sidebar was retired for.

2. **`needs` becomes enforcement, not documentation.** Today the idle-tick property (one
   `stat` per panel per tick) is verified by reading charter's own code. With third-party
   renderers that stops being possible, so the contract has to be enforced: a component that
   declared `needs = ["gather"]` is handed the cache and nothing else, and a repaint that
   tries to read the filesystem or spawn a process fails loudly in development rather than
   silently costing every operator 200 ms a tick.

3. **Output is contained.** A third-party component renders plane values — repo names,
   branch names, todo text — all committed, all untrusted. Its output goes through the same
   `contain.one_line` and `tui.width` path as a built-in, applied by charter after the
   component returns, not trusted to the component.

4. **A missing provider is a message, not a crash.** `charter.toml` naming `acme.metrics`
   on a machine without it says so plainly and draws the rest of the frame. A committed
   config must never make charter unusable for someone who has not installed a third-party
   package.

### What `charter.toml` gets to say

```toml
[[frame.component]]
use     = "repos"          # a built-in
edge    = "bottom"
size    = "content"

[[frame.component]]
use     = "acme.metrics"   # a provider's id
edge    = "right"
size    = 12
visible = false            # present, toggleable, off by default
```

Order in the file is split order, because **order is geometry** — the 200-column-versus-154
measurement is a property of tmux splitting, not a detail. Everything here is arrangement;
nothing here is code.

### Sequencing

This lands in **Phase 1**, but built inward-out: the registry serves built-ins first with the
provider interface as its only way in, so charter's own components are the first consumers of
the extension API. A component model that charter itself does not use through the public seam
is a component model with a private back door, and the back door is where the drift lives.

---

## 4c. Interactivity — components receive input

**Decided 2026-08-25: components are input-capable, not render-only.** Clicks, scroll, keys,
focus — whatever a terminal can deliver, a component may receive. This is the decision that
makes the difference between a dashboard framework and an IDE, and it was taken deliberately
in the knowledge that it is the harder of the two.

### What it costs, stated up front

Render-only would have been simpler and could have been widened later; input-capable cannot
be narrowed later without breaking people. Two consequences follow and neither is optional:

1. **Third-party code receives keystrokes and pointer events**, not just a chance to draw
   text. That is a materially larger trust surface than §4b's rendering contract, and the
   containment rules there (charter contains the output) do not cover it — the input path
   needs its own.
2. **Focus becomes a real concept.** Something must own "where does this keystroke go", and
   it must be answerable at every moment, including while an overlay is open and while a
   re-layout is in flight.

### The routing question charter already anticipated

`instance.FRAME_FIELDS` carries `mouse: (False, …)` with this note, written before any of
this was planned:

> Off by default: tmux's `set -g mouse on` takes over drag-select, so turning this on trades
> the operator's terminal text-selection for clickable panels. That trade belongs to a later
> release that actually ships clickable panels, not this one.

That release is this one, and the trade it names is the design problem: **with tmux's mouse
mode on, tmux consumes pointer events itself** — pane select, border drag, its own copy-mode
scroll — and charter's panels never see them. With it off, sequences pass through to each
pane's program.

Charter's panels **are** charter processes with their own tty. So the shape to measure first
is: leave tmux's mouse mode off, and have each panel enable SGR mouse reporting on its own
terminal, so pointer events in a charter pane reach charter and pointer events in the
harness pane reach the harness — which also preserves the drag-select the comment worried
about losing.

**That is a hypothesis, not a finding.** It must be measured against real tmux on both the
private server and the operator's own before anything is built on it, the way the engine
choice and the `-e` overlay behaviour were measured rather than assumed. Record what tmux
actually does with: pointer events in a pane whose program requested them; scroll in a pane
with a scrollback; a click on a pane border; and the same three inside the operator's own
tmux where their `.tmux.conf` may already set `mouse on`.

### What the model needs, whatever the routing turns out to be

- **A focus owner**, and a rule for how focus moves — by click, by key, and what happens when
  the focused component is hidden or its pane is killed mid-re-layout.
- **An event contract** parallel to the render contract: a component declares which event
  kinds it accepts, and receives only those. A component that never asked for pointer events
  should not be reachable by one.
- **Scroll that means something per component.** A repo table scrolls its rows; the harness
  pane scrolls its own scrollback. These are different operations that look identical to the
  wheel, and conflating them is how a scroll in the wrong place loses someone's place.
- **An escape hatch that always works.** One key that returns focus to the harness
  unconditionally, from any component, overlay, or wedged state. If a third-party component
  can capture input, it can capture input badly.
- **Input isolation matching §4b's four properties.** A component that raises while handling
  an event costs its own pane, never the session — and never the keystroke, which must still
  reach the escape hatch.

### Sequencing

Interactivity is **Phase 2**, alongside the command surface, and depends on Phase 1's
registry existing. But the routing measurement above should happen **during Phase 1**,
because if the hypothesis is wrong the overlay design changes, and it is cheaper to learn
that before the palette is written than after.

---

## 5. The command surface

One mechanism, three faces.

### 5.1 The palette

A charter-drawn overlay, opened by one key. It lists **actions**, filtered as you type:

```
  switch workspace …          W
  switch persona …            P
  toggle repo table           T
  toggle personas             E
  hide everything but harness Z
  reload plane                R
```

An action has an id, a title, an optional direct key, an availability predicate, and a
reason it is unavailable. That last field matters: the session lock refuses a workspace
switch today, and a menu that silently fails is worse than no menu.

### 5.2 Direct keys

Every action may bind a key, and toggles get first-class ones because that is the request:
show and hide components individually, without opening anything.

Keys are validated at the config boundary the way `[frame] hotkey` already is — that
constant is the single guard between a committed config value and tmux config text, and a
newline in it achieved code execution at launch once already.

### 5.3 Pickers

A list-selection overlay for workspaces, personas, and later changes. Same rendering as the
palette; the difference is the source of rows.

**Containment is not optional here.** Workspace names, persona names, repo names and todo
text are committed values — untrusted input from someone else's machine. Every row goes
through `contain.one_line` **before** any width arithmetic, and the bound is on the property
"renders as one line", not on a list of characters known to be bad. That distinction is what
#498 is about, and what U+2028 taught us after `\n` was handled.

---

## 6. Phases

Sequenced so each phase can be verified with the tools the previous one made trustworthy.

### Phase 0 — make the suite mean something
*Blocks everything. Do it alone.*

#519, #521, #528 (108 unisolated `patch.dict` sites), #527 (real detached children against
the developer's plane), #532 (skills read off the filesystem), #494 (a fixed 0.8s sleep
standing in for a condition), #507 (the tmux module's remaining flake), #531 (dead code
after `__main__`).

Prefer **one structural guard** over 100+ edits, matching how #402 (`RealPlaneWrite`) and
#492 (`RealPlaneRead`) were solved: make the unisolated case fail loudly and name itself.

**Exit test:** the full suite gives the same answer inside a frame, outside one, on a used
plane and on a fresh checkout.

### Phase 1 — the component registry
*The foundation. No new features.*

Introduce the registry; express today's four components in it; make `[frame] slots`,
density and live re-layout read from it. Ship with behaviour unchanged — this phase is
provably a refactor.

Absorbs #501, #510, #524, #526, #536 — the layout cluster — because each is a place that
recomputes geometry independently, and after this there is one place.

**Exit test:** the frame renders identically before and after, and the three mechanisms that
change what is on screen all route through one function.

### Phase 2 — the command surface
*The visible payoff.*

The overlay renderer, the action registry, the palette, per-component toggle keys, and
pickers for workspace and persona. Retire `display-menu` for everything but a plain confirm.

Absorbs #530 (the top bar stops duplicating the sidebar — a component that knows whether its
sibling is visible can answer this properly), and the `-` key class of bug disappears with
the mechanism that caused it.

**Exit test:** every action reachable in ≤2 keystrokes; every unavailable action says why;
a hostile workspace name renders as one line and runs nothing.

### Phase 3 — containment and honesty, once
*The cluster, paid down together.*

#498 (`contain.one_line` deciding on a category list rather than the property), #502, #503,
#505, #508, #509. One reviewer, one shape: a committed value crossing into structured
output.

**Exit test:** a name containing a newline, U+2028, an escape sequence, a duplicate key or a
mis-cased key produces exactly one row, and never a second command.

### Phase 4 — the cross-repo change
*The IDE's actual subject.*

The data model, then the surface: create a change, add repos to it, see its branches and
PRs as one status, act on it. This is where "monorepo on top of many repos" stops being a
description of the file layout and becomes something the tool understands.

Needs its own spec. Phases 0–3 are what make it buildable.

---

## 7. Backlog, beyond the phases

Things worth wanting, not yet designed. Recorded so they are not rediscovered as surprises.

- **Focus and mouse.** Once charter draws overlays, click-to-focus a component and
  scroll-in-place become reachable. Deliberately after Phase 2.
- **A component that shows what agents are doing.** `inflight` already tracks dispatches;
  #420 established the record has no *kind*, which is what makes clones and refreshes
  invisible. A real "work in flight" panel needs that field first.
- **The context gauge inside a frame.** #413's blocker stands: a panel knows the frame id,
  the usage file is keyed by the harness's session id, and only the suppressing
  `statusline.main` sees both. Needs a persisted mapping and a decision about what it shows
  before the first turn.
- **Search across the plane.** Not grep-in-a-repo — find a persona's memory, a workspace's
  todos, a change's PRs, from one place.
- **Layout persistence per workspace.** Different work wants different components visible.
  The registry makes this a small addition; do not build it before the registry.
- **A second harness in the same frame.** The layout can hold it; nothing else can yet.

---

## 8. What this spec deliberately does not do

- **It does not replace tmux.** That decision was made on measurement and stands.
- **It does not add a config key per component.** The registry is code; `[frame]` gains
  initial visibility, not thirty knobs.
- **It does not promise the guards become boundaries.** `SECURITY.md`'s position — "guard
  rails, not guarantees" — is unchanged. Four rounds of adversarial review established that
  deciding what a shell will execute, without executing it, is not winnable in a Python
  tokeniser. The honest limits stay documented rather than half-parsed.
