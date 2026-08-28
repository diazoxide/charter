#!/usr/bin/env bash
# M3b — where a Textual crash's REASON ends up.
#
# M3 showed the blast radius is one pane. This asks the follow-up question, which turned
# out to be the more interesting one: Textual does not let the exception out of `run()`.
# It catches it, prints a Rich traceback to **stderr**, and returns normally — so
# `Registry.draw` never sees a failure, charter's "failed to draw" message never appears,
# and `panel._write` clears the pane a moment later with whatever `render` answered.
#
# Sampled every 100 ms across the crash so the traceback can be seen arriving and being
# wiped.
source "$(dirname "${BASH_SOURCE[0]}")/rig.sh"

seed
start_session

victim=$(tm split-window -t exp:0.0 -v -l 12 -P -F '#{pane_id}' \
   -e CHARTER_TEXTUAL_FAULT=loop -- "$CHARTER" panel textual.live --session "$FID")

for i in $(seq 1 22); do
  "$PY" -c 'import time;time.sleep(0.1)'
  line=$(tm capture-pane -p -t "$victim" | grep -m1 '[^[:space:]]' || true)
  printf '%5sms | %s\n' "$((i*100))" "${line:0:110}"
done
