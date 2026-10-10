# The window is four regions, and what has no region is named

[ADR 0036](0036-the-workspace-is-an-axis-again-projects-workspaces-chats.md) gave the workspace
its axis back and left a question it had to open: **if the workspace strip is the axis, what is
the left sidebar for?** The sidebar was listing every workspace with its full vision text — the
same axis the strip above it had just been given, drawn twice, one of them a tablist that ADR 0036
had to demote in the same breath. The operator asked the question directly. This record is his
answer, taken on 2026-09-21.

**Four regions, and each one holds a kind of thing rather than a list of features.**

| region | holds |
| --- | --- |
| **Left sidebar** | a repo / worktree **explorer** — the selector *within* a workspace, because one workspace holds several repos and several worktrees |
| **Right sidebar** | personas, todos, the **needs-you queue**, alerts |
| **Bottom bar** | repo git state, worktrees, pipelines (CI) |
| **Centre** | the terminal panes |

## The reading, which is this record's and not his words

The operator specified the table. He did not give a rule for it, and one is offered here so the
next surface has somewhere to go rather than landing wherever there is room: **the left is
navigation, the bottom is state.** The left is where you go to change what you are looking at;
the bottom is where you read what is true and do not touch it. The right is neither — it is what
is asking for you, which is why the needs-you queue and the alerts sit together there.

**This is an interpretation and it is marked as one. If it is wrong, the table stands and the rule
is corrected.** A rule inferred from four rows is exactly the kind of thing that hardens into a
decision nobody made, which is how ADR 0036 and
[ADR 0029](0029-the-pane-footer-is-blanked-by-default-and-a-chat-may-keep-it.md) each came to be
written after the fact. Writing it down as inference is the only way it can be argued with.

The reading does have a visible seam, and naming it is better than hiding it: **worktrees appear
in two regions.** On the left they are what you select; on the bottom they are what git says
about them. Under "left navigates, bottom reports" that is consistent. Under any other reading it
is a duplication of exactly the kind this record exists to remove, and it is the first thing to
re-examine if the rule turns out to be wrong.

## What the left sidebar was, and why it had to change

Read off `app/src/Sidebar.tsx` on 2026-09-21: it renders every workspace as a button, each with
`<p className="vision">{ws.vision}</p>` and every chat under it. Its own doc comment already
argues the boundary — *"This is a listing, not the axis"* — and ADR 0036 already made it stop
being a `role="tablist"`. That was the correct minimum at the time and it does not go far enough:
a listing of the axis, under the axis, with the long-form vision text of every workspace in it,
is still the strip's question answered a second way in more words.

**The workspace listing is not what the left is for.** What the left gets instead is the level
ADR 0036's three strips do not reach. Projects, workspaces and chats are three tablists; a
workspace holds **several repos and several worktrees**, and nothing in the window selects one.
The plane already knows them — `workspace_repos` and `worktree_list` are Tauri commands today
and `Panels.tsx` draws their git state read-only, on the right. The explorer is the selector
those facts have never had.

## The right-hand side exists, and this decision splits it

There already is a right-hand region, and it would be wrong to write this record as though four
new regions were being invented. `app/src/PlaneView.tsx` renders `<div className="body">` as
`Sidebar` · `panes` · `Panels`, and `Panels.tsx` is `<aside className="panels"
aria-label="Workspace">` with four sections in it: **Repos, CI, Todos, Personas**. Its own
comment calls it "the right-hand side".

So what the table does to the right is **split it along the reading**. Repos and CI are state —
they go to the bottom bar, with worktrees beside them. Todos and personas are what is asking for
you, or nearly — they stay, and the needs-you queue and alerts join them. **No region in the
table is empty of code today; two of the four are re-tenanted and one is new.**

## What the right sidebar already holds, and what it does not

The needs-you queue is further along than a gap list would suggest, and this record would be
wrong to describe it as absent. `app/src/NeedsYou.tsx` is a real component: it lists every chat
that asked, by name, each a button that brings that chat forward; it names the chats that *can*
be waiting without saying so (`quietOnes`, purlis#52); and its two empty states are
different claims, deliberately — "Nothing needs you" when every open chat can report, and
"Nothing has said it needs you" when one cannot. `App.tsx` carries the count per project tab and
`PlaneView.tsx` carries it per workspace tab (ADR 0036).

What is true is **where it is**: `<NeedsYou>` is rendered inside `<header className="bar">`, in a
row with the tab strip, the `+`, the split buttons and the plane path. A queue that has to work
at fifty chats is sharing a line with six other things. Moving it to the right sidebar is this
decision; building it is not, because most of it exists.

**Alerts are the opposite case: the surface is assigned and there is nothing to draw.** purlis's
footer has an alert row in zone 2 (`charter/statusline.py:_alerts`), and it is not ported —
`crates/purlis-core/src/footer.rs` names the omission in the output rather than hiding it:

```
not drawn by this build: repos · personas · alerts · session
```

That line is there on purpose, and its own doc comment gives the reason this record inherits:
*"a footer that silently omitted the alert row would be worse than a sentence, because an
operator reads a footer to find out whether anything needs them, and one that can only ever say
'nothing' is a footer that lies once a week."* **A right sidebar that has an alerts area and no
alert source tells the same lie.** Whoever draws the region draws the sentence until the port
exists.

## What has no region, recorded as open

These are named as gaps, not scheduled as work. Each one is a fact purlis already has, or
already computes, with nowhere in the window to be. **None of them is decided by this record**,
and the reason they are in it is that a four-region table is exactly the document a later reader
will use to conclude that anything not in the table was considered and dropped.

- **The `ctx` and `cache` gauges have no home, and the history they need is being written.** The
  gauges are zone 3 of purlis's own footer. [ADR 0019](https://github.com/diazoxide/charter-plane/blob/0ae0961d8a6a8e59b48ba43b10d28de8fd87afb7/docs/adr/0019-the-frame-owns-the-surface.md)
  recorded the gap for the tmux frame — *"A framed Claude Code session has no context/cache gauge
  on any surface"* — and **that bullet is marked closed by #413**: the frame's top strip draws
  `statusline.recorded_context_gauge` from the recorded history, and `statusline.main` writes the
  harness-session mapping because it is the one process that sees both ids. **purlis has no
  equivalent closure.** ADR 0029 states the position for the app in as many words: *"charter's
  Rust footer does not draw them yet either."* `footer.rs` draws zone 1 and the sentence above.

  The recording side is alive and the drawing side does not exist. `usage::record` is called from
  `crates/purlis-cli/src/statusline.rs`, which is the side effect ADR 0019 exists to protect —
  so the history is accumulating. What is *not* ported is a renderer: `usage::rows_at` is a
  private helper of the writer, called only by `record_turn` to rewrite the file it keeps, and
  it has no caller outside its own module. The app exposes no command for any of it.

  And it is **per-chat** information in a window with fifty chats, which is why it fits no region
  in the table: a gauge in a sidebar describes the focused workspace, and `ctx` describes one
  conversation. The chat's own tab and the pane's corner are the two places suggested. **The
  operator has not ruled, and this record does not rule for him.**
- **Alerts.** Assigned to the right sidebar above; nothing to draw, per the section above.
- **Usage and the token trend.** Same history, a different view of it — the trend over a
  session's turns rather than this turn's percentage. Zone 3. Nothing renders it and, as above,
  no renderer was ported.
- **News, and "an update is available".** `crates/purlis-core/src/news.rs` is ported and
  `purlis news` works. The app has no command for it, so an operator who never types `purlis`
  in a pane is never told an update exists.
- **`doctor`.** `crates/purlis-core/src/doctor/` is ported across twelve modules and is CLI
  only. It is the thing an operator reaches for when something is wrong, and in a window whose
  whole premise is not typing `purlis`, it is reachable only by typing `purlis`.

The app's full command surface was read to check this: thirty-six `#[tauri::command]`
functions, none of them `news`, `doctor`, `usage`, `alerts` or `footer`.

## What was rejected

- **Keep the workspace listing in the left sidebar and add the explorer below it.** The cheapest
  change, and it keeps the duplication that caused the question. The sidebar would then answer
  "which workspace" (already answered by the strip) above "which repo" (answered nowhere), and
  the answered one is the one with the long text.
- **Leave the repos on the right and make that list the selector.** The smallest change of all:
  `Panels`'s Repos section is already there, already read-only, already per-workspace. It also
  collapses the seam named above, since the git state would stay beside what it describes.
  Rejected because it makes the right sidebar both the thing that asks for you and the thing you
  navigate with, and the needs-you queue is the one surface in this window that must never be
  competed with.
- **A single collapsible sidebar with panels, Zed-style.** The reference is Zed and Zed does
  roughly this. Rejected for now because it is a different decision — it is about how regions are
  arranged, and there is no agreement yet on what goes in them, which is what this record is.
- **Decide homes for the five gaps now.** Rejected by the operator not ruling. Recording them as
  named gaps is the alternative to either inventing a placement or letting the absence read as an
  intention.

## Consequences, including the ones that cost something

- **One region is new and two are re-tenanted.** There is no bottom bar; the right-hand `Panels`
  aside gives up Repos and CI to it and takes the queue and alerts in exchange; the left sidebar
  changes contents entirely. This is a layout change across `PlaneView.tsx`, and every scenario
  spec in `app/e2e/specs/` finds its elements by role and label inside that layout —
  `panels.e2e.ts` and `sidebar.e2e.ts` most of all.
- **The left sidebar loses the only place every workspace's chats can be seen at once.** ADR 0036
  gave it that job explicitly — *"the sidebar is the listing, and the only place the operator can
  see every workspace's chats at once"* — and re-purposing the region takes it away. The palette
  lists every chat in the project with a search over it (purlis#48), and the workspace strip
  carries the per-workspace counts and the needs-you marks. **That is the replacement, and it is
  a genuine loss of the at-a-glance view, not an equivalent.** If it is missed, the honest fix is
  a view of its own, not the vision text coming back.
- **Worktrees are drawn in two regions.** Named above. Consistent under this record's reading and
  a duplication under any other.
- **Four regions plus three tablists is a dense window, and the density is the product.**
  ADR 0026 measured fifty sessions and twenty panes on screen; ADR 0019 budgets its own strips in
  columns. A region that cannot degrade when the window is narrow will be the first thing that
  breaks, and nothing here says how any of them degrade.
- **The five gaps are now a list somebody can close.** That is the point of naming them. It is
  also a list that will be read as a backlog, and it is not one: three of the five (`ctx`/`cache`,
  usage, alerts) need a renderer written before any region can hold them.

## Amendment, 2026-10-07: the left region also lists the project's chats

The operator's ruling of 2026-10-07, in the session that decided how personas run as chats
(#1434, decision 15): **the left region gains a Chats section, above the explorer.** It lists
every running chat of the project, in every workspace, as a tree of which chat started which.
Each row has the persona's mark, the chat's name, its workspace, its state and a needs-you mark.

This gives back what the consequences above record as lost: the one place every workspace's
chats are seen at once. It does not bring back what this record removed. The section lists
chats and not workspaces, it carries no vision text, and it sits beside the explorer and does
not replace it.

It fits the reading above. **The left is navigation**: a row is a way to a chat, and pressing
it brings that chat's tab forward on whichever workspace's strip it is on. A chat started as a
task has no tab until its row is pressed, so six helpers add six rows and no tabs.

The explorer keeps its own axis, the place. It still draws each chat where it works. A chat
that started one in another workspace says so under its row, with a badge naming that
workspace, because nothing else in this workspace's explorer would.

## Amendment, 2026-10-10: the Chats section follows the focused workspace

The operator reported on 2026-10-10 that the Chats section lists every chat of the project
when he wanted only the focused workspace's (#1655). His report replaces the 2026-10-07
default above. **The section lists the trees that started in the workspace in the strip.** A
tree belongs to where its top row works: a chat the person opened, or a handoff, since the work
moved there. A task stays under the chat that asked for it wherever it works, so no tree is cut
in two. A chat started at the plane root is listed in the plane root's view.

The place where every workspace's chats are seen at once is still there, one press away: the
**all workspaces** chip beside _needs you_ and _working_. It lasts for the window's run and
is not saved. A row asked for in another workspace, such as the explorer's line for tasks from
other places, turns the chip on so that row can be shown. **No chat that needs the person is
left out silently.** The line under the filter says how many in other workspaces need you,
with a button that goes to the first one, as it does for the ones a filter hides. The title
bar's needs-you queue still lists every chat.

## Amendment, 2026-10-10: each side has an activity bar, and shows one view at a time

The operator's decision of 2026-10-10 (spec #1671, B-1, B-2, B-12 and B-13; built in #1673):
**each side gets an activity bar, a strip of icons at the window's edge that switches the side
between views, one view at a time.** It reverses, for this shape, the rejection above of _"a
single collapsible sidebar with panels, Zed-style"_: there is now agreement on what goes in each
side, which is what that rejection waited for.

- **The left side is navigation, as the reading above has it.** Its region is called
  _Navigation_ now, and holds its views: **Chats** (the chats and their tasks, as the
  2026-10-07 and first 2026-10-10 amendments describe them) and **Explorer** (the workspace, its
  repos and branches, and their files). Chats is open by default. **Search** (the files'
  content, ⌘⇧F) and **Changes** (⌃⇧G) joined it in #1676.
- **The bottom region is gone** (B-7, #1676). What it drew, each repo's branch, uncommitted
  files, branches and pipeline, is the Changes view, unchanged and still with nothing to press,
  so "the bottom is state" above now reads "the Changes view is state". The terminals have the
  window's whole height. Its icon counts the files git has uncommitted, in the plain count's
  colours since nothing is asking. A layout file that still places the `bottom` region is read
  past without a word. A bottom panel may come back one day for output only; it would be a new
  decision.
- **The right side stays the "for you" side**: Todos, Memory, Personas, Sessions, Vaults and the
  extensions' panels, each a view there (#1678). What it holds does not change, only that it
  shows one at a time. Memory is open by default. An extension's panel is a view of its own,
  after purlis's on the bar, so approving one never moves an icon the person already knows; the
  region still says nothing an extension declares decides where it goes (ADR 0043).
- **Each thing lives in one view.** The explorer no longer draws a chat where it works: the
  Chats view is the one place a chat is listed. The explorer's own axis is still the place, and
  the paragraph above that says it draws each chat where it works is replaced by this one.
- **Pressing the open view's icon puts the side away**, and pressing any icon brings it back on
  that view. The bar stays at the edge while the side is away, with its badges: the Chats icon
  counts the chats that need the person, in the needs-you colours, and the Todos icon the
  focused workspace's open todos, in the plain count's. Putting a side away hides its
  views and never unmounts them, so a view keeps its folds, scroll and filter.
- **Keys and the palette.** ⌘B puts the left side away and brings it back and ⌥⌘B the right
  (each names its region, as VS Code's ⌘B follows its primary side bar), ⌘⇧E shows Explorer,
  ⌘⇧C shows Chats and ⌘⇧F shows Search (Ctrl, and Ctrl+Alt+B, on other platforms), and ⌃⇧G
  shows Changes on every platform; every view is a row in the palette (Search's is _Search in
  files_).
- **Remembered per project, on this machine**, in the layout file (version 2): which view each
  side shows, its width and whether it is away. The needs-you queue stays in the title bar,
  where nothing competes with it.

The status line's toggles stay as each region's way back. The bar is a tab list built on the
window's own roving focus, holding tabs and nothing else (#1204); `docs/ui-primitives.md` has the
detail.

## Amendment, 2026-10-10: three scopes in place of the chip

The operator accepted B-15 of the sidebar grill on 2026-10-10 (#1671, #1679): the Chats view
switches between **This tab**, **Workspace** and **All**. The switch replaces the **all
workspaces** chip of the amendment above; there is one control, not two. It sits at the end of
the view's title line, a radio group drawn as one row of segments, one Tab stop with the arrows
between its three.

- **This tab** lists the chats of the tab in front, the set its chip counts and its menu lists
  (`tabChats.ts`), nested as the whole list nests them. A task in a tab of its own heads its
  tab's list.
- **Workspace**, the default, is the amendment above: the trees that started in the focused
  workspace.
- **All** lists every workspace's chats.

The pick is **kept for each project on this machine**, so it is there after a relaunch. It is
how one person looks at one project, like the folds, and is kept in the window's web storage
(local storage, where the folds use the session's, since a scope names no chat and outlives a
launch) rather than in `layout.json`, which has no per-project place for a view's state until
the views land in its next version (#1673). A row asked for outside the scope widens the scope
to the narrowest that lists it, and that is not kept, because the person did not pick it.

**In every scope, a chat the scope leaves out that needs the person is named at the top**: the
line under the filter says its name, how many more there are, and where they are ("outside this
tab", "in another workspace"), with a Go button to the one that has waited longest. A chat the
filter hides that needs the person is named the same way, first, and Go goes to it. The title
bar's needs-you queue still lists every chat.

## Amendment, 2026-10-10: Explorer is in sections

The operator's decision of 2026-10-10 (spec #1671, B-12; built in #1677): **Explorer is in
three sections, each folding on its heading**, as an editor's explorer is.

- **Workspaces**: every workspace the strip can bring forward, in its order, with the focused
  one marked as the current item. Pressing one focuses it exactly as its tab on the strip does,
  through the same catalogue row, and its menu is the tab's. This is not a second axis: the rows
  are not a tab list and select nothing, and the strip stays the axis (ADR 0036). It is the way
  to a workspace the strip is not drawing, one press away in the side that navigates.
- **Repos and branches**: the focused workspace, its clones and their branches. A branch is one
  row; picking it still decides where the next chat starts.
- **Files**: the files of where the next chat starts. The picked branch's or repo's files are its
  first level; with the workspace itself picked, each repo's own folder is a row that opens. A
  branch's files were drawn under its row until now; each is drawn once, here.

Which sections are folded is kept in the layout file on this machine. A focused branch is still
its cockpit, which draws in place of the three sections.

## Amendment, 2026-10-11: the Inbox heads the right side

The operator's decision of 2026-10-10 (spec #1688, I-2 and I-4; built in #1692): **the Inbox is
the right side's first view**, above Todos. It lists what waits on the person in the project, the
asks of the registry (#1690), grouped by chat with the chat waiting longest first, and answers
each in place through the path its source's Notice answers with. Its icon is the title bar's
hand, and its count is the asks', in the needs-you colours, on the bar while the side is away
too. Memory stays the view the side opens on.

**The title bar's ✋ opens the Inbox**, of the project in front where something waits there, and
otherwise of the first project along the strip with something waiting. The ✋ is no longer the
queue's only place: its count is the Inbox's, and its list is opened now only where something
asks for it (the away summary's dispatches refused while nobody was there), until #1693 and
#1695 move those into the Inbox and retire the list. ⌘⇧I (Ctrl+Shift+I elsewhere) shows the
Inbox, and so does its palette row.

**Only an ask raises a system notification** (I-7, built in #1694): an update never does. One
chat's asks a few seconds apart share one notification, titled by its chain and saying the kind of
ask, never its words. None is sent while the window holding the project is focused with that
project in front and its Inbox open, or with the chat itself on screen. A click on one brings the
window forward, and the Inbox opens at that chat's group.
