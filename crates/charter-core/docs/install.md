# Install

**charter is one desktop app, and the `charter` command ships inside it.** You install the
app; there is no separate package to install for the command, and no package manager is
involved at any point.

Builds are published on the releases page of
[diazoxide/charter](https://github.com/diazoxide/charter/releases): a macOS `.app` in
a `.dmg`, and a Linux `.deb` and AppImage. There is no Windows build yet.

## 1. Pick a channel

| Channel | Built from | Where it is published |
| --- | --- | --- |
| **stable** (the default) | a `v*` tag | the latest release |
| **dev** | every green `main` | the `dev` prerelease, whose files are replaced on each build |

Download from the channel you want. After the first install the app moves itself (see
*Updates*, below), and one command switches a machine from one channel to the other:

```bash
charter update --channel dev      # or: --channel stable
```

## 2. The first launch on macOS

Builds are signed, either with a Developer ID or ad hoc, and they are **not notarized**. A
browser marks what it downloads as quarantined, so macOS refuses the first launch once. The
one instruction that works for both kinds of signature, before that first launch:

```sh
xattr -dr com.apple.quarantine /Applications/charter.app
```

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

`charter update` on the command line does not install anything. It is the half of updating
that is about a plane's content: what the versions it skipped brought, and what this plane
has not taken up. `--to` and `--bump` are refused by name, and `charter version` says what
this charter is and what the plane pins.

## 4. The `charter` command

The bundle carries the `charter` binary beside the app's own executable
(`charter.app/Contents/MacOS/charter` on macOS). The app runs **that** copy for every hook
and every refresh, by its absolute path, and deliberately does not look at `PATH`: a
`charter` found on `PATH` could be an older install, or a different program, answering the
same events differently.

The app does not put the binary on your shell's `PATH`. A chat the app starts is different:
its `PATH` gets the bundle's directory added, so `charter` answers inside a chat. To use it
from your own shell, call it by its path in the bundle, or link it into a directory that is on
your `PATH`.

## 5. The Claude Code plugin

The app ships a Claude Code plugin inside its bundle and loads it into each chat it starts,
with `claude --plugin-dir`. There is nothing to install into Claude Code, no marketplace to
add and no per-project install to keep in step: the plugin and the binary its hooks call come
from the same build, so they cannot drift apart.

The plugin is called `charter-app`. It carries every hook charter answers — the ones that
report a chat's state and the Bash guard, described in [hooks.md](hooks.md) — and the
`handoff`, `working-in-a-clone` and `update` skills, which reach the model as
`charter-app:<skill>`. It lives in `Contents/Resources/plugin` on macOS and
`/usr/lib/charter/plugin` on Linux. A chat the app starts also turns a plugin named
`charter@charter` off for itself, so a plane whose settings enable an older charter plugin for
your own terminal sessions does not give an app chat two sets of hooks; a `claude` you run in
a terminal is untouched. A Codex chat gets charter's state hooks and Bash guard as `-c` flags on
its command line, and Codex asks once to trust them. How each harness is armed is in
[harnesses.md](harnesses.md#per-profile--armed-at-launch).

`charter doctor --fix` is refused in this version, and `charter doctor` reports without
it.

## What charter reaches on its own, and how to stop it

charter refreshes forge state in the background, so that nothing you look at waits on the
network: `charter gl-refresh` asks `gh` or `glab` about every clone in the workspace, for
the open change and CI columns, and caches the answer in `.charter/cache/glstate.json`. The
app's own update check is the other request it makes unasked.

On an offline machine, in a CI job, or anywhere you would rather charter did not reach your
forge unasked, switch the background refresh off:

```bash
export CHARTER_NO_BACKGROUND_CHECKS=1
```

**It is on whenever it holds more than whitespace, `0` and `false` included.** You are asking
charter not to reach the network, and a word it did not recognise must not read as
permission. Unset the variable, or set it empty, to turn the refresh back on. A command you
run yourself, such as `charter gl-refresh`, is not a background refresh and still reaches
the network.

Set it where every charter process inherits it: your shell profile, or the job's
environment. A profile's `env` cannot carry it, because charter refuses `CHARTER_*` names
there (see [control-plane.md](control-plane.md#what-is-refused-and-the-fix)).

## First control plane

```bash
mkdir my-control-plane && cd my-control-plane
charter init --forge github --owner my-org
charter doctor
charter discover
charter clone some-repo
```

Then open the directory in the app. A plane is untrusted until you open it there, and the app
is where its chats run.

`--forge` is `gitlab` (the default) or `github`; `--owner` is the GitLab group or GitHub
org/user whose repos this control plane tracks. Run at the top of an existing git repo,
`init` writes nothing at all and says so: a plane is a directory of its own and that repo
becomes its first clone. Make the plane beside it and adopt the repo in one command
(`charter init --adopt ../<repo>`), because work happens in a workspace, never in the plane
root. To make that repo the plane instead, ask for it by name with `charter
init --plane-is-this-repo`. That default is ADR 0035's, and charter-app spec decision 27's.

`discover` and `clone` go through the forge's own CLI, `gh` for GitHub and `glab` for
GitLab, which nothing above installs and which must be authenticated.

A chat runs a harness program, and charter does not install those either: `claude` for
Claude Code and `codex` for Codex. Starting opencode chats is not in this version yet.
`charter harness list` shows the profiles this plane offers; [harnesses.md](harnesses.md) is
the rest.
