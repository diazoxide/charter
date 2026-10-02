#!/usr/bin/env bash
# Run a command, then fail if it left a process of its own behind (#923).
#
#   tools/no-orphans.sh [--reap] [--grace SECONDS] -- COMMAND [ARGS...]
#
# A process counts as left behind when it started while COMMAND ran, belongs to this user,
# is still alive GRACE seconds (default 5) after COMMAND exited, and is not one this script
# started itself. Each one is named with its pid and command line, and the exit status is 1
# even when COMMAND passed. Otherwise the exit status is COMMAND's own.
#
# --reap also kills each one, by its pid, never by name. CI passes it: the runner is the
# job's own machine, so every new process on it is the job's. On a machine shared with other
# work, leave it off — a process another session started while COMMAND ran is counted too,
# and killing it would kill someone else's.
#
# Started-while-it-ran is read as pid plus start time, so a pid reused during the run is
# still a new process. `ps -A -o pid=,ppid=,uid=,lstart=,command=` reads the same on Linux
# (procps) and macOS.
set -u

usage() {
  echo "usage: $0 [--reap] [--grace SECONDS] -- COMMAND [ARGS...]" >&2
  exit 2
}

reap=0
grace=5
while [ $# -gt 0 ]; do
  case "$1" in
    --reap) reap=1; shift ;;
    --grace)
      case "${2-}" in
        '' | *[!0-9.]*) usage ;;
      esac
      grace="$2"; shift 2 ;;
    --) shift; break ;;
    -*) usage ;;
    *) break ;;
  esac
done
[ $# -gt 0 ] || usage

me=$(id -u)
here=$$

# Fails closed: a snapshot that could not be taken, or came back empty, is not "nothing new",
# so the check stops with status 3 rather than pass a suite it could not see.
#
# pid ppid uid lstart(5 words) command... -> "pid|ppid|uid|lstart|command". LC_ALL=C, so
# lstart is spelled the same in both snapshots whatever the locale.
table() {
  local raw
  if ! raw=$(LC_ALL=C ps -A -o pid=,ppid=,uid=,lstart=,command=) || [ -z "$raw" ]; then
    echo "no-orphans: ps could not list the processes; nothing was checked" >&2
    exit 3
  fi
  awk '{
    pid=$1; ppid=$2; uid=$3; start=$4" "$5" "$6" "$7" "$8
    cmd=""; for (i=9;i<=NF;i++) cmd=cmd (i>9?" ":"") $i
    print pid "|" ppid "|" uid "|" start "|" cmd
  }' <<<"$raw"
}

before=$(mktemp) || exit 3
trap 'rm -f "$before"' EXIT
first=$(table) || exit 3
awk -F'|' '{print $1 "|" $4}' <<<"$first" >"$before"

"$@"
status=$?

sleep "$grace"

now=$(table) || exit 3
left=$(
  awk -F'|' -v me="$me" -v here="$here" '
    FNR == NR { seen[$0] = 1; next }
    { pid[++rows]=$1; ppid[$1]=$2; uid[$1]=$3; key[$1]=$1 "|" $4; cmd[$1]=$5 }
    END {
      q = here
      for (hops = 0; q != "" && q > 1 && hops < 64; hops++) { above[q] = 1; q = ppid[q] }
      for (r = 1; r <= rows; r++) {
        p = pid[r]
        if (uid[p] != me || (key[p] in seen)) continue
        # One of this script'"'"'s own (the ps and awk reading the table right now), or one
        # of the processes this script itself runs under.
        q = p; ours = 0
        for (hops = 0; q != "" && q > 1 && hops < 64; hops++) {
          if (q == here) { ours = 1; break }
          q = ppid[q]
        }
        if (!ours && !(p in above)) print p "|" cmd[p]
      }
    }' "$before" - <<<"$now"
)

if [ -z "$left" ]; then
  exit "$status"
fi

echo "no-orphans: '$*' left these processes running:" >&2
while IFS='|' read -r pid cmd; do
  echo "  pid $pid: $cmd" >&2
done <<<"$left"
if [ "$reap" = 1 ]; then
  while IFS='|' read -r pid _; do
    kill -KILL "$pid" 2>/dev/null || true
  done <<<"$left"
  echo "no-orphans: killed them, by pid" >&2
fi
exit 1
