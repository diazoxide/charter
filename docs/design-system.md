# The window's design system

**A theme is a data file. Every colour in purlis comes from one, and so does every motion —
and nothing else may write either down.** `docs/ui-primitives.md` says what the window is built _out of_; this file says what
it is _drawn in_; `docs/ui-copy.md` says how it _talks_.

The rule, in one line each:

- **No colour literal outside `app/src/theme/`.** Not in CSS, not in TypeScript, not in a
  comment that becomes code. `app/src/theme/literals.test.ts` fails the build on one.
- **No arbitrary Tailwind value** — `text-[13px]`, `bg-[#fff]`, `w-[42rem]`. Same test, same
  reason: a theme cannot reach inside a bracket.
- **Every view is drawn from tokens, in both themes.** `app/src/theme/views.test.tsx` renders
  every view a tab shows, in every state it draws, in each built-in theme. It fails on what
  reaches the DOM: a colour in an inline style, an SVG paint attribute or a data URL; a `var(--x)`
  that is not a token; a token the theme in force does not set; or an arbitrary-value class. It
  catches a colour built at run time, which no source line shows. A view added to `OWN_MARKS` in
  `Views.tsx` fails that test until it has a state there.
- **Semantic names only.** A token is `surface.raised`, never `gray-800`.
- **Both built-in themes get every new token**, or the window will not start.
- **No time and no easing outside `app/src/theme/`** — no `150ms`, no `ease-out`, no
  `duration-150`, and no `prefers-reduced-motion` block. Same test; see [Motion](#motion-is-data-too).

## Why a theme is data and not CSS

Before this layer, purlis had two colour systems. `App.css` had five custom properties and
thirty hex literals scattered through 1,330 lines; `SessionPane.tsx` built its xterm terminal
with `theme: { background: "#181818", foreground: "#d8d8d8" }` — which is `--paper` and `--ink`
written out a second time, in a second language, with nothing making them agree. The terminal's
other eighteen colours were the library's defaults and were not purlis's at all.

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
means. Seventy-one names in thirteen groups:

| group                                  | tokens                                                                                          | what it is                                                                                        |
| -------------------------------------- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `surface.*`                            | `base` `sunken` `deep` `raised` `overlay` `hover`                                               | the layers of the window, deepest first                                                           |
| `control.*`                            | `base` `hover` `aimed` `count`                                                                  | things that are pressed; `aimed` is where the keyboard is, which is not where the pointer is      |
| `text.*`                               | `primary` `secondary` `muted`                                                                   |                                                                                                   |
| `border.*`                             | `subtle` `strong`                                                                               |                                                                                                   |
| `accent.*`, `focus.ring`, `tab.active` | `base` `surface`                                                                                | what purlis is drawing attention to                                                              |
| `layer.*`                              | `project` `workspace` `chat` `selected`                                                         | which of the three strips of the axis a row is, and the tab you are on                            |
| `needs-you.*`                          | `base` `text`                                                                                   | the one signal this app exists for                                                                |
| `danger.*`                             | `base` `surface` `text` `wash`                                                                  | an answer that cannot be taken back                                                               |
| `state.*`                              | `running` `waiting` `waiting-glow` `failed` `success` `unreadable`                              | what a chat, or a check on a branch, is doing                                                     |
| `overlay.*`                            | `scrim` `shadow`                                                                                | what goes over the window when something is modal                                                 |
| `terminal.*`                           | `background` `foreground` `cursor` `cursor-accent` `selection` `find-match` `find-match-active` | the two `find-*` are a find's matches in a pane, drawn under the text (`theme.searchDecorations`) |
| `terminal.ansi.*`                      | the eight, and the eight bright                                                                 |                                                                                                   |
| `icon.*`                               | `folder` `motive` `grey` `red` `orange` `yellow` `green` `teal` `blue` `purple` `pink`          | a file's or a folder's icon in a tree; the only colours an icon theme names (FM-3)                 |

**Two tokens may look like one token and are not.** `needs-you.base` and `danger.base` were a
single value before this — `--stop`, `#c05c5c`, "purlis's red" — and splitting them by meaning
paid for itself on the first measurement: white on `#c05c5c` is **4.26:1**, under WCAG AA, and
the badge that failed is the count of chats waiting for the operator. `needs-you.base` is now
`#b85050` (4.88:1) and the mark on an answer that cannot be undone is untouched. A palette token
cannot make that move; that is the whole argument for semantic names in one change.

It paid again the other way (#1210). `danger.base` is the _words_ of an answer that ends
something (`.ends-it` in an `AnswerBar`, a form's `SettingActions` and a menu), and `#c05c5c` on
the button's `control.base` was **3.73:1**, under the 4.5:1 that text at the window's 14px asks
for. charter-dark's `danger.base` is now `#c97474`, the same hue made lighter: **4.72:1** on
`control.base`, 4.94:1 on `danger.surface` (the same button under the pointer, and a highlighted
menu row), 5.06:1 on a menu's `surface.overlay` and 5.27:1 on the window. `danger.wash` follows
it, since a wash is its base at a low alpha. The badge kept `#b85050`; charter-light's
`#a83232` already cleared 4.5 on each of them and did not move.

**Every theme is held to a contrast floor.** `contrast.test.ts` checks each pair that ends up as
something drawn on something: 4.5:1 for text (AA's floor for text under 24px, or 18.66px bold,
which a label, a button and a menu row all are), 3:1 for the state marks and the sixteen ANSI
colours against the terminal's own background. "Complete" is not "legible", and a light theme
made by inverting a dark one passes every other test in the directory while being unreadable.
The one exemption is `terminal.ansi.black`, held to 1.5:1 — ANSI black on a dark terminal is dim
in every theme there has ever been, because it is the colour a program picks when it means
_recede_; it still has to be visible, and charter-dark measures 2.14:1.

**The `layer.*` tokens are a shade that means _depth_, and nothing louder.** purlis#171
said which of the three strips you were looking at with an indent; purlis#193 took the
indent away on the operator's reading of it. Coloured rules under each strip replaced it for one
review and were turned down on sight — _"this is not looks professional, it should be
minimalistic, and i prefer to change little bit backgrounds of tabs and little lighter for
selected tab, borders are not feeling well."_ So each strip is one small step of neutral grey,
outermost deepest, and `layer.selected` is a step lighter than the lightest of them; that
background is the whole of the selection signal. **Both themes step the same way** — deeper
outside, lighter in, lightest where you are — and `contrast.test.ts` holds a strip's muted tab
text to 4.5:1 on each shade and the selected tab's primary text on `layer.selected`. Their own
group rather than `surface.*`, so a theme author can move the axis without moving every other
surface in the window.

`layer.divider` is the one line the axis allows: a very light hairline between two tabs of a
strip, never under or around one — the operator's _"very very light visible border - just for
little bit highlight separation"_. Its own token because `border.subtle` sat too close to the
layer shades to be seen, and a theme should be able to lift the tabs apart without lifting every
other subtle rule in the window.

**A workspace's colour is a hue shift of the theme in force, not a colour of its own**
(purlis#281, ADR 0048). A workspace names one of eight hues (`tint.PALETTE`, the same eight
the core reads) or a `#rrggbb` whose hue is taken, and the window turns only these tokens to it:

- on the whole window, from the workspace in front: `accent.base`, `accent.surface`,
  `focus.ring`, `tab.active` (`theme.TINTED_WINDOW`, drawn by `drawTint`);
- on that workspace's own tab and on the chat strip, which holds its chats, the same and
  `layer.workspace`, `layer.chat`, `layer.selected` (`theme.TINTED_TABS`), set on the element as
  custom properties by `tintVariables`. Each workspace tab carries its own tint whether or not it
  is in front: a `.workspace-mark` dot in its accent and its own shade. (The title bar's
  breadcrumb carried the same dot until ADR 0054 took the breadcrumb away.)

Never the text, never `layer.project` (the project strip is not a workspace's), never the
terminal. `app/src/theme/tint.ts` does the arithmetic in OKLCH and then keeps each token's
**relative luminance** exactly, so a tinted shade clears every contrast floor the theme's own
cleared; `contrast.test.ts` runs every palette hue on both built-in themes, and holds two pairs
the tint adds — the mark (`accent.base`) and a tab's primary text on `layer.workspace`. A neutral
grey is given a small fixed chroma, felt rather than noticed, as the strips are meant to be;
white stays white. No colour is written for this anywhere but `app/src/theme/`: a component asks
`tintVariables` and sets what it answers.

**A persona's mark is tinted the same way, on two tokens only** (#1449). A persona is drawn
wherever it appears as one `PersonaMark` (`app/src/PersonaMark.tsx`): its custom `icon.png`,
else its built-in icon, else its initials, on a ground of its colour. The colour is the
workspace's vocabulary, one of the same eight hues or a `#rrggbb`, and a persona that names none
is given one of the eight by its name (`colourOfName`), so it is the same on every machine.
`tintVariables` turns `accent.base` and `accent.surface` to it (`MARK_TOKENS`) on the mark's own
element and nowhere else: the mark is a small ground with a glyph on it, and the contrast pair
it needs is the one the workspace tint already holds. No colour is written in the component.

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

`$PURLIS_CONFIG_HOME`, else `$XDG_CONFIG_HOME`, else `~/.config` — then `charter/theme.json`.
That is `machine.rs`'s ladder, rung for rung, and the argument is the one ADR 0040 made
for pins: **a theme is how one operator likes their window, not a fact about the plane.** A
theme committed to `charter.toml` would arrive with every clone and repaint somebody else's
window in colours they never chose.

**It is a file beside `machine.json`, not a fifth thing inside it.** `machine.rs` says four
things and nothing else, and says the count is load-bearing; `purlis report`'s publish consent
is already a second file in that same directory, so a third is the shape the directory already
has. Nothing about this needs ADR 0040 amended again.

**It is read before the window exists and handed to it as it is created** (M6.7), the same way
as the layout below, so an operator with a theme of their own never sees the built-in painted
first. `theme.ts`'s `load` judges it token by token, and whatever it had to put right — a
misspelled token, a value that is not hex, a file that is not JSON — is said in the alerts
drawer under **This machine**, with the file's path. **It wins over an extension's theme**: it
is the one theme the operator wrote for this machine themselves, so an approved extension's
contribution does not repaint over it. Delete the file to have the extension's. The drawer's
row about it offers **Use built-in…** (NO-6 #1238), which asks first and then moves the file
aside to `theme.aside.json` (or the next free `theme.aside-N.json`), never over another file,
and draws what is in force without it.

**A theme switched while the window is up reaches the terminal too.** The stylesheet follows by
itself — `apply` rewrites the custom properties it reads — but xterm was handed an object when a
pane was built. `theme.ts`'s `onDrawn` is the change signal, and each pane hands its terminal the
new object through `pane.options.theme`, which is xterm's own way to retheme a live terminal.

### The window's layout is a file, handed to the window as it is created

A theme and a **layout** — which regions are drawn, on which side, in what order and how big
(`app/src/regions.ts`) — are both "how one operator likes their window", and both are files
beside `machine.json`: `$PURLIS_CONFIG_HOME` (else `$XDG_CONFIG_HOME`, else `~/.config`), then
`charter/layout.json`. It is not a field of the machine store: ADR 0040 amended ADR
0034 for _"how the operator arranged what this file already names"_, and a region arrangement
names nothing that file holds.

**It is a file because a file is the one form an operator can hand-edit**, and a region's `side`
and `order` have no control in the window yet. **It is injected, not fetched, because of the
first frame.** A layout has no stand-in the way the built-in theme does: the operator's
arrangement _is_ the thing, and one read over an asynchronous Tauri command would land after
the window had painted the default and make it lay itself out again — the flash
purlis#141 left. So the Rust side reads the file before it builds the window
(`purlis_core::windowprefs`, `app/src-tauri/src/windowprefs.rs`) and puts what it read into
the page with the window's `initialization_script`; the page's first render is drawn from it
and nothing is fetched. What the window changes — a region put away, a slot dragged — is
written back to the file through a command, pretty-printed, `0600`.

Web storage held the arrangement before this, under `charter.layout`. The first launch that
finds no file draws from that value, moves it into the file, and removes the key once the file
has it; nothing reads the key after that.

#### The format, for editing it by hand

```json
{
  "version": 2,
  "regions": [
    {
      "id": "navigation",
      "side": "right",
      "order": 0,
      "collapsed": false,
      "size": 22,
      "view": "explorer"
    },
    { "id": "aside", "side": "left", "order": 0, "collapsed": false },
    { "id": "bottom", "side": "bottom", "order": 0, "collapsed": true }
  ],
  "projects": {
    "/home/me/project": {
      "regions": [{ "id": "navigation", "side": "left", "order": 0, "collapsed": true }]
    }
  },
  "text": { "window": 15, "terminal": 14 },
  "editor": "zed",
  "chats": { "lines": 1, "grouped": true, "tabbed": true, "away": false },
  "dismissed": { "/home/me/project": ["pin-dormant:ide", "chat-fresh:3"] }
}
```

- **`version`** is `2`, what purlis writes since #1673. A version 1 file, which every purlis
  before it wrote, is read too and moved forward at the first change: its arrangement becomes
  the machine's, every project starts from it, and the explorer's region is the navigation
  region (below). A file with no version, or another one, is not read: the window is drawn in
  the default arrangement and the alerts drawer says why.
- **`regions`** lists placements: **the machine's arrangement**, what a project with none of its
  own starts from. Changing the arrangement in any project writes it here too, so a project
  opened afterwards starts as the person last left one. The ids this build has are `navigation`
  (the left side: the Chats and Explorer views; version 1 called it `explorer`, and that id is
  still read as this one), `aside` (the attention region: personas, memory, contributed panels)
  and `bottom` (the repository state bar). A region the list leaves out is where it starts; an
  id this build does not have is left out and named in the alerts drawer.
- **`projects`** is each project's own arrangement, by its path, as `regions` is written
  (#1673): the side a view is on, which view it shows, its width and whether it is away come
  back as that project left them. purlis writes a project's entry when its arrangement changes;
  the core keeps the entries a window did not send, so two windows keep each other's, and keeps
  at most 32, letting go of the projects opened longest ago first. An entry that is not an
  arrangement is skipped, and the project starts from the machine's.
- **`view`**, on a region that has views, is the one it shows: `chats` or `explorer` for
  `navigation`. Leave it out for the first, `chats`. A view the region does not have is its
  first, and the alerts drawer says so.
- **`side`** is `left`, `right` or `bottom` — the three slots around the terminals. Two regions
  on one side stack in **`order`**, lowest first; a tie is broken the same way at every launch.
- **`collapsed`** puts a region away when it is `true`, and only then. Anything else draws it.
  A region with views keeps them, hidden, and its activity bar stays at the window's edge.
- **`size`** is how big the region's slot is, as a percentage of the window's width — of its
  height, for the bottom slot — above 0 and at most 100; leave it out for the default. Keep it
  inside the slot's own bounds, which a drag is held to as well: the left slot is 8–45%, the
  right 10–45%, the bottom 6–50% (`SLOTS` in `regions.ts`). The left slot is also never
  narrower than 11rem, whatever percentage that is of the window (#1499): under it a nested row
  of the Chats list has no room for its state, and the floor follows the text size.
- **`text`** is the two text sizes, in px (purlis#283): **`window`**, the root font size
  every `rem` in the stylesheet is measured by, and **`terminal`**, every chat's terminal. Each
  is a whole number from 10 to 24; leave one out for its default, 14 and 13. A size that is
  not one is its default, and the alerts drawer says so. Settings and the size keys
  (`⌘`/`Ctrl` with `=`, `-`, `0`, `app/src/textSize.ts`) write it; it is in this file and not in
  a plane because a size is this machine's, and a plane would carry it to every clone.
- **`editor`** is your editor (RC-20, ADR 0081 §3), where *Open in your editor* sends a file
  at a line: `vscode`, `zed`, `idea` (a JetBrains IDE) or `variable` (`$VISUAL`, else
  `$EDITOR`, from purlis's own environment, run with `+line` and the file). Leave it out and
  none is chosen: *Open in your editor* asks for one. Any other value is none, and the alerts
  drawer says so. Settings writes it (`app/src/yourEditor.ts`). It is a word and never
  a program: the core builds the URL, or reads the variable itself, so nothing written here is
  run.
- **`chats`** is how chats are listed and summed up, on this machine (#1499, V100-73): **`lines`**, `2` or `1`, how
  many lines a chat's row takes (on one, what the second line said is the row's tooltip), and
  **`grouped`**, `true` when the sessions of one workspace stand together. And what pressing a
  task does (#1489, V100-74): **`tabbed`**, `true` when it opens in a tab of its own, with a
  minimise button in place of the close; `false`, its session's tab is switched to it. And
  **`away`** (#1514, V100-73), `false` when coming back to the window draws no summary of what
  happened while you were away. Leave any out for its default, `2`, `false`, `false` and
  `true`; purlis leaves the whole of `chats` out
  while all are the defaults. A value that is neither is the default, and the alerts drawer says so. Settings
  writes it (`app/src/chatsListPrefs.ts`). It is in this file because it is how one person
  likes their window, and a project would carry it to every clone.
- **`dismissed`** is the Notices you dismissed (NO-2, V91j), per project by its path, each
  by its cause. A Notice stays hidden while its cause is kept here, and the window lets the
  cause go once the project answers without it, so the Notice shows again if the cause comes
  back. Only causes the core answers for are kept: `pin-dormant`, `chat-resumed`,
  `chat-guessed` and `chat-fresh`; any other is left out, as is anything past 200 per project
  (`app/src/dismissals.ts`). A resumed chat's cause names the conversation it was resumed by
  (`chat-resumed:3:<conversation>`), so another conversation's note shows. The core writes
  this field one project at a time, under the file's lock, and a layout write from a window
  never changes it, so two windows keep each other's. It takes at most half the file: past
  that, the projects opened longest ago lose theirs first. It is here and not in a project
  because what you have already seen is yours on this machine. Under the key
  **`"on this machine"`**, in place of a project's path, are the Notices shown once per
  machine whatever the project: `chip-explained`, the Notice the first dispatch shows to explain
  a tab's chip of tasks (#1501). The core keeps only those causes there, whoever writes the
  key and whatever was hand-edited into it, adds one under the file's lock so two windows keep
  it once, and the window asks the file again before it shows such a Notice. Nothing lets them
  go, and the projects' dismissals never push them out. Delete it to see every Notice again.
- **The file is read once, as the window is created.** Edit it while purlis is not running,
  or expect the next change made in the window to replace your edit.
- **Nothing in it can stop the window.** A file that is not JSON, is not a layout, is a link or
  is over 64 KiB is refused whole and said in the alerts drawer; a field that is wrong costs only
  that field. The next change made in the window rewrites a file that did not parse — the drawer
  says so — but never one purlis could not read at all (a link, a FIFO).

## Motion is data too

**How long a change in the window takes, and how it moves, is a theme token — named for what the
motion is for, read by name, and collapsed in one place for an operator who asked for less.**
This is M7.2, and it is the colour layer's argument applied to time.

Before it, the window had two motions — `900ms linear` on the spinner and `120ms ease` on the
explorer's twisty — each spelled out where it was used and each with its own
`@media (prefers-reduced-motion)` block beside it. That is where colour was before this file
existed: every value a local decision, every guard something the next component had to remember.
The operator has asked for animation twice (_"no icons, visual components, animations"_, _"show
pipelines with animation"_), and adding it on top of that shape would have multiplied both.

### The vocabulary

`DURATIONS` and `EASINGS` in `app/src/theme/motion.ts`. Nine names, written to the document as
`--motion-duration-*` and `--motion-easing-*` beside the colours:

| token              | built-in           | what it is for                                                          |
| ------------------ | ------------------ | ----------------------------------------------------------------------- |
| `duration.quick`   | 120 ms             | the window answering a press: a tab lit, a chevron turning              |
| `duration.enter`   | 160 ms             | a surface arriving: a menu, a popover, a dialog, a region brought back  |
| `duration.settle`  | 280 ms             | a mark arriving at an answer: a pipeline finished, a chat changed state |
| `duration.spin`    | 900 ms             | one turn of _still running_                                             |
| `duration.breathe` | 1600 ms            | one breath of _queued, not yet running_                                 |
| `easing.standard`  | `[0.2, 0, 0, 1]`   | a state changing in place                                               |
| `easing.enter`     | `[0, 0, 0.2, 1]`   | decelerating into place, which is what reads as _arrived_               |
| `easing.steady`    | `[0, 0, 1, 1]`     | a turn; an eased one stutters once a second                             |
| `easing.breathe`   | `[0.4, 0, 0.6, 1]` | a loop with no visible seam                                             |

**Semantic, not a scale**, for the reason `needs-you.base` and `danger.base` are two tokens: a
theme that wants arrivals quicker changes `duration.enter` and does not also speed up whatever
happened to share a rung of `duration-200` with it.

A theme file carries them under `motion`, beside `tokens`:

```json
{
  "name": "brisk",
  "appearance": "dark",
  "motion": {
    "duration.enter": 90,
    "easing.enter": [0, 0, 0.1, 1]
  }
}
```

- **A duration is a whole number of milliseconds, 0 to 10,000; an easing is the four numbers of
  a cubic Bézier**, each `x` in 0..1 and each `y` in -1..2. Numbers and never text, which is the
  same security boundary as hex-only colour: `motionVariables` writes the CSS from the numbers,
  so `"150ms; } body { display: none"` has nowhere to go (ADR 0041's parse-and-re-emit). The
  `x` range is CSS's own rule — a curve that breaks it drops its whole declaration, so the motion
  would silently vanish — and the `y` range leaves room for an overshoot and none for a curve
  that flings a menu off the window.
- **A partial `motion` is normal, and so is none.** What a theme does not name moves the way the
  built-in of its appearance does; a bad value falls back and is reported, exactly as a colour is.
- **`0` turns a motion off.** A theme that sets every duration to zero is a still window.
- **An extension restyles motion the same way it restyles colour** — through the theme it
  contributes (ADR 0041; the panel contract of ADR 0043 is the same registry). Nothing
  about the extension path changed: the theme text goes to `load`, and `load` now reads `motion`.

### Reduced motion is the layer's job, once

Under `prefers-reduced-motion: reduce`, `drawIn` writes **every duration as `0ms`**, and follows
the setting live if the operator changes it while the window is up. A transition over no time is
no transition, and a looping animation with a zero duration has a zero active duration (Web
Animations), so a spinner does not spin and a pulse does not pulse. **A component written
tomorrow gets this for free, because it reads a token** — and `literals.test.ts` refuses a
`prefers-reduced-motion` query anywhere outside `src/theme/`, so there is no second place that
could mean something different by it.

It follows that **every looping mark is designed to read standing still.** A spinner at rest is
the loader glyph; a breathing mark at rest is the mark at full weight. Motion is decoration on a
meaning that the shape and the word already carry, never the meaning itself.

Collapsing to zero rather than swapping movement for a cross-fade is a decision, and the cheaper
of the two: WCAG 2.3.3 asks that motion _can be disabled_, and a cross-fade layer would need a
second set of keyframes per motion. The seam for it, if it is ever wanted, is `motionVariables`.

### What moves, and what is deliberately still

The rule is **motion explains a change of state; it never decorates one.** Every motion is in
one labelled section at the end of `App.css`.

| moves                                      | how                                                      | why                                                                                                     |
| ------------------------------------------ | -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| a tab being selected, on all three strips  | its surface and lit edge cross-fade, `quick`             | the change the operator just made, answered; hangs off `[data-strip]`, so a restyled strip keeps it     |
| a strip starting to collapse into `N more` | the button fades in, `enter`                             | says the tabs went somewhere; not replayed as the count changes on resize                               |
| a popover or menu opening                  | fades in a quarter-rem out of its anchored side, `enter` | says what opened it; hangs off Radix's popper wrapper, so the next popover gets it                      |
| a dialog, its scrim, the alerts drawer     | fade, no movement, `enter`                               | a question should appear where the eye already is                                                       |
| a region brought back                      | fades in, `enter`                                        | only its opacity; see below                                                                             |
| a running pipeline                         | spins, `spin`                                            | _still happening_, which amber alone cannot say                                                         |
| a queued pipeline                          | a clock that breathes, `breathe`                         | alive and not working; a spinner would claim work being done                                            |
| a pipeline that finishes on screen         | its new mark grows into place once, `settle`             | the answer arriving                                                                                     |
| a chat's state mark                        | colour, shape and ring morph, `settle`                   | the state changing, rather than blinking                                                                |
| a chat that starts waiting on you          | its ring knocks twice, `settle`                          | the one signal this app exists for, arriving                                                            |
| the same, on a row of a list               | its hand knocks twice, `settle`                          | a row's mark is the hand and has no ring; only on the change, as the ring                               |
| a row of the Chats list that was asked for | the hovered row's band for a moment, faded in once, `settle` | where a press elsewhere went; the band is there, still, under reduced motion                            |

**Nothing animates because it mounted.** A mark that animates on a change has to tell a change
from a first draw, and CSS cannot: an animation plays when its element appears. Without that, a
"settle" would play on every finished pipeline at launch, and every waiting chat would knock each
time the workspace strip brought its tabs back into view. `useArrived` (`app/src/lib/arrived.ts`)
is how a component says _this value changed while I was on screen_, and the stylesheet only
animates what it is told arrived.

Left still, on purpose:

- **The terminal, always.** It is the product and it is what the operator is reading.
- **A region's size.** A slot that grew over time would refit xterm on every frame of it, which
  is terminal output moving; a region arrives at its full size and only fades.
- **A tab's width or position.** A tab that slides under the pointer breaks aiming (`tabs.ts`);
  the collapse into `N more` is instant, and only the button that appears is drawn arriving.
- **Closing anything.** Radix unmounts a closing surface at once unless an animation is running
  on it, and a close that lingers leaves a dismissed question over the window the operator has
  gone back to.
- **Counts and words** — the needs-you badge, the gauge, the status line. They are read, and a
  number that animates is a number that is briefly wrong.
- **Hover on rows.** Only a tab eases its hover, because a tab's hover and its selection are the
  same properties; a list of fifty rows fading under a moving pointer is decoration.

**Nothing delays input.** A dialog is focusable and answerable from its first frame; the fade is
only what it looks like. The longest one-shot motion is 280 ms and nothing waits on one.

### What it costs

Measured against `origin/main` with `vite build`: CSS **+2.80 kB (+0.62 kB gz)**, JS **+3.09 kB
(+1.06 kB gz)**. Cold start gains nine `setProperty` calls in `drawIn`, beside the fifty-four the
colours already make.

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
  colour and is better than the blue it used to default to. **A shadow is a token's colour
  too** (M6.8): Tailwind's `shadow-md` and its siblings carried their own `rgb(0 0 0 / 0.1)`,
  so every shadow namespace — `shadow`, `inset-shadow`, `drop-shadow`, `text-shadow` — is
  cleared like the palette and its sizes are put back drawn in `overlay.shadow`.
  `tailwind.test.ts` compiles every one of them to hold that.
- **Motion is bridged the same way.** `ease-*` and `animate-*` are cleared like the palette, so
  `ease-in-out` and `animate-spin` are not classes; `ease-enter` and `duration-enter` are the
  motion tokens; and a bare `transition` takes `duration.quick` and `easing.standard` instead of
  Tailwind's 150 ms. Tailwind still makes `duration-150` from any integer and no `@theme` entry
  can stop it, so `literals.test.ts` refuses one in the source.
- **Preflight is not imported, and `App.css` is imported into a layer.** A reset would restyle
  1,330 lines in one commit, and `scenario tests` read the real DOM. The layer order —
  `theme, base, purlis, components, utilities` — is what the reset would have been for:
  unlayered CSS beats layered CSS, so `App.css` had to go _into_ a layer or no utility could
  ever override it. Turning preflight on is its own change, with its own evidence.

**The Tailwind colour name is the token name**, stutter and all: `text-text-primary`,
`border-border-subtle`. A prettier alias would be a second vocabulary.

> **A dependency's stylesheet is in a layer too — `vendor`, below `purlis`** (M6.8). xterm's
> own stylesheet used to be imported from `SessionPane.tsx`, which put it **outside every
> layer**, and unlayered CSS beats every layer at any specificity. One declaration of it did
> collide, and it was measured (purlis#193): `.xterm .xterm-viewport { background-color:
#000 }`. A terminal is whole rows in a box that is not, so every pane has up to a row of slack
> at its bottom, and that strip was pure black under a terminal drawn in `#181818` — the
> operator's _"harness bottom seems overflowed - you can see black space"_. It was patched with
> an inline style read from the theme once per pane, which a theme switched later could not
> reach. Now `styles.css` imports it as `@import "@xterm/xterm/css/xterm.css" layer(vendor);`,
> `App.css` paints the strip `var(--terminal-background)` and wins by layer order, and a live
> theme switch repaints it with everything else. `pane-fill.e2e.ts` holds the pixel.
>
> **The guard is structural**, because `literals.test.ts` reads purlis's own sources and a
> colour a dependency ships is invisible to it: the same test refuses a stylesheet imported from
> anywhere but `styles.css`, and an `@import` there without a `layer(...)`.

**shadcn/ui: the conventions, and components when something needs one.** `cn` is at
`app/src/lib/utils.ts`, at shadcn's address with shadcn's two dependencies, so a component
pasted from shadcn finds what it expects. Nothing else is here yet, deliberately: copying in
components nothing renders is dead code, and `class-variance-authority` arrives with the first
component that has variants.

> **Settled by the operator on 2026-09-22 — copy shadcn components in.** This file used to flag
> a conflict here: `docs/ui-primitives.md` said _"Do not write a wrapper layer around them. No
> `<Modal>`, no `<Field>`, no house component library,"_ and a shadcn component is literally a
> thin wrapper around a Radix primitive. The ruling is that those are two different things
> wearing one word. A component library is **a dependency that owns your markup** — an upstream
> you cannot edit, an API you are stuck with, a look you fight. **A copied-in component is our
> own code in our own repo, editable line by line.** The rule is now _no library between you and
> the primitive, and no indirection you cannot read_, and copied-in source is neither. purlis
> **ADR 0037**'s amendment of 2026-09-22 holds the decision and the reasoning, and is
> authoritative over both this file and `ui-primitives.md`.

**What a copied component has to satisfy.** This file rather than `ui-primitives.md` has the say
on the first three, because they are about what the window is drawn _in_:

- **Every Tailwind colour class has to be renamed to this vocabulary, by hand, and nothing will
  tell you if you forget.** A shadcn component ships `bg-background`, `text-foreground`,
  `bg-destructive` — names from shadcn's own token set, which this app does not have. They are
  not classes here: the palette is deleted above and these were never in it, so each one emits
  **no CSS at all**. Nothing goes red. `literals.test.ts` catches a hex literal and an arbitrary
  value; it does not catch a class that does not exist, and `tailwind.test.ts` checks that the
  palette is gone rather than that a source file avoided it. The result of missing one is an
  element rendered undressed — which is exactly the `claudeclaudeclaudebuilt-indefault` defect
  ADR 0037 was written about. **Read the copied file's classes against the `@theme` block
  in `app/src/styles.css` before the PR, and look at the component running.**
  `bg-surface-base text-text-primary` is the shape they should end up in.
- **No arbitrary value survives the paste** — `text-[13px]`, `rounded-[6px]`, `bg-[#fff]`. This
  one _is_ mechanical: `literals.test.ts` reads the real source tree and fails on it.
- **No colour literal**, in the same test, for the same reason as everything else in this file.
- **It goes at shadcn's address**, `app/src/components/ui/`, and it is **edited freely**. That is
  the condition rather than a permission: a copy kept pristine "because upstream will fix it" is
  a dependency with worse ergonomics and no version, and there is no upstream once it is copied.
- **Say where it came from, and at what version, in the file.** No manifest records a copied
  file, so the file is the only place its provenance can live.

Still refused, unchanged by the ruling: a component library as a **dependency**, and a house
abstraction layer over Radix — `<ConfirmModal open onConfirm>` — whether it is written here or
copied from somewhere. Copying it would not launder it; what is refused is a purlis API in
front of the primitive. **The one exception** is the settings set (ADR 0037, amended
2026-10-04, V89f and V89j): SettingsLayout, SettingGroup, SettingRow, Field, Choice and
SettingActions in `app/src/settings/components.tsx`, six thin pieces over the Radix primitives
and native elements already in use, drawn in tokens. `docs/ui-primitives.md` gives the reasons. Nothing else joins it without a new
amendment.

**The answer bar** (`app/src/AnswerBar.tsx`, #1210) is the one row a question dialog is answered
from: the quit warning, a delete's confirm, approving a project, Restart to update, About. A
question with a way out and an act or two and nothing to fill in ends in it, as a form ends in
`SettingActions`. It is a `<div className="answer">` and no more, so it is neither a settings
piece nor an API in front of a primitive: the buttons inside it are native `<button>`s, or
Radix's own `Cancel`, `Action` and `Close` around one, written by the dialog. The rules it
carries are the dialog's to keep, and every question keeps the same ones:

- **At the trailing edge.** `.answer` is a flex row pushed to the end, with a gap, so every
  question's answers stand where the reader's eye ends the dialog.
- **The way out first, then the acts**, and the answer that moves things on last, at the edge:
  Cancel then Delete, Close then Read again, Cancel then Close then Smart close.
- **What cannot be taken back says so** with `ends-it`, and it is never the one Return finds:
  the dialog focuses its way out (or, for Reopen your sessions, the answer that loses nothing).
- **Focus and Escape stay the dialog's.** The bar handles no key. Escape is the dialog's Cancel,
  or whatever that dialog says it is.

`answerBar.guard.test.ts` fails on a dialog that builds an `answer` or `doing` row by hand, and
on a dialog that puts an `ends-it` act in a form's `SettingActions` (D-1210-8).

**One rule for both rows.** A form's `SettingActions` stands at the leading edge, in the form's
flow, with its act first (Create workspace, then Cancel). A question's bar stands at the
trailing edge with its act last. In both, the act that moves things on stands at the edge the
row is anchored to, and the way out sits inward of it. A dialog that holds a form that makes or
changes something (New vault, Rename) ends in the form's row, not the bar. **A confirm whose
only field is the typed name of what it ends is a question** (D-1210-8): Delete vault ends in the
bar, Cancel first and the delete last, as Delete workspace does. Its keyboard lands in the name
box, because typing is the answer, and Return there deletes only once the name is typed exactly
(D-1210-9). Whether every dialog that holds a form should end at the trailing edge would amend
V89j, and is left open.

**Lucide** is the icon set (`lucide-react`). The property that matters is that it draws with
`stroke="currentColor"` and `fill="none"`, so an icon takes the colour of the text it sits in
and a theme reaches it without an icon ever naming a colour. `app/src/lib/icons.test.tsx` pins
that.

**The icon layer is one CSS rule and no component.** Lucide puts `lucide` on every `<svg>` it
draws, and `App.css` sizes that class at `1em` — so an icon is the size of the text it sits in,
and no call site passes a `size`. Lucide also adds `aria-hidden="true"` to any icon given no
accessible name of its own, which is what keeps a button's name its words: `New tab` with a `+`
beside it is still `New tab` to a screen reader and to `pressOnly("New tab")`. Both facts are
pinned in `icons.test.tsx`, because the whole window leans on them and neither is ours.

The rules an icon has to meet here:

- **Beside words, never instead of them.** An icon-only control is one an operator has to learn.
  The exception is a control whose accessible name is already carried by `aria-label` — a tab's
  `×`, whose name is the catalogue's `End chat 3 steward`.
- **An icon that does not help a reader find something is noise at fifty sessions.** Marks go
  where they tell two kinds of thing apart (a worktree leaf from a chat leaf) or where they are
  the state (a pipeline's tick, cross or spinner). Not on every row because rows can have one.
- **Chosen by what a thing IS, not where it is.** The layout is data (`regions.ts`), so a region
  toggle drawn as "left panel" would point at the wrong edge the first time the region moved.
- **An icon that moves says _still happening_, and nothing else loops.** `.spinning` is a running
  pipeline and a listing purlis is still waiting for; `.breathing` is a queued pipeline. A
  settled answer is still, apart from the one `settle` it gets if it arrived while on screen.
  Reduced motion stops both loops and leaves the mark ([Motion](#motion-is-data-too)).
- **A contrast floor applies to an icon's colour as it does to text** — 3:1 for a graphic. A
  state colour that is too weak for words (`needs-you.base` measures 3.64:1 on `surface.base` in
  charter-dark) may colour the mark beside the words and never the words.
- **An icon is Lucide's or the icon theme's, never a glyph.** A ✓, a ▸ or a ✗ typed as text is
  the font's: its weight, its size and its baseline, which change with the font and match no
  icon beside it. `app/src/lib/strayIcons.test.ts` fails on a glyph drawn alone as an icon, on an
  `<svg>` drawn by hand outside the files it names, and on any other icon package (DS-4, #627).
  A glyph inside words is prose, and is left alone.

**Files and folders are the one exception: they are drawn from an icon theme** (FM-3, #1106), a
data file beside the colour theme (`app/src/theme/icons.ts`). It maps a file's whole name, then
its extensions longest first, and a folder's name, to a symbol of path outlines, and each outline
names one of the `icon.*` tokens — so the icons follow light and dark with the colour theme, a
colour theme recolours every icon set, and the literal guard covers them. The hue is the point
there: it is what tells a Rust file from a TypeScript one in a tree of hundreds. purlis's own
set, `charter-icons`, is a 67-symbol subset of Material Icon Theme (MIT), vendored with its
licence under `app/icons/material-icon-theme/` and shown in About; `icons.vendor.test.ts` holds
the converted symbols to those files. A project picks another with `[theme] icons`, and an
extension contributes one as `contributes.icon_themes`. A symbol is drawn by `FileIcon.tsx` as React
elements it builds, never as SVG text, so a contributed icon theme has nowhere to put a script, a
link or a style.

## The interrupt budget

**At most three prompts stand between a new machine and the first answered agent turn** (W10,
DS-9). It is part of ST9's definition of done, and the first run's scenario tests hold it: every
scenario in `app/src/FirstRun.test.tsx` counts the prompts it shows and fails above three,
naming each one. The longest way to a chat those scenarios take spends all three today: the
forge question when the repo's remote does not say, the trust question on a new machine, and the
picker when there is a choice. A new prompt on that way fails the build until another one goes.

A prompt is anything that stops the operator until they answer it, read from what the page says
it is (`app/src/interruptBudget.ts`):

- every `dialog` and `alertdialog`, an open native `<dialog>` included;
- every inline question: a `group` or `radiogroup` named by its question, ending in "?", and a
  `<fieldset>` whose `<legend>` is one. Moving an ask out of a dialog onto the page does not
  make it free.

It cannot see a question named without its "?", a toast or status line that carries an action,
or a native OS dialog such as the folder picker; a review has to.

So a surface that has to ask something on the way to the first chat is named by its question,
and the budget sees it. What costs nothing is a tab opened beside the chat (the first task, the
repo's agent instructions, the harness setup), a `status` line, and a sign-in run in a shell tab
the operator can leave.

### Nothing purlis says covers what a chat wrote

**A pane's Notices take a row of the pane, above its terminal, and never draw over it**
(#1647). The terminal is what the person reads to answer a question, so a question drawn over
it hides what it asks about: the operator's screenshot of 2026-10-10 had three Notices stacked
in a pane's corner over the conversation. Two stand in the row and the rest wait behind "+N
more", the band's rule (V91i), so purlis's own questions cost a chat at most two Notices' height
of terminal. Only the chat at a glance, one short line of chips, stays in the corner over the
terminal. The shape and the rules are in `docs/ui-primitives.md` ("The Notice is a house piece
too").

## What it costs

Measured on this branch, against `origin/main`:

|           | main                     | this                     | delta                  |
| --------- | ------------------------ | ------------------------ | ---------------------- |
| JS bundle | 749.64 kB (214.12 kB gz) | 755.44 kB (215.85 kB gz) | +5.80 kB (+1.73 kB gz) |
| CSS       | 18.08 kB (4.24 kB gz)    | 23.52 kB (5.23 kB gz)    | +5.44 kB (+0.99 kB gz) |

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
