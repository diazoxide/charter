#!/usr/bin/env bash
# The tmux rig every measurement in the report was taken on. Real tmux, real panes, real
# `charter panel` processes — nothing here fakes a terminal.
#
# **One cleanup, killing before unlinking.** #590 took this machine from 14 leaked tmux
# servers to 0 and the discipline that got it there is `tests/_tmuxreap.py`'s: a server
# that is still listening is killed BEFORE its socket file is removed, because
# unlink-then-kill points `kill-server` at a path with no server on it and leaves the real
# one running. The socket is named `charter-<slug>-<pid>` so that `tests/_tmuxreap.py`
# will also reap it if this script is killed before its trap can run.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV="${VENV:?set VENV to the venv root of the interpreter under test}"
WT="${WT:?set WT to the charter worktree}"
CHARTER="$VENV/bin/charter"
PY="$VENV/bin/python"
SOCK="charter-textualexp-$$"
FID="${FID:-exp-$$}"

export CHARTER_ROOT="$WT"
export CHARTER_SESSION_ID="$FID"
export CHARTER_WORKSPACE=""
export CHARTER_PERSONA=""
export CHARTER_HARNESS="claude"

tm() { tmux -L "$SOCK" "$@"; }

cleanup() {
  # Kill first, unlink second — see the header. Both in one function, and the trap is the
  # only caller, so there is no path out of this script that skips half of it.
  tmux -L "$SOCK" kill-server 2>/dev/null || true
  rm -f "/tmp/tmux-$(id -u)/$SOCK" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

seed() { "$PY" "$HERE/seed.py" "$FID" "${1:-failed}" >/dev/null; }

# A harness stand-in: prints numbered lines so scrollback can be measured, then sleeps.
# `charter frame` would launch Claude Code here; the experiment needs a pane that is
# reproducible and that charter's launcher treats identically.
HARNESS='for i in $(seq 1 200); do echo "harness line $i"; done; exec bash --norc --noprofile -i'

start_session() {
  tm new-session -d -s exp -x "${COLS:-150}" -y "${ROWS:-40}" -- sh -c "$HARNESS"
  tm set -t exp mouse off          # charter's default (instance.FRAME_FIELDS)
  tm set -t exp history-limit 50000
  tm set -t exp status off
}

split_panel() {   # split_panel <component> <rows>
  tm split-window -t exp:0.0 -v -l "$2" -P -F '#{pane_id}' \
     -- "$CHARTER" panel "$1" --session "$FID"
}
