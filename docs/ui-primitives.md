# The window's UI primitives

**Radix UI. New UI is built from its primitives, not from hand-rolled markup.**

This is a rule and not a preference, so the rest of this file is the reasons for it and the way
to follow it.

## The rule

- A dialog, a radio group, a checkbox, a menu, a popover, a tooltip, a collapsible section, a
  tab list: take the Radix primitive. Install the one package you need
  (`@radix-ui/react-<thing>`) — the primitives tree-shake per package, so the window pays for
  what it uses and nothing else.
- **Do not write a wrapper layer around them.** No `<Modal>`, no `<Field>`, no house component
  library. The primitive is the component; charter's look is CSS on it. A wrapper is the custom
  tooling this repo's first rule exists to prevent, and it is how a primitives migration turns
  back into hand-rolled markup with extra steps.
- Charter's own look, always. Radix ships **no CSS at all** — every primitive is an unstyled
  element with `data-state` attributes to hang rules off. `App.css` stays the one place the
  window is drawn.
- Native HTML that already does the job is not hand-rolled markup and does not need replacing:
  `<details>/<summary>`, `<label for>`, `<button>`. Reach for a primitive when the browser has
  no element for what you mean — a modal that traps focus, a listbox, a menu.
- `react-resizable-panels` stays what draws resizable panes. Radix has no panel primitive and
  there is nothing to move.

## Why Radix, and why not the other two

**Not Material (MUI).** Material is the wrong visual language for a dense terminal-adjacent
window — the reference for this app's design is Zed. Its runtime CSS-in-JS is also the one
thing that would put the render path back on the table: the workspace strip's counts measure
0.022 ms at fifty chats (#133), and Emotion serialises styles per render.

**Not Base UI** (`@base-ui/react`, MUI's unstyled layer), although it is a real candidate:
stable at 1.8.0, actively released, ships no CSS either, and has every primitive the next two
tickets need. It costs more for the same components. Measured on this repo, as the added weight
of the production bundle over `main`:

| primitives imported                                   | Radix     | Base UI   |
| ----------------------------------------------------- | --------- | --------- |
| dialog, radio group, checkbox, collapsible            | +60.6 kB  | +84.2 kB  |
| ...plus menu and popover (the strips and layout work) | +101.6 kB | +165.9 kB |

Gzipped, the six-primitive set is +32.6 kB against +54.7 kB. Both tree-shake — the four-way set
costs less than the six-way one in each — and both add zero CSS. Radix is also 2.3 MB installed
against 20 MB, which is the first priority in `AGENTS.md` rather than a rounding error.

What actually shipped is narrower than either row: dialog, radio group and checkbox, with
`<details>` left as the collapsible because the browser already has one. The window's bundle
goes from 641.27 kB to 700.62 kB — **+59.35 kB, +18.63 kB gzipped** — and `App.css` from
13.28 kB to 15.33 kB, which is the rules that were missing rather than anything the library
brought.

Cold start is already a missed limit: ADR 0026 gives 2 s, and Linux takes 25 s behind a dead
portal (#24, accepted). Neither library is what makes that true, but the smaller one is the one
to add to it.

## What is already converted

`StartChat`, `QuitWarning`, `ApprovePlane` and `Palette` — every modal surface the window has.
The terminal panes are xterm.js and are not a candidate. The strips are about to change and
were deliberately left alone.

## Two things learned converting them, which will catch the next person

**A radio group's pick does not follow the arrow keys on its own.** Radix selects an item on
focus only while it believes an arrow key is down, and it learns that from a `keydown` listener
on `document`. React attaches its delegated listeners to the root container and to each portal
container, both below `document`, so React's handler moves the focus before Radix's listener
runs. Every `RadioGroup.Item` in `StartChat` therefore carries its own `onFocus` that picks it.
The reasoning and the measurement are in that file; the guard is the test named "moves between
harnesses with the arrow keys".

**A modal dialog really is modal, and the tests notice.** Radix marks everything outside the
open dialog `aria-hidden`, so a `getByRole` for anything behind it finds nothing, and a scenario
spec cannot click a tab while a picker is up. That is the app behaving correctly. When a test
breaks on it, the test was reaching for something an operator could not have reached — fix the
test to take a route that exists.
