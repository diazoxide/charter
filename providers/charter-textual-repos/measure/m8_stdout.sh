#!/usr/bin/env bash
# M8 — what Textual's `sys.stdout` capture does to a charter panel, with and without the
# provider's four-line workaround (`adapter._give_stdout_back`).
#
# Two panes, identical but for `CHARTER_TEXTUAL_KEEP_CAPTURE=1`, watched across a version
# bump. The captured pane paints once and then draws blank, silently — see
# `adapter._give_stdout_back` for the chain (`panel._out` writes into Textual's print log,
# `slots._width`/`_height` measure fd -1 and fall back to 80x24, `chrome.colour_ok` is
# told the pane is a tty whatever it is).
source "$(dirname "${BASH_SOURCE[0]}")/rig.sh"

seed failed
start_session

fixed=$(tm split-window -t exp:0.0 -v -l 10 -P -F '#{pane_id}' \
   -- "$CHARTER" panel textual.repos --session "$FID")
broken=$(tm split-window -t exp:0.0 -v -l 10 -P -F '#{pane_id}' \
   -e CHARTER_TEXTUAL_KEEP_CAPTURE=1 -- "$CHARTER" panel textual.repos --session "$FID")

for i in $(seq 1 10); do
  "$PY" -c 'import time;time.sleep(0.5)'
  printf '%5.1fs  restored=%-2s  captured=%-2s  %s\n' \
    "$(echo "$i*0.5" | bc)" \
    "$(tm capture-pane -p -t "$fixed"  | grep -c '[^[:space:]]')" \
    "$(tm capture-pane -p -t "$broken" | grep -c '[^[:space:]]')" \
    "${note:-}"
  if [ "$i" = 4 ]; then seed passed; note="(non-blank rows in each pane; bumped at 2.0s)"; fi
done

echo
echo "== the captured pane, in full, after the bump =="
tm capture-pane -p -t "$broken" | sed 's/^/  |/'
echo
echo "== and nothing was reported: pane alive, no charter error, no stderr =="
tm list-panes -t exp -F '  #{pane_id} dead=#{pane_dead} cmd=#{pane_current_command}' \
  | grep -E "$broken|$fixed"
