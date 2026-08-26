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

**`default_edge` and `default_size` are defaults, and a committed arrangement beats them.**
The word in that table row is load-bearing: they are for a sensible arrangement *before*
anyone configures one, and where a `[[frame.component]]` table says an edge or a size, the
table is what draws. The inverse — a provider's Python overruling a committed file — is
*execution* deciding *arrangement*, which is the safety principle above run backwards, and
its symptom is one committed table drawing two different frames on two machines depending
on whether a package happens to be installed. Charter shipped exactly that inversion once
in `Registry.place` (§4i).

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

4. **A missing provider is a message, not a crash** — *and the message needs a surface to
   live on, which Phase 1 does not build.* `charter.toml` naming `acme.metrics` on a
   machine without it must never make charter unusable for someone who has not installed a
   third-party package. **Where the message goes, narrowed 2026-08-26:** in Phase 1 it is a
   standin *component* in the registry (`Registry.place`), holding the rectangle config
   asked for and drawing the reason — but nothing places it, because every step between a
   committed table and a painted pane still speaks the four committed **slot names**:
   `instance.component_tables` refuses a `use` outside `builtins.SLOT_OF`, `frame_of`
   filters against `FRAME_SLOTS`, `layout._derive` keys off `SLOT_OF` (a `KeyError`, by
   design, for a component with no slot name), `layout.panel_command` emits
   `charter panel <slot>` as tmux argv, and `frame/panel.py:run` refuses a slot
   `slots.SLOTS` has no renderer for. **So until Phase 2 gives the frame a surface that can
   SAY a component is missing, an arrangement carrying one charter cannot honour is refused
   whole and the frame falls back to `slots`** — the operator sees their whole arrangement
   not take effect, which is visible and actionable, rather than one pane quietly absent.
   A message with nowhere to appear is a silent drop, and a silent drop is #512 and #535.
   Placing a provider is Phase 2 work, and this property is what it must deliver.

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

## 4d. Settled by grilling — round 1 (2026-08-25)

**Vocabulary.** **Component** is the concept — identity, visibility, size policy, renderer,
events. **Panel** is the process that hosts one. **Pane** is tmux's rectangle. **Slot
retires**: it currently means "a string whose list position is secretly geometry", which is
the confusion this work removes.

**A component owns a pane, and components compose.** A `sidebar` may be *built from*
`personas` and `todos`, but charter never draws splits inside a pane. This gives
N-things-in-one-pane without a layout engine, and keeps tmux doing resize, borders and pane
focus — all solved, none of them cheap to redo. Composition is the seam to grow true
in-pane layout at, if it is ever needed.

(Note this was already happening informally: #516 put personas and todos in one `right` pane
with no concept for it.)

**Actions are extensible too, separate entry-point group, stricter contract.** A provider
that adds a CI panel but cannot add "rerun failed job" is half a plugin. `charter.components`
and `charter.actions`. But a component *draws* and an action *does*: an action declares what
it touches, and anything reaching a vault, a forge token or a shell goes **through** the
existing guards rather than around them. Narrow from day one — widening is easy, and this is
where real damage would live.

**A cross-repo change is a stored object, not a view over git.** A view cannot hold *intent*,
and intent is the only thing that actually spans repositories. Branch-name conventions break
the moment someone names one differently, and they cannot record why a change exists, what
blocks it, or that a fourth repo was considered and excluded. Storage also makes a change
addressable — by the frame, by actions, and eventually by agents.


## 4e. Settled by grilling — round 2 (2026-08-25)

**`ctx` is constructed FROM `needs`.** A component receives an object holding only what it
declared, plus geometry and its own identity. No filesystem handle, no subprocess factory,
no network client — **not present**, rather than present-and-discouraged. That makes the
idle-cost property enforced by construction rather than by a reviewer counting `stat`s,
which is the only version that survives third-party code. Asking for something undeclared
fails in development, naming what to add.

**Size: children declare, the parent arbitrates, exactly one `fill`.** Each child is
`fixed(n)`, `content` (its own height, capped), or `fill` (what remains). More than one
`fill` is a load-time configuration error, never a runtime tie-break — ambiguity here
produces layouts that shift with the data, which reads as a bug every time.

**Focus has two levels.** tmux owns **pane focus** — unchanged, free, already correct.
Charter owns **intra-pane focus** — which child of a composed component is active — and only
inside a pane tmux has already focused. **The escape hatch operates at the tmux level**,
returning to the harness pane, so it works even when charter's intra-pane focus is wedged or
a third-party component has captured input badly. A single-level model would put the escape
hatch inside the thing it must escape.

**A change is committed, like a workspace** — cross-repo work is team work, and the point is
that a teammate can see what "the auth migration" touches. Consequence, named now rather
than retrofitted: **a change's name and description are untrusted committed values**, needing
containment before any table row, menu item or pane, and the change list is a committed
reading site — the same class as #442.

## 4f. Settled by grilling — round 3 (2026-08-25)

**Store intent, derive facts.** A change stores what only a human knows — its name, why it
exists, which repos are members, what blocks it, which repo was considered and excluded. It
stores **no** derived state: PR numbers, CI results and branch positions come from the
sources the repo table already reads, through the same refresh path. Stored facts go stale
silently and become a second truth that disagrees with git — the shape that produced #524
and #526. The line: **if git or the forge knows it, do not store it.**

**One gather, extended — not a cache per source.** A single snapshot of the plane at refresh
time holding repos, personas, todos and changes, with one timestamp; `ctx` hands a component
the slice it declared. Multiple caches means multiple refresh clocks that can disagree, and
this codebase has paid for that twice already. One snapshot makes "everything on screen is
from the same moment" true by construction rather than by luck.

**Five event kinds, closed: `key`, `click`, `scroll`, `focus`/`blur`, `resize`.**
Deliberately excluding `drag` — stateful, hardest to get right across terminals, and most
likely to fight tmux's own selection. Adding it later costs nothing; removing it after a
provider ships against it costs everything.

**`[frame] slots` and `[frame] density` keep working, mapped rather than removed.** `slots`
becomes shorthand for "place these built-in components on their default edges in this split
order" — lossless, because slot order IS split order, which is what the registry stores.
`density` becomes a named arrangement. A config break is the most effective way to stop
people upgrading, and charter is already installed and in use.

## 4g. Settled by grilling — round 4 (2026-08-25)

**One refresh, one timestamp, the existing debounce — measure before optimising.** Git is
the expensive part and is already bounded by `statusline._repo_states`' 5-second TTL; file
reads for personas, todos and changes are small beside it. The tempting per-source clock is
the multiple-clock problem of §4f reintroduced one layer down. If measurement later says
otherwise, that is a decision with evidence rather than a guess with complexity.

**Actions are fire-and-report, never blocking.** An action returns immediately having
*started* work; progress surfaces through `inflight`; the palette closes. A blocking action
in a TUI is indistinguishable from a hang, and the operator's only recourse would be the
escape hatch — for something working correctly.

This makes **#420 load-bearing**: inflight records have no `kind` field, which is why clones
and refreshes are invisible. It stops being a nice-to-have.

**Overlays are `display-popup` with charter drawing the contents.** tmux 3.2+ runs a command
in a floating pane; charter runs its renderer there with its own tty and its own input,
mouse included. tmux keeps window management; charter owns the rectangle's interior. It
aligns with an existing floor — `SESSION_ENV_FLOOR` is `(3,2)`, and `display-popup` landed
in the same release. **`below_floor_message` promises charter still launches below 3.2**, so
that band needs a full-pane fallback. Like §4c's mouse routing, this is a hypothesis to
measure — confirm what `display-popup` does with mouse reporting and focus, on both servers,
before the palette is built on it.

**Provider compatibility is a single integer, refused at load.** Not semver negotiation, not
best-effort shims. Charter bumps the integer when the contract changes; a provider declaring
a different one does not load, and charter names the provider, the version it wants and the
version charter speaks. Loading-and-hoping means failure at render time inside someone's
frame; refusing at load is visible, actionable, and happens before anything is on screen. It
also makes the contract's cost real to us — bumping breaks every provider, which is the
friction that stops the contract churning.

## 4h. Settled by grilling — round 5 (2026-08-25). Frontier closed for Phases 1–2.

**An id collision refuses both providers, names both, and draws the rest of the frame.**
Silently picking one means the frame shows something whose origin cannot be determined, and
"which of my two plugins drew this" is a debugging problem with no entry point. Ids are
namespaced by distribution (`acme.metrics`), so a true collision is a mistake worth
surfacing rather than resolving by load order.

**Composition is one level.** A component is either a leaf or a composite of leaves.
Arbitrary nesting is the layout engine §4d refused, wearing a different hat. One level covers
every case on the table — the sidebar, and the bottom's status row plus table — and if a
second level is ever genuinely needed, that is evidence for real in-pane layout rather than a
reason to arrive at it by recursion.

**`F2` becomes the palette; the menu ceases to exist as a separate thing.** Not a new key —
the menu was always trying to be a palette. Per-component toggles get their own keys;
everything else is reachable through the palette. Keeping both would leave two answers to
"how do I do a thing", which is how the single menu became weird in the first place.

---

**Nineteen decisions, five rounds, 2026-08-25. The design frontier for Phases 1 and 2 is
empty.** What remains is Phase 4 (what identifies a change on disk, and its lifecycle) and
implementation detail that now follows from the above rather than needing a decision.

## 4i. Corrected by measurement — 2026-08-26

Task 1 of the Phase 1 plan measured the two hypotheses this spec was carrying. **Five were
refuted.** Findings in full: `docs/superpowers/specs/2026-08-26-tmux-input-findings.md`.

**§4c's drag-select claim is FALSE — delete it.** "which also preserves the drag-select the
comment worried about losing" is wrong. tmux enables mouse reporting on the outer terminal
from the **active pane's mode alone**: with a non-mouse pane active it writes
`1006l 1000l 1002l 1003l` to the client; `select-pane` onto a mouse-requesting pane and it
writes `1006h 1000h`. **There is no state where charter's panels are clickable and native
selection survives.** The trade `instance.FRAME_FIELDS` describes is real whichever way
`mouse` is set; leaving tmux's mouse off only makes it conditional on focus.

**§4c's omitted condition, and it changes Phase 2.** Charter's panels receive pointer events
only while the **active** pane requests reporting — and charter does not control what the
harness requests. So with `[frame] mouse` off, "clickable panels" is not a property charter
can promise.

**But this is a new argument FOR the popup, which the spec had not made:** a palette drawn in
a `display-popup` sidesteps the problem entirely, because the popup **is** the active surface
and its own request is what reaches the terminal.

**§4g's popup floor is 3.3, not 3.2.** `display-popup` appeared in 3.2, but on 3.2 **any
client resize kills it** — measured `rc 129` (SIGHUP), log ending mid-stream. 3.7c instead
delivers SIGWINCH and survives (CHANGES 3.2a→3.3: "Do not close popups on resize, instead
adjust them to fit"). **Recommendation, not yet a decision:** a full-pane palette everywhere
with the popup as an enhancement gated at 3.3 — rather than a popup with a fallback, which is
two surfaces to keep in step plus a resize-shaped bug on exactly one version.

**A popup's own program never receives focus events.** Measured on 3.7c and 3.2 with the
client's focus genuinely toggling. Only the pane **underneath** gets them, and only from 3.6.
Any design wanting a popup palette to know it lost focus needs another mechanism.

**§4f's `focus`/`blur` does not exist yet.** `focus-events` is off by default in tmux and
gates the whole path — and with it off, `#{client_flags}` still reads `attached,focused`, so a
guard written against that flag **passes with the feature dead**. Charter's `conf_text` does
not set `focus-events on` today.

**§4f's `click` must admit a release with no press.** Measured with `mouse` off: a drag
beginning on a border and ending in a pane delivers exactly one release,
`b'\x1b[<0;70;4m'`. The first third-party component that keeps press state wedges on it.

**§2 blamed tmux for charter's own cap.** "a nine-row cap with no way to page" — tmux 3.1c
drew 20 rows fine. The cap is `frame/menu.py:434`, and those rows are still drawn and
arrow-selectable; they lose only a digit shortcut. The rest of that sentence stands.

**Confirmed right:** §4c's per-pane routing mechanism works identically on 3.1c, 3.2 and 3.7c
(each pane receives its own rectangle's events, pane-relative, active pane unchanged); a popup
has its own tty, can request mouse, receives click and scroll popup-relative, owns the
keyboard, and is modal; charter's session-scoped `mouse off` cleanly overrides an operator's
global `mouse on` with no leak; and **all four `tmuxctl` version floors were confirmed by
running 3.1c and 3.2**, where before they were justified from CHANGES with a note that no
binary existed to check.

### A safety principle found running backwards in the code — 2026-08-26

**`Registry.place` ignored the committed `edge` and `size` whenever the provider actually
loaded**, applying them only to the standin drawn when it did not. Measured, before the fix:

```
place("acme.metrics", edge="top", size=Fixed(3))
  package installed  -> edge='right', size=Fixed(12)   (the provider's own declaration)
  package absent     -> edge='top',   size=Fixed(3)    (what the arrangement asked for)
```

So one committed `[[frame.component]]` table drew two different frames on two machines, and
`on_edge('top')` answered `['acme.metrics']` on one and `[]` on the other — and order is
geometry, so every panel split after it moved too. **This is §4b's own principle inverted:**
the provider's *code* overruled the *committed arrangement*, which is execution deciding
arrangement. `default_edge`/`default_size` are for a sensible arrangement before anyone
configures one; they do not beat one that exists.

**It was unasserted in both directions and 37 tests stayed green with it fixed.** The
standin half was pinned; the loading half — the half that runs on the machine the provider
is actually installed on — was pinned nowhere, and `place`'s own docstring asserted the
opposite of what it did. Fixed by resolving the rectangle in ONE function both paths call,
with `None` and only `None` meaning "the caller did not say"; the mirror test asserts edge,
size, split order and `on_edge` for a provider that loads, and asserts the two paths give
one answer in a single case so neither can be fixed without the other.

**The general lesson, which is the reusable half:** a guard written on the degraded path and
not on the succeeding one passes every test the degraded path has, and the succeeding path
is the one operators run. Two code paths that must agree need a test that asks *both in one
assertion*, not one test each.

### And a stale number, corrected

**The "200 versus 154" measurement this spec repeated in §4 and §4b is dated.** charter's
committed measurement for the slot set it ships **today** is **200 versus 177**:
`["top","bottom","right"]` gives a 200-column bottom row; `["top","right","bottom"]` gives 177,
inset beside one 22-column sidebar plus its border. **154 was the pre-#488 arrangement** — two
sidebars and two borders — and #488 deleted that comment along with the `left` slot.

The decision is untouched; only the number illustrating it moved. **The number moved when a
panel was retired; the property did not, and the property is what the registry stores.**

### One extension of a settled decision, flagged for overruling

`component.NEEDS` ships as `("gather", "repos", "todos")` — the slices `gather.scan` actually
carries — not the four names §4f anticipates. `personas` and `changes` join when the extended
gather can serve them.

The reason is the rule this project keeps relearning: a name accepted by `needs` that `ctx`
answered with an empty tuple would let a component declare it, draw nothing, pass its own tests
against an empty fixture, and be **indistinguishable from a plane that genuinely has none**. A
convincing empty is worse than a refusal — the same defect as the "no repos" panel #512 fixed.
`ctx.SERVES` and `component.NEEDS` are asserted against each other so they cannot drift as the
gather grows.

---

## 4j. Many chats, many workspaces, many harnesses — settled 2026-08-26

The IDE holds **workspace tabs**, and under them **chat tabs**. Switching a workspace does not
lose the chats open in it. Each chat picks its harness when it starts, so several harnesses run
in parallel in one IDE.

### Most of this already exists

Three facts found before designing anything:

- **`state.frame_id(workspace, pid)` already mints `{workspace}-{pid}`** — the naming scheme
  this idea proposed is already the frame id.
- **`state.record_harness_session(fid, sid)` / `harness_session(fid)` already exist** — charter
  already records which harness session belongs to which frame.
- **tmux already provides the persistence.** A server holds sessions; detaching does not kill
  them. "Do not lose the open chats" is not a feature to build, it is a property to stop
  discarding.

So this is not a rewrite. It is: let a workspace hold **several** frames, and put a selector
over them.

### The four decisions

**A chat is a tmux WINDOW, on one session per workspace.** Switching chats is `select-window`;
switching workspaces is `switch-client -t`. Both preserve everything and neither reattaches the
client. A session-per-chat would force a client reattach on every tab switch, which is visible
to the operator.

**Panels duplicate per chat for now — measure before optimising.** Panes belong to windows, so
N chats means N sets of panel processes, each rendering the same plane state. Four panels at one
`stat` per tick is trivial; the real cost is N× the *rendering*, and `_right` measured 4.8 ms.
**Ship it duplicated, measure at 5 and 10 chats, then decide.** Building shared panels
speculatively is the layout engine §4d refused, arriving through the back door.

**Charter names its own container and records the mapping — it never enumerates the harness's
sessions.** Asking each harness "what sessions do you have" is harness-specific work that fights
the point of being harness-agnostic, and Claude Code, codex and opencode store sessions three
different ways. Charter knows what it started, and `record_harness_session` already records it.

**A chat belongs to its workspace for life.** `{workspace}-{hash}` is identity, not a property.
Moving a chat between workspaces sounds convenient and means the harness's own context — its
cwd, its files, its history — is suddenly about a different plane. A conversation wanted
elsewhere is a new chat.

### What is genuinely new

- A **workspace tab bar** and a **chat tab bar**, both components in the Phase 1 registry.
- The chat bar **hides itself when there is one chat**, showing only "add chat" — the tab bar
  earns its row only when there is something to choose between.
- **Harness selection at chat creation**, which is the first place charter asks rather than
  detects.
- A frame model that holds **several** frames per workspace instead of one, and a switcher that
  moves between them without tearing anything down.

### Sequencing — Phase 5, after the command surface

The tab bars **are** components and the switcher **is** an action, so this becomes materially
cheaper once Phases 1 and 2 exist. Built before them it would need its own chrome, its own
key handling and its own selector, all of which Phase 2 provides.

It also interacts with **§3.2's cross-repo change**: a change spans repos, a chat happens in a
workspace, and the obvious next question — "show me the chats working on this change" — needs
both to exist first. Phase 5 does not answer it; it makes it askable.


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

### Phase 5 — many chats, many workspaces, many harnesses

Workspace tabs, chat tabs, harness-per-chat, and a switcher that loses nothing. See §4j — the
naming, the harness-session record and tmux's own persistence already exist; what is new is
holding several frames per workspace and the two tab bars, both of which are Phase 1 components
and Phase 2 actions.

**Exit test:** ten chats across three workspaces, switching between any two in one keystroke,
nothing torn down, and the panel cost measured at 5 and 10 rather than assumed.

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
