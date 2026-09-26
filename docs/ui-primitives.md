# The window's UI primitives

**Radix UI. New UI is built from its primitives, not from hand-rolled markup.**

This is a rule and not a preference, so the rest of this file is the reasons for it and the way
to follow it.

**Its sibling is [`design-system.md`](design-system.md), which says what the window is drawn
_in_: a theme is a data file, every colour is a semantic token, and a literal anywhere outside
`app/src/theme/` fails the build. Read that one before writing a rule with a colour in it, and
for what a copied-in shadcn/ui component has to satisfy.**

**Where the rule is decided.** charter **ADR 0037**, _charter takes the behaviour and keeps the
look_, and its amendment of 2026-09-22, which settled the conflict this file used to flag. That
record is authoritative; this file is its code-side expression, and where the two disagree this
file is the defect.

## The rule

- A dialog, a radio group, a checkbox, a menu, a popover, a tooltip, a collapsible section, a
  tab list: take the Radix primitive. Install the one package you need
  (`@radix-ui/react-<thing>`) — the primitives tree-shake per package, so the window pays for
  what it uses and nothing else.
- **No library between you and the primitive, and no indirection you cannot read.** No `<Modal>`,
  no `<Field>`, no house component library. The primitive is the component; charter's look is CSS
  on it. A charter API in front of Radix is the custom tooling this repo's first rule exists to
  prevent, and it is how a primitives migration turns back into hand-rolled markup with extra
  steps. The test is at the **call site**: can the next person see which primitive this is and
  reach its props?
- **A component's source copied into this repo is neither of those, and is allowed.** Until
  2026-09-22 the rule above read _"do not write a wrapper layer around them"_, which forbade a
  shadcn/ui component — a shadcn component is literally a thin wrapper around a Radix primitive.
  The operator settled it: that rule was written against **a dependency that owns your markup**,
  and a file in `app/src/components/ui/` is ours, editable line by line, and visible in the diff
  that adds it. `docs/design-system.md` has what a copy must satisfy.
- Charter's own look, always. Radix ships **no CSS at all** — every primitive is an unstyled
  element with `data-state` attributes to hang rules off. `App.css` stays the one place the
  window is drawn — but **no colour is written in it**: every one is `var(--<token>)` and the
  values live in `app/src/theme/`. See `design-system.md`.
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
The terminal panes are xterm.js and are not a candidate.

And the chat strip's **show-more menu** (`@radix-ui/react-dropdown-menu`, ADR 0039),
which is the first surface built under this rule rather than converted to it. Two decisions it
does NOT share with the four dialogs, both because it is a menu and not a question:

- **It is not modal** (`modal={false}`). A modal Radix surface marks everything outside itself
  `aria-hidden`, which is right for a dialog that must be answered and wrong for a menu on a
  strip — the rest of the window stays reachable, to a screen reader and to a scenario spec.
- **A click outside closes it.** The dialogs prevent `onInteractOutside` because a click outside
  would answer a question by accident. A menu has no answer to lose, and every menu on every
  platform closes this way.

It is on all three strips — projects, workspaces and chats — because the operator's ruling of
2026-09-22 was about all of them: a strip that has more than it can draw collapses the rest into
this menu rather than scrolling. What decides how many fit lives in `app/src/fits.ts` and is
arithmetic over one measured width per strip, not a measurement of fifty tabs: every tab takes an
equal share of its strip and is never drawn narrower than a floor, exactly as a browser sizes its
own tabs, so `n` tabs fit in `width` when `width / n >= least`. It replaced an
`IntersectionObserver` over every tab, which is the right question about a strip that scrolls and
a loop on a strip that collapses.

And the **context menus** (`app/src/Menus.tsx`): `@radix-ui/react-context-menu`, on the project
tabs, the workspace tabs, the chat tabs, the panes, the explorer's worktree and clone rows, the
persona rows and the bottom bar's repo rows. Four things about them are decisions and not
details:

- **A menu is a third reader of `app/src/actions.ts`**, after the palette and the bar.
  `actions.menuRows` filters the one catalogue to the item the menu was opened on, and a row the
  catalogue does not have is not in the menu — which is how the strip of chats outside every
  workspace gets a menu with no pin and no delete in it without anything in `Menus.tsx` knowing
  that strip exists. A menu with a list of its own is the defect the tmux frame shipped:
  `frame/tabmenu.py` and its palette grew two answers to "what can I do to this".
- **Not modal, and a click outside closes it**, exactly as the show-more menu above. It is a
  menu, not a question.
- **A row's accessible name is the catalogue's title alone** (`aria-label`), and its note — what
  the row costs — is its `aria-describedby`. Left to the content, every row would announce as a
  paragraph: _"End chat 3 steward Ends the program it runs. There is no undo."_
- **Shift+F10 and the menu key open it on the element that has the keyboard** (charter-app#174).
  macOS has no keyboard convention for a context menu and its WebView raises no `contextmenu`
  for either key, so `Menued` dispatches the one a right-click would — and only when the
  trigger itself has focus, never a terminal inside the panes' trigger, whose program may want
  the key.

And with them, **the WebView's own menu is taken away from the whole window**
(`useNoBrowserMenu`), because a shipped app that answers a right-click with `Reload` and
`Inspect Element` is showing the operator the browser it is built on. Two properties of that one
listener are load-bearing:

- **It is on the BUBBLE phase.** Radix opens its menu from an `onContextMenu` composed with
  `composeEventHandlers`, which skips its own handler when the event is already
  `defaultPrevented`. React 19 attaches its delegated listeners to the root container, which is
  below `window`, so a capturing listener there would run first and silently stop every context
  menu in the app from ever opening.
- **It leaves an `input`, a `textarea` and a contenteditable alone.** That menu is the platform's
  Cut/Copy/Paste and it is the only pointer route to the clipboard this window has. The
  inspector is off in a release build anyway.

And the **delete-a-workspace dialog** (`app/src/DeleteWorkspace.tsx`):
`@radix-ui/react-alert-dialog`, the first use of that primitive here. It is `Dialog`'s sibling
for a question whose answer destroys something — `role="alertdialog"`, so what a screen reader
is handed first is the sentence about what is about to be lost; Cancel focused by the primitive;
Escape meaning Cancel. It keeps the four dialogs' rule that a click outside answers nothing.

And the **alerts drawer** (`app/src/AlertsDrawer.tsx`, M6.5): `@radix-ui/react-dialog` drawn as a
sheet from the right, over the whole window, opened from the status line. It is the primitive
itself with charter's CSS on it — not a copied shadcn `Sheet`, whose class list is written in
shadcn's token names and would have emitted no CSS here (`design-system.md`). It is modal, and it
parts from the four dialogs below on one decision: **a click outside closes it**, because a
drawer asks nothing and a stray click cannot answer anything. Radix hands focus back only to a
`Dialog.Trigger`, and the button that opens this lives in a project's status line while the drawer
is the window's, so the drawer remembers where the keyboard was and puts it back itself.

And the **question a relaunch asks** (`app/src/RelaunchAsk.tsx`, charter-app#250): an
`AlertDialog`, because it arrives without being asked for. **"Reopen all sessions" is the
primitive's `Cancel`**, first and focused, so Escape and a stray Return both keep the work.
**"Start fresh" is a plain button, not the primitive's `Action`**: an `Action` also closes the
dialog, and closing is this dialog's "Reopen all", so one press would send both answers.

And the **ask before Restart to update** (`app/src/Updates.tsx`, charter-app#251): an
`AlertDialog` naming each chat that is mid-turn or reports no state, in the quit warning's rows
and words. **"Wait" is the primitive's `Cancel`**, first and focused; "Restart now" is a plain
button, for the relaunch question's reason.

And the **question before a chat ends** (`app/src/EndingChat.tsx`):
`@radix-ui/react-alert-dialog`, the operator's _"closing session should ask confirmation"_. It is
the first surface here that is **not** a `Dialog`, and the reason is the role: an
alert dialog is `role="alertdialog"`, announced as an interruption rather than as a surface, and
the primitive requires a `Cancel` that focus goes to. The four below are questions the operator
went looking for; this one arrives _because of_ something they did, which is the distinction the
role exists for. Two consequences worth knowing before the next one:

- **`AlertDialogContent` takes no `onInteractOutside`.** It refuses outside interaction itself,
  so the rule the four `Dialog`s write out by hand is the primitive here. A reviewer looking for
  the missing line should find this paragraph rather than a hole.
- **It is asked in one place** — `PlaneView`'s `run`, which carries out a catalogue row from
  whichever surface pressed it — so a tab's `×`, a pane's `×` and the palette's rows all ask.
  A confirmation on one surface and not another is the second answer the catalogue exists to
  not have.

And the **persona card** (`@radix-ui/react-popover`, `app/src/Panels.tsx`): what a row in the
right-hand region's persona list opens. It is the first popover in the window, and it was picked
over the other two surfaces Radix has for the same content:

- **Not a dialog**, because a dialog is modal and modal is wrong here twice. Radix marks
  everything outside an open dialog `aria-hidden`, which then
  included the needs-you queue two sections up (in the title bar since charter-app#249) — the
  one surface ADR 0038 says this region must never compete with — and a modal is
  for a question that has to be answered before anything else happens. A persona's role is
  reading.
- **Not a sheet**, because the window already has one and it is the window's: `AlertsDrawer` is a
  sheet from the right over every open project. A second sheet, over one project's region, would
  be two drawers with two different rules and two different scopes.
- **A popover is anchored to the row it is about**, which is what makes a card legible when five
  of them are listed one under the other.

It takes the show-more menu's two decisions rather than the four dialogs': **not modal**, so the
rest of the window stays reachable to a screen reader and to a scenario spec, and **a click
outside closes it**, because there is no answer to lose. `side="left"` is where it opens from in
the default arrangement and no more than that — a region MOVES (ADR 0038), and Radix flips to the
other side when there is no room, which is what makes naming a side safe at all.

**And then the persona card left the popover for a tab (2026-09-23).** It had grown a searchable
archive of everything a persona remembers and an extension's statistics, which a popover anchored
in a 260 px column is a thin surface for; a non-modal sheet over the centre region was built and
then replaced the same day, when the operator was asked where the card should open and chose
**"Its own tab"**: _"we dont have other tabs then sessions, and this can be good example for us -
that in tabs we can have what we want - not only harnesses"_.

So a persona is a **view**, and a view is what a tab's pane holds when it does not hold a chat
(ADR 0043, as amended; `app/src/tabs.ts`, `app/src/Views.tsx`). No primitive is involved
beyond the tablist the strip already is, and that is the point: the accessibility the sheet had to
argue for — not modal, not hiding the queue, dismissible, focus returned — is a tab's by
construction. A tab is not dismissed by focus leaving it, which is what closed the sheet when the
palette handed the keyboard back to a terminal (#212's adversarial review, finding 5). Every other
row's card is still the popover above, because a todo's six short rows are what a popover is for.

An extension's view — persona statistics — opens in a tab of its own the same way, from a button
on the personas panel's heading, a button on a persona's tab, or its palette row.

Two decisions those four share, taken once so they do not have to be taken again per dialog:

- **A click outside answers nothing.** `onInteractOutside` is prevented on all four, which is
  how every surface in this window has always behaved. It is written out rather than left to
  the default so it reads as a decision.
- **Escape answers, with the non-destructive answer.** The picker and the palette always did;
  the quit warning and the trust prompt did not, and a modal with no keyboard way out is the
  one thing a modal must not be. In both, Escape is exactly Cancel — nothing is ended, nothing
  is approved, and the core is told so the next ask is a first ask again.

## Two things learned converting them, which will catch the next person

**A radio group's pick does not follow the arrow keys on its own.** Radix selects an item on
focus only while it believes an arrow key is down, and it learns that from a `keydown` listener
on `document`. React attaches its delegated listeners to the root container and to each portal
container, both below `document`, so React's handler moves the focus before Radix's listener
runs. Every `RadioGroup.Item` in `StartChat` therefore carries its own `onFocus` that picks it.
The reasoning and the measurement are in that file; the guard is the test named "moves between
harnesses with the arrow keys".

**The arrow-key defect is the radio group's, not every primitive's — measured, not assumed.**
The menu added under ADR 0039 was expected to need the same `onFocus` repair and does not: its
roving focus is an `onKeyDown` on the content element rather than a `keydown` listener on
`document`, so React's delegated listeners being below `document` never comes into it. The guard
is `Strip.test.tsx`'s "moves between its rows with the arrow keys", which was written to fail and
passed first time. The rule the next primitive inherits is therefore **check, per primitive**:
the question is where Radix listens, and only a primitive that listens on `document` has this.

**A scenario run cannot right-click, on either engine — so a context menu is driven by the
event and not by the pointer.** `element.click({ button: "right" })` is a W3C pointer sequence;
`contextmenu` is a platform default action the engine raises from a native right-click, below
where a synthesised sequence lands. Measured in charter's own window with a listener on the
element: **0 `contextmenu` events after a WebDriver right-click, 1 after a dispatched
`MouseEvent`** (webkit 605.1.15 on macOS, 2026-09-22), and run 35771806598 was red the same way
on WebKitGTK 605.1.15. `e2e/specs/workspace-lifecycle.e2e.ts` therefore dispatches the event,
and says so where the dispatch is.

**That is a driver limit and not a product one, and the discriminator is a jsdom test.**
`src/Menus.test.tsx`, _"a real contextmenu event, with the suppressor live"_, dispatches one
`MouseEvent` at a real workspace tab of the real `App` with `useNoBrowserMenu` mounted and no
WebDriver anywhere, and charter's menu opens. Making that suppressor capture-phase — the one way
charter could swallow the event — turns that test and only that test red. Put the question where
the driver is not, before changing a spec that cannot answer it.

**A modal dialog really is modal, and the tests notice.** Radix marks everything outside the
open dialog `aria-hidden`, so a `getByRole` for anything behind it finds nothing, and a scenario
spec cannot click a tab while a picker is up. That is the app behaving correctly. When a test
breaks on it, the test was reaching for something an operator could not have reached — fix the
test to take a route that exists.

**Tab inside a dialog is the PLATFORM's, except at the two edges — and that is the third time
"check, per primitive" has earned its place.** `FocusScope`'s `handleKeyDown`
(`@radix-ui/react-focus-scope`) intercepts Tab only when the focus is on the first or the last
tabbable of the scope: on the first it acts on **Shift+Tab** and moves the focus to the last
itself, on the last it acts on **Tab** and moves to the first. Anywhere in between it does
nothing at all and the browser's own tab sequence decides.

That matters because **WebKit does not put a `<button>` in the tab sequence at all** unless "tab
to all controls" is turned on, or the button's `tabindex` is written down — and **WebKit is the
engine on both platforms the scenarios run on**: a WKWebView on macOS and WebKitGTK on Linux.
charter embeds the system WebView, so this is the window's own behaviour rather than one runner's
quirk. The second half of that sentence is the whole of the fix and is the section below; it was
not known when the rest of this was written.

It was measured rather than reasoned about, three times, and the third measurement **corrected
the first two**. `palette.e2e.ts`'s _"closes the chat it just opened, by the keyboard alone"_
pressed Tab to move from `Cancel` to the confirm; the question stayed on screen on `webkit macos`,
and after that was read as a macOS default it did the same on `WebKitGTK linux`. Then the spec was
made to write down every keydown the document sees, and it said (charter-app#176):

```
the page saw: Shift on <button> "Cancel"; Tab on <button> "Cancel"; Enter on <button> "Cancel"
```

**Two separate findings live in that one line, and only the first is about the product.**

1. The tab sequence really does skip buttons, as above — and the trace adds that
   `browser.keys(["Shift", "Tab"])` **is not delivered as a chord**: the Tab keydown arrives with
   `event.shiftKey` unset, so Radix's edge handler never fires either.
2. **A focused button is not activated by a synthesised `Enter`.** The engine delivered the
   keydown _to_ `Cancel`, unprevented — the focus was genuine and the engine agreed — and nothing
   happened. WebDriver key actions carry no implicit activation.

The second is a fact about the **test rig**, not about charter, and it is the one that matters
when writing a spec: **a scenario cannot press a button by keyboard at all, by any key.** That is
why Escape works in these specs where nothing aimed at a button does — Radix listens for Escape on
`document` — and why the palette's own Enter works, since the palette handles it in JavaScript.

**A third face of the rig's finding, and it is the same fact underneath — measured in
charter-app#186 while trying to prove the fix below in a real window.** The two above read like
two unrelated quirks; they are one. **This driver dispatches a synthetic DOM keydown and performs
no default action whatsoever.** The decisive measurement is a control nobody can argue with: two
plain text `<input>`s, injected into an open dialog in the running app, with the focus on the
first.

```
PROBE start: input "probe-one"
PROBE after Tab from text box: input "probe-one"
PROBE keys seen: ["Tab shift=false on INPUT prevented=false"]
```

**Tab did not move the focus between two text boxes.** WebKit tabs between text fields with full
keyboard access off — that is ordinary Safari, and `TextFieldInputType::isKeyboardFocusable`
never consults the gate at all — so there is nothing left for this to be but the driver. The
keydown arrived, unprevented, and nothing followed it. That explains all three symptoms at once:
no activation from `Enter`, no focus navigation from `Tab`, and no `shiftKey` on a chord, because
a synthetic dispatch carries none of them.

So, for whoever writes the next spec: **the keyboard half of a scenario can only assert what the
app does in JavaScript.** Escape, `F2`, the palette's own Enter — all handled by a listener — are
fair game. Anything the _engine_ would have done in response to a key is not, and asking for it
produces a red that looks like an app defect and is not one. The reachability half of
charter-app#186 therefore stayed in jsdom, where the engine's rule is written down and modelled
explicitly; `picker.e2e.ts` keeps the half a scenario really can prove, which is that the
attribute the rule needs survives the build and is on the element in the shipped app.

Three things follow, and the last is the one a reviewer should hold us to:

- **Shift+Tab from the first answer is Radix's own `focus()` call**, not the engine's tab
  sequence, so it reaches the last answer everywhere a real keyboard is driving. A _scenario_
  cannot use it, for finding 1 above; `App.test.tsx` is where that route is tested.
- **A claim of the form "this button can be pressed by keyboard" belongs in a unit test**, where
  jsdom implements activation. A scenario can prove that the keyboard reaches a surface and that
  keys the app handles in JavaScript do their work — `palette.e2e.ts` keeps exactly that half,
  raising the question by keyboard and answering it with Escape.
- **A control that is neither edge of a modal is reachable by neither mechanism**: the engine
  will not tab to a `<button>` and Radix only handles the edges. That was the state of this
  window until charter-app#186, and the section below is what was measured and what was done
  about it.

## What Tab actually reached in each modal, and the one attribute that fixed it

charter-app#186 asked the question above of **every** modal surface rather than of the one
dialog a scenario happened to break on, and the answer was worse than the ticket's guess.
`app/src/Modals.keyboard.test.tsx` is the measurement and now the guard; it walks each surface
with a Tab that is dispatched for real, so Radix's edge handling runs rather than being modelled,
and only the engine's half is written down. **This is a jsdom file on purpose**: a scenario
cannot answer the question at all, for finding 2 above.

What it found, before anything was changed:

| surface                            | tabbables | opened on       | Tab reached                  | Shift+Tab reached      | reachable by neither                      |
| ---------------------------------- | --------- | --------------- | ---------------------------- | ---------------------- | ----------------------------------------- |
| `StartChat` (the picker)           | 6         | `Cancel`        | **nothing — it never moved** | the form, then `Start` | the footer checkbox                       |
| `Updates` (the offer)              | 5         | channel         | nothing                      | `Close`                | `Install`, `Check now`                    |
| `NewProject`                       | 5         | the folder box  | nothing                      | nothing                | `Browse…`, the checkbox, `Create project` |
| `Extensions`                       | 4         | `Add…`          | nothing                      | `Done`                 | `Review`, `Remove`, per row               |
| `NewWorkspace`                     | 4         | the name box    | the vision box               | nothing                | `Create workspace`                        |
| `Doctor`                           | 3         | `<summary>`     | nothing                      | `Close`                | `Check again`                             |
| `QuitWarning`, `EndingChat`        | 2         | `Cancel`        | nothing                      | the other answer       | —                                         |
| `ApprovePlane`, `ApproveExtension` | 2         | `Cancel`        | the other answer             | nothing                | —                                         |
| `DeleteWorkspace`                  | 2         | `Cancel`        | nothing                      | the delete             | —                                         |
| `AlertsDrawer`, `PinItem`          | 1         | its one control | —                            | —                      | —                                         |
| `Palette`                          | 1         | its box         | —                            | —                      | —                                         |

The last three rows arrived from charter-app#172 while this was being measured, each with the
defect on the day it was written — which is the argument for a file that walks every surface
rather than a fix per dialog. `NewProject` is the sharpest: only its folder box was in the
engine's sequence at all, and the checkbox in the middle is the one that decides whether charter
writes into a repository the operator already has.

Read the `Tab reached` column first, because it is the one an operator lives in: **in eight of
the fourteen surfaces, pressing Tab moved the focus nowhere at all**, and in a ninth it moved
one step between two text boxes and then stopped. Three of the remaining five have a single
control and nowhere to go by construction. That leaves two — `ApprovePlane` and
`ApproveExtension` — where Tab did what Tab does, and it worked there for no better reason than
that `Cancel` happens to be written second in their JSX rather than first.

**The ticket's headline is half refuted, and the half that survives is the worse half.** It said
the picker's `Start` could not be reached; `Start` could be reached, by walking the dialog
backwards through every form control and out the far side on Radix's first edge — and never by
Tab. What was reachable by neither key was the footer checkbox, ADR 0029's one choice.
ADR 0022 makes this dialog the only way a chat ever starts, so "you may start a chat, but only
by pressing Shift+Tab five times" was the keyboard-only path to starting one.

**Also refuted: "radio groups and checkboxes are in WebKit's restricted tab sequence."** They
are when they are `<input>`s. Radix's are not — `RadioGroup.Item` and `Checkbox.Root` both
render `Primitive.button`, and the hidden `<input>` each keeps for form submission is
`tabIndex={-1}`. The radio rows were reachable for an unrelated reason: roving focus writes a
`tabindex` on them. The checkbox has no roving focus and was not.

**The fix is one attribute, and it is the engine's own escape hatch rather than a handler of
ours.** WebKit's rule, in full:

```cpp
bool HTMLFormControlElement::isKeyboardFocusable(const FocusEventData& focusEventData) const
{
    if (!!tabIndexSetExplicitly())
        return Element::isKeyboardFocusable(focusEventData);
    return isFocusable() && document().frame()
        && document().frame()->eventHandler().tabsToAllFormControls(focusEventData);
}
```

**A `tabindex` that is written down is never weighed against full keyboard access at all.** The
two lines that say so are WebKit's own, added in `[popover] Improve focus handling`
(r263447, 2023-04-29) and shipped in Safari 17 and WebKitGTK 2.42 — both older than anything
charter runs on. So every `<button>` inside a modal surface in this window now says
`tabIndex={0}`, and Tab moves through these dialogs the way it moves through every other window
on the machine.

Three things about that choice, because each was a fork:

- **It is not a hand-written Tab handler, which ADR 0037 is against and which this
  would have been the fourth of.** Nothing wraps a primitive, nothing intercepts a key, and
  Radix's own edge behaviour is untouched and still does the wrapping at the ends.
- **It is not a WebView setting, and that route does not exist.** Turning "tab to all controls"
  on for charter's own window would be the tidier answer and there is no public API for it:
  macOS exposes full keyboard access as a system preference and `WKPreferences` has only
  private SPI for the web half of it. WebKitGTK's `enable-tabs-to-links` is about links.
- **It goes on the two-answer dialogs as well**, which did not need it. "The keyboard works
  here" was a property of _how many buttons there are_, and a dialog that grows a third control
  should not silently lose it — which is the failure this whole section is the record of.

One consequence worth knowing before the next one: the same WebKit change makes an explicit
`tabindex` mouse-focusable too, so a click now leaves the keyboard on the button it pressed,
as it does on Windows and Linux and unlike the macOS default.

**Outside the modals, this is still true and is not fixed.** Every `<button>` in the window —
the tabs, the pane controls, the status line, the explorer rows — is equally absent from
WebKit's tab sequence, and nothing there has a focus scope to wrap at the edges, so there is no
"reachable backwards" to fall back on. That is a bigger change than a dialog's answer row and a
different question (where should Tab go between four regions?), so #186 stopped at the modals,
where a focus scope makes the boundary obvious and the surfaces are countable. charter-app#189
carries the rest.

## Where Tab goes in the rest of the window (charter-app#189)

The attribute above fixed the dialogs. Outside them it is the floor and not the answer: a
`tabIndex={0}` on every button would put fifty chat tabs between the explorer and the terminal.
So the window takes the WAI-ARIA Authoring Practices' shape, with the primitive this repo already
had in its tree under the radio groups and the menus, **`@radix-ui/react-roving-focus`**. It is
the package Radix's own `Tabs`, `RadioGroup`, `Menu` and `Toolbar` are built on rather than one of
the primitives it documents, and it is taken directly because `Tabs` would bring a `Tabs.Content`
model and a select-on-`mousedown` this window's strips do not have:

- **Each strip is ONE Tab stop** — projects, workspaces, chats. The selected tab says
  `tabindex="0"` and every other `-1`; Left, Right, Home and End move along it; Enter or Space
  selects, which is the button's own click. Moving is not selecting ("Tabs", manual activation):
  arrowing past a chat must not swap the panes under the operator.
- **Each list is ONE Tab stop** — the explorer, every panel's rows (`PanelList`, so a
  contributed panel gets it for nothing). Up, Down, Home and End move.
- **A list behind a button is a menu** — the title bar's needs-you list (charter-app#249), the
  strips' show-more menus. The button is the Tab stop; Enter opens the menu on its first item,
  the arrows move, Escape closes it and puts the keyboard back on the button. Radix's
  `DropdownMenu` does all of that, so nothing here is hand-written.
- **The explorer is a tree** (charter-app#238), the whole "Tree View" pattern on top of the
  same roving focus: `role="tree"`, each row a `treeitem` with its level and its place among
  its siblings, `aria-expanded` on every parent (a clone says whether it is open, a parent that
  cannot fold says `true`), Right to open or go in,
  Left to close or climb, and a typed letter to the next row it starts. `Explorer.tsx` says
  why the levels are written down and not left to the DOM. A tree's rows are `treeitem`s and
  no longer `button`s to a role query, so a test reaches them by that role.
- **Every other control says `tabIndex={0}`**, and a tab's `×` says `-1`: fifty closers would be
  fifty stops again. **Delete on a focused project or chat tab presses the row its `×` presses**
  (charter-app#239, `closeOnDelete` in `app/src/tabKeys.ts`), and so does Backspace on a Mac,
  whose key marked "delete" sends it: ending a chat still asks first, a view tab still closes
  without asking, closing a project with chats open now asks first too (`ClosingProject`, on
  the row's verb, so the `×`, the menu and the palette ask it as well), and the keyboard lands
  back on the tab after a Cancel and on the strip's stop after a close. A new strip with a `×`
  (a vault tab's) calls it from its tab's `onKeyDown` with that `×`'s row. The palette and the
  tab's own menu reach the same row.
- **The order is the document's, which is the order the window is drawn in**: the title bar,
  the three strips, then the regions as the arrangement places them (ADR 0038 — a region moves,
  and its stops move with it; a fixed order would contradict the layout on screen), each
  resize handle between them (`react-resizable-panels` makes a separator a keyboard control),
  the focused pane's controls before its terminal, and the status line last.
  `Window.keyboard.test.tsx` pins the whole sequence.

`useTabStop` (`app/src/roving.ts`) is the one decision the primitive leaves open: which item is
the stop before anyone has touched the group. Left alone it is none of them and the group's
own element takes the stop. It is the selected item at rest, the item the keyboard is on while
it is inside, and the selected one again when the keyboard leaves. **A new strip or list adopts
it** by wrapping its container in `RovingFocusGroup.Root asChild` with the hook's props, and each
item in `RovingFocusGroup.Item asChild tabStopId={…} active={…}`. The primitive stays visible at
the call site, which is this file's rule.

**Two things about wiring it that will catch the next person.** `useTabStop` has to be told
every item that is DRAWN — a tab the strip collapsed into its menu, or a row inside a folded
clone, is never the stop, or the group has none. And a conditional that puts the `Root` around
an element in one branch and not the other makes React remount that element when the branch
changes: the explorer's empty `nav` is inside the same `Root` as its full one for that reason,
and `FourRegions.test.tsx` found it by holding a `nav` that had been replaced.

**A terminal keeps Tab.** xterm prevents Tab and Shift+Tab and sends them to the shell —
completion, and Claude Code's mode cycling — and nothing in the window takes either. So a
terminal is a stop you can reach and cannot Tab out of, which is why the pane's own controls are
written _before_ it in the document (they are absolutely positioned, so the look is unchanged).
**They are shown on hover and while the keyboard is on them, and at no other time** — not
because their pane is focused, which is the pane the operator is typing in and a corner he
asked to keep clear. A `visibility: hidden` control is out of the sequence, so the focused
pane's are hidden by `opacity` with `pointer-events: none` instead: Tab still lands on them, and
`.pane-doing:focus-within` draws the group the moment it does.

**The way out is Ctrl+Tab, and Ctrl+Shift+Tab backwards** (`SessionPane.leavesTheChat`). It is
the platforms' own key for leaving a control that keeps Tab — GTK's text view, Windows'
multi-line box, AppKit's field editor — and by the rule in the next section it takes nothing
from the chat: xterm.js 6.0.0 ignores Ctrl on key code 9, so it would have sent `\t` and
`ESC [ Z`, which are Tab's and Shift+Tab's own bytes. WebKit has no default action for
Ctrl+Tab, so the window walks the engine's sequence itself (`app/src/tabSequence.ts`, the rule
above written down once and shared with the tests). The palette (`F2`, `⌘K`) remains the other
way to act from inside a chat without leaving it.

**What only a person can confirm**, because a scenario cannot press Tab (above): that WebKit
walks this sequence in the real window with full keyboard access off; that `Ctrl+Tab` reaches
the page on macOS and on GTK rather than being taken by the window system first; and that the
focus ring is visible on each stop in both themes. `e2e/specs/keyboard-reach.e2e.ts` checks
what can be checked in the shipped bundle — the attributes, the order, the focused pane's
controls staying unseen until the keyboard is on them, and a dispatched Ctrl+Tab leaving xterm's real textarea.

## The three strips say their depth in shade, and that took no primitive either

ADR 0036 makes the window an axis — a project holds workspaces, a workspace holds chats —
and charter-app#171 drew that with three signals so the nesting would be legible before a word
was read: **height** (a project's row is the tallest), **inset** (each row began under its
parent's first tab) and **surface** (deep, raised, then the bar). charter-app#193 keeps
**height**, drops the **inset**, and turns **surface** into a quiet shade per strip in tokens of
its own — and drops the accent edge under the selected tab too, on the operator's reading of
the running app.

**The inset went because it could not be told apart from padding.** His words, unprompted:
_"workspaces tabs and sessions tabs have some padding from left, they should be like project
tabs without padding."_ That is the signal failing at the only test that matters — the person
it was drawn for read it as slop. It was not free either: 1.1rem off the workspace strip and
2.2rem off the chat strip is a tab's worth of room at ADR 0026's widths, taken from the two
strips that collapse first (`fits.ts`).

**What replaces it is a shade, and it is a theme token rather than a look.** The operator asked
for the layers to differ — _"lets make some different styles/collor for each layer"_ — and then
turned down coloured rules under each strip, the first answer to that, as not minimal enough:
_"i prefer to change little bit backgrounds of tabs and little lighter for selected tab, borders
are not feeling well."_ So each strip is one quiet step of neutral grey — `layer.project`,
`layer.workspace`, `layer.chat`, outermost deepest — and nothing is drawn under or between the
rows. Three properties of that answer are decisions rather than details:

- **It is a token, so a theme owns it.** `docs/design-system.md` has the group and the contrast
  floor each shade is held to in both built-in themes.
- **The selected tab is a step lighter, and that is the whole selection signal.** #171 also lit
  the selected tab's bottom edge in `accent.base`; it went with the coloured rules, on the same
  instruction. `layer.selected` is one token for all three strips, so the tab you are on reads
  the same way on every row.
- **No primitive, and no component.** This is four declarations in `App.css` and four values per
  theme file. A "strip" component parameterised by depth would be exactly the charter API in
  front of nothing that the rule at the top of this file refuses.

**And a tab's label is centred in its cell** — _"also lets make tabs labels center aligned"_ —
which is one `justify-content` on the rule that already says what a tab is. A chat tab's `×` is
outside the button, so a chat's name is centred in the room the `×` leaves rather than in the
whole cell: the same rule, not an exception to it.

`src/StripMarks.test.tsx` is the guard on both halves, and it is a stylesheet test on purpose —
jsdom computes no layout and no cascade, so what can be held there is the rule as written: each
strip names its own shade and draws no line, the three shades are three different tokens, no
strip's tab carries an edge, and there is no `--nested` left to indent anything with.

## The four regions added no primitive, which is the rule working

ADR 0038 split the window into four regions, and the whole layout came out of what was
already here — a fact worth recording, because "a layout change" is the sort of ticket a
component library gets added on.

- **`react-resizable-panels` draws all four**, as this file already said it would. Every
  boundary is the same `Separator` the pane splits use, so a region's handle and a split's
  handle behave the same way and are styled once.
- **Putting a region away is the library's `collapse()`/`expand()`, not a conditional
  `<Panel>`.** Taking a panel out of a live group throws from a document listener where no
  `try` can reach it — _"Panel constraints not found for index 3"_ — because a separator
  recalculates its aria values against a constraint list the panel has just left.
  `RegionFrame`'s `Slot` holds the reasoning. What the constraints are is written once and
  never changed; only the collapse moves.
- **The layout is data, and the panels are slots** (`app/src/regions.ts`). The frame renders the
  same four panels for the life of a window — left, centre, right, bottom — and an _arrangement_
  says which slot each region's content goes in, in what order, whether it is drawn and how big
  its slot starts. That is what makes the rule above survivable: a region can move side while
  the window is up, because moving it adds nothing to the group and takes nothing out. Adding a
  region is a line in the catalogue and a piece of content; no JSX moves. A slot's
  `minSize`/`maxSize` therefore belong to the **slot** and not to the regions in it — a bound
  derived from the current occupants would change the moment one moved, which is the second half
  of the same throw.
- **A slot that starts with nothing in it starts at `0%`, and that is how the flash was fixed.**
  charter-app#141 sized a hidden region normally and collapsed it from a `useEffect`, which runs
  after the browser has painted, so every launch drew it for one frame. The obvious repair —
  `useLayoutEffect` — **throws**, _"Group &lt;id&gt; not found"_: the group registers itself in
  its own layout effect, and React runs a child's layout effects before its parent's, so there
  is no hook inside a group that runs after the group exists. The first layout is made right
  instead; the library snaps `0%` to `collapsedSize`, and the effect is left to handle only what
  changes while the window is up.
- **jsdom never lays a group out.** Every element measures zero, so the library defers its
  layout and no `defaultSize` is ever applied — which means a unit test cannot read a panel's
  width out of the DOM. `RegionFrame.sizes.test.tsx` mocks the library to assert what it was
  _told_; `RegionFrame.test.tsx` and `FourRegions.test.tsx` use the real one for everything that
  is about the tree. A size is only really checked by the scenario tests.
- **The explorer's repo groups are `<details>`**, per the rule above: the browser has a
  collapsible, and `@radix-ui/react-collapsible` is not installed because nothing needs it.
- **Picking a spot in the explorer is `aria-current`, not `aria-selected`.** `aria-selected`
  belongs to the three tablists that are the axis (ADR 0036); this is the current item of a
  list. Radix has no tree or listbox primitive, and native buttons are not hand-rolled markup.
- **The region buttons are `aria-pressed` toggles**, which is what the platform has for a
  control that is on or off. They are **on the status line and icon-only** since charter-app
  #193, which the operator asked for twice — _"show hide buttons can be movet to bottom status
  bar — again like ZED"_, and then _"let make them without labels, just small icons without
  texts, texts only with tooltips"_. Three things about that are decisions:
  - **The name did not go with the words.** `aria-label` carries it, which is the one condition
    `docs/design-system.md` puts on an icon with no text beside it — and not a formality here:
    `pressOnly("Explorer")` is how the palette and the specs reach a control, and a screen
    reader reads the same string. An icon-only button whose accessible name is an icon is a
    button nobody can find, by either route.
  - **The `title` says what pressing does, not what the thing is.** _"Put the Explorer region
    away"_ is where prose belongs once there is no visible text; a tooltip repeating the label
    is a tooltip nobody reads twice.
  - **The status line hosts every region's way back and has none of its own**, which is not a
    contradiction of `StatusLine.tsx`'s argument for not being a region but the sharpest form
    of it: it is the frame. `FourRegions.test.tsx`'s _"cannot be put away, because it is not a
    region"_ is the guard, and it presses all three toggles to get there.
  - **A scenario reaches them by `[aria-label="Explorer"]` and no longer by `=Explorer`.**
    WebdriverIO's `=` is a whole-text match, and there is no text. `regions.e2e.ts`,
    `pane-fill.e2e.ts` and `status-line.e2e.ts` all moved; `StripMarks.test.tsx` is what holds
    the name they match against.

## The title bar added no primitive either, and inherited a rule from Tauri

The window's title bar (`app/src/TitleBar.tsx`) is a `<header>` holding the project strip — the
same Radix roving-focus tablist it was in its own row (ADR 0054) — and a `<span>` of buttons at
its right-hand end. The one thing about it that is not ordinary markup is the drag region, and that is a
**third-party rule the window now depends on** — the same class of thing as WebKit's tab
sequence above, so it is written down here for the same reason.

- **`titleBarStyle: "Overlay"` is macOS only.** Windows and WebKitGTK ignore the key in
  `tauri.conf.json` and keep drawing their own title bar above the webview. So charter's bar is
  the title bar on one platform and the window's first row on the other two, and the only thing
  that differs in the markup is how much leading padding it reserves for the system's window
  controls. That number comes from `title_bar_room`, which is `cfg!(target_os = "macos")` in
  Rust — the target the binary was built for, rather than a `navigator.userAgent` string's guess
  at it, and it is a command because one frontend bundle is built per target and nothing in the
  bundle is told which target it landed in.
- **`data-tauri-drag-region="deep"`, never the bare attribute.** Tauri injects a `mousedown`
  listener (`tauri/src/window/scripts/drag.js`) that walks the composed path up from what was
  pressed. The bare attribute drags only on a **direct** press of the element carrying it —
  which, on a bar whose content is text in spans, means the bar drags everywhere except on its
  own words. `deep` drags anywhere in the subtree.
- **A control inside a drag region needs nothing to stay a control.** That same walk returns
  `false` at the first _clickable_ element it meets, and clickable there means a `<button>`, an
  `<a>`, an `<input>`, a `contenteditable`, an interactive `role` — **or anything carrying a
  `tabindex` other than `-1`**. **`BUTTON` is in that tag list, so the tag alone does it** and no
  attribute of charter's is load-bearing for dragging. Worth writing down because it is easy to
  get backwards: the title bar's scenario was first written asserting `tabindex="0"` on every
  control and the real app refuted it — the update item's trigger carries none, for
  charter-app#189's reason, and it presses perfectly well. Nothing in charter's own code says
  "do not drag here".
- **`core:window:allow-start-dragging` is NOT in `core:default`.** The handler ends in
  `invoke('plugin:window|start_dragging')`, and without that permission named in
  `capabilities/default.json` the call is refused at runtime — a title bar that looks right and
  cannot be grabbed, with nothing on screen to say why. `internal_toggle_maximize`, the
  double-click, _is_ in the default set, so the double-click worked and the drag did not.
- **No test proves the window moves.** The rig performs no default action (above), and a
  synthetic `mousedown` that reached Tauri's listener would end in an IPC call rather than an
  observable drag. What is measured is the DOM the handler reads — the attribute on the bar, and
  every control in it being an element the walk stops at — in jsdom (`TitleBar.test.tsx`) and
  again in the shipped bundle (`e2e/specs/title-bar.e2e.ts`), which is `picker.e2e.ts`'s split
  applied to a second attribute. The permission is checked by nothing and is the sharpest thing
  to attack.

## Which keys belong to the chat, and which to the window

Every pane in this window is a terminal running somebody's shell, and a shell's line editor has
bindings of its own. So "the window takes this key" is never free: it is taken out of the
program the operator is typing into. `F2` cost that once (charter-app#47) and `Ctrl-K` cost it
again (charter-app#106), which is enough to write the rule down rather than decide it a third
time per key.

> **A chord a terminal encodes belongs to the chat whenever a chat has the keyboard, unless the
> window claims it there deliberately — and a key claimed from under a focused chat must have a
> way to hand it back.**

Three keys, three answers, one rule. It is `Palette.opensIt` and `Palette.theChatKeepsIt` that
implement it, in that order: the first says whether this is the palette's chord at all, the
second whether the chat keeps it anyway.

- **`F2` is claimed from under the chat, and hands it back.** A second `F2` closes the palette
  and sends `ESC O Q` to the chat in front, through the catalogue's own `pane.sendkey` row.
  That is tmux's `send-prefix`, and it is the only reason claiming the key is defensible.
- **`⌘K` is claimed everywhere, and takes nothing.** Measured in the pinned `@xterm/xterm`
  6.0.0: `evaluateKeyboardEvent` answers a `⌘`-chord with `SELECT_ALL` for `⌘A` and with no
  bytes at all for anything else, so a pane that received `⌘K` would send nothing. There is
  nothing to hand back and nothing was taken.
- **`Ctrl-K` is the chat's while the chat has the keyboard.** xterm maps `Ctrl` with a key code
  in 65..90 to `String.fromCharCode(code - 64)`, so `Ctrl-K` is `\x0b` — readline's
  kill-to-end-of-line. From anywhere else in the window it still opens the palette, because on
  Linux there is no `⌘` and dropping the chord outright would take the desktop's own key away
  on the platform that has only it.

**How the window knows.** A capture listener on the window runs before the focus has had any
say, so the only thing it can ask is where the keystroke was _delivered_: `e.target`. A pane
marks itself with `actions.CHAT_KEYBOARD` (`data-chat-keyboard`) and xterm's textarea is a
descendant of it, so `target.closest()` is the whole test. An attribute and not the `.pane`
class, because the class is how a pane is drawn and this is what it means.

**A scenario cannot carry this claim.** WebDriver does not deliver a modifier chord here —
`browser.keys(["Shift", "Tab"])` arrives with `shiftKey` unset, measured in #176 and recorded
above — so a key claim is evaluated in jsdom, where the real event can be dispatched and what
happened to it asserted. The missing modifier is one symptom of a larger one, measured in #186
and written up above: **this driver dispatches a synthetic DOM keydown and performs no default
action at all**, which is also why no key it sends presses a button and why Tab does not move
the focus even between two text boxes. `Palette.test.tsx`'s _"a chord the chat's own terminal would encode"_
is where the rule is pinned, including the half that matters most: the keystroke reaches the
document **unprevented**, which is what reaching the shell actually means.

**What is still eaten, and where.** The palette's window listener is the only place the
front-end claims a key, and after #106 it claims no readline binding. The application menu is
the other claimant. On macOS its accelerators are `⌘` chords, which xterm encodes as nothing, so
it takes nothing. Off macOS muda 0.19.3 gives the predefined items fixed `Ctrl` chords, every one
a key a shell owns: `Ctrl-C` (copy, against SIGINT), `Ctrl-A` (select all, against
beginning-of-line), `Ctrl-Z` (undo, against SUSP), `Ctrl-Y` (redo, against `yank`), `Ctrl-V`
(paste, against `quoted-insert`), `Ctrl-X` (cut, against readline's `C-x` prefix) and `Ctrl-H`
(hide, against backward-delete-char). So since charter-app#187 the menu off macOS is Quit alone,
on `Ctrl+Shift+Q`: a terminal app's own keys add `Shift` (GNOME Terminal, Konsole), and xterm
sends nothing for a `Ctrl+Shift` letter. `lifecycle::layout` is where that is decided, and its
tests hold every accelerator it produces off macOS to this rule.

**The text-size keys take nothing either** (charter-app#283, `textSize.sizeKey`). `⌘` on a Mac
and `Ctrl` elsewhere, with `=` or `+`, makes the text in focus bigger, with `-` smaller, with `0`
its default: a terminal pane's size inside a pane, the window's anywhere else. Measured in
xterm.js 6.0.0, `Ctrl` is encoded with a letter, space, `3`–`8`, `[`, `\` and `]` — not `=`, `-`
or `0` — so none of these is a shell's key. One modifier per platform, not both as `⌘K` has, so
a Mac's `Ctrl` chords stay the terminal's whatever a later xterm.js sends for them. The one neighbour a shell does own is `Ctrl+Shift+-`, whose `key` is `_` and which xterm
sends as `^_`, readline's `undo`: it is never matched, and reaches the shell unprevented. (A
real xterm sends `^_` for a plain `Ctrl+-` too; xterm.js does not, and it is the terminal in
every pane.) They are a capture listener on the window, like the palette's, and not menu
accelerators: which size a key changes depends on where the keystroke landed, which only the
page knows. Preferences… is on the menu, on `⌘,` and `Ctrl+,`; xterm sends nothing for either.

**The new-shell key takes nothing either** (SI-5, ADR 0062, `shellKey.opensAShell`): `⌘⇧T` on a
Mac and `Ctrl+Shift+T` elsewhere — "new tab" in GNOME Terminal, Konsole and Windows Terminal,
whose tabs are shells. xterm.js 6.0.0 encodes `Ctrl` with a letter only when Shift is not held,
so `Ctrl+Shift+T` is no byte, and plain `Ctrl+T` still reaches the shell as transpose-chars.
It is a capture listener on the window, held by the project in front, and it presses the
catalogue's own `shell.new` row.

**Find in a pane is `⌘F` on a Mac and `Ctrl+Shift+F` elsewhere, and takes nothing either** (SI-4,
`SessionPane.opensFind`). xterm.js 6.0.0 sends nothing for `⌘F`. It would send `\x06` for
`Ctrl+F` — readline's forward-char — so off a Mac the chord adds `Shift`, as GNOME Terminal and
Konsole do for their own find, and a Mac's `Ctrl+F` stays the shell's as well. It is caught by the
pane's own terminal (`attachCustomKeyEventHandler`) rather than on the window, because find is
over the pane that has the keyboard and nowhere else. Neither the palette (`⌘K`, `F2`) nor the
native menu claims `F`: the macOS menu is charter's, Edit's predefined items and nothing else.

**Shift+Enter in a harness's pane is the harness's newline** (SI-4, `Harness::newline`). A
terminal has no Shift+Enter: xterm.js 6.0.0 sends a bare CR for it, the byte Enter sends, so every
harness submitted on it. The pane sends the harness's own newline instead — ESC CR for all three,
measured — and a shell's pane, which runs no harness, keeps xterm's Enter.
