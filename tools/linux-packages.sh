#!/usr/bin/env bash
# The Linux jobs' package install, bounded and cacheable (#1477).
#
#   tools/linux-packages.sh key [--tauri] PACKAGE...
#   tools/linux-packages.sh install [--cache DIR] [--tauri] PACKAGE...
#
# `--tauri` adds Tauri's Linux build dependencies, which this file names once for every job.
#
# `key` writes `key=...` to $GITHUB_OUTPUT: the runner image and the package list, in any order.
# The image is in it because the .deb files a run downloads are the ones that image lacked; on
# the next image they may not fit, and a new key starts over from the mirror.
#
# `install` installs PACKAGE... and says in the job summary how long it took and from where.
#   - With `--cache DIR` holding .deb files (`.github/actions/linux-packages` restores them
#     there), it puts them in apt's archives directory and installs them from there with
#     `--no-download`, so a slow or broken mirror is never asked. If they do not fit, it says so,
#     quoting apt-get, and goes on to the mirror.
#   - From the mirror, `apt-get update` and `apt-get install` each run under a limit of their
#     own (UPDATE_SECONDS, INSTALL_SECONDS), and a failed or stuck attempt is tried once more.
#     On 2026-10-07 the mirror took 1,285 s for an install that takes under 45 s, and a run
#     sat for over an hour; now the step fails in minutes and says why.
#   - With `--cache DIR`, after an install from the mirror it copies into DIR the .deb files
#     of exactly the versions now installed, and writes `save=true` to $GITHUB_OUTPUT.
#
# Seams for tools/linux-packages.test.mjs: LINUX_PACKAGES_SUDO (default `sudo`),
# LINUX_PACKAGES_ARCHIVES (default /var/cache/apt/archives), LINUX_PACKAGES_PAUSE (seconds
# between attempts, default 10).
set -euo pipefail

TAURI_PACKAGES="libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev"
# A normal install is under 45 s in all. Two attempts at the most, plus the try from the cache,
# stay under the 9 minutes the calling step allows, so this script's message is what is read
# and not the runner's "exceeded the maximum execution time": with each limit's 10 s for a
# process that ignores the first signal and the pause between attempts, the worst case is
# (60+10) + 2 x ((75+10) + (120+10)) + 10 = 510 s, under the step's 540.
UPDATE_SECONDS=75
INSTALL_SECONDS=120
OFFLINE_SECONDS=60
ATTEMPTS=2

SUDO=${LINUX_PACKAGES_SUDO-sudo}
ARCHIVES=${LINUX_PACKAGES_ARCHIVES:-/var/cache/apt/archives}
PAUSE=${LINUX_PACKAGES_PAUSE:-10}
OUTPUT=${GITHUB_OUTPUT:-/dev/null}
SUMMARY=${GITHUB_STEP_SUMMARY:-/dev/null}

# Every apt-get: retries per download, a stalled connection given up on, and a wait for the
# dpkg lock an image's own unattended upgrade may hold, rather than failing at once.
APT_OPTIONS="-y -q -o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 -o DPkg::Lock::Timeout=60"

usage() {
  echo "usage: $0 key [--tauri] PACKAGE... | $0 install [--cache DIR] [--tauri] PACKAGE..." >&2
  exit 2
}

[ $# -gt 0 ] || usage
mode=$1
shift
case "$mode" in key | install) ;; *) usage ;; esac

cache=""
packages=""
while [ $# -gt 0 ]; do
  case "$1" in
    --cache)
      [ $# -gt 1 ] || usage
      cache=$2
      shift 2
      ;;
    --tauri)
      packages="$packages $TAURI_PACKAGES"
      shift
      ;;
    -*) usage ;;
    *)
      packages="$packages $1"
      shift
      ;;
  esac
done
# One name per line, sorted, each once: the list the key is made of and apt-get is given.
# shellcheck disable=SC2086 # split into its names on purpose.
packages=$(printf '%s\n' $packages | sort -u | sed '/^$/d')
[ -n "$packages" ] || usage

if [ "$mode" = key ]; then
  if command -v sha256sum > /dev/null; then
    hash=$(printf '%s\n' "$packages" | sha256sum | cut -c1-16)
  else
    hash=$(printf '%s\n' "$packages" | shasum -a 256 | cut -c1-16)
  fi
  echo "key=linux-packages-v1-${ImageOS:-linux}-${ImageVersion:-unknown}-$(uname -m)-$hash" >> "$OUTPUT"
  exit 0
fi

count=$(printf '%s\n' "$packages" | wc -l | tr -d ' ')
start=$(date +%s)

say() {
  printf '### Linux packages\n\n%s\n' "$1" >> "$SUMMARY"
}

elapsed() {
  echo $(($(date +%s) - start))
}

# The DEBIAN_FRONTEND goes through `env` so `sudo` keeps it.
apt() {
  local seconds=$1
  shift
  # shellcheck disable=SC2086 # APT_OPTIONS is a list of words on purpose.
  $SUDO timeout -k 10 "$seconds" env DEBIAN_FRONTEND=noninteractive apt-get $APT_OPTIONS "$@"
}

# From the cache: the files themselves, and nothing downloaded.
#
# They are copied into apt's archives directory first and named there. apt takes a .deb named on
# its command line as already downloaded only when it finds the file in that directory; from any
# other path it queues the file as a download like any other, `--no-download` drops it, and the
# install fails with "Unable to fetch some archives". Every warm run failed so until 2026-10-10.
if [ -n "$cache" ] && ls "$cache"/*.deb > /dev/null 2>&1; then
  $SUDO cp "$cache"/*.deb "$ARCHIVES"/
  debs=()
  for deb in "$cache"/*.deb; do
    debs+=("$ARCHIVES/$(basename "$deb")")
  done
  offline=0
  said=$(apt "$OFFLINE_SECONDS" install --no-download "${debs[@]}" 2>&1) || offline=$?
  printf '%s\n' "$said"
  if [ "$offline" -eq 0 ]; then
    say "$count packages installed in $(elapsed) s from the cache, without asking the mirror."
    exit 0
  fi
  why=$(grep -E '^(E|W):' <<< "$said" | head -n 4 | tr '\n' ' ' || true)
  echo "::warning title=Linux packages::the cached .deb files do not install on this image; installing from the mirror instead. apt-get said: ${why:-nothing on its error lines}"
fi

# From the mirror, twice at the most.
attempt=1
while :; do
  status=0
  # shellcheck disable=SC2086 # one package name per word.
  apt "$UPDATE_SECONDS" update && apt "$INSTALL_SECONDS" install $packages || status=$?
  if [ "$status" -eq 0 ]; then
    break
  fi
  if [ "$status" -eq 124 ]; then
    why="did not finish in time"
  else
    why="failed (exit $status)"
  fi
  if [ "$attempt" -ge "$ATTEMPTS" ]; then
    echo "::error title=Linux packages::apt-get $why on attempt $attempt of $ATTEMPTS: the distribution's mirror is slow or down, not the code under test. Re-run the job later."
    say "The install failed after $(elapsed) s: apt-get $why on each of $ATTEMPTS attempts, so the mirror is slow or down."
    exit 1
  fi
  echo "::warning title=Linux packages::apt-get $why on attempt $attempt of $ATTEMPTS; trying once more"
  attempt=$((attempt + 1))
  sleep "$PAUSE"
done

note=""
if [ -n "$cache" ]; then
  # Only the files of the versions now installed, so a file apt left from something else
  # never rides along into the cache.
  installed=$(dpkg-query -W -f='${Package}_${Version}_${Architecture}\n')
  mkdir -p "$cache"
  kept=0
  for deb in "$ARCHIVES"/*.deb; do
    [ -e "$deb" ] || continue
    # apt writes an epoch's colon as %3a in the file name.
    name=$(basename "$deb" .deb | sed 's/%3a/:/g')
    # A here-string and not a pipe: under `pipefail`, `grep -q` leaving at its first match can
    # end the writer with SIGPIPE once the list outgrows the pipe's buffer, and a runner image
    # lists thousands of packages, so the match would read as a miss.
    if grep -qxF "$name" <<< "$installed"; then
      cp "$deb" "$cache/"
      kept=$((kept + 1))
    fi
  done
  if [ "$kept" -gt 0 ]; then
    echo "save=true" >> "$OUTPUT"
    note=" $kept .deb files kept for the cache."
  else
    note=" No .deb files were left in $ARCHIVES to keep, so the next run asks the mirror too."
  fi
fi
say "$count packages installed in $(elapsed) s from the mirror, on attempt $attempt of $ATTEMPTS.$note"
