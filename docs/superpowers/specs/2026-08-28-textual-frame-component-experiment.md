# The Textual experiment — one frame component, drawn by a real widget framework

**2026-08-28.** One charter frame component — the repo table — rendered with
[Textual](https://github.com/textualize/textual) and shipped as a third-party provider
through the existing `charter.components` entry point.

The package is `providers/charter-textual-repos/`. It is **not** in `charter/`, it declares
`textual` as its own dependency, and **charter's tree is unchanged**: `dependencies = []`
still holds, `tests/test_packaging.py::test_runtime_has_zero_dependencies` still passes,
and the full suite passes in two environments (counts at the end).

Everything below was measured. The scripts are in `providers/charter-textual-repos/measure/`
and each finding names the one that produced it. Every tmux claim is against real tmux
3.7c, one socket per script, killed before unlinking.

---

## What was built

Two components, because the experiment is a comparison and one of them could not exist.

| id | shape | `render` |
|---|---|---|
| `textual.repos` | **adapter** — Textual on the headless driver, on a background thread; its composited screen copied out as lines | returns, per repaint, like every other component |
| `textual.live` | **takeover** — Textual on the pane's own tty, alternate screen, own asyncio loop, `mouse=True` | never returns |

Both draw the same table off `ctx.gather`: repo name, branch, dirty/ahead/behind markers,
CI, open change, worktree badge, pieces indented underneath. Both are discovered by
charter with no change to charter:

```
$ python -c "from importlib import metadata; [print(ep.name,'->',ep.value,'|',ep.dist.name)
              for ep in metadata.entry_points(group='charter.components')]"
textual.live  -> charter_textual_repos:live_component  | charter-textual-repos
textual.repos -> charter_textual_repos:adapter_component | charter-textual-repos
```

and both are placeable from a committed `charter.toml`, resolved end to end into
`layout.panel_command`'s argv:

```toml
[[frame.component]]
use = "textual.live"
edge = "bottom"
size = 12
key = "F9"
```
```
components:
  {'use': 'textual.live', 'slot': 'textual.live', 'edge': 'bottom',
   'size': Fixed(n=12), 'visible': True, 'key': 'F9'}
slots: ['top', 'bottom', 'textual.live']
argv: [... 'panel', 'textual.live', '--session', 'f']

# the same charter.toml on a machine WITHOUT the provider installed:
components: []
slots: ['top', 'bottom', 'repos', 'right']
```

That is §4b property 4 working exactly as designed. **The extension model is real.**

It is a real distribution, not a `sys.path` trick: `uv build` produces
`charter_textual_repos-0.1.0-py3-none-any.whl`, and installing that wheel into a **Python
3.12** venv beside charter — no editable install, no `PYTHONPATH` — gives the same two
entry points and the same two panes (`m7_shot.sh` under `venv312`, 14 provider tests pass).

---

## 1. Did the `Component` contract fit?

**The declaration half fits. The rendering half fits one of the two shapes. The input half
does not exist.** Seven places it chafed, each named.

### 1a. `render(ctx) -> list[str]` is a *renderer* contract, and a widget framework is not a renderer

`component.Component.render` is `Callable[[Any], list[str]]`. `Registry.draw` calls it,
type-checks what came back, escapes it, clips it to the rectangle and hands it to
`panel._write`. A framework that owns the terminal has no seam there. The two ways out are
the two components in this package, and the second one leaves the contract:

* the **adapter** obeys the signature by running Textual where nobody can see it and
  copying its screen out;
* the **takeover** obeys the signature by never returning from it.

Nothing in the contract distinguishes "a component that draws" from "a component that owns
its pane", and the panel process has exactly one thread that a framework needs.

### 1b. A provider's lines are escaped, so a provider component is monochrome

`Registry.draw` ends in `_fit(lines, …, escape=cid in self._foreign)`, and every provider is
foreign. `escape=True` runs `contain.one_line`, which replaces every `Cc` character with its
own escape text — so `\x1b` arrives on screen as the four characters `\x1b`. The adapter
therefore returns `Strip.text` (plain) rather than `Strip.render(console)` (SGR), and
**every colour Textual computed is discarded at the boundary**:

```
m5_no_color.sh, SGR sequences visible in the pane:
                  colour on   NO_COLOR=1
  charter repos       54          0
  textual.live        15         15
  textual.repos        0          0
```

The zero in both columns is the finding. The comment in `Registry.draw` is right about
*why* — a provider's row could otherwise move the cursor out of its own rectangle — but the
consequence is not written down anywhere a provider author meets it: a third-party
component can never be as legible as charter's own, and the widget framework's entire
styling system is dead weight in the adapter shape.

### 1c. `Component.events` is declared, validated, and read by nothing

`grep -rn '\.events\b' charter/` returns two hits. One is
`commands_worktree.py:275`'s `pieces.events(ws)`, an unrelated function. The other is
`frame/component.py:320` — **the line that validates the field**. Nothing reads
`Component.events`. There is no dispatcher. `frame/overlay.py` does decode SGR mouse
reports and does know the modal rule, but it is charter's own palette surface and is not
wired to components at all.

So `EVENT_KINDS`' careful "declaring one of these is a declaration of what you HANDLE, never
a promise that it FIRES" is currently the weaker statement of the two: **none of them
fire, ever, for anybody.** `textual.live` declares `("key", "click", "scroll", "resize")`,
receives all four, and receives them by having taken the tty — charter neither delivers
them nor knows it is not delivering them. The adapter declares `()` and is the honest one:
a headless app has no terminal to read from, so its `DataTable` cursor cannot be moved and
its scrollbar cannot be reached. **A scrolling table that cannot scroll is the one thing
the widget was worth having.**

### 1d. `ctx` is one snapshot with no way to ask for another — which is right, and it is what freezes the takeover

`frame/ctx.py` is explicit and correct: "A component never gets a way to fetch a fresher
one: refreshing is the frame's decision, on the frame's clock." For the adapter that costs
nothing, because charter calls `render` again. For the takeover it is fatal — `render` never
returns, so nothing ever calls it again, and the table is frozen at the instant the pane
started. Measured (`m6_repaint.sh`), across one `gather.save` + `state.bump` that flips CI
from `failed` to `passed`:

```
                 before        after the bump
  repos          ✗ failed      ? passed          charter's own renderer
  textual.repos  fail          ok                the adapter — render returns
  textual.live   fail          fail              frozen; snapshot 6s old and climbing
  live+refresh   fail          ok                snapshot 2s old
```

That last row is `CHARTER_TEXTUAL_LIVE_REFRESH=1`, and it is the measurement of what
un-freezing costs: the app imports `charter.frame.state` and `charter.frame.gather` and
polls them itself. It works, and it reimplements inside a provider four things that are
charter's — which file carries the version, that comparing it is how a panel learns
anything happened, which workspace the *frame* resolved (#526), and that `gather.read` is
the cache rather than a scan. A provider that gets any of those wrong gets it wrong quietly:
a pane that stops updating, or a full `gather.scan` five times a second in every pane on the
machine. **`ctx`'s one-snapshot rule is right; what is missing is a way for the frame to
hand a long-lived component the next one.**

### 1e. A provider can never be `Content()` or `Fill()` where anyone has configured it

`instance.component_tables` requires a provider's `[[frame.component]]` table to carry both
`edge` and `size`, and coerces `size` to `Fixed(n)`. The reasoning is sound and is
`config.FRAME`-is-on-the-import-path — but the effect is that `Content(cap=…)` and `Fill()`
are reachable only on a frame nobody has arranged, i.e. from `Registry.place` with no
rectangle. charter's own `repos` is `Content()` and its height is what the plane's repos
need; a provider's table of the same data is a fixed rectangle or nothing.

### 1f. A provider directory inside charter's repo costs exactly one line, and CI is what says so

Committing `providers/` failed `test_plugin_freshness.
TheHashCoversWhatThePluginLoads.test_every_top_level_directory_is_classified_one_way_or_the_other`
on 3.11 and 3.13:

```
AssertionError: Items in the first set but not the second:
'providers' : new top-level directory ['providers']: does a Claude Code plugin LOAD it?
Add it to plugincache.PLUGIN_SURFACE, or to _NOT_PLUGIN_SURFACE in this file.
```

That is the check doing exactly its job — its own docstring says "add `mcp/` or `commands/`
to charter tomorrow and this fails on the PR that commits it, until somebody decides which
it is". The decision is made in `_NOT_PLUGIN_SURFACE`: a provider is a separate
distribution, nothing under it is importable from charter, none of it ships in the wheel,
and it must never become plugin surface, because a provider declares dependencies charter
does not have and that is the whole property `test_runtime_has_zero_dependencies` protects.

**Worth recording is how it was found, not that it happened.** The classification reads the
**git index**, deliberately (#529, so an untracked `.charter/` or `.venv/` cannot reach it).
Every local full-suite run before the commit therefore passed — `providers/` was untracked —
and the failure appeared on the first CI run after `git commit`. The local suite was re-run
on the committed tree afterwards; the counts at the end are from those runs. This is the one
place in the experiment where CI caught something a hand-run could not.

**And two places the directory travels anyway, neither of which costs the property that
matters.** Charter's marketplace entry is `"source": "./"`, so the whole repository is copied
into every plugin user's cache — `tests/` and `docs/` already, and now `providers/` too; the
classification above is what keeps its *content* out of the freshness hash, so an edit to it
is not reported as a stale plugin. And hatchling's default sdist is the whole repository, so
`providers/` is 22 of the sdist's 985 entries, beside `tests/`' 356 and `docs/`' 183. Neither
reaches the wheel (`packages = ["charter"]`; `unzip -l … | grep -c providers` is 0), so
`pip install charter-cp` installs nothing of it and `dependencies = []` is untouched in
every direction. If this were ever more than an experiment, the sdist is where somebody
should decide whether 22 files of third-party-provider source belong in charter's release
artifact.

### 1g. The contract is a Python class, so every provider hard-depends on charter

`Providers._one` finishes with `isinstance(obj, self.kind)` where `kind` is
`charter.frame.component.Component`. A provider cannot express a component without importing
charter, so `charter-cp` is a real install-time dependency of this package. That costs
nothing at runtime — charter is already in the process asking — but the entry-point group
alone does not make the coupling loose, and it is worth being accurate about that.

---

## 2. `sys.stdout` — the one that would have shipped broken

This is the sharpest finding and it is not about Textual specifically. It is about who owns
`sys.stdout` in a panel process.

`textual/app.py:3491` wraps the app's message pump in
`contextlib.redirect_stdout(self._capture_stdout)`, so a `print()` inside a widget lands in
the app's log rather than on the screen. `redirect_stdout` assigns `sys.stdout` — a
**process-wide module global** — from the app's own thread, **headless or not**. And
`_PrintCapture` (`textual/app.py:260`) answers:

```python
def isatty(self) -> bool: return True     # "Pretend we're a terminal."
def fileno(self) -> int:  return -1       # "Return invalid fileno."
```

charter reaches `sys.stdout` in three places, and that object subverts all three:

| charter | what it does | what it gets |
|---|---|---|
| `panel._out` | `sys.stdout.write(payload); flush()` | the paint goes into Textual's print log; the pane keeps whatever was on it |
| `slots._width` / `slots._height` | `os.get_terminal_size(sys.stdout.fileno())` | `get_terminal_size(-1)` raises `OSError`, which is caught — charter silently falls back to **80x24** |
| `chrome.colour_ok` | `sys.stdout.isatty()` | `True`, whatever the panel's stdout actually is |

Measured symptom in a real 150x10 pane, with the workaround disabled
(`m8_stdout.sh`, `CHARTER_TEXTUAL_KEEP_CAPTURE=1`):

```
  0.5s  restored=10  captured=10
  2.0s  restored=10  captured=10
  2.5s  restored=10  captured=1    <- version bumped at 2.0s
  5.0s  restored=10  captured=1
                                    (non-blank rows in each pane)

== the captured pane, in full, after the bump ==
  |
  |            (nine blank rows)
  | snapshot 0s old  ·  clicks 0  ·  scroll 0  ·  keys 0  ·  headless

== and nothing was reported: pane alive, no charter error, no stderr ==
  %2 dead=0 cmd=python3.14
```

The chain is all three rows of that table at once: charter measures 80x24 instead of
150x10, the component lays itself out 24 rows tall, `panel._write` clamps to 24 and writes
all of them into a 10-row pane, and the pane scrolls its own content away. **Nothing
raises. `Registry.draw` has nothing to catch, because nothing failed.** The first paint is
correct — the app is not up yet — which is exactly the shape of bug that ships.

The provider's fix is four lines (`adapter._give_stdout_back`), in `render`'s `finally`
rather than on the way in, because `panel._component_text` evaluates `slots._width()` and
`_rows()` **as arguments to `ctx.build`**, i.e. before `render` is called. Restoring on
entry fixes the paint and leaves the measurement one tick stale forever.

**This is a charter finding, not a Textual one.** Any provider that uses any library that
touches `sys.stdout` — a progress bar, a logging handler, `rich.Console(file=…)`, a
debugger — lands somewhere on this table. §4b's four properties are all downstream of a
`sys.stdout` a provider is able to take.

---

## 3. Does the alternate screen cost charter anything?

**No. Measured, and it costs less than expected** (`m2_altscreen.sh`).

```
== alternate-screen flag (tmux's own view) ==
%0 bash        alternate_on=0        the harness
%2 python3.14  alternate_on=1        charter panel textual.live
%1 python3.14  alternate_on=0        charter panel repos

== history size per pane ==
%0 history_size=191   harness
%2 history_size=0     textual.live
%1 history_size=0     charter's own repos

== capture-pane -S -200 (scrollback) non-blank lines ==
harness pane  : 204
charter repos : 6
textual live  : 10
```

* **Pane scrollback**: `history_size=0` for the Textual pane — and **also for charter's own
  panel**, which clears with `\x1b[H\x1b[2J` and never scrolls. The alternate screen costs
  nothing charter's own panels were not already costing. The harness pane, which is the one
  anybody scrolls, is untouched.
* **`capture-pane`**: works normally, `-p` and `-p -e` alike. tmux captures the alternate
  screen buffer; the `-e` capture carries Textual's real 24-bit SGR.
* **Resize**: `resize-window 150x40 -> 100x30 -> 150x40` — the pane goes 150x14 → 100x11 →
  150x14 and Textual reflows correctly each way, longest captured line 76 cells throughout.
  Textual installs its own SIGWINCH handler over charter's; charter's `_watch` is parked
  inside `render` and has nothing to repaint anyway.
* **Death**: `kill -9` with `remain-on-exit on` leaves `dead=1 alternate_on=1` and the pane
  still showing Textual's content, minus the one line tmux scrolls off to write its own
  message — identical to what `frame/panel.py`'s docstring measures for any pane.
* **Respawn**: `respawn-pane -k … charter panel textual.live --session …`, the argv
  `commands_frame.cmd_respawn` emits, brings the pane back clean: `dead=0 alternate_on=1`,
  table drawn.

---

## 4. What does a crash cost?

**Its own pane, every time — for three of the four failure modes.** The fourth loses the
reason (`m3_crash.sh`, `m3b_crash_message.sh`). In every case charter's own `repos` pane
beside it and the harness pane above it were untouched.

| failure | the pane says |
|---|---|
| `render` raises before Textual starts | `textual.live failed to draw — RuntimeError: injected fault before the app started` |
| provider's module raises on import | `charter-textual-repos 0.1.0 supplies textual.live, and importing charter_textual_repos raised ImportError: … Its pane says so; the rest of the frame is drawn` |
| id nothing supplies | `charter: unknown slot 'notinstalled.thing' (known: bottom, repos, right, top)` |
| **raises inside Textual's message pump** | `textual.live: app exited after 1s (clicks 0, scroll 0, keys 0)` |

That last row is the finding. Textual **does not let the exception out of `run()`**: it
catches it, prints a Rich traceback (with locals) to `sys.__stderr__`, and returns normally.
So `Registry.draw` sees a successful render, charter's "failed to draw" never appears, and
`panel._write` clears the pane a moment later with whatever `render` answered. Sampled every
100 ms across the crash, the traceback is never visible even once:

```
 1000ms |  repos 5  ·  harness-wrapper
 1100ms |                                              <- crash + traceback + charter's clear
 1200ms | textual.live: app exited after 1s (clicks 0, scroll 0, keys 0)
```

Confirmed out of tmux with stderr redirected: the app writes 19,358 bytes to **stderr**
(Textual's driver is `self._file = sys.__stderr__`), the last 2,520 of them a Rich traceback
with locals ending `RuntimeError: injected fault inside Textual's message pump` — while
stdout carries the two lines charter painted over it. **A crash inside a framework's own
loop is indistinguishable, from charter's side, from a clean quit.**

Two smaller ones from the same run:

* **`NO_COLOR` is not honoured by a takeover component.** `textual/drivers/linux_driver.py:58`
  sets `self._file = sys.__stderr__` and the app writes straight there — past
  `panel._write`, past `chrome.plain`, past `sys.stderr` itself. `panel._write`'s claim to
  be "the one place anything reaches a pane's screen" is true for the contract and false for
  a component that takes the pane. Numbers in §1b.
* **A thread-vs-mount race the provider hit and charter contained.** The adapter drives the
  app from another thread, so a repaint can land between `compose` and `on_mount`;
  `add_row` then raises `More values provided than there are columns`, and the pane read
  `textual.repos failed to draw — ValueError: …`. Charter behaved perfectly. The provider
  still shipped a broken pane until the columns were declared from whichever side asks
  first.

---

## 5. Startup

`split-window` to first non-blank paint, polled at 10 ms against real `capture-pane`, five
runs each (`m1_startup.sh`):

```
repos            174ms 115ms 130ms 142ms 113ms
textual.repos    339ms 273ms 242ms 241ms 245ms
textual.live     231ms 212ms 228ms 272ms 262ms
```

**Roughly 2x charter's own, ~110-130 ms extra, paid once per pane.** Consistent with the
import measurement (`m9_cost.py`, one fresh interpreter each):

```
charter.frame.slots     31 ms   25 MB peak RSS
textual.app            102 ms   39 MB peak RSS
charter_textual_repos  128 ms   43 MB peak RSS
```

Per-repaint, through `Registry.draw`, same rectangle, same snapshot, 8 repos:

```
charter _table_lines   median 0.38 ms   worst 0.70 ms
textual.repos draw     median 4.79 ms   worst 6.41 ms
```

12x, and both are two orders of magnitude under `panel.TICK` (200 ms). **Cost is not the
objection**, exactly as the brief said. The shape differs — a full compositor pass over a
widget tree versus a string join — but neither is close to the budget.

---

## 6. Mouse — measured against real tmux, and the premise is half right

`m4_mouse.py` attaches a real tmux client on a pty it owns and reads every byte tmux writes
to a terminal. tmux 3.7c, session `mouse off` (charter's default).

### The modal behaviour is real

```
select-pane -> textual.live : 1006l 1000l 1002l 1003l 1006h 1000h 1002h 1003h
select-pane -> harness      : 1006l 1000l 1002l 1003l
select-pane -> textual.live : 1006l 1000l 1002l 1003l 1006h 1000h 1002h 1003h
```

A pane running `App.run(mouse=True)` makes tmux turn the terminal's mouse reporting **on
when that pane is active and off when it is not**, with tmux's own `mouse` off throughout.
§4i's measurement extends exactly as predicted to a pane that genuinely wants the mouse.

### The keyboard reaches it too, and charter has no path for that either

```
after j j k                 : clicks 0  ·  scroll 0  ·  keys 3
```

Three `send-keys` and the app's own counter moves. It moves because the app read its own
tty, not because anything routed anything: `textual.live` declares `events=("key", …)` and
charter validates that tuple and never reads it again (§1c).

### And mouse events reach it — but not only when it is active

```
before                      : clicks 0  scroll 0
click, textual.live active  : clicks 1  scroll 0
wheel, textual.live active  : clicks 1  scroll 2
same clicks, harness active : clicks 2  scroll 3     <- still delivered
active pane after that click: %0 (harness is %0)     <- and the click did not focus it
```

**tmux routes a mouse report by POSITION, not by which pane is active.** The modality is
entirely in the *terminal's reporting mode*, which the active pane alone sets. So:

* the Textual pane gets clicks whenever *anything* has turned reporting on — including the
  harness, and a full-screen TUI harness is exactly the kind of program that does. Confirmed
  by having the harness pane print `?1000h ?1006h` for itself:
  ```
  select-pane -> harness (after it printed ?1000h ?1006h) : 1006l 1000l 1002l 1003l 1006h 1000h
  ```
* a click on the Textual pane does **not** make it active, so the operator has to focus it
  with the keyboard before the modal behaviour helps at all.

### Does it dissolve the trade?

**No. It relocates it, and adds a step.** While the Textual pane is active, the terminal is
reporting and the operator's native text-selection is gone **for the whole window**, not
just for that pane — the modes tmux writes are the terminal's, not a pane's. The harness
pane's own scrollback survives untouched (`history_size = 181` throughout, last line intact),
so nothing is *lost*; what is unavailable is dragging to select while that pane has focus.

So the honest statement is: `[frame] mouse` can stay `False`, a Textual panel can request the
mouse for itself, and the result is a panel that is clickable **once you have focused it with
a key** and that costs you selection **while you have**. That is better than
`[frame] mouse = true`, which costs it unconditionally. It is not the free lunch the open
question hoped for, and `EVENT_KINDS`' advice — give every pointer affordance a `key` as
well — survives this measurement unchanged.

---

## 7. Does the version-bump model coexist with Textual's loop?

**They do not fight — because in the takeover shape only one of them is running.**

`render` blocks inside `Registry.draw`, so `panel._watch` never reaches its next `_tick`,
never calls `state.version`, and never calls `_write`. There is no clear-screen underneath
Textual, no flicker, no torn frame. The cost is §1d: charter also never hands the component
another `ctx`.

The adapter is the opposite and is entirely well-behaved: charter's loop drives, the app is
a rendering service on a background thread, and a version bump updates the pane within one
tick (§1d's table, row 2). The idle cost is unchanged — the panel still pays one `stat` per
tick, because `render` is only called when something moved.

There is a third shape that looks like the obvious compromise, and it is refuted rather than
merely rejected (`m10_thread_takeover.py`): `render` starts a **non-headless** Textual app on
a background thread and returns `[]` immediately, so charter's loop keeps running and keeps
handing the component fresh snapshots while Textual owns the pixels.

```json
{
  "raised_out_of_run": [],          <- App.run() returned normally
  "started": true,
  "bytes_to_the_pane": 16747,
  "pane_got_a_traceback": true,
  "reason": "ValueError('signal only works in main thread of the main interpreter')"
}
```

`LinuxDriver.__init__` calls `signal.signal(SIGTSTP, …)` and `signal.signal(SIGCONT, …)`
unconditionally, and Python refuses that off the main thread. Textual catches it, prints
16,747 bytes of Rich traceback into the pane, and `run()` **returns with nothing raised** —
the same swallow as §4's fourth row, one layer earlier. Even if that were fixed upstream,
`panel._write` would then clear the alternate screen out from under the app on every version
bump.

**A component either owns the pane or returns lines. There is no in-between, and charter's
contract only describes one of them.**

---

## 8. Would I ship it?

**No — not this, and not yet.** Three reasons, in order of how much they matter.

**1. The adapter is the only shape charter can host, and it is strictly worse than what
charter already has.** No colour (§1b), no input (§1c), 12x the repaint cost and 2x the
startup for a table charter draws better. Every advantage a widget framework has — scrolling
a table longer than the pane, a cursor, click-to-open, styled cells — is on the far side of
either `Registry.draw`'s escaping or a dispatcher that does not exist. There is nothing left
to buy.

**2. The takeover works, looks good, and is outside the contract in a way that is not a
detail.** It draws well (§9), survives resize, respawn and death cleanly, and its crash costs
one pane. But it freezes on the first snapshot, it silently ignores `NO_COLOR`, its crashes
lose their reason, and the only way to make it live is to import charter's internals from a
stranger's package. Shipping it would be shipping "components may own their pane" as
folklore, discovered by whoever reads this package, rather than as a contract with rules.

**3. The `sys.stdout` finding is the one that must land first, and it is charter's.** §2 is
not a Textual problem. Any provider using any library that touches `sys.stdout` gets a pane
that paints once and then goes blank, with nothing raised, nothing logged, and a silent
80x24 geometry fallback underneath. Today the four properties in §4b rest on a global a
provider can take by accident.

### What it would take

In rough order of value, and none of it is large:

1. **Make charter's pane immune to `sys.stdout`.** Capture the panel's real fd once at
   startup and write, measure and `isatty` through *that*, not through `sys.stdout`. Three
   call sites: `panel._out`, `slots._width`/`_height`, `chrome.colour_ok`. This is worth
   doing whether or not anything below happens — it is a latent silent-blank-pane bug for
   every provider, not only this one.
2. **Decide what an escaped provider line means.** Either say plainly in `EVENT_KINDS`'
   neighbour ("a provider component is monochrome, and here is why") or give providers a
   narrow, validated colour channel — charter's own markup, sanitised the way `tui.sanitize`
   already sanitises everything else. The current state is a capability that looks available
   and is not.
3. **Deliver `events`, or delete the field.** A closed vocabulary that is validated and
   never dispatched is a promise the code does not keep. `frame/overlay.py` already decodes
   SGR mouse reports and already knows the modal rule; the missing piece is routing to a
   focused component, not new mechanism.
4. **Only then, and only if 1-3 land: a second component shape** — `owns_pane = True`, or a
   `Component` whose `render` is replaced by a `run(ctx, updates)` handed a channel the frame
   pushes fresh snapshots onto. That is the one thing that would make Textual worth its
   dependencies, because it is the only thing that makes the widget's advantages reachable.

**What the experiment proves, and it is the important part:** the provider model itself is
sound. A package outside `charter/` declaring ten dependencies charter does not have was
discovered, placed from a committed file, split into a pane, drawn, crashed four ways, and
never cost the frame anything but its own pane. `dependencies = []` was never in question.
The gaps found are in the *component* contract, not in the extension model, and every one of
them is a few dozen lines from being closed.

---

## 9. What it looks like

Real `capture-pane -p -e`, one pane per block, top to bottom, tmux 3.7c, 150x44, session
`mouse off`. Escapes shown as `\e`; nothing else altered. Produced by `measure/m7_shot.sh`.

The `alternate_on` column is the whole architectural difference between the two components,
sitting side by side in one frame.

```
┌─ %0  rows 0-3  150x4  alternate_on=0  the harness (stand-in)
│ The default interactive shell is now zsh.
│ To update your account to use zsh, please run `chsh -s /bin/zsh`.
│ For more details, please visit https://support.apple.com/kb/HT208050.
│ bash-3.2$
┌─ %5  rows 5-16  150x12  alternate_on=1  panel textual.live
│ \e[48;2;36;47;56m \e[38;2;226;227;229mrepos 5  ·  harness-wrapper\e[39m
│ \e[1m\e[38;2;224;224;224m\e[48;2;45;55;64m repo                 branch                      ci    change
│ \e[38;2;33;21;5m\e[48;2;254;166;43m ▸ charter ⑂3         main               * ↑2     fail  !554
│ \e[0m\e[38;2;224;224;224m\e[48;2;39;39;39m   easydmarc-app      feat/dmarc-rollup  ↓4       ok
│    infra              main                        run   #88
│    charter-docs       docs/frame         ? ↑1 ↓1  ok
│    statusline-lab ⑂2  main
│    ╰ pr-554                              ? ↑1     …
│    ╰ pr-560                                       ok    !560
│ 
│ 
│ \e[39m\e[48;2;36;47;56m \e[38;2;167;171;175msnapshot 4s old  ·  clicks 0  ·  scroll 0  ·  keys 0  ·  live · frozen\e[39m
┌─ %4  rows 18-29  150x12  alternate_on=0  panel textual.repos
│  repos 5  ·  harness-wrapper
│  repo                 branch                      ci    change
│  ▸ charter ⑂3         main               * ↑2     fail  !554
│    easydmarc-app      feat/dmarc-rollup  ↓4       ok
│    infra              main                        run   #88
│    charter-docs       docs/frame         ? ↑1 ↓1  ok
│    statusline-lab ⑂2  main
│    ╰ pr-554                              ? ↑1     …
│    ╰ pr-560                                       ok    !560
│ 
│ 
│  snapshot 0s old  ·  clicks 0  ·  scroll 0  ·  keys 0  ·  headless
┌─ %3  rows 31-39  150x9  alternate_on=0  panel repos
│ \e[2m▪ \e[0;1mrepos\e[0;2m 5\e[0m
│   \e[2m├─ \e[0;1;4m\e[35mcharter\e[0;2m ⑂3\e[0m                        \e[33mmain*\e[36m↑2\e[39m                             \e[31m✗ failed\e[39m      \e[32m!554\e[39m
│   \e[2m├─ \e[0m\e[34measydmarc-app\e[39m                     \e[2mfeat/dmarc-rollup\e[0m\e[34m↓4\e[39m                 \e[2m? passed\e[0m
│   \e[2m├─ \e[0m\e[36minfra\e[39m                             \e[2mmain\e[0m                                \e[36m● running\e[39m     \e[32m#88\e[39m
│   \e[2m├─ \e[0m\e[32mcharter-docs\e[39m                      \e[33mdocs/frame*\e[36m↑1\e[34m↓1\e[39m                     \e[2m? passed\e[0m
│   \e[2m└─ \e[0m\e[33mstatusline-lab\e[2m\e[39m ⑂2\e[0m                 \e[2mmain\e[0m
│ 
│ 
│ 
┌─ %2  rows 41-41  150x1  alternate_on=0  panel attention
│ 0 todos · \e[33m⚠\e[39m \e[2mreinit\e[0m 9 \e[2mws · charter ws reinit --all\e[0m · F2 palette
┌─ %1  rows 43-43  150x1  alternate_on=0  panel identity
│  ⬢ default  \e[35m◆\e[39m \e[1msteward\e[0;2m · ◇ personas forge · reddit · release · statusline\e[0m                                                           charter 0.53.0 \e[2mdev\e[0m
└─ tmux 3.7c · window 150x44 · session mouse off · python 3.14.2
```

Three things to look at:

* `%5` is `alternate_on=1` and carries Textual's real 24-bit SGR — a widget framework
  running in a pane charter split, at charter's own repaint clock's mercy and never
  needing it.
* `%4` is the same widget tree through charter's contract and has **not one escape in
  it**. That is §1b, visible: `Registry.draw` escapes what a provider returns, so the
  adapter can only ever hand back `Strip.text`.
* `%3` is charter's own `repos`, in colour, from the same `gather` snapshot — the thing
  the experiment set out to reimplement, and the one that still reads best.

---

## Reproducing

```sh
uv venv --python 3.14 .venv
uv pip install --python .venv/bin/python -e . -e providers/charter-textual-repos
uv pip install --python .venv/bin/python pytest
.venv/bin/python -m pytest -q providers/charter-textual-repos     # 14 passed

M=providers/charter-textual-repos/measure
VENV="$PWD/.venv" WT="$PWD" bash $M/m1_startup.sh                  # and m2 … m8
VENV="$PWD/.venv" WT="$PWD" bash $M/m7_shot.sh esc                 # the capture in §9
.venv/bin/python $M/m9_cost.py                                     # no tmux needed
.venv/bin/python $M/m10_thread_takeover.py                         # writes /tmp/m10_*.json
```

Each script owns one tmux socket named `charter-textualexp-<pid>` and kills the server
before unlinking the file, in one trap — `tests/_tmuxreap.py`'s discipline, and the name is
in charter's namespace so the suite's own reaper will also collect it if a script is killed
before its trap runs (#590).

## Suite

The full charter suite, on the **committed** tree — §1f is why that distinction is stated —
in two environments:

| | |
|---|---|
| Python 3.14.4 (Homebrew), inherited environment | **6975 tests, OK**, 362.6 s |
| Python 3.12.13 (Homebrew), with every `CHARTER_*`, `CLAUDE_*`, `ANTHROPIC_*` and `TMUX*` variable unset (16 of them present, 0 after; the child process saw none) | **6975 tests, OK**, 380.0 s |

`tests/test_packaging.py::test_runtime_has_zero_dependencies` passes in both, and both runs
were of the **unmodified** `charter/` and `tests/` — `git status` for those paths is empty on
this branch. The provider's own 14 tests pass on 3.14 (editable) and on 3.12 (from the built
wheel).

CI is green at this branch's head, asked of the sha rather than of the pull request (#561):
`gh api repos/diazoxide/charter/commits/<HEAD>/check-runs` reports `test (3.11)`,
`test (3.12)`, `test (3.13)` and `test (3.14)` all **success**.

**CI validates none of the Textual half, and it should not be read as if it did.**
`.github/workflows/test.yml` runs `python -m unittest discover -s tests -v` on 3.11-3.14 and
installs nothing: not this provider, not Textual, not pytest. Nothing in
`providers/` is built, imported or executed by any job, and no job starts a tmux server for
these measurements. What CI does prove here is the thing that matters most —
that charter is unchanged and its suite still passes — and what it does not prove is every
number in §§1-7, each of which was taken by hand on this machine and is reproducible with
the scripts named beside it.

No charter production line was changed, so there is no news entry.
