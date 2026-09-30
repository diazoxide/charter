#!/usr/bin/env bash
# Linux cold start against the spec's 2 s limit, on the desktops charter-app#24 is about.
# Run from the repository root, after `npx tauri build --debug --no-bundle`:
#
#   tools/coldstart-linux.sh bare     # X with no window manager, on the session bus as found
#   tools/coldstart-linux.sh i3       # i3 under X11, on the session bus as found
#   tools/coldstart-linux.sh i3-nobus # i3 under X11 with no session bus, as `startx` into i3
#
# On a GitHub runner "the session bus as found" is charter-app#24's machine: a bus that is up,
# with a desktop portal it can activate and that cannot start. Each case launches the app
# five times and fails when any launch after the first is past the limit (`tools/bench.mjs`).
set -euo pipefail

case="${1:?which case: bare, i3 or i3-nobus}"

if [[ ! -f tools/bench.mjs || ! -f Cargo.toml ]]; then
  echo "coldstart-linux.sh: run it from the repository root (tools/bench.mjs is not here)" >&2
  exit 2
fi

app="${APP:-target/debug/charter-app}"
limit="${LIMIT_MS:-2000}"
bench=(node tools/bench.mjs --skip-build --only coldstart --app "$app" --cold-starts 5 --limit "$limit")

if [[ -z "${DISPLAY:-}" ]]; then
  # A display of its own for the run, and this script again inside it.
  exec xvfb-run -a -s "-screen 0 1280x800x24" "$0" "$@"
fi

# What this run made, and the window manager it started: gone however the run ends.
scratch="$(mktemp -d)"
wm=""
cleanup() {
  if [[ -n "$wm" ]]; then
    kill "$wm" 2>/dev/null || true
    wait "$wm" 2>/dev/null || true
  fi
  rm -rf "$scratch"
}
trap cleanup EXIT

with_i3() {
  local config="$scratch/i3.config"
  # i3's packaged config starts its first-run wizard; one line is all it needs.
  printf 'font pango:monospace 8\n' >"$config"
  i3 -c "$config" >"$scratch/i3.log" 2>&1 &
  wm=$!
  # i3 owns the root window once it is managing it; nothing is measured before that, and a
  # case that says i3 and measured no i3 would be a green run about the wrong desktop.
  local up=""
  for _ in $(seq 50); do
    if i3 --get-socketpath >/dev/null 2>&1; then
      up=yes
      break
    fi
    kill -0 "$wm" 2>/dev/null || break
    sleep 0.1
  done
  if [[ -z "$up" ]]; then
    echo "coldstart-linux.sh: i3 did not come up (it exited, or took more than 5 s); nothing was measured" >&2
    cat "$scratch/i3.log" >&2 || true
    exit 1
  fi
  "$@"
}

case "$case" in
bare) "${bench[@]}" ;;
i3) with_i3 "${bench[@]}" ;;
i3-nobus)
  # No address and a runtime directory with no `bus` in it: the standard lookup finds nothing.
  # GIO would autolaunch a bus on the X display, and the app now looks for that one the same
  # way and starts on it (`portal.rs`).
  mkdir "$scratch/runtime"
  chmod 700 "$scratch/runtime"
  with_i3 env -u DBUS_SESSION_BUS_ADDRESS XDG_RUNTIME_DIR="$scratch/runtime" "${bench[@]}"
  ;;
*)
  echo "unknown case: $case" >&2
  exit 2
  ;;
esac
