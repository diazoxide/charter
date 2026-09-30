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
# five times and fails when the p50 to its first frame is past the limit (`tools/bench.mjs`).
set -euo pipefail

case="${1:?which case: bare, i3 or i3-nobus}"
app="${APP:-target/debug/charter-app}"
limit="${LIMIT_MS:-2000}"
bench=(node tools/bench.mjs --skip-build --only coldstart --app "$app" --cold-starts 5 --limit "$limit")

if [[ -z "${DISPLAY:-}" ]]; then
  # A display of its own for the run, and this script again inside it.
  exec xvfb-run -a -s "-screen 0 1280x800x24" "$0" "$@"
fi

with_i3() {
  local config
  config="$(mktemp)"
  # i3's packaged config starts its first-run wizard; one line is all it needs.
  printf 'font pango:monospace 8\n' >"$config"
  i3 -c "$config" >/dev/null 2>&1 &
  local wm=$!
  # i3 owns the root window once it is managing it; nothing is measured before that.
  for _ in $(seq 50); do
    i3 --get-socketpath >/dev/null 2>&1 && break
    sleep 0.1
  done
  local status=0
  "$@" || status=$?
  kill "$wm" 2>/dev/null || true
  return $status
}

case "$case" in
bare) "${bench[@]}" ;;
i3) with_i3 "${bench[@]}" ;;
i3-nobus)
  # No address and a runtime directory with no `bus` in it: the standard lookup finds nothing,
  # which is what GIO answers by autolaunching a bus of its own on X11.
  runtime="$(mktemp -d)"
  with_i3 env -u DBUS_SESSION_BUS_ADDRESS XDG_RUNTIME_DIR="$runtime" "${bench[@]}"
  ;;
*)
  echo "unknown case: $case" >&2
  exit 2
  ;;
esac
