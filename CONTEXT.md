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

**LIVE / LOCAL**:
Whether a workspace's charter, memory and todos are published with the plane (LIVE) or stay on
this machine (LOCAL, the default).
_Avoid_: shared/private, public

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

**Harness plugin**:
A Claude Code, Codex or opencode plugin, chosen per project. "Plugin" on its own always means
this, never a charter extension.
_Avoid_: extension (for this), charter plugin

**Vault**:
A named set of secrets charter keeps in the system keyring and hands to a command, never to
the model and never to an extension. Vaults are core.
_Avoid_: secret store, keychain (as the name of the concept)

**Forge extension**:
An extension about a code host's pull requests, merge requests or issues, which reaches the
forge through `gh` or `glab`'s own login and never through a secret charter hands it.
_Avoid_: forge plugin, GitHub integration
