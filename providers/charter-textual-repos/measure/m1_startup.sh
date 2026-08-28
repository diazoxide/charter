#!/usr/bin/env bash
# M1 — how long from `split-window` to the first non-blank paint, for three panels:
# charter's own `repos`, the Textual adapter (`textual.repos`) and the Textual takeover
# (`textual.live`). Polled at 10 ms against real `capture-pane`, five runs each.
source "$(dirname "${BASH_SOURCE[0]}")/rig.sh"

seed
start_session

first_paint() {  # first_paint <component> <rows>
  local pane start now
  start=$("$PY" -c 'import time;print(time.time())')
  pane=$(split_panel "$1" "$2")
  for _ in $(seq 1 400); do
    if tm capture-pane -p -t "$pane" | grep -q '[^[:space:]]'; then
      now=$("$PY" -c 'import time;print(time.time())')
      "$PY" -c "print(f'{1000*($now-$start):.0f}')"
      tm kill-pane -t "$pane"
      return
    fi
    "$PY" -c 'import time;time.sleep(0.01)'
  done
  echo "TIMEOUT"
  tm kill-pane -t "$pane" 2>/dev/null || true
}

for comp in repos textual.repos textual.live; do
  printf '%-16s' "$comp"
  for _ in 1 2 3 4 5; do printf ' %sms' "$(first_paint "$comp" 14)"; done
  printf '\n'
done
