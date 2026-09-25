# The control plane: `charter.toml`

A **control plane** is any directory marked by a `charter.toml` file. `charter` finds it
by walking up from your current directory — the same contract git, cargo, and npm use —
so once you're anywhere inside one, every command just works. There is nothing else
special about the directory: no required name, no fixed location. `charter init` creates
one from nothing (see the README's quickstart).

A fresh `charter init` writes the minimal file:

```toml
schema = 1

[[forge]]
kind = "gitlab"

[memory]
share = "local"

[persona]
default = "steward"
```

That last key is the plane's **front door** — the persona a session adopts when nobody has
chosen one. `init` also scaffolds it: one generic `personas/steward/persona.md` you own and
can rename, rewrite or delete. Name it something else with `charter init --front-door ops`,
or skip it entirely with `--no-front-door`; either way charter's own code knows only *that*
a plane may declare a default, never which one. If the plane already has personas, `init`
scaffolds nothing — it creates only what is absent.

The generated charter declares `routing: advise`. Showing the front door the persona roster
on each prompt is not in this version yet. See `charter docs show personas`.

## Every key, in full

```toml
# Layout version this file was written for. `charter` refuses to run against a
# schema NEWER than it understands (upgrade the CLI instead of guessing); it has no
# problem reading an OLDER schema. Omit it and 1 is assumed.
schema = 1

# Optional. The persona a session adopts when nothing else selects one — this plane's
# front door. `charter persona default <name>` writes it. The persona a chat is started
# with in the app's picker ($CHARTER_PERSONA) wins over it; a name that no longer exists
# resolves to no persona at all, and `charter doctor` says so.
[persona]
default = "steward"

# Optional, and not followed by this version: see "Where worktrees live" below.
[plane]
worktrees = "../plane.worktrees" # Where worktrees would live instead of
                                  # workspaces/<ws>/.worktrees/.

# One [[forge]] block per code-hosting forge this control plane tracks. A single-forge
# control plane (the common case) declares exactly one; see "Mixed-forge" below for more
# than one. Each block is independent — its own owner, host, and excludes.
[[forge]]
kind = "gitlab"                  # "gitlab" | "github". Default: "gitlab".
group = "my-org"                 # the GitLab group (or GitHub org/user) this forge tracks.
                                  # `owner` is accepted as a synonym — see "group vs owner" below.
host = "gitlab.com"              # optional: a self-hosted forge's host (GitLab Enterprise,
                                  # GitHub Enterprise Server). Default: the forge's own public
                                  # host (gitlab.com / github.com). See docs/forges.md.
                                  # A bare hostname, optionally :port — no scheme, no path,
                                  # no "@". It widens the SSH guard's deny set and becomes
                                  # the `url.https://<host>/.insteadOf` that
                                  # `charter git-policy --apply` writes, so a block whose
                                  # host is not a hostname is skipped and reported.
exclude = ["this-control-plane"] # repo names never written into the inventory — typically
                                  # the control plane's own repo, so `discover` doesn't list
                                  # itself as a clone target.

# How far a written memory (persona or workspace) travels by default. See "Memory
# posture" below — this is the single most consequential default in this file.
[memory]
share = "local"                  # "local" | "commit" | "push". Default: "local".

# The workspace selected when none is active yet (no --workspace, no $CHARTER_WORKSPACE,
# no prior `charter workspace use`).
[workspace]
default = "default"              # Default: "default".

# Which profile the app's new-chat picker starts on. Opt-in; absent, it starts on the
# first profile. `default` is the only key read here. Harness profiles — a command and an
# environment each — live in charter.local.toml, which is never committed, and a
# [harness.<name>] table in this file is refused. See "[harness] — profiles, and the
# default" below.
[harness]
default = "claude"               # a profile's name: "claude", "codex", "opencode", or one
                                  # charter.local.toml declares. A name this machine does
                                  # not have is REPORTED by `charter doctor`, not ignored.

# Which release channel this plane expects. Which build the app installs is chosen
# per machine, with `charter update --channel`; see "[update].channel" below.
[update]
channel = "stable"               # "stable" | "dev". Default: "stable". A CLOSED set:
                                  # anything else is not sanitised, it is discarded, and
                                  # the plane stays on "stable".
```

### `group` vs `owner`

GitLab calls the thing a repo lives under a **group**; GitHub calls it an **org** (or a
personal **user** account). `charter` accepts either key name in a `[[forge]]` block —
`group` and `owner` mean the same thing — so you can use whichever reads naturally for
the forge in question. If a block sets both, `group` wins. `charter init --owner <name>`
always writes the field as `owner`, since it works for either forge kind.

## One shape

A control plane is its own directory, and a workspace holds **clones** — one per repo the
task touches, each of which can carry worktrees. That is the only arrangement.

A `shape` key, which older planes may carry, is ignored.

`charter init` produces the same plane wherever it runs. Being at the top of a git repo does
not change what you get; it changes whether init writes anything at all. It writes nothing,
and says what the two ways on are:

```
$ charter init --forge github --owner acme
✗ this is the git repo 'myapp', and `charter init` does not make a repository into a
  control plane unless you ask it to. Nothing was written.
• A plane is a directory of its own, and this repo is the first clone in it:
      mkdir ../myapp-plane && cd ../myapp-plane
      charter init --forge github --owner acme --adopt ../myapp
• To make THIS repo the plane instead, ask for it by name:
      charter init --plane-is-this-repo --forge github --owner acme
```

That is a refusal, not a prompt: charter never reads stdin (it runs inside hooks, where
blocking would hang the turn), so naming the option *is* the acceptance. Take the first way and you get
`workspaces/default/myapp/`, cloned from the repo you were standing in and pointed at the
same `origin` it has; take the second and this repo becomes the plane. Either way the
control plane itself is identical, and until you choose, nothing is written to your repo at
all.

Why a repository is not made into a plane unasked is
[ADR 0035](https://github.com/diazoxide/charter/blob/main/docs/adr/0035-a-plane-is-untrusted-until-the-operator-opens-it.md)
and charter-app spec decision 27. `charter init` anywhere that is not the top of a git repo
makes the plane right there.

### The plane root is not a place to work

The directory holding `charter.toml` holds the control plane — personas, inventory,
workspaces, config — and nothing you edit. Work happens in a workspace's clones.

Nothing in the filesystem enforces that, which is why it is worth stating. The plane root
is often a real git repo, and not listing it as one of your repos is not the same as
preventing work in it: two sessions that both sit in the plane root share one working tree
and one HEAD, and will thrash each other's branches while charter reports two different
workspaces.

So `charter doctor`'s `plane root` row warns when the plane root is dirty or off its default
branch, and the Bash guard refuses a branch move in it.

### Where worktrees live

Worktrees normally sit at `workspaces/<ws>/.worktrees/<repo>/<piece>` — deliberately
**outside every clone**, so that nx, jest and maven never recurse into them and a
`git clean -xfd` inside a clone cannot destroy live work.

That default is right whenever a workspace holds clones, which is always. It matters
because the failure it avoids is quiet: put worktrees anywhere a build tool globs from and
the tree answers a root-level glob with several copies of itself. Measured in a layout that
made that mistake — 214 test files discoverable from one root, 142 of them duplicates.
`.gitignore` hides that from git and from nothing else; pytest, jest, nx, tsc and every IDE
indexer read the working tree directly.

Moving them somewhere else is not in this version yet. A plane whose `charter.toml` sets
`[plane].worktrees`, or a process with `$CHARTER_WORKTREES` set, is refused by name when a
worktree would be cut, rather than having its worktrees put somewhere it did not ask for:
unset it to keep worktrees in the plane.

### What a workspace is

A workspace is **a set of working trees** — one task's code, kept apart from every other
task's. It gets them by cloning: `charter workspace create feature-x`, then `charter clone`
the repos that task touches. `default` is an ordinary workspace like any other.

```
$ charter workspace create feature-x
✓ Workspace 'feature-x' ready (LOCAL — private (nothing committed); `charter workspace live` to share) → workspaces/feature-x/
• Select it with: charter workspace use feature-x  (or --workspace feature-x per command)
```

A solo user with one repo makes a plane in a directory of its own with
`charter init --adopt <repo>` — the first way out of the refusal above — and works in
`workspaces/default/<repo>/`.

**Selecting a workspace with no tree is refused**, because it would put you on the same
files as every other workspace — the thing workspaces exist to prevent. `charter workspace
create <name>` gives it one.

### The directory you are in decides the workspace

A workspace's trees live at paths that name it — `workspaces/<ws>/<repo>/` for a fleet
clone, `<worktrees-root>/<ws>/<repo>/<piece>/` for a worktree — so **being inside one is
the answer**, ahead of any pointer:

```
--workspace → $CHARTER_WORKSPACE → the tree you are standing in → session → terminal → default
```

You cannot be in two directories at once, which is precisely the property the pointers
lacked: a session that had never chosen anything could inherit another session's choice.
`charter workspace current` reports `via cwd` when the directory decided it.

### Working inside a worktree

`charter.toml` is a tracked file, so if the repo you are working in is itself a control
plane, every worktree cut from it checks out a copy — which makes each worktree *look* like
its own control plane. It is not: a worktree is a view of a repo, not a repo.

So the plane's identity follows the **main working tree**. Standing anywhere inside a
worktree, `charter` resolves the plane to the repo the worktree belongs to, and personas,
the vault directory, workspaces and memory all stay attached to it. Otherwise they would
resolve into the worktree itself — a directory `git worktree remove` deletes, taking any
memory written there with it.

`$CHARTER_ROOT` is never redirected, so pointing it at a worktree is the escape hatch if
you genuinely want a plane of its own there.

### Nested planes

`charter` takes the **innermost** `charter.toml` above your working directory — the rule
git, cargo and npm use — with **one exception, and charter builds the shape it covers.** A
plane clones repos into its workspaces, and a cloned repo may itself carry a `charter.toml`,
since `charter init` is run inside existing repos. Taking the innermost marker there landed
you in a *different* control plane: different personas, a different vault, and memories
written somewhere you did not choose.

So when a plane sits inside another plane's `workspaces/`, resolution hops **outward** to
the enclosing one, and keeps hopping until nothing encloses it — the plane that actually
holds the vault wins. The hop is allowed only through an enclosing plane's own
`workspaces/`: a stray `charter.toml` in `~` does not swallow every plane beneath it.
`$CHARTER_ROOT` still wins outright, and is the escape hatch when you genuinely mean the
inner one.

Charter is not quiet about the hop. `charter doctor`'s `nested plane` row reports it, and
**`charter save` refuses**
outright rather than committing a tree you are not standing in — that being the one command
whose whole job is to stage everything under the plane it resolved:

```
✗ Refusing to stage all of /…/plane — you are standing in /…/plane/workspaces/dev/charter,
  a control plane of its own under that plane's workspaces/. …
•   your own work:   git -C /…/workspaces/dev/charter add -A && git -C /…/workspaces/dev/charter commit
•   the plane's own: run `charter save` from /…/plane
•   you really do mean this plane: CHARTER_ROOT=/…/workspaces/dev/charter charter save
```

Only the unbounded stage refuses.

## A self-hosted example

A GitLab Enterprise instance behind your own domain, excluding the control plane itself
from the inventory:

```toml
schema = 1

[[forge]]
kind = "gitlab"
host = "gitlab.internal.acme.com"
group = "platform"
exclude = ["charter-control-plane"]

[memory]
share = "commit"

[workspace]
default = "default"
```

## Mixed-forge: tracking GitLab and GitHub together

A control plane isn't limited to one forge. Declare a `[[forge]]` block per forge and
`charter discover` queries each in turn, merging the results into one
`inventory/repos.json` — and into what it already lists, since each engineer's login reaches
different repos (a repo leaves it only through `exclude`). This is the non-obvious case — most control planes need only one
block — but it's fully supported:

```toml
schema = 1

# Internal platform repos, on a self-hosted GitLab.
[[forge]]
kind = "gitlab"
host = "gitlab.example.com"
group = "platform-team"
exclude = ["legacy-monolith"]

# Open-source repos, on github.com.
[[forge]]
kind = "github"
owner = "acme-oss"

[memory]
share = "commit"
```

Repos are exposed under their **bare name** (the final path segment) everywhere —
`charter clone api`, not `charter clone gitlab:platform-team/api` — so every command
keeps working unchanged. If two forges (or two blocks of the same kind — e.g. two GitHub
orgs) happen to expose a repo with the same bare name, `charter discover` refuses to
guess: it names both candidates and tells you to either qualify the name
(`github:api` vs `gitlab:api`) when cloning, or exclude one via that block's `exclude`.
Full detail, including exactly which collisions can and can't be qualified away:
`docs/forges.md`.

## Memory posture: `[memory].share`

Every persona and workspace can write **memory** — durable notes recorded with
`charter persona remember` / `charter workspace remember`. How far those notes travel is
one setting, `[memory].share`, with three values:

| Mode | What it asks for |
| --- | --- |
| `local` | The memory file is written to disk and nothing else. It never enters git, never leaves the machine. |
| `commit` | The file is committed **locally** — part of your history, never pushed. |
| `push` | Committed, then pushed to `origin`. |

**The default is `local`**, and that default is deliberate: a stranger who just ran
`charter init` has not decided yet whether this control plane's notes should be shared
with a team, and the failure mode of guessing wrong runs only one direction — publishing
an agent's working notes to a remote nobody reviewed.

**This version writes every memory as `local`.** Committing memory is not in this version
yet: a plane set to `commit` or `push` gets the file on disk, and the write says that nothing
was committed, so you can commit and push `memory/` with git yourself. A value charter does
not recognise reads as `local` — a typo in this file fails *safe*, not loud.

## `[workspace].default`

The workspace name `charter` resolves to when nothing else has selected one — no
`--workspace` flag, no `$CHARTER_WORKSPACE`, no prior `charter workspace use` in this
session. Defaults to `"default"` (and `default` always exists — `charter clone` creates it
on first use). [workspaces.md](workspaces.md) has the full precedence chain.

**A workspace name, not a path.** This file is committed, so the value is whoever last
edited it — and charter joins it onto `workspaces/`. It must therefore be a name `charter
workspace create` would mint (letters, digits, `.`, `_`, `-`; not starting with a dot).
Anything else degrades to `"default"`. The committed `workspaces/.default` file, written
by `charter workspace default`, is held to the identical rule.

## `[charter].version` — pinning charter

**Opt-in.** Absent, charter does nothing with it. Present, this control plane names the
charter version it expects, shared the way a lockfile is.

```toml
[charter]
version = "0.1.0"
```

The pin names a version of **this app**: the number `charter version` prints. `charter
version` shows that number and the pin, and its exit status is the part a script reads — 0
with no pin, 0 when the pin is met, 1 when it is not (drift). The window marks drift in its status bar.

This charter does not install anything to meet a pin. It is a binary inside the app, and the
app is what moves it (see [install.md](install.md)), so `charter version sync` and `charter
version bump` are refused by name rather than run.

A plane may carry a pin from an older charter release line, `charter-cp`, which ended at
`0.62.1`. **That pin is not drift.** A pin that is a version, is not this app's own and is no
newer than `0.62.1` is read as naming that line: `charter version` says so and exits 0, and
nothing asks you to move it. To hold the plane to this app instead, set `version` to the
number `charter version` prints, or remove the pin. A pin newer than `0.62.1` that is not this
app's version, or one that is not a version at all, is drift.

While this app's own version is at or below `0.62.1` the two lines share numbers, so a pin
written by hand for this app in that range reads as the older line too. The reasons are
[ADR 0045](https://github.com/diazoxide/charter/blob/main/docs/adr/0045-charters-version-is-the-apps-version.md).

## `[harness]` — profiles, and the default

Two Claude Code accounts, work in one config folder and personal in another, or a Codex
pinned to an older release. A shell alias does not help — the app runs the harness with no
shell, so the alias never resolves. A **harness profile** is the way: a kind, a command and
an environment, declared in a file that stays on your machine.

**Every chat starts on a profile.** Charter reads the profiles, refuses the broken ones by
name, and keeps the file out of git; the app's new-chat picker, a chat reopened when the app
starts again and a chat opened by a handoff all run the profile they name and record it. A
profile this file declares runs once you have seen its command and said so — see *A new or
changed command asks once*, below. Starting a chat on a profile from a terminal
(`charter <profile>`) is not in this version yet.

### Profiles live in `charter.local.toml`, never in `charter.toml`

```toml
# charter.local.toml — beside charter.toml, never committed
[harness]
default = "claude-work"                          # optional

[harness.claude-work]
kind = "claude"
command = ["claude"]
env = { CLAUDE_CONFIG_DIR = "~/.claude-work" }

[harness.codex-pinned]
kind = "codex"
command = ["npx", "-y", "@openai/codex@0.140.0"]
```

A profile's command runs on a click, with no harness permission prompt in between. In the
committed file a merged PR, or a chat, could change that command and have it run on every
machine that pulls it. So charter reads profiles only from `charter.local.toml`, and a
`[harness.<name>]` table in `charter.toml` is refused with a pointer to the local file.
`charter.toml`'s `[harness]` keeps `default`; any other key there is ignored, as it always
was. A `default` in the local file wins over the committed one.

**The local file carries `[harness]`, `[extensions]`, `[theme]`, `[harness_plugins]`,
`[plane]` and `[repos]`, and nothing else.** Any other section in it is refused by name. An
ignored file that could override `[[forge]]` — whose hosts steer the one-credential guard —
would change plane policy with no trace in git. And the file is read only while git would not
carry it: if git tracks it or would commit it, charter reads nothing in it, and
`charter.toml` and the workspace decide instead (charter-app#308).

- `kind` — which harness program: `claude`, `codex` or `opencode` (read and listed, but
  starting an opencode chat is not in this version yet).
- `command` — a list of arguments, never a shell string, because no shell runs it. A leading
  `~` in its first word is expanded, since no shell is there to do it.
- `env` — optional: variables set on the harness, each value's leading `~` expanded.
- The table's name is letters, digits, `_` and `-`, starting with a letter or digit, and no
  dot.

The record of each command you approved is `.charter/harness-profiles-launched.json`, in the
plane.

**Every harness is also a built-in profile named after itself** — `claude` runs `claude`,
with no extra environment — so a plane that declares nothing sees no change. A declared
profile of the same name replaces it, which is how plain `claude` gets pinned. A built-in
cannot be hidden, but a replacement that is refused takes the name with it rather than
letting the built-in stand in: you said how `claude` runs, and running the default in its
place would run the command you replaced.

### What is refused, and the fix

A broken profile is refused alone, by name, and every other profile still loads.
`charter harness list` and `charter doctor` both say which one and why.

| Refused | Why | Fix |
|---|---|---|
| a `kind` charter does not know | nothing could start it | `claude`, `codex` or `opencode` |
| a `command` that is not a non-empty list of text | no shell runs it, so `"claude --resume x"` names one program | `["claude", "--resume", "x"]` |
| a `command` whose first word is charter itself | a profile names the harness a chat runs, and charter is not a harness | the harness's own command |
| a name holding a dot or any other character, or named `default` | a name is letters, digits, `_` and `-`; `default` is the one key under `[harness]` that is not a profile | rename the table |
| a name `charter` reserves for a command — `doctor`, `workspace` and the rest | a profile named like a command would shadow it | rename the table |
| an `env` that is not a table of text | a variable holds text | `env = { NAME = "value" }` |
| an `env` name starting with `CHARTER_` | charter sets those itself, and a profile's own setting would tell charter's hooks the wrong harness or plane | delete the variable |
| an `env` name containing `KEY`, `TOKEN`, `SECRET` or `PASSWORD` | a variable set on the harness reaches the model's shell, as the next section measures | log in inside the harness |
| a key other than `kind`, `command` and `env` | a typo such as `enviroment` would drop `CLAUDE_CONFIG_DIR` and launch the default account without a word | remove or respell it |
| a file that is not valid TOML | nothing in it can be read | fix the file; the built-ins still load |

### A new or changed command asks once

A profile's command runs on a click, and charter starts it with no shell and no harness
permission prompt in between. So the first time you pick one in the app's new-chat picker, the
picker shows what it runs — its command and its environment — and starts nothing until you
press **Approve and start**. The approval is written down — the profile's `kind`, `command`
and `env`, exactly as the file spells them — and that profile then starts without a word
until one of those three changes. A change asks again.

**The approval is of what you were shown.** Between the picker reading the profile and your
press, `charter.local.toml` can change. The app checks the line it showed you against the file
again, and if they differ nothing is approved, nothing starts, and it says what the profile
runs now.

**Built-ins never ask.** `claude`, `codex` and `opencode` come out of charter's own
registry rather than out of a file, so there is nothing about them a question could catch —
and a question that never carries risk is one you learn to answer yes to without reading. A
profile the file declares under a built-in's name (`[harness.claude]`) is a declaration, and
asks with the rest.

**Why ask at all**, when nothing else charter does stops to. Once `charter.local.toml` is
ignored, an edit to it leaves no diff — no review, no `git status`, nothing on a branch for
anybody to notice. Nothing stops a chat editing your plane's config, and a chat is an agent
with a shell. Codex trusts its hooks by hash for the same reason.

**Where it asks, and where it refuses instead.**

| Opening a chat | What happens |
|---|---|
| the app's new-chat picker | shows the command and asks; nothing starts until you approve |
| a chat reopened when the app starts again | refused by name and left in the record — a reopen has nobody to ask. Start it once from the picker, and it comes back next time |
| a chat handed off by another chat | refused before anything is written: no chat, no first message |

**The limit, said plainly.** The record lives in `.charter/harness-profiles-launched.json`,
which is as writable by a chat as `charter.local.toml` is: both sit under paths no guard
covers, and charter does not police paths (that is host policy). So this catches a command
you did not change yourself **unless whatever changed it also forged the record**. It closes
the accident and the careless edit, and it is the difference between a command that ran
unseen and one that was read out loud first. It is not a boundary.

If charter cannot write that record, the approval fails and nothing starts: starting anyway
would mean asking you the identical question at the next open, which is not something you can
fix by answering.

### No credentials in a profile

Anything set on the harness process reaches the shell the model runs — measured on Claude
Code, and on Codex, whose default `shell_environment_policy` passed a `*_TOKEN` variable
straight through. A vault reference would change nothing: the key would only rest somewhere
else before landing in the same shell. So a variable named like a credential is refused, and
the refusal names the harness's own login, kept in the config folder the profile already
moves:

- Claude Code: set `CLAUDE_CONFIG_DIR` and run `/login` inside Claude Code.
- Codex: set `CODEX_HOME` and run `codex login`.

The pattern can refuse an innocent name — `KEYBOARD_LAYOUT` holds `KEY` — and a wrapper
script on `PATH` can still export a key. Charter declines to hold one; it cannot prevent one.

### Kept out of git, and checked

`charter init` writes `/charter.local.toml` into a new plane's `.gitignore`, and
`charter reinit` adds it to a plane made before this existed. The line is not trusted
blindly: `charter harness list` and `charter doctor` ask git, and while git tracks the file
or would commit it, every profile declared there is listed refused, with the reason, and one
line names the fix:

```
refused:
  claude-work: git would commit charter.local.toml, so charter reads nothing in it until it is ignored — charter reinit adds /charter.local.toml to .gitignore.
! to use the profiles in charter.local.toml: charter reinit
```

Each state has its own fix. A committable file needs the ignore line, which `charter reinit`
adds. A tracked file stays refused after the ignore line is added, because the next commit
still carries it — even after `git rm --cached`, until that removal is committed — so its fix
is `git rm --cached charter.local.toml`, commit that removal, then `charter reinit`. A plane
that is not a git repository has nothing to commit to, and passes. Any other answer git cannot
give — git missing, a timeout, an answer it cannot read — refuses too, because an unknown is
not a pass; the fix names the command to run by hand, `git status --ignored --
charter.local.toml`, beside what git said.

The check is one `git --no-optional-locks status`, which takes no `index.lock` from a commit
running beside it. It runs when a person asks — `charter harness list`, `charter doctor` —
and not when charter merely reads its config, and not on a hook path: the SessionStart hook
runs `charter doctor --preflight`, which skips it. `charter doctor`'s `harness profiles` row
warns for each refusal above, with each state's own fix, and for a `default` that names no
profile this machine has.

### The profile is armed at launch

Moving a profile to another config folder moves nothing of charter's: **its guard does not
live in that folder.** The app arms every chat it starts on the command line, for that session
alone, and installs nothing into any folder — a Claude Code chat gets the app's own plugin,
`charter`, with `--plugin-dir`, and `--settings` turning a plugin named
`charter@charter` off for that chat only; a Codex chat gets charter's hooks as
`-c hooks.<Event>=…` flags, which Codex asks once to trust. So a new `claude-alt` needs no
wiring step: once you have approved its command, it starts armed.

What still stops a chat on a profile is said before anything opens, and nothing is written:
a profile you have not approved, a `charter.local.toml` git would commit, or a kind this app
does not start — opencode, for now. `charter doctor`'s row per profile runs nothing; it says
how the app arms that kind and warns only when the harness's program cannot be found, naming
where it looked.

What each kind is armed with, and what a chat on each can and cannot report, are in
[harnesses.md](harnesses.md#per-profile--armed-at-launch).

### `default` — the picker's first choice

**Opt-in, and it launches nothing.** This key chooses which profile the app's new-chat picker
starts on, and marks it `default`. Absent, the picker starts on the first profile.

```toml
[harness]
default = "claude"
```

The value is the name of a profile: a built-in — `claude`, `codex`, `opencode`, as
`charter harness list` shows them — or a profile `charter.local.toml` declares. A `default`
in the local file wins over the committed one, which is how a machine chooses its own without
touching what everybody else pulls. A declared profile is a legal value, and starting it
asks like any other (*A new or changed command asks once*).

**Charter does not pick one for you.** Not "whatever is installed" (a machine with two of
them has no answer, and the answer would change the day a colleague installed a third) and
not "the one you ran last" (a machine-local memory deciding what a committed file does).

**A name this machine does not have marks nothing, and `doctor` says so:**

```
$ charter doctor
  !  charter.toml          [harness] default = "clyde" is not a harness charter can launch
```

## `[update].channel`

**Opt-in.** Absent, the plane says nothing about channels.

```toml
[update]
channel = "dev"
```

Which build the app installs, and from which channel, is decided **per machine** and not per
plane: `charter update --channel dev` or `--channel stable` (see [install.md](install.md)).
A plane is committed and arrives from a teammate's machine, so nothing in it decides what the
app installs on yours.

**The value is a closed set of two, not a string.** Anything that is not exactly `"stable"`
or `"dev"` is discarded and read as `stable`; nothing you can write here reaches a URL, a
command line, or an argv element.

## Schema drift and healing

`schema` is the plane's **format version**, and the one thing it buys is the ability to
refuse. It is the same contract git states for a program reading a repository it did not
write — *"an implementation which does not understand a particular version advertised by
an on-disk repository MUST NOT operate on that repository"* — and no more than that. It is
not a schema and not a spec: the shape of the files charter writes is charter's own
business and is free to change. The commands are the interface.

So a control plane written by a *newer* charter than the one you have installed is not
operated on. Every command declines with the "upgrade charter" message and exits 1, except
four, each of which is a question about charter rather than a read of the plane's contents:

| still runs | why |
|---|---|
| `charter doctor` | where the refusal is reported — the `schema` row names both versions |
| `charter --version`, `charter version` | which charter is this |
| `charter update` | nothing but a newer charter can understand a newer plane, and the app is what installs one |

`charter init` and `charter reinit` are deliberately **not** exempt: writing into a layout
charter has been told it does not understand is the most damaging guess available to it.
A `schema` charter cannot compare against at all — `schema = "2"`, `1.5`, `true` — is
refused on the same terms as one from the future.

**Omitting `schema` means 1**, not "whatever charter you are running". A plane created
before planes declared a version *is* a version-1 plane, so it stays readable by every
charter forever; reading it as "current" would be the same guess, arrived at through the
number that exists to prevent it.

Going the other way — an *older* control plane opened by a newer charter — is always fine:
newer charter versions can add baseline directories (`personas/`, `inventory/`,
`workspaces/`) a control plane predates, and `charter reinit` creates whatever's missing,
additively, never touching what's already there.

**`schema` and a workspace's own structure version are nested, and only `schema` refuses.**
`charter workspace reinit` repairs a workspace whose interior an older charter laid out;
that is a *repair* number, and a workspace charter can still read is exactly what makes the
repair additive. `schema` is the *refusal* number, and nothing heals it but a newer charter.
The two are never compared against each other.

**`.charter/` carries no such promise, on purpose.** It is charter's own state — per
machine and gitignored. It has no format version: the files in it may change shape in any
release.

## The plane, rendered

**In the app, the window draws the plane.** `charter statusline` is Claude Code's footer
command: inside a chat the app started it prints an empty line — so the harness's own footer
stays blank unless the chat was started with its footer on — and still records the turn's
token usage. Run anywhere else, it draws the plane's identity row — the active workspace and
how many workspaces there are — and says in its body which parts it does not draw yet: repos,
personas and the session.

**`charter init` does not wire it into Claude Code's footer.** A `statusLine` key in
`.claude/settings.json` works if you write one; charter neither adds it nor removes it, and a
chat the app starts gets charter's footer only where the settings in force fill it with
nothing ([harnesses.md](harnesses.md)).
