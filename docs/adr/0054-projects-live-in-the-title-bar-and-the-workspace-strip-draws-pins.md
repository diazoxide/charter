# Projects live in the title bar, and the workspace strip draws what is pinned

The window carries three tab rows above the panes — projects, workspaces, chats — under a title
bar that says `{project} / {workspace} / {N sessions running}`. The operator settled on
2026-09-25 how that should look for someone who works in **several projects in parallel and
about three workspaces in each**, with one constraint stated up front: **hiding a workspace must
never hide a chat that needs you.**

- **The project strip moves into the title bar**, and the breadcrumb goes.
- **The workspace strip draws the pinned workspaces and the one you are in** — nothing else. The
  rest are behind its show-more menu.
- **A show-more button carries the needs-you count of everything it hides.**
- **The chat strip does not change.**

This amends [ADR 0039](0039-tabs-keep-their-order-and-the-overflow-sorts-by-activity.md) (and its
2026-09-22 amendment) for the workspace strip, and replaces the operator's breadcrumb spec quoted
in `app/src/TitleBar.tsx`.

## The project strip moves into the title bar

**The bar already is the window's row, and projects are the window's strip.** `TitleBar.tsx`
says why About, updates and the needs-you queue are up there and not on the status line: they are
about the app, not one project. The project strip is the same kind of thing — `App.tsx` holds it
for the window, not for a `PlaneView` — so it goes in the same row, and the row it used to occupy
under the bar is given back to the panes.

The bar becomes, left to right:

```
● ● ●  [charter ²] [volaticloud] [▾ 3 more ¹] [+]  ···drag···  ✋2  About  ⟳
```

- **The project tabs keep every rule they have now**: fixed order, pinned first, equal shares
  floored at a minimum, the selected project always drawn, the rest in a show-more menu sorted
  by activity, and the `+` never collapsed (ADR 0039's measured constraint). Only their row
  changes.
- **The right-hand end still never gives way**, and the tabs give way before it does. Width the
  tabs cannot use is drag region; **a minimum stretch of it is reserved** so a window full of
  projects can still be grabbed. Every tab is a `<button>`, which Tauri's `deep` drag handler
  already stops at, so the tabs need no drag rule of their own.
- **On Windows and Linux it is the first row under the system's bar**, exactly as the bar is
  today. Nothing is gated per platform, for the reason `TitleBar.tsx` gives.

**The breadcrumb goes, rather than squeezing in beside the tabs.** The project tab in front says
which project; the workspace strip says which workspace. A crumb repeating both spends title-bar
width that the project tabs need. **`N sessions running` moves to the status line**, which is
per-project and is where the operator reads the project in front.

Rejected: **a dropdown switcher** in place of the crumb (`charter ▾ / ide`). It is the tidiest
bar, but switching projects takes two presses, and the other projects' needs-you counts are only
visible inside a menu. That loses exactly what parallel work across projects needs.

## The workspace strip draws pinned workspaces and the one you are in

**An operator with many workspaces works in about three.** Filling the width with whatever fits
puts workspaces nobody is working in next to the three that matter, and the pin that was meant
to say "these three" only orders them. So the strip draws:

1. **every pinned workspace**, in pin order, and
2. **the workspace in front, if it is not pinned**, after them, and gone again once you leave it.
   This is ADR 0039's "the selected tab is always drawn", and it is what keeps "where am I"
   visible without a breadcrumb.

**Everything else is in the show-more menu**, which lists all of them, sorted by activity, as it
does now. If the pinned ones alone do not fit the width, the existing collapse still applies to
them, so the strip never scrolls and never loses its `+`.

**A new workspace starts pinned** when it is created from the app, because you just made it in
order to work in it. **An existing plane is pinned once, on first open after this ships**: the
three most recently active workspaces (`last_active`, which `charter-core/src/briefing.rs`
already computes), **and only if nothing is pinned there yet**: a plane the operator has
already arranged is left as it is. The fact that the migration ran is recorded in the machine
store beside the pins (ADR 0040), so an operator who later unpins everything is not re-pinned.
With no pins at all, the strip draws the current workspace and the show-more button.

Rejected:

- **Pinned only, with no slot for the current workspace.** Opening an unpinned workspace from the
  menu would leave no tab saying where you are.
- **Keep width-fill.** It is the status quo, and it is the noise this record removes.
- **Auto-pinning by activity.** A strip that pins and unpins on its own is the moving target
  ADR 0039 refused for tabs.

## A show-more button carries what it hides

**This is the constraint the operator stated, and it is met on three surfaces, none new:**

1. **The show-more button shows the sum of the hidden items' needs-you counts** in the same red as
   a tab's count. It is on the project strip's button and the workspace strip's button, and it is
   read from the same queue as the tab counts, so it goes down when they do.
2. **In the show-more menu, rows that need you are sorted first**, then by activity. This is still
   a list you read, not a surface you aim at, so the ordering is ADR 0039's own rule.
3. **The title bar's ✋ needs-you menu stays the one list of every chat, in every project**, asking
   for you (charter-app#249). It is unchanged.

**Nothing is promoted onto a strip because it needs you.** A hidden workspace that jumped onto
the strip while it waited would move tabs under the operator's hand. That is the one thing
ADR 0039 exists to refuse, and the badge and the ✋ menu already say it.

## The chat strip does not change

The chats in a workspace are what the operator is working through, so hiding the unpinned ones
would hide their work. The strip keeps ADR 0039 as amended: fixed order, pinned first,
width-fill, show-more by activity. **This is where the three strips stop behaving alike**, which
the 2026-09-22 amendment said they would. The difference is deliberate: projects and workspaces
are places you go, and chats are the work in front of you.

## Consequences

- **One tab row fewer above the panes** on every platform.
- **`TitleBar.tsx`'s operator spec is superseded**, and its doc comment must say so. The tests
  that read the breadcrumb (`TitleBar.test.tsx`, and any scenario that reads the crumb) change
  to read the project tab and the status line instead.
- **A workspace strip that looks the same across launches is now a machine-store fact**, since
  it is drawn from pins. A machine that has not run the migration yet shows only the current
  workspace until it has.
- **The show-more button has become an attention surface as well as an overflow.** A defect that
  hides its count now hides a chat that needs you. So its count gets a test in its own right, not
  just as a side effect of the tab counts.

## Amendment, 2026-09-26: the project menu sorts by activity, and the move count is one per process

The project strip's show-more menu listed its rows in the strip's order after the ones that need
you, because the window had no activity signal it could compare across projects (charter#401).
Each project has its own board in the core, and `Board::moved_at` was a count of moves **on that
board**. So a project that had moved fifty times an hour ago read higher than one that had moved
once just now.

**The count is now one per process, shared by every board** (`MOVES` in
`crates/charter-core/src/state.rs`). Within a plane it orders chats exactly as before. Across
planes it now orders them too, so the window derives a project's last activity as the newest
`movedAt` among its chats (`PlaneReport.moved`) and sorts the menu by it. No field was added.
The count still rides `chat-moved` and the first snapshot.

**The cost, stated:** the count lives only as long as the app, and reopening a chat is a move. So
straight after a relaunch the projects are ranked by the order their chats were put back in, until
one of them does something. The chat and workspace menus already pay this within a project. A
persisted signal was not added for it: nothing in the plane records when a chat last moved, and
adding that record would be a new fact about the plane, not a sort order.
