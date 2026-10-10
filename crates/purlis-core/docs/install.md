# Install

**purlis is one desktop app, and the `purlis` command ships inside it.** You install the
app; there is no separate package to install for the command, and no package manager is
involved at any point.

Builds are published on the releases page of
[purlis/purlis](https://github.com/purlis/purlis/releases): a macOS `.app` in
a `.dmg`, and a Linux `.deb` and AppImage. There is no Windows build yet.

## 1. Pick a channel

| Channel | Built from | Where it is published |
| --- | --- | --- |
| **stable** (the default) | a `v*` tag | the latest release |
| **dev** | every green `main` | the `dev` prerelease, whose files are replaced on each build |

Download from the channel you want. After the first install the app moves itself (see
*Updates*, below), and one command switches a machine from one channel to the other:

```bash
purlis update --channel dev      # or: --channel stable
```

## 2. The first launch on macOS

Builds are signed, either with a Developer ID or ad hoc, and they are **not notarized**. A
browser marks what it downloads as quarantined, so macOS refuses the first launch once. The
one instruction that works for both kinds of signature, before that first launch:

```sh
xattr -dr com.apple.quarantine /Applications/purlis.app
```

purlis was called charter before. If `charter.app` is in Applications, delete it once
`purlis.app` is there: purlis replaces it and will not start while the old app runs. An install
that updated itself keeps the folder name `charter.app` and needs nothing.

On a build with a Developer ID, **System Settings → Privacy & Security → Open Anyway** also
works. Updates never meet this: the updater downloads into memory and unpacks the new bundle
itself, so nothing it writes carries the quarantine flag.

On Linux the AppImage updates itself and a `.deb` does not, because `dpkg` owns it.

## 3. Updates

The app checks for a newer build a minute after launch and every six hours after that, on the
channel this machine follows. It **installs only when you ask**, because installing restarts
the app and the app owns every running session. Every update is verified against the
updater's signing key before anything is unpacked, and one that does not verify is not
installed.

`purlis update` on the command line does not install anything. It is the half of updating
that is about a plane's content: what the versions it skipped brought, and what this plane
has not taken up. `--to` and `--bump` are refused by name, and `purlis version` says what
this purlis is and what the plane pins.

## 4. The `purlis` command

The bundle carries the `purlis` binary beside the app's own executable
(`charter.app/Contents/MacOS/charter` on macOS). The app runs **that** copy for every hook
and every refresh, by its absolute path, and deliberately does not look at `PATH`: a
`purlis` found on `PATH` could be an older install, or a different program, answering the
same events differently.

The app does not put the binary on your shell's `PATH`. A chat the app starts is different:
its `PATH` gets the bundle's directory added, so `purlis` answers inside a chat. To use it
from your own shell, call it by its path in the bundle, or link it into a directory that is on
your `PATH`.

## 5. The Claude Code plugin

The app ships a Claude Code plugin inside its bundle and loads it into each chat it starts,
with `claude --plugin-dir`. There is nothing to install into Claude Code, no marketplace to
add and no per-project install to keep in step: the plugin and the binary its hooks call come
from the same build, so they cannot drift apart.

The plugin is called `purlis`. It carries every hook purlis answers — the ones that
report a chat's state and the Bash guard, described in [hooks.md](hooks.md) — and the
`handoff`, `working-in-a-clone`, `update`, `persona`, `secrets`, `browser`, `safe-remove`,
`compact` and `add-curation-action` skills, which
reach the model as
`purlis:<skill>`. Until the rename to purlis it was called `charter`, with skills
`charter:<skill>`, and up to 0.2.0 `charter-app`, with skills `charter-app:<skill>`; a chat the
app starts turns every one of those ids off. It lives in `Contents/Resources/plugin` on macOS and
`/usr/lib/charter/plugin` on Linux. A chat the app starts also turns a plugin named
`charter@charter` off for itself, so a plane whose settings enable an older purlis plugin for
your own terminal sessions does not give an app chat two sets of hooks. A `claude` you run in
a terminal is untouched until you run `purlis plugin install` (below). A Codex chat gets purlis's state hooks and Bash guard as `-c` flags on
its command line, and Codex asks once to trust them. How each harness is armed is in
[harnesses.md](harnesses.md#per-profile--armed-at-launch).

### Chats you start in a terminal

The app arms only the chats it starts. For a `claude` or `codex` you start yourself, run

```
purlis plugin install            # --dry-run first to see each change
```

once. It prints every change it makes and changes nothing that is already so, so running it
again after moving or updating the app is safe. After an update you do not have to: when the
app starts, it brings an installed copy that runs its own `purlis` up to date. It never
installs for a harness you did not install for, and it leaves alone a copy that runs another
`purlis` that is still there. For Claude Code it keeps a copy of the app's
plugin in `~/.config/charter/plugin/` whose hooks name this `charter` by its path, and
registers it in your user `settings.json` as `purlis@purlis-app`, taking out
`charter@charter-app`, the id an install from before the rename used; the local migration
(`purlis migrate`, and the app at its launch) does the same for an install it finds, for every
harness, and `purlis migrate --undo` puts it back under the old ids. For opencode the guard is
`plugin/purlis.ts`, and a `plugin/charter.ts` of charter's own goes. A chat the app starts
still loads the app's own copy instead. For Codex it adds only purlis's Bash guard to
`~/.codex/config.toml`, because the app already gives its own Codex chats the rest and Codex
would run both. Codex asks you to trust that hook the next time it starts. It never enables
the retired `charter@charter` plugin, and turns it off in the files it writes.
`purlis plugin uninstall` takes back what it wrote, including Codex's record that you trusted
the guard. `--harness claude|codex` limits
either one to one harness.

`purlis doctor` says whether it is installed for each harness set up on the machine
(`plugin install`), whether the copy is what this purlis would install now (`plugin`),
whether the `purlis` its hooks run still exists (`plugin files`), and names every settings
file that still enables `charter@charter` (`superseded plugin`).

`purlis doctor --fix` repairs first, printing each change on stderr, and then reports.
Bare `--fix` makes the local repairs, which only add what is missing:

- **Plugin install** (`plugin-install`). It runs `purlis plugin install`, which writes this
  machine's harness configuration and no project file. Bare `--fix` always runs it, once, even
  when the `plugin install`, `plugin` and `plugin files` rows all offer it.
- **Every local fix a row offers.** Each one adds what is missing and never removes or replaces
  your content:
  - `reinit`, offered when a baseline folder is missing.
  - `local-ignore`, offered when git would commit `charter.local.toml`. It appends the one
    line `/charter.local.toml` to `.gitignore`. When git already tracks the file, it refuses,
    because an ignore line does not untrack a file. Run `git rm --cached charter.local.toml`
    and commit that removal yourself. It is not offered when `.gitignore` is a symbolic link,
    which git does not read.
  - `memory-optimize`, offered when a memory is missing from its `MEMORY.md`. It appends a link
    line to `MEMORY.md` for each memory that has none, and changes nothing else. Collapsing
    exact duplicates stays with `purlis persona optimize --apply` and
    `purlis workspace optimize --apply`.
- **The report ask rule.** It adds the project's ask rule for `purlis report --yes` when that
  rule is missing, as `purlis guard ask` does.

**`rename-plane` runs only by name**, as `purlis doctor --fix rename-plane`, because it makes a
commit every teammate pulls. It renames the project's committed files to purlis's names in one
commit and nothing else: `charter.toml` becomes `purlis.toml` and requires the `purlis-names`
feature, so a build without it opens the project read-only; `.charter-scan-allow.toml`, the
managed blocks' markers, a committed `workspace.json`'s digest key, `.claude/settings.json`'s
harness variable, and personas' `charter:` skill references follow. Each `charter …` ask or
deny rule gets a `purlis …` twin and is kept. The one it gives no twin is the retired ask
for `charter handoff *`, which `purlis doctor --fix handoff-rule` removes. Hook commands keep `charter`, which runs on every
build. It refuses, writing nothing, when run from inside a chat (run it from a terminal or the
app), on a project with uncommitted changes, outside git, with a file under both names, or one this
purlis may not write. A step that fails puts every file back.

**`persona-agents` runs only by name**, as `purlis doctor --fix persona-agents`, because it
changes committed files. It takes out the persona sub-agents purlis used to generate, now that
a persona runs as its own chat: it removes each file under `.claude/agents/` that purlis
generated, told by its marker where the generator wrote it, where git tracks the file and it
has no uncommitted change, and leaves and names every other file there. In each persona's own
definition it rewrites `model:` to `profile:` where that is a profile the project carries and
its chats already start on, and `color: cyan` or `magenta` to purlis's name for the colour. It
reports the keys nothing reads now with what widened, and any `.claude/agent-memory/` folder
the harness kept for a sub-agent. It makes no commit, `git restore -- .claude/agents personas`
takes it back before the next save, and a second run changes nothing more. It is offered by
the `personas` row when a generated file is still there. See `purlis docs show personas`,
*After updating*.

**`handoff-rule` runs only by name**, as `purlis doctor --fix handoff-rule` or the
`handoff gate` row's Fix button. It is the one fix that takes a line out, and the line is one
`purlis init` wrote: the ask rule for a handoff, `Bash(purlis handoff *)` and
`Bash(charter handoff *)` under `permissions.ask` in `.claude/settings.json`, and the same two
globs as `"ask"` in `opencode.json`. A handoff is a dispatch now, and its consent is the
dispatch grant, so the rule only makes your harness ask a second time. The fix removes exactly
that rule, names every other rule about a handoff it left because a person wrote it, and
commits nothing. It reads both files before it writes either. It waits to be asked because the
files are ones every teammate pulls, and a teammate on an older purlis is still asked by that
rule and by nothing else.

**`discover` runs only by name**, as `purlis doctor --fix discover` or the inventory row's
Fix button, because it goes over the network. It is offered when the inventory is empty and
`charter.toml` declares a forge. It runs `purlis discover`, which asks that forge, adds the
repos it lists to `inventory/repos.json`, and rewrites purlis's generated `docs/topology.md`.

**`workspace-reinit`** is offered by the `workspace layout` row, which names each workspace
behind the current layout, and by the Alerts drawer's Reinit button on its `reinit` row. The
two read the same workspaces. A workspace purlis cannot read is left out of the row, because
the rows that look inside it already say it cannot be checked. Bare `--fix` runs it, as does
`purlis doctor --fix workspace-reinit` or the row's Fix button. It runs `purlis workspace reinit --all`, which brings every workspace behind
the current layout up to it and never removes your content. It writes each workspace's missing
baseline files and its structure stamp, refreshes the live block purlis manages in the
project's `.gitignore`, and rewires purlis's harness layer in each workspace and in each clone
and worktree under it, including purlis's lines in a clone's `.git/info/exclude`. It removes
only the layer files purlis generated and the project no longer declares; a file purlis did
not write is left untouched. `purlis reinit` (the `reinit` fix) is the project root's and
never looks inside a workspace.

`purlis doctor --fix <id>` applies only the fix a row names: `purlis doctor --json` prints
that id as the row's `fix`, and `purlis doctor --help` lists the ids. Each fix says what it
changed, or why it refused. On a project this purlis can only read, `--fix` is refused and
writes nothing.

One fix takes input. When `user.name` or `user.email` is unset, the `git identity` row offers
`git-identity`, which needs a name and an email:
`purlis doctor --fix git-identity --name "Your Name" --email you@example.com`. It writes both
to git's global config, the scope the row's hint names, because purlis commits in the project
and in every clone. Each value is checked first, and a refused one writes nothing. A bare
`--fix` without `--name` and `--email` says what to give. In the window, its Fix button opens a
small form for the two values. The fix fills in only what is missing: a key that is already set
is shown locked and never replaced, and with both set the fix writes nothing. The doctor reads
git's identity with only `HOME` kept, so an identity kept under a non-default
`XDG_CONFIG_HOME` reads as missing there, and the fix writes `~/.gitconfig`.

### Rules that always ask, or stop asking

`purlis guard ask '<pattern>'` makes every harness prompt before a command, and
`purlis guard allow '<pattern>'` stops the prompt. Each writes the harness's own rule, in
the file that harness reads: `permissions` in `.claude/settings.json` for Claude Code, and
`permission.bash` in `opencode.json` for opencode. Codex has no command-pattern
permissions, and the command says so. If one of those files cannot be read, nothing is
written anywhere. `--local` writes `.claude/settings.local.json`, which is not committed,
so the rule is yours alone. An ask rule is written into every workspace's generated
settings at once, and the command names the workspaces it reached and any whose settings it
could not rewrite. An allow rule reaches a chat at the plane root only: a
workspace's and a clone's settings carry ask and deny rules and never allow.
A harness matches a rule against the command as written, so purlis's guard stands behind your
ask and deny rules: a program a rule names, run under another spelling the rule would not match
(a path to it, another case, a variable or wrapper in front, a quote or escape, a subshell, or a
string a shell runs), is refused and told to spell it as the rule does. A rule on the command
line itself holds under both its names. A rule whose program is a wildcard
(`'*kubectl delete*'`) is left to the harness alone.
`purlis guard handoff` is retired and writes nothing: a handoff's consent is the dispatch
grant, and `purlis init` writes no rule for one. To have your harness ask as well, write a
rule of your own with `purlis guard ask 'purlis handoff*'`, which purlis holds under every
spelling of the command. The glob `init` used to write, `purlis handoff *` with the space, is
read as the retired rule: as an `ask` it covers the plain spelling only, and the doctor
offers to remove it. A `deny` on either glob is yours and is held under every spelling.
`purlis guard` on its own lists the rules, grouped by the file each one is in.
`purlis doctor`'s `handoff gate` row says whether that rule is in force where you are.

## What purlis reaches on its own, and how to stop it

purlis refreshes forge state in the background, so that nothing you look at waits on the
network: `purlis gl-refresh` asks `gh` or `glab` about every clone in the workspace, for
the open change and CI columns, and caches the answer in `.charter/cache/glstate.json`. The
app's own update check is the other request it makes unasked.

On an offline machine, in a CI job, or anywhere you would rather purlis did not reach your
forge unasked, switch the background refresh off:

```bash
export PURLIS_NO_BACKGROUND_CHECKS=1
```

**It is on whenever it holds more than whitespace, `0` and `false` included.** You are asking
purlis not to reach the network, and a word it did not recognise must not read as
permission. Unset the variable, or set it empty, to turn the refresh back on. A command you
run yourself, such as `purlis gl-refresh`, is not a background refresh and still reaches
the network.

Set it where every purlis process inherits it: your shell profile, or the job's
environment. A profile's `env` cannot carry it, because purlis refuses `CHARTER_*` names
there (see [control-plane.md](control-plane.md#what-is-refused-and-the-fix)).

## First control plane

```bash
mkdir my-control-plane && cd my-control-plane
purlis init --forge github --owner my-org
purlis doctor
purlis discover
purlis clone some-repo
```

Then open the directory in the app. A plane is untrusted until you open it there, and the app
is where its chats run.

`--forge` is `gitlab` (the default) or `github`; `--owner` is the GitLab group or GitHub
org/user whose repos this control plane tracks. Run at the top of an existing git repo,
`init` writes nothing at all and says so: a plane is a directory of its own and that repo
becomes its first clone. Make the plane beside it and adopt the repo in one command
(`purlis init --adopt ../<repo>`), because work happens in a workspace, never in the plane
root. To make that repo the plane instead, ask for it by name with `purlis
init --plane-is-this-repo`. That default is ADR 0035's, and purlis spec decision 27's.

`discover` and `clone` go through the forge's own CLI, `gh` for GitHub and `glab` for
GitLab, which nothing above installs and which must be authenticated.

A chat runs a harness program, and purlis does not install those either: `claude` for
Claude Code, `codex` for Codex and `opencode` for opencode.
`purlis harness list` shows the profiles this plane offers; [harnesses.md](harnesses.md) is
the rest.
