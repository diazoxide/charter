# Getting started with GitLab

This page takes you from nothing installed to a chat working in one of your GitLab repos, on
gitlab.com or on a self-managed GitLab. The same steps for GitHub are in
[Getting started with GitHub](github.md).

charter says **repo** for what GitLab calls a project. In charter, a **project** is something
else: the git repository where charter keeps your workspaces, personas, memory and settings.

## What you need

- A Mac with Apple Silicon, or a Linux machine on x86_64. There is no Windows build yet.
- At least one harness, the coding agent a chat runs: Claude Code (`claude`), Codex or
  opencode. Each one signs in with its own login, the first time a chat starts it.
- GitLab's own command-line tool, [`glab`](https://gitlab.com/gitlab-org/cli), signed in with
  `glab auth login` (add `--hostname <host>` for a self-managed GitLab). charter lists your
  repos, clones them and opens merge requests through `glab`, as you: it keeps no GitLab token
  of its own.

## Install

Download the newest release from the
[releases page](https://github.com/diazoxide/charter/releases/latest):

- **macOS:** `charter-macos-arm64.dmg`. Open it and drag **charter** to Applications. The build
  is not notarized, so macOS refuses the first launch from a download. Run this once, before
  the first launch:

  ```sh
  xattr -dr com.apple.quarantine /Applications/charter.app
  ```

  To use the `charter` command in a terminal, run **Install `charter` command in PATH** from the
  command palette.
- **Linux:** `charter-linux-x86_64.deb` (`sudo apt install ./charter-linux-x86_64.deb`, which also
  puts `charter` on your `PATH`), or the AppImage, `charter-linux-x86_64-appimage.AppImage`.

After that, charter updates itself: it checks the release's signature before it installs
anything, and it asks before it installs. A `.deb` install is updated by installing the next
`.deb`. [How charter updates itself](../updating.md) has the details.

## The first run

The first window says **Open a repo to start**. Pick a repo folder on your machine with
**Open a repo…**, or type its path. A clone of a GitLab repo works like any other.

charter then makes a project for you, on this machine only. Your repo is cloned into a
workspace named after it, and the first chat starts in that clone. Nothing is written into the
repo you picked.

Under **On this machine**, the window shows which harnesses it found and whether each is
signed in, and whether `gh` and `glab` are. A GitLab user needs only `glab`. If `glab` is
installed but not signed in, **Sign in to GitLab** opens a shell tab running `glab auth login`.
For a self-managed GitLab, add `--hostname <host>` in that tab before you press Enter, because
the first run checks only `gitlab.com`.

**Open an existing project instead** opens a project you already have, for example one a
teammate shared as a GitLab repo.

## Working with your GitLab repos

The project charter makes on the first run tracks GitHub. To list and clone the repos in your
GitLab group:

1. Open **Project settings…** and, under **Forges**, set the first forge's **kind** to `gitlab`
   and its **owner** to your group, such as `my-group`. Set **host** to your GitLab's host, such
   as `gitlab.example.com`, or leave it empty for gitlab.com. In `charter.toml` a forge block
   may say `group` instead of `owner`, and `group` wins when it has both.
2. Open **New workspace…**. Its repo picker lists the repos your own `glab` login is a member
   of under that group, asked when the dialog opens; **Refresh** asks again. Pick the ones this
   piece of work needs. A workspace with no repos is fine too.
3. charter clones each one over HTTPS from its default branch, and sets the clone up to fetch
   and push with `glab auth git-credential`, so git uses your `glab` login.
4. Start a chat in the workspace and choose its harness.

If the picker cannot ask GitLab, it says why in `glab`'s words, for example that `glab` is not
signed in for that host, and tells you the command to run.

From a terminal, `charter init --owner <group>` makes a project in the current directory that
tracks that group: GitLab is `charter init`'s default forge. Add `--host <host>` for a
self-managed GitLab.

## Saving your project to GitLab

A project is saved with git, as far as its **mode** says, and each mode includes the ones
before it:

| Mode | A save… |
| --- | --- |
| `off` | is never committed |
| `commit` | commits on this machine |
| `push` | also pushes to the project's branch |
| `pr` | pushes to a save branch and opens or updates one merge request |
| `pr-merge` | also sets that merge request to merge when its pipeline succeeds |

charter sets that only while a pipeline is running or waiting to run for the merge request.
Until a mode is set, the **Saving** view asks once, before anything is pushed. The project from
the first run has no remote, so only `off` and `commit` have anywhere to go until you push it to
a GitLab repo of its own. The keys are in [the project format](../plane-format.md#chartertoml).

## The five words

charter has five concepts, and every other word belongs to one of them
([ADR 0072](../adr/0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md)):

- **Project:** the git repository that holds your workspaces, personas, memory and settings. A
  code repo is never a project, on any forge.
- **Workspace:** a named piece of work, with its own `workspace.md`, memory, todos and repos.
- **Chat:** one conversation with an agent, in a tab.
- **Persona:** a role a chat can take, with its own instructions, memory and vault.
- **Memory:** what charter keeps so that the next chat knows what earlier ones learned.
