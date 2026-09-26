# charter

A desktop app for running many harness sessions against one **plane**, and always knowing which
of them needs you. This file is the glossary: the words, not how they are built.

## Language

### The plane and what lives in it

**Plane**:
The git repo a project's charter lives in: its settings, personas, memory, todos and
workspaces. It is the project's database, and a change counts once it reaches the plane's
remote.
_Avoid_: control plane (in UI text), config repo, charter repo

**charter-plane**:
The charter project's own plane: the one charter is developed from, public as an example of a
plane. It is not the product. The product, the app and its core, is **charter**.
_Avoid_: charter (for the plane), the charter repo

**Project**:
One plane as the app has it open. The app can hold several.
_Avoid_: instance

**Workspace**:
A named piece of work inside a plane, with its own charter (`workspace.md`), memory, todos and
repos.
_Avoid_: task, context

**Repo** (of a workspace):
A clone of a code repository that a workspace holds. It has its own remote and its own rules,
and it is never part of the plane's commits.
_Avoid_: clone (as a noun in UI text), guest checkout, project

**Inventory**:
The plane's list of repos it can clone (`inventory/repos.json`), committed and shared. It only
grows: `discover` and the repo picker add to it, and a repo leaves it only through an exclude.
_Avoid_: repo list (for what the picker shows), catalogue

**Reachable repos**:
The repos your own forge login can reach under the plane's owners, as the repo picker shows
them. Asked each time the picker opens and never saved, because each engineer reaches different
ones.
_Avoid_: discovered repos, the inventory

**LIVE / LOCAL**:
Whether a workspace's charter, memory and todos are published with the plane (LIVE) or stay on
this machine (LOCAL, the default).
_Avoid_: shared/private, public

### The window

**Split window**:
An OS window a project tab was moved into, beside the main window. It holds its own projects,
and closing it moves them back to the main window with every chat still running.
_Avoid_: detached tab, pop-out, secondary window

**Strip**:
One row of tabs: projects (in the title bar), a project's workspaces, or a workspace's chats.
A strip's order never changes on its own, and it never scrolls.
_Avoid_: tab bar, scroller

**Pin**:
One operator's mark that a project, workspace or chat matters to them. It is kept on this
machine and never in the plane. A pinned item is drawn first, and the workspace strip draws
only pinned workspaces plus the one you are in.
_Avoid_: favourite, star, bookmark

**Show-more**:
The button at the end of a strip that lists what the strip is not drawing, sorted by activity,
with the needs-you count of everything it hides.
_Avoid_: overflow (in UI text), more tabs

**Needs you**:
A chat that is waiting on the operator. Every project's are listed in the title bar's ✋ menu,
and each is counted in red on its tab and on any show-more hiding it.
_Avoid_: notification, alert (alerts are a separate drawer)

**Plane updated** (of a chat):
A chat started before the plane's start-time instructions (`CLAUDE.md`, the harness settings
and sub-agents, a persona's charter) changed on disk. It runs on what it read until it is
started fresh, and its tab carries a quiet mark saying so. It is not a needs-you item.
_Avoid_: stale, behind, outdated, Incoming (that is the remote's commits)

### Saving

**Save**:
Taking what changed in the plane or a repo as far as its mode allows: commit, push, PR, merge.
_Avoid_: sync (that word is for fetching repos), commit (a save may be more than one), publish

**Mode**:
How far a save goes: `off`, `commit`, `push`, `pr` or `pr-merge`. Each value includes the
steps of the one before it. The plane has one mode, and each repo has its own.
_Avoid_: policy, posture, share

**Auto-save**:
A save charter starts by itself: after a quiet period, when a session ends, or when the app
quits.
_Avoid_: sync, background push

**Target branch**:
The branch a save is meant to end up on.
_Avoid_: base branch, main (it need not be)

**Save branch**:
The one branch per clone of the plane (named for the machine and the clone) that the plane's
PR modes push to, carrying one open PR into the target branch.
_Avoid_: PR branch, `charter/<sha>` branch

**Stage**:
Where unsaved work sits. It is *changed* (not committed), *committed* (not pushed), *pushed*
(PR open), or *saved* (on the target branch).
_Avoid_: status, sync state

**Blocked**:
A save that can't go further without a person: a conflict, a refused push, a secret the scan
caught, a mode the remote can't take. Auto-save pauses until it's cleared.
_Avoid_: failed, error, stuck

**Incoming**:
Commits on the remote that this machine doesn't have yet.
_Avoid_: behind (in UI text)

**Shared / Local** (settings):
Where a setting's value comes from: `charter.toml` (committed, the team's) or
`charter.local.toml` (this machine's). A Local value overrides the Shared one key by key.
_Avoid_: global/user, project/personal

### Core and extensions

**Core**:
What charter does itself, on every platform, with no extension on. A plane's instructions and
the session-start briefing may depend only on the core.
_Avoid_: built-ins (for core features), platform

**Extension**:
A directory the operator installs and approves on this machine, whose manifest declares what it
contributes and which capabilities it asks for. Its program runs as the operator, one question
at a time.
_Avoid_: plugin, add-on, module

**Built-in extension**:
An extension that ships inside the app and is trusted through the app's signature rather than
an approval prompt. A copy of one anywhere else is an ordinary extension.
_Avoid_: bundled plugin, first-party plugin, core extension

**Capability**:
One thing charter does for an extension that asked for it in its manifest and was approved,
such as showing a badge or adding a CLI command. It describes charter's conduct, never a limit
on the extension.
_Avoid_: permission, grant (as a noun in UI text), power

**Facts file**:
A file an extension keeps in its own state directory, holding the values charter shows for it
(badges, repo cells) without starting its program.
_Avoid_: cache, status file

**Event**:
One question charter asks an extension after a core action it hears about has finished, such as
a workspace being created or the plane being saved. What it answers never changes the action.
_Avoid_: hook (for this), notification, subscription

**Briefing section**:
Text an extension adds to a chat's session-start briefing, quoted as data under the
extension's name. It is never an instruction, and never a permission, a hook or a setting.
_Avoid_: prompt, context injection

**Action** (of an extension):
A verb an extension declares and offers on the rows of its views: pressing one asks its program
to *run action `<id>` on `<subject>`*. Never one of charter's own verbs. charter asks first when
the manifest says so, and always before one that deletes.
_Avoid_: command (for this), verb (unqualified), button

**Palette command** (of an extension):
A row an extension adds to the palette, named with the extension's name, that opens one of its
views or runs one of its actions.
_Avoid_: shortcut, menu item

**Extension command**:
A command an extension adds to the `charter` command line, run as `charter <extension id>
<command> …`. It says whether it writes, and what its program prints and its exit status reach
the caller unchanged. An extension's id is never one of charter's own command words.
_Avoid_: subcommand (unqualified), plugin command, palette command (for this)

**Core-owned alias**:
A core command whose words forward to an extension command and give its output, so a plane's
instructions keep working when a feature moves into an extension (`charter ws todo` once todos
does).
_Avoid_: shim, redirect

**Write paths**:
The plane-relative paths an extension declares it writes. charter hands them resolved with
each request and reports a change outside them; it does not stop one.
_Avoid_: sandbox, allowed paths, scope (as if enforced)

**Harness plugin**:
A Claude Code, Codex or opencode plugin, chosen per project. "Plugin" on its own always means
this, never a charter extension. charter's own is one too: the Claude Code plugin the app
bundles, named `charter` (`charter@inline`, skills `charter:<skill>`), always on in the chats
the app starts. It is not the Python charter's `charter@charter`, which is always off there.
`charter plugin install` puts a copy of it, `charter@charter-app`, in front of the chats the
operator starts outside the app (ADR 0057).
_Avoid_: extension (for this); "charter plugin" for anything but charter's own

**Vault**:
A named set of secrets charter keeps in the system keyring and hands to a command, never to
the model and never to an extension. Vaults are core.
_Avoid_: secret store, keychain (as the name of the concept)

**Forge extension**:
An extension about a code host's pull requests, merge requests or issues, which reaches the
forge through `gh` or `glab`'s own login and never through a secret charter hands it.
_Avoid_: forge plugin, GitHub integration
