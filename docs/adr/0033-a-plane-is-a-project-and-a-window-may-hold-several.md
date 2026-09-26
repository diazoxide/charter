# A plane is a project, and a window may hold several

The operator double-clicked charter-app and got an app that could not be used. A `.app`
launched from the Finder inherits `/` as its working directory; charter-app resolves its plane
from that directory and from nothing else, so it resolved none, drew `No plane: …` and offered
no way to give it one. The app is usable only when it is launched from a terminal standing
inside a plane — which is to say, only by someone who did not need a desktop app.

That is not a bug in a line of code. **ADR 0025 assumed the plane was given.** The spec it
produced (`docs/superpowers/specs/2026-09-17-charter-app.md`) opens with "one window", lists
workspaces, chats, panels and a palette, and never says where the plane comes from, because in
the tmux frame it came from the shell that ran `charter`. A desktop app has no shell. This
record and the two beside it — [ADR 0034](0034-charter-keeps-a-little-state-outside-every-plane.md)
and [ADR 0035](0035-a-plane-is-untrusted-until-the-operator-opens-it.md) — are the decisions the
operator took on 2026-09-20 to close that hole.

## What the code does today, measured

- `app/src-tauri/src/lib.rs:plane_root` is the command the UI asks: `std::env::current_dir()`,
  then `charter_core::plane::resolve`. Nothing else feeds it.
- `setup` resolves a second time — `std::env::current_dir().ok().and_then(|cwd|
  charter_core::plane::find_root(&cwd).ok())` — and stores the answer as `Plane(Option<PathBuf>)`.
- `app/src/App.tsx` holds `{ state: "loading" } | { state: "found"; root } | { state: "missing";
  reason }` and renders `No plane: {reason}` for the third. There is no fourth state and no
  action on the third.
- The single-instance plugin is already there and already throws the second launch away:
  `.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| lifecycle::show(app)))`. The
  arguments and the working directory of the second launch are bound to `_` and dropped.

So charter-app has one plane per process, decided once, from one directory, at startup.

## The decision

**A project is a plane.** The top-level switcher is a plane switcher, and "Open Project…" opens
a directory that has, or will get, a `charter.toml`.

There is no second container. charter already has exactly one thing that holds personas,
workspaces, todos, memory and vaults, and [ADR 0007](https://github.com/diazoxide/charter-plane/blob/0ae0961d8a6a8e59b48ba43b10d28de8fd87afb7/docs/adr/0007-one-plane-shape.md) deleted the
*second plane shape* precisely so that no function would ever again have to ask which shape it
was in. A "project" that was not a plane would be the embedded shape returning under a friendlier
name, with every one of those forks reopened. The word changes and nothing else does: what the
operator calls a project on the File menu is the directory charter calls a plane everywhere
else, and `CONTEXT.md` now says so, because `Workspace` has carried `_Avoid_: project` since the
glossary was written and that line is now load-bearing in a way it was not.

**One window per plane, and a window may hold several.** A window holds planes as top-level
tabs; planes merge into one window and split back out of it. The operator named Zed's project
tabs as the shape he wants, and the reason is the reason Zed has it: eight planes is eight
things to arrange, and the thing an operating system gives you to arrange is a window.

**The single-instance plugin stays as it is — one process, many windows.** What changes is the
closure, not the plugin: a second launch hands its plane to the process already running, which
raises the window holding that plane or opens one for it. The comment already in the file is the
whole argument, and it was right before this decision was taken: a second process would be *a
second set of sessions on the same plane*. It would also be a second writer of that plane's
`.charter/app/reopen.json`, a second hook socket, a second tray icon, and a Quit that ends some
of the operator's work and not the rest. ADR 0025's "no daemons — every session is a child of the
app" makes one process the only shape in which "quit" means anything.

**Cold launch restores the window set from the last quit** — the same planes, the same tabs, the
same merges. Each plane's own `.charter/app/reopen.json` still restores that plane's chats, and
that record does not change. The two are deliberately separate and their scopes are the reason:
*which planes were open and how they were arranged* is a fact about this machine, which no single
plane can hold; *which chats a plane had* is a fact about the plane, which travels with it. Where
the machine-level record lives and why charter is allowed one at all is ADR 0034.

**A plane in that record that has moved or is gone is dropped with a line saying so**, never an
error dialog. This is the rule `put_back` already follows for a chat whose profile has gone —
skipped by name, still recorded — and it is the same reason: a restore is a convenience, and a
convenience that blocks the launch is worse than the thing it was restoring. **`--no-restore`
starts clean**, for the launch after the one that restored something the operator did not want.

## What this costs, measured before it was decided

One plane per process is not a UI assumption. It is the state model:

```
app.manage(hooks);
app.manage(chats);
app.manage(Plane(plane));
```

`Plane` is `struct Plane(Option<PathBuf>)` — one optional path for the whole process. Thirteen
commands take `tauri::State<'_, Chats>` and two take `tauri::State<'_, Hooks>`; every one of them
is written as though there were one answer to "which plane". `Chats` numbers its sessions in a
single space. The hook channel is opened once per process for that one plane
(`hooks::socket_for(plane.as_deref())`), and it is named in the environment of every session the
app starts. The exit handler writes the reopen record for that one plane and no other.

So the work is not "add a tab bar". It is that the window has to carry the plane, the managed
state has to be keyed by it, and each of those fifteen commands has to be told which one it is
acting on — and until that is true, a second plane in a window is a second plane writing through
state that belongs to the first. That is the expensive half of this decision and it is stated
here so that nobody costs it as a UI change.

Two consequences follow immediately and are not pleasant. A window holding several planes holds
several hook sockets and several reopen records open at once, so "the app's socket" stops being a
phrase that means anything. And `Sidebar.root` — a single `String` today — becomes a per-plane
field, which is a change to the generated bindings and therefore to the UI.

## What was rejected

- **One process per plane.** The cheapest change by far: the state model above stays exactly as it
  is. It fails on ADR 0025's own rule. Every session is a child of the app, so N planes is N trays,
  N quit warnings, N sockets, and an operator who quits charter and finds charter still running.
- **One window per plane with no merging.** Half the feature, and the half the operator did not
  ask for. He asked for Zed's arrangement by name, and an operator with eight planes wants them
  arranged rather than scattered across eight windows the OS stacks for him.
- **A plane switcher as a filter inside one window.** It keeps the singleton state model, which is
  why it is tempting. It also means the operating system can never put two planes side by side,
  which is the one thing a window manager is for.
- **Resolving a plane again whenever the working directory changes.** Rejected in ADR 0034, where
  the resolution rule is argued in full: a window that has a plane has an explicit one.

## Consequences

- The app becomes launchable from an icon, which is what makes it a desktop app rather than a
  terminal program with a window. M1 was declared a daily driver against a definition that
  assumed a terminal launch; that is not reopened, but it is worth saying that the bar moved
  under it rather than pretending it did not.
- `charter`, the CLI, is untouched. It resolves its plane the way it always has, from the
  directory it was run in and `$CHARTER_ROOT`. Nothing in this record reaches it.
- Two records now describe what to reopen, at two scopes. A reader who finds only one of them
  will draw the wrong conclusion about the other, which is why both are named here and in ADR
  0034.
- The window set is machine-level, so it does not travel with a plane, and two machines that
  share every plane still arrange them differently. Intended: an arrangement is a fact about a
  desk.

## Amendment, 2026-09-26: splitting a project into a window of its own, and closing that window

charter#126 built the half of "planes merge into one window and split back out of it" that was
still missing. The state model was already keyed by window (`Showing`); what hard-coded one
window was the notification check, the tray's and the quit's window, the capability grant (ADR
0052) and the restore, which merged every remembered window into one. This amendment records
the decisions that record did not settle.

- **Windows.** The main window is the one `tauri.conf.json` declares. A project moved out of it
  goes into a **split window**, labelled `window-<n>`, which runs the same page with its own
  palette, title bar and `F2`. It is moved by a row on the project tab's menu and in the palette
  (*Move project X to a new window*), and moved back by *Move project X to the main window* in
  the split window. A window holding one project does not offer to split it: the row is there,
  greyed, saying why.
- **One project is held by one window.** Moving a project takes it out of one window and puts
  it in the other in one step (`Showing::move_into`), and nothing is closed or started: its chats
  go on running in the core, and the window it arrives in draws them from what the core has
  open. Opening a project another window holds brings that window to the front instead of
  taking the project in twice.
- **Closing a split window moves its projects back to the main window, behind what that window
  is showing, and ends nothing.** Every session is a child of the app (ADR 0025), and the main
  window's close already hides rather than ends for that reason. A split window cannot simply
  hide: nothing brings a hidden split window back, so its chats would run where the operator
  could not reach them. Closing its projects would end the day's work behind a close button,
  which is what the main window's rule refuses. Handing them back keeps everything reachable and
  is undone by moving them out again. If the main window was hidden, it is brought back, so the
  projects are somewhere the operator can see. The main window's own close still hides it; the
  tray, the dock and a second launch still bring back the main window.
- **A split window that holds nothing goes.** It was made for the projects moved into it. The
  main window holding nothing shows the opener, as before.
- **Each window gets its own projects' events.** `chat-moved`, `plane-changed`,
  `handoff-arrived` and `extension-heard` go to the window holding that plane. A plane no window
  holds yet (one being opened) is still sent to every window, which each filter by plane as they
  always have. A second launch's directory goes to the main window. **Every listener in the page
  names its own window as its target** (`app/src/here.ts`): Tauri delivers every event to a
  listener whose target is `Any`, including one sent to a different window, so a split window
  listening that way would answer the main window's quit.
- **The core keeps one window per project.** A move checks and moves under one lock, and refuses
  a project a third window holds. A window's report of its tabs can be a moment behind a move, so
  a project another window holds is left out of what that report records.
- **Needs you and Quit span every window.** The title bar's ✋ stays the one list of every chat,
  in every project, asking for the operator (ADR 0054): each window tells the others what it has
  asking, and pressing a row for a chat in another window raises that window and carries the row
  out there. The main window is the one asked to quit; its warning lists what would end in every
  window, and it does not quit outright until every window has said what it has open.
- **The project strip in each window lists that window's projects.** The title bar is the
  window's (ADR 0054), and the strip there is what the window holds.
- **A cold launch puts each remembered window back as a window.** The first remembered window
  goes into the main window; the rest become split windows again. Every project still goes
  through the trust gate in the main window first, so a question it raises is asked there, and a
  project two windows both claimed is kept by the first.

**Costs, stated.** A project moved to another window comes back as tabs without the splits it
had: the split layout of a project's panes is the window's state and is not carried across. A
needs-you row from another window is drawn from what that window last said, so it can be a
moment behind. The capability grant widens from `main` to `main` and `window-[0-9]*` (ADR 0052,
amended the same day).
