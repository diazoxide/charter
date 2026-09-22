# The window's design system

**A theme is a data file. Every colour in charter comes from one, and nothing else may write
one down.** `docs/ui-primitives.md` says what the window is built *out of*; this file says what
it is *drawn in*.

The rule, in one line each:

- **No colour literal outside `app/src/theme/`.** Not in CSS, not in TypeScript, not in a
  comment that becomes code. `app/src/theme/literals.test.ts` fails the build on one.
- **No arbitrary Tailwind value** — `text-[13px]`, `bg-[#fff]`, `w-[42rem]`. Same test, same
  reason: a theme cannot reach inside a bracket.
- **Semantic names only.** A token is `surface.raised`, never `gray-800`.
- **Both built-in themes get every new token**, or the window will not start.

## Why a theme is data and not CSS

Before this layer, charter had two colour systems. `App.css` had five custom properties and
thirty hex literals scattered through 1,330 lines; `SessionPane.tsx` built its xterm terminal
with `theme: { background: "#181818", foreground: "#d8d8d8" }` — which is `--paper` and `--ink`
written out a second time, in a second language, with nothing making them agree. The terminal's
other eighteen colours were the library's defaults and were not charter's at all.

**The two cannot be unified in CSS**, and that is the constraint that decides the design. xterm
is handed a JavaScript object of sixteen ANSI colours plus a background, a foreground, a cursor
and a selection; a CSS custom property cannot be given to it. Unifying in JavaScript instead
would make the stylesheet a copy of the script.

So the shared thing is neither: it is a **file**. `app/src/theme/*.json` holds semantic tokens;
`theme.ts` writes CSS custom properties from a theme **and** builds xterm's object from the same
theme. One source, two consumers, and no way for them to drift. This is VS Code's model and
Zed's, and it is the only one that can colour the terminal at all.

## The vocabulary

`TOKENS` in `app/src/theme/theme.ts` is the list, with a comment on each group saying what it
means. Fifty-four names in eleven groups:

| group | tokens | what it is |
| --- | --- | --- |
| `surface.*` | `base` `sunken` `deep` `raised` `overlay` `hover` | the layers of the window, deepest first |
| `control.*` | `base` `hover` `aimed` `count` | things that are pressed; `aimed` is where the keyboard is, which is not where the pointer is |
| `text.*` | `primary` `secondary` `muted` | |
| `border.*` | `subtle` `strong` | |
| `accent.*`, `focus.ring`, `tab.active` | `base` `surface` | what charter is drawing attention to |
| `needs-you.*` | `base` `text` | the one signal this app exists for |
| `danger.*` | `base` `surface` `text` `wash` | an answer that cannot be taken back |
| `state.*` | `running` `waiting` `waiting-glow` `failed` `success` `unreadable` | what a chat, or a check on a branch, is doing |
| `overlay.*` | `scrim` `shadow` | what goes over the window when something is modal |
| `terminal.*` | `background` `foreground` `cursor` `cursor-accent` `selection` | |
| `terminal.ansi.*` | the eight, and the eight bright | |

**Two tokens may look like one token and are not.** `needs-you.base` and `danger.base` were a
single value before this — `--stop`, `#c05c5c`, "charter's red" — and splitting them by meaning
paid for itself on the first measurement: white on `#c05c5c` is **4.26:1**, under WCAG AA, and
the badge that failed is the count of chats waiting for the operator. `needs-you.base` is now
`#b85050` (4.88:1) and the mark on an answer that cannot be undone is untouched. A palette token
cannot make that move; that is the whole argument for semantic names in one change.

**Every theme is held to a contrast floor.** `contrast.test.ts` checks each pair that ends up as
something drawn on something: 4.5:1 for text, 3:1 for the state marks and the sixteen ANSI
colours against the terminal's own background. "Complete" is not "legible", and a light theme
made by inverting a dark one passes every other test in the directory while being unreadable.
The one exemption is `terminal.ansi.black`, held to 1.5:1 — ANSI black on a dark terminal is dim
in every theme there has ever been, because it is the colour a program picks when it means
*recede*; it still has to be visible, and charter-dark measures 2.14:1.

**The chat states and the CI states share a group on purpose.** `.ci-pending` is
`var(--state-waiting)` because amber means "not finished" in both, and a theme author who wants
to change that changes one value rather than hunting for the second one.

## What a theme file looks like

```json
{
  "name": "charter-dark",
  "appearance": "dark",
  "tokens": {
    "surface.base": "#181818",
    "terminal.ansi.red": "#ff7b72"
  }
}
```

- **`appearance` is `dark` or `light`.** It decides two things: which built-in fills in the
  tokens this file does not name, and what `color-scheme` the document gets, which is what
  makes the platform's own scrollbars and form controls match.
- **A value is hex and only hex** — `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`. This is a security
  boundary, not a style preference: a value is written straight into a CSS declaration, so
  `red; } body { display: none` in a theme file would be a stylesheet somebody else wrote. The
  grammar removes the question rather than answering it, and every value it allows is one xterm
  also accepts. Converting an `rgb(0 0 0 / 55%)` to `#0000008c` costs an author one lookup.
- **A partial theme is normal.** Name the tokens you are changing; the rest come from the
  built-in of the same appearance.
- **Nothing in a theme file can stop the window.** A token that is missing, misspelled or
  malformed falls back and the substitution is reported. `load` always answers with a complete
  theme, because ADR 0026 holds cold start at 2 s and a window that will not come up because a
  colour was spelled wrong is worse than every possible wrong colour.

### Where a user's theme lives

`$CHARTER_CONFIG_HOME`, else `$XDG_CONFIG_HOME`, else `~/.config` — then `charter/theme.json`.
That is `machine.rs`'s ladder, rung for rung, and the argument is the one charter ADR 0040 made
for pins: **a theme is how one operator likes their window, not a fact about the plane.** A
theme committed to `charter.toml` would arrive with every clone and repaint somebody else's
window in colours they never chose.

**It is a file beside `machine.json`, not a fifth thing inside it.** `machine.rs` says four
things and nothing else, and says the count is load-bearing; `charter report`'s publish consent
is already a second file in that same directory, so a third is the shape the directory already
has. Nothing about this needs ADR 0040 amended again.

**Reading it is not in this change.** It needs a Tauri command, a Tauri command needs
`app/src/bindings.ts` regenerated, and that is `cargo`, which does not run on the machine this
was built on. The seam is `load(raw: unknown)`, which takes whatever `JSON.parse` gave and is
fully tested against garbage; the command that supplies `raw` is the only missing piece.

## Tailwind, shadcn and Lucide

**Tailwind v4**, wired in `app/src/styles.css`. Three decisions there, each with a test in
`app/src/theme/tailwind.test.ts` that fails if it is undone:

- **Tailwind owns no colour.** Every entry in `@theme` is `var(--<token>)`. v4's `@theme` is
  CSS-variable-native, which is why v4 and not v3: it sits on top of the token layer instead of
  being one.
- **Tailwind's palette is deleted.** `--color-*: initial` clears the namespace, so `bg-red-500`
  and `text-slate-300` are not classes — they are typos. That is the "semantic, not palette"
  rule enforced by the build rather than by whoever reviews the diff. `transparent` and
  `current` are put back, because they are the absence of a colour and the inherited one.
  Checked, because deleting a namespace is the sort of thing that takes a neighbour with it:
  `ring-2` still compiles and now falls back to `currentcolor`, which is the token-driven text
  colour and is better than the blue it used to default to. **`shadow-md` and its siblings are
  the one leak**: they carry their own `rgb(0 0 0 / 0.1)` rather than reading `overlay.shadow`,
  so a shadow utility is a colour a theme cannot reach. Use the token in CSS until somebody
  maps `--shadow-*` in `@theme` as well.
- **Preflight is not imported, and `App.css` is imported into a layer.** A reset would restyle
  1,330 lines in one commit, and `scenario tests` read the real DOM. The layer order —
  `theme, base, charter, components, utilities` — is what the reset would have been for:
  unlayered CSS beats layered CSS, so `App.css` had to go *into* a layer or no utility could
  ever override it. Turning preflight on is its own change, with its own evidence.

**The Tailwind colour name is the token name**, stutter and all: `text-text-primary`,
`border-border-subtle`. A prettier alias would be a second vocabulary.

> **One asymmetry the layer order does not cover, measured rather than assumed.** xterm's own
> stylesheet is imported from `SessionPane.tsx`, not from `styles.css`, so it lands **unlayered**
> — and unlayered CSS beats every layer, including `utilities`. Nothing collides today: it sets
> `cursor`, `position` and `user-select` on `.xterm`, and charter's rule sets `height` and
> `padding`, so no declaration is contested. But the next stylesheet imported from a component
> will outrank the whole stack silently. The fix, when something does collide, is one line —
> move the import into `styles.css` as `@import "@xterm/xterm/css/xterm.css" layer(vendor);` and
> put `vendor` before `charter` in the layer list — and it is deliberately not taken here,
> because it changes which rules win in the terminal and that cannot be checked without running
> the app.

**shadcn/ui: the conventions, and components when something needs one.** `cn` is at
`app/src/lib/utils.ts`, at shadcn's address with shadcn's two dependencies, so a component
pasted from shadcn finds what it expects. Nothing else is here yet, deliberately: copying in
components nothing renders is dead code, and `class-variance-authority` arrives with the first
component that has variants.

> **Settled by the operator on 2026-09-22 — copy shadcn components in.** This file used to flag
> a conflict here: `docs/ui-primitives.md` said *"Do not write a wrapper layer around them. No
> `<Modal>`, no `<Field>`, no house component library,"* and a shadcn component is literally a
> thin wrapper around a Radix primitive. The ruling is that those are two different things
> wearing one word. A component library is **a dependency that owns your markup** — an upstream
> you cannot edit, an API you are stuck with, a look you fight. **A copied-in component is our
> own code in our own repo, editable line by line.** The rule is now *no library between you and
> the primitive, and no indirection you cannot read*, and copied-in source is neither. charter
> **ADR 0037**'s amendment of 2026-09-22 holds the decision and the reasoning, and is
> authoritative over both this file and `ui-primitives.md`.

**What a copied component has to satisfy.** This file rather than `ui-primitives.md` has the say
on the first three, because they are about what the window is drawn *in*:

- **Every Tailwind colour class has to be renamed to this vocabulary, by hand, and nothing will
  tell you if you forget.** A shadcn component ships `bg-background`, `text-foreground`,
  `bg-destructive` — names from shadcn's own token set, which this app does not have. They are
  not classes here: the palette is deleted above and these were never in it, so each one emits
  **no CSS at all**. Nothing goes red. `literals.test.ts` catches a hex literal and an arbitrary
  value; it does not catch a class that does not exist, and `tailwind.test.ts` checks that the
  palette is gone rather than that a source file avoided it. The result of missing one is an
  element rendered undressed — which is exactly the `claudeclaudeclaudebuilt-indefault` defect
  charter ADR 0037 was written about. **Read the copied file's classes against the `@theme` block
  in `app/src/styles.css` before the PR, and look at the component running.**
  `bg-surface-base text-text-primary` is the shape they should end up in.
- **No arbitrary value survives the paste** — `text-[13px]`, `rounded-[6px]`, `bg-[#fff]`. This
  one *is* mechanical: `literals.test.ts` reads the real source tree and fails on it.
- **No colour literal**, in the same test, for the same reason as everything else in this file.
- **It goes at shadcn's address**, `app/src/components/ui/`, and it is **edited freely**. That is
  the condition rather than a permission: a copy kept pristine "because upstream will fix it" is
  a dependency with worse ergonomics and no version, and there is no upstream once it is copied.
- **Say where it came from, and at what version, in the file.** No manifest records a copied
  file, so the file is the only place its provenance can live.

Still refused, unchanged by the ruling: a component library as a **dependency**, and a house
abstraction layer over Radix — `<ConfirmModal open onConfirm>` — whether it is written here or
copied from somewhere. Copying it would not launder it; what is refused is a charter API in
front of the primitive.

**Lucide** is the icon set (`lucide-react`). The property that matters is that it draws with
`stroke="currentColor"` and `fill="none"`, so an icon takes the colour of the text it sits in
and a theme reaches it without an icon ever naming a colour. `app/src/lib/icons.test.tsx` pins
that. No icon is on a button yet; that is a separate ticket, on top of this one.

## What it costs

Measured on this branch, against `origin/main`:

| | main | this | delta |
| --- | --- | --- | --- |
| JS bundle | 749.64 kB (214.12 kB gz) | 755.44 kB (215.85 kB gz) | +5.80 kB (+1.73 kB gz) |
| CSS | 18.08 kB (4.24 kB gz) | 23.52 kB (5.23 kB gz) | +5.44 kB (+0.99 kB gz) |

Almost all of the CSS growth is `var(--control-base)` being fifteen characters longer than
`#222`, repeated fifty-odd times; gzip takes most of it back. Tailwind's own contribution to the
built stylesheet is **995 bytes**, because it emits only what a utility uses and no utility is
used yet.

**Cold start is untouched, by construction.** ADR 0026 gives it 2 s. Nothing is read from disk
on the way to the first frame: the built-in themes are compiled into the bundle, and `main.tsx`
calls `drawIn(DEFAULT_THEME)` before `createRoot`, so no frame is ever painted in one theme and
repainted in another. That call is fifty-four `setProperty` calls on one element and measures
**0.60 ms median, 0.85 ms p95** under jsdom — 0.03% of the budget, and jsdom's CSSOM is slower
than a real engine's, so it is an upper bound. Parsing a theme file measures 0.0035 ms.

When the user theme lands it must stay off that path: apply the built-in synchronously, read the
file after, repaint if it differs.
