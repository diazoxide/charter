#!/usr/bin/env bash
# M5 — does `NO_COLOR` still reach the pane?
#
# `frame/panel.py:_write` calls itself "the one place anything reaches the pane's screen",
# and that is where charter honours `NO_COLOR` (`chrome.colour_ok`) — for a component
# charter did not write as much as for one it did. A takeover component makes the sentence
# false: `textual/drivers/linux_driver.py:58` sets `self._file = sys.__stderr__` and the
# app writes straight there, past `_write`, past `chrome.plain`, past `sys.stderr` itself.
#
# Whether the operator's `NO_COLOR` is honoured is then entirely up to the framework.
source "$(dirname "${BASH_SOURCE[0]}")/rig.sh"

seed
start_session

sgr() {  # how many SGR sequences are in this pane's captured output
  tm capture-pane -p -e -t "$1" | grep -o $'\033\\[[0-9;]*m' | wc -l | tr -d ' '
}

own=$(split_panel repos 8)
live=$(split_panel textual.live 10)
adapt=$(tm split-window -t exp:0.0 -v -l 10 -P -F '#{pane_id}' \
   -- "$CHARTER" panel textual.repos --session "$FID")
sleep 3
echo "== colour ON =="
echo "  charter repos : $(sgr "$own") SGR sequences"
echo "  textual.live  : $(sgr "$live") SGR sequences"
echo "  textual.repos : $(sgr "$adapt") SGR sequences"
tm kill-pane -t "$own"; tm kill-pane -t "$live"; tm kill-pane -t "$adapt"

own=$(tm split-window -t exp:0.0 -v -l 8 -P -F '#{pane_id}' \
   -e NO_COLOR=1 -- "$CHARTER" panel repos --session "$FID")
live=$(tm split-window -t exp:0.0 -v -l 10 -P -F '#{pane_id}' \
   -e NO_COLOR=1 -- "$CHARTER" panel textual.live --session "$FID")
adapt=$(tm split-window -t exp:0.0 -v -l 10 -P -F '#{pane_id}' \
   -e NO_COLOR=1 -- "$CHARTER" panel textual.repos --session "$FID")
sleep 3
echo "== NO_COLOR=1 =="
echo "  charter repos : $(sgr "$own") SGR sequences"
echo "  textual.live  : $(sgr "$live") SGR sequences"
echo "  textual.repos : $(sgr "$adapt") SGR sequences"
