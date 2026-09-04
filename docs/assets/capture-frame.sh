#!/usr/bin/env bash
# Capture charter's FRAME — the whole composed surface, not one renderer's output.
#
#   ./capture-frame.sh <scratch-dir> > frame.ansi
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
set -uo pipefail

DIR="${1:?usage: capture-frame.sh <scratch-dir>}"
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

rm -rf "$DIR"; mkdir -p "$DIR"
DIR="$(cd "$DIR" && pwd -P)"

# ── this tree's charter, in a python that can find it from anywhere ───────────
# `capture-demo.sh` gets away with a `charter` shim on `$PATH` because every command it
# captures is a charter that this script starts. A frame is not: charter hands tmux an
# argv of its own for each panel (`frame/layout.panel_command` → `sys.executable -P -m
# charter panel …`), tmux starts those panes from the SERVER's environment — captured
# whenever that shared server first started, possibly days ago and by somebody else's
# charter — and `-P` means the interpreter will not look in the cwd either. Measured: with
# a `$PATH` shim alone, all four panels died at once with `No module named charter` and
# the capture came back as four `Pane is dead (status 1)` messages.
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

# ── the outer terminal ────────────────────────────────────────────────────────
# Its own socket, named for this process, so a capture never touches — and never waits on
# — whatever tmux the operator is sitting in.
OUTER="charter-capture-frame-$$"
cleanup() {
  tmux -L "$OUTER" kill-server >/dev/null 2>&1 || true
  # The socket FILE outlives the server it named, and a capture that leaves one behind
  # every run is the accumulation `tests/_tmuxreap.py` was written to clean up after
  # (#564: 497 stale files). tmux resolves its own socket directory, which on macOS turns
  # `/tmp` into `/private/tmp` — asked here rather than spelled, for the same reason.
  rm -f "$(python3 -c 'import os,sys; print(os.path.realpath(os.path.join(os.environ.get("TMUX_TMPDIR") or "/tmp", "tmux-%d" % os.getuid(), sys.argv[1])))' "$OUTER")"
  # charter's own server is SHARED between every frame on the machine (`commands_frame
  # .SOCKET`), so this kills the one session this script created and never the server.
  # The name is read back from tmux rather than assumed: charter suffixes a session whose
  # name is already taken, so the session this launch produced is the one that was not
  # there a moment ago.
  if [ -n "${MINE:-}" ]; then
    tmux -L charter kill-session -t "$MINE" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

sessions() { tmux -L charter list-sessions -F '#{session_name}' 2>/dev/null | sort; }
BEFORE="$(sessions)"

# `env -u TMUX -u TMUX_PANE`: inside the outer pane those name the OUTER server, and
# charter reads them to decide it is inside the operator's tmux and should open a window
# there instead of its own session (`commands_frame._launch_in_operator_tmux`). The frame
# would then be composed into a window this script is not looking at.
#
# `--workspace` aims the launch and skips the picker in one flag — without it the first
# thing on the screen is charter asking which workspace, which is a real surface and not
# this one.
#
# The harness command holds the pane open for as long as the capture needs and then exits,
# which is what tears the frame down: charter kills the session when its harness returns,
# so a capture that dies half-way leaves nothing running behind it either.
tmux -L "$OUTER" new-session -d -s cap -x "$COLS" -y "$ROWS" -- \
  env -u TMUX -u TMUX_PANE sh -c \
  "cd '$PLANE' && exec charter frame --workspace billing-migration -- sh -c 'charter status; sleep 90'" \
  >&2 || exit 1

# ── wait for the paint, then read the screen ──────────────────────────────────
# Polled rather than slept: a panel is a process that has to start, import charter, scan
# the plane and paint, and the six of them do it concurrently. The condition is the LAST
# thing to appear — the repo table, which waits on `gather` — so a screen holding it is a
# screen holding everything above it too.
SHOT=""
DEADLINE=$(( $(date +%s) + 60 ))
while [ "$(date +%s)" -lt "$DEADLINE" ]; do
  SHOT="$(tmux -L "$OUTER" capture-pane -p -e -N -t cap 2>/dev/null)"
  case "$SHOT" in
    *"payments-service"*"F2 palette"*) break ;;
  esac
  sleep 0.5
done

MINE="$(comm -13 <(printf '%s\n' "$BEFORE") <(sessions) | head -1)"

case "$SHOT" in
  *"F2 palette"*) : ;;
  *) echo "capture-frame.sh: the frame never finished painting — captured nothing." >&2
     printf '%s\n' "$SHOT" >&2
     exit 1 ;;
esac

# The scratch directory is the operator's machine and nobody else's business. It does not
# appear on any surface the frame draws today; this is the same belt-and-braces
# substitution `capture-demo.sh` runs, so a renderer that starts printing a path cannot
# publish one without anybody noticing.
printf '%s\n' "$SHOT" | python3 -c '
import sys
real = sys.argv[1]
sys.stdout.write(sys.stdin.read().replace(real, "~/my-control-plane"))
' "$PLANE"
