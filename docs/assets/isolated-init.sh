# Sourced, never run: every script in docs/assets that runs `charter init` goes through this.
#
#   . "$HERE/isolated-init.sh"
#   isolated_charter_init [runner …] -- <charter init arguments …>
#
# runs `[runner …] <charter> init <arguments …>` with no `claude` on PATH and with
# `$XDG_CONFIG_HOME` on a directory of its own, removed afterwards. The status is init's.
#
# **Why both.** `charter init` installs two things outside the plane it creates. With `claude`
# on PATH it runs `claude plugin marketplace add` and `claude plugin install charter@charter
# --scope project` — a fetch from GitHub and an entry in the operator's own
# `~/.claude/plugins/installed_plugins.json` naming the throwaway directory. Every capture that
# built a plane that way left one behind, the frame captures for #958 included. And init
# writes opencode's plugin under `$XDG_CONFIG_HOME`, which is the operator's `~/.config`
# unless something says otherwise.
#
# **Why PATH and not HOME.** `plugincache.available` is `shutil.which("claude")`, so to init a
# PATH with no `claude` on it is an ordinary opencode or Codex machine: no install, and no
# plugin row. An isolated HOME would leave `claude` findable, and init would run the same
# install against an empty Claude Code config — still a fetch, and still a real install.
#
# **Why one file.** `demo-plane.sh` was isolated first and `capture-demo.sh` was not, which is
# how the same leak outlived its own fix. A script here that runs `charter init` without this
# function is that defect again.
#
# `charter` is resolved to a full path before PATH is narrowed, because it may share a
# directory with `claude`. `python3`, `git` and `tmux` keep their directories unless one of
# those also holds a `claude`.
isolated_charter_init() {
  local runner=() charter_bin init_path init_xdg rc
  while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do
    runner+=("$1")
    shift
  done
  [ "$#" -gt 0 ] && shift
  charter_bin="$(command -v charter)" || {
    echo "isolated_charter_init: no charter on PATH" >&2
    return 127
  }
  init_path="$(python3 -c '
import os
print(os.pathsep.join(d for d in os.environ.get("PATH", "").split(os.pathsep)
                      if d and not os.access(os.path.join(d, "claude"), os.X_OK)))
')" || return 1
  init_xdg="$(mktemp -d)" || return 1
  # `${runner[@]+…}` rather than `"${runner[@]}"`: bash 3.2, which is macOS's `/bin/bash`,
  # calls an empty array unbound under `set -u`, and demo-plane.sh runs with `set -u`.
  env PATH="$init_path" XDG_CONFIG_HOME="$init_xdg" ${runner[@]+"${runner[@]}"} \
    "$charter_bin" init "$@"
  rc=$?
  rm -rf "$init_xdg"
  return "$rc"
}
