#!/usr/bin/env bash
# Linux cold start against the spec's 2 s limit, on the desktops charter-app#24 is about.
# After `npx tauri build --debug --no-bundle`, from anywhere:
#
#   tools/coldstart-linux.sh bare     # X with no window manager, on the session bus as found
#   tools/coldstart-linux.sh i3       # i3 under X11, on the session bus as found
#   tools/coldstart-linux.sh i3-nobus # i3 under X11 with no session bus, as `startx` into i3
#
# On a GitHub runner "the session bus as found" is charter-app#24's machine: a bus that is up,
# with a desktop portal it can activate and that cannot start.
#
# Each case launches the app once, discarded, and then five times, each on a fresh HOME and XDG
# directories. It fails when the median of the five is past the 2 s limit, or two or more of them
# are past a 2.5 s ceiling; one launch past it is reported, not gated (V39, ADR 0086 as amended;
# `tools/coldstart-gate.mjs`). Not "every one under 2 s": this is a debug build on a
# shared runner, where one launch in five went 63 ms over (main, run 36789430687) while the
# others sat near 1.35 s. A release build would be the one users run, but it costs a full
# `lto = true`, `codegen-units = 1` build — 6.5 min on this runner — where the debug build CI
# already makes takes about 1 min. Measured on PR #753: a person's first launch pays two costs
# that are not charter's.
#   - Once per user profile, Mesa compiles WebKit's shaders into ~/.cache/mesa_shader_cache
#     (about 0.45 s in a container, about 1 s on a 2-core runner). A fresh profile per launch
#     makes every held launch pay it, as a new user's first launch does.
#   - Once per boot or install, the binary and GTK/WebKitGTK's libraries come off a cold disk
#     (about 2.3 s on the runner, for a 500 MB debug binary). The discarded launch pays it, and
#     its time is printed as "cold disk, once per boot or install: reported, not gated".
# CI measures the debug build `npx tauri build --debug` makes; a release build is smaller, so
# its cold-disk cost is likely smaller too.
set -euo pipefail

case="${1:?which case: bare, i3 or i3-nobus}"

# `APP` as the caller named it, then the repository root for everything else.
app="$(realpath -m "${APP:-$(dirname "$0")/../target/debug/charter-app}")"
cd "$(dirname "$0")/.."
limit="${LIMIT_MS:-2000}"
ceiling="${CEILING_MS:-2500}"
bench=(node tools/bench.mjs --skip-build --only coldstart --app "$app" --cold-starts 5 --limit "$limit"
  --ceiling "$ceiling" --warm-up --fresh-profile)

if [[ -z "${DISPLAY:-}" ]]; then
  # A display of its own for the run, and this script again inside it.
  APP="$app" exec xvfb-run -a -s "-screen 0 1280x800x24" tools/coldstart-linux.sh "$@"
fi

# What this run made, and the window manager it started: gone however the run ends.
scratch="$(mktemp -d)"
wm=""
nobus=""
# The run's verdict is the script's: the cleanup keeps the status it was called with, and what it
# cannot remove is said, never failed on.
cleanup() {
  local status=$?
  if [[ -n "$wm" ]]; then
    kill "$wm" 2>/dev/null || true
    wait "$wm" 2>/dev/null || true
  fi
  # In the no-bus case the app starts on the X display's bus, whose document portal mounts FUSE
  # at $XDG_RUNTIME_DIR/doc — a directory `rm` cannot remove while it is mounted (#746, CI).
  local doc="$scratch/runtime/doc"
  if [[ -d "$doc" ]]; then
    fusermount3 -uz "$doc" 2>/dev/null || fusermount -uz "$doc" 2>/dev/null || true
  fi
  if [[ -n "$nobus" ]]; then
    the_x_displays_bus_ends
  fi
  rm -rf "$scratch" 2>/dev/null || echo "coldstart-linux.sh: could not remove $scratch" >&2
  exit "$status"
}
trap cleanup EXIT

# The bus `dbus-launch --autolaunch` started for the X display in the no-bus case, and the
# portals the app woke on it, ended by the process ids the bus itself gives — never by name.
# Asking `dbus-launch` again hands back the bus the display holds, with its daemon's pid.
the_x_displays_bus_ends() {
  local machine printed address daemon name pid
  machine="$(cat /etc/machine-id 2>/dev/null || true)"
  [[ -n "$machine" ]] || machine="$(cat /var/lib/dbus/machine-id 2>/dev/null || true)"
  [[ -n "$machine" ]] || return 0
  printed="$(dbus-launch --autolaunch="$machine" --sh-syntax --close-stderr 2>/dev/null)" || return 0
  address="$(sed -n "s/^DBUS_SESSION_BUS_ADDRESS='\(.*\)';$/\1/p" <<<"$printed")"
  daemon="$(sed -n 's/^DBUS_SESSION_BUS_PID=\([0-9]*\);$/\1/p' <<<"$printed")"
  [[ -n "$address" && -n "$daemon" ]] || return 0
  for name in org.freedesktop.portal.Documents org.freedesktop.portal.Desktop \
    org.freedesktop.impl.portal.desktop.gtk; do
    pid="$(dbus-send --bus="$address" --print-reply --dest=org.freedesktop.DBus \
      /org/freedesktop/DBus org.freedesktop.DBus.GetConnectionUnixProcessID "string:$name" \
      2>/dev/null | awk '/uint32/ { print $2 }')" || true
    if [[ -n "$pid" ]]; then
      kill "$pid" 2>/dev/null || true
    fi
  done
  kill "$daemon" 2>/dev/null || true
}

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
  nobus=yes
  with_i3 env -u DBUS_SESSION_BUS_ADDRESS XDG_RUNTIME_DIR="$scratch/runtime" "${bench[@]}"
  ;;
*)
  echo "unknown case: $case" >&2
  exit 2
  ;;
esac
