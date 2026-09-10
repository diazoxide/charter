#!/usr/bin/env bash
# Capture charter's FRAME — the whole composed surface, not one renderer's output.
#
#   ./capture-frame.sh <scratch-dir>          > frame.ansi       # frame.svg
#   ./capture-frame.sh <scratch-dir> --full   > frame-full.ansi  # frame-full.svg
#
# The frame is tmux's composition of charter's panels around a harness pane, so no
# renderer's stdout is the picture: the tab strips, the repo table and the persona column
# are five separate processes painting five rectangles, and the pane borders between them
# belong to none of them. `ptyrun.py` cannot reach that — it hands ONE command ONE pty and
# reads back what that command wrote. So this script renders the frame inside a SECOND
# tmux and captures the outer one:
#
#     outer tmux ── one pane ── `charter frame` ── attaches ── charter's own tmux server
#          │                                                        │
#          └── `capture-pane -e -N` on that pane == the whole frame's screen ────┘
#
# That is the same nesting `tests/test_a_planes_frame_really_reads_that_way.py` uses to
# measure pane borders, and for the identical reason: a border and a pane's default
# colours are composed by tmux for its CLIENT, so the only thing that can see them is
# another terminal. What comes back is escapes and all, which `ansi2svg.py` turns into
# the SVG.
#
# Everything on that screen is real. The panels are charter's own renderers reading a real
# plane; the repo rows are git's answers about real repositories; the harness pane runs
# `charter status`, which is a real command's real output and is SAID to be that in
# `docs/assets/README.md` — charter draws nothing in that rectangle (ADR 0018), so what
# goes there is whatever you ran, and a capture cannot run an agent.
#
# `--full` is the same frame on a plane in use rather than one at rest: four workspaces,
# three chats in the one on screen and a fourth in another, and a turn in flight on one of
# them. Without it the chat strip holds one chat and the workspace strip two tabs, which is
# the arrangement `docs/frame.md` walks through and too little to show what either strip is
# for — so the README's first picture is `--full`, and `frame.svg` stays the plain one.
set -uo pipefail

DIR="${1:-}"
# The scratch directory comes first, and this script `rm -rf`s it. `capture-frame.sh --full`
# with the directory left out would otherwise hand `--full` to `rm` and `mkdir` as a flag
# and carry on with no directory at all. No argument at all gets the same refusal and the
# same status: `${1:?}` exits with the shell's status for a failed expansion, not 2.
case "$DIR" in
  ""|-*) echo "usage: capture-frame.sh <scratch-dir> [--full] — the scratch directory comes first, and is emptied" >&2
         exit 2 ;;
esac
FULL=0
case "${2:-}" in
  "") ;;
  --full) FULL=1 ;;
  *) echo "usage: capture-frame.sh <scratch-dir> [--full]" >&2; exit 2 ;;
esac
COLS="${COLUMNS:-150}"
ROWS="${LINES:-30}"

# Resolved BEFORE anything moves, for `capture-demo.sh`'s reason exactly: `BASH_SOURCE[0]`
# is usually relative, and a HERE computed after a `cd` is a HERE that finds none of its
# siblings.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_ROOT="$(cd "$HERE/../.." && pwd)"

command -v tmux >/dev/null 2>&1 || {
  echo "capture-frame.sh: no tmux on this machine, and the frame IS tmux — nothing to capture." >&2
  exit 2
}

# The caller's charter identity is not the demo plane's. `demo-plane.sh` scrubs it for the
# plane it builds, but the launches below run in THIS shell, and `_frame_identity_env` hands
# a launcher's `CHARTER_*` to every pane it starts — so a capture run from inside a charter
# chat would carry that chat's pins into the demo plane's frame. `TMUX` and `TMUX_PANE` go
# with them: every tmux call here names its server outright, and none may fall back on the
# one this shell happens to be sitting in.
unset $(env | grep -o '^CHARTER_[A-Z_]*' || true) 2>/dev/null || true
unset TMUX TMUX_PANE

rm -rf "$DIR"; mkdir -p "$DIR"
DIR="$(cd "$DIR" && pwd -P)"
[ -n "$DIR" ] || { echo "capture-frame.sh: could not make the scratch directory — captured nothing." >&2; exit 2; }

# ── a tmux of its own — both of them ──────────────────────────────────────────
# charter's frame server is a module constant (`commands_frame.SOCKET` = `charter`) with no
# override, and every frame on the machine shares it. Launched straight onto it, this
# capture created its session on the operator's live server, sourced the frame's config
# there — where key tables are server-wide — and then had to pick its one session back out
# of theirs to kill it.
#
# tmux looks a `-L <name>` up under `$TMUX_TMPDIR` (`frame/tmuxctl.socket_path` spells the
# same rule), and charter hands its own environment to every pane, panel and hook it starts
# (`_frame_env` is `dict(os.environ, …)`). So with `$TMUX_TMPDIR` on a directory this run
# made, charter's `-L charter` IS a private server, and the capture can end it whole.
#
# Under `/tmp` rather than `<scratch-dir>` because a socket path has to fit a `sockaddr_un`
# — 104 bytes on macOS, 108 on Linux — after tmux resolves the directory. Measured: a
# scratch directory inside Claude Code's per-session temp path put the socket at 146 bytes,
# and tmux refused with `File name too long`.
SOCKDIR="$(mktemp -d /tmp/charter-capture.XXXXXX)" || exit 1
SOCKDIR="$(cd "$SOCKDIR" && pwd -P)"
export TMUX_TMPDIR="$SOCKDIR"
mkdir -m 700 "$SOCKDIR/tmux-$(id -u)"
OUTER="$SOCKDIR/tmux-$(id -u)/outer"
INNER="$SOCKDIR/tmux-$(id -u)/charter"

cleanup() {
  # Both servers live in $SOCKDIR, which this run made and nothing else names, so ending
  # them whole ends nothing anybody else is looking at. The outer one first: its panes are
  # the launchers, and a launcher that loses its terminal returns.
  tmux -S "$OUTER" kill-server >/dev/null 2>&1 || true
  tmux -S "$INNER" kill-server >/dev/null 2>&1 || true
  rm -rf "$SOCKDIR"
}
trap cleanup EXIT

# ── this tree's charter, in a python that can find it from anywhere ───────────
# `capture-demo.sh` gets away with a `charter` shim on `$PATH` because every command it
# captures is a charter that this script starts. A frame is not: charter hands tmux an
# argv of its own for each panel (`frame/layout.panel_command` → `sys.executable -P -m
# charter panel …`), tmux starts those panes from the SERVER's environment, and `-P` means
# the interpreter will not look in the cwd either. Measured, back when this ran on the
# shared server: with a `$PATH` shim alone, all four panels died at once with `No module
# named charter` and the capture came back as four `Pane is dead (status 1)` messages.
#
# So the tree is put somewhere an interpreter finds it with no environment at all: a venv
# whose `site-packages` holds one `.pth` line naming this checkout. `--without-pip`
# because nothing is being installed and nothing is fetched — this is stdlib `venv` and a
# text file, so regenerating still needs no toolchain and no network. The `charter` in its
# `bin/` is what `demo-plane.sh` below runs, so the plane and the render are built by one
# charter: the one in this tree, which is the property `capture-demo.sh`'s shim exists for.
VENV="$DIR/.venv"
python3 -m venv --without-pip "$VENV" >&2 || exit 1
SITE="$(echo "$VENV"/lib/python*/site-packages)"
printf '%s\n' "$SRC_ROOT" > "$SITE/charter-src.pth"
cat > "$VENV/bin/charter" <<SH
#!/bin/sh
exec "$VENV/bin/python3" -P -m charter "\$@"
SH
chmod +x "$VENV/bin/charter"
PATH="$VENV/bin:$PATH"
export PATH

PLANE="$DIR/plane"
"$HERE/demo-plane.sh" "$PLANE" >&2 || exit 1

# The newer-charter check's cooldown lock, the one a real machine already holds. Since #938
# every gather and every SessionStart may fork `charter _version-check`, a GET to PyPI, and a
# plane made a moment ago has no lock to stop it — `tests._isolation.no_update_check_in`
# measured one suite run forking 18 of them. This touches the file `update.maybe_spawn`
# touches before it forks, so the child's own throttle answers "attempted within the hour"
# and the capture stays off the network, which is what this recipe has always claimed.
( cd "$PLANE" && python3 -P -c '
from charter import config, update
lock = update._lock_file()
config.private_mkdir(lock.parent)
config.touch_for(lock)
' ) || exit 1

# ── launching a chat ──────────────────────────────────────────────────────────
# One chat per call: `charter frame` in a window of the outer terminal, so every launch is
# somebody's terminal — charter attaches the launch that opened a chat, and a launch with
# no terminal at all runs its command bare instead of framing it.
#
# `env -u TMUX -u TMUX_PANE`: inside the outer pane those name the OUTER server, and
# charter reads them to decide it is inside the operator's tmux and should open a window
# there instead of its own session (`commands_frame._launch_in_operator_tmux`). The frame
# would then be composed into a window this script is not looking at.
#
# `--workspace` aims the launch and skips the picker in one flag — without it the first
# thing on the screen is charter asking which workspace, which is a real surface and not
# this one. And a launch that names a command opens a chat whether or not a terminal is
# already attached to that workspace: `cmd_launch` answers with a focus only a launch that
# asked for nothing but the workspace, so three launches into one workspace are three
# chats, the last of them on screen.
#
# The harness command holds the pane open for as long as the capture needs and then exits,
# which is what tears the frame down: charter kills the session when its harness returns,
# so a capture that dies half-way leaves nothing running behind it either.
launch() { # <outer-window> <workspace>
  local run="cd '$PLANE' && exec charter frame --workspace $2 -- sh -c 'charter status; sleep 120'"
  if tmux -S "$OUTER" has-session -t cap 2>/dev/null; then
    tmux -S "$OUTER" new-window -d -t cap: -n "$1" -- env -u TMUX -u TMUX_PANE sh -c "$run" >&2
  else
    tmux -S "$OUTER" new-session -d -s cap -n "$1" -x "$COLS" -y "$ROWS" -- \
      env -u TMUX -u TMUX_PANE sh -c "$run" >&2
  fi
}

# Every chat window carries `@charter_chat` (`commands_frame._CHAT_OPTION`), so the private
# server answers how many chats exist without this script guessing at an ordinal.
chat_ids() { tmux -S "$INNER" list-windows -a -F '#{@charter_chat}' 2>/dev/null | grep . | sort; }
wait_for_chats() { # <count>
  local deadline=$(( $(date +%s) + 60 ))
  until [ "$(chat_ids | grep -c .)" -ge "$1" ]; do
    if [ "$(date +%s)" -ge "$deadline" ]; then
      echo "capture-frame.sh: chat $1 never opened — captured nothing." >&2
      exit 1
    fi
    sleep 0.5
  done
}

WORKING=""
if [ "$FULL" = 1 ]; then
  # ── a plane in use ──────────────────────────────────────────────────────────
  # Two more workspaces, so the workspace strip is a row of tabs rather than a pair, and a
  # chat in `checkout-redesign`, so a tab other than yours carries a count. Here and not in
  # `demo-plane.sh`, because that plane is also what `statusline.svg`, `personas.svg` and
  # the plain `frame.svg` are taken against, and none of those is about the strips.
  ( cd "$PLANE" \
    && charter workspace create platform-upgrade \
         --vision "Move every service onto the new base image before the old one stops getting patches." >/dev/null \
    && charter workspace create release-2-4 \
         --vision "Cut 2.4 with the ledger cutover behind a flag." >/dev/null ) || exit 1

  launch elsewhere checkout-redesign; wait_for_chats 1
  launch first     billing-migration; wait_for_chats 2
  launch second    billing-migration; wait_for_chats 3

  # A turn in flight on the first chat of the workspace on screen. `inflight.turn_begin` is
  # the call the `UserPromptSubmit` hook makes (`hooks._turn_begin`), and this writes exactly
  # its file: an empty one named for the chat, under the plane's `chat-turns/`. Made here
  # rather than by the hook because the hook marks only a chat whose `$CHARTER_HARNESS` is
  # Claude Code, and a capture cannot run an agent. The mark stands for ten minutes with no
  # tool call (`inflight.TURN_STALE_SECONDS`), far longer than this takes.
  WORKING="$(chat_ids | grep '^billing-migration\.' | head -1)"
  [ -n "$WORKING" ] || { echo "capture-frame.sh: no billing-migration chat to mark working." >&2; exit 1; }
  ( cd "$PLANE" && python3 -P -c 'import sys; from charter import inflight; inflight.turn_begin(sys.argv[1])' \
      "$WORKING" ) || exit 1
fi

launch shot billing-migration

# ── wait for the paint, then read the screen ──────────────────────────────────
# Polled rather than slept: a panel is a process that has to start, import charter, scan
# the plane and paint, and the six of them do it concurrently. The condition is the LAST
# thing to appear — the repo table, which waits on `gather` — so a screen holding it is a
# screen holding everything above it too. `--full` also waits for what it exists to show:
# the working chat's name on the strip, the last workspace tab, and the spinner in front of
# that chat on its `✢` frame. Which frame is not taste. `slots.TAB_SPINNER` turns through
# `✢✶✻✶`, and the chat on screen is marked `*`. Rendered at the width GitHub shows the
# README (an `<img>` 830 px wide, headless Chrome at 1x and 2x), `✻` and `✶` both read as
# that `*`, so a still taken on either shows a strip with two current chats; `✢` reads as a
# `+`. All three are East-Asian Neutral and one cell wide, so the choice moves no column.
has() { case "$SHOT" in *"$1"*) return 0 ;; esac; return 1; }
painted() {
  case "$SHOT" in *"payments-service"*"F2 palette"*) : ;; *) return 1 ;; esac
  [ "$FULL" = 1 ] || return 0
  has "$WORKING" && has "release-2-4" && has "✢"
}

SHOT=""
DEADLINE=$(( $(date +%s) + 60 ))
while [ "$(date +%s)" -lt "$DEADLINE" ]; do
  SHOT="$(tmux -S "$OUTER" capture-pane -p -e -N -t cap:shot 2>/dev/null)"
  painted && break
  sleep 0.5
done

if ! painted; then
  echo "capture-frame.sh: the frame never finished painting — captured nothing." >&2
  printf '%s\n' "$SHOT" >&2
  exit 1
fi

# The scratch directory is the operator's machine and nobody else's business. It does not
# appear on any surface the frame draws today; this is the same belt-and-braces
# substitution `capture-demo.sh` runs, so a renderer that starts printing a path cannot
# publish one without anybody noticing.
printf '%s\n' "$SHOT" | python3 -c '
import sys
real = sys.argv[1]
sys.stdout.write(sys.stdin.read().replace(real, "~/my-control-plane"))
' "$PLANE"
